//! Skenario U2 (plan service OS): drain sebelum restart untuk pembaruan.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::RunState;
use mandor_usecase::RunError;
use mandor_usecase::ports::Clock as _;
use mandor_usecase::run::{DRAIN_WINDOW_MS, DrainOutcome};
use mandor_usecase::run_ports::ReviewTrigger;
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn setup() -> (FakeLark, FakeClock, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    let st = station(&lark, &clock, "ou_me");
    (lark, clock, st)
}

async fn drain(st: &Station, clock: &FakeClock) -> DrainOutcome {
    st.svc
        .drain_gate()
        .drain(&st.runs, clock.now_ms())
        .await
        .unwrap()
}

async fn awaiting_dev(st: &Station) {
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
}

#[tokio::test]
async fn drain_idle_menolak_start_sebelum_klaim_dan_worktree() {
    let (_lark, clock, st) = setup();
    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);

    let err = st.svc.start("rec1", "repo1", None).await.unwrap_err();
    assert_eq!(err, RunError::Draining);
    assert!(st.runs.all().is_empty(), "tidak ada run yang dibuat");
    assert!(st.git.st().prepared.is_empty(), "tidak ada worktree");
    assert!(st.outbox.ops().is_empty(), "tidak ada tulis Lark");
}

#[tokio::test]
async fn drain_idle_menolak_pesan_dan_publish_tanpa_mengubah_run() {
    let (_lark, clock, st) = setup();
    awaiting_dev(&st).await;
    let before = st.runs.only();
    let chat_before = st.runs.chat.lock().unwrap().len();

    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);
    let sent = st.svc.send_message(&before.id, "lanjut").await.unwrap_err();
    assert_eq!(sent, RunError::Draining);
    let now = st.svc.send_now(&before.id, "lanjut").await.unwrap_err();
    assert_eq!(now, RunError::Draining);
    assert_eq!(
        st.svc.publish(&before.id).await.unwrap_err(),
        RunError::Draining
    );
    assert_eq!(
        st.svc.resume_failed(&before.id, None).await.unwrap_err(),
        RunError::Draining
    );

    let after = st.runs.only();
    assert_eq!(after.state, RunState::AwaitingDev);
    assert_eq!(after.generation, before.generation);
    assert_eq!(st.runs.chat.lock().unwrap().len(), chat_before);
}

#[tokio::test]
async fn drain_busy_saat_agent_berjalan_dan_kerja_baru_tetap_boleh() {
    let (lark, clock, st) = setup();
    st.svc.start("rec1", "repo1", None).await.unwrap();
    assert_eq!(st.runs.only().state, RunState::Running);

    assert_eq!(drain(&st, &clock).await, DrainOutcome::Busy);

    let mut t = ready_task("rec2", &["ou_me"]);
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    st.svc.start("rec2", "repo1", None).await.unwrap();
}

#[tokio::test]
async fn drain_busy_saat_kerja_sedang_dimulai() {
    let (_lark, clock, st) = setup();
    let gate = st.svc.drain_gate();
    let work = gate.try_enter(clock.now_ms()).unwrap();

    assert_eq!(drain(&st, &clock).await, DrainOutcome::Busy);
    drop(work);
    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);
}

#[tokio::test]
async fn drain_kedua_tidak_memperpanjang_jendela() {
    let (_lark, clock, st) = setup();
    let start = clock.now_ms();
    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);
    clock.set(start + 60_000);
    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);

    clock.set(start + DRAIN_WINDOW_MS + 1);
    st.svc.start("rec1", "repo1", None).await.unwrap();
}

#[tokio::test]
async fn undrain_membuka_kembali_kerja_baru() {
    let (_lark, clock, st) = setup();
    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);
    assert!(st.svc.drain_gate().is_draining(clock.now_ms()));
    st.svc.drain_gate().undrain();
    assert!(!st.svc.drain_gate().is_draining(clock.now_ms()));
    st.svc.start("rec1", "repo1", None).await.unwrap();
}

#[tokio::test]
async fn jendela_drain_habis_membuka_kembali_kerja_baru() {
    let (_lark, clock, st) = setup();
    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);
    assert_eq!(
        st.svc.start("rec1", "repo1", None).await.unwrap_err(),
        RunError::Draining
    );
    clock.set(clock.now_ms() + DRAIN_WINDOW_MS);
    st.svc.start("rec1", "repo1", None).await.unwrap();
}

#[tokio::test]
async fn drain_idle_menolak_review_takeover_plan_dan_fix() {
    let (_lark, clock, st) = setup();
    awaiting_dev(&st).await;
    let run_id = st.runs.only().id;
    assert_eq!(drain(&st, &clock).await, DrainOutcome::Idle);

    let review = st
        .svc
        .start_review(&run_id, ReviewTrigger::Manual, None)
        .await
        .unwrap_err();
    assert_eq!(review, RunError::Draining);
    assert_eq!(
        st.svc.approve_plan(&run_id, &[]).await.unwrap_err(),
        RunError::Draining
    );
    assert_eq!(
        st.svc.continue_to_plan(&run_id).await.unwrap_err(),
        RunError::Draining
    );
    assert_eq!(
        st.svc.takeover("rec1", "repo1").await.unwrap_err(),
        RunError::Draining
    );
    assert_eq!(
        st.svc.start_fix("rec1", "perbaiki", &[]).await.unwrap_err(),
        RunError::Draining
    );
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
}
