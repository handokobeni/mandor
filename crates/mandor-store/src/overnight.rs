//! Port `OvernightStore` usecase di atas tabel `overnight_*` dan setelan
//! `overnight` (KTD-57 plan VM).

use mandor_usecase::PortError;
use mandor_usecase::StartTarget;
use mandor_usecase::overnight::{
    CycleUsage, Dequeued, OvernightSettings, OvernightStore, QueueItem,
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
#[derive(Debug, Clone)]
pub struct OvernightsStore {
    store: Store,
}

impl OvernightsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl OvernightStore for OvernightsStore {
    async fn queue(&self) -> Result<Vec<QueueItem>, PortError> {
        let rows = sqlx::query!(
            "SELECT record_id, target_json, note, start_now, added_at_ms
             FROM overnight_queue ORDER BY position, added_at_ms"
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let target: TargetJson =
                serde_json::from_str(&r.target_json).map_err(StoreError::internal)?;
            out.push(QueueItem {
                record_id: r.record_id,
                target: target.into(),
                note: r.note,
                start_now: r.start_now != 0,
                added_at_ms: r.added_at_ms,
            });
        }
        Ok(out)
    }

    async fn enqueue(&self, item: &QueueItem) -> Result<(), PortError> {
        let target =
            serde_json::to_string(&TargetJson::from(&item.target)).map_err(StoreError::internal)?;
        let start_now = i64::from(item.start_now);
        sqlx::query!(
            "INSERT INTO overnight_queue (record_id, position, target_json, note, start_now, added_at_ms)
             VALUES (?, (SELECT COALESCE(MAX(position), 0) + 1 FROM overnight_queue), ?, ?, ?, ?)",
            item.record_id,
            target,
            item.note,
            start_now,
            item.added_at_ms
        )
        .execute(&self.store.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, QUEUED))?;
        Ok(())
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
