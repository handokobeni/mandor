//! Revisi folder prototype yang bisa dijalankan (plan 2026-11-03-002,
//! KTD-P4): dipakai untuk konfirmasi dev sebelum kode prototype dijalankan.
//! Hanya membaca; tidak menulis objek atau index git.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::{Git, GitError};

/// Batas waktu menghitung revisi folder bukan git.
const HASH_TIMEOUT: Duration = Duration::from_secs(30);
/// Batas jumlah berkas folder bukan git.
const MAX_FILES: usize = 20_000;
/// Folder hasil build dan dependensi yang tidak ikut revisi.
const SKIPPED_DIRS: [&str; 4] = ["node_modules", "dist", "build", "target"];
const GIT_PREFIX: &str = "git:";
const FILES_PREFIX: &str = "files:";

impl Git {
    /// Revisi folder: `git:<HEAD>` ditambah `+<hash status>` bila ada
    /// perubahan belum di-commit; folder bukan git memakai hash isi berkas
    /// tanpa folder tersembunyi, dependensi, dan hasil build.
    ///
    /// # Errors
    /// [`GitError`] bila git gagal, folder tidak terbaca, atau terlalu besar.
    pub async fn folder_revision(&self, dir: &Path) -> Result<String, GitError> {
        if !is_git_dir(dir).await {
            return content_revision(dir.to_path_buf()).await;
        }
        let head = self
            .ok("rev-parse HEAD", self.git(dir).args(["rev-parse", "HEAD"]))
            .await?
            .stdout
            .trim()
            .to_owned();
        let status = self.porcelain(dir).await?;
        if status.is_empty() {
            Ok(format!("{GIT_PREFIX}{head}"))
        } else {
            Ok(format!(
                "{GIT_PREFIX}{head}+{}",
                short_hash(status.as_bytes())
            ))
        }
    }

    /// Berkas yang berubah sejak revisi `from` (commit baru dan perubahan
    /// belum di-commit). Kosong untuk folder bukan git.
    ///
    /// # Errors
    /// [`GitError`] bila git gagal.
    pub async fn folder_changes(&self, dir: &Path, from: &str) -> Result<Vec<String>, GitError> {
        let Some(base) = from.strip_prefix(GIT_PREFIX) else {
            return Ok(Vec::new());
        };
        let base = base.split('+').next().unwrap_or_default();
        let mut files: Vec<String> = self
            .ok(
                "diff --name-only (prototype)",
                self.git(dir)
                    .args(["diff", "--name-only", base, "HEAD", "--"]),
            )
            .await?
            .stdout
            .lines()
            .map(str::to_owned)
            .collect();
        files.extend(
            self.porcelain(dir)
                .await?
                .lines()
                .filter_map(|l| l.get(3..))
                .map(str::to_owned),
        );
        files.sort();
        files.dedup();
        Ok(files)
    }

    async fn porcelain(&self, dir: &Path) -> Result<String, GitError> {
        Ok(self
            .ok(
                "status (prototype)",
                self.git(dir).args([
                    "--no-optional-locks",
                    "status",
                    "--porcelain=v1",
                    "--untracked-files=all",
                ]),
            )
            .await?
            .stdout)
    }
}

async fn is_git_dir(dir: &Path) -> bool {
    tokio::time::timeout(
        Duration::from_secs(5),
        tokio::fs::metadata(dir.join(".git")),
    )
    .await
    .is_ok_and(|m| m.is_ok())
}

fn short_hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes)[..8])
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

async fn content_revision(dir: PathBuf) -> Result<String, GitError> {
    let task = tokio::task::spawn_blocking(move || hash_folder(&dir));
    match tokio::time::timeout(HASH_TIMEOUT, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => Err(GitError::Io(format!("hash task failed: {e}"))),
        Err(_) => Err(GitError::Io("hashing the folder took too long".to_owned())),
    }
}

fn hash_folder(root: &Path) -> Result<String, GitError> {
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in &files {
        let bytes = std::fs::read(root.join(rel)).map_err(|e| GitError::Io(e.to_string()))?;
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    Ok(format!("{FILES_PREFIX}{}", hex(&hasher.finalize())))
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), GitError> {
    let entries = std::fs::read_dir(dir).map_err(|e| GitError::Io(e.to_string()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                collect(root, &path, out)?;
            }
        } else if kind.is_file() {
            if out.len() >= MAX_FILES {
                return Err(GitError::Io(format!(
                    "the folder has more than {MAX_FILES} files"
                )));
            }
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::GitConfig;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    fn git(dir: &Path) -> Git {
        Git::new(GitConfig::new(dir.join("ws")))
    }

    #[tokio::test]
    async fn revisi_repo_git_berubah_saat_commit_atau_perubahan_belum_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("proto");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "prototype"]);
        std::fs::write(repo.join(".gitignore"), "node_modules/\n").unwrap();
        std::fs::write(repo.join("page.tsx"), "a").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-qm", "awal"]);
        let g = git(dir.path());

        let a = g.folder_revision(&repo).await.unwrap();
        assert!(a.starts_with("git:") && !a.contains('+'), "{a}");
        std::fs::create_dir_all(repo.join("node_modules/x")).unwrap();
        std::fs::write(repo.join("node_modules/x/i.js"), "1").unwrap();
        assert_eq!(
            g.folder_revision(&repo).await.unwrap(),
            a,
            "dependensi diabaikan"
        );

        std::fs::write(repo.join("page.tsx"), "b").unwrap();
        let dirty = g.folder_revision(&repo).await.unwrap();
        assert!(dirty.starts_with(&a) && dirty.contains('+'), "{dirty}");

        sh(&repo, &["commit", "-qam", "ubah"]);
        std::fs::write(repo.join("baru.tsx"), "c").unwrap();
        let b = g.folder_revision(&repo).await.unwrap();
        assert_ne!(b, a);
        assert_eq!(
            g.folder_changes(&repo, &a).await.unwrap(),
            ["baru.tsx", "page.tsx"]
        );
    }

    #[tokio::test]
    async fn revisi_folder_biasa_dari_isi_tanpa_dependensi_dan_build() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proto");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/page.tsx"), "a").unwrap();
        let g = git(dir.path());

        let a = g.folder_revision(&root).await.unwrap();
        assert!(a.starts_with("files:"), "{a}");
        for skipped in [
            "node_modules/x.js",
            ".next/cache",
            "dist/a.js",
            "build/b.js",
        ] {
            let p = root.join(skipped);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        assert_eq!(g.folder_revision(&root).await.unwrap(), a);
        std::fs::write(root.join("src/page.tsx"), "b").unwrap();
        assert_ne!(g.folder_revision(&root).await.unwrap(), a);
        assert!(g.folder_changes(&root, &a).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn folder_yang_tidak_ada_gagal() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            git(dir.path())
                .folder_revision(&dir.path().join("hilang"))
                .await
                .is_err()
        );
    }
}
