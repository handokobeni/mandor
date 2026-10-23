//! Proses berumur panjang (agent) dengan penghentian bertahap:
//! SIGINT → tunggu → SIGTERM → tunggu → SIGKILL ke seluruh group (KTD-17).

use std::time::Duration;

use process_wrap::tokio::ChildWrapper;
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};

use crate::cmd::{Cmd, ProcError, kill_group};

#[cfg(unix)]
const SIGINT: i32 = 2;
#[cfg(unix)]
const SIGTERM: i32 = 15;

/// Cara proses berakhir saat dihentikan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    /// Sudah selesai sebelum dihentikan.
    AlreadyExited,
    /// Berhenti setelah SIGINT.
    Interrupted,
    /// Berhenti setelah SIGTERM.
    Terminated,
    /// Harus dibunuh paksa.
    Killed,
}

/// Proses anak yang diawasi beserta pipe-nya.
#[derive(Debug)]
pub struct Supervised {
    child: Box<dyn ChildWrapper>,
    program: String,
}

impl Supervised {
    /// Menjalankan proses tanpa batas waktu total; pipe stdout/stderr diambil
    /// pemanggil lewat [`Self::take_stdout`] dan [`Self::take_stderr`].
    ///
    /// # Errors
    /// [`ProcError::Spawn`] bila program tidak bisa dijalankan.
    pub fn spawn(cmd: &Cmd) -> Result<Self, ProcError> {
        let program = cmd.program_name();
        let child = cmd.build().spawn().map_err(|source| ProcError::Spawn {
            program: program.clone(),
            source,
        })?;
        Ok(Self { child, program })
    }

    /// PID proses (pemimpin group).
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Mengambil stdin (sekali).
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin().take()
    }

    /// Mengambil stdout (sekali).
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout().take()
    }

    /// Mengambil stderr (sekali).
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr().take()
    }

    /// Menunggu proses selesai dan mengembalikan kode keluarnya.
    ///
    /// # Errors
    /// [`ProcError::Io`] bila menunggu gagal.
    pub async fn wait(&mut self) -> Result<Option<i32>, ProcError> {
        let status = self.child.wait().await.map_err(|source| ProcError::Io {
            program: self.program.clone(),
            source,
        })?;
        Ok(status.code())
    }

    /// Proses sudah selesai (tanpa menunggu).
    #[must_use]
    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// Menghentikan proses secara bertahap, masing-masing tahap menunggu `grace`.
    pub async fn stop(&mut self, grace: Duration) -> StopOutcome {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return StopOutcome::AlreadyExited;
        }
        #[cfg(unix)]
        {
            for (sig, outcome) in [
                (SIGINT, StopOutcome::Interrupted),
                (SIGTERM, StopOutcome::Terminated),
            ] {
                if self.child.signal(sig).is_err() {
                    break;
                }
                if tokio::time::timeout(grace, self.child.wait()).await.is_ok() {
                    return outcome;
                }
            }
        }
        #[cfg(not(unix))]
        let _ = grace;
        kill_group(self.child.as_mut()).await;
        StopOutcome::Killed
    }
}
