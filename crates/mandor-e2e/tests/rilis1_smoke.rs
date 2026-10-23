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

mod common;

use std::process::Command;
use std::time::Duration;

use common::{Relay, card, env, wait_for};
use serde_json::{Value, json};

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
