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
use mandor_usecase::run_ports::{NoticeKind, RunRecord};
use mandor_usecase::waiting_fix::DEFAULT_QUIET_SECS as QUIET;
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
    let grouped: Vec<String> = st.runs.cards.lock().unwrap().keys().cloned().collect();
    st.runs
        .all()
        .into_iter()
        .find(|r| grouped.contains(&r.id))
        .unwrap()
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

#[tokio::test(flavor = "multi_thread")]
async fn r21_jam_lembur_habis_grup_berjeda_lalu_lanjut_siklus_berikutnya() {
    let (lark, clock, st) = vm_with(&["c1", "c2"], wib(0, 20, 0)).await;
    queue_group(&st, &["c1", "c2"]).await;
    tick(&st).await;
    at(&lark, &clock, wib(1, 6, 30));
    edits(&st);
    st.agent.last().done();
    eventually(|| held(&st).is_some()).await;
    assert_eq!(held(&st), Some(HoldReason::WindowEnded));
    assert_eq!(card_states(&st), [CardState::Done, CardState::Pending]);
    assert_eq!(st.overnight.ids(), ["c1"], "grup kembali di depan antrean");
    assert!(
        !st.notifier
            .sent
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.kind == NoticeKind::NeedsAttention),
        "jeda tidak memanggil dev"
    );

    at(&lark, &clock, wib(1, 7, 0));
    assert_eq!(tick(&st).await, None, "di luar jam lembur");
    at(&lark, &clock, wib(1, 19, 0));
    assert_eq!(tick(&st).await.as_deref(), Some("c1"));
    assert_eq!(turns(&st), 2, "lanjut dari kartu kedua");
    assert_eq!(card_states(&st), [CardState::Done, CardState::Active]);
}

#[tokio::test(flavor = "multi_thread")]
async fn r21_grup_start_now_di_luar_jam_tidak_berjeda_karena_jam() {
    let (_lark, _clock, st) = vm_with(&["c1", "c2"], wib(1, 12, 0)).await;
    queue_group(&st, &["c1", "c2"]).await;
    st.svc.queue_start_now("c1").await.unwrap();
    assert_eq!(tick(&st).await.as_deref(), Some("c1"));
    edits(&st);
    st.agent.last().done();
    eventually(|| turns(&st) == 2).await;
    assert_eq!(held(&st), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn r20_grup_memberi_jalan_ke_waiting_fix_lalu_lanjut_di_branch_yang_sama() {
    let (lark, clock, st) = vm_with(&["c1", "c2"], wib(1, 12, 0)).await;
    queue_group(&st, &["c1", "c2"]).await;
    st.svc.queue_start_now("c1").await.unwrap();
    tick(&st).await;
    let branch = group_run(&st).branch.unwrap();
    waiting_fix(&lark, &st, wib(1, 12, 0));
    st.svc.sync().poll(true, &[]).await.unwrap();
    assert!(
        st.svc
            .tick_waiting_fix(QUIET)
            .await
            .unwrap()
            .started
            .is_empty(),
        "VM sibuk"
    );
    at(&lark, &clock, wib(1, 12, 0) + (QUIET + 60) * 1000);
    edits(&st);
    st.agent.last().done();
    eventually(|| held(&st).is_some()).await;
    assert_eq!(held(&st), Some(HoldReason::YieldToFix));
    assert_eq!(st.overnight.ids(), ["c1"]);
    assert_eq!(tick(&st).await, None, "perbaikan didahulukan");

    // Perbaikan selesai.
    let mut fix = st.runs.all().into_iter().find(|r| r.id == "run7").unwrap();
    fix.state = RunState::InReviewLoop;
    st.runs.insert(fix);
    assert_eq!(tick(&st).await.as_deref(), Some("c1"), "Start now ikut");
    assert_eq!(turns(&st), 2);
    assert_eq!(group_run(&st).branch.unwrap(), branch);
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae7_batas_pemakaian_di_kartu_kedua_memarkir_grup_kartu_tunggal_tetap_jalan() {
    let (lark, clock, st) = vm_with(&["c1", "c2", "s"], wib(0, 20, 0)).await;
    st.svc
        .set_overnight_settings(&OvernightSettings {
            start: "19:00".to_owned(),
            end: "06:00".to_owned(),
            offset: "+07:00".to_owned(),
            token_limit: Some(1_000),
        })
        .await
        .unwrap();
    queue_group(&st, &["c1", "c2"]).await;
    st.svc
        .queue_add("s", &StartTarget::Repo("repo1".to_owned()), None)
        .await
        .unwrap();
    tick(&st).await;
    edits(&st);
    st.agent.last().done();
    eventually(|| turns(&st) == 2).await;
    st.agent
        .last()
        .emit(mandor_domain::AgentEvent::MessageUsage {
            message_id: "m1".to_owned(),
            usage: mandor_domain::Usage {
                input_tokens: 900,
                output_tokens: 200,
                ..mandor_domain::Usage::default()
            },
        });
    eventually(|| held(&st).is_some()).await;
    assert_eq!(held(&st), Some(HoldReason::UsageLimit));
    assert_eq!(card_states(&st), [CardState::Done, CardState::Active]);
    assert_eq!(
        st.overnight.ids(),
        ["s"],
        "grup tertahan tidak menahan antrean"
    );

    at(&lark, &clock, wib(1, 19, 0));
    assert_eq!(tick(&st).await.as_deref(), Some("s"), "siklus berikutnya");
}

/// Grup dua kartu sampai kartu pertama berubah di Lark setelah di-commit dan
/// gerbang menahan grup (R13).
async fn changed_after_commit() -> (FakeLark, FakeClock, Station) {
    let (lark, clock, st) = vm_with(&["c1", "c2"], wib(0, 20, 0)).await;
    queue_group(&st, &["c1", "c2"]).await;
    tick(&st).await;
    edits(&st);
    st.agent.last().done();
    eventually(|| turns(&st) == 2).await;
    lark.edit("c1", |t| {
        t.status = TaskStatus::Unknown("Blocked".to_owned());
    });
    st.svc.sync().poll(true, &[]).await.unwrap();
    edits(&st);
    st.agent.last().done();
    eventually(|| held(&st).is_some()).await;
    (lark, clock, st)
}

#[tokio::test(flavor = "multi_thread")]
async fn r13_kartu_berubah_setelah_commit_menahan_grup_tanpa_push() {
    let (_lark, _clock, st) = changed_after_commit().await;
    assert_eq!(held(&st), Some(HoldReason::NeedsConfirmation));
    assert!(
        group_run(&st)
            .hold
            .unwrap()
            .note
            .unwrap()
            .contains("changed in Lark")
    );
    assert!(st.git.st().pushes.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn r13_keep_dengan_alasan_mengantre_grup_lalu_pr_draft() {
    let (lark, _clock, st) = changed_after_commit().await;
    let run = group_run(&st);
    st.svc
        .gate_card_decision(
            &run.id,
            "c1",
            &mandor_domain::group::CardDecision::Keep {
                reason: "Status diubah PM tanpa sengaja".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(st.overnight.ids(), ["c1"], "grup diantre lagi di depan");
    tick(&st).await;
    eventually(|| group_run(&st).draft_pending.is_some()).await;
    st.svc.send_to_review(&group_run(&st).id).await.unwrap();
    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(
        lark.get("c1").unwrap().status,
        TaskStatus::Unknown("Blocked".to_owned()),
        "kartu yang tetap disertakan tidak dipindah"
    );
    assert_eq!(lark.get("c2").unwrap().status, TaskStatus::WaitingReview);
}

#[tokio::test(flavor = "multi_thread")]
async fn r13_revert_kartu_pertama_mengganti_kunci_antrean_lalu_pr_draft() {
    let (_lark, _clock, st) = changed_after_commit().await;
    let run = group_run(&st);
    st.svc
        .gate_card_decision(&run.id, "c1", &mandor_domain::group::CardDecision::Revert)
        .await
        .unwrap();
    assert_eq!(group_run(&st).record_id, "c2", "kartu utama pindah");
    assert_eq!(st.overnight.ids(), ["c2"]);
    assert_eq!(tick(&st).await.as_deref(), Some("c2"));
    eventually(|| group_run(&st).draft_pending.is_some()).await;
    assert_eq!(st.git.st().reverts.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn r13_revert_konflik_menahan_grup_tanpa_antre() {
    let (_lark, _clock, st) = changed_after_commit().await;
    let run = group_run(&st);
    let sha = st.runs.cards.lock().unwrap()[&run.id][0].commits[0]
        .sha
        .clone();
    st.git.st().revert_conflicts = vec![sha];
    st.svc
        .gate_card_decision(&run.id, "c1", &mandor_domain::group::CardDecision::Revert)
        .await
        .unwrap();
    assert_eq!(held(&st), Some(HoldReason::NeedsConfirmation));
    assert!(
        group_run(&st)
            .hold
            .unwrap()
            .note
            .unwrap()
            .contains("revert conflict")
    );
    assert!(st.overnight.ids().is_empty());
}

fn waiting_fix(lark: &FakeLark, st: &Station, now: i64) {
    let branch = "mandor/pgs-7-fix";
    let mut t = ready_task("rec7", &["ou_me"]);
    t.status = TaskStatus::WaitingFix;
    t.branch = Some(branch.to_owned());
    t.repo_id = Some("repo1".to_owned());
    t.pr_url = Some("https://github.com/o/r/pull/7".to_owned());
    t.claim = Some(mandor_domain::Claim {
        open_id: "ou_me".to_owned(),
        ts_ms: now,
        nonce: "n~vm".to_owned(),
    });
    t.comment_cursor = Some(format!("{}:", now / 1000 - 3600));
    t.last_modified_ms = now;
    lark.put(t);
    let wt = std::path::PathBuf::from(format!("/wt/{branch}"));
    let mut r = RunRecord::new("run7".to_owned(), "repo1".to_owned(), "rec7".to_owned());
    r.state = RunState::InReviewLoop;
    r.branch = Some(branch.to_owned());
    r.worktree = Some(wt.clone());
    r.session_id = Some("sess7".to_owned());
    r.session_started = true;
    r.pr_url = Some("https://github.com/o/r/pull/7".to_owned());
    st.runs.insert(r);
    st.git.st().worktrees.insert(wt);
    *lark.replies.lock().unwrap() = vec![mandor_domain::Reply {
        record_id: "rec7".to_owned(),
        comment_id: "c".to_owned(),
        reply_id: "r1".to_owned(),
        author: "ou_qa".to_owned(),
        create_time: now / 1000 - 600,
        text: "Tombol simpan error".to_owned(),
    }];
}

// --- U14: Take over grup ke laptop sebagai satu kesatuan (R22, KTD-G15) ---

async fn take_over(
    lark: &FakeLark,
    clock: &FakeClock,
    vm: &Station,
) -> (Station, mandor_usecase::overnight::Handback, String) {
    let laptop = common::station(lark, clock, "ou_me");
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    let to = laptop.svc.sync().identity().daemon_id.clone();
    let key = group_run(vm).record_id;
    let back = vm.svc.handoff(&key, &to).await.unwrap();
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    (laptop, back, key)
}

fn laptop_cards(laptop: &Station) -> Vec<CardState> {
    let run = laptop
        .runs
        .all()
        .into_iter()
        .find(|r| !laptop.runs.rows.lock().unwrap()[&r.id].1)
        .unwrap();
    laptop.runs.cards.lock().unwrap()[&run.id]
        .iter()
        .map(|c| c.state.clone())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn take_over_grup_tertahan_di_kartu_kedua_menjadi_run_grup_laptop() {
    let (lark, clock, vm) = vm_with(&["c1", "c2", "c3"], wib(0, 20, 0)).await;
    queue_group(&vm, &["c1", "c2", "c3"]).await;
    tick(&vm).await;
    edits(&vm);
    vm.agent.last().done();
    eventually(|| turns(&vm) == 2).await;
    vm.agent.last().finish("Tabel atau kartu?");
    eventually(|| held(&vm).is_some()).await;

    let (laptop, back, key) = take_over(&lark, &clock, &vm).await;
    assert_eq!(back.carry.cards.len(), 3);
    let run = laptop
        .svc
        .takeover(&key, &back.target.clone().unwrap(), None, &back.carry)
        .await
        .unwrap();
    assert_eq!(run.state, RunState::AwaitingDev);
    assert_eq!(
        laptop_cards(&laptop),
        [CardState::Done, CardState::Active, CardState::Pending]
    );
    assert!(
        run.group_plan,
        "kartu sisa di laptop menunggu persetujuan plan (R6)"
    );
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    let me = laptop.svc.sync().identity().daemon_id.clone();
    for id in ["c1", "c2", "c3"] {
        assert_eq!(
            lark.get(id).unwrap().claim.unwrap().daemon_id(),
            Some(me.as_str()),
            "klaim {id} pindah ke laptop"
        );
    }
    vm.svc.sync().poll(true, &[]).await.unwrap();
    let err = vm
        .svc
        .queue_add("c3", &StartTarget::Repo("repo1".to_owned()), None)
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("taken over"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn take_over_grup_pr_draft_lalu_send_to_review_dari_laptop() {
    let (lark, clock, vm) = vm_with(&["c1", "c2"], wib(0, 20, 0)).await;
    queue_group(&vm, &["c1", "c2"]).await;
    tick(&vm).await;
    for n in 1..=2 {
        edits(&vm);
        vm.agent.last().done();
        if n == 1 {
            eventually(|| turns(&vm) == 2).await;
        }
    }
    eventually(|| group_run(&vm).draft_pending.is_some()).await;
    vm.svc.sync().drain_outbox(50).await.unwrap();
    let pr = vm.git.st().created_prs[0].clone();
    let (laptop, back, key) = take_over(&lark, &clock, &vm).await;
    assert!(back.carry.draft);
    let url = "https://github.com/o/r/pull/1".to_owned();
    laptop
        .git
        .st()
        .prs
        .insert(format!("repo1:{}", pr.0), url.clone());
    let run = laptop
        .svc
        .takeover(&key, &back.target.clone().unwrap(), None, &back.carry)
        .await
        .unwrap();
    assert_eq!(laptop_cards(&laptop), [CardState::Done, CardState::Done]);
    assert_eq!(run.pr_url.as_deref(), Some(url.as_str()));
    laptop.svc.send_to_review(&run.id).await.unwrap();
    laptop.svc.sync().drain_outbox(50).await.unwrap();
    for id in ["c1", "c2"] {
        assert_eq!(lark.get(id).unwrap().status, TaskStatus::WaitingReview);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn handoff_grup_diulang_mengembalikan_hasil_yang_sama() {
    let (lark, clock, vm) = vm_with(&["c1", "c2"], wib(0, 20, 0)).await;
    queue_group(&vm, &["c1", "c2"]).await;
    tick(&vm).await;
    vm.agent.last().finish("Tanya?");
    eventually(|| held(&vm).is_some()).await;
    let (laptop, first, _) = take_over(&lark, &clock, &vm).await;
    let to = laptop.svc.sync().identity().daemon_id.clone();
    let again = vm.svc.handoff("c1", &to).await.unwrap();
    assert_eq!(again, first);
}
