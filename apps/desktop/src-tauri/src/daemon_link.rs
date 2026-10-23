//! Jembatan ke daemon `mandord` lewat relay stdio (KTD-2).
//!
//! Webview tidak pernah membuka koneksi jaringan sendiri: sisi Rust menjalankan
//! `mandord connect` (lokal) atau `wsl.exe -d <distro> -- mandord connect`
//! (mode WSL), lalu meneruskan baris JSON lewat event Tauri.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

/// Batas waktu `mandord token` (cold boot WSL bisa lambat).
const TOKEN_TIMEOUT: Duration = Duration::from_secs(20);

/// Event yang dikirim ke webview.
pub const EVENT_LINE: &str = "daemon://line";
pub const EVENT_CLOSED: &str = "daemon://closed";

/// Cara menjangkau daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkMode {
    /// Daemon di mesin yang sama.
    Local { bin: PathBuf },
    /// Daemon di dalam distro WSL; `None` berarti distro default.
    Wsl { distro: Option<String> },
}

impl LinkMode {
    /// Program dan argumen untuk menjalankan subcommand `mandord` (`token`
    /// atau `connect`). `keep_awake`: perintah penjaga keep-awake (JSON) yang
    /// diteruskan ke daemon yang dijalankan `connect`.
    #[must_use]
    pub fn command(&self, sub: &str, keep_awake: Option<&str>) -> (String, Vec<String>) {
        match self {
            Self::Local { bin } => (bin.to_string_lossy().into_owned(), vec![sub.to_owned()]),
            Self::Wsl { distro } => {
                // Shell login agar PATH dari ~/.profile terbaca untuk daemon yang
                // dinyalakan `connect`. Biner terpasang di ~/.local/bin dipakai
                // dengan jalur absolut (KTD-49); setup lama memakai PATH.
                let script = format!(
                    "b=\"$HOME/.local/bin/mandord\"; [ -x \"$b\" ] || b=mandord; \
                     [ -n \"$1\" ] && export MANDOR_KEEP_AWAKE_CMD=\"$1\"; exec \"$b\" {sub}"
                );
                let shell = ["sh", "-lc", script.as_str(), "sh", keep_awake.unwrap_or("")];
                let args = crate::provision::wsl_args(distro.as_deref(), &shell);
                ("wsl.exe".to_owned(), args)
            }
        }
    }

    /// Argumen proses penjaga yang menjaga VM WSL tetap hidup (KTD-18).
    #[must_use]
    pub fn keeper(&self) -> Option<(String, Vec<String>)> {
        match self {
            Self::Local { .. } => None,
            Self::Wsl { distro } => {
                let mut args = Vec::new();
                if let Some(d) = distro {
                    args.push("-d".to_owned());
                    args.push(d.clone());
                }
                args.extend(["-e".to_owned(), "sleep".to_owned(), "infinity".to_owned()]);
                Some(("wsl.exe".to_owned(), args))
            }
        }
    }
}

/// Kegagalan jembatan daemon; dikirim ke webview sebagai teks.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("cannot run {program}: {reason}")]
    Spawn { program: String, reason: String },
    #[error("daemon did not return a token within {0:?}")]
    TokenTimeout(Duration),
    #[error("invalid token response: {0}")]
    BadToken(String),
    #[error("not connected to the daemon")]
    NotConnected,
    #[error("failed to write to the daemon: {0}")]
    Write(String),
}

impl Serialize for LinkError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// State jembatan yang dikelola Tauri.
#[derive(Debug)]
pub struct DaemonLink {
    mode: LinkMode,
    /// Perintah penjaga keep-awake dari provisioning (JSON argv).
    keep_awake: std::sync::Mutex<Option<String>>,
    conn: Mutex<Option<Connection>>,
    keeper: Mutex<Option<Child>>,
}

#[derive(Debug)]
struct Connection {
    child: Child,
    stdin: ChildStdin,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenInfo {
    pub token: String,
    pub protocol: String,
}

impl DaemonLink {
    #[must_use]
    pub fn new(mode: LinkMode) -> Self {
        Self {
            mode,
            keep_awake: std::sync::Mutex::new(None),
            conn: Mutex::new(None),
            keeper: Mutex::new(None),
        }
    }

    /// Cara menjangkau daemon.
    #[must_use]
    pub fn mode(&self) -> &LinkMode {
        &self.mode
    }

    /// Menyimpan perintah penjaga keep-awake untuk `connect` berikutnya.
    pub fn set_keep_awake(&self, json: Option<String>) {
        *self
            .keep_awake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = json;
    }

    fn spawn(&self, sub: &str) -> Result<Child, LinkError> {
        let keep_awake = self
            .keep_awake
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let (program, args) = self.mode.command(sub, keep_awake.as_deref());
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        hide_console(&mut cmd);
        cmd.spawn().map_err(|e| LinkError::Spawn {
            program,
            reason: e.to_string(),
        })
    }

    /// Menjalankan `mandord token` dan mengembalikan token `hello`.
    ///
    /// # Errors
    /// [`LinkError`] bila proses gagal atau melewati batas waktu.
    pub async fn token(&self) -> Result<TokenInfo, LinkError> {
        let child = self.spawn("token")?;
        let out = tokio::time::timeout(TOKEN_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| LinkError::TokenTimeout(TOKEN_TIMEOUT))?
            .map_err(|e| LinkError::BadToken(e.to_string()))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().last().unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|e| LinkError::BadToken(e.to_string()))?;
        let field = |k: &str| value.get(k).and_then(|v| v.as_str()).map(str::to_owned);
        match (field("token"), field("protocol")) {
            (Some(token), Some(protocol)) => Ok(TokenInfo { token, protocol }),
            _ => Err(LinkError::BadToken(
                "token/protocol field missing".to_owned(),
            )),
        }
    }

    /// Membuka relay baru (menutup yang lama) dan meneruskan setiap baris ke webview.
    ///
    /// # Errors
    /// [`LinkError::Spawn`] bila relay tidak bisa dijalankan.
    pub async fn open(&self, app: AppHandle) -> Result<(), LinkError> {
        let mut child = self.spawn("connect")?;
        let stdout = child.stdout.take().ok_or(LinkError::NotConnected)?;
        let stdin = child.stdin.take().ok_or(LinkError::NotConnected)?;
        if let Some(mut old) = self.conn.lock().await.replace(Connection { child, stdin }) {
            let _ = old.child.kill().await;
        }
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if app.emit(EVENT_LINE, line).is_err() {
                    break;
                }
            }
            let _ = app.emit(EVENT_CLOSED, ());
        });
        Ok(())
    }

    /// Mengirim satu baris JSON ke daemon.
    ///
    /// # Errors
    /// [`LinkError::NotConnected`] bila relay belum dibuka, [`LinkError::Write`] bila gagal menulis.
    pub async fn send(&self, line: &str) -> Result<(), LinkError> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or(LinkError::NotConnected)?;
        conn.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(|e| LinkError::Write(e.to_string()))?;
        conn.stdin
            .flush()
            .await
            .map_err(|e| LinkError::Write(e.to_string()))
    }

    /// Menjalankan proses penjaga VM WSL (no-op di mode lokal).
    pub async fn start_keeper(&self) {
        let Some((program, args)) = self.mode.keeper() else {
            return;
        };
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        hide_console(&mut cmd);
        match cmd.spawn() {
            Ok(child) => *self.keeper.lock().await = Some(child),
            Err(err) => tracing::warn!(error = %err, "penjaga WSL tidak bisa dijalankan"),
        }
    }

    /// Menghentikan relay dan penjaga; daemon tetap berjalan.
    pub async fn shutdown(&self) {
        if let Some(mut conn) = self.conn.lock().await.take() {
            let _ = conn.child.kill().await;
        }
        if let Some(mut keeper) = self.keeper.lock().await.take() {
            let _ = keeper.kill().await;
        }
    }
}

#[cfg(windows)]
pub(crate) fn hide_console(cmd: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.env("WSL_UTF8", "1");
}

#[cfg(not(windows))]
pub(crate) fn hide_console(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perintah_lokal_memanggil_biner_langsung() {
        let mode = LinkMode::Local {
            bin: PathBuf::from("/opt/mandord"),
        };
        assert_eq!(
            mode.command("connect", Some("[]")),
            ("/opt/mandord".to_owned(), vec!["connect".to_owned()])
        );
        assert_eq!(mode.keeper(), None);
    }

    #[test]
    fn perintah_wsl_tanpa_shell_windows_dan_jalur_absolut() {
        let mode = LinkMode::Wsl {
            distro: Some("Ubuntu-24.04".to_owned()),
        };
        let json = r#"["/mnt/c/Users/A B/k.exe","--keep-awake"]"#;
        let (program, args) = mode.command("connect", Some(json));
        assert_eq!(program, "wsl.exe");
        assert_eq!(&args[..5], ["-d", "Ubuntu-24.04", "-e", "sh", "-lc"]);
        assert!(args[5].contains(r#"b="$HOME/.local/bin/mandord""#));
        assert!(args[5].contains("MANDOR_KEEP_AWAKE_CMD=\"$1\""));
        assert!(args[5].ends_with(r#"exec "$b" connect"#));
        assert_eq!(
            &args[6..],
            ["sh", json],
            "JSON satu argumen, bukan bagian skrip"
        );
        let (_, keeper) = mode.keeper().unwrap();
        assert_eq!(keeper, ["-d", "Ubuntu-24.04", "-e", "sleep", "infinity"]);
    }

    #[test]
    fn perintah_wsl_tanpa_distro_dan_tanpa_keep_awake() {
        let mode = LinkMode::Wsl { distro: None };
        let (_, args) = mode.command("token", None);
        assert_eq!(&args[..3], ["-e", "sh", "-lc"]);
        assert!(args[3].ends_with(r#"exec "$b" token"#));
        assert_eq!(&args[4..], ["sh", ""]);
    }

    /// Skrip relay dijalankan sungguhan oleh `sh`: biner di ~/.local/bin
    /// dipakai dan perintah keep-awake sampai ke environment-nya.
    #[cfg(unix)]
    #[test]
    fn skrip_relay_memakai_biner_terpasang_dan_meneruskan_keep_awake() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("mandord");
        std::fs::write(&fake, "#!/bin/sh\necho \"$1|$MANDOR_KEEP_AWAKE_CMD\"\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mode = LinkMode::Wsl { distro: None };
        let json = r#"["/mnt/c/A B/k.exe","--keep-awake"]"#;
        let (_, args) = mode.command("token", Some(json));
        // Lewati `-e`; jalankan argv yang akan dieksekusi wsl.exe.
        let out = std::process::Command::new(&args[1])
            .args(["-c", &args[3], &args[4], &args[5]])
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("token|{json}")
        );
    }
}
