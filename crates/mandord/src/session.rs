//! Sesi Lark untuk pengguna yang login: layanan sinkron, run, dan loop latar.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use mandor_agent::claude::guardrails::{agent_env, prepare_agent_home};
use mandor_agent::{ClaudeAgent, ClaudeConfig, ClaudePrompts, LedgerServer};
use mandor_git::{Git, GitAdapter, GitConfig};
use mandor_lark::{FileTokenStore, LarkClient, LarkConfig, LarkGateway, TableRef};
use mandor_protocol::{Notification, SyncStatusView, event};
use mandor_store::{ReposStore, ReviewsStore, RunsStore, Store, TaskStore};
use mandor_usecase::{PortError, RunPorts, RunService, SyncError, SyncService};
use tokio::sync::{Notify, broadcast};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;
use crate::prod::{
    BroadcastReviews, BroadcastRuns, BroadcastTasks, DaemonNotifier, Prod, SystemClock,
    TrackerSpawner, UuidIds,
};
use crate::settings::LarkSettings;
use crate::sync_status::SyncStatus;

const TICK: Duration = Duration::from_secs(5);
const POLL_EVERY: Duration = Duration::from_secs(15);
const FULL_EVERY: Duration = Duration::from_secs(5 * 60);
const COMMENTS_EVERY: Duration = Duration::from_secs(20);
const PR_EVERY: Duration = Duration::from_secs(60);
/// Papan Team (kartu seluruh tim) lebih jarang dari kartu milik pengguna.
const TEAM_EVERY: Duration = Duration::from_secs(60);
const HEARTBEAT_EVERY: Duration = Duration::from_secs(5 * 60);
const OUTBOX_BATCH: u32 = 20;
const LOGIN_ENV_TIMEOUT: Duration = Duration::from_secs(10);

/// Sesi aktif.
pub struct LarkSession {
    pub me: String,
    pub sync: Arc<SyncService<Prod>>,
    pub runs: Arc<RunService<Prod>>,
    pub git: Git,
    /// Adapter agent untuk pemeriksaan ekstensi Claude in Chrome.
    pub agent: ClaudeAgent,
    /// Jeda tenang Waiting Fix (detik); bisa diubah tanpa memulai ulang sesi.
    pub quiet_secs: AtomicI64,
    pub reauth: AtomicBool,
    pub sync_status: SyncStatus,
    /// Event ke client (status sinkron).
    pub events: broadcast::Sender<Notification>,
    pub poke: Notify,
    pub cancel: CancellationToken,
}

impl std::fmt::Debug for LarkSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LarkSession")
            .field("me", &self.me)
            .finish_non_exhaustive()
    }
}

/// Dependensi untuk membangun sesi.
pub struct SessionEnv<'a> {
    pub cfg: &'a Config,
    pub store: &'a Store,
    pub events: &'a broadcast::Sender<Notification>,
    pub tracker: &'a TaskTracker,
    pub parent: &'a CancellationToken,
    /// Port WebSocket daemon untuk server MCP ledger (`mandord mcp`).
    pub rpc_port: u16,
    /// Proses run app milik daemon.
    pub apps: &'a Arc<crate::apps::ProcApps>,
    /// Gerbang drain milik daemon (KTD-52).
    pub drain: &'a Arc<mandor_usecase::run::DrainGate>,
}

/// Klien Lark dari pengaturan.
///
/// # Errors
/// Klien HTTP tidak bisa dibangun.
pub fn lark_client(settings: &LarkSettings) -> anyhow::Result<LarkClient> {
    let mut lc = LarkConfig::larksuite(
        settings.app_id.clone(),
        settings.app_secret.clone(),
        settings.redirect_uri(),
    );
    lc.open_base.clone_from(&settings.open_base);
    lc.accounts_base.clone_from(&settings.accounts_base);
    LarkClient::new(lc).context("klien Lark")
}

/// Membangun sesi dan menjalankan loop latarnya.
///
/// # Errors
/// Gagal membangun klien atau menyiapkan direktori agent.
pub async fn start(
    env: &SessionEnv<'_>,
    settings: LarkSettings,
    me: String,
) -> anyhow::Result<Arc<LarkSession>> {
    let gateway = LarkGateway::new(
        lark_client(&settings)?,
        TableRef {
            app_token: settings.app_token.clone(),
            table_id: settings.table_id.clone(),
            ac_table_id: settings.ac_table_id.clone(),
        },
        FileTokenStore::new(env.cfg.lark_token_path()),
    );
    if let Some(created) = warn_err(gateway.bootstrap().await, "bootstrap field Lark gagal") {
        if !created.is_empty() {
            tracing::info!(?created, "field milik app dibuat di Sprint Tasks");
        }
    }
    let scope = format!("{}/{}", settings.app_token, settings.table_id);
    let tasks = TaskStore::new(env.store.clone(), scope);
    let agent = build_agent(env.cfg, env.rpc_port).await?;
    let git = Git::new(GitConfig::new(env.cfg.workspaces_dir()));

    let sync = Arc::new(SyncService::<Prod>::new(
        gateway,
        BroadcastTasks::new(tasks.clone(), env.events.clone()),
        tasks,
        SystemClock,
        UuidIds,
        me.clone(),
    ));
    let runs = Arc::new(
        RunService::new(
            Arc::clone(&sync),
            RunPorts {
                git: GitAdapter::new(git.clone()),
                agent: agent.clone(),
                runs: BroadcastRuns::new(RunsStore::new(env.store.clone()), env.events.clone()),
                notifier: DaemonNotifier::new(env.store.clone(), env.events.clone()),
                spawner: TrackerSpawner::new(env.tracker.clone()),
                repos: ReposStore::new(env.store.clone()),
                prompts: ClaudePrompts,
                reviews: BroadcastReviews::new(
                    ReviewsStore::new(env.store.clone()),
                    env.events.clone(),
                ),
                apps: crate::apps::BroadcastApps::new(
                    Arc::clone(env.apps),
                    mandor_store::AppStateStore::new(env.store.clone()),
                    env.events.clone(),
                ),
            },
        )
        .with_drain_gate(Arc::clone(env.drain)),
    );
    warn_err(runs.reap_apps().await, "pembersihan app yatim gagal");
    if let Some(marked) = warn_err(runs.reconcile().await, "rekonsiliasi run gagal") {
        if !marked.is_empty() {
            tracing::info!(?marked, "run terputus ditandai gagal");
        }
    }
    let session = Arc::new(LarkSession {
        me,
        sync,
        runs,
        agent,
        git,
        quiet_secs: AtomicI64::new(settings.quiet_secs),
        reauth: AtomicBool::new(false),
        sync_status: SyncStatus::default(),
        events: env.events.clone(),
        poke: Notify::new(),
        cancel: env.parent.child_token(),
    });
    env.tracker.spawn(run_loop(Arc::clone(&session)));
    Ok(session)
}

/// Mencatat error dan mengembalikan `None`; untuk pekerjaan latar yang tidak fatal.
fn warn_err<T, E: std::fmt::Display>(r: Result<T, E>, msg: &str) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "{msg}");
            None
        }
    }
}

/// Adapter agent dengan environment tanpa secret dan config git/gh terisolasi.
async fn build_agent(cfg: &Config, rpc_port: u16) -> anyhow::Result<ClaudeAgent> {
    let login_env = login_env(cfg).await;
    let agent_home = prepare_agent_home(&cfg.home)
        .await
        .context("menyiapkan config agent")?;
    Ok(ClaudeAgent::new(ClaudeConfig {
        claude_bin: cfg.claude_bin.clone(),
        hook_command: cfg.hook_command.clone(),
        env: agent_env(&login_env, &agent_home),
        model: None,
        commit_model: "haiku".to_owned(),
        mcp_dir: cfg.home.join("agent").join("mcp"),
        review_dir: cfg.home.join("agent").join("review"),
        ledger_server: Some(LedgerServer {
            command: cfg.exe.clone(),
            port: rpc_port,
        }),
    }))
}

/// Environment login shell pengguna; fallback ke environment minimal dari config.
async fn login_env(cfg: &Config) -> BTreeMap<String, String> {
    match mandor_proc::capture_login_env(&cfg.shell, LOGIN_ENV_TIMEOUT).await {
        Ok(env) => env,
        Err(e) => {
            tracing::warn!(error = %e, "env login shell tidak tertangkap; memakai env dasar");
            cfg.base_env.iter().cloned().collect()
        }
    }
}

/// Jadwal pekerjaan berkala.
struct Due {
    last: Option<Instant>,
    every: Duration,
}

impl Due {
    fn new(every: Duration) -> Self {
        Self { last: None, every }
    }

    fn take(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|l| now.duration_since(l) < self.every)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

/// Jadwal semua pekerjaan berkala sesi.
struct Schedule {
    poll: Due,
    full: Due,
    comments: Due,
    prs: Due,
    heartbeat: Due,
    team: Due,
}

impl Schedule {
    fn new() -> Self {
        Self {
            poll: Due::new(POLL_EVERY),
            full: Due::new(FULL_EVERY),
            comments: Due::new(COMMENTS_EVERY),
            prs: Due::new(PR_EVERY),
            heartbeat: Due::new(HEARTBEAT_EVERY),
            team: Due::new(TEAM_EVERY),
        }
    }

    async fn run_due(&mut self, s: &LarkSession, poked: bool) {
        let now = Instant::now();
        let mut comments_needed = false;
        if poked || self.poll.take(now) {
            let is_full = self.full.take(now);
            comments_needed = poll_once(s, is_full).await;
            if is_full {
                // AC di tabel tertaut tidak mengubah waktu ubah task (KTD-27).
                s.runs.refresh_active_ledgers().await;
            }
        }
        if comments_needed && self.comments.take(now) {
            let report = warn_err(
                s.runs
                    .tick_waiting_fix(s.quiet_secs.load(Ordering::Relaxed))
                    .await,
                "mesin Waiting Fix gagal",
            );
            if let Some(r) = report.filter(|r| !r.started.is_empty()) {
                tracing::info!(started = ?r.started, "perbaikan Waiting Fix dimulai");
            }
        }
        if (self.team.take(now) || poked)
            && warn_err(s.sync.poll_team().await, "papan Team gagal diambil") == Some(true)
        {
            crate::prod::emit(&s.events, event::BOARD_UPDATED, serde_json::json!({}));
        }
        if self.prs.take(now) {
            warn_err(s.runs.check_prs().await, "pemeriksaan PR gagal");
        }
        if self.heartbeat.take(now) {
            warn_err(s.runs.heartbeat_all().await, "heartbeat klaim gagal");
        }
        drain(s).await;
    }
}

async fn run_loop(s: Arc<LarkSession>) {
    let mut schedule = Schedule::new();
    let mut interval = tokio::time::interval(TICK);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let poked = tokio::select! {
            () = s.cancel.cancelled() => break,
            _ = interval.tick() => false,
            () = s.poke.notified() => true,
        };
        schedule.run_due(&s, poked).await;
    }
    tracing::info!("loop sesi Lark berhenti");
}

/// Satu poll; mengembalikan `true` bila komentar perlu diambil.
async fn poll_once(s: &LarkSession, full: bool) -> bool {
    let in_fix = s.runs.fix_loop_records().await.unwrap_or_default();
    let needed = poll_and_record(s, full, &in_fix).await;
    let snap = s.sync_status.snapshot();
    let view = SyncStatusView {
        synced_at_ms: snap.synced_at_ms,
        sync_error: snap.error,
    };
    crate::prod::emit(
        &s.events,
        event::SYNC_STATUS,
        serde_json::to_value(view).unwrap_or(serde_json::Value::Null),
    );
    needed
}

async fn poll_and_record(s: &LarkSession, full: bool, in_fix: &[String]) -> bool {
    match s.sync.poll(full, in_fix).await {
        Ok(report) => {
            s.reauth.store(false, Ordering::Relaxed);
            s.sync_status.ok(crate::prod::now_ms());
            report.comments_needed
        }
        Err(SyncError::Port(PortError::Reauth)) => {
            s.reauth.store(true, Ordering::Relaxed);
            s.sync_status.failed("Lark session expired".to_owned());
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "poll Lark gagal");
            s.sync_status.failed(e.to_string());
            false
        }
    }
}

async fn drain(s: &LarkSession) {
    match s.sync.drain_outbox(OUTBOX_BATCH).await {
        Ok(r) if !r.failed.is_empty() || !r.cancelled.is_empty() => {
            tracing::info!(failed = ?r.failed, cancelled = ?r.cancelled, "outbox: tulisan dibatalkan atau gagal");
        }
        Ok(_) => {}
        Err(SyncError::Port(PortError::Reauth)) => s.reauth.store(true, Ordering::Relaxed),
        Err(e) => tracing::warn!(error = %e, "outbox gagal"),
    }
}
