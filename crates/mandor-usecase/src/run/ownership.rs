//! Kepemilikan run: heartbeat klaim, ambil alih, dan pembersihan run yatim (KTD-8).

use std::sync::Arc;

use mandor_domain::RunState;

use super::{RunError, RunService};
use crate::notify::notice;
use crate::ports::{Deps, Ids};
use crate::run_ports::{GitPort, NoticeKind, Notifier, Repos, RunRecord, RunStore};
use crate::sync::Heartbeat;

impl<D: Deps> RunService<D> {
    /// Heartbeat klaim untuk semua run aktif. Run yang klaimnya hilang
    /// dihentikan: diambil alih menjadi Orphaned, record dihapus atau dialihkan
    /// menjadi Stopped.
    ///
    /// # Errors
    /// Error penyimpanan; kegagalan Lark per record hanya dicatat.
    pub async fn heartbeat_all(self: &Arc<Self>) -> Result<Vec<String>, RunError> {
        let mut orphaned = Vec::new();
        for mut run in self.runs.list_active().await? {
            if matches!(run.state, RunState::Orphaned | RunState::Stopped) {
                continue;
            }
            match self.sync.heartbeat(&run.record_id).await {
                Ok(Heartbeat::Lost(loss)) => {
                    if let Some(stopper) = self.take_stopper(&run.id) {
                        crate::run_ports::Stopper::stop(&stopper).await;
                    }
                    let expected = run.generation;
                    run.generation += 1;
                    run.state = loss.run_state();
                    run.pid = None;
                    self.runs.save(&run, expected, self.now()).await?;
                    self.notifier
                        .notify(
                            notice(self, NoticeKind::NeedsAttention, &run, loss.describe()).await,
                        )
                        .await;
                    orphaned.push(run.id);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(record_id = %run.record_id, error = %e, "heartbeat klaim gagal");
                }
            }
        }
        Ok(orphaned)
    }

    /// "Ambil alih" task yang klaimnya tidak aktif 30 menit: klaim dengan baca
    /// ulang, lalu run baru dari branch task (worktree dipulihkan).
    ///
    /// # Errors
    /// Klaim masih aktif, kalah klaim, atau git gagal.
    pub async fn takeover(
        self: &Arc<Self>,
        record_id: &str,
        repo_id: &str,
    ) -> Result<RunRecord, RunError> {
        let _work = self.enter_work()?;
        self.sync.claim(record_id, true).await?;
        let task = self.task_of(record_id).await?;
        let repo = self
            .repos
            .repo(repo_id)
            .await?
            .ok_or(RunError::NotFound("repo"))?;
        let branch = task
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        let worktree = self.git.restore_worktree(&repo, &branch, None).await?;
        let mut run = RunRecord::new(self.sync.ids.nonce(), repo.id.clone(), record_id.to_owned());
        run.state = RunState::AwaitingDev;
        run.branch = Some(branch);
        run.worktree = Some(worktree);
        run.session_id = Some(self.sync.ids.nonce());
        run.pr_url.clone_from(&task.pr_url);
        self.runs.create(&run, self.now()).await?;
        Ok(run)
    }

    /// "Discard changes" satu berkas: hanya berkas yang memang berubah, dan
    /// hanya saat agent tidak sedang bekerja di worktree.
    ///
    /// # Errors
    /// Run atau berkas tidak ada, state tidak mengizinkan, atau git gagal.
    pub async fn discard_file(self: &Arc<Self>, run_id: &str, path: &str) -> Result<(), RunError> {
        let run = self.load(run_id).await?;
        if run.state.is_working() {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let Some(branch) = &run.branch else {
            return Err(RunError::NotFound("worktree"));
        };
        if run.worktree.is_none() {
            return Err(RunError::NotFound("worktree"));
        }
        let members = self.members(&run).await?;
        // Run workspace menampilkan berkas dengan awalan `<repo>/`.
        let target = if run.workspace.is_some() {
            path.split_once('/').and_then(|(repo, rest)| {
                members
                    .iter()
                    .find(|m| m.repo.id == repo)
                    .map(|m| (m, rest))
            })
        } else {
            members.first().map(|m| (m, path))
        };
        let Some((member, file)) = target else {
            return Err(RunError::NotFound("changed file"));
        };
        let changes = self
            .git
            .changes(&member.repo, &member.worktree, branch)
            .await?;
        if !changes.files.iter().any(|f| f == file) {
            return Err(RunError::NotFound("changed file"));
        }
        self.git
            .discard_file(&member.repo, &member.worktree, file)
            .await?;
        Ok(())
    }

    /// Menutup run (mis. Orphaned atau task selesai): hentikan agent, lepas
    /// klaim bila masih milik kita, hapus worktree bila tidak ada perubahan
    /// yang belum di-push. Folder run workspace dihapus bila semua worktree-nya
    /// terhapus.
    ///
    /// # Errors
    /// Run tidak ada atau penyimpanan gagal.
    pub async fn close_run(self: &Arc<Self>, run_id: &str) -> Result<(), RunError> {
        let run = self.load(run_id).await?;
        if let Some(stopper) = self.take_stopper(run_id) {
            crate::run_ports::Stopper::stop(&stopper).await;
        }
        if let Err(e) = self.sync.release_claim(&run.record_id).await {
            tracing::warn!(record_id = %run.record_id, error = %e, "gagal melepas klaim saat menutup run");
        }
        if let (Some(_), Some(branch)) = (&run.worktree, &run.branch) {
            let mut removable = Vec::new();
            let mut kept = false;
            for m in self.members(&run).await? {
                let clean = self
                    .git
                    .changes(&m.repo, &m.worktree, branch)
                    .await
                    .is_ok_and(|c| c.is_empty());
                if !clean {
                    kept = true;
                } else if self.git.worktree_exists(&m.worktree).await {
                    removable.push((m.repo, m.worktree));
                }
            }
            let root = run
                .workspace
                .as_ref()
                .map(|w| w.root.as_path())
                .filter(|_| !kept);
            self.discard_members(&removable, branch, root).await;
        }
        self.runs.close(run_id, self.now()).await?;
        Ok(())
    }
}
