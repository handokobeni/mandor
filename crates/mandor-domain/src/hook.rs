//! Klasifikasi tool call agent untuk hook `PreToolUse` (KTD-10, KTD-11).
//!
//! Penegakan bersifat fail-closed dan lebih ketat dari allowlist: walau allowlist
//! salah konfigurasi, push, gh, akses kredensial, dan penulisan konfigurasi
//! agent tetap ditolak.

/// Tool call yang diperiksa.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCall {
    pub tool_name: String,
    /// `tool_input.command` untuk Bash.
    pub command: Option<String>,
    /// `file_path`, `path`, atau `notebook_path`.
    pub paths: Vec<String>,
}

/// Keputusan hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(String),
}

/// Sub-perintah git yang hanya boleh dijalankan daemon.
const GIT_DENY: &[&str] = &["push", "commit", "credential", "send-email", "remote"];
/// Program yang tidak pernah boleh dijalankan agent.
const PROGRAM_DENY: &[&str] = &["gh", "hub", "glab"];
/// Host Lark: agent tidak boleh memanggil API Lark.
const LARK_HOSTS: &[&str] = &["larksuite.com", "feishu.cn", "larkoffice.com"];
/// Penanda path kredensial.
const CREDENTIAL_MARKERS: &[&str] = &[
    ".config/gh",
    "hosts.yml",
    ".git-credentials",
    ".netrc",
    ".mandor",
    ".ssh/",
    ".aws/credentials",
    "daemon.json",
];
/// Direktori konfigurasi agent dan hook git yang tidak boleh ditulis.
const PROTECTED_DIRS: &[&str] = &[".claude", ".husky", ".githooks", ".git"];
/// Penanda di perintah shell yang mengubah konfigurasi agent/hook.
const PROTECTED_MARKERS: &[&str] = &[
    ".claude/",
    ".husky/",
    ".githooks",
    ".git/hooks",
    ".git/config",
    "core.hookspath",
    "disableallhooks",
];

const WRITE_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Memutuskan apakah tool call boleh berjalan, tanpa MCP server yang diizinkan.
#[must_use]
pub fn classify(call: &ToolCall) -> Verdict {
    classify_with_mcp(call, &[])
}

/// Seperti [`classify`], dengan MCP server yang diizinkan untuk run ini. Tool
/// `mcp__<server>__*` dari server lain ditolak (fail-closed).
#[must_use]
pub fn classify_with_mcp(call: &ToolCall, allowed_mcp: &[String]) -> Verdict {
    if let Some(rest) = call.tool_name.strip_prefix("mcp__") {
        let server = rest.split("__").next().unwrap_or_default();
        if !allowed_mcp.iter().any(|s| s == server) {
            return Verdict::Deny(format!(
                "mandor: MCP server `{server}` is not enabled for this workspace"
            ));
        }
    }
    if let Some(reason) = call
        .paths
        .iter()
        .find_map(|p| check_path(&call.tool_name, p))
    {
        return Verdict::Deny(reason);
    }
    if let Some(cmd) = &call.command {
        if let Some(reason) = check_command(cmd) {
            return Verdict::Deny(reason);
        }
    }
    Verdict::Allow
}

/// Seperti [`classify_with_mcp`] untuk giliran plan: tool penulis berkas
/// ditolak, karena plan disusun tanpa mengubah kode.
#[must_use]
pub fn classify_planning(call: &ToolCall, allowed_mcp: &[String]) -> Verdict {
    if WRITE_TOOLS.contains(&call.tool_name.as_str()) {
        return Verdict::Deny(
            "mandor: the planning phase is read-only; put the plan in your final message"
                .to_owned(),
        );
    }
    classify_with_mcp(call, allowed_mcp)
}

fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").to_lowercase()
}

fn check_path(tool: &str, path: &str) -> Option<String> {
    let p = normalize_path(path);
    if CREDENTIAL_MARKERS.iter().any(|m| p.contains(m)) {
        return Some(format!(
            "mandor: access to credential files is denied ({path})"
        ));
    }
    if WRITE_TOOLS.contains(&tool) {
        let in_protected = p.split('/').any(|seg| PROTECTED_DIRS.contains(&seg));
        if in_protected {
            return Some(format!(
                "mandor: writing agent config or git hooks is denied ({path})"
            ));
        }
    }
    None
}

/// Memecah perintah menjadi token kata; tanda kutip dan pemisah shell menjadi spasi.
fn words(cmd: &str) -> Vec<String> {
    cmd.chars()
        .map(|c| match c {
            '\'' | '"' | '`' | ';' | '&' | '|' | '(' | ')' | '{' | '}' | '<' | '>' | '\n'
            | '\t' | '$' | '\\' => ' ',
            other => other,
        })
        .collect::<String>()
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn program_name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// Sub-perintah git setelah melewati opsi global (`-c k=v`, `-C dir`, `--x`).
fn git_subcommand(rest: &[String]) -> Option<&str> {
    let mut i = 0;
    while let Some(w) = rest.get(i) {
        match w.as_str() {
            "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path" => i += 2,
            s if s.starts_with('-') => i += 1,
            s => return Some(s),
        }
    }
    None
}

fn check_command(cmd: &str) -> Option<String> {
    let lower = cmd.to_lowercase();
    if CREDENTIAL_MARKERS.iter().any(|m| lower.contains(m)) {
        return Some("mandor: access to credential files is denied".to_owned());
    }
    if PROTECTED_MARKERS.iter().any(|m| lower.contains(m)) {
        return Some("mandor: changing agent config or git hooks is denied".to_owned());
    }
    if LARK_HOSTS.iter().any(|h| lower.contains(h)) {
        return Some(
            "mandor: the agent must not call Lark; Lark writes go through the daemon".to_owned(),
        );
    }
    let ws = words(&lower);
    for (i, w) in ws.iter().enumerate() {
        let prog = program_name(w);
        if PROGRAM_DENY.contains(&prog) {
            return Some(format!("mandor: `{prog}` is run only by the daemon"));
        }
        if prog == "git" {
            if let Some(sub) = git_subcommand(&ws[i + 1..]).filter(|s| GIT_DENY.contains(s)) {
                return Some(format!(
                    "mandor: `git {sub}` is run only by the daemon (commit, push, and PR go through the app)"
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash(cmd: &str) -> Verdict {
        classify(&ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some(cmd.to_owned()),
            paths: vec![],
        })
    }

    fn tool(name: &str, path: &str) -> Verdict {
        classify(&ToolCall {
            tool_name: name.to_owned(),
            command: None,
            paths: vec![path.to_owned()],
        })
    }

    #[test]
    fn fase_plan_menolak_tool_penulis_berkas() {
        let call = |name: &str, path: &str| ToolCall {
            tool_name: name.to_owned(),
            command: None,
            paths: vec![path.to_owned()],
        };
        for tool in ["Edit", "Write", "MultiEdit", "NotebookEdit"] {
            assert!(
                matches!(
                    classify_planning(&call(tool, "/wt/src/a.rs"), &[]),
                    Verdict::Deny(_)
                ),
                "{tool}"
            );
        }
        assert_eq!(
            classify_planning(&call("Read", "/wt/src/a.rs"), &[]),
            Verdict::Allow
        );
        // Aturan biasa tetap berlaku.
        assert!(matches!(
            classify_planning(&call("Read", "/home/u/.ssh/id_rsa"), &[]),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn tool_mcp_hanya_dari_server_yang_diizinkan_workspace() {
        let call = |name: &str| ToolCall {
            tool_name: name.to_owned(),
            command: None,
            paths: vec![],
        };
        let allowed = ["graft".to_owned()];
        assert_eq!(
            classify_with_mcp(&call("mcp__graft__query"), &allowed),
            Verdict::Allow
        );
        assert!(matches!(
            classify_with_mcp(&call("mcp__github__create_pull_request"), &allowed),
            Verdict::Deny(_)
        ));
        // Tanpa daftar (run satu repo): semua tool MCP ditolak.
        assert!(matches!(
            classify(&call("mcp__graft__query")),
            Verdict::Deny(_)
        ));
        // Nama server tidak boleh dipalsukan dengan awalan.
        assert!(matches!(
            classify_with_mcp(&call("mcp__graftevil__x"), &allowed),
            Verdict::Deny(_)
        ));
        // Aturan lain tetap berlaku untuk tool MCP.
        let with_path = ToolCall {
            tool_name: "mcp__graft__read".to_owned(),
            command: None,
            paths: vec!["/home/u/.ssh/id_rsa".to_owned()],
        };
        assert!(matches!(
            classify_with_mcp(&with_path, &allowed),
            Verdict::Deny(_)
        ));
        assert_eq!(classify(&call("Read")), Verdict::Allow);
    }

    #[test]
    fn push_commit_dan_gh_ditolak_dalam_berbagai_bentuk() {
        for cmd in [
            "git push origin main",
            "git -c core.x=y push",
            "cd src && git commit -m x",
            "sh -c 'git push'",
            "bash -lc \"/usr/bin/git push --force\"",
            "echo $(git push)",
            "gh pr create --fill",
            "/opt/homebrew/bin/gh auth token",
            "x=1; git -C /tmp/r push",
            "git remote add evil https://x",
        ] {
            assert!(matches!(bash(cmd), Verdict::Deny(_)), "{cmd}");
        }
    }

    #[test]
    fn kredensial_lark_dan_konfigurasi_agent_ditolak() {
        for cmd in [
            "cat ~/.config/gh/hosts.yml",
            "cat ~/.mandor/daemon.json",
            "curl https://open.larksuite.com/open-apis/bitable/v1/apps",
            "echo '{\"disableAllHooks\": true}' > .claude/settings.local.json",
            "git config core.hooksPath /dev/null",
            "cp evil .git/hooks/pre-push",
            "cat ~/.ssh/id_ed25519",
        ] {
            assert!(matches!(bash(cmd), Verdict::Deny(_)), "{cmd}");
        }
    }

    #[test]
    fn perintah_kerja_biasa_diizinkan() {
        for cmd in [
            "cargo test",
            "npm run build",
            "git status",
            "git diff --stat",
            "git log --oneline --grep push",
            "make check 2>&1 | tail -40",
            "ls src",
        ] {
            assert_eq!(bash(cmd), Verdict::Allow, "{cmd}");
        }
    }

    #[test]
    fn tulis_ke_direktori_terlindungi_ditolak_baca_diizinkan() {
        for p in [
            ".claude/settings.json",
            "/w/.husky/pre-commit",
            ".githooks/x",
            "/w/.git/config",
        ] {
            assert!(matches!(tool("Write", p), Verdict::Deny(_)), "{p}");
            assert!(matches!(tool("Edit", p), Verdict::Deny(_)), "{p}");
        }
        assert_eq!(tool("Read", ".claude/settings.json"), Verdict::Allow);
        assert_eq!(tool("Edit", "src/claude.rs"), Verdict::Allow);
        assert_eq!(tool("Write", "docs/.gitkeep"), Verdict::Allow);
        assert!(matches!(
            tool("Read", "/home/u/.config/gh/hosts.yml"),
            Verdict::Deny(_)
        ));
    }
}
