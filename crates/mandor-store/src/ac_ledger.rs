//! Buku AC per run (KTD-25). Urutan baris disimpan agar sama dengan tracker.

use mandor_domain::{AcRow, Design, RowStatus, Verdict};

use crate::{Store, StoreError};

impl Store {
    /// Mengganti seluruh buku AC run dengan `rows`, sesuai urutannya.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila penulisan gagal.
    pub async fn save_ac_rows(&self, run_id: &str, rows: &[AcRow]) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(StoreError::internal)?;
        sqlx::query!("DELETE FROM ac_rows WHERE run_id = ?", run_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        for (i, r) in rows.iter().enumerate() {
            let position = i64::try_from(i).map_err(StoreError::internal)?;
            let design = r.design.as_str();
            let verdict = r.verdict.as_str();
            let status = r.status.as_str();
            let stale = i64::from(r.stale);
            sqlx::query!(
                "INSERT INTO ac_rows (run_id, ac_id, position, label, text, design, design_note,
                                      verdict, evidence, stale, status, reason, decided_by)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                run_id,
                r.ac_id,
                position,
                r.label,
                r.text,
                design,
                r.design_note,
                verdict,
                r.evidence,
                stale,
                status,
                r.reason,
                r.decided_by
            )
            .execute(&mut *tx)
            .await
            .map_err(StoreError::internal)?;
        }
        tx.commit().await.map_err(StoreError::internal)?;
        Ok(())
    }

    /// Buku AC run, urut seperti saat disimpan. Kosong bila belum ada.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila baris rusak atau pembacaan gagal.
    pub async fn ac_rows(&self, run_id: &str) -> Result<Vec<AcRow>, StoreError> {
        let rows = sqlx::query!(
            "SELECT ac_id, label, text, design, design_note, verdict, evidence, stale, status,
                    reason, decided_by
             FROM ac_rows WHERE run_id = ? ORDER BY position",
            run_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|r| {
                Ok(AcRow {
                    ac_id: r.ac_id,
                    label: r.label,
                    text: r.text,
                    design: Design::parse(&r.design).ok_or_else(|| corrupt("design", &r.design))?,
                    design_note: r.design_note,
                    verdict: Verdict::parse(&r.verdict)
                        .ok_or_else(|| corrupt("verdict", &r.verdict))?,
                    evidence: r.evidence,
                    stale: r.stale != 0,
                    status: RowStatus::parse(&r.status)
                        .ok_or_else(|| corrupt("status", &r.status))?,
                    reason: r.reason,
                    decided_by: r.decided_by,
                })
            })
            .collect()
    }
}

/// Nilai enum yang tidak dikenal di database.
#[derive(Debug, thiserror::Error)]
#[error("unknown {field} value in ac_rows: {value}")]
struct Corrupt {
    field: &'static str,
    value: String,
}

fn corrupt(field: &'static str, value: &str) -> StoreError {
    StoreError::internal(Corrupt {
        field,
        value: value.to_owned(),
    })
}
