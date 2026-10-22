//! State run app (KTD-36) sebagai port usecase: satu kunci `app_run:<run>` per
//! run dan kunci `app_owner` untuk run pemilik, di tabel `settings`.

use std::collections::BTreeMap;

use mandor_domain::app_run::AppRunState;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{AppMemberRecord, AppRecord, AppRecords};
use serde::{Deserialize, Serialize};

use crate::Store;

const OWNER_KEY: &str = "app_owner";

fn record_key(run_id: &str) -> String {
    format!("app_run:{run_id}")
}

#[derive(Serialize, Deserialize)]
struct StoredMember {
    repo_id: String,
    url: Option<String>,
    pgid: Option<u32>,
    state: String,
}

#[derive(Serialize, Deserialize)]
struct StoredRecord {
    state: String,
    #[serde(default)]
    members: Vec<StoredMember>,
    #[serde(default)]
    snapshot: BTreeMap<String, String>,
    #[serde(default)]
    setup_hashes: BTreeMap<String, String>,
    #[serde(default)]
    database_confirmed: bool,
    #[serde(default)]
    login_confirmed: bool,
    #[serde(default)]
    error: Option<String>,
}

fn state_of(s: &str) -> AppRunState {
    AppRunState::parse(s).unwrap_or_default()
}

/// State app per run di tabel `settings`.
#[derive(Debug, Clone)]
pub struct AppStateStore {
    store: Store,
}

impl AppStateStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl AppRecords for AppStateStore {
    async fn app_record(&self, run_id: &str) -> Result<Option<AppRecord>, PortError> {
        let Some(raw) = self.store.setting(&record_key(run_id)).await? else {
            return Ok(None);
        };
        let s: StoredRecord = serde_json::from_str(&raw)
            .map_err(|e| PortError::Storage(format!("app state: {e}")))?;
        Ok(Some(AppRecord {
            run_id: run_id.to_owned(),
            state: state_of(&s.state),
            members: s
                .members
                .into_iter()
                .map(|m| AppMemberRecord {
                    repo_id: m.repo_id,
                    url: m.url,
                    pgid: m.pgid,
                    state: state_of(&m.state),
                })
                .collect(),
            snapshot: s.snapshot,
            setup_hashes: s.setup_hashes,
            database_confirmed: s.database_confirmed,
            login_confirmed: s.login_confirmed,
            error: s.error,
        }))
    }

    async fn save_app_record(&self, rec: &AppRecord) -> Result<(), PortError> {
        let stored = StoredRecord {
            state: rec.state.as_str().to_owned(),
            members: rec
                .members
                .iter()
                .map(|m| StoredMember {
                    repo_id: m.repo_id.clone(),
                    url: m.url.clone(),
                    pgid: m.pgid,
                    state: m.state.as_str().to_owned(),
                })
                .collect(),
            snapshot: rec.snapshot.clone(),
            setup_hashes: rec.setup_hashes.clone(),
            database_confirmed: rec.database_confirmed,
            login_confirmed: rec.login_confirmed,
            error: rec.error.clone(),
        };
        let raw = serde_json::to_string(&stored)
            .map_err(|e| PortError::Storage(format!("app state: {e}")))?;
        Ok(self
            .store
            .set_setting(&record_key(&rec.run_id), &raw)
            .await?)
    }

    async fn app_owner(&self) -> Result<Option<String>, PortError> {
        Ok(self
            .store
            .setting(OWNER_KEY)
            .await?
            .filter(|o| !o.is_empty()))
    }

    async fn set_app_owner(&self, run_id: Option<&str>) -> Result<(), PortError> {
        Ok(self
            .store
            .set_setting(OWNER_KEY, run_id.unwrap_or_default())
            .await?)
    }
}
