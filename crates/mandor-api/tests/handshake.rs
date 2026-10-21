#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration tests: clippy.toml allowances miss #[tokio::test]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use mandor_api::{Dispatch, ServerConfig, relay_lines, serve};
use mandor_protocol::{ErrorCode, Notification, Request, Response, RpcError};
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct Echo;

impl Dispatch for Echo {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "status" => Ok(json!({"ok": true})),
            "echo" => Ok(params),
            _ => Err(RpcError::new(ErrorCode::MethodNotFound, method)),
        }
    }
}

struct Daemon {
    addr: SocketAddr,
    events: broadcast::Sender<Notification>,
    shutdown: CancellationToken,
}

async fn start() -> Daemon {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (events, _) = broadcast::channel(16);
    let shutdown = CancellationToken::new();
    let cfg = ServerConfig {
        token: SecretString::from("rahasia-uji".to_owned()),
        daemon_version: "0.1.0".into(),
    };
    tokio::spawn(serve(
        listener,
        cfg,
        Arc::new(Echo),
        events.clone(),
        shutdown.clone(),
    ));
    Daemon {
        addr,
        events,
        shutdown,
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(addr: SocketAddr) -> Ws {
    tokio_tungstenite::connect_async(format!("ws://{addr}/rpc"))
        .await
        .unwrap()
        .0
}

async fn send(ws: &mut Ws, req: &Request) {
    ws.send(Message::Text(serde_json::to_string(req).unwrap().into()))
        .await
        .unwrap();
}

async fn recv(ws: &mut Ws) -> Value {
    let msg = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match msg {
        Message::Text(t) => serde_json::from_str(t.as_str()).unwrap(),
        other => panic!("bukan teks: {other:?}"),
    }
}

fn hello(token: &str, protocol: &str) -> Request {
    Request::new(
        1,
        "hello",
        json!({"token": token, "protocol": protocol, "client": "test"}),
    )
}

#[tokio::test]
async fn token_salah_ditolak_dan_koneksi_ditutup() {
    let d = start().await;
    let mut ws = connect(d.addr).await;
    send(&mut ws, &hello("salah", "1.0")).await;
    let resp: Response = serde_json::from_value(recv(&mut ws).await).unwrap();
    assert_eq!(resp.error.unwrap().code, ErrorCode::Unauthorized);
    let next = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .unwrap();
    assert!(matches!(next, None | Some(Ok(Message::Close(_)) | Err(_))));
    d.shutdown.cancel();
}

#[tokio::test]
async fn versi_mayor_berbeda_ditolak_dengan_kedua_versi() {
    let d = start().await;
    let mut ws = connect(d.addr).await;
    send(&mut ws, &hello("rahasia-uji", "2.0")).await;
    let resp: Response = serde_json::from_value(recv(&mut ws).await).unwrap();
    let err = resp.error.unwrap();
    assert_eq!(err.code, ErrorCode::VersionMismatch);
    assert!(
        err.message.contains("2.0") && err.message.contains("1.0"),
        "{}",
        err.message
    );
    d.shutdown.cancel();
}

#[tokio::test]
async fn request_sebelum_hello_ditolak() {
    let d = start().await;
    let mut ws = connect(d.addr).await;
    send(&mut ws, &Request::new(5, "status", Value::Null)).await;
    let resp: Response = serde_json::from_value(recv(&mut ws).await).unwrap();
    assert_eq!(resp.error.unwrap().code, ErrorCode::Unauthorized);
    d.shutdown.cancel();
}

#[tokio::test]
async fn hello_valid_lalu_method_dijalankan() {
    let d = start().await;
    let mut ws = connect(d.addr).await;
    send(&mut ws, &hello("rahasia-uji", "1.0")).await;
    let ok: Response = serde_json::from_value(recv(&mut ws).await).unwrap();
    assert_eq!(ok.result.unwrap()["protocol"], "1.0");
    send(&mut ws, &Request::new(2, "echo", json!({"x": 1}))).await;
    let resp: Response = serde_json::from_value(recv(&mut ws).await).unwrap();
    assert_eq!(resp.id, 2);
    assert_eq!(resp.result.unwrap(), json!({"x": 1}));
    d.shutdown.cancel();
}

#[tokio::test]
async fn notifikasi_sampai_ke_semua_client_dan_client_putus_tidak_mengganggu() {
    let d = start().await;
    let mut a = connect(d.addr).await;
    let mut b = connect(d.addr).await;
    for ws in [&mut a, &mut b] {
        send(ws, &hello("rahasia-uji", "1.0")).await;
        recv(ws).await;
    }
    drop(a);
    tokio::time::sleep(Duration::from_millis(50)).await;
    d.events
        .send(Notification::new("card.updated", json!({"id": "rec1"})))
        .ok();
    let n = recv(&mut b).await;
    assert_eq!(n["method"], "card.updated");
    send(&mut b, &Request::new(3, "status", Value::Null)).await;
    let resp: Response = serde_json::from_value(recv(&mut b).await).unwrap();
    assert_eq!(resp.result.unwrap(), json!({"ok": true}));
    d.shutdown.cancel();
}

#[tokio::test]
async fn relay_meneruskan_baris_tanpa_menyisipkan_token() {
    let d = start().await;
    let (client, relay_side) = tokio::io::duplex(4096);
    let (relay_in, relay_out) = tokio::io::split(relay_side);
    let addr = d.addr;
    let relay = tokio::spawn(async move { relay_lines(addr, relay_in, relay_out).await });

    let (client_read, mut client_write) = tokio::io::split(client);
    let mut lines = BufReader::new(client_read).lines();

    // Tanpa token yang valid: relay hanya meneruskan, daemon menolak.
    let bad = serde_json::to_string(&hello("", "1.0")).unwrap();
    client_write
        .write_all(format!("{bad}\n").as_bytes())
        .await
        .unwrap();
    let line = tokio::time::timeout(Duration::from_secs(3), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let resp: Response = serde_json::from_str(&line).unwrap();
    assert_eq!(resp.error.unwrap().code, ErrorCode::Unauthorized);
    let _ = tokio::time::timeout(Duration::from_secs(3), relay).await;
    d.shutdown.cancel();
}
