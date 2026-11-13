//! Lab data (plan 2026-11-10-001, U14, KTD-L15–KTD-L23): sebelum kartu
//! pertama dengan calon `data`, penyimpanan lokal dikunci dan di-snapshot
//! dengan app berhenti; reviewer lab boleh menulis data selama sesi
//! `running`; sidik diperiksa setelah setiap kartu dan restore dijalankan bila
//! berbeda; setelah kartu terakhir semua penyimpanan dipulihkan dan sidiknya
//! dibandingkan. Status sesi tahan restart di `lab_sessions`.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use mandor_domain::AcRow;
use mandor_domain::app_run::AppRunState;
use mandor_domain::lab::LabNeed;
use mandor_domain::lab_data::{SessionEvent, SessionPhase, next_phase};

use super::lab::LabProgress;
use crate::ports::{Clock, Deps, Ids};
use crate::run::{RunError, RunService};
use crate::run_ports::{
    AppProcesses, AppRecords, ChatItem, DataReady, LabPort, LabSessionRecord, LabSessions,
    LedgerStore, LockOutcome, RestoreResult, RunRecord, RunStore, SqlResult,
};

/// Alasan calon data yang ditunda di laptop (KTD-L23).
pub(crate) const DEFERRED: &str = "Lab with test data deferred: it changes your local data and stops the app until it restores. Use Run lab in the AC tab.";
const LOCKED: &str = "local data is locked by another lab";
const APP_NOT_OURS: &str = "lab needs the app started from this run";
/// Jendela coba ulang restore saat pemulihan (KTD-L22).
const RECOVERY_WINDOW: Duration = Duration::from_secs(600);
/// Jeda coba ulang `restore_waiting`.
const WAIT_RETRY: Duration = Duration::from_secs(10);

/// Sesi lab data yang sedang berjalan untuk satu review.
#[derive(Debug, Clone)]
pub(crate) struct DataSession {
    pub id: String,
    pub repo_id: String,
    pub ready: DataReady,
    pub snapshot: PathBuf,
    pub before: Vec<(String, String)>,
    pub worktree: PathBuf,
    /// Review pemilik sesi; dipakai untuk menyamarkan nilai cookie sesi.
    pub review_id: String,
}

/// Baris calon `data` di kartu-kartu progres.
fn data_rows(rows: &[AcRow], progress: &LabProgress) -> Vec<String> {
    progress
        .cards
        .iter()
        .flat_map(|c| c.ac_ids.clone())
        .filter(|id| {
            rows.iter()
                .any(|r| &r.ac_id == id && r.lab_need == Some(LabNeed::Data))
        })
        .collect()
}

impl<D: Deps> RunService<D> {
    /// Calon `data` yang tidak bisa diuji: catatan alasan di baris dan baris
    /// itu dikeluarkan dari kartu lab.
    async fn drop_data_rows(
        &self,
        run_id: &str,
        progress: &mut LabProgress,
        ids: &[String],
        reason: &str,
    ) -> Result<(), RunError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.note_rows(run_id, ids, reason).await?;
        for c in &mut progress.cards {
            c.ac_ids.retain(|id| !ids.contains(id));
        }
        progress.cards.retain(|c| !c.ac_ids.is_empty());
        Ok(())
    }

    /// Memulai sesi lab data sebelum kartu pertama bila ada calon `data` dan
    /// lab data diizinkan (VM, atau developer memilih Run lab di laptop).
    pub(crate) async fn begin_data(
        &self,
        run: &RunRecord,
        progress: &mut LabProgress,
    ) -> Result<(), RunError> {
        if progress.data_attempted {
            return Ok(());
        }
        progress.data_attempted = true;
        let rows = self.reviews.ac_rows(&run.id).await?;
        let ids = data_rows(&rows, progress);
        if ids.is_empty() {
            return Ok(());
        }
        if !progress.plan.data_allowed {
            return self.drop_data_rows(&run.id, progress, &ids, DEFERRED).await;
        }
        match self.open_data_session(run, progress).await {
            Ok(session) => {
                progress.data = Some(session);
                self.lab.set_proxy_writes(&progress.review_id, true);
                Ok(())
            }
            Err(reason) => {
                let note = format!("Lab with test data did not run: {reason}.");
                self.runs
                    .append(&run.id, &ChatItem::System(note), self.now())
                    .await?;
                self.drop_data_rows(&run.id, progress, &ids, &format!("Can't verify: {reason}."))
                    .await
            }
        }
    }

    async fn open_data_session(
        &self,
        run: &RunRecord,
        progress: &LabProgress,
    ) -> Result<DataSession, String> {
        let repo_id = progress.plan.repo_id.clone();
        let worktree = self
            .member_worktree(run, &repo_id)
            .await
            .ok_or_else(|| "the run has no worktree".to_owned())?;
        let ready = self
            .lab
            .data_prepare(&repo_id, &worktree)
            .await
            .map_err(|e| e.to_string())??;
        if !self.is_vm() && !self.app_is_ours(&run.id).await {
            return Err(APP_NOT_OURS.to_owned());
        }
        let id = format!("lab-{}", self.sync.ids.nonce());
        let settings = self.reviews_lab_wait().await;
        match self.lab.data_lock(&ready.lock_keys, &id, settings).await {
            LockOutcome::Acquired => {}
            LockOutcome::Locked => return Err(LOCKED.to_owned()),
            LockOutcome::RestoreFailed(why) => return Err(why),
        }
        if let Some(clients) = self.lab.data_other_clients(&repo_id).await {
            self.lab.data_release(&ready.lock_keys).await;
            return Err(format!("another database client is connected: {clients}"));
        }
        let now = self.now();
        let rec = LabSessionRecord {
            id: id.clone(),
            run_id: run.id.clone(),
            repo_id: repo_id.clone(),
            lock_keys: ready.lock_keys.clone(),
            phase: SessionPhase::Snapshotting,
            snapshot_dir: None,
            scripts_digest: ready.scripts_digest.clone(),
            fingerprints: Vec::new(),
            error: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        if let Err(e) = self.lab.sessions().create_session(&rec).await {
            self.lab.data_release(&ready.lock_keys).await;
            return Err(e.to_string());
        }
        self.lab
            .data_mark(&ready.lock_keys, &id, SessionPhase::Snapshotting)
            .await;
        self.stop_run_app(&run.id).await;
        let snap = self.lab.data_snapshot(&repo_id, &id).await;
        let (snapshot, before) = match snap {
            Ok(v) => v,
            Err(why) => {
                self.move_phase(
                    &id,
                    SessionPhase::Snapshotting,
                    SessionEvent::SnapshotFailed,
                    None,
                )
                .await;
                self.lab.data_release(&ready.lock_keys).await;
                self.start_run_app(&run.id).await;
                return Err(format!("the snapshot failed: {why}"));
            }
        };
        let _ = self
            .lab
            .sessions()
            .set_snapshot(&id, &snapshot, &before, self.now())
            .await;
        self.move_phase(
            &id,
            SessionPhase::Snapshotting,
            SessionEvent::SnapshotOk,
            None,
        )
        .await;
        self.lab
            .data_mark(&ready.lock_keys, &id, SessionPhase::Running)
            .await;
        self.start_run_app(&run.id).await;
        Ok(DataSession {
            id,
            repo_id,
            ready,
            snapshot,
            before,
            worktree,
            review_id: progress.review_id.clone(),
        })
    }

    /// Tenggat menunggu kunci: batas waktu lab per run ditambah jendela coba
    /// ulang restore (KTD-L21).
    async fn reviews_lab_wait(&self) -> Duration {
        use crate::run_ports::ReviewHistory;
        let s = self.reviews.lab_settings().await;
        Duration::from_secs(s.run_timeout_secs) + RECOVERY_WINDOW
    }

    async fn member_worktree(&self, run: &RunRecord, repo_id: &str) -> Option<PathBuf> {
        match self.members(run).await {
            Ok(members) => members
                .into_iter()
                .find(|m| m.repo.id == repo_id)
                .map(|m| m.worktree)
                .or_else(|| run.worktree.clone()),
            Err(_) => run.worktree.clone(),
        }
    }

    /// App yang dibuka reviewer dilayani app milik run ini (KTD-L20).
    async fn app_is_ours(&self, run_id: &str) -> bool {
        let owner = self.apps.app_owner().await.ok().flatten();
        owner.as_deref() == Some(run_id)
            && self
                .apps
                .app_record(run_id)
                .await
                .ok()
                .flatten()
                .is_some_and(|r| r.state == AppRunState::Running)
    }

    async fn stop_run_app(&self, run_id: &str) {
        self.apps.stop(run_id).await;
    }

    /// Menyalakan app run lagi dari kode yang sama; gagal hanya dicatat.
    async fn start_run_app(&self, run_id: &str) {
        let snapshot = self
            .apps
            .app_record(run_id)
            .await
            .ok()
            .flatten()
            .map(|r| r.snapshot)
            .unwrap_or_default();
        if snapshot.is_empty() {
            return;
        }
        if let Err(why) = self.restart_app(run_id, &snapshot).await {
            let note =
                format!("The app did not start again after the lab ({why}); start it manually.");
            let _ = self
                .runs
                .append(run_id, &ChatItem::System(note), self.now())
                .await;
        }
    }

    /// Memindah fase sesi menurut diagram; transisi tidak sah hanya dicatat.
    async fn move_phase(
        &self,
        id: &str,
        from: SessionPhase,
        event: SessionEvent,
        error: Option<&str>,
    ) -> Option<SessionPhase> {
        let to = match next_phase(from, event) {
            Ok(to) => to,
            Err(e) => {
                tracing::warn!(session = id, error = %e, "transisi sesi lab ditolak");
                return None;
            }
        };
        match self
            .lab
            .sessions()
            .transition(id, from, to, error, self.now())
            .await
        {
            Ok(true) => Some(to),
            Ok(false) => None,
            Err(e) => {
                tracing::warn!(session = id, error = %e, "fase sesi lab tidak tersimpan");
                None
            }
        }
    }

    /// Setelah kartu: bila sidik berbeda dari awal, pulihkan sebelum kartu
    /// berikutnya (KTD-L20). `false` bila restore gagal dan lab data berhenti.
    pub(crate) async fn data_between_cards(
        self: &Arc<Self>,
        run_id: &str,
        session: &DataSession,
    ) -> bool {
        let now = self
            .lab
            .data_fingerprints(&session.repo_id, &session.snapshot)
            .await;
        if now.as_ref().is_ok_and(|fp| fp == &session.before) {
            return true;
        }
        self.restore_session(run_id, session, SessionPhase::Running, false)
            .await
    }

    /// Restore sesi dari `from` (`running` atau `restore_failed`): app
    /// dihentikan, restore, sidik, app dinyalakan. `last` = kartu terakhir
    /// selesai (berakhir `done`). `true` bila data pulih.
    pub(crate) async fn restore_session(
        self: &Arc<Self>,
        run_id: &str,
        session: &DataSession,
        from: SessionPhase,
        last: bool,
    ) -> bool {
        let event = if from == SessionPhase::RestoreFailed {
            SessionEvent::Retry
        } else {
            SessionEvent::NeedRestore
        };
        if self
            .move_phase(&session.id, from, event, None)
            .await
            .is_none()
        {
            return false;
        }
        self.lab
            .data_mark(
                &session.ready.lock_keys,
                &session.id,
                SessionPhase::Restoring,
            )
            .await;
        self.stop_run_app(run_id).await;
        let result = self
            .lab
            .data_restore(&session.repo_id, &session.snapshot, &session.before)
            .await;
        match result {
            RestoreResult::Restored => {
                if last {
                    self.close_session(session).await;
                } else {
                    self.move_phase(
                        &session.id,
                        SessionPhase::Restoring,
                        SessionEvent::RestoredNext,
                        None,
                    )
                    .await;
                    self.lab
                        .data_mark(&session.ready.lock_keys, &session.id, SessionPhase::Running)
                        .await;
                }
                self.start_run_app(run_id).await;
                true
            }
            RestoreResult::Waiting(clients) => {
                self.move_phase(
                    &session.id,
                    SessionPhase::Restoring,
                    SessionEvent::OtherClients,
                    None,
                )
                .await;
                self.lab
                    .data_mark(
                        &session.ready.lock_keys,
                        &session.id,
                        SessionPhase::RestoreWaiting,
                    )
                    .await;
                let note = format!(
                    "Restoring the lab data is waiting for another database client to disconnect ({clients}). Stop that app to continue."
                );
                let _ = self
                    .runs
                    .append(run_id, &ChatItem::System(note), self.now())
                    .await;
                self.spawn_restore_wait(run_id, session.clone());
                false
            }
            RestoreResult::Failed(why) => {
                self.fail_session(run_id, session, &why).await;
                false
            }
        }
    }

    async fn close_session(&self, session: &DataSession) {
        self.move_phase(
            &session.id,
            SessionPhase::Restoring,
            SessionEvent::RestoredLast,
            None,
        )
        .await;
        self.lab.data_discard(&session.snapshot).await;
        self.lab.data_release(&session.ready.lock_keys).await;
    }

    async fn fail_session(&self, run_id: &str, session: &DataSession, why: &str) {
        self.move_phase(
            &session.id,
            SessionPhase::Restoring,
            SessionEvent::RestoreFailed,
            Some(why),
        )
        .await;
        self.lab
            .data_mark(
                &session.ready.lock_keys,
                &session.id,
                SessionPhase::RestoreFailed,
            )
            .await;
        let note = format!(
            "Restoring the lab data failed: {why}. Reviews, app starts, and agent turns for this repo are paused until you retry the restore or confirm the data is restored."
        );
        let _ = self
            .runs
            .append(run_id, &ChatItem::System(note), self.now())
            .await;
    }

    /// `restore_waiting`: coba ulang sampai klien lain putus; tidak pernah
    /// naik sendiri ke `restore_failed` (KTD-L20).
    fn spawn_restore_wait(self: &Arc<Self>, run_id: &str, session: DataSession) {
        use crate::run_ports::Spawner;
        let this = Arc::clone(self);
        let run_id = run_id.to_owned();
        self.spawner.spawn(async move {
            loop {
                this.sync.clock.sleep(WAIT_RETRY).await;
                let phase = match this.lab.sessions().session(&session.id).await {
                    Ok(Some(rec)) => rec.phase,
                    _ => return,
                };
                if phase != SessionPhase::RestoreWaiting {
                    return;
                }
                if this
                    .lab
                    .data_other_clients(&session.repo_id)
                    .await
                    .is_some()
                {
                    continue;
                }
                this.move_phase(
                    &session.id,
                    SessionPhase::RestoreWaiting,
                    SessionEvent::ClientsGone,
                    None,
                )
                .await;
                match this
                    .lab
                    .data_restore(&session.repo_id, &session.snapshot, &session.before)
                    .await
                {
                    RestoreResult::Restored => {
                        this.close_session(&session).await;
                        this.start_run_app(&run_id).await;
                        return;
                    }
                    RestoreResult::Waiting(_) => {
                        this.move_phase(
                            &session.id,
                            SessionPhase::Restoring,
                            SessionEvent::OtherClients,
                            None,
                        )
                        .await;
                    }
                    RestoreResult::Failed(why) => {
                        this.fail_session(&run_id, &session, &why).await;
                        return;
                    }
                }
            }
        });
    }

    /// Akhir lab: proxy kembali hanya baca dan semua penyimpanan dipulihkan.
    pub(crate) async fn end_data(self: &Arc<Self>, run_id: &str, progress: &mut LabProgress) {
        let Some(session) = progress.data.take() else {
            return;
        };
        self.lab.set_proxy_writes(&progress.review_id, false);
        self.restore_session(run_id, &session, SessionPhase::Running, true)
            .await;
    }

    /// Lab data yang perlu tindakan developer (laporan pagi VM): restore
    /// gagal, atau `restore_waiting` lebih lama dari jendela coba ulang.
    pub(crate) async fn lab_data_alerts(&self, now: i64) -> Vec<crate::overnight::LabDataAlert> {
        let window = i64::try_from(RECOVERY_WINDOW.as_millis()).unwrap_or(i64::MAX);
        self.lab
            .sessions()
            .holding()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|s| {
                s.phase == SessionPhase::RestoreFailed
                    || (s.phase == SessionPhase::RestoreWaiting && now - s.updated_at_ms >= window)
            })
            .map(|s| crate::overnight::LabDataAlert {
                repo_id: s.repo_id,
                run_id: s.run_id,
                phase: s.phase,
                error: s.error,
            })
            .collect()
    }

    /// Sesi lab data yang masih memegang data lokal repo (banner UI).
    pub async fn lab_data_session(&self, repo_id: &str) -> Option<LabSessionRecord> {
        self.lab
            .sessions()
            .holding()
            .await
            .ok()?
            .into_iter()
            .find(|s| s.repo_id == repo_id)
    }

    /// Fase sesi lab data yang memegang kunci penyimpanan salah satu repo.
    pub(crate) async fn data_phase_for(&self, repos: &[String]) -> Option<SessionPhase> {
        let held = self.lab.sessions().holding().await.ok()?;
        held.iter()
            .find(|s| repos.contains(&s.repo_id))
            .map(|s| s.phase)
    }

    /// Penyimpanan repo menunggu developer karena restore gagal (R6); alasan
    /// untuk menolak giliran, review, dan start app.
    pub(crate) async fn data_blocked(&self, repo_id: &str) -> Option<String> {
        let held = self.lab.sessions().holding().await.ok()?;
        held.iter()
            .find(|s| s.repo_id == repo_id && s.phase == SessionPhase::RestoreFailed)
            .map(|s| {
                format!(
                    "the lab data of {repo_id} was not restored ({}); retry the restore or confirm the data is restored in Settings",
                    s.error.as_deref().unwrap_or("restore failed")
                )
            })
    }

    /// Satu pernyataan `lab_sql` reviewer lab selama sesi data `running`.
    ///
    /// # Errors
    /// Tidak ada sesi data berjalan, penyimpanan tidak dikenal, atau
    /// pernyataan ditolak.
    pub async fn ledger_lab_sql(
        &self,
        scope: &super::CapabilityScope,
        store: &str,
        sql: &str,
    ) -> Result<SqlResult, super::LedgerToolError> {
        let session = self.running_data(scope)?;
        if !session.ready.sql_stores.iter().any(|s| s == store) {
            return Err(super::LedgerToolError::Invalid(format!(
                "unknown database store {store}; use one of: {}",
                session.ready.sql_stores.join(", ")
            )));
        }
        let out = self
            .lab
            .data_sql(&session.repo_id, store, sql)
            .await
            .map_err(|e| super::LedgerToolError::Invalid(e.to_string()))?;
        self.mark_data_changed(&scope.run_id);
        let mut texts = self
            .lab
            .redact(
                std::slice::from_ref(&session.repo_id),
                &session.review_id,
                vec![out.text],
            )
            .await;
        Ok(SqlResult {
            text: texts.pop().unwrap_or_default(),
            ..out
        })
    }

    /// Seed bernama selama sesi data `running`.
    ///
    /// # Errors
    /// Tidak ada sesi data berjalan, seed tidak dikenal, atau seed gagal.
    pub async fn ledger_lab_seed(
        &self,
        scope: &super::CapabilityScope,
        name: &str,
    ) -> Result<String, super::LedgerToolError> {
        let session = self.running_data(scope)?;
        if !session.ready.seeds.iter().any(|s| s == name) {
            return Err(super::LedgerToolError::Invalid(format!(
                "unknown seed {name}; use one of: {}",
                session.ready.seeds.join(", ")
            )));
        }
        self.mark_data_changed(&scope.run_id);
        let out = self
            .lab
            .data_seed(&session.repo_id, name, &session.worktree)
            .await
            .map_err(|e| super::LedgerToolError::Invalid(e.to_string()))?;
        let mut texts = self
            .lab
            .redact(
                std::slice::from_ref(&session.repo_id),
                &session.review_id,
                vec![out],
            )
            .await;
        Ok(texts.pop().unwrap_or_default())
    }

    fn running_data(
        &self,
        scope: &super::CapabilityScope,
    ) -> Result<DataSession, super::LedgerToolError> {
        if scope.role != super::CapabilityRole::Verdict {
            return Err(super::LedgerToolError::WrongRole);
        }
        self.lab_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&scope.run_id)
            .and_then(|p| p.data.clone())
            .ok_or(super::LedgerToolError::NotAllowed(
                "lab data is not running for this review",
            ))
    }

    /// Developer memilih Run lab: review berikutnya run ini boleh
    /// menjalankan lab data (KTD-L23), lalu review dimulai.
    ///
    /// # Errors
    /// Seperti [`Self::start_review`].
    pub async fn start_lab_data(
        self: &Arc<Self>,
        run_id: &str,
    ) -> Result<crate::run_ports::ReviewRecord, RunError> {
        self.lab_data_requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run_id.to_owned());
        let started = self
            .start_review(run_id, crate::run_ports::ReviewTrigger::Manual, None)
            .await;
        if started.is_err() {
            self.lab_data_requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(run_id);
        }
        started
    }

    /// Mengambil permintaan Run lab untuk run (sekali per review).
    pub(crate) fn take_data_request(&self, run_id: &str) -> bool {
        self.lab_data_requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(run_id)
    }

    /// Kartu lab ini sudah mengubah data lewat `lab_sql`/`lab_seed`.
    pub(crate) fn lab_data_changed(&self, run_id: &str) -> bool {
        self.lab_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .is_some_and(|p| p.data_changed)
    }

    fn mark_data_changed(&self, run_id: &str) {
        if let Some(p) = self
            .lab_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(run_id)
        {
            p.data_changed = true;
        }
    }

    /// Sesi data berjalan untuk run (alat `lab_sql`/`lab_seed` ditawarkan).
    #[must_use]
    pub fn lab_data_running(&self, run_id: &str) -> bool {
        self.lab_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .is_some_and(|p| p.data.is_some())
    }

    /// Sesi data dari rekaman (pemulihan dan aksi developer).
    fn session_of(rec: &LabSessionRecord) -> Option<DataSession> {
        Some(DataSession {
            id: rec.id.clone(),
            repo_id: rec.repo_id.clone(),
            ready: DataReady {
                lock_keys: rec.lock_keys.clone(),
                scripts_digest: rec.scripts_digest.clone(),
                sql_stores: Vec::new(),
                seeds: Vec::new(),
            },
            snapshot: rec.snapshot_dir.clone()?,
            before: rec.fingerprints.clone(),
            worktree: PathBuf::new(),
            review_id: String::new(),
        })
    }

    /// Menjalankan [`Self::recover_lab_data`] di latar lewat spawner milik
    /// layanan, agar daemon langsung melayani.
    pub fn spawn_lab_recovery(self: &Arc<Self>) {
        use crate::run_ports::Spawner;
        let this = Arc::clone(self);
        self.spawner
            .spawn(async move { this.recover_lab_data().await });
    }

    /// Pemulihan saat daemon start (KTD-L22), dijalankan sebagai task
    /// terlacak setelah daemon siap: kunci diambil ulang; `snapshotting`
    /// selesai tanpa restore; `running`/`restoring`/`restore_waiting`
    /// dipulihkan dengan coba ulang sampai 10 menit sebelum `restore_failed`;
    /// `restore_failed` tetap memegang kunci sampai developer bertindak.
    pub async fn recover_lab_data(self: &Arc<Self>) {
        let held = match self.lab.sessions().holding().await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(error = %e, "sesi lab data tidak terbaca saat start");
                return;
            }
        };
        let active: Vec<String> = held.iter().flat_map(|s| s.lock_keys.clone()).collect();
        self.lab.data_clear_stale(&active).await;
        for rec in held {
            self.recover_one(&rec).await;
        }
    }

    async fn recover_one(self: &Arc<Self>, rec: &LabSessionRecord) {
        let lock = self
            .lab
            .data_lock(&rec.lock_keys, &rec.id, Duration::from_secs(30))
            .await;
        if lock != LockOutcome::Acquired {
            tracing::warn!(session = %rec.id, "kunci lab data tidak bisa diambil ulang saat start");
            return;
        }
        match rec.phase {
            SessionPhase::Snapshotting => {
                self.move_phase(&rec.id, rec.phase, SessionEvent::DaemonRestart, None)
                    .await;
                if let Some(dir) = &rec.snapshot_dir {
                    self.lab.data_discard(dir).await;
                }
                self.lab.data_release(&rec.lock_keys).await;
            }
            SessionPhase::Running | SessionPhase::Restoring | SessionPhase::RestoreWaiting => {
                self.recover_restore(rec).await;
            }
            SessionPhase::RestoreFailed | SessionPhase::Done => {}
        }
    }

    async fn recover_restore(self: &Arc<Self>, rec: &LabSessionRecord) {
        if rec.phase != SessionPhase::Restoring
            && self
                .move_phase(&rec.id, rec.phase, SessionEvent::DaemonRestart, None)
                .await
                .is_none()
        {
            return;
        }
        let Some(session) = Self::session_of(rec) else {
            self.fail_session(
                &rec.run_id,
                &fallback(rec),
                "the snapshot folder is unknown",
            )
            .await;
            return;
        };
        let started = self.now();
        let mut delay = Duration::from_secs(5);
        loop {
            if self
                .lab
                .data_other_clients(&session.repo_id)
                .await
                .is_none()
            {
                match self
                    .lab
                    .data_restore(&session.repo_id, &session.snapshot, &session.before)
                    .await
                {
                    RestoreResult::Restored => {
                        self.close_session(&session).await;
                        tracing::info!(session = %session.id, "lab data dipulihkan setelah daemon start");
                        return;
                    }
                    RestoreResult::Failed(why)
                        if self.now() - started
                            >= i64::try_from(RECOVERY_WINDOW.as_millis()).unwrap_or(i64::MAX) =>
                    {
                        self.fail_session(&rec.run_id, &session, &why).await;
                        return;
                    }
                    RestoreResult::Failed(_) | RestoreResult::Waiting(_) => {}
                }
            }
            if self.now() - started
                >= i64::try_from(RECOVERY_WINDOW.as_millis()).unwrap_or(i64::MAX)
            {
                self.move_phase(
                    &session.id,
                    SessionPhase::Restoring,
                    SessionEvent::OtherClients,
                    None,
                )
                .await;
                self.spawn_restore_wait(&rec.run_id, session);
                return;
            }
            self.sync.clock.sleep(delay).await;
            delay = (delay * 2).min(Duration::from_secs(60));
        }
    }

    /// Sesi `restore_failed` repo.
    async fn failed_session(&self, repo_id: &str) -> Result<LabSessionRecord, RunError> {
        self.lab
            .sessions()
            .holding()
            .await?
            .into_iter()
            .find(|s| s.repo_id == repo_id && s.phase == SessionPhase::RestoreFailed)
            .ok_or(RunError::InvalidState(
                "no failed lab restore for this repo",
            ))
    }

    /// Developer menekan Retry restore (KTD-L22): app dihentikan, restore,
    /// sidik, app dinyalakan.
    ///
    /// # Errors
    /// Tidak ada restore yang gagal, atau restore masih gagal.
    pub async fn retry_lab_restore(self: &Arc<Self>, repo_id: &str) -> Result<(), RunError> {
        let rec = self.failed_session(repo_id).await?;
        let session = Self::session_of(&rec).ok_or(RunError::InvalidState(
            "the snapshot of this lab is gone; confirm the data instead",
        ))?;
        if self
            .restore_session(&rec.run_id, &session, SessionPhase::RestoreFailed, true)
            .await
        {
            Ok(())
        } else {
            Err(RunError::LabData(
                self.data_blocked(repo_id).await.unwrap_or_else(|| {
                    "the restore is waiting for another database client".to_owned()
                }),
            ))
        }
    }

    /// Developer menyatakan data sudah pulih (R6): sesi `done`, kunci dilepas,
    /// dan snapshot dibuang bila diminta.
    ///
    /// # Errors
    /// Tidak ada restore yang gagal untuk repo.
    pub async fn confirm_lab_data_restored(
        &self,
        repo_id: &str,
        discard_snapshot: bool,
    ) -> Result<(), RunError> {
        let rec = self.failed_session(repo_id).await?;
        self.move_phase(
            &rec.id,
            SessionPhase::RestoreFailed,
            SessionEvent::DeveloperRestored,
            None,
        )
        .await
        .ok_or(RunError::InvalidState("the lab session changed; reload"))?;
        if discard_snapshot && let Some(dir) = &rec.snapshot_dir {
            self.lab.data_discard(dir).await;
        }
        self.lab.data_release(&rec.lock_keys).await;
        tracing::info!(repo = repo_id, "developer menyatakan lab data pulih");
        Ok(())
    }
}

/// Sesi minimal untuk mencatat kegagalan bila folder snapshot tidak dikenal.
fn fallback(rec: &LabSessionRecord) -> DataSession {
    DataSession {
        id: rec.id.clone(),
        repo_id: rec.repo_id.clone(),
        ready: DataReady {
            lock_keys: rec.lock_keys.clone(),
            scripts_digest: rec.scripts_digest.clone(),
            sql_stores: Vec::new(),
            seeds: Vec::new(),
        },
        snapshot: PathBuf::new(),
        before: Vec::new(),
        worktree: PathBuf::new(),
        review_id: String::new(),
    }
}
