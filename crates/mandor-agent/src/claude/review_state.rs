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

#[cfg_attr(not(unix), allow(unused_variables))]
fn open_with_mode(path: &Path, mode: u32) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        opts.mode(mode);
        let f = opts.open(path)?;
        // Mode `open` dipotong umask; disetel ulang agar grup benar-benar
        // mendapat akses yang diminta.
        f.set_permissions(std::fs::Permissions::from_mode(mode))?;
        Ok(f)
    }
    #[cfg(not(unix))]
    opts.open(path)
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
    let tmp = path.with_extension("tmp");
    let mut f =
        open_with_mode(&tmp, mode).map_err(|e| format!("cannot write review state: {e}"))?;
    f.write_all(&body)
        .and_then(|()| f.sync_all())
        .map_err(|e| format!("cannot write review state: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot write review state: {e}"))
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
}
