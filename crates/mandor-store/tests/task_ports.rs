//! `TaskStore` sebagai port cache dan outbox usecase.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::{AppField, CardFlag, Claim, FlagKind, Task, TaskStatus, WriteOp};
use mandor_store::{Store, TaskStore};
use mandor_usecase::{Outbox, TaskCache};
use pretty_assertions::assert_eq;

async fn open() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("mandor.db")).await.unwrap();
    (s, dir)
}

fn task(id: &str) -> Task {
    let mut t = Task::new(id, TaskStatus::WaitingFix);
    t.key = Some("PGS-1".to_owned());
    "Judul".clone_into(&mut t.title);
    t.assignees = vec!["ou_a".to_owned()];
    t.ac_ids = vec!["ac1".to_owned()];
    t.epic_ids = vec!["ep1".to_owned()];
    t.claim = Some(Claim {
        open_id: "ou_a".to_owned(),
        ts_ms: 5,
        nonce: "n".to_owned(),
    });
    t.last_modified_ms = 9;
    t
}

#[tokio::test]
async fn task_bolak_balik_dan_penanda_bertahan_saat_upsert() {
    let (s, _d) = open().await;
    let cache = TaskStore::new(s.clone(), "app/tbl".to_owned());
    let other = TaskStore::new(s, "app/lain".to_owned());
    let t = task("rec1");
    cache.upsert(&t).await.unwrap();
    assert_eq!(cache.get("rec1").await.unwrap().unwrap().task, t);
    assert!(other.get("rec1").await.unwrap().is_none(), "scope terpisah");

    let flag = CardFlag {
        kind: FlagKind::Overwritten,
        detail: "status berubah".to_owned(),
    };
    cache.set_flag("rec1", Some(&flag)).await.unwrap();
    let mut t2 = t.clone();
    t2.title = "Baru".to_owned();
    cache.upsert(&t2).await.unwrap();
    let got = cache.get("rec1").await.unwrap().unwrap();
    assert_eq!(got.task.title, "Baru");
    assert_eq!(got.flag, Some(flag));
    cache.set_flag("rec1", None).await.unwrap();
    assert_eq!(cache.list().await.unwrap()[0].flag, None);
}

#[tokio::test]
async fn baris_rusak_tidak_mencegah_baris_lain_terbaca() {
    let (s, _d) = open().await;
    let cache = TaskStore::new(s.clone(), "sc".to_owned());
    cache.upsert(&task("rec1")).await.unwrap();
    cache.upsert(&task("rec2")).await.unwrap();
    s.corrupt_task_for_test("sc", "rec1").await;
    let list = cache.list().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].task.record_id, "rec2");
}

#[tokio::test]
async fn outbox_menyimpan_operasi_dan_siklus_status() {
    let (s, _d) = open().await;
    let ob = TaskStore::new(s, "sc".to_owned());
    let op = WriteOp::Transition {
        from: TaskStatus::ToDo,
        to: TaskStatus::InProgress,
        fields: vec![
            AppField::Branch("mandor/pgs-1".to_owned()),
            AppField::Claim(None),
            AppField::Claim(Some(Claim {
                open_id: "ou_a".to_owned(),
                ts_ms: 1,
                nonce: "x".to_owned(),
            })),
        ],
    };
    let id = ob.enqueue("rec1", &op, 100).await.unwrap();
    assert!(ob.due(99, 10).await.unwrap().is_empty());
    let due = ob.due(100, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].op, op);
    assert_eq!(due[0].attempts, 0);

    ob.retry_later(id, 2, 500, "throttle").await.unwrap();
    assert!(ob.due(400, 10).await.unwrap().is_empty());
    assert_eq!(ob.due(500, 10).await.unwrap()[0].attempts, 2);

    ob.fail(id, "gagal").await.unwrap();
    assert!(ob.due(10_000, 10).await.unwrap().is_empty());
    assert_eq!(ob.requeue_failed("rec1", 600).await.unwrap(), 1);
    let again = ob.due(600, 10).await.unwrap();
    assert_eq!(again[0].attempts, 0);

    ob.complete(id).await.unwrap();
    let id2 = ob
        .enqueue("rec1", &WriteOp::Fields(vec![]), 1)
        .await
        .unwrap();
    ob.cancel(id2, "status berubah").await.unwrap();
    assert!(ob.due(10_000, 10).await.unwrap().is_empty());
}
