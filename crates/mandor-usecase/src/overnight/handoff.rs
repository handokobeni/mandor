//! "Take over to laptop" di daemon VM (U5 plan VM, KTD-63): kerja malam
//! di-push ke branch kartu tanpa PR, klaim pindah ke daemon laptop, dan kartu
//! tidak bisa kembali ke antrean VM.

use std::sync::Arc;

use mandor_domain::RunState;

use super::OvernightStore;
use crate::ports::Deps;
use crate::run::{RunError, RunService};
use crate::run_ports::{ChatItem, GitPort, RunStore};

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
    ) -> Result<String, RunError> {
        if !self.is_vm() {
            return Err(RunError::InvalidState("handoff runs on the VM daemon"));
        }
        let run = self
            .runs
            .active_for(record_id)
            .await?
            .ok_or(RunError::NotFound("run"))?;
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
        Ok(branch)
    }
}
