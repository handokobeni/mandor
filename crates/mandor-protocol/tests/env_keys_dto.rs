//! DTO Pick from .env (plan `2026-11-03-002`, U1, KTD-P1).
#![allow(clippy::unwrap_used)]

use mandor_protocol::{EnvKeysParams, EnvKeysView, method};

#[test]
fn parameter_repo_atau_sumber_desain_dan_hasil_hanya_nama() {
    let p: EnvKeysParams = serde_json::from_str(r#"{"repo_id":"kelola-api"}"#).unwrap();
    assert_eq!(p.repo_id.as_deref(), Some("kelola-api"));
    assert_eq!(p.design_source_id, None);

    let p: EnvKeysParams = serde_json::from_str(r#"{"design_source_id":"d1"}"#).unwrap();
    assert_eq!(p.design_source_id.as_deref(), Some("d1"));

    let view = EnvKeysView {
        found: true,
        keys: vec!["KELOLA_ADDR".to_owned()],
    };
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"found": true, "keys": ["KELOLA_ADDR"]})
    );
    assert_eq!(method::APP_ENV_KEYS, "app.env_keys");
}
