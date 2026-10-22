use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response as HttpResponse;
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use mandor_protocol::{
    ErrorCode, Hello, HelloOk, Notification, PROTOCOL_MAJOR, PROTOCOL_VERSION, Request, Response,
    RpcError, major_of, method,
};
use secrecy::{ExposeSecret, SecretString};
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, broadcast, mpsc};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Batas waktu client mengirim `hello` setelah tersambung.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// Batas ukuran satu pesan masuk.
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
/// Request yang diproses bersamaan per koneksi.
const MAX_IN_FLIGHT_PER_CONN: usize = 16;
/// Kapasitas antrean pesan keluar per koneksi.
const OUTBOUND_CAPACITY: usize = 256;

/// Lingkup koneksi yang memakai token kapabilitas (KTD-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityScope {
    pub run_id: String,
    pub role: String,
}

/// Penangan method; diimplementasikan lapisan app di atas use case.
pub trait Dispatch: Send + Sync + 'static {
    /// Menjalankan `method` dengan `params`.
    fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> impl Future<Output = Result<serde_json::Value, RpcError>> + Send;

    /// Lingkup token kapabilitas bila masih berlaku; `None` menolak.
    fn verify_capability(
        &self,
        _token: &str,
    ) -> impl Future<Output = Option<CapabilityScope>> + Send {
        async { None }
    }

    /// Method untuk koneksi kapabilitas; hanya tool ledger yang boleh lewat.
    fn call_scoped(
        &self,
        _scope: &CapabilityScope,
        method: &str,
        _params: serde_json::Value,
    ) -> impl Future<Output = Result<serde_json::Value, RpcError>> + Send {
        let method = method.to_owned();
        async move {
            Err(RpcError::new(
                ErrorCode::Unauthorized,
                format!("{method} is not available on this connection"),
            ))
        }
    }
}

/// Jenis koneksi yang diterima saat `hello`.
enum Accepted {
    /// Token daemon: semua method dan broadcast event.
    Full,
    /// Token kapabilitas: hanya tool ledger, tanpa event; token diperiksa ulang
    /// di setiap request.
    Scoped(String),
}

/// Konfigurasi server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub token: SecretString,
    pub daemon_version: String,
}

struct Shared<D> {
    cfg: ServerConfig,
    dispatch: Arc<D>,
    events: broadcast::Sender<Notification>,
    shutdown: CancellationToken,
}

/// Menjalankan server sampai `shutdown` dibatalkan.
///
/// # Errors
/// Mengembalikan error I/O dari server HTTP.
pub async fn serve<D: Dispatch>(
    listener: TcpListener,
    cfg: ServerConfig,
    dispatch: Arc<D>,
    events: broadcast::Sender<Notification>,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
    let shared = Arc::new(Shared {
        cfg,
        dispatch,
        events,
        shutdown: shutdown.clone(),
    });
    let app = Router::new()
        .route("/rpc", get(upgrade::<D>))
        .with_state(shared);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await
}

async fn upgrade<D: Dispatch>(
    State(shared): State<Arc<Shared<D>>>,
    ws: WebSocketUpgrade,
) -> HttpResponse {
    ws.max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection(socket, shared))
}

type WsSink = futures::stream::SplitSink<WebSocket, Message>;

/// Hasil satu langkah loop koneksi.
enum Step {
    Continue,
    Stop,
}

async fn connection<D: Dispatch>(socket: WebSocket, shared: Arc<Shared<D>>) {
    let (mut sink, mut stream) = socket.split();
    let (reply, accepted) = match handshake(&mut stream, &shared).await {
        Ok((resp, kind)) => (resp, Some(kind)),
        Err(resp) => (resp, None),
    };
    let Some(kind) = accepted else {
        let _ = sink.send(text(&reply)).await;
        let _ = sink.close().await;
        return;
    };
    if sink.send(text(&reply)).await.is_err() {
        return;
    }
    if let Accepted::Scoped(token) = kind {
        scoped_connection(sink, stream, &shared, &token).await;
        return;
    }

    let (out_tx, mut out_rx) = mpsc::channel::<String>(OUTBOUND_CAPACITY);
    let mut events = shared.events.subscribe();
    let limiter = Arc::new(Semaphore::new(MAX_IN_FLIGHT_PER_CONN));
    let mut tasks = JoinSet::new();

    loop {
        let step = tokio::select! {
            () = shared.shutdown.cancelled() => Step::Stop,
            Some(line) = out_rx.recv() => send_line(&mut sink, line).await,
            ev = events.recv() => forward_event(&mut sink, ev).await,
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(body))) => {
                    spawn_request(&mut tasks, &limiter, &shared, &out_tx, body.as_str().to_owned()).await
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => Step::Stop,
                Some(Ok(_)) => Step::Continue,
            },
            Some(_) = tasks.join_next(), if !tasks.is_empty() => Step::Continue,
        };
        if matches!(step, Step::Stop) {
            break;
        }
    }
    tasks.shutdown().await;
}

/// Koneksi kapabilitas: request diproses berurutan, tanpa event, dan token
/// diverifikasi ulang setiap request; token yang dicabut menutup koneksi.
async fn scoped_connection<D: Dispatch>(
    mut sink: WsSink,
    mut stream: futures::stream::SplitStream<WebSocket>,
    shared: &Shared<D>,
    token: &str,
) {
    loop {
        let msg = tokio::select! {
            () = shared.shutdown.cancelled() => break,
            msg = stream.next() => msg,
        };
        let body = match msg {
            Some(Ok(Message::Text(body))) => body,
            Some(Ok(Message::Close(_)) | Err(_)) | None => break,
            Some(Ok(_)) => continue,
        };
        let Ok(req) = serde_json::from_str::<Request>(body.as_str()) else {
            let resp = Response::err(0, RpcError::new(ErrorCode::ParseError, "invalid JSON-RPC"));
            if sink.send(text(&resp)).await.is_err() {
                break;
            }
            continue;
        };
        let Some(scope) = shared.dispatch.verify_capability(token).await else {
            let resp = Response::err(
                req.id,
                RpcError::new(ErrorCode::Unauthorized, "capability expired"),
            );
            let _ = sink.send(text(&resp)).await;
            break;
        };
        let resp = match shared
            .dispatch
            .call_scoped(&scope, &req.method, req.params)
            .await
        {
            Ok(result) => Response::ok(req.id, result),
            Err(err) => Response::err(req.id, err),
        };
        if sink.send(text(&resp)).await.is_err() {
            break;
        }
    }
    let _ = sink.close().await;
}

async fn send_line(sink: &mut WsSink, line: String) -> Step {
    match sink.send(Message::Text(line.into())).await {
        Ok(()) => Step::Continue,
        Err(_) => Step::Stop,
    }
}

async fn forward_event(
    sink: &mut WsSink,
    ev: Result<Notification, broadcast::error::RecvError>,
) -> Step {
    let n = match ev {
        Ok(n) => n,
        Err(broadcast::error::RecvError::Lagged(skipped)) => {
            tracing::warn!(skipped, "client tertinggal event; meminta resync");
            Notification::new("resync", serde_json::json!({ "skipped": skipped }))
        }
        Err(broadcast::error::RecvError::Closed) => return Step::Stop,
    };
    send_line(sink, to_line(&n)).await
}

async fn spawn_request<D: Dispatch>(
    tasks: &mut JoinSet<()>,
    limiter: &Arc<Semaphore>,
    shared: &Arc<Shared<D>>,
    out: &mpsc::Sender<String>,
    body: String,
) -> Step {
    let Ok(permit) = Arc::clone(limiter).acquire_owned().await else {
        return Step::Stop;
    };
    let shared = Arc::clone(shared);
    let out = out.clone();
    tasks.spawn(async move {
        let _permit = permit;
        if let Some(resp) = handle_request(&shared, &body).await {
            let _ = out.send(to_line(&resp)).await;
        }
    });
    Step::Continue
}

/// Memvalidasi `hello`. `Ok` berisi respons sukses dan jenis koneksi, `Err`
/// berisi respons error.
async fn handshake<S, D: Dispatch>(
    stream: &mut S,
    shared: &Shared<D>,
) -> Result<(Response, Accepted), Response>
where
    S: futures::Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    let cfg = &shared.cfg;
    let first = tokio::time::timeout(HELLO_TIMEOUT, stream.next()).await;
    let Ok(Some(Ok(Message::Text(body)))) = first else {
        return Err(Response::err(
            0,
            RpcError::new(ErrorCode::Unauthorized, "hello not accepted"),
        ));
    };
    let Ok(req) = serde_json::from_str::<Request>(body.as_str()) else {
        return Err(Response::err(
            0,
            RpcError::new(ErrorCode::ParseError, "first message is not JSON-RPC"),
        ));
    };
    if req.method != method::HELLO {
        return Err(Response::err(
            req.id,
            RpcError::new(ErrorCode::Unauthorized, "first message must be hello"),
        ));
    }
    let Ok(hello) = serde_json::from_value::<Hello>(req.params) else {
        return Err(Response::err(
            req.id,
            RpcError::new(ErrorCode::InvalidParams, "invalid hello parameters"),
        ));
    };
    let expected = cfg.token.expose_secret().as_bytes();
    let kind = if bool::from(expected.ct_eq(hello.token.as_bytes())) {
        Accepted::Full
    } else if shared
        .dispatch
        .verify_capability(&hello.token)
        .await
        .is_some()
    {
        Accepted::Scoped(hello.token.clone())
    } else {
        return Err(Response::err(
            req.id,
            RpcError::new(ErrorCode::Unauthorized, "invalid token"),
        ));
    };
    if major_of(&hello.protocol) != Some(PROTOCOL_MAJOR) {
        return Err(Response::err(
            req.id,
            RpcError::new(
                ErrorCode::VersionMismatch,
                format!(
                    "client protocol version {} does not match daemon {PROTOCOL_VERSION}",
                    hello.protocol
                ),
            ),
        ));
    }
    let ok = HelloOk {
        daemon_version: cfg.daemon_version.clone(),
        protocol: PROTOCOL_VERSION.to_owned(),
    };
    match serde_json::to_value(ok) {
        Ok(value) => Ok((Response::ok(req.id, value), kind)),
        Err(_) => Err(Response::err(
            req.id,
            RpcError::new(ErrorCode::Internal, "failed to build hello"),
        )),
    }
}

async fn handle_request<D: Dispatch>(shared: &Shared<D>, body: &str) -> Option<Response> {
    let Ok(req) = serde_json::from_str::<Request>(body) else {
        return Some(Response::err(
            0,
            RpcError::new(ErrorCode::ParseError, "invalid JSON-RPC"),
        ));
    };
    if req.method == method::HELLO {
        return Some(Response::err(
            req.id,
            RpcError::new(ErrorCode::InvalidRequest, "hello already done"),
        ));
    }
    Some(match shared.dispatch.call(&req.method, req.params).await {
        Ok(result) => Response::ok(req.id, result),
        Err(err) => Response::err(req.id, err),
    })
}

fn to_line<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn text<T: serde::Serialize>(value: &T) -> Message {
    Message::Text(to_line(value).into())
}
