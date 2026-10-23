//! Daftar sumber desain global (KTD-33). Konfigurasi per jenis disimpan sebagai
//! JSON agar jenis baru (Rilis 2.1) tidak butuh migrasi.

use crate::{Store, StoreError};

/// Sumber desain terdaftar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignSourceRow {
    pub id: String,
    pub name: String,
    /// `folder` di 2.0; `figma`, `url`, `images` di 2.1.
    pub kind: String,
    pub config_json: String,
    pub created_at_ms: i64,
}

impl Store {
    /// Mendaftarkan sumber desain.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] bila id atau nama sudah dipakai.
    pub async fn insert_design_source(&self, s: &DesignSourceRow) -> Result<(), StoreError> {
        sqlx::query!(
            "INSERT INTO design_sources (id, name, kind, config_json, created_at_ms)
             VALUES (?, ?, ?, ?, ?)",
            s.id,
            s.name,
            s.kind,
            s.config_json,
            s.created_at_ms
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "a design source with this name already exists"))?;
        Ok(())
    }

    /// Mengubah nama dan konfigurasi sumber desain.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila sumber tidak ada, [`StoreError::Conflict`]
    /// bila nama baru sudah dipakai.
    pub async fn update_design_source(
        &self,
        id: &str,
        name: &str,
        config_json: &str,
    ) -> Result<(), StoreError> {
        let done = sqlx::query!(
            "UPDATE design_sources SET name = ?, config_json = ? WHERE id = ?",
            name,
            config_json,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "a design source with this name already exists"))?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Menghapus sumber desain.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila sumber tidak ada.
    pub async fn remove_design_source(&self, id: &str) -> Result<(), StoreError> {
        let done = sqlx::query!("DELETE FROM design_sources WHERE id = ?", id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Semua sumber desain, urut nama.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila pembacaan gagal.
    pub async fn design_sources(&self) -> Result<Vec<DesignSourceRow>, StoreError> {
        let rows = sqlx::query!(
            "SELECT id, name, kind, config_json, created_at_ms FROM design_sources ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows
            .into_iter()
            .map(|r| DesignSourceRow {
                id: r.id,
                name: r.name,
                kind: r.kind,
                config_json: r.config_json,
                created_at_ms: r.created_at_ms,
            })
            .collect())
    }
}
