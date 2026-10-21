//! Daftar skill Claude untuk pilihan planning di dialog Start.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mandor_agent::available_skills;

fn skill(dir: &Path, name: &str, desc: &str) {
    std::fs::create_dir_all(dir.join(name)).unwrap();
    std::fs::write(
        dir.join(name).join("SKILL.md"),
        format!("---\nname: {name}\ndescription: \"{desc}\"\n---\n\nIsi skill"),
    )
    .unwrap();
}

#[tokio::test]
async fn skill_dari_plugin_aktif_dan_skill_user_tanpa_plugin_nonaktif() {
    let home = tempfile::tempdir().unwrap();
    let claude = home.path().join(".claude");
    let ce = home.path().join("cache/ce/3.18.0");
    let sp = home.path().join("cache/sp/5.0.0");
    skill(&ce.join("skills"), "ce-plan", "Create structured plans");
    skill(&ce.join("skills"), "ce-commit", "Commit changes");
    skill(&sp.join("skills"), "writing-plans", "Write plans");
    skill(&claude.join("skills"), "tdd", "Test-driven development");
    std::fs::create_dir_all(claude.join("plugins")).unwrap();
    std::fs::write(
        claude.join("plugins/installed_plugins.json"),
        serde_json::json!({"version": 2, "plugins": {
            "compound-engineering@m": [{"installPath": ce}],
            "superpowers@m": [{"installPath": sp}]
        }})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        claude.join("settings.json"),
        r#"{"enabledPlugins": {"compound-engineering@m": true, "superpowers@m": false}}"#,
    )
    .unwrap();

    let skills = available_skills(&claude).await;
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    // Skill bertema plan lebih dulu, lalu urut nama.
    assert_eq!(
        names,
        [
            "compound-engineering:ce-plan",
            "compound-engineering:ce-commit",
            "tdd"
        ]
    );
    assert_eq!(skills[0].description, "Create structured plans");
}

#[tokio::test]
async fn tanpa_konfigurasi_claude_daftarnya_kosong() {
    let home = tempfile::tempdir().unwrap();
    assert!(
        available_skills(&home.path().join(".claude"))
            .await
            .is_empty()
    );
}
