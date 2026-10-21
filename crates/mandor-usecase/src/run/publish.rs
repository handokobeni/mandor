//! "Push & buat PR" (R8, R10, R15) dan pemeriksaan PR berkala (KTD-11).

use std::sync::Arc;

use mandor_domain::{AppField, RunState, TaskStatus, WriteOp};

use super::{RunError, RunService};
use crate::notify::notice;
use crate::ports::{Clock, Deps};
use crate::run_ports::{GitPort, NoticeKind, Notifier, RunPhase, RunRecord, RunStore};

/// Kursor komentar yang menandai "sejak sekarang" (detik epoch, tanpa reply).
pub(crate) fn cursor_now(now_ms: i64) -> String {
    format!("{}:", now_ms / 1000)
}

impl<D: Deps> RunService<D> {
    /// Commit (pesan dari haiku), push ke branch task, buat atau pakai PR, lalu
    /// tulis link PR dan Waiting Review lewat outbox.
    ///
    /// # Errors
    /// Tidak ada perubahan, atau git/gh/agent gagal (run ditandai gagal).
    pub async fn publish(self: &Arc<Self>, run_id: &str) -> Result<String, RunError> {
        let mut run = self.load(run_id).await?;
        if run.phase != RunPhase::Implement {
            return Err(RunError::InvalidState("plan not approved"));
        }
        if run.state.is_busy() || run.state.is_parked() || run.state == RunState::Preparing {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let previous = run.state;
        run.state = RunState::Publishing;
        self.runs.save(&run, run.generation, self.now()).await?;
        match self.publish_inner(&mut run).await {
            Ok(url) => Ok(url),
            Err(e) => {
                run.state = if matches!(e, RunError::NoChanges) {
                    previous
                } else {
                    RunState::Failed
                };
                run.last_error = Some(e.to_string());
                self.runs.save(&run, run.generation, self.now()).await?;
                Err(e)
            }
        }
    }

    async fn publish_inner(self: &Arc<Self>, run: &mut RunRecord) -> Result<String, RunError> {
        let work = self.member_changes(run).await?;
        let url = self.publish_members(run, &work).await?;
        self.mark_waiting_review(run, &url, !run.automatic).await?;
        Ok(url)
    }

    /// Link PR + Waiting Review. `cursor_now_too` menulis kursor "sejak sekarang"
    /// (publish awal); perbaikan memakai kursor balasan terakhir yang terkirim.
    pub(crate) async fn mark_waiting_review(
        &self,
        run: &mut RunRecord,
        url: &str,
        cursor_now_too: bool,
    ) -> Result<(), RunError> {
        let task = self.task_of(&run.record_id).await?;
        let mut fields = vec![AppField::PrUrl(url.to_owned())];
        if task.status == TaskStatus::WaitingReview {
            if task.pr_url.as_deref() != Some(url) {
                self.sync
                    .request_write(&run.record_id, &WriteOp::Fields(fields))
                    .await?;
            }
        } else {
            // Bila status sudah diubah manusia, revalidasi outbox membatalkan
            // tulisan ini dan menandai kartu (KTD-7).
            if cursor_now_too {
                fields.push(AppField::CommentCursor(cursor_now(
                    self.sync.clock.now_ms(),
                )));
            }
            let op = WriteOp::Transition {
                from: TaskStatus::InProgress,
                to: TaskStatus::WaitingReview,
                fields,
            };
            self.sync.request_write(&run.record_id, &op).await?;
        }
        // Run workspace menyimpan link per anggota (lihat `publish_members`).
        if run.workspace.is_none() {
            run.pr_url = Some(url.to_owned());
        }
        run.state = RunState::InReviewLoop;
        run.last_error = None;
        self.runs.save(run, run.generation, self.now()).await?;
        self.notifier
            .notify(notice(self, NoticeKind::PrCreated, run, url).await)
            .await;
        Ok(())
    }

    /// PR yang dibuat di luar app untuk branch task memicu Waiting Review. Hanya
    /// untuk run tanpa link PR yang tidak sedang dalam loop Waiting Fix.
    ///
    /// # Errors
    /// Error penyimpanan; kegagalan per run hanya dicatat.
    pub async fn check_prs(self: &Arc<Self>) -> Result<Vec<String>, RunError> {
        let mut found = Vec::new();
        for mut run in self.runs.list_active().await? {
            match self.check_pr_for(&mut run).await {
                Ok(true) => found.push(run.record_id.clone()),
                Ok(false) => {}
                Err(e) => tracing::warn!(run_id = %run.id, error = %e, "pemeriksaan PR gagal"),
            }
        }
        Ok(found)
    }

    async fn check_pr_for(&self, run: &mut RunRecord) -> Result<bool, RunError> {
        // Run workspace hanya Waiting Review lewat publish, setelah semua PR dibuat.
        let eligible = run.pr_url.is_none()
            && run.workspace.is_none()
            && !run.state.in_fix_loop()
            && !run.state.is_parked()
            && !matches!(run.state, RunState::Publishing | RunState::Preparing);
        let Some(branch) = run.branch.clone().filter(|_| eligible) else {
            return Ok(false);
        };
        let repo = self.repo_of(run).await?;
        let Some(pr) = self.git.find_pr(&repo, &branch).await? else {
            return Ok(false);
        };
        self.mark_waiting_review(run, &pr.url, true).await?;
        Ok(true)
    }
}
