//! Berkas state pagar sesi reviewer (KTD-31): origin yang diizinkan dan tab
//! milik reviewer, 0600, di direktori daemon yang ditolak bagi agent. Adapter
//! agent menulis state awal; hook membaca dan memperbaruinya di bawah lock.
//!
//! Hook berjalan sebagai proses sinkron tanpa runtime async, jadi I/O di sini
//! memakai `std`; pemanggil async memakai `spawn_blocking`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Menghapus berkas state dan lock-nya (review selesai).
pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(lock_path(path));
}

use mandor_domain::hook_review::{ReviewState, TabState};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct StoredTab {
    url: Option<String>,
    settled: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredState {
    origins: Vec<String>,
    #[serde(default)]
    tabs: BTreeMap<u64, StoredTab>,
    /// Halaman Playwright reviewer VM.
    #[serde(default)]
    page: Option<StoredTab>,
}

fn to_domain(s: StoredState) -> ReviewState {
    ReviewState {
        origins: s.origins,
        tabs: s
            .tabs
            .into_iter()
            .map(|(id, t)| {
                (
                    id,
                    TabState {
                        url: t.url,
                        settled: t.settled,
                    },
                )
            })
            .collect(),
        page: s.page.map(|t| TabState {
            url: t.url,
            settled: t.settled,
        }),
    }
}

fn from_domain(s: &ReviewState) -> StoredState {
    StoredState {
        origins: s.origins.clone(),
        tabs: s
            .tabs
            .iter()
            .map(|(id, t)| {
                (
                    *id,
                    StoredTab {
                        url: t.url.clone(),
                        settled: t.settled,
                    },
                )
            })
            .collect(),
        page: s.page.as_ref().map(|t| StoredTab {
            url: t.url.clone(),
            settled: t.settled,
        }),
    }
}

/// Berkas lock di samping berkas state (`<state>.lock`).
#[must_use]
pub fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    path.with_file_name(name)
}

/// Menulis `body` ke `path` secara atomik: berkas sementara bernama acak
/// dibuat baru (`O_EXCL`, tidak pernah mengikuti symlink yang sudah ada),
/// modenya disetel lewat fd, lalu di-rename ke `path`. Aman di direktori yang
/// juga bisa ditulis user agent (KTD-62 plan VM): symlink di `path` hanya
/// tergantikan, targetnya tidak tersentuh.
///
/// # Errors
/// Kegagalan I/O.
#[cfg_attr(not(unix), allow(unused_variables))]
pub fn write_atomic(path: &Path, body: &[u8], mode: u32) -> std::io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", crate::prompt::new_nonce()));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    let written = (|| {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Mode `open` dipotong umask; disetel lewat fd berkas baru ini.
            f.set_permissions(std::fs::Permissions::from_mode(mode))?;
        }
        f.write_all(body)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Menulis state secara atomik (berkas sementara lalu rename), 0600.
///
/// # Errors
/// Pesan bila berkas tidak bisa ditulis.
pub fn save(path: &Path, state: &ReviewState) -> Result<(), String> {
    save_with_mode(path, state, 0o600)
}

/// Seperti [`save`] dengan mode berkas tertentu; di VM 0660 agar hook yang
/// berjalan sebagai user agent bisa memperbaruinya (KTD-62 plan VM).
///
/// # Errors
/// Pesan bila berkas tidak bisa ditulis.
pub fn save_with_mode(path: &Path, state: &ReviewState, mode: u32) -> Result<(), String> {
    let body = serde_json::to_vec(&from_domain(state)).map_err(|e| e.to_string())?;
    write_atomic(path, &body, mode).map_err(|e| format!("cannot write review state: {e}"))
}

/// Membaca state.
///
/// # Errors
/// Pesan bila berkas tidak ada atau rusak.
pub fn load(path: &Path) -> Result<ReviewState, String> {
    let raw = std::fs::read(path).map_err(|e| format!("review state unavailable: {e}"))?;
    serde_json::from_slice::<StoredState>(&raw)
        .map(to_domain)
        .map_err(|e| format!("review state is corrupt: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_bolak_balik_dengan_izin_0600_lalu_dihapus() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("r1.json");
        assert!(load(&path).is_err());
        let s = ReviewState {
            origins: vec!["http://localhost:5173".to_owned()],
            ..ReviewState::default()
        };
        save(&path, &s).unwrap();
        assert_eq!(load(&path).unwrap(), s);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&path, "{rusak").unwrap();
        assert!(load(&path).is_err());
        remove(&path);
        assert!(!path.exists());
    }

    /// Symlink yang ditanam di direktori bersama tidak pernah diikuti.
    #[cfg(unix)]
    #[test]
    fn symlink_di_jalur_state_diganti_tanpa_menyentuh_targetnya() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rahasia");
        std::fs::write(&target, "milik daemon").unwrap();
        let path = dir.path().join("r1.json");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        save_with_mode(&path, &ReviewState::default(), 0o660).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "milik daemon");
        assert!(
            !std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 2, "tanpa sisa berkas sementara");
    }
}
