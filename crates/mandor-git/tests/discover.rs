//! Saran repo dan penjelajah folder untuk dialog Add repo.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;
use std::path::Path;

use mandor_git::{discover_repos, list_dirs};
use pretty_assertions::assert_eq;

fn repo(dir: &Path, origin: Option<&str>) {
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    let mut cfg = "[core]\n\tbare = false\n".to_owned();
    if let Some(url) = origin {
        let _ = write!(
            cfg,
            "[remote \"origin\"]\n\turl = {url}\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n"
        );
    }
    std::fs::write(dir.join(".git/config"), cfg).unwrap();
}

#[tokio::test]
async fn saran_repo_dengan_origin_dan_folder_yang_dilewati() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    repo(
        &h.join("Work/pegasus-web"),
        Some("git@github.com:talentlytica/pegasus-web.git"),
    );
    repo(&h.join("Work/tanpa-origin"), None);
    repo(&h.join("Work/a/b/terlalu-dalam"), Some("https://x/y.git"));
    repo(&h.join(".hidden/rahasia"), Some("https://x/y.git"));
    repo(&h.join("Work/node_modules/dep"), Some("https://x/y.git"));
    repo(&h.join(".mandor/workspaces/wt"), Some("https://x/y.git"));
    // Repo di dalam repo tidak ditelusuri.
    repo(
        &h.join("Work/pegasus-web/vendor/sub"),
        Some("https://x/y.git"),
    );
    // Worktree git (`.git` berupa berkas) bukan repo utama.
    std::fs::create_dir_all(h.join("Work/wt-lain")).unwrap();
    std::fs::write(h.join("Work/wt-lain/.git"), "gitdir: /x").unwrap();

    let found = discover_repos(&[h.to_path_buf()], &[h.join(".mandor")], 3, 50).await;
    let names: Vec<_> = found.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["pegasus-web"]);
    assert_eq!(found[0].path, h.join("Work/pegasus-web"));
    assert_eq!(
        found[0].origin.as_deref(),
        Some("git@github.com:talentlytica/pegasus-web.git")
    );

    // Batas jumlah hasil dihormati.
    repo(&h.join("Work/kedua"), Some("https://x/k.git"));
    assert_eq!(discover_repos(&[h.to_path_buf()], &[], 3, 1).await.len(), 1);
}

#[tokio::test]
async fn daftar_folder_hanya_subfolder_terlihat_dengan_penanda_repo() {
    let home = tempfile::tempdir().unwrap();
    let h = home.path();
    repo(&h.join("pegasus-web"), Some("https://x/p.git"));
    std::fs::create_dir_all(h.join("docs")).unwrap();
    std::fs::create_dir_all(h.join(".config")).unwrap();
    std::fs::write(h.join("catatan.txt"), "x").unwrap();

    let listing = list_dirs(h).await.unwrap();
    assert_eq!(listing.path, h);
    assert_eq!(listing.parent.as_deref(), h.parent());
    let entries: Vec<_> = listing
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.is_repo))
        .collect();
    assert_eq!(entries, [("docs", false), ("pegasus-web", true)]);
    assert!(list_dirs(Path::new("relatif/path")).await.is_err());
    assert!(list_dirs(&h.join("tidak-ada")).await.is_err());
}
