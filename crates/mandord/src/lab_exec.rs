//! Eksekutor perintah lab data (plan 2026-11-10-001, U13, KTD-L17–KTD-L19).
//!
//! Folder skrip repo disalin ke `<lab_root>/<repo>/private/scripts/<digest>/`
//! (baca-saja, symlink ditolak) dan semua perintah dijalankan dari salinan itu
//! sebagai argv tanpa shell, dengan env bersih ditambah variabel koneksi dari
//! daemon dan batas waktu. Keluaran dipotong dan disamarkan. Snapshot ditulis
//! ke folder 0700 milik daemon; mode berkasnya diperiksa dan dikencangkan
//! (umask proses anak tidak bisa disetel tanpa `unsafe`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use mandor_domain::lab_data::{LabSeed, LabStore, StoreKind, check_command, command_argv};
use mandor_proc::{Cmd, EnvPolicy, ProcError};
use mandor_usecase::run_ports::DbCredentials;
use secrecy::ExposeSecret;
use sha2::{Digest, Sha256};

/// Batas waktu satu perintah lab (KTD-L19).
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
/// Batas keluaran perintah yang disimpan atau ditampilkan.
pub const MAX_OUTPUT: usize = 64 * 1024;
/// Batas jumlah dan ukuran berkas folder skrip.
const MAX_FILES: usize = 200;
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
/// Batas waktu operasi berkas.
const FILE_TIMEOUT: Duration = Duration::from_secs(30);

/// Kegagalan eksekutor lab.
#[derive(Debug)]
pub enum LabExecError {
    Scripts(String),
    Command(String),
    Io(String),
}

impl std::fmt::Display for LabExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scripts(m) => write!(f, "the lab scripts folder {m}"),
            Self::Command(m) => f.write_str(m),
            Self::Io(m) => write!(f, "lab files: {m}"),
        }
    }
}

impl std::error::Error for LabExecError {}

impl From<std::io::Error> for LabExecError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Salinan folder skrip yang dipin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    pub digest: String,
    pub dir: PathBuf,
}

/// Hasil satu perintah lab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    pub success: bool,
    pub timed_out: bool,
    /// stdout lalu stderr, disamarkan dan dipotong [`MAX_OUTPUT`].
    pub output: String,
}

/// Perintah penyimpanan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreCommand {
    Snapshot,
    Restore,
    Fingerprint,
    TestChange,
}

impl StoreCommand {
    fn of(self, store: &LabStore) -> &str {
        match self {
            Self::Snapshot => &store.snapshot,
            Self::Restore => &store.restore,
            Self::Fingerprint => &store.fingerprint,
            Self::TestChange => &store.test_change,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Restore => "restore",
            Self::Fingerprint => "fingerprint",
            Self::TestChange => "test change",
        }
    }
}

/// Eksekutor perintah lab milik daemon.
#[derive(Debug, Clone)]
pub struct LabExec {
    lab_root: PathBuf,
    base_env: Vec<(String, String)>,
    timeout: Duration,
}

impl LabExec {
    /// `base_env` = env login yang sudah dibersihkan; variabel `PG*` dan
    /// `DATABASE_URL` milik developer dibuang agar hanya kredensial dari
    /// daemon yang sampai ke perintah.
    #[must_use]
    pub fn new(lab_root: PathBuf, base_env: Vec<(String, String)>) -> Self {
        let base_env = base_env
            .into_iter()
            .filter(|(k, _)| {
                !k.starts_with("PG") && k != "DATABASE_URL" && !k.starts_with("MANDOR_")
            })
            .collect();
        Self {
            lab_root,
            base_env,
            timeout: COMMAND_TIMEOUT,
        }
    }

    /// Batas waktu lain (test).
    #[cfg(test)]
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn private(&self, repo_id: &str) -> PathBuf {
        crate::lab_secrets::private_dir(&self.lab_root, repo_id)
    }

    /// Digest isi folder skrip di worktree (untuk `scripts_match`).
    ///
    /// # Errors
    /// Folder tidak ada, melewati symlink, terlalu besar, atau tidak terbaca.
    pub async fn worktree_digest(
        &self,
        worktree: &Path,
        scripts_dir: &str,
    ) -> Result<String, LabExecError> {
        let root = worktree.to_path_buf();
        let rel = scripts_dir.to_owned();
        blocking(move || {
            let src = scripts_source(&root, &rel)?;
            Ok(digest_of(&collect(&src)?))
        })
        .await
    }

    /// Menyalin folder skrip ke salinan baca-saja milik daemon dan
    /// mengembalikan digest serta foldernya (KTD-L18). Salinan dengan digest
    /// sama dipakai ulang.
    ///
    /// # Errors
    /// Folder skrip tidak sah, berisi symlink, atau tidak bisa disalin.
    pub async fn pin_scripts(
        &self,
        repo_id: &str,
        worktree: &Path,
        scripts_dir: &str,
    ) -> Result<Pinned, LabExecError> {
        let root = worktree.to_path_buf();
        let rel = scripts_dir.to_owned();
        let base = self.private(repo_id).join("scripts");
        blocking(move || {
            let src = scripts_source(&root, &rel)?;
            let files = collect(&src)?;
            let digest = digest_of(&files);
            let dir = base.join(&digest);
            if !dir.is_dir() {
                write_copy(&base, &dir, &files)?;
            }
            Ok(Pinned { digest, dir })
        })
        .await
    }

    /// Folder snapshot baru (0700) untuk sesi.
    ///
    /// # Errors
    /// Folder tidak bisa dibuat.
    pub async fn snapshot_dir(
        &self,
        repo_id: &str,
        session_id: &str,
    ) -> Result<PathBuf, LabExecError> {
        let dir = self
            .private(repo_id)
            .join("snapshots")
            .join(safe(session_id));
        let d = dir.clone();
        blocking(move || {
            private_dir_all(&d)?;
            Ok(d)
        })
        .await
    }

    /// Mengencangkan mode berkas snapshot menjadi 0600; mengembalikan berkas
    /// yang sebelumnya bisa dibaca user lain.
    ///
    /// # Errors
    /// Folder tidak terbaca.
    pub async fn tighten_snapshot(&self, dir: &Path) -> Result<Vec<String>, LabExecError> {
        let d = dir.to_path_buf();
        blocking(move || tighten(&d)).await
    }

    /// Menghapus folder snapshot; tidak pernah gagal.
    pub async fn remove_snapshot(&self, dir: &Path) {
        if tokio::time::timeout(FILE_TIMEOUT, tokio::fs::remove_dir_all(dir))
            .await
            .is_err()
        {
            tracing::warn!(dir = %dir.display(), "folder snapshot lab tidak terhapus tepat waktu");
        }
    }

    /// Menjalankan perintah penyimpanan dengan kredensial pemeliharaan (untuk
    /// database). Tidak pernah menerima path worktree.
    ///
    /// # Errors
    /// Perintah tidak sah atau tidak bisa dijalankan.
    pub async fn run_store(
        &self,
        pinned: &Pinned,
        store: &LabStore,
        which: StoreCommand,
        maintenance: Option<&DbCredentials>,
        snapshot_dir: &Path,
    ) -> Result<ExecResult, LabExecError> {
        let what = format!("The {} command of {}", which.name(), store.name);
        let mut env = vec![
            ("MANDOR_STORE".to_owned(), store.name.clone()),
            (
                "MANDOR_SNAPSHOT_DIR".to_owned(),
                snapshot_dir.display().to_string(),
            ),
        ];
        if let StoreKind::Folder(path) = &store.kind {
            env.push(("MANDOR_STORE_PATH".to_owned(), path.clone()));
        }
        self.run(pinned, which.of(store), &what, true, env, maintenance)
            .await
    }

    /// Menjalankan seed dengan kredensial role lab dan `MANDOR_WORKTREE`
    /// (KTD-L17); kredensial pemeliharaan tidak pernah diberikan.
    ///
    /// # Errors
    /// Perintah tidak sah atau tidak bisa dijalankan.
    pub async fn run_seed(
        &self,
        pinned: &Pinned,
        seed: &LabSeed,
        lab: Option<&DbCredentials>,
        worktree: &Path,
    ) -> Result<ExecResult, LabExecError> {
        let env = vec![
            ("MANDOR_SEED".to_owned(), seed.name.clone()),
            ("MANDOR_WORKTREE".to_owned(), worktree.display().to_string()),
        ];
        self.run(
            pinned,
            &seed.command,
            &format!("Seed {}", seed.name),
            false,
            env,
            lab,
        )
        .await
    }

    async fn run(
        &self,
        pinned: &Pinned,
        command: &str,
        what: &str,
        self_contained: bool,
        extra: Vec<(String, String)>,
        creds: Option<&DbCredentials>,
    ) -> Result<ExecResult, LabExecError> {
        check_command(command, what, self_contained)
            .map_err(|e| LabExecError::Command(e.to_string()))?;
        let argv = command_argv(command);
        let Some((program, rest)) = argv.split_first() else {
            return Err(LabExecError::Command(format!("{what} is empty")));
        };
        let path = pinned
            .dir
            .join(program.strip_prefix("./").unwrap_or(program));
        let meta = tokio::fs::symlink_metadata(&path).await.map_err(|_| {
            LabExecError::Command(format!(
                "{what}: {program} is not in the approved scripts folder"
            ))
        })?;
        if !meta.is_file() {
            return Err(LabExecError::Command(format!(
                "{what}: {program} is not a regular file"
            )));
        }
        let mut env = self.base_env.clone();
        env.extend(extra);
        let mut secrets = Vec::new();
        if let Some(c) = creds {
            env.extend(pg_env(c));
            secrets.push(c.password.expose_secret().to_owned());
        }
        let cmd = Cmd::new(&path)
            .args(rest)
            .cwd(&pinned.dir)
            .env_policy(EnvPolicy::Explicit(env))
            .scrub_tokens()
            .timeout(self.timeout);
        match mandor_proc::run(&cmd).await {
            Ok(out) => Ok(ExecResult {
                success: out.success(),
                timed_out: false,
                output: limit(&redact(&format!("{}{}", out.stdout, out.stderr), &secrets)),
            }),
            Err(ProcError::Timeout { .. }) => Ok(ExecResult {
                success: false,
                timed_out: true,
                output: format!(
                    "{what} exceeded the {}s limit and was stopped",
                    self.timeout.as_secs()
                ),
            }),
            Err(e) => Err(LabExecError::Command(format!("{what}: {e}"))),
        }
    }
}

/// Kredensial satu penyimpanan `postgres`.
pub struct StoreCreds {
    pub lab: DbCredentials,
    pub maintenance: DbCredentials,
}

/// Jeda pemantauan `pg_stat_activity` selama seed berjalan.
const SEED_POLL: Duration = Duration::from_millis(200);

impl LabExec {
    /// Uji persetujuan lab data (KTD-L15–KTD-L19): database lokal, role
    /// pemeliharaan membaca semua klien, role lab berhak minimum, lalu untuk
    /// setiap penyimpanan sidik → snapshot → perubahan uji → sidik berubah →
    /// restore → sidik kembali sama; setiap seed hanya boleh tersambung dengan
    /// role lab. Snapshot sementara ditulis ke `snap`. Mengembalikan alasan
    /// penolakan dalam bahasa Inggris.
    ///
    /// # Errors
    /// Alasan pertama perlengkapan tidak bisa disetujui.
    pub async fn approval_round_trip<D: mandor_usecase::run_ports::LabDatabase>(
        &self,
        db: &D,
        data: &mandor_domain::lab_data::LabData,
        pinned: &Pinned,
        creds: &std::collections::BTreeMap<String, StoreCreds>,
        worktree: &Path,
        snap: &Path,
    ) -> Result<(), String> {
        let primary = self.check_databases(db, data, creds).await?;
        let mut before = Vec::new();
        for store in &data.stores {
            let maint = creds.get(&store.name).map(|c| &c.maintenance);
            before.push(self.round_trip_store(pinned, store, maint, snap).await?);
        }
        for seed in &data.seeds {
            self.check_seed(db, pinned, seed, primary, worktree).await?;
        }
        if !data.seeds.is_empty() {
            for (store, fp0) in data.stores.iter().zip(&before) {
                let maint = creds.get(&store.name).map(|c| &c.maintenance);
                self.expect(pinned, store, StoreCommand::Restore, maint, snap)
                    .await?;
                let fp = self.fingerprint(pinned, store, maint, snap).await?;
                if &fp != fp0 {
                    return Err(format!(
                        "restoring {} after the seeds did not bring its fingerprint back",
                        store.name
                    ));
                }
            }
        }
        Ok(())
    }

    async fn check_databases<'a, D: mandor_usecase::run_ports::LabDatabase>(
        &self,
        db: &D,
        data: &mandor_domain::lab_data::LabData,
        creds: &'a std::collections::BTreeMap<String, StoreCreds>,
    ) -> Result<Option<&'a StoreCreds>, String> {
        let mut primary = None;
        for store in &data.stores {
            if !matches!(store.kind, StoreKind::Postgres(_)) {
                continue;
            }
            let Some(c) = creds.get(&store.name) else {
                return Err(format!("enter the database roles of {}", store.name));
            };
            let local = db
                .server_is_local(&c.maintenance)
                .await
                .map_err(|e| e.to_string())?;
            if !local {
                return Err(format!(
                    "lab only runs against local databases ({} is not local)",
                    store.name
                ));
            }
            if !db
                .reads_all_stats(&c.maintenance)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err(format!(
                    "the maintenance role of {} must be a superuser or a member of pg_read_all_stats",
                    store.name
                ));
            }
            let p = db
                .role_privileges(&c.maintenance, &c.lab.user)
                .await
                .map_err(|e| e.to_string())?;
            if let Some(block) = mandor_domain::lab_data::role_block(&p) {
                return Err(block.to_string());
            }
            primary.get_or_insert(c);
        }
        Ok(primary)
    }

    async fn round_trip_store(
        &self,
        pinned: &Pinned,
        store: &LabStore,
        maint: Option<&DbCredentials>,
        snap: &Path,
    ) -> Result<String, String> {
        let fp0 = self.fingerprint(pinned, store, maint, snap).await?;
        self.expect(pinned, store, StoreCommand::Snapshot, maint, snap)
            .await?;
        if let Ok(loose) = self.tighten_snapshot(snap).await
            && !loose.is_empty()
        {
            tracing::warn!(store = %store.name, files = loose.len(), "berkas snapshot lab bermode longgar dikencangkan");
        }
        self.expect(pinned, store, StoreCommand::TestChange, maint, snap)
            .await?;
        let fp1 = self.fingerprint(pinned, store, maint, snap).await?;
        if fp1 == fp0 {
            return Err(format!(
                "the test change of {} did not change its fingerprint; the fingerprint must reflect the data",
                store.name
            ));
        }
        self.expect(pinned, store, StoreCommand::Restore, maint, snap)
            .await?;
        let fp2 = self.fingerprint(pinned, store, maint, snap).await?;
        if fp2 != fp0 {
            return Err(format!(
                "restoring {} did not bring its fingerprint back (before: {}, after restore: {})",
                store.name,
                short(&fp0),
                short(&fp2)
            ));
        }
        Ok(fp0)
    }

    async fn fingerprint(
        &self,
        pinned: &Pinned,
        store: &LabStore,
        maint: Option<&DbCredentials>,
        snap: &Path,
    ) -> Result<String, String> {
        let out = self
            .expect(pinned, store, StoreCommand::Fingerprint, maint, snap)
            .await?;
        let fp = out.trim().to_owned();
        if fp.is_empty() {
            return Err(format!(
                "the fingerprint command of {} printed nothing",
                store.name
            ));
        }
        Ok(fp)
    }

    /// Menjalankan perintah penyimpanan dan mewajibkan berhasil.
    async fn expect(
        &self,
        pinned: &Pinned,
        store: &LabStore,
        which: StoreCommand,
        maint: Option<&DbCredentials>,
        snap: &Path,
    ) -> Result<String, String> {
        let r = self
            .run_store(pinned, store, which, maint, snap)
            .await
            .map_err(|e| e.to_string())?;
        if r.success {
            Ok(r.output)
        } else {
            Err(format!(
                "the {} command of {} failed: {}",
                which.name(),
                store.name,
                short(&r.output)
            ))
        }
    }

    /// Seed berjalan sambil `pg_stat_activity` dipantau; role selain role lab
    /// membuat persetujuan ditolak (KTD-L17).
    async fn check_seed<D: mandor_usecase::run_ports::LabDatabase>(
        &self,
        db: &D,
        pinned: &Pinned,
        seed: &LabSeed,
        primary: Option<&StoreCreds>,
        worktree: &Path,
    ) -> Result<(), String> {
        let run = self.run_seed(pinned, seed, primary.map(|c| &c.lab), worktree);
        tokio::pin!(run);
        let mut foreign: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let result = loop {
            tokio::select! {
                r = &mut run => break r,
                () = tokio::time::sleep(SEED_POLL) => {
                    let Some(c) = primary else { continue };
                    if let Ok((rows, own)) = db.activity(&c.maintenance).await {
                        let dbs = [c.lab.target.database.clone()];
                        for r in mandor_domain::lab_data::other_clients(&rows, &dbs, &[own]) {
                            let user = r.usename.clone().unwrap_or_else(|| "a hidden role".to_owned());
                            if user != c.lab.user {
                                foreign.insert(user);
                            }
                        }
                    }
                }
            }
        };
        let r = result.map_err(|e| e.to_string())?;
        if !r.success {
            return Err(format!("seed {} failed: {}", seed.name, short(&r.output)));
        }
        if let Some(user) = foreign.into_iter().next() {
            return Err(format!(
                "seed {} connected to the database as {user}; seeds may only use the lab role",
                seed.name
            ));
        }
        Ok(())
    }
}

/// Potongan pendek keluaran untuk alasan penolakan.
fn short(text: &str) -> String {
    let t = text.trim();
    let mut end = t.len().min(300);
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    t[..end].to_owned()
}

/// Variabel koneksi `PostgreSQL` dari daemon (KTD-L15); password lewat env,
/// tidak pernah lewat argv.
fn pg_env(c: &DbCredentials) -> Vec<(String, String)> {
    vec![
        ("PGHOST".to_owned(), c.target.host.clone()),
        ("PGPORT".to_owned(), c.target.port.to_string()),
        ("PGDATABASE".to_owned(), c.target.database.clone()),
        ("PGUSER".to_owned(), c.user.clone()),
        (
            "PGPASSWORD".to_owned(),
            c.password.expose_secret().to_owned(),
        ),
        ("PGSSLMODE".to_owned(), "disable".to_owned()),
        ("PGAPPNAME".to_owned(), "mandor-lab-script".to_owned()),
    ]
}

fn redact(text: &str, secrets: &[String]) -> String {
    let refs: Vec<&str> = secrets.iter().map(String::as_str).collect();
    mandor_domain::lab::redact_values(text, &refs)
}

fn limit(text: &str) -> String {
    if text.len() <= MAX_OUTPUT {
        return text.to_owned();
    }
    let mut end = MAX_OUTPUT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{} [truncated]", &text[..end])
}

fn safe(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, LabExecError> + Send + 'static,
) -> Result<T, LabExecError> {
    match tokio::time::timeout(FILE_TIMEOUT, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(LabExecError::Io(e.to_string())),
        Err(_) => Err(LabExecError::Io("lab files timed out".to_owned())),
    }
}

/// Folder skrip di bawah worktree; setiap bagian jalur diperiksa tanpa
/// mengikuti symlink.
fn scripts_source(root: &Path, rel: &str) -> Result<PathBuf, LabExecError> {
    let mut path = root.to_path_buf();
    for part in Path::new(rel.trim_end_matches('/')).components() {
        let std::path::Component::Normal(name) = part else {
            return Err(LabExecError::Scripts(
                "must be a plain relative path".to_owned(),
            ));
        };
        path.push(name);
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|_| LabExecError::Scripts(format!("{rel} does not exist")))?;
        if meta.file_type().is_symlink() {
            return Err(LabExecError::Scripts(format!(
                "{rel} passes through a link"
            )));
        }
    }
    if !path.is_dir() {
        return Err(LabExecError::Scripts(format!("{rel} is not a folder")));
    }
    Ok(path)
}

/// Satu berkas folder skrip: path relatif, bisa dieksekusi, isi.
struct ScriptFile {
    rel: String,
    exec: bool,
    bytes: Vec<u8>,
}

fn collect(src: &Path) -> Result<Vec<ScriptFile>, LabExecError> {
    let mut out = Vec::new();
    walk(src, src, &mut out)?;
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<ScriptFile>) -> Result<(), LabExecError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)?;
        let rel = path
            .strip_prefix(root)
            .map_err(|e| LabExecError::Io(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        if meta.file_type().is_symlink() {
            return Err(LabExecError::Scripts(format!("contains a link: {rel}")));
        }
        if meta.is_dir() {
            walk(root, &path, out)?;
        } else if meta.is_file() {
            if out.len() >= MAX_FILES || meta.len() > MAX_FILE_BYTES {
                return Err(LabExecError::Scripts("is too large".to_owned()));
            }
            out.push(ScriptFile {
                rel,
                exec: is_exec(&meta),
                bytes: std::fs::read(&path)?,
            });
        }
    }
    Ok(())
}

#[cfg(unix)]
fn is_exec(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_exec(_meta: &std::fs::Metadata) -> bool {
    true
}

/// SHA-256 atas path, bit eksekusi, dan isi setiap berkas, terurut.
fn digest_of(files: &[ScriptFile]) -> String {
    let mut h = Sha256::new();
    for f in files {
        h.update(f.rel.as_bytes());
        h.update([0, u8::from(f.exec), 0]);
        h.update((f.bytes.len() as u64).to_le_bytes());
        h.update(&f.bytes);
    }
    h.finalize()
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Menulis salinan ke folder sementara lalu memindahkannya, baca-saja.
fn write_copy(base: &Path, dir: &Path, files: &[ScriptFile]) -> Result<(), LabExecError> {
    private_dir_all(base)?;
    let tmp = base.join(format!(".tmp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    private_dir_all(&tmp)?;
    for f in files {
        let target = tmp.join(&f.rel);
        if let Some(parent) = target.parent() {
            private_dir_all(parent)?;
        }
        std::fs::write(&target, &f.bytes)?;
        set_mode(&target, if f.exec { 0o500 } else { 0o400 })?;
    }
    std::fs::rename(&tmp, dir)?;
    Ok(())
}

#[cfg(unix)]
fn private_dir_all(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(not(unix))]
fn private_dir_all(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn tighten(dir: &Path) -> Result<Vec<String>, LabExecError> {
    use std::os::unix::fs::PermissionsExt;
    set_mode(dir, 0o700)?;
    let mut loose = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = std::fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            loose.extend(tighten(&entry.path())?);
        } else if meta.permissions().mode() & 0o077 != 0 {
            loose.push(entry.file_name().to_string_lossy().into_owned());
            set_mode(&entry.path(), 0o600)?;
        }
    }
    Ok(loose)
}

#[cfg(not(unix))]
fn tighten(_dir: &Path) -> Result<Vec<String>, LabExecError> {
    Ok(Vec::new())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use mandor_domain::lab_data::DbTarget;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &Path, name: &str, body: &str) {
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("{e}"));
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        worktree: PathBuf,
        exec: LabExec,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let worktree = dir.path().join("wt");
        let scripts = worktree.join("lab");
        std::fs::create_dir_all(&scripts).unwrap_or_else(|e| panic!("{e}"));
        script(&scripts, "env.sh", "env | sort");
        script(&scripts, "echo-pass.sh", "echo \"pw=$PGPASSWORD\"");
        script(
            &scripts,
            "slow.sh",
            "sleep 30 & echo $! > \"$MANDOR_SNAPSHOT_DIR/child\"; wait",
        );
        let base = vec![
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            ("PGPASSWORD".to_owned(), "milik-developer".to_owned()),
            ("DATABASE_URL".to_owned(), "postgres://x".to_owned()),
        ];
        let exec = LabExec::new(dir.path().join("lab"), base);
        Fixture {
            _dir: dir,
            worktree,
            exec,
        }
    }

    fn creds(user: &str, password: &str) -> DbCredentials {
        DbCredentials {
            target: DbTarget {
                host: "localhost".to_owned(),
                port: 5432,
                database: "kelola".to_owned(),
            },
            user: user.to_owned(),
            password: secrecy::SecretString::from(password.to_owned()),
        }
    }

    fn store(snapshot: &str) -> LabStore {
        LabStore {
            name: "tenant".to_owned(),
            kind: StoreKind::Folder("/srv/uploads".to_owned()),
            snapshot: snapshot.to_owned(),
            restore: "env.sh".to_owned(),
            fingerprint: "env.sh".to_owned(),
            test_change: "env.sh".to_owned(),
        }
    }

    #[tokio::test]
    async fn password_lewat_env_dan_tersamarkan_di_keluaran() {
        let f = fixture();
        let pinned = f
            .exec
            .pin_scripts("repo1", &f.worktree, "lab")
            .await
            .unwrap();
        let snap = f.exec.snapshot_dir("repo1", "s1").await.unwrap();
        let maint = creds("maint", "rahasia-pemeliharaan");
        let out = f
            .exec
            .run_store(
                &pinned,
                &store("echo-pass.sh"),
                StoreCommand::Snapshot,
                Some(&maint),
                &snap,
            )
            .await
            .unwrap();
        assert!(out.success);
        assert!(
            !out.output.contains("rahasia-pemeliharaan"),
            "{}",
            out.output
        );
        assert!(out.output.contains("[redacted]"), "{}", out.output);
        // Env perintah penyimpanan: kredensial pemeliharaan, tanpa worktree,
        // tanpa PG*/DATABASE_URL milik developer.
        let env = f
            .exec
            .run_store(
                &pinned,
                &store("env.sh"),
                StoreCommand::Snapshot,
                Some(&maint),
                &snap,
            )
            .await
            .unwrap()
            .output;
        assert!(env.contains("PGUSER=maint"), "{env}");
        assert!(env.contains("MANDOR_SNAPSHOT_DIR="), "{env}");
        assert!(!env.contains("MANDOR_WORKTREE"), "{env}");
        assert!(
            !env.contains("milik-developer") && !env.contains("DATABASE_URL"),
            "{env}"
        );
        assert!(!env.contains(&f.worktree.display().to_string()), "{env}");
    }

    #[tokio::test]
    async fn seed_mendapat_role_lab_dan_worktree_bukan_pemeliharaan() {
        let f = fixture();
        let pinned = f
            .exec
            .pin_scripts("repo1", &f.worktree, "lab")
            .await
            .unwrap();
        let seed = LabSeed {
            name: "kb".to_owned(),
            command: "env.sh".to_owned(),
        };
        let env = f
            .exec
            .run_seed(
                &pinned,
                &seed,
                Some(&creds("lab_min", "lab-pass")),
                &f.worktree,
            )
            .await
            .unwrap()
            .output;
        assert!(env.contains("PGUSER=lab_min"), "{env}");
        assert!(
            env.contains(&format!("MANDOR_WORKTREE={}", f.worktree.display())),
            "{env}"
        );
        assert!(!env.contains("maint"), "{env}");
        // Perintah di luar folder skrip atau runner ditolak.
        for bad in [
            "bun run seed",
            "/bin/sh -c id",
            "../wt/lab/env.sh",
            "missing.sh",
        ] {
            let seed = LabSeed {
                name: "x".to_owned(),
                command: bad.to_owned(),
            };
            assert!(
                f.exec
                    .run_seed(&pinned, &seed, None, &f.worktree)
                    .await
                    .is_err(),
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn batas_waktu_menghentikan_grup_proses() {
        let f = fixture();
        let exec = f.exec.clone().with_timeout(Duration::from_millis(500));
        let pinned = exec.pin_scripts("repo1", &f.worktree, "lab").await.unwrap();
        let snap = exec.snapshot_dir("repo1", "s2").await.unwrap();
        let out = exec
            .run_store(
                &pinned,
                &store("slow.sh"),
                StoreCommand::Snapshot,
                None,
                &snap,
            )
            .await
            .unwrap();
        assert!(out.timed_out && !out.success);
        let child = std::fs::read_to_string(snap.join("child")).unwrap_or_default();
        let pid = child.trim();
        assert!(!pid.is_empty());
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "anak ikut dihentikan"
        );
    }

    #[tokio::test]
    async fn salinan_dipin_dan_symlink_ditolak() {
        let f = fixture();
        let a = f
            .exec
            .pin_scripts("repo1", &f.worktree, "lab")
            .await
            .unwrap();
        assert!(a.dir.join("env.sh").is_file());
        let mode = std::fs::metadata(a.dir.join("env.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o500, "salinan baca-saja");
        assert_eq!(
            f.exec.worktree_digest(&f.worktree, "lab").await.unwrap(),
            a.digest
        );
        // Skrip di worktree berubah: digest berbeda, salinan lama tetap utuh
        // dan perintah dijalankan dari salinan.
        script(&f.worktree.join("lab"), "env.sh", "echo diubah");
        let now = f.exec.worktree_digest(&f.worktree, "lab").await.unwrap();
        assert!(mandor_domain::lab_data::scripts_match(&a.digest, &now).is_err());
        let snap = f.exec.snapshot_dir("repo1", "s3").await.unwrap();
        let out = f
            .exec
            .run_store(&a, &store("env.sh"), StoreCommand::Snapshot, None, &snap)
            .await
            .unwrap();
        assert!(!out.output.contains("diubah"));
        // Symlink di folder skrip ditolak saat disalin.
        std::os::unix::fs::symlink("/etc/passwd", f.worktree.join("lab/pw")).unwrap();
        assert!(matches!(
            f.exec.pin_scripts("repo1", &f.worktree, "lab").await,
            Err(LabExecError::Scripts(m)) if m.contains("link")
        ));
        assert!(
            f.exec
                .pin_scripts("repo1", &f.worktree, "../wt")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn folder_snapshot_privat_dan_mode_longgar_dilaporkan() {
        let f = fixture();
        let snap = f.exec.snapshot_dir("repo1", "s4").await.unwrap();
        let mode = std::fs::metadata(&snap).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        std::fs::write(snap.join("dump.sql"), b"x").unwrap();
        std::fs::set_permissions(
            snap.join("dump.sql"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert_eq!(f.exec.tighten_snapshot(&snap).await.unwrap(), ["dump.sql"]);
        let mode = std::fs::metadata(snap.join("dump.sql"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        f.exec.remove_snapshot(&snap).await;
        assert!(!snap.exists());
    }

    /// Database palsu: role lab berhak minimum, aktivitas bisa berisi role
    /// `kelola` milik seed yang memakai `.env` app.
    struct FakeDb {
        foreign: bool,
    }

    impl mandor_usecase::run_ports::LabDatabase for FakeDb {
        async fn server_is_local(
            &self,
            _c: &DbCredentials,
        ) -> Result<bool, mandor_usecase::PortError> {
            Ok(true)
        }
        async fn reads_all_stats(
            &self,
            _c: &DbCredentials,
        ) -> Result<bool, mandor_usecase::PortError> {
            Ok(true)
        }
        async fn role_privileges(
            &self,
            _c: &DbCredentials,
            _role: &str,
        ) -> Result<mandor_domain::lab_data::RolePrivileges, mandor_usecase::PortError> {
            Ok(mandor_domain::lab_data::RolePrivileges::default())
        }
        async fn activity(
            &self,
            _c: &DbCredentials,
        ) -> Result<(Vec<mandor_domain::lab_data::PgActivity>, i32), mandor_usecase::PortError>
        {
            let row = |pid, user: &str| mandor_domain::lab_data::PgActivity {
                pid,
                datname: Some("kelola".to_owned()),
                backend_type: Some("client backend".to_owned()),
                usename: Some(user.to_owned()),
                ..mandor_domain::lab_data::PgActivity::default()
            };
            let mut rows = vec![row(1, "maint"), row(2, "lab_min")];
            if self.foreign {
                rows.push(row(3, "kelola"));
            }
            Ok((rows, 1))
        }
        async fn tables(
            &self,
            _c: &DbCredentials,
        ) -> Result<Vec<String>, mandor_usecase::PortError> {
            Ok(Vec::new())
        }
        async fn run_statement(
            &self,
            _c: &DbCredentials,
            _sql: &str,
        ) -> Result<mandor_usecase::run_ports::SqlResult, mandor_usecase::PortError> {
            Err(mandor_usecase::PortError::Rejected("not used".to_owned()))
        }
    }

    /// Satu penyimpanan dengan skrip snapshot/restore/sidik/perubahan uji
    /// yang hanya menyalin dan menimpa satu berkas.
    fn store_with(name: &str, kind: StoreKind) -> LabStore {
        LabStore {
            name: name.to_owned(),
            kind,
            snapshot: "snap.sh".to_owned(),
            restore: "restore.sh".to_owned(),
            fingerprint: "fp.sh".to_owned(),
            test_change: "change.sh".to_owned(),
        }
    }

    fn folder_lab(f: &Fixture, restore_body: &str) -> (mandor_domain::lab_data::LabData, PathBuf) {
        let data_dir = f.worktree.parent().unwrap_or(&f.worktree).join("uploads");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("a.txt"), b"awal").unwrap();
        let scripts = f.worktree.join("lab");
        script(
            &scripts,
            "fp.sh",
            "sha256sum < \"$MANDOR_STORE_PATH/a.txt\"",
        );
        script(
            &scripts,
            "snap.sh",
            "cp \"$MANDOR_STORE_PATH/a.txt\" \"$MANDOR_SNAPSHOT_DIR/a.txt\"",
        );
        script(
            &scripts,
            "change.sh",
            "echo ubah >> \"$MANDOR_STORE_PATH/a.txt\"",
        );
        script(&scripts, "restore.sh", restore_body);
        script(&scripts, "seed.sh", "echo seeded");
        let data = mandor_domain::lab_data::LabData {
            scripts_dir: "lab".to_owned(),
            stores: vec![store_with(
                "uploads",
                StoreKind::Folder(data_dir.display().to_string()),
            )],
            seeds: vec![LabSeed {
                name: "kb".to_owned(),
                command: "seed.sh".to_owned(),
            }],
        };
        (data, data_dir)
    }

    const GOOD_RESTORE: &str = "cp \"$MANDOR_SNAPSHOT_DIR/a.txt\" \"$MANDOR_STORE_PATH/a.txt\"";

    async fn round_trip(
        f: &Fixture,
        data: &mandor_domain::lab_data::LabData,
        db: &FakeDb,
        creds: &std::collections::BTreeMap<String, StoreCreds>,
    ) -> Result<(), String> {
        let pinned = f
            .exec
            .pin_scripts("repo1", &f.worktree, "lab")
            .await
            .unwrap();
        let snap = f.exec.snapshot_dir("repo1", "approve").await.unwrap();
        f.exec
            .approval_round_trip(db, data, &pinned, creds, &f.worktree, &snap)
            .await
    }

    #[tokio::test]
    async fn uji_bolak_balik_lulus_dan_data_kembali() {
        let f = fixture();
        let (data, dir) = folder_lab(&f, GOOD_RESTORE);
        let r = round_trip(
            &f,
            &data,
            &FakeDb { foreign: false },
            &std::collections::BTreeMap::default(),
        )
        .await;
        assert_eq!(r, Ok(()));
        assert_eq!(std::fs::read(dir.join("a.txt")).unwrap(), b"awal");
    }

    /// Restore palsu yang tidak memulihkan penyimpanan → persetujuan ditolak.
    #[tokio::test]
    async fn restore_yang_tidak_memulihkan_ditolak() {
        let f = fixture();
        let (data, _) = folder_lab(&f, "true");
        let err = round_trip(
            &f,
            &data,
            &FakeDb { foreign: false },
            &std::collections::BTreeMap::default(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("did not bring its fingerprint back"), "{err}");
    }

    #[tokio::test]
    async fn sidik_yang_tidak_berubah_ditolak() {
        let f = fixture();
        let (mut data, _) = folder_lab(&f, GOOD_RESTORE);
        data.stores[0].test_change = "seed.sh".to_owned();
        let err = round_trip(
            &f,
            &data,
            &FakeDb { foreign: false },
            &std::collections::BTreeMap::default(),
        )
        .await
        .unwrap_err();
        assert!(err.contains("did not change its fingerprint"), "{err}");
    }

    /// Penyimpanan `postgres` tanpa kredensial ditolak; seed yang tersambung
    /// dengan role selain role lab membuat persetujuan ditolak.
    #[tokio::test]
    async fn seed_dengan_role_lain_ditolak() {
        let f = fixture();
        let (mut data, dir) = folder_lab(&f, GOOD_RESTORE);
        script(&f.worktree.join("lab"), "seed.sh", "sleep 1; echo seeded");
        // Skrip `postgres` palsu memakai berkas folder yang sama; yang diuji
        // adalah pemeriksaan kredensial dan pemantauan role selama seed.
        let pg = store_with(
            "tenant",
            StoreKind::Postgres(DbTarget {
                host: "localhost".to_owned(),
                port: 5432,
                database: "kelola".to_owned(),
            }),
        );
        data.stores = vec![pg];
        let none = round_trip(
            &f,
            &data,
            &FakeDb { foreign: true },
            &std::collections::BTreeMap::default(),
        )
        .await
        .unwrap_err();
        assert!(
            none.contains("enter the database roles of tenant"),
            "{none}"
        );
        let mut by_store = std::collections::BTreeMap::new();
        by_store.insert(
            "tenant".to_owned(),
            StoreCreds {
                lab: creds("lab_min", "pw-uji"),
                maintenance: creds("maint", "pw-uji"),
            },
        );
        // Kredensial pemeliharaan menunjuk berkas penyimpanan lewat env folder
        // tidak ada untuk `postgres`; sediakan lewat skrip yang membaca path
        // tetap.
        let path = dir.join("a.txt").display().to_string();
        let scripts = f.worktree.join("lab");
        script(&scripts, "fp.sh", &format!("sha256sum < {path}"));
        script(
            &scripts,
            "snap.sh",
            &format!("cp {path} \"$MANDOR_SNAPSHOT_DIR/a.txt\""),
        );
        script(&scripts, "change.sh", &format!("echo ubah >> {path}"));
        script(
            &scripts,
            "restore.sh",
            &format!("cp \"$MANDOR_SNAPSHOT_DIR/a.txt\" {path}"),
        );
        let err = round_trip(&f, &data, &FakeDb { foreign: true }, &by_store)
            .await
            .unwrap_err();
        assert!(
            err.contains("seed kb connected to the database as kelola"),
            "{err}"
        );
        assert_eq!(
            round_trip(&f, &data, &FakeDb { foreign: false }, &by_store).await,
            Ok(())
        );
    }
}
