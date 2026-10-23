//! Adapter API `mandord` (KTD-2).
//!
//! - [`serve`]: server JSON-RPC 2.0 lewat WebSocket di `127.0.0.1`, dengan
//!   `hello` (token + versi protokol) sebagai pesan pertama setiap koneksi.
//! - [`relay_stdio`]: relay baris JSON antara stdio dan server, dipakai
//!   subcommand `mandord connect` (default di mode WSL).
//!
//! Crate ini satu-satunya pengguna `axum` selain `mandord`.

mod client;
mod mcp;
mod mcp_client;
mod relay;
mod server;

pub use client::DaemonClient;
pub use mcp::{LedgerRpc, McpSession};
pub use mcp_client::mcp_lines;
pub use relay::{RelayError, relay_lines};
pub use server::{CapabilityScope, Dispatch, ServerConfig, serve};

/// Relay stdio proses ke server di `addr`.
///
/// # Errors
/// [`RelayError`] bila koneksi atau I/O gagal.
pub async fn relay_stdio(addr: std::net::SocketAddr) -> Result<(), RelayError> {
    relay_lines(addr, tokio::io::stdin(), tokio::io::stdout()).await
}

/// Server MCP stdio untuk agent (`mandord mcp`).
///
/// # Errors
/// [`RelayError`] bila koneksi, handshake, atau I/O gagal.
pub async fn mcp_stdio(
    addr: std::net::SocketAddr,
    token: secrecy::SecretString,
) -> Result<(), RelayError> {
    mcp_lines(addr, token, tokio::io::stdin(), tokio::io::stdout()).await
}
