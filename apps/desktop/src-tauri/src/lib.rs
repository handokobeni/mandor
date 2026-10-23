//! Client Tauri mandor. Tidak berisi logika bisnis (KTD-1); hanya UI dan
//! jembatan ke daemon `mandord`.

mod daemon_link;
pub mod keep_awake;
mod oauth_callback;
mod provision;
mod tray;
pub mod uninstall;
mod wslconfig;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use daemon_link::{DaemonLink, LinkError, LinkMode, TokenInfo};
use oauth_callback::CallbackError;
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_opener::OpenerExt;

/// Konfigurasi client, dibaca sekali saat start.
#[derive(Debug, Clone)]
struct ClientConfig {
    mode: LinkMode,
    /// `%LOCALAPPDATA%` (salinan penjaga keep-awake).
    local_app_data: Option<PathBuf>,
    /// `%UserProfile%` (berkas `.wslconfig`).
    user_profile: Option<PathBuf>,
}

impl ClientConfig {
    /// `MANDOR_LINK=wsl[:distro]` atau `local[:path]`. Default: WSL di Windows,
    /// biner `mandord` di PATH pada OS lain.
    fn from_env() -> Self {
        let raw = std::env::var("MANDOR_LINK").unwrap_or_default();
        let mode = match raw
            .split_once(':')
            .map_or((raw.as_str(), ""), |(a, b)| (a, b))
        {
            ("wsl", distro) => LinkMode::Wsl {
                distro: (!distro.is_empty()).then(|| distro.to_owned()),
            },
            ("local", path) if !path.is_empty() => LinkMode::Local {
                bin: PathBuf::from(path),
            },
            _ if cfg!(windows) => LinkMode::Wsl { distro: None },
            _ => LinkMode::Local {
                bin: PathBuf::from("mandord"),
            },
        };
        let dir = |key: &str| {
            std::env::var_os(key)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Self {
            mode,
            local_app_data: dir("LOCALAPPDATA"),
            user_profile: dir("USERPROFILE"),
        }
    }
}

/// State provisioning: satu proses pada satu waktu, hasil terakhir disimpan.
struct Provisioner {
    cfg: ClientConfig,
    last: tokio::sync::Mutex<Option<provision::ProvisionState>>,
}

/// Memasang atau memperbarui daemon di WSL lalu menyiapkan penjaga
/// keep-awake (KTD-49). Dipanggil UI sebelum tersambung dan saat mencoba
/// ulang pembaruan yang tertunda.
#[tauri::command]
async fn provision_run(
    app: AppHandle,
    link: State<'_, Arc<DaemonLink>>,
    prov: State<'_, Arc<Provisioner>>,
) -> Result<provision::ProvisionState, String> {
    let mut last = prov.last.lock().await;
    let LinkMode::Wsl { distro } = link.mode().clone() else {
        return Ok(provision::ProvisionState::NotNeeded);
    };
    let resource = app
        .path()
        .resolve("resources/mandord", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let inputs = provision::Inputs {
        distro,
        resource,
        current_exe,
        local_app_data: prov.cfg.local_app_data.clone(),
        version: app.package_info().version.to_string(),
    };
    let keep_awake = if provision::DAEMON_SHA256.len() == 64 {
        match provision::keep_awake_command(&inputs).await {
            Ok(json) => Some(json),
            Err(e) => {
                tracing::warn!(error = %e, "penjaga keep-awake tidak bisa disiapkan");
                None
            }
        }
    } else {
        None
    };
    link.set_keep_awake(keep_awake.clone());
    let state = provision::provision(&inputs, keep_awake.as_deref().unwrap_or("")).await;
    *last = Some(state.clone());
    Ok(state)
}

/// Keep-awake milik app saat daemon yang tersambung belum mendukungnya.
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "argumen command Tauri harus owned"
)]
fn power_app_hold(hold: bool, power: State<'_, keep_awake::AppHold>) {
    power.set(hold);
}

/// Status keep-alive WSL untuk UI.
#[derive(Debug, serde::Serialize)]
struct KeepAliveView {
    status: wslconfig::KeepAliveStatus,
    windows_build: Option<u32>,
}

fn wslconfig_path(prov: &Provisioner) -> Result<PathBuf, String> {
    prov.cfg
        .user_profile
        .as_ref()
        .map(|p| p.join(".wslconfig"))
        .ok_or_else(|| "USERPROFILE is not set".to_owned())
}

async fn windows_build() -> Option<u32> {
    if !cfg!(windows) {
        return None;
    }
    let mut cmd = tokio::process::Command::new("cmd");
    cmd.args(["/c", "ver"]).kill_on_drop(true);
    daemon_link::hide_console(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .ok()?
        .ok()?;
    wslconfig::windows_build(&String::from_utf8_lossy(&out.stdout))
}

#[tauri::command]
async fn wsl_keepalive_status(prov: State<'_, Arc<Provisioner>>) -> Result<KeepAliveView, String> {
    let build = windows_build().await;
    if build.is_some_and(|b| !wslconfig::supports_vm_idle_timeout(b)) {
        return Ok(KeepAliveView {
            status: wslconfig::KeepAliveStatus::Unsupported,
            windows_build: build,
        });
    }
    let path = wslconfig_path(&prov)?;
    let enabled = tokio::task::spawn_blocking(move || wslconfig::status_at(&path))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(KeepAliveView {
        status: if enabled {
            wslconfig::KeepAliveStatus::Enabled
        } else {
            wslconfig::KeepAliveStatus::Disabled
        },
        windows_build: build,
    })
}

#[tauri::command]
async fn wsl_keepalive_set(enabled: bool, prov: State<'_, Arc<Provisioner>>) -> Result<(), String> {
    let path = wslconfig_path(&prov)?;
    tokio::task::spawn_blocking(move || {
        if enabled {
            wslconfig::enable_at(&path)
        } else {
            wslconfig::disable_at(&path)
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn daemon_token(link: State<'_, Arc<DaemonLink>>) -> Result<TokenInfo, LinkError> {
    link.token().await
}

#[tauri::command]
async fn daemon_open(app: AppHandle, link: State<'_, Arc<DaemonLink>>) -> Result<(), LinkError> {
    link.open(app).await
}

#[tauri::command]
async fn daemon_send(message: String, bridge: State<'_, Arc<DaemonLink>>) -> Result<(), LinkError> {
    bridge.send(&message).await
}

/// Batas waktu pengguna menyelesaikan login Lark di browser.
const OAUTH_TIMEOUT: Duration = Duration::from_secs(300);

/// Login Lark: mengikat port callback, membuka URL otorisasi dari daemon di
/// browser, lalu mengembalikan kode untuk diteruskan ke daemon.
#[tauri::command]
async fn oauth_login(
    app: AppHandle,
    url: String,
    port: u16,
    state: String,
) -> Result<String, CallbackError> {
    let listener = oauth_callback::bind(port).await?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| CallbackError::Io(e.to_string()))?;
    oauth_callback::wait_for_code(listener, &state, OAUTH_TIMEOUT).await
}

/// Memperbarui tray dari run yang menunggu dev (dipanggil UI saat daftar run berubah).
#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    reason = "argumen command Tauri harus owned"
)]
fn tray_update(app: AppHandle, waiting: Vec<tray::WaitingRun>) -> Result<(), String> {
    tray::update(&app, &waiting).map_err(|e| e.to_string())
}

/// Mode pembersihan uninstall (`--uninstall-cleanup`), dipanggil hook NSIS.
#[must_use]
pub fn run_uninstall_cleanup() -> ExitCode {
    let cfg = ClientConfig::from_env();
    let distro = match &cfg.mode {
        LinkMode::Wsl { distro } => distro.clone(),
        LinkMode::Local { .. } => return ExitCode::SUCCESS,
    };
    uninstall::run(distro.as_deref(), cfg.user_profile.as_deref())
}

/// Menjalankan aplikasi desktop.
#[must_use]
pub fn run() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cfg = ClientConfig::from_env();
    let link = Arc::new(DaemonLink::new(cfg.mode.clone()));
    let prov = Arc::new(Provisioner {
        cfg,
        last: tokio::sync::Mutex::new(None),
    });

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::clone(&link))
        .manage(prov)
        .manage(keep_awake::AppHold::start())
        .invoke_handler(tauri::generate_handler![
            daemon_token,
            daemon_open,
            daemon_send,
            oauth_login,
            tray_update,
            provision_run,
            power_app_hold,
            wsl_keepalive_status,
            wsl_keepalive_set
        ])
        .setup(move |app| {
            tray::setup(app)?;
            let link = Arc::clone(&link);
            tauri::async_runtime::spawn(async move { link.start_keeper().await });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Menutup jendela hanya menyembunyikannya; app tetap di tray agar
            // penjaga WSL dan daemon tetap hidup (KTD-18).
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(err) => {
            tracing::error!(error = %err, "aplikasi gagal dibangun");
            return ExitCode::FAILURE;
        }
    };
    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            let link = Arc::clone(handle.state::<Arc<DaemonLink>>().inner());
            tauri::async_runtime::block_on(async move { link.shutdown().await });
        }
    });
    ExitCode::SUCCESS
}
