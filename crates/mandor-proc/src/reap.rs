//! Menghentikan process group yatim dari daemon sebelumnya (KTD-13).

use std::time::Duration;

use crate::{Cmd, run};

/// Menghentikan process group `pid` bila proses itu masih ada dan command
/// line-nya memuat `marker` (melindungi dari PID yang sudah dipakai ulang).
/// Mengembalikan `true` bila sinyal dikirim.
pub async fn reap_group(pid: u32, marker: &str) -> bool {
    if !matches_marker(pid, marker).await {
        return false;
    }
    terminate(pid).await;
    true
}

#[cfg(unix)]
async fn matches_marker(pid: u32, marker: &str) -> bool {
    let out = run(&Cmd::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .timeout(Duration::from_secs(5)))
    .await;
    matches!(out, Ok(o) if o.success() && o.stdout.contains(marker))
}

#[cfg(windows)]
async fn matches_marker(pid: u32, marker: &str) -> bool {
    let filter = format!("PID eq {pid}");
    let out = run(&Cmd::new("tasklist")
        .args(["/FI", &filter, "/V", "/FO", "CSV"])
        .timeout(Duration::from_secs(5)))
    .await;
    matches!(out, Ok(o) if o.success() && o.stdout.to_lowercase().contains(&marker.to_lowercase()))
}

#[cfg(unix)]
async fn terminate(pid: u32) {
    let group = format!("-{pid}");
    let _ = run(&Cmd::new("kill")
        .args(["-TERM", "--", &group])
        .timeout(Duration::from_secs(5)))
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = run(&Cmd::new("kill")
        .args(["-KILL", "--", &group])
        .timeout(Duration::from_secs(5)))
    .await;
}

#[cfg(windows)]
async fn terminate(pid: u32) {
    let _ = run(&Cmd::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .timeout(Duration::from_secs(5)))
    .await;
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::Supervised;

    #[tokio::test]
    async fn hanya_proses_dengan_penanda_yang_dihentikan() {
        let mut child =
            Supervised::spawn(&Cmd::new("sleep").arg("30")).unwrap_or_else(|e| panic!("{e}"));
        let pid = child.pid().unwrap_or_default();
        assert!(!reap_group(pid, "claude-tidak-ada").await);
        assert!(reap_group(pid, "sleep").await);
        let code = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        assert!(code.is_ok(), "proses tidak berhenti");
    }
}
