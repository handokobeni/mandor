//! Penyimpanan token user di berkas 0600 (direktori 0700), ditulis atomik.
//!
//! Secret Service (keyring OS) tidak dipakai: store yang tersedia memasukkan
//! runtime async kedua dan dependensi yang tidak dirawat, sehingga melanggar
//! gerbang `cargo deny` standar `rust-engineering`.

use std::path::PathBuf;

use crate::{LarkError, TokenSet};

/// Penyimpan token per pengguna.
#[derive(Debug, Clone)]
pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Membaca token; `None` bila belum login.
    ///
    /// # Errors
    /// [`LarkError::Io`] bila berkas ada tetapi rusak.
    pub async fn load(&self) -> Result<Option<TokenSet>, LarkError> {
        match tokio::fs::read(&self.path).await {
            Ok(body) => serde_json::from_slice(&body)
                .map(Some)
                .map_err(|e| LarkError::Io(format!("token file is corrupt: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(LarkError::Io(e.to_string())),
        }
    }

    /// Menyimpan token secara atomik (berkas sementara 0600 lalu rename).
    ///
    /// # Errors
    /// [`LarkError::Io`] bila berkas tidak bisa ditulis.
    pub async fn save(&self, tokens: &TokenSet) -> Result<(), LarkError> {
        let io = |e: std::io::Error| LarkError::Io(e.to_string());
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await.map_err(io)?;
        }
        let body = serde_json::to_vec(tokens).map_err(|e| LarkError::Io(e.to_string()))?;
        let tmp = self.path.with_extension("tmp");
        let mut opts = tokio::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(&tmp).await.map_err(io)?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &body)
            .await
            .map_err(io)?;
        file.sync_all().await.map_err(io)?;
        drop(file);
        tokio::fs::rename(&tmp, &self.path).await.map_err(io)?;
        Ok(())
    }

    /// Menghapus token (logout / offboarding).
    ///
    /// # Errors
    /// [`LarkError::Io`] bila penghapusan gagal selain karena berkas tidak ada.
    pub async fn clear(&self) -> Result<(), LarkError> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(LarkError::Io(e.to_string())),
        }
    }
}
