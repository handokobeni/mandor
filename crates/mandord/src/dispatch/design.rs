//! Method `design_sources.*`: sumber desain folder atau repo (KTD-33, R3).

use std::path::{Path, PathBuf};
use std::time::Duration;

use mandor_domain::app_run::{AppSettings, PortUse, TestMode, check_prototype_url, port_conflict};
use mandor_protocol::{
    DesignSourceAdd, DesignSourceCheckView, DesignSourceRef, DesignSourceTest, DesignSourceUpdate,
    DesignSourceView, PrototypeConfirm, PrototypeRevisionView, RepoAppView, RpcError, method,
};
use mandor_store::DesignSourceRow;
use mandor_usecase::review::DesignSources;
use mandor_usecase::review::{DesignRoot, PathProbe, SourceCheck, check_source};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::app_run::{app_settings_from, check_app_port, repo_app_view};
use super::{AppDispatch, ok, params, rejected, store_error};
use crate::prod::now_ms;

/// Jenis sumber desain Rilis 2.0.
const FOLDER: &str = "folder";
/// Batas waktu memeriksa satu path.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Konfigurasi sumber berjenis folder; `app` berisi setelan prototype yang
/// bisa dijalankan (plan 2026-11-03-002, KTD-P2).
#[derive(Debug, Serialize, Deserialize)]
struct FolderConfig {
    path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app: Option<Value>,
    /// Revisi folder yang terakhir dikonfirmasi dev (R15, R28).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confirmed_rev: Option<String>,
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

/// Setelan app sumber yang bisa dijalankan; `None` untuk folder biasa (R6).
pub(crate) fn prototype_app(row: &DesignSourceRow) -> Option<AppSettings> {
    let app = serde_json::from_str::<FolderConfig>(&row.config_json)
        .ok()?
        .app?;
    Some(mandor_store::app_settings_of(&json!({ "app": app }))).filter(AppSettings::runnable)
}

/// Revisi prototype yang terakhir dikonfirmasi dev.
pub(crate) fn confirmed_revision(row: &DesignSourceRow) -> Option<String> {
    serde_json::from_str::<FolderConfig>(&row.config_json)
        .ok()?
        .confirmed_rev
}

/// Konfigurasi `row` dengan revisi terkonfirmasi `rev`.
pub(crate) fn with_confirmed_revision(row: &DesignSourceRow, rev: &str) -> Option<String> {
    let mut config = serde_json::from_str::<FolderConfig>(&row.config_json).ok()?;
    config.confirmed_rev = Some(rev.to_owned());
    serde_json::to_string(&config).ok()
}

/// Konfigurasi baru; menyimpan setelan mereset revisi terkonfirmasi sehingga
/// dev mengonfirmasi ulang kode prototype.
fn config_json(path: &str, app: Option<&AppSettings>) -> Result<String, RpcError> {
    serde_json::to_string(&FolderConfig {
        path: path.to_owned(),
        app: app.map(mandor_store::app_settings_json),
        confirmed_rev: None,
    })
    .map_err(rejected)
}

async fn view(row: DesignSourceRow) -> DesignSourceView {
    let path = folder_path(&row);
    let status = check(&path).await;
    DesignSourceView {
        ready: matches!(status, SourceCheck::Ready(_)),
        status: status.message().to_owned(),
        app: prototype_app(&row).map(|a| repo_app_view(&a)),
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
            method::DESIGN_SOURCES_REVISION,
            method::DESIGN_SOURCES_CONFIRM,
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
                let app = match &p.app {
                    Some(view) => Some(self.prototype_settings(view, None).await?),
                    None => None,
                };
                let row = DesignSourceRow {
                    id: uuid_like(),
                    name,
                    kind: FOLDER.to_owned(),
                    config_json: config_json(p.path.trim(), app.as_ref())?,
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
                let app = match &p.app {
                    Some(view) => Some(self.prototype_settings(view, Some(&p.id)).await?),
                    None => None,
                };
                store
                    .update_design_source(&p.id, &name, &config_json(p.path.trim(), app.as_ref())?)
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
            method::DESIGN_SOURCES_REVISION => {
                let p: DesignSourceRef = params(v)?;
                ok(&self.prototype_revision(&p.id).await?)
            }
            method::DESIGN_SOURCES_CONFIRM => {
                let p: PrototypeConfirm = params(v)?;
                let current = self.prototype_revision(&p.id).await?;
                if current.current != p.revision {
                    return Err(rejected(
                        "The prototype changed since you checked it; review the changes again",
                    ));
                }
                self.design_port()
                    .confirm(&p.id, &p.revision)
                    .await
                    .map_err(|e| rejected(e.to_string()))?;
                ok(&PrototypeRevisionView {
                    confirmed: Some(p.revision),
                    files: Vec::new(),
                    ..current
                })
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

    /// Adapter sumber desain tanpa sesi Lark (revisi dan konfirmasi prototype).
    fn design_port(&self) -> crate::design_port::DesignStore {
        let mut cfg = mandor_git::GitConfig::new(self.app.cfg.workspaces_dir());
        cfg.gh_bin.clone_from(&self.app.cfg.gh_bin);
        cfg.distrust_repo_config = self.app.cfg.vm_agent.is_some();
        crate::design_port::DesignStore::new(
            self.app.store.clone(),
            mandor_git::Git::new(cfg),
            self.app.cfg.home.clone(),
        )
    }

    /// Revisi prototype `id` saat ini, yang terakhir dikonfirmasi, dan berkas
    /// yang berubah sejak itu (R15, R28).
    async fn prototype_revision(&self, id: &str) -> Result<PrototypeRevisionView, RpcError> {
        let port = self.design_port();
        let p = port
            .prototype(id)
            .await
            .map_err(|e| rejected(e.to_string()))?
            .ok_or_else(|| rejected("This design source is not a runnable prototype"))?;
        let current = port
            .revision(&p.path)
            .await
            .map_err(|e| rejected(e.to_string()))?;
        let files = match p.confirmed.as_deref() {
            Some(from) if from != current => port
                .changes_since(&p.path, from)
                .await
                .map_err(|e| rejected(e.to_string()))?,
            _ => Vec::new(),
        };
        Ok(PrototypeRevisionView {
            current,
            confirmed: p.confirmed,
            files,
        })
    }

    /// Setelan prototype dari input dev: harus bisa dijalankan, port tidak
    /// bentrok dengan app repo atau prototype lain, di rentang port VM, dan
    /// URL loopback dengan host berbeda dari app (R7, R8).
    async fn prototype_settings(
        &self,
        view: &RepoAppView,
        design_id: Option<&str>,
    ) -> Result<AppSettings, RpcError> {
        let mut app = app_settings_from(view)?;
        app.test_mode = TestMode::Web;
        let Some(port) = app.port.filter(|_| app.runnable()) else {
            return Err(rejected(
                "A runnable prototype needs a start command and a port",
            ));
        };
        let url = app
            .app_url
            .get_or_insert_with(|| format!("http://127.0.0.1:{port}"))
            .clone();
        check_app_port(self.app.cfg.vm_agent.as_ref().map(|v| v.ports), &app)?;
        let (repos, repo_urls) = self.repo_ports().await?;
        let mut taken = repos;
        taken.extend(self.prototype_ports(design_id).await?);
        if let Some(u) = port_conflict(app.port, &taken) {
            return Err(rejected(format!(
                "Port {port} is already used by {}",
                u.owner
            )));
        }
        check_prototype_url(&url, port, &repo_urls).map_err(|e| rejected(e.to_string()))?;
        Ok(app)
    }

    /// Port dan URL app repo Web di daemon ini.
    async fn repo_ports(&self) -> Result<(Vec<PortUse>, Vec<String>), RpcError> {
        let mut ports = Vec::new();
        let mut urls = Vec::new();
        for r in self.app.store.list_repos().await.map_err(store_error)? {
            let app = mandor_store::app_settings_of(&r.settings);
            if app.test_mode != TestMode::Web {
                continue;
            }
            urls.extend(app.canonical_url());
            if let Some(port) = app.port {
                ports.push(PortUse { owner: r.id, port });
            }
        }
        Ok((ports, urls))
    }

    /// Port prototype terdaftar, kecuali sumber `skip`.
    async fn prototype_ports(&self, skip: Option<&str>) -> Result<Vec<PortUse>, RpcError> {
        Ok(self
            .app
            .store
            .design_sources()
            .await
            .map_err(store_error)?
            .into_iter()
            .filter(|r| Some(r.id.as_str()) != skip)
            .filter_map(|r| {
                let port = prototype_app(&r)?.port?;
                Some(PortUse {
                    owner: r.name,
                    port,
                })
            })
            .collect())
    }

    /// Port app repo tidak boleh dipakai prototype (R8, dua arah).
    pub(super) async fn check_repo_port(&self, app: &AppSettings) -> Result<(), RpcError> {
        if app.test_mode != TestMode::Web {
            return Ok(());
        }
        match port_conflict(app.port, &self.prototype_ports(None).await?) {
            Some(u) => Err(rejected(format!(
                "Port {} is already used by the prototype {}",
                u.port, u.owner
            ))),
            None => Ok(()),
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
