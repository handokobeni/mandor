//! Akhir giliran kartu di daemon VM (U4 plan VM, KTD-60, KTD-61): kartu
//! dikerjakan sampai PR, dan setiap hasil yang menunggu developer menjadi
//! `Held` dengan alasan sehingga penjadwal lanjut ke kartu berikutnya.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use mandor_domain::run::touches_sensitive_path;
use mandor_domain::{
    AgentEvent, DaemonRole, GateDecision, HoldReason, RunState, Usage, UsageTracker,
};

use super::{CycleUsage, OvernightStore};
use crate::notify::notice;
use crate::ports::{Clock, Deps};
use crate::run::{PublishOutcome, RunError, RunService};
use crate::run_ports::{
    AgentPort as _, Hold, NoticeKind, Notifier, ReviewHistory as _, ReviewStatus, RunRecord,
    RunStore,
};

/// Catatan untuk perubahan yang butuh konfirmasi dev.
const SENSITIVE: &str =
    "Changes touch CI, agent config, or git hooks; push needs developer confirmation";

/// Catatan untuk review yang basi karena kode berubah selama review.
const STALE_REVIEW: &str = "Push held: the code changed during the review, so its results are out of date; review again or decide the AC";

type Boxed<'a> = Pin<Box<dyn Future<Output = Result<(), RunError>> + Send + 'a>>;

impl<D: Deps> RunService<D> {
    /// Daemon ini berperan VM.
    pub(crate) fn is_vm(&self) -> bool {
        self.sync.role() == DaemonRole::Vm
    }

    /// Menahan run dengan alasan (KTD-60) dan memberi tahu dev.
    pub(crate) async fn hold(
        &self,
        run: &mut RunRecord,
        reason: HoldReason,
        note: Option<String>,
    ) -> Result<(), RunError> {
        run.state = RunState::Held;
        run.hold = Some(Hold {
            reason,
            note: note.clone(),
        });
        self.runs.save(run, run.generation, self.now()).await?;
        if !reason.needs_attention() {
            return Ok(());
        }
        let body = note.unwrap_or_else(|| format!("Parked: {}", reason.as_str()));
        self.notifier
            .notify(notice(self, NoticeKind::NeedsAttention, run, &body).await)
            .await;
        Ok(())
    }

    /// Giliran implementasi kartu VM selesai: tanpa perubahan berarti agent
    /// buntu (R15); perubahan sensitif menunggu konfirmasi (R12); selain itu
    /// Push & PR lewat gerbang AC yang selalu memblokir (R11, R12).
    pub(crate) fn vm_after_turn(self: &Arc<Self>, mut run: RunRecord, result: String) -> Boxed<'_> {
        Box::pin(async move {
            if !self.runs.cards(&run.id).await?.is_empty() {
                return self.vm_group_after_turn(run, result).await;
            }
            run.state = RunState::AwaitingDev;
            run.hold = None;
            self.runs.save(&run, run.generation, self.now()).await?;
            let work = self.member_changes(&run).await?;
            if work.iter().all(|(_, c)| c.is_empty()) {
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
            self.vm_publish(&run.id).await
        })
    }

    /// Push & PR otomatis di VM; gerbang yang tidak siap menahan kartu
    /// `ac_gate` tanpa push (R12). Review yang dimulai berlanjut di sini lagi
    /// setelah selesai.
    pub(crate) fn vm_publish<'a>(self: &'a Arc<Self>, run_id: &'a str) -> Boxed<'a> {
        Box::pin(async move {
            // Pemeriksaan ulang tepat sebelum push: berkas bisa berubah sejak
            // akhir giliran, mis. selama review (R12).
            if self.touches_sensitive(run_id).await? {
                let mut run = self.load(run_id).await?;
                return self
                    .hold(
                        &mut run,
                        HoldReason::NeedsConfirmation,
                        Some(SENSITIVE.to_owned()),
                    )
                    .await;
            }
            let held = match self.publish_gated(run_id, &GateDecision::default()).await {
                Ok(PublishOutcome::Published { .. } | PublishOutcome::ReviewStarted { .. }) => {
                    return Ok(());
                }
                Ok(PublishOutcome::Blocked { failed }) => (
                    HoldReason::AcGate,
                    format!("Push held: {} failed the review", failed.join(", ")),
                ),
                Ok(PublishOutcome::NeedsConfirmation { unverified }) => (
                    HoldReason::AcGate,
                    format!(
                        "Push held: {} could not be verified; confirm before pushing",
                        unverified.join(", ")
                    ),
                ),
                Ok(PublishOutcome::CardsChanged { cards }) => (
                    HoldReason::NeedsConfirmation,
                    format!("Push held: {} changed in Lark", cards.join(", ")),
                ),
                Err(RunError::NoChanges) => (HoldReason::Stuck, "No changes to push".to_owned()),
                Err(e) => return Err(e),
            };
            let mut run = self.load(run_id).await?;
            self.hold(&mut run, held.0, Some(held.1)).await
        })
    }

    /// Menghitung pemakaian event ke siklus berjalan. `true` bila batas siklus
    /// tercapai atau kuota langganan menolak (R10); kegagalan menyimpan hanya
    /// dicatat.
    pub(crate) async fn vm_count_usage(
        &self,
        event: &AgentEvent,
        tracker: &mut UsageTracker,
    ) -> bool {
        match event {
            AgentEvent::RateLimit { status, .. } => status == "rejected",
            AgentEvent::MessageUsage { message_id, usage } => {
                let delta = tracker.observe(message_id, usage);
                match self.add_usage(&delta).await {
                    Ok(hit) => hit,
                    Err(e) => {
                        tracing::warn!(error = %e, "pemakaian siklus tidak tersimpan");
                        false
                    }
                }
            }
            _ => false,
        }
    }

    /// Menambah pemakaian ke siklus; `true` bila sudah mencapai batas.
    async fn add_usage(&self, delta: &Usage) -> Result<bool, RunError> {
        let Some(settings) = self.overnight.settings().await? else {
            return Ok(false);
        };
        let Some(window) = super::window_of(&settings) else {
            return Ok(false);
        };
        let start = window.cycle_start(self.sync.clock.now_ms());
        let usage = CycleUsage {
            input_tokens: delta.input_tokens,
            output_tokens: delta.output_tokens,
            cache_read_tokens: delta.cache_read_input_tokens,
            cache_write_tokens: delta.cache_creation_input_tokens,
        };
        self.overnight.add_cycle_usage(start, &usage).await?;
        let used = self.overnight.cycle_usage(start).await?.counted();
        Ok(settings.token_limit.is_some_and(|l| used >= l))
    }

    /// Run dihentikan karena batas: `Held usage_limit` (R10). Review yang sedang
    /// berjalan ikut berhenti.
    pub(crate) async fn hold_for_limit(
        &self,
        run_id: &str,
        generation: i64,
    ) -> Result<(), RunError> {
        self.clear_stopper(run_id, generation);
        self.revoke_capabilities(run_id);
        let mut run = self.load(run_id).await?;
        if run.generation != generation {
            return Ok(());
        }
        run.pid = None;
        if let Some(id) = run.review_id.take() {
            self.close_review_for_limit(&run.id, &id).await?;
        }
        self.hold(
            &mut run,
            HoldReason::UsageLimit,
            Some("Usage limit reached for this overnight cycle".to_owned()),
        )
        .await
    }

    /// Review yang dihentikan batas siklus ditutup sebagai gagal, agar tidak
    /// tertinggal "running".
    async fn close_review_for_limit(&self, run_id: &str, review_id: &str) -> Result<(), RunError> {
        self.agent.end_review(review_id).await;
        if let Some(mut r) = self
            .reviews
            .reviews(run_id)
            .await?
            .into_iter()
            .find(|r| r.id == review_id)
        {
            r.status = ReviewStatus::Failed;
            r.error = Some("stopped: usage limit reached for this overnight cycle".to_owned());
            r.finished_at_ms = Some(self.now());
            self.reviews.save_review(&r).await?;
        }
        Ok(())
    }

    /// Perubahan run menyentuh CI, konfigurasi agent, atau hook git.
    pub(crate) async fn touches_sensitive(&self, run_id: &str) -> Result<bool, RunError> {
        let run = self.load(run_id).await?;
        let work = self.member_changes(&run).await?;
        Ok(touches_sensitive_path(
            work.iter()
                .flat_map(|(_, c)| c.files.iter().map(String::as_str)),
        ))
    }

    /// Review terakhir run ini selesai sebagai basi.
    async fn last_review_stale(&self, run_id: &str) -> Result<bool, RunError> {
        Ok(self
            .reviews
            .reviews(run_id)
            .await?
            .into_iter()
            .max_by_key(|r| r.started_at_ms)
            .is_some_and(|r| r.stale))
    }

    /// Review Push & PR di VM selesai atau gagal: lanjut publish, atau tahan
    /// `ac_gate` bila review tidak selesai.
    pub(crate) async fn vm_after_review(
        self: &Arc<Self>,
        mut run: RunRecord,
        finished: bool,
        why: &str,
    ) -> Result<(), RunError> {
        if finished {
            // Review basi (kode berubah selama review) tidak diulang otomatis:
            // review baru bisa basi lagi dan menguras kuota tanpa batas.
            if self.last_review_stale(&run.id).await? {
                return self
                    .hold(&mut run, HoldReason::AcGate, Some(STALE_REVIEW.to_owned()))
                    .await;
            }
            return self.vm_publish(&run.id).await;
        }
        self.hold(
            &mut run,
            HoldReason::AcGate,
            Some(format!("Push held: {why}")),
        )
        .await
    }
}
