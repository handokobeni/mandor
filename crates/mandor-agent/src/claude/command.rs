//! Perintah `claude -p` per giliran (KTD-10).

use std::path::PathBuf;

use mandor_proc::{Cmd, EnvPolicy};

use super::guardrails::{review_settings_json, settings_json};

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
    /// Sesi reviewer: hook `PostToolUse`, browser, dan batas biaya.
    pub review: Option<ReviewFlags>,
    /// Daemon VM: agent dijalankan sebagai user Linux terpisah lewat
    /// `sudo -n -u <user> mandord agent-exec` (KTD-62 plan VM).
    pub runner: Option<AgentRunner>,
}

/// Cara menjalankan agent sebagai user agent di VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunner {
    /// Biner `mandord` yang bisa dieksekusi user agent.
    pub exe: PathBuf,
    /// User Linux agent, mis. `beni-agent`.
    pub user: String,
    /// Direktori berkas run yang bisa dibaca user agent.
    pub run_dir: PathBuf,
}

/// `sudo` absolut: environment eksplisit tanpa `PATH` warisan.
const SUDO: &str = "/usr/bin/sudo";

/// Variabel yang boleh sampai ke agent di VM. Disaring di daemon sebelum
/// argv `sudo` (argv terlihat di daftar proses) dan disaring lagi oleh
/// `agent-exec` di sisi user agent.
pub const AGENT_ENV_ALLOWED: &[&str] = &[
    "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TZ", "TMPDIR",
];

/// Argumen `sudo` sampai `--` sebelum program agent; hanya variabel
/// [`AGENT_ENV_ALLOWED`] yang ikut.
#[must_use]
pub fn runner_args(r: &AgentRunner, env: &[(String, String)]) -> Vec<String> {
    let mut a = vec![
        "-n".to_owned(),
        "-u".to_owned(),
        r.user.clone(),
        r.exe.display().to_string(),
        "agent-exec".to_owned(),
        "--run-dir".to_owned(),
        r.run_dir.display().to_string(),
    ];
    for (k, v) in env
        .iter()
        .filter(|(k, _)| AGENT_ENV_ALLOWED.contains(&k.as_str()))
    {
        a.push("--env".to_owned());
        a.push(format!("{k}={v}"));
    }
    a.push("--".to_owned());
    a
}

/// Argumen tambahan sesi reviewer (KTD-29–KTD-31).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFlags {
    /// Perintah hook `PostToolUse` (`mandord hook --review <state> --post`).
    pub post_hook_command: String,
    /// `--chrome` hanya bila semua syarat kesiapan terpenuhi.
    pub chrome: bool,
    /// Nilai `--max-budget-usd`, mis. `2.00`.
    pub budget_usd: String,
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
        // Mode `plan` Claude menolak tool MCP ledger; giliran plan tetap
        // hanya-baca lewat allowlist dan hook `--plan`.
        if spec.plan_mode {
            "default"
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
    match &spec.review {
        Some(r) => {
            a.push(review_settings_json(
                &spec.hook_command,
                &r.post_hook_command,
            ));
            if r.chrome {
                a.push("--chrome".to_owned());
            }
            a.push("--max-budget-usd".to_owned());
            a.push(r.budget_usd.clone());
        }
        None => a.push(settings_json(&spec.hook_command)),
    }
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
    claude_cmd(&spec.claude_bin, spec.runner.as_ref(), &spec.env)
        .args(args(spec))
        .cwd(&spec.worktree)
}

/// `claude` dengan environment eksplisit; di VM dibungkus
/// `sudo -n -u <user agent> mandord agent-exec` sehingga berjalan sebagai user
/// agent dengan login Claude miliknya (KTD-62 plan VM). Argumen `claude`
/// ditambahkan pemanggil.
#[must_use]
pub fn claude_cmd(
    claude_bin: &std::path::Path,
    runner: Option<&AgentRunner>,
    env: &[(String, String)],
) -> Cmd {
    match runner {
        None => Cmd::new(claude_bin).env_policy(EnvPolicy::Explicit(env.to_vec())),
        // `agent-exec` menyaring environment lagi di sisi user agent.
        Some(r) => Cmd::new(SUDO)
            .args(runner_args(r, env))
            .arg(claude_bin)
            .env_policy(EnvPolicy::Explicit(Vec::new())),
    }
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
            review: None,
            runner: None,
        }
    }

    #[test]
    fn peran_vm_membungkus_agent_dengan_sudo_dan_agent_exec() {
        let mut s = spec(Session::New("u-1".to_owned()));
        s.env = vec![
            ("PATH".to_owned(), "/usr/bin".to_owned()),
            ("GH_TOKEN".to_owned(), "ghs_rahasia".to_owned()),
            ("SOME_VAR".to_owned(), "x".to_owned()),
        ];
        s.runner = Some(AgentRunner {
            exe: "/home/beni/.local/bin/mandord".into(),
            user: "beni-agent".to_owned(),
            run_dir: "/srv/mandor/beni/runs".into(),
        });
        let cmd = build(&s);
        assert_eq!(cmd.program_name(), "/usr/bin/sudo");
        let a = runner_args(s.runner.as_ref().unwrap(), &s.env);
        assert_eq!(
            a,
            [
                "-n",
                "-u",
                "beni-agent",
                "/home/beni/.local/bin/mandord",
                "agent-exec",
                "--run-dir",
                "/srv/mandor/beni/runs",
                "--env",
                "PATH=/usr/bin",
                "--"
            ]
        );
        s.runner = None;
        assert_eq!(
            build(&s).program_name(),
            "claude",
            "peran laptop tidak berubah"
        );
    }

    #[test]
    fn sesi_reviewer_memakai_post_hook_batas_biaya_dan_chrome_bila_siap() {
        let mut s = spec(Session::New("r-1".to_owned()));
        s.review = Some(ReviewFlags {
            post_hook_command: "/opt/mandord hook --review /s.json --post".to_owned(),
            chrome: true,
            budget_usd: "2.00".to_owned(),
        });
        let a = args(&s);
        assert!(a.contains(&"--chrome".to_owned()));
        assert_eq!(after(&a, "--max-budget-usd"), Some("2.00"));
        assert!(after(&a, "--settings").is_some_and(|x| x.contains("PostToolUse")));
        s.review.as_mut().unwrap().chrome = false;
        assert!(!args(&s).contains(&"--chrome".to_owned()));
        // Sesi biasa tidak memakai browser maupun batas biaya.
        let plain = args(&spec(Session::New("u-1".to_owned())));
        assert!(!plain.contains(&"--chrome".to_owned()));
        assert!(!plain.contains(&"--max-budget-usd".to_owned()));
    }

    /// Mode `plan` milik Claude menolak tool MCP ledger (`ac_set_design`), jadi
    /// giliran plan memakai mode `default`: hanya tool di allowlist yang jalan,
    /// edit berkas tidak otomatis disetujui, dan hook `--plan` menolak penulisan.
    #[test]
    fn giliran_plan_memakai_mode_izin_default_bukan_plan() {
        let mut s = spec(Session::New("u-1".to_owned()));
        s.plan_mode = true;
        let a = args(&s);
        assert_eq!(after(&a, "--permission-mode"), Some("default"));
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
