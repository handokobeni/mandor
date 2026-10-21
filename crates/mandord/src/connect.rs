//! `mandord connect`: relay stdio ke daemon. Bila daemon belum berjalan,
//! daemon dijalankan terlepas penuh dari sesi ini (setsid) sehingga menutup
//! relay atau UI tidak menghentikan daemon (KTD-18).

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::Context;
use mandor_api::relay_stdio;

use crate::config::Config;
use crate::discovery::{self, DaemonInfo};

/// Batas waktu daemon baru menulis `daemon.json`.
const START_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Menjalankan relay.
///
/// # Errors
/// Gagal bila daemon tidak bisa dijalankan atau koneksi gagal.
pub async fn run(cfg: &Config) -> anyhow::Result<()> {
    let info = match reachable(cfg).await {
        Some(info) => info,
        None => start_detached(cfg).await?,
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, info.port));
    relay_stdio(addr).await.context("relaying to the daemon")?;
    Ok(())
}

/// Mencetak token daemon ke stdout (satu baris JSON `{"token": ...}`).
///
/// # Errors
/// Gagal bila daemon tidak bisa dijalankan.
pub async fn print_token(cfg: &Config) -> anyhow::Result<()> {
    let info = match reachable(cfg).await {
        Some(info) => info,
        None => start_detached(cfg).await?,
    };
    let line = serde_json::json!({ "token": info.token, "protocol": info.protocol }).to_string();
    let mut out = tokio::io::stdout();
    tokio::io::AsyncWriteExt::write_all(&mut out, format!("{line}\n").as_bytes())
        .await
        .context("writing the token")?;
    tokio::io::AsyncWriteExt::flush(&mut out)
        .await
        .context("flush token")?;
    Ok(())
}

async fn reachable(cfg: &Config) -> Option<DaemonInfo> {
    let info = discovery::read(&cfg.home).await?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, info.port));
    let probe = tokio::time::timeout(Duration::from_secs(2), tokio::net::TcpStream::connect(addr));
    matches!(probe.await, Ok(Ok(_))).then_some(info)
}

async fn start_detached(cfg: &Config) -> anyhow::Result<DaemonInfo> {
    let exe = std::env::current_exe().context("locating the mandord binary")?;
    spawn_detached(&exe).context("starting mandord serve")?;
    let deadline = tokio::time::Instant::now() + START_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        if let Some(info) = reachable(cfg).await {
            return Ok(info);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    anyhow::bail!(
        "mandord was not ready within {START_TIMEOUT:?}; check {}",
        cfg.log_path().display()
    )
}

#[cfg(unix)]
fn spawn_detached(exe: &std::path::Path) -> std::io::Result<()> {
    use process_wrap::tokio::{CommandWrap, ProcessSession};
    use std::process::Stdio;
    let mut cmd = CommandWrap::with_new(exe, |c| {
        c.arg("serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    });
    cmd.wrap(ProcessSession);
    // Sengaja tidak ditunggu: daemon hidup lebih lama dari relay ini.
    let child = cmd.spawn()?;
    drop(child);
    Ok(())
}

#[cfg(windows)]
fn spawn_detached(exe: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::process::Command::new(exe)
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(drop)
}
