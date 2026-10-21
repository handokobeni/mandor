use crate::{Store, StoreError};

impl Store {
    /// Menutup run (selesai atau dibatalkan).
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn close_run(&self, id: &str, now_ms: i64) -> Result<(), StoreError> {
        sqlx::query!(
            "UPDATE runs SET closed = 1, updated_at_ms = ? WHERE id = ?",
            now_ms,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }
}
