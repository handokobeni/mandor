//! Operasi Bitable: cari, ambil, ubah record, daftar dan buat field.

use reqwest::Method;
use secrecy::SecretString;
use serde_json::{Value, json};

use crate::{LarkClient, LarkError};

/// Ukuran halaman pencarian (maksimum Lark 500).
const PAGE_SIZE: u32 = 200;
/// Batas `batch_get` per panggilan.
const BATCH_MAX: usize = 100;

/// Record mentah dari Bitable.
#[derive(Debug, Clone, PartialEq)]
pub struct RawRecord {
    pub record_id: String,
    pub fields: Value,
    pub last_modified_ms: i64,
    pub last_modified_by: Option<String>,
}

/// Filter pencarian (konjungsi AND).
#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub conditions: Vec<(String, &'static str, Vec<String>)>,
}

impl SearchFilter {
    #[must_use]
    pub fn and(mut self, field: &str, op: &'static str, values: Vec<String>) -> Self {
        self.conditions.push((field.to_owned(), op, values));
        self
    }

    fn to_json(&self) -> Option<Value> {
        if self.conditions.is_empty() {
            return None;
        }
        let conds: Vec<Value> = self
            .conditions
            .iter()
            .map(|(f, op, v)| json!({"field_name": f, "operator": op, "value": v}))
            .collect();
        Some(json!({"conjunction": "and", "conditions": conds}))
    }
}

fn parse_record(item: &Value) -> Option<RawRecord> {
    let record_id = item.get("record_id")?.as_str()?.to_owned();
    Some(RawRecord {
        record_id,
        fields: item.get("fields").cloned().unwrap_or(Value::Null),
        last_modified_ms: item
            .get("last_modified_time")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        last_modified_by: item
            .get("last_modified_by")
            .and_then(|u| u.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

impl LarkClient {
    pub(crate) fn table_path(app_token: &str, table_id: &str, rest: &str) -> String {
        format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}{rest}")
    }

    /// Mencari semua record yang cocok (semua halaman).
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal.
    pub async fn search_records(
        &self,
        access: &SecretString,
        app_token: &str,
        table_id: &str,
        filter: &SearchFilter,
        field_names: &[&str],
    ) -> Result<Vec<RawRecord>, LarkError> {
        let path = Self::table_path(app_token, table_id, "/records/search");
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("page_size", PAGE_SIZE.to_string())];
            if let Some(t) = &page_token {
                query.push(("page_token", t.clone()));
            }
            let mut body = json!({"automatic_fields": true});
            if !field_names.is_empty() {
                body["field_names"] = json!(field_names);
            }
            if let Some(f) = filter.to_json() {
                body["filter"] = f;
            }
            let data = self
                .call(Method::POST, &path, &query, Some(&body), access)
                .await?;
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                out.extend(items.iter().filter_map(parse_record));
            }
            let more = data
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            page_token = data
                .get("page_token")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if !more || page_token.is_none() {
                return Ok(out);
            }
        }
    }

    /// Mengambil record berdasarkan ID (dipecah per 100).
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal.
    pub async fn batch_get(
        &self,
        access: &SecretString,
        app_token: &str,
        table_id: &str,
        record_ids: &[String],
    ) -> Result<Vec<RawRecord>, LarkError> {
        let path = Self::table_path(app_token, table_id, "/records/batch_get");
        let mut out = Vec::new();
        for chunk in record_ids.chunks(BATCH_MAX) {
            let body = json!({"record_ids": chunk, "automatic_fields": true});
            let data = self
                .call(Method::POST, &path, &[], Some(&body), access)
                .await?;
            if let Some(items) = data.get("records").and_then(Value::as_array) {
                out.extend(items.iter().filter_map(parse_record));
            }
        }
        Ok(out)
    }

    /// Mengubah field record; hanya field yang dikirim yang berubah.
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal.
    pub async fn update_record(
        &self,
        access: &SecretString,
        app_token: &str,
        table_id: &str,
        record_id: &str,
        fields: &Value,
    ) -> Result<RawRecord, LarkError> {
        let path = Self::table_path(app_token, table_id, &format!("/records/{record_id}"));
        let body = json!({"fields": fields});
        let data = self
            .call(Method::PUT, &path, &[], Some(&body), access)
            .await?;
        data.get("record")
            .and_then(parse_record)
            .ok_or_else(|| LarkError::Parse("record missing from update response".to_owned()))
    }

    /// Nama field yang ada di tabel beserta opsi SingleSelect-nya.
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal.
    pub async fn list_fields(
        &self,
        access: &SecretString,
        app_token: &str,
        table_id: &str,
    ) -> Result<Vec<Value>, LarkError> {
        let path = Self::table_path(app_token, table_id, "/fields");
        let mut out = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("page_size", "100".to_owned())];
            if let Some(t) = &page_token {
                query.push(("page_token", t.clone()));
            }
            let data = self.call(Method::GET, &path, &query, None, access).await?;
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                out.extend(items.iter().cloned());
            }
            let more = data
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            page_token = data
                .get("page_token")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if !more || page_token.is_none() {
                return Ok(out);
            }
        }
    }
}
