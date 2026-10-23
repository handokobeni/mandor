//! Run app (KTD-36, R24–R27): menyalakan app web dari worktree run, satu run
//! pemilik dalam satu waktu, dengan konfirmasi perubahan sebelum dijalankan.

use std::collections::BTreeMap;
use std::sync::Arc;

use mandor_domain::app_run::{AppRunState, is_flagged_change};

use crate::ports::{Deps, PortError};
use crate::run::{RunError, RunService};
use crate::run_ports::{
    AppMember, AppMemberRecord, AppProcesses, AppRecord, AppRecords, GitPort, Notifier, Repos,
    RunStore, Spawner,
};

/// Berkas yang berubah di satu repo sejak app terakhir dijalankan (atau sejak
/// base branch untuk app pertama run), untuk konfirmasi dev (R25a).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppChanges {
    pub repo_id: String,
    /// Snapshot saat ini; dikirim balik oleh client saat `app_run`.
    pub snapshot: String,
    pub files: Vec<String>,
    /// Manifest paket, lockfile, skrip, dan berkas perintah start.
    pub flagged: Vec<String>,
}

fn rejected(message: &str) -> RunError {
    RunError::Port(PortError::Rejected(message.to_owned()))
}

impl<D: Deps> RunService<D> {
    /// Anggota run yang punya perintah start dan port.
    async fn app_members(&self, run_id: &str) -> Result<Vec<AppMember>, RunError> {
        let run = self
            .runs
            .get(run_id)
            .await?
            .ok_or(RunError::NotFound("run"))?;
        Ok(self
            .members(&run)
            .await?
            .into_iter()
            .filter(|m| m.repo.app.runnable())
            .map(|m| AppMember {
                repo_id: m.repo.id.clone(),
                worktree: m.worktree,
                repo_path: m.repo.path.clone(),
                settings: m.repo.app,
            })
            .collect())
    }

    /// State app run itu; `Off` bila belum pernah dijalankan.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn app_status(&self, run_id: &str) -> Result<AppRecord, RunError> {
        Ok(self
            .apps
            .app_record(run_id)
            .await?
            .unwrap_or_else(|| AppRecord {
                run_id: run_id.to_owned(),
                ..AppRecord::default()
            }))
    }

    /// State app milik run pemilik saat ini, bila ada.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn app_owner_status(&self) -> Result<Option<AppRecord>, RunError> {
        let Some(owner) = self.apps.app_owner().await? else {
            return Ok(None);
        };
        Ok(self.apps.app_record(&owner).await?)
    }

    /// Perubahan per repo untuk konfirmasi sebelum app dijalankan.
    ///
    /// # Errors
    /// Run tidak ada, tidak ada repo dengan perintah start, atau git gagal.
    pub async fn app_changes(&self, run_id: &str) -> Result<Vec<AppChanges>, RunError> {
        let members = self.app_members(run_id).await?;
        if members.is_empty() {
            return Err(rejected(
                "No repo in this run has a start command and port; set them in Settings",
            ));
        }
        let last = self.app_status(run_id).await?.snapshot;
        let mut out = Vec::with_capacity(members.len());
        for m in members {
            let snapshot = self.git.snapshot(&m.worktree).await?;
            let repo = self
                .repos
                .repo(&m.repo_id)
                .await?
                .ok_or(RunError::NotFound("repo"))?;
            let base = last
                .get(&m.repo_id)
                .cloned()
                .unwrap_or_else(|| format!("origin/{}", repo.default_branch));
            let files = self
                .git
                .changed_between(&m.worktree, &base, &snapshot)
                .await?;
            let flagged = files
                .iter()
                .filter(|f| is_flagged_change(f, &m.settings))
                .cloned()
                .collect();
            out.push(AppChanges {
                repo_id: m.repo_id,
                snapshot,
                files,
                flagged,
            });
        }
        Ok(out)
    }

    /// Menyalakan app run: `confirmed` adalah snapshot yang ditampilkan ke dev
    /// lewat [`Self::app_changes`]. App run lain dihentikan lebih dulu. Proses
    /// berjalan di latar; progres lewat state app.
    ///
    /// # Errors
    /// Run tidak ada, tidak ada app, kode berubah sejak dikonfirmasi, atau app
    /// run ini sedang dinyalakan.
    pub async fn app_run(
        self: &Arc<Self>,
        run_id: &str,
        confirmed: &BTreeMap<String, String>,
    ) -> Result<AppRecord, RunError> {
        let run = self
            .runs
            .get(run_id)
            .await?
            .ok_or(RunError::NotFound("run"))?;
        // Kode tidak boleh berubah antara konfirmasi dan start.
        if run.state.is_busy() {
            return Err(RunError::InvalidState(
                "the agent is working; run the app when it finishes",
            ));
        }
        let members = self.app_members(run_id).await?;
        if members.is_empty() {
            return Err(rejected(
                "No repo in this run has a start command and port; set them in Settings",
            ));
        }
        let mut snapshot = BTreeMap::new();
        for m in &members {
            let current = self.git.snapshot(&m.worktree).await?;
            if confirmed.get(&m.repo_id) != Some(&current) {
                return Err(rejected(
                    "The code changed since you checked it; review the changes again",
                ));
            }
            snapshot.insert(m.repo_id.clone(), current);
        }
        let prev = self.app_status(run_id).await?;
        if matches!(
            prev.state,
            AppRunState::SettingUp | AppRunState::Starting | AppRunState::WaitingForPort
        ) {
            return Err(RunError::InvalidState("the app is starting"));
        }
        self.stop_other_app(run_id).await?;
        self.apps.stop(run_id).await;
        self.apps.set_app_owner(Some(run_id)).await?;
        let rec = AppRecord {
            run_id: run_id.to_owned(),
            state: AppRunState::Starting,
            members: members
                .iter()
                .map(|m| AppMemberRecord {
                    repo_id: m.repo_id.clone(),
                    url: m.settings.canonical_url(),
                    pgid: None,
                    state: AppRunState::Off,
                })
                .collect(),
            snapshot,
            setup_hashes: prev.setup_hashes,
            database_confirmed: false,
            login_confirmed: false,
            error: None,
        };
        self.apps.save_app_record(&rec).await?;
        let svc = Arc::clone(self);
        let started = rec.clone();
        self.spawner.spawn(async move {
            svc.start_members(started, members).await;
        });
        Ok(rec)
    }

    /// App run lain yang menyala dihentikan dan ditandai `StoppedByOtherRun`.
    async fn stop_other_app(&self, run_id: &str) -> Result<(), RunError> {
        let Some(other) = self.apps.app_owner().await?.filter(|o| o != run_id) else {
            return Ok(());
        };
        self.apps.stop(&other).await;
        if let Some(mut rec) = self.apps.app_record(&other).await? {
            if rec.state.is_active() {
                rec.state = AppRunState::StoppedByOtherRun;
                for m in &mut rec.members {
                    m.state = AppRunState::StoppedByOtherRun;
                    m.pgid = None;
                }
                rec.database_confirmed = false;
                rec.login_confirmed = false;
                self.apps.save_app_record(&rec).await?;
            }
        }
        Ok(())
    }

    /// Menyalakan anggota berurutan; anggota yang gagal menghentikan semuanya
    /// dan anggota berikutnya tidak dinyalakan (R27).
    async fn start_members(&self, mut rec: AppRecord, members: Vec<AppMember>) {
        for (i, m) in members.iter().enumerate() {
            if let Err(e) = self.start_member(&mut rec, i, m).await {
                self.apps.stop(&rec.run_id).await;
                rec.state = AppRunState::Failed;
                rec.members[i].state = AppRunState::Failed;
                for later in &mut rec.members {
                    later.pgid = None;
                }
                rec.error = Some(format!("{}: {e}", m.repo_id));
                self.save_quietly(&rec).await;
                if let Ok(run) = self.load(&rec.run_id).await {
                    let body = format!("The app failed to start ({})", m.repo_id);
                    self.notifier
                        .notify(
                            crate::notify::notice(
                                self,
                                crate::run_ports::NoticeKind::AppFailed,
                                &run,
                                &body,
                            )
                            .await,
                        )
                        .await;
                }
                return;
            }
        }
        rec.state = AppRunState::Running;
        self.save_quietly(&rec).await;
    }

    async fn start_member(
        &self,
        rec: &mut AppRecord,
        i: usize,
        m: &AppMember,
    ) -> Result<(), PortError> {
        let port = m
            .settings
            .port
            .ok_or_else(|| PortError::Rejected("no port set".to_owned()))?;
        self.ensure_unchanged(rec, m).await?;
        let hash = self.apps.lockfile_hash(&m.worktree).await;
        let setup = m
            .settings
            .setup_command
            .as_deref()
            .is_some_and(|c| !c.trim().is_empty());
        if setup && (hash.is_none() || rec.setup_hashes.get(&m.repo_id) != hash.as_ref()) {
            self.set_member_state(rec, i, AppRunState::SettingUp).await;
            self.apps.setup(&rec.run_id, m).await?;
            if let Some(h) = hash {
                rec.setup_hashes.insert(m.repo_id.clone(), h);
            }
        }
        if self.apps.port_open(port).await {
            return Err(PortError::Rejected(format!(
                "port {port} is already in use by another program; stop it and try again"
            )));
        }
        self.set_member_state(rec, i, AppRunState::Starting).await;
        self.ensure_unchanged(rec, m).await?;
        let pgid = self.apps.spawn(&rec.run_id, m).await?;
        rec.members[i].pgid = Some(pgid);
        self.set_member_state(rec, i, AppRunState::WaitingForPort)
            .await;
        self.apps.wait_ready(&rec.run_id, &m.repo_id, port).await?;
        self.set_member_state(rec, i, AppRunState::Running).await;
        Ok(())
    }

    /// Kode repo masih sama dengan snapshot yang dikonfirmasi dev.
    async fn ensure_unchanged(&self, rec: &AppRecord, m: &AppMember) -> Result<(), PortError> {
        let current = self.git.snapshot(&m.worktree).await?;
        if rec.snapshot.get(&m.repo_id) == Some(&current) {
            Ok(())
        } else {
            Err(PortError::Rejected(
                "the code changed after you confirmed it; review the changes again".to_owned(),
            ))
        }
    }

    async fn set_member_state(&self, rec: &mut AppRecord, i: usize, state: AppRunState) {
        rec.members[i].state = state;
        if state != AppRunState::Running {
            rec.state = state;
        }
        self.save_quietly(rec).await;
    }

    async fn save_quietly(&self, rec: &AppRecord) {
        if let Err(e) = self.apps.save_app_record(rec).await {
            tracing::warn!(run_id = %rec.run_id, error = %e, "state app tidak tersimpan");
        }
    }

    /// Menghentikan app run; konfirmasi database dan login direset.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn app_stop(&self, run_id: &str) -> Result<AppRecord, RunError> {
        self.apps.stop(run_id).await;
        let mut rec = self.app_status(run_id).await?;
        rec.state = AppRunState::Off;
        for m in &mut rec.members {
            m.state = AppRunState::Off;
            m.pgid = None;
        }
        rec.database_confirmed = false;
        rec.login_confirmed = false;
        rec.error = None;
        self.apps.save_app_record(&rec).await?;
        if self.apps.app_owner().await?.as_deref() == Some(run_id) {
            self.apps.set_app_owner(None).await?;
        }
        Ok(rec)
    }

    /// Konfirmasi dev bahwa database menyala dan ia sudah login (KTD-36).
    ///
    /// # Errors
    /// App tidak menyala, atau penyimpanan gagal.
    pub async fn app_confirm(
        &self,
        run_id: &str,
        database: Option<bool>,
        login: Option<bool>,
    ) -> Result<AppRecord, RunError> {
        let mut rec = self.app_status(run_id).await?;
        if rec.state != AppRunState::Running {
            return Err(RunError::InvalidState("the app is not running"));
        }
        if let Some(d) = database {
            rec.database_confirmed = d;
        }
        if let Some(l) = login {
            rec.login_confirmed = l;
        }
        self.apps.save_app_record(&rec).await?;
        Ok(rec)
    }

    /// Log app per repo.
    pub async fn app_logs(&self, run_id: &str) -> Vec<(String, String)> {
        self.apps.logs(run_id).await
    }

    /// Saat daemon start: group app yatim dibunuh dan state kembali Off; app
    /// tidak dinyalakan ulang otomatis.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn reap_apps(&self) -> Result<(), RunError> {
        let Some(owner) = self.apps.app_owner().await? else {
            return Ok(());
        };
        if let Some(rec) = self.apps.app_record(&owner).await? {
            for pgid in rec.members.iter().filter_map(|m| m.pgid) {
                self.apps.reap(&owner, pgid).await;
            }
        }
        self.app_stop(&owner).await?;
        Ok(())
    }
}
