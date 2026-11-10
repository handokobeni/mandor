//! Berkas izin satu giliran (KTD-A4 plan 2026-11-06-001): ditulis adapter
//! agent, diverifikasi hook lewat digest SHA-256 di argv `--settings`, dan
//! dibaca daemon untuk pemeriksaan ulang. Juga catatan bukti pemeriksaan hook
//! untuk permintaan tanpa keputusan (KTD-A9).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use mandor_domain::permit::{BashAllowlist, PathFacts, Scope, TurnKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Versi format berkas izin.
pub const VERSION: u32 = 1;
/// Batas ukuran berkas izin dan catatan bukti.
const MAX_BYTES: u64 = 1024 * 1024;
/// Batas panjang `tool_use_id`.
const ID_MAX: usize = 128;

/// Isi berkas izin; tanpa rahasia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermitFile {
    pub version: u32,
    /// `implement`, `plan`, atau `review`.
    pub turn: String,
    /// Allowlist giliran apa adanya (pola non-Bash diabaikan pencocok).
    pub allow: Vec<String>,
    /// Server MCP yang diizinkan.
    pub mcp: Vec<String>,
    /// Akar sumber desain.
    pub design: Vec<String>,
    /// Akar run (folder kerja giliran).
    pub run_root: String,
    /// Folder worktree anggota run.
    pub members: Vec<String>,
    /// Folder catatan bukti; `None` bila giliran ini tidak punya kartu.
    pub evidence: Option<String>,
    /// Folder staging berkas uji giliran lab (KTD-L11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixtures: Option<String>,
}

impl PermitFile {
    /// Cakupan keputusan domain; `None` bila jenis giliran tidak dikenal.
    #[must_use]
    pub fn scope(&self, vm_run_dir: Option<String>, private: Vec<String>) -> Option<Scope> {
        Some(Scope {
            turn: TurnKind::parse(&self.turn)?,
            allowlist: BashAllowlist::new(&self.allow),
            mcp: self.mcp.clone(),
            design_roots: self.design.clone(),
            run_root: self.run_root.clone(),
            members: self.members.clone(),
            vm_run_dir,
            private,
            fixtures: self.fixtures.clone(),
        })
    }

    /// Isi berkas terserialisasi.
    ///
    /// # Errors
    /// Pesan bila serialisasi gagal.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec_pretty(self).map_err(|e| e.to_string())
    }
}

/// Digest SHA-256 heksadesimal.
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Memuat berkas izin dan memastikan digest-nya cocok. Tidak mengikuti
/// symlink.
///
/// # Errors
/// Pesan bila berkas hilang, terlalu besar, rusak, atau digest tidak cocok.
pub fn load(path: &Path, expected_sha: &str) -> Result<PermitFile, String> {
    let bytes = read_small(path).map_err(|e| format!("permit file unreadable: {e}"))?;
    if digest(&bytes) != expected_sha.to_ascii_lowercase() {
        return Err("permit file digest does not match".to_owned());
    }
    let file: PermitFile =
        serde_json::from_slice(&bytes).map_err(|e| format!("permit file malformed: {e}"))?;
    if file.version != VERSION {
        return Err(format!(
            "permit file version {} is not supported",
            file.version
        ));
    }
    Ok(file)
}

fn read_small(path: &Path) -> Result<Vec<u8>, String> {
    let file = crate::screens::open_no_follow(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".to_owned());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err("file too large".to_owned());
    }
    Ok(bytes)
}

/// Catatan bukti: hook memeriksa permintaan ini sampai selesai tanpa keputusan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub tool_use_id: String,
    pub cwd: String,
}

/// `tool_use_id` yang aman sebagai nama berkas.
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_MAX
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Menulis catatan bukti sebagai berkas baru (0600) tanpa mengikuti symlink.
///
/// # Errors
/// Pesan bila id tidak valid atau berkas tidak bisa dibuat (termasuk sudah ada).
pub fn write_evidence(dir: &Path, evidence: &Evidence) -> Result<(), String> {
    if !valid_id(&evidence.tool_use_id) {
        return Err("tool_use_id is not a plain id".to_owned());
    }
    let body = serde_json::to_vec(evidence).map_err(|e| e.to_string())?;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = opts
        .open(evidence_path(dir, &evidence.tool_use_id))
        .map_err(|e| e.to_string())?;
    file.write_all(&body).map_err(|e| e.to_string())
}

/// Catatan bukti untuk `tool_use_id`, bila ada dan utuh.
#[must_use]
pub fn read_evidence(dir: &Path, tool_use_id: &str) -> Option<Evidence> {
    if !valid_id(tool_use_id) {
        return None;
    }
    let bytes = read_small(&evidence_path(dir, tool_use_id)).ok()?;
    let evidence: Evidence = serde_json::from_slice(&bytes).ok()?;
    (evidence.tool_use_id == tool_use_id).then_some(evidence)
}

fn evidence_path(dir: &Path, tool_use_id: &str) -> PathBuf {
    dir.join(format!("{tool_use_id}.json"))
}

/// Fakta path dari filesystem lokal (sinkron; daemon memanggilnya lewat
/// `spawn_blocking`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FsFacts;

impl PathFacts for FsFacts {
    fn canonical(&self, path: &str) -> Option<String> {
        std::fs::canonicalize(path)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }

    fn is_symlink(&self, path: &str) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
    }

    fn repo_root(&self, dir: &str) -> Option<String> {
        Path::new(dir)
            .ancestors()
            .find(|d| std::fs::symlink_metadata(d.join(".git")).is_ok())
            .map(|d| d.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn sample() -> PermitFile {
        PermitFile {
            version: VERSION,
            turn: "implement".to_owned(),
            allow: vec!["Bash(git diff*)".to_owned()],
            mcp: vec![],
            design: vec![],
            run_root: "/w".to_owned(),
            members: vec!["/w".to_owned()],
            evidence: None,
            fixtures: None,
        }
    }

    #[test]
    fn berkas_izin_dimuat_hanya_bila_digest_cocok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.json");
        let bytes = sample().to_bytes().unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let sha = digest(&bytes);
        assert_eq!(load(&path, &sha).unwrap(), sample());
        assert!(load(&path, &digest(b"lain")).is_err());
        assert!(load(&dir.path().join("hilang.json"), &sha).is_err());
        std::fs::write(&path, b"{rusak").unwrap();
        assert!(load(&path, &digest(b"{rusak")).is_err());
        let scope = sample().scope(None, vec![]).unwrap();
        assert_eq!(scope.turn, TurnKind::Implement);
        let mut bad = sample();
        bad.turn = "lain".to_owned();
        assert!(bad.scope(None, vec![]).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn berkas_izin_symlink_ditolak() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("asli.json");
        let bytes = sample().to_bytes().unwrap();
        std::fs::write(&real, &bytes).unwrap();
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(load(&link, &digest(&bytes)).is_err());
    }

    #[test]
    fn catatan_bukti_baru_dan_terbaca() {
        let dir = tempfile::tempdir().unwrap();
        let ev = Evidence {
            tool_use_id: "toolu_01AB".to_owned(),
            cwd: "/w".to_owned(),
        };
        write_evidence(dir.path(), &ev).unwrap();
        assert_eq!(read_evidence(dir.path(), "toolu_01AB"), Some(ev.clone()));
        assert!(write_evidence(dir.path(), &ev).is_err(), "sudah ada");
        assert!(read_evidence(dir.path(), "toolu_lain").is_none());
        let bad = Evidence {
            tool_use_id: "../x".to_owned(),
            cwd: "/w".to_owned(),
        };
        assert!(write_evidence(dir.path(), &bad).is_err());
        assert!(read_evidence(dir.path(), "../x").is_none());
    }

    #[test]
    fn fakta_path_dari_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("repo/.git")).unwrap();
        std::fs::create_dir_all(root.join("repo/src")).unwrap();
        let facts = FsFacts;
        let src = root.join("repo/src").to_string_lossy().into_owned();
        assert_eq!(facts.canonical(&src).as_deref(), Some(src.as_str()));
        assert!(facts.canonical(&format!("{src}/hilang")).is_none());
        assert_eq!(
            facts.repo_root(&src),
            Some(root.join("repo").to_string_lossy().into_owned())
        );
    }
}
