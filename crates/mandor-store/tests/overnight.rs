//! Antrean, siklus, dan setelan lembur VM (U2 plan VM).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::Design;
use mandor_domain::planned::{DesignMark, PlannedState, PlannedWork};
use mandor_store::{OvernightsStore, Store};
use mandor_usecase::overnight::{
    CycleUsage, OvernightSettings, OvernightStore, PlannedEntry, QueueItem, ReportState,
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
        design_sources: Vec::new(),
        planned: None,
        group: Vec::new(),
    }
}

fn planned(id: &str, at: i64) -> QueueItem {
    let mut it = item(id, at);
    it.design_sources = vec!["ds1".to_owned(), "ds2".to_owned()];
    it.planned = Some(PlannedEntry {
        work: PlannedWork {
            plan: "## Plan\n1. Tambah tombol".to_owned(),
            branch: "pgs-1/tombol".to_owned(),
            design: vec![
                DesignMark {
                    ac_id: "ac1".to_owned(),
                    design: Design::FollowDesign,
                    note: Some("Header beda".to_owned()),
                },
                DesignMark {
                    ac_id: "ac2".to_owned(),
                    design: Design::InDesign,
                    note: None,
                },
            ],
            laptop_design: vec!["lap1".to_owned()],
            repos: vec!["web".to_owned(), "api".to_owned()],
            group_acs: None,
        },
        state: PlannedState::HandoverPending,
        origin_daemon: Some("laptop".to_owned()),
        reason: None,
    });
    it
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
async fn item_grup_bolak_balik_dan_sisip_di_depan() {
    let (s, _d) = open().await;
    s.enqueue(&item("a", 1)).await.unwrap();
    let mut g = item("g1", 2);
    g.group = vec!["g1".to_owned(), "g2".to_owned(), "g3".to_owned()];
    s.enqueue(&g).await.unwrap();
    let q = s.queue().await.unwrap();
    assert_eq!(q[1], g, "urutan kartu grup tetap");
    assert!(q[0].group.is_empty(), "item tanpa grup = kartu tunggal");

    s.dequeue("g1", None, 3).await.unwrap();
    s.enqueue(&item("b", 4)).await.unwrap();
    s.enqueue_front(&g).await.unwrap();
    assert_eq!(ids(&s.queue().await.unwrap()), ["g1", "a", "b"]);
    assert_eq!(s.enqueue_front(&g).await.unwrap_err(), PortError::Conflict);
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

#[tokio::test]
async fn item_planned_dan_sumber_desain_bolak_balik() {
    let (s, _d) = open().await;
    s.enqueue(&item("a", 1)).await.unwrap();
    let p = planned("b", 2);
    s.enqueue(&p).await.unwrap();

    let q = s.queue().await.unwrap();
    assert_eq!(q[0], item("a", 1), "item lama: No plan tanpa desain");
    assert_eq!(q[1], p);

    s.set_planned_state("b", PlannedState::Waiting, None)
        .await
        .unwrap();
    s.set_planned_state("b", PlannedState::Held, Some("the card has no claim"))
        .await
        .unwrap();
    let got = s.queue().await.unwrap()[1].planned.clone().unwrap();
    assert_eq!(got.state, PlannedState::Held);
    assert_eq!(got.reason.as_deref(), Some("the card has no claim"));

    s.set_planned_state("b", PlannedState::Waiting, None)
        .await
        .unwrap();
    let got = s.queue().await.unwrap()[1].planned.clone().unwrap();
    assert_eq!(got.reason, None, "alasan dihapus saat status berubah");

    assert!(matches!(
        s.set_planned_state("a", PlannedState::Waiting, None).await,
        Err(PortError::Rejected(_))
    ));
    s.reorder(&["b".to_owned()]).await.unwrap();
    assert_eq!(ids(&s.queue().await.unwrap()), ["b", "a"]);
}
