use std::path::Path;

use mandor_proc::{git_credential_env, run};
use secrecy::SecretString;

use crate::{Git, GitError, NETWORK_TIMEOUT};

pub use mandor_domain::run::touches_sensitive_path;

impl Git {
    /// Commit semua perubahan worktree dengan `message` (tanpa hook).
    /// Mengembalikan hash pendek, atau `None` bila tidak ada yang di-commit.
    ///
    /// # Errors
    /// [`GitError`] bila `git add`/`git commit` gagal.
    pub async fn commit_all(
        &self,
        worktree: &Path,
        message: &str,
    ) -> Result<Option<String>, GitError> {
        self.ok("add", self.git(worktree).args(["add", "-A"]))
            .await?;
        let staged = run(&self.git(worktree).args(["diff", "--cached", "--quiet"])).await?;
        if staged.success() {
            return Ok(None);
        }
        self.ok(
            "commit",
            self.git(worktree)
                .args(["commit", "--no-verify", "-F", "-"])
                .stdin_bytes(message.as_bytes().to_vec()),
        )
        .await?;
        let head = self
            .ok(
                "rev-parse",
                self.git(worktree).args(["rev-parse", "--short", "HEAD"]),
            )
            .await?;
        Ok(Some(head.stdout.trim().to_owned()))
    }

    /// Push HEAD ke `origin/<task_branch>`. Ditolak sebelum memanggil git bila
    /// branch worktree saat ini bukan branch task.
    ///
    /// # Errors
    /// [`GitError::WrongBranch`] bila branch tidak cocok; [`GitError::Command`] bila push gagal.
    pub async fn push(
        &self,
        worktree: &Path,
        task_branch: &str,
        token: Option<&SecretString>,
    ) -> Result<(), GitError> {
        let current = self
            .ok(
                "branch --show-current",
                self.git(worktree).args(["branch", "--show-current"]),
            )
            .await?;
        let current = current.stdout.trim();
        if current != task_branch {
            return Err(GitError::WrongBranch {
                expected: task_branch.to_owned(),
                actual: current.to_owned(),
            });
        }
        let refspec = format!("HEAD:refs/heads/{task_branch}");
        let mut cmd = self
            .git(worktree)
            .args(["push", "--no-verify", "-u", "origin", &refspec])
            .timeout(NETWORK_TIMEOUT);
        if let Some(t) = token {
            for (k, v) in git_credential_env(t) {
                cmd = cmd.env(k, v);
            }
        }
        self.ok("push", cmd).await.map(drop)
    }
}
