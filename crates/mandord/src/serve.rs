//! Menjalankan daemon: lock single-instance, store, server, dan shutdown bersih.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use mandor_api::{ServerConfig, serve};
use mandor_protocol::{Notification, PROTOCOL_VERSION};
use mandor_store::Store;
use secrecy::SecretString;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::VERSION;
use crate::app::App;
use crate::config::Config;
use crate::discovery::{self, DaemonInfo};
use crate::dispatch::AppDispatch;

/// Batas menunggu task berhenti saat shutdown.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(30);

/// Kapasitas kanal event ke client.
const EVENT_CAPACITY: usize = 1024;

/// Menjalankan daemon sampai SIGINT/SIGTERM.
///
/// # Errors
/// Gagal bila instance lain sudah berjalan, atau store/server tidak bisa dibuka.
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let boot = Instant::now();
    discovery::ensure_home(&cfg.home).await?;
    init_logging(&cfg)?;

    let mut lock = fd_lock::RwLock::new(open_lock_file(&cfg)?);
    let _guard = lock
        .try_write()
        .context("another mandord is already running for this data directory")?;

    let store = Store::open(&cfg.db_path())
        .await
        .context("opening the database")?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("opening a local port")?;
    let port = listener.local_addr().context("alamat lokal")?.port();
    let token = new_token();
    let pid = std::process::id();
    publish(&cfg, pid, port, &token).await?;

    let shutdown = CancellationToken::new();
    let tracker = TaskTracker::new();
    let (events, _) = broadcast::channel::<Notification>(EVENT_CAPACITY);
    let server_cfg = ServerConfig {
        token: SecretString::from(token),
        daemon_version: VERSION.to_owned(),
    };
    let app = Arc::new(App::new(
        cfg.clone(),
        store,
        events.clone(),
        tracker.clone(),
        shutdown.clone(),
    ));
    let booting = Arc::clone(&app);
    tracker.spawn(async move { booting.boot().await });
    let app_for_shutdown = Arc::clone(&app);
    let dispatch = Arc::new(AppDispatch::new(app));
    let server = tracker.spawn(serve(
        listener,
        server_cfg,
        dispatch,
        events,
        shutdown.clone(),
    ));
    tracker.close();
    tracing::info!(port, pid, ready_ms = boot.elapsed().as_millis(), "ready");

    wait_for_signal().await;
    tracing::info!("menghentikan daemon");
    stop_all(&app_for_shutdown, &shutdown, &tracker).await;
    log_server_exit(server.await);
    discovery::remove_if_owned(&cfg.home, pid).await;
    Ok(())
}

/// Menghentikan sesi, agent, dan semua task dengan batas waktu.
async fn stop_all(app: &App, shutdown: &CancellationToken, tracker: &TaskTracker) {
    shutdown.cancel();
    if let Some(session) = app.session().await {
        session.runs.shutdown_agents().await;
    }
    if tokio::time::timeout(SHUTDOWN_GRACE, tracker.wait())
        .await
        .is_err()
    {
        tracing::warn!("sebagian task belum selesai saat daemon berhenti");
    }
}

fn open_lock_file(cfg: &Config) -> anyhow::Result<std::fs::File> {
    let lock_path = cfg.home.join("daemon.lock");
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("membuka {}", lock_path.display()))
}

async fn publish(cfg: &Config, pid: u32, port: u16, token: &str) -> anyhow::Result<()> {
    discovery::write(
        &cfg.home,
        &DaemonInfo {
            pid,
            port,
            token: token.to_owned(),
            protocol: PROTOCOL_VERSION.to_owned(),
            version: VERSION.to_owned(),
            started_at_ms: now_ms(),
        },
    )
    .await
}

fn log_server_exit(result: Result<std::io::Result<()>, tokio::task::JoinError>) {
    match result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => tracing::error!(error = %err, "server berhenti dengan galat"),
        Err(err) => tracing::error!(error = %err, "task server gagal"),
    }
}

fn init_logging(cfg: &Config) -> anyhow::Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(cfg.log_path())
        .with_context(|| format!("membuka {}", cfg.log_path().display()))?;
    let _ = tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&cfg.log_filter))
        .with_writer(std::sync::Mutex::new(file))
        .try_init();
    Ok(())
}

fn new_token() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = term.recv() => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "SIGTERM tidak bisa dipasang; hanya ctrl-c");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
