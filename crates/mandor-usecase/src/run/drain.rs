//! Gerbang drain (KTD-52, plan service OS): sebelum daemon di-restart untuk
//! pembaruan, `drain` memastikan tidak ada run yang bekerja lalu menolak semua
//! kerja baru sampai daemon berhenti, `undrain`, atau jendela habis.
//!
//! Kerja baru masuk lewat [`DrainGate::try_enter`] sebelum mengubah state apa
//! pun dan memegang [`WorkGuard`] sampai state barunya tersimpan. Drain
//! memeriksa penghitung itu di bawah lock yang sama dengan penyetelan jendela,
//! sehingga tidak ada kerja yang lolos di antara cek dan restart.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mandor_domain::run::any_working;

use super::RunError;
use crate::ports::PortError;
use crate::run_ports::RunStore;

/// Lama jendela drain. Installer harus menyelesaikan salin dan restart di
/// dalamnya; bila tidak, kerja baru kembali diterima.
pub const DRAIN_WINDOW_MS: i64 = 2 * 60 * 1000;

/// Hasil `daemon.drain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainOutcome {
    /// Tidak ada run yang bekerja; kerja baru ditolak selama jendela.
    Idle,
    /// Ada run yang bekerja; drain dibatalkan dan kerja baru tetap diterima.
    Busy,
}

#[derive(Debug, Default)]
struct State {
    draining_until: Option<i64>,
    in_flight: u32,
}

impl State {
    fn draining(&self, now_ms: i64) -> bool {
        self.draining_until.is_some_and(|until| now_ms < until)
    }
}

/// Gerbang drain milik daemon, dibagi ke layanan run.
#[derive(Debug, Default)]
pub struct DrainGate {
    state: Arc<Mutex<State>>,
}

/// Penanda kerja yang sedang dimulai; melepasnya menurunkan penghitung.
#[derive(Debug)]
pub struct WorkGuard {
    state: Arc<Mutex<State>>,
}

impl Drop for WorkGuard {
    fn drop(&mut self) {
        let mut s = lock(&self.state);
        s.in_flight = s.in_flight.saturating_sub(1);
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DrainGate {
    /// Mendaftarkan kerja baru; ditolak selama jendela drain aktif.
    ///
    /// # Errors
    /// [`RunError::Draining`] bila daemon sedang bersiap untuk pembaruan.
    pub fn try_enter(&self, now_ms: i64) -> Result<WorkGuard, RunError> {
        let mut s = lock(&self.state);
        if s.draining(now_ms) {
            return Err(RunError::Draining);
        }
        s.in_flight = s.in_flight.saturating_add(1);
        Ok(WorkGuard {
            state: Arc::clone(&self.state),
        })
    }

    /// Memulai drain. Jendela yang masih aktif tidak diperpanjang. Bila ada
    /// kerja yang sedang dimulai atau run yang bekerja di penyimpanan, drain
    /// dibatalkan dan hasilnya [`DrainOutcome::Busy`].
    ///
    /// # Errors
    /// Penyimpanan run gagal dibaca; drain dibatalkan.
    pub async fn drain(
        &self,
        runs: &impl RunStore,
        now_ms: i64,
    ) -> Result<DrainOutcome, PortError> {
        {
            let mut s = lock(&self.state);
            if s.draining(now_ms) {
                return Ok(DrainOutcome::Idle);
            }
            if s.in_flight > 0 {
                return Ok(DrainOutcome::Busy);
            }
            s.draining_until = Some(now_ms.saturating_add(DRAIN_WINDOW_MS));
        }
        let working = match runs.list_active().await {
            Ok(active) => any_working(active.iter().map(|r| r.state)),
            Err(e) => {
                self.undrain();
                return Err(e);
            }
        };
        if working {
            self.undrain();
            return Ok(DrainOutcome::Busy);
        }
        Ok(DrainOutcome::Idle)
    }

    /// Membatalkan drain; kerja baru kembali diterima.
    pub fn undrain(&self) {
        lock(&self.state).draining_until = None;
    }
}
