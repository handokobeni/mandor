//! Lock berkas state pagar reviewer untuk hook (KTD-31). Format dan I/O
//! berkasnya ada di `mandor_agent::claude::review_state`.

use std::path::Path;
use std::time::{Duration, Instant};

use mandor_agent::claude::review_state::{load, lock_path, save};
use mandor_domain::hook_review::ReviewState;

/// Batas waktu menunggu lock state.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_POLL: Duration = Duration::from_millis(20);

fn open_lock(path: &Path) -> std::io::Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).write(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(lock_path(path))
}

/// Membaca state di bawah lock eksklusif, menjalankan `f`, lalu menyimpan
/// state bila `f` mengubahnya. Berkas hilang, rusak, atau lock yang tidak
/// didapat dalam batas waktu menghasilkan error (hook menolak).
///
/// # Errors
/// Pesan alasan kegagalan.
pub fn with_locked<T>(path: &Path, f: impl FnOnce(&mut ReviewState) -> T) -> Result<T, String> {
    let file = open_lock(path).map_err(|e| format!("cannot open the review lock: {e}"))?;
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
    use mandor_domain::hook_review::TabState;

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
