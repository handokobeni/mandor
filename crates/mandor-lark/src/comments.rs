//! Komentar record Bitable lewat API komentar drive (`file_type=bitable`).
//!
//! API ini belum terdokumentasi resmi untuk Bitable. Komentar tidak punya field
//! anchor record; record diketahui dari `reply.extra.notify_extra.record`.
//! Komentar yang tidak bisa dipetakan dilewati dengan log.

use reqwest::Method;
use secrecy::SecretString;
use serde_json::Value;

use crate::{LarkClient, LarkError};

/// Satu balasan (unit terkecil komentar) yang terpetakan ke record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordReply {
    pub record_id: String,
    pub comment_id: String,
    pub reply_id: String,
    pub author_open_id: String,
    /// Detik epoch.
    pub create_time: i64,
    pub text: String,
}

fn reply_text(reply: &Value) -> String {
    reply
        .get("content")
        .and_then(|c| c.get("elements"))
        .and_then(Value::as_array)
        .map(|els| {
            els.iter()
                .map(|e| {
                    e.get("text_run")
                        .and_then(|t| t.get("text"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            e.get("person").map(|p| {
                                format!(
                                    "@{}",
                                    p.get("user_id").and_then(Value::as_str).unwrap_or("?")
                                )
                            })
                        })
                        .or_else(|| {
                            e.get("docs_link")
                                .and_then(|d| d.get("url"))
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        })
                        .unwrap_or_default()
                })
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn record_of(reply: &Value) -> Option<String> {
    let extra = reply.get("extra")?;
    // `extra` bisa objek atau string JSON.
    let extra: Value = match extra {
        Value::String(s) => serde_json::from_str(s).ok()?,
        other => other.clone(),
    };
    let notify = extra.get("notify_extra")?;
    let notify: Value = match notify {
        Value::String(s) => serde_json::from_str(s).ok()?,
        other => other.clone(),
    };
    notify
        .get("record")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Memetakan item komentar dari API ke balasan per record. Record sebuah thread
/// diambil dari balasan mana pun yang memuat metadata record.
#[must_use]
pub fn map_record_replies(items: &[Value]) -> Vec<RecordReply> {
    let mut out = Vec::new();
    for comment in items {
        let Some(comment_id) = comment.get("comment_id").and_then(Value::as_str) else {
            continue;
        };
        let replies = comment
            .get("reply_list")
            .and_then(|r| r.get("replies"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(record_id) = replies.iter().find_map(record_of) else {
            tracing::debug!(comment_id, "komentar tanpa metadata record dilewati");
            continue;
        };
        for r in &replies {
            let (Some(reply_id), Some(author)) = (
                r.get("reply_id").and_then(Value::as_str),
                r.get("user_id").and_then(Value::as_str),
            ) else {
                continue;
            };
            out.push(RecordReply {
                record_id: record_id.clone(),
                comment_id: comment_id.to_owned(),
                reply_id: reply_id.to_owned(),
                author_open_id: author.to_owned(),
                create_time: r.get("create_time").and_then(Value::as_i64).unwrap_or(0),
                text: reply_text(r),
            });
        }
    }
    out
}

impl LarkClient {
    /// Semua balasan komentar di Base yang terpetakan ke record. Paging berhenti
    /// lebih awal bila `stop_before` (detik) diberikan dan seluruh komentar di
    /// halaman lebih lama darinya.
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal.
    pub async fn record_replies(
        &self,
        access: &SecretString,
        app_token: &str,
        stop_before: Option<i64>,
    ) -> Result<Vec<RecordReply>, LarkError> {
        let path = format!("/open-apis/drive/v1/files/{app_token}/comments");
        let mut items: Vec<Value> = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![
                ("file_type", "bitable".to_owned()),
                ("page_size", "100".to_owned()),
                ("user_id_type", "open_id".to_owned()),
            ];
            if let Some(t) = &page_token {
                query.push(("page_token", t.clone()));
            }
            let data = self.call(Method::GET, &path, &query, None, access).await?;
            let page: Vec<Value> = data
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let all_old = stop_before.is_some_and(|limit| {
                !page.is_empty() && page.iter().all(|c| latest_time(c) < limit)
            });
            for mut c in page {
                self.fill_replies(access, app_token, &mut c).await?;
                items.push(c);
            }
            let more = data
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            page_token = data
                .get("page_token")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if !more || page_token.is_none() || all_old {
                break;
            }
        }
        Ok(map_record_replies(&items))
    }

    /// Mengambil sisa balasan bila thread punya lebih banyak dari yang dikembalikan.
    async fn fill_replies(
        &self,
        access: &SecretString,
        app_token: &str,
        comment: &mut Value,
    ) -> Result<(), LarkError> {
        let has_more = comment
            .get("has_more")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let Some(comment_id) = comment
            .get("comment_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return Ok(());
        };
        if !has_more {
            return Ok(());
        }
        let path = format!("/open-apis/drive/v1/files/{app_token}/comments/{comment_id}/replies");
        let mut replies = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![
                ("file_type", "bitable".to_owned()),
                ("page_size", "100".to_owned()),
                ("user_id_type", "open_id".to_owned()),
            ];
            if let Some(t) = &page_token {
                query.push(("page_token", t.clone()));
            }
            let data = self.call(Method::GET, &path, &query, None, access).await?;
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                replies.extend(items.iter().cloned());
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
                break;
            }
        }
        comment["reply_list"] = serde_json::json!({ "replies": replies });
        Ok(())
    }
}

fn latest_time(comment: &Value) -> i64 {
    let created = comment
        .get("create_time")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let updated = comment
        .get("update_time")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let replies = comment
        .get("reply_list")
        .and_then(|r| r.get("replies"))
        .and_then(Value::as_array)
        .and_then(|rs| {
            rs.iter()
                .filter_map(|r| r.get("create_time").and_then(Value::as_i64))
                .max()
        })
        .unwrap_or(0);
    created.max(updated).max(replies)
}
