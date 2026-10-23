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
    /// `$HOME` (pemasangan daemon lokal di Linux).
    home: Option<PathBuf>,
    /// `$XDG_CURRENT_DESKTOP` (pilihan penahan keep-awake di Linux).
    desktop: Option<String>,
}

impl ClientConfig {
    fn from_env() -> Self {
        Self::from_vars(
            &|key: &str| std::env::var(key).ok(),
            cfg!(windows),
            provision::DAEMON_SHA256.len() == 64,
        )
    }

    /// `MANDOR_LINK=wsl[:distro]` atau `local[:path]`. Default: WSL di Windows;
    /// di OS lain biner terpasang di `~/.local/bin` bila app membawa daemon
    /// (paket Linux), selain itu `mandord` dari PATH.
    fn from_vars(var: &dyn Fn(&str) -> Option<String>, windows: bool, bundled: bool) -> Self {
        let raw = var("MANDOR_LINK").unwrap_or_default();
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
            _ if windows => LinkMode::Wsl { distro: None },
            _ if bundled => LinkMode::LocalInstalled,
            _ => LinkMode::Local {
                bin: PathBuf::from("mandord"),
            },
        };
        let text = |key: &str| var(key).filter(|v| !v.is_empty());
        Self {
            mode,
            local_app_data: text("LOCALAPPDATA").map(PathBuf::from),
            user_profile: text("USERPROFILE").map(PathBuf::from),
            home: text("HOME").map(PathBuf::from),
            desktop: text("XDG_CURRENT_DESKTOP"),
        }
    }
}

/// State provisioning: satu proses pada satu waktu, hasil terakhir disimpan.
struct Provisioner {
    cfg: ClientConfig,
    last: tokio::sync::Mutex<Option<ProvisionView>>,
}

/// Hasil provisioning untuk UI: keadaan dan pelaksananya (`wsl` atau `local`;
/// tidak ada bila app tidak memasang daemon).
#[derive(Debug, Clone, serde::Serialize)]
struct ProvisionView {
    runner: Option<&'static str>,
    #[serde(flatten)]
    state: provision::ProvisionState,
}

/// Memasang atau memperbarui daemon (di WSL, atau lokal di Linux) lalu
/// menyiapkan penjaga keep-awake (KTD-49, KTD-L2). Dipanggil UI sebelum
/// tersambung dan saat mencoba ulang pembaruan yang tertunda.
#[tauri::command]
async fn provision_run(
    app: AppHandle,
    link: State<'_, Arc<DaemonLink>>,
    prov: State<'_, Arc<Provisioner>>,
) -> Result<ProvisionView, String> {
    let mut last = prov.last.lock().await;
    let runner = match link.mode() {
        LinkMode::Wsl { distro } => provision::Runner::Wsl {
            distro: distro.clone(),
        },
        LinkMode::LocalInstalled => provision::Runner::Local {
            home: prov
                .cfg
                .home
                .clone()
                .ok_or_else(|| "HOME is not set".to_owned())?,
        },
        LinkMode::Local { .. } => {
            return Ok(ProvisionView {
                runner: None,
                state: provision::ProvisionState::NotNeeded,
            });
        }
    };
    let resource = app
        .path()
        .resolve("resources/mandord", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let inputs = provision::Inputs {
        runner,
        resource,
        current_exe,
        local_app_data: prov.cfg.local_app_data.clone(),
        version: app.package_info().version.to_string(),
    };
    let keep_awake = keep_awake_for(&inputs, &prov.cfg).await;
    link.set_keep_awake(keep_awake.clone());
    let state = provision::provision(&inputs, keep_awake.as_deref().unwrap_or("")).await;
    let view = ProvisionView {
        runner: Some(inputs.runner.kind()),
        state,
    };
    *last = Some(view.clone());
    Ok(view)
}

/// Perintah penjaga keep-awake untuk daemon, bila app membawa daemon.
async fn keep_awake_for(inputs: &provision::Inputs, cfg: &ClientConfig) -> Option<String> {
    if provision::DAEMON_SHA256.len() != 64 {
        return None;
    }
    match &inputs.runner {
        provision::Runner::Wsl { .. } => match provision::keep_awake_command(inputs).await {
            Ok(json) => Some(json),
            Err(e) => {
                tracing::warn!(error = %e, "penjaga keep-awake tidak bisa disiapkan");
                None
            }
        },
        provision::Runner::Local { .. } => {
            let desktop = cfg.desktop.clone();
            let json = tokio::task::spawn_blocking(move || {
                provision::linux_keep_awake_json(desktop.as_deref(), &|p| p.is_file())
            })
            .await
            .ok()
            .flatten();
            if json.is_none() {
                tracing::warn!(
                    "tidak ada program penahan keep-awake (gnome-session-inhibit atau systemd-inhibit)"
                );
            }
            json
        }
    }
}

/// Melepas service daemon yang dipasang app di Linux, sebelum paket dihapus
/// (KTD-L6 plan paket Linux).
#[tauri::command]
async fn daemon_uninstall(
    link: State<'_, Arc<DaemonLink>>,
    prov: State<'_, Arc<Provisioner>>,
) -> Result<provision::InstallReport, String> {
    if *link.mode() != LinkMode::LocalInstalled {
        return Err("the background service is managed elsewhere on this system".to_owned());
    }
    let home = prov
        .cfg
        .home
        .clone()
        .ok_or_else(|| "HOME is not set".to_owned())?;
    provision::uninstall_local(&home, Duration::from_secs(150)).await
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
        LinkMode::Local { .. } | LinkMode::LocalInstalled => return ExitCode::SUCCESS,
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
            daemon_uninstall,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k: &str| map.get(k).cloned()
    }

    #[test]
    fn mode_link_dari_environment() {
        let linux = ClientConfig::from_vars(&vars(&[("HOME", "/home/u")]), false, true);
        assert_eq!(
            linux.mode,
            LinkMode::LocalInstalled,
            "paket Linux membawa daemon"
        );
        assert_eq!(linux.home.as_deref(), Some(std::path::Path::new("/home/u")));

        let dev = ClientConfig::from_vars(&vars(&[]), false, false);
        assert_eq!(
            dev.mode,
            LinkMode::Local {
                bin: PathBuf::from("mandord")
            },
            "build dev tanpa daemon bawaan: PATH"
        );

        let explicit =
            ClientConfig::from_vars(&vars(&[("MANDOR_LINK", "local:/opt/mandord")]), false, true);
        assert_eq!(
            explicit.mode,
            LinkMode::Local {
                bin: PathBuf::from("/opt/mandord")
            },
            "MANDOR_LINK menang"
        );

        let windows = ClientConfig::from_vars(&vars(&[]), true, true);
        assert_eq!(windows.mode, LinkMode::Wsl { distro: None });
        let distro = ClientConfig::from_vars(&vars(&[("MANDOR_LINK", "wsl:Ubuntu")]), false, true);
        assert_eq!(
            distro.mode,
            LinkMode::Wsl {
                distro: Some("Ubuntu".to_owned())
            }
        );

        let gnome = ClientConfig::from_vars(
            &vars(&[("XDG_CURRENT_DESKTOP", "ubuntu:GNOME")]),
            false,
            true,
        );
        assert_eq!(gnome.desktop.as_deref(), Some("ubuntu:GNOME"));
    }
}
