//! Parser `claude -p --output-format stream-json --verbose`.
//!
//! Satu baris bisa menghasilkan beberapa event (mis. teks dan beberapa
//! `tool_use`). Usage hanya diambil dari event `result` agar tidak terhitung
//! ganda (perbaikan bug §9.5 spesifikasi lama).

use mandor_domain::{AgentEvent, Usage};
use serde_json::Value;

/// Batas panjang ringkasan input tool dan hasil tool.
const PREVIEW_MAX: usize = 200;

/// Mengurai satu baris NDJSON. Baris rusak dilewati dengan log.
#[must_use]
pub fn parse_line(line: &str) -> Vec<AgentEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(error = %err, "baris stream agent rusak dilewati");
            return Vec::new();
        }
    };
    match (str_at(&v, "type"), str_at(&v, "subtype")) {
        ("system", "init") => vec![AgentEvent::SessionStarted {
            session_id: str_at(&v, "session_id").to_owned(),
            model: v.get("model").and_then(Value::as_str).map(str::to_owned),
        }],
        ("system", "permission_denied") => vec![AgentEvent::PermissionDenied {
            tool_name: str_at(&v, "tool_name").to_owned(),
            tool_use_id: str_at(&v, "tool_use_id").to_owned(),
            reason: truncate(
                v.get("decision_reason")
                    .or_else(|| v.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
        }],
        ("assistant", _) => assistant(&v),
        ("user", _) => tool_results(&v),
        ("rate_limit_event", _) => {
            let info = v.get("rate_limit_info");
            vec![AgentEvent::RateLimit {
                status: info
                    .and_then(|i| i.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                utilization: info
                    .and_then(|i| i.get("unifiedWindows"))
                    .and_then(|w| w.get("five_hour"))
                    .and_then(|f| f.get("utilization"))
                    .and_then(Value::as_f64),
            }]
        }
        ("result", _) => result(&v),
        _ => Vec::new(),
    }
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn truncate(s: &str) -> String {
    if s.chars().count() <= PREVIEW_MAX {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(PREVIEW_MAX).collect();
    out.push('…');
    out
}

fn content(v: &Value) -> &[Value] {
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn assistant(v: &Value) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    let mut text = Vec::new();
    for block in content(v) {
        match str_at(block, "type") {
            "text" => text.push(str_at(block, "text").to_owned()),
            "tool_use" => out.push(AgentEvent::ToolUse {
                id: str_at(block, "id").to_owned(),
                name: str_at(block, "name").to_owned(),
                preview: tool_preview(block.get("input")),
            }),
            _ => {}
        }
    }
    let joined = text.join("\n");
    if !joined.trim().is_empty() {
        out.insert(0, AgentEvent::AssistantMessage { text: joined });
    }
    out
}

/// Ringkasan input tool: perintah, path, pola, kueri, URL, atau string pendek pertama.
fn tool_preview(input: Option<&Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    for key in [
        "command",
        "file_path",
        "path",
        "pattern",
        "query",
        "url",
        "description",
    ] {
        if let Some(s) = input.get(key).and_then(Value::as_str) {
            return truncate(s);
        }
    }
    input
        .as_object()
        .and_then(|o| o.values().find_map(Value::as_str))
        .map(truncate)
        .unwrap_or_default()
}

fn tool_results(v: &Value) -> Vec<AgentEvent> {
    content(v)
        .iter()
        .filter(|b| str_at(b, "type") == "tool_result")
        .map(|b| AgentEvent::ToolResult {
            tool_use_id: str_at(b, "tool_use_id").to_owned(),
            is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            preview: truncate(&result_text(b.get("content"))),
        })
        .collect()
}

fn result_text(c: Option<&Value>) -> String {
    match c {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn u64_at(v: Option<&Value>, key: &str) -> u64 {
    v.and_then(|u| u.get(key))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn result(v: &Value) -> Vec<AgentEvent> {
    let u = v.get("usage");
    let usage = Usage {
        input_tokens: u64_at(u, "input_tokens"),
        output_tokens: u64_at(u, "output_tokens"),
        cache_creation_input_tokens: u64_at(u, "cache_creation_input_tokens"),
        cache_read_input_tokens: u64_at(u, "cache_read_input_tokens"),
    };
    let denials = v
        .get("permission_denials")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    vec![
        AgentEvent::Usage(usage),
        AgentEvent::Done {
            is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            result: str_at(v, "result").to_owned(),
            num_turns: u32::try_from(u64_at(Some(v), "num_turns")).unwrap_or(u32::MAX),
            denials: u32::try_from(denials).unwrap_or(u32::MAX),
        },
    ]
}
