//! Saran repo dan penjelajah folder di mesin daemon (dialog Add repo).
//! Hanya membaca nama folder dan `.git/config`; semua I/O async.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::GitError;

/// Batas waktu seluruh pemindaian saran repo.
const DISCOVER_TIMEOUT: Duration = Duration::from_secs(5);
/// Batas entri yang dikembalikan penjelajah folder.
const LIST_MAX: usize = 500;
/// Folder yang tidak pernah berisi repo milik dev.
const SKIP_NAMES: &[&str] = &["node_modules", "target", "vendor", "dist", "build"];

/// Repo git dengan remote `origin` yang bisa ditambahkan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoCandidate {
    pub path: PathBuf,
    pub name: String,
    pub origin: Option<String>,
}

/// Satu subfolder di penjelajah.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    /// Berisi `.git` (folder), yaitu clone utama.
    pub is_repo: bool,
}

/// Isi satu folder untuk penjelajah.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirListing {
    pub path: PathBuf,
    pub parent: Option<PathBuf>,
    pub entries: Vec<DirEntry>,
}

/// URL remote `origin` dari isi `.git/config`.
fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line == "[remote \"origin\"]";
            continue;
        }
        if in_origin {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "url" {
                    return Some(value.trim().to_owned());
                }
            }
        }
    }
    None
}

async fn is_main_repo(dir: &Path) -> bool {
    tokio::fs::metadata(dir.join(".git"))
        .await
        .is_ok_and(|m| m.is_dir())
}

fn visible(name: &str) -> bool {
    !name.starts_with('.') && !SKIP_NAMES.contains(&name)
}

/// Subfolder terlihat dari `dir`, urut nama; folder tak terbaca dianggap kosong.
async fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(mut rd) = tokio::fs::read_dir(dir).await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while let Ok(Some(entry)) = rd.next_entry().await {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if visible(&name) && entry.file_type().await.is_ok_and(|t| t.is_dir()) {
            out.push((name, entry.path()));
        }
    }
    out.sort();
    out
}

/// Repo dengan `origin` di bawah `roots` sampai kedalaman `max_depth`. Folder
/// tersembunyi, folder build, dan `skip` dilewati; repo tidak ditelusuri ke
/// dalam. Dibatasi `limit` hasil dan waktu [`DISCOVER_TIMEOUT`].
pub async fn discover_repos(
    roots: &[PathBuf],
    skip: &[PathBuf],
    max_depth: usize,
    limit: usize,
) -> Vec<RepoCandidate> {
    let mut found = Vec::new();
    let walk = async {
        let mut queue: Vec<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
        while let Some((dir, depth)) = (!queue.is_empty()).then(|| queue.remove(0)) {
            for (name, path) in subdirs(&dir).await {
                if found.len() >= limit {
                    return;
                }
                if skip.iter().any(|s| path.starts_with(s)) {
                    continue;
                }
                if is_main_repo(&path).await {
                    let config = tokio::fs::read_to_string(path.join(".git/config"))
                        .await
                        .unwrap_or_default();
                    if let Some(origin) = origin_url(&config) {
                        found.push(RepoCandidate {
                            path,
                            name,
                            origin: Some(origin),
                        });
                    }
                } else if depth + 1 < max_depth {
                    queue.push((path, depth + 1));
                }
            }
        }
    };
    if tokio::time::timeout(DISCOVER_TIMEOUT, walk).await.is_err() {
        tracing::warn!("pemindaian saran repo melewati batas waktu");
    }
    found
}

/// Subfolder terlihat dari `path` (absolut), dengan penanda repo.
///
/// # Errors
/// [`GitError::Io`] bila path relatif atau bukan folder yang bisa dibaca.
pub async fn list_dirs(path: &Path) -> Result<DirListing, GitError> {
    if !path.is_absolute() {
        return Err(GitError::Io(format!(
            "not an absolute path: {}",
            path.display()
        )));
    }
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| GitError::Io(format!("{}: {e}", path.display())))?;
    if !meta.is_dir() {
        return Err(GitError::Io(format!("not a folder: {}", path.display())));
    }
    let mut entries = Vec::new();
    for (name, p) in subdirs(path).await.into_iter().take(LIST_MAX) {
        let is_repo = is_main_repo(&p).await;
        entries.push(DirEntry {
            name,
            path: p,
            is_repo,
        });
    }
    Ok(DirListing {
        path: path.to_path_buf(),
        parent: path.parent().map(Path::to_path_buf),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_origin_dari_config_git() {
        let cfg = "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://u/x.git\n[remote \"origin\"]\n\turl = git@github.com:o/r.git\n";
        assert_eq!(origin_url(cfg).as_deref(), Some("git@github.com:o/r.git"));
        assert_eq!(origin_url("[core]\n"), None);
    }
}
