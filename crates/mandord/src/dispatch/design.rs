//! Method `design_sources.*`: sumber desain folder atau repo (KTD-33, R3).

use std::path::{Path, PathBuf};
use std::time::Duration;

use mandor_protocol::{
    DesignSourceAdd, DesignSourceCheckView, DesignSourceRef, DesignSourceTest, DesignSourceUpdate,
    DesignSourceView, RpcError, method,
};
use mandor_store::DesignSourceRow;
use mandor_usecase::review::{DesignRoot, PathProbe, SourceCheck, check_source};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{AppDispatch, ok, params, rejected, store_error};
use crate::prod::now_ms;

/// Jenis sumber desain Rilis 2.0.
const FOLDER: &str = "folder";
/// Batas waktu memeriksa satu path.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Konfigurasi sumber berjenis folder.
#[derive(Debug, Serialize, Deserialize)]
struct FolderConfig {
    path: String,
}

/// Memeriksa path di sistem berkas, dibatasi waktu.
pub(super) async fn probe(path: &Path) -> PathProbe {
    let check = async {
        let Ok(meta) = tokio::fs::metadata(path).await else {
            return PathProbe::default();
        };
        PathProbe {
            exists: true,
            is_dir: meta.is_dir(),
            canonical: tokio::fs::canonicalize(path).await.ok(),
        }
    };
    tokio::time::timeout(PROBE_TIMEOUT, check)
        .await
        .unwrap_or_default()
}

pub(crate) async fn check(path: &str) -> SourceCheck {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return SourceCheck::Missing;
    }
    check_source(&path, &probe(&path).await)
}

pub(crate) fn folder_path(row: &DesignSourceRow) -> String {
    serde_json::from_str::<FolderConfig>(&row.config_json)
        .map(|c| c.path)
        .unwrap_or_default()
}

fn config_json(path: &str) -> Result<String, RpcError> {
    serde_json::to_string(&FolderConfig {
        path: path.to_owned(),
    })
    .map_err(rejected)
}

async fn view(row: DesignSourceRow) -> DesignSourceView {
    let path = folder_path(&row);
    let status = check(&path).await;
    DesignSourceView {
        ready: matches!(status, SourceCheck::Ready(_)),
        status: status.message().to_owned(),
        id: row.id,
        name: row.name,
        kind: row.kind,
        path,
    }
}

/// Sumber yang tidak layak tidak bisa didaftarkan.
async fn ready_path(path: &str) -> Result<(), RpcError> {
    match check(path.trim()).await {
        SourceCheck::Ready(_) => Ok(()),
        other => Err(rejected(other.message())),
    }
}

fn valid_name(name: &str) -> Result<String, RpcError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(rejected("Name is required"));
    }
    Ok(name.to_owned())
}

impl AppDispatch {
    pub(super) fn is_design_method(name: &str) -> bool {
        [
            method::DESIGN_SOURCES_LIST,
            method::DESIGN_SOURCES_ADD,
            method::DESIGN_SOURCES_UPDATE,
            method::DESIGN_SOURCES_REMOVE,
            method::DESIGN_SOURCES_TEST,
        ]
        .contains(&name)
    }

    pub(super) async fn design_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let store = &self.app.store;
        match name {
            method::DESIGN_SOURCES_ADD => {
                let p: DesignSourceAdd = params(v)?;
                let name = valid_name(&p.name)?;
                ready_path(&p.path).await?;
                let row = DesignSourceRow {
                    id: uuid_like(),
                    name,
                    kind: FOLDER.to_owned(),
                    config_json: config_json(p.path.trim())?,
                    created_at_ms: now_ms(),
                };
                store
                    .insert_design_source(&row)
                    .await
                    .map_err(store_error)?;
                ok(&view(row).await)
            }
            method::DESIGN_SOURCES_UPDATE => {
                let p: DesignSourceUpdate = params(v)?;
                let name = valid_name(&p.name)?;
                ready_path(&p.path).await?;
                store
                    .update_design_source(&p.id, &name, &config_json(p.path.trim())?)
                    .await
                    .map_err(store_error)?;
                let row = self.design_row(&p.id).await?;
                ok(&view(row).await)
            }
            method::DESIGN_SOURCES_REMOVE => {
                let p: DesignSourceRef = params(v)?;
                store
                    .remove_design_source(&p.id)
                    .await
                    .map_err(store_error)?;
                ok(&json!({}))
            }
            method::DESIGN_SOURCES_TEST => {
                let p: DesignSourceTest = params(v)?;
                let status = check(p.path.trim()).await;
                ok(&DesignSourceCheckView {
                    ready: matches!(status, SourceCheck::Ready(_)),
                    status: status.message().to_owned(),
                })
            }
            _ => {
                let rows = store.design_sources().await.map_err(store_error)?;
                let mut out = Vec::with_capacity(rows.len());
                for row in rows {
                    out.push(view(row).await);
                }
                ok(&out)
            }
        }
    }

    async fn design_row(&self, id: &str) -> Result<DesignSourceRow, RpcError> {
        self.app
            .store
            .design_sources()
            .await
            .map_err(store_error)?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| store_error(mandor_store::StoreError::NotFound))
    }

    /// Sumber desain pilihan Start, sudah diperiksa dan dikanonisasi. Sumber
    /// yang hilang atau tidak layak menolak Start dengan menyebut namanya.
    pub(super) async fn design_roots(&self, ids: &[String]) -> Result<Vec<DesignRoot>, RpcError> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let row = self
                .design_row(id)
                .await
                .map_err(|_| rejected("A selected design source was removed; pick again"))?;
            match check(&folder_path(&row)).await {
                SourceCheck::Ready(path) => out.push(DesignRoot {
                    id: row.id,
                    name: row.name,
                    path,
                }),
                other => {
                    return Err(rejected(format!(
                        "Design source \"{}\" is not available: {}",
                        row.name,
                        other.message()
                    )));
                }
            }
        }
        Ok(out)
    }
}

/// Id acak untuk sumber desain baru.
fn uuid_like() -> String {
    mandor_agent::new_session_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pemeriksaan_path_relatif_ditolak_dan_folder_dikenali() {
        assert_eq!(check("relatif/x").await, SourceCheck::Missing);
        let dir = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(dir.path()).unwrap();
        assert_eq!(
            check(dir.path().to_str().unwrap()).await,
            SourceCheck::Ready(canonical)
        );
        let row = DesignSourceRow {
            id: "x".to_owned(),
            name: "x".to_owned(),
            kind: FOLDER.to_owned(),
            config_json: "{rusak".to_owned(),
            created_at_ms: 0,
        };
        assert_eq!(folder_path(&row), "");
        assert!(valid_name("  ").is_err());
    }
}
