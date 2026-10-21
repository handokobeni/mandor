//! Fase plan sebelum implementasi: plan hanya baca, menunggu persetujuan dev.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{RunState, TaskStatus};
use mandor_usecase::run_ports::{NoticeKind, RunPhase, SessionRef, WorkChanges};
use mandor_usecase::{Planning, RunError, StartTarget};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;
const PLAN: &str = "## Plan\n1. Tambah kolom\n2. Uji";

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Tab Data".clone_into(&mut t.title);
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    (lark, clock)
}

async fn planned(lark: &FakeLark, clock: &FakeClock, planning: Planning) -> Station {
    let st = station(lark, clock, "ou_me");
    st.svc
        .start_planned(
            "rec1",
            &StartTarget::Repo("repo1".to_owned()),
            Some("pakai komponen lama"),
            &planning,
            |_| {},
        )
        .await
        .unwrap();
    st
}

async fn plan_ready(st: &Station) {
    st.agent.last().finish(PLAN);
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
}

#[tokio::test]
async fn start_dengan_plan_menjalankan_agent_hanya_baca_dengan_pesan_plan() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Plan { skill: None }).await;

    let req = st.agent.last().req;
    assert!(req.plan_mode, "giliran plan memakai mode plan (hanya baca)");
    assert!(req.message.starts_with("PLAN Tab Data"), "{}", req.message);
    assert!(req.message.contains("pakai komponen lama"));
    assert!(req.message.contains("SKILL:-"));
    assert!(st.runs.dev_messages()[0].starts_with("Started: plan with Mandor built-in."));
    let run = st.runs.only();
    assert_eq!(run.phase, RunPhase::Plan);
    assert_eq!(run.plan, None);
    // Status tetap pindah ke In Progress saat Start.
    st.svc.sync().drain_outbox(10).await.unwrap();
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::InProgress);
}

#[tokio::test]
async fn skill_plugin_diteruskan_ke_pesan_plan() {
    let (lark, clock) = setup();
    let st = planned(
        &lark,
        &clock,
        Planning::Plan {
            skill: Some("compound-engineering:ce-plan".to_owned()),
        },
    )
    .await;
    assert!(
        st.agent
            .last()
            .req
            .message
            .contains("SKILL:compound-engineering:ce-plan")
    );
}

#[tokio::test]
async fn akhir_giliran_plan_menyimpan_plan_dan_menunggu_persetujuan() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Plan { skill: None }).await;
    plan_ready(&st).await;

    let run = st.runs.only();
    assert_eq!(run.plan.as_deref(), Some(PLAN));
    assert_eq!(run.phase, RunPhase::Plan);
    let notices = st.notifier.sent.lock().unwrap().clone();
    let last = notices.last().unwrap();
    assert_eq!(last.kind, NoticeKind::AwaitingAnswer);
    assert!(last.body.contains("Plan ready"), "{}", last.body);
}

#[tokio::test]
async fn revisi_lewat_chat_tetap_di_fase_plan_dan_mengganti_plan() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Plan { skill: None }).await;
    plan_ready(&st).await;

    let id = st.runs.only().id;
    st.svc
        .send_message(&id, "pisahkan migrasi ke langkah sendiri")
        .await
        .unwrap();
    let req = st.agent.last().req;
    assert!(req.plan_mode);
    assert!(matches!(req.session, SessionRef::Resume(_)));
    st.agent.last().finish("## Plan v2");
    eventually(|| st.runs.only().plan.as_deref() == Some("## Plan v2")).await;
}

#[tokio::test]
async fn push_ditolak_sebelum_plan_disetujui() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Plan { skill: None }).await;
    plan_ready(&st).await;
    st.git.set_changes(
        &std::path::PathBuf::from("/wt/mandor/pgs-7-tab-data"),
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["a.rs".to_owned()],
            diff: "+a".to_owned(),
        },
    );
    assert_eq!(
        st.svc.publish(&st.runs.only().id).await.unwrap_err(),
        RunError::InvalidState("plan not approved")
    );
}

#[tokio::test]
async fn setujui_plan_memulai_implementasi_di_sesi_yang_sama() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Plan { skill: None }).await;
    plan_ready(&st).await;

    let id = st.runs.only().id;
    st.svc.approve_plan(&id).await.unwrap();
    let req = st.agent.last().req;
    assert!(!req.plan_mode);
    assert!(matches!(req.session, SessionRef::Resume(_)));
    assert_eq!(req.message, "APPROVED");
    let run = st.runs.only();
    assert_eq!(run.phase, RunPhase::Implement);
    assert_eq!(run.state, RunState::Running);
    assert_eq!(run.plan.as_deref(), Some(PLAN), "plan tetap bisa dibaca");

    // Persetujuan kedua tidak berlaku.
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert_eq!(
        st.svc.approve_plan(&id).await.unwrap_err(),
        RunError::InvalidState("no plan to approve")
    );
}

#[tokio::test]
async fn persetujuan_ditolak_saat_plan_belum_selesai() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Plan { skill: None }).await;
    assert_eq!(
        st.svc.approve_plan(&st.runs.only().id).await.unwrap_err(),
        RunError::InvalidState("running")
    );
}

#[tokio::test]
async fn lewati_planning_langsung_implementasi() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Skip).await;
    let req = st.agent.last().req;
    assert!(!req.plan_mode);
    assert!(req.message.starts_with("Tab Data"));
    assert_eq!(st.runs.only().phase, RunPhase::Implement);
    assert_eq!(
        st.runs.dev_messages(),
        ["Started: implement the task.\n\nNote: pakai komponen lama"]
    );
}

fn brainstorm_first() -> Planning {
    Planning::Brainstorm {
        brainstorm: Some("compound-engineering:ce-brainstorm".to_owned()),
        plan: Some("compound-engineering:ce-plan".to_owned()),
    }
}

#[tokio::test]
async fn brainstorm_dulu_dengan_pertanyaan_lewat_chat_tanpa_plan() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, brainstorm_first()).await;
    let req = st.agent.last().req;
    assert!(req.plan_mode, "brainstorm juga hanya baca");
    assert!(
        req.message.starts_with("BRAINSTORM Tab Data"),
        "{}",
        req.message
    );
    assert!(
        req.message
            .contains("SKILL:compound-engineering:ce-brainstorm")
    );
    assert_eq!(st.runs.only().phase, RunPhase::Brainstorm);
    // Chat menampilkan ringkasan untuk dev, bukan prompt mentah agent.
    assert_eq!(
        st.runs.dev_messages(),
        [
            "Started: brainstorm with compound-engineering:ce-brainstorm, then plan with \
          compound-engineering:ce-plan.\n\nNote: pakai komponen lama"
        ]
    );

    st.agent.last().finish("Apakah data lama ikut dimigrasi?");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only();
    assert_eq!(run.plan, None, "pertanyaan brainstorm bukan plan");
    let notices = st.notifier.sent.lock().unwrap().clone();
    assert!(
        notices.last().unwrap().body.contains("question"),
        "{}",
        notices.last().unwrap().body
    );
    // Belum ada plan untuk disetujui.
    assert_eq!(
        st.svc.approve_plan(&run.id).await.unwrap_err(),
        RunError::InvalidState("no plan to approve")
    );

    // Jawaban dev tetap di fase brainstorm.
    st.svc.send_message(&run.id, "ya, ikut").await.unwrap();
    assert!(st.agent.last().req.plan_mode);
    st.agent.last().finish("Ringkasan requirements");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert_eq!(st.runs.only().phase, RunPhase::Brainstorm);
}

#[tokio::test]
async fn lanjut_ke_plan_memakai_skill_plan_pilihan_lalu_bisa_disetujui() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, brainstorm_first()).await;
    st.agent.last().finish("Ringkasan requirements");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    let id = st.runs.only().id;
    st.svc.continue_to_plan(&id).await.unwrap();
    let req = st.agent.last().req;
    assert!(req.plan_mode);
    assert!(matches!(req.session, SessionRef::Resume(_)));
    assert_eq!(req.message, "CONTINUE SKILL:compound-engineering:ce-plan");
    assert_eq!(st.runs.only().phase, RunPhase::Plan);

    plan_ready(&st).await;
    assert_eq!(st.runs.only().plan.as_deref(), Some(PLAN));
    st.svc.approve_plan(&id).await.unwrap();
    assert_eq!(st.runs.only().phase, RunPhase::Implement);
    // Lanjut ke plan hanya dari fase brainstorm.
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert_eq!(
        st.svc.continue_to_plan(&id).await.unwrap_err(),
        RunError::InvalidState("not brainstorming")
    );
}

#[tokio::test]
async fn perintah_garis_miring_menjalankan_skill_dan_chat_menyimpan_teks_asli() {
    let (lark, clock) = setup();
    let st = planned(&lark, &clock, Planning::Skip).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    let id = st.runs.only().id;
    st.svc
        .send_message(&id, "/compound-engineering:ce-plan fokus ke migrasi")
        .await
        .unwrap();
    assert_eq!(
        st.agent.last().req.message,
        "COMMAND compound-engineering:ce-plan ARGS fokus ke migrasi"
    );
    assert!(
        st.runs
            .dev_messages()
            .contains(&"/compound-engineering:ce-plan fokus ke migrasi".to_owned())
    );

    // Bukan perintah: garis miring di tengah, atau path.
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.svc
        .send_message(&id, "/home/u/a.rs perlu dicek")
        .await
        .unwrap();
    assert_eq!(st.agent.last().req.message, "/home/u/a.rs perlu dicek");
}
