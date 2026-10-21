//! Client Tauri mandor. Tidak berisi logika bisnis (KTD-1); hanya UI dan
//! jembatan ke daemon `mandord`.

mod daemon_link;
mod oauth_callback;
mod tray;

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
        Self { mode }
    }
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

/// Menjalankan aplikasi desktop.
#[must_use]
pub fn run() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cfg = ClientConfig::from_env();
    let link = Arc::new(DaemonLink::new(cfg.mode));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::clone(&link))
        .invoke_handler(tauri::generate_handler![
            daemon_token,
            daemon_open,
            daemon_send,
            oauth_login,
            tray_update
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
