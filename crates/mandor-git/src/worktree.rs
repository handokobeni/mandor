use std::path::{Path, PathBuf};

use mandor_proc::{git_credential_env, run};
use secrecy::SecretString;

use crate::{Git, GitError, NETWORK_TIMEOUT};

/// Worktree task yang sudah dibuat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
}

impl Git {
    /// `git fetch origin <refspec>` dengan kredensial (bila ada).
    ///
    /// # Errors
    /// [`GitError::Command`] bila fetch gagal.
    pub async fn fetch(
        &self,
        repo: &Path,
        refspec: &str,
        token: Option<&SecretString>,
    ) -> Result<(), GitError> {
        let mut cmd = self
            .git(repo)
            .args(["fetch", "--no-tags", "origin", refspec])
            .timeout(NETWORK_TIMEOUT);
        if let Some(t) = token {
            for (k, v) in git_credential_env(t) {
                cmd = cmd.env(k, v);
            }
        }
        self.ok("fetch", cmd).await.map(drop)
    }

    /// Membuat worktree `dir_name` di bawah direktori workspace pada branch baru
    /// `branch` dari `origin/<base>` terbaru. Bila gagal di tengah jalan, folder
    /// dan branch yang sempat dibuat dihapus (KTD-9).
    ///
    /// # Errors
    /// [`GitError`] bila fetch, validasi nama, atau `worktree add` gagal.
    pub async fn create_worktree(
        &self,
        repo: &Path,
        dir_name: &str,
        branch: &str,
        base: &str,
        token: Option<&SecretString>,
    ) -> Result<Worktree, GitError> {
        self.validate_branch(repo, branch).await?;
        self.fetch(repo, base, token).await?;
        let path = self.cfg.workspaces_dir.join(dir_name);
        tokio::fs::create_dir_all(&self.cfg.workspaces_dir)
            .await
            .map_err(|e| GitError::Io(e.to_string()))?;
        let start = format!("origin/{base}");
        let path_str = path.to_string_lossy().into_owned();
        let added = self
            .ok(
                "worktree add",
                self.git(repo)
                    .args(["worktree", "add", "-b", branch, &path_str, &start]),
            )
            .await;
        if let Err(err) = added {
            self.remove_worktree(repo, &path, Some(branch)).await;
            return Err(err);
        }
        Ok(Worktree {
            path,
            branch: branch.to_owned(),
        })
    }

    /// Menghapus worktree (dan branch lokalnya bila diberikan). Tidak pernah gagal:
    /// sisa yang tidak bisa dihapus dicatat ke log.
    pub async fn remove_worktree(&self, repo: &Path, path: &Path, branch: Option<&str>) {
        let path_str = path.to_string_lossy().into_owned();
        let removed = run(&self
            .git(repo)
            .args(["worktree", "remove", "--force", &path_str]))
        .await;
        if !matches!(&removed, Ok(o) if o.success()) {
            let _ = tokio::fs::remove_dir_all(path).await;
            let _ = run(&self.git(repo).args(["worktree", "prune"])).await;
        }
        if let Some(b) = branch {
            if let Ok(out) = run(&self.git(repo).args(["branch", "-D", b])).await {
                if !out.success() && !out.stderr.contains("not found") {
                    tracing::warn!(branch = b, stderr = %out.stderr.trim(), "branch worktree tidak terhapus");
                }
            }
        }
    }

    /// Menghapus folder run workspace (sisa seperti `CLAUDE.md` salinan). Hanya
    /// folder di bawah direktori worktree daemon; path lain diabaikan.
    pub async fn remove_run_root(&self, root: &Path) {
        let inside = root.starts_with(&self.cfg.workspaces_dir) && root != self.cfg.workspaces_dir;
        if !inside {
            tracing::warn!(root = %root.display(), "folder run di luar direktori worktree; tidak dihapus");
            return;
        }
        if let Err(e) = tokio::fs::remove_dir_all(root).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(root = %root.display(), error = %e, "folder run tidak terhapus");
            }
        }
    }

    /// Membuat ulang worktree untuk branch task yang sudah ada di remote (R11a).
    ///
    /// # Errors
    /// [`GitError`] bila fetch atau `worktree add` gagal.
    pub async fn restore_worktree(
        &self,
        repo: &Path,
        dir_name: &str,
        branch: &str,
        token: Option<&SecretString>,
    ) -> Result<Worktree, GitError> {
        self.validate_branch(repo, branch).await?;
        self.fetch(repo, branch, token).await?;
        let path = self.cfg.workspaces_dir.join(dir_name);
        if path.exists() {
            return Ok(Worktree {
                path,
                branch: branch.to_owned(),
            });
        }
        let path_str = path.to_string_lossy().into_owned();
        let remote = format!("origin/{branch}");
        let local_exists = run(&self.git(repo).args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ]))
        .await?
        .success();
        let cmd = if local_exists {
            self.git(repo).args(["worktree", "add", &path_str, branch])
        } else {
            self.git(repo)
                .args(["worktree", "add", "-b", branch, &path_str, &remote])
        };
        self.ok("worktree add", cmd).await?;
        Ok(Worktree {
            path,
            branch: branch.to_owned(),
        })
    }
}
