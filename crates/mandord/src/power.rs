//! Pemantau keep-awake (KTD-45 plan service OS): selama ada run yang bekerja,
//! daemon menjalankan satu penjaga keep-awake dari `MANDOR_KEEP_AWAKE_CMD`
//! dan memegang stdin-nya. Penjaga dihentikan dengan menutup stdin setelah
//! masa tenggang, atau saat daemon berhenti. Di Windows + WSL penjaganya
//! adalah `mandor-keepawake.exe --keep-awake` lewat interop.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use mandor_domain::power::{Change, DEFAULT_GRACE_MS, KeepAwake, KeepAwakeState, Respawn};
use mandor_proc::{Cmd, Supervised};
use mandor_store::{RunsStore, Store};
use mandor_usecase::run_ports::RunStore as _;
use tokio::process::ChildStdin;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Selang pemeriksaan `busy`.
const TICK: Duration = Duration::from_secs(15);
/// Penjaga yang hidup selama ini dianggap stabil; hitungan gagal direset.
const STABLE_MS: i64 = 60_000;
/// Waktu penjaga keluar sendiri setelah stdin ditutup.
const EOF_GRACE: Duration = Duration::from_secs(3);
/// Jeda tiap tahap penghentian paksa.
const STOP_GRACE: Duration = Duration::from_secs(1);

/// Status keep-awake untuk RPC `status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub state: KeepAwakeState,
    pub error: Option<String>,
}

/// Status yang dibagi antara pemantau dan dispatch.
#[derive(Debug, Clone)]
pub struct PowerStatus(Arc<Mutex<Snapshot>>);

impl PowerStatus {
    fn new(state: KeepAwakeState, error: Option<String>) -> Self {
        Self(Arc::new(Mutex::new(Snapshot { state, error })))
    }

    #[must_use]
    pub fn get(&self) -> Snapshot {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, state: KeepAwakeState, error: Option<String>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Snapshot { state, error };
    }
}

/// Penjaga yang berjalan: proses dan stdin yang dipegang daemon.
struct Guard {
    proc: Supervised,
    stdin: Option<ChildStdin>,
    started_ms: i64,
}

impl Guard {
    async fn stop(mut self) {
        // EOF dulu agar penjaga melepas keep-awake dengan bersih.
        drop(self.stdin.take());
        if tokio::time::timeout(EOF_GRACE, self.proc.wait())
            .await
            .is_err()
        {
            self.proc.stop(STOP_GRACE).await;
        }
    }
}

/// Keputusan dan proses penjaga; satu langkah per pemeriksaan.
pub struct Monitor {
    argv: Option<Result<Vec<String>, String>>,
    grace_ms: i64,
    decide: KeepAwake,
    respawn: Respawn,
    guard: Option<Guard>,
    last_error: Option<String>,
    status: PowerStatus,
}

impl Monitor {
    #[must_use]
    pub fn new(argv: Option<Result<Vec<String>, String>>, grace_ms: i64) -> Self {
        let status = match &argv {
            None => PowerStatus::new(KeepAwakeState::NotConfigured, None),
            Some(Err(e)) => PowerStatus::new(KeepAwakeState::Failing, Some(e.clone())),
            Some(Ok(_)) => PowerStatus::new(KeepAwakeState::Off, None),
        };
        Self {
            argv,
            grace_ms,
            decide: KeepAwake::default(),
            respawn: Respawn::default(),
            guard: None,
            last_error: None,
            status,
        }
    }

    #[must_use]
    pub fn status(&self) -> PowerStatus {
        self.status.clone()
    }

    /// Satu pemeriksaan: `busy` saat ini pada `now_ms`.
    pub async fn step(&mut self, busy: bool, now_ms: i64) {
        let Some(Ok(argv)) = self.argv.clone() else {
            return;
        };
        if self.decide.observe(busy, now_ms, self.grace_ms) == Change::Release {
            self.release().await;
        }
        if !self.decide.holding() {
            self.status.set(KeepAwakeState::Off, None);
            return;
        }
        if let Some(g) = self.guard.as_mut() {
            if g.proc.has_exited() {
                self.guard = None;
                self.last_error = Some("the keep-awake helper exited".to_owned());
                self.respawn.failed(now_ms);
            } else if now_ms.saturating_sub(g.started_ms) >= STABLE_MS {
                self.respawn.succeeded();
            }
        }
        if self.guard.is_none() && self.respawn.due(now_ms) {
            match spawn_guard(&argv, now_ms) {
                Ok(g) => self.guard = Some(g),
                Err(e) => {
                    tracing::warn!(error = %e, "penjaga keep-awake gagal dijalankan");
                    self.last_error = Some(e);
                    self.respawn.failed(now_ms);
                }
            }
        }
        if self.respawn.failing() {
            self.status
                .set(KeepAwakeState::Failing, self.last_error.clone());
        } else if self.guard.is_some() {
            self.status.set(KeepAwakeState::Held, None);
        } else {
            self.status
                .set(KeepAwakeState::Off, self.last_error.clone());
        }
    }

    async fn release(&mut self) {
        if let Some(g) = self.guard.take() {
            g.stop().await;
        }
        self.respawn.succeeded();
        self.last_error = None;
    }

    /// Menghentikan penjaga saat daemon berhenti.
    pub async fn shutdown(&mut self) {
        if let Some(g) = self.guard.take() {
            g.stop().await;
        }
    }

    /// PID penjaga yang berjalan (untuk test).
    #[cfg(all(test, unix))]
    fn guard_pid(&self) -> Option<u32> {
        self.guard.as_ref().and_then(|g| g.proc.pid())
    }
}

fn spawn_guard(argv: &[String], now_ms: i64) -> Result<Guard, String> {
    let (program, rest) = argv
        .split_first()
        .ok_or_else(|| "the keep-awake command is empty".to_owned())?;
    let cmd = Cmd::new(program).args(rest).stdin_bytes(Vec::new());
    let mut proc = Supervised::spawn(&cmd).map_err(|e| e.to_string())?;
    let stdin = proc.take_stdin();
    Ok(Guard {
        proc,
        stdin,
        started_ms: now_ms,
    })
}

/// Menjalankan pemantau sebagai task milik `tracker` sampai `shutdown`.
pub fn start(
    argv: Option<Result<Vec<String>, String>>,
    store: Store,
    tracker: &TaskTracker,
    shutdown: CancellationToken,
) -> PowerStatus {
    let mut monitor = Monitor::new(argv, DEFAULT_GRACE_MS);
    let status = monitor.status();
    tracker.spawn(async move {
        let runs = RunsStore::new(store);
        let mut busy = false;
        loop {
            match runs.list_active().await {
                Ok(active) => {
                    busy = mandor_domain::run::any_working(active.iter().map(|r| r.state));
                }
                Err(e) => tracing::warn!(error = %e, "keep-awake: gagal membaca run"),
            }
            monitor.step(busy, crate::prod::now_ms()).await;
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = tokio::time::sleep(TICK) => {}
            }
        }
        monitor.shutdown().await;
    });
    status
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const G: i64 = 60_000;

    /// Proses masih ada (Linux dan macOS; `/proc` tidak ada di macOS).
    fn alive(pid: u32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Penjaga tiruan: menahan sampai stdin EOF, seperti `--keep-awake`.
    fn holder() -> Vec<String> {
        vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "cat > /dev/null".to_owned(),
        ]
    }

    #[tokio::test]
    async fn busy_menjalankan_penjaga_dan_melepas_setelah_tenggang() {
        let mut m = Monitor::new(Some(Ok(holder())), G);
        m.step(true, 0).await;
        let pid = m.guard_pid().unwrap();
        assert!(alive(pid));
        assert_eq!(m.status().get().state, KeepAwakeState::Held);

        m.step(true, 1_000).await;
        assert_eq!(m.guard_pid(), Some(pid), "tidak dijalankan ganda");

        m.step(false, 2_000).await;
        assert_eq!(
            m.status().get().state,
            KeepAwakeState::Held,
            "masih tenggang"
        );
        m.step(false, 2_000 + G).await;
        assert_eq!(m.guard_pid(), None);
        assert_eq!(m.status().get().state, KeepAwakeState::Off);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!alive(pid), "penjaga berhenti setelah stdin ditutup");
    }

    /// Bentuk penjaga Linux (KTD-L4 plan paket Linux): pembungkus penahan
    /// yang menjalankan `cat` sebagai proses anak. Pembungkus dan anaknya
    /// berhenti saat daemon menutup stdin.
    #[tokio::test]
    async fn penjaga_pembungkus_dengan_cat_anak_berhenti_saat_dilepas() {
        let wrapper = vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "/bin/cat; :".to_owned(),
        ];
        let mut m = Monitor::new(Some(Ok(wrapper)), G);
        m.step(true, 0).await;
        let pid = m.guard_pid().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let children = std::process::Command::new("pgrep")
            .args(["-P", &pid.to_string()])
            .output()
            .unwrap();
        let child: u32 = String::from_utf8_lossy(&children.stdout)
            .trim()
            .parse()
            .unwrap();
        assert!(alive(child), "cat anak berjalan");
        m.step(false, 1).await;
        m.step(false, 1 + G).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!alive(pid), "pembungkus berhenti");
        assert!(!alive(child), "cat anak berhenti");
    }

    #[tokio::test]
    async fn penjaga_yang_keluar_dijalankan_ulang_lalu_failing() {
        let argv = Some(Ok(vec![
            "/bin/sh".to_owned(),
            "-c".to_owned(),
            "exit 0".to_owned(),
        ]));
        let mut m = Monitor::new(argv, G);
        let mut now = 0;
        for _ in 0..12 {
            m.step(true, now).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            now += 6 * 60 * 1000;
        }
        let s = m.status().get();
        assert_eq!(s.state, KeepAwakeState::Failing);
        assert!(s.error.unwrap().contains("exited"));
    }

    #[tokio::test]
    async fn program_tidak_ada_menjadi_failing_dengan_pesan() {
        let argv = Some(Ok(vec!["/tidak/ada/penjaga.exe".to_owned()]));
        let mut m = Monitor::new(argv, G);
        let mut now = 0;
        for _ in 0..5 {
            m.step(true, now).await;
            now += 6 * 60 * 1000;
        }
        let s = m.status().get();
        assert_eq!(s.state, KeepAwakeState::Failing);
        assert!(s.error.is_some());
    }

    #[tokio::test]
    async fn tanpa_perintah_tidak_menjalankan_apa_pun() {
        let mut m = Monitor::new(None, G);
        m.step(true, 0).await;
        assert_eq!(m.guard_pid(), None);
        assert_eq!(m.status().get().state, KeepAwakeState::NotConfigured);
    }

    #[tokio::test]
    async fn perintah_tidak_valid_dilaporkan_failing() {
        let mut m = Monitor::new(Some(Err("rusak".to_owned())), G);
        m.step(true, 0).await;
        let s = m.status().get();
        assert_eq!(s.state, KeepAwakeState::Failing);
        assert_eq!(s.error.as_deref(), Some("rusak"));
    }

    #[tokio::test]
    async fn shutdown_menghentikan_penjaga() {
        let mut m = Monitor::new(Some(Ok(holder())), G);
        m.step(true, 0).await;
        let pid = m.guard_pid().unwrap();
        m.shutdown().await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!alive(pid));
    }
}
