#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration tests
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

use mandor_git::{ChangeStatus, ChecksState, Git, GitConfig, GitError, PrState};
use pretty_assertions::assert_eq;

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    origin: PathBuf,
    git: Git,
}

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "uji")
        .env("GIT_AUTHOR_EMAIL", "uji@example.com")
        .env("GIT_COMMITTER_NAME", "uji")
        .env("GIT_COMMITTER_EMAIL", "uji@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let origin = root.join("origin.git");
    let repo = root.join("repo");
    sh(
        &root,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    sh(
        &root,
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            repo.to_str().unwrap(),
        ],
    );
    sh(&repo, &["config", "user.name", "uji"]);
    sh(&repo, &["config", "user.email", "uji@example.com"]);
    std::fs::write(repo.join("README.md"), "halo\n").unwrap();
    std::fs::write(repo.join("lama.txt"), "isi lama\n").unwrap();
    sh(&repo, &["add", "-A"]);
    sh(&repo, &["commit", "-q", "-m", "awal"]);
    sh(&repo, &["push", "-q", "origin", "main"]);
    sh(&repo, &["remote", "set-head", "origin", "main"]);
    let git = Git::new(GitConfig::new(root.join("workspaces")));
    Fixture {
        _dir: dir,
        root,
        repo,
        origin,
        git,
    }
}

#[tokio::test]
async fn default_branch_dari_ref_remote() {
    let f = fixture();
    assert_eq!(f.git.default_branch(&f.repo).await.unwrap(), "main");
}

#[tokio::test]
async fn folder_induk_bukan_repo_ditolak_dengan_arahan_ke_workspace() {
    let f = fixture();
    let parent = tempfile::tempdir().unwrap();
    let err = f.git.default_branch(parent.path()).await.unwrap_err();
    assert!(matches!(err, mandor_git::GitError::NotARepo), "{err:?}");
    assert!(err.to_string().contains("Settings › Workspaces"), "{err}");
}

#[tokio::test]
async fn worktree_dibuat_dari_origin_terbaru() {
    let f = fixture();
    let wt = f
        .git
        .create_worktree(&f.repo, "ws1", "mandor/pgs-1", "main", None)
        .await
        .unwrap();
    assert!(wt.path.join("README.md").exists());
    assert_eq!(
        sh(&wt.path, &["branch", "--show-current"]).trim(),
        "mandor/pgs-1"
    );
}

#[tokio::test]
async fn gagal_membuat_worktree_tidak_meninggalkan_sisa() {
    let f = fixture();
    sh(&f.repo, &["branch", "sudah-ada"]);
    let err = f
        .git
        .create_worktree(&f.repo, "ws-gagal", "sudah-ada", "main", None)
        .await
        .unwrap_err();
    assert!(matches!(err, GitError::Command { .. }), "{err:?}");
    assert!(!f.root.join("workspaces/ws-gagal").exists());

    let bad = f
        .git
        .create_worktree(&f.repo, "ws-x", "bad..name", "main", None)
        .await;
    assert!(matches!(bad, Err(GitError::InvalidBranch(_))));
}

#[tokio::test]
async fn changed_files_mengenali_rename_biner_untracked_dan_ubah() {
    let f = fixture();
    let wt = f
        .git
        .create_worktree(&f.repo, "ws2", "mandor/pgs-2", "main", None)
        .await
        .unwrap();
    std::fs::write(wt.path.join("README.md"), "halo\ndunia\n").unwrap();
    sh(&wt.path, &["mv", "lama.txt", "baru.txt"]);
    std::fs::write(wt.path.join("gambar.bin"), [0u8, 159, 146, 150, 0, 1, 2]).unwrap();
    sh(&wt.path, &["add", "gambar.bin"]);
    std::fs::write(wt.path.join("catatan.md"), "baru\n").unwrap();

    let files = f.git.changed_files(&wt.path, "main").await.unwrap();
    let find = |p: &str| {
        files
            .iter()
            .find(|c| c.path == p)
            .unwrap_or_else(|| panic!("{p} tidak ada: {files:?}"))
    };
    assert_eq!(find("README.md").status, ChangeStatus::Modified);
    assert_eq!(find("README.md").additions, Some(1));
    assert_eq!(find("baru.txt").status, ChangeStatus::Renamed);
    assert_eq!(find("baru.txt").old_path.as_deref(), Some("lama.txt"));
    assert_eq!(find("gambar.bin").additions, None);
    assert_eq!(find("catatan.md").status, ChangeStatus::Untracked);

    let patch = f
        .git
        .file_patch(&wt.path, "main", "catatan.md")
        .await
        .unwrap();
    assert!(patch.contains("+baru"), "{patch}");
    let unchanged = f
        .git
        .file_patch(&wt.path, "main", "lama-tidak-ada.txt")
        .await;
    assert!(unchanged.is_ok());
}

#[tokio::test]
async fn discard_mengembalikan_berkas_ke_isi_base() {
    let f = fixture();
    let wt = f
        .git
        .create_worktree(&f.repo, "ws-d", "mandor/pgs-d", "main", None)
        .await
        .unwrap();
    std::fs::write(wt.path.join("README.md"), "halo\ndiubah\n").unwrap();
    sh(&wt.path, &["mv", "lama.txt", "baru.txt"]);
    std::fs::write(wt.path.join("tambah.txt"), "dilacak\n").unwrap();
    sh(&wt.path, &["add", "tambah.txt"]);
    // Perubahan yang sudah di-commit di branch juga dikembalikan ke base.
    sh(&wt.path, &["commit", "-q", "-m", "wip"]);
    std::fs::write(wt.path.join("liar.md"), "untracked\n").unwrap();

    let files = f.git.changed_files(&wt.path, "main").await.unwrap();
    for c in &files {
        f.git
            .discard_file(&wt.path, "main", &c.path, c.old_path.as_deref())
            .await
            .unwrap();
    }

    assert_eq!(
        std::fs::read_to_string(wt.path.join("README.md")).unwrap(),
        "halo\n"
    );
    assert!(wt.path.join("lama.txt").exists());
    assert!(!wt.path.join("baru.txt").exists());
    assert!(!wt.path.join("tambah.txt").exists());
    assert!(!wt.path.join("liar.md").exists());
    assert!(
        f.git
            .changed_files(&wt.path, "main")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn commit_dan_push_ke_branch_task_tanpa_menjalankan_hook() {
    let f = fixture();
    let wt = f
        .git
        .create_worktree(&f.repo, "ws3", "mandor/pgs-3", "main", None)
        .await
        .unwrap();
    let marker = f.root.join("hook-dijalankan");
    let hook = format!("#!/bin/sh\nenv > {}\n", marker.display());
    // Hook di .git/hooks repo dan hook lewat core.hooksPath yang ditulis "agent".
    let hooks_dir = f.repo.join(".git/hooks");
    for name in ["pre-commit", "pre-push", "commit-msg"] {
        let p = hooks_dir.join(name);
        std::fs::write(&p, &hook).unwrap();
        sh(
            &f.repo,
            &["update-index", "--chmod=+x", "--add", "README.md"],
        );
        Command::new("chmod").arg("+x").arg(&p).status().unwrap();
    }
    std::fs::create_dir_all(wt.path.join(".husky")).unwrap();
    std::fs::write(wt.path.join(".husky/pre-push"), &hook).unwrap();
    Command::new("chmod")
        .arg("+x")
        .arg(wt.path.join(".husky/pre-push"))
        .status()
        .unwrap();
    sh(&wt.path, &["config", "core.hooksPath", ".husky"]);

    std::fs::write(wt.path.join("fitur.rs"), "fn main() {}\n").unwrap();
    assert!(f.git.is_dirty(&wt.path).await.unwrap());
    let hash = f
        .git
        .commit_all(&wt.path, "feat: fitur\n\nbadan")
        .await
        .unwrap();
    assert!(hash.is_some());
    assert_eq!(
        f.git
            .unpushed_commits(&wt.path, "mandor/pgs-3", "main")
            .await
            .unwrap(),
        1
    );
    f.git.push(&wt.path, "mandor/pgs-3", None).await.unwrap();
    assert!(
        !marker.exists(),
        "hook repo dijalankan oleh git milik daemon"
    );

    let remote_head = sh(&f.origin, &["rev-parse", "mandor/pgs-3"]);
    assert!(!remote_head.trim().is_empty());
    assert_eq!(
        f.git
            .unpushed_commits(&wt.path, "mandor/pgs-3", "main")
            .await
            .unwrap(),
        0
    );
    assert_eq!(f.git.commit_all(&wt.path, "kosong").await.unwrap(), None);
}

#[tokio::test]
async fn push_ke_branch_lain_ditolak_sebelum_git_dipanggil() {
    let f = fixture();
    let wt = f
        .git
        .create_worktree(&f.repo, "ws4", "mandor/pgs-4", "main", None)
        .await
        .unwrap();
    let err = f.git.push(&wt.path, "main", None).await.unwrap_err();
    assert!(matches!(err, GitError::WrongBranch { .. }), "{err:?}");
}

#[tokio::test]
async fn worktree_dipulihkan_dari_branch_remote() {
    let f = fixture();
    let wt = f
        .git
        .create_worktree(&f.repo, "ws5", "mandor/pgs-5", "main", None)
        .await
        .unwrap();
    std::fs::write(wt.path.join("a.txt"), "a\n").unwrap();
    f.git.commit_all(&wt.path, "a").await.unwrap();
    f.git.push(&wt.path, "mandor/pgs-5", None).await.unwrap();
    f.git
        .remove_worktree(&f.repo, &wt.path, Some("mandor/pgs-5"))
        .await;
    assert!(!wt.path.exists());

    let restored = f
        .git
        .restore_worktree(&f.repo, "ws5b", "mandor/pgs-5", None)
        .await
        .unwrap();
    assert!(restored.path.join("a.txt").exists());
}

fn fake_gh(root: &Path, script_body: &str) -> PathBuf {
    let p = root.join("fake-gh");
    std::fs::write(&p, format!("#!/bin/sh\n{script_body}\n")).unwrap();
    Command::new("chmod").arg("+x").arg(&p).status().unwrap();
    p
}

#[tokio::test]
async fn gh_create_pr_dan_view_lewat_biner_palsu() {
    let f = fixture();
    let args_log = f.root.join("gh-args");
    let gh = fake_gh(
        &f.root,
        &format!(
            r#"echo "$@" >> {log}
echo "token=$GH_TOKEN" >> {log}
case "$2" in
  create) cat > /dev/null; echo "https://github.com/o/r/pull/7";;
  view) echo '{{"number":7,"url":"https://github.com/o/r/pull/7","state":"OPEN","statusCheckRollup":[{{"status":"COMPLETED","conclusion":"SUCCESS"}}]}}';;
esac"#,
            log = args_log.display()
        ),
    );
    let mut cfg = GitConfig::new(f.root.join("workspaces"));
    cfg.gh_bin = gh;
    let git = Git::new(cfg);
    let token = secrecy::SecretString::from("ghs_ujiujiujiuji".to_owned());
    let url = git
        .create_pr(
            &f.repo,
            "main",
            "mandor/pgs-6",
            "feat: x",
            "badan",
            Some(&token),
        )
        .await
        .unwrap();
    assert_eq!(url, "https://github.com/o/r/pull/7");
    let pr = git
        .pr_for_branch(&f.repo, "mandor/pgs-6", Some(&token))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pr.number, 7);
    assert_eq!(pr.state, PrState::Open);
    assert_eq!(pr.checks, ChecksState::Passing);
    let log = std::fs::read_to_string(&args_log).unwrap();
    assert!(
        log.contains("pr create --base main --head mandor/pgs-6"),
        "{log}"
    );
    assert!(log.contains("token=ghs_ujiujiujiuji"));
}

#[tokio::test]
async fn gh_membaca_dan_mengganti_badan_pr() {
    let f = fixture();
    let args_log = f.root.join("gh-args");
    let stdin_log = f.root.join("gh-stdin");
    let gh = fake_gh(
        &f.root,
        &format!(
            r#"echo "$@" >> {log}
case "$2" in
  view) printf '%s\n' '{{"body":"Lark task: PGS-1\r\n\r\nCreated by mandor."}}';;
  edit) cat > {stdin};;
esac"#,
            log = args_log.display(),
            stdin = stdin_log.display()
        ),
    );
    let mut cfg = GitConfig::new(f.root.join("workspaces"));
    cfg.gh_bin = gh;
    let git = Git::new(cfg);
    let url = "https://github.com/o/r/pull/7";
    let body = git.pr_body(&f.repo, url, None).await.unwrap();
    assert_eq!(
        body.as_deref(),
        Some("Lark task: PGS-1\r\n\r\nCreated by mandor.")
    );
    git.set_pr_body(&f.repo, url, "## Summary\nBaru", None)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&stdin_log).unwrap(),
        "## Summary\nBaru"
    );
    let log = std::fs::read_to_string(&args_log).unwrap();
    assert!(log.contains(&format!("pr view {url} --json body")), "{log}");
    assert!(
        log.contains(&format!("pr edit {url} --body-file -")),
        "{log}"
    );
}

#[tokio::test]
async fn gh_tanpa_pr_mengembalikan_none() {
    let f = fixture();
    let gh = fake_gh(
        &f.root,
        "echo 'no pull requests found for branch \"x\"' >&2; exit 1",
    );
    let mut cfg = GitConfig::new(f.root.join("workspaces"));
    cfg.gh_bin = gh;
    let git = Git::new(cfg);
    assert_eq!(git.pr_for_branch(&f.repo, "x", None).await.unwrap(), None);
}

#[tokio::test]
async fn port_git_siklus_worktree_perubahan_commit_push() {
    use mandor_git::GitAdapter;
    use mandor_usecase::run_ports::{GitPort, RepoInfo};

    let f = fixture();
    let port = GitAdapter::new(f.git.clone());
    let repo = RepoInfo {
        id: "r1".to_owned(),
        path: f.repo.clone(),
        default_branch: "main".to_owned(),
        gh_profile: None,
        companions: vec![],
        trusted_reviewers: vec![],
        extra_allowed: vec![],
        app: mandor_domain::app_run::AppSettings::default(),
        gate_mode: None,
    };
    let branch = "mandor/pgs-1-uji";
    let wt = port.prepare_worktree(&repo, branch, None).await.unwrap();
    assert!(wt.ends_with("r1-mandor-pgs-1-uji"));
    assert!(port.worktree_exists(&wt).await);
    assert!(port.changes(&repo, &wt, branch).await.unwrap().is_empty());

    std::fs::write(wt.join("README.md"), "halo dunia\n").unwrap();
    std::fs::write(wt.join("baru.rs"), "fn baru() {}\n").unwrap();
    let c = port.changes(&repo, &wt, branch).await.unwrap();
    assert!(c.dirty);
    assert!(c.diff.contains("halo dunia"));
    assert!(c.diff.contains("new file: baru.rs"));
    assert!(c.files.contains(&"baru.rs".to_owned()));

    port.commit_all(&wt, "feat: uji").await.unwrap();
    let c = port.changes(&repo, &wt, branch).await.unwrap();
    assert_eq!((c.dirty, c.unpushed), (false, 1));
    port.push(&repo, &wt, branch).await.unwrap();
    assert!(port.changes(&repo, &wt, branch).await.unwrap().is_empty());
    assert!(sh(&f.origin, &["branch", "--list", branch]).contains("pgs-1-uji"));

    port.discard_worktree(&repo, &wt, branch).await.unwrap();
    assert!(!port.worktree_exists(&wt).await);
    // Dipulihkan dari branch remote.
    let restored = port.restore_worktree(&repo, branch, None).await.unwrap();
    assert!(restored.join("baru.rs").exists());

    // Discard rename lewat port memulihkan path lama juga.
    sh(&restored, &["mv", "lama.txt", "pindah.txt"]);
    port.discard_file(&repo, &restored, "pindah.txt")
        .await
        .unwrap();
    assert!(restored.join("lama.txt").exists());
    assert!(!restored.join("pindah.txt").exists());
}

#[tokio::test]
async fn worktree_workspace_dikumpulkan_di_folder_run_dan_folder_dihapus_saat_tutup() {
    use mandor_git::GitAdapter;
    use mandor_usecase::run_ports::{GitPort, RepoInfo};

    let f = fixture();
    let port = GitAdapter::new(f.git.clone());
    let repo = RepoInfo {
        id: "kelola-api".to_owned(),
        path: f.repo.clone(),
        default_branch: "main".to_owned(),
        gh_profile: None,
        companions: vec![],
        trusted_reviewers: vec![],
        extra_allowed: vec![],
        app: mandor_domain::app_run::AppSettings::default(),
        gate_mode: None,
    };
    let wt = port
        .prepare_worktree(&repo, "mandor/pgs-9", Some("kelola-v2-mandor-pgs-9"))
        .await
        .unwrap();
    let root = f.root.join("workspaces/kelola-v2-mandor-pgs-9");
    assert_eq!(wt, root.join("kelola-api"));
    assert!(wt.join("README.md").exists());

    port.discard_worktree(&repo, &wt, "mandor/pgs-9")
        .await
        .unwrap();
    port.remove_run_root(&root).await;
    assert!(!root.exists());
}

#[tokio::test]
async fn snapshot_tree_stabil_berubah_karena_berkas_baru_dan_tidak_mengubah_index() {
    let f = fixture();
    let a = f.git.snapshot_tree(&f.repo).await.unwrap();
    let b = f.git.snapshot_tree(&f.repo).await.unwrap();
    assert_eq!(a, b, "worktree yang sama, tree yang sama");
    assert_eq!(a.len(), 40);

    let status_before = sh(&f.repo, &["status", "--porcelain"]);
    std::fs::write(f.repo.join("baru.txt"), "isi baru\n").unwrap();
    let c = f.git.snapshot_tree(&f.repo).await.unwrap();
    assert_ne!(a, c, "berkas untracked mengubah tree");
    assert_eq!(
        sh(&f.repo, &["status", "--porcelain"]),
        format!("{status_before}?? baru.txt\n"),
        "index asli tidak disentuh: berkas tetap untracked"
    );
    assert_eq!(
        f.git.changed_between(&f.repo, &a, &c).await.unwrap(),
        ["baru.txt"]
    );

    // Berkas yang diabaikan .gitignore tidak memengaruhi tree.
    std::fs::write(f.repo.join(".git/info/exclude"), "node_modules/\n").unwrap();
    std::fs::create_dir_all(f.repo.join("node_modules")).unwrap();
    std::fs::write(f.repo.join("node_modules/x.js"), "x").unwrap();
    assert_eq!(f.git.snapshot_tree(&f.repo).await.unwrap(), c);
    assert!(
        std::fs::read_dir(f.repo.join(".git")).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("mandor-snapshot")),
        "index sementara dibersihkan"
    );
}

/// Revert commit satu kartu run grup (R13): bersih bila tidak ada commit lain
/// yang bergantung, konflik dibatalkan tanpa mengubah worktree.
#[tokio::test]
async fn revert_commit_bersih_atau_konflik_dibatalkan() {
    let f = fixture();
    let wt = f
        .git
        .create_worktree(&f.repo, "ws-rv", "mandor/pgs-rv", "main", None)
        .await
        .unwrap();
    std::fs::write(wt.path.join("satu.txt"), "satu\n").unwrap();
    let first = f
        .git
        .commit_all(&wt.path, "kartu 1")
        .await
        .unwrap()
        .unwrap();
    std::fs::write(wt.path.join("dua.txt"), "dua\n").unwrap();
    let second = f
        .git
        .commit_all(&wt.path, "kartu 2")
        .await
        .unwrap()
        .unwrap();

    assert!(f.git.revert_commit(&wt.path, &first).await.unwrap());
    assert!(!wt.path.join("satu.txt").exists());
    assert!(wt.path.join("dua.txt").exists());
    assert!(!f.git.is_dirty(&wt.path).await.unwrap());

    // Kartu 3 mengubah berkas kartu 2: revert kartu 2 konflik.
    std::fs::write(wt.path.join("dua.txt"), "dua diubah kartu 3\n").unwrap();
    f.git.commit_all(&wt.path, "kartu 3").await.unwrap();
    let head = sh(&wt.path, &["rev-parse", "HEAD"]);
    assert!(!f.git.revert_commit(&wt.path, &second).await.unwrap());
    assert_eq!(
        sh(&wt.path, &["rev-parse", "HEAD"]),
        head,
        "tidak ada commit revert"
    );
    assert!(
        !f.git.is_dirty(&wt.path).await.unwrap(),
        "revert dibatalkan bersih"
    );
}
