//! Kartu run grup (KTD-G1, KTD-G2 plan run grup) dan label kartu di buku AC.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::group::{CardState, RemovedReason};
use mandor_domain::{AcRow, RowStatus};
use mandor_store::{NewRepo, RunsStore, Store};
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{CardCommit, GroupCardRow, RunRecord, RunStore};
use pretty_assertions::assert_eq;

async fn open() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("mandor.db")).await.unwrap();
    s.insert_repo(&NewRepo {
        id: "repo1".into(),
        path: "/src/app".into(),
        gh_profile: None,
        default_branch: Some("main".into()),
        now_ms: 1,
    })
    .await
    .unwrap();
    (s, dir)
}

fn run(id: &str, record: &str) -> RunRecord {
    RunRecord::new(id.to_owned(), "repo1".to_owned(), record.to_owned())
}

fn cards(ids: &[&str]) -> Vec<GroupCardRow> {
    ids.iter().map(|id| GroupCardRow::pending(id)).collect()
}

#[tokio::test]
async fn kartu_grup_bolak_balik_dan_dicari_lewat_kartu() {
    let (s, _d) = open().await;
    let runs = RunsStore::new(s);
    runs.create_group(&run("g", "c1"), &cards(&["c1", "c2", "c3"]), 1)
        .await
        .unwrap();

    let mut got = runs.cards("g").await.unwrap();
    assert_eq!(
        got.iter().map(|c| c.record_id.as_str()).collect::<Vec<_>>(),
        ["c1", "c2", "c3"]
    );
    assert_eq!(got[0].state, CardState::Pending);
    assert_eq!(runs.active_for("c3").await.unwrap().unwrap().id, "g");

    got[0].state = CardState::Done;
    got[0].commits = vec![CardCommit {
        repo_id: "repo1".to_owned(),
        sha: "abc123".to_owned(),
    }];
    got[0].plan = Some("Plan kartu 1".to_owned());
    got[1].state = CardState::Active;
    got[1].fix_requested_by = Some("ou_qa".to_owned());
    got[1].quiet_until_ms = Some(9);
    got[1].in_fix = true;
    got[2].changed_in_lark = true;
    got[2].kept_reason = Some("QA butuh kodenya".to_owned());
    runs.save_cards("g", &got, None).await.unwrap();
    assert_eq!(runs.cards("g").await.unwrap(), got);
    assert!(runs.cards("lain").await.unwrap().is_empty());
}

#[tokio::test]
async fn satu_kartu_hanya_di_satu_run_terbuka() {
    let (s, _d) = open().await;
    let runs = RunsStore::new(s);
    runs.create_group(&run("g", "c1"), &cards(&["c1", "c2"]), 1)
        .await
        .unwrap();
    assert_eq!(
        runs.create_group(&run("h", "c2"), &cards(&["c2", "c9"]), 1)
            .await
            .unwrap_err(),
        PortError::Conflict,
        "kartu di dua grup"
    );
    assert_eq!(
        runs.create(&run("t", "c2"), 1).await.unwrap_err(),
        PortError::Conflict,
        "run tunggal untuk kartu grup"
    );
    runs.create(&run("t", "c7"), 1).await.unwrap();
    assert_eq!(
        runs.create_group(&run("h", "c8"), &cards(&["c8", "c7"]), 1)
            .await
            .unwrap_err(),
        PortError::Conflict,
        "grup untuk kartu yang punya run tunggal"
    );
    assert!(
        runs.get("h").await.unwrap().is_none(),
        "tidak ada run setengah jadi"
    );

    runs.close("g", 2).await.unwrap();
    runs.create(&run("t2", "c2"), 3).await.unwrap();
    assert_eq!(runs.active_for("c2").await.unwrap().unwrap().id, "t2");
}

#[tokio::test]
async fn kartu_utama_keluar_memindah_record_run_dan_membebaskan_kartu() {
    let (s, _d) = open().await;
    let runs = RunsStore::new(s);
    runs.create_group(&run("g", "c1"), &cards(&["c1", "c2"]), 1)
        .await
        .unwrap();
    let mut got = runs.cards("g").await.unwrap();
    got[0].state = CardState::Removed(RemovedReason::Reverted);
    runs.save_cards("g", &got, Some("c2")).await.unwrap();

    assert_eq!(runs.get("g").await.unwrap().unwrap().record_id, "c2");
    assert!(runs.active_for("c1").await.unwrap().is_none());
    runs.create(&run("t", "c1"), 2).await.unwrap();
    assert_eq!(runs.active_for("c2").await.unwrap().unwrap().id, "g");
}

#[tokio::test]
async fn buku_ac_membawa_label_kartu() {
    let (s, _d) = open().await;
    let runs = RunsStore::new(s.clone());
    runs.create_group(&run("g", "c1"), &cards(&["c1", "c2"]), 1)
        .await
        .unwrap();
    let mut a = AcRow::new("ac1", Some("AC-1"), "A", RowStatus::Active);
    a.card = Some("c1".to_owned());
    let b = AcRow::new("ac2", None, "B", RowStatus::Active);
    s.save_ac_rows("g", &[a.clone(), b.clone()]).await.unwrap();
    assert_eq!(s.ac_rows("g").await.unwrap(), [a, b]);
}
