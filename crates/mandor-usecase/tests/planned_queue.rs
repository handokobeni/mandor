//! Kartu Planned di antrean VM (U4 plan 2026-10-30-001): serah-terima
//! tertunda, heartbeat klaim, mulai dari plan yang disetujui, dan kembali ke
//! laptop.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::{FakeClock, FakeLark, Station, ready_task, vm_station};
use mandor_domain::claim::STALE_MS;
use mandor_domain::planned::{CLAIM_ON_LAPTOP, DesignMark, PlannedState, PlannedWork};
use mandor_domain::{Claim, Design, RunState, TaskStatus};
use mandor_usecase::overnight::{OvernightSettings, QueueRequest};
use mandor_usecase::run_ports::{RunPhase, RunRecord, SessionRef};
use mandor_usecase::{AcItem, StartTarget};
use pretty_assertions::assert_eq;

const MIN: i64 = 60_000;
/// 2026-10-23T00:00:00Z.
const DAY0: i64 = 1_792_713_600_000;
const BRANCH: &str = "mandor/pgs-7-tab-data";

/// ms epoch hari `day` pukul `h:m` WIB (+07:00).
fn wib(day: i64, h: i64, m: i64) -> i64 {
    DAY0 + day * 24 * 60 * MIN + ((h * 60 + m) - 7 * 60) * MIN
}

fn at(lark: &FakeLark, clock: &FakeClock, ms: i64) {
    clock.set(ms);
    lark.clock.store(ms, Ordering::SeqCst);
}

fn claim_of(daemon: &str, ts: i64) -> Claim {
    Claim {
        open_id: "ou_me".to_owned(),
        ts_ms: ts,
        nonce: format!("r~{daemon}"),
    }
}

/// Daemon VM dan kartu `rec1` In Progress yang klaimnya masih di laptop.
async fn vm(now: i64) -> (FakeLark, FakeClock, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    at(&lark, &clock, now);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Tab Data".clone_into(&mut t.title);
    t.status = TaskStatus::InProgress;
    t.branch = Some(BRANCH.to_owned());
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned()];
    t.claim = Some(claim_of("local", now));
    t.last_modified_ms = now;
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
    let st = vm_station(&lark, &clock, "ou_me");
    st.svc
        .set_overnight_settings(&OvernightSettings {
            start: "19:00".to_owned(),
            end: "06:00".to_owned(),
            offset: "+07:00".to_owned(),
            token_limit: Some(1_000_000),
        })
        .await
        .unwrap();
    st.svc.sync().poll(true, &[]).await.unwrap();
    (lark, clock, st)
}

fn work() -> PlannedWork {
    PlannedWork {
        plan: "## Plan\n1. Tambah tab".to_owned(),
        branch: BRANCH.to_owned(),
        design: vec![DesignMark {
            ac_id: "ac2".to_owned(),
            design: Design::Conflicts,
            note: Some("Header beda".to_owned()),
        }],
        laptop_design: vec![],
        repos: vec!["repo1".to_owned()],
    }
}

fn repo1() -> StartTarget {
    StartTarget::Repo("repo1".to_owned())
}

async fn queue_planned(st: &Station, work: PlannedWork) {
    st.svc
        .enqueue_card(
            "rec1",
            &QueueRequest {
                target: repo1(),
                note: None,
                design_sources: vec![],
                planned: Some((work, "local".to_owned())),
            },
        )
        .await
        .unwrap();
}

/// Langkah (3) selesai: klaim pindah ke VM.
async fn claim_to_vm(lark: &FakeLark, st: &Station, ts: i64) {
    lark.edit("rec1", |t| t.claim = Some(claim_of("vm", ts)));
    st.svc.sync().poll(true, &[]).await.unwrap();
}

fn state(st: &Station) -> Option<PlannedState> {
    st.overnight
        .queue
        .lock()
        .unwrap()
        .first()
        .and_then(|i| i.planned.as_ref().map(|p| p.state))
}

fn reason(st: &Station) -> Option<String> {
    st.overnight
        .queue
        .lock()
        .unwrap()
        .first()
        .and_then(|i| i.planned.as_ref().and_then(|p| p.reason.clone()))
}

#[tokio::test]
async fn covers_ae1_kartu_planned_dimulai_dari_branch_dengan_plan_yang_disetujui() {
    let (lark, clock, st) = vm(wib(0, 15, 0)).await;
    st.design.add("ds1", "kelola-mock");
    let mut w = work();
    w.design[0].design = Design::FollowDesign;
    st.svc
        .enqueue_card(
            "rec1",
            &QueueRequest {
                target: repo1(),
                note: None,
                design_sources: vec!["ds1".to_owned()],
                planned: Some((w, "local".to_owned())),
            },
        )
        .await
        .unwrap();
    assert_eq!(state(&st), Some(PlannedState::HandoverPending));
    claim_to_vm(&lark, &st, wib(0, 15, 1)).await;
    st.svc.heartbeat_all().await.unwrap();
    assert_eq!(state(&st), Some(PlannedState::Waiting));

    at(&lark, &clock, wib(0, 19, 0));
    let tick = st.svc.overnight_tick().await.unwrap();
    assert_eq!(tick.started.as_deref(), Some("rec1"));
    assert!(st.overnight.ids().is_empty(), "item keluar dari antrean");
    assert_eq!(st.git.st().restored, [BRANCH]);
    assert!(st.git.st().prepared.is_empty(), "tidak membuat branch baru");
    let run = st.runs.only();
    assert_eq!(run.phase, RunPhase::Implement);
    assert_eq!(run.plan.as_deref(), Some("## Plan\n1. Tambah tab"));
    let req = st.agent.last().req;
    assert!(!req.plan_mode);
    assert!(matches!(req.session, SessionRef::New(_)));
    assert!(
        req.message.starts_with("IMPLEMENT PLAN Tab Data"),
        "{}",
        req.message
    );
    assert!(
        req.message.contains("FOLLOW:AC-ac2: teks ac2"),
        "{}",
        req.message
    );
    let rows = st.svc.ledger(&run.id).await.unwrap();
    assert_eq!(rows[1].design, Design::FollowDesign);
    assert_eq!(rows[1].design_note.as_deref(), Some("Header beda"));
}

#[tokio::test]
async fn serah_terima_tertunda_tidak_dikeluarkan_tick_lalu_ditahan_setelah_basi() {
    let (lark, clock, st) = vm(wib(0, 19, 30)).await;
    queue_planned(&st, work()).await;
    let tick = st.svc.overnight_tick().await.unwrap();
    assert_eq!(tick.started, None);
    assert!(tick.dequeued.is_empty(), "{tick:?}");
    st.svc.heartbeat_all().await.unwrap();
    assert_eq!(state(&st), Some(PlannedState::HandoverPending));
    assert_eq!(
        lark.get("rec1").unwrap().claim.unwrap().daemon_id(),
        Some("local"),
        "klaim laptop tidak disentuh"
    );

    at(&lark, &clock, wib(0, 19, 30) + STALE_MS + MIN);
    st.svc.heartbeat_all().await.unwrap();
    assert_eq!(state(&st), Some(PlannedState::Held));
    assert_eq!(reason(&st).as_deref(), Some(CLAIM_ON_LAPTOP));
    let report = st.svc.overnight_report().await.unwrap();
    assert_eq!(report.planned_held.len(), 1);
    assert_eq!(report.planned_held[0].reason, CLAIM_ON_LAPTOP);
}

#[tokio::test]
async fn remove_hanya_untuk_serah_terima_yang_belum_selesai() {
    let (lark, _clock, st) = vm(wib(0, 15, 0)).await;
    queue_planned(&st, work()).await;
    st.svc.queue_remove("rec1").await.unwrap();
    assert!(st.overnight.ids().is_empty());

    queue_planned(&st, work()).await;
    claim_to_vm(&lark, &st, wib(0, 15, 1)).await;
    st.svc.heartbeat_all().await.unwrap();
    let err = st.svc.queue_remove("rec1").await.unwrap_err();
    assert!(format!("{err}").contains("Take over"), "{err}");
    assert_eq!(st.overnight.ids(), ["rec1"]);
}

#[tokio::test]
async fn covers_ae4_klaim_item_waiting_diperbarui_sepanjang_hari() {
    let (lark, clock, st) = vm(wib(0, 12, 0)).await;
    queue_planned(&st, work()).await;
    claim_to_vm(&lark, &st, wib(0, 12, 0)).await;
    for step in 1..=72 {
        at(&lark, &clock, wib(0, 12, 0) + step * 5 * MIN);
        st.svc.heartbeat_all().await.unwrap();
    }
    let claim = lark.get("rec1").unwrap().claim.unwrap();
    assert_eq!(claim.daemon_id(), Some("vm"));
    assert!(!claim.is_stale(wib(0, 18, 0)), "klaim tetap segar");
    assert_eq!(state(&st), Some(PlannedState::Waiting));
}

#[tokio::test]
async fn klaim_yang_diambil_daemon_lain_tidak_ditimpa_dan_item_keluar() {
    let (lark, clock, st) = vm(wib(0, 12, 0)).await;
    queue_planned(&st, work()).await;
    claim_to_vm(&lark, &st, wib(0, 12, 0)).await;
    st.svc.heartbeat_all().await.unwrap();
    at(&lark, &clock, wib(0, 13, 0));
    lark.edit("rec1", |t| {
        t.claim = Some(claim_of("laptop2", wib(0, 13, 0)));
    });
    st.svc.heartbeat_all().await.unwrap();
    assert!(st.overnight.ids().is_empty());
    assert_eq!(
        lark.get("rec1").unwrap().claim.unwrap().daemon_id(),
        Some("laptop2")
    );
    let out = st.svc.overnight_report().await.unwrap();
    assert_eq!(out.dequeued.len(), 1);
}

#[tokio::test]
async fn covers_ae7_syarat_mulai_gagal_menahan_kartu_dengan_plan() {
    let (lark, clock, st) = vm(wib(0, 15, 0)).await;
    queue_planned(&st, work()).await;
    claim_to_vm(&lark, &st, wib(0, 15, 0)).await;
    st.svc.heartbeat_all().await.unwrap();
    // Start gagal: target tidak lagi terdaftar di VM.
    st.repos.remove("repo1");
    at(&lark, &clock, wib(0, 19, 0));
    let tick = st.svc.overnight_tick().await.unwrap();
    assert_eq!(tick.started, None);
    assert_eq!(state(&st), Some(PlannedState::Held));
    assert!(reason(&st).is_some());
    assert!(st.runs.all().iter().all(|r| !r.state.is_working()));
    assert_eq!(
        lark.get("rec1").unwrap().claim.unwrap().daemon_id(),
        Some("vm"),
        "klaim tetap di VM"
    );
}

#[tokio::test]
async fn covers_ae3_take_over_item_planned_tanpa_run_mengembalikan_paket() {
    let (lark, _clock, st) = vm(wib(0, 15, 0)).await;
    queue_planned(&st, work()).await;
    claim_to_vm(&lark, &st, wib(0, 15, 0)).await;
    st.svc.heartbeat_all().await.unwrap();

    let back = st.svc.handoff("rec1", "local").await.unwrap();
    assert_eq!(back.branch, BRANCH);
    assert_eq!(back.target, Some(repo1()));
    assert_eq!(back.planned, Some(work()));
    assert!(st.overnight.ids().is_empty());
    assert_eq!(
        lark.get("rec1").unwrap().claim.unwrap().daemon_id(),
        Some("local")
    );
    assert!(st.git.st().pushes.is_empty(), "tidak ada kerja malam");
    // Boleh diantre lagi (tidak "hanya laptop").
    claim_to_vm(&lark, &st, wib(0, 15, 5)).await;
    queue_planned(&st, work()).await;
}

#[tokio::test]
async fn take_over_item_serah_terima_tertunda_ditolak() {
    let (_lark, _clock, st) = vm(wib(0, 15, 0)).await;
    queue_planned(&st, work()).await;
    assert!(st.svc.handoff("rec1", "local").await.is_err());
    assert_eq!(st.overnight.ids(), ["rec1"]);
}

#[tokio::test]
async fn antrean_planned_divalidasi() {
    let (lark, _clock, st) = vm(wib(0, 15, 0)).await;
    let mut other = work();
    other.branch = "mandor/lain".to_owned();
    let mut follow = work();
    follow.design[0].design = Design::FollowDesign;
    let mut workspace = work();
    workspace.repos = vec!["repo1".to_owned(), "repo2".to_owned()];
    let cases = [
        ("workspace ke satu repo", repo1(), workspace, "same repos"),
        ("branch", repo1(), other, "branch"),
        ("follow", repo1(), follow, "design source"),
        (
            "target",
            StartTarget::Repo("tidak-ada".to_owned()),
            work(),
            "repo",
        ),
    ];
    for (name, target, w, want) in cases {
        let err = st
            .svc
            .enqueue_card(
                "rec1",
                &QueueRequest {
                    target,
                    note: None,
                    design_sources: vec![],
                    planned: Some((w, "local".to_owned())),
                },
            )
            .await
            .unwrap_err();
        assert!(format!("{err}").contains(want), "{name}: {err}");
    }
    lark.edit("rec1", |t| t.status = TaskStatus::ToDo);
    st.svc.sync().poll(true, &[]).await.unwrap();
    assert!(
        st.svc
            .enqueue_card(
                "rec1",
                &QueueRequest {
                    target: repo1(),
                    note: None,
                    design_sources: vec![],
                    planned: Some((work(), "local".to_owned())),
                }
            )
            .await
            .is_err(),
        "kartu Planned harus In Progress"
    );
    assert!(st.overnight.ids().is_empty());
}

#[tokio::test]
async fn restart_sebelum_pesan_pertama_mengirim_ulang_plan() {
    let (lark, clock, st) = vm(wib(0, 19, 0)).await;
    claim_to_vm(&lark, &st, wib(0, 19, 0)).await;
    let mut r = RunRecord::new("run-1".to_owned(), "repo1".to_owned(), "rec1".to_owned());
    r.state = RunState::Failed;
    r.phase = RunPhase::Implement;
    r.plan = Some("## Plan lama".to_owned());
    r.branch = Some(BRANCH.to_owned());
    r.worktree = Some(format!("/wt/{BRANCH}").into());
    r.session_id = Some("s1".to_owned());
    st.runs.insert(r);
    st.svc.queue_add("rec1", &repo1(), None).await.unwrap();
    at(&lark, &clock, wib(0, 19, 1));
    st.svc.overnight_tick().await.unwrap();
    let req = st.agent.last().req;
    assert!(req.message.starts_with("IMPLEMENT PLAN"), "{}", req.message);
    assert!(req.message.contains("## Plan lama"));
    assert!(matches!(req.session, SessionRef::New(_)));
}

/// Kartu To Do `nd` (No plan) di VM yang sama.
fn no_plan_card(lark: &FakeLark, now: i64) {
    let mut t = ready_task("nd", &["ou_me"]);
    t.last_modified_ms = now;
    lark.put(t);
}

async fn queue_no_plan(st: &Station, design: &[&str]) -> Result<(), mandor_usecase::RunError> {
    st.svc
        .enqueue_card(
            "nd",
            &QueueRequest {
                target: repo1(),
                note: None,
                design_sources: design.iter().map(|d| (*d).to_owned()).collect(),
                planned: None,
            },
        )
        .await
}

#[tokio::test]
async fn covers_ae5_desain_repo_diperbarui_dan_dikirim_ke_agent() {
    let (lark, clock, st) = vm(wib(0, 18, 0)).await;
    no_plan_card(&lark, wib(0, 18, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    st.design.add("ds1", "kelola-mock");
    queue_no_plan(&st, &["ds1"]).await.unwrap();
    assert!(
        st.design.refreshed.lock().unwrap().is_empty(),
        "belum diperbarui saat antre"
    );

    at(&lark, &clock, wib(0, 19, 0));
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("nd")
    );
    assert_eq!(*st.design.refreshed.lock().unwrap(), ["ds1"]);
    let req = st.agent.last().req;
    assert!(
        req.message.contains("DESIGN REF kelola-mock"),
        "{}",
        req.message
    );
    assert_eq!(req.design.len(), 1);
}

#[tokio::test]
async fn covers_ae6_desain_gagal_diperbarui_tetap_dimulai_dan_dicatat() {
    let (lark, clock, st) = vm(wib(0, 18, 0)).await;
    no_plan_card(&lark, wib(0, 18, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    st.design.add("ds1", "kelola-mock");
    st.design
        .stale
        .lock()
        .unwrap()
        .insert("ds1".to_owned(), "fetch failed".to_owned());
    queue_no_plan(&st, &["ds1"]).await.unwrap();
    at(&lark, &clock, wib(0, 19, 0));
    assert!(st.svc.overnight_tick().await.unwrap().started.is_some());
    let report = st.svc.overnight_report().await.unwrap();
    assert_eq!(
        report.cards[0].design_notes,
        ["Design kelola-mock not updated: fetch failed"]
    );
}

#[tokio::test]
async fn covers_ae7_desain_hilang_mengeluarkan_no_plan_dan_menahan_planned() {
    let (lark, clock, st) = vm(wib(0, 15, 0)).await;
    no_plan_card(&lark, wib(0, 15, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    st.design.add("ds1", "kelola-mock");
    queue_no_plan(&st, &["ds1"]).await.unwrap();
    st.svc
        .enqueue_card(
            "rec1",
            &QueueRequest {
                target: repo1(),
                note: None,
                design_sources: vec!["ds1".to_owned()],
                planned: Some((work(), "local".to_owned())),
            },
        )
        .await
        .unwrap();
    claim_to_vm(&lark, &st, wib(0, 15, 0)).await;
    st.svc.heartbeat_all().await.unwrap();
    st.design.missing.lock().unwrap().insert("ds1".to_owned());

    at(&lark, &clock, wib(0, 19, 0));
    let tick = st.svc.overnight_tick().await.unwrap();
    assert_eq!(tick.started, None);
    let want = "Design source \"kelola-mock\" is not available: Path missing";
    assert_eq!(tick.dequeued.len(), 2, "{tick:?}");
    assert!(tick.dequeued.iter().all(|(_, r)| r == want), "{tick:?}");
    assert_eq!(st.overnight.ids(), ["rec1"], "Planned tetap di antrean");
    assert_eq!(state(&st), Some(PlannedState::Held));
    assert_eq!(reason(&st).as_deref(), Some(want));
}

#[tokio::test]
async fn sumber_desain_tak_dikenal_ditolak_saat_antre() {
    let (lark, _clock, st) = vm(wib(0, 15, 0)).await;
    no_plan_card(&lark, wib(0, 15, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    let err = queue_no_plan(&st, &["hilang"]).await.unwrap_err();
    assert!(
        format!("{err}").contains("Design source \"hilang\""),
        "{err}"
    );
    assert!(st.overnight.ids().is_empty());
}
