//! `mandord agent-exec`: induk pengawas agent sebagai user Linux terpisah di
//! VM (KTD-62 plan VM). Dijalankan daemon lewat
//! `sudo -n -u <dev>-agent mandord agent-exec --run-dir <dir> [--env K=V]... -- <program> <args>...`.
//!
//! - Environment hanya dari allowlist, ditambah variabel isolasi git, gh, dan
//!   Docker rootless milik user agent. Tidak ada token daemon.
//! - Agent berjalan di process group sendiri; SIGTERM dan SIGINT diteruskan ke
//!   group, lalu dieskalasi sampai SIGKILL setelah batas waktu.
//! - Stdout dan stderr agent disalurkan ke daemon; bila pipe ke daemon putus
//!   (daemon mati), seluruh group dibunuh.
//! - `--reap <pid> <marker>` membersihkan group run lama setelah restart daemon.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, bail};
use mandor_proc::{Cmd, EnvPolicy, Supervised};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt as _};
use tokio::signal::unix::{SignalKind, signal};

/// Jeda sebelum eskalasi setelah sinyal diteruskan.
const GRACE: Duration = Duration::from_secs(10);

/// Variabel yang boleh diteruskan dari daemon atau dari environment sudo.
use mandor_agent::claude::command::AGENT_ENV_ALLOWED as ALLOWED;

/// Mode `agent-exec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Run {
        run_dir: PathBuf,
        /// Variabel dari daemon (disaring lagi lewat allowlist).
        env: Vec<(String, String)>,
        program: String,
        args: Vec<String>,
    },
    Reap {
        pid: u32,
        marker: String,
    },
}

/// Mengurai argumen setelah `agent-exec`.
///
/// # Errors
/// Pesan untuk pemanggil bila bentuknya salah.
pub fn parse(args: &[String]) -> Result<Mode, String> {
    if args.first().map(String::as_str) == Some("--reap") {
        let pid = args
            .get(1)
            .and_then(|p| p.parse().ok())
            .ok_or("--reap needs a pid")?;
        let marker = args.get(2).cloned().ok_or("--reap needs a marker")?;
        return Ok(Mode::Reap { pid, marker });
    }
    let mut run_dir = None;
    let mut env = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--run-dir" => {
                run_dir = args.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--env" => {
                let kv = args.get(i + 1).ok_or("--env needs KEY=VALUE")?;
                let (k, v) = kv.split_once('=').ok_or("--env needs KEY=VALUE")?;
                env.push((k.to_owned(), v.to_owned()));
                i += 2;
            }
            "--" => {
                let program = args.get(i + 1).cloned().ok_or("missing program after --")?;
                return Ok(Mode::Run {
                    run_dir: run_dir.ok_or("missing --run-dir")?,
                    env,
                    program,
                    args: args[i + 2..].to_vec(),
                });
            }
            other => return Err(format!("unknown agent-exec argument: {other}")),
        }
    }
    Err("missing -- <program>".to_owned())
}

/// Environment agent: allowlist dari daemon dan sudo, lalu isolasi milik user
/// agent. Variabel dari environment sudo (`HOME`, `USER`) menang atas daemon.
#[must_use]
pub fn agent_env(
    passed: &[(String, String)],
    own: impl Fn(&str) -> Option<String>,
    run_dir: &Path,
    uid: Option<u32>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut set = |k: &str, v: String| {
        out.retain(|(key, _)| key != k);
        out.push((k.to_owned(), v));
    };
    for (k, v) in passed {
        if ALLOWED.contains(&k.as_str()) {
            set(k, v.clone());
        }
    }
    for k in ["HOME", "USER", "LOGNAME"] {
        if let Some(v) = own(k) {
            set(k, v);
        }
    }
    let dir = run_dir.display();
    set("GIT_CONFIG_NOSYSTEM", "1".to_owned());
    set("GIT_CONFIG_GLOBAL", format!("{dir}/gitconfig"));
    set("GH_CONFIG_DIR", format!("{dir}/gh"));
    set("GIT_SSH_COMMAND", "ssh -o BatchMode=yes".to_owned());
    set("GIT_TERMINAL_PROMPT", "0".to_owned());
    if let Some(uid) = uid {
        set("DOCKER_HOST", format!("unix:///run/user/{uid}/docker.sock"));
        set("XDG_RUNTIME_DIR", format!("/run/user/{uid}"));
    }
    out
}

/// uid proses ini dari `/proc/self/status` (tanpa `unsafe`).
fn own_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|u| u.parse().ok())
}

/// Menjalankan `agent-exec`; mengembalikan kode keluar agent.
///
/// # Errors
/// Argumen salah atau agent tidak bisa dijalankan.
pub async fn run(args: &[String]) -> anyhow::Result<i32> {
    match parse(args).map_err(anyhow::Error::msg)? {
        Mode::Reap { pid, marker } => {
            mandor_proc::reap_group(pid, &marker).await;
            Ok(0)
        }
        Mode::Run {
            run_dir,
            env,
            program,
            args,
        } => {
            let uid = tokio::task::spawn_blocking(own_uid)
                .await
                .context("reading the agent uid")?;
            let vars = agent_env(&env, |k| std::env::var(k).ok(), &run_dir, uid);
            supervise(&program, &args, vars).await
        }
    }
}

async fn supervise(
    program: &str,
    args: &[String],
    env: Vec<(String, String)>,
) -> anyhow::Result<i32> {
    let cmd = Cmd::new(program)
        .args(args)
        .env_policy(EnvPolicy::Explicit(env))
        .stdin_bytes(Vec::new());
    let mut child = Supervised::spawn(&cmd).context("starting the agent")?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.take_stdin(), child.take_stdout(), child.take_stderr())
    else {
        bail!("agent pipes are not available");
    };
    let mut term = signal(SignalKind::terminate()).context("SIGTERM handler")?;
    let mut int = signal(SignalKind::interrupt()).context("SIGINT handler")?;
    // Penyalur keluaran memberi tahu bila daemon tidak lagi membaca.
    let (gone_tx, mut gone) = tokio::sync::mpsc::channel::<()>(2);
    let feed = tokio::spawn(forward(tokio::io::stdin(), stdin));
    let out = tokio::spawn(pump(stdout, tokio::io::stdout(), gone_tx.clone()));
    let err = tokio::spawn(pump(stderr, tokio::io::stderr(), gone_tx));
    let code = tokio::select! {
        code = child.wait() => code.context("waiting for the agent")?,
        _ = term.recv() => stop(&mut child, 15).await,
        _ = int.recv() => stop(&mut child, 2).await,
        Some(()) = gone.recv() => kill(&mut child).await,
    };
    feed.abort();
    let _ = out.await;
    let _ = err.await;
    Ok(code.unwrap_or(1))
}

/// Menyalurkan keluaran agent ke daemon; pipe daemon putus dilaporkan.
async fn pump(
    from: impl AsyncRead + Unpin,
    to: impl AsyncWrite + Unpin,
    gone: tokio::sync::mpsc::Sender<()>,
) {
    if !forward(from, to).await {
        let _ = gone.send(()).await;
    }
}

/// Meneruskan sinyal ke group lalu mengeskalasi sampai SIGKILL.
async fn stop(child: &mut Supervised, sig: i32) -> Option<i32> {
    if child.signal(sig).is_ok()
        && let Ok(Ok(code)) = tokio::time::timeout(GRACE, child.wait()).await
    {
        return code;
    }
    kill(child).await
}

async fn kill(child: &mut Supervised) -> Option<i32> {
    child.kill().await;
    None
}

/// Menyalin sampai EOF. `false` bila sisi tulis putus (pembaca sudah pergi).
async fn forward(mut from: impl AsyncRead + Unpin, mut to: impl AsyncWrite + Unpin) -> bool {
    let copied = tokio::io::copy(&mut from, &mut to).await;
    let flushed = to.flush().await;
    let _ = to.shutdown().await;
    copied.is_ok() && flushed.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn argumen_jalan_dan_reap() {
        assert_eq!(
            parse(&s(&[
                "--run-dir",
                "/g/runs/r1",
                "--env",
                "LANG=C.UTF-8",
                "--",
                "claude",
                "-p"
            ])),
            Ok(Mode::Run {
                run_dir: "/g/runs/r1".into(),
                env: vec![("LANG".to_owned(), "C.UTF-8".to_owned())],
                program: "claude".to_owned(),
                args: s(&["-p"]),
            })
        );
        assert_eq!(
            parse(&s(&["--reap", "42", "mandor-run-r1"])),
            Ok(Mode::Reap {
                pid: 42,
                marker: "mandor-run-r1".to_owned()
            })
        );
        assert!(parse(&s(&["--", "claude"])).is_err(), "run dir wajib");
        assert!(parse(&s(&["--run-dir", "/x"])).is_err(), "program wajib");
        assert!(parse(&s(&["--bogus"])).is_err());
    }

    #[test]
    fn environment_hanya_allowlist_dan_isolasi() {
        let passed = vec![
            ("PATH".to_owned(), "/usr/bin".to_owned()),
            ("GH_TOKEN".to_owned(), "rahasia".to_owned()),
            ("LARK_TOKEN".to_owned(), "rahasia".to_owned()),
            ("HOME".to_owned(), "/home/beni".to_owned()),
        ];
        let own = |k: &str| match k {
            "HOME" => Some("/home/beni-agent".to_owned()),
            "USER" => Some("beni-agent".to_owned()),
            _ => None,
        };
        let env = agent_env(&passed, own, Path::new("/g/runs/r1"), Some(1001));
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("PATH"), Some("/usr/bin"));
        assert_eq!(get("HOME"), Some("/home/beni-agent"), "home user agent");
        assert_eq!(get("GH_TOKEN"), None);
        assert_eq!(get("LARK_TOKEN"), None);
        assert_eq!(get("GIT_CONFIG_NOSYSTEM"), Some("1"));
        assert_eq!(get("GIT_CONFIG_GLOBAL"), Some("/g/runs/r1/gitconfig"));
        assert_eq!(get("GH_CONFIG_DIR"), Some("/g/runs/r1/gh"));
        assert_eq!(get("GIT_TERMINAL_PROMPT"), Some("0"));
        assert_eq!(
            get("DOCKER_HOST"),
            Some("unix:///run/user/1001/docker.sock")
        );
        assert!(env.iter().all(|(k, _)| !k.contains("TOKEN")));
    }
}
