//! Kartu pertanyaan agent (U2 plan 2026-11-07-001).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::question::{QuestionOption, QuestionStatus};
use mandor_store::{QuestionsStore, Store};
use mandor_usecase::run_ports::{QuestionRecord, QuestionStore};
use pretty_assertions::assert_eq;

async fn open(path: &std::path::Path) -> QuestionsStore {
    QuestionsStore::new(Store::open(path).await.unwrap())
}

fn record(id: &str, run: &str, generation: i64, at: i64) -> QuestionRecord {
    QuestionRecord {
        id: id.to_owned(),
        run_id: run.to_owned(),
        generation,
        question: "Keep the old result?".to_owned(),
        options: vec![
            QuestionOption {
                label: "Keep both".to_owned(),
                description: "Show both results".to_owned(),
                recommended: true,
            },
            QuestionOption {
                label: "Replace".to_owned(),
                description: "Only the new one".to_owned(),
                recommended: false,
            },
        ],
        status: QuestionStatus::Pending,
        answer: None,
        created_ms: at,
        closed_ms: None,
    }
}

#[tokio::test]
async fn simpan_dan_baca_pertanyaan_utuh() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("mandor.db")).await;
    let q = record("q1", "run1", 3, 10);
    assert!(s.create_question(&q).await.unwrap());
    assert!(
        s.create_question(&record("x", "run2", 3, 11))
            .await
            .unwrap()
    );
    assert_eq!(s.question("q1").await.unwrap(), Some(q.clone()));
    assert_eq!(s.questions("run1").await.unwrap(), vec![q.clone()]);
    assert_eq!(s.pending_question("run1").await.unwrap(), Some(q));
    assert_eq!(s.question("nope").await.unwrap(), None);
}

#[tokio::test]
async fn satu_pertanyaan_per_generasi_giliran() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("mandor.db")).await;
    assert!(
        s.create_question(&record("q1", "run1", 3, 10))
            .await
            .unwrap()
    );
    assert!(
        !s.create_question(&record("q2", "run1", 3, 11))
            .await
            .unwrap()
    );
    assert_eq!(s.questions("run1").await.unwrap().len(), 1);
    assert!(
        s.create_question(&record("q3", "run1", 4, 12))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn hanya_satu_transisi_keluar_pending() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("mandor.db")).await;
    s.create_question(&record("q1", "run1", 1, 10))
        .await
        .unwrap();
    assert!(
        s.close_question("q1", QuestionStatus::Answered, Some("Keep both"), 20)
            .await
            .unwrap()
    );
    assert!(
        !s.close_question("q1", QuestionStatus::AnsweredInChat, Some("no"), 21)
            .await
            .unwrap()
    );
    let q = s.question("q1").await.unwrap().unwrap();
    assert_eq!(q.status, QuestionStatus::Answered);
    assert_eq!(q.answer.as_deref(), Some("Keep both"));
    assert_eq!(q.closed_ms, Some(20));
    assert_eq!(s.pending_question("run1").await.unwrap(), None);
    assert!(
        !s.close_question("nope", QuestionStatus::Skipped, None, 22)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn pending_terbaru_dan_urutan() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(&dir.path().join("mandor.db")).await;
    s.create_question(&record("q1", "run1", 1, 10))
        .await
        .unwrap();
    s.create_question(&record("q2", "run1", 2, 20))
        .await
        .unwrap();
    let ids: Vec<String> = s
        .questions("run1")
        .await
        .unwrap()
        .into_iter()
        .map(|q| q.id)
        .collect();
    assert_eq!(ids, ["q1", "q2"]);
    assert_eq!(
        s.pending_question("run1").await.unwrap().map(|q| q.id),
        Some("q2".to_owned())
    );
}

#[tokio::test]
async fn pertanyaan_bertahan_setelah_dibuka_ulang() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mandor.db");
    let q = record("q1", "run1", 1, 10);
    open(&db).await.create_question(&q).await.unwrap();
    let s = open(&db).await;
    assert_eq!(s.pending_question("run1").await.unwrap(), Some(q));
    let store = Store::open(&db).await.unwrap();
    let runs = store.pending_question_runs().await.unwrap();
    assert_eq!(runs.get("run1").map(String::as_str), Some("q1"));
    assert_eq!(runs.len(), 1);
}
