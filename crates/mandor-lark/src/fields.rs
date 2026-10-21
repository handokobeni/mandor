//! Pembacaan nilai field Bitable ke tipe sederhana.

use serde_json::Value;

/// Teks dari field Text (segmen `[{type, text}]`), string, angka, atau formula.
#[must_use]
pub fn text_of(v: Option<&Value>) -> Option<String> {
    let v = v?;
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => items
            .iter()
            .filter_map(|seg| match seg {
                Value::String(s) => Some(s.clone()),
                Value::Object(_) => seg
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| seg.get("name").and_then(Value::as_str).map(str::to_owned)),
                _ => None,
            })
            .collect::<String>(),
        // Formula/lookup: {"type": 1, "value": [...]}
        Value::Object(o) => {
            return text_of(o.get("value"))
                .or_else(|| o.get("link").and_then(Value::as_str).map(str::to_owned));
        }
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

/// Isi field URL. Teks dipakai bila berisi URL (beberapa PR, satu per baris),
/// selain itu link-nya (teks hanya label tampilan).
#[must_use]
pub fn url_text(v: Option<&Value>) -> Option<String> {
    let text = v
        .and_then(|o| o.get("text"))
        .and_then(Value::as_str)
        .filter(|t| t.starts_with("http"));
    match text {
        Some(t) => Some(t.to_owned()),
        None => text_of(v),
    }
}

/// Nama opsi dari field `SingleSelect`.
#[must_use]
pub fn select_text(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Array(items) => items.first().and_then(|i| i.as_str()).map(str::to_owned),
        _ => None,
    }
}

/// `open_id` dari field User (`[{id, name, ...}]`).
#[must_use]
pub fn user_ids(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|u| u.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// ID record dari field Link/DuplexLink (`{link_record_ids: [...]}`).
#[must_use]
pub fn link_ids(v: Option<&Value>) -> Vec<String> {
    let Some(v) = v else { return Vec::new() };
    let arr = v
        .get("link_record_ids")
        .and_then(Value::as_array)
        .or_else(|| v.as_array());
    arr.map(|items| {
        items
            .iter()
            .filter_map(|i| {
                i.as_str().map(str::to_owned).or_else(|| {
                    i.get("record_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn membaca_bentuk_field_nyata_sprint_tasks() {
        let fields = json!({
            "Task name": [{"text": "Profile Data: Tab Data", "type": "text"}],
            "Description": [{"text": "7 AC.\n\n", "type": "text"}, {"text": "Sumber", "type": "text"}],
            "ID": {"type": 1, "value": [{"text": "PGS-1388", "type": "text"}]},
            "Task Status": "Waiting Deploy",
            "Assignee (PIC)": [{"id": "ou_a", "name": "Dev A"}, {"id": "ou_b", "name": "Dev B"}],
            "Acceptance Criteria": {"link_record_ids": ["rec1", "rec2"]},
            "Sprint": {},
            "mandor_pr": {"text": "PR", "link": "https://github.com/o/r/pull/1"}
        });
        assert_eq!(
            text_of(fields.get("Task name")).as_deref(),
            Some("Profile Data: Tab Data")
        );
        assert_eq!(
            text_of(fields.get("Description")).as_deref(),
            Some("7 AC.\n\nSumber")
        );
        assert_eq!(text_of(fields.get("ID")).as_deref(), Some("PGS-1388"));
        assert_eq!(
            select_text(fields.get("Task Status")).as_deref(),
            Some("Waiting Deploy")
        );
        assert_eq!(user_ids(fields.get("Assignee (PIC)")), ["ou_a", "ou_b"]);
        assert_eq!(
            link_ids(fields.get("Acceptance Criteria")),
            ["rec1", "rec2"]
        );
        assert!(link_ids(fields.get("Sprint")).is_empty());
        assert_eq!(
            text_of(fields.get("mandor_pr")).as_deref(),
            Some("https://github.com/o/r/pull/1")
        );
        assert_eq!(text_of(fields.get("tidak ada")), None);
    }
}
