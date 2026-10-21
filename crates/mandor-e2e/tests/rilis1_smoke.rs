//! Smoke test Rilis 1 terhadap Base uji, repo GitHub uji, dan login Claude milik
//! pengguna. Tidak berjalan di CI. Jalankan:
//!
//! ```text
//! MANDOR_E2E=1 cargo test -p mandor-e2e --test rilis1_smoke -- --ignored --test-threads 1
//! ```
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Konfigurasi smoke dari environment.
struct Env {
    bin: PathBuf,
    home: PathBuf,
    record: String,
    repo: String,
}

fn env() -> Option<Env> {
    if std::env::var("MANDOR_E2E").ok().as_deref() != Some("1") {
        eprintln!("lewati: set MANDOR_E2E=1 untuk menjalankan smoke test");
        return None;
    }
    let var = |k: &str| {
        std::env::var(k).unwrap_or_else(|_| panic!("{k} wajib diisi (lihat README mandor-e2e)"))
    };
    let bin = std::env::var("MANDOR_E2E_BIN").map_or_else(
        |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mandord"),
        PathBuf::from,
    );
    Some(Env {
        bin,
        home: PathBuf::from(var("MANDOR_E2E_HOME")),
        record: var("MANDOR_E2E_RECORD"),
        repo: var("MANDOR_E2E_REPO"),
    })
}

/// Relay `mandord connect` yang sudah melakukan `hello`.
struct Relay {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Relay {
    fn open(e: &Env) -> Self {
        let token = Command::new(&e.bin)
            .arg("token")
            .env("MANDOR_HOME", &e.home)
            .output()
            .unwrap();
        assert!(token.status.success(), "mandord token gagal");
        let token = String::from_utf8(token.stdout).unwrap().trim().to_owned();
        let mut child = Command::new(&e.bin)
            .arg("connect")
            .env("MANDOR_HOME", &e.home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut relay = Self {
            child,
            stdin,
            stdout,
            next: 1,
        };
        relay.call(
            "hello",
            &json!({"token": token, "protocol": "1.0", "client": "mandor-e2e"}),
        );
        relay
    }

    /// Memanggil method; event (tanpa `id`) dilewati.
    fn call(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .unwrap();
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "relay tertutup"
            );
            let v: Value = serde_json::from_str(&line).unwrap();
            if v.get("id").and_then(Value::as_u64) == Some(id) {
                assert!(v.get("error").is_none(), "{method} gagal: {}", v["error"]);
                return v["result"].clone();
            }
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn card(relay: &mut Relay, record: &str) -> Value {
    let board = relay.call("board.list", &Value::Null);
    board["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["record_id"] == record)
        .cloned()
        .unwrap_or_else(|| panic!("record {record} tidak ada di papan pengguna"))
}

fn wait_for(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    panic!("menunggu {what} melewati batas waktu");
}

/// Covers F1: Start → agent selesai → Push & buat PR → Waiting Review.
#[test]
#[ignore = "smoke langsung; butuh Lark, GitHub, dan Claude milik pengguna"]
fn f1_start_sampai_waiting_review() {
    let Some(e) = env() else { return };
    let mut relay = Relay::open(&e);
    assert_eq!(
        relay.call("status", &Value::Null)["lark"]["logged_in"],
        true,
        "login Lark lewat UI lebih dulu"
    );
    relay.call("board.refresh", &Value::Null);
    let c = card(&mut relay, &e.record);
    assert_eq!(c["status"], "To Do", "record uji harus To Do");
    assert_eq!(
        c["start_blocked"],
        Value::Null,
        "Start diblokir: {}",
        c["start_blocked"]
    );

    let run = relay.call("runs.start", &json!({"record_id": e.record, "repo_id": e.repo, "note": "Smoke test mandor: buat perubahan kecil saja."}));
    let run_id = run["id"].as_str().unwrap().to_owned();
    wait_for(
        "agent selesai giliran pertama",
        Duration::from_secs(20 * 60),
        || {
            let runs = relay.call("runs.list", &Value::Null);
            let state = runs
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == run_id.as_str())
                .map(|r| r["state"].clone());
            assert_ne!(state, Some(json!("failed")), "run gagal");
            state == Some(json!("awaiting_dev"))
        },
    );
    wait_for(
        "status In Progress di Lark",
        Duration::from_secs(120),
        || {
            relay.call("board.refresh", &Value::Null);
            card(&mut relay, &e.record)["status"] == "In Progress"
        },
    );
    let published = relay.call("runs.publish", &json!({"run_id": run_id}));
    assert!(
        published["pr_url"]
            .as_str()
            .unwrap()
            .starts_with("https://")
    );
    wait_for(
        "status Waiting Review dan link PR di Lark",
        Duration::from_secs(120),
        || {
            relay.call("board.refresh", &Value::Null);
            let c = card(&mut relay, &e.record);
            c["status"] == "Waiting Review" && c["pr_url"] == published["pr_url"]
        },
    );
}

/// Covers F2: reviewer mengubah status ke Waiting Fix dan berkomentar; perbaikan
/// berjalan sendiri dan kembali ke Waiting Review. Langkah reviewer manual.
#[test]
#[ignore = "smoke langsung; butuh reviewer manusia di Base uji"]
fn f2_waiting_fix_kembali_ke_waiting_review() {
    let Some(e) = env() else { return };
    let mut relay = Relay::open(&e);
    eprintln!(
        "Reviewer: ubah status {} ke Waiting Fix lalu tulis komentar di record. Menunggu hingga 30 menit…",
        e.record
    );
    wait_for(
        "perbaikan dimulai (In Progress)",
        Duration::from_secs(30 * 60),
        || {
            relay.call("board.refresh", &Value::Null);
            let c = card(&mut relay, &e.record);
            c["status"] == "In Progress" && c["run"]["state"] == "wf_running"
        },
    );
    wait_for(
        "kembali ke Waiting Review",
        Duration::from_secs(30 * 60),
        || {
            relay.call("board.refresh", &Value::Null);
            card(&mut relay, &e.record)["status"] == "Waiting Review"
        },
    );
}

/// Restart daemon di tengah run tidak meninggalkan proses agent yatim dan run
/// bisa dilanjutkan.
#[test]
#[ignore = "smoke langsung; butuh run yang sedang berjalan"]
fn restart_daemon_tidak_meninggalkan_proses() {
    let Some(e) = env() else { return };
    let info: Value =
        serde_json::from_slice(&std::fs::read(e.home.join("daemon.json")).unwrap()).unwrap();
    let pid = info["pid"].as_u64().unwrap().to_string();
    assert!(
        Command::new("kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    std::thread::sleep(Duration::from_secs(15));
    let ps = Command::new("pgrep")
        .args(["-f", "claude -p --output-format stream-json"])
        .output()
        .unwrap();
    assert!(
        ps.stdout.is_empty(),
        "proses agent yatim: {}",
        String::from_utf8_lossy(&ps.stdout)
    );
    let mut relay = Relay::open(&e);
    let runs = relay.call("runs.list", &Value::Null);
    assert!(
        runs.as_array()
            .unwrap()
            .iter()
            .all(|r| r["state"] != "running"),
        "run Running tidak boleh tersisa: {runs}"
    );
}
