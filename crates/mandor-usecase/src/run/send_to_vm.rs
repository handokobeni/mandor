//! "Approve and queue for VM" di daemon laptop (plan 2026-10-30-001, U3,
//! KTD-1, KTD-10): plan di-commit dan di-push, paket plan diserahkan ke app
//! untuk diantrekan di VM, lalu klaim pindah ke VM dan run laptop ditutup.

use std::sync::Arc;

use mandor_domain::planned::{DesignMark, PlannedWork, stray_files};
use mandor_domain::{Holder, RunState};

use super::session::{follow_design_rows, rejected};
use super::{RunError, RunService, StartTarget};
use crate::ports::Deps;
use crate::run_ports::{ChatItem, GitPort, LedgerStore, RunPhase, RunRecord, RunStore};

/// Hasil langkah pertama: paket plan dan target run laptop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmHandover {
    pub work: PlannedWork,
    pub target: StartTarget,
}

/// Target Start run: repo tunggal atau workspace dengan anggotanya.
pub(crate) fn target_of(run: &RunRecord) -> StartTarget {
    match &run.workspace {
        Some(ws) => StartTarget::Workspace {
            id: ws.id.clone(),
            repo_ids: std::iter::once(run.repo_id.clone())
                .chain(run.extra.iter().map(|m| m.repo_id.clone()))
                .collect(),
        },
        None => StartTarget::Repo(run.repo_id.clone()),
    }
}

impl<D: Deps> RunService<D> {
    /// Langkah (1): commit dan push plan, lalu kembalikan paket plan. Run tetap
    /// menunggu persetujuan, jadi kegagalan langkah berikutnya tidak menghapus
    /// apa pun di laptop (R4).
    ///
    /// # Errors
    /// Daemon VM, run bukan plan yang menunggu persetujuan, run grup, berkas
    /// baru di luar folder plan (KTD-10), atau git gagal.
    pub async fn prepare_for_vm(
        self: &Arc<Self>,
        run_id: &str,
        follow_design: &[String],
    ) -> Result<VmHandover, RunError> {
        let _work = self.enter_work()?;
        if self.is_vm() {
            return Err(RunError::InvalidState("queue for the VM from the laptop"));
        }
        let run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let Some(plan) = run.plan.clone().filter(|_| run.phase == RunPhase::Plan) else {
            return Err(RunError::InvalidState("no plan to approve"));
        };
        if !self.runs.cards(run_id).await?.is_empty() {
            return Err(RunError::InvalidState(
                "grouped runs cannot be queued for the VM yet",
            ));
        }
        let branch = run
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        let rows = follow_design_rows(self.reviews.ac_rows(run_id).await?, follow_design)?;
        let members = self.members(&run).await?;

        let mut stray = Vec::new();
        for m in &members {
            let files = stray_files(&self.git.untracked(&m.worktree).await?);
            if run.workspace.is_some() {
                stray.extend(files.into_iter().map(|f| format!("{}/{f}", m.repo.id)));
            } else {
                stray.extend(files);
            }
        }
        if !stray.is_empty() {
            return Err(rejected(format!(
                "Commit or remove these new files before queueing for the VM: {}",
                stray.join(", ")
            )));
        }

        let task = self.task_of(&run.record_id).await?;
        let target = target_of(&run);
        let message = format!(
            "{}: approved plan",
            task.key.as_deref().unwrap_or(&run.record_id)
        );
        // Semua anggota di-push agar branch tiap repo ada di remote untuk VM.
        for m in &members {
            self.git.commit_all(&m.worktree, &message).await?;
            self.git.push(&m.repo, &m.worktree, &branch).await?;
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;

        Ok(VmHandover {
            work: PlannedWork {
                plan,
                branch,
                design: rows
                    .iter()
                    .map(|r| DesignMark {
                        ac_id: r.ac_id.clone(),
                        design: r.design,
                        note: r.design_note.clone(),
                    })
                    .collect(),
                laptop_design: run.design.iter().map(|d| d.id.clone()).collect(),
                repos: target.repo_ids(),
            },
            target,
        })
    }

    /// Langkah (3): klaim pindah ke daemon VM `vm_daemon`, run laptop ditutup
    /// dan worktree-nya dibuang. Aman diulang bila klaim sudah pindah.
    ///
    /// # Errors
    /// Daemon VM, run bukan plan yang menunggu persetujuan, klaim dipegang
    /// pihak lain, atau Lark gagal.
    pub async fn send_to_vm(
        self: &Arc<Self>,
        run_id: &str,
        vm_daemon: &str,
    ) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        if self.is_vm() {
            return Err(RunError::InvalidState("queue for the VM from the laptop"));
        }
        let run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev || run.phase != RunPhase::Plan {
            return Err(RunError::InvalidState("no plan to approve"));
        }
        let fresh = self
            .sync
            .fetch_one(&run.record_id)
            .await?
            .ok_or(RunError::NotFound("task"))?;
        let identity = self.sync.identity();
        let at_vm = fresh.claim.as_ref().is_some_and(|c| {
            identity.holder(c) == Holder::OwnOtherDaemon && c.daemon_id() == Some(vm_daemon)
        });
        if !at_vm {
            self.sync.hand_over_claim(&run.record_id, vm_daemon).await?;
        }
        self.runs
            .append(
                run_id,
                &ChatItem::System(format!("Queued for the VM ({vm_daemon})")),
                self.now(),
            )
            .await?;
        self.close_run(run_id).await
    }
}
