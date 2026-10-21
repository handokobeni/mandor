//! Server HTTP tiruan untuk contract test Lark.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mandor_lark::{LarkClient, LarkConfig};
use secrecy::SecretString;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    pub target: String,
    pub auth: Option<String>,
    pub body: Value,
}

impl Req {
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }
    pub fn query_has(&self, needle: &str) -> bool {
        self.target
            .split('?')
            .nth(1)
            .is_some_and(|q| q.split('&').any(|kv| kv == needle))
    }
}

pub type Handler = Arc<dyn Fn(&Req) -> (u16, String) + Send + Sync>;

pub struct Mock {
    pub base: String,
    log: Arc<Mutex<Vec<Req>>>,
}

impl Mock {
    pub async fn start(handler: Handler) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let log2 = Arc::clone(&log);
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let handler = Arc::clone(&handler);
                let log = Arc::clone(&log2);
                tokio::spawn(async move {
                    let req = read_request(&mut sock).await;
                    let (status, body) = handler(&req);
                    log.lock().unwrap().push(req);
                    let resp = format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        Self { base, log }
    }

    pub fn requests(&self) -> Vec<Req> {
        self.log.lock().unwrap().clone()
    }

    pub fn client(&self) -> LarkClient {
        let mut cfg = LarkConfig::larksuite(
            "cli_app".to_owned(),
            SecretString::from("app-secret".to_owned()),
            "http://127.0.0.1:53682/callback".to_owned(),
        );
        cfg.open_base.clone_from(&self.base);
        cfg.accounts_base.clone_from(&self.base);
        cfg.timeout = Duration::from_secs(5);
        LarkClient::new(cfg).unwrap()
    }
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Req {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let n = sock.read(&mut chunk).await.unwrap();
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        assert!(n > 0, "koneksi tertutup sebelum header lengkap");
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next().unwrap().split(' ');
    let method = first.next().unwrap().to_owned();
    let target = first.next().unwrap().to_owned();
    let mut len = 0usize;
    let mut auth = None;
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            match k.to_ascii_lowercase().as_str() {
                "content-length" => len = v.trim().parse().unwrap(),
                "authorization" => auth = Some(v.trim().to_owned()),
                _ => {}
            }
        }
    }
    while buf.len() < header_end + len {
        let n = sock.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = serde_json::from_slice(&buf[header_end..header_end + len]).unwrap_or(Value::Null);
    Req {
        method,
        target,
        auth,
        body,
    }
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

pub fn access() -> SecretString {
    SecretString::from("u-token".to_owned())
}
