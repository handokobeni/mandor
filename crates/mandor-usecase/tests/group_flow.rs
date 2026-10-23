//! Run grup kartu satu flow (plan `2026-10-23-004`): mulai, pindah kartu,
//! berhenti, gerbang, dan PR.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::group::{CardDecision, CardState, RemovedReason};
use mandor_domain::{AppField, Claim, GateDecision, RunState, TaskStatus, Verdict, WriteOp};
use mandor_usecase::run::{CardOutcome, PublishOutcome};
use mandor_usecase::run_ports::{GroupCardRow, RunPhase, SessionRef, WorkChanges};
use mandor_usecase::{AcItem, EpicItem, Planning, RunError, StartTarget};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

/// Tiga kartu To Do satu epic milik `ou_me`, masing-masing dengan satu AC.
fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    for (i, (id, key, title)) in [
        ("c1", "PGS-1401", "Daftar job"),
        ("c2", "PGS-1402", "Kartu job"),
        ("c3", "PGS-1403", "Buat job"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut t = ready_task(id, &["ou_me"]);
        t.key = Some(key.to_owned());
        title.clone_into(&mut t.title);
        t.epic_ids = vec!["ep1".to_owned()];
        t.ac_ids = vec![format!("ac{}", i + 1)];
        t.last_modified_ms = 10 * DAY;
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
    lark.epics.lock().unwrap().insert(
        "ep1".to_owned(),
        EpicItem {
            record_id: "ep1".to_owned(),
            key: Some("E-PGS43".to_owned()),
            name: "Job & Position".to_owned(),
        },
    );
    (lark, clock)
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

async fn start_with(
    st: &Station,
    cards: &[&str],
    planning: &Planning,
) -> Result<mandor_usecase::run_ports::RunRecord, RunError> {
    st.svc
        .start_group(
            &ids(cards),
            &StartTarget::Repo("repo1".to_owned()),
            None,
            planning,
            &[],
            |_| {},
        )
        .await
}

async fn start(
    st: &Station,
    cards: &[&str],
) -> Result<mandor_usecase::run_ports::RunRecord, RunError> {
    start_with(st, cards, &Planning::Plan { skill: None }).await
}

#[tokio::test]
async fn tanpa_plan_hanya_ac_kartu_pertama_yang_dikirim() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start_with(&st, &["c1", "c2"], &Planning::Skip)
        .await
        .unwrap();
    assert_eq!(run.phase, RunPhase::Implement);
    let msg = st.agent.last().req.message;
    assert!(msg.contains("AC kartu 1"));
    assert!(!msg.contains("AC kartu 2"), "AC kartu lain tidak dikirim");
}

#[tokio::test]
async fn f1_mulai_grup_mengklaim_semua_kartu_dan_plan_kartu_pertama() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, &["c1", "c2", "c3"]).await.unwrap();

    assert_eq!(run.record_id, "c1", "kartu utama = kartu pertama");
    assert_eq!(run.phase, RunPhase::Plan);
    assert_eq!(run.state, RunState::Running);
    assert_eq!(st.git.st().prepared, ["mandor/pgs-1401-1403-job-position"]);

    let cards = st.svc.group_cards(&run.id).await.unwrap();
    let states: Vec<(&str, &CardState)> = cards
        .iter()
        .map(|c| (c.record_id.as_str(), &c.state))
        .collect();
    assert_eq!(
        states,
        [
            ("c1", &CardState::Active),
            ("c2", &CardState::Pending),
            ("c3", &CardState::Pending)
        ]
    );

    let msg = st.agent.last().req.message;
    assert!(msg.contains("Daftar job"), "kartu pertama dikerjakan");
    assert!(
        msg.contains("PGS-1402") && msg.contains("PGS-1403"),
        "konteks flow"
    );

    for id in ["c1", "c2", "c3"] {
        assert_eq!(
            lark.get(id).unwrap().claim.unwrap().open_id,
            "ou_me",
            "{id}"
        );
    }
    let transitions = st
        .outbox
        .ops()
        .into_iter()
        .filter(|op| {
            matches!(op, WriteOp::Transition { from: TaskStatus::ToDo, to: TaskStatus::InProgress, fields }
                if fields.contains(&AppField::Branch("mandor/pgs-1401-1403-job-position".to_owned())))
        })
        .count();
    assert_eq!(
        transitions, 3,
        "ketiga kartu In Progress dengan branch grup"
    );

    let rows = st.svc.ledger(&run.id).await.unwrap();
    let labelled: Vec<(&str, Option<&str>)> = rows
        .iter()
        .map(|r| (r.ac_id.as_str(), r.card.as_deref()))
        .collect();
    assert_eq!(
        labelled,
        [
            ("ac1", Some("c1")),
            ("ac2", Some("c2")),
            ("ac3", Some("c3"))
        ]
    );
}

#[tokio::test]
async fn klaim_gagal_melepas_klaim_yang_sudah_didapat() {
    let (lark, clock) = setup();
    lark.edit("c2", |t| {
        t.assignees.push("ou_other".to_owned());
        t.claim = Some(Claim {
            open_id: "ou_other".to_owned(),
            ts_ms: 10 * DAY,
            nonce: "x".to_owned(),
        });
    });
    let st = station(&lark, &clock, "ou_me");
    assert!(start(&st, &["c1", "c2", "c3"]).await.is_err());
    assert_eq!(lark.get("c1").unwrap().claim, None, "klaim kartu 1 dilepas");
    assert_eq!(lark.get("c2").unwrap().claim.unwrap().open_id, "ou_other");
    assert_eq!(lark.get("c3").unwrap().claim, None, "kartu 3 tidak diklaim");
    assert!(st.git.st().prepared.is_empty());
    assert!(st.outbox.ops().is_empty());
    assert!(st.svc.active_runs().await.unwrap().is_empty());
}

#[tokio::test]
async fn kartu_dengan_run_dan_grup_satu_kartu_ditolak() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.svc.start("c2", "repo1", None).await.unwrap();
    let err = start(&st, &["c1", "c2"]).await.unwrap_err();
    assert!(matches!(err, RunError::InvalidState(_)), "{err:?}");
    assert_eq!(lark.get("c1").unwrap().claim, None, "tidak ada klaim");

    let err = start(&st, &["c3"]).await.unwrap_err();
    assert!(matches!(err, RunError::InvalidState(_)), "{err:?}");
}

#[tokio::test]
async fn kandidat_grup_dari_cache() {
    let (lark, clock) = setup();
    let mut other = ready_task("c9", &["ou_lain"]);
    other.epic_ids = vec!["ep1".to_owned()];
    lark.put(other);
    let st = station(&lark, &clock, "ou_me");
    st.svc.sync().poll(true, &[]).await.unwrap();
    st.svc.start("c3", "repo1", None).await.unwrap();

    let c = st.svc.group_candidates("c1").await.unwrap();
    let got: Vec<(&str, bool)> = c
        .iter()
        .map(|c| (c.record_id.as_str(), c.blocked.is_none()))
        .collect();
    assert_eq!(got, [("c1", true), ("c2", true), ("c3", false)]);
}

#[tokio::test]
async fn refresh_buku_ac_grup_dari_semua_kartu_dan_gagal_sebagian_tidak_mengubah() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, &["c1", "c2", "c3"]).await.unwrap();

    // PM menambah AC di kartu 2 setelah grup dimulai.
    lark.acs.lock().unwrap().insert(
        "ac9".to_owned(),
        AcItem {
            record_id: "ac9".to_owned(),
            label: Some("AC-9".to_owned()),
            text: "AC baru kartu 2".to_owned(),
            ..Default::default()
        },
    );
    lark.edit("c2", |t| t.ac_ids.push("ac9".to_owned()));
    let r = st.svc.refresh_ledger(&run.id).await.unwrap();
    assert_eq!(r.error, None);
    let labelled: Vec<(&str, Option<&str>)> = r
        .rows
        .iter()
        .map(|r| (r.ac_id.as_str(), r.card.as_deref()))
        .collect();
    assert_eq!(
        labelled,
        [
            ("ac1", Some("c1")),
            ("ac2", Some("c2")),
            ("ac9", Some("c2")),
            ("ac3", Some("c3"))
        ]
    );

    // Kartu 3 tidak terbaca: buku AC tidak berubah (tidak ada baris Removed palsu).
    lark.records.lock().unwrap().remove("c3");
    let r = st.svc.refresh_ledger(&run.id).await.unwrap();
    assert!(r.error.is_some());
    assert_eq!(st.svc.ledger(&run.id).await.unwrap().len(), 4);
    assert!(
        st.svc
            .ledger(&run.id)
            .await
            .unwrap()
            .iter()
            .all(|r| r.status != mandor_domain::RowStatus::Removed)
    );
}

/// Grup tanpa plan dengan giliran kartu aktif selesai (`AwaitingDev`).
async fn working(lark: &FakeLark, clock: &FakeClock, cards: &[&str]) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    let run = start_with(&st, cards, &Planning::Skip).await.unwrap();
    st.svc.sync().drain_outbox(20).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    (st, run.id)
}

fn dirty(st: &Station, diff: &str) {
    let wt = st.runs.only().worktree.unwrap();
    st.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/a.rs".to_owned()],
            diff: diff.to_owned(),
        },
    );
}

fn card_states(cards: &[GroupCardRow]) -> Vec<(&str, &CardState)> {
    cards
        .iter()
        .map(|c| (c.record_id.as_str(), &c.state))
        .collect()
}

#[tokio::test]
async fn next_card_commit_lokal_lalu_kartu_berikutnya() {
    let (lark, clock) = setup();
    let (st, run_id) = working(&lark, &clock, &["c1", "c2", "c3"]).await;
    dirty(&st, "+fn daftar(){}");

    st.svc.next_card(&run_id).await.unwrap();
    let commits = st.git.st().commits.clone();
    assert_eq!(commits.len(), 1);
    assert!(commits[0].1.starts_with("PGS-1401: "), "{}", commits[0].1);
    assert!(st.git.st().pushes.is_empty(), "commit lokal, tanpa push");

    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert_eq!(
        card_states(&cards),
        [
            ("c1", &CardState::Done),
            ("c2", &CardState::Active),
            ("c3", &CardState::Pending)
        ]
    );
    assert_eq!(cards[0].commits.len(), 1);
    let req = st.agent.last().req;
    assert!(
        matches!(req.session, SessionRef::Resume(_)),
        "sesi yang sama"
    );
    assert!(req.message.contains("Kartu job"));
    assert!(req.message.contains("card 2 of 3"));
    assert_eq!(st.runs.only().state, RunState::Running);
}

#[tokio::test]
async fn next_card_hanya_saat_menunggu_dev_di_fase_implementasi() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, &["c1", "c2"]).await.unwrap();
    let err = st.svc.next_card(&run.id).await.unwrap_err();
    assert!(
        matches!(err, RunError::InvalidState(_)),
        "masih bekerja: {err:?}"
    );
    st.agent.last().finish("Plan kartu 1");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let err = st.svc.next_card(&run.id).await.unwrap_err();
    assert!(
        matches!(err, RunError::InvalidState(_)),
        "plan belum disetujui: {err:?}"
    );
}

#[tokio::test]
async fn kartu_berikutnya_dengan_plan_bila_grup_memakai_plan() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let run = start(&st, &["c1", "c2"]).await.unwrap();
    st.svc.sync().drain_outbox(20).await.unwrap();
    st.agent.last().finish("Plan kartu 1");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.svc.approve_plan(&run.id, &[]).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    st.svc.next_card(&run.id).await.unwrap();
    let r = st.runs.only();
    assert_eq!(r.phase, RunPhase::Plan, "kartu 2 diawali plan");
    assert!(st.agent.last().req.plan_mode);
    let cards = st.svc.group_cards(&run.id).await.unwrap();
    assert_eq!(cards[0].plan.as_deref(), Some("Plan kartu 1"));
}

#[tokio::test]
async fn kartu_terakhir_selesai_menunggu_push() {
    let (lark, clock) = setup();
    let (st, run_id) = working(&lark, &clock, &["c1", "c2"]).await;
    st.svc.next_card(&run_id).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let before = st.agent.count();
    st.svc.next_card(&run_id).await.unwrap();
    assert_eq!(st.agent.count(), before, "tidak ada giliran baru");
    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert!(cards.iter().all(|c| c.state == CardState::Done));
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
    assert!(st.runs.only().note.unwrap().contains("Push"));
}

#[tokio::test]
async fn kartu_berubah_di_lark_dikeluarkan_dan_grup_berhenti() {
    // Covers AE4 (laptop).
    let (lark, clock) = setup();
    let (st, run_id) = working(&lark, &clock, &["c1", "c2", "c3"]).await;
    lark.edit("c2", |t| {
        t.status = TaskStatus::Unknown("Blocked".to_owned());
    });
    let before = st.agent.count();

    st.svc.next_card(&run_id).await.unwrap();
    assert_eq!(st.agent.count(), before, "grup berhenti");
    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert_eq!(
        card_states(&cards),
        [
            ("c1", &CardState::Done),
            ("c2", &CardState::Removed(RemovedReason::ChangedInLark)),
            ("c3", &CardState::Pending)
        ]
    );
    assert_eq!(lark.get("c2").unwrap().claim, None, "klaim dilepas");
    assert_eq!(
        lark.get("c2").unwrap().status,
        TaskStatus::Unknown("Blocked".to_owned()),
        "status tidak diubah"
    );
    let run = st.runs.only();
    assert_eq!(run.state, RunState::AwaitingDev);
    assert!(run.note.unwrap().contains("PGS-1402"));

    // Dev memilih lanjut tanpa kartu itu.
    st.svc.continue_group(&run_id).await.unwrap();
    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert_eq!(cards[2].state, CardState::Active);
    assert!(st.agent.last().req.message.contains("Buat job"));
}

#[tokio::test]
async fn keluarkan_kartu_dan_hentikan_grup_melepas_klaim() {
    let (lark, clock) = setup();
    let (st, run_id) = working(&lark, &clock, &["c1", "c2", "c3"]).await;
    st.svc.remove_card(&run_id, "c3").await.unwrap();
    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert_eq!(cards[2].state, CardState::Removed(RemovedReason::ByDev));
    assert_eq!(lark.get("c3").unwrap().claim, None);
    let err = st.svc.remove_card(&run_id, "c1").await.unwrap_err();
    assert!(
        matches!(err, RunError::InvalidState(_)),
        "kartu aktif: {err:?}"
    );

    dirty(&st, "+fn a(){}");
    st.svc.next_card(&run_id).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.svc.stop_group(&run_id).await.unwrap();
    assert_eq!(
        st.git.st().pushes.len(),
        1,
        "commit kartu selesai di-push tanpa PR"
    );
    assert!(st.git.st().created_prs.is_empty());
    for id in ["c1", "c2", "c3"] {
        assert_eq!(lark.get(id).unwrap().claim, None, "{id}");
    }
    assert!(st.svc.active_runs().await.unwrap().is_empty());
}

#[tokio::test]
async fn heartbeat_pic_berubah_di_kartu_lain_tidak_menghentikan_grup() {
    let (lark, clock) = setup();
    let (st, run_id) = working(&lark, &clock, &["c1", "c2", "c3"]).await;
    let before = lark.get("c2").unwrap().claim.unwrap().ts_ms;
    lark.edit("c3", |t| t.assignees = vec!["ou_lain".to_owned()]);
    clock.advance(6 * 60 * 1000);
    lark.clock.store(10 * DAY + 6 * 60 * 1000, Ordering::SeqCst);
    st.svc.heartbeat_all().await.unwrap();
    assert_eq!(st.runs.only().state, RunState::AwaitingDev, "grup tetap");
    assert!(
        lark.get("c2").unwrap().claim.unwrap().ts_ms > before,
        "klaim kartu yang belum dimulai ikut diperbarui"
    );
    // Kartu aktif kehilangan PIC: grup berhenti seperti run tunggal.
    lark.edit("c1", |t| t.assignees = vec!["ou_lain".to_owned()]);
    clock.advance(6 * 60 * 1000);
    st.svc.heartbeat_all().await.unwrap();
    let _ = &run_id;
    assert_eq!(st.runs.only().state, RunState::Stopped);
}

/// Grup tanpa plan dengan semua kartu selesai, satu commit per kartu.
async fn finished(lark: &FakeLark, clock: &FakeClock, cards: &[&str]) -> (Station, String) {
    let (st, run_id) = working(lark, clock, cards).await;
    for i in 0..cards.len() {
        dirty(&st, &format!("+fn kartu{i}(){{}}"));
        st.svc.next_card(&run_id).await.unwrap();
        if i + 1 < cards.len() {
            st.agent.last().done();
            eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
        }
    }
    (st, run_id)
}

/// Semua baris buku AC run diberi verdict, kecuali `ac_id` di `fails`.
fn verdicts(st: &Station, run: &str, fails: &[&str]) {
    let mut ledgers = st.reviews.ledgers.lock().unwrap();
    for r in ledgers.get_mut(run).unwrap().iter_mut() {
        r.verdict = if fails.contains(&r.ac_id.as_str()) {
            Verdict::Fail
        } else {
            Verdict::Pass
        };
        r.stale = false;
    }
}

fn blocked(lark: &FakeLark, id: &str) {
    lark.edit(id, |t| t.status = TaskStatus::Unknown("Blocked".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae2_satu_pr_dan_semua_kartu_waiting_review_bersama() {
    let (lark, clock) = setup();
    let (st, run_id) = finished(&lark, &clock, &["c1", "c2", "c3"]).await;
    verdicts(&st, &run_id, &[]);

    let out = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    let PublishOutcome::Published { url, .. } = out else {
        panic!("{out:?}")
    };
    let created = st.git.st().created_prs.clone();
    assert_eq!(created.len(), 1, "satu PR");
    assert_eq!(created[0].1, "PGS-1401–PGS-1403: Job & Position");
    let body = st.git.st().pr_bodies.last().cloned().unwrap();
    for key in ["PGS-1401", "PGS-1402", "PGS-1403"] {
        assert!(body.contains(key), "{key} di deskripsi PR:\n{body}");
    }

    st.svc.sync().drain_outbox(20).await.unwrap();
    for id in ["c1", "c2", "c3"] {
        let t = lark.get(id).unwrap();
        assert_eq!(t.status, TaskStatus::WaitingReview, "{id}");
        assert_eq!(t.pr_url.as_deref(), Some(url.as_str()), "{id}");
    }
    assert_eq!(st.runs.only().state, RunState::InReviewLoop);
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_grup_menunggu_semua_kartu_selesai() {
    let (lark, clock) = setup();
    let (st, run_id) = working(&lark, &clock, &["c1", "c2"]).await;
    st.svc.next_card(&run_id).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let err = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap_err();
    assert!(matches!(err, RunError::InvalidState(_)), "{err:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae8_kartu_berubah_menahan_gerbang_lalu_tetap_dengan_alasan() {
    let (lark, clock) = setup();
    let (st, run_id) = finished(&lark, &clock, &["c1", "c2", "c3"]).await;
    verdicts(&st, &run_id, &[]);
    blocked(&lark, "c1");

    let out = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    assert_eq!(
        out,
        PublishOutcome::CardsChanged {
            cards: vec!["PGS-1401".to_owned()]
        }
    );
    assert!(st.git.st().pushes.is_empty(), "PR tidak berjalan");
    assert!(
        st.reviews.history.lock().unwrap().is_empty(),
        "review tidak berjalan"
    );
    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert!(cards[0].changed_in_lark);

    let err = st
        .svc
        .gate_card_decision(
            &run_id,
            "c1",
            &CardDecision::Keep {
                reason: " ".to_owned(),
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, RunError::InvalidState(_)),
        "alasan wajib: {err:?}"
    );
    let got = st
        .svc
        .gate_card_decision(
            &run_id,
            "c1",
            &CardDecision::Keep {
                reason: "QA masih butuh kodenya".to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(got, CardOutcome::Kept);

    let out = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    assert!(matches!(out, PublishOutcome::Published { .. }), "{out:?}");
    let body = st.git.st().pr_bodies.last().cloned().unwrap();
    assert!(body.contains("QA masih butuh kodenya"), "{body}");

    st.svc.sync().drain_outbox(20).await.unwrap();
    assert_eq!(
        lark.get("c1").unwrap().status,
        TaskStatus::Unknown("Blocked".to_owned()),
        "kartu yang berubah tidak dipindah"
    );
    for id in ["c2", "c3"] {
        assert_eq!(
            lark.get(id).unwrap().status,
            TaskStatus::WaitingReview,
            "{id}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn revert_kartu_utama_memindah_run_dan_meminta_review_ulang() {
    let (lark, clock) = setup();
    let (st, run_id) = finished(&lark, &clock, &["c1", "c2", "c3"]).await;
    verdicts(&st, &run_id, &[]);
    blocked(&lark, "c1");
    let sha = st.svc.group_cards(&run_id).await.unwrap()[0].commits[0]
        .sha
        .clone();

    st.svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    let got = st
        .svc
        .gate_card_decision(&run_id, "c1", &CardDecision::Revert)
        .await
        .unwrap();
    assert_eq!(got, CardOutcome::Reverted);
    assert_eq!(st.git.st().reverts, [sha]);

    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert_eq!(cards[0].state, CardState::Removed(RemovedReason::Reverted));
    assert_eq!(lark.get("c1").unwrap().claim, None, "klaim dilepas");
    assert_eq!(st.runs.only().record_id, "c2", "kartu utama pindah");
    assert!(
        st.svc
            .ledger(&run_id)
            .await
            .unwrap()
            .iter()
            .all(|r| r.stale),
        "kode berubah: verdict usang"
    );
    let out = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    assert!(
        matches!(out, PublishOutcome::ReviewStarted { .. }),
        "{out:?}"
    );

    // Kartu yang di-revert bisa dimulai lagi sebagai run tunggal.
    lark.edit("c1", |t| t.status = TaskStatus::ToDo);
    st.svc.sync().poll(true, &[]).await.unwrap();
    assert!(st.svc.start("c1", "repo1", None).await.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn judul_pr_memakai_kartu_aktif_setelah_revert() {
    let (lark, clock) = setup();
    let (st, run_id) = finished(&lark, &clock, &["c1", "c2", "c3"]).await;
    blocked(&lark, "c1");
    st.svc
        .gate_card_decision(&run_id, "c1", &CardDecision::Revert)
        .await
        .unwrap_err();
    st.svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    st.svc
        .gate_card_decision(&run_id, "c1", &CardDecision::Revert)
        .await
        .unwrap();
    verdicts(&st, &run_id, &[]);
    let out = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    assert!(matches!(out, PublishOutcome::Published { .. }), "{out:?}");
    assert_eq!(
        st.git.st().created_prs[0].1,
        "PGS-1402–PGS-1403: Job & Position"
    );
    let body = st.git.st().pr_bodies.last().cloned().unwrap();
    assert!(
        body.contains("PGS-1401") && body.contains("reverted"),
        "{body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn revert_konflik_menahan_gerbang() {
    let (lark, clock) = setup();
    let (st, run_id) = finished(&lark, &clock, &["c1", "c2"]).await;
    verdicts(&st, &run_id, &[]);
    blocked(&lark, "c1");
    let sha = st.svc.group_cards(&run_id).await.unwrap()[0].commits[0]
        .sha
        .clone();
    st.git.state.lock().unwrap().revert_conflicts.push(sha);
    st.svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();

    let got = st
        .svc
        .gate_card_decision(&run_id, "c1", &CardDecision::Revert)
        .await
        .unwrap();
    assert_eq!(got, CardOutcome::RevertConflict);
    assert!(st.git.st().reverts.is_empty());
    let cards = st.svc.group_cards(&run_id).await.unwrap();
    assert_eq!(cards[0].state, CardState::Done);
    assert!(cards[0].changed_in_lark);
    assert!(st.runs.only().note.unwrap().contains("revert conflict"));
    let out = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    assert!(
        matches!(out, PublishOutcome::CardsChanged { .. }),
        "{out:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ac_gagal_di_satu_kartu_menahan_grup_dengan_label_kartu() {
    let (lark, clock) = setup();
    let (st, run_id) = finished(&lark, &clock, &["c1", "c2", "c3"]).await;
    verdicts(&st, &run_id, &["ac3"]);
    let out = st
        .svc
        .publish_gated(&run_id, &GateDecision::default())
        .await
        .unwrap();
    assert_eq!(
        out,
        PublishOutcome::Blocked {
            failed: vec!["PGS-1403 AC-3".to_owned()]
        }
    );
    let anyway = GateDecision {
        push_anyway_reason: Some("Di luar cakupan sprint".to_owned()),
        decided_by: Some("Beni".to_owned()),
        ..GateDecision::default()
    };
    let out = st.svc.publish_gated(&run_id, &anyway).await.unwrap();
    assert!(matches!(out, PublishOutcome::Published { .. }), "{out:?}");
    assert_eq!(st.git.st().created_prs.len(), 1);
}

fn ws_repo(id: &str) -> mandor_usecase::run_ports::RepoInfo {
    mandor_usecase::run_ports::RepoInfo {
        id: id.to_owned(),
        path: std::path::PathBuf::from(format!("/src/{id}")),
        default_branch: "main".to_owned(),
        gh_profile: None,
        companions: vec![],
        trusted_reviewers: vec![],
        extra_allowed: vec![],
        app: mandor_domain::app_run::AppSettings::default(),
        gate_mode: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn grup_workspace_satu_pr_per_repo_dan_tautan_sama_di_semua_kartu() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.repos.add(ws_repo("kelola-api"));
    st.repos.add(ws_repo("kelola-web"));
    st.repos
        .add_workspace(mandor_usecase::run_ports::WorkspaceInfo {
            id: "kelola-v2".to_owned(),
            name: "kelola-v2".to_owned(),
            root: std::path::PathBuf::from("/src/kelola-v2"),
            repo_ids: vec!["kelola-api".to_owned(), "kelola-web".to_owned()],
            mcp_servers: vec![],
            browser_tests: false,
            gate_mode: None,
        });
    let target = StartTarget::Workspace {
        id: "kelola-v2".to_owned(),
        repo_ids: vec!["kelola-api".to_owned(), "kelola-web".to_owned()],
    };
    let run = st
        .svc
        .start_group(
            &ids(&["c1", "c2"]),
            &target,
            None,
            &Planning::Skip,
            &[],
            |_| {},
        )
        .await
        .unwrap();
    st.svc.sync().drain_outbox(20).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    // Kartu 1 mengubah kedua repo, kartu 2 hanya web.
    let r = st.runs.only();
    let api = r.worktree.clone().unwrap();
    let web = r.extra[0].worktree.clone();
    let change = |wt: &std::path::Path| {
        st.git.set_changes(
            wt,
            WorkChanges {
                dirty: true,
                unpushed: 0,
                files: vec!["src/a.rs".to_owned()],
                diff: "+a".to_owned(),
            },
        );
    };
    change(&api);
    change(&web);
    st.svc.next_card(&run.id).await.unwrap();
    assert_eq!(
        st.svc.group_cards(&run.id).await.unwrap()[0].commits.len(),
        2,
        "satu commit per repo"
    );
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    change(&web);
    st.svc.next_card(&run.id).await.unwrap();

    verdicts(&st, &run.id, &[]);
    let out = st
        .svc
        .publish_gated(&run.id, &GateDecision::default())
        .await
        .unwrap();
    let PublishOutcome::Published { url, .. } = out else {
        panic!("{out:?}")
    };
    assert_eq!(st.git.st().created_prs.len(), 2, "satu PR per repo");
    assert_eq!(url.lines().count(), 2);
    st.svc.sync().drain_outbox(20).await.unwrap();
    for id in ["c1", "c2"] {
        let t = lark.get(id).unwrap();
        assert_eq!(t.status, TaskStatus::WaitingReview, "{id}");
        assert_eq!(t.pr_url.as_deref(), Some(url.as_str()), "{id}");
    }
}
