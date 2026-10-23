//! Adapter run app (KTD-36): proses app milik daemon, hidup sepanjang daemon
//! (bukan sesi Lark), dengan log per repo dan pembatalan saat dihentikan.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use mandor_domain::app_run::{LOCKFILES, parse_env, select_env};
use mandor_proc::{AppProcess, AppSpec, app_marker};
use mandor_protocol::{Notification, RunEventView, event};
use mandor_store::AppStateStore;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{AppMember, AppProcesses, AppRecord, AppRecords};
use serde_json::Value;
use tokio::sync::{Mutex, OnceCell, broadcast};
use tokio_util::sync::CancellationToken;

use crate::prod::emit;

/// Batas waktu perintah setup.
const SETUP_TIMEOUT: Duration = Duration::from_secs(600);
/// Batas waktu port app terbuka, per repo.
const READY_TIMEOUT: Duration = Duration::from_secs(180);
const READY_INTERVAL: Duration = Duration::from_secs(1);
/// Batas waktu membaca `.env` dan lockfile.
const FILE_TIMEOUT: Duration = Duration::from_secs(5);
/// Baris log terakhir yang ikut di pesan kegagalan setup.
const FAIL_TAIL_LINES: usize = 20;

type Shared = Arc<Mutex<AppProcess>>;

#[derive(Default)]
struct RunApps {
    cancel: CancellationToken,
    members: Vec<(String, Shared)>,
}

/// Proses app semua run.
pub struct ProcApps {
    shell: PathBuf,
    fallback_env: Vec<(String, String)>,
    env: OnceCell<Vec<(String, String)>>,
    runs: Mutex<HashMap<String, RunApps>>,
    /// Log terakhir per run, termasuk app yang sudah berhenti atau gagal.
    logs: std::sync::Mutex<HashMap<String, BTreeMap<String, String>>>,
}

impl std::fmt::Debug for ProcApps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcApps").finish_non_exhaustive()
    }
}

impl ProcApps {
    pub fn new(shell: PathBuf, fallback_env: Vec<(String, String)>) -> Self {
        Self {
            shell,
            fallback_env,
            env: OnceCell::new(),
            runs: Mutex::new(HashMap::new()),
            logs: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Basis env bersih dari login shell (tanpa token, SSH agent, kunci API).
    async fn base_env(&self) -> Vec<(String, String)> {
        self.env
            .get_or_init(|| async {
                let login = match mandor_proc::capture_login_env(&self.shell, FILE_TIMEOUT).await {
                    Ok(env) => env,
                    Err(e) => {
                        tracing::warn!(error = %e, "env login shell tidak tertangkap untuk app");
                        self.fallback_env.iter().cloned().collect()
                    }
                };
                mandor_agent::claude::guardrails::clean_env(&login)
            })
            .await
            .clone()
    }

    fn keep_log(&self, run_id: &str, repo_id: &str, text: String) {
        self.logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(run_id.to_owned())
            .or_default()
            .insert(repo_id.to_owned(), text);
    }

    async fn member(&self, run_id: &str, repo_id: &str) -> Option<(Shared, CancellationToken)> {
        let runs = self.runs.lock().await;
        let run = runs.get(run_id)?;
        let proc = run
            .members
            .iter()
            .find(|(r, _)| r == repo_id)
            .map(|(_, p)| Arc::clone(p))?;
        Some((proc, run.cancel.clone()))
    }

    /// Menghentikan app semua run (shutdown daemon).
    pub async fn stop_all(&self) {
        let ids: Vec<String> = self.runs.lock().await.keys().cloned().collect();
        for id in ids {
            self.stop_run(&id).await;
        }
    }

    async fn stop_run(&self, run_id: &str) {
        let Some(run) = self.runs.lock().await.remove(run_id) else {
            return;
        };
        run.cancel.cancel();
        for (repo_id, proc) in run.members {
            let mut p = proc.lock().await;
            p.stop().await;
            self.keep_log(run_id, &repo_id, p.log());
        }
    }
}

/// Kunci `.env` terpilih dari folder repo asli; nilainya tidak pernah dicatat.
async fn env_keys(repo_path: &Path, keys: &[String]) -> Vec<(String, String)> {
    if keys.is_empty() {
        return Vec::new();
    }
    let read = tokio::time::timeout(
        FILE_TIMEOUT,
        tokio::fs::read_to_string(repo_path.join(".env")),
    );
    if let Ok(Ok(text)) = read.await {
        select_env(&parse_env(&text), keys)
    } else {
        tracing::warn!(repo = %repo_path.display(), "berkas .env tidak terbaca");
        Vec::new()
    }
}

/// FNV-1a 64-bit dari isi lockfile; cukup untuk mendeteksi perubahan.
fn fnv1a(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(FAIL_TAIL_LINES)..].join("\n")
}

impl AppProcesses for ProcApps {
    async fn port_open(&self, port: u16) -> bool {
        mandor_proc::port_open(port).await
    }

    async fn lockfile_hash(&self, worktree: &Path) -> Option<String> {
        for name in LOCKFILES {
            let read = tokio::time::timeout(FILE_TIMEOUT, tokio::fs::read(worktree.join(name)));
            if let Ok(Ok(bytes)) = read.await {
                return Some(format!("{name}:{}", fnv1a(&bytes)));
            }
        }
        None
    }

    async fn setup(&self, run_id: &str, m: &AppMember) -> Result<(), PortError> {
        let Some(cmd) = m.settings.setup_command.as_deref() else {
            return Ok(());
        };
        let env = self.base_env().await;
        let out = mandor_proc::run_setup(m.worktree.clone(), cmd, env, SETUP_TIMEOUT)
            .await
            .map_err(|e| PortError::Rejected(format!("setup failed: {e}")))?;
        let log = format!("$ {cmd}\n{}{}", out.stdout, out.stderr);
        self.keep_log(run_id, &m.repo_id, log.clone());
        if out.success() {
            Ok(())
        } else {
            Err(PortError::Rejected(format!(
                "setup failed (exit {:?}):\n{}",
                out.code,
                tail(&log)
            )))
        }
    }

    async fn spawn(&self, run_id: &str, m: &AppMember) -> Result<u32, PortError> {
        let command = m
            .settings
            .start_command
            .clone()
            .ok_or_else(|| PortError::Rejected("no start command".to_owned()))?;
        let mut env = self.base_env().await;
        env.extend(env_keys(&m.repo_path, &m.settings.env_keys).await);
        let spec = AppSpec {
            marker: app_marker(run_id),
            cwd: m.worktree.clone(),
            command,
            env,
        };
        let proc = AppProcess::spawn(&spec)
            .map_err(|e| PortError::Rejected(format!("cannot start the app: {e}")))?;
        let pid = proc
            .pid()
            .ok_or_else(|| PortError::Rejected("the app exited at once".to_owned()))?;
        self.runs
            .lock()
            .await
            .entry(run_id.to_owned())
            .or_default()
            .members
            .push((m.repo_id.clone(), Arc::new(Mutex::new(proc))));
        Ok(pid)
    }

    async fn wait_ready(&self, run_id: &str, repo_id: &str, port: u16) -> Result<(), PortError> {
        let (proc, cancel) = self
            .member(run_id, repo_id)
            .await
            .ok_or_else(|| PortError::Rejected("the app was stopped".to_owned()))?;
        let mut p = proc.lock().await;
        let outcome = tokio::select! {
            r = p.wait_ready(port, READY_TIMEOUT, READY_INTERVAL) => r.map_err(|e| e.to_string()),
            () = cancel.cancelled() => Err("the app was stopped".to_owned()),
        };
        self.keep_log(run_id, repo_id, p.log());
        outcome.map_err(|reason| {
            let log = tail(&p.log());
            if log.is_empty() {
                PortError::Rejected(reason)
            } else {
                PortError::Rejected(format!("{reason}\n{log}"))
            }
        })
    }

    async fn stop(&self, run_id: &str) {
        self.stop_run(run_id).await;
    }

    async fn logs(&self, run_id: &str) -> Vec<(String, String)> {
        let live: Vec<(String, Shared)> = self
            .runs
            .lock()
            .await
            .get(run_id)
            .map(|r| r.members.clone())
            .unwrap_or_default();
        for (repo_id, proc) in live {
            if let Ok(p) = proc.try_lock() {
                self.keep_log(run_id, &repo_id, p.log());
            }
        }
        self.logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }

    async fn reap(&self, run_id: &str, pgid: u32) {
        if mandor_proc::reap_group(pgid, &app_marker(run_id)).await {
            tracing::info!(run_id, pgid, "app yatim dihentikan");
        }
    }
}

/// Port app untuk usecase: proses milik daemon dan state di store, dengan
/// event `app.updated` setiap state berubah.
#[derive(Debug, Clone)]
pub struct BroadcastApps {
    procs: Arc<ProcApps>,
    store: AppStateStore,
    events: broadcast::Sender<Notification>,
}

impl BroadcastApps {
    pub fn new(
        procs: Arc<ProcApps>,
        store: AppStateStore,
        events: broadcast::Sender<Notification>,
    ) -> Self {
        Self {
            procs,
            store,
            events,
        }
    }
}

impl AppProcesses for BroadcastApps {
    async fn port_open(&self, port: u16) -> bool {
        self.procs.port_open(port).await
    }
    async fn lockfile_hash(&self, worktree: &Path) -> Option<String> {
        self.procs.lockfile_hash(worktree).await
    }
    async fn setup(&self, run_id: &str, m: &AppMember) -> Result<(), PortError> {
        self.procs.setup(run_id, m).await
    }
    async fn spawn(&self, run_id: &str, m: &AppMember) -> Result<u32, PortError> {
        self.procs.spawn(run_id, m).await
    }
    async fn wait_ready(&self, run_id: &str, repo_id: &str, port: u16) -> Result<(), PortError> {
        self.procs.wait_ready(run_id, repo_id, port).await
    }
    async fn stop(&self, run_id: &str) {
        self.procs.stop(run_id).await;
    }
    async fn logs(&self, run_id: &str) -> Vec<(String, String)> {
        self.procs.logs(run_id).await
    }
    async fn reap(&self, run_id: &str, pgid: u32) {
        self.procs.reap(run_id, pgid).await;
    }
}

impl AppRecords for BroadcastApps {
    async fn app_record(&self, run_id: &str) -> Result<Option<AppRecord>, PortError> {
        self.store.app_record(run_id).await
    }
    async fn save_app_record(&self, rec: &AppRecord) -> Result<(), PortError> {
        self.store.save_app_record(rec).await?;
        let view = RunEventView {
            run_id: rec.run_id.clone(),
        };
        emit(
            &self.events,
            event::APP_UPDATED,
            serde_json::to_value(view).unwrap_or(Value::Null),
        );
        Ok(())
    }
    async fn app_owner(&self) -> Result<Option<String>, PortError> {
        self.store.app_owner().await
    }
    async fn set_app_owner(&self, run_id: Option<&str>) -> Result<(), PortError> {
        self.store.set_app_owner(run_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_stabil_dan_tail_membatasi_baris() {
        assert_eq!(fnv1a(b"abc"), fnv1a(b"abc"));
        assert_ne!(fnv1a(b"abc"), fnv1a(b"abd"));
        let text = (0..50)
            .map(|i| format!("baris {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let t = tail(&text);
        assert_eq!(t.lines().count(), FAIL_TAIL_LINES);
        assert!(t.ends_with("baris 49"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_menghentikan_semua_app_dan_log_tetap_ada() {
        use mandor_domain::app_run::{AppSettings, TestMode};
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let dir = tempfile::tempdir().unwrap();
        let path = std::env::var("PATH").unwrap();
        let procs = ProcApps::new("/bin/sh".into(), vec![("PATH".to_owned(), path)]);
        let member = AppMember {
            repo_id: "web".to_owned(),
            worktree: dir.path().to_path_buf(),
            repo_path: dir.path().to_path_buf(),
            settings: AppSettings {
                test_mode: TestMode::Web,
                start_command: Some(format!(
                    "echo mulai; python3 -c \"import socket,time;s=socket.socket();\
                     s.bind(('127.0.0.1',{port}));s.listen();time.sleep(60)\""
                )),
                port: Some(port),
                ..AppSettings::default()
            },
        };
        procs.spawn("r1", &member).await.unwrap();
        procs.wait_ready("r1", "web", port).await.unwrap();
        assert!(procs.port_open(port).await);
        procs.stop_all().await;
        assert!(!procs.port_open(port).await);
        let logs = procs.logs("r1").await;
        assert!(logs[0].1.contains("mulai"), "{logs:?}");
        assert!(procs.wait_ready("r1", "web", port).await.is_err());
    }

    #[tokio::test]
    async fn env_hanya_kunci_terpilih_dari_berkas_repo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "API_URL=http://x\nSECRET=rahasia\n",
        )
        .unwrap();
        let env = env_keys(dir.path(), &["API_URL".to_owned()]).await;
        assert_eq!(env, [("API_URL".to_owned(), "http://x".to_owned())]);
        assert!(env_keys(dir.path(), &[]).await.is_empty());
        assert!(
            env_keys(&dir.path().join("x"), &["A".to_owned()])
                .await
                .is_empty()
        );
    }
}
