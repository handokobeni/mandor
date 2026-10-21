use crate::{Store, StoreError, from_json, to_json};

/// Data untuk mendaftarkan repo.
#[derive(Debug, Clone)]
pub struct NewRepo {
    pub id: String,
    pub path: String,
    pub gh_profile: Option<String>,
    pub default_branch: Option<String>,
    pub now_ms: i64,
}

/// Repo terdaftar beserta setting-nya (JSON bebas milik use case).
#[derive(Debug, Clone, PartialEq)]
pub struct Repo {
    pub id: String,
    pub path: String,
    pub gh_profile: Option<String>,
    pub default_branch: Option<String>,
    pub settings: serde_json::Value,
}

impl Store {
    /// Mendaftarkan repo baru.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] bila path atau id sudah terdaftar.
    pub async fn insert_repo(&self, r: &NewRepo) -> Result<(), StoreError> {
        sqlx::query!(
            "INSERT INTO repos (id, path, gh_profile, default_branch, created_at_ms) VALUES (?, ?, ?, ?, ?)",
            r.id,
            r.path,
            r.gh_profile,
            r.default_branch,
            r.now_ms
        )
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "repo is already registered"))?;
        Ok(())
    }

    /// Mengambil satu repo.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila tidak ada.
    pub async fn get_repo(&self, id: &str) -> Result<Repo, StoreError> {
        let row = sqlx::query!(
            "SELECT id, path, gh_profile, default_branch, settings_json FROM repos WHERE id = ?",
            id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "repo"))?;
        Ok(Repo {
            id: row.id,
            path: row.path,
            gh_profile: row.gh_profile,
            default_branch: row.default_branch,
            settings: from_json(&row.settings_json)?,
        })
    }

    /// Semua repo, urut menurut path.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn list_repos(&self) -> Result<Vec<Repo>, StoreError> {
        let rows = sqlx::query!(
            "SELECT id, path, gh_profile, default_branch, settings_json FROM repos ORDER BY path"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|row| {
                Ok(Repo {
                    id: row.id,
                    path: row.path,
                    gh_profile: row.gh_profile,
                    default_branch: row.default_branch,
                    settings: from_json(&row.settings_json)?,
                })
            })
            .collect()
    }

    /// Mengganti setting repo.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila repo tidak ada.
    pub async fn set_repo_settings(
        &self,
        id: &str,
        settings: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let json = to_json(settings);
        let res = sqlx::query!("UPDATE repos SET settings_json = ? WHERE id = ?", json, id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Mengganti profil `gh` repo.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila repo tidak ada.
    pub async fn set_repo_gh_profile(
        &self,
        id: &str,
        profile: Option<&str>,
    ) -> Result<(), StoreError> {
        let res = sqlx::query!("UPDATE repos SET gh_profile = ? WHERE id = ?", profile, id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Menghapus repo beserta run-nya.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn remove_repo(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query!("DELETE FROM repos WHERE id = ?", id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        Ok(())
    }
}
