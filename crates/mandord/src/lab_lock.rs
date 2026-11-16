//! Kunci berkas per penyimpanan lab data (plan 2026-11-10-001, KTD-L21).
//!
//! Satu berkas kunci per penyimpanan di `lab_lock_dir` (di VM folder 1777
//! bersama antar user). Berkas dibuka dengan `O_NOFOLLOW`, dibuat 0666 bila
//! belum ada. Setiap kunci yang dipegang ditahan oleh satu thread OS yang
//! memegang guard `fd-lock` sampai diberi sinyal lepas, sehingga kunci
//! bertahan lintas `await` tanpa `unsafe`. Penanda `<kunci>.dirty` menyimpan
//! uid, sesi, dan fase agar daemon lain yang menunggu langsung tahu bila
//! pemegang gagal restore.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use mandor_domain::lab_data::{SessionPhase, lock_file_name};
use mandor_usecase::run_ports::LockOutcome;
use serde::{Deserialize, Serialize};

/// Jeda poll pengambilan kunci (KTD-L21).
const POLL: Duration = Duration::from_secs(2);

/// Penanda fase sesi pemegang kunci.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub uid: u32,
    pub session: String,
    pub phase: String,
}

/// Satu kunci yang dipegang.
struct Held {
    release: mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}

/// Kunci penyimpanan milik daemon ini.
pub struct LabLocks {
    dir: PathBuf,
    uid: u32,
    poll: Duration,
    held: Mutex<HashMap<String, Held>>,
}

impl std::fmt::Debug for LabLocks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabLocks")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl LabLocks {
    #[must_use]
    pub fn new(dir: PathBuf, uid: u32) -> Self {
        Self {
            dir,
            uid,
            poll: POLL,
            held: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn with_poll(mut self, poll: Duration) -> Self {
        self.poll = poll;
        self
    }

    fn lock_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.lock", lock_file_name(key)))
    }

    fn marker_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.dirty", lock_file_name(key)))
    }

    /// Mengambil semua kunci berurutan; menunggu sampai `wait`. Kunci yang
    /// sudah diambil dilepas lagi bila gagal.
    pub async fn acquire(&self, keys: &[String], wait: Duration) -> LockOutcome {
        let mut keys = keys.to_vec();
        keys.sort();
        keys.dedup();
        let start = Instant::now();
        let mut got: Vec<String> = Vec::new();
        for key in &keys {
            loop {
                if let Some(m) = self.foreign_marker(key) {
                    if m.phase == SessionPhase::RestoreFailed.as_str() {
                        self.release(&got).await;
                        return LockOutcome::RestoreFailed(format!(
                            "another lab could not restore this local data (session {})",
                            m.session
                        ));
                    }
                }
                match self.try_hold(key).await {
                    Ok(Some(held)) => {
                        // Penanda milik orang lain: penyimpanan masih kotor.
                        if self.foreign_marker(key).is_some() {
                            let _ = held.release.send(());
                            let _ = tokio::task::spawn_blocking(move || held.thread.join()).await;
                        } else {
                            self.held
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner)
                                .insert(key.clone(), held);
                            got.push(key.clone());
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(key, error = %e, "berkas kunci lab tidak bisa dibuka");
                        self.release(&got).await;
                        return LockOutcome::Locked;
                    }
                }
                if start.elapsed() >= wait {
                    self.release(&got).await;
                    return LockOutcome::Locked;
                }
                tokio::time::sleep(self.poll).await;
            }
        }
        LockOutcome::Acquired
    }

    /// Mencoba memegang satu kunci; `None` bila dipegang orang lain.
    async fn try_hold(&self, key: &str) -> std::io::Result<Option<Held>> {
        let path = self.lock_path(key);
        let dir = self.dir.clone();
        let file = tokio::task::spawn_blocking(move || open_lock(&dir, &path))
            .await
            .map_err(std::io::Error::other)??;
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<bool>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("mandor-lab-lock".to_owned())
            .spawn(move || {
                let mut lock = fd_lock::RwLock::new(file);
                match lock.try_write() {
                    Ok(_guard) => {
                        let _ = ready_tx.send(true);
                        // Guard dipegang sampai sinyal lepas atau pemiliknya
                        // hilang (daemon berhenti).
                        let _ = release_rx.recv();
                    }
                    Err(_) => {
                        let _ = ready_tx.send(false);
                    }
                }
            })?;
        let acquired = ready_rx.await.unwrap_or(false);
        let held = Held {
            release: release_tx,
            thread,
        };
        if acquired {
            Ok(Some(held))
        } else {
            let _ = tokio::task::spawn_blocking(move || held.thread.join()).await;
            Ok(None)
        }
    }

    /// Menulis penanda fase pada setiap kunci yang dipegang.
    pub async fn mark(&self, keys: &[String], session: &str, phase: SessionPhase) {
        let marker = Marker {
            uid: self.uid,
            session: session.to_owned(),
            phase: phase.as_str().to_owned(),
        };
        let paths: Vec<PathBuf> = keys.iter().map(|k| self.marker_path(k)).collect();
        let written = tokio::task::spawn_blocking(move || {
            for p in &paths {
                write_marker(p, &marker)?;
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        if !matches!(written, Ok(Ok(()))) {
            tracing::warn!(session, "penanda kunci lab tidak tertulis");
        }
    }

    /// Menghapus penanda milik daemon ini lalu melepas kunci.
    pub async fn release(&self, keys: &[String]) {
        for key in keys {
            if self.own_marker(key) {
                let _ = tokio::fs::remove_file(self.marker_path(key)).await;
            }
            let held = self
                .held
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(key);
            if let Some(h) = held {
                let _ = h.release.send(());
                let _ = tokio::task::spawn_blocking(move || h.thread.join()).await;
            }
        }
    }

    /// Penanda milik daemon ini yang tidak punya sesi aktif dihapus saat
    /// start (KTD-L21).
    pub async fn clear_stale(&self, active_keys: &[String]) {
        let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await else {
            return;
        };
        let active: Vec<PathBuf> = active_keys.iter().map(|k| self.marker_path(k)).collect();
        while let Ok(Some(e)) = entries.next_entry().await {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("dirty") || active.contains(&path)
            {
                continue;
            }
            if read_marker(&path).is_some_and(|(owner, _)| owner == self.uid) {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
    }

    /// Penanda milik user lain, ditentukan dari pemilik berkas (bukan isi
    /// JSON). Penanda yang tidak terbaca dianggap kotor (gagal tertutup).
    fn foreign_marker(&self, key: &str) -> Option<Marker> {
        let (owner, marker) = read_marker(&self.marker_path(key))?;
        if owner == self.uid && marker.is_some() {
            return None;
        }
        Some(marker.unwrap_or_else(|| Marker {
            uid: owner,
            session: "unknown".to_owned(),
            phase: "unknown".to_owned(),
        }))
    }

    fn own_marker(&self, key: &str) -> bool {
        read_marker(&self.marker_path(key)).is_some_and(|(owner, _)| owner == self.uid)
    }
}

/// Membuka berkas kunci tanpa `O_CREAT` lebih dulu (aman terhadap
/// `fs.protected_regular`), lalu membuatnya 0666 dengan `create_new` bila
/// belum ada; symlink ditolak lewat `O_NOFOLLOW`.
fn open_lock(dir: &Path, path: &Path) -> std::io::Result<File> {
    std::fs::create_dir_all(dir)?;
    match no_follow(OpenOptions::new().read(true).write(true)).open(path) {
        Ok(f) => checked(f),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match no_follow(OpenOptions::new().read(true).write(true).create_new(true)).open(path) {
                Ok(f) => {
                    set_shared(&f)?;
                    Ok(f)
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    checked(no_follow(OpenOptions::new().read(true).write(true)).open(path)?)
                }
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Berkas kunci harus berkas biasa.
fn checked(f: File) -> std::io::Result<File> {
    if f.metadata()?.is_file() {
        Ok(f)
    } else {
        Err(std::io::Error::other("the lock path is not a regular file"))
    }
}

#[cfg(unix)]
fn no_follow(opts: &mut OpenOptions) -> &mut OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    opts.custom_flags(libc::O_NOFOLLOW).mode(0o666)
}

#[cfg(not(unix))]
fn no_follow(opts: &mut OpenOptions) -> &mut OpenOptions {
    opts
}

#[cfg(unix)]
fn set_shared(f: &File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    f.set_permissions(std::fs::Permissions::from_mode(0o666))
}

#[cfg(not(unix))]
fn set_shared(_f: &File) -> std::io::Result<()> {
    Ok(())
}

/// Penanda ditulis 0644 milik penulisnya; user lain hanya bisa membacanya.
fn write_marker(path: &Path, m: &Marker) -> std::io::Result<()> {
    let body = serde_json::to_vec(m).map_err(std::io::Error::other)?;
    let mut f = no_follow(OpenOptions::new().write(true).create(true).truncate(true)).open(path)?;
    set_readable(&f)?;
    f.write_all(&body)
}

/// Pemilik berkas penanda dan isinya (`None` bila tidak bisa diurai).
fn read_marker(path: &Path) -> Option<(u32, Option<Marker>)> {
    let mut f = no_follow(OpenOptions::new().read(true)).open(path).ok()?;
    let owner = file_owner(&f);
    let mut body = Vec::new();
    let parsed = Read::by_ref(&mut f)
        .take(4096)
        .read_to_end(&mut body)
        .ok()
        .and_then(|_| serde_json::from_slice(&body).ok());
    Some((owner, parsed))
}

#[cfg(unix)]
fn file_owner(f: &File) -> u32 {
    use std::os::unix::fs::MetadataExt;
    f.metadata().map_or(u32::MAX, |m| m.uid())
}

#[cfg(not(unix))]
fn file_owner(_f: &File) -> u32 {
    0
}

#[cfg(unix)]
fn set_readable(f: &File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    f.set_permissions(std::fs::Permissions::from_mode(0o644))
}

#[cfg(not(unix))]
fn set_readable(_f: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn locks(dir: &Path, uid: u32) -> LabLocks {
        LabLocks::new(dir.to_path_buf(), uid).with_poll(Duration::from_millis(20))
    }

    /// uid user test (pemilik berkas yang ditulis test).
    fn me_uid(dir: &Path) -> u32 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(dir).unwrap().uid()
    }

    #[tokio::test]
    async fn kunci_eksklusif_dan_dilepas() {
        let dir = tempfile::tempdir().unwrap();
        let a = locks(dir.path(), me_uid(dir.path()));
        let b = locks(dir.path(), me_uid(dir.path()));
        let keys = vec!["pg:local:5432/kelola".to_owned(), "dir:/srv/up".to_owned()];
        assert_eq!(
            a.acquire(&keys, Duration::from_secs(1)).await,
            LockOutcome::Acquired
        );
        assert_eq!(
            b.acquire(&keys, Duration::from_millis(100)).await,
            LockOutcome::Locked
        );
        a.release(&keys).await;
        assert_eq!(
            b.acquire(&keys, Duration::from_secs(1)).await,
            LockOutcome::Acquired
        );
        b.release(&keys).await;
    }

    /// Penanda `restore_failed` milik user lain langsung ditolak; penanda
    /// lain membuat kunci tetap dianggap dipegang.
    #[tokio::test]
    async fn penanda_user_lain() {
        let dir = tempfile::tempdir().unwrap();
        let keys = vec!["pg:local:5432/kelola".to_owned()];
        let real = me_uid(dir.path());
        let other = locks(dir.path(), real);
        assert_eq!(
            other.acquire(&keys, Duration::from_secs(1)).await,
            LockOutcome::Acquired
        );
        other
            .mark(&keys, "s-lain", SessionPhase::RestoreFailed)
            .await;
        other.release(&[]).await;
        // Pemegang mati (kunci lepas) tetapi penanda tetap ada.
        drop(other);
        // Daemon user lain (uid berbeda): penanda dinilai dari pemilik
        // berkas, bukan dari isi JSON.
        let me = locks(dir.path(), real + 1);
        assert!(matches!(
            me.acquire(&keys, Duration::from_millis(200)).await,
            LockOutcome::RestoreFailed(m) if m.contains("s-lain")
        ));
        write_marker(
            &me.marker_path(&keys[0]),
            &Marker {
                uid: real + 1,
                session: "s-lain".to_owned(),
                phase: "running".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(
            me.acquire(&keys, Duration::from_millis(100)).await,
            LockOutcome::Locked
        );
        // Penanda rusak dianggap kotor (gagal tertutup).
        std::fs::write(me.marker_path(&keys[0]), b"bukan json").unwrap();
        assert_eq!(
            me.acquire(&keys, Duration::from_millis(100)).await,
            LockOutcome::Locked
        );
    }

    #[tokio::test]
    async fn penanda_sendiri_dihapus_saat_lepas_dan_start() {
        let dir = tempfile::tempdir().unwrap();
        let keys = vec!["pg:local:5432/kelola".to_owned()];
        let me = locks(dir.path(), me_uid(dir.path()));
        assert_eq!(
            me.acquire(&keys, Duration::from_secs(1)).await,
            LockOutcome::Acquired
        );
        me.mark(&keys, "s1", SessionPhase::Running).await;
        assert!(me.marker_path(&keys[0]).exists());
        me.release(&keys).await;
        assert!(!me.marker_path(&keys[0]).exists());
        // Penanda yatim milik sendiri dihapus saat start.
        write_marker(
            &me.marker_path("dir:/x"),
            &Marker {
                uid: 0,
                session: "lama".to_owned(),
                phase: "running".to_owned(),
            },
        )
        .unwrap();
        me.clear_stale(&[]).await;
        assert!(!me.marker_path("dir:/x").exists());
    }

    #[tokio::test]
    async fn symlink_di_folder_kunci_ditolak() {
        let dir = tempfile::tempdir().unwrap();
        let keys = vec!["pg:local:5432/kelola".to_owned()];
        let me = locks(dir.path(), me_uid(dir.path()));
        std::os::unix::fs::symlink("/etc/passwd", me.lock_path(&keys[0])).unwrap();
        assert_eq!(
            me.acquire(&keys, Duration::from_millis(100)).await,
            LockOutcome::Locked
        );
    }

    #[tokio::test]
    async fn berkas_kunci_dibuat_0666() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let keys = vec!["dir:/srv/up".to_owned()];
        let me = locks(dir.path(), me_uid(dir.path()));
        assert_eq!(
            me.acquire(&keys, Duration::from_secs(1)).await,
            LockOutcome::Acquired
        );
        let mode = std::fs::metadata(me.lock_path(&keys[0]))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o666);
        me.release(&keys).await;
    }
}
