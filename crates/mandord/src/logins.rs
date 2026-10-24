//! Status tiga login daemon VM untuk RPC `status` (KTD-66 plan VM): Lark di
//! daemon, `gh` di user developer, dan Claude Code di user agent. Masing-masing
//! `ok`, `missing`, atau `expired` beserta langkah perbaikannya.

use std::time::Duration;

use mandor_agent::claude::command::{AgentRunner, claude_cmd};
use mandor_proc::{Cmd, Output, ProcError, run};
use mandor_protocol::{LarkStatus, LoginCheck};

use crate::config::{Config, VmAgent};

/// Batas waktu satu pemeriksaan login.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const RUNBOOK: &str = "docs/runbook/setup-vm.md";

/// Status ketiga login, diperiksa bersamaan.
pub async fn check(cfg: &Config, vm: &VmAgent, lark: &LarkStatus) -> Vec<LoginCheck> {
    let gh = Cmd::new(&cfg.gh_bin)
        .args(["auth", "status", "--hostname", "github.com"])
        .scrub_tokens()
        .timeout(CHECK_TIMEOUT);
    let runner = AgentRunner {
        exe: cfg.exe.clone(),
        user: vm.user.clone(),
        run_dir: vm.run_dir(),
    };
    let claude = claude_cmd(&cfg.claude_bin, Some(&runner), &cfg.base_env)
        .args(["auth", "status", "--json"])
        .timeout(CHECK_TIMEOUT);
    let (gh, claude) = tokio::join!(run(&gh), run(&claude));
    vec![
        lark_check(lark),
        gh_check(&gh),
        claude_check(&claude, &vm.user),
    ]
}

fn login(service: &str, state: &str, fix: Option<String>) -> LoginCheck {
    LoginCheck {
        service: service.to_owned(),
        state: state.to_owned(),
        fix,
    }
}

fn lark_check(l: &LarkStatus) -> LoginCheck {
    if l.logged_in && !l.reauth_required {
        return login("lark", "ok", None);
    }
    let (state, why) = if l.reauth_required {
        ("expired", "The Lark session expired. ")
    } else if !l.configured {
        ("missing", "Set up the Lark App in the app, then ")
    } else {
        ("missing", "")
    };
    login(
        "lark",
        state,
        Some(format!(
            "{why}Run `mandord login lark` on the VM (see {RUNBOOK})."
        )),
    )
}

fn gh_check(out: &Result<Output, ProcError>) -> LoginCheck {
    match out {
        Ok(o) if o.success() => login("gh", "ok", None),
        Ok(o) if mentions_expired(&o.stderr) || mentions_expired(&o.stdout) => login(
            "gh",
            "expired",
            Some(
                "The GitHub token is no longer valid. Run `gh auth login` as your user on the VM."
                    .to_owned(),
            ),
        ),
        _ => login(
            "gh",
            "missing",
            Some(format!(
                "Run `gh auth login` as your user on the VM (see {RUNBOOK})."
            )),
        ),
    }
}

fn mentions_expired(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("invalid") || t.contains("expired")
}

fn claude_check(out: &Result<Output, ProcError>, agent: &str) -> LoginCheck {
    let logged_in = out.as_ref().is_ok_and(|o| {
        serde_json::from_str::<serde_json::Value>(&o.stdout)
            .ok()
            .and_then(|v| v.get("loggedIn").and_then(serde_json::Value::as_bool))
            .unwrap_or(false)
    });
    if logged_in {
        login("claude", "ok", None)
    } else {
        login(
            "claude",
            "missing",
            Some(format!(
                "Log in to Claude Code as the agent user {agent}, as described in {RUNBOOK}."
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(code: i32, stdout: &str, stderr: &str) -> Output {
        Output {
            code: Some(code),
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn status_lark_dari_sesi_daemon() {
        let mut l = LarkStatus {
            configured: true,
            logged_in: true,
            open_id: None,
            name: None,
            reauth_required: false,
        };
        assert_eq!(lark_check(&l).state, "ok");
        l.reauth_required = true;
        assert_eq!(lark_check(&l).state, "expired");
        l = LarkStatus {
            configured: false,
            logged_in: false,
            reauth_required: false,
            ..l
        };
        let c = lark_check(&l);
        assert_eq!(c.state, "missing");
        assert!(c.fix.unwrap().contains("mandord login lark"));
    }

    #[test]
    fn status_gh_dan_claude_dari_keluaran_perintah() {
        let gh = |code, err: &str| gh_check(&Ok(out(code, "", err))).state;
        assert_eq!(gh(0, "Logged in"), "ok");
        assert_eq!(gh(1, "The token in GH_CONFIG is invalid."), "expired");
        assert_eq!(
            gh(1, "You are not logged into any GitHub hosts."),
            "missing"
        );
        let ok = claude_check(&Ok(out(0, r#"{"loggedIn":true}"#, "")), "a");
        assert_eq!(ok.state, "ok");
        let c = claude_check(&Ok(out(1, "", "sudo: unknown user")), "beni-agent");
        assert_eq!(c.state, "missing");
        assert!(c.fix.unwrap().contains("beni-agent"));
    }
}
