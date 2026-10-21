//! Outbox tulis Lark dengan validasi ulang (KTD-7).

use std::time::Duration;

use mandor_domain::task::{check_enqueue, revalidate};
use mandor_domain::{CardFlag, FlagKind, WriteOp};

use super::{SyncError, SyncService};
use crate::ports::{Clock, Deps, LarkTasks, Outbox, PortError, TaskCache};

/// Jeda minimal antar tulis karena Bitable menolak tulis serentak.
pub const WRITE_SPACING: Duration = Duration::from_millis(600);
/// Percobaan sebelum kartu ditandai gagal sinkron.
pub const MAX_ATTEMPTS: u32 = 3;
/// Jeda dasar retry; berlipat dua tiap percobaan.
const RETRY_BASE_MS: i64 = 2_000;

/// Ringkasan satu putaran pengurasan outbox.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrainReport {
    pub written: Vec<String>,
    pub cancelled: Vec<String>,
    pub failed: Vec<String>,
    pub retrying: Vec<String>,
}

impl<D: Deps> SyncService<D> {
    /// Mengantrekan tulisan. Transisi di luar allowlist ditolak di tempat.
    ///
    /// # Errors
    /// [`SyncError::Rejected`] untuk transisi terlarang, atau error penyimpanan.
    pub async fn request_write(&self, record_id: &str, op: &WriteOp) -> Result<i64, SyncError> {
        check_enqueue(op)?;
        Ok(self
            .outbox
            .enqueue(record_id, op, self.clock.now_ms())
            .await?)
    }

    /// Menjalankan tulisan yang jatuh tempo, satu per satu dengan jeda.
    ///
    /// # Errors
    /// [`SyncError::Port`] dengan [`PortError::Reauth`] (pengurasan berhenti tanpa
    /// menghitung percobaan) atau bila penyimpanan gagal.
    pub async fn drain_outbox(&self, limit: u32) -> Result<DrainReport, SyncError> {
        let items = self.outbox.due(self.clock.now_ms(), limit).await?;
        let mut report = DrainReport::default();
        for (i, item) in items.into_iter().enumerate() {
            if i > 0 {
                self.clock.sleep(WRITE_SPACING).await;
            }
            match self.write_one(&item.record_id, &item.op).await {
                Ok(()) => {
                    self.outbox.complete(item.id).await?;
                    self.cache.set_flag(&item.record_id, None).await?;
                    report.written.push(item.record_id);
                }
                Err(SyncError::Rejected(reason)) => {
                    let flag = CardFlag::from(&reason);
                    self.outbox.cancel(item.id, &flag.detail).await?;
                    self.cache.set_flag(&item.record_id, Some(&flag)).await?;
                    report.cancelled.push(item.record_id);
                }
                Err(SyncError::Port(PortError::Reauth)) => {
                    return Err(SyncError::Port(PortError::Reauth));
                }
                Err(e) => {
                    let attempts = item.attempts + 1;
                    let msg = e.to_string();
                    if attempts >= MAX_ATTEMPTS {
                        self.outbox.fail(item.id, &msg).await?;
                        let flag = CardFlag {
                            kind: FlagKind::SyncFailed,
                            detail: msg,
                        };
                        self.cache.set_flag(&item.record_id, Some(&flag)).await?;
                        report.failed.push(item.record_id);
                    } else {
                        let next = self.clock.now_ms() + RETRY_BASE_MS * (1 << attempts);
                        self.outbox
                            .retry_later(item.id, attempts, next, &msg)
                            .await?;
                        report.retrying.push(item.record_id);
                    }
                }
            }
        }
        Ok(report)
    }

    /// Aksi "coba lagi" pada kartu gagal sinkron. Tulisan tetap divalidasi ulang.
    ///
    /// # Errors
    /// [`SyncError::Port`] bila penyimpanan gagal.
    pub async fn retry_failed(&self, record_id: &str) -> Result<u32, SyncError> {
        let n = self
            .outbox
            .requeue_failed(record_id, self.clock.now_ms())
            .await?;
        self.cache.set_flag(record_id, None).await?;
        Ok(n)
    }

    async fn write_one(&self, record_id: &str, op: &WriteOp) -> Result<(), SyncError> {
        let fresh = self.fetch_one(record_id).await?;
        revalidate(fresh.as_ref(), op, &self.me)?;
        let (status, fields) = match op {
            WriteOp::Transition { to, fields, .. } => (Some(to), fields.as_slice()),
            WriteOp::Fields(fields) => (None, fields.as_slice()),
        };
        let written = self.lark.write(record_id, status, fields).await?;
        // Respons update hanya memuat field yang ditulis; field lain (key, judul,
        // assignee) diambil dari baca segar sebelum tulis.
        let merged = match fresh {
            Some(mut task) => {
                task.apply_write(status, fields);
                task.last_modified_ms = task.last_modified_ms.max(written.last_modified_ms);
                task
            }
            None => written,
        };
        self.cache.upsert(&merged).await?;
        Ok(())
    }
}
