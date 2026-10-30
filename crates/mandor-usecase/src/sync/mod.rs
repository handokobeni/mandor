//! Mesin sinkron Lark: poller, outbox, dan klaim (KTD-6, KTD-7, KTD-8).

mod claim;
mod outbox;
mod poller;

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use mandor_domain::{ClaimIdentity, DaemonRole};
use mandor_domain::{ClaimRefused, GateRejection, Task, WriteRejected};

use crate::ports::{AcItem, Clock, Deps, LarkTasks, PortError, SprintItem};

pub use claim::Heartbeat;
pub use outbox::DrainReport;
pub use poller::PollReport;

/// Jeda sebelum mencoba lagi Contact API atau tabel sprint setelah gagal.
const NAMES_RETRY_MS: i64 = 5 * 60 * 1000;

/// Isi tab Task: record dari cache dan Acceptance Criteria tertautnya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDetails {
    pub task: Task,
    /// Urut sesuai tautan di record; yang di-drop tetap ada (ditampilkan dicoret).
    pub acs: Vec<AcItem>,
}

/// Kegagalan use case sinkron.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Rejected(#[from] WriteRejected),
    #[error(transparent)]
    Gate(#[from] GateRejection),
    #[error(transparent)]
    Claim(#[from] ClaimRefused),
    #[error("lost the claim: another developer started this task at the same time")]
    ClaimLost,
    #[error("record not found in Lark")]
    NotFound,
    #[error(transparent)]
    Port(#[from] PortError),
}

/// Layanan sinkron untuk satu pengguna (`open_id` `me`).
pub struct SyncService<D: Deps> {
    pub(crate) lark: D::Lark,
    pub(crate) cache: D::Cache,
    pub(crate) outbox: D::Outbox,
    pub(crate) clock: D::Clock,
    pub(crate) ids: D::Ids,
    pub(crate) me: String,
    /// Identitas daemon untuk klaim (KTD-55).
    pub(crate) identity: ClaimIdentity,
    /// Peran daemon (KTD-54).
    pub(crate) role: DaemonRole,
    /// Nama orang per `open_id`; hanya di memori, terisi lagi setelah restart.
    names: Mutex<HashMap<String, String>>,
    /// Setelah Contact API gagal, jangan coba lagi sebelum waktu ini (ms epoch).
    names_retry_at: Mutex<i64>,
    /// Sprint per record ID; hanya di memori, terisi lagi setelah restart.
    sprints: Mutex<HashMap<String, SprintItem>>,
    sprints_retry_at: Mutex<i64>,
    /// Kartu tim (papan Team), hanya di memori; diambil ulang tiap putaran.
    team: Mutex<Vec<mandor_domain::Task>>,
}

/// Kartu tim yang sudah Done tetap tampil selama 14 hari sejak perubahan terakhir.
pub const TEAM_DONE_WINDOW_MS: i64 = 14 * 24 * 60 * 60 * 1000;

impl<D: Deps> std::fmt::Debug for SyncService<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncService")
            .field("me", &self.me)
            .finish_non_exhaustive()
    }
}

impl<D: Deps> SyncService<D> {
    #[must_use]
    pub fn new(
        lark: D::Lark,
        cache: D::Cache,
        outbox: D::Outbox,
        clock: D::Clock,
        ids: D::Ids,
        me: String,
    ) -> Self {
        Self {
            lark,
            cache,
            outbox,
            clock,
            ids,
            identity: ClaimIdentity::new(&me, "local", true),
            role: DaemonRole::Laptop,
            me,
            names: Mutex::new(HashMap::new()),
            names_retry_at: Mutex::new(0),
            sprints: Mutex::new(HashMap::new()),
            sprints_retry_at: Mutex::new(0),
            team: Mutex::new(Vec::new()),
        }
    }

    /// Sprint untuk `ids` (tanpa duplikat, urut kemunculan). Yang belum dikenal
    /// diambil dari Lark; bila gagal, hanya sprint dari cache yang dikembalikan.
    pub async fn sprints(&self, ids: &[String]) -> Vec<SprintItem> {
        let mut unique: Vec<String> = Vec::new();
        for id in ids {
            if !unique.contains(id) {
                unique.push(id.clone());
            }
        }
        let missing: Vec<String> = {
            let known = self.sprints.lock().unwrap_or_else(PoisonError::into_inner);
            unique
                .iter()
                .filter(|id| !known.contains_key(*id))
                .cloned()
                .collect()
        };
        let now = self.clock.now_ms();
        let backing_off = *self
            .sprints_retry_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            > now;
        if !missing.is_empty() && !backing_off {
            match self.lark.sprints(&missing).await {
                Ok(found) => self
                    .sprints
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend(found.into_iter().map(|s| (s.record_id.clone(), s))),
                Err(e) => {
                    *self
                        .sprints_retry_at
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = now + NAMES_RETRY_MS;
                    tracing::warn!(error = %e, "data sprint dari Lark tidak tersedia");
                }
            }
        }
        let known = self.sprints.lock().unwrap_or_else(PoisonError::into_inner);
        unique
            .iter()
            .filter_map(|id| known.get(id).cloned())
            .collect()
    }

    /// Nama tampilan untuk `open_ids`. Yang belum dikenal diambil dari Lark;
    /// bila Lark gagal (mis. scope Contact belum disetujui), hanya nama dari
    /// cache yang dikembalikan dan UI memakai `open_id`.
    pub async fn user_names(&self, open_ids: &[String]) -> HashMap<String, String> {
        let missing: Vec<String> = {
            let names = self.names.lock().unwrap_or_else(PoisonError::into_inner);
            let mut m: Vec<String> = open_ids
                .iter()
                .filter(|id| !names.contains_key(*id))
                .cloned()
                .collect();
            m.sort();
            m.dedup();
            m
        };
        let now = self.clock.now_ms();
        let backing_off = *self
            .names_retry_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            > now;
        if !missing.is_empty() && !backing_off {
            match self.lark.user_names(&missing).await {
                Ok(found) => self
                    .names
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend(found),
                Err(e) => {
                    // Kegagalan seperti scope yang belum disetujui tidak pulih sendiri
                    // dalam hitungan detik; jangan membanjiri Lark.
                    *self
                        .names_retry_at
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = now + NAMES_RETRY_MS;
                    tracing::warn!(error = %e, "nama pengguna dari Lark tidak tersedia");
                }
            }
        }
        let names = self.names.lock().unwrap_or_else(PoisonError::into_inner);
        open_ids
            .iter()
            .filter_map(|id| names.get(id).map(|n| (id.clone(), n.clone())))
            .collect()
    }

    /// Orang yang muncul di kartu sendiri dan papan Team (assignee, reporter,
    /// pengubah terakhir) beserta namanya, urut nama. Orang tanpa nama dari
    /// Contact API tidak ikut. Dipakai pemilih reviewer di Settings.
    ///
    /// # Errors
    /// Error penyimpanan cache kartu.
    pub async fn known_people(&self) -> Result<Vec<(String, String)>, SyncError> {
        let mut tasks: Vec<mandor_domain::Task> =
            self.board().await?.into_iter().map(|c| c.task).collect();
        tasks.extend(
            self.team
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .cloned(),
        );
        let mut ids: Vec<String> = tasks
            .iter()
            .flat_map(|t| {
                t.assignees
                    .iter()
                    .chain(t.reporter.as_ref())
                    .chain(t.last_modified_by.as_ref())
                    .cloned()
            })
            .collect();
        ids.sort();
        ids.dedup();
        let mut people: Vec<(String, String)> = self.user_names(&ids).await.into_iter().collect();
        people.sort_by(|a, b| {
            a.1.to_lowercase()
                .cmp(&b.1.to_lowercase())
                .then(a.0.cmp(&b.0))
        });
        Ok(people)
    }

    /// `open_id` pengguna.
    #[must_use]
    pub fn me(&self) -> &str {
        &self.me
    }

    /// Menetapkan id dan peran daemon ini (KTD-54). Daemon laptop menganggap
    /// klaim lama tanpa id daemon miliknya; daemon VM tidak.
    #[must_use]
    pub fn with_daemon(mut self, daemon_id: &str, role: DaemonRole) -> Self {
        self.identity = ClaimIdentity::new(&self.me, daemon_id, role == DaemonRole::Laptop);
        self.role = role;
        self
    }

    /// Identitas klaim daemon ini.
    #[must_use]
    pub fn identity(&self) -> &ClaimIdentity {
        &self.identity
    }

    /// Peran daemon ini.
    #[must_use]
    pub fn role(&self) -> DaemonRole {
        self.role
    }

    /// Semua kartu di cache beserta penandanya.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn board(&self) -> Result<Vec<crate::ports::CachedTask>, SyncError> {
        Ok(crate::ports::TaskCache::list(&self.cache).await?)
    }

    /// Mengambil ulang kartu tim: semua yang belum Done dan yang Done dalam
    /// [`TEAM_DONE_WINDOW_MS`] terakhir. Cache kartu milik pengguna tidak disentuh.
    ///
    /// Mengembalikan `true` bila isi papan Team berubah.
    ///
    /// # Errors
    /// [`SyncError::Port`] bila Lark gagal; papan Team terakhir tetap dipakai.
    pub async fn poll_team(&self) -> Result<bool, SyncError> {
        let since = self.clock.now_ms() - TEAM_DONE_WINDOW_MS;
        let mut tasks = self.lark.search_team(since).await?;
        tasks.sort_by(|a, b| a.record_id.cmp(&b.record_id));
        let key = |ts: &[mandor_domain::Task]| -> Vec<(String, i64)> {
            ts.iter()
                .map(|t| (t.record_id.clone(), t.last_modified_ms))
                .collect()
        };
        let mut team = self.team.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = key(&team) != key(&tasks);
        *team = tasks;
        Ok(changed)
    }

    /// Kartu tim yang tidak di-assign ke pengguna (kartu pengguna ada di cache).
    #[must_use]
    pub fn team_board(&self) -> Vec<mandor_domain::Task> {
        self.team
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|t| !t.assigned_to(&self.me))
            .cloned()
            .collect()
    }

    /// Satu kartu dari cache.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn cached(
        &self,
        record_id: &str,
    ) -> Result<Option<crate::ports::CachedTask>, SyncError> {
        Ok(crate::ports::TaskCache::get(&self.cache, record_id).await?)
    }

    /// Detail task untuk tab Task. Hanya record yang ada di cache (milik
    /// pengguna); AC dibaca dari tabel tertaut.
    ///
    /// # Errors
    /// [`SyncError::NotFound`] bila record tidak ada di cache, atau kegagalan Lark.
    pub async fn task_details(&self, record_id: &str) -> Result<TaskDetails, SyncError> {
        let task = self
            .cached(record_id)
            .await?
            .ok_or(SyncError::NotFound)?
            .task;
        let acs = self.lark.acceptance_criteria(&task.ac_ids).await?;
        Ok(TaskDetails { task, acs })
    }

    pub(crate) async fn fetch_one(
        &self,
        record_id: &str,
    ) -> Result<Option<mandor_domain::Task>, PortError> {
        let mut found = self.lark.fetch(&[record_id.to_owned()]).await?;
        Ok(found.pop().filter(|t| t.record_id == record_id))
    }
}
