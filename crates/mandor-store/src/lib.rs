//! Adapter penyimpanan mandord di atas `SQLite` (KTD-3).
//!
//! Satu-satunya crate yang memakai `sqlx`. Error `sqlx` diterjemahkan ke
//! [`StoreError`] sebelum keluar dari crate.

mod ac_ledger;
mod app_port;
mod codec;
mod design_sources;
mod error;
mod journal;
mod messages;
mod notifications;
mod outbox;
mod overnight;
mod repos;
mod review_port;
mod reviews;
mod run_cards;
mod run_port;
mod runs;
mod settings;
mod tasks;
mod workspaces;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

pub use app_port::AppStateStore;
pub use design_sources::DesignSourceRow;
pub use error::StoreError;
pub use messages::Message;
pub use notifications::{NOTIFICATIONS_KEPT, Notification};
pub use outbox::{OutboxItem, OutboxStatus};
pub use overnight::OvernightsStore;
pub use repos::{NewRepo, Repo};
pub use review_port::{CHROME_KEY, ReviewsStore};
pub use reviews::{StoredReview, StoredVerdict};
pub use run_port::{ReposStore, RunsStore, app_settings_json, app_settings_of};
pub use tasks::TaskStore;
pub use workspaces::{NewWorkspace, Workspace};

/// Batas waktu menunggu koneksi dari pool.
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
/// Batas waktu `SQLite` menunggu lock tulis.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
/// Daemon adalah satu-satunya penulis; pool kecil cukup.
const MAX_CONNECTIONS: u32 = 4;

/// Penyimpanan mandord.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Membuka (dan membuat bila belum ada) database lalu menjalankan migrasi.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila database tidak bisa dibuka atau dimigrasi.
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        let url = format!("sqlite://{}", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .map_err(StoreError::internal)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(BUSY_TIMEOUT)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect_with(options)
            .await
            .map_err(StoreError::internal)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(StoreError::internal)?;
        Ok(Self { pool })
    }
}

fn to_json(value: &serde_json::Value) -> String {
    value.to_string()
}

fn from_json(raw: &str) -> Result<serde_json::Value, StoreError> {
    serde_json::from_str(raw).map_err(StoreError::internal)
}
