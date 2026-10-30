//! Item Planned di antrean VM (plan 2026-10-30-001, U4): antre dengan paket
//! plan, serah-terima tertunda, dan heartbeat klaim selama menunggu.

use std::sync::Arc;

use mandor_domain::gate::check_start;
use mandor_domain::planned::{ClaimAction, PlannedState, PlannedWork, planned_claim_action};
use mandor_domain::{ClaimLoss, DaemonRole, TaskStatus};

use super::{OvernightStore, PlannedEntry, QueueItem, resumable};
use crate::StartTarget;
use crate::ports::{Clock, Deps, PortError};
use crate::review::DesignSources as _;
use crate::run::{RunError, RunService};
use crate::run_ports::RunStore;
use crate::sync::Heartbeat;

/// Permintaan mengantre satu kartu ke VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRequest {
    /// Repo atau workspace di VM.
    pub target: StartTarget,
    /// Catatan atau jawaban dev untuk agent.
    pub note: Option<String>,
    /// Sumber desain VM pilihan dev.
    pub design_sources: Vec<String>,
    /// Plan yang disetujui di laptop dan `daemon_id` laptop asal.
    pub planned: Option<(PlannedWork, String)>,
}

/// Alasan laporan pagi untuk klaim kartu Planned yang hilang.
fn lost_reason(loss: ClaimLoss) -> &'static str {
    match loss {
        ClaimLoss::Deleted => "the card was deleted in Lark",
        ClaimLoss::Unassigned => "the card is no longer assigned to you",
        ClaimLoss::TakenByOther => "the claim was taken by another Mandor daemon",
    }
}

fn rejected(message: impl Into<String>) -> RunError {
    RunError::Port(PortError::Rejected(message.into()))
}

impl<D: Deps> RunService<D> {
    /// Mengantrekan kartu: To Do atau tertahan (No plan), atau kartu Planned
    /// yang sedang diserahkan laptop (KTD-11).
    ///
    /// # Errors
    /// Bukan daemon VM, kartu "hanya laptop", sudah diantre, punya run lain,
    /// atau paket plan tidak valid.
    pub async fn enqueue_card(&self, record_id: &str, req: &QueueRequest) -> Result<(), RunError> {
        if self.sync.role() != DaemonRole::Vm {
            return Err(RunError::InvalidState(
                "the overnight queue runs on the VM daemon",
            ));
        }
        if self.overnight.laptop_only(record_id).await? {
            return Err(RunError::InvalidState(
                "this card was taken over to the laptop",
            ));
        }
        let active = self.runs.active_for(record_id).await?;
        if !req.design_sources.is_empty() {
            let check = self.design.prepare(&req.design_sources, false).await?;
            if let Some(refusal) = check.refusal() {
                return Err(rejected(refusal));
            }
        }
        let planned = if let Some((work, origin)) = &req.planned {
            if active.is_some() {
                return Err(RunError::InvalidState("the card already has a run"));
            }
            self.check_planned(record_id, &req.target, work, &req.design_sources)
                .await?;
            Some(PlannedEntry {
                work: work.clone(),
                state: PlannedState::HandoverPending,
                origin_daemon: Some(origin.clone()),
                reason: None,
            })
        } else {
            self.check_no_plan(record_id, active.map(|r| r.state))
                .await?;
            None
        };
        let item = QueueItem {
            record_id: record_id.to_owned(),
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
            planned,
        };
        match self.overnight.enqueue(&item).await {
            Err(PortError::Conflict) => Err(RunError::InvalidState("the card is already queued")),
            other => Ok(other?),
        }
    }

    /// Kartu No plan: To Do yang lolos gerbang Start, atau run yang boleh
    /// dilanjutkan.
    async fn check_no_plan(
        &self,
        record_id: &str,
        active: Option<mandor_domain::RunState>,
    ) -> Result<(), RunError> {
        match active {
            Some(state) if !resumable(state) => {
                Err(RunError::InvalidState("the card already has a run"))
            }
            Some(_) => Ok(()),
            None => {
                let task = self.task_of(record_id).await?;
                if !task.assigned_to(self.sync.me()) {
                    return Err(RunError::InvalidState("the card is not assigned to you"));
                }
                Ok(check_start(&task.status, task.readiness.as_deref())?)
            }
        }
    }

    /// Paket plan cocok dengan kartu dan VM (KTD-11, R11).
    async fn check_planned(
        &self,
        record_id: &str,
        target: &StartTarget,
        work: &PlannedWork,
        design_sources: &[String],
    ) -> Result<(), RunError> {
        let task = self.task_of(record_id).await?;
        if !task.assigned_to(self.sync.me()) {
            return Err(RunError::InvalidState("the card is not assigned to you"));
        }
        if task.status != TaskStatus::InProgress {
            return Err(rejected(format!(
                "a planned card must be In Progress, not {}",
                task.status.as_str()
            )));
        }
        if task.branch.as_deref() != Some(work.branch.as_str()) {
            return Err(rejected(format!(
                "the plan's branch {} is not this card's branch",
                work.branch
            )));
        }
        if work.follows_design() && design_sources.is_empty() {
            return Err(rejected(
                "pick a design source on the VM: the plan follows the design for some acceptance criteria",
            ));
        }
        self.check_target(target).await
    }

    /// Satu langkah perawatan item Planned dari baca segar kartu (KTD-5).
    /// `renew`: klaim milik VM diperbarui (jadwal heartbeat); tanpa itu hanya
    /// serah-terima tertunda yang diperiksa (tick).
    pub(crate) async fn tend_planned(self: &Arc<Self>, renew: bool) -> Result<(), RunError> {
        let now = self.sync.clock.now_ms();
        for item in self.overnight.queue().await? {
            let Some(p) = &item.planned else { continue };
            if !renew && p.state != PlannedState::HandoverPending {
                continue;
            }
            let id = item.record_id.as_str();
            let fresh = match self.sync.fetch_one(id).await {
                Ok(Some(t)) => t,
                Ok(None) => {
                    self.overnight
                        .dequeue(id, Some("the card was deleted in Lark"), now)
                        .await?;
                    continue;
                }
                Err(e) => {
                    tracing::warn!(record_id = id, error = %e, "kartu Planned gagal dibaca");
                    continue;
                }
            };
            let action = planned_claim_action(
                p.state,
                p.origin_daemon.as_deref(),
                item.added_at_ms,
                &fresh,
                self.sync.identity(),
                now,
            );
            self.apply_claim_action(id, p, action, renew, now).await?;
        }
        Ok(())
    }

    async fn apply_claim_action(
        &self,
        id: &str,
        p: &PlannedEntry,
        action: ClaimAction,
        renew: bool,
        now: i64,
    ) -> Result<(), RunError> {
        match action {
            ClaimAction::Renew if renew => self.renew_planned(id, now).await?,
            ClaimAction::Wait | ClaimAction::Leave | ClaimAction::Renew => {}
            ClaimAction::Promote => {
                self.overnight
                    .set_planned_state(id, PlannedState::Waiting, None)
                    .await?;
                if renew {
                    self.renew_planned(id, now).await?;
                }
            }
            ClaimAction::Hold(reason) => {
                if p.state != PlannedState::Held || p.reason.as_deref() != Some(&reason) {
                    self.overnight
                        .set_planned_state(id, PlannedState::Held, Some(&reason))
                        .await?;
                }
            }
            ClaimAction::Drop(reason) => {
                tracing::info!(
                    record_id = id,
                    reason,
                    "kartu Planned dikeluarkan dari antrean VM"
                );
                self.overnight.dequeue(id, Some(&reason), now).await?;
            }
        }
        Ok(())
    }

    /// Heartbeat klaim item Planned; klaim yang hilang mengeluarkan item.
    async fn renew_planned(&self, record_id: &str, now: i64) -> Result<(), RunError> {
        match self.sync.heartbeat(record_id).await {
            Ok(Heartbeat::Lost(loss)) => {
                self.overnight
                    .dequeue(record_id, Some(lost_reason(loss)), now)
                    .await?;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(record_id, error = %e, "heartbeat klaim kartu Planned gagal");
            }
        }
        Ok(())
    }
}
