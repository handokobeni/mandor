//! Bootstrap field milik app di tabel Sprint Tasks (idempoten).

use std::fmt::Write as _;

use reqwest::Method;
use secrecy::SecretString;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{LarkClient, LarkError};

/// Definisi field milik app yang dibuat saat bootstrap.
#[derive(Debug, Clone, Copy)]
pub struct FieldDef {
    pub name: &'static str,
    /// Kode tipe Bitable: 1 Text, 15 Url, 1002 `ModifiedTime`.
    pub kind: u32,
}

/// Field milik app di Sprint Tasks (KTD-6, KTD-8, KTD-12).
pub const APP_FIELDS: &[FieldDef] = &[
    FieldDef {
        name: "mandor_branch",
        kind: 1,
    },
    FieldDef {
        name: "mandor_pr",
        kind: 15,
    },
    FieldDef {
        name: "mandor_owner",
        kind: 1,
    },
    FieldDef {
        name: "mandor_comment_cursor",
        kind: 1,
    },
    FieldDef {
        name: "mandor_modified",
        kind: 1002,
    },
    FieldDef {
        name: "Fix Notes",
        kind: 1,
    },
];

/// Memastikan `option` adalah opsi yang sudah ada pada field `SingleSelect`
/// `field`, agar penulisan tidak membuat opsi baru di Lark.
///
/// # Errors
/// [`LarkError::InvalidOption`] bila field atau opsinya tidak ada.
pub fn validate_select(fields: &[Value], field: &str, option: &str) -> Result<(), LarkError> {
    let found = fields
        .iter()
        .find(|f| f.get("field_name").and_then(Value::as_str) == Some(field))
        .and_then(|f| f.get("property"))
        .and_then(|p| p.get("options"))
        .and_then(Value::as_array)
        .is_some_and(|opts| {
            opts.iter()
                .any(|o| o.get("name").and_then(Value::as_str) == Some(option))
        });
    if found {
        Ok(())
    } else {
        Err(LarkError::InvalidOption {
            field: field.to_owned(),
            option: option.to_owned(),
        })
    }
}

/// `client_token` pembuatan field: Lark mewajibkan format `UUIDv4`. Diturunkan
/// dari tabel dan nama field agar permintaan ulang tetap idempoten.
fn field_client_token(table_id: &str, name: &str) -> String {
    let digest = Sha256::digest(format!("mandor-field:{table_id}:{name}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&digest[..16]);
    b[6] = (b[6] & 0x0f) | 0x40; // versi 4
    b[8] = (b[8] & 0x3f) | 0x80; // varian RFC 4122
    let hex = b.iter().fold(String::with_capacity(32), |mut acc, x| {
        let _ = write!(acc, "{x:02x}");
        acc
    });
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

impl LarkClient {
    /// Membuat field milik app yang belum ada (idempoten).
    /// Mengembalikan nama field yang dibuat.
    ///
    /// # Errors
    /// [`LarkError`] bila permintaan gagal.
    pub async fn ensure_app_fields(
        &self,
        access: &SecretString,
        app_token: &str,
        table_id: &str,
    ) -> Result<Vec<&'static str>, LarkError> {
        let existing = self.list_fields(access, app_token, table_id).await?;
        let has = |name: &str| {
            existing
                .iter()
                .any(|f| f.get("field_name").and_then(Value::as_str) == Some(name))
        };
        let path = Self::table_path(app_token, table_id, "/fields");
        let mut created = Vec::new();
        for def in APP_FIELDS.iter().filter(|d| !has(d.name)) {
            let client_token = field_client_token(table_id, def.name);
            let body = json!({"field_name": def.name, "type": def.kind});
            self.call(
                Method::POST,
                &path,
                &[("client_token", client_token)],
                Some(&body),
                access,
            )
            .await?;
            created.push(def.name);
        }
        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_token_uuid_v4_stabil_dan_unik_per_field() {
        let a = field_client_token("tbl1", "mandor_branch");
        assert_eq!(a, field_client_token("tbl1", "mandor_branch"));
        assert_ne!(a, field_client_token("tbl1", "mandor_pr"));
        assert_ne!(a, field_client_token("tbl2", "mandor_branch"));
        // Format UUIDv4: 8-4-4-4-12 hex, versi 4, varian 8/9/a/b.
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(
            a.chars()
                .all(|c| c == '-' || c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert!(parts[2].starts_with('4'));
        assert!(matches!(
            parts[3].chars().next(),
            Some('8' | '9' | 'a' | 'b')
        ));
    }

    #[test]
    fn opsi_select_divalidasi_terhadap_daftar_field() {
        let fields = vec![json!({
            "field_name": "Task Status",
            "type": 3,
            "property": {"options": [{"name": "To Do"}, {"name": "In Progress"}]}
        })];
        assert!(validate_select(&fields, "Task Status", "In Progress").is_ok());
        assert!(matches!(
            validate_select(&fields, "Task Status", "In Progres"),
            Err(LarkError::InvalidOption { .. })
        ));
        assert!(validate_select(&fields, "Tidak Ada", "To Do").is_err());
    }
}
