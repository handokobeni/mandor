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
pub use gh::{ChecksState, NewPr, PrInfo, PrState, summarize_checks};
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
    /// Daemon VM: konfigurasi repo yang bisa diubah agent tidak dipercaya
    /// (KTD-65 plan VM). Konfigurasi sistem diabaikan dan repo dengan kunci
    /// yang bisa mengeksekusi atau mengalihkan ditolak.
    pub distrust_repo_config: bool,
}

impl GitConfig {
    #[must_use]
    pub fn new(workspaces_dir: PathBuf) -> Self {
        Self {
            git_bin: PathBuf::from("git"),
            gh_bin: PathBuf::from("gh"),
            workspaces_dir,
            distrust_repo_config: false,
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
    #[error(
        "refusing to run git: the repo config sets {0}, which can run commands or redirect git"
    )]
    UnsafeConfig(String),
}

/// Kunci konfigurasi repo yang bisa mengeksekusi perintah atau mengalihkan
/// git (KTD-65 plan VM). `key` dalam huruf kecil.
#[must_use]
pub fn unsafe_config_key(key: &str) -> bool {
    const EXACT: &[&str] = &[
        "core.sshcommand",
        "core.pager",
        "core.askpass",
        "core.editor",
        "core.hookspath",
        "core.fsmonitor",
        "diff.external",
        "gpg.program",
        "sequence.editor",
    ];
    const PREFIX: &[&str] = &["credential.", "url.", "include.", "includeif.", "filter."];
    const SUFFIX: &[&str] = &[".command", ".textconv", ".driver", ".program"];
    EXACT.contains(&key)
        || PREFIX.iter().any(|p| key.starts_with(p))
        || ((key.starts_with("diff.") || key.starts_with("merge.") || key.starts_with("gpg."))
            && SUFFIX.iter().any(|s| key.ends_with(s)))
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

    /// Menolak repo di `dir` bila konfigurasi lokal atau worktree-nya memuat
    /// kunci berbahaya. Direktori yang bukan repo tidak diperiksa.
    async fn check_repo_config(&self, dir: &Path) -> Result<(), GitError> {
        let cmd = Cmd::new(&self.cfg.git_bin)
            .cwd(dir)
            .args(["config", "--list", "--show-scope", "--name-only"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .scrub_tokens()
            .timeout(LOCAL_TIMEOUT);
        let out = run(&cmd).await?;
        if !out.success() {
            return Ok(());
        }
        for line in out.stdout.lines() {
            let Some((scope, key)) = line.split_once('\t') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            if matches!(scope, "local" | "worktree") && unsafe_config_key(&key) {
                return Err(GitError::UnsafeConfig(key));
            }
        }
        Ok(())
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

    /// Menjalankan perintah git atau gh milik daemon. Di VM konfigurasi repo
    /// diperiksa dulu dan konfigurasi sistem diabaikan (KTD-65).
    async fn exec(&self, cmd: &Cmd) -> Result<Output, GitError> {
        if !self.cfg.distrust_repo_config {
            return Ok(run(cmd).await?);
        }
        if let Some(dir) = cmd.cwd_path() {
            self.check_repo_config(dir).await?;
        }
        Ok(run(&cmd.clone().env("GIT_CONFIG_NOSYSTEM", "1")).await?)
    }

    async fn ok(&self, op: &'static str, cmd: Cmd) -> Result<Output, GitError> {
        let out = self.exec(&cmd).await?;
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
