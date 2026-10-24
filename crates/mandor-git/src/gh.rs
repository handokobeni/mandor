use std::path::Path;

use mandor_proc::Cmd;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::{Git, GitError, NETWORK_TIMEOUT};

/// Status PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

/// Ringkasan check CI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecksState {
    None,
    Pending,
    Passing,
    Failing,
}

/// PR yang akan dibuat.
#[derive(Debug, Clone, Copy)]
pub struct NewPr<'a> {
    pub base: &'a str,
    pub head: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    /// PR draft (daemon VM).
    pub draft: bool,
}

/// Info PR untuk sebuah branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    pub state: PrState,
    pub checks: ChecksState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPr {
    number: u64,
    url: String,
    state: String,
    #[serde(default)]
    status_check_rollup: Vec<serde_json::Value>,
}

impl Git {
    fn gh(&self, dir: &Path, token: Option<&SecretString>) -> Cmd {
        let mut cmd = Cmd::new(&self.cfg.gh_bin)
            .cwd(dir)
            .scrub_tokens()
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .timeout(NETWORK_TIMEOUT);
        if let Some(t) = token {
            cmd = cmd.env("GH_TOKEN", t.expose_secret());
        }
        cmd
    }

    /// Token profil `gh` (`gh auth token --user <profile>`), tanpa mewarisi
    /// `GH_TOKEN` dari lingkungan pengguna.
    ///
    /// # Errors
    /// [`GitError::Command`] bila profil tidak login.
    pub async fn gh_token(&self, profile: &str) -> Result<SecretString, GitError> {
        let out = self
            .ok(
                "gh auth token",
                self.gh(Path::new("."), None)
                    .args(["auth", "token", "--user", profile]),
            )
            .await?;
        Ok(SecretString::from(out.stdout.trim().to_owned()))
    }

    /// Membuat PR dari `head` ke `base`; mengembalikan URL PR. PR draft di
    /// daemon VM (KTD-68 plan VM).
    ///
    /// # Errors
    /// [`GitError::Command`] bila `gh pr create` gagal.
    pub async fn create_pr(
        &self,
        worktree: &Path,
        pr: &NewPr<'_>,
        token: Option<&SecretString>,
    ) -> Result<String, GitError> {
        let mut args = vec![
            "pr",
            "create",
            "--base",
            pr.base,
            "--head",
            pr.head,
            "--title",
            pr.title,
            "--body-file",
            "-",
        ];
        if pr.draft {
            args.push("--draft");
        }
        let out = self
            .ok(
                "gh pr create",
                self.gh(worktree, token)
                    .args(args)
                    .stdin_bytes(pr.body.as_bytes().to_vec()),
            )
            .await?;
        out.stdout
            .lines()
            .rev()
            .map(str::trim)
            .find(|l| l.starts_with("https://"))
            .map(str::to_owned)
            .ok_or_else(|| GitError::Parse(out.stdout.clone()))
    }

    /// Badan PR di `url`; `None` bila PR tidak ditemukan.
    ///
    /// # Errors
    /// [`GitError::Command`] bila `gh pr view` gagal, [`GitError::Parse`] bila
    /// keluarannya tidak bisa diurai.
    pub async fn pr_body(
        &self,
        repo: &Path,
        url: &str,
        token: Option<&SecretString>,
    ) -> Result<Option<String>, GitError> {
        let out = self
            .exec(
                &self
                    .gh(repo, token)
                    .args(["pr", "view", url, "--json", "body"]),
            )
            .await?;
        if !out.success() {
            if out.stderr.contains("Could not resolve")
                || out.stderr.contains("no pull requests found")
            {
                return Ok(None);
            }
            return Err(GitError::Command {
                op: "gh pr view",
                stderr: out.stderr.trim().to_owned(),
            });
        }
        let v: serde_json::Value =
            serde_json::from_str(&out.stdout).map_err(|e| GitError::Parse(e.to_string()))?;
        Ok(v.get("body")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned))
    }

    /// Menandai PR draft siap di-review (`gh pr ready`, KTD-68 plan VM).
    ///
    /// # Errors
    /// [`GitError::Command`] bila `gh pr ready` gagal atau melewati batas waktu.
    pub async fn pr_ready(
        &self,
        repo: &Path,
        url: &str,
        token: Option<&SecretString>,
    ) -> Result<(), GitError> {
        self.ok(
            "gh pr ready",
            self.gh(repo, token).args(["pr", "ready", url]),
        )
        .await?;
        Ok(())
    }

    /// Mengganti badan PR di `url`; badan dikirim lewat stdin.
    ///
    /// # Errors
    /// [`GitError::Command`] bila `gh pr edit` gagal.
    pub async fn set_pr_body(
        &self,
        repo: &Path,
        url: &str,
        body: &str,
        token: Option<&SecretString>,
    ) -> Result<(), GitError> {
        self.ok(
            "gh pr edit",
            self.gh(repo, token)
                .args(["pr", "edit", url, "--body-file", "-"])
                .stdin_bytes(body.as_bytes().to_vec()),
        )
        .await?;
        Ok(())
    }

    /// PR untuk `branch`, bila ada.
    ///
    /// # Errors
    /// [`GitError::Parse`] bila output `gh` tidak bisa diurai.
    pub async fn pr_for_branch(
        &self,
        repo: &Path,
        branch: &str,
        token: Option<&SecretString>,
    ) -> Result<Option<PrInfo>, GitError> {
        let out = self
            .exec(&self.gh(repo, token).args([
                "pr",
                "view",
                branch,
                "--json",
                "number,url,state,statusCheckRollup",
            ]))
            .await?;
        if !out.success() {
            if out.stderr.contains("no pull requests found") {
                return Ok(None);
            }
            return Err(GitError::Command {
                op: "gh pr view",
                stderr: out.stderr.trim().to_owned(),
            });
        }
        let pr: GhPr =
            serde_json::from_str(&out.stdout).map_err(|e| GitError::Parse(e.to_string()))?;
        let state = match pr.state.as_str() {
            "OPEN" => PrState::Open,
            "MERGED" => PrState::Merged,
            _ => PrState::Closed,
        };
        Ok(Some(PrInfo {
            number: pr.number,
            url: pr.url,
            state,
            checks: summarize_checks(&pr.status_check_rollup),
        }))
    }
}

/// Meringkas `statusCheckRollup` yang berisi `CheckRun` (status/conclusion)
/// maupun `StatusContext` (state).
#[must_use]
pub fn summarize_checks(rollup: &[serde_json::Value]) -> ChecksState {
    if rollup.is_empty() {
        return ChecksState::None;
    }
    let mut pending = false;
    for item in rollup {
        let field = |k: &str| {
            item.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_ascii_uppercase()
        };
        let (done, failed) = if item.get("state").is_some() && item.get("status").is_none() {
            let state = field("state");
            (
                !matches!(state.as_str(), "PENDING" | "EXPECTED"),
                matches!(state.as_str(), "FAILURE" | "ERROR"),
            )
        } else {
            let conclusion = field("conclusion");
            (
                field("status") == "COMPLETED",
                matches!(
                    conclusion.as_str(),
                    "FAILURE"
                        | "ERROR"
                        | "TIMED_OUT"
                        | "CANCELLED"
                        | "ACTION_REQUIRED"
                        | "STARTUP_FAILURE"
                ),
            )
        };
        if failed {
            return ChecksState::Failing;
        }
        pending |= !done;
    }
    if pending {
        ChecksState::Pending
    } else {
        ChecksState::Passing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn meringkas_check_run_dan_status_context() {
        let cases = [
            ("kosong", vec![], ChecksState::None),
            (
                "lulus",
                vec![json!({"status":"COMPLETED","conclusion":"SUCCESS"})],
                ChecksState::Passing,
            ),
            (
                "berjalan",
                vec![json!({"status":"IN_PROGRESS","conclusion":""})],
                ChecksState::Pending,
            ),
            (
                "dibatalkan gagal",
                vec![json!({"status":"COMPLETED","conclusion":"CANCELLED"})],
                ChecksState::Failing,
            ),
            (
                "status context pending",
                vec![json!({"state":"PENDING"})],
                ChecksState::Pending,
            ),
            (
                "status context sukses",
                vec![json!({"state":"SUCCESS"})],
                ChecksState::Passing,
            ),
            (
                "campuran gagal",
                vec![json!({"state":"SUCCESS"}), json!({"state":"FAILURE"})],
                ChecksState::Failing,
            ),
        ];
        for (name, rollup, want) in cases {
            assert_eq!(summarize_checks(&rollup), want, "{name}");
        }
    }
}
