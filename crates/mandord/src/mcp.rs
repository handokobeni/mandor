//! `mandord mcp`: server MCP ledger untuk satu giliran agent (KTD-26), atau
//! dengan `--permit` alat persetujuan Claude Code (plan 2026-11-06-001).
//!
//! Dijalankan Claude Code dari konfigurasi MCP giliran; port daemon dan token
//! kapabilitas datang lewat environment, bukan argv. Stdout adalah kanal
//! protokol, jadi tidak ada log ke stdout.

use std::net::{Ipv4Addr, SocketAddr};

use anyhow::Context;

use crate::config::Config;

/// Menjalankan server MCP stdio sampai agent menutup stdin.
///
/// # Errors
/// Gagal bila port atau token tidak ada, atau koneksi ke daemon gagal.
pub async fn run(cfg: &Config, permit: bool) -> anyhow::Result<()> {
    let port = cfg.mcp_port.context("MANDOR_MCP_PORT is not set")?;
    let token = cfg
        .mcp_token
        .clone()
        .context("MANDOR_MCP_TOKEN is not set")?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    if permit {
        return mandor_api::permit_stdio(addr, token)
            .await
            .context("serving the approval tool");
    }
    mandor_api::mcp_stdio(addr, token)
        .await
        .context("serving the ledger tools")
}
