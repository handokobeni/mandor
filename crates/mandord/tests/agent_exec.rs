//! `mandord agent-exec` (U6 plan VM) dijalankan sebagai user yang sama
//! (tanpa sudo); pemisahan uid dibuktikan di uji langsung VM.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_mandord");

fn agent_exec(run_dir: &std::path::Path, script: &str) -> Command {
    let mut c = Command::new(BIN);
    c.args(["agent-exec", "--run-dir"])
        .arg(run_dir)
        .args(["--env", "LANG=C.UTF-8", "--env", "PATH=/usr/bin:/bin"])
        .args(["--", "/bin/sh", "-c", script])
        .env("GH_TOKEN", "ghs_rahasia")
        .env("MANDOR_LARK_SECRET", "rahasia");
    c
}

/// pid proses cucu yang ditulis skrip agent ke `pidfile`.
fn grandchild(pidfile: &std::path::Path) -> u32 {
    assert!(
        wait_until(
            || std::fs::read_to_string(pidfile).is_ok_and(|p| !p.trim().is_empty()),
            5
        ),
        "cucu agent berjalan"
    );
    std::fs::read_to_string(pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
        && !std::fs::read_to_string(format!("/proc/{pid}/status")).is_ok_and(|s| {
            s.lines()
                .any(|l| l.starts_with("State:") && l.contains('Z'))
        })
}

fn wait_until(mut f: impl FnMut() -> bool, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn signal(child: &Child, sig: &str) {
    Command::new("kill")
        .args([sig, &child.id().to_string()])
        .status()
        .unwrap();
}

#[test]
fn environment_hanya_allowlist_tanpa_token() {
    let dir = tempfile::tempdir().unwrap();
    let out = agent_exec(dir.path(), "env")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    let env = String::from_utf8_lossy(&out.stdout);
    assert!(
        !env.contains("ghs_rahasia") && !env.contains("MANDOR_LARK_SECRET"),
        "{env}"
    );
    assert!(env.contains("GIT_CONFIG_NOSYSTEM=1"), "{env}");
    assert!(env.contains("GIT_TERMINAL_PROMPT=0"), "{env}");
    assert!(env.contains(&format!("GH_CONFIG_DIR={}/gh", dir.path().display())));
    assert!(env.contains("LANG=C.UTF-8"));
}

#[test]
fn stdin_diteruskan_dan_kode_keluar_dikembalikan() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = agent_exec(dir.path(), "read x; echo \"got $x\"; exit 3")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"halo\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "got halo");
}

#[test]
fn sigterm_diteruskan_ke_group_dan_group_habis() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("cucu");
    let mut child = agent_exec(
        dir.path(),
        &format!("sleep 300 & echo $! > {}; wait", pidfile.display()),
    )
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .spawn()
    .unwrap();
    let pid = grandchild(&pidfile);
    assert!(alive(pid));
    signal(&child, "-TERM");
    assert!(wait_until(|| child.try_wait().unwrap().is_some(), 15));
    assert!(wait_until(|| !alive(pid), 5), "seluruh group berhenti");
}

#[test]
fn agent_yang_mengabaikan_sigterm_dibunuh_paksa() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("cucu");
    let mut child = agent_exec(
        dir.path(),
        &format!(
            "trap '' TERM INT; sh -c \"trap '' TERM INT; sleep 300\" & echo $! > {}; while true; do sleep 1; done",
            pidfile.display()
        ),
    )
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .spawn()
    .unwrap();
    let pid = grandchild(&pidfile);
    signal(&child, "-TERM");
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        child.try_wait().unwrap().is_none(),
        "masih dalam masa tenggang"
    );
    assert!(
        wait_until(|| child.try_wait().unwrap().is_some(), 20),
        "eskalasi sampai SIGKILL"
    );
    assert!(wait_until(|| !alive(pid), 5));
}

#[test]
fn pipe_daemon_putus_membunuh_group() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("cucu");
    let mut child = agent_exec(
        dir.path(),
        &format!(
            "sleep 300 & echo $! > {}; while true; do echo tick; sleep 0.2; done",
            pidfile.display()
        ),
    )
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "tick");
    let pid = grandchild(&pidfile);
    drop(reader);
    assert!(
        wait_until(|| child.try_wait().unwrap().is_some(), 15),
        "agent-exec berhenti"
    );
    assert!(wait_until(|| !alive(pid), 5), "group dibunuh");
}

#[test]
fn reap_membersihkan_group_run_lama() {
    let marker = "mandor-ae-reap-3b8e";
    // Penanda ada di argv proses (seperti `--session-id` run agent).
    let mut old = Command::new("setsid")
        .args(["sleep", "300", marker])
        .spawn()
        .unwrap();
    assert!(wait_until(|| alive(old.id()), 5));
    let status = Command::new(BIN)
        .args(["agent-exec", "--reap", &old.id().to_string(), marker])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(wait_until(|| old.try_wait().unwrap().is_some(), 10));
}
