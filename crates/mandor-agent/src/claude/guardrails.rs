//! Pagar izin agent di luar prompt (KTD-10, KTD-11, R11e).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use mandor_domain::hook::{ToolCall, Verdict};
use serde_json::{Value, json};

use crate::AgentError;

/// Batas ukuran input hook.
const HOOK_INPUT_MAX: u64 = 1024 * 1024;
/// Kode keluar hook yang memblokir tool call.
pub const HOOK_BLOCK: u8 = 2;

/// Allowlist default per repo: test, build, lint, dan git baca.
#[must_use]
pub fn default_allowed() -> Vec<String> {
    [
        "Read",
        "Edit",
        "Write",
        "MultiEdit",
        "Glob",
        "Grep",
        "TodoWrite",
        "NotebookEdit",
        // Skill plugin hanya memuat instruksi; tool yang dipakai skill tetap
        // melewati allowlist dan hook.
        "Skill",
        // Memuat skema tool MCP yang ditunda; hanya baca (spike A1).
        "ToolSearch",
        "Bash(git status*)",
        "Bash(git diff*)",
        "Bash(git log*)",
        "Bash(git show*)",
        "Bash(git branch --show-current)",
        "Bash(git rev-parse*)",
        "Bash(cargo *)",
        "Bash(make *)",
        "Bash(npm *)",
        "Bash(npx *)",
        "Bash(bun *)",
        "Bash(pnpm *)",
        "Bash(yarn *)",
        "Bash(node *)",
        "Bash(go *)",
        "Bash(python3 *)",
        "Bash(pytest*)",
        "Bash(ls*)",
        "Bash(cat *)",
        "Bash(head *)",
        "Bash(tail *)",
        "Bash(wc *)",
        "Bash(grep *)",
        "Bash(rg *)",
        "Bash(find *)",
        "Bash(mkdir *)",
        // Perintah shell tanpa efek di luar worktree; Claude memeriksa setiap
        // bagian perintah gabungan (`cd x && pnpm test`), dan hook tetap
        // memeriksa seluruh teks perintah.
        "Bash(cd *)",
        "Bash(pwd)",
        "Bash(echo *)",
        "Bash(printf *)",
        "Bash(sed *)",
        "Bash(awk *)",
        "Bash(sort *)",
        "Bash(uniq *)",
        "Bash(cut *)",
        "Bash(tr *)",
        "Bash(diff *)",
        "Bash(test *)",
        "Bash(true)",
        "Bash(which *)",
        "Bash(basename *)",
        "Bash(dirname *)",
        "Bash(jq *)",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// Deny list: commit/push/gh, tulis konfigurasi agent dan hook, baca kredensial,
/// serta tulis ke path pendamping read-only.
#[must_use]
pub fn default_denied(companions: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = [
        "Bash(git commit*)",
        "Bash(git push*)",
        "Bash(gh *)",
        "Bash(curl *larksuite*)",
        "Bash(curl *feishu*)",
        "EnterWorktree",
        "ExitWorktree",
        "Edit(.claude/**)",
        "Write(.claude/**)",
        "Edit(.husky/**)",
        "Write(.husky/**)",
        "Edit(.githooks/**)",
        "Write(.githooks/**)",
        "Edit(.git/**)",
        "Write(.git/**)",
        "Read(~/.config/gh/**)",
        "Read(~/.mandor/**)",
        "Read(~/.git-credentials)",
        "Read(~/.netrc)",
        "Read(~/.ssh/**)",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for dir in companions {
        let d = dir.display();
        out.push(format!("Edit({d}/**)"));
        out.push(format!("Write({d}/**)"));
    }
    out
}

/// Settings per pemanggilan: hook `PreToolUse` untuk semua tool lewat path absolut.
#[must_use]
pub fn settings_json(hook_command: &str) -> String {
    json!({
        "disableAllHooks": false,
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{"type": "command", "command": hook_command, "timeout": 30}]
            }]
        }
    })
    .to_string()
}

/// Settings sesi reviewer: `PreToolUse` seperti biasa ditambah `PostToolUse`
/// untuk tool browser, yang mencatat tab dan alamat terakhirnya (KTD-31).
#[must_use]
pub fn review_settings_json(hook_command: &str, post_command: &str) -> String {
    json!({
        "disableAllHooks": false,
        "hooks": {
            "PreToolUse": [{
                "matcher": "*",
                "hooks": [{"type": "command", "command": hook_command, "timeout": 30}]
            }],
            "PostToolUse": [{
                "matcher": "mcp__claude-in-chrome__.*",
                "hooks": [{"type": "command", "command": post_command, "timeout": 30}]
            }]
        }
    })
    .to_string()
}

/// Variabel yang selalu dibuang dari environment agent.
fn is_secret_var(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ["TOKEN", "SECRET", "PASSWORD", "API_KEY", "CREDENTIAL"]
        .iter()
        .any(|m| upper.contains(m))
        || upper.starts_with("LARK_")
        || upper.starts_with("MANDOR_")
        || upper.starts_with("GIT_CONFIG")
        || upper.starts_with("GCM_")
        || matches!(
            upper.as_str(),
            "SSH_AUTH_SOCK"
                | "GIT_ASKPASS"
                | "SSH_ASKPASS"
                | "GH_HOST"
                | "GH_CONFIG_DIR"
                | "GIT_SSH_COMMAND"
        )
}

/// Direktori konfigurasi git dan gh terisolasi milik agent.
#[derive(Debug, Clone)]
pub struct AgentHome {
    pub gitconfig: PathBuf,
    pub gh_dir: PathBuf,
}

/// Menyiapkan `<base>/agent/gitconfig` (identitas, `credential.helper` kosong)
/// dan direktori gh kosong.
///
/// # Errors
/// [`AgentError::Io`] bila berkas tidak bisa ditulis.
pub async fn prepare_agent_home(base: &Path) -> Result<AgentHome, AgentError> {
    let dir = base.join("agent");
    let gh_dir = dir.join("gh");
    let io = |e: std::io::Error| AgentError::Io(e.to_string());
    tokio::fs::create_dir_all(&gh_dir).await.map_err(io)?;
    let gitconfig = dir.join("gitconfig");
    let body = "[user]\n\tname = mandor agent\n\temail = agent@mandor.invalid\n\
                [credential]\n\thelper =\n\
                [core]\n\tfsmonitor = false\n";
    tokio::fs::write(&gitconfig, body).await.map_err(io)?;
    Ok(AgentHome { gitconfig, gh_dir })
}

/// Env login shell tanpa secret: token, kunci API, SSH agent, dan variabel
/// Lark/Mandor dibuang. Basis env agent dan proses app (KTD-36).
#[must_use]
pub fn clean_env(login_env: &BTreeMap<String, String>) -> Vec<(String, String)> {
    login_env
        .iter()
        .filter(|(k, _)| !is_secret_var(k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Environment agent: env login shell tanpa secret, ditambah config git/gh
/// terisolasi sehingga credential helper global dan kunci SSH tidak terpakai.
#[must_use]
pub fn agent_env(login_env: &BTreeMap<String, String>, home: &AgentHome) -> Vec<(String, String)> {
    let mut env = clean_env(login_env);
    let set = [
        ("GIT_CONFIG_NOSYSTEM", "1".to_owned()),
        ("GIT_CONFIG_GLOBAL", home.gitconfig.display().to_string()),
        ("GH_CONFIG_DIR", home.gh_dir.display().to_string()),
        ("GIT_TERMINAL_PROMPT", "0".to_owned()),
        (
            "GIT_SSH_COMMAND",
            "ssh -F /dev/null -o BatchMode=yes -o IdentitiesOnly=yes -o IdentityAgent=none -o IdentityFile=/dev/null"
                .to_owned(),
        ),
    ];
    env.extend(set.into_iter().map(|(k, v)| (k.to_owned(), v)));
    env
}

/// Mengurai input hook `PreToolUse` dari Claude Code.
///
/// # Errors
/// Pesan bila JSON rusak atau field wajib tidak ada.
pub fn parse_hook_input(raw: &[u8]) -> Result<ToolCall, String> {
    let v: Value = serde_json::from_slice(raw).map_err(|e| format!("malformed hook input: {e}"))?;
    let tool_name = v
        .get("tool_name")
        .and_then(Value::as_str)
        .ok_or("hook input has no tool_name")?
        .to_owned();
    let input = v.get("tool_input").cloned().unwrap_or(Value::Null);
    let command = input
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let paths = ["file_path", "path", "notebook_path"]
        .iter()
        .filter_map(|k| input.get(*k).and_then(Value::as_str).map(str::to_owned))
        .collect();
    let text = |k: &str| input.get(k).and_then(Value::as_str).map(str::to_owned);
    Ok(ToolCall {
        tool_name,
        command,
        paths,
        url: text("url"),
        tab_id: input.get("tabId").and_then(Value::as_u64),
        action: text("action"),
        response: v.get("tool_response").map(response_text),
    })
}

/// Teks respons tool untuk `PostToolUse`: bagian `text` digabung; gambar dan
/// bagian lain dibuang.
fn response_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.get("content").map_or_else(String::new, response_text),
    }
}

/// Menjalankan hook secara fail-closed: input rusak, error baca, atau panic
/// menghasilkan [`HOOK_BLOCK`]. Alasan penolakan ditulis ke `err`.
pub fn run_hook<R, W, F>(input: R, err: &mut W, decide: F) -> u8
where
    R: Read,
    W: Write,
    F: Fn(&ToolCall) -> Verdict,
{
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let mut raw = Vec::new();
        input
            .take(HOOK_INPUT_MAX)
            .read_to_end(&mut raw)
            .map_err(|e| format!("cannot read hook input: {e}"))?;
        let call = parse_hook_input(&raw)?;
        Ok::<Verdict, String>(decide(&call))
    }));
    let reason = match outcome {
        Ok(Ok(Verdict::Allow)) => return 0,
        Ok(Ok(Verdict::Deny(reason)) | Err(reason)) => reason,
        Err(_) => "mandor: internal hook failure; tool call denied".to_owned(),
    };
    let _ = writeln!(err, "{reason}");
    HOOK_BLOCK
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandor_domain::hook::classify;

    fn hook(input: &str) -> (u8, String) {
        let mut err = Vec::new();
        let code = run_hook(input.as_bytes(), &mut err, classify);
        (code, String::from_utf8_lossy(&err).into_owned())
    }

    fn bash(cmd: &str) -> String {
        json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": cmd}}).to_string()
    }

    #[test]
    fn hook_menolak_push_gh_dan_kredensial_mengizinkan_cargo_test() {
        for cmd in [
            "git push origin main",
            "gh pr create",
            "cat ~/.config/gh/hosts.yml",
        ] {
            let (code, msg) = hook(&bash(cmd));
            assert_eq!(code, HOOK_BLOCK, "{cmd}");
            assert!(msg.starts_with("mandor:"), "{msg}");
        }
        assert_eq!(hook(&bash("cargo test")).0, 0);
        let write = json!({"tool_name": "Write", "tool_input": {"file_path": "/w/.claude/settings.local.json"}});
        assert_eq!(hook(&write.to_string()).0, HOOK_BLOCK);
    }

    #[test]
    fn settings_review_mendaftarkan_post_hook_untuk_tool_browser() {
        let v: Value = serde_json::from_str(&review_settings_json(
            "/opt/mandord hook --review /s.json",
            "/opt/mandord hook --review /s.json --post",
        ))
        .unwrap();
        assert_eq!(
            v["hooks"]["PostToolUse"][0]["matcher"],
            "mcp__claude-in-chrome__.*"
        );
        assert_eq!(
            v["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "/opt/mandord hook --review /s.json --post"
        );
        assert_eq!(v["hooks"]["PreToolUse"][0]["matcher"], "*");
        let plain: Value = serde_json::from_str(&settings_json("/opt/mandord hook")).unwrap();
        assert!(plain["hooks"].get("PostToolUse").is_none());
    }

    #[test]
    fn respons_tool_digabung_tanpa_gambar() {
        let raw = json!({
            "tool_name": "mcp__claude-in-chrome__computer",
            "tool_input": {"action": "screenshot", "tabId": 7},
            "tool_response": [
                {"type": "text", "text": "Captured"},
                {"type": "image", "source": {"data": "AAAA"}},
                {"type": "text", "text": "Tab Context"}
            ]
        });
        let call = parse_hook_input(raw.to_string().as_bytes()).unwrap();
        assert_eq!(call.response.as_deref(), Some("Captured\nTab Context"));
        assert_eq!(call.tab_id, Some(7));
        assert_eq!(call.action.as_deref(), Some("screenshot"));
    }

    #[test]
    fn input_rusak_dan_panic_sama_sama_menolak() {
        assert_eq!(hook("{bukan json").0, HOOK_BLOCK);
        assert_eq!(hook("{}").0, HOOK_BLOCK);
        assert_eq!(hook("").0, HOOK_BLOCK);
        let mut err = Vec::new();
        let code = run_hook(bash("ls").as_bytes(), &mut err, |_| panic!("simulasi"));
        assert_eq!(code, HOOK_BLOCK);
        assert!(String::from_utf8_lossy(&err).contains("internal hook failure"));
    }

    #[test]
    fn env_agent_tanpa_token_dan_dengan_config_terisolasi() {
        let mut login = BTreeMap::new();
        for (k, v) in [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/dev"),
            ("GH_TOKEN", "ghp_x"),
            ("GITHUB_TOKEN", "ghs_y"),
            ("LARK_APP_SECRET", "s"),
            ("ANTHROPIC_API_KEY", "k"),
            ("SSH_AUTH_SOCK", "/tmp/agent.sock"),
            ("GIT_CONFIG_GLOBAL", "/home/dev/.gitconfig"),
        ] {
            login.insert(k.to_owned(), v.to_owned());
        }
        let home = AgentHome {
            gitconfig: "/k/agent/gitconfig".into(),
            gh_dir: "/k/agent/gh".into(),
        };
        let env: BTreeMap<_, _> = agent_env(&login, &home).into_iter().collect();
        for gone in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "LARK_APP_SECRET",
            "ANTHROPIC_API_KEY",
            "SSH_AUTH_SOCK",
        ] {
            assert!(!env.contains_key(gone), "{gone}");
        }
        assert_eq!(env["PATH"], "/usr/bin");
        assert_eq!(env["GIT_CONFIG_GLOBAL"], "/k/agent/gitconfig");
        assert_eq!(env["GH_CONFIG_DIR"], "/k/agent/gh");
        assert_eq!(env["GIT_CONFIG_NOSYSTEM"], "1");
    }

    #[test]
    fn settings_memuat_hook_absolut_dan_tidak_menonaktifkan_hook() {
        let s: Value =
            serde_json::from_str(&settings_json("/opt/mandord hook")).unwrap_or_default();
        assert_eq!(s["disableAllHooks"], json!(false));
        assert_eq!(
            s["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
            json!("/opt/mandord hook")
        );
        // Perintah shell aman yang dipakai agent di perintah gabungan (`cd x && ...`).
        for safe in [
            "Bash(cd *)",
            "Bash(pwd)",
            "Bash(echo *)",
            "Bash(sed *)",
            "Bash(sort *)",
        ] {
            assert!(default_allowed().contains(&safe.to_owned()), "{safe}");
        }
        let denied = default_denied(&[PathBuf::from("/specs")]);
        assert!(denied.contains(&"Bash(git push*)".to_owned()));
        assert!(denied.contains(&"Write(/specs/**)".to_owned()));
        assert!(
            !default_allowed()
                .iter()
                .any(|a| a.contains("commit") || a.contains("push"))
        );
    }
}
