//! Gerbang Push & PR, ringkasan AC di PR, dan review perbaikan otomatis (U11
//! Rilis 2).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{
    AcRow, Claim, GateDecision, GateMode, Reply, RowStatus, RunState, TaskStatus, Verdict,
};
use mandor_usecase::AcItem;
use mandor_usecase::run::PublishOutcome;
use mandor_usecase::run_ports::{RunRecord, WorkChanges};
use mandor_usecase::waiting_fix::DEFAULT_QUIET_SECS as QUIET;
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
    put_acs(&lark);
    (lark, clock)
}

fn put_acs(lark: &FakeLark) {
    for (i, id) in ["ac1", "ac2", "ac3"].iter().enumerate() {
        lark.acs.lock().unwrap().insert(
            (*id).to_owned(),
            AcItem {
                record_id: (*id).to_owned(),
                label: Some(format!("AC-{}", i + 1)),
                text: format!("teks {id}"),
                ..Default::default()
            },
        );
    }
}

/// Run menunggu dev dengan perubahan siap di-push.
async fn waiting(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let wt = st.runs.only().worktree.unwrap();
    st.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/a.rs".to_owned()],
            diff: "+fn a(){}".to_owned(),
        },
    );
    let id = st.runs.only().id;
    (st, id)
}

/// Verdict buku AC run, sesuai urutan AC-1..AC-3.
fn verdicts(st: &Station, run: &str, v: [Verdict; 3]) {
    let mut ledgers = st.reviews.ledgers.lock().unwrap();
    let rows = ledgers.get_mut(run).unwrap();
    for (r, v) in rows.iter_mut().zip(v) {
        r.verdict = v;
    }
}

fn decision(confirmed: &[&str], reason: Option<&str>) -> GateDecision {
    GateDecision {
        confirmed: confirmed.iter().map(|s| (*s).to_owned()).collect(),
        push_anyway_reason: reason.map(str::to_owned),
        decided_by: Some("Beni Handoko".to_owned()),
    }
}

fn body(st: &Station) -> String {
    st.git.st().pr_bodies.last().cloned().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae1_blocked_lalu_konfirmasi_lalu_pr_mencatat_checked_manually() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    verdicts(
        &st,
        &run,
        [Verdict::Pass, Verdict::CantVerify, Verdict::Fail],
    );
    let out = st
        .svc
        .publish_gated(&run, &GateDecision::default())
        .await
        .unwrap();
    assert_eq!(
        out,
        PublishOutcome::Blocked {
            failed: vec!["AC-3".to_owned()]
        }
    );
    assert!(st.git.st().pushes.is_empty());
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);

    // Review ulang: AC-3 lulus.
    verdicts(
        &st,
        &run,
        [Verdict::Pass, Verdict::CantVerify, Verdict::Pass],
    );
    let out = st
        .svc
        .publish_gated(&run, &GateDecision::default())
        .await
        .unwrap();
    assert_eq!(
        out,
        PublishOutcome::NeedsConfirmation {
            unverified: vec!["AC-2".to_owned()]
        }
    );
    let out = st
        .svc
        .publish_gated(&run, &decision(&["ac2"], None))
        .await
        .unwrap();
    assert!(matches!(out, PublishOutcome::Published { .. }), "{out:?}");
    let b = body(&st);
    assert!(b.contains("## Acceptance criteria review"), "{b}");
    assert!(b.contains("AC-2: Checked manually by Beni Handoko"), "{b}");
    let rows = st.svc.ledger(&run).await.unwrap();
    assert_eq!(rows[1].decided_by.as_deref(), Some("Beni Handoko"));
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae3_push_anyway_dan_konfirmasi_dalam_satu_panggilan() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    verdicts(
        &st,
        &run,
        [Verdict::Pass, Verdict::CantVerify, Verdict::Fail],
    );
    let out = st
        .svc
        .publish_gated(&run, &decision(&["ac2"], Some("API dependency down")))
        .await
        .unwrap();
    assert!(matches!(out, PublishOutcome::Published { .. }), "{out:?}");
    let b = body(&st);
    assert!(
        b.contains("AC-3: Skipped by Beni Handoko: API dependency down"),
        "{b}"
    );
    assert!(b.contains("AC-2: Checked manually"), "{b}");
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae2_mode_peringatan_membuat_pr_tanpa_alasan_dan_mencatat_fail() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].gate_mode = Some(GateMode::Warn);
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only().id;
    verdicts(&st, &run, [Verdict::Pass, Verdict::Pass, Verdict::Fail]);
    let out = st
        .svc
        .publish_gated(&run, &GateDecision::default())
        .await
        .unwrap();
    let PublishOutcome::Published { warnings, .. } = out else {
        panic!("{out:?}")
    };
    assert_eq!(warnings, ["AC-3"]);
    assert!(body(&st).contains("AC-3: Fail"), "{}", body(&st));
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae4_tanpa_review_berlaku_review_dimulai_tanpa_push() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let out = st
        .svc
        .publish_gated(&run, &GateDecision::default())
        .await
        .unwrap();
    assert!(
        matches!(out, PublishOutcome::ReviewStarted { .. }),
        "{out:?}"
    );
    assert_eq!(st.runs.only().state, RunState::Reviewing);
    assert!(st.git.st().pushes.is_empty());
    let review = st.agent.last().req.review.unwrap();
    assert!(!review.chrome);
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_ac_gagal_tetap_push_dengan_peringatan_dan_catatan_sinkron() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    verdicts(&st, &run, [Verdict::Pass, Verdict::Pass, Verdict::Pass]);
    *lark.acs_fail.lock().unwrap() = true;
    let out = st
        .svc
        .publish_gated(&run, &GateDecision::default())
        .await
        .unwrap();
    let PublishOutcome::Published { sync_warning, .. } = out else {
        panic!("{out:?}")
    };
    assert!(sync_warning.unwrap().contains("could not be refreshed"));
    assert!(body(&st).contains("AC last synced at"), "{}", body(&st));
}

#[tokio::test(flavor = "multi_thread")]
async fn push_gagal_setelah_keputusan_tersimpan_retry_tanpa_meminta_ulang() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    verdicts(
        &st,
        &run,
        [Verdict::Pass, Verdict::CantVerify, Verdict::Pass],
    );
    st.git.st().fail_create_pr.push("repo1".to_owned());
    assert!(
        st.svc
            .publish_gated(&run, &decision(&["ac2"], None))
            .await
            .is_err()
    );
    assert_eq!(
        st.svc.ledger(&run).await.unwrap()[1].verdict,
        Verdict::CheckedManually
    );
    st.git.st().fail_create_pr.clear();
    let out = st
        .svc
        .publish_gated(&run, &GateDecision::default())
        .await
        .unwrap();
    assert!(matches!(out, PublishOutcome::Published { .. }), "{out:?}");
}

// --- Review perbaikan otomatis (KTD-40, AE6) ---

const NOW_MS: i64 = 10 * DAY;
const NOW: i64 = NOW_MS / 1000;
const BRANCH: &str = "mandor/pgs-7-tab-data";

fn wt() -> PathBuf {
    PathBuf::from(format!("/wt/{BRANCH}"))
}

/// Record Waiting Fix dengan run lama dan buku AC berisi tiga AC lulus.
async fn fix_setup(mode: GateMode) -> (FakeLark, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(NOW_MS);
    lark.clock.store(NOW_MS, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    t.status = TaskStatus::WaitingFix;
    t.branch = Some(BRANCH.to_owned());
    t.repo_id = Some("repo1".to_owned());
    t.pr_url = Some("https://github.com/o/r/pull/1".to_owned());
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned(), "ac3".to_owned()];
    t.claim = Some(Claim {
        open_id: "ou_me".to_owned(),
        ts_ms: NOW_MS,
        nonce: "n".to_owned(),
    });
    t.comment_cursor = Some(format!("{}:", NOW - 3600));
    t.kanban_mirror = Some("in_progress".to_owned());
    t.last_modified_ms = NOW_MS;
    lark.put(t);
    put_acs(&lark);
    let st = station(&lark, &clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].gate_mode = Some(mode);
    let mut r = RunRecord::new("run1".to_owned(), "repo1".to_owned(), "rec1".to_owned());
    r.state = RunState::InReviewLoop;
    r.generation = 2;
    r.branch = Some(BRANCH.to_owned());
    r.worktree = Some(wt());
    r.session_id = Some("sess-lama".to_owned());
    r.session_started = true;
    r.pr_url = Some("https://github.com/o/r/pull/1".to_owned());
    st.runs.insert(r);
    st.git.st().worktrees.insert(wt());
    let rows: Vec<AcRow> = (1..=3)
        .map(|i| {
            let mut a = AcRow::new(
                &format!("ac{i}"),
                Some(&format!("AC-{i}")),
                &format!("teks ac{i}"),
                RowStatus::Active,
            );
            a.verdict = Verdict::Pass;
            a
        })
        .collect();
    st.reviews
        .ledgers
        .lock()
        .unwrap()
        .insert("run1".to_owned(), rows);
    st.svc.sync().poll(true, &[]).await.unwrap();
    (lark, st)
}

/// Satu giliran perbaikan dengan perubahan sampai review otomatis berjalan.
async fn fix_turn_reviewing(lark: &FakeLark, st: &Station) {
    *lark.replies.lock().unwrap() = vec![Reply {
        record_id: "rec1".to_owned(),
        comment_id: "c1".to_owned(),
        reply_id: "r1".to_owned(),
        author: "ou_qa".to_owned(),
        create_time: NOW - 600,
        text: "Tombol simpan error".to_owned(),
    }];
    let report = st.svc.tick_waiting_fix(QUIET).await.unwrap();
    assert_eq!(report.started, ["rec1"]);
    st.svc.sync().drain_outbox(20).await.unwrap();
    st.git.set_changes(
        &wt(),
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/form.rs".to_owned()],
            diff: "+fix".to_owned(),
        },
    );
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::Reviewing).await;
    assert!(st.git.st().pushes.is_empty(), "belum push sebelum review");
    let req = st.agent.last().req;
    assert!(req.review.is_some());
}

fn report(st: &Station, ac: &str, v: Verdict) {
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    let rt = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        rt.block_on(st.svc.ledger_set_verdict(&scope, ac, v, Some("x")))
    })
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae6_review_otomatis_gagal_di_mode_blok_menahan_push() {
    let (lark, st) = fix_setup(GateMode::Block).await;
    fix_turn_reviewing(&lark, &st).await;
    report(&st, "ac1", Verdict::Pass);
    report(&st, "ac2", Verdict::Pass);
    report(&st, "ac3", Verdict::Fail);
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only();
    assert!(
        run.note.as_deref().unwrap().contains("Push held"),
        "{run:?}"
    );
    assert!(st.git.st().pushes.is_empty());
    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::InProgress);
    let notes = st.notifier.sent.lock().unwrap().clone();
    assert!(
        notes.iter().any(|n| n.body.contains("Push held")),
        "{notes:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn review_otomatis_mode_peringatan_tetap_push_ke_waiting_review() {
    let (lark, st) = fix_setup(GateMode::Warn).await;
    fix_turn_reviewing(&lark, &st).await;
    report(&st, "ac1", Verdict::Pass);
    report(&st, "ac2", Verdict::Pass);
    report(&st, "ac3", Verdict::Fail);
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::InReviewLoop).await;
    assert_eq!(st.git.st().pushes, [BRANCH]);
    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::WaitingReview);
}

#[tokio::test(flavor = "multi_thread")]
async fn review_otomatis_yang_gagal_menahan_push_dengan_alasan() {
    let (lark, st) = fix_setup(GateMode::Block).await;
    fix_turn_reviewing(&lark, &st).await;
    st.agent.last().budget_stop();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only();
    assert!(
        run.note.as_deref().unwrap().contains("cost limit"),
        "{run:?}"
    );
    assert!(st.git.st().pushes.is_empty());
}
