//! Proses `mandord mcp`: server MCP stdio milik agent yang meneruskan tool
//! ledger ke daemon lewat koneksi kapabilitas (KTD-26).

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use mandor_protocol::{ErrorCode, Hello, PROTOCOL_VERSION, Request, Response, RpcError, method};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::mcp::{LedgerRpc, McpSession};
use crate::relay::RelayError;

/// Batas waktu membuka koneksi ke daemon.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Batas waktu satu panggilan tool ke daemon.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Conn = (SplitSink<Ws, Message>, SplitStream<Ws>);

/// Client RPC berurutan di atas koneksi kapabilitas. Koneksi dibuka saat
/// panggilan pertama, sehingga daemon yang tidak terjangkau muncul sebagai
/// error tool, bukan server MCP yang gagal start.
struct WsLedgerRpc {
    addr: SocketAddr,
    token: SecretString,
    conn: Mutex<Option<Conn>>,
    next_id: AtomicU64,
}

impl WsLedgerRpc {
    fn new(addr: SocketAddr, token: SecretString) -> Self {
        Self {
            addr,
            token,
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
            client: "mandord-mcp".to_owned(),
        };
        let params = serde_json::to_value(hello).map_err(|e| internal(&e.to_string()))?;
        self.exchange(&mut conn, method::HELLO, params).await?;
        Ok(conn)
    }

    async fn request(&self, name: &str, params: Value) -> Result<Value, RpcError> {
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
            let resp: Response =
                serde_json::from_str(text.as_str()).map_err(|e| internal(&e.to_string()))?;
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

impl LedgerRpc for WsLedgerRpc {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.request(method, params).await
    }
}

/// Melayani MCP satu pesan per baris dari `input` ke `output` sampai input
/// habis.
///
/// # Errors
/// [`RelayError`] bila I/O stdio gagal. Kegagalan daemon dilaporkan sebagai
/// error tool.
pub async fn mcp_lines<R, W>(
    addr: SocketAddr,
    token: SecretString,
    input: R,
    mut output: W,
) -> Result<(), RelayError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut session = McpSession::new(WsLedgerRpc::new(addr, token));
    let mut lines = BufReader::new(input).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| RelayError::Io(e.to_string()))?
    {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = session.handle(&line).await {
            output
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .map_err(|e| RelayError::Io(e.to_string()))?;
            output
                .flush()
                .await
                .map_err(|e| RelayError::Io(e.to_string()))?;
        }
    }
    Ok(())
}

fn unreachable_daemon(reason: &str) -> RpcError {
    internal(&format!("the Mandor daemon is unreachable: {reason}"))
}

fn closed(reason: &str) -> RpcError {
    RpcError::new(
        ErrorCode::Unauthorized,
        format!("the ledger connection was closed: {reason}"),
    )
}

fn internal(message: &str) -> RpcError {
    RpcError::new(ErrorCode::Internal, message)
}
