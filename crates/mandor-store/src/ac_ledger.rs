//! Buku AC per run (KTD-25). Urutan baris disimpan agar sama dengan tracker.

use mandor_domain::ac_ledger::{Visual, VisualDetail};
use mandor_domain::{AcRow, Design, RowStatus, Verdict};
use serde::{Deserialize, Serialize};

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
            let visual = r.visual.as_str();
            let visual_json = visual_json(&r.visual_detail)?;
            sqlx::query!(
                "INSERT INTO ac_rows (run_id, ac_id, position, label, text, design, design_note,
                                      verdict, evidence, stale, status, reason, decided_by, card,
                                      visual, visual_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
                r.decided_by,
                r.card,
                visual,
                visual_json
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
                    reason, decided_by, card, visual, visual_json
             FROM ac_rows WHERE run_id = ? ORDER BY position",
            run_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|r| {
                to_row(Raw {
                    ac_id: r.ac_id,
                    label: r.label,
                    text: r.text,
                    design: r.design,
                    design_note: r.design_note,
                    verdict: r.verdict,
                    evidence: r.evidence,
                    stale: r.stale,
                    status: r.status,
                    reason: r.reason,
                    decided_by: r.decided_by,
                    card: r.card,
                    visual: r.visual,
                    visual_json: r.visual_json,
                })
            })
            .collect()
    }

    /// Baris buku AC semua run aktif dalam satu query: `(run_id, baris)`, untuk
    /// ringkasan AC di papan dan sidebar.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila pembacaan gagal atau nilai rusak.
    pub async fn active_ac_rows(&self) -> Result<Vec<(String, AcRow)>, StoreError> {
        let rows = sqlx::query!(
            "SELECT a.run_id, a.ac_id, a.label, a.text, a.design, a.design_note, a.verdict,
                    a.evidence, a.stale, a.status, a.reason, a.decided_by, a.card, a.visual,
                    a.visual_json
             FROM ac_rows a JOIN runs r ON r.id = a.run_id
             WHERE r.closed = 0 ORDER BY a.run_id, a.position"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|r| {
                let row = to_row(Raw {
                    ac_id: r.ac_id,
                    label: r.label,
                    text: r.text,
                    design: r.design,
                    design_note: r.design_note,
                    verdict: r.verdict,
                    evidence: r.evidence,
                    stale: r.stale,
                    status: r.status,
                    reason: r.reason,
                    decided_by: r.decided_by,
                    card: r.card,
                    visual: r.visual,
                    visual_json: r.visual_json,
                })?;
                Ok((r.run_id, row))
            })
            .collect()
    }
}

/// Kolom baris `ac_rows` sebelum dipetakan.
struct Raw {
    ac_id: String,
    label: Option<String>,
    text: String,
    design: String,
    design_note: Option<String>,
    verdict: String,
    evidence: Option<String>,
    stale: i64,
    status: String,
    reason: Option<String>,
    decided_by: Option<String>,
    card: Option<String>,
    visual: String,
    visual_json: Option<String>,
}

/// Bentuk simpan [`VisualDetail`] di kolom `visual_json`.
#[derive(Serialize, Deserialize, Default)]
struct StoredVisual {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prototype_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prototype_rev: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    screenshots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decision_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    decided_by: Option<String>,
}

fn visual_json(d: &VisualDetail) -> Result<Option<String>, StoreError> {
    if *d == VisualDetail::default() {
        return Ok(None);
    }
    serde_json::to_string(&StoredVisual {
        description: d.description.clone(),
        app_path: d.app_path.clone(),
        prototype_path: d.prototype_path.clone(),
        prototype_rev: d.prototype_rev.clone(),
        screenshots: d.screenshots.clone(),
        decision_reason: d.decision_reason.clone(),
        decided_by: d.decided_by.clone(),
    })
    .map(Some)
    .map_err(StoreError::internal)
}

fn visual_detail(raw: Option<&str>) -> Result<VisualDetail, StoreError> {
    let s: StoredVisual = match raw {
        Some(j) => serde_json::from_str(j).map_err(StoreError::internal)?,
        None => StoredVisual::default(),
    };
    Ok(VisualDetail {
        description: s.description,
        app_path: s.app_path,
        prototype_path: s.prototype_path,
        prototype_rev: s.prototype_rev,
        screenshots: s.screenshots,
        decision_reason: s.decision_reason,
        decided_by: s.decided_by,
    })
}

fn to_row(r: Raw) -> Result<AcRow, StoreError> {
    Ok(AcRow {
        ac_id: r.ac_id,
        label: r.label,
        text: r.text,
        design: Design::parse(&r.design).ok_or_else(|| corrupt("design", &r.design))?,
        design_note: r.design_note,
        verdict: Verdict::parse(&r.verdict).ok_or_else(|| corrupt("verdict", &r.verdict))?,
        evidence: r.evidence,
        stale: r.stale != 0,
        status: RowStatus::parse(&r.status).ok_or_else(|| corrupt("status", &r.status))?,
        reason: r.reason,
        decided_by: r.decided_by,
        card: r.card,
        visual: Visual::parse(&r.visual).ok_or_else(|| corrupt("visual", &r.visual))?,
        visual_detail: visual_detail(r.visual_json.as_deref())?,
    })
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
