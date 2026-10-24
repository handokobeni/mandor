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
const NO_HOOKS_KEY: &str = "core.hooksPath";
#[cfg(not(windows))]
const NO_HOOKS_VALUE: &str = "/dev/null";
#[cfg(windows)]
const NO_HOOKS_VALUE: &str = "NUL";

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
        "refusing to run git: the repo config sets {0}, which Mandor does not trust on the VM; remove it with `git config --unset`"
    )]
    UnsafeConfig(String),
    #[error("refusing to run git: cannot read the repo config: {0}")]
    UnreadableConfig(String),
}

/// Kunci konfigurasi lokal/worktree yang dipercaya daemon VM (KTD-65 plan
/// VM). Selain ini repo ditolak: konfigurasi repo bisa ditulis agent dan
/// banyak kunci git yang mengeksekusi perintah atau mengalihkan jaringan.
/// `key` dalam huruf kecil.
#[must_use]
pub fn trusted_repo_config_key(key: &str) -> bool {
    const CORE: &[&str] = &[
        "repositoryformatversion",
        "filemode",
        "bare",
        "logallrefupdates",
        "ignorecase",
        "precomposeunicode",
        "symlinks",
        "autocrlf",
        "eol",
        "safecrlf",
        "longpaths",
    ];
    const REMOTE: &[&str] = &["url", "pushurl", "fetch", "tagopt", "prune", "gh-resolved"];
    const BRANCH: &[&str] = &[
        "remote",
        "merge",
        "rebase",
        "pushremote",
        "description",
        "gh-merge-base",
    ];
    let mut parts = key.splitn(2, '.');
    let (Some(section), Some(rest)) = (parts.next(), parts.next()) else {
        return false;
    };
    // `<section>.<subsection>.<name>`: nama setelah titik terakhir.
    let name = rest.rsplit_once('.').map(|(_, n)| n);
    match section {
        "core" => CORE.contains(&rest),
        "user" | "color" | "advice" => name.is_none(),
        "extensions" => matches!(rest, "worktreeconfig" | "objectformat"),
        "fetch" => rest == "prune",
        "pull" => rest == "rebase",
        "remote" => name.is_some_and(|n| REMOTE.contains(&n)),
        "branch" => name.is_some_and(|n| BRANCH.contains(&n)),
        _ => false,
    }
}

/// Override konfigurasi untuk setiap proses git daemon VM, termasuk git yang
/// dijalankan `gh`: menetralkan kunci yang mengeksekusi perintah walau
/// konfigurasi repo diubah setelah diperiksa.
const VM_OVERRIDES: &[(&str, &str)] = &[
    (NO_HOOKS_KEY, NO_HOOKS_VALUE),
    ("core.sshCommand", "ssh -o BatchMode=yes"),
    ("core.pager", "cat"),
    ("core.askPass", ""),
    ("core.editor", "true"),
    ("core.fsmonitor", "false"),
    ("core.gitProxy", ""),
    ("core.alternateRefsCommand", ""),
    ("diff.external", ""),
    ("credential.helper", ""),
    ("protocol.ext.allow", "never"),
    ("http.proxy", ""),
    ("http.sslVerify", "true"),
    ("sequence.editor", "true"),
    ("gpg.program", "gpg"),
    ("commit.gpgSign", "false"),
    ("tag.gpgSign", "false"),
    ("fetch.recurseSubmodules", "false"),
    ("submodule.recurse", "false"),
];

/// Environment untuk [`VM_OVERRIDES`]. Lewat `GIT_CONFIG_PARAMETERS` (bentuk
/// yang dipakai `git -c`), sehingga tidak bertabrakan dengan `-c` di argv
/// maupun `GIT_CONFIG_COUNT` milik kredensial push.
fn vm_override_env() -> [(&'static str, String); 2] {
    let params = VM_OVERRIDES
        .iter()
        .map(|(k, v)| format!("'{k}={v}'"))
        .collect::<Vec<_>>()
        .join(" ");
    [
        ("GIT_CONFIG_NOSYSTEM", "1".to_owned()),
        ("GIT_CONFIG_PARAMETERS", params),
    ]
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
    /// kunci di luar allowlist, atau tidak bisa dibaca (gagal-tertutup).
    async fn check_repo_config(&self, dir: &Path) -> Result<(), GitError> {
        let cmd = Cmd::new(&self.cfg.git_bin)
            .cwd(dir)
            .args(["config", "--list", "--show-scope", "--name-only"])
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .scrub_tokens()
            .timeout(LOCAL_TIMEOUT);
        let out = run(&cmd).await?;
        if !out.success() {
            return Err(GitError::UnreadableConfig(out.stderr.trim().to_owned()));
        }
        for line in out.stdout.lines() {
            let Some((scope, key)) = line.split_once('\t') else {
                return Err(GitError::UnreadableConfig(format!(
                    "unexpected line: {line}"
                )));
            };
            let key = key.trim().to_ascii_lowercase();
            if matches!(scope, "local" | "worktree") && !trusted_repo_config_key(&key) {
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
        let mut guarded = cmd.clone();
        for (k, v) in vm_override_env() {
            guarded = guarded.env(k, v);
        }
        Ok(run(&guarded).await?)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// Override menang atas konfigurasi repo yang diubah sesudah diperiksa.
    #[test]
    fn override_vm_menang_atas_konfigurasi_repo() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let mut c = std::process::Command::new("git");
            c.current_dir(dir.path()).args(args);
            for (k, v) in vm_override_env() {
                c.env(k, v);
            }
            c.output().unwrap()
        };
        assert!(git(&["init", "-q"]).status.success());
        assert!(
            git(&["config", "core.sshCommand", "touch /tmp/pwned"])
                .status
                .success()
        );
        let out = git(&["config", "--get", "core.sshcommand"]);
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "ssh -o BatchMode=yes"
        );
        let out = git(&["config", "--get-all", "credential.helper"]);
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "");
    }
}
