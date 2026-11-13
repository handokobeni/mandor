//! Sesi lab data sebagai port usecase (plan 2026-11-10-001, U12). Transisi
//! fase memakai `UPDATE ... WHERE phase = ?` sehingga dua transisi bersamaan
//! dari fase yang sama hanya menghasilkan satu pemenang.

use std::path::PathBuf;

use mandor_domain::lab_data::SessionPhase;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{LabSessionRecord, LabSessions};

use crate::{Store, StoreError};

/// Sesi lab data di tabel `lab_sessions`.
#[derive(Debug, Clone)]
pub struct LabSessionsStore {
    store: Store,
}

impl LabSessionsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

/// Baris mentah `lab_sessions`.
struct Row {
    id: String,
    run_id: String,
    repo_id: String,
    lock_keys_json: String,
    phase: String,
    snapshot_dir: Option<String>,
    scripts_digest: String,
    fingerprints_json: Option<String>,
    error: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

fn record_of(r: Row) -> Result<LabSessionRecord, PortError> {
    let phase = SessionPhase::parse(&r.phase)
        .ok_or_else(|| PortError::Storage(format!("unknown lab session phase: {}", r.phase)))?;
    let lock_keys: Vec<String> = serde_json::from_str(&r.lock_keys_json)
        .map_err(|e| PortError::Storage(format!("lab session locks: {e}")))?;
    let fingerprints: Vec<(String, String)> = match r.fingerprints_json {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|e| PortError::Storage(format!("lab session fingerprints: {e}")))?,
        None => Vec::new(),
    };
    Ok(LabSessionRecord {
        id: r.id,
        run_id: r.run_id,
        repo_id: r.repo_id,
        lock_keys,
        phase,
        snapshot_dir: r.snapshot_dir.map(PathBuf::from),
        scripts_digest: r.scripts_digest,
        fingerprints,
        error: r.error,
        created_at_ms: r.created_at_ms,
        updated_at_ms: r.updated_at_ms,
    })
}

fn json<T: serde::Serialize>(v: &T) -> Result<String, PortError> {
    serde_json::to_string(v).map_err(|e| PortError::Storage(e.to_string()))
}

impl LabSessionsStore {
    /// Sesi yang belum `done`, terurut menurut waktu dibuat.
    async fn open_sessions(&self) -> Result<Vec<LabSessionRecord>, PortError> {
        let rows = sqlx::query_as!(
            Row,
            r#"SELECT id as "id!", run_id, repo_id, lock_keys_json, phase, snapshot_dir,
                   scripts_digest, fingerprints_json, error, created_at_ms, updated_at_ms
               FROM lab_sessions WHERE phase != 'done' ORDER BY created_at_ms, id"#
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        rows.into_iter().map(record_of).collect()
    }
}

impl LabSessions for LabSessionsStore {
    async fn create_session(&self, rec: &LabSessionRecord) -> Result<(), PortError> {
        let locks = json(&rec.lock_keys)?;
        let fingerprints = (!rec.fingerprints.is_empty())
            .then(|| json(&rec.fingerprints))
            .transpose()?;
        let snapshot = rec.snapshot_dir.as_ref().map(|p| p.display().to_string());
        let phase = rec.phase.as_str();
        sqlx::query!(
            "INSERT INTO lab_sessions (id, run_id, repo_id, lock_keys_json, phase, snapshot_dir,
                 scripts_digest, fingerprints_json, error, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rec.id,
            rec.run_id,
            rec.repo_id,
            locks,
            phase,
            snapshot,
            rec.scripts_digest,
            fingerprints,
            rec.error,
            rec.created_at_ms,
            rec.updated_at_ms
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    async fn session(&self, id: &str) -> Result<Option<LabSessionRecord>, PortError> {
        let row = sqlx::query_as!(
            Row,
            r#"SELECT id as "id!", run_id, repo_id, lock_keys_json, phase, snapshot_dir,
                   scripts_digest, fingerprints_json, error, created_at_ms, updated_at_ms
               FROM lab_sessions WHERE id = ?"#,
            id
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        row.map(record_of).transpose()
    }

    async fn transition(
        &self,
        id: &str,
        from: SessionPhase,
        to: SessionPhase,
        error: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, PortError> {
        let (to, from) = (to.as_str(), from.as_str());
        let res = sqlx::query!(
            "UPDATE lab_sessions SET phase = ?, error = ?, updated_at_ms = ?
             WHERE id = ? AND phase = ?",
            to,
            error,
            now_ms,
            id,
            from
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(res.rows_affected() == 1)
    }

    async fn set_snapshot(
        &self,
        id: &str,
        dir: &std::path::Path,
        fingerprints: &[(String, String)],
        now_ms: i64,
    ) -> Result<(), PortError> {
        let raw = json(&fingerprints)?;
        let dir = dir.display().to_string();
        sqlx::query!(
            "UPDATE lab_sessions SET snapshot_dir = ?, fingerprints_json = ?, updated_at_ms = ?
             WHERE id = ?",
            dir,
            raw,
            now_ms,
            id
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(())
    }

    async fn pending_recovery(&self) -> Result<Vec<LabSessionRecord>, PortError> {
        Ok(self
            .open_sessions()
            .await?
            .into_iter()
            .filter(|s| s.phase.needs_recovery())
            .collect())
    }

    async fn holding(&self) -> Result<Vec<LabSessionRecord>, PortError> {
        self.open_sessions().await
    }
}
