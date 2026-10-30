//! Kepemilikan run: heartbeat klaim, ambil alih, dan pembersihan run yatim (KTD-8).

use std::sync::Arc;

use mandor_domain::{ClaimLoss, RunState};

use mandor_domain::planned::PlannedWork;

use super::{RunError, RunService, StartTarget};
use crate::notify::notice;
use crate::ports::Deps;
use crate::review::DesignSources as _;
use crate::run_ports::{ChatItem, GitPort, NoticeKind, Notifier, RunPhase, RunRecord, RunStore};
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
            let cards = self.runs.cards(&run.id).await?;
            if !cards.is_empty() {
                if let Some(id) = self.heartbeat_group(&mut run, cards).await? {
                    orphaned.push(id);
                }
                continue;
            }
            match self.sync.heartbeat(&run.record_id).await {
                Ok(Heartbeat::Lost(loss)) => {
                    self.stop_for_loss(&mut run, loss).await?;
                    orphaned.push(run.id);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(record_id = %run.record_id, error = %e, "heartbeat klaim gagal");
                }
            }
        }
        // Kartu Planned di antrean VM belum punya run (KTD-5).
        if self.is_vm() {
            self.tend_planned(true).await?;
        }
        Ok(orphaned)
    }

    /// Heartbeat setiap kartu terbuka run grup (KTD-G5). Kehilangan klaim
    /// kartu aktif menghentikan run seperti run tunggal; kartu selesai ditandai
    /// berubah di Lark (diputuskan di gerbang); kartu yang belum dimulai
    /// diperiksa ulang sebelum gilirannya (R9).
    async fn heartbeat_group(
        &self,
        run: &mut RunRecord,
        mut cards: Vec<crate::run_ports::GroupCardRow>,
    ) -> Result<Option<String>, RunError> {
        use mandor_domain::group::{CardState, LossAction, on_claim_loss};
        let mut marked = false;
        for card in &mut cards {
            if matches!(card.state, CardState::Removed(_)) {
                continue;
            }
            let loss = match self.sync.heartbeat(&card.record_id).await {
                Ok(Heartbeat::Lost(loss)) => loss,
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(record_id = %card.record_id, error = %e, "heartbeat klaim kartu grup gagal");
                    continue;
                }
            };
            match on_claim_loss(&card.state) {
                LossAction::StopRun => {
                    self.stop_for_loss(run, loss).await?;
                    return Ok(Some(run.id.clone()));
                }
                LossAction::MarkCard => {
                    if card.state == CardState::Done && !card.changed_in_lark {
                        card.changed_in_lark = true;
                        marked = true;
                    }
                }
            }
        }
        if marked {
            self.runs.save_cards(&run.id, &cards, None).await?;
        }
        Ok(None)
    }

    /// Melepas klaim kartu utama dan semua kartu grup yang masih terbuka.
    async fn release_run_claims(&self, run: &RunRecord) -> Result<(), RunError> {
        let mut claimed = vec![run.record_id.clone()];
        for card in self.runs.cards(&run.id).await? {
            if !matches!(card.state, mandor_domain::group::CardState::Removed(_))
                && !claimed.contains(&card.record_id)
            {
                claimed.push(card.record_id);
            }
        }
        for record_id in &claimed {
            if let Err(e) = self.sync.release_claim(record_id).await {
                tracing::warn!(record_id = %record_id, error = %e, "gagal melepas klaim saat menutup run");
            }
        }
        Ok(())
    }

    /// Menghentikan run karena klaim hilang dan memberi tahu dev.
    async fn stop_for_loss(&self, run: &mut RunRecord, loss: ClaimLoss) -> Result<(), RunError> {
        if let Some(stopper) = self.take_stopper(&run.id) {
            crate::run_ports::Stopper::stop(&stopper).await;
        }
        let expected = run.generation;
        run.generation += 1;
        run.state = loss.run_state();
        run.pid = None;
        self.runs.save(run, expected, self.now()).await?;
        self.notifier
            .notify(notice(self, NoticeKind::NeedsAttention, run, loss.describe()).await)
            .await;
        Ok(())
    }

    /// "Ambil alih" task yang klaimnya tidak aktif 30 menit atau diserahkan
    /// VM: klaim dengan baca ulang, lalu run baru dari branch task. Run
    /// workspace memulihkan folder run dan worktree tiap anggota. Dengan paket
    /// plan (KTD-12), run kembali menunggu persetujuan plan.
    ///
    /// # Errors
    /// Klaim masih aktif, kalah klaim, target tidak valid, atau git gagal.
    pub async fn takeover(
        self: &Arc<Self>,
        record_id: &str,
        target: &StartTarget,
        planned: Option<&PlannedWork>,
    ) -> Result<RunRecord, RunError> {
        let _work = self.enter_work()?;
        self.sync.claim(record_id, true).await?;
        let task = self.task_of(record_id).await?;
        let branch = planned
            .map(|p| p.branch.clone())
            .or_else(|| task.branch.clone())
            .ok_or(RunError::InvalidState("no branch"))?;
        let (mut run, _) = self.restore_run(record_id, target, &branch).await?;
        run.pr_url.clone_from(&task.pr_url);
        let mut missing = Vec::new();
        if let Some(p) = planned {
            let check = self.design.prepare(&p.laptop_design, false).await?;
            missing = check
                .unavailable
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            run.phase = RunPhase::Plan;
            run.plan = Some(p.plan.clone());
            run.design = check.roots;
        }
        let now = self.now();
        self.runs.create(&run, now).await?;
        if let Some(p) = planned {
            self.seed_planned_ledger(&run.id, &task, &p.design).await?;
            if !missing.is_empty() {
                let note = format!(
                    "Design sources no longer on this laptop: {}",
                    missing.join(", ")
                );
                self.runs
                    .append(&run.id, &ChatItem::System(note), now)
                    .await?;
            }
        }
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
        self.release_run_claims(&run).await?;
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
