use std::net::SocketAddr;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio_tungstenite::tungstenite::Message;

/// Batas waktu membuka koneksi ke daemon.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Kegagalan relay.
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("cannot connect to the daemon at {addr}: {reason}")]
    Connect { addr: SocketAddr, reason: String },
    #[error("connection to the daemon was lost: {0}")]
    Io(String),
}

/// Meneruskan baris JSON dari `input` ke server dan pesan server ke `output`,
/// satu pesan per baris. Relay tidak menyisipkan token: client tetap harus
/// mengirim `hello` yang valid.
///
/// # Errors
/// [`RelayError`] bila koneksi gagal atau terputus tidak normal.
pub async fn relay_lines<R, W>(addr: SocketAddr, input: R, mut output: W) -> Result<(), RelayError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let url = format!("ws://{addr}/rpc");
    let (ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(&url))
        .await
        .map_err(|_| RelayError::Connect {
            addr,
            reason: "timeout".to_owned(),
        })?
        .map_err(|e| RelayError::Connect {
            addr,
            reason: e.to_string(),
        })?;
    let (mut sink, mut stream) = ws.split();
    let mut lines = BufReader::new(input).lines();

    loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    sink.send(Message::Text(line.into()))
                        .await
                        .map_err(|e| RelayError::Io(e.to_string()))?;
                }
                Ok(None) => {
                    let _ = sink.close().await;
                    return Ok(());
                }
                Err(e) => return Err(RelayError::Io(e.to_string())),
            },
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(body))) => {
                    output.write_all(body.as_bytes()).await.map_err(|e| RelayError::Io(e.to_string()))?;
                    output.write_all(b"\n").await.map_err(|e| RelayError::Io(e.to_string()))?;
                    output.flush().await.map_err(|e| RelayError::Io(e.to_string()))?;
                }
                Some(Ok(Message::Close(_))) | None => return Ok(()),
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(RelayError::Io(e.to_string())),
            },
        }
    }
}
