//! Penjadwal lembur di daemon VM (U2 plan VM): antrean, jam lembur, Start now,
//! dan giliran Waiting Fix, satu kartu sekaligus.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station, vm_station};
use mandor_domain::{Claim, Reply, RunState, TaskStatus};
use mandor_usecase::overnight::OvernightSettings;
use mandor_usecase::run_ports::{RunPhase, RunRecord, SessionRef};
use mandor_usecase::waiting_fix::DEFAULT_QUIET_SECS as QUIET;
use mandor_usecase::{RunError, StartTarget};
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

fn settings(limit: Option<u64>) -> OvernightSettings {
    OvernightSettings {
        start: "19:00".to_owned(),
        end: "06:00".to_owned(),
        offset: "+07:00".to_owned(),
        token_limit: limit,
    }
}

/// Daemon VM dengan kartu To Do `ids`, jam lembur 19:00–06:00, batas diisi.
async fn vm_with(ids: &[&str], now: i64) -> (FakeLark, FakeClock, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    at(&lark, &clock, now);
    for id in ids {
        let mut t = ready_task(id, &["ou_me"]);
        t.last_modified_ms = now;
        lark.put(t);
    }
    let st = vm_station(&lark, &clock, "ou_me");
    st.svc
        .set_overnight_settings(&settings(Some(1_000_000)))
        .await
        .unwrap();
    st.svc.sync().poll(true, &[]).await.unwrap();
    (lark, clock, st)
}

fn repo1() -> StartTarget {
    StartTarget::Repo("repo1".to_owned())
}

async fn queue(st: &Station, ids: &[&str]) {
    for id in ids {
        st.svc.queue_add(id, &repo1(), None).await.unwrap();
    }
}

fn running(st: &Station) -> Vec<String> {
    st.runs
        .all()
        .into_iter()
        .filter(|r| r.state.is_working())
        .map(|r| r.record_id)
        .collect()
}

#[tokio::test]
async fn batas_siklus_belum_diisi_tidak_memulai_apapun() {
    let (_lark, _clock, st) = vm_with(&["a"], wib(0, 20, 0)).await;
    queue(&st, &["a"]).await;
    st.svc
        .set_overnight_settings(&settings(None))
        .await
        .unwrap();
    st.svc.queue_start_now("a").await.unwrap();
    let tick = st.svc.overnight_tick().await.unwrap();
    assert_eq!(tick.started, None, "Start now juga butuh batas");
    assert!(st.runs.all().is_empty());
}

#[tokio::test]
async fn covers_ae1_satu_kartu_sekaligus_berurutan() {
    let (_lark, _clock, st) = vm_with(&["a", "b", "c"], wib(0, 19, 0)).await;
    queue(&st, &["a", "b", "c"]).await;
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("a")
    );
    let run = st.runs.only();
    assert_eq!(
        run.phase,
        RunPhase::Implement,
        "VM tidak menunggu persetujuan plan"
    );
    assert_eq!(run.repo_id, "repo1", "target dari antrean");
    assert_eq!(st.overnight.ids(), ["b", "c"]);
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started,
        None,
        "masih berjalan"
    );

    st.agent.last().done();
    eventually(|| running(&st).is_empty()).await;
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("b")
    );
    assert_eq!(running(&st), ["b"]);
}

#[tokio::test]
async fn covers_ae2_kartu_berjalan_diselesaikan_setelah_jam_lembur_berakhir() {
    let (lark, clock, st) = vm_with(&["a", "b"], wib(1, 5, 40)).await;
    queue(&st, &["a", "b"]).await;
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("a")
    );
    at(&lark, &clock, wib(1, 6, 0));
    assert_eq!(running(&st), ["a"], "kartu yang berjalan tidak dihentikan");
    st.agent.last().done();
    eventually(|| running(&st).is_empty()).await;
    assert_eq!(st.svc.overnight_tick().await.unwrap().started, None);
    assert_eq!(st.overnight.ids(), ["b"]);
}

#[tokio::test]
async fn covers_ae3_start_now_siang_hari_bila_vm_kosong() {
    let (_lark, _clock, st) = vm_with(&["a", "b"], wib(1, 14, 0)).await;
    queue(&st, &["a", "b"]).await;
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started,
        None,
        "di luar jam lembur"
    );
    st.svc.queue_start_now("b").await.unwrap();
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("b")
    );
    assert_eq!(st.overnight.ids(), ["a"]);
}

#[tokio::test]
async fn covers_ae4_kartu_yang_sudah_in_progress_dikeluarkan_dengan_alasan() {
    let (lark, clock, st) = vm_with(&["a", "b"], wib(0, 18, 0)).await;
    queue(&st, &["a", "b"]).await;
    lark.edit("a", |t| {
        t.status = TaskStatus::InProgress;
        t.claim = Some(Claim {
            open_id: "ou_lain".to_owned(),
            ts_ms: wib(0, 18, 0),
            nonce: "x".to_owned(),
        });
    });
    at(&lark, &clock, wib(0, 19, 0));
    let tick = st.svc.overnight_tick().await.unwrap();
    assert_eq!(tick.dequeued.len(), 1);
    assert_eq!(tick.dequeued[0].0, "a");
    assert!(
        tick.dequeued[0].1.contains("In Progress"),
        "{:?}",
        tick.dequeued
    );
    assert_eq!(
        tick.started.as_deref(),
        Some("b"),
        "kartu berikutnya dicoba"
    );
    let logged = st.overnight.dequeued.lock().unwrap().clone();
    assert_eq!(logged[0].record_id, "a", "dicatat untuk laporan pagi");
}

const BRANCH: &str = "mandor/pgs-7-tab-data";

/// Kartu `rec7` milik VM di Waiting Fix dengan run lama dan komentar tepercaya
/// yang jeda tenangnya sudah lewat.
fn vm_waiting_fix(lark: &FakeLark, st: &Station, now: i64) {
    let mut t = ready_task("rec7", &["ou_me"]);
    t.status = TaskStatus::WaitingFix;
    t.branch = Some(BRANCH.to_owned());
    t.repo_id = Some("repo1".to_owned());
    t.pr_url = Some("https://github.com/o/r/pull/7".to_owned());
    t.claim = Some(Claim {
        open_id: "ou_me".to_owned(),
        ts_ms: now,
        nonce: "n~vm".to_owned(),
    });
    t.comment_cursor = Some(format!("{}:", now / 1000 - 3600));
    t.last_modified_ms = now;
    lark.put(t);
    let wt = PathBuf::from(format!("/wt/{BRANCH}"));
    let mut r = RunRecord::new("run7".to_owned(), "repo1".to_owned(), "rec7".to_owned());
    r.state = RunState::InReviewLoop;
    r.branch = Some(BRANCH.to_owned());
    r.worktree = Some(wt.clone());
    r.session_id = Some("sess7".to_owned());
    r.session_started = true;
    r.pr_url = Some("https://github.com/o/r/pull/7".to_owned());
    st.runs.insert(r);
    st.git.st().worktrees.insert(wt);
    *lark.replies.lock().unwrap() = vec![Reply {
        record_id: "rec7".to_owned(),
        comment_id: "c".to_owned(),
        reply_id: "r1".to_owned(),
        author: "ou_qa".to_owned(),
        create_time: now / 1000 - 600,
        text: "Tombol simpan error".to_owned(),
    }];
}

#[tokio::test]
async fn covers_ae9_waiting_fix_mulai_langsung_saat_vm_kosong() {
    let (lark, _clock, st) = vm_with(&[], wib(1, 11, 0)).await;
    vm_waiting_fix(&lark, &st, wib(1, 11, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    let report = st.svc.tick_waiting_fix(QUIET).await.unwrap();
    assert_eq!(report.started, ["rec7"], "tanpa menunggu jam lembur");
}

#[tokio::test]
async fn covers_ae10_waiting_fix_menunggu_lalu_mendahului_antrean() {
    let (lark, _clock, st) = vm_with(&["a", "b"], wib(1, 11, 0)).await;
    queue(&st, &["a", "b"]).await;
    st.svc.queue_start_now("a").await.unwrap();
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("a")
    );
    st.svc.queue_start_now("b").await.unwrap();

    vm_waiting_fix(&lark, &st, wib(1, 11, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    let report = st.svc.tick_waiting_fix(QUIET).await.unwrap();
    assert!(report.started.is_empty(), "VM sibuk: perbaikan menunggu");
    let fix = st.runs.all().into_iter().find(|r| r.id == "run7").unwrap();
    assert_eq!(fix.state, RunState::WfQuiet);

    // Kartu "a" selesai: perbaikan mendapat giliran sebelum kartu antrean.
    let a_turn = st.agent.last();
    a_turn.done();
    eventually(|| running(&st).is_empty()).await;
    assert_eq!(st.svc.overnight_tick().await.unwrap().started, None);
    assert_eq!(st.overnight.ids(), ["b"]);
    let report = st.svc.tick_waiting_fix(QUIET).await.unwrap();
    assert_eq!(report.started, ["rec7"]);
}

#[tokio::test]
async fn kartu_tertahan_yang_diantre_ulang_dilanjutkan_di_sesi_yang_sama() {
    let (lark, _clock, st) = vm_with(&[], wib(0, 20, 0)).await;
    let mut t = ready_task("held", &["ou_me"]);
    t.status = TaskStatus::InProgress;
    lark.put(t);
    st.svc.sync().poll(true, &[]).await.unwrap();
    let wt = PathBuf::from("/wt/mandor/held");
    let mut r = RunRecord::new("run-h".to_owned(), "repo1".to_owned(), "held".to_owned());
    r.state = RunState::Held;
    r.phase = RunPhase::Implement;
    r.branch = Some("mandor/held".to_owned());
    r.worktree = Some(wt.clone());
    r.session_id = Some("sess-h".to_owned());
    r.session_started = true;
    st.runs.insert(r);
    st.git.st().worktrees.insert(wt);

    st.svc
        .queue_add("held", &repo1(), Some("Pakai warna primer"))
        .await
        .unwrap();
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("held")
    );
    let req = st.agent.last().req;
    assert_eq!(req.session, SessionRef::Resume("sess-h".to_owned()));
    assert!(req.message.contains("Pakai warna primer"));
    assert_eq!(st.runs.all().len(), 1, "run yang sama, bukan run baru");
}

#[tokio::test]
async fn antrean_hanya_to_do_atau_tertahan_dan_bukan_kartu_laptop() {
    let (lark, _clock, st) = vm_with(&["a", "b", "c"], wib(0, 10, 0)).await;
    lark.edit("c", |t| t.status = TaskStatus::InProgress);
    st.svc.sync().poll(true, &[]).await.unwrap();
    queue(&st, &["a", "b"]).await;
    let err = st.svc.queue_add("c", &repo1(), None).await.unwrap_err();
    assert!(matches!(err, RunError::Sync(_)), "bukan To Do: {err:?}");
    let err = st.svc.queue_add("a", &repo1(), None).await.unwrap_err();
    assert!(
        matches!(err, RunError::InvalidState(_)),
        "sudah diantre: {err:?}"
    );
    st.overnight
        .laptop_only
        .lock()
        .unwrap()
        .insert("b".to_owned());
    st.svc.queue_remove("b").await.unwrap();
    let err = st.svc.queue_add("b", &repo1(), None).await.unwrap_err();
    assert!(
        matches!(err, RunError::InvalidState(_)),
        "hanya laptop: {err:?}"
    );

    lark.edit("c", |t| t.status = TaskStatus::ToDo);
    st.svc.sync().poll(true, &[]).await.unwrap();
    st.svc.queue_add("c", &repo1(), None).await.unwrap();
    st.svc
        .queue_reorder(&["c".to_owned(), "a".to_owned()])
        .await
        .unwrap();
    assert_eq!(st.overnight.ids(), ["c", "a"]);
}

#[tokio::test]
async fn daemon_laptop_tidak_menjalankan_penjadwal_atau_antrean() {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    at(&lark, &clock, wib(0, 20, 0));
    lark.put(ready_task("a", &["ou_me"]));
    let st = station(&lark, &clock, "ou_me");
    st.svc.sync().poll(true, &[]).await.unwrap();
    assert!(st.svc.queue_add("a", &repo1(), None).await.is_err());
    assert_eq!(st.svc.overnight_tick().await.unwrap().started, None);
}

#[tokio::test]
async fn setelan_jam_lembur_divalidasi() {
    let (_lark, _clock, st) = vm_with(&[], wib(0, 10, 0)).await;
    let mut bad = settings(Some(1));
    bad.offset = "7".to_owned();
    assert!(st.svc.set_overnight_settings(&bad).await.is_err());
}

// --- Kartu tertahan dan gerbang AC VM (U4 plan VM) ---

/// Daemon VM dengan kartu `a` (AC `ac1` bila `with_ac`) dan `b` di antrean;
/// kartu `a` sudah dimulai.
async fn vm_started(with_ac: bool) -> (FakeLark, FakeClock, Station) {
    let (lark, clock, st) = vm_with(&["a", "b"], wib(0, 20, 0)).await;
    if with_ac {
        lark.acs.lock().unwrap().insert(
            "ac1".to_owned(),
            mandor_usecase::AcItem {
                record_id: "ac1".to_owned(),
                label: Some("AC-1".to_owned()),
                text: "Tombol simpan".to_owned(),
                ..Default::default()
            },
        );
        lark.edit("a", |t| t.ac_ids = vec!["ac1".to_owned()]);
        st.svc.sync().poll(true, &[]).await.unwrap();
    }
    queue(&st, &["a", "b"]).await;
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("a")
    );
    (lark, clock, st)
}

fn run_of(st: &Station, record: &str) -> RunRecord {
    st.runs
        .all()
        .into_iter()
        .find(|r| r.record_id == record)
        .unwrap()
}

fn changed(st: &Station, record: &str, file: &str) {
    let wt = run_of(st, record).worktree.unwrap();
    st.git.set_changes(
        &wt,
        mandor_usecase::run_ports::WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec![file.to_owned()],
            diff: "+x".to_owned(),
        },
    );
}

fn report_verdict(st: &Station, ac: &str, v: mandor_domain::Verdict) {
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    let rt = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        rt.block_on(st.svc.ledger_set_verdict(&scope, ac, v, Some("x")))
    })
    .unwrap();
}

fn held(st: &Station, record: &str) -> Option<mandor_domain::HoldReason> {
    let r = run_of(st, record);
    (r.state == RunState::Held).then(|| r.hold.unwrap().reason)
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae6_cant_verify_menahan_ac_gate_tanpa_push_lalu_kartu_berikutnya() {
    let (_lark, _clock, st) = vm_started(true).await;
    changed(&st, "a", "src/form.rs");
    st.agent.last().done();
    eventually(|| run_of(&st, "a").state == RunState::Reviewing).await;
    report_verdict(&st, "ac1", mandor_domain::Verdict::CantVerify);
    st.agent.last().done();
    eventually(|| held(&st, "a").is_some()).await;
    assert_eq!(held(&st, "a"), Some(mandor_domain::HoldReason::AcGate));
    assert!(
        st.git.st().pushes.is_empty(),
        "VM tidak push melewati gerbang"
    );
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("b")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae12_konfirmasi_dev_pada_kartu_tertahan_langsung_push_tanpa_agent() {
    let (lark, _clock, st) = vm_started(true).await;
    changed(&st, "a", "src/form.rs");
    st.agent.last().done();
    eventually(|| run_of(&st, "a").state == RunState::Reviewing).await;
    report_verdict(&st, "ac1", mandor_domain::Verdict::CantVerify);
    st.agent.last().done();
    eventually(|| held(&st, "a").is_some()).await;

    let turns = st.agent.count();
    let run_id = run_of(&st, "a").id;
    let decision = mandor_domain::GateDecision {
        confirmed: vec!["ac1".to_owned()],
        push_anyway_reason: None,
        decided_by: Some("Beni".to_owned()),
    };
    let out = st.svc.publish_gated(&run_id, &decision).await.unwrap();
    assert!(
        matches!(out, mandor_usecase::run::PublishOutcome::Published { .. }),
        "{out:?}"
    );
    assert_eq!(st.git.st().pushes.len(), 1);
    assert_eq!(st.git.st().created_prs.len(), 1);
    assert_eq!(st.git.st().drafts.len(), 1, "PR draft");
    assert_eq!(st.agent.count(), turns, "tanpa menjalankan agent");
    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(lark.get("a").unwrap().status, TaskStatus::InProgress);
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_buntu_menahan_stuck_dengan_pertanyaan_lalu_kartu_berikutnya() {
    let (_lark, _clock, st) = vm_started(false).await;
    st.agent.last().finish("Warna tombol primer atau sekunder?");
    eventually(|| held(&st, "a").is_some()).await;
    let r = run_of(&st, "a");
    assert_eq!(
        r.hold.as_ref().unwrap().reason,
        mandor_domain::HoldReason::Stuck
    );
    assert_eq!(
        r.hold.unwrap().note.as_deref(),
        Some("Warna tombol primer atau sekunder?")
    );
    assert_eq!(
        st.svc.overnight_tick().await.unwrap().started.as_deref(),
        Some("b")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn perubahan_ci_menahan_needs_confirmation() {
    let (_lark, _clock, st) = vm_started(false).await;
    changed(&st, "a", ".github/workflows/ci.yml");
    st.agent.last().done();
    eventually(|| held(&st, "a").is_some()).await;
    assert_eq!(
        held(&st, "a"),
        Some(mandor_domain::HoldReason::NeedsConfirmation)
    );
    assert!(st.git.st().pushes.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae13_pr_draft_lalu_send_to_review() {
    let (lark, _clock, st) = vm_started(false).await;
    changed(&st, "a", "src/form.rs");
    st.agent.last().done();
    eventually(|| !st.git.st().created_prs.is_empty()).await;
    eventually(|| run_of(&st, "a").draft_pending.is_some()).await;
    assert_eq!(held(&st, "a"), None);
    assert_eq!(st.git.st().drafts.len(), 1, "PR dibuat draft");
    st.svc.sync().drain_outbox(20).await.unwrap();
    let card = lark.get("a").unwrap();
    assert_eq!(
        card.status,
        TaskStatus::InProgress,
        "reviewer belum diberi tahu"
    );
    assert!(card.pr_url.is_some(), "link PR tetap ditulis");
    let run = run_of(&st, "a");
    assert_eq!(run.state, RunState::AwaitingDev);

    // gh pr ready gagal: tidak ada transisi.
    st.git.st().fail_ready = true;
    assert!(st.svc.send_to_review(&run.id).await.is_err());
    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(lark.get("a").unwrap().status, TaskStatus::InProgress);

    st.git.st().fail_ready = false;
    st.svc.send_to_review(&run.id).await.unwrap();
    assert_eq!(st.git.st().ready.len(), 1);
    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(lark.get("a").unwrap().status, TaskStatus::WaitingReview);
    assert_eq!(run_of(&st, "a").draft_pending, None);
    let err = st.svc.send_to_review(&run.id).await.unwrap_err();
    assert!(
        matches!(err, RunError::InvalidState(_)),
        "tanpa PR draft: {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn send_to_review_ditolak_untuk_kartu_yang_dipegang_daemon_lain() {
    let (lark, _clock, st) = vm_started(false).await;
    changed(&st, "a", "src/form.rs");
    st.agent.last().done();
    eventually(|| run_of(&st, "a").draft_pending.is_some()).await;
    lark.edit("a", |t| {
        t.claim = Some(Claim {
            open_id: "ou_me".to_owned(),
            ts_ms: wib(0, 20, 0),
            nonce: "x~laptop".to_owned(),
        });
    });
    st.svc.sync().poll(true, &[]).await.unwrap();
    let run = run_of(&st, "a");
    assert!(st.svc.send_to_review(&run.id).await.is_err());
    assert!(st.git.st().ready.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn waiting_fix_pada_pr_yang_sudah_dikirim_kembali_ke_waiting_review_tanpa_konfirmasi() {
    let (lark, _clock, st) = vm_with(&[], wib(1, 11, 0)).await;
    vm_waiting_fix(&lark, &st, wib(1, 11, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    assert_eq!(
        st.svc.tick_waiting_fix(QUIET).await.unwrap().started,
        ["rec7"]
    );
    st.svc.sync().drain_outbox(20).await.unwrap();
    changed(&st, "rec7", "src/form.rs");
    st.agent.last().done();
    eventually(|| run_of(&st, "rec7").state == RunState::InReviewLoop).await;
    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(lark.get("rec7").unwrap().status, TaskStatus::WaitingReview);
    assert!(st.git.st().drafts.is_empty(), "PR lama tidak dibuat ulang");
}

#[tokio::test]
async fn gerbang_selalu_block_di_vm_dan_tetap_warn_di_laptop() {
    let warn_repo = |st: &Station| {
        let mut r = st.repos.repos.lock().unwrap()[0].clone();
        r.gate_mode = Some(mandor_domain::GateMode::Warn);
        st.repos.repos.lock().unwrap()[0] = r;
        let mut run = RunRecord::new("r1".to_owned(), "repo1".to_owned(), "x".to_owned());
        run.worktree = Some(PathBuf::from("/wt/x"));
        st.runs.insert(run);
    };
    let (_lark, _clock, on_vm) = vm_with(&[], wib(0, 20, 0)).await;
    warn_repo(&on_vm);
    assert_eq!(
        on_vm.svc.gate_mode_for("r1").await.unwrap(),
        mandor_domain::GateMode::Block
    );
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    let laptop = station(&lark, &clock, "ou_me");
    warn_repo(&laptop);
    assert_eq!(
        laptop.svc.gate_mode_for("r1").await.unwrap(),
        mandor_domain::GateMode::Warn
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn waiting_fix_vm_dengan_ac_fail_ditahan_ac_gate_dan_tercatat_masuk_waiting_fix() {
    let (lark, _clock, st) = vm_with(&[], wib(1, 11, 0)).await;
    vm_waiting_fix(&lark, &st, wib(1, 11, 0));
    let mut row =
        mandor_domain::AcRow::new("ac1", Some("AC-1"), "x", mandor_domain::RowStatus::Active);
    row.verdict = mandor_domain::Verdict::Pass;
    st.reviews
        .ledgers
        .lock()
        .unwrap()
        .insert("run7".to_owned(), vec![row]);
    lark.edit("rec7", |t| t.ac_ids = vec!["ac1".to_owned()]);
    lark.acs.lock().unwrap().insert(
        "ac1".to_owned(),
        mandor_usecase::AcItem {
            record_id: "ac1".to_owned(),
            label: Some("AC-1".to_owned()),
            text: "x".to_owned(),
            ..Default::default()
        },
    );
    st.svc.sync().poll(true, &[]).await.unwrap();
    let report = st.svc.tick_waiting_fix(QUIET).await.unwrap();
    assert_eq!(report.started, ["rec7"]);
    assert!(run_of(&st, "rec7").entered_waiting_fix_ms.is_some());
    changed(&st, "rec7", "src/form.rs");
    st.agent.last().done();
    eventually(|| run_of(&st, "rec7").state == RunState::Reviewing).await;
    report_verdict(&st, "ac1", mandor_domain::Verdict::Fail);
    st.agent.last().done();
    eventually(|| held(&st, "rec7").is_some()).await;
    assert_eq!(held(&st, "rec7"), Some(mandor_domain::HoldReason::AcGate));
    assert!(st.git.st().pushes.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn berkas_ci_yang_berubah_saat_review_menahan_push_vm() {
    let (_lark, _clock, st) = vm_started(true).await;
    changed(&st, "a", "src/form.rs");
    st.agent.last().done();
    eventually(|| run_of(&st, "a").state == RunState::Reviewing).await;
    report_verdict(&st, "ac1", mandor_domain::Verdict::Pass);
    // Berkas CI ikut berubah selama review.
    changed(&st, "a", ".github/workflows/ci.yml");
    st.agent.last().done();
    eventually(|| held(&st, "a").is_some()).await;
    assert_eq!(
        held(&st, "a"),
        Some(mandor_domain::HoldReason::NeedsConfirmation)
    );
    assert!(st.git.st().pushes.is_empty());
}
