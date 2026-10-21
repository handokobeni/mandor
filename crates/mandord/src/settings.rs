//! Pengaturan Lark milik daemon (`~/.mandor/lark.json`, 0600, ditulis atomik).

use std::path::Path;

use anyhow::Context;
use mandor_protocol::{LarkSettingsInput, LarkSettingsView};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

/// Port callback OAuth tetap (KTD-5).
pub const DEFAULT_REDIRECT_PORT: u16 = 53_682;
/// Jeda tenang Waiting Fix default (detik).
pub const DEFAULT_QUIET_SECS: i64 = 120;

/// Pengaturan Custom App dan tabel Sprint Tasks.
#[derive(Clone, Serialize, Deserialize)]
pub struct LarkSettings {
    pub app_id: String,
    #[serde(with = "secret")]
    pub app_secret: SecretString,
    pub app_token: String,
    pub table_id: String,
    pub ac_table_id: String,
    pub redirect_port: u16,
    pub open_base: String,
    pub accounts_base: String,
    pub quiet_secs: i64,
}

impl std::fmt::Debug for LarkSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LarkSettings")
            .field("app_id", &self.app_id)
            .field("app_token", &self.app_token)
            .field("table_id", &self.table_id)
            .finish_non_exhaustive()
    }
}

mod secret {
    use secrecy::{ExposeSecret, SecretString};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &SecretString, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(v.expose_secret())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SecretString, D::Error> {
        Ok(SecretString::from(String::deserialize(d)?))
    }
}

impl LarkSettings {
    /// URL callback OAuth.
    #[must_use]
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.redirect_port)
    }

    #[must_use]
    pub fn view(&self) -> LarkSettingsView {
        LarkSettingsView {
            app_id: self.app_id.clone(),
            has_secret: !self.app_secret.expose_secret().is_empty(),
            app_token: self.app_token.clone(),
            table_id: self.table_id.clone(),
            ac_table_id: self.ac_table_id.clone(),
            redirect_port: self.redirect_port,
            open_base: self.open_base.clone(),
            accounts_base: self.accounts_base.clone(),
            quiet_secs: self.quiet_secs,
        }
    }

    /// Menggabungkan input UI dengan pengaturan lama; secret kosong mempertahankan yang lama.
    ///
    /// # Errors
    /// Field wajib kosong.
    pub fn merge(old: Option<&Self>, input: LarkSettingsInput) -> anyhow::Result<Self> {
        let secret = match input.app_secret.filter(|s| !s.is_empty()) {
            Some(s) => SecretString::from(s),
            None => old
                .map(|o| o.app_secret.clone())
                .context("App Secret is required")?,
        };
        for (name, v) in [
            ("App ID", &input.app_id),
            ("Base app_token", &input.app_token),
            ("Sprint Tasks table ID", &input.table_id),
            ("Acceptance Criteria table ID", &input.ac_table_id),
        ] {
            anyhow::ensure!(!v.trim().is_empty(), "{name} is required");
        }
        Ok(Self {
            app_id: input.app_id.trim().to_owned(),
            app_secret: secret,
            app_token: input.app_token.trim().to_owned(),
            table_id: input.table_id.trim().to_owned(),
            ac_table_id: input.ac_table_id.trim().to_owned(),
            redirect_port: if input.redirect_port == 0 {
                DEFAULT_REDIRECT_PORT
            } else {
                input.redirect_port
            },
            open_base: input
                .open_base
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://open.larksuite.com".to_owned()),
            accounts_base: input
                .accounts_base
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "https://accounts.larksuite.com".to_owned()),
            quiet_secs: input.quiet_secs.unwrap_or(DEFAULT_QUIET_SECS).max(0),
        })
    }
}

/// Membaca pengaturan; `None` bila belum diatur.
///
/// # Errors
/// Berkas ada tetapi rusak atau tidak terbaca.
pub async fn load(path: &Path) -> anyhow::Result<Option<LarkSettings>> {
    match tokio::fs::read(path).await {
        Ok(body) => Ok(Some(
            serde_json::from_slice(&body).context("lark.json is corrupt")?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("reading lark.json"),
    }
}

/// Menyimpan pengaturan secara atomik dengan izin 0600.
///
/// # Errors
/// Berkas tidak bisa ditulis.
pub async fn save(path: &Path, settings: &LarkSettings) -> anyhow::Result<()> {
    let body = serde_json::to_vec_pretty(settings)?;
    let tmp = path.with_extension("tmp");
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file = opts
        .open(&tmp)
        .await
        .context("opening temporary lark.json")?;
    tokio::io::AsyncWriteExt::write_all(&mut file, &body).await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(&tmp, path)
        .await
        .context("writing lark.json")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(secret: Option<&str>) -> LarkSettingsInput {
        LarkSettingsInput {
            app_id: "cli_x".to_owned(),
            app_secret: secret.map(str::to_owned),
            app_token: "app".to_owned(),
            table_id: "tbl".to_owned(),
            ac_table_id: "tblac".to_owned(),
            redirect_port: 0,
            open_base: None,
            accounts_base: None,
            quiet_secs: None,
        }
    }

    #[tokio::test]
    async fn simpan_muat_0600_dan_secret_lama_dipertahankan() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("lark.json");
        assert!(
            load(&path)
                .await
                .unwrap_or_else(|e| panic!("{e}"))
                .is_none()
        );
        let s = LarkSettings::merge(None, input(Some("rahasia"))).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.redirect_port, DEFAULT_REDIRECT_PORT);
        save(&path, &s).await.unwrap_or_else(|e| panic!("{e}"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).map_or(0, |m| m.permissions().mode());
            assert_eq!(mode & 0o777, 0o600);
        }
        let back = load(&path)
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .unwrap_or_else(|| panic!("kosong"));
        let merged =
            LarkSettings::merge(Some(&back), input(None)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(merged.app_secret.expose_secret(), "rahasia");
        assert!(merged.view().has_secret);
        assert!(!format!("{merged:?}").contains("rahasia"));
        assert!(LarkSettings::merge(None, input(None)).is_err());
    }
}
