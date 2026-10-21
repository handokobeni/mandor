//! Lapisan backend agent dan satu run proses agent (KTD-10, KTD-17).

use std::sync::Arc;
use std::time::Duration;

use mandor_domain::AgentEvent;
use mandor_proc::{Cmd, StopOutcome, Supervised};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::AgentError;
use crate::claude::command::{SessionSpec, build};
use crate::claude::stream::parse_line;

/// Batas satu baris stream (hasil tool bisa besar).
const LINE_MAX: usize = 16 * 1024 * 1024;
/// Batas stderr yang disimpan untuk pesan gagal.
const STDERR_MAX: u64 = 64 * 1024;
/// Kapasitas antrean event per run.
const EVENT_BUFFER: usize = 1024;

/// Backend agent. Satu implementasi (Claude) di Rilis 1; Codex/Agent SDK menyusul.
pub trait AgentBackend: Send + Sync {
    fn command(&self, spec: &SessionSpec) -> Cmd;
    fn parse_line(&self, line: &str) -> Vec<AgentEvent>;
}

/// Backend `claude -p`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeBackend;

impl AgentBackend for ClaudeBackend {
    fn command(&self, spec: &SessionSpec) -> Cmd {
        build(spec)
    }

    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        parse_line(line)
    }
}

/// Event beserta generasi run asalnya, agar event run lama bisa diabaikan.
#[derive(Debug, Clone, PartialEq)]
pub struct RunEvent {
    pub generation: i64,
    pub event: AgentEvent,
}

/// Satu proses agent yang sedang berjalan (satu giliran).
#[derive(Debug)]
pub struct AgentRun {
    generation: i64,
    pid: Option<u32>,
    child: Arc<Mutex<Supervised>>,
    events: mpsc::Receiver<RunEvent>,
    pump: Option<JoinHandle<()>>,
}

impl AgentRun {
    /// Menjalankan agent dan mengirim `message` lewat stdin.
    ///
    /// # Errors
    /// [`AgentError::Spawn`] bila proses tidak bisa dijalankan.
    pub async fn start<B: AgentBackend + Clone + 'static>(
        backend: &B,
        spec: &SessionSpec,
        message: &str,
        generation: i64,
    ) -> Result<Self, AgentError> {
        let mut child = Supervised::spawn(
            &backend
                .command(spec)
                .stdin_bytes(message.as_bytes().to_vec()),
        )
        .map_err(|e| AgentError::Spawn(e.to_string()))?;
        let pid = child.pid();
        let stdout = child
            .take_stdout()
            .ok_or(AgentError::Spawn("stdout not available".to_owned()))?;
        let stderr = child.take_stderr();
        if let Some(mut stdin) = child.take_stdin() {
            // Pesan user lewat stdin; ditutup agar CLI tahu input selesai.
            if let Err(e) = stdin.write_all(message.as_bytes()).await {
                tracing::warn!(error = %e, "gagal menulis pesan ke stdin agent");
            }
            drop(stdin);
        }
        let child = Arc::new(Mutex::new(child));
        let (tx, events) = mpsc::channel(EVENT_BUFFER);
        let pump = tokio::spawn(pump(
            backend.clone(),
            BufReader::new(stdout),
            stderr,
            Arc::clone(&child),
            tx,
            generation,
        ));
        Ok(Self {
            generation,
            pid,
            child,
            events,
            pump: Some(pump),
        })
    }

    #[must_use]
    pub fn generation(&self) -> i64 {
        self.generation
    }

    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Event berikutnya; `None` setelah run selesai dan semua event terbaca.
    pub async fn next_event(&mut self) -> Option<RunEvent> {
        self.events.recv().await
    }

    /// Menghentikan run: SIGINT, SIGTERM, lalu SIGKILL ke group setelah tenggat.
    pub async fn stop(&self, grace: Duration) -> StopOutcome {
        self.child.lock().await.stop(grace).await
    }

    /// Penghenti yang bisa dipegang di luar pemilik run.
    #[must_use]
    pub fn stopper(&self) -> RunStopper {
        RunStopper {
            child: Arc::clone(&self.child),
        }
    }

    /// Menunggu pump selesai (proses keluar dan event terakhir terkirim).
    pub async fn wait(&mut self) {
        if let Some(pump) = self.pump.take() {
            let _ = pump.await;
        }
    }
}

/// Penghenti run agent: SIGINT, SIGTERM, lalu SIGKILL ke seluruh group.
#[derive(Debug, Clone)]
pub struct RunStopper {
    child: Arc<Mutex<Supervised>>,
}

impl RunStopper {
    pub async fn stop(&self, grace: Duration) -> StopOutcome {
        self.child.lock().await.stop(grace).await
    }
}

impl Drop for AgentRun {
    fn drop(&mut self) {
        // Proses ikut dibunuh lewat KillOnDrop milik `Supervised`.
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
    }
}

/// Membaca satu baris dibatasi [`LINE_MAX`]; baris yang terlalu panjang dibuang.
async fn read_line<R: AsyncBufRead + Unpin>(r: &mut R, buf: &mut Vec<u8>) -> std::io::Result<bool> {
    buf.clear();
    let mut oversized = false;
    loop {
        let available = r.fill_buf().await?;
        if available.is_empty() {
            return Ok(!buf.is_empty() || oversized);
        }
        let (chunk, done) = match available.iter().position(|b| *b == b'\n') {
            Some(i) => (&available[..=i], true),
            None => (available, false),
        };
        let len = chunk.len();
        if buf.len() + len <= LINE_MAX {
            buf.extend_from_slice(chunk);
        } else {
            oversized = true;
            buf.clear();
        }
        r.consume(len);
        if done {
            if oversized {
                tracing::warn!("baris stream agent melebihi batas, dilewati");
                buf.clear();
            }
            return Ok(true);
        }
    }
}

async fn read_capped<R: AsyncRead + Unpin>(r: R) -> String {
    let mut out = Vec::new();
    let _ = r.take(STDERR_MAX).read_to_end(&mut out).await;
    String::from_utf8_lossy(&out).into_owned()
}

async fn pump<B: AgentBackend, R: AsyncBufRead + Unpin, E: AsyncRead + Unpin + Send + 'static>(
    backend: B,
    mut stdout: R,
    stderr: Option<E>,
    child: Arc<Mutex<Supervised>>,
    tx: mpsc::Sender<RunEvent>,
    generation: i64,
) {
    let stderr_task = stderr.map(|e| tokio::spawn(read_capped(e)));
    let mut done = false;
    let mut buf = Vec::new();
    loop {
        match read_line(&mut stdout, &mut buf).await {
            Ok(true) => {
                for event in backend.parse_line(&String::from_utf8_lossy(&buf)) {
                    done |= matches!(event, AgentEvent::Done { .. });
                    let _ = tx.send(RunEvent { generation, event }).await;
                }
            }
            Ok(false) => break,
            Err(e) => {
                tracing::warn!(error = %e, "stdout agent tidak terbaca");
                break;
            }
        }
    }
    let code = child.lock().await.wait().await.ok().flatten();
    let stderr_text = match stderr_task {
        Some(t) => t.await.unwrap_or_default(),
        None => String::new(),
    };
    if !done {
        let tail: String = stderr_text
            .trim()
            .chars()
            .rev()
            .take(2_000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let message = match (code, tail.is_empty()) {
            (_, false) => format!(
                "agent stopped without a result (exit {}): {tail}",
                code.map_or("?".to_owned(), |c| c.to_string())
            ),
            (Some(c), true) => format!("agent stopped without a result (exit {c})"),
            (None, true) => "agent dihentikan sebelum selesai".to_owned(),
        };
        let _ = tx
            .send(RunEvent {
                generation,
                event: AgentEvent::Failed { message },
            })
            .await;
    }
}
