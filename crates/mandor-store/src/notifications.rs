use crate::{Store, StoreError, from_json, to_json};

/// Jumlah notifikasi terbaru yang disimpan sebagai riwayat.
pub const NOTIFICATIONS_KEPT: i64 = 500;

/// Notifikasi untuk dev beserta status kirim dan baca.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at_ms: i64,
    pub read: bool,
}

impl Store {
    /// Menyimpan notifikasi ke riwayat. `delivered` = sudah dikirim langsung ke
    /// client yang tersambung; yang belum diambil saat client tersambung.
    /// Riwayat dipangkas ke [`NOTIFICATIONS_KEPT`] terbaru.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn push_notification(
        &self,
        kind: &str,
        payload: &serde_json::Value,
        now_ms: i64,
        delivered: bool,
    ) -> Result<i64, StoreError> {
        let json = to_json(payload);
        let delivered = i64::from(delivered);
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        let id = sqlx::query!(
            "INSERT INTO notifications (kind, payload_json, created_at_ms, delivered) VALUES (?, ?, ?, ?)",
            kind,
            json,
            now_ms,
            delivered
        )
        .execute(&mut *tx)
        .await
        .map_err(StoreError::internal)?
        .last_insert_rowid();
        let floor = id - NOTIFICATIONS_KEPT;
        sqlx::query!("DELETE FROM notifications WHERE id <= ?", floor)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(id)
    }

    /// Mengambil notifikasi yang belum terkirim lalu menandainya terkirim.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn take_notifications(&self, limit: i64) -> Result<Vec<Notification>, StoreError> {
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        let rows = sqlx::query!(
            "SELECT id, kind, payload_json, created_at_ms, read FROM notifications
             WHERE delivered = 0 ORDER BY id LIMIT ?",
            limit
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(StoreError::internal)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            sqlx::query!(
                "UPDATE notifications SET delivered = 1 WHERE id = ?",
                row.id
            )
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
            out.push(Notification {
                id: row.id,
                kind: row.kind,
                payload: from_json(&row.payload_json)?,
                created_at_ms: row.created_at_ms,
                read: row.read != 0,
            });
        }
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(out)
    }

    /// Riwayat notifikasi, terbaru dulu.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn list_notifications(&self, limit: i64) -> Result<Vec<Notification>, StoreError> {
        let rows = sqlx::query!(
            "SELECT id, kind, payload_json, created_at_ms, read FROM notifications
             ORDER BY id DESC LIMIT ?",
            limit
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|row| {
                Ok(Notification {
                    id: row.id,
                    kind: row.kind,
                    payload: from_json(&row.payload_json)?,
                    created_at_ms: row.created_at_ms,
                    read: row.read != 0,
                })
            })
            .collect()
    }

    /// Menandai notifikasi sudah dibaca; `None` = semuanya.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn mark_notifications_read(&self, ids: Option<&[i64]>) -> Result<(), StoreError> {
        let Some(ids) = ids else {
            sqlx::query!("UPDATE notifications SET read = 1 WHERE read = 0")
                .execute(&self.pool)
                .await
                .map_err(StoreError::internal)?;
            return Ok(());
        };
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        for id in ids {
            sqlx::query!("UPDATE notifications SET read = 1 WHERE id = ?", id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::internal)?;
        }
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(())
    }
}
