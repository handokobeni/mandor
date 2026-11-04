//! Run app (KTD-36, R24–R27): state, pengaturan per repo, parser `.env`, dan
//! penanda berkas untuk konfirmasi sebelum app dijalankan.

/// State app milik satu run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppRunState {
    #[default]
    Off,
    SettingUp,
    Starting,
    WaitingForPort,
    Running,
    Failed,
    StoppedByOtherRun,
}

impl AppRunState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::SettingUp => "setting_up",
            Self::Starting => "starting",
            Self::WaitingForPort => "waiting_for_port",
            Self::Running => "running",
            Self::Failed => "failed",
            Self::StoppedByOtherRun => "stopped_by_other_run",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "setting_up" => Self::SettingUp,
            "starting" => Self::Starting,
            "waiting_for_port" => Self::WaitingForPort,
            "running" => Self::Running,
            "failed" => Self::Failed,
            "stopped_by_other_run" => Self::StoppedByOtherRun,
            _ => return None,
        })
    }

    /// Proses app sedang dinyalakan atau menyala.
    #[must_use]
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::SettingUp | Self::Starting | Self::WaitingForPort | Self::Running
        )
    }
}

/// Cara repo diuji reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TestMode {
    /// Tanpa app; AC tampilan dinilai Can't verify.
    #[default]
    NoApp,
    /// App web dijalankan Mandor dan diuji lewat browser.
    Web,
}

impl TestMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoApp => "no_app",
            Self::Web => "web",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "no_app" => Some(Self::NoApp),
            "web" => Some(Self::Web),
            _ => None,
        }
    }
}

/// Pengaturan app per repo (KTD-37).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppSettings {
    pub test_mode: TestMode,
    /// Mis. `bun install`; dijalankan bila lockfile berubah.
    pub setup_command: Option<String>,
    /// Mis. `bun run dev`; tanpa ini repo dilewati saat Run app.
    pub start_command: Option<String>,
    pub port: Option<u16>,
    /// URL app kanonik; bawaan `http://localhost:<port>`.
    pub app_url: Option<String>,
    /// Kunci `.env` yang diteruskan ke proses app.
    pub env_keys: Vec<String>,
    /// Uji browser untuk run satu repo (bawaan aktif).
    pub browser_tests: bool,
    /// Sebelum review berbrowser, Mandor menyalakan ulang app run ini dari
    /// kode terbaru tanpa konfirmasi perubahan (opt-in, bawaan mati).
    pub auto_restart: bool,
    /// Setup command repo menyiapkan login uji sendiri, sehingga review
    /// headless di VM boleh memakai browser tanpa konfirmasi login dev
    /// (KTD-64 plan VM).
    pub unattended_login: bool,
}

impl AppSettings {
    /// Repo ini punya app yang bisa dinyalakan.
    #[must_use]
    pub fn runnable(&self) -> bool {
        self.start_command
            .as_deref()
            .is_some_and(|c| !c.trim().is_empty())
            && self.port.is_some()
    }

    /// URL app kanonik: satu-satunya alamat login dev dan origin reviewer.
    #[must_use]
    pub fn canonical_url(&self) -> Option<String> {
        match self.app_url.as_deref().map(str::trim) {
            Some(u) if !u.is_empty() => Some(u.trim_end_matches('/').to_owned()),
            _ => self.port.map(|p| format!("http://localhost:{p}")),
        }
    }
}

/// Pemakai port lain di daemon yang sama: app repo atau prototype (R8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortUse {
    /// Nama repo atau sumber desain.
    pub owner: String,
    pub port: u16,
}

/// Pemakai lain yang sudah memakai `port`, bila ada (R8).
#[must_use]
pub fn port_conflict(port: Option<u16>, others: &[PortUse]) -> Option<&PortUse> {
    let port = port?;
    others.iter().find(|u| u.port == port)
}

/// Alasan URL prototype ditolak (R7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PrototypeUrlError {
    #[error("The prototype URL must be a local address such as http://127.0.0.1:<port>")]
    NotLoopback,
    #[error("The prototype URL must use the prototype's port")]
    PortMismatch,
    #[error(
        "The prototype URL must use a different host from the app URL (for example 127.0.0.1 when the app uses localhost), so the app's login cookies are not sent to the prototype"
    )]
    SameHostAsApp,
}

/// URL prototype harus loopback, memakai `port`, dan host-nya berbeda dari
/// host URL app repo, karena cookie terikat host, bukan port (R7).
///
/// # Errors
/// [`PrototypeUrlError`] dengan alasan untuk dev.
pub fn check_prototype_url(
    url: &str,
    port: u16,
    app_urls: &[String],
) -> Result<(), PrototypeUrlError> {
    let (host, url_port) = host_port(url).ok_or(PrototypeUrlError::NotLoopback)?;
    if !is_loopback(&host) {
        return Err(PrototypeUrlError::NotLoopback);
    }
    if url_port != port {
        return Err(PrototypeUrlError::PortMismatch);
    }
    if app_urls
        .iter()
        .filter_map(|u| host_port(u))
        .any(|(h, _)| h == host)
    {
        return Err(PrototypeUrlError::SameHostAsApp);
    }
    Ok(())
}

fn host_port(url: &str) -> Option<(String, u16)> {
    let origin = crate::hook_review::origin_of(url)?;
    let authority = origin.split_once("://")?.1;
    let (host, port) = authority.rsplit_once(':')?;
    Some((host.to_owned(), port.parse().ok()?))
}

fn is_loopback(host: &str) -> bool {
    host == "localhost" || host == "[::1]" || host.starts_with("127.")
}

/// Lockfile yang menentukan kapan perintah setup perlu dijalankan ulang.
pub const LOCKFILES: &[&str] = &[
    "bun.lock",
    "bun.lockb",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "Cargo.lock",
    "poetry.lock",
    "uv.lock",
    "Gemfile.lock",
    "go.sum",
    "composer.lock",
];

/// Berkas manifest paket yang diberi penanda saat konfirmasi (R25a).
const MANIFESTS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "Gemfile",
    "go.mod",
    "composer.json",
    "Makefile",
    "Procfile",
    "Dockerfile",
    "docker-compose.yml",
    "justfile",
];

/// Ekstensi skrip yang diberi penanda saat konfirmasi.
const SCRIPT_EXT: &[&str] = &[".sh", ".bash", ".zsh", ".ps1", ".bat", ".cmd"];

/// Berkas yang perlu diperhatikan dev sebelum app dijalankan: manifest paket,
/// lockfile, skrip, dan berkas yang disebut perintah start atau setup.
#[must_use]
pub fn is_flagged_change(path: &str, settings: &AppSettings) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if MANIFESTS.contains(&name) || LOCKFILES.contains(&name) {
        return true;
    }
    if SCRIPT_EXT.iter().any(|e| name.ends_with(e)) || path.starts_with("scripts/") {
        return true;
    }
    [&settings.start_command, &settings.setup_command]
        .into_iter()
        .flatten()
        .any(|cmd| {
            cmd.split_whitespace()
                .any(|w| w.trim_start_matches("./") == path)
        })
}

/// Mengurai isi `.env` menjadi pasangan kunci-nilai. Komentar, baris kosong,
/// awalan `export`, dan nilai berkutip didukung; baris rusak diabaikan.
#[must_use]
pub fn parse_env(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(env_line)
        .map(|(key, value)| (key.to_owned(), env_value(value.trim())))
        .collect()
}

/// Nama kunci `.env` saja, unik dan berurutan sesuai berkas (R1, KTD-P1).
/// Baris lanjutan nilai berkutip multi-baris (mis. kunci PEM) dilewati
/// sampai kutip penutup, sehingga potongan nilai tidak pernah terbaca
/// sebagai kunci.
#[must_use]
pub fn env_key_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut open: Option<char> = None;
    for line in text.lines() {
        if let Some(quote) = open {
            if line.contains(quote) {
                open = None;
            }
            continue;
        }
        let Some((key, value)) = env_line(line) else {
            continue;
        };
        open = unclosed_quote(value.trim());
        if !names.iter().any(|n| n == key) {
            names.push(key.to_owned());
        }
    }
    names
}

/// Kutip pembuka nilai yang tidak ditutup di baris yang sama.
fn unclosed_quote(value: &str) -> Option<char> {
    ['"', '\''].into_iter().find(|&q| {
        value
            .strip_prefix(q)
            .is_some_and(|inner| !inner.contains(q))
    })
}

/// Kunci valid dan nilai mentah satu baris `.env`.
fn env_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let valid = !key.is_empty()
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !key.starts_with(|c: char| c.is_ascii_digit());
    valid.then_some((key, value))
}

fn env_value(raw: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = raw.strip_prefix(quote) {
            return match inner.find(quote) {
                Some(end) => inner[..end].to_owned(),
                None => inner.to_owned(),
            };
        }
    }
    // Komentar setelah nilai tanpa kutip: `A=1 # catatan`.
    match raw.find(" #") {
        Some(i) => raw[..i].trim_end().to_owned(),
        None => raw.to_owned(),
    }
}

/// Hanya kunci `.env` yang dipilih dev, dengan urutan pilihan.
#[must_use]
pub fn select_env(pairs: &[(String, String)], keys: &[String]) -> Vec<(String, String)> {
    keys.iter()
        .filter_map(|k| pairs.iter().rev().find(|(pk, _)| pk == k).cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn web(port: u16) -> AppSettings {
        AppSettings {
            test_mode: TestMode::Web,
            start_command: Some("bun run dev".to_owned()),
            port: Some(port),
            ..AppSettings::default()
        }
    }

    #[test]
    fn state_bolak_balik_dan_yang_aktif() {
        for s in [
            AppRunState::Off,
            AppRunState::SettingUp,
            AppRunState::Starting,
            AppRunState::WaitingForPort,
            AppRunState::Running,
            AppRunState::Failed,
            AppRunState::StoppedByOtherRun,
        ] {
            assert_eq!(AppRunState::parse(s.as_str()), Some(s));
        }
        assert!(AppRunState::Running.is_active());
        assert!(!AppRunState::StoppedByOtherRun.is_active());
        assert_eq!(TestMode::parse("web"), Some(TestMode::Web));
        assert_eq!(TestMode::parse("x"), None);
    }

    #[test]
    fn url_kanonik_dari_port_atau_pengaturan() {
        assert_eq!(
            web(5173).canonical_url().as_deref(),
            Some("http://localhost:5173")
        );
        let mut s = web(5173);
        s.app_url = Some("http://app.localhost:5173/".to_owned());
        assert_eq!(
            s.canonical_url().as_deref(),
            Some("http://app.localhost:5173")
        );
        assert!(web(1).runnable());
        assert!(!AppSettings::default().runnable());
    }

    #[test]
    fn parser_env_menangani_komentar_export_kutip_dan_baris_rusak() {
        let text = "# komentar\nexport API_URL=http://localhost:8000\nSECRET=\"a b # c\"\n\
                    SINGLE='x=y'\nPLAIN=nilai # catatan\nrusak tanpa sama dengan\n=kosong\n\
                    1BAD=x\nOPEN=\"tanpa tutup\nAPI_URL=ganti\n";
        let pairs = parse_env(text);
        assert_eq!(
            pairs,
            [
                ("API_URL".to_owned(), "http://localhost:8000".to_owned()),
                ("SECRET".to_owned(), "a b # c".to_owned()),
                ("SINGLE".to_owned(), "x=y".to_owned()),
                ("PLAIN".to_owned(), "nilai".to_owned()),
                ("OPEN".to_owned(), "tanpa tutup".to_owned()),
                ("API_URL".to_owned(), "ganti".to_owned()),
            ]
        );
        let picked = select_env(&pairs, &["API_URL".to_owned(), "TIDAK_ADA".to_owned()]);
        assert_eq!(picked, [("API_URL".to_owned(), "ganti".to_owned())]);
    }

    #[test]
    fn penanda_manifest_skrip_dan_berkas_perintah_start() {
        let mut s = web(3000);
        s.start_command = Some("./bin/dev.rb --port 3000".to_owned());
        for flagged in [
            "package.json",
            "web/package.json",
            "bun.lock",
            "scripts/seed.ts",
            "tools/run.sh",
            "bin/dev.rb",
        ] {
            assert!(is_flagged_change(flagged, &s), "{flagged}");
        }
        assert!(!is_flagged_change("src/App.svelte", &s));
    }

    #[test]
    fn nama_kunci_env_tanpa_nilai_unik_dan_berurutan() {
        let text = "A=1\n# B=2\n\nexport C=3\nA=4\n9X=5\nbukan baris\n";
        assert_eq!(env_key_names(text), ["A", "C"]);
    }

    #[test]
    fn nama_kunci_env_melewati_nilai_berkutip_multi_baris() {
        let text = "KEY=\"-----BEGIN KEY-----\nABCDEF==\nQUOTE=palsu\n-----END KEY-----\"\nNEXT='satu\nX=dua'\nLAST=ok\nONE=\"utuh\"\n";
        assert_eq!(env_key_names(text), ["KEY", "NEXT", "LAST", "ONE"]);
    }

    #[test]
    fn url_prototype_loopback_port_sama_dan_host_beda_dari_app() {
        let apps = ["http://localhost:3000".to_owned()];
        assert_eq!(
            check_prototype_url("http://127.0.0.1:3001", 3001, &apps),
            Ok(())
        );
        assert_eq!(
            check_prototype_url("http://[::1]:3001/x", 3001, &apps),
            Ok(())
        );
        assert_eq!(
            check_prototype_url("http://example.com:3001", 3001, &apps),
            Err(PrototypeUrlError::NotLoopback)
        );
        assert_eq!(
            check_prototype_url("http://127.0.0.1:3002", 3001, &apps),
            Err(PrototypeUrlError::PortMismatch)
        );
        assert_eq!(
            check_prototype_url("http://LOCALHOST:3001", 3001, &apps),
            Err(PrototypeUrlError::SameHostAsApp)
        );
        assert_eq!(
            check_prototype_url("bukan-url", 3001, &apps),
            Err(PrototypeUrlError::NotLoopback)
        );
    }

    #[test]
    fn port_bentrok_dengan_pemakai_lain() {
        let others = [
            PortUse {
                owner: "kelola-web".to_owned(),
                port: 3000,
            },
            PortUse {
                owner: "kelola-api".to_owned(),
                port: 8080,
            },
        ];
        assert_eq!(
            port_conflict(Some(3000), &others).map(|u| u.owner.as_str()),
            Some("kelola-web")
        );
        assert!(port_conflict(Some(3001), &others).is_none());
        assert!(port_conflict(None, &others).is_none());
    }
}
