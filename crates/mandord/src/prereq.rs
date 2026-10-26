//! Cek prasyarat untuk onboarding: git, gh (beserta login), dan claude.

use std::path::Path;
use std::time::Duration;

use mandor_proc::{Cmd, run};
use mandor_protocol::PrereqView;

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Menjalankan satu perintah; lolos bila keluar 0. `detail` berisi baris
/// pertama keluaran (biasanya versi) atau alasan gagal.
async fn probe(name: &str, cmd: Cmd) -> PrereqView {
    let (ok, detail) = match run(&cmd.timeout(PROBE_TIMEOUT)).await {
        Ok(out) => {
            let ok = out.success();
            let text = if out.stdout.trim().is_empty() {
                out.stderr
            } else {
                out.stdout
            };
            let line = text.lines().next().unwrap_or_default().trim().to_owned();
            (ok, line)
        }
        Err(e) => (false, e.to_string()),
    };
    PrereqView {
        name: name.to_owned(),
        ok,
        detail,
    }
}

/// Semua prasyarat, berurutan. Dijalankan bersamaan agar cepat. `path`
/// adalah PATH login shell pengguna: service systemd hanya membawa PATH
/// sistem, sedangkan agent dijalankan dengan PATH login shell (mis. `claude`
/// di `~/.local/bin`).
pub async fn check_all(claude_bin: &Path, path: Option<&str>) -> Vec<PrereqView> {
    let cmd = |program: &Path| match path {
        Some(p) => Cmd::new(program).env("PATH", p),
        None => Cmd::new(program),
    };
    let (git, gh, gh_auth, claude) = tokio::join!(
        probe("git", cmd(Path::new("git")).arg("--version")),
        probe("gh", cmd(Path::new("gh")).arg("--version")),
        probe("gh login", cmd(Path::new("gh")).args(["auth", "status"])),
        probe("claude", cmd(claude_bin).arg("--version")),
    );
    vec![git, gh, gh_auth, claude]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn perintah_berhasil_membawa_baris_pertama_dan_yang_tidak_ada_gagal() {
        let ok = probe("sh", Cmd::new("sh").args(["-c", "echo 'v1.2'; echo lain"])).await;
        assert!(ok.ok);
        assert_eq!(ok.detail, "v1.2");

        let gagal = probe(
            "sh",
            Cmd::new("sh").args(["-c", "echo 'not logged in' >&2; exit 1"]),
        )
        .await;
        assert!(!gagal.ok);
        assert_eq!(gagal.detail, "not logged in");

        let hilang = probe("x", Cmd::new("program-yang-tidak-ada-mandor")).await;
        assert!(!hilang.ok);
        assert!(
            hilang.detail.contains("program-yang-tidak-ada-mandor"),
            "{}",
            hilang.detail
        );
    }

    /// Service systemd punya PATH sistem saja; `claude` di `~/.local/bin`
    /// ditemukan lewat PATH login shell, sama seperti agent.
    #[cfg(unix)]
    #[tokio::test]
    async fn claude_dicari_lewat_path_login_shell() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("claude-uji-mandor");
        std::fs::write(&fake, "#!/bin/sh\necho '2.1.310 (Claude Code)'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:/usr/bin:/bin", dir.path().display());
        let checks = check_all(Path::new("claude-uji-mandor"), Some(&path)).await;
        let claude = checks.iter().find(|c| c.name == "claude").unwrap();
        assert!(claude.ok, "{}", claude.detail);
        assert_eq!(claude.detail, "2.1.310 (Claude Code)");
        let tanpa = check_all(Path::new("claude-uji-mandor"), None).await;
        assert!(!tanpa.iter().find(|c| c.name == "claude").unwrap().ok);
    }
}
