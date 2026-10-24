//! Klaim pemilik: tulis, baca ulang, heartbeat, lepas (KTD-8).

use std::time::Duration;

use mandor_domain::claim::{READBACK_DELAY_MS, heartbeat_due, may_claim, won};
use mandor_domain::gate::check_start;
use mandor_domain::{AppField, Claim, ClaimLoss};

use super::{SyncError, SyncService};
use crate::ports::{Clock, Deps, Ids, LarkTasks, TaskCache};

/// Hasil heartbeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heartbeat {
    /// Klaim masih baru; tidak ada yang ditulis.
    Fresh,
    Renewed,
    /// Klaim tidak berlaku lagi; alasannya menentukan state run berikutnya.
    Lost(ClaimLoss),
}

impl<D: Deps> SyncService<D> {
    /// Mengklaim record untuk Start (atau "Ambil alih" bila `takeover`).
    /// Start biasa memeriksa gerbang readiness dan status dari baca segar.
    ///
    /// # Errors
    /// Gerbang, klaim aktif dev lain, kalah klaim, atau kegagalan Lark.
    pub async fn claim(&self, record_id: &str, takeover: bool) -> Result<Claim, SyncError> {
        let fresh = self
            .fetch_one(record_id)
            .await?
            .ok_or(SyncError::NotFound)?;
        if !takeover {
            check_start(&fresh.status, fresh.readiness.as_deref())?;
        }
        let now = self.clock.now_ms();
        may_claim(fresh.claim.as_ref(), &self.identity, now, takeover)?;

        let mine = Claim {
            open_id: self.me.clone(),
            ts_ms: now,
            nonce: self.identity.nonce(&self.ids.nonce()),
        };
        self.lark
            .write(record_id, None, &[AppField::Claim(Some(mine.clone()))])
            .await?;
        self.clock
            .sleep(Duration::from_millis(READBACK_DELAY_MS))
            .await;

        let readback = self
            .fetch_one(record_id)
            .await?
            .ok_or(SyncError::NotFound)?;
        self.cache.upsert(&readback).await?;
        if won(readback.claim.as_ref(), &mine.nonce, &self.me) {
            Ok(mine)
        } else {
            Err(SyncError::ClaimLost)
        }
    }

    /// Memperbarui `ts` klaim bila sudah 5 menit; mendeteksi klaim yang hilang.
    ///
    /// # Errors
    /// Kegagalan Lark.
    pub async fn heartbeat(&self, record_id: &str) -> Result<Heartbeat, SyncError> {
        let Some(fresh) = self.fetch_one(record_id).await? else {
            return Ok(Heartbeat::Lost(ClaimLoss::Deleted));
        };
        if !fresh.assigned_to(&self.me) {
            return Ok(Heartbeat::Lost(ClaimLoss::Unassigned));
        }
        let Some(current) = fresh.claim.filter(|c| self.identity.holds(c)) else {
            return Ok(Heartbeat::Lost(ClaimLoss::TakenByOther));
        };
        let now = self.clock.now_ms();
        if !heartbeat_due(&current, now) {
            return Ok(Heartbeat::Fresh);
        }
        // Klaim lama tanpa id daemon diadopsi dengan nonce baru (KTD-55).
        let nonce = if current.daemon_id().is_some() {
            current.nonce
        } else {
            self.identity.nonce(&self.ids.nonce())
        };
        let renewed = Claim {
            ts_ms: now,
            nonce,
            ..current
        };
        self.lark
            .write(record_id, None, &[AppField::Claim(Some(renewed))])
            .await?;
        Ok(Heartbeat::Renewed)
    }

    /// Melepas klaim bila masih milik kita (rollback Start, bersihkan orphan).
    ///
    /// # Errors
    /// Kegagalan Lark.
    pub async fn release_claim(&self, record_id: &str) -> Result<(), SyncError> {
        let Some(fresh) = self.fetch_one(record_id).await? else {
            return Ok(());
        };
        if fresh.held_here(&self.identity) {
            let t = self
                .lark
                .write(record_id, None, &[AppField::Claim(None)])
                .await?;
            self.cache.upsert(&t).await?;
        }
        Ok(())
    }
}
