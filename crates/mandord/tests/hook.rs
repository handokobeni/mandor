//! `mandord hook` sebagai proses: exit 2 menolak, exit 0 mengizinkan.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_mandord");

fn hook(input: &str, clear_env: bool) -> (i32, String) {
    hook_with(input, clear_env, &[])
}

fn hook_with(input: &str, clear_env: bool, args: &[&str]) -> (i32, String) {
    let mut cmd = Command::new(BIN);
    cmd.arg("hook")
        .args(args)
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

#[cfg(unix)]
fn file_tool(tool: &str, path: &std::path::Path) -> String {
    serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": {"file_path": path}}).to_string()
}

#[cfg(unix)]
#[test]
fn akar_desain_hanya_baca_dan_symlink_keluar_ditolak() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("mock");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/page.tsx"), "x").unwrap();
    std::fs::write(dir.path().join("secret.txt"), "s").unwrap();
    std::os::unix::fs::symlink(dir.path().join("secret.txt"), root.join("link")).unwrap();
    let root_arg = root.to_str().unwrap();
    let args = ["--design", root_arg];

    let read = file_tool("Read", &root.join("src/page.tsx"));
    assert_eq!(hook_with(&read, false, &args).0, 0);
    let write = file_tool("Write", &root.join("src/new.tsx"));
    let (code, err) = hook_with(&write, false, &args);
    assert_eq!(code, 2);
    assert!(err.contains("read-only"), "{err}");
    let escape = file_tool("Read", &root.join("link"));
    assert_eq!(hook_with(&escape, false, &args).0, 2);
    let dotdot = file_tool("Read", &root.join("../secret.txt"));
    assert_eq!(hook_with(&dotdot, false, &args).0, 2);
    // Tanpa akar desain, path di luar worktree diatur oleh aturan lain.
    assert_eq!(hook_with(&read, false, &[]).0, 0);
}

/// Akar disebut lewat direktori symlink (seperti `/var` → `/private/var` di
/// macOS): symlink keluar dari akar tetap ditolak.
#[cfg(unix)]
#[test]
fn akar_desain_lewat_symlink_tetap_menolak_jalan_keluar() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("asli");
    std::fs::create_dir_all(target.join("mock/src")).unwrap();
    std::fs::write(target.join("mock/src/page.tsx"), "x").unwrap();
    std::fs::write(target.join("secret.txt"), "s").unwrap();
    std::os::unix::fs::symlink(target.join("secret.txt"), target.join("mock/link")).unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    let root = alias.join("mock");
    let args = ["--design", root.to_str().unwrap()];

    let read = file_tool("Read", &root.join("src/page.tsx"));
    assert_eq!(hook_with(&read, false, &args).0, 0);
    let escape = file_tool("Read", &root.join("link"));
    assert_eq!(hook_with(&escape, false, &args).0, 2);
    let write = file_tool("Write", &root.join("src/new.tsx"));
    assert_eq!(hook_with(&write, false, &args).0, 2);
}

fn browser(tool: &str, input: &serde_json::Value, response: Option<&str>) -> String {
    let mut v = serde_json::json!({
        "hook_event_name": if response.is_some() { "PostToolUse" } else { "PreToolUse" },
        "tool_name": format!("mcp__claude-in-chrome__{tool}"),
        "tool_input": input,
    });
    if let Some(text) = response {
        v["tool_response"] = serde_json::json!([{"type": "text", "text": text}]);
    }
    v.to_string()
}

#[test]
fn mode_review_memakai_state_dan_gagal_tertutup_tanpa_state() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("r1.json");
    let state_s = state.to_str().unwrap();
    let pre = ["--review", state_s];
    let post = ["--review", state_s, "--post"];
    let nav_app = browser(
        "navigate",
        &serde_json::json!({"url": "http://localhost:5173/orders", "tabId": 7}),
        None,
    );
    // Berkas state hilang: ditolak.
    assert_eq!(hook_with(&nav_app, false, &pre).0, 2);

    std::fs::write(&state, r#"{"origins":["http://localhost:5173"],"tabs":{}}"#).unwrap();
    // Tab belum dibuat reviewer.
    assert_eq!(hook_with(&nav_app, false, &pre).0, 2);
    let create = browser("tabs_create_mcp", &serde_json::json!({}), None);
    assert_eq!(hook_with(&create, false, &pre).0, 0);
    let created = browser(
        "tabs_create_mcp",
        &serde_json::json!({}),
        Some("Created new tab. Tab ID: 7"),
    );
    assert_eq!(hook_with(&created, false, &post).0, 0);
    assert_eq!(hook_with(&nav_app, false, &pre).0, 0);
    let nav_out = browser(
        "navigate",
        &serde_json::json!({"url": "https://example.com", "tabId": 7}),
        None,
    );
    let (code, err) = hook_with(&nav_out, false, &pre);
    assert_eq!(code, 2);
    assert!(err.contains("outside the app"), "{err}");
    // Tulis berkas ditolak; test dari allowlist diizinkan.
    let write = serde_json::json!({"tool_name": "Write", "tool_input": {"file_path": "/wt/a.rs"}})
        .to_string();
    assert_eq!(hook_with(&write, false, &pre).0, 2);
    assert_eq!(hook_with(&bash("git stash"), false, &pre).0, 2);
    assert_eq!(hook_with(&bash("cargo test"), false, &pre).0, 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&state).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "state ditulis ulang 0600");
    }
}
