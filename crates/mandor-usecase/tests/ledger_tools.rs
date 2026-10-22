//! Skenario U5–U6 Rilis 2: token kapabilitas, operasi ledger, dan sumber desain.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{Design, RunState, Verdict};
use mandor_usecase::review::{CapabilityRole, DesignRoot, LedgerToolError};
use mandor_usecase::{AcItem, Planning, StartTarget};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned()];
    t.kanban_mirror = Some("todo".to_owned());
    t.last_modified_ms = 10 * DAY;
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
    (lark, clock)
}

fn mock_design() -> DesignRoot {
    DesignRoot {
        id: "ds1".to_owned(),
        name: "kelola-mock".to_owned(),
        path: "/design/kelola-mock".into(),
    }
}

async fn plan_run(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.svc
        .start_planned(
            "rec1",
            &StartTarget::Repo("repo1".to_owned()),
            None,
            &Planning::Plan { skill: None },
            &[mock_design()],
            &|_| {},
        )
        .await
        .unwrap();
    let id = st.runs.only().id;
    (st, id)
}

#[tokio::test]
async fn giliran_plan_mendapat_token_design_yang_dicabut_saat_giliran_selesai() {
    let (lark, clock) = setup();
    let (st, run) = plan_run(&lark, &clock).await;
    let access = st
        .agent
        .last()
        .req
        .ledger
        .expect("giliran plan memakai ledger");
    assert_eq!(access.role, CapabilityRole::Design);
    let token = access.token.expose().to_owned();
    let scope = st.svc.verify_capability(&token).expect("token berlaku");
    assert_eq!(scope.run_id, run);
    assert_eq!(scope.role, CapabilityRole::Design);
    assert!(
        !format!("{access:?}").contains(&token),
        "token tidak tercetak"
    );

    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(st.svc.verify_capability(&token).is_none(), "dicabut");
}

#[tokio::test]
async fn giliran_implement_tidak_mendapat_token() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.svc.start("rec1", "repo1", None).await.unwrap();
    assert!(st.agent.last().req.ledger.is_none());
}

#[tokio::test]
async fn peran_design_mengisi_kolom_design_dan_ditolak_mengisi_verdict() {
    let (lark, clock) = setup();
    let (st, run) = plan_run(&lark, &clock).await;
    let scope = st
        .svc
        .verify_capability(st.agent.last().req.ledger.unwrap().token.expose())
        .unwrap();
    let rows = st
        .svc
        .ledger_set_design(&scope, "ac1", Design::Missing, Some("no reason field"))
        .await
        .unwrap();
    assert_eq!(rows[0].design, Design::Missing);
    assert_eq!(rows[0].design_note.as_deref(), Some("no reason field"));
    assert_eq!(
        st.svc.ledger(&run).await.unwrap()[0].design,
        Design::Missing
    );

    assert_eq!(
        st.svc
            .ledger_set_verdict(&scope, "ac1", Verdict::Pass, Some("ok"))
            .await
            .unwrap_err(),
        LedgerToolError::WrongRole
    );
    assert_eq!(
        st.svc
            .ledger_set_design(&scope, "ac9", Design::InDesign, None)
            .await
            .unwrap_err(),
        LedgerToolError::UnknownAc("ac9".to_owned())
    );
    assert_eq!(
        st.svc
            .ledger_set_design(&scope, "ac1", Design::FollowDesign, None)
            .await
            .unwrap_err(),
        LedgerToolError::NotAllowed("follow_design")
    );
    let read = st.svc.ledger_read(&scope).await.unwrap();
    assert_eq!(read.len(), 2);
}

#[tokio::test]
async fn peran_verdict_hanya_lulus_gagal_atau_tidak_bisa_dipastikan_dan_bukti_dipotong() {
    let (lark, clock) = setup();
    let (st, run) = plan_run(&lark, &clock).await;
    let token = st.svc.issue_capability(&run, CapabilityRole::Verdict);
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    let long = "x".repeat(5_000);
    let rows = st
        .svc
        .ledger_set_verdict(&scope, "ac2", Verdict::Fail, Some(&long))
        .await
        .unwrap();
    let evidence = rows[1].evidence.clone().unwrap();
    assert!(evidence.chars().count() <= 2_020, "{}", evidence.len());
    assert!(evidence.ends_with("[truncated]"));
    assert_eq!(rows[1].verdict, Verdict::Fail);
    for v in [
        Verdict::Skipped,
        Verdict::CheckedManually,
        Verdict::NotReviewed,
    ] {
        assert!(matches!(
            st.svc.ledger_set_verdict(&scope, "ac2", v, None).await,
            Err(LedgerToolError::NotAllowed(_))
        ));
    }
    assert_eq!(
        st.svc
            .ledger_set_design(&scope, "ac1", Design::InDesign, None)
            .await
            .unwrap_err(),
        LedgerToolError::WrongRole
    );
    st.svc.revoke_capabilities(&run);
    assert!(st.svc.verify_capability(token.expose()).is_none());
}

#[tokio::test]
async fn token_salah_atau_kosong_ditolak() {
    let (lark, clock) = setup();
    let (st, _run) = plan_run(&lark, &clock).await;
    assert!(st.svc.verify_capability("").is_none());
    assert!(st.svc.verify_capability("bukan-token").is_none());
}

#[tokio::test]
async fn covers_f1_plan_mengisi_design_dua_dari_tiga_ac_dan_plan_tersimpan() {
    let (lark, clock) = setup();
    let mut t = lark.get("rec1").unwrap();
    t.ac_ids.push("ac3".to_owned());
    lark.put(t);
    lark.acs.lock().unwrap().insert(
        "ac3".to_owned(),
        AcItem {
            record_id: "ac3".to_owned(),
            label: Some("AC-ac3".to_owned()),
            text: "teks ac3".to_owned(),
            ..Default::default()
        },
    );
    let (st, run) = plan_run(&lark, &clock).await;
    let req = st.agent.last().req;
    assert_eq!(req.design, [mock_design()]);
    assert!(
        req.message.contains("DESIGN kelola-mock"),
        "{}",
        req.message
    );
    assert_eq!(st.runs.only().design, [mock_design()]);
    let scope = st
        .svc
        .verify_capability(req.ledger.unwrap().token.expose())
        .unwrap();
    st.svc
        .ledger_set_design(&scope, "ac1", Design::InDesign, Some("Data tab"))
        .await
        .unwrap();
    st.svc
        .ledger_set_design(
            &scope,
            "ac2",
            Design::Conflicts,
            Some("design hides the button"),
        )
        .await
        .unwrap();
    st.agent.last().finish("## Plan\n1. Kerjakan");
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    let rows = st.svc.ledger(&run).await.unwrap();
    let design: Vec<_> = rows.iter().map(|r| r.design).collect();
    assert_eq!(
        design,
        [Design::InDesign, Design::Conflicts, Design::NotChecked]
    );
    assert_eq!(st.runs.only().plan.as_deref(), Some("## Plan\n1. Kerjakan"));
}

#[tokio::test]
async fn start_tanpa_sumber_desain_tidak_meminta_design() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.svc
        .start_planned(
            "rec1",
            &StartTarget::Repo("repo1".to_owned()),
            None,
            &Planning::Plan { skill: None },
            &[],
            &|_| {},
        )
        .await
        .unwrap();
    let req = st.agent.last().req;
    assert!(req.ledger.is_none(), "tanpa sumber, Design tidak diisi");
    assert!(req.design.is_empty());
    assert!(!req.message.contains("DESIGN"), "{}", req.message);
    assert!(st.runs.only().design.is_empty());
}
