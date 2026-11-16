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
    // Origin selalu dari argv yang ditulis daemon.
    let origin = "http://localhost:5173";
    let pre = ["--review", state_s, "--origin", origin];
    let post = ["--review", state_s, "--origin", origin, "--post"];
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

/// U11 plan VM: `--vm <run dir>` menambah pagar lembur, termasuk untuk sesi
/// reviewer, dan berkas run daemon tidak bisa dibaca agent.
#[cfg(unix)]
#[test]
fn hook_vm_menolak_pesan_staging_dan_berkas_run() {
    let vm = ["--vm", "/srv/mandor/beni/runs"];
    for cmd in [
        "sendmail -t < msg",
        "curl https://staging.kelola.id/",
        "cat /srv/mandor/beni/runs/mcp/x.json",
        "git push origin main",
        "gh pr merge 12",
    ] {
        let (code, err) = hook_with(&bash(cmd), false, &vm);
        assert_eq!(code, 2, "{cmd}");
        assert!(err.contains("mandor:"), "{err}");
    }
    assert_eq!(hook_with(&bash("cargo test"), false, &vm).0, 0);
    // Path lewat symlink ke direktori run tetap dikenali setelah kanonisasi.
    let runs = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(runs.path().join("mcp")).unwrap();
    std::fs::write(runs.path().join("mcp/x.json"), "{}").unwrap();
    let other = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(runs.path(), other.path().join("l")).unwrap();
    let sneaky = format!("{}/l/mcp/x.json", other.path().display());
    let read = serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "Read",
        "tool_input": {"file_path": sneaky}})
    .to_string();
    let dir = runs.path().display().to_string();
    assert_eq!(hook_with(&read, false, &["--vm", &dir]).0, 2);
    // Tanpa --vm (laptop) aturan lembur tidak berlaku.
    assert_eq!(hook(&bash("curl https://staging.kelola.id/"), false).0, 0);
    // Sesi reviewer VM juga terkena pagar sebelum state review dibaca.
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("r.json");
    std::fs::write(&state, r#"{"origins":[]}"#).unwrap();
    let review = [
        "--vm",
        "/srv/mandor/beni/runs",
        "--review",
        state.to_str().unwrap(),
    ];
    assert_eq!(hook_with(&bash("sendmail -t"), false, &review).0, 2);
}

/// Plan 2026-11-06-001 (U3): giliran dengan berkas izin.
#[cfg(unix)]
mod permit {
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use mandor_agent::claude::guardrails::{default_allowed, plan_allowed, review_allowed};
    use mandor_agent::claude::permit::{PermitFile, VERSION, digest};

    use super::BIN;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        permit_dir: PathBuf,
        evidence: PathBuf,
    }

    /// Run satu repo: `<root>` repo git dengan `sub/` dan repo bersarang
    /// `nested/`, berkas izin di folder terpisah.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let root = base.join("run");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join("nested/.git")).unwrap();
        let permit_dir = base.join("permit");
        let evidence = permit_dir.join("run.d");
        std::fs::create_dir_all(&evidence).unwrap();
        Fixture {
            _dir: dir,
            root,
            permit_dir,
            evidence,
        }
    }

    impl Fixture {
        /// Menulis berkas izin; mengembalikan argumen hook.
        fn permit(&self, turn: &str, allow: Vec<String>) -> Vec<String> {
            let file = PermitFile {
                version: VERSION,
                turn: turn.to_owned(),
                allow,
                mcp: vec![],
                design: vec![],
                run_root: self.root.display().to_string(),
                members: vec![self.root.display().to_string()],
                evidence: (turn != "review").then(|| self.evidence.display().to_string()),
                fixtures: None,
            };
            let bytes = file.to_bytes().unwrap();
            let path = self.permit_dir.join("run.json");
            std::fs::write(&path, &bytes).unwrap();
            vec![
                "--permit".to_owned(),
                path.display().to_string(),
                "--permit-sha".to_owned(),
                digest(&bytes),
            ]
        }

        fn input(tool: &str, tool_input: &serde_json::Value, cwd: &Path) -> String {
            serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": tool,
                "tool_input": tool_input, "cwd": cwd, "tool_use_id": "toolu_01"})
            .to_string()
        }

        fn bash(&self, cmd: &str) -> String {
            Self::input("Bash", &serde_json::json!({"command": cmd}), &self.root)
        }
    }

    /// (exit, stdout, stderr)
    fn run(input: &str, args: &[String]) -> (i32, String, String) {
        let mut child = Command::new(BIN)
            .arg("hook")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    fn is_allow(stdout: &str) -> bool {
        serde_json::from_str::<serde_json::Value>(stdout.trim())
            .is_ok_and(|v| v["hookSpecificOutput"]["permissionDecision"] == "allow")
    }

    #[test]
    fn tiga_keputusan_dan_bukti_pemeriksaan() {
        let f = fixture();
        let args = f.permit("implement", default_allowed());
        let (code, out, err) = run(&f.bash("cd sub && git diff --stat"), &args);
        assert_eq!(code, 0, "{err}");
        assert!(is_allow(&out), "{out}");

        let (code, out, _) = run(&f.bash("docker ps"), &args);
        assert_eq!((code, out.as_str()), (0, ""));
        let evidence: serde_json::Value =
            serde_json::from_slice(&std::fs::read(f.evidence.join("toolu_01.json")).unwrap())
                .unwrap();
        assert_eq!(evidence["cwd"], f.root.display().to_string());

        let (code, out, err) = run(&f.bash("git push origin HEAD"), &args);
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(err.contains("mandor:"), "{err}");
        assert_eq!(run("{rusak", &args).0, 2);

        // Repo bersarang sebagai folder kerja: tanpa keputusan.
        let nested = Fixture::input(
            "Bash",
            &serde_json::json!({"command": "git status"}),
            &f.root.join("nested"),
        );
        let input = nested.replace("toolu_01", "toolu_02");
        assert_eq!(run(&input, &args).1, "");
    }

    #[test]
    fn berkas_izin_hilang_rusak_atau_digest_beda_menolak() {
        let f = fixture();
        let mut args = f.permit("implement", default_allowed());
        let input = f.bash("git status");
        assert!(is_allow(&run(&input, &args).1));
        let sha = args[3].clone();
        args[3] = "0".repeat(64);
        assert_eq!(run(&input, &args).0, 2);
        args[3] = sha.clone();
        std::fs::write(f.permit_dir.join("run.json"), b"{rusak").unwrap();
        assert_eq!(run(&input, &args).0, 2);
        std::fs::remove_file(f.permit_dir.join("run.json")).unwrap();
        assert_eq!(run(&input, &args).0, 2);
    }

    #[test]
    fn agent_tidak_bisa_menyentuh_folder_berkas_izin() {
        let f = fixture();
        let args = f.permit("implement", default_allowed());
        let target = f.permit_dir.join("x");
        let (code, _, err) = run(&f.bash(&format!("echo x > {}", target.display())), &args);
        assert_eq!(code, 2, "{err}");
        let write = Fixture::input(
            "Write",
            &serde_json::json!({"file_path": f.evidence.join("toolu_09.json")}),
            &f.root,
        );
        assert_eq!(run(&write, &args).0, 2);
    }

    #[test]
    fn tenggat_internal_terlewati_menolak() {
        let f = fixture();
        let mut args = f.permit("implement", default_allowed());
        args.extend(["--deadline-ms".to_owned(), "200".to_owned()]);
        let mut child = Command::new(BIN)
            .arg("hook")
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Input tidak pernah selesai: stdin tetap terbuka.
        let _stdin = child.stdin.take().unwrap();
        let status = child.wait().unwrap();
        let mut err = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        assert_eq!(status.code(), Some(2));
        assert!(err.contains("too long"), "{err}");
    }

    #[test]
    fn plan_dan_reviewer_tetap_hanya_baca() {
        let f = fixture();
        let args = f.permit("plan", plan_allowed());
        let write = Fixture::input(
            "Write",
            &serde_json::json!({"file_path": f.root.join("a.rs")}),
            &f.root,
        );
        assert_eq!(run(&write, &args).0, 2);
        assert!(is_allow(&run(&f.bash("git log --oneline -5"), &args).1));

        let mut args = f.permit("review", review_allowed());
        let state = f.permit_dir.join("r1.json");
        std::fs::write(&state, r#"{"origins":[],"tabs":{}}"#).unwrap();
        args.extend(["--review".to_owned(), state.display().to_string()]);
        assert!(is_allow(&run(&f.bash("git diff --stat"), &args).1));
        assert_eq!(run(&f.bash("echo x > a.rs"), &args).0, 2);
        // Berkas izin reviewer tanpa --review tidak cocok dengan sesi.
        let args = f.permit("review", review_allowed());
        assert_eq!(run(&f.bash("git diff --stat"), &args).0, 2);
    }
}
