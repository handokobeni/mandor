//! `RunsStore` dan `ReposStore` sebagai port usecase.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mandor_domain::{AgentEvent, RunState};
use mandor_store::{NewRepo, ReposStore, RunsStore, Store};
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{
    ChatItem, Repos, RunMember, RunPhase, RunRecord, RunStore, RunWorkspace,
};
use pretty_assertions::assert_eq;
use serde_json::json;

async fn open() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("mandor.db")).await.unwrap();
    s.insert_repo(&NewRepo {
        id: "repo1".into(),
        path: "/src/app".into(),
        gh_profile: Some("dev".into()),
        default_branch: Some("develop".into()),
        now_ms: 1,
    })
    .await
    .unwrap();
    (s, dir)
}

fn run(id: &str) -> RunRecord {
    let mut r = RunRecord::new(id.to_owned(), "repo1".to_owned(), "rec1".to_owned());
    r.branch = Some("mandor/pgs-1".to_owned());
    r.worktree = Some("/wt/pgs-1".into());
    r.session_id = Some("sess".to_owned());
    r
}

#[tokio::test]
async fn run_bolak_balik_konflik_generasi_dan_pid_bisa_dikosongkan() {
    let (s, _d) = open().await;
    let runs = RunsStore::new(s);
    let mut r = run("a");
    runs.create(&r, 1).await.unwrap();
    assert_eq!(
        runs.create(&run("b"), 1).await.unwrap_err(),
        PortError::Conflict
    );

    r.generation = 1;
    r.state = RunState::Running;
    r.pid = Some(99);
    r.session_started = true;
    r.queue = vec!["pesan".to_owned()];
    r.automatic = true;
    r.quiet_until_ms = Some(120_000);
    // Run workspace: folder induk dan repo tambahan ikut tersimpan.
    r.workspace = Some(RunWorkspace {
        id: "kelola-v2".to_owned(),
        root: "/wt/kelola-v2-pgs-1".into(),
    });
    r.extra = vec![RunMember {
        repo_id: "repo2".to_owned(),
        worktree: "/wt/kelola-v2-pgs-1/repo2".into(),
        pr_url: Some("https://github.com/o/web/pull/2".to_owned()),
    }];
    // Fase plan dan isi plan ikut tersimpan.
    r.phase = RunPhase::Brainstorm;
    r.plan = Some("## Plan".to_owned());
    r.plan_skill = Some("compound-engineering:ce-plan".to_owned());
    runs.save(&r, 0, 2).await.unwrap();
    // Waktu dibuat dan diubah diisi oleh store.
    (r.created_at_ms, r.updated_at_ms) = (1, 2);
    assert_eq!(runs.get("a").await.unwrap().unwrap(), r);
    // Generasi basi ditolak.
    assert_eq!(runs.save(&r, 0, 3).await.unwrap_err(), PortError::Conflict);

    r.pid = None;
    r.last_error = Some("gagal".to_owned());
    runs.save(&r, 1, 4).await.unwrap();
    let back = runs.active_for("rec1").await.unwrap().unwrap();
    assert_eq!(
        (back.pid, back.last_error.as_deref()),
        (None, Some("gagal"))
    );
    assert_eq!(runs.list_active().await.unwrap().len(), 1);

    runs.close("a", 5).await.unwrap();
    assert!(runs.active_for("rec1").await.unwrap().is_none());
    runs.create(&run("b"), 6).await.unwrap();
}

#[tokio::test]
async fn chat_disimpan_dengan_peran_dan_bentuk_stabil() {
    let (s, _d) = open().await;
    let runs = RunsStore::new(s.clone());
    runs.create(&run("a"), 1).await.unwrap();
    runs.append("a", &ChatItem::Dev("halo".to_owned()), 1)
        .await
        .unwrap();
    runs.append(
        "a",
        &ChatItem::Agent(AgentEvent::ToolUse {
            id: "t1".into(),
            name: "Bash".into(),
            preview: "cargo test".into(),
        }),
        2,
    )
    .await
    .unwrap();
    runs.append("a", &ChatItem::System("dihentikan".to_owned()), 3)
        .await
        .unwrap();
    let msgs = s.list_messages("a", 0, 10).await.unwrap();
    let roles: Vec<_> = msgs.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ["dev", "agent", "system"]);
    assert_eq!(
        msgs[1].body,
        json!({"type": "tool_use", "id": "t1", "name": "Bash", "preview": "cargo test"})
    );
}

#[tokio::test]
async fn repo_dengan_pengaturan_pendamping_dan_reviewer() {
    let (s, _d) = open().await;
    s.set_repo_settings(
        "repo1",
        &json!({"companions": ["/specs"], "trusted_reviewers": ["ou_qa"]}),
    )
    .await
    .unwrap();
    let repos = ReposStore::new(s);
    let r = repos.repo("repo1").await.unwrap().unwrap();
    assert_eq!(r.default_branch, "develop");
    assert_eq!(r.companions, [std::path::PathBuf::from("/specs")]);
    assert_eq!(r.trusted_reviewers, ["ou_qa"]);
    assert!(repos.repo("lain").await.unwrap().is_none());
}
