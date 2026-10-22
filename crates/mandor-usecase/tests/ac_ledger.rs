//! Skenario U3 Rilis 2: buku AC dibuat saat Start dan diselaraskan dengan tracker.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{RowStatus, Verdict};
use mandor_usecase::AcItem;
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn ac(lark: &FakeLark, id: &str, text: &str, dropped: bool) {
    lark.acs.lock().unwrap().insert(
        id.to_owned(),
        AcItem {
            record_id: id.to_owned(),
            label: Some(format!("AC-{id}")),
            text: text.to_owned(),
            dropped,
            ..Default::default()
        },
    );
}

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned(), "ac3".to_owned()];
    t.kanban_mirror = Some("todo".to_owned());
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    ac(&lark, "ac1", "Nama tampil", false);
    ac(&lark, "ac2", "Email tampil", false);
    ac(&lark, "ac3", "Lama", true);
    (lark, clock)
}

async fn started(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.svc.start("rec1", "repo1", None).await.unwrap();
    let id = st.runs.only().id;
    (st, id)
}

#[tokio::test]
async fn start_membuat_buku_ac_tanpa_ac_yang_di_drop() {
    let (lark, clock) = setup();
    let (st, run) = started(&lark, &clock).await;
    let rows = st.svc.ledger(&run).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.status == RowStatus::Active));
    assert!(rows.iter().all(|r| r.verdict == Verdict::NotReviewed));
    assert_eq!(rows[0].label.as_deref(), Some("AC-ac1"));
}

#[tokio::test]
async fn ae7_refresh_menambah_ac_baru_yang_ditautkan_setelah_start() {
    let (lark, clock) = setup();
    let (st, run) = started(&lark, &clock).await;
    // PM menautkan AC baru dari sisi tabel AC: cache task lokal belum tahu.
    ac(&lark, "ac4", "Telepon tampil", false);
    lark.edit("rec1", |t| t.ac_ids.push("ac4".to_owned()));
    let refreshed = st.svc.refresh_ledger(&run).await.unwrap();
    assert_eq!(refreshed.error, None);
    assert!(refreshed.synced_at_ms.is_some_and(|t| t >= 10 * DAY));
    let new: Vec<&str> = refreshed
        .rows
        .iter()
        .filter(|r| r.status == RowStatus::New)
        .map(|r| r.ac_id.as_str())
        .collect();
    assert_eq!(new, ["ac4"]);
    assert_eq!(st.svc.ledger(&run).await.unwrap().len(), 3, "tersimpan");
}

#[tokio::test]
async fn refresh_menandai_ac_yang_dilepas_sebagai_removed() {
    let (lark, clock) = setup();
    let (st, run) = started(&lark, &clock).await;
    lark.edit("rec1", |t| t.ac_ids = vec!["ac1".to_owned()]);
    let rows = st.svc.refresh_ledger(&run).await.unwrap().rows;
    let removed: Vec<&str> = rows
        .iter()
        .filter(|r| r.status == RowStatus::Removed)
        .map(|r| r.ac_id.as_str())
        .collect();
    assert_eq!(removed, ["ac2"]);
}

#[tokio::test]
async fn refresh_gagal_membiarkan_buku_ac_dan_melaporkan_error() {
    let (lark, clock) = setup();
    let (st, run) = started(&lark, &clock).await;
    let first = st.svc.refresh_ledger(&run).await.unwrap();
    *lark.acs_fail.lock().unwrap() = true;
    clock.set(11 * DAY);
    let failed = st.svc.refresh_ledger(&run).await.unwrap();
    assert!(failed.error.is_some());
    assert_eq!(failed.rows, first.rows);
    assert_eq!(
        failed.synced_at_ms, first.synced_at_ms,
        "waktu sinkron lama"
    );
}

#[tokio::test]
async fn run_lama_tanpa_buku_ac_mendapat_buku_aktif_saat_refresh() {
    let (lark, clock) = setup();
    let (st, run) = started(&lark, &clock).await;
    st.reviews.clear(&run);
    let rows = st.svc.refresh_ledger(&run).await.unwrap().rows;
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter().all(|r| r.status == RowStatus::Active),
        "bukan New"
    );
}

#[tokio::test]
async fn refresh_semua_run_aktif_melewati_run_yang_ditutup() {
    let (lark, clock) = setup();
    let (st, run) = started(&lark, &clock).await;
    ac(&lark, "ac4", "Telepon tampil", false);
    lark.edit("rec1", |t| t.ac_ids.push("ac4".to_owned()));
    assert_eq!(st.svc.refresh_active_ledgers().await, 1);
    assert_eq!(st.svc.ledger(&run).await.unwrap().len(), 3);
    st.svc.close_run(&run).await.unwrap();
    assert_eq!(st.svc.refresh_active_ledgers().await, 0);
}
