//! Penyamaran nilai yang tampak rahasia (KTD-A18, R25) untuk catatan Chat,
//! notifikasi, dan laporan pagi.

use serde_json::Value;

/// Pengganti nilai rahasia.
pub const MASK: &str = "***";

/// Nama kunci yang nilainya dianggap rahasia.
const SECRET_NAMES: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "key",
    "authorization",
    "credential",
];

/// Awalan token yang dikenal.
const TOKEN_PREFIXES: &[&str] = &[
    "ghp_",
    "gho_",
    "ghs_",
    "ghu_",
    "github_pat_",
    "AKIA",
    "sk-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxs-",
];

/// Program database yang menerima kata sandi lewat `-p<rahasia>`.
const PASSWORD_P_PROGRAMS: &[&str] = &["mysql", "mysqldump", "mariadb", "psql"];

fn secret_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SECRET_NAMES.iter().any(|s| lower.contains(s))
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Teks dengan nilai rahasia disamarkan; teks lain tidak berubah.
#[must_use]
pub fn redact_text(text: &str) -> String {
    let text = bearer(text);
    let text = key_values(&text);
    let text = url_userinfo(&text);
    let text = flag_values(&text);
    token_prefixes(&text)
}

/// Input tool JSON: nilai berkunci nama rahasia disamarkan secara rekursif,
/// string lain lewat [`redact_text`].
#[must_use]
pub fn redact_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = if secret_name(k) && !v.is_null() && !v.is_object() && !v.is_array() {
                        Value::String(MASK.to_owned())
                    } else {
                        redact_json(v)
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_json).collect()),
        Value::String(s) => Value::String(redact_text(s)),
        other => other.clone(),
    }
}

/// `Bearer <token>`.
fn bearer(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(at) = lower.find("bearer ") else {
            out.push_str(rest);
            return out;
        };
        let boundary = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !ident_char(c));
        let value_start = at + "bearer ".len();
        let value_len = rest[value_start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';'))
            .unwrap_or(rest.len() - value_start);
        out.push_str(&rest[..value_start]);
        if boundary && value_len > 0 {
            out.push_str(MASK);
        } else {
            out.push_str(&rest[value_start..value_start + value_len]);
        }
        rest = &rest[value_start + value_len..];
    }
}

/// `name=value`, `name: value`, `"name":"value"` dengan nama rahasia.
/// Nilai `authorization` berlanjut sampai akhir baris atau kutipan.
fn key_values(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !ident_char(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while chars.get(i).copied().is_some_and(ident_char) {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        out.push_str(&name);
        if !secret_name(&name) {
            continue;
        }
        // Kutip penutup nama, spasi, lalu `=` atau `:`.
        let mut j = i;
        if chars.get(j).is_some_and(|c| matches!(c, '"' | '\'')) {
            j += 1;
        }
        while chars.get(j) == Some(&' ') {
            j += 1;
        }
        if !chars.get(j).is_some_and(|c| matches!(c, '=' | ':')) {
            continue;
        }
        j += 1;
        while chars.get(j) == Some(&' ') {
            j += 1;
        }
        let quote = chars.get(j).copied().filter(|c| matches!(c, '"' | '\''));
        let value_start = j + usize::from(quote.is_some());
        let whole_line = name.to_ascii_lowercase().contains("authorization");
        let mut k = value_start;
        while let Some(&c) = chars.get(k) {
            let end = match quote {
                Some(q) => c == q,
                None if whole_line => c == '\n' || c == '"' || c == '\'',
                None => c.is_whitespace() || matches!(c, '&' | ',' | ';' | '}' | '"' | '\''),
            };
            if end {
                break;
            }
            k += 1;
        }
        if k == value_start || chars[value_start..k].iter().collect::<String>() == MASK {
            continue;
        }
        out.extend(&chars[i..value_start]);
        out.push_str(MASK);
        i = k;
    }
    out
}

/// `scheme://user:pass@host` → `scheme://***@host`.
fn url_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let auth_start = at + 3;
        out.push_str(&rest[..auth_start]);
        let tail = &rest[auth_start..];
        let auth_end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '?' | '#' | '"' | '\''))
            .unwrap_or(tail.len());
        match tail[..auth_end].rfind('@') {
            Some(userinfo) => {
                out.push_str(MASK);
                rest = &tail[userinfo..];
            }
            None => rest = tail,
        }
    }
    out.push_str(rest);
    out
}

/// Nilai flag sebagai kata berikutnya: `--password x`, `--token x`,
/// `-u user:pass`/`--user user:pass`, dan `-p<rahasia>` untuk klien database.
fn flag_values(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let db = words.iter().any(|w| {
        let prog = w.rsplit('/').next().unwrap_or(w);
        PASSWORD_P_PROGRAMS.contains(&prog)
    });
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut mask_next = false;
    let mut mask_pass = false;
    for w in words {
        if mask_next && !w.is_empty() {
            out.push(MASK.to_owned());
            mask_next = false;
            continue;
        }
        if mask_pass && !w.is_empty() {
            out.push(match w.split_once(':') {
                Some((user, _)) => format!("{user}:{MASK}"),
                None => w.to_owned(),
            });
            mask_pass = false;
            continue;
        }
        let flag = w.trim_start_matches('-');
        if w.starts_with("--") && !w.contains('=') && secret_name(flag) {
            mask_next = true;
        } else if w == "-u" || w == "--user" {
            mask_pass = true;
        } else if db && w.len() > 2 && w.starts_with("-p") && !w.starts_with("--") {
            out.push(format!("-p{MASK}"));
            continue;
        }
        out.push(w.to_owned());
    }
    out.join(" ")
}

/// Token dengan awalan yang dikenal, mis. `ghp_…`, `AKIA…`, `sk-…`.
fn token_prefixes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        match TOKEN_PREFIXES
            .iter()
            .find(|p| word.starts_with(*p) && word.len() > p.len() + 3)
        {
            Some(p) => {
                out.push_str(p);
                out.push_str(MASK);
            }
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in text.chars() {
        if ident_char(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn nilai_rahasia_disamarkan() {
        for (input, want) in [
            (
                "curl -H 'Authorization: Bearer abc.def' https://x",
                "curl -H 'Authorization: ***' https://x",
            ),
            ("Bearer abc123", "Bearer ***"),
            ("PGPASSWORD=secret psql -h db", "PGPASSWORD=*** psql -h db"),
            (
                "git clone https://user:pw@host/r.git",
                "git clone https://***@host/r.git",
            ),
            ("curl -u a:b https://x", "curl -u a:*** https://x"),
            ("mysql -psecret -h db", "mysql -p*** -h db"),
            ("tool --password=x", "tool --password=***"),
            ("tool --password x y", "tool --password *** y"),
            ("echo ghp_abcdefghijkl", "echo ghp_***"),
            ("AKIAABCDEFGHIJKL", "AKIA***"),
            ("export OPENAI=sk-abcdefgh", "export OPENAI=sk-***"),
            (
                r#"{"api_key": "x1", "ok": 1}"#,
                r#"{"api_key": "***", "ok": 1}"#,
            ),
        ] {
            assert_eq!(redact_text(input), want, "{input}");
        }
    }

    #[test]
    fn teks_biasa_tidak_berubah() {
        for text in [
            "cd kelola-api && git log --oneline -5",
            "jq .keys package.json",
            "sort -u f | head",
            "psql -p 5432 -h localhost",
            "https://docs.example.com/a?b=c",
            "npx vitest run \"src/app/(app)/x.test.tsx\" 2>&1 | tail -30",
        ] {
            assert_eq!(redact_text(text), text);
        }
    }

    #[test]
    fn json_disamarkan_rekursif() {
        let input = json!({"api_key": "x", "nested": {"token": "y", "n": 2},
            "url": "https://u:p@h/x", "list": ["Bearer z"]});
        assert_eq!(
            redact_json(&input),
            json!({"api_key": "***", "nested": {"token": "***", "n": 2},
                "url": "https://***@h/x", "list": ["Bearer ***"]})
        );
    }
}
