//! Pemasangan dan pembaruan daemon di WSL saat app dibuka (KTD-49 plan service
//! OS). App membawa biner `mandord` Linux sebagai resource dengan SHA-256 yang
//! ditanam saat build; bytes yang sudah diverifikasi dialirkan lewat stdin ke
//! WSL lalu `mandord install` dijalankan dari sana. Daemon tidak pernah
//! dieksekusi dari `/mnt/c`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// SHA-256 biner daemon yang dibawa installer; kosong bila build tidak
/// membawa biner asli (build dev atau OS selain Windows).
pub const DAEMON_SHA256: &str = env!("MANDOR_DAEMON_SHA256");

/// Batas waktu probe (cold boot WSL bisa lambat).
const PROBE_TIMEOUT: Duration = Duration::from_secs(45);
/// Batas waktu salin dan `install` (termasuk drain dan restart daemon).
const INSTALL_TIMEOUT: Duration = Duration::from_secs(150);
/// Batas waktu `wslpath`.
const WSLPATH_TIMEOUT: Duration = Duration::from_secs(30);
/// Keluaran probe yang dibaca paling banyak (byte).
const PROBE_OUTPUT_MAX: usize = 16 * 1024;

/// Skrip probe: hash biner terpasang (dibaca, tidak dieksekusi), lalu
/// `install --check` dengan jalur absolut. `$1` = perintah keep-awake (JSON).
const PROBE_SCRIPT: &str = r#"b="$HOME/.local/bin/mandord"
[ -f "$b" ] || exit 0
sha256sum "$b"
"$b" install --check --keep-awake-cmd "$1" 2>/dev/null || true"#;

/// Skrip pasang: bytes dari stdin ditulis ke berkas baru (noclobber, `O_EXCL`) di
/// `~/.cache/mandor` (0700), dijalankan `install`, lalu dihapus.
const INSTALL_SCRIPT: &str = r#"set -C
umask 077
d="$HOME/.cache/mandor"
mkdir -p "$d" && chmod 700 "$d" || exit 90
f="$d/mandord.$$"
cat > "$f" || { rm -f "$f"; exit 91; }
chmod 700 "$f"
"$f" install --keep-awake-cmd "$1"
rc=$?
rm -f "$f"
exit $rc"#;

/// Argumen `wsl.exe` untuk menjalankan `argv` tanpa shell default Windows/WSL.
#[must_use]
pub fn wsl_args(distro: Option<&str>, argv: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(d) = distro {
        out.push("-d".to_owned());
        out.push(d.to_owned());
    }
    out.push("-e".to_owned());
    out.extend(argv.iter().map(|a| (*a).to_owned()));
    out
}

#[must_use]
pub fn probe_args(distro: Option<&str>, keep_awake_json: &str) -> Vec<String> {
    wsl_args(distro, &["sh", "-c", PROBE_SCRIPT, "sh", keep_awake_json])
}

#[must_use]
pub fn install_args(distro: Option<&str>, keep_awake_json: &str) -> Vec<String> {
    wsl_args(distro, &["sh", "-c", INSTALL_SCRIPT, "sh", keep_awake_json])
}

#[must_use]
pub fn wslpath_args(distro: Option<&str>, windows_path: &str) -> Vec<String> {
    wsl_args(distro, &["wslpath", "-u", windows_path])
}

/// Hasil `install --check` yang dipakai app.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct CheckReport {
    service: String,
    systemd: bool,
    #[serde(default)]
    linger: bool,
}

/// Keputusan dari probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// Biner terpasang identik dan service sesuai: langsung tersambung.
    Ready { systemd: bool, linger: bool },
    /// Biner tidak ada, berbeda, versi lama, atau service belum sesuai.
    NeedsInstall,
}

/// Membaca keluaran probe: baris `sha256sum` lalu JSON `install --check`.
#[must_use]
pub fn parse_probe(stdout: &str, expected_sha256: &str) -> Probe {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let Some(hash_line) = lines.next() else {
        return Probe::NeedsInstall;
    };
    let hash = hash_line.split_whitespace().next().unwrap_or_default();
    if !hash.eq_ignore_ascii_case(expected_sha256) {
        return Probe::NeedsInstall;
    }
    let check = lines.find_map(|l| serde_json::from_str::<CheckReport>(l).ok());
    match check {
        Some(c) if !c.systemd || c.service == "matches" => Probe::Ready {
            systemd: c.systemd,
            linger: c.linger,
        },
        _ => Probe::NeedsInstall,
    }
}

/// Hasil `mandord install` (satu baris JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallReport {
    pub result: String,
    pub service: String,
    pub linger: bool,
    pub apps_stopped: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Baris JSON terakhir keluaran `install`.
///
/// # Errors
/// Tidak ada baris JSON yang dikenali.
pub fn parse_install(stdout: &str) -> Result<InstallReport, String> {
    stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<InstallReport>(l.trim()).ok())
        .ok_or_else(|| "the installer did not report a result".to_owned())
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// Keadaan provisioning untuk UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProvisionState {
    /// Mode lokal: tidak ada yang dipasang app.
    NotNeeded,
    /// Build tanpa biner daemon (build dev).
    NotBundled,
    /// Daemon terpasang sudah sesuai. `systemd: false` berarti fallback setsid.
    UpToDate {
        systemd: bool,
        linger: bool,
    },
    Installed {
        report: InstallReport,
    },
    /// Pekerjaan berjalan; app tersambung ke daemon lama dan mencoba lagi nanti.
    Deferred {
        report: InstallReport,
    },
    /// Probe tidak selesai; app tetap tersambung tanpa memasang.
    CannotCheck {
        message: String,
    },
    Failed {
        message: String,
    },
}

/// Perintah keep-awake untuk daemon: argv JSON salinan exe ini di WSL.
#[must_use]
pub fn keep_awake_json(wsl_exe_path: &str) -> String {
    serde_json::json!([wsl_exe_path, crate::keep_awake::FLAG]).to_string()
}

/// Lokasi salinan penjaga keep-awake per versi:
/// `<LOCALAPPDATA>\Mandor\keepawake\<versi>\mandor-keepawake.exe`.
#[must_use]
pub fn helper_path(local_app_data: &Path, version: &str) -> PathBuf {
    local_app_data
        .join("Mandor")
        .join("keepawake")
        .join(version)
        .join("mandor-keepawake.exe")
}

/// Menyalin exe app ke lokasi penjaga bila belum ada atau isinya berbeda.
/// Salinan per versi membuat installer tidak pernah mengganti exe yang
/// sedang dipakai penjaga.
///
/// # Errors
/// Berkas tidak bisa dibaca atau ditulis.
pub fn prepare_helper(source: &Path, target: &Path) -> std::io::Result<()> {
    let src = std::fs::read(source)?;
    if std::fs::read(target).is_ok_and(|cur| cur == src) {
        return Ok(());
    }
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = target.with_extension("tmp");
    std::fs::write(&tmp, &src)?;
    std::fs::rename(&tmp, target)
}

/// Menjalankan `wsl.exe` dengan batas waktu dan stdin opsional.
async fn run_wsl(
    args: &[String],
    stdin: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut cmd = Command::new("wsl.exe");
    cmd.args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::daemon_link::hide_console(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot run wsl.exe: {e}"))?;
    let pipe = child.stdin.take();
    let writer = async move {
        if let (Some(mut pipe), Some(data)) = (pipe, stdin) {
            pipe.write_all(&data).await.map_err(|e| e.to_string())?;
            pipe.shutdown().await.map_err(|e| e.to_string())?;
        }
        Ok::<(), String>(())
    };
    // Tulis stdin dan baca keluaran bersamaan agar pipe tidak saling menunggu.
    let (fed, out) = tokio::time::timeout(timeout, async {
        tokio::join!(writer, child.wait_with_output())
    })
    .await
    .map_err(|_| format!("wsl.exe did not finish within {timeout:?}"))?;
    fed?;
    out.map_err(|e| e.to_string())
}

/// Masukan provisioning.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub distro: Option<String>,
    /// Biner daemon dari resource installer.
    pub resource: PathBuf,
    /// Exe app ini (sumber salinan penjaga).
    pub current_exe: PathBuf,
    /// `%LOCALAPPDATA%`.
    pub local_app_data: Option<PathBuf>,
    pub version: String,
}

/// Menyiapkan penjaga keep-awake dan mengembalikan perintahnya untuk daemon.
///
/// # Errors
/// Salinan atau konversi jalur gagal.
pub async fn keep_awake_command(inputs: &Inputs) -> Result<String, String> {
    let base = inputs
        .local_app_data
        .clone()
        .ok_or_else(|| "LOCALAPPDATA is not set".to_owned())?;
    let target = helper_path(&base, &inputs.version);
    let (source, dest) = (inputs.current_exe.clone(), target.clone());
    tokio::task::spawn_blocking(move || prepare_helper(&source, &dest))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("cannot copy the keep-awake helper: {e}"))?;
    let windows_path = target.display().to_string();
    let out = run_wsl(
        &wslpath_args(inputs.distro.as_deref(), &windows_path),
        None,
        WSLPATH_TIMEOUT,
    )
    .await?;
    let wsl_path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() || !wsl_path.starts_with('/') {
        return Err("wslpath could not convert the helper path".to_owned());
    }
    Ok(keep_awake_json(&wsl_path))
}

/// Probe lalu pasang bila perlu.
pub async fn provision(inputs: &Inputs, keep_awake: &str) -> ProvisionState {
    if DAEMON_SHA256.len() != 64 {
        return ProvisionState::NotBundled;
    }
    let distro = inputs.distro.as_deref();
    match run_wsl(&probe_args(distro, keep_awake), None, PROBE_TIMEOUT).await {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let text: String = text.chars().take(PROBE_OUTPUT_MAX).collect();
            if let Probe::Ready { systemd, linger } = parse_probe(&text, DAEMON_SHA256) {
                return ProvisionState::UpToDate { systemd, linger };
            }
        }
        Err(message) => return ProvisionState::CannotCheck { message },
    }
    let resource = inputs.resource.clone();
    let bytes = match tokio::task::spawn_blocking(move || std::fs::read(resource)).await {
        Ok(Ok(b)) => b,
        Ok(Err(e)) => {
            return ProvisionState::Failed {
                message: format!("cannot read the bundled daemon: {e}"),
            };
        }
        Err(e) => {
            return ProvisionState::Failed {
                message: e.to_string(),
            };
        }
    };
    if !sha256_hex(&bytes).eq_ignore_ascii_case(DAEMON_SHA256) {
        return ProvisionState::Failed {
            message: "the bundled daemon does not match this app; reinstall Mandor".to_owned(),
        };
    }
    let out = match run_wsl(
        &install_args(distro, keep_awake),
        Some(bytes),
        INSTALL_TIMEOUT,
    )
    .await
    {
        Ok(out) => out,
        Err(message) => return ProvisionState::Failed { message },
    };
    match parse_install(&String::from_utf8_lossy(&out.stdout)) {
        Ok(report) => match report.result.as_str() {
            "installed" | "updated" | "service_only" => ProvisionState::Installed { report },
            "up_to_date" => ProvisionState::UpToDate {
                systemd: report.service == "systemd",
                linger: report.linger,
            },
            "deferred" => ProvisionState::Deferred { report },
            _ => ProvisionState::Failed {
                message: report
                    .message
                    .unwrap_or_else(|| "the installer failed".to_owned()),
            },
        },
        Err(message) => ProvisionState::Failed {
            message: format!("{message}: {}", String::from_utf8_lossy(&out.stderr).trim()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12ab12";

    #[test]
    fn argumen_wsl_tanpa_shell_dan_satu_argumen_per_nilai() {
        let json = r#"["/mnt/c/Users/A B/AppData/Local/Mandor/keepawake/0.1.0/mandor-keepawake.exe","--keep-awake"]"#;
        let args = probe_args(Some("Ubuntu 24.04"), json);
        assert_eq!(&args[..4], ["-d", "Ubuntu 24.04", "-e", "sh"]);
        assert_eq!(args[4], "-c");
        assert_eq!(args[5], PROBE_SCRIPT);
        assert_eq!(args[6], "sh");
        assert_eq!(args[7], json, "JSON tetap satu argumen");
        assert_eq!(args.len(), 8);

        let args = install_args(None, json);
        assert_eq!(&args[..2], ["-e", "sh"]);
        assert_eq!(args[3], INSTALL_SCRIPT);

        assert_eq!(
            wslpath_args(None, r"C:\Users\A B\x.exe"),
            ["-e", "wslpath", "-u", r"C:\Users\A B\x.exe"]
        );
    }

    #[test]
    fn skrip_memakai_jalur_absolut_dan_tidak_mengeksekusi_dari_mnt() {
        assert!(PROBE_SCRIPT.contains(r#""$HOME/.local/bin/mandord""#));
        assert!(INSTALL_SCRIPT.contains("set -C"));
        assert!(INSTALL_SCRIPT.contains(".cache/mandor"));
        assert!(!INSTALL_SCRIPT.contains("/mnt/"));
    }

    #[test]
    fn probe_siap_hanya_bila_hash_sama_dan_service_sesuai() {
        let ok = format!(
            "{H}  /home/u/.local/bin/mandord\n{{\"binary\":\"same\",\"installed_version\":\"0.1.0\",\"service\":\"matches\",\"systemd\":true,\"linger\":true,\"running\":\"systemd\"}}\n"
        );
        let ready = Probe::Ready {
            systemd: true,
            linger: true,
        };
        assert_eq!(parse_probe(&ok, H), ready);
        let upper = ok.replacen(H, &H.to_uppercase(), 1);
        assert_eq!(parse_probe(&upper, H), ready);

        let no_systemd = format!(
            "{H}  /x\n{{\"binary\":\"same\",\"service\":\"unavailable\",\"systemd\":false,\"linger\":false,\"running\":\"no\"}}\n"
        );
        assert_eq!(
            parse_probe(&no_systemd, H),
            Probe::Ready {
                systemd: false,
                linger: false
            }
        );

        let unit_missing = format!("{H}  /x\n{{\"service\":\"missing\",\"systemd\":true}}\n");
        assert_eq!(parse_probe(&unit_missing, H), Probe::NeedsInstall);
        assert_eq!(parse_probe("", H), Probe::NeedsInstall, "biner belum ada");
        let other = "0000  /x\n{\"service\":\"matches\",\"systemd\":true}\n";
        assert_eq!(parse_probe(other, H), Probe::NeedsInstall, "hash beda");
        let old = format!("{H}  /x\n2026-10-18T01:00:00Z INFO mandord: version=0.1.0\n");
        assert_eq!(
            parse_probe(&old, H),
            Probe::NeedsInstall,
            "biner lama tanpa install"
        );
    }

    #[test]
    fn hasil_install_dari_baris_json_terakhir() {
        let out = "log\n{\"result\":\"deferred\",\"service\":\"systemd\",\"linger\":true,\"apps_stopped\":false,\"warnings\":[]}\n";
        let r = parse_install(out).unwrap();
        assert_eq!(r.result, "deferred");
        assert!(parse_install("bukan json").is_err());
        let err = "{\"result\":\"error\",\"service\":\"none\",\"linger\":false,\"apps_stopped\":false,\"warnings\":[],\"message\":\"x\"}";
        assert_eq!(parse_install(err).unwrap().message.as_deref(), Some("x"));
    }

    #[test]
    fn hash_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn perintah_keep_awake_json() {
        assert_eq!(
            keep_awake_json("/mnt/c/Users/A B/k.exe"),
            r#"["/mnt/c/Users/A B/k.exe","--keep-awake"]"#
        );
    }

    #[test]
    fn salinan_penjaga_per_versi_dan_tidak_disalin_ulang() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("mandor.exe");
        std::fs::write(&exe, b"exe v1").unwrap();
        let target = helper_path(dir.path(), "0.1.0");
        assert!(target.ends_with("Mandor/keepawake/0.1.0/mandor-keepawake.exe"));

        prepare_helper(&exe, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"exe v1");
        let modified = std::fs::metadata(&target).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        prepare_helper(&exe, &target).unwrap();
        assert_eq!(
            std::fs::metadata(&target).unwrap().modified().unwrap(),
            modified,
            "isi sama: tidak disalin ulang"
        );

        std::fs::write(&exe, b"exe v2").unwrap();
        let next = helper_path(dir.path(), "0.2.0");
        prepare_helper(&exe, &next).unwrap();
        assert_eq!(std::fs::read(&next).unwrap(), b"exe v2");
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"exe v1",
            "versi lama tetap"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn build_tanpa_biner_daemon_tidak_memasang_apa_pun() {
        if DAEMON_SHA256.len() == 64 {
            return;
        }
        let inputs = Inputs {
            distro: None,
            resource: PathBuf::from("/tidak/ada"),
            current_exe: PathBuf::from("/tidak/ada"),
            local_app_data: None,
            version: "0.1.0".to_owned(),
        };
        assert_eq!(provision(&inputs, "[]").await, ProvisionState::NotBundled);
    }
}
