//! Berkas state pagar sesi reviewer (KTD-31): origin yang diizinkan dan tab
//! milik reviewer. Ditulis daemon saat review dimulai, lalu dibaca dan
//! diperbarui hook di bawah lock. Berkas berada di `~/.mandor/agent/review/`,
//! yang ditolak bagi agent, dengan izin 0600.
//!
//! Hook berjalan sebagai proses sinkron tanpa runtime async, jadi I/O di sini
//! memakai `std`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mandor_domain::hook_review::{ReviewState, TabState};
use serde::{Deserialize, Serialize};

/// Batas waktu menunggu lock state.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(20);

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
    }
}

fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    path.with_file_name(name)
}

fn open_private(path: &Path, truncate: bool) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(truncate);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Menulis state secara atomik (berkas sementara lalu rename).
///
/// # Errors
/// Pesan bila berkas tidak bisa ditulis.
pub fn save(path: &Path, state: &ReviewState) -> Result<(), String> {
    let body = serde_json::to_vec(&from_domain(state)).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    let mut f = open_private(&tmp, true).map_err(|e| format!("cannot write review state: {e}"))?;
    f.write_all(&body)
        .and_then(|()| f.sync_all())
        .map_err(|e| format!("cannot write review state: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot write review state: {e}"))
}

fn load(path: &Path) -> Result<ReviewState, String> {
    let raw = std::fs::read(path).map_err(|e| format!("review state unavailable: {e}"))?;
    serde_json::from_slice::<StoredState>(&raw)
        .map(to_domain)
        .map_err(|e| format!("review state is corrupt: {e}"))
}

/// Membaca state di bawah lock eksklusif, menjalankan `f`, lalu menyimpan
/// state bila `f` mengubahnya. Berkas hilang, rusak, atau lock yang tidak
/// didapat dalam batas waktu menghasilkan error (hook menolak).
///
/// # Errors
/// Pesan alasan kegagalan.
pub fn with_locked<T>(path: &Path, f: impl FnOnce(&mut ReviewState) -> T) -> Result<T, String> {
    let file = open_private(&lock_path(path), false)
        .map_err(|e| format!("cannot open the review lock: {e}"))?;
    let mut lock = fd_lock::RwLock::new(file);
    let deadline = Instant::now() + LOCK_TIMEOUT;
    let _guard = loop {
        match lock.try_write() {
            Ok(guard) => break guard,
            Err(_) if Instant::now() < deadline => std::thread::sleep(LOCK_POLL),
            Err(_) => return Err("review state is locked".to_owned()),
        }
    };
    let mut state = load(path)?;
    let before = state.clone();
    let out = f(&mut state);
    if state != before {
        save(path, &state)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_bolak_balik_dengan_izin_0600_dan_berkas_hilang_menjadi_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("r1.json");
        assert!(with_locked(&path, |_| ()).is_err());
        let mut s = ReviewState {
            origins: vec!["http://localhost:5173".to_owned()],
            ..ReviewState::default()
        };
        save(&path, &s).unwrap();
        with_locked(&path, |st| {
            st.tabs.insert(
                7,
                TabState {
                    url: Some("http://localhost:5173/".to_owned()),
                    settled: true,
                },
            );
        })
        .unwrap();
        s.tabs.insert(
            7,
            TabState {
                url: Some("http://localhost:5173/".to_owned()),
                settled: true,
            },
        );
        assert_eq!(load(&path).unwrap(), s);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&path, "{rusak").unwrap();
        assert!(with_locked(&path, |_| ()).is_err());
    }
}
