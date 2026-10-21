//! Cache task Lark per scope (tabel Lark) dan outbox, sebagai port usecase.

use mandor_domain::{CardFlag, FlagKind, Task, WriteOp};
use mandor_usecase::{CachedTask, Outbox, PortError, QueuedWrite, TaskCache};

use crate::codec::{TaskJson, WriteOpJson};
use crate::{OutboxStatus, Store, StoreError};

impl From<StoreError> for PortError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Conflict(_) => Self::Conflict,
            other => Self::Storage(other.to_string()),
        }
    }
}

/// Cache dan outbox untuk satu tabel Lark (`scope` = `app_token/table_id`).
#[derive(Debug, Clone)]
pub struct TaskStore {
    store: Store,
    scope: String,
}

impl TaskStore {
    #[must_use]
    pub fn new(store: Store, scope: String) -> Self {
        Self { store, scope }
    }
}

fn encode_task(t: &Task) -> Result<String, StoreError> {
    serde_json::to_string(&TaskJson::from(t)).map_err(StoreError::internal)
}

fn decode_task(raw: &str) -> Result<Task, StoreError> {
    serde_json::from_str::<TaskJson>(raw)
        .map(Task::from)
        .map_err(StoreError::internal)
}

fn decode_flag(kind: Option<&str>, detail: Option<String>) -> Option<CardFlag> {
    Some(CardFlag {
        kind: FlagKind::parse(kind?)?,
        detail: detail.unwrap_or_default(),
    })
}

impl TaskCache for TaskStore {
    async fn upsert(&self, task: &Task) -> Result<(), PortError> {
        let json = encode_task(task)?;
        sqlx::query!(
            "INSERT INTO tasks (scope, record_id, data_json, last_modified_ms)
             VALUES (?, ?, ?, ?)
             ON CONFLICT (scope, record_id) DO UPDATE SET
               data_json = excluded.data_json,
               last_modified_ms = excluded.last_modified_ms",
            self.scope,
            task.record_id,
            json,
            task.last_modified_ms
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    async fn get(&self, record_id: &str) -> Result<Option<CachedTask>, PortError> {
        let row = sqlx::query!(
            "SELECT data_json, flag_kind, flag_detail FROM tasks
             WHERE scope = ? AND record_id = ?",
            self.scope,
            record_id
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(CachedTask {
            task: decode_task(&row.data_json)?,
            flag: decode_flag(row.flag_kind.as_deref(), row.flag_detail),
        }))
    }

    async fn list(&self) -> Result<Vec<CachedTask>, PortError> {
        let rows = sqlx::query!(
            "SELECT record_id, data_json, flag_kind, flag_detail FROM tasks
             WHERE scope = ? ORDER BY record_id",
            self.scope
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows
            .into_iter()
            .filter_map(|row| match decode_task(&row.data_json) {
                Ok(task) => Some(CachedTask {
                    task,
                    flag: decode_flag(row.flag_kind.as_deref(), row.flag_detail),
                }),
                Err(err) => {
                    tracing::warn!(record_id = %row.record_id, error = %err, "cache task rusak dilewati");
                    None
                }
            })
            .collect())
    }

    async fn set_flag(&self, record_id: &str, flag: Option<&CardFlag>) -> Result<(), PortError> {
        let kind = flag.map(|f| f.kind.as_str());
        let detail = flag.map(|f| f.detail.as_str());
        sqlx::query!(
            "UPDATE tasks SET flag_kind = ?, flag_detail = ? WHERE scope = ? AND record_id = ?",
            kind,
            detail,
            self.scope,
            record_id
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }
}

impl Outbox for TaskStore {
    async fn enqueue(&self, record_id: &str, op: &WriteOp, now_ms: i64) -> Result<i64, PortError> {
        let payload = serde_json::to_value(WriteOpJson::from(op)).map_err(StoreError::internal)?;
        Ok(self
            .store
            .enqueue(&self.scope, record_id, &payload, now_ms)
            .await?)
    }

    async fn due(&self, now_ms: i64, limit: u32) -> Result<Vec<QueuedWrite>, PortError> {
        let items = self
            .store
            .outbox_due(&self.scope, now_ms, i64::from(limit))
            .await?;
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            match serde_json::from_value::<WriteOpJson>(item.payload) {
                Ok(op) => out.push(QueuedWrite {
                    id: item.id,
                    record_id: item.record_id,
                    op: WriteOp::from(op),
                    attempts: u32::try_from(item.attempts).unwrap_or(u32::MAX),
                }),
                Err(err) => {
                    // Payload tak terbaca tidak boleh mengulang selamanya.
                    tracing::warn!(id = item.id, error = %err, "item outbox rusak dibatalkan");
                    self.store
                        .outbox_mark(item.id, OutboxStatus::Cancelled, Some("corrupt payload"))
                        .await?;
                }
            }
        }
        Ok(out)
    }

    async fn complete(&self, id: i64) -> Result<(), PortError> {
        Ok(self.store.outbox_mark(id, OutboxStatus::Done, None).await?)
    }

    async fn cancel(&self, id: i64, reason: &str) -> Result<(), PortError> {
        Ok(self
            .store
            .outbox_mark(id, OutboxStatus::Cancelled, Some(reason))
            .await?)
    }

    async fn retry_later(
        &self,
        id: i64,
        attempts: u32,
        next_attempt_ms: i64,
        error: &str,
    ) -> Result<(), PortError> {
        let attempts = i64::from(attempts);
        sqlx::query!(
            "UPDATE outbox SET attempts = ?, next_attempt_ms = ?, last_error = ? WHERE id = ?",
            attempts,
            next_attempt_ms,
            error,
            id
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    async fn fail(&self, id: i64, error: &str) -> Result<(), PortError> {
        Ok(self
            .store
            .outbox_mark(id, OutboxStatus::Failed, Some(error))
            .await?)
    }

    async fn requeue_failed(&self, record_id: &str, now_ms: i64) -> Result<u32, PortError> {
        let res = sqlx::query!(
            "UPDATE outbox SET status = 'pending', attempts = 0, next_attempt_ms = ?
             WHERE queue = ? AND record_id = ? AND status = 'failed'",
            now_ms,
            self.scope,
            record_id
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(u32::try_from(res.rows_affected()).unwrap_or(u32::MAX))
    }
}

impl Store {
    /// Merusak JSON satu task; hanya untuk test ketahanan pembacaan.
    #[doc(hidden)]
    pub async fn corrupt_task_for_test(&self, scope: &str, record_id: &str) {
        let _ = sqlx::query!(
            "UPDATE tasks SET data_json = '{rusak' WHERE scope = ? AND record_id = ?",
            scope,
            record_id
        )
        .execute(&self.pool)
        .await;
    }
}
