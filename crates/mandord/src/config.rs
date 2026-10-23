//! Konfigurasi daemon, dibaca sekali di `main` (satu-satunya pemakai `std::env::var`).

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
        })
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
    use super::{agent_may_read, shell_quote, workspaces_dir_for};
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
