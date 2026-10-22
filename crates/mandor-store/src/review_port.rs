//! Penyimpanan Rilis 2 sebagai port usecase.

use mandor_domain::{AcRow, RowStatus, Usage, Verdict};
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{
    LedgerStore, ReviewHistory, ReviewRecord, ReviewStatus, ReviewTrigger, ReviewerSettings,
};
use serde::{Deserialize, Serialize};

use crate::{Store, StoredReview, StoredVerdict};

/// Kunci setelan reviewer (KTD-30).
const REVIEWER_KEY: &str = "reviewer";
/// Kunci status ekstensi Claude in Chrome terakhir (U13).
pub const CHROME_KEY: &str = "chrome_connected";

/// Detail review di kolom `usage_json`.
#[derive(Serialize, Deserialize, Default)]
struct Details {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    stale: bool,
    #[serde(default)]
    browser_reason: Option<String>,
    #[serde(default)]
    skill: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredSettings {
    #[serde(default)]
    model: Option<String>,
    #[serde(default = "default_budget")]
    budget_cents: u32,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_budget() -> u32 {
    ReviewerSettings::default().budget_cents
}

fn default_timeout() -> u64 {
    ReviewerSettings::default().timeout_secs
}

fn to_stored(r: &ReviewRecord) -> StoredReview {
    let details = Details {
        input_tokens: r.usage.input_tokens,
        output_tokens: r.usage.output_tokens,
        cache_creation_input_tokens: r.usage.cache_creation_input_tokens,
        cache_read_input_tokens: r.usage.cache_read_input_tokens,
        stale: r.stale,
        browser_reason: r.browser_reason.clone(),
        skill: r.skill.clone(),
    };
    StoredReview {
        id: r.id.clone(),
        run_id: r.run_id.clone(),
        trigger: r.trigger.as_str().to_owned(),
        status: r.status.as_str().to_owned(),
        browser_used: r.browser_used,
        error: r.error.clone(),
        cost_usd: r.cost_usd,
        usage_json: serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_owned()),
        started_at_ms: r.started_at_ms,
        finished_at_ms: r.finished_at_ms,
    }
}

fn from_stored(s: StoredReview, verdicts: Vec<AcRow>) -> ReviewRecord {
    let d: Details = serde_json::from_str(&s.usage_json).unwrap_or_default();
    ReviewRecord {
        id: s.id,
        run_id: s.run_id,
        trigger: ReviewTrigger::parse(&s.trigger).unwrap_or(ReviewTrigger::Manual),
        status: ReviewStatus::parse(&s.status).unwrap_or(ReviewStatus::Failed),
        browser_used: s.browser_used,
        browser_reason: d.browser_reason,
        error: s.error,
        cost_usd: s.cost_usd,
        usage: Usage {
            input_tokens: d.input_tokens,
            output_tokens: d.output_tokens,
            cache_creation_input_tokens: d.cache_creation_input_tokens,
            cache_read_input_tokens: d.cache_read_input_tokens,
        },
        stale: d.stale,
        skill: d.skill,
        started_at_ms: s.started_at_ms,
        finished_at_ms: s.finished_at_ms,
        verdicts,
    }
}

fn verdict_row(v: StoredVerdict) -> AcRow {
    let mut row = AcRow::new(&v.ac_id, v.label.as_deref(), "", RowStatus::Active);
    row.verdict = Verdict::parse(&v.verdict).unwrap_or(Verdict::NotReviewed);
    row.evidence = v.evidence;
    row
}

/// Buku AC, review, sumber desain, dan setelan untuk usecase.
#[derive(Debug, Clone)]
pub struct ReviewsStore {
    store: Store,
}

impl ReviewsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl LedgerStore for ReviewsStore {
    async fn ac_rows(&self, run_id: &str) -> Result<Vec<AcRow>, PortError> {
        Ok(self.store.ac_rows(run_id).await?)
    }

    async fn save_ac_rows(&self, run_id: &str, rows: &[AcRow]) -> Result<(), PortError> {
        Ok(self.store.save_ac_rows(run_id, rows).await?)
    }
}

impl ReviewHistory for ReviewsStore {
    async fn save_review(&self, r: &ReviewRecord) -> Result<(), PortError> {
        self.store.save_review(&to_stored(r)).await?;
        let verdicts: Vec<StoredVerdict> = r
            .verdicts
            .iter()
            .map(|row| StoredVerdict {
                ac_id: row.ac_id.clone(),
                label: row.label.clone(),
                verdict: row.verdict.as_str().to_owned(),
                evidence: row.evidence.clone(),
            })
            .collect();
        Ok(self.store.save_review_verdicts(&r.id, &verdicts).await?)
    }

    async fn reviews(&self, run_id: &str) -> Result<Vec<ReviewRecord>, PortError> {
        let reviews = self.store.reviews(run_id).await?;
        let mut by_review: std::collections::HashMap<String, Vec<AcRow>> =
            std::collections::HashMap::new();
        for (id, v) in self.store.run_review_verdicts(run_id).await? {
            by_review.entry(id).or_default().push(verdict_row(v));
        }
        Ok(reviews
            .into_iter()
            .map(|r| {
                let mine = by_review.remove(&r.id).unwrap_or_default();
                from_stored(r, mine)
            })
            .collect())
    }

    async fn reviewer_settings(&self) -> ReviewerSettings {
        let stored = match self.store.setting(REVIEWER_KEY).await {
            Ok(Some(raw)) => serde_json::from_str::<StoredSettings>(&raw).ok(),
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(error = %e, "setelan reviewer tidak terbaca; memakai bawaan");
                None
            }
        };
        stored.map_or_else(ReviewerSettings::default, |s| ReviewerSettings {
            model: s.model.filter(|m| !m.trim().is_empty()),
            budget_cents: s.budget_cents,
            timeout_secs: s.timeout_secs,
        })
    }

    async fn set_reviewer_settings(&self, s: &ReviewerSettings) -> Result<(), PortError> {
        let raw = serde_json::to_string(&StoredSettings {
            model: s.model.clone(),
            budget_cents: s.budget_cents,
            timeout_secs: s.timeout_secs,
        })
        .map_err(|e| PortError::Storage(e.to_string()))?;
        Ok(self.store.set_setting(REVIEWER_KEY, &raw).await?)
    }

    async fn chrome_connected(&self) -> bool {
        matches!(self.store.setting(CHROME_KEY).await, Ok(Some(v)) if v == "true")
    }
}
