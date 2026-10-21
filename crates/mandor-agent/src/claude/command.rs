//! Perintah `claude -p` per giliran (KTD-10).

use std::path::PathBuf;

use mandor_proc::{Cmd, EnvPolicy};

use super::guardrails::settings_json;

/// Sesi baru dengan ID pilihan daemon, atau lanjutan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    New(String),
    Resume(String),
}

/// Semua yang dibutuhkan untuk satu spawn agent.
#[derive(Debug, Clone)]
pub struct SessionSpec {
    pub claude_bin: PathBuf,
    pub worktree: PathBuf,
    pub session: Session,
    /// Operator prompt; dikirim di setiap spawn agar tidak hilang saat resume.
    pub operator_prompt: String,
    pub model: Option<String>,
    pub allowed: Vec<String>,
    pub denied: Vec<String>,
    /// Path pendamping read-only (`--add-dir`).
    pub companions: Vec<PathBuf>,
    /// Perintah hook absolut, mis. `/usr/local/bin/mandord hook`.
    pub hook_command: String,
    /// Environment lengkap proses agent (lihat `agent_env`).
    pub env: Vec<(String, String)>,
    /// Berkas `--mcp-config` berisi MCP server yang diizinkan (run workspace).
    pub mcp_config: Option<PathBuf>,
    /// Giliran plan: mode izin `plan` (hanya baca) menggantikan `acceptEdits`.
    pub plan_mode: bool,
}

/// Argumen `claude`. Pesan user dikirim lewat stdin, bukan argv.
#[must_use]
pub fn args(spec: &SessionSpec) -> Vec<String> {
    let mut a: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode",
        if spec.plan_mode {
            "plan"
        } else {
            "acceptEdits"
        },
        "--permission-prompts",
        "none",
        "--setting-sources",
        "user,project",
        "--strict-mcp-config",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if let Some(path) = &spec.mcp_config {
        a.push("--mcp-config".to_owned());
        a.push(path.display().to_string());
    }
    a.push("--settings".to_owned());
    a.push(settings_json(&spec.hook_command));
    if !spec.allowed.is_empty() {
        a.push("--allowedTools".to_owned());
        a.push(spec.allowed.join(","));
    }
    if !spec.denied.is_empty() {
        a.push("--disallowedTools".to_owned());
        a.push(spec.denied.join(","));
    }
    a.push("--append-system-prompt".to_owned());
    a.push(spec.operator_prompt.clone());
    if let Some(m) = &spec.model {
        a.push("--model".to_owned());
        a.push(m.clone());
    }
    match &spec.session {
        Session::New(id) => {
            a.push("--session-id".to_owned());
            a.push(id.clone());
        }
        Session::Resume(id) => {
            a.push("--resume".to_owned());
            a.push(id.clone());
        }
    }
    for dir in &spec.companions {
        a.push("--add-dir".to_owned());
        a.push(dir.display().to_string());
    }
    a
}

/// [`Cmd`] siap dijalankan dengan environment eksplisit.
#[must_use]
pub fn build(spec: &SessionSpec) -> Cmd {
    Cmd::new(&spec.claude_bin)
        .args(args(spec))
        .cwd(&spec.worktree)
        .env_policy(EnvPolicy::Explicit(spec.env.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(session: Session) -> SessionSpec {
        SessionSpec {
            claude_bin: "claude".into(),
            worktree: "/w/pgs-1".into(),
            session,
            operator_prompt: "OP".to_owned(),
            model: Some("sonnet".to_owned()),
            allowed: vec!["Bash(cargo *)".to_owned(), "Read".to_owned()],
            denied: vec!["Bash(git push*)".to_owned()],
            companions: vec!["/specs".into()],
            hook_command: "/opt/mandord hook".to_owned(),
            env: vec![],
            mcp_config: None,
            plan_mode: false,
        }
    }

    #[test]
    fn giliran_plan_memakai_mode_izin_plan() {
        let mut s = spec(Session::New("u-1".to_owned()));
        s.plan_mode = true;
        let a = args(&s);
        assert_eq!(after(&a, "--permission-mode"), Some("plan"));
        assert!(a.contains(&"--strict-mcp-config".to_owned()));
    }

    fn after<'a>(a: &'a [String], flag: &str) -> Option<&'a str> {
        a.iter()
            .position(|x| x == flag)
            .and_then(|i| a.get(i + 1))
            .map(String::as_str)
    }

    #[test]
    fn mcp_config_hanya_dikirim_bila_ada_server_terpilih() {
        let a = args(&spec(Session::New("u-1".to_owned())));
        assert!(!a.contains(&"--mcp-config".to_owned()));
        let mut with = spec(Session::New("u-1".to_owned()));
        with.mcp_config = Some("/home/u/.mandor/agent/mcp/x.json".into());
        let a = args(&with);
        assert_eq!(
            after(&a, "--mcp-config"),
            Some("/home/u/.mandor/agent/mcp/x.json")
        );
        // Tetap strict: hanya server di berkas itu yang dimuat.
        assert!(a.contains(&"--strict-mcp-config".to_owned()));
    }

    #[test]
    fn sesi_baru_memakai_pagar_lengkap_tanpa_bypass() {
        let a = args(&spec(Session::New("u-1".to_owned())));
        assert_eq!(after(&a, "--permission-mode"), Some("acceptEdits"));
        assert_eq!(after(&a, "--permission-prompts"), Some("none"));
        assert_eq!(after(&a, "--setting-sources"), Some("user,project"));
        assert_eq!(after(&a, "--session-id"), Some("u-1"));
        assert_eq!(after(&a, "--allowedTools"), Some("Bash(cargo *),Read"));
        assert_eq!(after(&a, "--disallowedTools"), Some("Bash(git push*)"));
        assert_eq!(after(&a, "--add-dir"), Some("/specs"));
        assert!(after(&a, "--settings").is_some_and(|s| s.contains("/opt/mandord hook")));
        assert!(a.contains(&"--strict-mcp-config".to_owned()));
        assert!(
            !a.iter()
                .any(|x| x.contains("bypass") || x.contains("dangerously"))
        );
    }

    #[test]
    fn resume_tetap_mengirim_operator_prompt_dan_hook() {
        let a = args(&spec(Session::Resume("u-1".to_owned())));
        assert_eq!(after(&a, "--resume"), Some("u-1"));
        assert!(!a.contains(&"--session-id".to_owned()));
        assert_eq!(after(&a, "--append-system-prompt"), Some("OP"));
        assert!(a.contains(&"--settings".to_owned()));
    }
}
