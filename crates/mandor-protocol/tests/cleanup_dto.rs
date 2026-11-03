//! DTO penutupan run selesai (plan `2026-11-03-001`, U3).
#![allow(clippy::unwrap_used)]

use mandor_protocol::{
    CleanupSettingsView, CloseFinishedStarted, FinishedRunView, FinishedRunsView, SkippedRunView,
};

#[test]
fn dto_bolak_balik_dan_ukuran_kosong_menjadi_null() {
    let run = FinishedRunView {
        run_id: "r1".to_owned(),
        keys: vec!["PGS-1".to_owned()],
        title: "Judul".to_owned(),
        bytes: None,
    };
    let view = FinishedRunsView {
        closable: vec![run.clone()],
        skipped: vec![SkippedRunView {
            run: run.clone(),
            reason: "unpushed_changes".to_owned(),
        }],
    };
    let json = serde_json::to_value(&view).unwrap();
    assert!(json["closable"][0]["bytes"].is_null());
    assert_eq!(
        serde_json::from_value::<FinishedRunsView>(json).unwrap(),
        view
    );

    let started = CloseFinishedStarted { count: 17 };
    let back: CloseFinishedStarted =
        serde_json::from_str(&serde_json::to_string(&started).unwrap()).unwrap();
    assert_eq!(back, started);

    let s: CleanupSettingsView = serde_json::from_str(r#"{"auto_close":true}"#).unwrap();
    assert!(s.auto_close);
}
