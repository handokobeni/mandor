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
/// Folder yang tidak ikut revisi folder bukan git: isinya dibuat ulang dari
/// lockfile atau sumber yang ikut di-hash (KTD-P4), atau metadata git.
const SKIPPED_DIRS: [&str; 6] = ["node_modules", ".next", "dist", "build", "target", ".git"];
const GIT_PREFIX: &str = "git:";
const FILES_PREFIX: &str = "files:";

impl Git {
    /// Revisi folder: `git:<HEAD>` ditambah `+<hash>` atas isi perubahan
    /// belum di-commit (diff dan berkas untracked); folder bukan git memakai
    /// hash isi semua berkas dan target symlink, tanpa dependensi dan hasil
    /// build.
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
            return Ok(format!("{GIT_PREFIX}{head}"));
        }
        // Isi perubahan, bukan hanya daftar berkas: mengubah lagi berkas yang
        // sudah kotor harus mengubah revisi.
        let diff = self
            .ok(
                "diff HEAD (prototype)",
                self.git(dir)
                    .args(["--no-optional-locks", "diff", "HEAD", "--binary"]),
            )
            .await?
            .stdout;
        let untracked: Vec<String> = self
            .ok(
                "ls-files --others (prototype)",
                self.git(dir)
                    .args(["ls-files", "--others", "--exclude-standard", "-z"]),
            )
            .await?
            .stdout
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        let root = dir.to_path_buf();
        let task = tokio::task::spawn_blocking(move || {
            let mut hasher = Sha256::new();
            hasher.update(status.as_bytes());
            hasher.update(diff.as_bytes());
            for rel in &untracked {
                hash_entry(&mut hasher, &root, rel)?;
            }
            Ok::<_, GitError>(hex(&hasher.finalize()[..8]))
        });
        let dirty = match tokio::time::timeout(HASH_TIMEOUT, task).await {
            Ok(Ok(result)) => result?,
            Ok(Err(e)) => return Err(GitError::Io(format!("hash task failed: {e}"))),
            Err(_) => return Err(GitError::Io("hashing the folder took too long".to_owned())),
        };
        Ok(format!("{GIT_PREFIX}{head}+{dirty}"))
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
        hash_entry(&mut hasher, root, rel)?;
    }
    Ok(format!("{FILES_PREFIX}{}", hex(&hasher.finalize())))
}

/// Path, jenis, dan isi satu entri: berkas biasa dengan isinya, symlink
/// dengan targetnya (tidak diikuti).
fn hash_entry(hasher: &mut Sha256, root: &Path, rel: &str) -> Result<(), GitError> {
    let path = root.join(rel);
    let meta = std::fs::symlink_metadata(&path).map_err(|e| GitError::Io(e.to_string()))?;
    hasher.update(rel.as_bytes());
    hasher.update([0]);
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&path).map_err(|e| GitError::Io(e.to_string()))?;
        hasher.update(b"link:");
        hasher.update(target.to_string_lossy().as_bytes());
    } else if meta.is_file() {
        let bytes = std::fs::read(&path).map_err(|e| GitError::Io(e.to_string()))?;
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    hasher.update([0]);
    Ok(())
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), GitError> {
    let entries = std::fs::read_dir(dir).map_err(|e| GitError::Io(e.to_string()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                collect(root, &path, out)?;
            }
        } else if kind.is_file() || kind.is_symlink() {
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
        std::fs::write(repo.join("page.tsx"), "c").unwrap();
        let dirtier = g.folder_revision(&repo).await.unwrap();
        assert_ne!(dirtier, dirty, "isi berkas yang sudah kotor ikut dihitung");
        std::fs::write(repo.join("lepas.js"), "1").unwrap();
        let untracked = g.folder_revision(&repo).await.unwrap();
        std::fs::write(repo.join("lepas.js"), "2").unwrap();
        assert_ne!(
            g.folder_revision(&repo).await.unwrap(),
            untracked,
            "isi untracked ikut"
        );
        std::fs::remove_file(repo.join("lepas.js")).unwrap();
        std::fs::write(repo.join("page.tsx"), "b").unwrap();

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
        std::fs::write(root.join(".env.example"), "A=1").unwrap();
        let dot = g.folder_revision(&root).await.unwrap();
        assert_ne!(dot, a, "dotfile ikut dihitung");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/hosts", root.join("src/link")).unwrap();
            let linked = g.folder_revision(&root).await.unwrap();
            assert_ne!(linked, dot, "symlink dicatat dengan targetnya");
            std::fs::remove_file(root.join("src/link")).unwrap();
            std::os::unix::fs::symlink("/etc/passwd", root.join("src/link")).unwrap();
            assert_ne!(g.folder_revision(&root).await.unwrap(), linked);
        }
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
