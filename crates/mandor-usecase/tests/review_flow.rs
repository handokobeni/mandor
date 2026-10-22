//! Sesi reviewer AC (U9 Rilis 2): mulai, verdict lewat tool ledger, akhir
//! normal atau gagal, antrean pesan dev, kirim temuan, dan restart.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::app_run::{AppRunState, AppSettings, TestMode};
use mandor_domain::{RunState, Verdict};
use mandor_usecase::run_ports::{
    AppMemberRecord, AppRecord, AppRecords, ReviewHistory, ReviewStatus, ReviewTrigger,
};
use mandor_usecase::{AcItem, PortError, RunError};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Tab Data".clone_into(&mut t.title);
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned(), "ac3".to_owned()];
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    for id in ["ac1", "ac2", "ac3"] {
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

/// Run yang sudah selesai giliran pertamanya dan menunggu dev.
async fn waiting(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let id = st.runs.only().id;
    (st, id)
}

fn verdict(st: &Station, ac: &str, v: Verdict, evidence: &str) {
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    let rt = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        rt.block_on(st.svc.ledger_set_verdict(&scope, ac, v, Some(evidence)))
    })
    .unwrap();
}

async fn reviewed(st: &Station, run: &str) -> mandor_usecase::run_ports::ReviewRecord {
    eventually(|| st.runs.only().state != RunState::Reviewing).await;
    st.reviews.reviews(run).await.unwrap().remove(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn review_semua_ac_tersimpan_dan_run_kembali_ke_dev() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let rec = st
        .svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    assert_eq!(st.runs.only().state, RunState::Reviewing);
    let req = st.agent.last().req;
    let review = req.review.clone().unwrap();
    assert_eq!(review.review_id, rec.id);
    assert!(!review.chrome, "repo tanpa app");
    assert_eq!(review.budget_cents, 200);
    assert!(matches!(
        req.session,
        mandor_usecase::run_ports::SessionRef::New(_)
    ));
    assert!(req.message.contains("REVIEW"), "{}", req.message);
    assert!(req.message.contains("REASON:No app for this run"));
    // Sesi pelaksana tidak berubah.
    assert_eq!(st.runs.only().session_id, st.runs.all()[0].session_id);

    verdict(&st, "ac1", Verdict::Pass, "src/a.rs:10");
    verdict(&st, "ac2", Verdict::Fail, "test b gagal");
    verdict(&st, "ac3", Verdict::CantVerify, "butuh browser");
    st.agent.last().done();
    let done = reviewed(&st, &run).await;
    assert_eq!(done.status, ReviewStatus::Finished);
    assert_eq!(done.verdicts.len(), 3);
    assert!(!done.stale);
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
    assert_eq!(st.runs.only().review_id, None);
    assert_eq!(st.agent.ended_reviews.lock().unwrap().clone(), [rec.id]);
    let notes = st.notifier.sent.lock().unwrap().clone();
    assert!(
        notes
            .last()
            .unwrap()
            .body
            .contains("1 passed, 1 failed, 1 can't verify"),
        "{notes:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ac_yang_tidak_dilaporkan_menjadi_cant_verify() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Pass, "ok");
    st.agent.last().done();
    reviewed(&st, &run).await;
    let rows = st.svc.ledger(&run).await.unwrap();
    assert_eq!(rows[2].verdict, Verdict::CantVerify);
    assert_eq!(rows[2].evidence.as_deref(), Some("reviewer did not report"));
}

#[tokio::test(flavor = "multi_thread")]
async fn batas_biaya_membuat_review_failed_dan_ac_sisa_tetap_not_reviewed() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Pass, "ok");
    st.agent.last().budget_stop();
    let done = reviewed(&st, &run).await;
    assert_eq!(done.status, ReviewStatus::Failed);
    assert_eq!(
        done.error.as_deref(),
        Some("Review stopped at the cost limit")
    );
    assert_eq!(done.cost_usd, Some(2.01));
    assert_eq!(done.usage.output_tokens, 5);
    let rows = st.svc.ledger(&run).await.unwrap();
    assert_eq!(rows[2].verdict, Verdict::NotReviewed);
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae4_agent_yang_menulis_berkas_saat_review_ditandai_stale() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    let wt = st.runs.only().worktree.unwrap();
    st.git.st().trees.insert(wt, "tree-diubah".to_owned());
    st.agent.last().done();
    let done = reviewed(&st, &run).await;
    assert!(done.stale);
    assert!(st.svc.ledger(&run).await.unwrap()[0].stale);
}

#[tokio::test(flavor = "multi_thread")]
async fn pesan_dev_selama_review_diantrekan_dan_push_ditolak() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let out = st.svc.send_message(&run, "tambah test").await.unwrap();
    assert_eq!(out, mandor_usecase::SendOutcome::Queued);
    assert!(matches!(
        st.svc.publish(&run).await,
        Err(RunError::InvalidState(_))
    ));
    let spawned = st.agent.runs.lock().unwrap().len();
    st.agent.last().done();
    eventually(|| st.agent.runs.lock().unwrap().len() == spawned + 1).await;
    let next = st.agent.last().req;
    assert!(next.review.is_none());
    assert_eq!(next.message, "tambah test");
    assert!(matches!(
        next.session,
        mandor_usecase::run_ports::SessionRef::Resume(_)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn kirim_temuan_hanya_untuk_baris_fail() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Fail, "salah");
    st.agent.last().done();
    reviewed(&st, &run).await;
    let err = st
        .svc
        .send_findings(&run, &["ac1".to_owned()])
        .await
        .unwrap_err();
    assert!(matches!(err, RunError::Port(PortError::Rejected(_))));
    st.svc
        .send_findings(&run, &["ac2".to_owned()])
        .await
        .unwrap();
    assert_eq!(st.agent.last().req.message, "FINDINGS ac2");
    assert_eq!(
        st.svc.ledger(&run).await.unwrap()[1].verdict,
        Verdict::FixInProgress
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn batal_review_dan_restart_daemon_membuat_review_tidak_selesai() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    st.svc.cancel_review(&run).await.unwrap();
    let done = reviewed(&st, &run).await;
    assert_eq!(done.status, ReviewStatus::Cancelled);
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);

    // Restart di tengah review: Failed, run menunggu dev, proses dihentikan.
    let second = st
        .svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    st.runs.rows.lock().unwrap().get_mut(&run).unwrap().0.pid = Some(4321);
    let marked = st.svc.reconcile().await.unwrap();
    assert_eq!(marked, std::slice::from_ref(&run));
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
    let last = st
        .reviews
        .reviews(&run)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id == second.id)
        .unwrap();
    assert_eq!(last.status, ReviewStatus::Failed);
    assert!(st.agent.reaped.lock().unwrap().contains(&4321));
}

/// Satu review yang langsung selesai.
async fn cycle(st: &Station, run: &str) {
    st.svc
        .start_review(run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae5_browser_hanya_bila_semua_syarat_kesiapan_terpenuhi() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
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

    let reason = |st: &Station| {
        let msg = st.agent.last().req.message;
        msg.split("REASON:")
            .nth(1)
            .unwrap()
            .split(" SKILL:")
            .next()
            .unwrap()
            .to_owned()
    };
    // App tidak menyala.
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "The app is not running");

    let mut app = AppRecord {
        run_id: run.clone(),
        state: AppRunState::Running,
        members: vec![AppMemberRecord {
            repo_id: "repo1".to_owned(),
            url: Some("http://localhost:5173".to_owned()),
            pgid: Some(1),
            state: AppRunState::Running,
        }],
        snapshot: [("repo1".to_owned(), "tree-lama".to_owned())].into(),
        ..AppRecord::default()
    };
    st.apps.save_app_record(&app).await.unwrap();
    st.apps.set_app_owner(Some(&run)).await.unwrap();
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "app running older code");

    st.git.st().trees.insert(wt, "tree-s2".to_owned());
    app.snapshot
        .insert("repo1".to_owned(), "tree-s2".to_owned());
    st.apps.save_app_record(&app).await.unwrap();
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "Login not confirmed");

    app.login_confirmed = true;
    st.apps.save_app_record(&app).await.unwrap();
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "Claude in Chrome extension not connected");
    assert!(!st.agent.last().req.review.unwrap().chrome);

    *st.reviews.chrome.lock().unwrap() = true;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, Some("qa:review"))
        .await
        .unwrap();
    let req = st.agent.last().req;
    let review = req.review.unwrap();
    assert!(review.chrome);
    assert_eq!(review.origins, ["http://localhost:5173"]);
    assert!(req.message.contains("SKILL:qa:review"));

    // Uji browser dimatikan untuk repo.
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.repos.repos.lock().unwrap()[0].app.browser_tests = false;
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "Browser tests are off");
}

#[tokio::test(flavor = "multi_thread")]
async fn setelan_reviewer_baru_dipakai_review_berikutnya_dan_prasyarat_dicek() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.reviews
        .set_reviewer_settings(&mandor_usecase::run_ports::ReviewerSettings {
            model: Some("haiku".to_owned()),
            budget_cents: 50,
            timeout_secs: 120,
        })
        .await
        .unwrap();
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let review = st.agent.last().req.review.unwrap();
    assert_eq!(review.model.as_deref(), Some("haiku"));
    assert_eq!(review.budget_cents, 50);
    assert_eq!(review.timeout.as_secs(), 120);
    // Review kedua saat yang pertama berjalan ditolak.
    assert!(matches!(
        st.svc.start_review(&run, ReviewTrigger::Manual, None).await,
        Err(RunError::InvalidState(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn kesiapan_per_item_menyebut_pemilik_app_lain_dan_dialog_hanya_untuk_web() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let r = st.svc.review_readiness(&run).await.unwrap();
    assert!(!r.needs_dialog(), "repo tanpa app web");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        ..AppSettings::default()
    };
    st.apps.set_app_owner(Some("run-lain")).await.unwrap();
    let r = st.svc.review_readiness(&run).await.unwrap();
    assert!(r.needs_dialog());
    assert_eq!(r.app, mandor_usecase::review::AppReadiness::NotRunning);
    assert_eq!(r.other_owner.as_deref(), Some("run-lain"));
    assert_eq!(r.facts().reason.as_deref(), Some("The app is not running"));
}
