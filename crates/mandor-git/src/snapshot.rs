//! Snapshot kode worktree berupa tree id git (KTD-28): mengikat hasil review ke
//! kode yang direview tanpa mengubah index atau worktree run.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Git, GitError};

/// Penomor berkas index sementara dalam proses ini.
static NEXT: AtomicU64 = AtomicU64::new(0);

impl Git {
    /// Tree id isi worktree saat ini, termasuk perubahan belum di-commit dan
    /// berkas untracked yang tidak diabaikan. Memakai salinan index agar stat
    /// cache dipakai ulang dan index asli tidak berubah.
    ///
    /// # Errors
    /// [`GitError`] bila perintah git atau penyalinan index gagal.
    pub async fn snapshot_tree(&self, worktree: &Path) -> Result<String, GitError> {
        let index = self.git_path(worktree, "index").await?;
        let git_dir = self.git_path(worktree, "").await?;
        let temp = git_dir.join(format!(
            "mandor-snapshot-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        if tokio::fs::try_exists(&index).await.unwrap_or(false) {
            tokio::fs::copy(&index, &temp)
                .await
                .map_err(|e| GitError::Io(e.to_string()))?;
        }
        let result = self.write_tree(worktree, &temp).await;
        let _ = tokio::fs::remove_file(&temp).await;
        result
    }

    async fn write_tree(&self, worktree: &Path, index: &Path) -> Result<String, GitError> {
        let with_index = |args: &[&str]| {
            self.git(worktree)
                .args(args.iter().copied())
                .env("GIT_INDEX_FILE", index.to_string_lossy().as_ref())
        };
        self.ok("add -A (snapshot)", with_index(&["add", "-A"]))
            .await?;
        let out = self.ok("write-tree", with_index(&["write-tree"])).await?;
        Ok(out.stdout.trim().to_owned())
    }

    /// Berkas yang berbeda antara dua tree.
    ///
    /// # Errors
    /// [`GitError`] bila `git diff` gagal.
    pub async fn changed_between(
        &self,
        worktree: &Path,
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, GitError> {
        let out = self
            .ok(
                "diff --name-only (snapshot)",
                self.git(worktree).args(["diff", "--name-only", from, to]),
            )
            .await?;
        Ok(out.stdout.lines().map(str::to_owned).collect())
    }

    /// Path di dalam direktori git worktree (absolut).
    async fn git_path(&self, worktree: &Path, name: &str) -> Result<PathBuf, GitError> {
        let out = self
            .exec(&self.git(worktree).args([
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                name,
            ]))
            .await?;
        if !out.success() {
            return Err(GitError::Command {
                op: "rev-parse --git-path",
                stderr: out.stderr.trim().to_owned(),
            });
        }
        Ok(PathBuf::from(out.stdout.trim()))
    }
}
