use crate::{Store, StoreError, from_json, to_json};

/// Status item outbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxStatus {
    Pending,
    Done,
    Cancelled,
    Failed,
}

impl OutboxStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

/// Item tulis Lark yang menunggu dikirim.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxItem {
    pub id: i64,
    pub queue: String,
    pub record_id: String,
    pub payload: serde_json::Value,
    pub attempts: i64,
    pub last_error: Option<String>,
}

impl Store {
    /// Memasukkan tulisan ke antrean `queue` (satu antrean per tabel Lark).
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn enqueue(
        &self,
        queue: &str,
        record_id: &str,
        payload: &serde_json::Value,
        now_ms: i64,
    ) -> Result<i64, StoreError> {
        let json = to_json(payload);
        let res = sqlx::query!(
            "INSERT INTO outbox (queue, record_id, payload_json, next_attempt_ms, created_at_ms)
             VALUES (?, ?, ?, ?, ?)",
            queue,
            record_id,
            json,
            now_ms,
            now_ms
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(res.last_insert_rowid())
    }

    /// Item `pending` yang jatuh tempo, urut masuk.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn outbox_due(
        &self,
        queue: &str,
        now_ms: i64,
        limit: i64,
    ) -> Result<Vec<OutboxItem>, StoreError> {
        let rows = sqlx::query!(
            "SELECT id, queue, record_id, payload_json, attempts, last_error FROM outbox
             WHERE queue = ? AND status = 'pending' AND next_attempt_ms <= ?
             ORDER BY id LIMIT ?",
            queue,
            now_ms,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|row| {
                Ok(OutboxItem {
                    id: row.id,
                    queue: row.queue,
                    record_id: row.record_id,
                    payload: from_json(&row.payload_json)?,
                    attempts: row.attempts,
                    last_error: row.last_error,
                })
            })
            .collect()
    }

    /// Mengubah status akhir item.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn outbox_mark(
        &self,
        id: i64,
        status: OutboxStatus,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let status = status.as_str();
        sqlx::query!(
            "UPDATE outbox SET status = ?, last_error = COALESCE(?, last_error) WHERE id = ?",
            status,
            error,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }
}
