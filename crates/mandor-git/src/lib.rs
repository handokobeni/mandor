//! Adapter git dan `gh` untuk mandord (U6, KTD-9, KTD-11).
//!
//! - Semua git di dalam worktree berjalan tanpa hook repo dan fsmonitor, agar
//!   berkas yang ditulis agent tidak pernah dieksekusi dengan kredensial daemon.
//! - Kredensial hanya diberikan ke proses fetch dan push milik daemon.
//! - Push hanya ke branch task yang diminta pemanggil.

mod diff;
mod discover;
mod gh;
mod port;
mod publish;
mod repo;
mod snapshot;
mod worktree;

use std::path::{Path, PathBuf};
use std::time::Duration;

use mandor_proc::{Cmd, Output, ProcError, run};

pub use diff::{ChangeStatus, ChangedFile};
pub use discover::{DirEntry, DirListing, RepoCandidate, discover_repos, list_dirs};
pub use gh::{ChecksState, PrInfo, PrState, summarize_checks};
pub use port::GitAdapter;
pub use publish::touches_sensitive_path;
pub use worktree::Worktree;

/// Nilai `core.hooksPath` yang menonaktifkan semua hook.
#[cfg(not(windows))]
const NO_HOOKS: &str = "core.hooksPath=/dev/null";
#[cfg(windows)]
const NO_HOOKS: &str = "core.hooksPath=NUL";

/// Batas waktu operasi git lokal.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);
/// Batas waktu operasi jaringan (fetch, push, gh).
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

/// Lokasi biner dan direktori kerja.
#[derive(Debug, Clone)]
pub struct GitConfig {
    pub git_bin: PathBuf,
    pub gh_bin: PathBuf,
    /// Induk semua worktree (`~/.mandor/workspaces`).
    pub workspaces_dir: PathBuf,
}

impl GitConfig {
    #[must_use]
    pub fn new(workspaces_dir: PathBuf) -> Self {
        Self {
            git_bin: PathBuf::from("git"),
            gh_bin: PathBuf::from("gh"),
            workspaces_dir,
        }
    }
}

/// Kegagalan operasi git atau gh. Pesan sudah diredaksi dari token.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error(transparent)]
    Proc(#[from] ProcError),
    #[error("{op} failed: {stderr}")]
    Command { op: &'static str, stderr: String },
    #[error(
        "no default branch on the remote (origin/HEAD, origin/main, origin/master); run `git remote set-head origin --auto`"
    )]
    NoDefaultBranch,
    #[error(
        "not a git repository; for a parent folder that holds several repos, add each repo here and group them in Settings › Workspaces"
    )]
    NotARepo,
    #[error("invalid branch name: {0}")]
    InvalidBranch(String),
    #[error("push rejected: branch {actual} is not the task branch {expected}")]
    WrongBranch { expected: String, actual: String },
    #[error("cannot parse gh output: {0}")]
    Parse(String),
    #[error("file I/O failed: {0}")]
    Io(String),
}

/// Adapter git.
#[derive(Debug, Clone)]
pub struct Git {
    cfg: GitConfig,
}

impl Git {
    #[must_use]
    pub fn new(cfg: GitConfig) -> Self {
        Self { cfg }
    }

    /// `git` dengan hook repo dinonaktifkan, di direktori `dir`.
    fn git(&self, dir: &Path) -> Cmd {
        Cmd::new(&self.cfg.git_bin)
            .cwd(dir)
            .args(["-c", NO_HOOKS, "-c", "core.fsmonitor=false"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .scrub_tokens()
            .timeout(LOCAL_TIMEOUT)
    }

    async fn ok(&self, op: &'static str, cmd: Cmd) -> Result<Output, GitError> {
        let out = run(&cmd).await?;
        if out.success() {
            Ok(out)
        } else {
            Err(GitError::Command {
                op,
                stderr: out.stderr.trim().to_owned(),
            })
        }
    }
}
