//! "Push & buat PR" (R8, R10, R15) dan pemeriksaan PR berkala (KTD-11).

use std::sync::Arc;

use mandor_domain::push_gate::{apply_decision, evaluate};
use mandor_domain::{AppField, GateDecision, GateMode, GateOutcome, RunState, WriteOp};

use super::group_gate::labelled_rows;
use super::{RunError, RunService};
use crate::notify::notice;
use crate::ports::Deps;
use crate::review::settings::{resolve_gate_mode, utc_minutes};
use crate::run_ports::{
    GitPort, LedgerStore, NoticeKind, Notifier, Repos, ReviewTrigger, RunPhase, RunRecord, RunStore,
};

/// Hasil Push & PR setelah gerbang AC (KTD-35).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishOutcome {
    Published {
        url: String,
        /// Label AC Fail/Can't verify yang dibiarkan (mode peringatan).
        warnings: Vec<String>,
        /// AC tidak bisa disegarkan dari tracker; dipakai salinan terakhir.
        sync_warning: Option<String>,
    },
    /// Belum ada review yang berlaku: review dimulai, push tidak berlanjut.
    ReviewStarted { review_id: String },
    /// Mode blokir: AC gagal tanpa alasan Push anyway.
    Blocked { failed: Vec<String> },
    /// Mode blokir: AC Can't verify belum dikonfirmasi.
    NeedsConfirmation { unverified: Vec<String> },
    /// Run grup: kartu yang berubah di Lark menunggu keputusan revert atau
    /// tetap (R13). Berisi kunci kartu.
    CardsChanged { cards: Vec<String> },
}

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
        match self.publish_gated(run_id, &GateDecision::default()).await? {
            PublishOutcome::Published { url, .. } => Ok(url),
            PublishOutcome::ReviewStarted { .. } => Err(RunError::InvalidState(
                "the acceptance criteria review started; push after it finishes",
            )),
            PublishOutcome::Blocked { .. } | PublishOutcome::NeedsConfirmation { .. } => Err(
                RunError::InvalidState("the acceptance criteria gate holds the push"),
            ),
            PublishOutcome::CardsChanged { .. } => Err(RunError::InvalidState(
                "a card changed in Lark; decide to revert or keep it",
            )),
        }
    }

    /// Push & PR dengan gerbang AC: refresh AC, tanda usang, evaluasi gerbang
    /// dengan keputusan dev, lalu push hanya bila Ready. Keputusan dev disimpan
    /// di buku AC sebelum push agar Retry memakai keputusan yang sama.
    ///
    /// # Errors
    /// Run tidak bisa di-push, review gagal dimulai, atau git/gh gagal.
    pub async fn publish_gated(
        self: &Arc<Self>,
        run_id: &str,
        decision: &GateDecision,
    ) -> Result<PublishOutcome, RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        if run.phase != RunPhase::Implement {
            return Err(RunError::InvalidState("plan not approved"));
        }
        if run.state.is_busy() || run.state.is_parked() || run.state == RunState::Preparing {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let group = self.group_gate(&mut run).await?;
        if let Some((holds, _)) = &group
            && !holds.is_empty()
        {
            return Ok(PublishOutcome::CardsChanged {
                cards: holds.clone(),
            });
        }
        let refresh = self.refresh_ledger(run_id).await?;
        let sync_warning = refresh.error.as_ref().map(|_| {
            let at = refresh
                .synced_at_ms
                .map_or_else(|| "never".to_owned(), utc_minutes);
            format!("Acceptance criteria could not be refreshed from Lark; using the copy synced at {at}")
        });
        self.mark_stale_if_changed(&run).await;
        let mut rows = self.reviews.ac_rows(run_id).await?;
        let mode = self.gate_mode(&run).await?;
        let shown = match &group {
            Some((_, keys)) => labelled_rows(&rows, keys),
            None => rows.clone(),
        };
        let warnings = match evaluate(&shown, mode, decision) {
            GateOutcome::NeedsReview => {
                let rec = self
                    .start_review(run_id, ReviewTrigger::PushAndPr, None)
                    .await?;
                return Ok(PublishOutcome::ReviewStarted { review_id: rec.id });
            }
            GateOutcome::Blocked { failed } => return Ok(PublishOutcome::Blocked { failed }),
            GateOutcome::NeedsConfirmation { unverified } => {
                return Ok(PublishOutcome::NeedsConfirmation { unverified });
            }
            GateOutcome::Ready { warnings } => warnings,
        };
        let before = rows.clone();
        apply_decision(&mut rows, decision);
        if rows != before {
            self.reviews.save_ac_rows(run_id, &rows).await?;
        }
        let url = self.push_and_mark(&mut run).await?;
        Ok(PublishOutcome::Published {
            url,
            warnings,
            sync_warning,
        })
    }

    /// Mode gerbang run untuk tampilan.
    ///
    /// # Errors
    /// Run tidak ada atau repo/workspace tidak terbaca.
    pub async fn gate_mode_for(&self, run_id: &str) -> Result<GateMode, RunError> {
        let run = self.load(run_id).await?;
        self.gate_mode(&run).await
    }

    /// Mode gerbang run (KTD-37). Di daemon VM selalu Block (KTD-61 plan VM).
    pub(crate) async fn gate_mode(&self, run: &RunRecord) -> Result<GateMode, RunError> {
        if self.is_vm() {
            return Ok(GateMode::Block);
        }
        let overrides: Vec<_> = self
            .members(run)
            .await?
            .iter()
            .map(|m| m.repo.gate_mode)
            .collect();
        let workspace = match &run.workspace {
            Some(ws) => self
                .repos
                .workspace(&ws.id)
                .await?
                .and_then(|w| w.gate_mode),
            None => None,
        };
        Ok(resolve_gate_mode(&overrides, workspace))
    }

    /// Publishing → commit, push, PR → Waiting Review; gagal = Failed.
    async fn push_and_mark(self: &Arc<Self>, run: &mut RunRecord) -> Result<String, RunError> {
        let previous = run.state;
        run.state = RunState::Publishing;
        self.runs.save(run, run.generation, self.now()).await?;
        match self.publish_inner(run).await {
            Ok(url) => Ok(url),
            Err(e) => {
                run.state = if matches!(e, RunError::NoChanges) {
                    previous
                } else {
                    RunState::Failed
                };
                run.last_error = Some(e.to_string());
                self.runs.save(run, run.generation, self.now()).await?;
                Err(e)
            }
        }
    }

    async fn publish_inner(self: &Arc<Self>, run: &mut RunRecord) -> Result<String, RunError> {
        let work = self.member_changes(run).await?;
        let url = self.publish_members(run, &work).await?;
        if run.draft_pending.is_some() {
            self.mark_draft(run, &url).await?;
        } else {
            self.mark_waiting_review(run, &url, !run.automatic).await?;
        }
        Ok(url)
    }

    /// PR draft VM: link PR ke Lark tanpa transisi; kartu tetap In Progress
    /// sampai dev menekan Send to review (R11, KTD-68 plan VM).
    async fn mark_draft(&self, run: &mut RunRecord, url: &str) -> Result<(), RunError> {
        let op = WriteOp::Fields(vec![AppField::PrUrl(url.to_owned())]);
        self.sync.request_write(&run.record_id, &op).await?;
        if run.workspace.is_none() {
            run.pr_url = Some(url.to_owned());
        }
        run.state = RunState::AwaitingDev;
        run.hold = None;
        run.last_error = None;
        run.note =
            Some("Draft PR ready. Send it to review when the change looks right.".to_owned());
        self.runs.save(run, run.generation, self.now()).await?;
        self.notifier
            .notify(notice(self, NoticeKind::PrCreated, run, url).await)
            .await;
        Ok(())
    }

    /// "Send to review" (R23): PR draft ditandai siap lalu kartu pindah ke
    /// Waiting Review. Gagal menandai PR berarti tidak ada transisi Lark.
    ///
    /// # Errors
    /// Run tanpa PR draft, kartu tidak dipegang daemon ini, agent bekerja, atau
    /// `gh pr ready` gagal.
    pub async fn send_to_review(&self, run_id: &str) -> Result<(), RunError> {
        let mut run = self.load(run_id).await?;
        if run.draft_pending.is_none() {
            return Err(RunError::InvalidState("no draft pull request to send"));
        }
        if run.state.is_working() {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let task = self.task_of(&run.record_id).await?;
        if !task.held_here(self.sync.identity()) {
            return Err(RunError::InvalidState("the card is held by another daemon"));
        }
        let mut urls = Vec::new();
        for m in self.members(&run).await? {
            if let Some(url) = m.pr_url {
                self.git.mark_pr_ready(&m.repo, &url).await?;
                urls.push(url);
            }
        }
        if urls.is_empty() {
            return Err(RunError::InvalidState("no draft pull request to send"));
        }
        run.draft_pending = None;
        self.mark_waiting_review(&mut run, &urls.join("\n"), true)
            .await
    }

    /// Link PR + Waiting Review. `cursor_now_too` menulis kursor "sejak sekarang"
    /// (publish awal); perbaikan memakai kursor balasan terakhir yang terkirim.
    pub(crate) async fn mark_waiting_review(
        &self,
        run: &mut RunRecord,
        url: &str,
        cursor_now_too: bool,
    ) -> Result<(), RunError> {
        if self.runs.cards(&run.id).await?.is_empty() {
            self.mark_card(&run.record_id, url, cursor_now_too).await?;
        } else {
            self.mark_group_cards(&run.id, url, cursor_now_too).await?;
        }
        // Run workspace menyimpan link per anggota (lihat `publish_members`).
        if run.workspace.is_none() {
            run.pr_url = Some(url.to_owned());
        }
        run.state = RunState::InReviewLoop;
        run.last_error = None;
        // Catatan seperti "Push held" sudah selesai begitu PR terbuat.
        run.note = None;
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
