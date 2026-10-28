//! Antrean, siklus, dan setelan lembur VM (U2 plan VM).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_store::{OvernightsStore, Store};
use mandor_usecase::overnight::{
    CycleUsage, OvernightSettings, OvernightStore, QueueItem, ReportState,
};
use mandor_usecase::{PortError, StartTarget};
use pretty_assertions::assert_eq;

async fn open() -> (OvernightsStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("mandor.db")).await.unwrap();
    (OvernightsStore::new(s), dir)
}

fn item(id: &str, at: i64) -> QueueItem {
    QueueItem {
        record_id: id.to_owned(),
        target: StartTarget::Repo("repo1".to_owned()),
        note: None,
        start_now: false,
        added_at_ms: at,
    }
}

fn ids(q: &[QueueItem]) -> Vec<&str> {
    q.iter().map(|i| i.record_id.as_str()).collect()
}

#[tokio::test]
async fn antrean_tambah_urut_ulang_dan_keluarkan() {
    let (s, _d) = open().await;
    s.enqueue(&item("a", 1)).await.unwrap();
    let mut ws = item("b", 2);
    ws.target = StartTarget::Workspace {
        id: "kelola".to_owned(),
        repo_ids: vec!["api".to_owned(), "web".to_owned()],
    };
    ws.note = Some("Pakai komponen lama".to_owned());
    s.enqueue(&ws).await.unwrap();
    s.enqueue(&item("c", 3)).await.unwrap();
    assert_eq!(
        s.enqueue(&item("a", 4)).await.unwrap_err(),
        PortError::Conflict
    );

    let q = s.queue().await.unwrap();
    assert_eq!(ids(&q), ["a", "b", "c"]);
    assert_eq!(q[1], ws, "target workspace dan catatan bolak-balik");

    s.reorder(&["c".to_owned(), "a".to_owned()]).await.unwrap();
    assert_eq!(ids(&s.queue().await.unwrap()), ["c", "a", "b"]);

    s.set_start_now("b").await.unwrap();
    assert!(s.queue().await.unwrap()[2].start_now);
    assert!(matches!(
        s.set_start_now("x").await,
        Err(PortError::Rejected(_))
    ));

    s.dequeue("a", None, 10).await.unwrap();
    s.dequeue("c", Some("the card moved to In Progress in Lark"), 11)
        .await
        .unwrap();
    assert_eq!(ids(&s.queue().await.unwrap()), ["b"]);
    let out = s.dequeued_since(11).await.unwrap();
    assert_eq!(out.len(), 1, "hanya yang punya alasan dicatat");
    assert_eq!(out[0].record_id, "c");
    assert!(s.dequeued_since(12).await.unwrap().is_empty());
}

#[tokio::test]
async fn hanya_laptop_setelan_dan_pemakaian_siklus() {
    let (s, _d) = open().await;
    assert!(!s.laptop_only("a").await.unwrap());
    s.set_laptop_only("a", 1).await.unwrap();
    s.set_laptop_only("a", 2).await.unwrap();
    assert!(s.laptop_only("a").await.unwrap());

    assert_eq!(s.settings().await.unwrap(), None);
    let set = OvernightSettings {
        start: "19:00".to_owned(),
        end: "06:00".to_owned(),
        offset: "+07:00".to_owned(),
        token_limit: Some(2_000_000),
    };
    s.set_settings(&set).await.unwrap();
    assert_eq!(s.settings().await.unwrap(), Some(set));

    assert_eq!(s.cycle_usage(100).await.unwrap(), CycleUsage::default());
    let u = CycleUsage {
        input_tokens: 10,
        output_tokens: 5,
        cache_read_tokens: 100,
        cache_write_tokens: 7,
    };
    s.add_cycle_usage(100, &u).await.unwrap();
    s.add_cycle_usage(100, &u).await.unwrap();
    let got = s.cycle_usage(100).await.unwrap();
    // 20 input + 10 output + 14 cache tulis + 200/10 cache baca.
    assert_eq!(got.counted(), 64);
    assert_eq!(got.cache_read_tokens, 200);
    assert_eq!(s.cycle_usage(200).await.unwrap(), CycleUsage::default());

    assert_eq!(s.report_state(100).await.unwrap(), ReportState::default());
    s.record_report(100, false, 5).await.unwrap();
    s.record_report(100, true, 6).await.unwrap();
    s.record_report(100, false, 7).await.unwrap();
    assert_eq!(
        s.report_state(100).await.unwrap(),
        ReportState {
            sent: true,
            attempts: 3
        }
    );
    assert_eq!(
        s.cycle_usage(100).await.unwrap().counted(),
        64,
        "pemakaian tidak berubah"
    );
    s.record_report(300, true, 1).await.unwrap();
    assert!(s.report_state(300).await.unwrap().sent);
}
