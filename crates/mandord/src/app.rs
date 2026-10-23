//! State daemon: pengaturan Lark, login OAuth, dan siklus hidup sesi.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use anyhow::Context;
use mandor_lark::{FileTokenStore, Pkce, authorize_url, new_state};
use mandor_protocol::{LarkSettingsInput, LarkStatus, LoginBegin, Notification, event};
use mandor_store::Store;
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::config::Config;
use crate::prod::now_ms;
use crate::session::{self, LarkSession, SessionEnv};
use crate::settings::{self, LarkSettings};

/// Login yang sedang menunggu callback.
struct PendingLogin {
    state: String,
    pkce: Pkce,
}

/// State bersama daemon.
pub struct App {
    pub cfg: Config,
    pub store: Store,
    pub events: broadcast::Sender<Notification>,
    pub tracker: TaskTracker,
    pub shutdown: CancellationToken,
    pub started: Instant,
    /// Port WebSocket daemon; dipakai server MCP ledger milik agent.
    pub rpc_port: u16,
    /// Proses run app; hidup sepanjang daemon, bukan sesi Lark.
    pub apps: Arc<crate::apps::ProcApps>,
    /// Gerbang drain untuk pembaruan; hidup sepanjang daemon, bukan sesi Lark.
    pub drain: Arc<mandor_usecase::run::DrainGate>,
    /// Status keep-awake dari pemantau (KTD-45).
    pub power: crate::power::PowerStatus,
    settings: RwLock<Option<LarkSettings>>,
    session: RwLock<Option<Arc<LarkSession>>>,
    /// Nama akun Lark yang login (token lama tidak menyimpannya).
    my_name: RwLock<Option<String>>,
    pending: Mutex<Option<PendingLogin>>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App").finish_non_exhaustive()
    }
}

impl App {
    pub fn new(
        cfg: Config,
        store: Store,
        events: broadcast::Sender<Notification>,
        tracker: TaskTracker,
        shutdown: CancellationToken,
        rpc_port: u16,
        power: crate::power::PowerStatus,
    ) -> Self {
        let apps = Arc::new(crate::apps::ProcApps::new(
            cfg.shell.clone(),
            cfg.base_env.clone(),
        ));
        Self {
            apps,
            drain: Arc::default(),
            power,
            cfg,
            store,
            events,
            tracker,
            shutdown,
            started: Instant::now(),
            rpc_port,
            settings: RwLock::new(None),
            session: RwLock::new(None),
            my_name: RwLock::new(None),
            pending: Mutex::new(None),
        }
    }

    fn tokens(&self) -> FileTokenStore {
        FileTokenStore::new(self.cfg.lark_token_path())
    }

    /// Memuat pengaturan dan memulai sesi bila sudah login. Kegagalan hanya
    /// dicatat agar daemon tetap siap (start tidak memblok pada layanan luar).
    pub async fn boot(&self) {
        let Some(s) = self.load_settings().await else {
            return;
        };
        *self.settings.write().await = Some(s.clone());
        let Some((me, name)) = self.stored_identity().await else {
            return;
        };
        *self.my_name.write().await = name;
        if let Err(e) = self.start_session(s, me).await {
            tracing::warn!(error = %e, "sesi Lark gagal dimulai");
        }
    }

    async fn load_settings(&self) -> Option<LarkSettings> {
        settings::load(&self.cfg.lark_settings_path())
            .await
            .inspect_err(|e| tracing::warn!(error = %e, "pengaturan Lark tidak terbaca"))
            .ok()
            .flatten()
    }

    /// `open_id` dan nama akun dari token tersimpan, bila sudah login.
    async fn stored_identity(&self) -> Option<(String, Option<String>)> {
        let tokens = self
            .tokens()
            .load()
            .await
            .inspect_err(|e| tracing::warn!(error = %e, "token Lark tidak terbaca"))
            .ok()
            .flatten()?;
        if tokens.open_id.is_none() {
            tracing::warn!("token Lark tanpa open_id; login ulang diperlukan");
        }
        Some((tokens.open_id?, tokens.name))
    }

    async fn start_session(&self, settings: LarkSettings, me: String) -> anyhow::Result<()> {
        self.stop_session().await;
        let env = SessionEnv {
            cfg: &self.cfg,
            store: &self.store,
            events: &self.events,
            tracker: &self.tracker,
            parent: &self.shutdown,
            rpc_port: self.rpc_port,
            apps: &self.apps,
            drain: &self.drain,
        };
        let s = session::start(&env, settings, me).await?;
        *self.session.write().await = Some(s);
        // Client yang memuat papan saat sesi belum ada (mis. Base baru sedang
        // disiapkan) memuat ulang sekarang.
        crate::prod::emit(&self.events, event::RESYNC, serde_json::Value::Null);
        Ok(())
    }

    async fn stop_session(&self) {
        if let Some(old) = self.session.write().await.take() {
            old.cancel.cancel();
        }
    }

    /// Sesi aktif, bila sudah login.
    pub async fn session(&self) -> Option<Arc<LarkSession>> {
        self.session.read().await.clone()
    }

    pub async fn lark_status(&self) -> LarkStatus {
        let configured = self.settings.read().await.is_some();
        let session = self.session().await;
        LarkStatus {
            configured,
            logged_in: session.is_some(),
            open_id: session.as_ref().map(|s| s.me.clone()),
            name: if session.is_some() {
                self.my_name.read().await.clone()
            } else {
                None
            },
            reauth_required: session
                .as_ref()
                .is_some_and(|s| s.reauth.load(Ordering::Relaxed)),
        }
    }

    pub async fn settings_view(&self) -> Option<mandor_protocol::LarkSettingsView> {
        self.settings.read().await.as_ref().map(LarkSettings::view)
    }

    /// Mengubah jeda tenang Waiting Fix; sesi yang berjalan langsung memakainya.
    ///
    /// # Errors
    /// Lark belum diatur, nilai di luar batas, atau berkas tidak bisa ditulis.
    pub async fn set_quiet_secs(
        &self,
        secs: i64,
    ) -> anyhow::Result<mandor_protocol::LarkSettingsView> {
        let mut guard = self.settings.write().await;
        let mut next = guard.clone().context("Set up the Lark app first")?;
        next.set_quiet_secs(secs)?;
        settings::save(&self.cfg.lark_settings_path(), &next).await?;
        let view = next.view();
        *guard = Some(next);
        drop(guard);
        if let Some(s) = self.session().await {
            s.quiet_secs.store(secs, Ordering::Relaxed);
        }
        Ok(view)
    }

    /// Menyimpan pengaturan; sesi dimulai ulang bila sudah login.
    ///
    /// # Errors
    /// Input tidak lengkap atau berkas tidak bisa ditulis.
    pub async fn set_settings(&self, input: LarkSettingsInput) -> anyhow::Result<()> {
        let merged = LarkSettings::merge(self.settings.read().await.as_ref(), input)?;
        settings::save(&self.cfg.lark_settings_path(), &merged).await?;
        *self.settings.write().await = Some(merged.clone());
        if let Some(s) = self.session().await {
            self.start_session(merged, s.me.clone()).await?;
        }
        Ok(())
    }

    /// Memulai login: URL otorisasi dengan PKCE dan `state` acak.
    ///
    /// # Errors
    /// Pengaturan Lark belum diisi.
    pub async fn login_begin(&self) -> anyhow::Result<LoginBegin> {
        let settings = self
            .settings
            .read()
            .await
            .clone()
            .context("configure the Lark App first")?;
        let client = session::lark_client(&settings)?;
        let pkce = Pkce::generate();
        let state = new_state();
        let url = authorize_url(&client, &state, &pkce);
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(PendingLogin {
            state: state.clone(),
            pkce,
        });
        Ok(LoginBegin {
            url,
            state,
            port: settings.redirect_port,
        })
    }

    /// Menukar kode dari callback, menyimpan token, lalu memulai sesi.
    ///
    /// # Errors
    /// `state` tidak cocok, tidak ada login berjalan, atau Lark menolak.
    pub async fn login_complete(&self, code: &str, state: &str) -> anyhow::Result<()> {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .context("no login in progress")?;
        anyhow::ensure!(
            pending.state == state,
            "login state mismatch; login cancelled"
        );
        let settings = self
            .settings
            .read()
            .await
            .clone()
            .context("Lark settings are missing")?;
        let client = session::lark_client(&settings)?;
        let tokens = client
            .exchange_code(code, &pending.pkce, now_ms())
            .await
            .context("exchanging the Lark login code")?;
        let me = tokens
            .open_id
            .clone()
            .context("Lark did not return an open_id")?;
        self.tokens().save(&tokens).await?;
        *self.my_name.write().await = tokens.name.clone();
        self.start_session(settings, me).await
    }

    /// Logout: hentikan sesi dan hapus token.
    ///
    /// # Errors
    /// Berkas token tidak bisa dihapus.
    pub async fn logout(&self) -> anyhow::Result<()> {
        self.stop_session().await;
        self.tokens().clear().await?;
        Ok(())
    }
}
