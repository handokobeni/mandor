//! Pembersihan saat app di-uninstall (KTD-48 plan service OS): dijalankan hook
//! NSIS sebagai `mandor --uninstall-cleanup` sebelum berkas app dihapus.
//! Menghapus dua kunci keep-alive Mandor dari `.wslconfig`, lalu melepas
//! daemon di WSL dengan `mandord uninstall`. Uninstall tidak dipaksa: bila ada
//! agent yang bekerja, daemon dibiarkan terpasang (R3) dan hook memberi tahu
//! pengguna langkah manualnya.

use std::path::Path;
use std::process::{ExitCode, Stdio};
use std::time::Duration;

/// Argumen yang menyalakan mode pembersihan.
pub const FLAG: &str = "--uninstall-cleanup";

/// Kode keluar untuk hook NSIS.
pub const EXIT_DONE: u8 = 0;
pub const EXIT_FAILED: u8 = 1;
/// Ada agent yang bekerja; daemon dibiarkan terpasang.
pub const EXIT_DEFERRED: u8 = 2;

const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(150);

/// Skrip WSL: lepas daemon bila terpasang di `~/.local/bin`.
const SCRIPT: &str = r#"b="$HOME/.local/bin/mandord"
[ -x "$b" ] || { echo '{"result":"uninstalled"}'; exit 0; }
exec "$b" uninstall"#;

#[must_use]
pub fn requested(mut args: impl Iterator<Item = String>) -> bool {
    args.nth(1).is_some_and(|a| a == FLAG)
}

/// Argumen `wsl.exe` untuk melepas daemon.
#[must_use]
pub fn wsl_uninstall_args(distro: Option<&str>) -> Vec<String> {
    crate::provision::wsl_args(distro, &["sh", "-c", SCRIPT])
}

/// Kode keluar dari keluaran `mandord uninstall`.
#[must_use]
pub fn exit_code_for(stdout: &str) -> u8 {
    let result = stdout.lines().rev().find_map(|l| {
        serde_json::from_str::<serde_json::Value>(l.trim())
            .ok()?
            .get("result")?
            .as_str()
            .map(str::to_owned)
    });
    match result.as_deref() {
        Some("uninstalled") => EXIT_DONE,
        Some("deferred") => EXIT_DEFERRED,
        _ => EXIT_FAILED,
    }
}

/// Menghapus kunci keep-alive Mandor dari `.wslconfig` di `user_profile`.
fn clean_wslconfig(user_profile: Option<&Path>) -> bool {
    let Some(profile) = user_profile else {
        return true;
    };
    match crate::wslconfig::disable_at(&profile.join(".wslconfig")) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, ".wslconfig tidak bisa dibersihkan");
            false
        }
    }
}

async fn uninstall_daemon(distro: Option<&str>) -> u8 {
    let mut cmd = tokio::process::Command::new("wsl.exe");
    cmd.args(wsl_uninstall_args(distro))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    crate::daemon_link::hide_console(&mut cmd);
    match tokio::time::timeout(UNINSTALL_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => exit_code_for(&String::from_utf8_lossy(&out.stdout)),
        _ => EXIT_FAILED,
    }
}

/// Menjalankan pembersihan; dipanggil sebelum Tauri dimulai.
#[must_use]
pub fn run(distro: Option<&str>, user_profile: Option<&Path>) -> ExitCode {
    let wslconfig_ok = clean_wslconfig(user_profile);
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "runtime pembersihan gagal dibuat");
            return ExitCode::from(EXIT_FAILED);
        }
    };
    let code = runtime.block_on(uninstall_daemon(distro));
    if code == EXIT_DONE && !wslconfig_ok {
        return ExitCode::from(EXIT_FAILED);
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kode_keluar_dari_hasil_uninstall() {
        assert_eq!(
            exit_code_for("{\"result\":\"uninstalled\",\"service\":\"systemd\"}\n"),
            EXIT_DONE
        );
        assert_eq!(exit_code_for("{\"result\":\"deferred\"}"), EXIT_DEFERRED);
        assert_eq!(
            exit_code_for("{\"result\":\"error\",\"message\":\"x\"}"),
            EXIT_FAILED
        );
        assert_eq!(exit_code_for(""), EXIT_FAILED);
    }

    #[test]
    fn argumen_wsl_tanpa_force() {
        let args = wsl_uninstall_args(Some("Ubuntu"));
        assert_eq!(&args[..4], ["-d", "Ubuntu", "-e", "sh"]);
        assert!(args[5].contains("uninstall"));
        assert!(
            !args[5].contains("--force"),
            "agent yang bekerja tidak diputus"
        );
    }

    #[test]
    fn flag_hanya_sebagai_argumen_pertama() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(requested(args(&["mandor.exe", FLAG]).into_iter()));
        assert!(!requested(args(&["mandor.exe"]).into_iter()));
    }

    #[cfg(unix)]
    #[test]
    fn skrip_tanpa_biner_terpasang_berhasil() {
        let home = tempfile::tempdir().unwrap();
        let out = std::process::Command::new("sh")
            .args(["-c", SCRIPT])
            .env("HOME", home.path())
            .output()
            .unwrap();
        assert_eq!(
            exit_code_for(&String::from_utf8_lossy(&out.stdout)),
            EXIT_DONE
        );
    }

    #[test]
    fn wslconfig_dibersihkan() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".wslconfig");
        std::fs::write(&path, "[wsl2]\nmemory=8GB\n").unwrap();
        crate::wslconfig::enable_at(&path).unwrap();
        assert!(clean_wslconfig(Some(dir.path())));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[wsl2]\nmemory=8GB\n"
        );
        assert!(clean_wslconfig(None));
    }
}
