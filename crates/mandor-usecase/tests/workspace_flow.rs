//! Run workspace multi-repo: Start, Publish banyak PR, kegagalan sebagian, Close.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{RunState, TaskStatus};
use mandor_usecase::run_ports::{RepoInfo, WorkChanges, WorkspaceInfo};
use mandor_usecase::{RunError, StartTarget};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;
const ROOT: &str = "/wt/kelola-v2-mandor-pgs-9-job";

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec9", &["ou_me"]);
    t.key = Some("PGS-9".to_owned());
    "Job".clone_into(&mut t.title);
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    (lark, clock)
}

fn repo(id: &str, allowed: &str) -> RepoInfo {
    RepoInfo {
        id: id.to_owned(),
        path: PathBuf::from(format!("/src/{id}")),
        default_branch: "main".to_owned(),
        gh_profile: None,
        companions: vec![],
        trusted_reviewers: vec![],
        extra_allowed: vec![allowed.to_owned()],
        app: mandor_domain::app_run::AppSettings::default(),
        gate_mode: None,
    }
}

fn with_workspace(st: &Station) {
    st.repos.add(repo("kelola-api", "Bash(cargo *)"));
    st.repos.add(repo("kelola-web", "Bash(pnpm *)"));
    st.repos.add(repo("kelola-app", "Bash(flutter *)"));
    st.repos.add_workspace(WorkspaceInfo {
        id: "kelola-v2".to_owned(),
        name: "kelola-v2".to_owned(),
        root: PathBuf::from("/src/kelola-v2"),
        repo_ids: vec![
            "kelola-api".to_owned(),
            "kelola-web".to_owned(),
            "kelola-app".to_owned(),
        ],
        mcp_servers: vec!["graft".to_owned()],
        browser_tests: true,
        gate_mode: None,
    });
}

fn target() -> StartTarget {
    StartTarget::Workspace {
        id: "kelola-v2".to_owned(),
        repo_ids: vec!["kelola-api".to_owned(), "kelola-web".to_owned()],
    }
}

fn wt(repo: &str) -> PathBuf {
    PathBuf::from(format!("{ROOT}/{repo}"))
}

fn changed(st: &Station, repo: &str) {
    st.git.set_changes(
        &wt(repo),
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec![format!("src/{repo}.rs")],
            diff: format!("+{repo}"),
        },
    );
}

async fn started(lark: &FakeLark, clock: &FakeClock) -> Station {
    let st = station(lark, clock, "ou_me");
    with_workspace(&st);
    st.svc
        .start_with_progress("rec9", &target(), None, |_| {})
        .await
        .unwrap();
    st
}

#[tokio::test]
async fn start_workspace_membuat_worktree_tiap_repo_terpilih_di_satu_folder() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;

    let run = st.runs.only();
    assert_eq!(run.worktree, Some(wt("kelola-api")));
    let ws = run.workspace.clone().unwrap();
    assert_eq!(
        (ws.id.as_str(), ws.root.clone()),
        ("kelola-v2", PathBuf::from(ROOT))
    );
    assert_eq!(run.extra.len(), 1);
    assert_eq!(run.extra[0].repo_id, "kelola-web");
    assert_eq!(run.extra[0].worktree, wt("kelola-web"));
    // Repo yang tidak dicentang tidak dibuatkan worktree.
    assert_eq!(st.git.st().prepared.len(), 2);

    let req = st.agent.last().req;
    assert_eq!(req.worktree, PathBuf::from(ROOT));
    let ctx = req.workspace.unwrap();
    assert_eq!(ctx.source, PathBuf::from("/src/kelola-v2"));
    assert_eq!(ctx.mcp_servers, ["graft"]);
    let repos: Vec<_> = ctx
        .repos
        .iter()
        .map(|(r, p)| (r.id.as_str(), p.clone()))
        .collect();
    assert_eq!(
        repos,
        [
            ("kelola-api", wt("kelola-api")),
            ("kelola-web", wt("kelola-web"))
        ]
    );
}

#[tokio::test]
async fn gagal_membuat_worktree_kedua_merollback_yang_pertama() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    with_workspace(&st);
    st.git.st().fail_prepare.push("kelola-web".to_owned());
    let err = st
        .svc
        .start_with_progress("rec9", &target(), None, |_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, RunError::Port(_)), "{err:?}");
    assert!(st.runs.all().is_empty());
    assert_eq!(
        st.git.st().discarded.len(),
        1,
        "worktree kelola-api dihapus"
    );
    assert!(st.git.st().removed_roots.contains(&PathBuf::from(ROOT)));
    assert!(lark.get("rec9").unwrap().claim.is_none(), "klaim dilepas");
}

#[tokio::test]
async fn workspace_tidak_dikenal_atau_repo_bukan_anggota_ditolak() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    with_workspace(&st);
    let bad = StartTarget::Workspace {
        id: "kelola-v2".to_owned(),
        repo_ids: vec!["repo1".to_owned()],
    };
    assert_eq!(
        st.svc
            .start_with_progress("rec9", &bad, None, |_| {})
            .await
            .unwrap_err(),
        RunError::InvalidState("repo is not in the workspace")
    );
    let none = StartTarget::Workspace {
        id: "x".to_owned(),
        repo_ids: vec![],
    };
    assert_eq!(
        st.svc
            .start_with_progress("rec9", &none, None, |_| {})
            .await
            .unwrap_err(),
        RunError::NotFound("workspace")
    );
}

#[tokio::test]
async fn publish_satu_pr_per_repo_berubah_lalu_waiting_review_sekali() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.svc.sync().drain_outbox(10).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    changed(&st, "kelola-api");
    changed(&st, "kelola-web");

    let url = st.svc.publish(&st.runs.only().id).await.unwrap();
    assert_eq!(
        url,
        "https://github.com/o/kelola-api/pull/1\nhttps://github.com/o/kelola-web/pull/2"
    );
    assert_eq!(st.git.st().pushed_wts, [wt("kelola-api"), wt("kelola-web")]);
    let run = st.runs.only();
    assert_eq!(run.state, RunState::InReviewLoop);
    assert_eq!(
        run.pr_url.as_deref(),
        Some("https://github.com/o/kelola-api/pull/1")
    );
    assert_eq!(
        run.extra[0].pr_url.as_deref(),
        Some("https://github.com/o/kelola-web/pull/2")
    );
    st.svc.sync().drain_outbox(10).await.unwrap();
    let rec = lark.get("rec9").unwrap();
    assert_eq!(rec.status, TaskStatus::WaitingReview);
    assert_eq!(rec.pr_url.as_deref(), Some(url.as_str()));
}

#[tokio::test]
async fn repo_tanpa_perubahan_dilewati_saat_publish() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    changed(&st, "kelola-web");
    // kelola-api tidak punya commit di branch task.
    st.git
        .st()
        .branch_diffs
        .insert(wt("kelola-api"), String::new());

    let url = st.svc.publish(&st.runs.only().id).await.unwrap();
    assert_eq!(url, "https://github.com/o/kelola-web/pull/1");
    assert_eq!(st.git.st().pushed_wts, [wt("kelola-web")]);
    assert_eq!(st.runs.only().pr_url, None, "kelola-api tidak dibuatkan PR");
}

#[tokio::test]
async fn pr_gagal_sebagian_tanpa_waiting_review_lalu_retry_melanjutkan() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.svc.sync().drain_outbox(10).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    changed(&st, "kelola-api");
    changed(&st, "kelola-web");
    st.git.st().fail_create_pr.push("kelola-web".to_owned());

    let id = st.runs.only().id;
    st.svc.publish(&id).await.unwrap_err();
    let run = st.runs.only();
    assert_eq!(run.state, RunState::Failed);
    assert_eq!(
        run.pr_url.as_deref(),
        Some("https://github.com/o/kelola-api/pull/1")
    );
    st.svc.sync().drain_outbox(10).await.unwrap();
    assert_eq!(lark.get("rec9").unwrap().status, TaskStatus::InProgress);

    st.git.st().fail_create_pr.clear();
    let url = st.svc.publish(&id).await.unwrap();
    assert_eq!(
        url,
        "https://github.com/o/kelola-api/pull/1\nhttps://github.com/o/kelola-web/pull/2"
    );
    assert_eq!(st.git.st().created_prs.len(), 2, "PR pertama dipakai lagi");
    st.svc.sync().drain_outbox(10).await.unwrap();
    assert_eq!(lark.get("rec9").unwrap().status, TaskStatus::WaitingReview);
}

#[tokio::test]
async fn tutup_run_workspace_menghapus_worktree_bersih_dan_folder_run() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.svc.close_run(&st.runs.only().id).await.unwrap();
    assert_eq!(st.git.st().discarded.len(), 2);
    assert!(st.git.st().removed_roots.contains(&PathBuf::from(ROOT)));
}

#[tokio::test]
async fn discard_berkas_memakai_awalan_nama_repo() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    changed(&st, "kelola-web");
    let id = st.runs.only().id;
    st.svc
        .discard_file(&id, "kelola-web/src/kelola-web.rs")
        .await
        .unwrap();
    assert_eq!(st.git.st().reverted, ["src/kelola-web.rs"]);
    assert_eq!(
        st.svc
            .discard_file(&id, "kelola-api/src/kelola-web.rs")
            .await
            .unwrap_err(),
        RunError::NotFound("changed file")
    );
}

#[tokio::test]
async fn pesan_lanjutan_memulihkan_worktree_anggota_di_folder_run_yang_sama() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.git.st().worktrees.remove(&wt("kelola-web"));

    let id = st.runs.only().id;
    st.svc.send_message(&id, "lanjut").await.unwrap();
    assert_eq!(st.git.st().restored.len(), 1);
    assert!(st.git.st().worktrees.contains(&wt("kelola-web")));
    let req = st.agent.last().req;
    assert_eq!(req.worktree, PathBuf::from(ROOT));
    assert_eq!(req.workspace.unwrap().repos.len(), 2);
    assert!(matches!(
        req.session,
        mandor_usecase::run_ports::SessionRef::Resume(_)
    ));
}

#[tokio::test]
async fn workspace_dihapus_saat_run_berjalan_tetap_lanjut_tanpa_mcp() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.repos.workspaces.lock().unwrap().clear();

    let id = st.runs.only().id;
    st.svc.send_message(&id, "lanjut").await.unwrap();
    let ctx = st.agent.last().req.workspace.unwrap();
    assert!(
        ctx.mcp_servers.is_empty(),
        "server MCP tidak lagi dipilih siapa pun"
    );
    assert_eq!(ctx.source, PathBuf::from(ROOT));
    assert_eq!(ctx.repos.len(), 2);
}
