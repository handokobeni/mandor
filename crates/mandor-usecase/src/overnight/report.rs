//! Laporan siklus lembur untuk tab Overnight (R18 plan VM): hasil per kartu,
//! kartu yang dikeluarkan dari antrean, dan pemakaian terhadap batas.

use mandor_domain::group::CardState;
use mandor_domain::planned::PlannedState;
use mandor_domain::{HoldReason, RunState, Verdict};

use super::{CycleUsage, Dequeued, OvernightStore, QueueItem, window_of};
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

/// Keadaan satu kartu grup di laporan (R23 plan grup).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupCardStatus {
    /// Sudah di-commit di branch grup.
    Committed,
    /// Sedang dikerjakan atau tempat grup berhenti.
    Active,
    /// Belum dimulai.
    Pending,
    /// Dikeluarkan dari grup.
    Removed,
}

impl GroupCardStatus {
    fn of(state: &CardState) -> Self {
        match state {
            CardState::Done => Self::Committed,
            CardState::Active => Self::Active,
            CardState::Pending => Self::Pending,
            CardState::Removed(_) => Self::Removed,
        }
    }

    /// Nama stabil untuk protokol.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::Active => "active",
            Self::Pending => "pending",
            Self::Removed => "removed",
        }
    }
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
    /// Sumber desain yang gagal diperbarui saat kartu dimulai.
    pub design_notes: Vec<String>,
    /// Keadaan kartu di grupnya; `None` untuk kartu tunggal.
    pub group: Option<GroupCardStatus>,
    /// Kartu grup ter-commit yang berubah di Lark dan belum diputuskan (R13).
    pub changed_in_lark: bool,
}

/// Kartu Planned yang ditahan di antrean tanpa dikerjakan (R15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldPlanned {
    pub record_id: String,
    pub reason: String,
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
    /// Kartu Planned yang ditahan; plan dan klaimnya masih di VM.
    pub planned_held: Vec<HeldPlanned>,
}

/// Item Planned berstatus `Held` di antrean.
pub(super) fn held_planned(queue: &[QueueItem]) -> Vec<HeldPlanned> {
    queue
        .iter()
        .filter_map(|i| {
            let p = i.planned.as_ref()?;
            (p.state == PlannedState::Held).then(|| HeldPlanned {
                record_id: i.record_id.clone(),
                reason: p.reason.clone().unwrap_or_default(),
            })
        })
        .collect()
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
        report.planned_held = held_planned(&self.overnight.queue().await?);
        for run in self.runs.list_active().await? {
            let rows = self.reviews.ac_rows(&run.id).await?;
            let cards = self.runs.cards(&run.id).await?;
            // Run grup: satu baris per kartu dengan hasil grupnya (KTD-G16).
            let entries: Vec<(String, Option<GroupCardStatus>, bool)> = if cards.is_empty() {
                vec![(run.record_id.clone(), None, false)]
            } else {
                cards
                    .iter()
                    .map(|c| {
                        let waiting = c.changed_in_lark && c.kept_reason.is_none();
                        (
                            c.record_id.clone(),
                            Some(GroupCardStatus::of(&c.state)),
                            waiting,
                        )
                    })
                    .collect()
            };
            for (record_id, group, changed_in_lark) in entries {
                let count = |v: Verdict| {
                    let n = rows
                        .iter()
                        .filter(|r| r.verdict == v)
                        .filter(|r| {
                            group.is_none() || r.card.as_deref() == Some(record_id.as_str())
                        })
                        .count();
                    u32::try_from(n).unwrap_or(u32::MAX)
                };
                let card = ReportCard {
                    record_id: record_id.clone(),
                    run_id: run.id.clone(),
                    repo_id: run.repo_id.clone(),
                    outcome: outcome_of(&run),
                    hold_note: run.hold.as_ref().and_then(|h| h.note.clone()),
                    pr_url: run.pr_url.clone(),
                    ac_pass: count(Verdict::Pass),
                    ac_fail: count(Verdict::Fail),
                    ac_cant_verify: count(Verdict::CantVerify),
                    entered_waiting_fix: run.entered_waiting_fix_ms.is_some(),
                    design_notes: run.design_notes.clone(),
                    group,
                    changed_in_lark,
                };
                if card.outcome == CardOutcome::SentToReview && !card.entered_waiting_fix {
                    report.reviewed_without_fix += 1;
                }
                report.cards.push(card);
            }
        }
        Ok(report)
    }
}
