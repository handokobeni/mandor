//! `mandord hook` sebagai proses: exit 2 menolak, exit 0 mengizinkan.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_mandord");

fn hook(input: &str, clear_env: bool) -> (i32, String) {
    let mut cmd = Command::new(BIN);
    cmd.arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if clear_env {
        // Tanpa HOME dan PATH: konfigurasi daemon tidak bisa dibaca, hook tetap bekerja.
        cmd.env_clear();
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn bash(cmd: &str) -> String {
    serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": cmd}}).to_string()
}

#[test]
fn menolak_push_gh_dan_kredensial_mengizinkan_cargo_test() {
    for cmd in [
        "git push origin main",
        "gh pr create",
        "cat ~/.config/gh/hosts.yml",
    ] {
        let (code, err) = hook(&bash(cmd), false);
        assert_eq!(code, 2, "{cmd}");
        assert!(err.contains("mandor:"), "{err}");
    }
    assert_eq!(hook(&bash("cargo test"), false).0, 0);
}

#[test]
fn input_rusak_menolak_dan_tanpa_konfigurasi_tetap_bekerja() {
    assert_eq!(hook("{rusak", false).0, 2);
    assert_eq!(hook("", true).0, 2);
    assert_eq!(hook(&bash("git push"), true).0, 2);
    assert_eq!(hook(&bash("cargo test"), true).0, 0);
}
