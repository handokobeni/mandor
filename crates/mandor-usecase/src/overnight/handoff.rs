//! "Take over to laptop" di daemon VM (U5 plan VM, KTD-63): kerja malam
//! di-push ke branch kartu tanpa PR, klaim pindah ke daemon laptop, dan kartu
//! tidak bisa kembali ke antrean VM. Kartu Planned yang belum dimulai kembali
//! bersama plan-nya dan boleh diantre lagi (plan 2026-10-30-001, KTD-6).

use std::sync::Arc;

use mandor_domain::RunState;
use mandor_domain::planned::{PlannedState, PlannedWork};

use super::OvernightStore;
use crate::StartTarget;
use crate::ports::{Clock, Deps};
use crate::run::{RunError, RunService, target_of};
use crate::run_ports::{ChatItem, GitPort, RunStore};

/// Hasil serah-terima untuk dipulihkan laptop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handback {
    pub branch: String,
    /// Repo atau workspace kartu di VM.
    pub target: Option<StartTarget>,
    /// Plan kartu Planned yang belum dimulai.
    pub planned: Option<PlannedWork>,
    /// PR VM masih draft: laptop melanjutkan di PR itu dan Send to review
    /// menandainya siap.
    pub draft: bool,
}

impl<D: Deps> RunService<D> {
    /// Serah-terima kartu VM ke daemon laptop `to_daemon`. Mengembalikan branch
    /// kartu untuk dipulihkan laptop. Push gagal berarti klaim tidak pindah.
    ///
    /// # Errors
    /// Bukan daemon VM, kartu tanpa run atau tidak dipegang daemon ini, atau
    /// git/Lark gagal.
    pub async fn handoff(
        self: &Arc<Self>,
        record_id: &str,
        to_daemon: &str,
    ) -> Result<Handback, RunError> {
        if !self.is_vm() {
            return Err(RunError::InvalidState("handoff runs on the VM daemon"));
        }
        let Some(run) = self.runs.active_for(record_id).await? else {
            if let Some(back) = self.handed_back(record_id, to_daemon).await? {
                return Ok(back);
            }
            return self.hand_back_planned(record_id, to_daemon).await;
        };
        if matches!(run.state, RunState::Preparing | RunState::Publishing) {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let task = self.task_of(record_id).await?;
        if !task.held_here(self.sync.identity()) {
            return Err(RunError::InvalidState("the card is held by another daemon"));
        }
        // Agent yang masih bekerja dihentikan dulu; akhir gilirannya diabaikan.
        self.interrupt(&run.id).await?;
        let run = self.load(&run.id).await?;
        let branch = run
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        let key = task.key.as_deref().unwrap_or(record_id);
        for (m, changes) in self.member_changes(&run).await? {
            if changes.dirty {
                let message = format!("{key}: work in progress, handed off to the laptop");
                self.git.commit_all(&m.worktree, &message).await?;
            }
            if changes.dirty || changes.unpushed > 0 {
                self.git.push(&m.repo, &m.worktree, &branch).await?;
            }
        }
        self.sync.hand_over_claim(record_id, to_daemon).await?;
        let now = self.now();
        self.overnight.set_laptop_only(record_id, now).await?;
        self.overnight.dequeue(record_id, None, now).await?;
        self.runs
            .append(
                &run.id,
                &ChatItem::System(format!("Handed off to the laptop ({to_daemon})")),
                now,
            )
            .await?;
        self.close_run(&run.id).await?;
        Ok(Handback {
            branch,
            target: Some(target_of(&run)),
            planned: None,
            draft: run.draft_pending.is_some(),
        })
    }

    /// Serah-terima yang sudah selesai ke `to_daemon` diulang tanpa efek, agar
    /// Take over bisa dicoba lagi bila langkah laptop gagal.
    async fn handed_back(
        &self,
        record_id: &str,
        to_daemon: &str,
    ) -> Result<Option<Handback>, RunError> {
        if !self.overnight.laptop_only(record_id).await? {
            return Ok(None);
        }
        let task = self.task_of(record_id).await?;
        if task.claim.as_ref().and_then(|c| c.daemon_id()) != Some(to_daemon) {
            return Err(RunError::InvalidState(
                "the card was already handed to another daemon",
            ));
        }
        let Some(run) = self.runs.latest_for(record_id).await? else {
            return Ok(None);
        };
        Ok(run.branch.clone().map(|branch| Handback {
            branch,
            target: Some(target_of(&run)),
            planned: None,
            draft: run.draft_pending.is_some(),
        }))
    }

    /// Kartu Planned di antrean tanpa run: keluar dari antrean, klaim ke
    /// laptop, dan plan dikembalikan. Kartu tidak ditandai "hanya laptop".
    async fn hand_back_planned(
        &self,
        record_id: &str,
        to_daemon: &str,
    ) -> Result<Handback, RunError> {
        let item = self
            .overnight
            .queue()
            .await?
            .into_iter()
            .find(|i| i.record_id == record_id)
            .ok_or(RunError::NotFound("run"))?;
        let planned = item.planned.ok_or(RunError::NotFound("run"))?;
        match planned.state {
            PlannedState::Starting => {
                return Err(RunError::InvalidState("the VM is starting this card"));
            }
            PlannedState::HandoverPending => {
                return Err(RunError::InvalidState(
                    "the card was not handed over to the VM yet; remove it from the queue",
                ));
            }
            PlannedState::Waiting | PlannedState::Held => {}
        }
        let task = self.task_of(record_id).await?;
        if !task.held_here(self.sync.identity()) {
            return Err(RunError::InvalidState("the card is held by another daemon"));
        }
        self.sync.hand_over_claim(record_id, to_daemon).await?;
        self.overnight
            .dequeue(record_id, None, self.sync.clock.now_ms())
            .await?;
        Ok(Handback {
            branch: planned.work.branch.clone(),
            target: Some(item.target),
            planned: Some(planned.work),
            draft: false,
        })
    }
}
