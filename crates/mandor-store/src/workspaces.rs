//! Workspace multi-repo: repo terdaftar di bawah satu folder induk.

use crate::{Store, StoreError};

/// Data untuk mendaftarkan workspace.
#[derive(Debug, Clone)]
pub struct NewWorkspace {
    pub id: String,
    pub name: String,
    pub root_path: String,
    /// Urutan repo dipertahankan (repo pertama = repo utama run).
    pub repo_ids: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub now_ms: i64,
}

/// Workspace terdaftar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub repo_ids: Vec<String>,
    pub mcp_servers: Vec<String>,
}

fn names_json(names: &[String]) -> Result<String, StoreError> {
    serde_json::to_string(names).map_err(StoreError::internal)
}

impl Store {
    /// Mendaftarkan workspace beserta repo-nya.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] bila id sudah dipakai atau repo tidak terdaftar.
    pub async fn insert_workspace(&self, w: &NewWorkspace) -> Result<(), StoreError> {
        let mcp = names_json(&w.mcp_servers)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        sqlx::query!(
            "INSERT INTO workspaces (id, name, root_path, mcp_servers_json, created_at_ms) VALUES (?, ?, ?, ?, ?)",
            w.id,
            w.name,
            w.root_path,
            mcp,
            w.now_ms
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "workspace is already registered"))?;
        set_repos(&mut tx, &w.id, &w.repo_ids).await?;
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(())
    }

    /// Mengganti daftar repo dan MCP server workspace.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila workspace tidak ada.
    pub async fn update_workspace(
        &self,
        id: &str,
        repo_ids: &[String],
        mcp_servers: &[String],
    ) -> Result<(), StoreError> {
        let mcp = names_json(mcp_servers)?;
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        let updated = sqlx::query!(
            "UPDATE workspaces SET mcp_servers_json = ? WHERE id = ?",
            mcp,
            id
        )
        .execute(&mut *tx)
        .await
        .map_err(StoreError::internal)?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        sqlx::query!("DELETE FROM workspace_repos WHERE workspace_id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        set_repos(&mut tx, id, repo_ids).await?;
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(())
    }

    /// Satu workspace.
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila tidak ada.
    pub async fn get_workspace(&self, id: &str) -> Result<Workspace, StoreError> {
        let row = sqlx::query!(
            "SELECT id, name, root_path, mcp_servers_json FROM workspaces WHERE id = ?",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::internal)?
        .ok_or(StoreError::NotFound)?;
        Ok(Workspace {
            repo_ids: self.workspace_repo_ids(&row.id).await?,
            mcp_servers: serde_json::from_str(&row.mcp_servers_json).unwrap_or_default(),
            id: row.id,
            name: row.name,
            root_path: row.root_path,
        })
    }

    /// Semua workspace, urut nama.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>, StoreError> {
        let ids = sqlx::query_scalar!("SELECT id FROM workspaces ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(self.get_workspace(&id).await?);
        }
        Ok(out)
    }

    /// Menghapus workspace; repo-nya tetap terdaftar.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila query gagal.
    pub async fn remove_workspace(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query!("DELETE FROM workspaces WHERE id = ?", id)
            .execute(&self.pool)
            .await
            .map_err(StoreError::internal)?;
        Ok(())
    }

    async fn workspace_repo_ids(&self, id: &str) -> Result<Vec<String>, StoreError> {
        sqlx::query_scalar!(
            "SELECT repo_id FROM workspace_repos WHERE workspace_id = ? ORDER BY position",
            id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)
    }
}

async fn set_repos(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
    repo_ids: &[String],
) -> Result<(), StoreError> {
    for (i, repo_id) in repo_ids.iter().enumerate() {
        let position = i64::try_from(i).map_err(StoreError::internal)?;
        sqlx::query!(
            "INSERT INTO workspace_repos (workspace_id, repo_id, position) VALUES (?, ?, ?)",
            id,
            repo_id,
            position
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "repo is not registered"))?;
    }
    Ok(())
}
