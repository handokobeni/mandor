//! Link kedua ke daemon VM lewat SSH (KTD-67 plan VM): host VM tersimpan di
//! direktori konfigurasi app, host key dipin di `known_hosts` milik app setelah
//! developer mengonfirmasi fingerprint-nya.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::daemon_link::{DaemonLink, LinkError, LinkMode, SshVia};

/// Batas waktu satu perintah ssh lokal (`ssh -G`, `ssh-keyscan`, `ssh-keygen`).
const SSH_TIMEOUT: Duration = Duration::from_secs(20);

/// Kegagalan menyiapkan link VM; dikirim ke webview sebagai teks.
#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error(transparent)]
    Link(#[from] LinkError),
    #[error("{0} failed: {1}")]
    Command(&'static str, String),
    #[error("no host key found for {0}; check the host name and that the VM is reachable")]
    NoKeys(String),
    #[error(
        "can't reach {host} over SSH ({reason}); check the alias in ~/.ssh/config and that `ssh {host}` works from this machine"
    )]
    Unreachable { host: String, reason: String },
    #[error("check the host key of {0} first")]
    NotScanned(String),
    #[error("saving the VM setting failed: {0}")]
    Store(String),
}

impl Serialize for VmError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// Host key VM yang menunggu konfirmasi developer.
#[derive(Debug, Clone, Serialize)]
pub struct HostScan {
    pub host: String,
    /// Baris `ssh-keygen -l`, mis. `256 SHA256:abc... vm (ED25519)`.
    pub fingerprints: Vec<String>,
    /// Baris `known_hosts` yang akan dipin; tidak dikirim ke webview.
    #[serde(skip)]
    keys: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    host: String,
}

/// Link laptop dan VM, diakses lewat kuncinya (`laptop`, `vm`).
#[derive(Debug)]
pub struct Links {
    pub laptop: Arc<DaemonLink>,
    vm: Mutex<Option<Arc<DaemonLink>>>,
    via: SshVia,
    pending: Mutex<Option<HostScan>>,
}

impl Links {
    #[must_use]
    pub fn new(laptop: Arc<DaemonLink>, via: SshVia) -> Self {
        Self {
            laptop,
            vm: Mutex::new(None),
            via,
            pending: Mutex::new(None),
        }
    }

    /// Link untuk `key`; `vm` hanya ada setelah VM ditambahkan.
    ///
    /// # Errors
    /// [`LinkError::NotConnected`] bila kunci tidak dikenal atau VM belum ada.
    pub async fn get(&self, key: &str) -> Result<Arc<DaemonLink>, LinkError> {
        match key {
            "laptop" => Ok(Arc::clone(&self.laptop)),
            "vm" => self.vm.lock().await.clone().ok_or(LinkError::NotConnected),
            _ => Err(LinkError::NotConnected),
        }
    }

    /// Memuat host VM tersimpan di `dir` (saat app mulai).
    pub async fn load(&self, dir: &Path) {
        if let Some(host) = read_host(dir).await {
            match LinkMode::ssh(&host, self.via.clone()) {
                Ok(mode) => *self.vm.lock().await = Some(Arc::new(DaemonLink::new("vm", mode))),
                Err(e) => tracing::warn!(error = %e, "host VM tersimpan tidak valid"),
            }
        }
    }

    /// Host VM tersimpan.
    pub async fn host(&self, dir: &Path) -> Option<String> {
        read_host(dir).await
    }

    /// Membaca host key VM dan fingerprint-nya untuk dikonfirmasi developer.
    ///
    /// # Errors
    /// [`VmError`] bila host tidak valid, tidak terjangkau, atau tanpa kunci.
    pub async fn scan(&self, host: &str) -> Result<HostScan, VmError> {
        LinkMode::ssh(host, self.via.clone())?;
        let keys = match self
            .run(
                "read host key",
                &["sh", "-c", SCAN_SCRIPT, "sh", host],
                None,
            )
            .await
        {
            Ok(out) => key_lines(&out),
            Err(VmError::Command(_, reason)) => {
                return Err(VmError::Unreachable {
                    host: host.to_owned(),
                    reason,
                });
            }
            Err(e) => return Err(e),
        };
        if keys.is_empty() {
            return Err(VmError::NoKeys(host.to_owned()));
        }
        let fp = self
            .run("ssh-keygen", &["ssh-keygen", "-l", "-f", "-"], Some(&keys))
            .await?;
        let scan = HostScan {
            host: host.to_owned(),
            fingerprints: fp.lines().map(str::to_owned).collect(),
            keys,
        };
        *self.pending.lock().await = Some(scan.clone());
        Ok(scan)
    }

    /// Mempin host key yang sudah dikonfirmasi (menggantikan pin lama),
    /// menyimpan host, dan menyiapkan link VM.
    ///
    /// # Errors
    /// [`VmError::NotScanned`] bila host ini belum di-scan.
    pub async fn trust(&self, host: &str, dir: &Path) -> Result<(), VmError> {
        let scan = self
            .pending
            .lock()
            .await
            .take()
            .filter(|s| s.host == host)
            .ok_or_else(|| VmError::NotScanned(host.to_owned()))?;
        // Sama dengan VM_KNOWN_HOSTS; known_hosts milik app hanya berisi VM ini.
        let script = "umask 077; f=\"$HOME/.config/mandor/vm_known_hosts\"; \
                      mkdir -p \"$(dirname \"$f\")\" && cat > \"$f\"";
        self.run("pin host key", &["sh", "-c", script], Some(&scan.keys))
            .await?;
        write_host(dir, host).await?;
        let mode = LinkMode::ssh(host, self.via.clone())?;
        if let Some(old) = self
            .vm
            .lock()
            .await
            .replace(Arc::new(DaemonLink::new("vm", mode)))
        {
            old.shutdown().await;
        }
        Ok(())
    }

    /// Melepas VM: link ditutup dan host dilupakan.
    ///
    /// # Errors
    /// [`VmError::Store`] bila berkas setelan tidak bisa dihapus.
    pub async fn remove(&self, dir: &Path) -> Result<(), VmError> {
        if let Some(old) = self.vm.lock().await.take() {
            old.shutdown().await;
        }
        match tokio::fs::remove_file(store_path(dir)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(VmError::Store(e.to_string())),
        }
    }

    /// Menutup semua link saat app keluar.
    pub async fn shutdown(&self) {
        self.laptop.shutdown().await;
        if let Some(vm) = self.vm.lock().await.take() {
            vm.shutdown().await;
        }
    }

    /// Menjalankan `argv` di tempat ssh berjalan (langsung, atau di WSL),
    /// tanpa shell lokal; stdout bila keluar 0.
    async fn run(
        &self,
        what: &'static str,
        argv: &[&str],
        stdin: Option<&str>,
    ) -> Result<String, VmError> {
        let (program, argument_list) = match &self.via {
            SshVia::Native => (
                argv[0].to_owned(),
                argv[1..].iter().map(|a| (*a).to_owned()).collect(),
            ),
            SshVia::Wsl { distro } => (
                "wsl.exe".to_owned(),
                crate::provision::wsl_args(distro.as_deref(), argv),
            ),
        };
        let mut cmd = Command::new(&program);
        cmd.args(&argument_list)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::daemon_link::hide_console(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| VmError::Command(what, e.to_string()))?;
        if let (Some(text), Some(mut input)) = (stdin, child.stdin.take()) {
            input
                .write_all(text.as_bytes())
                .await
                .map_err(|e| VmError::Command(what, e.to_string()))?;
        }
        let out = tokio::time::timeout(SSH_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| VmError::Command(what, "timed out".to_owned()))?
            .map_err(|e| VmError::Command(what, e.to_string()))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(VmError::Command(
                what,
                err.lines().last().unwrap_or_default().trim().to_owned(),
            ))
        }
    }
}

/// Membaca host key VM lewat `ssh` sendiri, sehingga `~/.ssh/config`
/// pengguna (alias, `ProxyCommand` seperti IAP Google Cloud, port) ikut
/// berlaku; `ssh-keyscan` hanya bisa menyambung langsung. Kunci baru dicatat
/// ke `known_hosts` sementara (`accept-new`) tanpa login
/// (`PreferredAuthentications=none`), lalu dicetak untuk dikonfirmasi
/// developer. `$1` adalah host, diteruskan sebagai argumen, bukan teks skrip.
/// Gagal: baris stderr terakhir `ssh` sebagai alasan.
pub(crate) const SCAN_SCRIPT: &str = r#"f=$(mktemp) || exit 1
ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="$f" \
  -o GlobalKnownHostsFile=/dev/null -o PreferredAuthentications=none -o ConnectTimeout=20 \
  -- "$1" true >/dev/null 2>"$f.err"
if [ -s "$f" ]; then cat "$f"; rm -f "$f" "$f.err"; exit 0; fi
grep -v '^ *$' "$f.err" | tail -n 1 >&2
rm -f "$f" "$f.err"
exit 1"#;

/// Baris kunci dari keluaran `ssh-keyscan` (komentar dibuang).
fn key_lines(out: &str) -> String {
    let mut keys = String::new();
    for l in out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        keys.push_str(l);
        keys.push('\n');
    }
    keys
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("vm.json")
}

async fn read_host(dir: &Path) -> Option<String> {
    let raw = tokio::fs::read(store_path(dir)).await.ok()?;
    serde_json::from_slice::<Stored>(&raw).ok().map(|s| s.host)
}

async fn write_host(dir: &Path, host: &str) -> Result<(), VmError> {
    let body = serde_json::to_vec(&Stored {
        host: host.to_owned(),
    })
    .map_err(|e| VmError::Store(e.to_string()))?;
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| VmError::Store(e.to_string()))?;
    tokio::fs::write(store_path(dir), body)
        .await
        .map_err(|e| VmError::Store(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baris_kunci_tanpa_komentar() {
        let out = "# vm:22 SSH-2.0-OpenSSH\nvm ssh-ed25519 AAAA\n\n[vm]:2222 ecdsa-sha2 BBBB\n";
        assert_eq!(
            key_lines(out),
            "vm ssh-ed25519 AAAA\n[vm]:2222 ecdsa-sha2 BBBB\n"
        );
    }

    #[tokio::test]
    async fn host_vm_tersimpan_lalu_dimuat_dan_dilepas() {
        let dir = tempfile::tempdir().unwrap();
        let laptop = Arc::new(DaemonLink::new(
            "laptop",
            LinkMode::Local {
                bin: "mandord".into(),
            },
        ));
        let links = Links::new(laptop, SshVia::Native);
        assert!(links.get("vm").await.is_err(), "belum ada VM");
        assert!(links.get("lain").await.is_err());
        assert!(links.get("laptop").await.is_ok());
        write_host(dir.path(), "beni@vm").await.unwrap();
        links.load(dir.path()).await;
        assert_eq!(links.host(dir.path()).await.as_deref(), Some("beni@vm"));
        let vm = links.get("vm").await.unwrap();
        assert_eq!(vm.events().0, "daemon://vm/line");
        links.remove(dir.path()).await.unwrap();
        assert!(links.get("vm").await.is_err());
        assert_eq!(links.host(dir.path()).await, None);
    }

    #[tokio::test]
    async fn pin_tanpa_scan_ditolak() {
        let dir = tempfile::tempdir().unwrap();
        let laptop = Arc::new(DaemonLink::new(
            "laptop",
            LinkMode::Local {
                bin: "mandord".into(),
            },
        ));
        let links = Links::new(laptop, SshVia::Native);
        assert!(matches!(
            links.trust("vm", dir.path()).await,
            Err(VmError::NotScanned(_))
        ));
        assert!(matches!(
            links.scan("-oProxyCommand=x").await,
            Err(VmError::Link(LinkError::BadHost(_)))
        ));
    }

    /// `ssh` palsu: menulis host key ke berkas `UserKnownHostsFile` yang
    /// diberikan (seperti `StrictHostKeyChecking=accept-new`), lalu gagal
    /// login seperti `PreferredAuthentications=none`.
    #[cfg(unix)]
    fn fake_ssh(dir: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let f = dir.join("ssh");
        std::fs::write(&f, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    fn run_scan(dir: &Path, host: &str) -> std::process::Output {
        let path = format!("{}:/usr/bin:/bin", dir.display());
        std::process::Command::new("sh")
            .args(["-c", SCAN_SCRIPT, "sh", host])
            .env("PATH", path)
            .output()
            .unwrap()
    }

    /// Host key dibaca lewat `ssh` dengan `~/.ssh/config` pengguna (mis.
    /// `ProxyCommand` IAP), bukan `ssh-keyscan` yang hanya bisa langsung.
    #[cfg(unix)]
    #[test]
    fn scan_memakai_ssh_dan_alias_apa_adanya() {
        let dir = tempfile::tempdir().unwrap();
        fake_ssh(
            dir.path(),
            r#"for a in "$@"; do case "$a" in UserKnownHostsFile=*) kh="${a#UserKnownHostsFile=}";; esac; done
last=""; for a in "$@"; do last2="$last"; last="$a"; done
echo "$last2 ssh-ed25519 AAAAuji" > "$kh"; echo "$*" > "$kh.args"
echo "handoko_ben@lembur: Permission denied (publickey)." >&2; exit 255"#,
        );
        let out = run_scan(dir.path(), "lembur");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            key_lines(&String::from_utf8_lossy(&out.stdout)),
            "lembur ssh-ed25519 AAAAuji\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_host_tak_terjangkau_memberi_alasan_ssh() {
        let dir = tempfile::tempdir().unwrap();
        fake_ssh(
            dir.path(),
            "echo 'ssh: connect to host 34.101.156.130 port 22: Connection timed out' >&2; exit 255",
        );
        let out = run_scan(dir.path(), "handoko_ben@34.101.156.130");
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("Connection timed out"), "{err}");
    }
}
