//! Agent pengusul perlengkapan lab (plan 2026-11-10-001, U8, KTD-L14).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, station};
use mandor_domain::app_run::{AppSettings, TestMode};
use mandor_domain::lab::{LabAccount, LabEquipment, LoginForm, SuccessMarker};
use mandor_usecase::RunError;
use mandor_usecase::review::{
    CapabilityRole, LAB_SETUP_BUDGET_CENTS, LAB_SETUP_TIMEOUT, LabSetupStatus, LedgerToolError,
};
use pretty_assertions::assert_eq;

fn web_repo(st: &Station) {
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        port: Some(5173),
        browser_tests: true,
        ..AppSettings::default()
    };
}

fn proposal() -> LabEquipment {
    LabEquipment {
        accounts: vec![LabAccount {
            role: "admin".to_owned(),
            username: "admin@uji.test".to_owned(),
            default: true,
        }],
        login: LoginForm {
            url: "http://localhost:5173/login".to_owned(),
            username_field: "#email".to_owned(),
            password_field: "#password".to_owned(),
            submit: "text=Sign in".to_owned(),
            submit_path: "/api/login".to_owned(),
            success: SuccessMarker::PathPrefix("/admin".to_owned()),
            authenticated_url: "http://localhost:5173/admin".to_owned(),
        },
        safe_posts: vec![],
        fixtures_dir: Some("review-fixtures".to_owned()),
        data: None,
    }
}

/// Lingkup token giliran pengusul yang sedang berjalan.
fn setup_scope(st: &Station) -> mandor_usecase::review::CapabilityScope {
    let access = st.agent.last().req.ledger.unwrap();
    st.svc.verify_capability(access.token.expose()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn giliran_pengusul_berjalan_di_salinan_bersih_dengan_batas_sendiri() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    web_repo(&st);
    st.svc.propose_lab("repo1").await.unwrap();
    let req = st.agent.last().req;
    // Salinan bersih branch default, bukan checkout developer (KTD-L14).
    assert_eq!(
        req.worktree,
        std::path::PathBuf::from("/wt/lab-setup-repo1")
    );
    assert_eq!(req.message, "LAB SETUP http://localhost:5173");
    assert_eq!(req.ledger.as_ref().unwrap().role, CapabilityRole::LabSetup);
    let review = req.review.unwrap();
    assert!(!review.browser);
    assert!(review.proxy.is_none() && review.sessions.is_empty());
    assert_eq!(review.budget_cents, LAB_SETUP_BUDGET_CENTS);
    assert_eq!(review.timeout, LAB_SETUP_TIMEOUT);
    assert!(review.lab.unwrap().setup);
    assert!(st.svc.lab_setup_status("repo1").running);
    // Satu giliran per repo.
    assert!(matches!(
        st.svc.propose_lab("repo1").await,
        Err(RunError::InvalidState(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn usulan_valid_menjadi_draf_tanpa_persetujuan() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    web_repo(&st);
    st.svc.propose_lab("repo1").await.unwrap();
    let scope = setup_scope(&st);
    st.svc.ledger_lab_propose(&scope, proposal()).await.unwrap();
    // Usulan kedua setelah diterima ditolak.
    assert!(matches!(
        st.svc.ledger_lab_propose(&scope, proposal()).await,
        Err(LedgerToolError::NotAllowed(_))
    ));
    st.agent.last().done();
    eventually(|| !st.svc.lab_setup_status("repo1").running).await;
    assert_eq!(
        st.svc.lab_setup_status("repo1"),
        LabSetupStatus {
            running: false,
            proposal: Some(proposal()),
            error: None,
        }
    );
    // Draf tidak menyentuh perlengkapan tersimpan maupun persetujuan.
    assert!(st.lab.equipment.lock().unwrap().is_empty());
    // Token dicabut dan berkas sesi dibersihkan.
    assert!(
        st.svc
            .verify_capability(st.agent.last().req.ledger.unwrap().token.expose())
            .is_none()
    );
    let review_id = st.agent.last().req.review.unwrap().review_id;
    assert!(st.agent.ended_reviews.lock().unwrap().contains(&review_id));
    // Salinan bersih dihapus.
    assert!(
        st.git
            .state
            .lock()
            .unwrap()
            .removed_roots
            .contains(&std::path::PathBuf::from("/wt/lab-setup-repo1"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn usulan_tidak_valid_ditolak_dan_boleh_dikirim_ulang_sekali() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    web_repo(&st);
    st.svc.propose_lab("repo1").await.unwrap();
    let scope = setup_scope(&st);
    let mut bad = proposal();
    bad.login.url = "https://staging.example.com/login".to_owned();
    let first = st.svc.ledger_lab_propose(&scope, bad.clone()).await;
    let Err(LedgerToolError::Invalid(msg)) = first else {
        panic!("{first:?}");
    };
    assert!(msg.contains("once more"), "{msg}");
    let second = st.svc.ledger_lab_propose(&scope, bad).await;
    let Err(LedgerToolError::Invalid(msg)) = second else {
        panic!("{second:?}");
    };
    assert!(msg.contains("no attempts left"), "{msg}");
    // Kiriman ketiga, walau valid, tidak lagi diterima.
    assert!(matches!(
        st.svc.ledger_lab_propose(&scope, proposal()).await,
        Err(LedgerToolError::NotAllowed(_))
    ));
    st.agent.last().done();
    eventually(|| !st.svc.lab_setup_status("repo1").running).await;
    let status = st.svc.lab_setup_status("repo1");
    assert_eq!(status.proposal, None);
    assert!(status.error.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn usulan_ulang_dengan_perbaikan_diterima() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    web_repo(&st);
    st.svc.propose_lab("repo1").await.unwrap();
    let scope = setup_scope(&st);
    let mut bad = proposal();
    bad.accounts[0].role = "Admin Role".to_owned();
    assert!(st.svc.ledger_lab_propose(&scope, bad).await.is_err());
    st.svc.ledger_lab_propose(&scope, proposal()).await.unwrap();
    assert_eq!(st.svc.lab_setup_status("repo1").proposal, Some(proposal()));
}

#[tokio::test(flavor = "multi_thread")]
async fn peran_lain_tidak_bisa_mengusulkan() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    web_repo(&st);
    let token = st
        .svc
        .issue_capability("lab-setup/repo1", CapabilityRole::Verdict);
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    assert_eq!(
        st.svc.ledger_lab_propose(&scope, proposal()).await,
        Err(LedgerToolError::WrongRole)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn repo_tanpa_url_app_tidak_bisa_diusulkan() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    assert!(matches!(
        st.svc.propose_lab("repo1").await,
        Err(RunError::InvalidState(_))
    ));
    assert_eq!(st.agent.count(), 0);
    assert!(matches!(
        st.svc.propose_lab("tidak-ada").await,
        Err(RunError::NotFound(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_gagal_dimulai_meninggalkan_status_galat() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    web_repo(&st);
    *st.agent.fail_spawn.lock().unwrap() = true;
    assert!(st.svc.propose_lab("repo1").await.is_err());
    let status = st.svc.lab_setup_status("repo1");
    assert!(!status.running);
    assert!(status.error.is_some());
    // Bisa dicoba lagi.
    *st.agent.fail_spawn.lock().unwrap() = false;
    st.svc.propose_lab("repo1").await.unwrap();
    assert!(st.svc.lab_setup_status("repo1").running);
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_berhenti_di_batas_biaya_tercatat() {
    let st = station(&FakeLark::default(), &FakeClock::default(), "ou_me");
    web_repo(&st);
    st.svc.propose_lab("repo1").await.unwrap();
    st.agent.last().budget_stop();
    eventually(|| !st.svc.lab_setup_status("repo1").running).await;
    let error = st.svc.lab_setup_status("repo1").error.unwrap();
    assert!(error.contains("budget"), "{error}");
}
