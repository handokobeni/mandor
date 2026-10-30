//! Port `DesignSources` usecase di atas tabel `design_sources` dan git
//! (plan 2026-10-30-001, KTD-8): resolusi folder dan pembaruan repo desain
//! sebelum kartu VM dimulai.

use std::path::{Path, PathBuf};

use mandor_git::Git;
use mandor_store::{DesignSourceRow, Store};
use mandor_usecase::PortError;
use mandor_usecase::review::{DesignCheck, DesignRoot, DesignSources, SourceCheck};

use crate::dispatch::design::{check, folder_path};

/// Batas waktu memeriksa berkas sumber desain.
const FS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Adapter sumber desain milik daemon.
#[derive(Debug, Clone)]
pub struct DesignStore {
    store: Store,
    git: Git,
    /// Folder milik user daemon (`~/.mandor`); `.git` sumber desain harus
    /// dimiliki user yang sama agar boleh di-pull.
    owner_ref: PathBuf,
}

impl DesignStore {
    #[must_use]
    pub fn new(store: Store, git: Git, owner_ref: PathBuf) -> Self {
        Self {
            store,
            git,
            owner_ref,
        }
    }

    /// Pull fast-forward untuk folder yang berupa repo git; `Some(alasan)`
    /// bila gagal atau dilewati.
    async fn refresh(&self, path: &Path) -> Option<String> {
        let dot_git = path.join(".git");
        let is_repo = tokio::time::timeout(FS_TIMEOUT, tokio::fs::metadata(&dot_git))
            .await
            .is_ok_and(|m| m.is_ok());
        if !is_repo {
            return None;
        }
        let owned = tokio::time::timeout(FS_TIMEOUT, same_owner(&dot_git, &self.owner_ref))
            .await
            .unwrap_or(false);
        if !owned {
            return Some("its .git folder is not owned by the Mandor user".to_owned());
        }
        let token = if self.git.distrusts_repo_config() {
            match self.git.gh_active_token().await {
                Ok(t) => Some(t),
                Err(e) => return Some(without_urls(&e.to_string())),
            }
        } else {
            None
        };
        self.git
            .update_design(path, token.as_ref())
            .await
            .err()
            .map(|e| without_urls(&e.to_string()))
    }

    async fn prepare_one(&self, row: &DesignSourceRow, refresh: bool, out: &mut DesignCheck) {
        let path = folder_path(row);
        if refresh {
            if let Some(why) = self.refresh(Path::new(&path)).await {
                out.stale.push((row.name.clone(), why));
            }
        }
        match check(&path).await {
            SourceCheck::Ready(canonical) => out.roots.push(DesignRoot {
                id: row.id.clone(),
                name: row.name.clone(),
                path: canonical,
            }),
            other => out
                .unavailable
                .push((row.name.clone(), other.message().to_owned())),
        }
    }
}

impl DesignSources for DesignStore {
    async fn prepare(&self, ids: &[String], refresh: bool) -> Result<DesignCheck, PortError> {
        let mut out = DesignCheck::default();
        if ids.is_empty() {
            return Ok(out);
        }
        let rows = self
            .store
            .design_sources()
            .await
            .map_err(|e| PortError::Storage(e.to_string()))?;
        for id in ids {
            match rows.iter().find(|r| &r.id == id) {
                Some(row) => self.prepare_one(row, refresh, &mut out).await,
                None => out
                    .unavailable
                    .push((id.clone(), "it was removed".to_owned())),
            }
        }
        Ok(out)
    }
}

/// `true` bila `path` dimiliki user yang sama dengan `reference`.
#[cfg(unix)]
async fn same_owner(path: &Path, reference: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (
        tokio::fs::symlink_metadata(path).await,
        tokio::fs::metadata(reference).await,
    ) {
        (Ok(a), Ok(b)) => a.uid() == b.uid(),
        _ => false,
    }
}

/// Windows: tidak ada daemon VM; kepemilikan tidak diperiksa.
#[cfg(not(unix))]
async fn same_owner(_path: &Path, _reference: &Path) -> bool {
    true
}

/// Pesan kegagalan tanpa URL remote, untuk laporan pagi.
fn without_urls(message: &str) -> String {
    message
        .split_whitespace()
        .map(|w| if w.contains("://") { "<remote>" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    async fn adapter(dir: &Path) -> DesignStore {
        let store = Store::open(&dir.join("mandor.db")).await.unwrap();
        let git = Git::new(mandor_git::GitConfig::new(dir.join("ws")));
        DesignStore::new(store, git, dir.to_path_buf())
    }

    fn row(id: &str, name: &str, path: &Path) -> DesignSourceRow {
        DesignSourceRow {
            id: id.to_owned(),
            name: name.to_owned(),
            kind: "folder".to_owned(),
            config_json: serde_json::json!({ "path": path }).to_string(),
            created_at_ms: 1,
        }
    }

    #[tokio::test]
    async fn folder_biasa_siap_dan_yang_hilang_atau_dihapus_tidak_tersedia() {
        let dir = tempfile::tempdir().unwrap();
        let a = adapter(dir.path()).await;
        let mock = dir.path().join("mock");
        std::fs::create_dir(&mock).unwrap();
        a.store
            .insert_design_source(&row("d1", "kelola-mock", &mock))
            .await
            .unwrap();
        a.store
            .insert_design_source(&row("d2", "lama", &dir.path().join("hilang")))
            .await
            .unwrap();

        let ids = ["d1", "d2", "d3"].map(str::to_owned);
        let got = a.prepare(&ids, true).await.unwrap();
        assert_eq!(got.roots.len(), 1);
        assert_eq!(got.roots[0].name, "kelola-mock");
        assert_eq!(got.roots[0].path, std::fs::canonicalize(&mock).unwrap());
        assert!(got.stale.is_empty(), "folder biasa tidak di-pull");
        let names: Vec<&str> = got.unavailable.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["lama", "d3"]);
        assert!(got.refusal().unwrap().starts_with("Design source \"lama\""));
    }

    #[tokio::test]
    async fn repo_tanpa_remote_gagal_diperbarui_tetapi_tetap_dipakai() {
        let dir = tempfile::tempdir().unwrap();
        let a = adapter(dir.path()).await;
        let mock = dir.path().join("mock");
        std::fs::create_dir(&mock).unwrap();
        let ok = std::process::Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(&mock)
            .status()
            .unwrap();
        assert!(ok.success());
        a.store
            .insert_design_source(&row("d1", "kelola-mock", &mock))
            .await
            .unwrap();
        let got = a.prepare(&["d1".to_owned()], true).await.unwrap();
        assert_eq!(got.roots.len(), 1);
        assert_eq!(got.stale.len(), 1);
        assert_eq!(
            got.notes()[0].split(':').next(),
            Some("Design kelola-mock not updated")
        );
    }

    #[test]
    fn url_remote_tidak_ikut_ke_laporan() {
        assert_eq!(
            without_urls("fetch failed: unable to access 'https://github.com/a/b.git/': timeout"),
            "fetch failed: unable to access <remote> timeout"
        );
    }
}
