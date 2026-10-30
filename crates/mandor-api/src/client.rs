//! Client RPC berurutan ke daemon: `hello` dengan token, lalu satu
//! permintaan per panggilan. Dipakai server MCP agent dan `mandord install`.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use mandor_protocol::{ErrorCode, Hello, PROTOCOL_VERSION, Request, Response, RpcError, method};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Batas waktu membuka koneksi ke daemon.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Batas waktu satu panggilan tool ke daemon.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Conn = (SplitSink<Ws, Message>, SplitStream<Ws>);

/// Client RPC berurutan. Koneksi dibuka saat panggilan pertama, sehingga
/// daemon yang tidak terjangkau muncul sebagai error panggilan.
pub struct DaemonClient {
    addr: SocketAddr,
    token: SecretString,
    client: &'static str,
    conn: Mutex<Option<Conn>>,
    next_id: AtomicU64,
}

impl std::fmt::Debug for DaemonClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonClient")
            .field("addr", &self.addr)
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

impl DaemonClient {
    /// Client untuk daemon di `addr`; `client` adalah nama yang dikirim di `hello`.
    #[must_use]
    pub fn new(addr: SocketAddr, token: SecretString, client: &'static str) -> Self {
        Self {
            addr,
            token,
            client,
            conn: Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    async fn open(&self) -> Result<Conn, RpcError> {
        let url = format!("ws://{}/rpc", self.addr);
        let (ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(&url))
            .await
            .map_err(|_| unreachable_daemon("timeout"))?
            .map_err(|e| unreachable_daemon(&e.to_string()))?;
        let mut conn = ws.split();
        let hello = Hello {
            token: self.token.expose_secret().to_owned(),
            protocol: PROTOCOL_VERSION.to_owned(),
            client: self.client.to_owned(),
        };
        let params = serde_json::to_value(hello).map_err(|e| internal(&e.to_string()))?;
        self.exchange(&mut conn, method::HELLO, params).await?;
        Ok(conn)
    }

    /// Mengirim satu permintaan dan menunggu jawabannya dengan batas waktu.
    ///
    /// # Errors
    /// Error RPC dari daemon, atau daemon tidak terjangkau atau tidak menjawab.
    pub async fn request(&self, name: &str, params: Value) -> Result<Value, RpcError> {
        let mut guard = self.conn.lock().await;
        let mut conn = match guard.take() {
            Some(conn) => conn,
            None => self.open().await?,
        };
        let result = self.exchange(&mut conn, name, params).await;
        // Koneksi yang putus dibuang; panggilan berikutnya membuka ulang dan
        // token yang sudah dicabut ditolak saat hello.
        if !matches!(&result, Err(e) if e.code == ErrorCode::Unauthorized) {
            *guard = Some(conn);
        }
        result
    }

    async fn exchange(
        &self,
        conn: &mut Conn,
        name: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = serde_json::to_string(&Request::new(id, name, params))
            .map_err(|e| internal(&e.to_string()))?;
        let (sink, stream) = conn;
        tokio::time::timeout(CALL_TIMEOUT, sink.send(Message::Text(body.into())))
            .await
            .map_err(|_| internal("daemon did not accept the request in time"))?
            .map_err(|e| closed(&e.to_string()))?;
        loop {
            let msg = tokio::time::timeout(CALL_TIMEOUT, stream.next())
                .await
                .map_err(|_| internal("daemon did not answer in time"))?;
            let text = match msg {
                Some(Ok(Message::Text(text))) => text,
                Some(Err(e)) => return Err(closed(&e.to_string())),
                Some(Ok(Message::Close(_))) | None => return Err(closed("closed")),
                Some(Ok(_)) => continue,
            };
            let value: Value =
                serde_json::from_str(text.as_str()).map_err(|e| internal(&e.to_string()))?;
            // Notifikasi daemon (tanpa `id`) di koneksi yang sama dilewati.
            if value.get("id").is_none() {
                continue;
            }
            let resp: Response =
                serde_json::from_value(value).map_err(|e| internal(&e.to_string()))?;
            if resp.id != id {
                continue;
            }
            return match resp.error {
                Some(err) => Err(err),
                None => Ok(resp.result.unwrap_or(Value::Null)),
            };
        }
    }
}

fn unreachable_daemon(reason: &str) -> RpcError {
    internal(&format!("the Mandor daemon is unreachable: {reason}"))
}

fn closed(reason: &str) -> RpcError {
    RpcError::new(
        ErrorCode::Unauthorized,
        format!("the daemon connection was closed: {reason}"),
    )
}

fn internal(message: &str) -> RpcError {
    RpcError::new(ErrorCode::Internal, message)
}
