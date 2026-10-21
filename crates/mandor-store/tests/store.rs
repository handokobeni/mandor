#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration tests: clippy.toml allowances miss #[tokio::test]

use mandor_store::{NewRepo, NewWorkspace, OutboxStatus, Store, StoreError};
use pretty_assertions::assert_eq;
use serde_json::json;

async fn store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("mandor.db")).await.unwrap();
    (s, dir)
}

async fn repo(s: &Store, id: &str) {
    s.insert_repo(&NewRepo {
        id: id.into(),
        path: format!("/tmp/{id}"),
        gh_profile: Some("dev".into()),
        default_branch: Some("main".into()),
        now_ms: 1,
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn migrasi_dua_kali_tidak_mengubah_apa_pun() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mandor.db");
    let s = Store::open(&path).await.unwrap();
    repo(&s, "r1").await;
    drop(s);
    let s = Store::open(&path).await.unwrap();
    assert_eq!(s.list_repos().await.unwrap().len(), 1);
}

#[tokio::test]
async fn repo_dan_settings_tersimpan() {
    let (s, _d) = store().await;
    repo(&s, "r1").await;
    s.set_repo_settings("r1", &json!({"allowlist": ["cargo test"]}))
        .await
        .unwrap();
    let r = s.get_repo("r1").await.unwrap();
    assert_eq!(r.default_branch.as_deref(), Some("main"));
    assert_eq!(r.settings, json!({"allowlist": ["cargo test"]}));
    assert!(matches!(
        s.get_repo("tidak-ada").await,
        Err(StoreError::NotFound)
    ));
}

#[tokio::test]
async fn pesan_dipaginasi_berurutan() {
    let (s, _d) = store().await;
    repo(&s, "r1").await;
    mandor_usecase::run_ports::RunStore::create(
        &mandor_store::RunsStore::new(s.clone()),
        &mandor_usecase::run_ports::RunRecord::new("run-1".into(), "r1".into(), "rec1".into()),
        10,
    )
    .await
    .unwrap();
    for i in 0..5 {
        s.append_message("run-1", "assistant", &json!({"n": i}), 20 + i)
            .await
            .unwrap();
    }
    let first = s.list_messages("run-1", 0, 2).await.unwrap();
    assert_eq!(first.len(), 2);
    let next = s.list_messages("run-1", first[1].id, 10).await.unwrap();
    assert_eq!(next.len(), 3);
    assert_eq!(next[0].body, json!({"n": 2}));
}

#[tokio::test]
async fn outbox_urut_per_antrean_dan_batal_tidak_kembali() {
    let (s, _d) = store().await;
    let a = s
        .enqueue("tbl1", "rec1", &json!({"op": 1}), 100)
        .await
        .unwrap();
    let b = s
        .enqueue("tbl1", "rec2", &json!({"op": 2}), 100)
        .await
        .unwrap();
    s.enqueue("tbl2", "rec9", &json!({"op": 9}), 100)
        .await
        .unwrap();

    let due = s.outbox_due("tbl1", 100, 10).await.unwrap();
    assert_eq!(due.iter().map(|i| i.id).collect::<Vec<_>>(), vec![a, b]);

    s.outbox_mark(a, OutboxStatus::Cancelled, Some("status berubah"))
        .await
        .unwrap();
    let left = s.outbox_due("tbl1", 200, 10).await.unwrap();
    assert_eq!(left.iter().map(|i| i.id).collect::<Vec<_>>(), vec![b]);
}

#[tokio::test]
async fn jurnal_komentar_dua_tahap() {
    let (s, _d) = store().await;
    s.journal_pending("rec1", "reply1", "run-1", 1)
        .await
        .unwrap();
    s.journal_pending("rec1", "reply2", "run-1", 2)
        .await
        .unwrap();
    s.journal_delivered("rec1", "reply1").await.unwrap();
    let pending = s.journal_pending_list("rec1").await.unwrap();
    assert_eq!(pending, vec!["reply2".to_owned()]);
    let delivered = s.journal_delivered_list("rec1").await.unwrap();
    assert_eq!(delivered, vec!["reply1".to_owned()]);
}

#[tokio::test]
async fn notifikasi_diambil_sekali() {
    let (s, _d) = store().await;
    s.push_notification("waiting_answer", &json!({"run": "run-1"}), 1, false)
        .await
        .unwrap();
    // Sudah dikirim langsung ke client: tidak diambil ulang, tetapi tetap di riwayat.
    s.push_notification("failed", &json!({}), 2, true)
        .await
        .unwrap();
    assert_eq!(s.take_notifications(10).await.unwrap().len(), 1);
    assert!(s.take_notifications(10).await.unwrap().is_empty());
    assert_eq!(s.list_notifications(10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn riwayat_notifikasi_terbaru_dulu_dengan_status_baca() {
    let (s, _d) = store().await;
    let a = s.push_notification("a", &json!({}), 1, true).await.unwrap();
    let b = s.push_notification("b", &json!({}), 2, true).await.unwrap();
    s.push_notification("c", &json!({}), 3, true).await.unwrap();

    let kinds = |l: &[mandor_store::Notification]| {
        l.iter()
            .map(|n| (n.kind.clone(), n.read))
            .collect::<Vec<_>>()
    };
    let list = s.list_notifications(2).await.unwrap();
    assert_eq!(kinds(&list), [("c".into(), false), ("b".into(), false)]);

    s.mark_notifications_read(Some(&[a, b])).await.unwrap();
    let list = s.list_notifications(10).await.unwrap();
    assert_eq!(
        kinds(&list),
        [("c".into(), false), ("b".into(), true), ("a".into(), true)]
    );
    s.mark_notifications_read(None).await.unwrap();
    assert!(
        s.list_notifications(10)
            .await
            .unwrap()
            .iter()
            .all(|n| n.read)
    );
}

#[tokio::test]
async fn riwayat_notifikasi_dibatasi() {
    let (s, _d) = store().await;
    for i in 0..(mandor_store::NOTIFICATIONS_KEPT + 5) {
        s.push_notification("x", &json!({ "i": i }), i, true)
            .await
            .unwrap();
    }
    let all = s.list_notifications(10_000).await.unwrap();
    assert_eq!(
        i64::try_from(all.len()).unwrap(),
        mandor_store::NOTIFICATIONS_KEPT
    );
    assert_eq!(
        all[0].payload["i"],
        json!(mandor_store::NOTIFICATIONS_KEPT + 4)
    );
}

#[tokio::test]
async fn workspace_berisi_repo_terurut_dan_ikut_terhapus_bersama_repo() {
    let (s, _d) = store().await;
    for (id, path) in [("api", "/w/api"), ("web", "/w/web")] {
        s.insert_repo(&NewRepo {
            id: id.into(),
            path: path.into(),
            gh_profile: None,
            default_branch: Some("main".into()),
            now_ms: 1,
        })
        .await
        .unwrap();
    }
    s.insert_workspace(&NewWorkspace {
        id: "kelola-v2".into(),
        name: "kelola-v2".into(),
        root_path: "/w".into(),
        repo_ids: vec!["web".into(), "api".into()],
        mcp_servers: vec!["graft".into()],
        now_ms: 2,
    })
    .await
    .unwrap();
    let ws = s.get_workspace("kelola-v2").await.unwrap();
    assert_eq!(ws.repo_ids, ["web", "api"]);
    assert_eq!(ws.mcp_servers, ["graft"]);
    assert_eq!(ws.root_path, "/w");

    s.update_workspace("kelola-v2", &["api".to_owned()], &[])
        .await
        .unwrap();
    let ws = s.get_workspace("kelola-v2").await.unwrap();
    assert_eq!(
        (ws.repo_ids, ws.mcp_servers),
        (vec!["api".to_owned()], vec![])
    );

    s.update_workspace("kelola-v2", &["api".to_owned(), "web".to_owned()], &[])
        .await
        .unwrap();
    s.remove_repo("web").await.unwrap();
    assert_eq!(
        s.get_workspace("kelola-v2").await.unwrap().repo_ids,
        ["api"]
    );
    assert_eq!(s.list_workspaces().await.unwrap().len(), 1);

    s.remove_workspace("kelola-v2").await.unwrap();
    assert!(matches!(
        s.get_workspace("kelola-v2").await,
        Err(StoreError::NotFound)
    ));
}
