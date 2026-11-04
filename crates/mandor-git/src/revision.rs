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
    /// Revisi folder. Repo git: `git:<HEAD>+<hash>` atas isi berkas yang
    /// dilacak dan untracked yang tidak diabaikan, dibaca langsung tanpa
    /// perintah git yang menjalankan filter, diff eksternal, atau textconv
    /// dari konfigurasi repo. Folder bukan git: hash isi semua berkas dan
    /// target symlink, tanpa dependensi dan hasil build.
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
        // `ls-files` hanya membaca index dan pola abaikan; isi berkas tidak
        // melewati filter repo.
        let files = self.listed_files(dir).await?;
        let root = dir.to_path_buf();
        let task = tokio::task::spawn_blocking(move || {
            let mut hasher = Sha256::new();
            for rel in &files {
                hash_entry(&mut hasher, &root, rel)?;
            }
            Ok::<_, GitError>(hex(&hasher.finalize()[..8]))
        });
        let content = match tokio::time::timeout(HASH_TIMEOUT, task).await {
            Ok(Ok(result)) => result?,
            Ok(Err(e)) => return Err(GitError::Io(format!("hash task failed: {e}"))),
            Err(_) => return Err(GitError::Io("hashing the folder took too long".to_owned())),
        };
        Ok(format!("{GIT_PREFIX}{head}+{content}"))
    }

    /// Berkas yang berubah sejak revisi `from`: commit baru ditambah
    /// perubahan yang belum di-commit, dihitung tanpa filter repo dengan
    /// membandingkan blob id isi berkas (`hash-object --no-filters`) terhadap
    /// `HEAD`. Kosong untuk folder bukan git.
    ///
    /// # Errors
    /// [`GitError`] bila git gagal atau folder terlalu besar.
    pub async fn folder_changes(&self, dir: &Path, from: &str) -> Result<Vec<String>, GitError> {
        let Some(base) = from.strip_prefix(GIT_PREFIX) else {
            return Ok(Vec::new());
        };
        let base = base.split('+').next().unwrap_or_default();
        // Diff antar-commit: tidak menyentuh worktree, jadi tidak ada filter.
        let mut files: Vec<String> = self
            .ok(
                "diff --name-only (prototype)",
                self.git(dir).args([
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--name-only",
                    base,
                    "HEAD",
                    "--",
                ]),
            )
            .await?
            .stdout
            .lines()
            .map(str::to_owned)
            .collect();
        files.extend(self.uncommitted(dir).await?);
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// Berkas yang isinya berbeda dari `HEAD`, baru, atau terhapus.
    async fn uncommitted(&self, dir: &Path) -> Result<Vec<String>, GitError> {
        let head: std::collections::HashMap<String, String> = self
            .ok(
                "ls-tree (prototype)",
                self.git(dir).args(["ls-tree", "-r", "-z", "HEAD"]),
            )
            .await?
            .stdout
            .split('\0')
            .filter_map(|entry| {
                let (meta, path) = entry.split_once('\t')?;
                let blob = meta.split_whitespace().nth(2)?;
                Some((path.to_owned(), blob.to_owned()))
            })
            .collect();
        let present = self.listed_files(dir).await?;
        // Path dengan baris baru tidak bisa lewat --stdin-paths; selalu
        // dianggap berubah.
        let (plain, odd): (Vec<&String>, Vec<&String>) =
            present.iter().partition(|p| !p.contains('\n'));
        let mut changed: Vec<String> = odd.into_iter().cloned().collect();
        if !plain.is_empty() {
            let input = plain.iter().fold(String::new(), |mut acc, p| {
                acc.push_str(p);
                acc.push('\n');
                acc
            });
            let hashes = self
                .ok(
                    "hash-object --no-filters (prototype)",
                    self.git(dir)
                        .args(["hash-object", "--no-filters", "--stdin-paths"])
                        .stdin_bytes(input),
                )
                .await?
                .stdout;
            for (path, hash) in plain.iter().zip(hashes.lines()) {
                if head.get(path.as_str()).map(String::as_str) != Some(hash.trim()) {
                    changed.push((*path).clone());
                }
            }
        }
        changed.extend(head.keys().filter(|p| !present.contains(p)).cloned());
        Ok(changed)
    }

    /// Berkas dilacak dan untracked yang tidak diabaikan; tidak menjalankan
    /// filter repo.
    async fn listed_files(&self, dir: &Path) -> Result<Vec<String>, GitError> {
        let mut files: Vec<String> = self
            .ok(
                "ls-files (prototype)",
                self.git(dir).args([
                    "ls-files",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "-z",
                ]),
            )
            .await?
            .stdout
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        if files.len() > MAX_FILES {
            return Err(GitError::Io(format!(
                "the folder has more than {MAX_FILES} files"
            )));
        }
        files.sort();
        files.dedup();
        // Berkas yang dihapus dari worktree masih tercantum di index.
        let root = dir.to_path_buf();
        let task = tokio::task::spawn_blocking(move || {
            files.retain(|rel| std::fs::symlink_metadata(root.join(rel)).is_ok());
            files
        });
        match tokio::time::timeout(HASH_TIMEOUT, task).await {
            Ok(Ok(files)) => Ok(files),
            Ok(Err(e)) => Err(GitError::Io(format!("listing task failed: {e}"))),
            Err(_) => Err(GitError::Io("listing the folder took too long".to_owned())),
        }
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
        assert!(a.starts_with("git:"), "{a}");
        std::fs::create_dir_all(repo.join("node_modules/x")).unwrap();
        std::fs::write(repo.join("node_modules/x/i.js"), "1").unwrap();
        assert_eq!(
            g.folder_revision(&repo).await.unwrap(),
            a,
            "dependensi diabaikan"
        );

        std::fs::write(repo.join("page.tsx"), "b").unwrap();
        let dirty = g.folder_revision(&repo).await.unwrap();
        assert_ne!(dirty, a, "perubahan belum di-commit mengubah revisi");
        assert_eq!(
            g.folder_changes(&repo, &a).await.unwrap(),
            ["page.tsx"],
            "perubahan belum di-commit dirinci"
        );
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
        std::fs::remove_file(repo.join(".gitignore")).unwrap();
        assert_eq!(
            g.folder_changes(&repo, &a).await.unwrap(),
            [".gitignore", "baru.tsx", "node_modules/x/i.js", "page.tsx"],
            "commit baru, berkas baru, dan berkas terhapus"
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

    #[tokio::test]
    async fn diff_eksternal_dan_textconv_repo_prototype_tidak_dijalankan() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("proto");
        std::fs::create_dir(&repo).unwrap();
        sh(&repo, &["init", "-q", "-b", "prototype"]);
        std::fs::write(repo.join("page.tsx"), "a").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-qm", "awal"]);
        let marker = dir.path().join("dijalankan");
        let cmd = format!("touch {}", marker.display());
        sh(&repo, &["config", "diff.external", &cmd]);
        sh(&repo, &["config", "diff.tx.textconv", &cmd]);
        sh(&repo, &["config", "filter.tx.clean", &cmd]);
        sh(&repo, &["config", "filter.tx.process", &cmd]);
        std::fs::write(repo.join(".gitattributes"), "*.tsx diff=tx filter=tx\n").unwrap();
        std::fs::write(repo.join("page.tsx"), "b").unwrap();
        let g = git(dir.path());
        let a = g.folder_revision(&repo).await.unwrap();
        let from = format!(
            "git:{}",
            a.trim_start_matches("git:").split('+').next().unwrap()
        );
        g.folder_changes(&repo, &from).await.unwrap();
        assert!(
            !marker.exists(),
            "diff eksternal, textconv, dan filter repo tidak boleh jalan"
        );
    }
}
