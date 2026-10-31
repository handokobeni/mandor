//! Grup kartu sebagai satu item antrean VM lembur (plan 2026-10-23-004, U12).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, vm_station};
use mandor_domain::group::{CardState, RemovedReason};
use mandor_domain::{HoldReason, RunState, TaskStatus};
use mandor_usecase::StartTarget;
use mandor_usecase::overnight::{OvernightSettings, QueueRequest};
use mandor_usecase::run_ports::RunRecord;
use pretty_assertions::assert_eq;

const MIN: i64 = 60_000;
/// 2026-10-23T00:00:00Z.
const DAY0: i64 = 1_792_713_600_000;

/// ms epoch hari `day` pukul `h:m` WIB (+07:00).
fn wib(day: i64, h: i64, m: i64) -> i64 {
    DAY0 + day * 24 * 60 * MIN + ((h * 60 + m) - 7 * 60) * MIN
}

fn at(lark: &FakeLark, clock: &FakeClock, ms: i64) {
    clock.set(ms);
    lark.clock.store(ms, Ordering::SeqCst);
}

/// Daemon VM dengan kartu To Do `ids` milik `ou_me`, jam lembur 19:00–06:00.
async fn vm_with(ids: &[&str], now: i64) -> (FakeLark, FakeClock, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    at(&lark, &clock, now);
    for (i, id) in ids.iter().enumerate() {
        let mut t = ready_task(id, &["ou_me"]);
        t.key = Some(format!("PGS-140{}", i + 1));
        t.title = format!("Kartu {}", i + 1);
        t.last_modified_ms = now;
        lark.put(t);
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

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

fn request() -> QueueRequest {
    QueueRequest {
        target: StartTarget::Repo("repo1".to_owned()),
        note: None,
        design_sources: Vec::new(),
        planned: None,
    }
}

async fn queue_group(st: &Station, cards: &[&str]) {
    st.svc.enqueue_group(&ids(cards), &request()).await.unwrap();
}

/// Tick penjadwal lalu kirim outbox (kartu grup In Progress di Lark), seperti
/// loop sesi daemon.
async fn tick(st: &Station) -> Option<String> {
    let started = st.svc.overnight_tick().await.unwrap().started;
    st.svc.sync().drain_outbox(50).await.unwrap();
    started
}

fn group_run(st: &Station) -> RunRecord {
    st.runs.only()
}

fn card_states(st: &Station) -> Vec<CardState> {
    let run = group_run(st);
    st.runs.cards.lock().unwrap()[&run.id]
        .iter()
        .map(|c| c.state.clone())
        .collect()
}

/// Agent mengubah worktree run grup.
fn edits(st: &Station) {
    let wt = group_run(st).worktree.unwrap();
    let mut c = st.git.st().changes.get(&wt).cloned().unwrap_or_default();
    c.dirty = true;
    c.files = vec!["src/job.rs".to_owned()];
    "+job".clone_into(&mut c.diff);
    st.git.set_changes(&wt, c);
}

fn held(st: &Station) -> Option<HoldReason> {
    let r = group_run(st);
    (r.state == RunState::Held).then(|| r.hold.unwrap().reason)
}

fn turns(st: &Station) -> usize {
    st.agent.count()
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_f2_dan_ae6_grup_dikerjakan_berurutan_sampai_pr_draft_lalu_send_to_review() {
    let (lark, _clock, st) = vm_with(&["c1", "c2", "c3"], wib(0, 20, 0)).await;
    queue_group(&st, &["c1", "c2", "c3"]).await;
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("c1")
    );
    assert!(
        st.overnight.ids().is_empty(),
        "item grup keluar dari antrean"
    );
    st.svc.sync().drain_outbox(20).await.unwrap();
    for id in ["c1", "c2", "c3"] {
        assert_eq!(lark.get(id).unwrap().status, TaskStatus::InProgress);
    }

    for n in 1..=3 {
        assert_eq!(
            turns(&st),
            n,
            "satu giliran per kartu, tanpa persetujuan plan"
        );
        edits(&st);
        st.agent.last().done();
        if n < 3 {
            eventually(|| turns(&st) == n + 1).await;
        }
    }
    eventually(|| group_run(&st).draft_pending.is_some()).await;
    let commits: Vec<String> = st.git.st().commits.iter().map(|(_, m)| m.clone()).collect();
    assert_eq!(commits.len(), 3, "satu commit per kartu: {commits:?}");
    assert!(commits[0].starts_with("PGS-1401"), "{commits:?}");
    assert!(commits[2].starts_with("PGS-1403"), "{commits:?}");
    assert_eq!(st.git.st().drafts.len(), 1, "satu PR draft untuk grup");
    st.svc.sync().drain_outbox(20).await.unwrap();
    for id in ["c1", "c2", "c3"] {
        assert_eq!(lark.get(id).unwrap().status, TaskStatus::InProgress);
    }

    let run = group_run(&st);
    st.svc.send_to_review(&run.id).await.unwrap();
    st.svc.sync().drain_outbox(20).await.unwrap();
    for id in ["c1", "c2", "c3"] {
        assert_eq!(lark.get(id).unwrap().status, TaskStatus::WaitingReview);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae4_vm_kartu_berubah_di_lark_memarkir_grup_lalu_lanjut_tanpanya() {
    let (lark, _clock, st) = vm_with(&["c1", "c2", "c3"], wib(0, 20, 0)).await;
    queue_group(&st, &["c1", "c2", "c3"]).await;
    tick(&st).await;
    lark.edit("c2", |t| {
        t.status = TaskStatus::Unknown("Blocked".to_owned());
    });
    st.svc.sync().poll(true, &[]).await.unwrap();
    edits(&st);
    st.agent.last().done();
    eventually(|| held(&st).is_some()).await;
    assert_eq!(held(&st), Some(HoldReason::CardRemoved));
    assert_eq!(
        card_states(&st),
        [
            CardState::Done,
            CardState::Removed(RemovedReason::ChangedInLark),
            CardState::Pending
        ]
    );
    assert_eq!(turns(&st), 1, "kartu ketiga belum dikerjakan");
    assert!(
        lark.get("c2").unwrap().claim.is_none(),
        "klaim kartu kedua dilepas"
    );

    let run = group_run(&st);
    st.svc
        .queue_add(
            "c1",
            &StartTarget::Repo("repo1".to_owned()),
            Some("Lanjut tanpa PGS-1402"),
        )
        .await
        .unwrap();
    assert_eq!(tick(&st).await.as_deref(), Some(run.record_id.as_str()));
    assert_eq!(turns(&st), 2, "kartu ketiga dimulai");
    assert!(
        st.agent
            .last()
            .req
            .message
            .contains("Lanjut tanpa PGS-1402"),
        "catatan dev ikut ke kartu berikutnya"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn r8_kartu_kedua_tanpa_perubahan_kode_memarkir_grup_sebagai_buntu() {
    let (_lark, _clock, st) = vm_with(&["c1", "c2"], wib(0, 20, 0)).await;
    queue_group(&st, &["c1", "c2"]).await;
    tick(&st).await;
    edits(&st);
    st.agent.last().done();
    eventually(|| turns(&st) == 2).await;
    // Kartu pertama sudah di-commit lokal: unpushed > 0, worktree bersih.
    st.agent
        .last()
        .finish("Daftar job memakai tabel atau kartu?");
    eventually(|| held(&st).is_some()).await;
    assert_eq!(held(&st), Some(HoldReason::Stuck));
    assert_eq!(
        group_run(&st).hold.unwrap().note.as_deref(),
        Some("Daftar job memakai tabel atau kartu?")
    );
    assert_eq!(card_states(&st), [CardState::Done, CardState::Active]);
    assert!(st.git.st().drafts.is_empty());

    let run = group_run(&st);
    st.svc
        .queue_add(
            &run.record_id,
            &StartTarget::Repo("repo1".to_owned()),
            Some("Pakai tabel"),
        )
        .await
        .unwrap();
    tick(&st).await;
    assert_eq!(turns(&st), 3, "jawaban melanjutkan sesi kartu kedua");
    assert_eq!(st.agent.last().req.message, "Pakai tabel");
}

#[tokio::test(flavor = "multi_thread")]
async fn r2_antrean_grup_ditolak_bila_kartu_sudah_diantre_atau_kurang_dari_dua() {
    let (_lark, _clock, st) = vm_with(&["c1", "c2", "c3"], wib(0, 12, 0)).await;
    st.svc
        .queue_add("c2", &StartTarget::Repo("repo1".to_owned()), None)
        .await
        .unwrap();
    let err = st
        .svc
        .enqueue_group(&ids(&["c1", "c2"]), &request())
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("already queued"), "{err}");
    let err = st
        .svc
        .enqueue_group(&ids(&["c1"]), &request())
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("two cards"), "{err}");
    queue_group(&st, &["c1", "c3"]).await;
    let err = st
        .svc
        .queue_add("c3", &StartTarget::Repo("repo1".to_owned()), None)
        .await
        .unwrap_err();
    assert!(
        format!("{err}").contains("already queued"),
        "anggota grup: {err}"
    );
    assert_eq!(st.overnight.ids(), ["c2", "c1"]);
}
