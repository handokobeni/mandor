//! Proses app web milik run (KTD-36): `sh -c '<perintah>; :' mandor-app-<run>`
//! di process group sendiri, log stdout/stderr di ring buffer yang sudah
//! diredaksi, dan probe port TCP untuk kesiapan.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::task::JoinSet;

use crate::redact::redact;
use crate::{Cmd, EnvPolicy, Output, ProcError, StopOutcome, Supervised, run};

/// Batas log per repo.
pub const APP_LOG_MAX: usize = 256 * 1024;
/// Jeda penghentian bertahap app.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// Batas waktu satu probe port.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// Penanda di command line pemimpin group, dipakai `reap_group` saat restart.
#[must_use]
pub fn app_marker(run_id: &str) -> String {
    format!("mandor-app-{run_id}")
}

/// Proses app yang akan dijalankan.
#[derive(Debug, Clone)]
pub struct AppSpec {
    /// Penanda dari [`app_marker`].
    pub marker: String,
    pub cwd: PathBuf,
    pub command: String,
    /// Environment lengkap proses app (basis bersih + kunci `.env` terpilih).
    pub env: Vec<(String, String)>,
}

/// Log app dengan batas ukuran; baris terlama dibuang lebih dulu.
#[derive(Debug, Default)]
struct LogRing {
    lines: VecDeque<String>,
    bytes: usize,
}

impl LogRing {
    fn push(&mut self, line: String) {
        self.bytes += line.len() + 1;
        self.lines.push_back(line);
        while self.bytes > APP_LOG_MAX {
            match self.lines.pop_front() {
                Some(old) => self.bytes -= old.len() + 1,
                None => break,
            }
        }
    }

    fn text(&self) -> String {
        let mut out = String::with_capacity(self.bytes);
        for l in &self.lines {
            out.push_str(l);
            out.push('\n');
        }
        out
    }
}

type SharedLog = Arc<Mutex<LogRing>>;

/// Mencatat satu baris ke log, sudah diredaksi.
fn record(log: &SharedLog, line: &str) {
    let clean = redact(line).into_owned();
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(clean);
}

/// Alasan app tidak siap.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppNotReady {
    #[error("the app exited (code {0:?}) before opening its port")]
    Exited(Option<i32>),
    #[error("the app did not open port {0} in time")]
    Timeout(u16),
}

/// Proses app yang menyala, dengan pembaca log miliknya.
#[derive(Debug)]
pub struct AppProcess {
    child: Supervised,
    log: SharedLog,
    readers: JoinSet<()>,
}

impl AppProcess {
    /// Menjalankan perintah start di process group sendiri.
    ///
    /// # Errors
    /// [`ProcError::Spawn`] bila `sh` tidak bisa dijalankan.
    pub fn spawn(spec: &AppSpec) -> Result<Self, ProcError> {
        // `; :` mencegah `exec` oleh shell, jadi penanda tetap ada di command
        // line pemimpin group.
        let script = format!("{}; :", spec.command);
        let cmd = Cmd::new("sh")
            .args(["-c", script.as_str(), spec.marker.as_str()])
            .cwd(&spec.cwd)
            .env_policy(EnvPolicy::Explicit(spec.env.clone()));
        let mut child = Supervised::spawn(&cmd)?;
        let log = SharedLog::default();
        let mut readers = JoinSet::new();
        if let Some(out) = child.take_stdout() {
            readers.spawn(read_lines(out, Arc::clone(&log)));
        }
        if let Some(err) = child.take_stderr() {
            readers.spawn(read_lines(err, Arc::clone(&log)));
        }
        Ok(Self {
            child,
            log,
            readers,
        })
    }

    /// PID pemimpin group.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.pid()
    }

    /// Log app saat ini.
    #[must_use]
    pub fn log(&self) -> String {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .text()
    }

    /// Menunggu port app terbuka: probe setiap `interval` sampai `timeout`.
    ///
    /// # Errors
    /// [`AppNotReady`] bila proses keluar atau batas waktu habis.
    pub async fn wait_ready(
        &mut self,
        port: u16,
        timeout: Duration,
        interval: Duration,
    ) -> Result<(), AppNotReady> {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        loop {
            if port_open(port).await {
                return Ok(());
            }
            tokio::select! {
                code = self.child.wait() => {
                    return Err(AppNotReady::Exited(code.ok().flatten()));
                }
                () = tokio::time::sleep(interval) => {}
                () = &mut deadline => return Err(AppNotReady::Timeout(port)),
            }
        }
    }

    /// Menghentikan seluruh group app dan menunggu pembaca log selesai.
    pub async fn stop(&mut self) -> StopOutcome {
        let outcome = self.child.stop(STOP_GRACE).await;
        while self.readers.join_next().await.is_some() {}
        outcome
    }
}

async fn read_lines(stream: impl AsyncRead + Unpin, log: SharedLog) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        record(&log, &line);
    }
}

/// Port `127.0.0.1:<port>` menerima koneksi.
pub async fn port_open(port: u16) -> bool {
    let connect = tokio::net::TcpStream::connect(("127.0.0.1", port));
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, connect).await,
        Ok(Ok(_))
    )
}

/// Menjalankan perintah setup (mis. `bun install`) sampai selesai.
///
/// # Errors
/// [`ProcError`] bila proses gagal dijalankan atau melewati batas waktu.
pub async fn run_setup(
    cwd: PathBuf,
    command: &str,
    env: Vec<(String, String)>,
    timeout: Duration,
) -> Result<Output, ProcError> {
    run(&Cmd::new("sh")
        .args(["-c", command])
        .cwd(cwd)
        .env_policy(EnvPolicy::Explicit(env))
        .timeout(timeout))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_membuang_baris_terlama_saat_penuh() {
        let mut ring = LogRing::default();
        let line = "x".repeat(1024);
        for _ in 0..300 {
            ring.push(line.clone());
        }
        assert!(ring.bytes <= APP_LOG_MAX);
        assert_eq!(ring.lines.len(), APP_LOG_MAX / 1025);
        assert_eq!(app_marker("r1"), "mandor-app-r1");
    }
}
