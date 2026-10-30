use std::path::Path;

use crate::{Git, GitError};

impl Git {
    /// Default branch dari ref remote saja (`origin/HEAD`, lalu `origin/main`,
    /// lalu `origin/master`); tidak pernah dari branch lokal.
    ///
    /// # Errors
    /// [`GitError::NotARepo`] bila `repo` bukan repo git, [`GitError::NoDefaultBranch`]
    /// bila tidak ada satu pun.
    pub async fn default_branch(&self, repo: &Path) -> Result<String, GitError> {
        let inside = self
            .exec(&self.git(repo).args(["rev-parse", "--is-inside-work-tree"]))
            .await?;
        if !inside.success() {
            return Err(GitError::NotARepo);
        }
        let head = self
            .exec(
                &self
                    .git(repo)
                    .args(["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]),
            )
            .await?;
        if head.success() {
            if let Some(name) = head.stdout.trim().strip_prefix("refs/remotes/origin/") {
                return Ok(name.to_owned());
            }
        }
        for candidate in ["main", "master"] {
            let r = format!("refs/remotes/origin/{candidate}");
            let out = self
                .exec(
                    &self
                        .git(repo)
                        .args(["rev-parse", "--verify", "--quiet", &r]),
                )
                .await?;
            if out.success() {
                return Ok(candidate.to_owned());
            }
        }
        Err(GitError::NoDefaultBranch)
    }

    /// Memastikan `name` adalah nama branch git yang valid.
    ///
    /// # Errors
    /// [`GitError::InvalidBranch`] bila tidak valid.
    pub async fn validate_branch(&self, repo: &Path, name: &str) -> Result<(), GitError> {
        let out = self
            .exec(&self.git(repo).args(["check-ref-format", "--branch", name]))
            .await?;
        if out.success() && !name.starts_with('-') {
            Ok(())
        } else {
            Err(GitError::InvalidBranch(name.to_owned()))
        }
    }
}
