use crate::{Store, StoreError, from_json, to_json};

/// Satu pesan chat run.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: i64,
    pub run_id: String,
    pub role: String,
    pub body: serde_json::Value,
    pub created_at_ms: i64,
}

impl Store {
    /// Menambah pesan dan mengembalikan id-nya.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn append_message(
        &self,
        run_id: &str,
        role: &str,
        body: &serde_json::Value,
        now_ms: i64,
    ) -> Result<i64, StoreError> {
        let json = to_json(body);
        let res = sqlx::query!(
            "INSERT INTO messages (run_id, role, body_json, created_at_ms) VALUES (?, ?, ?, ?)",
            run_id,
            role,
            json,
            now_ms
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(res.last_insert_rowid())
    }

    /// Pesan setelah `after_id`, paling banyak `limit` (paginasi keyset).
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn list_messages(
        &self,
        run_id: &str,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<Message>, StoreError> {
        let rows = sqlx::query!(
            "SELECT id, run_id, role, body_json, created_at_ms FROM messages
             WHERE run_id = ? AND id > ? ORDER BY id LIMIT ?",
            run_id,
            after_id,
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|row| {
                Ok(Message {
                    id: row.id,
                    run_id: row.run_id,
                    role: row.role,
                    body: from_json(&row.body_json)?,
                    created_at_ms: row.created_at_ms,
                })
            })
            .collect()
    }
}
