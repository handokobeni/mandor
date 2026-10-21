//! Hasil poll Lark terakhir untuk indikator sinkron di papan. Hanya di memori:
//! setelah restart terisi lagi oleh poll berikutnya.

use std::sync::{Mutex, PoisonError};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncSnapshot {
    /// Poll terakhir yang berhasil (ms epoch).
    pub synced_at_ms: Option<i64>,
    /// Error poll terakhir; dikosongkan oleh poll yang berhasil.
    pub error: Option<String>,
}

#[derive(Debug, Default)]
pub struct SyncStatus(Mutex<SyncSnapshot>);

impl SyncStatus {
    pub fn ok(&self, now_ms: i64) {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        s.synced_at_ms = Some(now_ms);
        s.error = None;
    }

    /// Waktu sinkron terakhir yang berhasil tetap dipertahankan.
    pub fn failed(&self, error: String) {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).error = Some(error);
    }

    #[must_use]
    pub fn snapshot(&self) -> SyncSnapshot {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gagal_menyimpan_waktu_berhasil_terakhir_dan_berhasil_menghapus_error() {
        let s = SyncStatus::default();
        assert_eq!(s.snapshot(), SyncSnapshot::default());
        s.ok(10);
        s.failed("Lark returned 503".to_owned());
        assert_eq!(
            s.snapshot(),
            SyncSnapshot {
                synced_at_ms: Some(10),
                error: Some("Lark returned 503".to_owned())
            }
        );
        s.ok(20);
        assert_eq!(
            s.snapshot(),
            SyncSnapshot {
                synced_at_ms: Some(20),
                error: None
            }
        );
    }
}
