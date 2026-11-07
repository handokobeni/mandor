//! Kartu pertanyaan agent (plan 2026-11-07-001, KTD-Q1, KTD-Q3).

use mandor_domain::question::{QuestionOption, QuestionStatus};
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{QuestionRecord, QuestionStore};
use serde_json::{Value, json};

use crate::{Store, StoreError, from_json, to_json};

/// Penyimpanan kartu pertanyaan.
#[derive(Debug, Clone)]
pub struct QuestionsStore {
    store: Store,
}

impl QuestionsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

struct Row {
    id: String,
    run_id: String,
    generation: i64,
    question: String,
    options_json: String,
    status: String,
    answer: Option<String>,
    created_ms: i64,
    closed_ms: Option<i64>,
}

fn options_json(options: &[QuestionOption]) -> String {
    to_json(&Value::Array(
        options
            .iter()
            .map(|o| {
                json!({
                    "label": o.label,
                    "description": o.description,
                    "recommended": o.recommended,
                })
            })
            .collect(),
    ))
}

fn parse_options(raw: &str) -> Result<Vec<QuestionOption>, StoreError> {
    let bad = || StoreError::corrupt(format!("question options `{raw}`"));
    let Value::Array(items) = from_json(raw)? else {
        return Err(bad());
    };
    items
        .iter()
        .map(|o| {
            Ok(QuestionOption {
                label: o["label"].as_str().ok_or_else(bad)?.to_owned(),
                description: o["description"].as_str().ok_or_else(bad)?.to_owned(),
                recommended: o["recommended"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

fn to_record(r: Row) -> Result<QuestionRecord, StoreError> {
    Ok(QuestionRecord {
        status: QuestionStatus::parse(&r.status)
            .ok_or_else(|| StoreError::corrupt(format!("question status `{}`", r.status)))?,
        options: parse_options(&r.options_json)?,
        id: r.id,
        run_id: r.run_id,
        generation: r.generation,
        question: r.question,
        answer: r.answer,
        created_ms: r.created_ms,
        closed_ms: r.closed_ms,
    })
}

impl Store {
    /// Pertanyaan Pending terbaru per run (satu query, untuk daftar run).
    ///
    /// # Errors
    /// [`StoreError`] bila query gagal.
    pub async fn pending_question_runs(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, StoreError> {
        let rows = sqlx::query!(
            "SELECT run_id, id FROM questions WHERE status = 'pending'
             ORDER BY created_ms, rowid"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows.into_iter().map(|r| (r.run_id, r.id)).collect())
    }
}

impl QuestionStore for QuestionsStore {
    async fn create_question(&self, q: &QuestionRecord) -> Result<bool, PortError> {
        let options = options_json(&q.options);
        let status = q.status.as_str();
        let inserted = sqlx::query!(
            "INSERT INTO questions (id, run_id, generation, question, options_json, status,
                                    answer, created_ms, closed_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(run_id, generation) DO NOTHING",
            q.id,
            q.run_id,
            q.generation,
            q.question,
            options,
            status,
            q.answer,
            q.created_ms,
            q.closed_ms
        )
        .execute(&self.store.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "question already exists"))?
        .rows_affected();
        Ok(inserted == 1)
    }

    async fn question(&self, id: &str) -> Result<Option<QuestionRecord>, PortError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, run_id, generation, question, options_json, status, answer,
                    created_ms, closed_ms
             FROM questions WHERE id = ?",
            id
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(row.map(to_record).transpose()?)
    }

    async fn questions(&self, run_id: &str) -> Result<Vec<QuestionRecord>, PortError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, run_id, generation, question, options_json, status, answer,
                    created_ms, closed_ms
             FROM questions WHERE run_id = ? ORDER BY created_ms, rowid",
            run_id
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows.into_iter().map(to_record).collect::<Result<_, _>>()?)
    }

    async fn pending_question(&self, run_id: &str) -> Result<Option<QuestionRecord>, PortError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, run_id, generation, question, options_json, status, answer,
                    created_ms, closed_ms
             FROM questions WHERE run_id = ? AND status = 'pending'
             ORDER BY created_ms DESC, rowid DESC LIMIT 1",
            run_id
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(row.map(to_record).transpose()?)
    }

    async fn close_question(
        &self,
        id: &str,
        status: QuestionStatus,
        answer: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, PortError> {
        if status == QuestionStatus::Pending {
            return Ok(false);
        }
        let status = status.as_str();
        let changed = sqlx::query!(
            "UPDATE questions SET status = ?, answer = ?, closed_ms = ?
             WHERE id = ? AND status = 'pending'",
            status,
            answer,
            now_ms,
            id
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?
        .rows_affected();
        Ok(changed == 1)
    }
}
