//! Aturan murni review lab fase 2 (plan 2026-11-10-001, U11, KTD-L15–KTD-L24):
//! database lokal, pernyataan SQL, hak role lab, klien lain, status sesi data,
//! hasil restore, kunci per penyimpanan, dan perintah skrip yang dipin.

use std::net::IpAddr;

/// Alasan lab data tidak boleh berjalan; teks dalam bahasa Inggris untuk
/// developer (UI) dan calon *Can't verify*.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LabDataBlock {
    #[error("lab only runs against local databases ({0} is not local)")]
    NotLocal(String),
    #[error("the lab database role is a superuser")]
    Superuser,
    #[error("the lab database role has the {0} attribute")]
    RoleAttribute(&'static str),
    #[error("the lab database role is a member of role {0}")]
    MemberOf(String),
    #[error("the lab database role can create objects in {0}")]
    CanCreate(String),
    #[error("the lab database role has TRIGGER on table {0}")]
    Trigger(String),
    #[error("the lab database role owns {0} database objects")]
    OwnsObjects(u64),
    #[error("extension {0} is installed in the lab database")]
    Extension(String),
    #[error("lab database role has no access to {0}")]
    NoAccess(String),
    #[error("another database client is connected: {0}")]
    OtherClients(String),
    #[error("lab commands changed since approval")]
    CommandsChanged,
}

// ---------------------------------------------------------------------------
// Database lokal (KTD-L15)

/// Host database di mesin sendiri: `localhost`, `127.0.0.0/8`, `::1`, atau
/// folder socket Unix (diawali `/`). Nama seperti `localhost.evil.com` atau
/// alamat IPv4 yang dipetakan ke IPv6 di luar loopback bukan lokal.
#[must_use]
pub fn is_local_db(host: &str) -> bool {
    let h = host.trim();
    if h.starts_with('/') {
        return true;
    }
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let bare = h.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.is_loopback(),
        Ok(IpAddr::V6(v6)) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        Err(_) => false,
    }
}

/// Target database: host, port, dan nama database terpisah (KTD-L15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbTarget {
    pub host: String,
    pub port: u16,
    pub database: String,
}

/// DSN yang ditolak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DsnError {
    #[error("the database URL is not a postgres:// URL")]
    NotPostgres,
    #[error("the database URL names several hosts")]
    SeveralHosts,
    #[error(
        "the database URL has options that can change the server; use only sslmode, application_name or connect_timeout"
    )]
    HostInQuery,
    #[error("the database URL has an invalid port")]
    BadPort,
    #[error("the database URL has no database name")]
    NoDatabase,
}

const PG_DEFAULT_PORT: u16 = 5432;
/// Opsi query string DSN yang diizinkan.
const DSN_SAFE_OPTIONS: &[&str] = &["sslmode", "application_name", "connect_timeout"];

/// Mengurai `postgres://user:pass@host:port/db?opsi` dari env app. Beberapa
/// host atau `host`/`hostaddr` di query string ditolak: driver bisa memakai
/// host itu, bukan yang terlihat di depan (KTD-L15).
///
/// # Errors
/// [`DsnError`] untuk bentuk yang tidak bisa dipastikan lokal.
pub fn parse_dsn(dsn: &str) -> Result<DbTarget, DsnError> {
    let rest = dsn
        .strip_prefix("postgres://")
        .or_else(|| dsn.strip_prefix("postgresql://"))
        .ok_or(DsnError::NotPostgres)?;
    let (main, query) = rest.split_once('?').unwrap_or((rest, ""));
    // Hanya opsi yang tidak bisa mengganti server; host, hostaddr, service,
    // passfile, port, dan percent-encoding ditolak (KTD-L15).
    let safe_query = query.split('&').filter(|kv| !kv.is_empty()).all(|kv| {
        let key = kv.split('=').next().unwrap_or_default();
        !kv.contains('%') && DSN_SAFE_OPTIONS.contains(&key)
    });
    if !safe_query {
        return Err(DsnError::HostInQuery);
    }
    if main.contains('%') {
        return Err(DsnError::HostInQuery);
    }
    let (authority, path) = main.split_once('/').unwrap_or((main, ""));
    let hostport = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    if hostport.contains(',') {
        return Err(DsnError::SeveralHosts);
    }
    let (host, port) = split_host_port(hostport)?;
    let database = path.trim_end_matches('/').to_owned();
    if database.is_empty() {
        return Err(DsnError::NoDatabase);
    }
    Ok(DbTarget {
        host,
        port,
        database,
    })
}

fn split_host_port(hostport: &str) -> Result<(String, u16), DsnError> {
    if let Some(rest) = hostport.strip_prefix('[') {
        let (h, tail) = rest.split_once(']').ok_or(DsnError::BadPort)?;
        let port = match tail.strip_prefix(':') {
            Some(p) => p.parse().map_err(|_| DsnError::BadPort)?,
            None if tail.is_empty() => PG_DEFAULT_PORT,
            None => return Err(DsnError::BadPort),
        };
        return Ok((h.to_owned(), port));
    }
    match hostport.rsplit_once(':') {
        Some((h, p)) => Ok((h.to_owned(), p.parse().map_err(|_| DsnError::BadPort)?)),
        None => Ok((hostport.to_owned(), PG_DEFAULT_PORT)),
    }
}

// ---------------------------------------------------------------------------
// Pernyataan SQL (KTD-L16, lapisan kedua)

/// Kata yang tidak boleh muncul di `lab_sql`; batas utamanya tetap pemeriksaan
/// role positif.
const FORBIDDEN_WORDS: &[&str] = &[
    "do",
    "call",
    "copy",
    "create",
    "alter",
    "drop",
    "grant",
    "revoke",
    "language",
    "listen",
    "notify",
    "vacuum",
    "truncate",
    "reindex",
    "cluster",
    "load",
    "import",
    "security",
    "set_config",
    "pg_read_file",
    "pg_read_binary_file",
    "pg_ls_dir",
    "pg_stat_file",
    "pg_terminate_backend",
    "pg_cancel_backend",
];
/// Awalan nama fungsi yang ditolak.
const FORBIDDEN_PREFIXES: &[&str] = &["lo_", "dblink", "pg_file"];

/// Pernyataan `lab_sql` yang ditolak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatementError {
    #[error("send one SQL statement")]
    Empty,
    #[error("send one SQL statement at a time")]
    Several,
    #[error("`{0}` is not allowed in lab SQL")]
    Forbidden(String),
    #[error("SET ROLE and SET SESSION AUTHORIZATION are not allowed in lab SQL")]
    SetRole,
}

/// Memeriksa satu pernyataan `lab_sql` terhadap daftar larangan: komentar
/// dibuang, huruf besar-kecil diabaikan, dan pernyataan kedua ditolak.
///
/// # Errors
/// [`StatementError`] dengan kata yang ditolak.
pub fn check_statement(sql: &str) -> Result<(), StatementError> {
    let text = strip_comments(sql).to_ascii_lowercase();
    let body = text.trim().trim_end_matches(';').trim();
    if body.is_empty() {
        return Err(StatementError::Empty);
    }
    if body.contains(';') {
        return Err(StatementError::Several);
    }
    // Aturan kutip PostgreSQL (dollar quote, E'' dengan backslash, U&'')
    // berbeda dari pengurai di sini; bentuk itu ditolak seluruhnya.
    if let Some(bad) = ["$", "\\"].into_iter().find(|t| body.contains(t)) {
        return Err(StatementError::Forbidden(bad.to_owned()));
    }
    if escape_string_prefix(body) {
        return Err(StatementError::Forbidden("E'' or U&'' strings".to_owned()));
    }
    let words: Vec<&str> = body
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect();
    for w in &words {
        if FORBIDDEN_WORDS.contains(w) || FORBIDDEN_PREFIXES.iter().any(|p| w.starts_with(p)) {
            return Err(StatementError::Forbidden((*w).to_owned()));
        }
    }
    let set_role = words.windows(2).any(|p| p == ["set", "role"])
        || words.windows(2).any(|p| p == ["session", "authorization"]);
    if set_role {
        return Err(StatementError::SetRole);
    }
    Ok(())
}

/// Ada string `E'...'` atau `U&'...'`: huruf `e` atau `u&` tepat sebelum kutip
/// tunggal yang bukan bagian dari identifier lain.
fn escape_string_prefix(body: &str) -> bool {
    let bytes = body.as_bytes();
    let ident = |at: usize| bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_';
    let mut in_string = false;
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'\'' {
            continue;
        }
        if in_string {
            in_string = false;
            continue;
        }
        in_string = true;
        let e_prefix = i >= 1 && bytes[i - 1] == b'e' && (i < 2 || !ident(i - 2));
        let u_prefix =
            i >= 2 && bytes[i - 1] == b'&' && bytes[i - 2] == b'u' && (i < 3 || !ident(i - 3));
        if e_prefix || u_prefix {
            return true;
        }
    }
    false
}

/// Membuang komentar `--` dan `/* */` (termasuk bersarang) di luar string.
fn strip_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if depth > 0 {
            if c == '*' && next == Some('/') {
                depth -= 1;
                i += 2;
            } else if c == '/' && next == Some('*') {
                depth += 1;
                i += 2;
            } else {
                i += 1;
            }
            if depth == 0 {
                out.push(' ');
            }
            continue;
        }
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match (c, next) {
            ('\'' | '"', _) => {
                quote = Some(c);
                out.push(c);
                i += 1;
            }
            ('-', Some('-')) => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                out.push(' ');
            }
            ('/', Some('*')) => {
                depth = 1;
                i += 2;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Hak role lab (KTD-L16, aturan positif)

/// Hasil kueri hak role lab, diisi adapter `PostgreSQL`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RolePrivileges {
    /// Atribut role yang menyala (`rolsuper`, `rolcreaterole`, dst.).
    pub attributes: Vec<RoleAttribute>,
    /// Semua role yang diwarisi, transitif, selain PUBLIC.
    pub member_of: Vec<String>,
    /// Database atau schema tempat role punya `CREATE` (bukan `TEMP`).
    pub create_on: Vec<String>,
    /// Tabel tempat role punya `TRIGGER`.
    pub trigger_on: Vec<String>,
    /// Jumlah objek milik role.
    pub owned_objects: u64,
    /// Extension terpasang di database lab.
    pub extensions: Vec<String>,
    /// Tabel penyimpanan yang tidak bisa di-SELECT/INSERT/UPDATE/DELETE role.
    pub missing_access: Vec<String>,
}

/// Atribut role `PostgreSQL` yang membuat role lab ditolak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleAttribute {
    Superuser,
    CreateRole,
    CreateDb,
    Replication,
    BypassRls,
}

impl RoleAttribute {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Superuser => "SUPERUSER",
            Self::CreateRole => "CREATEROLE",
            Self::CreateDb => "CREATEDB",
            Self::Replication => "REPLICATION",
            Self::BypassRls => "BYPASSRLS",
        }
    }
}

const FORBIDDEN_EXTENSIONS: &[&str] = &["dblink", "postgres_fdw", "file_fdw"];

/// Alasan pertama role lab tidak boleh dipakai; `None` bila hanya punya hak
/// baca/tulis data biasa.
#[must_use]
pub fn role_block(p: &RolePrivileges) -> Option<LabDataBlock> {
    if p.attributes.contains(&RoleAttribute::Superuser) {
        return Some(LabDataBlock::Superuser);
    }
    if let Some(attr) = p.attributes.first() {
        return Some(LabDataBlock::RoleAttribute(attr.as_str()));
    }
    if let Some(r) = p
        .member_of
        .iter()
        .find(|r| !r.eq_ignore_ascii_case("public"))
    {
        return Some(LabDataBlock::MemberOf(r.clone()));
    }
    if let Some(on) = p.create_on.first() {
        return Some(LabDataBlock::CanCreate(on.clone()));
    }
    if let Some(t) = p.trigger_on.first() {
        return Some(LabDataBlock::Trigger(t.clone()));
    }
    if p.owned_objects > 0 {
        return Some(LabDataBlock::OwnsObjects(p.owned_objects));
    }
    if let Some(e) = p
        .extensions
        .iter()
        .find(|e| FORBIDDEN_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    {
        return Some(LabDataBlock::Extension(e.clone()));
    }
    p.missing_access
        .first()
        .map(|t| LabDataBlock::NoAccess(t.clone()))
}

// ---------------------------------------------------------------------------
// Klien lain (KTD-L20)

/// Satu baris `pg_stat_activity`; kolom yang disembunyikan server bernilai
/// `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PgActivity {
    pub pid: i32,
    pub datname: Option<String>,
    pub backend_type: Option<String>,
    pub usename: Option<String>,
    pub application_name: Option<String>,
    pub client_addr: Option<String>,
}

/// Klien lain di database lab: client backend (atau kolom tersembunyi) pada
/// database lab, selain pid milik daemon. Gagal tertutup: baris tanpa nama
/// database atau jenis backend dihitung.
#[must_use]
pub fn other_clients<'a>(
    activity: &'a [PgActivity],
    lab_dbs: &[String],
    own_pids: &[i32],
) -> Vec<&'a PgActivity> {
    activity
        .iter()
        .filter(|a| !own_pids.contains(&a.pid))
        .filter(|a| {
            a.backend_type
                .as_deref()
                .is_none_or(|t| t == "client backend")
        })
        .filter(|a| {
            a.datname
                .as_deref()
                .is_none_or(|d| lab_dbs.iter().any(|l| l == d))
        })
        .collect()
}

/// Ringkasan klien yang menghalangi untuk alasan dan banner.
#[must_use]
pub fn describe_clients(clients: &[&PgActivity]) -> String {
    clients
        .iter()
        .map(|c| {
            format!(
                "{} from {} as {}",
                c.application_name
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("unknown app"),
                c.client_addr.as_deref().unwrap_or("local socket"),
                c.usename.as_deref().unwrap_or("unknown user"),
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Digest salinan skrip yang disetujui sama dengan isi folder skrip sekarang
/// (KTD-L18); beda membuat lab data tidak berjalan.
///
/// # Errors
/// [`LabDataBlock::CommandsChanged`] bila berbeda.
pub fn scripts_match(approved: &str, current: &str) -> Result<(), LabDataBlock> {
    if !approved.is_empty() && approved.eq_ignore_ascii_case(current) {
        Ok(())
    } else {
        Err(LabDataBlock::CommandsChanged)
    }
}

// ---------------------------------------------------------------------------
// Status sesi data (diagram di plan)

/// Fase sesi lab data; disimpan di `lab_sessions.phase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionPhase {
    Snapshotting,
    Running,
    Restoring,
    RestoreWaiting,
    RestoreFailed,
    Done,
}

impl SessionPhase {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Snapshotting => "snapshotting",
            Self::Running => "running",
            Self::Restoring => "restoring",
            Self::RestoreWaiting => "restore_waiting",
            Self::RestoreFailed => "restore_failed",
            Self::Done => "done",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "snapshotting" => Self::Snapshotting,
            "running" => Self::Running,
            "restoring" => Self::Restoring,
            "restore_waiting" => Self::RestoreWaiting,
            "restore_failed" => Self::RestoreFailed,
            "done" => Self::Done,
            _ => return None,
        })
    }

    /// Kunci penyimpanan dipegang di semua fase selain `done` (KTD-L21).
    #[must_use]
    pub const fn holds_lock(self) -> bool {
        !matches!(self, Self::Done)
    }

    /// Fase yang dipulihkan saat daemon start (KTD-L22).
    #[must_use]
    pub const fn needs_recovery(self) -> bool {
        matches!(
            self,
            Self::Snapshotting | Self::Running | Self::Restoring | Self::RestoreWaiting
        )
    }
}

/// Peristiwa yang menggerakkan sesi data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    SnapshotOk,
    SnapshotFailed,
    /// Sidik kartu berbeda, kartu terakhir selesai, lab dibatalkan atau gagal.
    NeedRestore,
    RestoredNext,
    RestoredLast,
    OtherClients,
    ClientsGone,
    RestoreFailed,
    Retry,
    DeveloperRestored,
    /// Daemon start dengan sesi yang belum selesai.
    DaemonRestart,
}

/// Transisi yang tidak sah.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("lab session cannot go from {from:?} on {event:?}")]
pub struct BadTransition {
    pub from: SessionPhase,
    pub event: SessionEvent,
}

/// Fase berikutnya, atau [`BadTransition`] bila peristiwa tidak sah di fase
/// ini (mis. `running` langsung ke `done`).
///
/// # Errors
/// [`BadTransition`] untuk pasangan fase dan peristiwa yang tidak ada di
/// diagram.
pub fn next_phase(from: SessionPhase, event: SessionEvent) -> Result<SessionPhase, BadTransition> {
    use SessionEvent as E;
    use SessionPhase as P;
    let to = match (from, event) {
        (P::Snapshotting, E::SnapshotOk) | (P::Restoring, E::RestoredNext) => P::Running,
        (P::Snapshotting, E::SnapshotFailed | E::DaemonRestart)
        | (P::Restoring, E::RestoredLast)
        | (P::RestoreFailed, E::DeveloperRestored) => P::Done,
        (P::Running | P::RestoreWaiting, E::NeedRestore | E::DaemonRestart)
        | (P::RestoreWaiting, E::ClientsGone)
        | (P::RestoreFailed, E::Retry)
        | (P::Restoring, E::DaemonRestart) => P::Restoring,
        (P::Restoring, E::OtherClients) => P::RestoreWaiting,
        (P::Restoring, E::RestoreFailed) => P::RestoreFailed,
        _ => return Err(BadTransition { from, event }),
    };
    Ok(to)
}

// ---------------------------------------------------------------------------
// Hasil restore (R23)

/// Hasil restore dan sidik satu penyimpanan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreRestore {
    pub store: String,
    pub succeeded: bool,
    pub timed_out: bool,
    /// Sidik sebelum lab.
    pub before: String,
    /// Sidik setelah restore; `None` bila sidik gagal.
    pub after: Option<String>,
    /// Daftar tabel berubah (mis. migrasi run lain).
    pub schema_changed: bool,
}

/// Hasil restore semua penyimpanan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOutcome {
    Restored,
    Failed(String),
}

/// Restore berhasil hanya bila setiap penyimpanan selesai tepat waktu dan
/// sidiknya sama dengan sebelum lab (R23).
#[must_use]
pub fn restore_outcome(results: &[StoreRestore]) -> RestoreOutcome {
    for r in results {
        if r.schema_changed {
            return RestoreOutcome::Failed("the database schema changed during the lab".to_owned());
        }
        if r.timed_out {
            return RestoreOutcome::Failed(format!("restoring {} timed out", r.store));
        }
        if !r.succeeded {
            return RestoreOutcome::Failed(format!("restoring {} failed", r.store));
        }
        match &r.after {
            None => {
                return RestoreOutcome::Failed(format!(
                    "the fingerprint of {} could not be read",
                    r.store
                ));
            }
            Some(after) if after != &r.before => {
                return RestoreOutcome::Failed(format!(
                    "{} differs from before the lab after restoring",
                    r.store
                ));
            }
            Some(_) => {}
        }
    }
    RestoreOutcome::Restored
}

// ---------------------------------------------------------------------------
// Penyimpanan, kunci, dan perintah (KTD-L17, KTD-L19, KTD-L21)

/// Jenis penyimpanan state lokal app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreKind {
    Postgres(DbTarget),
    /// Folder lokal (mis. unggahan); path kanonis diisi daemon.
    Folder(String),
}

/// Satu penyimpanan dengan perintahnya (KTD-L19).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabStore {
    pub name: String,
    pub kind: StoreKind,
    pub snapshot: String,
    pub restore: String,
    pub fingerprint: String,
    /// Perubahan kecil untuk uji bolak-balik saat persetujuan.
    pub test_change: String,
}

/// Seed bernama yang boleh dipanggil reviewer lab (`lab_seed`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabSeed {
    pub name: String,
    pub command: String,
}

/// Bagian data perlengkapan lab (fase 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabData {
    /// Folder skrip di repo, relatif; disalin dan dipin saat persetujuan.
    pub scripts_dir: String,
    pub stores: Vec<LabStore>,
    pub seeds: Vec<LabSeed>,
}

/// Batas jumlah penyimpanan dan seed.
pub const MAX_STORES: usize = 4;
pub const MAX_SEEDS: usize = 16;

/// Kunci satu penyimpanan: `pg:local:<port>/<db>` untuk `PostgreSQL` lokal
/// (socket Unix dan TCP loopback ke port yang sama dianggap satu server),
/// `pg:<host>:<port>/<db>` untuk host lain, `dir:<path>` untuk folder.
#[must_use]
pub fn lock_key(kind: &StoreKind) -> String {
    match kind {
        StoreKind::Postgres(t) => {
            let host = if is_local_db(&t.host) {
                "local".to_owned()
            } else {
                t.host.to_ascii_lowercase()
            };
            format!("pg:{host}:{}/{}", t.port, t.database)
        }
        StoreKind::Folder(path) => format!("dir:{}", path.trim_end_matches('/')),
    }
}

/// Kunci semua penyimpanan, terurut dan tanpa duplikat, agar diambil dengan
/// urutan yang sama oleh setiap daemon (tanpa deadlock).
#[must_use]
pub fn lock_keys(stores: &[LabStore]) -> Vec<String> {
    let mut keys: Vec<String> = stores.iter().map(|s| lock_key(&s.kind)).collect();
    keys.sort();
    keys.dedup();
    keys
}

/// Nama berkas aman untuk kunci (`<nama>.lock`, `<nama>.dirty`).
#[must_use]
pub fn lock_file_name(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Program yang menjalankan kode lain dari argumen; tidak boleh menjadi
/// perintah lab (KTD-L17).
const RUNNERS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "fish", "env", "npm", "npx", "pnpm", "yarn", "bun", "bunx",
    "node", "deno", "python", "python3", "ruby", "perl", "php", "make", "just", "go", "cargo",
    "sudo", "xargs",
];
const EVAL_FLAGS: &[&str] = &["-e", "-c", "--eval", "--command", "-p", "--print"];

/// Perintah lab yang ditolak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("{0} is empty")]
    Empty(String),
    #[error("{0} must be an executable file inside the scripts folder, e.g. snapshot.sh")]
    NotInScripts(String),
    #[error("{0} cannot use a runner or interpreter flag such as `npm run` or `-e`")]
    Runner(String),
    #[error("{0} cannot refer to paths outside the scripts folder")]
    OutsidePath(String),
    #[error("{0} contains shell characters; commands run without a shell")]
    ShellChars(String),
}

/// Memisah perintah menjadi argv tanpa shell (spasi saja; tanpa kutip).
#[must_use]
pub fn command_argv(command: &str) -> Vec<String> {
    command.split_whitespace().map(str::to_owned).collect()
}

/// Perintah lab: argv tanpa shell yang argv[0]-nya berkas di folder skrip
/// (relatif, tanpa `..`), bukan runner, tanpa flag eval. Untuk snapshot,
/// restore, dan sidik (`self_contained`), argumen juga tidak boleh menunjuk
/// keluar folder skrip.
///
/// # Errors
/// [`CommandError`] dengan nama perintah `what`.
pub fn check_command(command: &str, what: &str, self_contained: bool) -> Result<(), CommandError> {
    let name = what.to_owned();
    if command.contains([
        '|', '&', ';', '<', '>', '`', '$', '(', ')', '\'', '"', '\\', '*', '?',
    ]) {
        return Err(CommandError::ShellChars(name));
    }
    let argv = command_argv(command);
    let Some(program) = argv.first() else {
        return Err(CommandError::Empty(name));
    };
    if !relative_inside(program) {
        return Err(CommandError::NotInScripts(name));
    }
    let base = program.rsplit('/').next().unwrap_or(program);
    if RUNNERS.contains(&base) || argv.iter().any(|a| EVAL_FLAGS.contains(&a.as_str())) {
        return Err(CommandError::Runner(name));
    }
    if self_contained
        && argv[1..]
            .iter()
            .any(|a| a.starts_with('/') || a.split('/').any(|s| s == "..") || a.starts_with('~'))
    {
        return Err(CommandError::OutsidePath(name));
    }
    Ok(())
}

fn relative_inside(path: &str) -> bool {
    let p = path.strip_prefix("./").unwrap_or(path);
    !p.is_empty()
        && !p.starts_with('/')
        && !p.starts_with('~')
        && p.split('/').all(|s| !s.is_empty() && s != "..")
}

/// Bagian data perlengkapan yang ditolak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DataError {
    #[error("the scripts folder must be a subfolder of the repo, without `..`")]
    BadScriptsDir,
    #[error("declare at least one local store (database or folder)")]
    NoStores,
    #[error("declare at most {MAX_STORES} stores")]
    TooManyStores,
    #[error("declare at most {MAX_SEEDS} seeds")]
    TooManySeeds,
    #[error("store or seed name `{0}` must be 1-16 lowercase letters, digits or dashes")]
    BadName(String),
    #[error("`{0}` is listed twice")]
    Duplicate(String),
    #[error("store {0} is not local")]
    NotLocal(String),
    #[error("store {0} must be an absolute folder path")]
    BadFolder(String),
    #[error(transparent)]
    Command(#[from] CommandError),
}

/// Memeriksa bagian data perlengkapan (KTD-L17, KTD-L19): setiap penyimpanan
/// lokal punya snapshot, restore, sidik, dan perubahan uji yang mandiri di
/// folder skrip; seed bernama dengan perintah di folder skrip.
///
/// # Errors
/// [`DataError`] pertama yang ditemukan.
pub fn validate_data(d: &LabData) -> Result<(), DataError> {
    if !relative_inside(d.scripts_dir.trim_end_matches('/')) {
        return Err(DataError::BadScriptsDir);
    }
    if d.stores.is_empty() {
        return Err(DataError::NoStores);
    }
    if d.stores.len() > MAX_STORES {
        return Err(DataError::TooManyStores);
    }
    if d.seeds.len() > MAX_SEEDS {
        return Err(DataError::TooManySeeds);
    }
    let mut names: Vec<&str> = Vec::new();
    for s in &d.stores {
        check_name(&s.name, &mut names)?;
        match &s.kind {
            StoreKind::Postgres(t) if !is_local_db(&t.host) => {
                return Err(DataError::NotLocal(s.name.clone()));
            }
            StoreKind::Folder(p) if !p.starts_with('/') || p.split('/').any(|x| x == "..") => {
                return Err(DataError::BadFolder(s.name.clone()));
            }
            _ => {}
        }
        for (cmd, what) in [
            (&s.snapshot, "snapshot"),
            (&s.restore, "restore"),
            (&s.fingerprint, "fingerprint"),
            (&s.test_change, "test change"),
        ] {
            check_command(cmd, &format!("The {what} command of {}", s.name), true)?;
        }
    }
    for seed in &d.seeds {
        check_name(&seed.name, &mut names)?;
        check_command(&seed.command, &format!("Seed {}", seed.name), false)?;
    }
    Ok(())
}

fn check_name<'a>(name: &'a str, seen: &mut Vec<&'a str>) -> Result<(), DataError> {
    if !crate::lab::valid_role(name) {
        return Err(DataError::BadName(name.to_owned()));
    }
    if seen.contains(&name) {
        return Err(DataError::Duplicate(name.to_owned()));
    }
    seen.push(name);
    Ok(())
}

/// Nama seed yang persis ada di perlengkapan (KTD-L17).
#[must_use]
pub fn find_seed<'a>(d: &'a LabData, name: &str) -> Option<&'a LabSeed> {
    d.seeds.iter().find(|s| s.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(host: &str, port: u16, db: &str) -> StoreKind {
        StoreKind::Postgres(DbTarget {
            host: host.to_owned(),
            port,
            database: db.to_owned(),
        })
    }

    fn store(name: &str, kind: StoreKind) -> LabStore {
        LabStore {
            name: name.to_owned(),
            kind,
            snapshot: "snapshot.sh tenant".to_owned(),
            restore: "restore.sh tenant".to_owned(),
            fingerprint: "fingerprint.sh tenant".to_owned(),
            test_change: "touch-row.sh".to_owned(),
        }
    }

    fn data() -> LabData {
        LabData {
            scripts_dir: "review-fixtures/lab".to_owned(),
            stores: vec![
                store("tenant", pg("localhost", 5432, "kelola")),
                store(
                    "uploads",
                    StoreKind::Folder("/home/dev/kelola/uploads".to_owned()),
                ),
            ],
            seeds: vec![LabSeed {
                name: "kb-values".to_owned(),
                command: "seed-kb.sh 12".to_owned(),
            }],
        }
    }

    /// AE2: hanya database di mesin sendiri.
    #[test]
    fn database_lokal() {
        for h in [
            "localhost",
            "127.0.0.1",
            "127.1.2.3",
            "::1",
            "[::1]",
            "/var/run/postgresql",
        ] {
            assert!(is_local_db(h), "{h}");
        }
        for h in [
            "db.staging.example.com",
            "10.0.0.5",
            "localhost.evil.com",
            "::ffff:10.0.0.5",
            "",
        ] {
            assert!(!is_local_db(h), "{h}");
        }
        assert!(is_local_db("::ffff:127.0.0.1"));
    }

    #[test]
    fn dsn_dengan_host_tersembunyi_ditolak() {
        assert_eq!(
            parse_dsn("postgres://u@localhost/db?host=prod.example.com"),
            Err(DsnError::HostInQuery)
        );
        assert_eq!(
            parse_dsn("postgres://u@localhost,prod/db"),
            Err(DsnError::SeveralHosts)
        );
        assert_eq!(
            parse_dsn("postgresql://u:p@127.0.0.1:5433/kelola?sslmode=disable"),
            Ok(DbTarget {
                host: "127.0.0.1".to_owned(),
                port: 5433,
                database: "kelola".to_owned()
            })
        );
        assert_eq!(
            parse_dsn("postgres://u@[::1]/db").map(|t| (t.host, t.port)),
            Ok(("::1".to_owned(), 5432))
        );
        assert_eq!(parse_dsn("mysql://x/y"), Err(DsnError::NotPostgres));
        for bad in [
            "postgres://u@localhost/db?%68ost=prod",
            "postgres://u@localhost/db?service=prod",
            "postgres://u@localhost/db?passfile=/tmp/p",
            "postgres://u@localhost/db?port=6000",
            "postgres://u@local%68ost/db",
        ] {
            assert_eq!(parse_dsn(bad), Err(DsnError::HostInQuery), "{bad}");
        }
        assert!(
            parse_dsn("postgres://u@localhost/db?sslmode=disable&application_name=api").is_ok()
        );
        assert_eq!(
            parse_dsn("postgres://u@localhost"),
            Err(DsnError::NoDatabase)
        );
        assert_eq!(
            parse_dsn("postgres://u@localhost:abc/db"),
            Err(DsnError::BadPort)
        );
    }

    #[test]
    fn pernyataan_terlarang_ditolak() {
        for sql in [
            "CREATE EXTENSION dblink",
            "COPY t TO PROGRAM 'id'",
            "SELECT lo_export(1, '/tmp/x')",
            "ALTER SYSTEM SET x = 1",
            "CREATE FUNCTION f() RETURNS int LANGUAGE plpython3u AS 'x'",
            "DO $$ BEGIN PERFORM 1; END $$",
            "/* x */ COPY t FROM stdin",
            "cOpY t TO '/tmp/a'",
            "SET ROLE postgres",
            "set session authorization admin",
            "select set_config('role','x',false)",
            "SELECT 1; DROP TABLE t",
            "select pg_read_file('/etc/passwd')",
            "select E'\\x27; drop table t'",
            "select e'a'",
            "select U&'\\0063opy'",
            "select $x$ copy $x$",
            "select $1",
            "  ",
        ] {
            assert!(check_statement(sql).is_err(), "{sql}");
        }
        for sql in [
            "UPDATE corporate_configs SET kb_max = 2 WHERE id = 7",
            "INSERT INTO kb_values (employee_id, score) VALUES (3, 1);",
            "SELECT * FROM t WHERE kind = 'e' AND note = 'it''s'",
            "SELECT count(*) FROM kb_values -- cek\n",
            "select 'create' as label",
        ] {
            // Kata terlarang di dalam string tetap ditolak kecuali di komentar;
            // pernyataan biasa diterima.
            let r = check_statement(sql);
            if sql.contains("'create'") {
                assert!(r.is_err(), "{sql}");
            } else {
                assert_eq!(r, Ok(()), "{sql}");
            }
        }
    }

    #[test]
    fn hak_role_dipetakan_positif() {
        assert_eq!(role_block(&RolePrivileges::default()), None);
        let with = |f: fn(&mut RolePrivileges)| {
            let mut p = RolePrivileges::default();
            f(&mut p);
            role_block(&p)
        };
        assert_eq!(
            with(|p| p.attributes = vec![RoleAttribute::CreateDb, RoleAttribute::Superuser]),
            Some(LabDataBlock::Superuser)
        );
        assert_eq!(
            with(|p| p.attributes = vec![RoleAttribute::CreateRole]),
            Some(LabDataBlock::RoleAttribute("CREATEROLE"))
        );
        assert_eq!(
            with(|p| p.member_of = vec!["pg_read_all_data".to_owned()]),
            Some(LabDataBlock::MemberOf("pg_read_all_data".to_owned()))
        );
        assert_eq!(
            with(|p| p.member_of = vec!["kelola_app".to_owned()]),
            Some(LabDataBlock::MemberOf("kelola_app".to_owned()))
        );
        assert_eq!(with(|p| p.member_of = vec!["PUBLIC".to_owned()]), None);
        assert_eq!(
            with(|p| p.create_on = vec!["schema public".to_owned()]),
            Some(LabDataBlock::CanCreate("schema public".to_owned()))
        );
        assert_eq!(
            with(|p| p.trigger_on = vec!["kb_values".to_owned()]),
            Some(LabDataBlock::Trigger("kb_values".to_owned()))
        );
        assert_eq!(
            with(|p| p.owned_objects = 3),
            Some(LabDataBlock::OwnsObjects(3))
        );
        assert_eq!(
            with(|p| p.extensions = vec!["plpgsql".to_owned(), "postgres_fdw".to_owned()]),
            Some(LabDataBlock::Extension("postgres_fdw".to_owned()))
        );
        assert_eq!(
            with(|p| p.missing_access = vec!["kb_new".to_owned()]),
            Some(LabDataBlock::NoAccess("kb_new".to_owned()))
        );
        assert!(
            LabDataBlock::NoAccess("kb_new".to_owned())
                .to_string()
                .contains("lab database role has no access to kb_new")
        );
    }

    #[test]
    fn klien_lain_hanya_di_database_lab() {
        let row = |pid, db: Option<&str>, kind: Option<&str>| PgActivity {
            pid,
            datname: db.map(str::to_owned),
            backend_type: kind.map(str::to_owned),
            usename: Some("kelola".to_owned()),
            application_name: Some("psql".to_owned()),
            client_addr: Some("127.0.0.1".to_owned()),
        };
        let activity = vec![
            row(1, Some("kelola"), Some("client backend")),
            row(2, Some("lain"), Some("client backend")),
            row(3, Some("kelola"), Some("autovacuum worker")),
            row(4, Some("kelola"), Some("client backend")),
            row(5, None, None),
        ];
        let lab = vec!["kelola".to_owned()];
        let found: Vec<i32> = other_clients(&activity, &lab, &[4])
            .iter()
            .map(|a| a.pid)
            .collect();
        assert_eq!(found, [1, 5], "kolom tersembunyi dihitung (gagal tertutup)");
        let text = describe_clients(&other_clients(&activity, &lab, &[4, 5]));
        assert_eq!(text, "psql from 127.0.0.1 as kelola");
    }

    #[test]
    fn digest_skrip() {
        assert_eq!(scripts_match("abc", "ABC"), Ok(()));
        assert_eq!(
            scripts_match("abc", "abd"),
            Err(LabDataBlock::CommandsChanged)
        );
        assert_eq!(scripts_match("", ""), Err(LabDataBlock::CommandsChanged));
    }

    #[test]
    fn mesin_status_sesi() {
        use SessionEvent as E;
        use SessionPhase as P;
        assert_eq!(next_phase(P::Snapshotting, E::SnapshotOk), Ok(P::Running));
        assert_eq!(next_phase(P::Snapshotting, E::DaemonRestart), Ok(P::Done));
        assert_eq!(next_phase(P::Running, E::NeedRestore), Ok(P::Restoring));
        assert_eq!(next_phase(P::Running, E::DaemonRestart), Ok(P::Restoring));
        assert_eq!(next_phase(P::Restoring, E::RestoredNext), Ok(P::Running));
        assert_eq!(next_phase(P::Restoring, E::RestoredLast), Ok(P::Done));
        assert_eq!(
            next_phase(P::Restoring, E::OtherClients),
            Ok(P::RestoreWaiting)
        );
        assert_eq!(
            next_phase(P::RestoreWaiting, E::ClientsGone),
            Ok(P::Restoring)
        );
        assert_eq!(
            next_phase(P::Restoring, E::RestoreFailed),
            Ok(P::RestoreFailed)
        );
        assert_eq!(next_phase(P::RestoreFailed, E::Retry), Ok(P::Restoring));
        assert_eq!(
            next_phase(P::RestoreFailed, E::DeveloperRestored),
            Ok(P::Done)
        );
        // Transisi tidak sah.
        assert!(
            next_phase(P::Running, E::RestoredLast).is_err(),
            "running → done langsung"
        );
        assert!(next_phase(P::Running, E::DeveloperRestored).is_err());
        assert!(next_phase(P::RestoreFailed, E::RestoredLast).is_err());
        assert!(
            next_phase(P::RestoreWaiting, E::RestoreFailed).is_err(),
            "tidak naik sendiri"
        );
        assert!(next_phase(P::Done, E::Retry).is_err());
        for p in [
            P::Snapshotting,
            P::Running,
            P::Restoring,
            P::RestoreWaiting,
            P::RestoreFailed,
            P::Done,
        ] {
            assert_eq!(SessionPhase::parse(p.as_str()), Some(p));
        }
        assert!(P::RestoreFailed.holds_lock() && !P::Done.holds_lock());
        assert!(P::RestoreWaiting.needs_recovery() && !P::RestoreFailed.needs_recovery());
    }

    /// R23: satu penyimpanan dengan sidik berbeda membuat restore gagal.
    #[test]
    fn hasil_restore_per_penyimpanan() {
        let ok = |name: &str| StoreRestore {
            store: name.to_owned(),
            succeeded: true,
            timed_out: false,
            before: "aa".to_owned(),
            after: Some("aa".to_owned()),
            schema_changed: false,
        };
        assert_eq!(
            restore_outcome(&[ok("tenant"), ok("uploads")]),
            RestoreOutcome::Restored
        );
        let mut differs = ok("uploads");
        differs.after = Some("bb".to_owned());
        assert!(matches!(
            restore_outcome(&[ok("tenant"), differs]),
            RestoreOutcome::Failed(r) if r.contains("uploads")
        ));
        let mut slow = ok("tenant");
        slow.timed_out = true;
        assert!(
            matches!(restore_outcome(&[slow]), RestoreOutcome::Failed(r) if r.contains("timed out"))
        );
        let mut schema = ok("tenant");
        schema.schema_changed = true;
        assert_eq!(
            restore_outcome(&[schema]),
            RestoreOutcome::Failed("the database schema changed during the lab".to_owned())
        );
        let mut exit = ok("tenant");
        exit.succeeded = false;
        assert!(matches!(
            restore_outcome(&[exit]),
            RestoreOutcome::Failed(_)
        ));
        let mut unread = ok("tenant");
        unread.after = None;
        assert!(matches!(
            restore_outcome(&[unread]),
            RestoreOutcome::Failed(_)
        ));
    }

    #[test]
    fn kunci_per_penyimpanan() {
        let a = lock_key(&pg("localhost", 5432, "db"));
        assert_eq!(a, lock_key(&pg("127.0.0.1", 5432, "db")));
        assert_eq!(a, lock_key(&pg("/var/run/postgresql", 5432, "db")));
        assert_ne!(a, lock_key(&pg("localhost", 5433, "db")));
        // Database sama, folder unggahan berbeda: kunci database dibagi.
        let one = vec![
            store("db", pg("localhost", 5432, "db")),
            store("up", StoreKind::Folder("/srv/a".to_owned())),
        ];
        let two = vec![
            store("up", StoreKind::Folder("/srv/b/".to_owned())),
            store("db", pg("127.0.0.1", 5432, "db")),
        ];
        let k1 = lock_keys(&one);
        let k2 = lock_keys(&two);
        assert!(k1.contains(&a) && k2.contains(&a));
        assert_ne!(k1, k2);
        assert!(k1.windows(2).all(|w| w[0] < w[1]), "terurut");
        assert_eq!(lock_file_name("pg:local:5432/db"), "pg_local_5432_db");
    }

    #[test]
    fn perintah_lab_harus_berkas_di_folder_skrip() {
        assert_eq!(
            check_command("snapshot.sh tenant", "snapshot", true),
            Ok(())
        );
        assert_eq!(
            check_command("./bin/restore tenant", "restore", true),
            Ok(())
        );
        for (cmd, own) in [
            ("bun run seed", false),
            ("npm run seed", false),
            ("node -e x", false),
            ("python3 seed.py", false),
            ("/usr/bin/pg_dump x", true),
            ("../src/x.sh", true),
            ("snapshot.sh ../src/x", true),
            ("snapshot.sh /etc/passwd", true),
            ("seed.sh; rm -rf /", false),
            ("seed.sh $(id)", false),
            ("seed.sh -c x", false),
            ("", true),
        ] {
            assert!(check_command(cmd, "x", own).is_err(), "{cmd}");
        }
        // Seed boleh berargumen path relatif biasa.
        assert_eq!(
            check_command("seed-kb.sh fixtures/kb.xlsx", "seed", false),
            Ok(())
        );
    }

    #[test]
    fn validasi_bagian_data() {
        assert_eq!(validate_data(&data()), Ok(()));
        let mut d = data();
        d.stores.clear();
        assert_eq!(validate_data(&d), Err(DataError::NoStores));
        let mut d = data();
        d.stores[0].kind = pg("db.staging.example.com", 5432, "kelola");
        assert_eq!(
            validate_data(&d),
            Err(DataError::NotLocal("tenant".to_owned()))
        );
        let mut d = data();
        d.stores[0].fingerprint = String::new();
        assert!(matches!(
            validate_data(&d),
            Err(DataError::Command(CommandError::Empty(_)))
        ));
        let mut d = data();
        d.stores[0].restore = "restore.sh ../src/x".to_owned();
        assert!(matches!(
            validate_data(&d),
            Err(DataError::Command(CommandError::OutsidePath(_)))
        ));
        let mut d = data();
        d.seeds[0].name = "tenant".to_owned();
        assert_eq!(
            validate_data(&d),
            Err(DataError::Duplicate("tenant".to_owned()))
        );
        let mut d = data();
        d.seeds[0].name = "Bad Name".to_owned();
        assert!(matches!(validate_data(&d), Err(DataError::BadName(_))));
        let mut d = data();
        d.scripts_dir = "../luar".to_owned();
        assert_eq!(validate_data(&d), Err(DataError::BadScriptsDir));
        let mut d = data();
        d.stores[1].kind = StoreKind::Folder("relative/uploads".to_owned());
        assert_eq!(
            validate_data(&d),
            Err(DataError::BadFolder("uploads".to_owned()))
        );
        assert!(find_seed(&data(), "kb-values").is_some());
        assert!(find_seed(&data(), "kb-values; rm").is_none());
    }
}
