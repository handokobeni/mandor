use std::path::Path;

use serde::Serialize;

use crate::{Git, GitError};

/// Jenis perubahan berkas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
}

/// Satu berkas yang berubah dibanding merge-base dengan base branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    /// `None` untuk berkas biner.
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

impl Git {
    async fn merge_base(&self, worktree: &Path, base: &str) -> Result<String, GitError> {
        let remote = format!("origin/{base}");
        let out = self
            .exec(&self.git(worktree).args(["merge-base", &remote, "HEAD"]))
            .await?;
        Ok(if out.success() {
            out.stdout.trim().to_owned()
        } else {
            remote
        })
    }

    /// Berkas yang berubah dibanding merge-base `origin/<base>`, termasuk
    /// perubahan belum di-commit dan berkas untracked.
    ///
    /// # Errors
    /// [`GitError`] bila perintah git gagal.
    pub async fn changed_files(
        &self,
        worktree: &Path,
        base: &str,
    ) -> Result<Vec<ChangedFile>, GitError> {
        let mb = self.merge_base(worktree, base).await?;
        let status = self
            .ok(
                "diff --name-status",
                self.git(worktree)
                    .args(["diff", "--name-status", "-M", "-z", &mb]),
            )
            .await?;
        let numstat = self
            .ok(
                "diff --numstat",
                self.git(worktree)
                    .args(["diff", "--numstat", "-M", "-z", &mb]),
            )
            .await?;
        let untracked = self
            .ok(
                "ls-files",
                self.git(worktree)
                    .args(["ls-files", "--others", "--exclude-standard", "-z"]),
            )
            .await?;
        let mut files = parse_name_status(&status.stdout);
        apply_numstat(&mut files, &parse_numstat(&numstat.stdout));
        for path in untracked.stdout.split('\0').filter(|p| !p.is_empty()) {
            files.push(ChangedFile {
                path: path.to_owned(),
                old_path: None,
                status: ChangeStatus::Untracked,
                additions: None,
                deletions: Some(0),
            });
        }
        Ok(files)
    }

    /// Patch satu berkas terhadap merge-base (kosong untuk berkas tanpa perubahan).
    ///
    /// # Errors
    /// [`GitError`] bila perintah git gagal.
    pub async fn file_patch(
        &self,
        worktree: &Path,
        base: &str,
        path: &str,
    ) -> Result<String, GitError> {
        let mb = self.merge_base(worktree, base).await?;
        let out = self
            .ok(
                "diff",
                self.git(worktree).args(["diff", "-M", &mb, "--", path]),
            )
            .await?;
        if !out.stdout.is_empty() {
            return Ok(out.stdout);
        }
        // Berkas untracked: tampilkan sebagai berkas baru tanpa memalsukan diff berkas terlacak.
        let tracked = self
            .exec(
                &self
                    .git(worktree)
                    .args(["ls-files", "--error-unmatch", "--", path]),
            )
            .await?;
        if tracked.success() {
            return Ok(String::new());
        }
        let out = self
            .exec(
                &self
                    .git(worktree)
                    .args(["diff", "--no-index", "--", null_path(), path]),
            )
            .await?;
        Ok(out.stdout)
    }

    /// Mengembalikan satu berkas ke isinya di merge-base `origin/<base>`
    /// ("Discard changes"). Berkas yang tidak ada di base dihapus; untuk rename,
    /// `old_path` ikut dipulihkan. Perubahan ini belum di-commit.
    ///
    /// # Errors
    /// [`GitError`] bila perintah git gagal.
    pub async fn discard_file(
        &self,
        worktree: &Path,
        base: &str,
        path: &str,
        old_path: Option<&str>,
    ) -> Result<(), GitError> {
        let mb = self.merge_base(worktree, base).await?;
        for p in std::iter::once(path).chain(old_path) {
            self.restore_to(worktree, &mb, p).await?;
        }
        Ok(())
    }

    async fn restore_to(&self, worktree: &Path, mb: &str, path: &str) -> Result<(), GitError> {
        let spec = format!("{mb}:{path}");
        let in_base = self
            .exec(&self.git(worktree).args(["cat-file", "-e", &spec]))
            .await?
            .success();
        if in_base {
            self.ok(
                "checkout",
                self.git(worktree).args(["checkout", mb, "--", path]),
            )
            .await?;
            return Ok(());
        }
        let tracked = self
            .exec(
                &self
                    .git(worktree)
                    .args(["ls-files", "--error-unmatch", "--", path]),
            )
            .await?
            .success();
        if tracked {
            self.ok(
                "rm",
                self.git(worktree).args(["rm", "-q", "-f", "--", path]),
            )
            .await?;
        } else {
            self.ok(
                "clean",
                self.git(worktree).args(["clean", "-q", "-f", "--", path]),
            )
            .await?;
        }
        Ok(())
    }

    /// Diff perubahan belum di-commit terhadap HEAD ditambah nama berkas
    /// untracked, dibatasi `max` karakter (untuk pembuat pesan commit).
    ///
    /// # Errors
    /// [`GitError`] bila perintah git gagal.
    pub async fn working_diff(&self, worktree: &Path, max: usize) -> Result<String, GitError> {
        let diff = self
            .ok("diff HEAD", self.git(worktree).args(["diff", "HEAD", "-M"]))
            .await?;
        let untracked = self
            .ok(
                "ls-files",
                self.git(worktree)
                    .args(["ls-files", "--others", "--exclude-standard", "-z"]),
            )
            .await?;
        let mut out = diff.stdout;
        for path in untracked.stdout.split('\0').filter(|p| !p.is_empty()) {
            out.push_str("\nnew file: ");
            out.push_str(path);
        }
        if out.chars().count() > max {
            let stat = self
                .ok(
                    "diff --stat",
                    self.git(worktree).args(["diff", "HEAD", "--stat"]),
                )
                .await?;
            out = format!("[Diff terlalu besar; ringkasan:]\n{}", stat.stdout);
        }
        Ok(out)
    }

    /// Diff commit branch terhadap merge-base `origin/<base>`, dibatasi `max`
    /// karakter (ringkasan `--stat` bila terlalu besar).
    ///
    /// # Errors
    /// [`GitError`] bila perintah git gagal.
    pub async fn branch_diff(
        &self,
        worktree: &Path,
        base: &str,
        max: usize,
    ) -> Result<String, GitError> {
        let range = format!("origin/{base}...HEAD");
        let out = self
            .ok(
                "diff branch",
                self.git(worktree).args(["diff", "-M", &range]),
            )
            .await?;
        if out.stdout.chars().count() <= max {
            return Ok(out.stdout);
        }
        let stat = self
            .ok(
                "diff --stat",
                self.git(worktree).args(["diff", "--stat", &range]),
            )
            .await?;
        Ok(format!("[Diff terlalu besar; ringkasan:]\n{}", stat.stdout))
    }

    /// `true` bila worktree punya perubahan yang belum di-commit.
    ///
    /// # Errors
    /// [`GitError`] bila `git status` gagal.
    pub async fn is_dirty(&self, worktree: &Path) -> Result<bool, GitError> {
        let out = self
            .ok("status", self.git(worktree).args(["status", "--porcelain"]))
            .await?;
        Ok(!out.stdout.trim().is_empty())
    }

    /// Jumlah commit lokal yang belum ada di `origin/<branch>`
    /// (semua commit dihitung bila branch remote belum ada).
    ///
    /// # Errors
    /// [`GitError`] bila `git rev-list` gagal.
    pub async fn unpushed_commits(
        &self,
        worktree: &Path,
        branch: &str,
        base: &str,
    ) -> Result<u32, GitError> {
        let remote = format!("origin/{branch}");
        let has_remote = self
            .exec(
                &self
                    .git(worktree)
                    .args(["rev-parse", "--verify", "--quiet", &remote]),
            )
            .await?
            .success();
        let range = if has_remote {
            format!("{remote}..HEAD")
        } else {
            format!("origin/{base}..HEAD")
        };
        let out = self
            .ok(
                "rev-list",
                self.git(worktree).args(["rev-list", "--count", &range]),
            )
            .await?;
        out.stdout
            .trim()
            .parse()
            .map_err(|_| GitError::Parse(out.stdout.clone()))
    }
}

fn null_path() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

fn status_of(code: char) -> Option<ChangeStatus> {
    Some(match code {
        'A' => ChangeStatus::Added,
        'M' => ChangeStatus::Modified,
        'D' => ChangeStatus::Deleted,
        'R' => ChangeStatus::Renamed,
        'C' => ChangeStatus::Copied,
        'T' => ChangeStatus::TypeChanged,
        _ => return None,
    })
}

/// Mengurai `git diff --name-status -z`.
fn parse_name_status(out: &str) -> Vec<ChangedFile> {
    let mut parts = out.split('\0').filter(|p| !p.is_empty());
    let mut files = Vec::new();
    while let Some(code) = parts.next() {
        let Some(status) = code.chars().next().and_then(status_of) else {
            continue;
        };
        let two_paths = matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied);
        let (old_path, path) = if two_paths {
            let old = parts.next().map(str::to_owned);
            (old, parts.next().map(str::to_owned))
        } else {
            (None, parts.next().map(str::to_owned))
        };
        if let Some(path) = path {
            files.push(ChangedFile {
                path,
                old_path,
                status,
                additions: Some(0),
                deletions: Some(0),
            });
        }
    }
    files
}

/// `(path, additions, deletions)`; `None` untuk biner.
type NumStat = (String, Option<u32>, Option<u32>);

/// Mengurai `git diff --numstat -z` (rename memakai dua path setelah tab kosong).
fn parse_numstat(out: &str) -> Vec<NumStat> {
    let mut parts = out.split('\0').filter(|p| !p.is_empty()).peekable();
    let mut stats = Vec::new();
    while let Some(head) = parts.next() {
        let mut fields = head.splitn(3, '\t');
        let adds = fields.next().unwrap_or_default();
        let dels = fields.next().unwrap_or_default();
        let path = fields.next().unwrap_or_default();
        let path = if path.is_empty() {
            // Rename: path lama lalu path baru sebagai entri berikutnya.
            let _old = parts.next();
            parts.next().unwrap_or_default().to_owned()
        } else {
            path.to_owned()
        };
        stats.push((path, adds.parse().ok(), dels.parse().ok()));
    }
    stats
}

fn apply_numstat(files: &mut [ChangedFile], stats: &[NumStat]) {
    for f in files {
        if let Some((_, a, d)) = stats.iter().find(|(p, _, _)| *p == f.path) {
            f.additions = *a;
            f.deletions = *d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn mengurai_name_status_dengan_rename() {
        let out = "M\0src/a.rs\0R100\0old.rs\0new.rs\0D\0gone.rs\0";
        let files = parse_name_status(out);
        assert_eq!(files.len(), 3);
        assert_eq!(files[1].status, ChangeStatus::Renamed);
        assert_eq!(files[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[1].path, "new.rs");
        assert_eq!(files[2].status, ChangeStatus::Deleted);
    }

    #[test]
    fn mengurai_numstat_biner_dan_rename() {
        let out = "3\t1\tsrc/a.rs\0-\t-\timg.png\0".to_owned() + "0\t0\t\0old.rs\0new.rs\0";
        let stats = parse_numstat(&out);
        assert_eq!(stats[0], ("src/a.rs".to_owned(), Some(3), Some(1)));
        assert_eq!(stats[1], ("img.png".to_owned(), None, None));
        assert_eq!(stats[2].0, "new.rs");
    }
}
