//! Lab data fase 2 (plan 2026-11-10-001, U14): snapshot, tulis data selama
//! sesi, restore terverifikasi, kunci, kegagalan, tunda di laptop, dan
//! pemulihan saat start.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::app_run::{AppRunState, AppSettings, TestMode};
use mandor_domain::lab::{LabNeed, Simulated};
use mandor_domain::lab_data::{DbTarget, LabData, LabSeed, LabStore, SessionPhase, StoreKind};
use mandor_domain::{RunState, Verdict};
use mandor_usecase::review::{LedgerToolError, VerdictReport};
use mandor_usecase::run_ports::{
    AppMemberRecord, AppRecord, AppRecords, LabSessionRecord, LockOutcome, RestoreResult,
    ReviewTrigger,
};
use mandor_usecase::{AcItem, RunError};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Nilai KB".clone_into(&mut t.title);
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned()];
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    for id in ["ac1", "ac2"] {
        lark.acs.lock().unwrap().insert(
            id.to_owned(),
            AcItem {
                record_id: id.to_owned(),
                label: Some(format!("AC-{id}")),
                text: format!("teks {id}"),
                ..Default::default()
            },
        );
    }
    (lark, clock)
}

/// Perlengkapan dengan akun admin dan bagian data satu database lokal.
fn lab_with_data(st: &Station) {
    use mandor_domain::lab::{LabAccount, LabEquipment, LoginForm, SuccessMarker};
    let store = LabStore {
        name: "tenant".to_owned(),
        kind: StoreKind::Postgres(DbTarget {
            host: "localhost".to_owned(),
            port: 5432,
            database: "kelola".to_owned(),
        }),
        snapshot: "snapshot.sh".to_owned(),
        restore: "restore.sh".to_owned(),
        fingerprint: "fingerprint.sh".to_owned(),
        test_change: "change.sh".to_owned(),
    };
    st.lab.equipment.lock().unwrap().insert(
        "repo1".to_owned(),
        LabEquipment {
            accounts: vec![LabAccount {
                role: "admin".to_owned(),
                username: "admin@uji.test".to_owned(),
                default: true,
            }],
            login: LoginForm {
                url: "http://localhost:5173/login".to_owned(),
                username_field: "#email".to_owned(),
                password_field: "#password".to_owned(),
                submit: "text=Sign in".to_owned(),
                submit_path: "/api/login".to_owned(),
                success: SuccessMarker::PathPrefix("/admin".to_owned()),
                authenticated_url: "http://localhost:5173/admin".to_owned(),
            },
            safe_posts: vec![],
            fixtures_dir: None,
            data: Some(LabData {
                scripts_dir: "review-fixtures/lab".to_owned(),
                stores: vec![store],
                seeds: vec![LabSeed {
                    name: "kb".to_owned(),
                    command: "seed-kb.sh".to_owned(),
                }],
            }),
        },
    );
}

/// Run web menunggu dev dengan app milik run ini menyala, perlengkapan data
/// disetujui.
async fn waiting(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        ..AppSettings::default()
    };
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only().id;
    let wt: PathBuf = st.runs.only().worktree.unwrap();
    st.git.st().trees.insert(wt, "tree-s2".to_owned());
    let app = AppRecord {
        run_id: run.clone(),
        state: AppRunState::Running,
        members: vec![AppMemberRecord {
            repo_id: "repo1".to_owned(),
            url: Some("http://localhost:5173".to_owned()),
            pgid: Some(1),
            state: AppRunState::Running,
            prototype: None,
        }],
        snapshot: [("repo1".to_owned(), "tree-s2".to_owned())].into(),
        ..AppRecord::default()
    };
    st.apps.save_app_record(&app).await.unwrap();
    st.apps.set_app_owner(Some(&run)).await.unwrap();
    lab_with_data(&st);
    (st, run)
}

fn report(steps: &[&str]) -> VerdictReport {
    VerdictReport {
        evidence: Some("diperiksa".to_owned()),
        steps: steps.iter().map(|s| (*s).to_owned()).collect(),
        ..VerdictReport::default()
    }
}

fn needs_data() -> VerdictReport {
    VerdictReport {
        evidence: Some("butuh nilai KB di database".to_owned()),
        lab_need: Some(LabNeed::Data),
        ..VerdictReport::default()
    }
}

fn scope_of_last(st: &Station) -> mandor_usecase::review::CapabilityScope {
    let token = st.agent.last().req.ledger.unwrap().token;
    st.svc.verify_capability(token.expose()).unwrap()
}

/// Review biasa: ac1 butuh data; berakhir lalu menunggu sesi lab.
async fn review_needing_data(st: &Station, run: &str, data: bool) -> String {
    if data {
        st.svc.start_lab_data(run).await.unwrap();
    } else {
        st.svc
            .start_review(run, ReviewTrigger::Manual, None)
            .await
            .unwrap();
    }
    let ac = st.svc.ledger(run).await.unwrap()[0].ac_id.clone();
    st.svc
        .ledger_set_verdict(&scope_of_last(st), &ac, Verdict::CantVerify, needs_data())
        .await
        .unwrap();
    st.agent.last().done();
    ac
}

fn row(st: &Station, run: &str, ac: &str) -> mandor_domain::AcRow {
    st.reviews.ledgers.lock().unwrap()[run]
        .iter()
        .find(|r| r.ac_id == ac)
        .cloned()
        .unwrap()
}

/// AE1: snapshot dengan app berhenti, `lab_sql` di sesi, restore, sidik cocok
/// → `done`; verdict Pass bertanda SQL dan app menyala lagi.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae1_alur_penuh_lab_data() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.agent.last().req.review.is_some_and(|r| r.lab.is_some())).await;
    assert_eq!(st.lab.last_phase(), Some(SessionPhase::Running));
    let calls = st.lab.calls();
    let at = |what: &str| calls.iter().position(|c| c.starts_with(what)).unwrap();
    assert!(at("lock") < at("snapshot"), "{calls:?}");
    assert!(st.svc.lab_data_running(&run));
    // Proxy review boleh menulis selama sesi `running`.
    assert!(
        st.lab
            .proxies
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .1
            .write_allowed
    );
    let scope = scope_of_last(&st);
    let out = st
        .svc
        .ledger_lab_sql(&scope, "tenant", "UPDATE corporate_configs SET kb_max = 2")
        .await
        .unwrap();
    assert!(out.text.contains("Statement ran"));
    st.svc
        .ledger_set_verdict(
            &scope,
            &ac,
            Verdict::Pass,
            report(&["Upload nilai", "Periksa penanda"]),
        )
        .await
        .unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    eventually(|| st.lab.last_phase() == Some(SessionPhase::Done)).await;
    let calls = st.lab.calls();
    assert!(calls.contains(&"restore".to_owned()), "{calls:?}");
    assert!(calls.contains(&"release".to_owned()) && calls.contains(&"discard".to_owned()));
    let r = row(&st, &run, &ac);
    assert_eq!(r.verdict, Verdict::Pass);
    assert_eq!(r.evidence_detail.unwrap().simulated, Simulated::Sql);
    assert!(
        !st.lab
            .proxies
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .1
            .write_allowed
    );
    assert!(!st.svc.lab_data_running(&run));
}

/// AE2: env database app ke host staging → lab data tidak jalan.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae2_database_bukan_lokal() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    *st.lab.data_ready.lock().unwrap() = Err(
        "lab only runs against local databases (DATABASE_URL points to db.staging.example.com)"
            .to_owned(),
    );
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let r = row(&st, &run, &ac);
    assert_eq!(r.verdict, Verdict::CantVerify);
    assert!(
        r.evidence
            .unwrap()
            .contains("lab only runs against local databases")
    );
    assert!(!st.lab.calls().contains(&"snapshot".to_owned()));
}

/// AE3: restore gagal → `restore_failed`; review dan giliran baru repo itu
/// ditolak sampai developer bertindak; Retry restore yang berhasil → `done`.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae3_restore_gagal_menahan_repo() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.agent.last().req.review.is_some_and(|r| r.lab.is_some())).await;
    st.lab
        .restores
        .lock()
        .unwrap()
        .push_back(RestoreResult::Failed("restoring tenant failed".to_owned()));
    let scope = scope_of_last(&st);
    st.svc
        .ledger_set_verdict(&scope, &ac, Verdict::Pass, report(&["x"]))
        .await
        .unwrap();
    st.agent.last().done();
    eventually(|| st.lab.last_phase() == Some(SessionPhase::RestoreFailed)).await;
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(matches!(
        st.svc.start_review(&run, ReviewTrigger::Manual, None).await,
        Err(RunError::LabData(m)) if m.contains("was not restored")
    ));
    assert!(
        !st.lab.calls().contains(&"release".to_owned()),
        "kunci tetap dipegang"
    );
    st.svc.retry_lab_restore("repo1").await.unwrap();
    assert_eq!(st.lab.last_phase(), Some(SessionPhase::Done));
    assert!(
        st.svc
            .start_review(&run, ReviewTrigger::Manual, None)
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn developer_menyatakan_data_pulih() {
    let (lark, clock) = setup();
    let (st, _run) = waiting(&lark, &clock).await;
    st.lab
        .sessions
        .rows
        .lock()
        .unwrap()
        .push(session("s9", SessionPhase::RestoreFailed));
    assert!(st.svc.retry_lab_restore("repo-lain").await.is_err());
    st.svc
        .confirm_lab_data_restored("repo1", true)
        .await
        .unwrap();
    assert_eq!(st.lab.last_phase(), Some(SessionPhase::Done));
    assert!(st.lab.calls().contains(&"discard".to_owned()));
    assert!(
        st.svc
            .confirm_lab_data_restored("repo1", false)
            .await
            .is_err()
    );
}

fn session(id: &str, phase: SessionPhase) -> LabSessionRecord {
    LabSessionRecord {
        id: id.to_owned(),
        run_id: "run-lama".to_owned(),
        repo_id: "repo1".to_owned(),
        lock_keys: vec!["pg:local:5432/kelola".to_owned()],
        phase,
        snapshot_dir: Some(PathBuf::from(format!("/snap/{id}"))),
        scripts_digest: "ab".repeat(32),
        fingerprints: vec![("tenant".to_owned(), "fp0".to_owned())],
        error: Some("restoring tenant failed".to_owned()),
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

/// Tenggat kunci lewat → calon *Can't verify* "local data is locked by
/// another lab"; klien lain → alasan menyebut klien itu.
#[tokio::test(flavor = "multi_thread")]
async fn kunci_dan_klien_lain_menghentikan_lab_data() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    *st.lab.lock_outcome.lock().unwrap() = LockOutcome::Locked;
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(
        row(&st, &run, &ac)
            .evidence
            .unwrap()
            .contains("local data is locked by another lab")
    );

    *st.lab.lock_outcome.lock().unwrap() = LockOutcome::Acquired;
    *st.lab.clients.lock().unwrap() = Some("psql from 127.0.0.1 as kelola".to_owned());
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let ev = row(&st, &run, &ac).evidence.unwrap();
    assert!(
        ev.contains("another database client is connected: psql from 127.0.0.1"),
        "{ev}"
    );
    assert!(st.lab.calls().contains(&"release".to_owned()));
    assert!(!st.lab.calls().contains(&"snapshot".to_owned()));
}

/// KTD-L23: di laptop tanpa Run lab, calon data ditunda dan tidak ada
/// snapshot; KTD-L20: app milik run lain → lab data tidak jalan.
#[tokio::test(flavor = "multi_thread")]
async fn laptop_menunda_dan_app_harus_milik_run() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let ac = review_needing_data(&st, &run, false).await;
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(row(&st, &run, &ac).evidence.unwrap().contains("deferred"));
    assert!(st.lab.calls().is_empty());

    st.apps.set_app_owner(Some("run-lain")).await.unwrap();
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    // App milik run lain: review berjalan tanpa browser, lab (termasuk data)
    // tidak dimulai dan baris tetap Can't verify.
    assert_eq!(row(&st, &run, &ac).verdict, Verdict::CantVerify);
    assert!(!st.lab.calls().contains(&"snapshot".to_owned()));
}

/// `restore_waiting` saat klien lain tersambung, lalu restore setelah klien
/// putus tanpa `restore_failed`; Start manual ditolak selama terkunci.
#[tokio::test(flavor = "multi_thread")]
async fn restore_menunggu_klien_lain_lalu_selesai() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.agent.last().req.review.is_some_and(|r| r.lab.is_some())).await;
    st.lab
        .restores
        .lock()
        .unwrap()
        .push_back(RestoreResult::Waiting(
            "psql from 127.0.0.1 as kelola".to_owned(),
        ));
    *st.lab.clients.lock().unwrap() = Some("psql".to_owned());
    st.svc
        .ledger_set_verdict(&scope_of_last(&st), &ac, Verdict::Pass, report(&["x"]))
        .await
        .unwrap();
    st.agent.last().done();
    eventually(|| st.lab.last_phase() == Some(SessionPhase::RestoreWaiting)).await;
    assert!(matches!(
        st.svc
            .app_run(&run, &std::collections::BTreeMap::default())
            .await,
        Err(RunError::LabData(_))
    ));
    // Stop manual diizinkan saat `restore_waiting`.
    st.svc.app_stop(&run).await.unwrap();
    *st.lab.clients.lock().unwrap() = None;
    eventually(|| st.lab.last_phase() == Some(SessionPhase::Done)).await;
}

/// `lab_sql` hanya selama sesi data, dari token reviewer lab, dan pernyataan
/// terlarang ditolak.
#[tokio::test(flavor = "multi_thread")]
async fn lab_sql_dibatasi_sesi_dan_pernyataan() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let ac = review_needing_data(&st, &run, true).await;
    eventually(|| st.agent.last().req.review.is_some_and(|r| r.lab.is_some())).await;
    let scope = scope_of_last(&st);
    assert!(matches!(
        st.svc
            .ledger_lab_sql(&scope, "tenant", "COPY t TO PROGRAM 'id'")
            .await,
        Err(LedgerToolError::Invalid(_))
    ));
    assert!(matches!(
        st.svc.ledger_lab_sql(&scope, "lain", "SELECT 1").await,
        Err(LedgerToolError::Invalid(m)) if m.contains("unknown database store")
    ));
    assert!(matches!(
        st.svc.ledger_lab_seed(&scope, "tidak-ada").await,
        Err(LedgerToolError::Invalid(_))
    ));
    assert_eq!(
        st.svc.ledger_lab_seed(&scope, "kb").await.unwrap(),
        "seeded"
    );
    let design = st
        .svc
        .issue_capability(&run, mandor_usecase::review::CapabilityRole::Design);
    let design = st.svc.verify_capability(design.expose()).unwrap();
    assert_eq!(
        st.svc
            .ledger_lab_sql(&design, "tenant", "SELECT 1")
            .await
            .unwrap_err(),
        LedgerToolError::WrongRole
    );
    st.svc
        .ledger_set_verdict(&scope, &ac, Verdict::Pass, report(&["x"]))
        .await
        .unwrap();
    st.agent.last().done();
    eventually(|| st.lab.last_phase() == Some(SessionPhase::Done)).await;
    assert!(matches!(
        st.svc.ledger_lab_sql(&scope, "tenant", "SELECT 1").await,
        Err(LedgerToolError::NotAllowed(_) | LedgerToolError::WrongRole)
    ));
}

/// KTD-L22: saat start, `snapshotting` selesai tanpa restore, `running`
/// dipulihkan, `restore_failed` tetap memegang kunci.
#[tokio::test(flavor = "multi_thread")]
async fn pemulihan_saat_start() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    {
        let mut rows = st.lab.sessions.rows.lock().unwrap();
        rows.push(session("a", SessionPhase::Snapshotting));
        rows.push(session("b", SessionPhase::Running));
        rows.push(session("c", SessionPhase::RestoreFailed));
    }
    st.svc.recover_lab_data().await;
    let phase = |id: &str| {
        st.lab
            .sessions
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .phase
    };
    assert_eq!(phase("a"), SessionPhase::Done);
    assert_eq!(phase("b"), SessionPhase::Done);
    assert_eq!(phase("c"), SessionPhase::RestoreFailed);
    let calls = st.lab.calls();
    assert_eq!(
        calls.iter().filter(|c| *c == "restore").count(),
        1,
        "{calls:?}"
    );
    assert_eq!(calls.iter().filter(|c| c.starts_with("lock")).count(), 3);
}

/// Laporan pagi VM memuat restore yang gagal dan `restore_waiting` yang
/// melewati jendela coba ulang.
#[tokio::test(flavor = "multi_thread")]
async fn laporan_pagi_memuat_lab_data_yang_tertahan() {
    let (lark, clock) = setup();
    let st = common::vm_station(&lark, &clock, "ou_me");
    {
        let mut rows = st.lab.sessions.rows.lock().unwrap();
        rows.push(session("gagal", SessionPhase::RestoreFailed));
        let mut waiting = session("tunggu", SessionPhase::RestoreWaiting);
        waiting.updated_at_ms = 10 * DAY - 60 * 60 * 1000;
        rows.push(waiting);
        let mut fresh = session("baru", SessionPhase::RestoreWaiting);
        fresh.updated_at_ms = 10 * DAY;
        rows.push(fresh);
    }
    let report = st.svc.overnight_report().await.unwrap();
    let phases: Vec<SessionPhase> = report.lab_data.iter().map(|a| a.phase).collect();
    assert_eq!(
        phases,
        [SessionPhase::RestoreFailed, SessionPhase::RestoreWaiting]
    );
    assert_eq!(
        report.lab_data[0].error.as_deref(),
        Some("restoring tenant failed")
    );
}
