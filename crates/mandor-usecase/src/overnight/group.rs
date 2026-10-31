//! Grup kartu sebagai satu item antrean VM (plan 2026-10-23-004, U12,
//! KTD-G10–KTD-G13, KTD-G18): diantre, dimulai tanpa persetujuan plan,
//! berlanjut otomatis per kartu, berjeda di antara kartu, dan dilanjutkan
//! setelah tertahan.

use std::sync::Arc;

use mandor_domain::gate::check_start;
use mandor_domain::group::{CardState, PauseInput, vm_pause};
use mandor_domain::run::touches_sensitive_path;
use mandor_domain::{DaemonRole, HoldReason, RunState, TaskStatus};

use super::{CONTINUE, OvernightStore, QueueItem, QueueRequest, fix_waiting};
use crate::Deps;
use crate::ports::{Clock, PortError};
use crate::review::DesignSources as _;
use crate::run::{RunError, RunService, target_of};
use crate::run_ports::{GroupCardRow, RunRecord, RunStore, VmStart};

/// Catatan untuk perubahan yang butuh konfirmasi dev (sama dengan kartu tunggal).
const SENSITIVE: &str =
    "Changes touch CI, agent config, or git hooks; push needs developer confirmation";

impl<D: Deps> RunService<D> {
    /// Mengantre grup sebagai satu item (R20, KTD-G10). Kartu pertama menjadi
    /// kunci item; semua kartu dianggap di antrean.
    ///
    /// # Errors
    /// Bukan daemon VM, kurang dari dua kartu, kartu sudah diantre, punya run,
    /// "hanya laptop", gagal gerbang Start, atau sumber desain tidak tersedia.
    pub async fn enqueue_group(
        &self,
        record_ids: &[String],
        req: &QueueRequest,
    ) -> Result<(), RunError> {
        if self.sync.role() != DaemonRole::Vm {
            return Err(RunError::InvalidState(
                "the overnight queue runs on the VM daemon",
            ));
        }
        let Some(first) = record_ids.first().filter(|_| record_ids.len() >= 2) else {
            return Err(RunError::InvalidState("a group needs at least two cards"));
        };
        let queued = self.queued_cards().await?;
        for id in record_ids {
            if queued.contains(id) {
                return Err(RunError::InvalidState("a card is already queued"));
            }
            if self.overnight.laptop_only(id).await? {
                return Err(RunError::InvalidState(
                    "this card was taken over to the laptop",
                ));
            }
            if self.runs.active_for(id).await?.is_some() {
                return Err(RunError::InvalidState("a card already has a run"));
            }
            self.check_no_plan(id, None).await?;
        }
        if !req.design_sources.is_empty() {
            let check = self.design.prepare(&req.design_sources, false).await?;
            if let Some(refusal) = check.refusal() {
                return Err(RunError::Port(PortError::Rejected(refusal)));
            }
        }
        let item = QueueItem {
            record_id: first.clone(),
            target: req.target.clone(),
            note: req
                .note
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned),
            start_now: false,
            added_at_ms: self.sync.clock.now_ms(),
            design_sources: req.design_sources.clone(),
            planned: None,
            group: record_ids.to_vec(),
        };
        match self.overnight.enqueue(&item).await {
            Err(PortError::Conflict) => Err(RunError::InvalidState("a card is already queued")),
            other => Ok(other?),
        }
    }

    /// Kartu yang dipegang antrean, termasuk anggota grup.
    pub(super) async fn queued_cards(&self) -> Result<Vec<String>, RunError> {
        Ok(self
            .overnight
            .queue()
            .await?
            .iter()
            .flat_map(|i| i.cards().into_iter().map(str::to_owned).collect::<Vec<_>>())
            .collect())
    }

    /// Run terbuka item grup, dicari lewat kartu mana pun di item (kartu
    /// utama bisa pindah setelah revert, KTD-G10).
    pub(super) async fn group_item_run(
        &self,
        item: &QueueItem,
    ) -> Result<Option<RunRecord>, RunError> {
        for id in item.cards() {
            if let Some(run) = self.runs.active_for(id).await? {
                return Ok(Some(run));
            }
        }
        Ok(None)
    }

    /// Item grup baru: periksa setiap kartu lalu mulai grup (R6 VM, F2).
    pub(super) async fn try_start_group(self: &Arc<Self>, item: &QueueItem) -> Result<(), String> {
        for id in &item.group {
            let task = self
                .sync
                .fetch_one(id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("{id} was deleted in Lark"))?;
            let key = task.key.clone().unwrap_or_else(|| id.clone());
            if !task.assigned_to(self.sync.me()) {
                return Err(format!("{key} is no longer assigned to you"));
            }
            if task.status != TaskStatus::ToDo {
                return Err(format!("{key} moved to {} in Lark", task.status.as_str()));
            }
            check_start(&task.status, task.readiness.as_deref())
                .map_err(|e| format!("{key}: {e}"))?;
        }
        let (design, notes) = self.queue_design(item).await?;
        self.start_vm_group(
            &item.group,
            &item.target,
            item.note.as_deref(),
            (&design, &notes),
            item.start_now,
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// Melanjutkan grup VM yang tertahan atau berjeda (KTD-G18): kartu aktif
    /// lanjut di sesinya; tanpa kartu aktif, kartu berikutnya dimulai; semua
    /// selesai, gerbang dan PR draft.
    pub(super) async fn resume_group(
        self: &Arc<Self>,
        mut run: RunRecord,
        item: &QueueItem,
    ) -> Result<(), RunError> {
        let mut cards = self.group_of(&run.id).await?;
        if item.start_now {
            run.vm_start = VmStart::Now;
        }
        let note = item.note.as_deref();
        if cards.iter().any(|c| c.state == CardState::Active) {
            self.runs.save(&run, run.generation, self.now()).await?;
            self.send_message(&run.id, note.unwrap_or(CONTINUE)).await?;
            return Ok(());
        }
        run.state = RunState::AwaitingDev;
        run.hold = None;
        if cards.iter().any(|c| c.state == CardState::Pending) {
            return self.advance(&mut run, &mut cards, note).await;
        }
        self.runs.save(&run, run.generation, self.now()).await?;
        self.vm_publish(&run.id).await
    }

    /// Giliran kartu grup selesai di VM (KTD-G11): worktree bersih berarti
    /// buntu; selain itu kartu di-commit, lalu kartu berikutnya, jeda, atau
    /// gerbang setelah kartu terakhir.
    pub(super) async fn vm_group_after_turn(
        self: &Arc<Self>,
        mut run: RunRecord,
        result: String,
    ) -> Result<(), RunError> {
        run.state = RunState::AwaitingDev;
        run.hold = None;
        self.runs.save(&run, run.generation, self.now()).await?;
        // Commit lokal kartu sebelumnya membuat `unpushed` selalu > 0, jadi
        // yang dilihat hanya worktree yang berubah.
        let work = self.member_changes(&run).await?;
        if !work.iter().any(|(_, c)| c.dirty) {
            let question = Some(result.trim().to_owned()).filter(|q| !q.is_empty());
            return self.hold(&mut run, HoldReason::Stuck, question).await;
        }
        let files = work
            .iter()
            .flat_map(|(_, c)| c.files.iter().map(String::as_str));
        if touches_sensitive_path(files) {
            return self
                .hold(
                    &mut run,
                    HoldReason::NeedsConfirmation,
                    Some(SENSITIVE.to_owned()),
                )
                .await;
        }
        let mut cards = self.group_of(&run.id).await?;
        self.finish_active_card(&mut run, &mut cards).await?;
        if !cards.iter().any(|c| c.state == CardState::Pending) {
            self.save_group(&mut run, &cards).await?;
            return self.vm_publish(&run.id).await;
        }
        if let Some(reason) = self.group_pause(&run).await? {
            self.save_group(&mut run, &cards).await?;
            self.hold(&mut run, reason, None).await?;
            return self.requeue_group(&run, &cards).await;
        }
        self.advance(&mut run, &mut cards, None).await
    }

    /// Jeda sebelum kartu berikutnya (R20, R21, KTD-G12).
    async fn group_pause(&self, run: &RunRecord) -> Result<Option<HoldReason>, RunError> {
        let now = self.sync.clock.now_ms();
        let in_window = self.window().await?.is_some_and(|w| w.is_open(now));
        let fix_ready = self
            .runs
            .list_active()
            .await?
            .iter()
            .any(|r| r.id != run.id && fix_waiting(r, now));
        Ok(vm_pause(&PauseInput {
            in_window,
            start_now: run.vm_start.is_now(),
            fix_ready,
        }))
    }

    /// Grup yang berjeda kembali di posisi pertama antrean dengan kartu yang
    /// tersisa; tanda Start now ikut (KTD-G12).
    pub(crate) async fn requeue_group(
        &self,
        run: &RunRecord,
        cards: &[GroupCardRow],
    ) -> Result<(), RunError> {
        let item = QueueItem {
            record_id: run.record_id.clone(),
            target: target_of(run),
            note: None,
            start_now: run.vm_start.is_now(),
            added_at_ms: self.sync.clock.now_ms(),
            design_sources: run.design.iter().map(|d| d.id.clone()).collect(),
            planned: None,
            group: cards
                .iter()
                .filter(|c| !matches!(c.state, CardState::Removed(_)))
                .map(|c| c.record_id.clone())
                .collect(),
        };
        match self.overnight.enqueue_front(&item).await {
            Err(PortError::Conflict) => Ok(()),
            other => Ok(other?),
        }
    }
}
