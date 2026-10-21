//! OAuth per user dengan PKCE (KTD-5).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngExt;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{LarkClient, LarkError};

/// Scope yang diminta: baca/tulis Bitable, baca komentar, nama orang (Contact
/// `users/batch` butuh `contact:contact.base:readonly` dan field dasar
/// `contact:user.base:readonly`), refresh token.
pub const SCOPES: &str = "bitable:app docs:document.comment:read contact:contact.base:readonly \
     contact:user.base:readonly offline_access";

/// Pasangan verifier/challenge PKCE (S256).
#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: SecretString,
    pub challenge: String,
}

impl Pkce {
    /// Membuat verifier acak 64 karakter.
    #[must_use]
    pub fn generate() -> Self {
        const ALPHABET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let mut rng = rand::rng();
        let verifier: String = (0..64)
            .map(|_| char::from(ALPHABET[rng.random_range(0..ALPHABET.len())]))
            .collect();
        Self::from_verifier(&verifier)
    }

    /// Menghitung challenge S256 dari verifier.
    #[must_use]
    pub fn from_verifier(verifier: &str) -> Self {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier: SecretString::from(verifier.to_owned()),
            challenge,
        }
    }
}

/// Membuat nilai `state` acak untuk mencegah login CSRF.
#[must_use]
pub fn new_state() -> String {
    let bytes: [u8; 16] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// URL otorisasi yang dibuka di browser pengguna.
#[must_use]
pub fn authorize_url(client: &LarkClient, state: &str, pkce: &Pkce) -> String {
    let cfg = &client.cfg;
    let q = [
        ("client_id", cfg.app_id.as_str()),
        ("response_type", "code"),
        ("redirect_uri", cfg.redirect_uri.as_str()),
        ("scope", SCOPES),
        ("state", state),
        ("code_challenge", pkce.challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];
    let query: Vec<String> = q
        .iter()
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect();
    format!(
        "{}/open-apis/authen/v1/authorize?{}",
        cfg.accounts_base,
        query.join("&")
    )
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Pengguna pemilik token user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdentity {
    pub open_id: String,
    pub name: Option<String>,
}

/// Token user beserta waktu kedaluwarsa (ms epoch).
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenSet {
    #[serde(with = "secret_str")]
    pub access_token: SecretString,
    #[serde(with = "secret_str")]
    pub refresh_token: SecretString,
    pub access_expires_at_ms: i64,
    pub refresh_expires_at_ms: i64,
    pub open_id: Option<String>,
    /// Nama akun Lark yang login (dari `user_info`); kosong pada token lama.
    #[serde(default)]
    pub name: Option<String>,
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_expires_at_ms", &self.access_expires_at_ms)
            .field("refresh_expires_at_ms", &self.refresh_expires_at_ms)
            .field("open_id", &self.open_id)
            .finish_non_exhaustive()
    }
}

impl TokenSet {
    /// `true` bila access token habis dalam `margin_ms`.
    #[must_use]
    pub fn access_expiring(&self, now_ms: i64, margin_ms: i64) -> bool {
        self.access_expires_at_ms - margin_ms <= now_ms
    }
}

mod secret_str {
    use secrecy::{ExposeSecret, SecretString};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &SecretString, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(v.expose_secret())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SecretString, D::Error> {
        Ok(SecretString::from(String::deserialize(d)?))
    }
}

impl LarkClient {
    /// Menukar kode otorisasi dengan token user.
    ///
    /// # Errors
    /// [`LarkError`] bila Lark menolak penukaran.
    pub async fn exchange_code(
        &self,
        code: &str,
        pkce: &Pkce,
        now_ms: i64,
    ) -> Result<TokenSet, LarkError> {
        let body = json!({
            "grant_type": "authorization_code",
            "client_id": self.cfg.app_id,
            "client_secret": self.cfg.app_secret.expose_secret(),
            "code": code,
            "redirect_uri": self.cfg.redirect_uri,
            "code_verifier": pkce.verifier.expose_secret(),
        });
        let mut tokens = self.token_request(&body, now_ms).await?;
        let who = self.user_identity(&tokens.access_token).await?;
        tokens.open_id = Some(who.open_id);
        tokens.name = who.name;
        Ok(tokens)
    }

    /// Memperbarui token. Refresh token berotasi; pemanggil wajib menyimpan
    /// pasangan baru secara atomik sebelum memakainya.
    ///
    /// # Errors
    /// [`LarkError::ReauthRequired`] bila refresh token tidak berlaku lagi.
    pub async fn refresh(&self, current: &TokenSet, now_ms: i64) -> Result<TokenSet, LarkError> {
        let body = json!({
            "grant_type": "refresh_token",
            "client_id": self.cfg.app_id,
            "client_secret": self.cfg.app_secret.expose_secret(),
            "refresh_token": current.refresh_token.expose_secret(),
        });
        let mut tokens = self.token_request(&body, now_ms).await?;
        tokens.open_id.clone_from(&current.open_id);
        tokens.name.clone_from(&current.name);
        Ok(tokens)
    }

    async fn token_request(&self, body: &Value, now_ms: i64) -> Result<TokenSet, LarkError> {
        let url = format!("{}/open-apis/authen/v2/oauth/token", self.cfg.open_base);
        let data = self.send(self.post(&url).json(body)).await?;
        let str_field = |k: &str| {
            data.get(k)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| LarkError::Parse(format!("field {k} missing")))
        };
        let secs = |k: &str| data.get(k).and_then(Value::as_i64).unwrap_or(0);
        Ok(TokenSet {
            access_token: SecretString::from(str_field("access_token")?),
            refresh_token: SecretString::from(str_field("refresh_token")?),
            access_expires_at_ms: now_ms + secs("expires_in") * 1000,
            refresh_expires_at_ms: now_ms + secs("refresh_token_expires_in") * 1000,
            open_id: None,
            name: None,
        })
    }

    /// `open_id` dan nama pengguna pemilik token (tanpa scope Contact).
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal.
    pub async fn user_identity(&self, access: &SecretString) -> Result<UserIdentity, LarkError> {
        let url = format!("{}/open-apis/authen/v1/user_info", self.cfg.open_base);
        let data = self
            .send(self.get(&url).bearer_auth(access.expose_secret()))
            .await?;
        let open_id = data
            .get("open_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| LarkError::Parse("open_id missing".to_owned()))?;
        let name = data
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .map(str::to_owned);
        Ok(UserIdentity { open_id, name })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_s256_sesuai_rfc7636() {
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(p.challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn verifier_acak_panjang_64_dan_berbeda() {
        let a = Pkce::generate();
        let b = Pkce::generate();
        assert_eq!(a.verifier.expose_secret().len(), 64);
        assert_ne!(a.verifier.expose_secret(), b.verifier.expose_secret());
        assert_ne!(new_state(), new_state());
    }

    #[test]
    fn debug_token_tidak_membocorkan_rahasia() {
        let t = TokenSet {
            access_token: SecretString::from("u-rahasia".to_owned()),
            refresh_token: SecretString::from("r-rahasia".to_owned()),
            access_expires_at_ms: 1,
            refresh_expires_at_ms: 2,
            open_id: None,
            name: None,
        };
        let s = format!("{t:?}");
        assert!(!s.contains("rahasia"), "{s}");
    }
}
