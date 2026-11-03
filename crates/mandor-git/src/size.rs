//! Ukuran folder worktree untuk pembersihan run selesai (plan
//! `2026-11-03-001`, KTD-C3).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::GitError;

/// Batas waktu mengukur satu folder.
pub const SIZE_TIMEOUT: Duration = Duration::from_secs(60);

/// Jumlah ukuran berkas di bawah `root` tanpa mengikuti symlink, dijalankan
/// di thread blocking dan dibatasi `timeout`. Entri yang tidak terbaca
/// dilewati; folder yang sudah hilang tidak memakan ruang, jadi berukuran 0.
///
/// # Errors
/// `root` tidak terbaca, atau pengukuran melewati batas waktu.
pub async fn dir_size(root: &Path, timeout: Duration) -> Result<u64, GitError> {
    let root = root.to_path_buf();
    let task = tokio::task::spawn_blocking(move || walk(&root));
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => Err(GitError::Io(format!("size task failed: {e}"))),
        Err(_) => Err(GitError::Io(
            "measuring the folder took too long".to_owned(),
        )),
    }
}

fn walk(root: &Path) -> Result<u64, GitError> {
    let meta = match std::fs::symlink_metadata(root) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(GitError::Io(e.to_string())),
    };
    if !meta.is_dir() {
        return Ok(meta.len());
    }
    let mut total = 0u64;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[tokio::test]
    async fn menjumlah_berkas_bertingkat_tanpa_mengikuti_symlink() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), vec![0u8; 100]).unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/x")).unwrap();
        std::fs::write(dir.path().join("node_modules/x/b.js"), vec![0u8; 250]).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("big"), vec![0u8; 10_000]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        assert_eq!(dir_size(dir.path(), SIZE_TIMEOUT).await.unwrap(), 350);
    }

    #[tokio::test]
    async fn folder_yang_sudah_hilang_berukuran_nol() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            dir_size(&dir.path().join("hilang"), SIZE_TIMEOUT)
                .await
                .unwrap(),
            0
        );
    }
}
