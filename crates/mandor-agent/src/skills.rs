//! Skill Claude yang bisa dipakai agent untuk planning: skill dari plugin yang
//! aktif di pengaturan user dan skill user di `~/.claude/skills`. Hanya nama dan
//! deskripsi yang dibaca.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

/// Batas waktu membaca seluruh daftar skill dari disk.
const LIST_TIMEOUT: Duration = Duration::from_secs(5);

/// Satu skill yang bisa dipanggil lewat tool `Skill`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInfo {
    /// `plugin:skill` untuk skill plugin, `skill` untuk skill user.
    pub name: String,
    pub description: String,
}

/// Skill yang tersedia untuk agent; kosong bila konfigurasi Claude tidak ada
/// atau tidak terbaca dalam [`LIST_TIMEOUT`].
pub async fn available_skills(claude_home: &Path) -> Vec<SkillInfo> {
    if let Ok(skills) = tokio::time::timeout(LIST_TIMEOUT, collect(claude_home)).await {
        skills
    } else {
        tracing::warn!("membaca daftar skill Claude melewati batas waktu");
        Vec::new()
    }
}

async fn collect(claude_home: &Path) -> Vec<SkillInfo> {
    let mut out = Vec::new();
    for (plugin, dir) in enabled_plugins(claude_home).await {
        for (name, description) in skills_in(&dir.join("skills")).await {
            out.push(SkillInfo {
                name: format!("{plugin}:{name}"),
                description,
            });
        }
    }
    for (name, description) in skills_in(&claude_home.join("skills")).await {
        out.push(SkillInfo { name, description });
    }
    // Skill bertema plan lebih dulu, lalu urut nama.
    out.sort_by_key(|s| (!s.name.contains("plan"), s.name.clone()));
    out
}

async fn read_json(path: &Path) -> Option<Value> {
    let raw = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str(&raw).ok()
}

/// `(nama plugin, folder instalasi)` untuk plugin yang aktif.
async fn enabled_plugins(claude_home: &Path) -> Vec<(String, PathBuf)> {
    let Some(settings) = read_json(&claude_home.join("settings.json")).await else {
        return Vec::new();
    };
    let Some(installed) = read_json(&claude_home.join("plugins/installed_plugins.json")).await
    else {
        return Vec::new();
    };
    let Some(enabled) = settings.get("enabledPlugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, on) in enabled {
        if on.as_bool() != Some(true) {
            continue;
        }
        let path = installed
            .get("plugins")
            .and_then(|p| p.get(key))
            .and_then(Value::as_array)
            .and_then(|v| v.first())
            .and_then(|i| i.get("installPath"))
            .and_then(Value::as_str);
        if let Some(path) = path {
            let plugin = key.split('@').next().unwrap_or(key).to_owned();
            out.push((plugin, PathBuf::from(path)));
        }
    }
    out
}

/// `(nama, deskripsi)` dari `<dir>/<skill>/SKILL.md`.
async fn skills_in(dir: &Path) -> Vec<(String, String)> {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(text) = tokio::fs::read_to_string(entry.path().join("SKILL.md")).await else {
            continue;
        };
        let fallback = entry.file_name().to_string_lossy().into_owned();
        let (name, description) = frontmatter(&text);
        out.push((name.unwrap_or(fallback), description.unwrap_or_default()));
    }
    out
}

/// `name` dan `description` dari frontmatter YAML sederhana.
fn frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let (mut name, mut description) = (None, None);
    for line in lines.take_while(|l| l.trim() != "---") {
        let field = |key: &str| {
            line.strip_prefix(key)
                .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_owned())
        };
        if let Some(v) = field("name:") {
            name = Some(v);
        } else if let Some(v) = field("description:") {
            description = Some(v);
        }
    }
    (name, description)
}

#[cfg(test)]
mod tests {
    use super::frontmatter;

    #[test]
    fn frontmatter_dengan_kutip_dan_tanpa_frontmatter() {
        assert_eq!(
            frontmatter("---\nname: ce-plan\ndescription: \"Buat plan\"\n---\nisi"),
            (Some("ce-plan".to_owned()), Some("Buat plan".to_owned()))
        );
        assert_eq!(frontmatter("# Judul saja"), (None, None));
    }
}
