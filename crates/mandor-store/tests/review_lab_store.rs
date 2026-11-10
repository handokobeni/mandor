//! Penyimpanan review lab (plan 2026-11-10-001, U2).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::lab::{
    BlockedRequest, Evidence, Finding, LabAccount, LabEquipment, LabNeed, LoginForm, LookAdvice,
    LookRecommendation, Simulated, SuccessMarker,
};
use mandor_domain::{AcRow, RowStatus, Verdict};
use mandor_store::{NewRepo, ReviewsStore, RunsStore, Store, StoredReview};
use mandor_usecase::run_ports::{
    LabSettings, ReviewHistory, ReviewRecord, ReviewStatus, ReviewTrigger, RunRecord, RunStore,
};
use pretty_assertions::assert_eq;

async fn open(dir: &tempfile::TempDir) -> Store {
    Store::open(&dir.path().join("mandor.db")).await.unwrap()
}

async fn with_run(s: &Store, id: &str) {
    let _ = s
        .insert_repo(&NewRepo {
            id: "repo1".into(),
            path: "/src/app".into(),
            gh_profile: None,
            default_branch: Some("main".into()),
            now_ms: 1,
        })
        .await;
    let r = RunRecord::new(id.to_owned(), "repo1".to_owned(), format!("rec-{id}"));
    RunsStore::new(s.clone()).create(&r, 1).await.unwrap();
}

fn evidence() -> Evidence {
    Evidence {
        steps: vec!["Buka /admin".to_owned(), "Klik Save".to_owned()],
        screenshots: vec!["admin-1.png".to_owned()],
        prepared: Some("akun admin, fetch /api/x digagalkan 500".to_owned()),
        simulated: Simulated::Request,
        blocked_requests: vec![BlockedRequest {
            method: "POST".to_owned(),
            url: "http://localhost:3000/api/x".to_owned(),
        }],
        lab: true,
    }
}

fn equipment() -> LabEquipment {
    LabEquipment {
        accounts: vec![LabAccount {
            role: "admin".to_owned(),
            username: "admin@uji.test".to_owned(),
            default: true,
        }],
        login: LoginForm {
            url: "http://localhost:3000/login".to_owned(),
            username_field: "#email".to_owned(),
            password_field: "#password".to_owned(),
            submit: "text=Sign in".to_owned(),
            submit_path: "/api/login".to_owned(),
            success: SuccessMarker::Text("Dashboard".to_owned()),
            authenticated_url: "http://localhost:3000/admin".to_owned(),
        },
        safe_posts: vec!["/api/search".to_owned()],
        fixtures_dir: Some("review-fixtures".to_owned()),
    }
}

#[tokio::test]
async fn bukti_lab_dan_rekomendasi_tersimpan_di_buku_ac() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    let mut cv = AcRow::new("1", Some("AC-1"), "teks", RowStatus::Active);
    cv.verdict = Verdict::CantVerify;
    cv.lab_need = Some(LabNeed::Role);
    let mut pass = AcRow::new("2", Some("AC-2"), "teks", RowStatus::Active);
    pass.verdict = Verdict::Pass;
    pass.evidence_detail = Some(evidence());
    pass.look_recommendation = Some(LookRecommendation {
        advice: LookAdvice::Accept,
        reason: "AC menulis Key Behavior".to_owned(),
    });
    s.save_ac_rows("run1", &[cv.clone(), pass.clone()])
        .await
        .unwrap();
    assert_eq!(s.ac_rows("run1").await.unwrap(), vec![cv, pass.clone()]);
    let active = s.active_ac_rows().await.unwrap();
    assert_eq!(active[1].1, pass);
}

#[tokio::test]
async fn bukti_lab_tersalin_ke_verdict_review() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    let mut row = AcRow::new("1", Some("AC-1"), "teks", RowStatus::Active);
    row.verdict = Verdict::CantVerify;
    row.lab_need = Some(LabNeed::Data);
    row.evidence_detail = Some(evidence());
    let record = ReviewRecord {
        id: "rv1".to_owned(),
        run_id: "run1".to_owned(),
        trigger: ReviewTrigger::Manual,
        status: ReviewStatus::Finished,
        browser_used: true,
        browser_reason: None,
        error: None,
        cost_usd: None,
        usage: mandor_domain::Usage::default(),
        stale: false,
        skill: None,
        started_at_ms: 1,
        finished_at_ms: Some(2),
        verdicts: vec![row],
    };
    let reviews = ReviewsStore::new(s.clone());
    reviews.save_review(&record).await.unwrap();
    let got = reviews.reviews("run1").await.unwrap();
    assert_eq!(got[0].verdicts[0].lab_need, Some(LabNeed::Data));
    assert_eq!(got[0].verdicts[0].evidence_detail, Some(evidence()));
}

#[tokio::test]
async fn simpan_perlengkapan_mengosongkan_persetujuan() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    assert_eq!(s.repo_lab("repo1").await.unwrap(), None);
    let eq = equipment();
    s.save_repo_lab("repo1", &eq).await.unwrap();
    let got = s.repo_lab("repo1").await.unwrap().unwrap();
    assert_eq!(got.equipment, eq);
    assert_eq!(got.approved_digest, None);

    assert!(s.approve_repo_lab("repo1", &eq, "abc", 10).await.unwrap());
    let got = s.repo_lab("repo1").await.unwrap().unwrap();
    assert_eq!(got.approved_digest.as_deref(), Some("abc"));
    assert_eq!(got.approved_at_ms, Some(10));

    let mut changed = eq.clone();
    changed.safe_posts.push("/api/other".to_owned());
    s.save_repo_lab("repo1", &changed).await.unwrap();
    let got = s.repo_lab("repo1").await.unwrap().unwrap();
    assert_eq!(got.approved_digest, None);
    assert_eq!(got.approved_at_ms, None);
}

#[tokio::test]
async fn persetujuan_ditolak_bila_perlengkapan_berubah_sejak_dilihat() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    let eq = equipment();
    s.save_repo_lab("repo1", &eq).await.unwrap();
    let mut changed = eq.clone();
    changed.fixtures_dir = None;
    s.save_repo_lab("repo1", &changed).await.unwrap();
    assert!(!s.approve_repo_lab("repo1", &eq, "abc", 10).await.unwrap());
    assert_eq!(
        s.repo_lab("repo1").await.unwrap().unwrap().approved_digest,
        None
    );
}

#[tokio::test]
async fn temuan_tersimpan_per_review_dan_terbaca_per_run() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    for (id, at) in [("rv1", 1), ("rv2", 5)] {
        s.save_review(&StoredReview {
            id: id.to_owned(),
            run_id: "run1".to_owned(),
            trigger: "manual".to_owned(),
            status: "finished".to_owned(),
            browser_used: true,
            error: None,
            cost_usd: None,
            usage_json: "{}".to_owned(),
            started_at_ms: at,
            finished_at_ms: None,
        })
        .await
        .unwrap();
    }
    let finding = Finding {
        card: Some("k1".to_owned()),
        title: "Tombol Save tidak aktif".to_owned(),
        detail: "Setelah upload, Save tetap disabled".to_owned(),
        evidence: evidence(),
    };
    s.save_finding("rv1", &finding, 3).await.unwrap();
    let other = Finding {
        card: None,
        title: "Typo".to_owned(),
        detail: "Recieve".to_owned(),
        evidence: Evidence::default(),
    };
    s.save_finding("rv2", &other, 6).await.unwrap();
    let got = s.run_findings("run1").await.unwrap();
    assert_eq!(
        got,
        vec![("rv1".to_owned(), finding), ("rv2".to_owned(), other)]
    );
}

#[tokio::test]
async fn setelan_lab_bawaan_lalu_tersimpan() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    let reviews = ReviewsStore::new(s);
    let def = reviews.lab_settings().await;
    assert_eq!(def, LabSettings::default());
    assert_eq!(def.budget_cents, 300);
    assert_eq!(def.timeout_secs, 1200);
    assert_eq!(def.run_budget_cents, 1500);
    assert_eq!(def.run_timeout_secs, 5400);
    let custom = LabSettings {
        model: Some("claude-sonnet-5-5".to_owned()),
        budget_cents: 100,
        timeout_secs: 600,
        run_budget_cents: 900,
        run_timeout_secs: 3600,
    };
    reviews.set_lab_settings(&custom).await.unwrap();
    assert_eq!(reviews.lab_settings().await, custom);
}
