//! Method `workspaces.*`: workspace multi-repo (folder induk + repo terdaftar).

use std::path::PathBuf;

use mandor_protocol::{
    McpServersQuery, RepoRef, RpcError, WorkspaceAdd, WorkspaceUpdate, WorkspaceView,
};
use mandor_store::NewWorkspace;
use serde_json::{Value, json};

use super::{AppDispatch, ok, rejected, store_error};
use crate::prod::now_ms;

fn workspace_view(w: mandor_store::Workspace) -> WorkspaceView {
    WorkspaceView {
        id: w.id,
        name: w.name,
        root: w.root_path,
        repo_ids: w.repo_ids,
        mcp_servers: w.mcp_servers,
    }
}

/// ID dari nama: huruf kecil, selain alfanumerik menjadi `-`.
fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() {
        "workspace".to_owned()
    } else {
        s
    }
}

impl AppDispatch {
    pub(super) async fn workspaces_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        use mandor_protocol::method;
        match name {
            method::WORKSPACES_LIST => self.workspaces_list().await,
            method::WORKSPACES_ADD => self.workspaces_add(super::params(v)?).await,
            method::WORKSPACES_UPDATE => self.workspaces_update(super::params(v)?).await,
            method::WORKSPACES_REMOVE => {
                let p: RepoRef = super::params(v)?;
                self.app
                    .store
                    .remove_workspace(&p.id)
                    .await
                    .map_err(store_error)?;
                ok(&json!({}))
            }
            _ => {
                let q: McpServersQuery = super::params(v)?;
                let root = self.workspace_root(&q.root).await?;
                ok(&mandor_agent::mcp_server_names(&root)
                    .await
                    .map_err(rejected)?)
            }
        }
    }

    /// Method `workspaces.*` yang dikenal.
    pub(super) fn is_workspaces_method(name: &str) -> bool {
        use mandor_protocol::method;
        [
            method::WORKSPACES_LIST,
            method::WORKSPACES_ADD,
            method::WORKSPACES_UPDATE,
            method::WORKSPACES_REMOVE,
            method::WORKSPACES_MCP_SERVERS,
        ]
        .contains(&name)
    }

    async fn workspaces_list(&self) -> Result<Value, RpcError> {
        let all = self
            .app
            .store
            .list_workspaces()
            .await
            .map_err(store_error)?;
        let out: Vec<WorkspaceView> = all.into_iter().map(workspace_view).collect();
        ok(&out)
    }

    /// Folder induk: path absolut ke folder yang ada.
    async fn workspace_root(&self, root: &str) -> Result<PathBuf, RpcError> {
        let path = PathBuf::from(root);
        if !path.is_absolute() {
            return Err(rejected("workspace folder must be an absolute path"));
        }
        let path = tokio::fs::canonicalize(&path)
            .await
            .map_err(|e| rejected(format!("workspace folder not found: {e}")))?;
        let meta = tokio::fs::metadata(&path).await.map_err(rejected)?;
        if !meta.is_dir() {
            return Err(rejected("workspace folder is not a directory"));
        }
        Ok(path)
    }

    /// Repo harus terdaftar dan MCP server harus ada di `.mcp.json` folder induk.
    async fn validate_members(
        &self,
        root: &std::path::Path,
        repo_ids: &[String],
        mcp_servers: &[String],
    ) -> Result<(), RpcError> {
        if repo_ids.is_empty() {
            return Err(rejected("choose at least one repo"));
        }
        for id in repo_ids {
            self.app.store.get_repo(id).await.map_err(store_error)?;
        }
        let known = mandor_agent::mcp_server_names(root)
            .await
            .map_err(rejected)?;
        if let Some(bad) = mcp_servers.iter().find(|s| !known.contains(s)) {
            return Err(rejected(format!("MCP server {bad} is not in .mcp.json")));
        }
        Ok(())
    }

    async fn workspaces_add(&self, p: WorkspaceAdd) -> Result<Value, RpcError> {
        let root = self.workspace_root(&p.root).await?;
        self.validate_members(&root, &p.repo_ids, &p.mcp_servers)
            .await?;
        let existing = self
            .app
            .store
            .list_workspaces()
            .await
            .map_err(store_error)?;
        let base = slug(&p.name);
        let mut id = base.clone();
        let mut n = 2;
        while existing.iter().any(|w| w.id == id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        self.app
            .store
            .insert_workspace(&NewWorkspace {
                id: id.clone(),
                name: p.name.trim().to_owned(),
                root_path: root.display().to_string(),
                repo_ids: p.repo_ids,
                mcp_servers: p.mcp_servers,
                now_ms: now_ms(),
            })
            .await
            .map_err(store_error)?;
        ok(&workspace_view(
            self.app
                .store
                .get_workspace(&id)
                .await
                .map_err(store_error)?,
        ))
    }

    async fn workspaces_update(&self, p: WorkspaceUpdate) -> Result<Value, RpcError> {
        let current = self
            .app
            .store
            .get_workspace(&p.id)
            .await
            .map_err(store_error)?;
        let root = PathBuf::from(&current.root_path);
        self.validate_members(&root, &p.repo_ids, &p.mcp_servers)
            .await?;
        self.app
            .store
            .update_workspace(&p.id, &p.repo_ids, &p.mcp_servers)
            .await
            .map_err(store_error)?;
        ok(&workspace_view(
            self.app
                .store
                .get_workspace(&p.id)
                .await
                .map_err(store_error)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn slug_dari_nama_workspace() {
        assert_eq!(slug("Kelola V2"), "kelola-v2");
        assert_eq!(slug("  --  "), "workspace");
    }
}
