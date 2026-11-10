//! Password akun uji review lab per repo (plan 2026-11-10-001, KTD-L2).
//!
//! Berkas `<lab_root>/<repo>/private/secrets.json` (0600, folder 0700),
//! ditulis atomik. `lab_root` ada di bawah `~/.mandor`, yang selalu ditolak
//! hook untuk semua giliran agent. Password tidak pernah masuk log.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use mandor_protocol::LabPasswordInput;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

/// Password per role satu repo.
#[derive(Default)]
pub struct LabSecrets {
    by_role: BTreeMap<String, SecretString>,
}

impl std::fmt::Debug for LabSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabSecrets")
            .field("roles", &self.by_role.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Stored {
    #[serde(default)]
    passwords: BTreeMap<String, String>,
}

impl LabSecrets {
    /// Role punya password tersimpan.
    #[must_use]
    pub fn has(&self, role: &str) -> bool {
        self.by_role
            .get(role)
            .is_some_and(|p| !p.expose_secret().is_empty())
    }

    /// Password role, bila ada.
    #[must_use]
    pub fn password(&self, role: &str) -> Option<&SecretString> {
        self.by_role
            .get(role)
            .filter(|p| !p.expose_secret().is_empty())
    }

    /// Nilai password untuk penyamaran bukti (KTD-L12); tidak pernah dicatat.
    #[must_use]
    pub fn exposed_values(&self) -> Vec<String> {
        self.by_role
            .values()
            .map(|p| p.expose_secret().to_owned())
            .filter(|p| !p.is_empty())
            .collect()
    }

    /// Menggabungkan input UI: password kosong mempertahankan yang lama, role
    /// yang tidak lagi ada di perlengkapan dihapus.
    #[must_use]
    pub fn merge(mut self, roles: &[String], input: Vec<LabPasswordInput>) -> Self {
        self.by_role.retain(|role, _| roles.contains(role));
        for i in input {
            let Some(p) = i.password.filter(|p| !p.is_empty()) else {
                continue;
            };
            if roles.contains(&i.role) {
                self.by_role.insert(i.role, SecretString::from(p));
            }
        }
        self
    }
}

/// Folder perlengkapan privat repo.
#[must_use]
pub fn private_dir(lab_root: &Path, repo_id: &str) -> PathBuf {
    lab_root.join(repo_id).join("private")
}

fn secrets_path(lab_root: &Path, repo_id: &str) -> PathBuf {
    private_dir(lab_root, repo_id).join("secrets.json")
}

/// Membaca password repo; kosong bila belum ada.
///
/// # Errors
/// Berkas ada tetapi rusak atau tidak terbaca.
pub async fn load(lab_root: &Path, repo_id: &str) -> anyhow::Result<LabSecrets> {
    match tokio::fs::read(secrets_path(lab_root, repo_id)).await {
        Ok(body) => {
            let s: Stored = serde_json::from_slice(&body).context("lab secrets are corrupt")?;
            Ok(LabSecrets {
                by_role: s
                    .passwords
                    .into_iter()
                    .map(|(r, p)| (r, SecretString::from(p)))
                    .collect(),
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LabSecrets::default()),
        Err(e) => Err(e).context("reading lab secrets"),
    }
}

/// Menyimpan password repo secara atomik: folder 0700, berkas 0600.
///
/// # Errors
/// Folder atau berkas tidak bisa ditulis.
pub async fn save(lab_root: &Path, repo_id: &str, secrets: &LabSecrets) -> anyhow::Result<()> {
    let dir = private_dir(lab_root, repo_id);
    create_private_dir(&dir).await?;
    let body = serde_json::to_vec(&Stored {
        passwords: secrets
            .by_role
            .iter()
            .map(|(r, p)| (r.clone(), p.expose_secret().to_owned()))
            .collect(),
    })?;
    let path = secrets_path(lab_root, repo_id);
    let tmp = path.with_extension("tmp");
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts.open(&tmp).await.context("opening lab secrets")?;
    tokio::io::AsyncWriteExt::write_all(&mut file, &body).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&tmp, &path)
        .await
        .context("writing lab secrets")?;
    Ok(())
}

async fn create_private_dir(dir: &Path) -> anyhow::Result<()> {
    let mut b = tokio::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    b.mode(0o700);
    b.create(dir).await.context("creating lab folder")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .await
            .context("restricting lab folder")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(role: &str, password: Option<&str>) -> LabPasswordInput {
        LabPasswordInput {
            role: role.to_owned(),
            password: password.map(str::to_owned),
        }
    }

    fn roles(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn password_kosong_mempertahankan_yang_lama_dan_role_hilang_dihapus() {
        let s = LabSecrets::default().merge(
            &roles(&["admin", "manager"]),
            vec![input("admin", Some("a1")), input("manager", Some("m1"))],
        );
        let s = s.merge(
            &roles(&["admin"]),
            vec![input("admin", Some("")), input("manager", Some("m2"))],
        );
        assert_eq!(
            s.by_role.get("admin").map(ExposeSecret::expose_secret),
            Some("a1")
        );
        assert!(!s.has("manager"));
    }

    #[test]
    fn debug_tidak_memuat_password() {
        let s = LabSecrets::default()
            .merge(&roles(&["admin"]), vec![input("admin", Some("rahasia-1"))]);
        assert!(!format!("{s:?}").contains("rahasia-1"));
        assert!(!format!("{:?}", input("admin", Some("rahasia-1"))).contains("rahasia-1"));
    }

    #[tokio::test]
    async fn simpan_muat_dengan_mode_ketat() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let root = dir.path().join("lab");
        let loaded = load(&root, "repo1").await.unwrap_or_else(|e| panic!("{e}"));
        assert!(!loaded.has("admin"));
        let s = LabSecrets::default().merge(&roles(&["admin"]), vec![input("admin", Some("p"))]);
        save(&root, "repo1", &s)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let back = load(&root, "repo1").await.unwrap_or_else(|e| panic!("{e}"));
        assert!(back.has("admin"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| {
                std::fs::metadata(p)
                    .unwrap_or_else(|e| panic!("{e}"))
                    .permissions()
                    .mode()
                    & 0o777
            };
            assert_eq!(mode(&private_dir(&root, "repo1")), 0o700);
            assert_eq!(mode(&secrets_path(&root, "repo1")), 0o600);
        }
    }

    #[test]
    fn folder_lab_ada_di_bawah_mandor_dan_ditolak_hook() {
        let p = secrets_path(Path::new("/home/dev/.mandor/lab"), "repo1");
        assert!(mandor_domain::hook::is_credential_path(
            &p.to_string_lossy()
        ));
    }
}
