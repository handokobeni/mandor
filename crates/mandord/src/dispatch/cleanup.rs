//! Method penutupan run kartu selesai dan daftar storage run (plan
//! `2026-11-03-001`, U3).

use mandor_protocol::{
    CleanupSettingsView, CloseFinishedParams, CloseFinishedStarted, FinishedRunView,
    FinishedRunsView, RpcError, RunStorageView, SkippedRunView, method,
};
use mandor_usecase::run::{CleanupSettings, FinishedRun, SkippedRun};
use serde_json::{Value, json};

use super::{AppDispatch, ok, params, run_error};

impl AppDispatch {
    pub(super) fn is_cleanup_method(name: &str) -> bool {
        [
            method::RUNS_FINISHED,
            method::RUNS_CLOSE_FINISHED,
            method::RUNS_STORAGE,
            method::SETTINGS_CLEANUP_GET,
            method::SETTINGS_CLEANUP_SET,
        ]
        .contains(&name)
    }

    pub(super) async fn cleanup_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let runs = &s.runs;
        match name {
            method::RUNS_FINISHED => {
                let found = runs.finished_runs().await.map_err(run_error)?;
                ok(&FinishedRunsView {
                    closable: found.closable.iter().map(finished_view).collect(),
                    skipped: found.skipped.iter().map(skipped_view).collect(),
                })
            }
            method::RUNS_CLOSE_FINISHED => {
                let p: CloseFinishedParams = params(v)?;
                let count = runs.start_close_finished(p.run_ids).map_err(run_error)?;
                ok(&CloseFinishedStarted {
                    count: u32::try_from(count).unwrap_or(u32::MAX),
                })
            }
            method::RUNS_STORAGE => {
                let rows = runs.run_storage().await.map_err(run_error)?;
                let views: Vec<RunStorageView> = rows
                    .into_iter()
                    .map(|r| RunStorageView {
                        run_id: r.run_id,
                        keys: r.keys,
                        title: r.title,
                        run_state: r.state.as_str().to_owned(),
                        card_status: r
                            .card_status
                            .iter()
                            .map(|c| c.as_str().to_owned())
                            .collect(),
                        bytes: r.bytes,
                        updated_at_ms: r.updated_at_ms,
                    })
                    .collect();
                ok(&views)
            }
            method::SETTINGS_CLEANUP_GET => ok(&CleanupSettingsView {
                auto_close: runs.cleanup_settings().await.auto_close,
            }),
            method::SETTINGS_CLEANUP_SET => {
                let p: CleanupSettingsView = params(v)?;
                runs.set_cleanup_settings(&CleanupSettings {
                    auto_close: p.auto_close,
                })
                .await
                .map_err(run_error)?;
                ok(&json!({}))
            }
            other => Err(RpcError::new(
                mandor_protocol::ErrorCode::MethodNotFound,
                format!("unknown method: {other}"),
            )),
        }
    }
}

fn finished_view(r: &FinishedRun) -> FinishedRunView {
    FinishedRunView {
        run_id: r.run_id.clone(),
        keys: r.keys.clone(),
        title: r.title.clone(),
        bytes: r.bytes,
    }
}

fn skipped_view(r: &SkippedRun) -> SkippedRunView {
    SkippedRunView {
        run: finished_view(&r.run),
        reason: r.reason.as_str().to_owned(),
    }
}
