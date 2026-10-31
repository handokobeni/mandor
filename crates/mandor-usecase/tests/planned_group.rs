//! Grup Planned: plan kartu pertama disetujui di laptop lalu grup diantre ke
//! VM (plan 2026-10-23-004, U13, R24, AE9).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station, vm_station};
use mandor_domain::group::CardState;
use mandor_domain::planned::{DesignMark, PlannedState};
use mandor_domain::{Claim, Design, RunState, TaskStatus};
use mandor_usecase::overnight::{OvernightSettings, QueueRequest};
use mandor_usecase::run_ports::{RunPhase, RunRecord, WorkChanges};
use mandor_usecase::{AcItem, Planning, StartTarget, VmHandover};
use pretty_assertions::assert_eq;

const MIN: i64 = 60_000;
/// 2026-10-23T00:00:00Z.
const DAY0: i64 = 1_792_713_600_000;
const PLAN: &str = "## Plan PGS-1401\n1. Tabel daftar job";

fn wib(day: i64, h: i64, m: i64) -> i64 {
    DAY0 + day * 24 * 60 * MIN + ((h * 60 + m) - 7 * 60) * MIN
}

fn at(lark: &FakeLark, clock: &FakeClock, ms: i64) {
    clock.set(ms);
    lark.clock.store(ms, Ordering::SeqCst);
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn repo1() -> StartTarget {
    StartTarget::Repo("repo1".to_owned())
}

/// Laptop dan VM milik `ou_me` dengan kartu c1, c2 (masing-masing satu AC).
async fn pair(now: i64) -> (FakeLark, FakeClock, Station, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    at(&lark, &clock, now);
    for (i, id) in ["c1", "c2"].iter().enumerate() {
        let mut t = ready_task(id, &["ou_me"]);
        t.key = Some(format!("PGS-140{}", i + 1));
        t.ac_ids = vec![format!("ac{}", i + 1)];
        t.last_modified_ms = now;
        lark.put(t);
        lark.acs.lock().unwrap().insert(
            format!("ac{}", i + 1),
            AcItem {
                record_id: format!("ac{}", i + 1),
                label: Some(format!("AC-{}", i + 1)),
                text: format!("AC kartu {}", i + 1),
                ..Default::default()
            },
        );
    }
    let laptop = station(&lark, &clock, "ou_me");
    let vm = vm_station(&lark, &clock, "ou_me");
    vm.svc
        .set_overnight_settings(&OvernightSettings {
            start: "19:00".to_owned(),
            end: "06:00".to_owned(),
            offset: "+07:00".to_owned(),
            token_limit: Some(1_000_000),
        })
        .await
        .unwrap();
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    (lark, clock, laptop, vm)
}

/// Grup laptop dengan plan kartu pertama menunggu persetujuan.
async fn plan_ready(laptop: &Station) -> RunRecord {
    let run = laptop
        .svc
        .start_group(
            &ids(&["c1", "c2"]),
            &repo1(),
            None,
            &Planning::Plan { skill: None },
            &[],
            |_| {},
        )
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    laptop.agent.last().finish(PLAN);
    eventually(|| laptop.runs.all()[0].state == RunState::AwaitingDev).await;
    laptop
        .runs
        .all()
        .into_iter()
        .find(|r| r.id == run.id)
        .unwrap()
}

fn vm_daemon(vm: &Station) -> String {
    vm.svc.sync().identity().daemon_id.clone()
}

fn laptop_daemon(laptop: &Station) -> String {
    laptop.svc.sync().identity().daemon_id.clone()
}

async fn enqueue(vm: &Station, laptop: &Station, back: &VmHandover, design: &[&str]) {
    vm.svc.sync().poll(true, &[]).await.unwrap();
    vm.svc
        .enqueue_group(
            &back.cards,
            &QueueRequest {
                target: back.target.clone(),
                note: None,
                design_sources: ids(design),
                planned: Some((back.work.clone(), laptop_daemon(laptop))),
            },
        )
        .await
        .unwrap();
}

fn queue_state(vm: &Station) -> Option<PlannedState> {
    vm.overnight.queue.lock().unwrap()[0]
        .planned
        .as_ref()
        .map(|p| p.state)
}

fn claim_daemon(lark: &FakeLark, id: &str) -> Option<String> {
    lark.get(id)
        .unwrap()
        .claim
        .and_then(|c| c.daemon_id().map(str::to_owned))
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae9_grup_dengan_plan_kartu_pertama_dikerjakan_vm_malam_hari() {
    let (lark, clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    let run = plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    assert_eq!(back.cards, ids(&["c1", "c2"]));
    assert_eq!(back.work.plan, PLAN);
    assert_eq!(laptop.git.st().pushes.len(), 1, "branch grup di-push");

    enqueue(&vm, &laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(&vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    for id in ["c1", "c2"] {
        assert_eq!(claim_daemon(&lark, id), Some(vm_daemon(&vm)));
    }
    assert!(
        laptop.runs.rows.lock().unwrap()[&run.id].1,
        "run laptop ditutup"
    );

    vm.svc.sync().poll(true, &[]).await.unwrap();
    assert_eq!(
        vm.svc.overnight_tick().await.unwrap().started,
        None,
        "siang hari"
    );
    assert_eq!(queue_state(&vm), Some(PlannedState::Waiting));

    at(&lark, &clock, wib(0, 20, 0));
    assert_eq!(
        vm.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("c1")
    );
    vm.svc.sync().drain_outbox(50).await.unwrap();
    let vm_run = vm.runs.only();
    assert_eq!(vm_run.phase, RunPhase::Implement, "tanpa giliran plan");
    assert!(vm.agent.last().req.message.contains(PLAN));
    let cards: Vec<CardState> = vm.runs.cards.lock().unwrap()[&vm_run.id]
        .iter()
        .map(|c| c.state.clone())
        .collect();
    assert_eq!(cards, [CardState::Active, CardState::Pending]);
    let rows = vm.reviews.ledgers.lock().unwrap()[&vm_run.id].clone();
    let cards_of_rows: Vec<Option<&str>> = rows.iter().map(|r| r.card.as_deref()).collect();
    assert_eq!(
        cards_of_rows,
        [Some("c1"), Some("c2")],
        "buku AC semua kartu"
    );

    let wt = vm_run.worktree.unwrap();
    vm.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/job.rs".to_owned()],
            diff: "+job".to_owned(),
        },
    );
    vm.agent.last().done();
    eventually(|| vm.agent.count() == 2).await;
    assert!(
        !vm.agent.last().req.message.contains(PLAN),
        "kartu kedua tanpa plan dari laptop"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn grup_yang_sudah_lewat_kartu_pertama_tidak_bisa_diantre_ke_vm() {
    let (_lark, _clock, laptop, _vm) = pair(wib(0, 14, 0)).await;
    let run = plan_ready(&laptop).await;
    laptop.svc.approve_plan(&run.id, &[]).await.unwrap();
    let wt = laptop.runs.all()[0].worktree.clone().unwrap();
    laptop.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/job.rs".to_owned()],
            diff: "+job".to_owned(),
        },
    );
    laptop.agent.last().done();
    eventually(|| laptop.runs.all()[0].state == RunState::AwaitingDev).await;
    laptop.svc.next_card(&run.id).await.unwrap();
    laptop.agent.last().finish("## Plan PGS-1402");
    eventually(|| laptop.runs.all()[0].state == RunState::AwaitingDev).await;
    let err = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap_err();
    assert!(format!("{err}").contains("first card"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn follow_design_tanpa_sumber_desain_vm_ditolak() {
    let (_lark, _clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    let run = plan_ready(&laptop).await;
    let mut back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    back.work.design = vec![DesignMark {
        ac_id: "ac1".to_owned(),
        design: Design::FollowDesign,
        note: None,
    }];
    let err = vm
        .svc
        .enqueue_group(
            &back.cards,
            &QueueRequest {
                target: back.target.clone(),
                note: None,
                design_sources: Vec::new(),
                planned: Some((back.work.clone(), laptop_daemon(&laptop))),
            },
        )
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("design source"), "{err}");
    assert!(vm.overnight.ids().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn serah_terima_belum_lengkap_tidak_dimulai_lalu_pengulangan_menyelesaikan() {
    let (lark, clock, laptop, vm) = pair(wib(0, 20, 0)).await;
    let run = plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    enqueue(&vm, &laptop, &back, &[]).await;
    // Hanya klaim kartu kedua yang sempat pindah.
    let vm_id = vm_daemon(&vm);
    lark.edit("c2", |t| {
        t.claim = Some(Claim {
            open_id: "ou_me".to_owned(),
            ts_ms: wib(0, 20, 0),
            nonce: format!("x~{vm_id}"),
        });
    });
    vm.svc.sync().poll(true, &[]).await.unwrap();
    assert_eq!(vm.svc.overnight_tick().await.unwrap().started, None);
    assert_eq!(queue_state(&vm), Some(PlannedState::HandoverPending));

    laptop.svc.send_to_vm(&run.id, &vm_id).await.unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    at(&lark, &clock, wib(0, 20, 5));
    assert_eq!(
        vm.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("c1")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn klaim_semua_kartu_grup_planned_tetap_segar_enam_jam() {
    let (lark, clock, laptop, vm) = pair(wib(0, 10, 0)).await;
    let run = plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    enqueue(&vm, &laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(&vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    for step in 1..=24 {
        at(&lark, &clock, wib(0, 10, 0) + step * 15 * MIN);
        vm.svc.sync().poll(true, &[]).await.unwrap();
        vm.svc.heartbeat_all().await.unwrap();
        vm.svc.sync().drain_outbox(50).await.unwrap();
    }
    for id in ["c1", "c2"] {
        let claim = lark.get(id).unwrap().claim.unwrap();
        assert_eq!(claim.daemon_id(), Some(vm_daemon(&vm).as_str()));
        assert!(claim.ts_ms >= wib(0, 16, 0) - 30 * MIN, "{id} segar");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn klaim_kartu_kedua_diambil_daemon_lain_menahan_item() {
    let (lark, clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    let run = plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    enqueue(&vm, &laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(&vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    vm.svc.overnight_tick().await.unwrap();
    lark.edit("c2", |t| {
        t.claim = Some(Claim {
            open_id: "ou_me".to_owned(),
            ts_ms: wib(0, 14, 0),
            nonce: "x~lain".to_owned(),
        });
    });
    vm.svc.sync().poll(true, &[]).await.unwrap();
    at(&lark, &clock, wib(0, 20, 0));
    assert_eq!(vm.svc.overnight_tick().await.unwrap().started, None);
    assert_eq!(queue_state(&vm), Some(PlannedState::Held));
    assert_eq!(lark.get("c2").unwrap().status, TaskStatus::InProgress);
}

#[tokio::test(flavor = "multi_thread")]
async fn take_over_grup_planned_di_antrean_kembali_menunggu_persetujuan_plan() {
    let (lark, _clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    let run = plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    enqueue(&vm, &laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(&vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    vm.svc.overnight_tick().await.unwrap();
    assert_eq!(queue_state(&vm), Some(PlannedState::Waiting));

    let handback = vm.svc.handoff("c1", &laptop_daemon(&laptop)).await.unwrap();
    assert!(vm.overnight.ids().is_empty());
    vm.svc.sync().drain_outbox(50).await.unwrap();
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    let back_run = laptop
        .svc
        .takeover(
            "c1",
            &handback.target.clone().unwrap(),
            handback.planned.as_ref(),
            &handback.carry,
        )
        .await
        .unwrap();
    assert_eq!(back_run.phase, RunPhase::Plan);
    assert_eq!(back_run.plan.as_deref(), Some(PLAN));
    let cards: Vec<CardState> = laptop.runs.cards.lock().unwrap()[&back_run.id]
        .iter()
        .map(|c| c.state.clone())
        .collect();
    assert_eq!(cards, [CardState::Active, CardState::Pending]);
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    for id in ["c1", "c2"] {
        assert_eq!(claim_daemon(&lark, id), Some(laptop_daemon(&laptop)));
    }
}

// ---- Satu plan untuk grup di VM (R24, R25, U19) ----

/// Grup laptop satu plan dengan plan grup menunggu persetujuan.
async fn one_plan_ready(laptop: &Station) -> RunRecord {
    let run = laptop
        .svc
        .start_one_plan_group(&ids(&["c1", "c2"]), &repo1(), None, None, &[], |_| {})
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    laptop.agent.last().finish(PLAN);
    eventually(|| laptop.runs.all()[0].state == RunState::AwaitingDev).await;
    laptop
        .runs
        .all()
        .into_iter()
        .find(|r| r.id == run.id)
        .unwrap()
}

/// Grup satu plan dikirim ke VM dan kartu pertama mulai malam hari.
async fn one_plan_on_vm(lark: &FakeLark, clock: &FakeClock, laptop: &Station, vm: &Station) {
    let run = one_plan_ready(laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    enqueue(vm, laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    at(lark, clock, wib(0, 20, 0));
    vm.svc.overnight_tick().await.unwrap();
    vm.svc.sync().drain_outbox(50).await.unwrap();
}

fn vm_dirty(vm: &Station) {
    let wt = vm.runs.only().worktree.unwrap();
    vm.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/job.rs".to_owned()],
            diff: "+job".to_owned(),
        },
    );
}

fn vm_cards(vm: &Station) -> Vec<CardState> {
    let id = vm.runs.only().id;
    vm.runs.cards.lock().unwrap()[&id]
        .iter()
        .map(|c| c.state.clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn f6_grup_satu_plan_dikerjakan_vm_tanpa_giliran_plan() {
    let (lark, clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    let run = one_plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    let snaps = back.work.group_acs.clone().expect("cuplikan plan grup");
    let cards: Vec<&str> = snaps.iter().map(|s| s.card.as_str()).collect();
    assert_eq!(cards, ["c1", "c2"]);

    enqueue(&vm, &laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(&vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    at(&lark, &clock, wib(0, 20, 0));
    vm.svc.overnight_tick().await.unwrap();
    vm.svc.sync().drain_outbox(50).await.unwrap();
    let vm_run = vm.runs.only();
    assert!(vm_run.one_plan.is_some());
    let first = vm.agent.last().req.message;
    assert!(first.contains(PLAN) && first.contains("section for PGS-1401"));

    vm_dirty(&vm);
    vm.agent.last().done();
    eventually(|| vm.agent.count() == 2).await;
    let req = vm.agent.last().req;
    assert!(!req.plan_mode);
    assert!(req.message.contains("section for PGS-1402"));
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae11_vm_ac_kartu_berikutnya_berubah_grup_diparkir_plan_outdated() {
    let (lark, clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    one_plan_on_vm(&lark, &clock, &laptop, &vm).await;
    lark.acs.lock().unwrap().insert(
        "ac2".to_owned(),
        AcItem {
            record_id: "ac2".to_owned(),
            label: Some("AC-2".to_owned()),
            text: "AC kartu 2 diubah".to_owned(),
            ..Default::default()
        },
    );

    vm_dirty(&vm);
    vm.agent.last().done();
    eventually(|| vm.runs.only().state == RunState::Held).await;
    let r = vm.runs.only();
    assert_eq!(
        r.hold.map(|h| h.reason),
        Some(mandor_domain::HoldReason::PlanOutdated)
    );
    assert_eq!(vm_cards(&vm), [CardState::Done, CardState::Pending]);
    assert_eq!(vm.agent.count(), 1, "kartu kedua tidak dimulai");
    assert!(
        vm.overnight.ids().is_empty(),
        "tidak diantre ulang otomatis"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn grup_planned_satu_plan_dengan_ac_berubah_sebelum_mulai_ditahan() {
    let (lark, clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    let run = one_plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    enqueue(&vm, &laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(&vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    lark.acs.lock().unwrap().insert(
        "ac2".to_owned(),
        AcItem {
            record_id: "ac2".to_owned(),
            label: Some("AC-2".to_owned()),
            text: "AC kartu 2 diubah sore".to_owned(),
            ..Default::default()
        },
    );
    vm.svc.sync().poll(true, &[]).await.unwrap();
    at(&lark, &clock, wib(0, 20, 0));
    vm.svc.overnight_tick().await.unwrap();
    let r = vm.runs.only();
    assert_eq!(r.state, RunState::Held);
    assert_eq!(
        r.hold.map(|h| h.reason),
        Some(mandor_domain::HoldReason::PlanOutdated)
    );
    assert_eq!(vm.agent.count(), 0, "tidak ada giliran agent");
}

#[tokio::test(flavor = "multi_thread")]
async fn take_over_grup_plan_outdated_laptop_merevisi_plan() {
    let (lark, clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    one_plan_on_vm(&lark, &clock, &laptop, &vm).await;
    lark.acs.lock().unwrap().insert(
        "ac2".to_owned(),
        AcItem {
            record_id: "ac2".to_owned(),
            label: Some("AC-2".to_owned()),
            text: "AC kartu 2 diubah".to_owned(),
            ..Default::default()
        },
    );
    vm_dirty(&vm);
    vm.agent.last().done();
    eventually(|| vm.runs.only().state == RunState::Held).await;

    let handback = vm.svc.handoff("c1", &laptop_daemon(&laptop)).await.unwrap();
    assert!(handback.carry.one_plan.is_some(), "plan grup ikut");
    vm.svc.sync().drain_outbox(50).await.unwrap();
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    let back_run = laptop
        .svc
        .takeover(
            "c1",
            &handback.target.clone().unwrap(),
            handback.planned.as_ref(),
            &handback.carry,
        )
        .await
        .unwrap();
    assert!(back_run.one_plan.is_some());
    laptop.svc.continue_group(&back_run.id).await.unwrap();
    let r = laptop
        .runs
        .all()
        .into_iter()
        .find(|r| r.id == back_run.id)
        .unwrap();
    assert_eq!(r.phase, RunPhase::Plan, "plan grup direvisi di laptop");
    assert!(
        laptop
            .agent
            .last()
            .req
            .message
            .contains("AC kartu 2 diubah")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn take_over_grup_planned_satu_plan_di_antrean_tetap_satu_plan() {
    let (_lark, _clock, laptop, vm) = pair(wib(0, 14, 0)).await;
    let run = one_plan_ready(&laptop).await;
    let back = laptop.svc.prepare_for_vm(&run.id, &[]).await.unwrap();
    enqueue(&vm, &laptop, &back, &[]).await;
    laptop
        .svc
        .send_to_vm(&run.id, &vm_daemon(&vm))
        .await
        .unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    vm.svc.sync().poll(true, &[]).await.unwrap();
    vm.svc.overnight_tick().await.unwrap();

    let handback = vm.svc.handoff("c1", &laptop_daemon(&laptop)).await.unwrap();
    vm.svc.sync().drain_outbox(50).await.unwrap();
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    let back_run = laptop
        .svc
        .takeover(
            "c1",
            &handback.target.clone().unwrap(),
            handback.planned.as_ref(),
            &handback.carry,
        )
        .await
        .unwrap();
    assert_eq!(back_run.phase, RunPhase::Plan);
    assert!(
        back_run.one_plan.is_some(),
        "kembali sebagai grup satu plan"
    );
}
