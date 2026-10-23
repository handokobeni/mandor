//! Run grup kartu satu flow (plan `2026-10-23-004`): mulai, pindah kartu,
//! berhenti, gerbang, dan PR.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::group::CardState;
use mandor_domain::{AppField, Claim, RunState, TaskStatus, WriteOp};
use mandor_usecase::run_ports::RunPhase;
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
