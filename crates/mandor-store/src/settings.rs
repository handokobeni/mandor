//! Setelan global kunci-nilai (KTD-37), mis. model dan batas reviewer.

use crate::{Store, StoreError};

impl Store {
    /// Nilai setelan; `None` bila belum pernah diatur.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila pembacaan gagal.
    pub async fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        let row = sqlx::query!("SELECT value FROM settings WHERE key = ?", key)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        Ok(row.map(|r| r.value))
    }

    /// Menyimpan setelan; nilai lama diganti.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila penulisan gagal.
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<(), StoreError> {
        sqlx::query!(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            key,
            value
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    /// Setelan workspace (uji browser, mode gerbang) sebagai JSON.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila workspace tidak ada.
    pub async fn workspace_settings(&self, id: &str) -> Result<String, StoreError> {
        let row = sqlx::query!("SELECT settings_json FROM workspaces WHERE id = ?", id)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        row.map(|r| r.settings_json).ok_or(StoreError::NotFound)
    }

    /// Mengganti setelan workspace.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila workspace tidak ada.
    pub async fn set_workspace_settings(&self, id: &str, json: &str) -> Result<(), StoreError> {
        let done = sqlx::query!(
            "UPDATE workspaces SET settings_json = ? WHERE id = ?",
            json,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }
}
