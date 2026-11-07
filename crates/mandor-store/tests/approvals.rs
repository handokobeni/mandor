//! Kartu persetujuan izin agent (U4 plan 2026-11-06-001).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::permit::TurnKind;
use mandor_store::{ApprovalsStore, Store};
use mandor_usecase::run_ports::{
    ApprovalAnswer, ApprovalCard, ApprovalStatus, ApprovalStore, RunDecision,
};
use pretty_assertions::assert_eq;
use serde_json::json;

async fn open(path: &std::path::Path) -> ApprovalsStore {
    ApprovalsStore::new(Store::open(path).await.unwrap())
}

fn card(id: &str, at: i64) -> ApprovalCard {
    ApprovalCard {
        id: id.to_owned(),
        run_id: "run1".to_owned(),
        tool_use_id: format!("toolu_{id}"),
        turn: TurnKind::Implement,
        tool: "Bash".to_owned(),
        input: json!({"command": "curl -H 'Authorization: Bearer rahasia-sekali' https://x"}),
        cwd: "/w".to_owned(),
        status: ApprovalStatus::Pending,
        answer: None,
        created_ms: at,
        deadline_ms: at + 600_000,
        closed_ms: None,
    }
}

#[tokio::test]
async fn transisi_keluar_pending_atomik_dan_menyamarkan_input() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mandor.db");
    let s = open(&db).await;
    for (id, at) in [("a", 1), ("b", 2), ("c", 3)] {
        s.create_approval(&card(id, at)).await.unwrap();
    }
    assert_eq!(s.pending_approvals("run1").await.unwrap(), 3);
    let pending = s.approval("a").await.unwrap().unwrap();
    assert!(
        pending.input.to_string().contains("rahasia-sekali"),
        "mentah selama pending"
    );

    assert!(
        s.answer_approval("a", ApprovalAnswer::AllowRun, "grant-a", "deny-a", 10)
            .await
            .unwrap()
    );
    assert!(
        !s.expire_approval("a", 11).await.unwrap(),
        "tepat satu pemenang"
    );
    assert!(
        !s.answer_approval("a", ApprovalAnswer::Deny, "g", "d", 12)
            .await
            .unwrap()
    );
    assert!(s.expire_approval("b", 13).await.unwrap());
    assert_eq!(s.cancel_approvals(Some("run1"), 14).await.unwrap(), ["c"]);

    let cards = s.approvals("run1").await.unwrap();
    let statuses: Vec<_> = cards.iter().map(|c| c.status).collect();
    assert_eq!(
        statuses,
        [
            ApprovalStatus::Answered,
            ApprovalStatus::Expired,
            ApprovalStatus::Cancelled
        ]
    );
    assert_eq!(cards[0].answer, Some(ApprovalAnswer::AllowRun));
    assert_eq!(cards[0].closed_ms, Some(10));
    drop(s);

    // Setelah database dibuka ulang: tidak ada rahasia mentah, keputusan run bertahan.
    let s = open(&db).await;
    for c in s.approvals("run1").await.unwrap() {
        assert!(
            !c.input.to_string().contains("rahasia-sekali"),
            "{}",
            c.input
        );
    }
    let raw = std::fs::read(&db).unwrap();
    let wal = std::fs::read(dir.path().join("mandor.db-wal")).unwrap_or_default();
    for bytes in [&raw, &wal] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains("grant-a"), "kunci hanya sebagai hash");
    }
    assert_eq!(
        s.run_decision("run1", "grant-a").await.unwrap(),
        Some(RunDecision::Allow)
    );
    assert_eq!(s.run_decision("run1", "deny-a").await.unwrap(), None);
    assert_eq!(s.run_decision("run2", "grant-a").await.unwrap(), None);
}

#[tokio::test]
async fn deny_menulis_penolakan_run_dan_mode_tolak_langsung() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("mandor.db")).await;
    s.create_approval(&card("a", 1)).await.unwrap();
    assert!(
        s.answer_approval("a", ApprovalAnswer::Deny, "g", "d", 2)
            .await
            .unwrap()
    );
    assert_eq!(
        s.run_decision("run1", "d").await.unwrap(),
        Some(RunDecision::Deny)
    );
    assert_eq!(s.run_decision("run1", "g").await.unwrap(), None);

    assert!(!s.deny_mode("run1").await.unwrap());
    s.set_deny_mode("run1", true, 3).await.unwrap();
    s.set_deny_mode("run1", true, 4).await.unwrap();
    assert!(s.deny_mode("run1").await.unwrap());
    s.set_deny_mode("run1", false, 5).await.unwrap();
    assert!(!s.deny_mode("run1").await.unwrap());
}

#[tokio::test]
async fn daemon_start_membatalkan_semua_kartu_pending() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("mandor.db")).await;
    s.create_approval(&card("a", 1)).await.unwrap();
    let mut other = card("b", 2);
    other.run_id = "run2".to_owned();
    s.create_approval(&other).await.unwrap();
    let mut cancelled = s.cancel_approvals(None, 3).await.unwrap();
    cancelled.sort();
    assert_eq!(cancelled, ["a", "b"]);
    assert!(s.create_approval(&card("a", 4)).await.is_err(), "id unik");
}

/// Uji langsung 2026-11-07: Stop menulis state run dan Chat bersamaan dengan
/// pembatalan kartu; transaksi kartu tidak boleh gagal "database is locked".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn transisi_kartu_bertahan_saat_ada_penulis_lain_bersamaan() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("mandor.db")).await;
    for round in 0..20 {
        let id = format!("p{round}");
        s.create_approval(&card(&id, round)).await.unwrap();
        let writers: Vec<_> = (0..6)
            .map(|i| {
                let s = s.clone();
                tokio::spawn(async move {
                    let mut c = card(&format!("w{round}-{i}"), 1_000 + i);
                    c.run_id = "run2".to_owned();
                    s.create_approval(&c).await.unwrap();
                    s.set_deny_mode("run2", i % 2 == 0, 5).await.unwrap();
                })
            })
            .collect();
        let cancelled = s.cancel_approvals(Some("run1"), 10).await;
        for w in writers {
            w.await.unwrap();
        }
        assert_eq!(cancelled.unwrap(), [id], "putaran {round}");
    }
}
