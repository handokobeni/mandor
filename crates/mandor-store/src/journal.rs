use crate::{Store, StoreError};

impl Store {
    /// Mencatat balasan sebagai `pending` sebelum dikirim ke agent (KTD-12).
    /// Tidak mengubah entri yang sudah ada.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn journal_pending(
        &self,
        record_id: &str,
        reply_id: &str,
        run_id: &str,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            "INSERT INTO comment_journal (record_id, reply_id, run_id, status, created_at_ms)
             VALUES (?, ?, ?, 'pending', ?) ON CONFLICT DO NOTHING",
            record_id,
            reply_id,
            run_id,
            now_ms
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    /// Menandai balasan sudah diterima proses agent.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn journal_delivered(
        &self,
        record_id: &str,
        reply_id: &str,
    ) -> Result<(), StoreError> {
        sqlx::query!(
            "UPDATE comment_journal SET status = 'delivered' WHERE record_id = ? AND reply_id = ?",
            record_id,
            reply_id
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    /// Balasan yang masih `pending` untuk record (dikirim ulang saat restart).
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn journal_pending_list(&self, record_id: &str) -> Result<Vec<String>, StoreError> {
        sqlx::query_scalar!(
            "SELECT reply_id FROM comment_journal
             WHERE record_id = ? AND status = 'pending' ORDER BY created_at_ms, reply_id",
            record_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)
    }

    /// Balasan yang sudah `delivered` untuk record.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn journal_delivered_list(&self, record_id: &str) -> Result<Vec<String>, StoreError> {
        sqlx::query_scalar!(
            "SELECT reply_id FROM comment_journal
             WHERE record_id = ? AND status = 'delivered' ORDER BY created_at_ms, reply_id",
            record_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)
    }
}
