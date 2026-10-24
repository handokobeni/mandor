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
    /// `tool_input.url` (tool browser `navigate`).
    pub url: Option<String>,
    /// `tool_input.tabId` (tool browser).
    pub tab_id: Option<u64>,
    /// `tool_input.action` (tool browser `computer`).
    pub action: Option<String>,
    /// Teks respons tool, hanya untuk `PostToolUse`.
    pub response: Option<String>,
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
    if let Some(reason) = call
        .command
        .as_deref()
        .and_then(crate::hook_review::readonly_violation)
    {
        return Verdict::Deny(format!("mandor: {reason}; the planning phase is read-only"));
    }
    classify_with_mcp(call, allowed_mcp)
}

/// Program pengirim pesan yang ditolak di VM (R13 plan VM).
const VM_MESSAGE_PROGRAMS: &[&str] = &[
    "mail",
    "mailx",
    "s-nail",
    "sendmail",
    "mutt",
    "msmtp",
    "ssmtp",
    "swaks",
    "telegram-send",
    "ntfy",
    "apprise",
    "slack",
    "slackcat",
];
/// Host layanan pesan dan webhook yang ditolak di VM.
const VM_MESSAGE_HOSTS: &[&str] = &[
    "hooks.slack.com",
    "slack.com/api",
    "api.telegram.org",
    "discord.com/api/webhooks",
    "discordapp.com/api/webhooks",
    "webhook.office.com",
    "outlook.office.com/webhook",
    "ntfy.sh",
    "api.sendgrid.com",
    "api.mailgun.net",
    "api.postmarkapp.com",
    "smtp.",
];
/// Penanda environment staging (R13): tidak disentuh dari VM.
const VM_STAGING: &[&str] = &["staging"];
/// Berkas git per worktree yang tidak boleh diubah agent di VM (KTD-65).
const VM_GIT_MARKERS: &[&str] = &[".git/worktrees", "config.worktree"];
/// Klien database; ditolak bila menuju host yang bukan lokal.
const DB_CLIENTS: &[&str] = &[
    "psql",
    "pg_dump",
    "pg_restore",
    "mysql",
    "mariadb",
    "mysqldump",
    "mongo",
    "mongosh",
    "mongodump",
    "redis-cli",
    "sqlcmd",
];
/// Kata di nama tool MCP yang menandai pengiriman pesan ke orang lain.
const VM_MESSAGE_TOOL_WORDS: &[&str] = &[
    "send", "message", "mail", "notif", "comment", "reply", "chat", "invite", "share", "post_",
    "_post", "publish",
];
/// Server MCP milik Mandor di VM: ledger dan browser reviewer.
const VM_OWN_MCP: &[&str] = &["mandor", "playwright"];

/// Pagar lembur tambahan untuk semua run di daemon VM (R13, R14 plan VM),
/// di atas aturan biasa: tidak mengirim pesan, tidak menyentuh staging atau
/// database jarak jauh, tidak mengubah konfigurasi git, dan tidak membaca
/// berkas run daemon di `run_dir`. `Some(alasan)` bila ditolak.
#[must_use]
pub fn classify_vm(call: &ToolCall, run_dir: &str) -> Option<String> {
    let run_dir = normalize_path(run_dir.trim_end_matches('/'));
    if let Some(rest) = call.tool_name.strip_prefix("mcp__") {
        let (server, tool) = rest.split_once("__").unwrap_or((rest, ""));
        let tool = tool.to_ascii_lowercase();
        if !VM_OWN_MCP.contains(&server) && VM_MESSAGE_TOOL_WORDS.iter().any(|w| tool.contains(w)) {
            return Some(format!(
                "mandor: `{}` may message other people, which overnight work never does",
                call.tool_name
            ));
        }
    }
    for p in &call.paths {
        if let Some(reason) = vm_path_violation(&normalize_path(p), &run_dir) {
            return Some(reason);
        }
    }
    if let Some(url) = &call.url {
        if let Some(reason) = vm_text_violation(&url.to_ascii_lowercase(), &run_dir) {
            return Some(reason);
        }
    }
    let cmd = call.command.as_deref()?;
    let lower = cmd.to_lowercase();
    if let Some(reason) = vm_text_violation(&lower, &run_dir) {
        return Some(reason);
    }
    // Sambungan baris shell (`\` lalu baris baru) adalah satu perintah.
    let flat = lower.replace("\\\n", " ");
    let all = words(&flat);
    // Klien database: seluruh perintah diperiksa (gagal-tertutup), karena
    // host bisa datang dari assignment atau segmen lain.
    if all.iter().any(|w| DB_CLIENTS.contains(&program_name(w))) {
        if let Some(host) = remote_db_host(&all) {
            return Some(format!(
                "mandor: overnight work only uses local databases, not {host}"
            ));
        }
    }
    flat.split([';', '&', '|', '\n'])
        .find_map(|segment| vm_command_violation(&words(segment)))
}

/// Path yang ditolak di VM: berkas run daemon, berkas git per worktree, dan
/// berkas environment staging (`.env.staging`). Kode lain bernama staging
/// tetap boleh dibaca dan diubah.
fn vm_path_violation(path: &str, run_dir: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or_default();
    if name.starts_with(".env") && VM_STAGING.iter().any(|m| name.contains(m)) {
        return Some("mandor: overnight work never touches staging".to_owned());
    }
    if !run_dir.is_empty() && path.contains(run_dir) {
        return Some("mandor: the daemon's run files are not available to the agent".to_owned());
    }
    VM_GIT_MARKERS
        .iter()
        .any(|m| path.contains(m))
        .then(|| "mandor: changing git configuration is denied on the VM".to_owned())
}

/// Penanda di teks (URL atau perintah) yang ditolak di VM.
fn vm_text_violation(text: &str, run_dir: &str) -> Option<String> {
    if !run_dir.is_empty() && text.contains(run_dir) {
        return Some("mandor: the daemon's run files are not available to the agent".to_owned());
    }
    if VM_STAGING.iter().any(|m| text.contains(m)) {
        return Some("mandor: overnight work never touches staging".to_owned());
    }
    if VM_MESSAGE_HOSTS.iter().any(|h| text.contains(h)) {
        return Some(
            "mandor: sending messages or webhooks is not allowed in overnight work".to_owned(),
        );
    }
    if VM_GIT_MARKERS.iter().any(|m| text.contains(m)) {
        return Some("mandor: changing git configuration is denied on the VM".to_owned());
    }
    None
}

fn vm_command_violation(ws: &[String]) -> Option<String> {
    for (i, w) in ws.iter().enumerate() {
        let prog = program_name(w);
        let rest = &ws[i + 1..];
        if VM_MESSAGE_PROGRAMS.contains(&prog) {
            return Some(format!(
                "mandor: `{prog}` sends messages, which overnight work never does"
            ));
        }
        if prog == "git" && git_subcommand(rest) == Some("config") && !git_config_read_only(rest) {
            return Some("mandor: changing git configuration is denied on the VM".to_owned());
        }
    }
    None
}

/// `git config` (argumen satu perintah sederhana) yang hanya membaca: ada
/// opsi baca dan tidak ada opsi tulis; selain itu dianggap menulis.
fn git_config_read_only(rest: &[String]) -> bool {
    const READ: &[&str] = &[
        "--get",
        "--get-all",
        "--get-regexp",
        "--list",
        "-l",
        "get",
        "list",
    ];
    const WRITE: &[&str] = &[
        "--add",
        "--unset",
        "--unset-all",
        "--replace-all",
        "--rename-section",
        "--remove-section",
        "--edit",
        "-e",
        "set",
        "unset",
        "edit",
    ];
    rest.iter().any(|a| READ.contains(&a.as_str()))
        && !rest.iter().any(|a| WRITE.contains(&a.as_str()))
}

/// Host database yang bukan loopback dari kata satu perintah sederhana:
/// `-h X`, `-hX`, `--host[=]X`, `host=X` (conninfo), URL, dan assignment
/// `PGHOST`/`MYSQL_HOST`/`REDISCLI_HOST`.
fn remote_db_host(words: &[String]) -> Option<String> {
    let mut hosts = Vec::new();
    for (i, a) in words.iter().enumerate() {
        if a == "-h" || a == "--host" {
            hosts.extend(words.get(i + 1).cloned());
        } else if let Some(h) = a
            .strip_prefix("--host=")
            .or_else(|| a.strip_prefix("host="))
            .or_else(|| a.strip_prefix("hostaddr="))
            .or_else(|| a.strip_prefix("pghostaddr="))
            .or_else(|| a.strip_prefix("pghost="))
            .or_else(|| a.strip_prefix("mysql_host="))
            .or_else(|| a.strip_prefix("rediscli_host="))
        {
            hosts.push(h.to_owned());
        } else if let Some(h) = a.strip_prefix("-h").filter(|h| !h.is_empty()) {
            hosts.push(h.to_owned());
        } else if let Some((_, after)) = a.split_once("://") {
            // Seperti libpq: userinfo dipotong di `@` terakhir sebelum `/`
            // pertama, lalu host dipotong di `?`; multi-host dipisah koma.
            let pre = after.split('/').next().unwrap_or_default();
            let hostpart = pre.rsplit('@').next().unwrap_or_default();
            let hostlist = hostpart.split('?').next().unwrap_or_default();
            hosts.extend(
                hostlist
                    .split(',')
                    .map(|h| h.split(':').next().unwrap_or_default().to_owned()),
            );
            // Gagal-tertutup: setiap `host=`/`hostaddr=` di mana pun dalam URL
            // dianggap host tujuan, dan URL ber-percent-encoding (kunci atau
            // nilai bisa tersamar) ditolak karena tidak bisa diperiksa pasti.
            for key in ["host=", "hostaddr="] {
                hosts.extend(after.match_indices(key).map(|(i, _)| {
                    after[i + key.len()..]
                        .split(['&', ' '])
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                }));
            }
            if after.contains('%') {
                hosts.push("an encoded address".to_owned());
            }
        }
    }
    hosts.into_iter().find(|h| !local_host(h))
}

/// Hanya loopback yang lokal; nama layanan container ikut ditolak (agent
/// memakai port yang dipetakan ke localhost).
fn local_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0" | "")
}

/// Path menyentuh berkas kredensial atau state mandor (aturan KTD-21).
#[must_use]
pub fn is_credential_path(path: &str) -> bool {
    let p = normalize_path(path);
    CREDENTIAL_MARKERS.iter().any(|m| p.contains(m))
}

/// Akar sumber desain: jalur seperti yang diberikan dan hasil kanonisasinya.
/// Keduanya dipakai karena agent bisa menyebut akar lewat jalur yang memuat
/// symlink (mis. `/var` → `/private/var` di macOS).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignRoot {
    pub given: String,
    pub canonical: String,
}

/// Aturan akar sumber desain (KTD-33, R7a). `raw` adalah path dari tool call,
/// `canonical` hasil kanonisasi (`realpath`) oleh pemanggil. Path yang disebut
/// di dalam akar (lewat jalur yang diberikan maupun kanoniknya) harus tetap di
/// dalam akar kanonik setelah kanonisasi (symlink atau `../` tidak bisa
/// keluar), dan tidak ada tool penulis berkas di dalam akar.
#[must_use]
pub fn check_design_path(
    tool: &str,
    raw: &str,
    canonical: &str,
    roots: &[DesignRoot],
) -> Option<String> {
    let named = roots
        .iter()
        .find(|r| under(raw, &r.given) || under(raw, &r.canonical));
    if let Some(root) = named {
        if !under(canonical, &root.canonical) {
            return Some(format!(
                "mandor: {raw} resolves outside the design source {}",
                root.given
            ));
        }
    }
    let inside = named.is_some() || roots.iter().any(|r| under(canonical, &r.canonical));
    if inside && WRITE_TOOLS.contains(&tool) {
        return Some(format!("mandor: design sources are read-only ({raw})"));
    }
    None
}

/// `path` sama dengan `root` atau berada di bawahnya, per komponen path.
fn under(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
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
pub(crate) fn words(cmd: &str) -> Vec<String> {
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

pub(crate) fn program_name(word: &str) -> &str {
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

    fn roots() -> Vec<DesignRoot> {
        vec![DesignRoot {
            given: "/design/mock".to_owned(),
            canonical: "/design/mock".to_owned(),
        }]
    }

    #[test]
    fn akar_lewat_symlink_tetap_dikenali() {
        // macOS: tempdir `/var/...` dikanonisasi menjadi `/private/var/...`.
        let r = vec![DesignRoot {
            given: "/var/t/mock".to_owned(),
            canonical: "/private/var/t/mock".to_owned(),
        }];
        assert_eq!(
            check_design_path("Read", "/var/t/mock/a.tsx", "/private/var/t/mock/a.tsx", &r),
            None
        );
        assert!(
            check_design_path("Read", "/var/t/mock/link", "/private/var/t/secret.txt", &r)
                .is_some(),
            "symlink keluar dari akar ditolak"
        );
        assert!(
            check_design_path(
                "Read",
                "/var/t/mock/../secret.txt",
                "/private/var/t/secret.txt",
                &r
            )
            .is_some(),
            "../ keluar dari akar ditolak"
        );
        assert!(
            check_design_path(
                "Write",
                "/var/t/mock/a.tsx",
                "/private/var/t/mock/a.tsx",
                &r
            )
            .is_some(),
            "tetap hanya baca"
        );
    }

    #[test]
    fn baca_di_akar_desain_boleh_tulis_ditolak() {
        let r = roots();
        let p = "/design/mock/src/page.tsx";
        assert_eq!(check_design_path("Read", p, p, &r), None);
        assert_eq!(
            check_design_path("Grep", "/design/mock", "/design/mock", &r),
            None
        );
        assert!(check_design_path("Write", p, p, &r).is_some());
        assert!(check_design_path("Edit", p, p, &r).is_some());
        // Path lain tidak terpengaruh, termasuk nama yang hanya berawalan sama.
        assert_eq!(check_design_path("Write", "/wt/a.rs", "/wt/a.rs", &r), None);
        assert_eq!(
            check_design_path("Write", "/design/mockup/x", "/design/mockup/x", &r),
            None
        );
    }

    #[test]
    fn symlink_atau_dotdot_yang_keluar_dari_akar_ditolak() {
        let r = roots();
        // Symlink di dalam akar menunjuk ke luar.
        assert!(
            check_design_path("Read", "/design/mock/link", "/home/dev/.ssh/id_rsa", &r).is_some()
        );
        // `../` yang keluar dari akar setelah kanonisasi.
        assert!(
            check_design_path(
                "Read",
                "/design/mock/../secret.txt",
                "/design/secret.txt",
                &r
            )
            .is_some()
        );
        // Symlink dari luar yang menunjuk ke dalam akar tetap hanya baca.
        assert!(check_design_path("Write", "/wt/link", "/design/mock/a.css", &r).is_some());
        assert_eq!(
            check_design_path("Read", "/wt/link", "/design/mock/a.css", &r),
            None
        );
    }

    #[test]
    fn path_kredensial_dikenali() {
        assert!(is_credential_path("/home/dev/.mandor/design"));
        assert!(is_credential_path("/home/dev/.ssh/"));
        assert!(!is_credential_path("/home/dev/work/kelola-mock"));
    }

    fn bash(cmd: &str) -> Verdict {
        classify(&ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some(cmd.to_owned()),
            paths: vec![],
            ..ToolCall::default()
        })
    }

    fn tool(name: &str, path: &str) -> Verdict {
        classify(&ToolCall {
            tool_name: name.to_owned(),
            command: None,
            paths: vec![path.to_owned()],
            ..ToolCall::default()
        })
    }

    #[test]
    fn fase_plan_menolak_tool_penulis_berkas() {
        let call = |name: &str, path: &str| ToolCall {
            tool_name: name.to_owned(),
            command: None,
            paths: vec![path.to_owned()],
            ..ToolCall::default()
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
    fn fase_plan_hanya_menjalankan_bash_yang_membaca() {
        let bash = |cmd: &str| ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some(cmd.to_owned()),
            ..ToolCall::default()
        };
        for cmd in [
            "rg --pre ./x.sh foo",
            "rg --pre-glob '*.rs' --pre sh foo",
            "git diff --output=/wt/x.patch",
            "git log --ext-diff",
            "git stash",
            "cat a.rs > b.rs",
            "ls | xargs rm",
            "grep -r foo $(echo src)",
        ] {
            assert!(
                matches!(classify_planning(&bash(cmd), &[]), Verdict::Deny(_)),
                "{cmd}"
            );
        }
        for cmd in [
            "rg -n 'fn main' src",
            "git diff main",
            "cat README.md",
            "ls web",
        ] {
            assert_eq!(classify_planning(&bash(cmd), &[]), Verdict::Allow, "{cmd}");
        }
    }

    #[test]
    fn tool_mcp_hanya_dari_server_yang_diizinkan_workspace() {
        let call = |name: &str| ToolCall {
            tool_name: name.to_owned(),
            command: None,
            paths: vec![],
            ..ToolCall::default()
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
            ..ToolCall::default()
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

    const RUNS: &str = "/srv/mandor/beni/runs";

    fn vm(call: &ToolCall) -> Option<String> {
        classify_vm(call, RUNS)
    }

    fn vm_bash(cmd: &str) -> Option<String> {
        vm(&ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some(cmd.to_owned()),
            ..ToolCall::default()
        })
    }

    /// Covers AE3 (R13): pagar lembur berlaku untuk semua run di VM.
    #[test]
    fn vm_menolak_pengirim_pesan_staging_dan_db_jarak_jauh() {
        for cmd in [
            "mail -s hi boss@example.com < report.txt",
            "sendmail -t < msg",
            "curl -X POST https://hooks.slack.com/services/T/B/x -d @m.json",
            "curl https://api.telegram.org/bot1/sendMessage",
            "curl -d x https://discord.com/api/webhooks/1/2",
            "curl https://staging.kelola.id/api/jobs",
            "cat .env.staging",
            "psql -h db.prod.internal -U app",
            "psql postgres://app@10.0.0.5/kelola",
            "mysql --host=db.example.com",
            "redis-cli -h cache.example.com",
            "git config core.sshCommand 'touch /tmp/x'",
            "git config --get user.name; git config core.pager evil",
            "git config --list && git config include.path x",
            "psql -hdb.prod.internal",
            "psql 'host=db.prod.internal dbname=x'",
            "PGHOST=db.prod.internal psql",
            "psql -h db",
            "PGHOST=10.0.0.9; psql",
            "psql 'hostaddr=10.0.0.9 dbname=x'",
            "psql 'postgres:///x?host=db.prod.internal'",
            "psql 'postgres://u?x@db.prod.internal/kelola'",
            "psql 'postgres://localhost,db.prod.internal/kelola'",
            "psql 'postgres://u?x@localhost/db?host=db.prod.internal'",
            "psql 'postgres://localhost/db?%68ost=db.prod.internal'",
            "git config \\\ncore.pager evil",
            "git -C . config --add include.path ../x",
            "echo x >> .git/worktrees/pgs-1/config.worktree",
            "cat /srv/mandor/beni/runs/mcp/review-rv1.json",
        ] {
            assert!(vm_bash(cmd).is_some(), "{cmd}");
        }
        for cmd in [
            "cargo test",
            "bun run test",
            "psql -h localhost -U app",
            "psql postgres://app@127.0.0.1:5432/kelola",
            "redis-cli -h 127.0.0.1",
            "git config --get user.name",
            "git config --list",
            "curl http://localhost:5173/api/jobs",
            "docker compose up -d db",
        ] {
            assert_eq!(vm_bash(cmd), None, "{cmd}");
        }
    }

    #[test]
    fn vm_menolak_berkas_run_staging_dan_url_pengirim_pesan() {
        let path = |tool: &str, p: &str| {
            vm(&ToolCall {
                tool_name: tool.to_owned(),
                paths: vec![p.to_owned()],
                ..ToolCall::default()
            })
        };
        assert!(path("Read", "/srv/mandor/beni/runs/review/rv1.json").is_some());
        assert!(path("Read", "/w/pgs-1/.env.staging").is_some());
        assert!(path("Write", "/w/pgs-1/.git/worktrees/x/config.worktree").is_some());
        assert_eq!(path("Read", "/w/pgs-1/src/main.rs"), None);
        assert_eq!(path("Edit", "/w/pgs-1/config/staging.rb"), None);
        let fetch = |url: &str| {
            vm(&ToolCall {
                tool_name: "WebFetch".to_owned(),
                url: Some(url.to_owned()),
                ..ToolCall::default()
            })
        };
        assert!(fetch("https://hooks.slack.com/services/x").is_some());
        assert!(fetch("https://staging.kelola.id/").is_some());
        assert_eq!(fetch("https://docs.rs/tokio"), None);
    }

    #[test]
    fn vm_menolak_tool_mcp_yang_mengirim_pesan_kecuali_milik_mandor() {
        let mcp = |name: &str| {
            vm(&ToolCall {
                tool_name: name.to_owned(),
                ..ToolCall::default()
            })
        };
        for name in [
            "mcp__slack__chat_postMessage",
            "mcp__lark__im_v1_message_create",
            "mcp__gmail__send_email",
            "mcp__github__add_issue_comment",
            "mcp__notion__notify_user",
        ] {
            assert!(mcp(name).is_some(), "{name}");
        }
        for name in [
            "mcp__mandor__ac_set_verdict",
            "mcp__playwright__browser_click",
            "mcp__graft__search_code",
        ] {
            assert_eq!(mcp(name), None, "{name}");
        }
    }
}
