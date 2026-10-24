//! Konfigurasi daemon, dibaca sekali di `main` (satu-satunya pemakai `std::env::var`).

use mandor_domain::DaemonRole;
use std::path::{Path, PathBuf};

use anyhow::Context;

/// Konfigurasi `mandord`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Direktori data (`~/.mandor` secara default, izin 0700).
    pub home: PathBuf,
    /// Home pengguna di mesin daemon: akar saran repo dan penjelajah folder.
    pub user_home: PathBuf,
    /// Induk semua worktree agent; selalu di luar direktori data (lihat
    /// [`workspaces_dir_for`]).
    pub workspaces_dir: PathBuf,
    /// Filter log `tracing` (`MANDOR_LOG`, default `info`).
    pub log_filter: String,
    /// Biner Claude Code (`MANDOR_CLAUDE_BIN`, default `claude`).
    pub claude_bin: PathBuf,
    /// Shell login untuk menangkap PATH pengguna (`SHELL`, default `/bin/bash`).
    pub shell: PathBuf,
    /// Perintah hook absolut: biner `mandord` ini ditambah ` hook`.
    pub hook_command: String,
    /// Biner `mandord` ini (server MCP ledger untuk agent).
    pub exe: PathBuf,
    /// Subcommand `mcp`: port daemon (`MANDOR_MCP_PORT`).
    pub mcp_port: Option<u16>,
    /// Subcommand `mcp`: token kapabilitas giliran (`MANDOR_MCP_TOKEN`).
    pub mcp_token: Option<secrecy::SecretString>,
    /// Environment dasar (PATH, HOME, bahasa) bila env login shell gagal ditangkap.
    pub base_env: Vec<(String, String)>,
    /// Pengelola proses daemon, dari `INVOCATION_ID` yang di-set systemd.
    pub managed_by: ManagedBy,
    /// `mandord install`: direktori biner terpasang (`~/.local/bin`).
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "hanya dipakai `mandord install` (Linux)")
    )]
    pub bin_dir: PathBuf,
    /// `mandord install`: direktori unit systemd user
    /// (`$XDG_CONFIG_HOME/systemd/user`, default `~/.config/systemd/user`).
    pub systemd_user_dir: PathBuf,
    /// Program `systemctl` (`MANDOR_SYSTEMCTL`, default `systemctl`).
    pub systemctl: PathBuf,
    /// Program `loginctl` (`MANDOR_LOGINCTL`, default `loginctl`).
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "hanya dipakai `mandord install` (Linux)")
    )]
    pub loginctl: PathBuf,
    /// Nama pengguna untuk status linger (`USER`).
    #[cfg_attr(
        not(target_os = "linux"),
        allow(dead_code, reason = "hanya dipakai `mandord install` (Linux)")
    )]
    pub user: Option<String>,
    /// Penjaga keep-awake (`MANDOR_KEEP_AWAKE_CMD`, array JSON argv).
    /// `Err` bila nilainya tidak valid; daemon tetap start dan melaporkan
    /// `keep_awake: failing`.
    pub keep_awake_cmd: Option<Result<Vec<String>, String>>,
    /// Peran daemon (`MANDOR_ROLE`: `laptop` default, `vm`; KTD-54 plan VM).
    pub role: DaemonRole,
    /// Id daemon di klaim (`MANDOR_DAEMON_ID`, default dari user, host, dan
    /// peran; KTD-55).
    pub daemon_id: String,
    /// Peran VM: user Linux agent (`MANDOR_AGENT_USER`) dan direktori grup
    /// developer untuk clone, worktree, dan berkas run (`MANDOR_GROUP_DIR`,
    /// KTD-62).
    pub vm_agent: Option<VmAgent>,
}

/// Agent di VM sebagai user Linux terpisah (KTD-62 plan VM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmAgent {
    pub user: String,
    pub group_dir: PathBuf,
    /// Biner MCP Playwright untuk review headless (KTD-64 plan VM).
    pub playwright_mcp: PathBuf,
}

/// Lokasi bawaan `mcp-server-playwright` dari `npm install -g @playwright/mcp`.
const DEFAULT_PLAYWRIGHT_MCP: &str = "/usr/local/bin/mcp-server-playwright";

/// Biner MCP Playwright dari `MANDOR_PLAYWRIGHT_MCP`; harus absolut karena
/// agent berjalan dengan `PATH` dari allowlist.
///
/// # Errors
/// Pesan untuk admin bila nilainya bukan path absolut.
pub fn playwright_mcp_of(raw: Option<&str>) -> Result<PathBuf, String> {
    match raw.map(str::trim).filter(|r| !r.is_empty()) {
        None => Ok(PathBuf::from(DEFAULT_PLAYWRIGHT_MCP)),
        Some(p) if p.starts_with('/') => Ok(PathBuf::from(p)),
        Some(p) => Err(format!(
            "MANDOR_PLAYWRIGHT_MCP must be an absolute path, got {p:?}"
        )),
    }
}

impl VmAgent {
    /// Berkas run yang bisa dibaca user agent, di luar `~/.mandor`.
    #[must_use]
    pub fn run_dir(&self) -> PathBuf {
        self.group_dir.join("runs")
    }
}

/// Peran VM wajib punya user agent dan direktori grup; peran laptop
/// mengabaikannya.
///
/// # Errors
/// Pesan untuk admin bila peran VM tanpa salah satunya.
pub fn vm_agent_of(
    role: DaemonRole,
    user: Option<&str>,
    group_dir: Option<&str>,
) -> Result<Option<VmAgent>, String> {
    if role != DaemonRole::Vm {
        return Ok(None);
    }
    let user = user.map(str::trim).filter(|u| !u.is_empty());
    let dir = group_dir.map(str::trim).filter(|d| !d.is_empty());
    match (user, dir) {
        (Some(u), Some(d)) if d.starts_with('/') => Ok(Some(VmAgent {
            user: u.to_owned(),
            group_dir: PathBuf::from(d),
            playwright_mcp: PathBuf::from(DEFAULT_PLAYWRIGHT_MCP),
        })),
        _ => Err(
            "MANDOR_ROLE=vm needs MANDOR_AGENT_USER and an absolute MANDOR_GROUP_DIR (see docs/runbook/setup-vm.md)"
                .to_owned(),
        ),
    }
}

/// Peran dari `MANDOR_ROLE`; kosong berarti laptop.
///
/// # Errors
/// Pesan untuk pengguna bila nilainya bukan `laptop` atau `vm`.
pub fn parse_role(raw: Option<&str>) -> Result<DaemonRole, String> {
    match raw.map(str::trim).filter(|r| !r.is_empty()) {
        None => Ok(DaemonRole::Laptop),
        Some(r) => DaemonRole::parse(r)
            .ok_or_else(|| format!("MANDOR_ROLE must be laptop or vm, got {r:?}")),
    }
}

/// Id daemon bawaan: `<user>@<host>` bila host diketahui, selain itu
/// `<user>-<peran>`; nilai eksplisit menang.
#[must_use]
pub fn default_daemon_id(
    explicit: Option<&str>,
    user: Option<&str>,
    host: Option<&str>,
    role: DaemonRole,
) -> String {
    if let Some(id) = explicit.map(str::trim).filter(|i| !i.is_empty()) {
        return id.to_owned();
    }
    let user = user.filter(|u| !u.is_empty()).unwrap_or("mandor");
    match host.map(str::trim).filter(|h| !h.is_empty()) {
        Some(h) => format!("{user}@{h}"),
        None => format!("{user}-{}", role.as_str()),
    }
}

/// Parse `MANDOR_KEEP_AWAKE_CMD`: array JSON argv tanpa shell, program berupa
/// jalur absolut, tanpa karakter kontrol.
///
/// # Errors
/// Pesan untuk pengguna bila nilainya tidak valid.
pub fn parse_keep_awake_cmd(raw: &str) -> Result<Vec<String>, String> {
    let argv: Vec<String> = serde_json::from_str(raw)
        .map_err(|_| "MANDOR_KEEP_AWAKE_CMD must be a JSON array of strings".to_owned())?;
    let program = argv
        .first()
        .ok_or_else(|| "MANDOR_KEEP_AWAKE_CMD is empty".to_owned())?;
    if !program.starts_with('/') {
        return Err("the keep-awake program must be an absolute path".to_owned());
    }
    if argv.iter().any(|a| a.chars().any(char::is_control)) {
        return Err("MANDOR_KEEP_AWAKE_CMD contains a control character".to_owned());
    }
    Ok(argv)
}

/// Siapa yang mengelola siklus hidup proses daemon (KTD-44).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedBy {
    /// Unit systemd user (`mandord.service`).
    Systemd,
    /// Dijalankan terlepas oleh `connect` (setsid) atau manual.
    Detached,
}

impl ManagedBy {
    /// systemd men-set `INVOCATION_ID` untuk setiap proses unit.
    #[must_use]
    pub fn from_invocation_id(value: Option<&str>) -> Self {
        match value {
            Some(v) if !v.is_empty() => Self::Systemd,
            _ => Self::Detached,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Systemd => "systemd",
            Self::Detached => "detached",
        }
    }
}

impl Config {
    /// Membaca konfigurasi dari environment.
    ///
    /// # Errors
    /// Gagal bila direktori home pengguna tidak bisa ditentukan.
    pub fn from_env() -> anyhow::Result<Self> {
        let user_home = PathBuf::from(
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .context("HOME or USERPROFILE must be set to locate ~/.mandor")?,
        );
        let explicit = std::env::var_os("MANDOR_HOME").filter(|d| !d.is_empty());
        let home = explicit
            .clone()
            .map_or_else(|| user_home.join(".mandor"), PathBuf::from);
        let workspaces_dir = workspaces_dir_for(&user_home, &home, explicit.is_some());
        let log_filter = std::env::var("MANDOR_LOG").unwrap_or_else(|_| "info".to_owned());
        let claude_bin = std::env::var_os("MANDOR_CLAUDE_BIN")
            .filter(|v| !v.is_empty())
            .map_or_else(|| PathBuf::from("claude"), PathBuf::from);
        let shell = std::env::var_os("SHELL")
            .filter(|v| !v.is_empty())
            .map_or_else(|| PathBuf::from("/bin/bash"), PathBuf::from);
        let exe = std::env::current_exe().context("locating the mandord binary")?;
        let hook_command = format!("{} hook", shell_quote(&exe.display().to_string()));
        let mcp_port = std::env::var("MANDOR_MCP_PORT")
            .ok()
            .and_then(|p| p.parse().ok());
        let mcp_token = std::env::var("MANDOR_MCP_TOKEN")
            .ok()
            .filter(|t| !t.is_empty())
            .map(secrecy::SecretString::from);
        let base_env = ["PATH", "HOME", "USER", "LANG", "LC_ALL", "TERM", "TMPDIR"]
            .iter()
            .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_owned(), v)))
            .collect();
        let managed_by =
            ManagedBy::from_invocation_id(std::env::var("INVOCATION_ID").ok().as_deref());
        let bin_dir = user_home.join(".local").join("bin");
        let systemd_user_dir = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map_or_else(|| user_home.join(".config"), PathBuf::from)
            .join("systemd")
            .join("user");
        let program = |key: &str, default: &str| {
            std::env::var_os(key)
                .filter(|v| !v.is_empty())
                .map_or_else(|| PathBuf::from(default), PathBuf::from)
        };
        let systemctl = program("MANDOR_SYSTEMCTL", "systemctl");
        let loginctl = program("MANDOR_LOGINCTL", "loginctl");
        let user = std::env::var("USER").ok().filter(|u| !u.is_empty());
        let role =
            parse_role(std::env::var("MANDOR_ROLE").ok().as_deref()).map_err(anyhow::Error::msg)?;
        let vm_agent = vm_agent_of(
            role,
            std::env::var("MANDOR_AGENT_USER").ok().as_deref(),
            std::env::var("MANDOR_GROUP_DIR").ok().as_deref(),
        )
        .map_err(anyhow::Error::msg)?
        .map(|mut v| {
            playwright_mcp_of(std::env::var("MANDOR_PLAYWRIGHT_MCP").ok().as_deref()).map(|p| {
                v.playwright_mcp = p;
                v
            })
        })
        .transpose()
        .map_err(anyhow::Error::msg)?;
        // Di VM clone dan worktree ada di direktori grup developer.
        let workspaces_dir = vm_agent
            .as_ref()
            .map_or(workspaces_dir, |v| v.group_dir.join("workspaces"));
        let daemon_id = default_daemon_id(
            std::env::var("MANDOR_DAEMON_ID").ok().as_deref(),
            user.as_deref(),
            std::env::var("HOSTNAME").ok().as_deref(),
            role,
        );
        let keep_awake_cmd = std::env::var("MANDOR_KEEP_AWAKE_CMD")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| parse_keep_awake_cmd(&v));
        Ok(Self {
            home,
            user_home,
            workspaces_dir,
            log_filter,
            claude_bin,
            shell,
            hook_command,
            exe,
            mcp_port,
            mcp_token,
            base_env,
            managed_by,
            bin_dir,
            systemd_user_dir,
            systemctl,
            loginctl,
            user,
            keep_awake_cmd,
            role,
            daemon_id,
            vm_agent,
        })
    }

    /// Direktori data adalah `~/.mandor` bawaan. Unit systemd hanya melayani
    /// direktori ini, jadi `install` dan `connect` dengan `MANDOR_HOME` lain
    /// tidak memakai service.
    #[must_use]
    pub fn uses_default_home(&self) -> bool {
        self.home == self.user_home.join(".mandor")
    }

    /// Path database `SQLite`.
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.home.join("mandor.db")
    }

    /// Berkas pengaturan Lark (0600).
    #[must_use]
    pub fn lark_settings_path(&self) -> PathBuf {
        self.home.join("lark.json")
    }

    /// Berkas token user Lark (0600).
    #[must_use]
    pub fn lark_token_path(&self) -> PathBuf {
        self.home.join("secrets").join("lark-token.json")
    }

    /// Induk semua worktree.
    #[must_use]
    pub fn workspaces_dir(&self) -> PathBuf {
        self.workspaces_dir.clone()
    }

    /// Path log daemon.
    #[must_use]
    pub fn log_path(&self) -> PathBuf {
        self.home.join("mandord.log")
    }
}

/// Pagar agent (hook dan deny list) menolak setiap path yang memuat `.mandor`,
/// tempat token Lark disimpan. Worktree harus bisa dibaca agent.
fn agent_may_read(path: &Path) -> bool {
    let call = mandor_domain::hook::ToolCall {
        tool_name: "Read".to_owned(),
        command: None,
        paths: vec![path.display().to_string()],
        ..mandor_domain::hook::ToolCall::default()
    };
    mandor_domain::hook::classify(&call) == mandor_domain::hook::Verdict::Allow
}

/// Induk worktree agent. `<MANDOR_HOME>/workspaces` hanya bila `MANDOR_HOME`
/// diatur eksplisit dan aman bagi pagar agent; selain itu
/// `~/.local/share/mandor/workspaces`, di luar direktori data.
fn workspaces_dir_for(user_home: &Path, home: &Path, explicit_home: bool) -> PathBuf {
    let inside = home.join("workspaces");
    if explicit_home && agent_may_read(&inside.join("x")) {
        inside
    } else {
        user_home
            .join(".local")
            .join("share")
            .join("mandor")
            .join("workspaces")
    }
}

/// Mengutip path untuk baris perintah hook bila mengandung spasi atau kutip.
fn shell_quote(path: &str) -> String {
    if path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+:".contains(c))
    {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn perintah_keep_awake_argv_json() {
        assert_eq!(
            parse_keep_awake_cmd(r#"["/mnt/c/Users/A B/100%/k.exe","--keep-awake"]"#),
            Ok(vec![
                "/mnt/c/Users/A B/100%/k.exe".to_owned(),
                "--keep-awake".to_owned()
            ])
        );
        assert!(parse_keep_awake_cmd("/mnt/c/k.exe --keep-awake").is_err());
        assert!(parse_keep_awake_cmd("[]").is_err());
        assert!(parse_keep_awake_cmd(r#"["k.exe"]"#).is_err());
        assert!(parse_keep_awake_cmd(r#"["/k.exe","a\nb"]"#).is_err());
        assert!(parse_keep_awake_cmd("[1, 2]").is_err());
    }

    #[test]
    fn pengelola_daemon_dari_invocation_id() {
        assert_eq!(
            ManagedBy::from_invocation_id(Some("abc")),
            ManagedBy::Systemd
        );
        assert_eq!(ManagedBy::from_invocation_id(Some("")), ManagedBy::Detached);
        assert_eq!(ManagedBy::from_invocation_id(None), ManagedBy::Detached);
        assert_eq!(ManagedBy::Systemd.as_str(), "systemd");
        assert_eq!(ManagedBy::Detached.as_str(), "detached");
    }

    use super::{
        ManagedBy, agent_may_read, default_daemon_id, parse_keep_awake_cmd, parse_role,
        shell_quote, workspaces_dir_for,
    };
    use mandor_domain::DaemonRole;

    #[test]
    fn peran_vm_wajib_user_agent_dan_direktori_grup() {
        use super::vm_agent_of;
        assert_eq!(vm_agent_of(DaemonRole::Laptop, None, None), Ok(None));
        assert!(vm_agent_of(DaemonRole::Vm, None, Some("/srv/m/beni")).is_err());
        assert!(vm_agent_of(DaemonRole::Vm, Some("beni-agent"), Some("rel")).is_err());
        let v = vm_agent_of(DaemonRole::Vm, Some("beni-agent"), Some("/srv/m/beni"))
            .unwrap()
            .unwrap();
        assert_eq!(v.user, "beni-agent");
        assert_eq!(v.run_dir(), std::path::Path::new("/srv/m/beni/runs"));
        assert_eq!(
            v.playwright_mcp,
            std::path::Path::new("/usr/local/bin/mcp-server-playwright")
        );
    }

    #[test]
    fn biner_playwright_harus_absolut() {
        use super::playwright_mcp_of;
        assert_eq!(
            playwright_mcp_of(Some("/opt/pw/bin/mcp-server-playwright")),
            Ok("/opt/pw/bin/mcp-server-playwright".into())
        );
        assert_eq!(
            playwright_mcp_of(None),
            Ok("/usr/local/bin/mcp-server-playwright".into())
        );
        assert!(playwright_mcp_of(Some("mcp-server-playwright")).is_err());
    }

    #[test]
    fn peran_dan_id_daemon_dari_environment() {
        assert_eq!(parse_role(None), Ok(DaemonRole::Laptop));
        assert_eq!(parse_role(Some(" ")), Ok(DaemonRole::Laptop));
        assert_eq!(parse_role(Some("vm")), Ok(DaemonRole::Vm));
        assert!(parse_role(Some("server")).is_err());
        assert_eq!(
            default_daemon_id(Some("vm-beni"), Some("beni"), Some("h"), DaemonRole::Vm),
            "vm-beni"
        );
        assert_eq!(
            default_daemon_id(None, Some("beni"), Some("lembur01"), DaemonRole::Vm),
            "beni@lembur01"
        );
        assert_eq!(
            default_daemon_id(None, Some("beni"), None, DaemonRole::Laptop),
            "beni-laptop"
        );
        assert_eq!(
            default_daemon_id(None, None, None, DaemonRole::Vm),
            "mandor-vm"
        );
    }
    use std::path::Path;

    #[test]
    fn worktree_di_luar_direktori_data_agar_agent_boleh_membacanya() {
        let user = Path::new("/home/dev");
        let default = workspaces_dir_for(user, &user.join(".mandor"), false);
        assert_eq!(default, user.join(".local/share/mandor/workspaces"));
        assert!(agent_may_read(
            &default.join("kelola-v2-x/kelola-api/src/main.rs")
        ));
        // Direktori data yang ditolak pagar agent tidak pernah dipakai untuk worktree.
        assert_eq!(
            workspaces_dir_for(user, &user.join(".mandor"), true),
            default
        );
        // MANDOR_HOME eksplisit (test, e2e) tetap dipakai bila aman.
        assert_eq!(
            workspaces_dir_for(user, Path::new("/tmp/uji"), true),
            Path::new("/tmp/uji/workspaces")
        );
    }

    #[test]
    fn path_dengan_spasi_dikutip() {
        assert_eq!(shell_quote("/usr/bin/mandord"), "/usr/bin/mandord");
        assert_eq!(shell_quote("/a b/mandord"), "'/a b/mandord'");
        assert_eq!(shell_quote("/a'b"), "'/a'\\''b'");
    }
}
