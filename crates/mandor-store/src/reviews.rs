//! Riwayat review per run (R32) beserta verdict per AC saat review itu.

use crate::{Store, StoreError};

/// Satu review yang tersimpan.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredReview {
    pub id: String,
    pub run_id: String,
    /// `manual`, `push_and_pr`, atau `automatic_fix`.
    pub trigger: String,
    /// `running`, `finished`, atau `failed`.
    pub status: String,
    pub browser_used: bool,
    pub error: Option<String>,
    pub cost_usd: Option<f64>,
    pub usage_json: String,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

/// Verdict satu AC dalam satu review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVerdict {
    pub ac_id: String,
    pub label: Option<String>,
    pub verdict: String,
    pub evidence: Option<String>,
}

impl Store {
    /// Menyimpan atau memperbarui review.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila penulisan gagal.
    pub async fn save_review(&self, r: &StoredReview) -> Result<(), StoreError> {
        let browser = i64::from(r.browser_used);
        sqlx::query!(
            "INSERT INTO reviews (id, run_id, trigger, status, browser_used, error, cost_usd,
                                  usage_json, started_at_ms, finished_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (id) DO UPDATE SET status = excluded.status,
                 browser_used = excluded.browser_used, error = excluded.error,
                 cost_usd = excluded.cost_usd, usage_json = excluded.usage_json,
                 finished_at_ms = excluded.finished_at_ms",
            r.id,
            r.run_id,
            r.trigger,
            r.status,
            browser,
            r.error,
            r.cost_usd,
            r.usage_json,
            r.started_at_ms,
            r.finished_at_ms
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    /// Mengganti verdict per AC milik review.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila penulisan gagal.
    pub async fn save_review_verdicts(
        &self,
        review_id: &str,
        verdicts: &[StoredVerdict],
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        sqlx::query!("DELETE FROM review_verdicts WHERE review_id = ?", review_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        for v in verdicts {
            sqlx::query!(
                "INSERT INTO review_verdicts (review_id, ac_id, label, verdict, evidence)
                 VALUES (?, ?, ?, ?, ?)",
                review_id,
                v.ac_id,
                v.label,
                v.verdict,
                v.evidence
            )
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        }
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(())
    }

    /// Review run, terbaru dulu.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila pembacaan gagal.
    pub async fn reviews(&self, run_id: &str) -> Result<Vec<StoredReview>, StoreError> {
        let rows = sqlx::query!(
            r#"SELECT id, run_id, trigger, status, browser_used, error, cost_usd, usage_json,
                      started_at_ms, finished_at_ms
               FROM reviews WHERE run_id = ? ORDER BY started_at_ms DESC, id DESC"#,
            run_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows
            .into_iter()
            .map(|r| StoredReview {
                id: r.id,
                run_id: r.run_id,
                trigger: r.trigger,
                status: r.status,
                browser_used: r.browser_used != 0,
                error: r.error,
                cost_usd: r.cost_usd,
                usage_json: r.usage_json,
                started_at_ms: r.started_at_ms,
                finished_at_ms: r.finished_at_ms,
            })
            .collect())
    }

    /// Verdict per AC satu review.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila pembacaan gagal.
    pub async fn review_verdicts(&self, review_id: &str) -> Result<Vec<StoredVerdict>, StoreError> {
        let rows = sqlx::query!(
            "SELECT ac_id, label, verdict, evidence FROM review_verdicts
             WHERE review_id = ? ORDER BY rowid",
            review_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows
            .into_iter()
            .map(|r| StoredVerdict {
                ac_id: r.ac_id,
                label: r.label,
                verdict: r.verdict,
                evidence: r.evidence,
            })
            .collect())
    }

    /// Masa simpan riwayat review (R32): review dari run yang ditutup sebelum
    /// `closed_before_ms` dihapus, dan setiap run hanya menyimpan `keep_per_run`
    /// review terbaru. Mengembalikan jumlah review yang dihapus.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila penghapusan gagal.
    pub async fn prune_reviews(
        &self,
        closed_before_ms: i64,
        keep_per_run: u32,
    ) -> Result<u64, StoreError> {
        let keep = i64::from(keep_per_run);
        let old = sqlx::query!(
            "DELETE FROM reviews WHERE run_id IN
               (SELECT id FROM runs WHERE closed = 1 AND updated_at_ms < ?)",
            closed_before_ms
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        let extra = sqlx::query!(
            "DELETE FROM reviews WHERE id IN (
               SELECT id FROM (
                 SELECT id, ROW_NUMBER() OVER (
                   PARTITION BY run_id ORDER BY started_at_ms DESC, id DESC) AS n
                 FROM reviews)
               WHERE n > ?)",
            keep
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(old.rows_affected() + extra.rows_affected())
    }
}
