//! Smoke test Rilis 2.0 (review AC) terhadap kartu uji, repo uji, dan login
//! Claude milik pengguna. Tidak berjalan di CI. Urutan: F1 lalu F2 pada run
//! yang sama, F3 butuh reviewer manusia. Jalankan:
//!
//! ```text
//! MANDOR_E2E=1 MANDOR_E2E_DESIGN=/path/ke/folder-desain \
//!   cargo test -p mandor-e2e --test rilis2_smoke -- --ignored --test-threads 1
//! ```
//!
//! Biaya dan token setiap review dicetak ke stderr untuk dicatat di
//! `docs/progress/rilis-2.md`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr
)]

mod common;

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use common::{Env, Relay, card, env, run_of, wait_awaiting_dev, wait_for};
use serde_json::{Value, json};

const TURN: Duration = Duration::from_secs(20 * 60);
const GAPS: [&str; 3] = ["missing", "partly", "conflicts"];

fn rows(relay: &mut Relay, run_id: &str) -> Vec<Value> {
    relay.call("ledger.get", &json!({"run_id": run_id}))["rows"]
        .as_array()
        .unwrap()
        .clone()
}

/// Sumber desain smoke; didaftarkan bila belum ada.
fn design_source(relay: &mut Relay) -> String {
    let path = std::env::var("MANDOR_E2E_DESIGN")
        .unwrap_or_else(|_| panic!("MANDOR_E2E_DESIGN wajib diisi (folder desain uji)"));
    let list = relay.call("design_sources.list", &Value::Null);
    if let Some(s) = list
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == path.as_str())
    {
        assert_eq!(
            s["ready"], true,
            "sumber desain tidak siap: {}",
            s["status"]
        );
        return s["id"].as_str().unwrap().to_owned();
    }
    let s = relay.call(
        "design_sources.add",
        &json!({"name": "smoke-design", "path": path}),
    );
    s["id"].as_str().unwrap().to_owned()
}

/// Menunggu review terakhir selesai lalu mencetak biaya dan token.
fn finished_review(relay: &mut Relay, run_id: &str) -> Value {
    let mut last = Value::Null;
    wait_for("review selesai", TURN, || {
        let history = relay.call("review.history", &json!({"run_id": run_id}));
        last = history
            .as_array()
            .unwrap()
            .first()
            .cloned()
            .unwrap_or(Value::Null);
        !last.is_null() && last["status"] != "running"
    });
    eprintln!(
        "review {}: status {}, browser {}, biaya ${}, token in {} out {}",
        last["id"],
        last["status"],
        last["browser_used"],
        last["cost_usd"],
        last["input_tokens"],
        last["output_tokens"]
    );
    assert_eq!(
        last["status"], "finished",
        "review gagal: {}",
        last["error"]
    );
    last
}

fn ids_with(rows: &[Value], verdict: &str) -> Vec<String> {
    rows.iter()
        .filter(|r| r["in_gate"] == true && r["verdict"] == verdict)
        .map(|r| r["ac_id"].as_str().unwrap().to_owned())
        .collect()
}

fn start_relay(e: &Env) -> Relay {
    let mut relay = Relay::open(e);
    assert_eq!(
        relay.call("status", &Value::Null)["lark"]["logged_in"],
        true,
        "login Lark lewat UI lebih dulu"
    );
    relay.call("board.refresh", &Value::Null);
    relay
}

/// Covers F1: Start dengan sumber desain, kolom Design terisi, satu baris
/// Follow design, lalu approve.
#[test]
#[ignore = "smoke langsung; butuh Lark, GitHub, dan Claude milik pengguna"]
fn f1_sumber_desain_dan_follow_design_saat_approve() {
    let Some(e) = env() else { return };
    let mut relay = start_relay(&e);
    let c = card(&mut relay, &e.record);
    assert_eq!(
        c["start_blocked"],
        Value::Null,
        "Start diblokir: {}",
        c["start_blocked"]
    );
    let source = design_source(&mut relay);
    let run = relay.call(
        "runs.start",
        &json!({
            "record_id": e.record,
            "repo_id": e.repo,
            "note": "Smoke test mandor Rilis 2: buat perubahan kecil saja.",
            "planning": {"kind": "plan", "brainstorm_skill": null, "plan_skill": null},
            "design_sources": [source],
        }),
    );
    let run_id = run["id"].as_str().unwrap().to_owned();
    let planned = wait_awaiting_dev(&mut relay, &e.record, "plan siap", TURN);
    assert_eq!(planned["phase"], "plan");
    assert!(planned["plan"].is_string(), "plan kosong");

    let ledger = rows(&mut relay, &run_id);
    assert!(!ledger.is_empty(), "kartu uji butuh AC");
    assert!(
        ledger.iter().any(|r| r["design"] != "not_checked"),
        "kolom Design tidak terisi: {ledger:?}"
    );
    let follow: Vec<String> = ledger
        .iter()
        .find(|r| GAPS.contains(&r["design"].as_str().unwrap_or_default()))
        .map(|r| r["ac_id"].as_str().unwrap().to_owned())
        .into_iter()
        .collect();
    if follow.is_empty() {
        eprintln!("tidak ada selisih desain; Follow design dilewati");
    }
    relay.call(
        "runs.approve_plan",
        &json!({"run_id": run_id, "follow_design": follow}),
    );
    wait_awaiting_dev(&mut relay, &e.record, "implementasi selesai", TURN);
    if let Some(id) = follow.first() {
        let after = rows(&mut relay, &run_id);
        let row = after.iter().find(|r| r["ac_id"] == id.as_str()).unwrap();
        assert_eq!(row["design"], "follow_design");
        assert_eq!(row["in_gate"], false);
        eprintln!(
            "Periksa field mandor_design_gaps di kartu {} pada Lark.",
            e.record
        );
    }
}

/// Covers F2: Push & PR memulai review; AC Fail dikirim ke agent, review lagi,
/// AC Can't verify dikonfirmasi, lalu PR berisi ringkasan AC.
#[test]
#[ignore = "smoke langsung; jalankan setelah f1 pada run yang sama"]
fn f2_review_kirim_temuan_konfirmasi_lalu_pr() {
    let Some(e) = env() else { return };
    let mut relay = start_relay(&e);
    let run = run_of(&mut relay, &e.record).expect("jalankan f1 lebih dulu");
    let run_id = run["id"].as_str().unwrap().to_owned();
    assert_eq!(run["state"], "awaiting_dev");

    let first = relay.call("runs.publish", &json!({"run_id": run_id}));
    assert_eq!(first["outcome"], "review_started", "{first}");
    finished_review(&mut relay, &run_id);
    let mut ledger = rows(&mut relay, &run_id);

    let failed = ids_with(&ledger, "fail");
    if failed.is_empty() {
        eprintln!("tidak ada AC Fail; langkah Send to agent dilewati");
    } else {
        relay.call(
            "review.send_findings",
            &json!({"run_id": run_id, "ac_ids": failed}),
        );
        wait_awaiting_dev(&mut relay, &e.record, "perbaikan temuan selesai", TURN);
        let again = relay.call("runs.publish", &json!({"run_id": run_id}));
        assert_eq!(again["outcome"], "review_started", "{again}");
        finished_review(&mut relay, &run_id);
        ledger = rows(&mut relay, &run_id);
        assert!(
            ids_with(&ledger, "fail").is_empty(),
            "AC masih Fail setelah perbaikan: {ledger:?}"
        );
    }

    let confirmed = ids_with(&ledger, "cant_verify");
    let published = relay.call(
        "runs.publish",
        &json!({"run_id": run_id, "confirmed": confirmed}),
    );
    assert_eq!(published["outcome"], "published", "{published}");
    let pr = published["pr_url"].as_str().unwrap().to_owned();
    eprintln!("PR {pr}: periksa bagian ringkasan AC di deskripsinya.");
    wait_for("Waiting Review di Lark", Duration::from_secs(120), || {
        relay.call("board.refresh", &Value::Null);
        card(&mut relay, &e.record)["status"] == "Waiting Review"
    });
}

/// Covers F3: komentar reviewer tepercaya memicu perbaikan dan review otomatis;
/// push ditahan bila AC gagal. Langkah reviewer manual.
#[test]
#[ignore = "smoke langsung; butuh reviewer manusia di kartu uji"]
fn f3_waiting_fix_dengan_review_otomatis() {
    let Some(e) = env() else { return };
    let mut relay = start_relay(&e);
    eprintln!(
        "Reviewer: ubah status {} ke Waiting Fix lalu tulis komentar. Menunggu hingga 30 menit…",
        e.record
    );
    wait_for("perbaikan dimulai", Duration::from_secs(30 * 60), || {
        relay.call("board.refresh", &Value::Null);
        card(&mut relay, &e.record)["run"]["state"] == "wf_running"
    });
    let run_id = run_of(&mut relay, &e.record).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let review = finished_review(&mut relay, &run_id);
    assert_eq!(review["trigger"], "automatic_fix");
    let ledger = rows(&mut relay, &run_id);
    if ids_with(&ledger, "fail").is_empty() {
        wait_for(
            "kembali ke Waiting Review",
            Duration::from_secs(30 * 60),
            || {
                relay.call("board.refresh", &Value::Null);
                card(&mut relay, &e.record)["status"] == "Waiting Review"
            },
        );
    } else {
        let run = wait_awaiting_dev(&mut relay, &e.record, "push ditahan", TURN);
        assert!(
            run["note"]
                .as_str()
                .unwrap_or_default()
                .starts_with("Push held"),
            "{run}"
        );
    }
}

/// Covers AE8: hook reviewer menolak navigasi ke situs di luar origin app dan
/// menyebut alasannya (yang dicatat Claude di log hook).
#[test]
#[ignore = "smoke langsung; butuh biner mandord"]
fn ae8_hook_reviewer_menolak_situs_luar() {
    let Some(e) = env() else { return };
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("review.json");
    std::fs::write(&state, r#"{"origins":["http://localhost:5173"],"tabs":{"1":{"url":"http://localhost:5173/","settled":true}}}"#).unwrap();
    let hook = |url: &str| {
        let mut child = Command::new(&e.bin)
            .args(["hook", "--review"])
            .arg(&state)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = json!({
            "tool_name": "mcp__claude-in-chrome__navigate",
            "tool_input": {"url": url, "tabId": 1},
        });
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let allowed = hook("http://localhost:5173/invoices");
    assert_eq!(
        allowed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let denied = hook("https://example.com/");
    assert_eq!(denied.status.code(), Some(2));
    let reason = String::from_utf8_lossy(&denied.stderr);
    assert!(reason.contains("mandor"), "{reason}");
    eprintln!("alasan tolak: {reason}");
}
