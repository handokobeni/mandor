//! Laporan siklus lembur untuk tab Overnight (R18 plan VM): hasil per kartu,
//! kartu yang dikeluarkan dari antrean, dan pemakaian terhadap batas.

use mandor_domain::{HoldReason, RunState, Verdict};

use super::{CycleUsage, Dequeued, OvernightStore, window_of};
use crate::ports::{Clock, Deps};
use crate::run::{RunError, RunService};
use crate::run_ports::{LedgerStore, RunRecord, RunStore};

/// Hasil satu kartu VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardOutcome {
    /// PR draft menunggu "Send to review".
    Draft,
    /// PR sudah dikirim ke review.
    SentToReview,
    Held(HoldReason),
    Failed,
    /// Masih dikerjakan atau menunggu langkah lain.
    Working,
}

/// Satu kartu VM di laporan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportCard {
    pub record_id: String,
    pub run_id: String,
    pub repo_id: String,
    pub outcome: CardOutcome,
    /// Pertanyaan atau alasan tertahan.
    pub hold_note: Option<String>,
    pub pr_url: Option<String>,
    pub ac_pass: u32,
    pub ac_fail: u32,
    pub ac_cant_verify: u32,
    /// Kartu pernah masuk Waiting Fix.
    pub entered_waiting_fix: bool,
}

/// Laporan siklus berjalan (atau siklus semalam, di pagi hari).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OvernightReport {
    /// Awal siklus; `None` bila jam lembur belum diatur.
    pub cycle_start_ms: Option<i64>,
    pub usage: CycleUsage,
    pub limit: Option<u64>,
    pub cards: Vec<ReportCard>,
    pub dequeued: Vec<Dequeued>,
    /// Kartu yang sampai Waiting Review tanpa pernah masuk Waiting Fix.
    pub reviewed_without_fix: u32,
}

fn outcome_of(run: &RunRecord) -> CardOutcome {
    match (&run.state, &run.hold) {
        (RunState::Held, Some(h)) => CardOutcome::Held(h.reason),
        (RunState::Failed, _) => CardOutcome::Failed,
        _ if run.draft_pending.is_some() => CardOutcome::Draft,
        _ if run.pr_url.is_some() && !run.state.is_working() => CardOutcome::SentToReview,
        _ => CardOutcome::Working,
    }
}

impl<D: Deps> RunService<D> {
    /// Laporan siklus lembur untuk tab Overnight.
    ///
    /// # Errors
    /// Error penyimpanan.
    pub async fn overnight_report(&self) -> Result<OvernightReport, RunError> {
        let settings = self.overnight.settings().await?;
        let now = self.sync.clock.now_ms();
        let cycle_start_ms = settings
            .as_ref()
            .and_then(window_of)
            .map(|w| w.cycle_start(now));
        let mut report = OvernightReport {
            cycle_start_ms,
            limit: settings.and_then(|s| s.token_limit),
            ..OvernightReport::default()
        };
        if let Some(start) = cycle_start_ms {
            report.usage = self.overnight.cycle_usage(start).await?;
            report.dequeued = self.overnight.dequeued_since(start).await?;
        }
        for run in self.runs.list_active().await? {
            let rows = self.reviews.ac_rows(&run.id).await?;
            let count = |v: Verdict| {
                u32::try_from(rows.iter().filter(|r| r.verdict == v).count()).unwrap_or(u32::MAX)
            };
            let card = ReportCard {
                record_id: run.record_id.clone(),
                run_id: run.id.clone(),
                repo_id: run.repo_id.clone(),
                outcome: outcome_of(&run),
                hold_note: run.hold.as_ref().and_then(|h| h.note.clone()),
                pr_url: run.pr_url.clone(),
                ac_pass: count(Verdict::Pass),
                ac_fail: count(Verdict::Fail),
                ac_cant_verify: count(Verdict::CantVerify),
                entered_waiting_fix: run.entered_waiting_fix_ms.is_some(),
            };
            if card.outcome == CardOutcome::SentToReview && !card.entered_waiting_fix {
                report.reviewed_without_fix += 1;
            }
            report.cards.push(card);
        }
        Ok(report)
    }
}
