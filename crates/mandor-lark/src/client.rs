use std::time::Duration;

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

use crate::LarkError;

/// Konfigurasi Custom App Lark.
#[derive(Debug, Clone)]
pub struct LarkConfig {
    /// `https://open.larksuite.com` (atau `https://open.feishu.cn`).
    pub open_base: String,
    /// `https://accounts.larksuite.com`.
    pub accounts_base: String,
    pub app_id: String,
    pub app_secret: SecretString,
    /// Mis. `http://127.0.0.1:53682/callback`.
    pub redirect_uri: String,
    pub timeout: Duration,
}

impl LarkConfig {
    #[must_use]
    pub fn larksuite(app_id: String, app_secret: SecretString, redirect_uri: String) -> Self {
        Self {
            open_base: "https://open.larksuite.com".to_owned(),
            accounts_base: "https://accounts.larksuite.com".to_owned(),
            app_id,
            app_secret,
            redirect_uri,
            timeout: Duration::from_secs(20),
        }
    }
}

/// Klien HTTP Lark. Murah di-clone (satu `reqwest::Client` dibagi).
#[derive(Debug, Clone)]
pub struct LarkClient {
    pub(crate) cfg: LarkConfig,
    http: reqwest::Client,
}

impl LarkClient {
    /// Membuat klien dengan timeout dari konfigurasi.
    ///
    /// # Errors
    /// [`LarkError::Http`] bila klien HTTP tidak bisa dibangun.
    pub fn new(cfg: LarkConfig) -> Result<Self, LarkError> {
        let http = reqwest::Client::builder()
            .timeout(cfg.timeout)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| LarkError::Http(e.to_string()))?;
        Ok(Self { cfg, http })
    }

    /// Memanggil API Open Platform dan mengembalikan field `data`.
    pub(crate) async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        access: &SecretString,
    ) -> Result<Value, LarkError> {
        let url = format!("{}{path}", self.cfg.open_base);
        let mut req = self
            .http
            .request(method, url)
            .query(query)
            .bearer_auth(access.expose_secret());
        if let Some(b) = body {
            req = req.json(b);
        }
        self.send(req).await
    }

    /// Mengirim permintaan apa pun ke Lark dan memeriksa amplop `{code, msg, data}`.
    pub(crate) async fn send(&self, req: reqwest::RequestBuilder) -> Result<Value, LarkError> {
        let resp = req
            .send()
            .await
            .map_err(|e| LarkError::Http(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LarkError::RateLimited);
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| LarkError::Parse(format!("HTTP {status}: {e}")))?;
        let code = body.get("code").and_then(Value::as_i64).unwrap_or(0);
        if code != 0 {
            let msg = body
                .get("msg")
                .or_else(|| body.get("error_description"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Err(LarkError::from_code(code, msg));
        }
        if !status.is_success() {
            return Err(LarkError::Http(format!("HTTP {status}")));
        }
        Ok(body.get("data").cloned().unwrap_or(body))
    }

    pub(crate) fn post(&self, url: &str) -> reqwest::RequestBuilder {
        self.http.post(url)
    }

    pub(crate) fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http.get(url)
    }
}
