//! Port `OvernightStore` usecase di atas tabel `overnight_*` dan setelan
//! `overnight` (KTD-57 plan VM).

use mandor_domain::Design;
use mandor_domain::planned::{DesignMark, PlannedState, PlannedWork};
use mandor_usecase::PortError;
use mandor_usecase::StartTarget;
use mandor_usecase::overnight::{
    CycleUsage, Dequeued, OvernightSettings, OvernightStore, PlannedEntry, QueueItem, ReportState,
};
use serde::{Deserialize, Serialize};

use crate::{Store, StoreError};

/// Kunci setelan lembur di tabel `settings`.
const SETTINGS_KEY: &str = "overnight";

/// Pesan konflik saat kartu sudah ada di antrean.
const QUEUED: &str = "card is already queued";

/// Target Start yang disimpan di `target_json`.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TargetJson {
    Repo { repo_id: String },
    Workspace { id: String, repo_ids: Vec<String> },
}

impl From<&StartTarget> for TargetJson {
    fn from(t: &StartTarget) -> Self {
        match t {
            StartTarget::Repo(r) => Self::Repo { repo_id: r.clone() },
            StartTarget::Workspace { id, repo_ids } => Self::Workspace {
                id: id.clone(),
                repo_ids: repo_ids.clone(),
            },
        }
    }
}

impl From<TargetJson> for StartTarget {
    fn from(t: TargetJson) -> Self {
        match t {
            TargetJson::Repo { repo_id } => Self::Repo(repo_id),
            TargetJson::Workspace { id, repo_ids } => Self::Workspace { id, repo_ids },
        }
    }
}

/// Satu baris Design di `planned_json`.
#[derive(Serialize, Deserialize)]
struct DesignMarkJson {
    ac_id: String,
    design: String,
    note: Option<String>,
}

/// Paket plan di `planned_json`.
#[derive(Serialize, Deserialize)]
struct PlannedJson {
    plan: String,
    branch: String,
    design: Vec<DesignMarkJson>,
    laptop_design: Vec<String>,
    /// Tidak ada pada baris dari versi sebelum daftar repo dibawa.
    #[serde(default)]
    repos: Vec<String>,
}

impl From<&PlannedWork> for PlannedJson {
    fn from(w: &PlannedWork) -> Self {
        Self {
            plan: w.plan.clone(),
            branch: w.branch.clone(),
            design: w
                .design
                .iter()
                .map(|d| DesignMarkJson {
                    ac_id: d.ac_id.clone(),
                    design: d.design.as_str().to_owned(),
                    note: d.note.clone(),
                })
                .collect(),
            laptop_design: w.laptop_design.clone(),
            repos: w.repos.clone(),
        }
    }
}

impl TryFrom<PlannedJson> for PlannedWork {
    type Error = StoreError;

    fn try_from(j: PlannedJson) -> Result<Self, StoreError> {
        let mut design = Vec::with_capacity(j.design.len());
        for d in j.design {
            let value = Design::parse(&d.design)
                .ok_or_else(|| StoreError::corrupt(format!("unknown design {:?}", d.design)))?;
            design.push(DesignMark {
                ac_id: d.ac_id,
                design: value,
                note: d.note,
            });
        }
        Ok(Self {
            plan: j.plan,
            branch: j.branch,
            design,
            laptop_design: j.laptop_design,
            repos: j.repos,
        })
    }
}

/// Bagian Planned dari kolom baris antrean; `None` untuk item No plan.
fn planned_entry(
    planned_json: Option<String>,
    state: Option<&str>,
    origin_daemon: Option<String>,
    reason: Option<String>,
) -> Result<Option<PlannedEntry>, StoreError> {
    let Some(json) = planned_json else {
        return Ok(None);
    };
    let work: PlannedJson = serde_json::from_str(&json).map_err(StoreError::internal)?;
    let state = state
        .and_then(PlannedState::parse)
        .ok_or_else(|| StoreError::corrupt(format!("unknown planned state {state:?}")))?;
    Ok(Some(PlannedEntry {
        work: work.try_into()?,
        state,
        origin_daemon,
        reason,
    }))
}

/// Setelan lembur di tabel `settings`.
#[derive(Serialize, Deserialize)]
struct SettingsJson {
    start: String,
    end: String,
    offset: String,
    #[serde(default)]
    token_limit: Option<u64>,
}

fn to_i64(v: u64) -> Result<i64, StoreError> {
    i64::try_from(v).map_err(StoreError::internal)
}

fn to_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

/// Adapter penyimpanan lembur.
/// Posisi penyisipan item antrean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    Back,
    Front,
}

#[derive(Debug, Clone)]
pub struct OvernightsStore {
    store: Store,
}

impl OvernightsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Menyisipkan item di akhir atau awal antrean.
    async fn insert(&self, item: &QueueItem, at: Position) -> Result<(), PortError> {
        let target =
            serde_json::to_string(&TargetJson::from(&item.target)).map_err(StoreError::internal)?;
        let start_now = i64::from(item.start_now);
        let design = if item.design_sources.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&item.design_sources).map_err(StoreError::internal)?)
        };
        let planned = item
            .planned
            .as_ref()
            .map(|p| serde_json::to_string(&PlannedJson::from(&p.work)))
            .transpose()
            .map_err(StoreError::internal)?;
        let state = item.planned.as_ref().map(|p| p.state.as_str());
        let origin = item
            .planned
            .as_ref()
            .and_then(|p| p.origin_daemon.as_deref());
        let reason = item.planned.as_ref().and_then(|p| p.reason.as_deref());
        let group = if item.group.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&item.group).map_err(StoreError::internal)?)
        };
        let front = i64::from(at == Position::Front);
        sqlx::query!(
            "INSERT INTO overnight_queue (record_id, position, target_json, note, start_now,
                added_at_ms, design_json, planned_json, planned_state, origin_daemon, planned_reason,
                group_json)
             VALUES (?, CASE WHEN ? = 1
                    THEN (SELECT COALESCE(MIN(position), 1) - 1 FROM overnight_queue)
                    ELSE (SELECT COALESCE(MAX(position), 0) + 1 FROM overnight_queue) END,
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            item.record_id,
            front,
            target,
            item.note,
            start_now,
            item.added_at_ms,
            design,
            planned,
            state,
            origin,
            reason,
            group
        )
        .execute(&self.store.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, QUEUED))?;
        Ok(())
    }
}

impl OvernightStore for OvernightsStore {
    async fn queue(&self) -> Result<Vec<QueueItem>, PortError> {
        let rows = sqlx::query!(
            "SELECT record_id, target_json, note, start_now, added_at_ms, design_json,
                    planned_json, planned_state, origin_daemon, planned_reason, group_json
             FROM overnight_queue ORDER BY position, added_at_ms"
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let target: TargetJson =
                serde_json::from_str(&r.target_json).map_err(StoreError::internal)?;
            let design_sources: Vec<String> = match r.design_json {
                Some(j) => serde_json::from_str(&j).map_err(StoreError::internal)?,
                None => Vec::new(),
            };
            let group: Vec<String> = match r.group_json {
                Some(j) => serde_json::from_str(&j).map_err(StoreError::internal)?,
                None => Vec::new(),
            };
            out.push(QueueItem {
                record_id: r.record_id,
                target: target.into(),
                note: r.note,
                start_now: r.start_now != 0,
                added_at_ms: r.added_at_ms,
                design_sources,
                planned: planned_entry(
                    r.planned_json,
                    r.planned_state.as_deref(),
                    r.origin_daemon,
                    r.planned_reason,
                )?,
                group,
            });
        }
        Ok(out)
    }

    async fn enqueue(&self, item: &QueueItem) -> Result<(), PortError> {
        self.insert(item, Position::Back).await
    }

    async fn enqueue_front(&self, item: &QueueItem) -> Result<(), PortError> {
        self.insert(item, Position::Front).await
    }

    async fn dequeue(
        &self,
        record_id: &str,
        reason: Option<&str>,
        now_ms: i64,
    ) -> Result<(), PortError> {
        let mut tx = self
            .store
            .pool
            .begin()
            .await
            .map_err(StoreError::internal)?;
        sqlx::query!("DELETE FROM overnight_queue WHERE record_id = ?", record_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        if let Some(reason) = reason {
            sqlx::query!(
                "INSERT INTO overnight_dequeued (record_id, reason, at_ms) VALUES (?, ?, ?)",
                record_id,
                reason,
                now_ms
            )
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        }
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(())
    }

    async fn reorder(&self, record_ids: &[String]) -> Result<(), PortError> {
        let mut tx = self
            .store
            .pool
            .begin()
            .await
            .map_err(StoreError::internal)?;
        // Kartu yang tidak disebut tetap di akhir, urutan lamanya dipertahankan.
        let offset = i64::try_from(record_ids.len()).map_err(StoreError::internal)? + 1;
        sqlx::query!("UPDATE overnight_queue SET position = position + ?", offset)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        for (i, id) in record_ids.iter().enumerate() {
            let pos = i64::try_from(i).map_err(StoreError::internal)?;
            sqlx::query!(
                "UPDATE overnight_queue SET position = ? WHERE record_id = ?",
                pos,
                id
            )
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        }
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(())
    }

    async fn set_planned_state(
        &self,
        record_id: &str,
        state: PlannedState,
        reason: Option<&str>,
    ) -> Result<(), PortError> {
        let state = state.as_str();
        let done = sqlx::query!(
            "UPDATE overnight_queue SET planned_state = ?, planned_reason = ?
             WHERE record_id = ? AND planned_json IS NOT NULL",
            state,
            reason,
            record_id
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        if done.rows_affected() == 0 {
            return Err(PortError::Rejected(
                "the card is not a planned queue item".to_owned(),
            ));
        }
        Ok(())
    }

    async fn set_start_now(&self, record_id: &str) -> Result<(), PortError> {
        let done = sqlx::query!(
            "UPDATE overnight_queue SET start_now = 1 WHERE record_id = ?",
            record_id
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        if done.rows_affected() == 0 {
            return Err(PortError::Rejected("the card is not queued".to_owned()));
        }
        Ok(())
    }

    async fn dequeued_since(&self, since_ms: i64) -> Result<Vec<Dequeued>, PortError> {
        let rows = sqlx::query!(
            "SELECT record_id, reason, at_ms FROM overnight_dequeued
             WHERE at_ms >= ? ORDER BY at_ms, id",
            since_ms
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows
            .into_iter()
            .map(|r| Dequeued {
                record_id: r.record_id,
                reason: r.reason,
                at_ms: r.at_ms,
            })
            .collect())
    }

    async fn laptop_only(&self, record_id: &str) -> Result<bool, PortError> {
        let n = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM overnight_laptop_only WHERE record_id = ?",
            record_id
        )
        .fetch_one(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(n > 0)
    }

    async fn set_laptop_only(&self, record_id: &str, now_ms: i64) -> Result<(), PortError> {
        sqlx::query!(
            "INSERT INTO overnight_laptop_only (record_id, at_ms) VALUES (?, ?)
             ON CONFLICT (record_id) DO NOTHING",
            record_id,
            now_ms
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    async fn settings(&self) -> Result<Option<OvernightSettings>, PortError> {
        let Some(raw) = self.store.setting(SETTINGS_KEY).await? else {
            return Ok(None);
        };
        let s: SettingsJson = serde_json::from_str(&raw).map_err(StoreError::internal)?;
        Ok(Some(OvernightSettings {
            start: s.start,
            end: s.end,
            offset: s.offset,
            token_limit: s.token_limit,
        }))
    }

    async fn set_settings(&self, s: &OvernightSettings) -> Result<(), PortError> {
        let raw = serde_json::to_string(&SettingsJson {
            start: s.start.clone(),
            end: s.end.clone(),
            offset: s.offset.clone(),
            token_limit: s.token_limit,
        })
        .map_err(StoreError::internal)?;
        Ok(self.store.set_setting(SETTINGS_KEY, &raw).await?)
    }

    async fn cycle_usage(&self, cycle_start_ms: i64) -> Result<CycleUsage, PortError> {
        let row = sqlx::query!(
            "SELECT input_tokens, output_tokens, cache_read_tokens, cache_write_tokens
             FROM overnight_cycles WHERE cycle_start_ms = ?",
            cycle_start_ms
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(row.map_or_else(CycleUsage::default, |r| CycleUsage {
            input_tokens: to_u64(r.input_tokens),
            output_tokens: to_u64(r.output_tokens),
            cache_read_tokens: to_u64(r.cache_read_tokens),
            cache_write_tokens: to_u64(r.cache_write_tokens),
        }))
    }

    async fn report_state(&self, cycle_start_ms: i64) -> Result<ReportState, PortError> {
        let row = sqlx::query!(
            "SELECT report_sent_at_ms, report_attempts FROM overnight_cycles WHERE cycle_start_ms = ?",
            cycle_start_ms
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(row.map_or_else(ReportState::default, |r| ReportState {
            sent: r.report_sent_at_ms.is_some(),
            attempts: u32::try_from(r.report_attempts).unwrap_or(u32::MAX),
        }))
    }

    async fn record_report(
        &self,
        cycle_start_ms: i64,
        sent: bool,
        now_ms: i64,
    ) -> Result<(), PortError> {
        let sent_at = sent.then_some(now_ms);
        sqlx::query!(
            "INSERT INTO overnight_cycles (cycle_start_ms, report_sent_at_ms, report_attempts)
             VALUES (?, ?, 1)
             ON CONFLICT (cycle_start_ms) DO UPDATE SET
                 report_attempts = report_attempts + 1,
                 report_sent_at_ms = COALESCE(report_sent_at_ms, excluded.report_sent_at_ms)",
            cycle_start_ms,
            sent_at
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    async fn add_cycle_usage(&self, cycle_start_ms: i64, u: &CycleUsage) -> Result<(), PortError> {
        let (i, o, cr, cw) = (
            to_i64(u.input_tokens)?,
            to_i64(u.output_tokens)?,
            to_i64(u.cache_read_tokens)?,
            to_i64(u.cache_write_tokens)?,
        );
        sqlx::query!(
            "INSERT INTO overnight_cycles
                 (cycle_start_ms, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (cycle_start_ms) DO UPDATE SET
                 input_tokens = input_tokens + excluded.input_tokens,
                 output_tokens = output_tokens + excluded.output_tokens,
                 cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                 cache_write_tokens = cache_write_tokens + excluded.cache_write_tokens",
            cycle_start_ms,
            i,
            o,
            cr,
            cw
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kolom_planned_rusak_adalah_error_bukan_no_plan() {
        let state = || Some("waiting");
        assert!(planned_entry(Some("{".to_owned()), state(), None, None).is_err());
        let bad_design = r#"{"plan":"p","branch":"b","design":[{"ac_id":"a","design":"x","note":null}],"laptop_design":[]}"#;
        assert!(planned_entry(Some(bad_design.to_owned()), state(), None, None).is_err());
        let ok = r#"{"plan":"p","branch":"b","design":[],"laptop_design":[]}"#;
        assert!(planned_entry(Some(ok.to_owned()), Some("x"), None, None).is_err());
        assert!(
            planned_entry(Some(ok.to_owned()), state(), None, None)
                .unwrap()
                .is_some()
        );
        assert!(planned_entry(None, None, None, None).unwrap().is_none());
    }
}
