//! Method `app.*`: run app milik run (KTD-36).

use mandor_domain::app_run::{AppSettings, TestMode};
use mandor_protocol::{
    AppChangesView, AppConfirm, AppLogView, AppMemberView, AppRunParams, AppRunView, RepoAppView,
    RpcError, RunRef, method,
};
use mandor_usecase::run_ports::AppRecord;
use serde_json::Value;

use super::{AppDispatch, ok, params, rejected, run_error};

pub(super) fn repo_app_view(a: &AppSettings) -> RepoAppView {
    RepoAppView {
        test_mode: a.test_mode.as_str().to_owned(),
        setup_command: a.setup_command.clone(),
        start_command: a.start_command.clone(),
        port: a.port,
        app_url: a.app_url.clone(),
        env_keys: a.env_keys.clone(),
        browser_tests: a.browser_tests,
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
    })
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
    pub(super) fn is_app_method(name: &str) -> bool {
        [
            method::APP_STATUS,
            method::APP_CHANGES,
            method::APP_RUN,
            method::APP_STOP,
            method::APP_CONFIRM,
            method::APP_LOGS,
        ]
        .contains(&name)
    }

    pub(super) async fn app_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let runs = &s.runs;
        match name {
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
        }
    }

    #[test]
    fn pengaturan_app_divalidasi_dan_dirapikan() {
        let a = app_settings_from(&view()).unwrap();
        assert_eq!(a.setup_command, None);
        assert_eq!(a.start_command.as_deref(), Some("bun run dev"));
        assert_eq!(repo_app_view(&a).test_mode, "web");
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
}
