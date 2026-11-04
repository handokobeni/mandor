//! Method `app.*`: run app milik run (KTD-36).

use std::path::{Path, PathBuf};
use std::time::Duration;

use mandor_domain::app_run::{AppSettings, TestMode, env_key_names};
use mandor_protocol::{
    AppChangesView, AppConfirm, AppLogView, AppMemberView, AppRunParams, AppRunView, EnvKeysParams,
    EnvKeysView, RepoAppView, RpcError, RunRef, method,
};
use mandor_usecase::run_ports::AppRecord;
use serde_json::Value;

use super::{AppDispatch, ok, params, rejected, run_error, store_error};

/// Batas waktu membaca `.env` untuk Pick from .env.
const ENV_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Folder sumber `.env` untuk Pick from .env.
#[derive(Debug)]
enum EnvSource<'a> {
    Repo(&'a str),
    Design(&'a str),
}

/// Tepat satu dari repo atau sumber desain.
fn env_source(p: &EnvKeysParams) -> Result<EnvSource<'_>, RpcError> {
    match (p.repo_id.as_deref(), p.design_source_id.as_deref()) {
        (Some(id), None) => Ok(EnvSource::Repo(id)),
        (None, Some(id)) => Ok(EnvSource::Design(id)),
        _ => Err(rejected("Pass either a repo or a design source")),
    }
}

/// Nama kunci `.env` di `dir`; nilai dan isi berkas tidak pernah keluar,
/// termasuk di pesan error (R4, KTD-P1).
async fn read_env_keys(dir: &Path) -> Result<EnvKeysView, RpcError> {
    let read = tokio::time::timeout(
        ENV_READ_TIMEOUT,
        tokio::fs::read_to_string(dir.join(".env")),
    );
    match read.await {
        Ok(Ok(text)) => Ok(EnvKeysView {
            found: true,
            keys: env_key_names(&text),
        }),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(EnvKeysView {
            found: false,
            keys: Vec::new(),
        }),
        Ok(Err(e)) => {
            tracing::warn!(dir = %dir.display(), kind = ?e.kind(), "berkas .env tidak terbaca");
            Err(rejected("The .env file cannot be read"))
        }
        Err(_) => Err(rejected("Reading the .env file took too long")),
    }
}

pub(super) fn repo_app_view(a: &AppSettings) -> RepoAppView {
    RepoAppView {
        test_mode: a.test_mode.as_str().to_owned(),
        setup_command: a.setup_command.clone(),
        start_command: a.start_command.clone(),
        port: a.port,
        app_url: a.app_url.clone(),
        env_keys: a.env_keys.clone(),
        browser_tests: a.browser_tests,
        auto_restart: a.auto_restart,
        unattended_login: a.unattended_login,
    }
}

/// Pengaturan app dari input dev; kunci `.env` harus berupa nama variabel.
pub(super) fn app_settings_from(v: &RepoAppView) -> Result<AppSettings, RpcError> {
    let test_mode = TestMode::parse(&v.test_mode).ok_or_else(|| rejected("Unknown test mode"))?;
    let trim = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    let valid_key = |k: &String| {
        !k.is_empty()
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !k.starts_with(|c: char| c.is_ascii_digit())
    };
    if let Some(bad) = v.env_keys.iter().find(|k| !valid_key(k)) {
        return Err(rejected(format!("Invalid .env key: {bad}")));
    }
    if let Some(url) = trim(&v.app_url) {
        if mandor_domain::hook_review::origin_of(&url).is_none() {
            return Err(rejected("The app URL must start with http:// or https://"));
        }
    }
    Ok(AppSettings {
        test_mode,
        setup_command: trim(&v.setup_command),
        start_command: trim(&v.start_command),
        port: v.port,
        app_url: trim(&v.app_url),
        env_keys: v.env_keys.clone(),
        browser_tests: v.browser_tests,
        auto_restart: v.auto_restart,
        unattended_login: v.unattended_login,
    })
}

/// Di VM bersama port app repo harus di rentang port developer (R5 plan VM).
pub(super) fn check_app_port(
    ports: Option<crate::config::PortRange>,
    app: &AppSettings,
) -> Result<(), RpcError> {
    match (ports, app.port) {
        (Some(range), Some(port)) if !range.contains(port) => Err(rejected(format!(
            "On this VM your ports are {range}; port {port} is outside it"
        ))),
        _ => Ok(()),
    }
}

fn app_view(r: AppRecord) -> AppRunView {
    AppRunView {
        run_id: r.run_id,
        state: r.state.as_str().to_owned(),
        members: r
            .members
            .into_iter()
            .map(|m| AppMemberView {
                repo_id: m.repo_id,
                url: m.url,
                state: m.state.as_str().to_owned(),
            })
            .collect(),
        database_confirmed: r.database_confirmed,
        login_confirmed: r.login_confirmed,
        error: r.error,
    }
}

impl AppDispatch {
    /// Pick from .env: nama kunci dari folder repo asli atau sumber desain.
    pub(super) async fn env_keys(&self, p: EnvKeysParams) -> Result<Value, RpcError> {
        let store = &self.app.store;
        let dir: PathBuf = match env_source(&p)? {
            EnvSource::Repo(id) => store.get_repo(id).await.map_err(store_error)?.path.into(),
            EnvSource::Design(id) => {
                let row = store
                    .design_sources()
                    .await
                    .map_err(store_error)?
                    .into_iter()
                    .find(|r| r.id == id)
                    .ok_or_else(|| rejected("Design source not found"))?;
                super::design::folder_path(&row).into()
            }
        };
        ok(&read_env_keys(&dir).await?)
    }

    pub(super) fn is_app_method(name: &str) -> bool {
        [
            method::APP_STATUS,
            method::APP_CHANGES,
            method::APP_RUN,
            method::APP_STOP,
            method::APP_CONFIRM,
            method::APP_LOGS,
            method::APP_OWNER,
            method::APP_ENV_KEYS,
        ]
        .contains(&name)
    }

    pub(super) async fn app_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        // Membaca nama kunci tidak butuh sesi Lark.
        if name == method::APP_ENV_KEYS {
            return self.env_keys(params(v)?).await;
        }
        let s = self.session().await?;
        let runs = &s.runs;
        match name {
            method::APP_OWNER => {
                let owner = runs.app_owner_status().await.map_err(run_error)?;
                ok(&owner.map(app_view))
            }
            method::APP_RUN => {
                let p: AppRunParams = params(v)?;
                let rec = runs
                    .app_run(&p.run_id, &p.snapshots)
                    .await
                    .map_err(run_error)?;
                ok(&app_view(rec))
            }
            method::APP_CONFIRM => {
                let p: AppConfirm = params(v)?;
                let rec = runs
                    .app_confirm(&p.run_id, p.database, p.login)
                    .await
                    .map_err(run_error)?;
                ok(&app_view(rec))
            }
            _ => {
                let p: RunRef = params(v)?;
                match name {
                    method::APP_CHANGES => {
                        let changes = runs.app_changes(&p.run_id).await.map_err(run_error)?;
                        ok(&changes
                            .into_iter()
                            .map(|c| AppChangesView {
                                repo_id: c.repo_id,
                                snapshot: c.snapshot,
                                files: c.files,
                                flagged: c.flagged,
                            })
                            .collect::<Vec<_>>())
                    }
                    method::APP_STOP => ok(&app_view(
                        runs.app_stop(&p.run_id).await.map_err(run_error)?,
                    )),
                    method::APP_LOGS => ok(&runs
                        .app_logs(&p.run_id)
                        .await
                        .into_iter()
                        .map(|(repo_id, text)| AppLogView { repo_id, text })
                        .collect::<Vec<_>>()),
                    _ => ok(&app_view(
                        runs.app_status(&p.run_id).await.map_err(run_error)?,
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> RepoAppView {
        RepoAppView {
            test_mode: "web".to_owned(),
            setup_command: Some("  ".to_owned()),
            start_command: Some(" bun run dev ".to_owned()),
            port: Some(5173),
            app_url: None,
            env_keys: vec!["API_URL".to_owned()],
            browser_tests: true,
            auto_restart: true,
            unattended_login: true,
        }
    }

    #[test]
    fn port_app_di_vm_harus_di_rentang_developer() {
        let range = crate::config::PortRange::parse("6100-6199").ok();
        let mut a = app_settings_from(&view()).unwrap();
        assert!(check_app_port(range, &a).is_err(), "5173 di luar rentang");
        assert!(check_app_port(None, &a).is_ok(), "laptop tidak dibatasi");
        a.port = Some(6150);
        assert!(check_app_port(range, &a).is_ok());
        a.port = None;
        assert!(check_app_port(range, &a).is_ok());
    }

    #[test]
    fn pengaturan_app_divalidasi_dan_dirapikan() {
        let a = app_settings_from(&view()).unwrap();
        assert_eq!(a.setup_command, None);
        assert_eq!(a.start_command.as_deref(), Some("bun run dev"));
        assert_eq!(repo_app_view(&a).test_mode, "web");
        assert!(a.auto_restart && repo_app_view(&a).auto_restart);
        assert!(a.unattended_login && repo_app_view(&a).unattended_login);
        let mut bad = view();
        bad.env_keys = vec!["API URL".to_owned()];
        assert!(app_settings_from(&bad).is_err());
        let mut bad = view();
        bad.app_url = Some("localhost:5173".to_owned());
        assert!(app_settings_from(&bad).is_err());
        let mut bad = view();
        bad.test_mode = "desktop".to_owned();
        assert!(app_settings_from(&bad).is_err());
    }

    #[tokio::test]
    async fn nama_kunci_env_dibaca_tanpa_nilai() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "SECRET=hunter2\nexport KELOLA_ADDR=:8080\n",
        )
        .unwrap();
        let view = read_env_keys(dir.path()).await.unwrap();
        assert!(view.found);
        assert_eq!(view.keys, ["SECRET", "KELOLA_ADDR"]);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("hunter2") && !json.contains("8080"));
    }

    #[tokio::test]
    async fn folder_tanpa_env_tidak_ditemukan() {
        let dir = tempfile::tempdir().unwrap();
        let view = read_env_keys(dir.path()).await.unwrap();
        assert!(!view.found);
        assert!(view.keys.is_empty());
    }

    #[test]
    fn sumber_env_harus_tepat_satu() {
        let p = |repo: Option<&str>, design: Option<&str>| EnvKeysParams {
            repo_id: repo.map(str::to_owned),
            design_source_id: design.map(str::to_owned),
        };
        assert!(matches!(
            env_source(&p(Some("r"), None)),
            Ok(EnvSource::Repo("r"))
        ));
        assert!(matches!(
            env_source(&p(None, Some("d"))),
            Ok(EnvSource::Design("d"))
        ));
        assert!(env_source(&p(None, None)).is_err());
        assert!(env_source(&p(Some("r"), Some("d"))).is_err());
    }
}
