//! `daemon.json`: alamat dan token daemon yang sedang berjalan.
//! Ditulis atomik dengan izin 0600 di direktori 0700 (KTD-2, KTD-17).

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Isi `daemon.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub protocol: String,
    pub version: String,
    pub started_at_ms: i64,
}

fn path(home: &Path) -> PathBuf {
    home.join("daemon.json")
}

/// Membuat direktori data dengan izin 0700.
///
/// # Errors
/// Gagal bila direktori tidak bisa dibuat atau izinnya tidak bisa diatur.
pub async fn ensure_home(home: &Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(home)
        .await
        .with_context(|| format!("membuat {}", home.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(home, std::fs::Permissions::from_mode(0o700))
            .await
            .with_context(|| format!("mengatur izin {}", home.display()))?;
    }
    Ok(())
}

/// Menulis `daemon.json` secara atomik (file sementara lalu rename) dengan izin 0600.
///
/// # Errors
/// Gagal bila file tidak bisa ditulis.
pub async fn write(home: &Path, info: &DaemonInfo) -> anyhow::Result<()> {
    let final_path = path(home);
    let tmp = home.join(format!("daemon.json.{}.tmp", info.pid));
    let body = serde_json::to_vec_pretty(info).context("serializing daemon.json")?;
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts
        .open(&tmp)
        .await
        .with_context(|| format!("membuka {}", tmp.display()))?;
    tokio::io::AsyncWriteExt::write_all(&mut file, &body)
        .await
        .context("writing daemon.json")?;
    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .context("flush daemon.json")?;
    drop(file);
    tokio::fs::rename(&tmp, &final_path)
        .await
        .context("replacing daemon.json")?;
    Ok(())
}

/// Membaca `daemon.json`; `None` bila tidak ada atau rusak.
pub async fn read(home: &Path) -> Option<DaemonInfo> {
    let body = tokio::fs::read(path(home)).await.ok()?;
    serde_json::from_slice(&body).ok()
}

/// Menghapus `daemon.json` milik `pid` (bila masih milik pid itu).
pub async fn remove_if_owned(home: &Path, pid: u32) {
    if read(home).await.is_some_and(|i| i.pid == pid) {
        let _ = tokio::fs::remove_file(path(home)).await;
    }
}
