//! Kartu persetujuan izin agent (plan 2026-11-06-001, KTD-A11, KTD-A12).

use mandor_domain::permit::{TurnKind, redact_json};
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{
    ApprovalAnswer, ApprovalCard, ApprovalStatus, ApprovalStore, RunDecision,
};
use sha2::{Digest, Sha256};

use crate::{Store, StoreError, from_json, to_json};

/// Hash SHA-256 heksadesimal dari string kunci; input mentah tidak disimpan.
fn key_hash(key: &str) -> String {
    use std::fmt::Write as _;
    Sha256::digest(key.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Penyimpanan kartu persetujuan.
#[derive(Debug, Clone)]
pub struct ApprovalsStore {
    store: Store,
}

impl ApprovalsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

struct Row {
    id: String,
    run_id: String,
    tool_use_id: String,
    turn: String,
    tool: String,
    input_json: String,
    cwd: String,
    status: String,
    answer: Option<String>,
    created_ms: i64,
    deadline_ms: i64,
    closed_ms: Option<i64>,
}

fn to_card(r: Row) -> Result<ApprovalCard, StoreError> {
    let bad = |what: &str, v: &str| StoreError::corrupt(format!("approval {what} `{v}`"));
    Ok(ApprovalCard {
        turn: TurnKind::parse(&r.turn).ok_or_else(|| bad("turn", &r.turn))?,
        status: ApprovalStatus::parse(&r.status).ok_or_else(|| bad("status", &r.status))?,
        answer: match r.answer.as_deref() {
            Some(a) => Some(ApprovalAnswer::parse(a).ok_or_else(|| bad("answer", a))?),
            None => None,
        },
        input: from_json(&r.input_json)?,
        id: r.id,
        run_id: r.run_id,
        tool_use_id: r.tool_use_id,
        tool: r.tool,
        cwd: r.cwd,
        created_ms: r.created_ms,
        deadline_ms: r.deadline_ms,
        closed_ms: r.closed_ms,
    })
}

impl Store {
    /// Pending → `status` bila masih Pending, dengan input disamarkan dalam
    /// transaksi yang sama. `answer` dan keputusan run ikut ditulis.
    async fn close_approval(
        &self,
        id: &str,
        status: ApprovalStatus,
        answer: Option<ApprovalAnswer>,
        decision: Option<(&str, Option<TurnKind>, RunDecision)>,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        let row = sqlx::query!(
            "SELECT run_id, input_json FROM approvals WHERE id = ? AND status = 'pending'",
            id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::internal)?;
        let Some(row) = row else {
            return Ok(false);
        };
        let redacted = to_json(&redact_json(&from_json(&row.input_json)?));
        let status = status.as_str();
        let answer = answer.map(ApprovalAnswer::as_str);
        let changed = sqlx::query!(
            "UPDATE approvals SET status = ?, answer = ?, input_json = ?, closed_ms = ?
             WHERE id = ? AND status = 'pending'",
            status,
            answer,
            redacted,
            now_ms,
            id
        )
        .execute(&mut *tx)
        .await
        .map_err(StoreError::internal)?
        .rows_affected();
        if changed != 1 {
            return Ok(false);
        }
        if let Some((key, turn, kind)) = decision {
            let hash = key_hash(key);
            let turn = turn.map(TurnKind::as_str);
            let kind = match kind {
                RunDecision::Allow => "allow",
                RunDecision::Deny => "deny",
            };
            sqlx::query!(
                "INSERT INTO approval_decisions (run_id, key_hash, turn, kind, created_ms)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(run_id, key_hash) DO UPDATE SET kind = excluded.kind",
                row.run_id,
                hash,
                turn,
                kind,
                now_ms
            )
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        }
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(true)
    }
}

impl Store {
    /// Jumlah kartu Pending per run (satu query, untuk badge papan).
    ///
    /// # Errors
    /// [`StoreError`] bila query gagal.
    pub async fn pending_approval_counts(
        &self,
    ) -> Result<std::collections::HashMap<String, u32>, StoreError> {
        let rows = sqlx::query!(
            "SELECT run_id, COUNT(*) AS n FROM approvals WHERE status = 'pending' GROUP BY run_id"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows
            .into_iter()
            .map(|r| (r.run_id, u32::try_from(r.n).unwrap_or(u32::MAX)))
            .collect())
    }
}

impl ApprovalStore for ApprovalsStore {
    async fn create_approval(&self, card: &ApprovalCard) -> Result<(), PortError> {
        let turn = card.turn.as_str();
        let input = to_json(&card.input);
        let status = card.status.as_str();
        let answer = card.answer.map(ApprovalAnswer::as_str);
        sqlx::query!(
            "INSERT INTO approvals (id, run_id, tool_use_id, turn, tool, input_json, cwd, status,
                                    answer, created_ms, deadline_ms, closed_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            card.id,
            card.run_id,
            card.tool_use_id,
            turn,
            card.tool,
            input,
            card.cwd,
            status,
            answer,
            card.created_ms,
            card.deadline_ms,
            card.closed_ms
        )
        .execute(&self.store.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "approval already exists"))?;
        Ok(())
    }

    async fn approval(&self, id: &str) -> Result<Option<ApprovalCard>, PortError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, run_id, tool_use_id, turn, tool, input_json, cwd, status, answer,
                    created_ms, deadline_ms, closed_ms
             FROM approvals WHERE id = ?",
            id
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(row.map(to_card).transpose()?)
    }

    async fn approvals(&self, run_id: &str) -> Result<Vec<ApprovalCard>, PortError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, run_id, tool_use_id, turn, tool, input_json, cwd, status, answer,
                    created_ms, deadline_ms, closed_ms
             FROM approvals WHERE run_id = ? ORDER BY created_ms, rowid",
            run_id
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows.into_iter().map(to_card).collect::<Result<_, _>>()?)
    }

    async fn pending_approvals(&self, run_id: &str) -> Result<usize, PortError> {
        let n = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM approvals WHERE run_id = ? AND status = 'pending'",
            run_id
        )
        .fetch_one(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    async fn answer_approval(
        &self,
        id: &str,
        answer: ApprovalAnswer,
        grant_key: &str,
        deny_key: &str,
        now_ms: i64,
    ) -> Result<bool, PortError> {
        let turn = match self.approval(id).await? {
            Some(c) => c.turn,
            None => return Ok(false),
        };
        let decision = match answer {
            ApprovalAnswer::AllowRun => Some((grant_key, Some(turn), RunDecision::Allow)),
            ApprovalAnswer::Deny => Some((deny_key, None, RunDecision::Deny)),
            ApprovalAnswer::AllowOnce => None,
        };
        Ok(self
            .store
            .close_approval(id, ApprovalStatus::Answered, Some(answer), decision, now_ms)
            .await?)
    }

    async fn expire_approval(&self, id: &str, now_ms: i64) -> Result<bool, PortError> {
        Ok(self
            .store
            .close_approval(id, ApprovalStatus::Expired, None, None, now_ms)
            .await?)
    }

    async fn cancel_approvals(
        &self,
        run_id: Option<&str>,
        now_ms: i64,
    ) -> Result<Vec<String>, PortError> {
        let ids = sqlx::query_scalar!(
            "SELECT id FROM approvals WHERE status = 'pending' AND (? IS NULL OR run_id = ?)",
            run_id,
            run_id
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        let mut cancelled = Vec::with_capacity(ids.len());
        for id in ids {
            if self
                .store
                .close_approval(&id, ApprovalStatus::Cancelled, None, None, now_ms)
                .await?
            {
                cancelled.push(id);
            }
        }
        Ok(cancelled)
    }

    async fn run_decision(
        &self,
        run_id: &str,
        key: &str,
    ) -> Result<Option<RunDecision>, PortError> {
        let hash = key_hash(key);
        let kind = sqlx::query_scalar!(
            "SELECT kind FROM approval_decisions WHERE run_id = ? AND key_hash = ?",
            run_id,
            hash
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(match kind.as_deref() {
            Some("allow") => Some(RunDecision::Allow),
            Some("deny") => Some(RunDecision::Deny),
            Some(other) => {
                return Err(StoreError::corrupt(format!("approval decision `{other}`")).into());
            }
            None => None,
        })
    }

    async fn deny_mode(&self, run_id: &str) -> Result<bool, PortError> {
        let n = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM approval_deny_mode WHERE run_id = ?",
            run_id
        )
        .fetch_one(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(n > 0)
    }

    async fn set_deny_mode(&self, run_id: &str, on: bool, now_ms: i64) -> Result<(), PortError> {
        if on {
            sqlx::query!(
                "INSERT INTO approval_deny_mode (run_id, since_ms) VALUES (?, ?)
                 ON CONFLICT(run_id) DO NOTHING",
                run_id,
                now_ms
            )
            .execute(&self.store.pool)
            .await
            .map_err(StoreError::internal)?;
        } else {
            sqlx::query!("DELETE FROM approval_deny_mode WHERE run_id = ?", run_id)
                .execute(&self.store.pool)
                .await
                .map_err(StoreError::internal)?;
        }
        Ok(())
    }
}
