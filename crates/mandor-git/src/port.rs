//! Implementasi port [`GitPort`] usecase di atas [`Git`].

use std::path::{Path, PathBuf};

use mandor_usecase::PortError;
use mandor_usecase::run_ports::{GitPort, PrRef, RepoInfo, WorkChanges};
use secrecy::SecretString;

use crate::{Git, GitError, PrInfo, PrState};

/// PR dari gh menjadi [`PrRef`] untuk usecase.
fn pr_ref(p: PrInfo) -> PrRef {
    PrRef {
        open: p.state == PrState::Open,
        merged: p.state == PrState::Merged,
        url: p.url,
    }
}

/// Batas diff yang dikumpulkan untuk pembuat pesan commit.
const DIFF_MAX: usize = 50_000;

impl From<GitError> for PortError {
    fn from(e: GitError) -> Self {
        match e {
            GitError::Proc(p) => Self::Transient(p.to_string()),
            GitError::Io(m) => Self::Storage(m),
            other => Self::Rejected(other.to_string()),
        }
    }
}

/// Adapter git untuk orkestrasi run.
#[derive(Debug, Clone)]
pub struct GitAdapter {
    git: Git,
}

impl GitAdapter {
    #[must_use]
    pub fn new(git: Git) -> Self {
        Self { git }
    }

    /// Token profil `gh` repo, bila diatur. Token hanya dipakai proses daemon.
    async fn token(&self, repo: &RepoInfo) -> Result<Option<SecretString>, GitError> {
        match &repo.gh_profile {
            Some(p) => self.git.gh_token(p).await.map(Some),
            None => Ok(None),
        }
    }
}

/// Nama folder worktree: `<repo>-<branch>` tanpa pemisah path, atau
/// `<group>/<repo>` untuk run workspace.
fn dir_name(repo: &RepoInfo, branch: &str, group: Option<&str>) -> String {
    match group {
        Some(g) => format!("{}/{}", g.replace(['/', '\\'], "-"), repo.id),
        None => format!("{}-{}", repo.id, branch.replace(['/', '\\'], "-")),
    }
}

impl GitPort for GitAdapter {
    async fn prepare_worktree(
        &self,
        repo: &RepoInfo,
        branch: &str,
        group: Option<&str>,
    ) -> Result<PathBuf, PortError> {
        let token = self.token(repo).await?;
        let wt = self
            .git
            .create_worktree(
                &repo.path,
                &dir_name(repo, branch, group),
                branch,
                &repo.default_branch,
                token.as_ref(),
            )
            .await?;
        Ok(wt.path)
    }

    async fn restore_worktree(
        &self,
        repo: &RepoInfo,
        branch: &str,
        group: Option<&str>,
    ) -> Result<PathBuf, PortError> {
        let token = self.token(repo).await?;
        let wt = self
            .git
            .restore_worktree(
                &repo.path,
                &dir_name(repo, branch, group),
                branch,
                token.as_ref(),
            )
            .await?;
        Ok(wt.path)
    }

    async fn discard_worktree(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        branch: &str,
    ) -> Result<(), PortError> {
        self.git
            .remove_worktree(&repo.path, worktree, Some(branch))
            .await;
        Ok(())
    }

    async fn remove_run_root(&self, root: &Path) {
        self.git.remove_run_root(root).await;
    }

    async fn worktree_exists(&self, worktree: &Path) -> bool {
        tokio::fs::try_exists(worktree).await.unwrap_or(false)
    }

    async fn discard_file(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        path: &str,
    ) -> Result<(), PortError> {
        // Rename ikut memulihkan path lamanya.
        let old_path = self
            .git
            .changed_files(worktree, &repo.default_branch)
            .await?
            .into_iter()
            .find(|f| f.path == path)
            .and_then(|f| f.old_path);
        self.git
            .discard_file(worktree, &repo.default_branch, path, old_path.as_deref())
            .await?;
        Ok(())
    }

    async fn changes(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        branch: &str,
    ) -> Result<WorkChanges, PortError> {
        let dirty = self.git.is_dirty(worktree).await?;
        let unpushed = self
            .git
            .unpushed_commits(worktree, branch, &repo.default_branch)
            .await?;
        let files = self
            .git
            .changed_files(worktree, &repo.default_branch)
            .await?
            .into_iter()
            .map(|f| f.path)
            .collect();
        let diff = if dirty {
            self.git.working_diff(worktree, DIFF_MAX).await?
        } else {
            String::new()
        };
        Ok(WorkChanges {
            dirty,
            unpushed,
            files,
            diff,
        })
    }

    async fn commit_all(&self, worktree: &Path, message: &str) -> Result<(), PortError> {
        self.git.commit_all(worktree, message).await?;
        Ok(())
    }

    async fn push(&self, repo: &RepoInfo, worktree: &Path, branch: &str) -> Result<(), PortError> {
        let token = self.token(repo).await?;
        self.git.push(worktree, branch, token.as_ref()).await?;
        Ok(())
    }

    async fn branch_diff(&self, repo: &RepoInfo, worktree: &Path) -> Result<String, PortError> {
        Ok(self
            .git
            .branch_diff(worktree, &repo.default_branch, DIFF_MAX)
            .await?)
    }

    async fn find_pr(&self, repo: &RepoInfo, branch: &str) -> Result<Option<PrRef>, PortError> {
        let token = self.token(repo).await?;
        let pr = self
            .git
            .pr_for_branch(&repo.path, branch, token.as_ref())
            .await?;
        Ok(pr.map(pr_ref))
    }

    async fn snapshot(&self, worktree: &Path) -> Result<String, PortError> {
        Ok(self.git.snapshot_tree(worktree).await?)
    }

    async fn changed_between(
        &self,
        worktree: &Path,
        from: &str,
        to: &str,
    ) -> Result<Vec<String>, PortError> {
        Ok(self.git.changed_between(worktree, from, to).await?)
    }

    async fn pr_body(&self, repo: &RepoInfo, url: &str) -> Result<Option<String>, PortError> {
        let token = self.token(repo).await?;
        Ok(self.git.pr_body(&repo.path, url, token.as_ref()).await?)
    }

    async fn set_pr_body(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        url: &str,
        body: &str,
    ) -> Result<(), PortError> {
        let token = self.token(repo).await?;
        Ok(self
            .git
            .set_pr_body(worktree, url, body, token.as_ref())
            .await?)
    }

    async fn create_pr(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<String, PortError> {
        let token = self.token(repo).await?;
        Ok(self
            .git
            .create_pr(
                worktree,
                &repo.default_branch,
                branch,
                title,
                body,
                token.as_ref(),
            )
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChecksState;

    fn info(state: PrState) -> PrInfo {
        PrInfo {
            number: 1,
            url: "https://github.com/o/r/pull/1".to_owned(),
            state,
            checks: ChecksState::None,
        }
    }

    #[test]
    fn status_pr_dari_gh() {
        let open = pr_ref(info(PrState::Open));
        assert!(open.open && !open.merged);
        let merged = pr_ref(info(PrState::Merged));
        assert!(!merged.open && merged.merged);
        let closed = pr_ref(info(PrState::Closed));
        assert!(!closed.open && !closed.merged);
    }
}
