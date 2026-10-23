//! Proses `mandord mcp`: server MCP stdio milik agent yang meneruskan tool
//! ledger ke daemon lewat koneksi kapabilitas (KTD-26).

use std::net::SocketAddr;

use mandor_protocol::RpcError;
use secrecy::SecretString;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::client::DaemonClient;
use crate::mcp::{LedgerRpc, McpSession};
use crate::relay::RelayError;

impl LedgerRpc for DaemonClient {
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
    let mut session = McpSession::new(DaemonClient::new(addr, token, "mandord-mcp"));
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
