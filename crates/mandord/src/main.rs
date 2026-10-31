//! `mandord`: daemon mandor.
//!
//! Subcommand:
//! - `serve` (default): menjalankan daemon.
//! - `connect`: relay stdio ke daemon, menjalankan daemon terlepas bila belum ada.
//! - `token`: mencetak token `hello` (menjalankan daemon bila belum ada); dipakai
//!   client yang tidak bisa membaca `daemon.json` langsung (mode WSL).
//! - `mcp`: server MCP ledger (stdio) untuk satu giliran agent.
//! - `hook`: hook `PreToolUse` Claude Code untuk agent; fail-closed (exit 2).
//! - `agent-exec`: pengawas agent sebagai user Linux terpisah di VM (plan VM U6).
//! - `login lark`: login Lark dari terminal (SSH) ke daemon yang berjalan
//!   (plan VM U8).
//! - `install [--check] [--keep-awake-cmd <json>]` dan `uninstall [--force]`:
//!   memasang atau melepas daemon sebagai service systemd user.

#[cfg(unix)]
mod agent_exec;
mod app;
mod apps;
mod config;
mod connect;
mod design_port;
mod discovery;
mod dispatch;
mod hook;
#[cfg(target_os = "linux")]
mod install;
mod login;
mod logins;
mod mcp;
mod power;
mod prereq;
mod prod;
mod review_state;
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
    // Pengawas agent berjalan sebagai user agent tanpa konfigurasi daemon.
    #[cfg(unix)]
    if args.first().map(String::as_str) == Some("agent-exec") {
        return match agent_exec::run(&args[1..]).await {
            Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            Err(err) => fail(&err),
        };
    }
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(err) => return fail(&err),
    };
    let result = match args.first().map(String::as_str) {
        None | Some("serve") => serve::run(cfg).await,
        Some("connect") => connect::run(&cfg).await,
        Some("token") => connect::print_token(&cfg).await,
        Some("mcp") => mcp::run(&cfg).await,
        Some("login") => login::run(&cfg, &args[1..]).await,
        Some("--version") => print_version().await,
        // Pemasangan sebagai service systemd hanya untuk daemon Linux (WSL,
        // tahap 2); macOS memakai launchd di tahap 3.
        #[cfg(target_os = "linux")]
        Some("install") => return exit(install::run_install(&cfg, &args[1..]).await),
        #[cfg(target_os = "linux")]
        Some("uninstall") => return exit(install::run_uninstall(&cfg, &args[1..]).await),
        Some(other) => Err(anyhow::anyhow!(
            "unknown subcommand: {other} (use serve, connect, token, mcp, login, install, uninstall, or hook)"
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => fail(&err),
    }
}

/// Kode keluar `install`/`uninstall`: hasilnya sudah tercetak sebagai JSON;
/// hasil `error` keluar dengan kode 1.
#[cfg(target_os = "linux")]
fn exit(result: anyhow::Result<bool>) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => fail(&err),
    }
}

/// Satu baris `mandord <versi>` untuk dibaca mesin (installer, app).
async fn print_version() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use tokio::io::AsyncWriteExt as _;
    let mut out = tokio::io::stdout();
    out.write_all(format!("mandord {VERSION}\n").as_bytes())
        .await
        .context("writing the version")?;
    out.flush().await.context("flush version")
}

fn fail(err: &anyhow::Error) -> ExitCode {
    // Log mungkin belum terpasang; tulis langsung ke stderr lewat tracing default.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .try_init();
    tracing::error!(error = format!("{err:#}"), "mandord gagal");
    ExitCode::FAILURE
}
