use std::path::{Path, PathBuf};

use mandor_proc::git_credential_env;
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

    /// Memperbarui folder sumber desain yang berupa repo git (plan
    /// 2026-10-30-001, KTD-8): fetch branch yang sedang aktif lalu fast-forward
    /// saja. Di VM konfigurasi repo diperiksa dan kunci yang mengeksekusi
    /// dinetralkan seperti git daemon lainnya (KTD-65).
    ///
    /// # Errors
    /// HEAD terlepas, fetch gagal, atau riwayat lokal bercabang dari remote.
    pub async fn update_design(
        &self,
        dir: &Path,
        token: Option<&SecretString>,
    ) -> Result<(), GitError> {
        let head = self
            .ok(
                "rev-parse",
                self.git(dir).args(["rev-parse", "--abbrev-ref", "HEAD"]),
            )
            .await?;
        let branch = head.stdout.trim();
        if branch.is_empty() || branch == "HEAD" {
            return Err(GitError::Command {
                op: "update design",
                stderr: "the design folder is not on a branch".to_owned(),
            });
        }
        self.validate_branch(dir, branch).await?;
        self.fetch(dir, branch, token).await?;
        self.ok(
            "merge --ff-only",
            self.git(dir).args(["merge", "--ff-only", "FETCH_HEAD"]),
        )
        .await
        .map(drop)
    }

    /// Membuat worktree `dir_name` di bawah direktori workspace pada branch baru
    /// `branch` dari `origin/<base>` terbaru (KTD-9).
    ///
    /// Branch yang sudah ada secara lokal (run lama yang sudah ditutup, mis.
    /// karena worktree-nya dipertahankan Close run) dilanjutkan: worktree yang
    /// sudah memegang branch itu dipakai apa adanya, atau worktree dibuat ulang
    /// dari branch itu. Bila gagal di tengah jalan, hanya folder dan branch yang
    /// dibuat oleh percobaan ini yang dihapus.
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
        let branch_existed = self.local_branch_exists(repo, branch).await?;
        if branch_existed {
            if let Some(existing) = self.worktree_of(repo, branch).await? {
                // Clone milik dev (atau folder lain di luar direktori worktree
                // daemon) tidak pernah menjadi folder kerja agent.
                if !existing.starts_with(&self.cfg.workspaces_dir) {
                    return Err(GitError::BranchCheckedOutElsewhere {
                        branch: branch.to_owned(),
                        path: existing.display().to_string(),
                    });
                }
                return Ok(Worktree {
                    path: existing,
                    branch: branch.to_owned(),
                });
            }
        }
        let path_existed = tokio::fs::symlink_metadata(&path).await.is_ok();
        let path_str = path.to_string_lossy().into_owned();
        let start = format!("origin/{base}");
        let cmd = if branch_existed {
            self.git(repo).args(["worktree", "add", &path_str, branch])
        } else {
            self.git(repo)
                .args(["worktree", "add", "-b", branch, &path_str, &start])
        };
        if let Err(err) = self.ok("worktree add", cmd).await {
            if !path_existed {
                self.remove_worktree(repo, &path, None).await;
            }
            if !branch_existed {
                self.delete_branch(repo, branch).await;
            }
            return Err(err);
        }
        Ok(Worktree {
            path,
            branch: branch.to_owned(),
        })
    }

    /// Branch lokal `branch` ada.
    async fn local_branch_exists(&self, repo: &Path, branch: &str) -> Result<bool, GitError> {
        Ok(self
            .exec(&self.git(repo).args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ]))
            .await?
            .success())
    }

    /// Folder worktree yang sedang memegang `branch`, bila ada dan foldernya masih ada.
    async fn worktree_of(&self, repo: &Path, branch: &str) -> Result<Option<PathBuf>, GitError> {
        let out = self
            .ok(
                "worktree list",
                self.git(repo).args(["worktree", "list", "--porcelain"]),
            )
            .await?;
        let want = format!("branch refs/heads/{branch}");
        let mut current: Option<PathBuf> = None;
        for line in out.stdout.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                current = Some(PathBuf::from(p));
            } else if line == want {
                if let Some(p) = current.take().filter(|p| p.exists()) {
                    return Ok(Some(p));
                }
            }
        }
        Ok(None)
    }

    /// Menghapus branch lokal; kegagalan selain "tidak ada" dicatat ke log.
    async fn delete_branch(&self, repo: &Path, branch: &str) {
        if let Ok(out) = self
            .exec(&self.git(repo).args(["branch", "-D", branch]))
            .await
        {
            if !out.success() && !out.stderr.contains("not found") {
                tracing::warn!(branch, stderr = %out.stderr.trim(), "branch worktree tidak terhapus");
            }
        }
    }

    /// Menghapus worktree (dan branch lokalnya bila diberikan). Tidak pernah gagal:
    /// sisa yang tidak bisa dihapus dicatat ke log.
    pub async fn remove_worktree(&self, repo: &Path, path: &Path, branch: Option<&str>) {
        let path_str = path.to_string_lossy().into_owned();
        let removed = self
            .exec(
                &self
                    .git(repo)
                    .args(["worktree", "remove", "--force", &path_str]),
            )
            .await;
        if !matches!(&removed, Ok(o) if o.success()) {
            let _ = tokio::fs::remove_dir_all(path).await;
            let _ = self.exec(&self.git(repo).args(["worktree", "prune"])).await;
        }
        if let Some(b) = branch {
            self.delete_branch(repo, b).await;
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
        let local_exists = self
            .exec(&self.git(repo).args([
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
