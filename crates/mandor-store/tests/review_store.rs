//! Penyimpanan Rilis 2: buku AC, riwayat review, sumber desain, setelan.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::{AcRow, Design, RowStatus, Verdict};
use mandor_store::{
    DesignSourceRow, NewRepo, NewWorkspace, RunsStore, Store, StoreError, StoredReview,
    StoredVerdict,
};
use mandor_usecase::run_ports::{RunRecord, RunStore};
use pretty_assertions::assert_eq;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

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

fn ac(id: &str, verdict: Verdict) -> AcRow {
    let mut r = AcRow::new(
        id,
        Some(&format!("AC-{id}")),
        &format!("teks {id}"),
        RowStatus::Active,
    );
    r.verdict = verdict;
    r
}

fn review(id: &str, run: &str, started: i64) -> StoredReview {
    StoredReview {
        id: id.to_owned(),
        run_id: run.to_owned(),
        trigger: "manual".to_owned(),
        status: "finished".to_owned(),
        browser_used: true,
        error: None,
        cost_usd: Some(0.42),
        usage_json: "{\"output_tokens\":10}".to_owned(),
        started_at_ms: started,
        finished_at_ms: Some(started + 1),
    }
}

#[tokio::test]
async fn buku_ac_bertahan_setelah_store_dibuka_ulang_dan_urutan_terjaga() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    let mut a = ac("2", Verdict::Fail);
    a.design = Design::Conflicts;
    a.design_note = Some("Bulk approve moved".to_owned());
    a.evidence = Some("kelola-web src/a.ts:3".to_owned());
    a.stale = true;
    a.status = RowStatus::Changed;
    a.reason = Some("API down".to_owned());
    a.decided_by = Some("Beni".to_owned());
    let rows = vec![a, ac("1", Verdict::Pass)];
    s.save_ac_rows("run1", &rows).await.unwrap();
    drop(s);

    let s = open(&dir).await;
    assert_eq!(s.ac_rows("run1").await.unwrap(), rows);
    assert!(s.ac_rows("lain").await.unwrap().is_empty());
}

#[tokio::test]
async fn menyimpan_ulang_buku_ac_mengganti_bukan_menduplikasi() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    s.save_ac_rows("run1", &[ac("1", Verdict::Fail)])
        .await
        .unwrap();
    s.save_ac_rows("run1", &[ac("1", Verdict::Pass)])
        .await
        .unwrap();
    let got = s.ac_rows("run1").await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].verdict, Verdict::Pass);
}

#[tokio::test]
async fn review_dan_verdict_per_ac_tersimpan_terbaru_dulu() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "run1").await;
    let mut first = review("r1", "run1", 100);
    first.status = "running".to_owned();
    s.save_review(&first).await.unwrap();
    first.status = "failed".to_owned();
    first.error = Some("time limit".to_owned());
    s.save_review(&first).await.unwrap();
    s.save_review(&review("r2", "run1", 200)).await.unwrap();
    let verdicts = vec![
        StoredVerdict {
            ac_id: "1".to_owned(),
            label: Some("AC-1".to_owned()),
            verdict: "pass".to_owned(),
            evidence: Some("test ok".to_owned()),
        },
        StoredVerdict {
            ac_id: "2".to_owned(),
            label: None,
            verdict: "fail".to_owned(),
            evidence: None,
        },
    ];
    s.save_review_verdicts("r2", &verdicts).await.unwrap();

    let got = s.reviews("run1").await.unwrap();
    assert_eq!(
        got.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["r2", "r1"]
    );
    assert_eq!(got[1].status, "failed");
    assert_eq!(got[1].error.as_deref(), Some("time limit"));
    assert_eq!(got[0], review("r2", "run1", 200));
    assert_eq!(s.review_verdicts("r2").await.unwrap(), verdicts);
    assert!(s.review_verdicts("r1").await.unwrap().is_empty());
}

#[tokio::test]
async fn pemangkasan_riwayat_review_run_tertutup_lama_dan_lebih_dari_batas() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "lama").await;
    with_run(&s, "aktif").await;
    s.save_review(&review("old", "lama", 1)).await.unwrap();
    s.close_run("lama", 10).await.unwrap();
    for i in 0..21 {
        s.save_review(&review(&format!("a{i:02}"), "aktif", 100 + i))
            .await
            .unwrap();
    }
    let now = 91 * DAY_MS + 10;
    let removed = s.prune_reviews(now - 90 * DAY_MS, 20).await.unwrap();
    assert_eq!(removed, 2);
    assert!(s.reviews("lama").await.unwrap().is_empty());
    let left = s.reviews("aktif").await.unwrap();
    assert_eq!(left.len(), 20);
    assert!(left.iter().all(|r| r.id != "a00"), "yang tertua dihapus");
}

#[tokio::test]
async fn sumber_desain_tambah_ubah_hapus_dan_nama_ganda_ditolak() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    let src = |id: &str, name: &str| DesignSourceRow {
        id: id.to_owned(),
        name: name.to_owned(),
        kind: "folder".to_owned(),
        config_json: "{\"path\":\"/home/beni/kelola-mock\"}".to_owned(),
        created_at_ms: 1,
    };
    s.insert_design_source(&src("d1", "kelola-mock"))
        .await
        .unwrap();
    s.insert_design_source(&src("d2", "pegasus-mock"))
        .await
        .unwrap();
    assert!(matches!(
        s.insert_design_source(&src("d3", "kelola-mock"))
            .await
            .unwrap_err(),
        StoreError::Conflict(_)
    ));
    assert!(matches!(
        s.update_design_source("d2", "kelola-mock", "{}")
            .await
            .unwrap_err(),
        StoreError::Conflict(_)
    ));
    s.update_design_source("d2", "pegasus-design", "{\"path\":\"/x\"}")
        .await
        .unwrap();
    s.remove_design_source("d1").await.unwrap();
    assert!(matches!(
        s.remove_design_source("d1").await.unwrap_err(),
        StoreError::NotFound
    ));
    assert!(matches!(
        s.update_design_source("nope", "x", "{}").await.unwrap_err(),
        StoreError::NotFound
    ));
    let all = s.design_sources().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "pegasus-design");
    assert_eq!(all[0].config_json, "{\"path\":\"/x\"}");
}

#[tokio::test]
async fn setelan_global_dan_setelan_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    assert_eq!(s.setting("reviewer.model").await.unwrap(), None);
    s.set_setting("reviewer.model", "sonnet").await.unwrap();
    s.set_setting("reviewer.model", "opus").await.unwrap();
    assert_eq!(
        s.setting("reviewer.model").await.unwrap().as_deref(),
        Some("opus")
    );

    s.insert_repo(&NewRepo {
        id: "api".into(),
        path: "/w/kelola/api".into(),
        gh_profile: None,
        default_branch: Some("main".into()),
        now_ms: 1,
    })
    .await
    .unwrap();
    s.insert_workspace(&NewWorkspace {
        id: "kelola".into(),
        name: "kelola".into(),
        root_path: "/w/kelola".into(),
        repo_ids: vec!["api".into()],
        mcp_servers: vec![],
        now_ms: 1,
    })
    .await
    .unwrap();
    assert_eq!(s.workspace_settings("kelola").await.unwrap(), "{}");
    s.set_workspace_settings("kelola", "{\"gate\":\"warn\"}")
        .await
        .unwrap();
    assert_eq!(
        s.workspace_settings("kelola").await.unwrap(),
        "{\"gate\":\"warn\"}"
    );
    assert!(matches!(
        s.workspace_settings("nope").await.unwrap_err(),
        StoreError::NotFound
    ));
    assert!(matches!(
        s.set_workspace_settings("nope", "{}").await.unwrap_err(),
        StoreError::NotFound
    ));
}

#[tokio::test]
async fn riwayat_review_sebagai_port_dengan_verdict_dan_setelan_reviewer() {
    use mandor_store::{CHROME_KEY, ReviewsStore};
    use mandor_usecase::run_ports::{
        ReviewHistory, ReviewRecord, ReviewStatus, ReviewTrigger, ReviewerSettings,
    };

    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir).await;
    with_run(&s, "r1").await;
    let port = ReviewsStore::new(s.clone());
    assert_eq!(port.reviewer_settings().await, ReviewerSettings::default());
    assert!(!port.chrome_connected().await);

    let mut first = ReviewRecord {
        id: "rv1".to_owned(),
        run_id: "r1".to_owned(),
        trigger: ReviewTrigger::Manual,
        status: ReviewStatus::Running,
        browser_used: false,
        browser_reason: Some("No app for this run".to_owned()),
        error: None,
        cost_usd: None,
        usage: mandor_domain::Usage::default(),
        stale: false,
        skill: Some("qa:review".to_owned()),
        started_at_ms: 10,
        finished_at_ms: None,
        verdicts: vec![],
    };
    port.save_review(&first).await.unwrap();
    first.status = ReviewStatus::Finished;
    first.cost_usd = Some(0.42);
    first.usage.output_tokens = 900;
    first.stale = true;
    first.finished_at_ms = Some(20);
    let mut pass = ac("a1", Verdict::Pass);
    pass.evidence = Some("src/a.rs:1".to_owned());
    first.verdicts = vec![pass, ac("a2", Verdict::Fail)];
    port.save_review(&first).await.unwrap();
    let mut second = first.clone();
    second.id = "rv2".to_owned();
    second.started_at_ms = 30;
    second.verdicts = vec![ac("a1", Verdict::CantVerify)];
    port.save_review(&second).await.unwrap();

    let all = port.reviews("r1").await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, "rv2", "terbaru dulu");
    let back = &all[1];
    assert_eq!(back.status, ReviewStatus::Finished);
    assert_eq!(back.cost_usd, Some(0.42));
    assert_eq!(back.usage.output_tokens, 900);
    assert!(back.stale);
    assert_eq!(back.skill.as_deref(), Some("qa:review"));
    assert_eq!(back.browser_reason.as_deref(), Some("No app for this run"));
    let verdicts: Vec<_> = back.verdicts.iter().map(|r| r.verdict).collect();
    assert_eq!(verdicts, [Verdict::Pass, Verdict::Fail]);
    assert_eq!(back.verdicts[0].evidence.as_deref(), Some("src/a.rs:1"));
    assert_eq!(all[0].verdicts.len(), 1);

    let custom = ReviewerSettings {
        model: Some("haiku".to_owned()),
        budget_cents: 75,
        timeout_secs: 300,
    };
    port.set_reviewer_settings(&custom).await.unwrap();
    assert_eq!(port.reviewer_settings().await, custom);
    s.set_setting(CHROME_KEY, "true").await.unwrap();
    assert!(port.chrome_connected().await);
}
