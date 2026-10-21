//! Nama pengguna dari Contact API (scope `contact:contact.base:readonly` dan
//! `contact:user.base:readonly`).

use std::collections::HashMap;

use reqwest::Method;
use secrecy::SecretString;
use serde_json::Value;

use crate::{LarkClient, LarkError};

/// Batas ID per permintaan `users/batch`.
const USERS_BATCH_MAX: usize = 50;

impl LarkClient {
    /// Nama tampilan per `open_id`; ID yang tidak dikenal Lark tidak ada di hasil.
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal (mis. scope belum disetujui).
    pub async fn user_names(
        &self,
        access: &SecretString,
        open_ids: &[String],
    ) -> Result<HashMap<String, String>, LarkError> {
        let mut out = HashMap::new();
        for chunk in open_ids.chunks(USERS_BATCH_MAX) {
            let mut query: Vec<(&str, String)> = vec![("user_id_type", "open_id".to_owned())];
            query.extend(chunk.iter().map(|id| ("user_ids", id.clone())));
            let data = self
                .call(
                    Method::GET,
                    "/open-apis/contact/v3/users/batch",
                    &query,
                    None,
                    access,
                )
                .await?;
            for item in data
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let text = |k: &str| {
                    item.get(k)
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                };
                if let (Some(id), Some(name)) =
                    (text("open_id"), text("name").or_else(|| text("en_name")))
                {
                    out.insert(id.to_owned(), name.to_owned());
                }
            }
        }
        Ok(out)
    }
}
