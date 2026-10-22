//! `mandord`: daemon mandor.
//!
//! Subcommand:
//! - `serve` (default): menjalankan daemon.
//! - `connect`: relay stdio ke daemon, menjalankan daemon terlepas bila belum ada.
//! - `token`: mencetak token `hello` (menjalankan daemon bila belum ada); dipakai
//!   client yang tidak bisa membaca `daemon.json` langsung (mode WSL).
//! - `mcp`: server MCP ledger (stdio) untuk satu giliran agent.
//! - `hook`: hook `PreToolUse` Claude Code untuk agent; fail-closed (exit 2).

mod app;
mod config;
mod connect;
mod discovery;
mod dispatch;
mod hook;
mod mcp;
mod prereq;
mod prod;
mod serve;
mod session;
mod settings;
mod sync_status;
mod views;

use std::process::ExitCode;

use config::Config;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Hook ditangani sebelum konfigurasi dan runtime: kegagalan apa pun di jalur
    // ini harus berakhir exit 2 (menolak), bukan exit 1 yang dianggap
    // non-blocking oleh Claude Code.
    if args.first().map(String::as_str) == Some("hook") {
        return ExitCode::from(hook::run(&args[1..]));
    }
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt.block_on(run(args)),
        Err(err) => fail(&anyhow::Error::new(err).context("failed to build the tokio runtime")),
    }
}

async fn run(args: Vec<String>) -> ExitCode {
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(err) => return fail(&err),
    };
    let result = match args.first().map(String::as_str) {
        None | Some("serve") => serve::run(cfg).await,
        Some("connect") => connect::run(&cfg).await,
        Some("token") => connect::print_token(&cfg).await,
        Some("mcp") => mcp::run(&cfg).await,
        Some("--version") => {
            tracing_subscriber::fmt().init();
            tracing::info!(version = VERSION, "mandord");
            Ok(())
        }
        Some(other) => Err(anyhow::anyhow!(
            "unknown subcommand: {other} (use serve, connect, token, mcp, or hook)"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => fail(&err),
    }
}

fn fail(err: &anyhow::Error) -> ExitCode {
    // Log mungkin belum terpasang; tulis langsung ke stderr lewat tracing default.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .try_init();
    tracing::error!(error = format!("{err:#}"), "mandord gagal");
    ExitCode::FAILURE
}
