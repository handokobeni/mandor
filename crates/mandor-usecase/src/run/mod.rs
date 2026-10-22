//! Orkestrasi task run (U10): Start, sesi agent, publish, rekonsiliasi.

mod members;
mod ownership;
mod publish;
mod reconcile;
mod session;
mod start;
pub use start::{Planning, StartStep, StartTarget};

pub use session::SendOutcome;
pub(crate) use session::TurnEnd;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use mandor_domain::{GateRejection, Task};

use crate::ports::{Deps, PortError};
use crate::run_ports::{AgentPort, AgentRunHandle, RepoInfo, Repos, RunRecord, RunStore};
use crate::sync::{SyncError, SyncService};

/// Kegagalan orkestrasi run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Sync(#[from] SyncError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("no changes to push")]
    NoChanges,
    #[error("action not available ({0})")]
    InvalidState(&'static str),
}

impl From<GateRejection> for RunError {
    fn from(e: GateRejection) -> Self {
        Self::Sync(SyncError::Gate(e))
    }
}

type StopperOf<D> = <<<D as Deps>::Agent as AgentPort>::Run as AgentRunHandle>::Stopper;

/// Layanan run untuk pengguna yang login.
pub struct RunService<D: Deps> {
    pub(crate) sync: Arc<SyncService<D>>,
    pub(crate) git: D::Git,
    pub(crate) agent: D::Agent,
    pub(crate) runs: D::Runs,
    pub(crate) notifier: D::Notifier,
    pub(crate) spawner: D::Spawner,
    pub(crate) repos: D::Repos,
    pub(crate) prompts: D::Prompts,
    pub(crate) reviews: D::Reviews,
    pub(crate) apps: D::Apps,
    /// Waktu sinkron AC terakhir yang berhasil per run (dibangun ulang saat
    /// refresh berikutnya setelah restart).
    pub(crate) ledger_synced: Mutex<HashMap<String, i64>>,
    /// Token kapabilitas tool ledger yang berlaku (KTD-26).
    pub(crate) capabilities: Arc<Mutex<crate::review::capability::Issued>>,
    /// Balasan komentar terakhir yang diambil (dibangun ulang tiap poll).
    pub(crate) replies: Mutex<Vec<mandor_domain::Reply>>,
    /// Kursor yang terakhir diantrekan per record, agar tidak diantrekan berulang.
    pub(crate) cursor_sent: Mutex<HashMap<String, String>>,
    /// Penghenti run aktif per run ID beserta generasinya.
    stoppers: Mutex<HashMap<String, (i64, StopperOf<D>)>>,
}

impl<D: Deps> std::fmt::Debug for RunService<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunService").finish_non_exhaustive()
    }
}

/// Port tambahan untuk [`RunService`].
pub struct RunPorts<D: Deps> {
    pub git: D::Git,
    pub agent: D::Agent,
    pub runs: D::Runs,
    pub notifier: D::Notifier,
    pub spawner: D::Spawner,
    pub repos: D::Repos,
    pub prompts: D::Prompts,
    pub reviews: D::Reviews,
    pub apps: D::Apps,
}

impl<D: Deps> std::fmt::Debug for RunPorts<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunPorts").finish_non_exhaustive()
    }
}

impl<D: Deps> RunService<D> {
    #[must_use]
    pub fn new(sync: Arc<SyncService<D>>, ports: RunPorts<D>) -> Self {
        Self {
            sync,
            git: ports.git,
            agent: ports.agent,
            runs: ports.runs,
            notifier: ports.notifier,
            spawner: ports.spawner,
            repos: ports.repos,
            prompts: ports.prompts,
            reviews: ports.reviews,
            apps: ports.apps,
            ledger_synced: Mutex::new(HashMap::new()),
            capabilities: crate::review::capability::empty(),
            replies: Mutex::new(Vec::new()),
            cursor_sent: Mutex::new(HashMap::new()),
            stoppers: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn sync(&self) -> &Arc<SyncService<D>> {
        &self.sync
    }

    pub(crate) fn now(&self) -> i64 {
        crate::ports::Clock::now_ms(&self.sync.clock)
    }

    pub(crate) fn set_stopper(&self, run_id: &str, generation: i64, stopper: StopperOf<D>) {
        self.stoppers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run_id.to_owned(), (generation, stopper));
    }

    /// Mengambil penghenti run bila generasinya masih yang terdaftar.
    pub(crate) fn take_stopper(&self, run_id: &str) -> Option<StopperOf<D>> {
        self.stoppers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(run_id)
            .map(|(_, s)| s)
    }

    pub(crate) fn clear_stopper(&self, run_id: &str, generation: i64) {
        let mut map = self.stoppers.lock().unwrap_or_else(PoisonError::into_inner);
        if map.get(run_id).is_some_and(|(g, _)| *g == generation) {
            map.remove(run_id);
        }
    }

    pub(crate) async fn load(&self, run_id: &str) -> Result<RunRecord, RunError> {
        self.runs
            .get(run_id)
            .await?
            .ok_or(RunError::NotFound("run"))
    }

    pub(crate) async fn repo_of(&self, run: &RunRecord) -> Result<RepoInfo, RunError> {
        self.repos
            .repo(&run.repo_id)
            .await?
            .ok_or(RunError::NotFound("repo"))
    }

    pub(crate) async fn task_of(&self, record_id: &str) -> Result<Task, RunError> {
        Ok(self
            .sync
            .fetch_one(record_id)
            .await?
            .ok_or(SyncError::NotFound)?)
    }

    /// Menghentikan semua proses agent (daemon dimatikan). State run tidak
    /// diubah; rekonsiliasi saat start menandainya gagal dan bisa dilanjutkan.
    pub async fn shutdown_agents(&self) {
        let all: Vec<_> = self
            .stoppers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .map(|(_, (_, s))| s)
            .collect();
        for s in all {
            crate::run_ports::Stopper::stop(&s).await;
        }
    }

    /// Record yang state alur kerjanya dalam loop perbaikan (untuk poll komentar).
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn fix_loop_records(&self) -> Result<Vec<String>, RunError> {
        Ok(self
            .runs
            .list_active()
            .await?
            .into_iter()
            .filter(|r| r.state.in_fix_loop())
            .map(|r| r.record_id)
            .collect())
    }

    /// Run aktif untuk tampilan "agent aktif" (R1).
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn active_runs(&self) -> Result<Vec<RunRecord>, RunError> {
        Ok(self.runs.list_active().await?)
    }
}
