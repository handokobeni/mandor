//! DM bot ke developer pemilik daemon (KTD-69 plan VM).

use reqwest::Method;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};

use crate::{LarkClient, LarkError};

impl LarkClient {
    /// Token bot Custom App (`tenant_access_token`).
    ///
    /// # Errors
    /// [`LarkError`] bila Lark menolak App ID atau Secret.
    pub async fn tenant_token(&self) -> Result<SecretString, LarkError> {
        let url = format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.cfg.open_base
        );
        let body = json!({
            "app_id": self.cfg.app_id,
            "app_secret": self.cfg.app_secret.expose_secret(),
        });
        let v = self.send(self.post(&url).json(&body)).await?;
        v.get("tenant_access_token")
            .and_then(Value::as_str)
            .map(|t| SecretString::from(t.to_owned()))
            .ok_or_else(|| LarkError::Parse("tenant_access_token missing".to_owned()))
    }

    /// Pesan teks dari bot ke `open_id`.
    ///
    /// # Errors
    /// [`LarkError`] bila Lark menolak (mis. bot tanpa izin kirim pesan).
    pub async fn send_text(
        &self,
        tenant: &SecretString,
        open_id: &str,
        text: &str,
    ) -> Result<(), LarkError> {
        let body = json!({
            "receive_id": open_id,
            "msg_type": "text",
            "content": json!({ "text": text }).to_string(),
        });
        self.call(
            Method::POST,
            "/open-apis/im/v1/messages",
            &[("receive_id_type", "open_id".to_owned())],
            Some(&body),
            tenant,
        )
        .await?;
        Ok(())
    }
}
