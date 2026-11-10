//! Perlengkapan lab per repo dan temuan reviewer di luar AC (plan
//! 2026-11-10-001, U2). Password akun uji tidak pernah disimpan di sini.

use mandor_domain::lab::{Finding, LabEquipment};

use crate::lab_codec::{equipment_json, equipment_of, evidence_json, evidence_of};
use crate::{Store, StoreError};

/// Perlengkapan lab repo beserta status persetujuannya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoLab {
    pub equipment: LabEquipment,
    /// Digest saat disetujui (KTD-L3); `None` = belum disetujui.
    pub approved_digest: Option<String>,
    pub approved_at_ms: Option<i64>,
}

impl Store {
    /// Perlengkapan lab repo, bila ada.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila pembacaan gagal atau JSON rusak.
    pub async fn repo_lab(&self, repo_id: &str) -> Result<Option<RepoLab>, StoreError> {
        let row = sqlx::query!(
            "SELECT lab_json, lab_approved_digest, lab_approved_at_ms FROM repos WHERE id = ?",
            repo_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        let Some(row) = row else { return Ok(None) };
        let Some(raw) = row.lab_json else {
            return Ok(None);
        };
        Ok(Some(RepoLab {
            equipment: equipment_of(&raw)?,
            approved_digest: row.lab_approved_digest,
            approved_at_ms: row.lab_approved_at_ms,
        }))
    }

    /// Menyimpan perlengkapan dan mengosongkan persetujuan dalam satu
    /// pernyataan, sehingga perlengkapan yang berubah selalu perlu disetujui
    /// ulang (R14).
    ///
    /// # Errors
    /// [`StoreError::NotFound`] bila repo tidak ada; [`StoreError::Internal`]
    /// bila penulisan gagal.
    pub async fn save_repo_lab(&self, repo_id: &str, eq: &LabEquipment) -> Result<(), StoreError> {
        let json = equipment_json(eq)?;
        let res = sqlx::query!(
            "UPDATE repos SET lab_json = ?, lab_approved_digest = NULL, lab_approved_at_ms = NULL
             WHERE id = ?",
            json,
            repo_id
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        if res.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    /// Menyetujui perlengkapan yang dilihat developer. Gagal (`false`) bila
    /// perlengkapan tersimpan sudah berbeda dari `seen`.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila penulisan gagal.
    pub async fn approve_repo_lab(
        &self,
        repo_id: &str,
        seen: &LabEquipment,
        digest: &str,
        at_ms: i64,
    ) -> Result<bool, StoreError> {
        let json = equipment_json(seen)?;
        let res = sqlx::query!(
            "UPDATE repos SET lab_approved_digest = ?, lab_approved_at_ms = ?
             WHERE id = ? AND lab_json = ?",
            digest,
            at_ms,
            repo_id,
            json
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(res.rows_affected() == 1)
    }

    /// Menyimpan temuan reviewer di luar AC (R19).
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila penulisan gagal.
    pub async fn save_finding(
        &self,
        review_id: &str,
        f: &Finding,
        at_ms: i64,
    ) -> Result<(), StoreError> {
        let evidence = evidence_json(Some(&f.evidence))?;
        sqlx::query!(
            "INSERT INTO review_findings (review_id, card, title, detail, evidence_json,
                                          created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?)",
            review_id,
            f.card,
            f.title,
            f.detail,
            evidence,
            at_ms
        )
        .execute(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    /// Temuan semua review run: `(review_id, temuan)`, urut review lalu urutan
    /// simpan.
    ///
    /// # Errors
    /// [`StoreError::Internal`] bila pembacaan gagal atau JSON rusak.
    pub async fn run_findings(&self, run_id: &str) -> Result<Vec<(String, Finding)>, StoreError> {
        let rows = sqlx::query!(
            "SELECT f.review_id, f.card, f.title, f.detail, f.evidence_json
             FROM review_findings f JOIN reviews r ON r.id = f.review_id
             WHERE r.run_id = ? ORDER BY r.started_at_ms, f.id",
            run_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter()
            .map(|r| {
                Ok((
                    r.review_id,
                    Finding {
                        card: r.card,
                        title: r.title,
                        detail: r.detail,
                        evidence: evidence_of(r.evidence_json.as_deref())?.unwrap_or_default(),
                    },
                ))
            })
            .collect()
    }
}
