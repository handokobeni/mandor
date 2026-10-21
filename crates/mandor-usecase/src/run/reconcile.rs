//! Rekonsiliasi saat daemon start (KTD-13).

use std::sync::Arc;

use mandor_domain::RunState;

use super::{RunError, RunService};
use crate::ports::Deps;
use crate::run_ports::{AgentPort, ChatItem, RunStore};

impl<D: Deps> RunService<D> {
    /// Run yang sedang berjalan saat daemon mati menjadi "gagal" yang bisa
    /// dilanjutkan; process group lamanya dihentikan. State Running tidak pernah
    /// diubah diam-diam menjadi menunggu. Mengembalikan ID run yang ditandai.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn reconcile(self: &Arc<Self>) -> Result<Vec<String>, RunError> {
        let mut marked = Vec::new();
        for mut run in self.runs.list_active().await? {
            let interrupted = matches!(
                run.state,
                RunState::Running
                    | RunState::WfRunning
                    | RunState::Preparing
                    | RunState::Publishing
            );
            if !interrupted {
                continue;
            }
            if let Some(pid) = run.pid.and_then(|p| u32::try_from(p).ok()) {
                self.agent.reap(pid).await;
            }
            let expected = run.generation;
            run.generation += 1;
            run.state = RunState::Failed;
            run.pid = None;
            run.last_error = Some(
                "the daemon restarted while this run was active; resume to continue the session"
                    .to_owned(),
            );
            self.runs.save(&run, expected, self.now()).await?;
            self.runs
                .append(
                    &run.id,
                    &ChatItem::System("Run interrupted because the daemon restarted".to_owned()),
                    self.now(),
                )
                .await?;
            marked.push(run.id);
        }
        Ok(marked)
    }

    /// "Lanjutkan" run gagal: giliran baru pada sesi yang sama.
    ///
    /// # Errors
    /// Run tidak dalam state gagal, atau agent gagal.
    pub async fn resume_failed(
        self: &Arc<Self>,
        run_id: &str,
        text: Option<&str>,
    ) -> Result<(), RunError> {
        let run = self.load(run_id).await?;
        if run.state != RunState::Failed {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let text = text.unwrap_or("Continue the previous work from where you left off.");
        self.send_message(run_id, text).await.map(|_| ())
    }
}
