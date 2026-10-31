//! Penjadwal lembur di daemon VM (U2 plan VM, KTD-57): antrean kartu, jam
//! lembur, "Start now", dan giliran Waiting Fix, satu kartu sekaligus.
//!
//! Penjadwal hanya memutuskan dan memulai; eksekusi memakai jalur Start,
//! lanjutan sesi, dan mesin Waiting Fix yang sudah ada.

mod flow;
mod group;
mod handoff;
mod planned;
mod ports;
mod report;

use std::sync::Arc;

pub use handoff::Handback;
use mandor_domain::gate::check_start;
use mandor_domain::overnight::{CycleReport, Next, Slot, TickState, Window, decide, report_text};
use mandor_domain::planned::{PlannedState, PlannedWork, check_planned_start};
use mandor_domain::{DaemonRole, HoldReason, RunState, TaskStatus};
pub use planned::QueueRequest;
pub use ports::{
    CycleUsage, Dequeued, OvernightSettings, OvernightStore, PlannedEntry, QueueItem, ReportState,
};
pub use report::{CardOutcome, HeldPlanned, OvernightReport, ReportCard};

use crate::StartTarget;
use crate::ports::{Clock, Deps, LarkTasks, PortError, TaskCache};
use crate::review::DesignSources as _;
use crate::run::{RunError, RunService};
use crate::run_ports::{AgentPort, RunPhase, RunRecord, RunStore};

/// Batas percobaan kirim DM laporan per siklus.
const REPORT_ATTEMPTS: u32 = 5;

/// Pesan lanjutan untuk kartu tertahan yang diantre ulang tanpa catatan.
const CONTINUE: &str = "Continue the previous work from where you left off.";

/// Hasil satu tick penjadwal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OvernightTick {
    /// Kartu yang mulai atau dilanjutkan pada tick ini.
    pub started: Option<String>,
    /// Kartu yang dikeluarkan karena tidak lagi memenuhi syarat, beserta alasan.
    pub dequeued: Vec<(String, String)>,
    /// Alasan tidak ada kartu yang dimulai walau giliran ada, mis. login Claude.
    pub blocked: Option<String>,
}

impl<D: Deps> RunService<D> {
    /// Satu tick penjadwal lembur; tidak melakukan apa pun di daemon laptop.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn overnight_tick(self: &Arc<Self>) -> Result<OvernightTick, RunError> {
        let mut out = OvernightTick::default();
        if self.sync.role() != DaemonRole::Vm {
            return Ok(out);
        }
        let now = self.sync.clock.now_ms();
        let ready = self.overnight_ready(now).await?;
        let active = self.runs.list_active().await?;
        let running = active.iter().any(|r| r.state.is_working());
        // DM laporan tidak pernah menahan antrean (R24).
        if let Err(e) = self.maybe_report(now, running, &active).await {
            tracing::warn!(error = %e, "laporan lembur gagal diproses");
        }
        let fix_ready = active.iter().any(|r| fix_waiting(r, now));
        let in_window = self.window().await?.is_some_and(|w| w.is_open(now));
        self.prepare_planned().await?;
        // Kartu yang gagal cek ulang dikeluarkan, lalu kartu berikutnya dicoba.
        loop {
            let queue = self.startable_queue().await?;
            let state = TickState {
                slot: if running {
                    Slot::Busy
                } else if ready {
                    Slot::Free
                } else {
                    Slot::NotReady
                },
                fix_ready,
                start_now: queue
                    .iter()
                    .find(|i| i.start_now)
                    .map(|i| i.record_id.as_str()),
                in_window,
                queue_head: queue.first().map(|i| i.record_id.as_str()),
            };
            let Next::Start(id) = decide(&state) else {
                return Ok(out);
            };
            let Some(item) = queue.iter().find(|i| i.record_id == id).cloned() else {
                return Ok(out);
            };
            // Login Claude diperiksa tepat sebelum memulai (KTD-59).
            if !self.agent.logged_in().await {
                out.blocked = Some("Claude Code is not logged in on the VM".to_owned());
                return Ok(out);
            }
            match self.start_queued(&item, now).await? {
                Ok(()) => {
                    out.started = Some(item.record_id);
                    return Ok(out);
                }
                Err(reason) => out.dequeued.push((item.record_id, reason)),
            }
        }
    }

    /// DM laporan siklus sekali setelah jam lembur berakhir dan tidak ada run
    /// VM berjalan (R24, KTD-69). Kegagalan kirim dicoba lagi pada tick
    /// berikutnya sampai [`REPORT_ATTEMPTS`].
    async fn maybe_report(
        &self,
        now: i64,
        running: bool,
        active: &[RunRecord],
    ) -> Result<(), RunError> {
        let Some(settings) = self.overnight.settings().await? else {
            return Ok(());
        };
        let Some(window) = window_of(&settings) else {
            return Ok(());
        };
        if !window.report_due(now, running) {
            return Ok(());
        }
        let start = window.cycle_start(now);
        let state = self.overnight.report_state(start).await?;
        if state.sent || state.attempts >= REPORT_ATTEMPTS {
            return Ok(());
        }
        let report = self.cycle_report(start, &settings, active).await?;
        let sent = match self.sync.lark.message_me(&report_text(&report)).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "DM laporan lembur gagal dikirim");
                false
            }
        };
        self.overnight.record_report(start, sent, now).await?;
        Ok(())
    }

    /// Ringkasan siklus: kartu VM yang menunggu dev, yang dikeluarkan dari
    /// antrean, dan pemakaian.
    async fn cycle_report(
        &self,
        start: i64,
        settings: &OvernightSettings,
        active: &[RunRecord],
    ) -> Result<CycleReport, RunError> {
        let mut report = CycleReport {
            used: self.overnight.cycle_usage(start).await?.counted(),
            limit: settings.token_limit,
            ..CycleReport::default()
        };
        for run in active {
            let key = self.key_of_record(&run.record_id).await;
            match (&run.state, &run.hold) {
                (RunState::Held, Some(h)) if h.reason == HoldReason::AcGate => {
                    report.held_ac_gate.push(key);
                }
                (RunState::Held, _) => report.held_other.push(key),
                (RunState::Failed, _) => report.failed.push(key),
                _ if run.draft_pending.is_some() => report.drafts.push(key),
                _ => {}
            }
        }
        for d in self.overnight.dequeued_since(start).await? {
            report.dequeued.push(self.key_of_record(&d.record_id).await);
        }
        for h in report::held_planned(&self.overnight.queue().await?) {
            report
                .held_other
                .push(self.key_of_record(&h.record_id).await);
        }
        Ok(report)
    }

    /// Kunci kartu dari cache; `record_id` bila tidak ada.
    async fn key_of_record(&self, record_id: &str) -> String {
        match self.sync.cache.get(record_id).await {
            Ok(Some(c)) => c.task.key.unwrap_or_else(|| record_id.to_owned()),
            _ => record_id.to_owned(),
        }
    }

    /// Batas siklus diisi dan belum habis (R10). Login Claude diperiksa di U3.
    async fn overnight_ready(&self, now: i64) -> Result<bool, RunError> {
        let Some(settings) = self.overnight.settings().await? else {
            return Ok(false);
        };
        let Some(limit) = settings.token_limit.filter(|l| *l > 0) else {
            return Ok(false);
        };
        let Some(window) = window_of(&settings) else {
            return Ok(false);
        };
        let used = self.overnight.cycle_usage(window.cycle_start(now)).await?;
        Ok(used.counted() < limit)
    }

    pub(super) async fn window(&self) -> Result<Option<Window>, RunError> {
        Ok(self
            .overnight
            .settings()
            .await?
            .as_ref()
            .and_then(window_of))
    }

    /// Mulai atau lanjutkan kartu antrean. `Ok(Err(alasan))` bila kartu tidak
    /// lagi memenuhi syarat (R9): item No plan dikeluarkan, item Planned
    /// ditahan bersama plan dan klaimnya (R15).
    async fn start_queued(
        self: &Arc<Self>,
        item: &QueueItem,
        now: i64,
    ) -> Result<Result<(), String>, RunError> {
        let attempt = match &item.planned {
            Some(p) => {
                self.overnight
                    .set_planned_state(&item.record_id, PlannedState::Starting, None)
                    .await?;
                self.try_start_planned(item, &p.work).await
            }
            None => self.try_start(item).await,
        };
        let reason = match attempt {
            Ok(()) => {
                self.overnight.dequeue(&item.record_id, None, now).await?;
                return Ok(Ok(()));
            }
            Err(reason) => reason,
        };
        if item.planned.is_some() {
            tracing::info!(record_id = %item.record_id, reason, "kartu Planned ditahan di antrean VM");
            self.overnight
                .set_planned_state(&item.record_id, PlannedState::Held, Some(&reason))
                .await?;
        } else {
            tracing::info!(record_id = %item.record_id, reason, "kartu dikeluarkan dari antrean VM");
            self.overnight
                .dequeue(&item.record_id, Some(&reason), now)
                .await?;
        }
        Ok(Err(reason))
    }

    /// Cek syarat kartu Planned (KTD-4) lalu mulai dari plan (KTD-3).
    async fn try_start_planned(
        self: &Arc<Self>,
        item: &QueueItem,
        work: &PlannedWork,
    ) -> Result<(), String> {
        let task = self
            .sync
            .fetch_one(&item.record_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "the card was deleted in Lark".to_owned())?;
        check_planned_start(&task, self.sync.identity())?;
        let (design, notes) = self.queue_design(item).await?;
        self.start_from_plan(&item.record_id, &item.target, work, &design, &notes)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Item Planned sebelum tick memilih kartu: `Starting` yang tersisa
    /// dipulihkan, serah-terima tertunda diperiksa.
    async fn prepare_planned(self: &Arc<Self>) -> Result<(), RunError> {
        self.reset_starting().await?;
        if let Err(e) = self.tend_planned(false).await {
            tracing::warn!(error = %e, "serah-terima kartu Planned gagal diperiksa");
        }
        Ok(())
    }

    /// Antrean yang boleh dimulai penjadwal, urut posisi.
    async fn startable_queue(&self) -> Result<Vec<QueueItem>, RunError> {
        Ok(self
            .overnight
            .queue()
            .await?
            .into_iter()
            .filter(startable)
            .collect())
    }

    /// Sumber desain kartu antrean: repo git diperbarui lalu di-resolve (R12,
    /// R13). `Err` berisi alasan bila ada sumber yang tidak tersedia.
    pub(super) async fn queue_design(
        &self,
        item: &QueueItem,
    ) -> Result<(Vec<crate::review::DesignRoot>, Vec<String>), String> {
        let check = self
            .design
            .prepare(&item.design_sources, true)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(refusal) = check.refusal() {
            return Err(refusal);
        }
        let notes = check.notes();
        Ok((check.roots, notes))
    }

    /// Item `Starting` tanpa run tersisa dari tick yang terputus (mis. daemon
    /// restart); kembali menunggu.
    async fn reset_starting(&self) -> Result<(), RunError> {
        for item in self.overnight.queue().await? {
            let starting = item
                .planned
                .as_ref()
                .is_some_and(|p| p.state == PlannedState::Starting);
            if starting && self.runs.active_for(&item.record_id).await?.is_none() {
                self.overnight
                    .set_planned_state(&item.record_id, PlannedState::Waiting, None)
                    .await?;
            }
        }
        Ok(())
    }

    /// Cek ulang lalu mulai; error berisi alasan untuk laporan pagi.
    async fn try_start(self: &Arc<Self>, item: &QueueItem) -> Result<(), String> {
        let existing = self.group_item_run(item).await.map_err(|e| e.to_string())?;
        if let Some(run) = existing {
            if !resumable(run.state) {
                return Err("the card already has a run".to_owned());
            }
            // Grup tertahan atau berjeda: lanjut menurut keadaan kartunya.
            let group = !self
                .runs
                .cards(&run.id)
                .await
                .map_err(|e| e.to_string())?
                .is_empty();
            if group {
                return self
                    .resume_group(run, item)
                    .await
                    .map_err(|e| e.to_string());
            }
            // Kartu Planned terputus sebelum pesan pertamanya sampai.
            if run.plan.is_some() && run.phase == RunPhase::Implement && !run.session_started {
                return self
                    .resend_approved_plan(&run.id)
                    .await
                    .map_err(|e| e.to_string());
            }
            // Kartu tertahan atau terputus: lanjut di branch, worktree, dan
            // sesi yang sama.
            let text = item.note.as_deref().unwrap_or(CONTINUE);
            return self
                .send_message(&run.id, text)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
        if !item.group.is_empty() {
            return self.try_start_group(item).await;
        }
        let task = self
            .sync
            .fetch_one(&item.record_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "the card was deleted in Lark".to_owned())?;
        if !task.assigned_to(self.sync.me()) {
            return Err("the card is no longer assigned to you".to_owned());
        }
        if task.status != TaskStatus::ToDo {
            return Err(format!(
                "the card moved to {} in Lark",
                task.status.as_str()
            ));
        }
        check_start(&task.status, task.readiness.as_deref()).map_err(|e| e.to_string())?;
        let (design, notes) = self.queue_design(item).await?;
        self.start_queued_card(&item.record_id, &item.target, &design, &notes)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Ada run VM yang sedang bekerja; perbaikan Waiting Fix menunggu (R8).
    pub(crate) async fn vm_busy(&self) -> Result<bool, RunError> {
        Ok(self
            .runs
            .list_active()
            .await?
            .iter()
            .any(|r| r.state.is_working()))
    }

    /// Antrean VM, urut posisi.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn overnight_queue(&self) -> Result<Vec<QueueItem>, RunError> {
        Ok(self.overnight.queue().await?)
    }

    /// Mengantrekan kartu To Do, kartu tertahan (R6, R20), atau kartu yang
    /// gagal karena terputus, tanpa plan dan tanpa sumber desain.
    ///
    /// # Errors
    /// Lihat [`Self::enqueue_card`].
    pub async fn queue_add(
        &self,
        record_id: &str,
        target: &StartTarget,
        note: Option<&str>,
    ) -> Result<(), RunError> {
        self.enqueue_card(
            record_id,
            &QueueRequest {
                target: target.clone(),
                note: note.map(str::to_owned),
                design_sources: Vec::new(),
                planned: None,
            },
        )
        .await
    }

    /// Target antrean terdaftar di daemon ini (KTD-11).
    pub(crate) async fn check_target(&self, target: &StartTarget) -> Result<(), RunError> {
        self.resolve(target).await.map(|_| ()).map_err(|e| match e {
            RunError::NotFound(what) => rejected(format!("the {what} is not set up on the VM")),
            other => other,
        })
    }

    /// Dev mengeluarkan kartu dari antrean sebelum dikerjakan.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn queue_remove(&self, record_id: &str) -> Result<(), RunError> {
        let queue = self.overnight.queue().await?;
        let handed_over = queue.iter().any(|i| {
            i.record_id == record_id
                && i.planned
                    .as_ref()
                    .is_some_and(|p| p.state != PlannedState::HandoverPending)
        });
        if handed_over {
            return Err(RunError::InvalidState(
                "the VM holds this planned card; use Take over to laptop instead",
            ));
        }
        Ok(self
            .overnight
            .dequeue(record_id, None, self.sync.clock.now_ms())
            .await?)
    }

    /// Mengurutkan ulang antrean.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn queue_reorder(&self, record_ids: &[String]) -> Result<(), RunError> {
        Ok(self.overnight.reorder(record_ids).await?)
    }

    /// "Start now" pada kartu antrean (R7).
    ///
    /// # Errors
    /// Kartu tidak di antrean.
    pub async fn queue_start_now(&self, record_id: &str) -> Result<(), RunError> {
        Ok(self.overnight.set_start_now(record_id).await?)
    }

    /// Setelan lembur.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn overnight_settings(&self) -> Result<Option<OvernightSettings>, RunError> {
        Ok(self.overnight.settings().await?)
    }

    /// Menyimpan setelan lembur setelah jam dan offset divalidasi.
    ///
    /// # Errors
    /// Jam atau offset tidak valid, atau error penyimpanan.
    pub async fn set_overnight_settings(&self, s: &OvernightSettings) -> Result<(), RunError> {
        Window::parse(&s.start, &s.end, &s.offset)
            .map_err(|e| RunError::Port(PortError::Rejected(e.to_string())))?;
        Ok(self.overnight.set_settings(s).await?)
    }
}

/// Item yang boleh dimulai penjadwal: No plan, atau Planned yang klaimnya
/// sudah di VM.
fn startable(item: &QueueItem) -> bool {
    item.planned
        .as_ref()
        .is_none_or(|p| p.state == PlannedState::Waiting)
}

fn rejected(message: String) -> RunError {
    RunError::Port(PortError::Rejected(message))
}

/// Run kartu yang boleh diantre ulang dan dilanjutkan di sesi yang sama:
/// tertahan, atau gagal (mis. terputus karena VM atau daemon restart).
fn resumable(state: RunState) -> bool {
    matches!(state, RunState::Held | RunState::Failed)
}

pub(crate) fn window_of(s: &OvernightSettings) -> Option<Window> {
    Window::parse(&s.start, &s.end, &s.offset).ok()
}

/// Perbaikan Waiting Fix yang siap dimulai dan menunggu giliran (R21).
pub(super) fn fix_waiting(run: &RunRecord, now: i64) -> bool {
    run.state == RunState::WfQuiet && run.quiet_until_ms.is_some_and(|u| u <= now)
}
