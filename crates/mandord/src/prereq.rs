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

/// Semua prasyarat, berurutan. Dijalankan bersamaan agar cepat.
pub async fn check_all(claude_bin: &Path) -> Vec<PrereqView> {
    let (git, gh, gh_auth, claude) = tokio::join!(
        probe("git", Cmd::new("git").arg("--version")),
        probe("gh", Cmd::new("gh").arg("--version")),
        probe("gh login", Cmd::new("gh").args(["auth", "status"])),
        probe("claude", Cmd::new(claude_bin).arg("--version")),
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
}
