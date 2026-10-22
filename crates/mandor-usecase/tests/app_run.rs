//! Run app (U10 Rilis 2): konfirmasi perubahan, satu run pemilik, anggota
//! berurutan, setup saat lockfile berubah, dan pembersihan saat restart.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::app_run::{AppRunState, AppSettings, TestMode};
use mandor_usecase::run_ports::{RepoInfo, WorkspaceInfo};
use mandor_usecase::{PortError, RunError, StartTarget};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    for (id, key) in [("rec1", "PGS-1"), ("rec2", "PGS-2")] {
        let mut t = ready_task(id, &["ou_me"]);
        t.key = Some(key.to_owned());
        t.last_modified_ms = 10 * DAY;
        lark.put(t);
    }
    (lark, clock)
}

fn web(port: u16, setup: Option<&str>) -> AppSettings {
    AppSettings {
        test_mode: TestMode::Web,
        setup_command: setup.map(str::to_owned),
        start_command: Some("bun run dev".to_owned()),
        port: Some(port),
        ..AppSettings::default()
    }
}

fn repo(id: &str, app: AppSettings) -> RepoInfo {
    RepoInfo {
        id: id.to_owned(),
        path: PathBuf::from(format!("/src/{id}")),
        default_branch: "main".to_owned(),
        gh_profile: None,
        companions: vec![],
        trusted_reviewers: vec![],
        extra_allowed: vec![],
        app,
    }
}

/// Station dengan `repo1` yang punya app web di port 5173.
async fn single(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.repos.repos.lock().unwrap().clear();
    st.repos.add(repo("repo1", web(5173, Some("bun install"))));
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| !st.runs.only().state.is_busy()).await;
    let id = st.runs.only().id;
    (st, id)
}

fn confirmed(st: &Station, run_id: &str) -> BTreeMap<String, String> {
    let rt = tokio::runtime::Handle::current();
    let changes = tokio::task::block_in_place(|| rt.block_on(st.svc.app_changes(run_id))).unwrap();
    changes
        .into_iter()
        .map(|c| (c.repo_id, c.snapshot))
        .collect()
}

async fn running(st: &Station, run_id: &str) {
    let snap = confirmed(st, run_id);
    st.svc.app_run(run_id, &snap).await.unwrap();
    eventually(|| {
        st.apps
            .record(run_id)
            .is_some_and(|r| !r.state.is_active() || r.state == AppRunState::Running)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn perubahan_pertama_dibandingkan_dengan_base_dan_manifest_ditandai() {
    let (lark, clock) = setup();
    let (st, run) = single(&lark, &clock).await;
    st.git.st().trees.insert(
        PathBuf::from("/wt/mandor/pgs-1-judul-rec1"),
        "t1".to_owned(),
    );
    st.git.st().diff_files = vec!["package.json".to_owned(), "src/App.svelte".to_owned()];
    let changes = st.svc.app_changes(&run).await.unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].files.len(), 2);
    assert_eq!(changes[0].flagged, ["package.json"]);
    let from = st.git.st().calls_between.lock().unwrap()[0].0.clone();
    assert_eq!(from, "origin/main");
}

#[tokio::test(flavor = "multi_thread")]
async fn app_menyala_setelah_setup_spawn_dan_port_siap() {
    let (lark, clock) = setup();
    let (st, run) = single(&lark, &clock).await;
    st.apps.st.lock().unwrap().lockfile = Some("lock-a".to_owned());
    running(&st, &run).await;
    let rec = st.apps.record(&run).unwrap();
    assert_eq!(rec.state, AppRunState::Running, "{rec:?}");
    assert_eq!(rec.members[0].url.as_deref(), Some("http://localhost:5173"));
    assert!(rec.members[0].pgid.is_some());
    assert_eq!(
        st.apps.calls(),
        [
            format!("stop:{run}"),
            "setup:repo1".to_owned(),
            "spawn:repo1".to_owned(),
            "ready:repo1".to_owned()
        ]
    );
    assert_eq!(
        rec.setup_hashes.get("repo1").map(String::as_str),
        Some("lock-a")
    );

    // Konfirmasi hanya saat menyala; berhenti mereset konfirmasi.
    let rec = st
        .svc
        .app_confirm(&run, Some(true), Some(true))
        .await
        .unwrap();
    assert!(rec.database_confirmed && rec.login_confirmed);
    let rec = st.svc.app_stop(&run).await.unwrap();
    assert_eq!(rec.state, AppRunState::Off);
    assert!(!rec.login_confirmed);
    assert!(matches!(
        st.svc.app_confirm(&run, None, Some(true)).await,
        Err(RunError::InvalidState(_))
    ));

    // Lockfile tidak berubah: setup dilewati pada start berikutnya.
    st.apps.st.lock().unwrap().calls.clear();
    running(&st, &run).await;
    assert!(!st.apps.calls().iter().any(|c| c.starts_with("setup:")));
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_konfirmasi_yang_basi_ditolak() {
    let (lark, clock) = setup();
    let (st, run) = single(&lark, &clock).await;
    let mut snap = confirmed(&st, &run);
    snap.insert("repo1".to_owned(), "tree-lama".to_owned());
    let err = st.svc.app_run(&run, &snap).await.unwrap_err();
    assert!(
        matches!(err, RunError::Port(PortError::Rejected(_))),
        "{err:?}"
    );
    assert!(st.apps.calls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn setup_gagal_atau_port_dipakai_program_lain_membuat_failed() {
    let (lark, clock) = setup();
    let (st, run) = single(&lark, &clock).await;
    st.apps
        .st
        .lock()
        .unwrap()
        .fail_setup
        .insert("repo1".to_owned());
    running(&st, &run).await;
    let rec = st.apps.record(&run).unwrap();
    assert_eq!(rec.state, AppRunState::Failed);
    assert!(rec.error.as_deref().unwrap().contains("setup failed"));
    assert!(!st.apps.calls().iter().any(|c| c.starts_with("spawn:")));

    {
        let mut st_apps = st.apps.st.lock().unwrap();
        st_apps.fail_setup.clear();
        st_apps.foreign_ports.insert(5173);
    }
    running(&st, &run).await;
    let rec = st.apps.record(&run).unwrap();
    assert_eq!(rec.state, AppRunState::Failed);
    assert!(rec.error.as_deref().unwrap().contains("already in use"));
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_r27_anggota_dinyalakan_berurutan_dan_berhenti_saat_gagal() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.repos.add(repo("api", web(8000, None)));
    st.repos.add(repo("web", web(5173, None)));
    st.repos.add_workspace(WorkspaceInfo {
        id: "kv2".to_owned(),
        name: "kv2".to_owned(),
        root: PathBuf::from("/src/kv2"),
        repo_ids: vec!["api".to_owned(), "web".to_owned()],
        mcp_servers: vec![],
    });
    let target = StartTarget::Workspace {
        id: "kv2".to_owned(),
        repo_ids: vec!["api".to_owned(), "web".to_owned()],
    };
    st.svc
        .start_with_progress("rec1", &target, None, |_| {})
        .await
        .unwrap();
    st.agent.last().done();
    eventually(|| !st.runs.only().state.is_busy()).await;
    let run = st.runs.only().id;
    running(&st, &run).await;
    let calls: Vec<String> = st
        .apps
        .calls()
        .into_iter()
        .filter(|c| !c.starts_with("stop:"))
        .collect();
    assert_eq!(calls, ["spawn:api", "ready:api", "spawn:web", "ready:web"]);

    st.svc.app_stop(&run).await.unwrap();
    st.apps.st.lock().unwrap().calls.clear();
    st.apps
        .st
        .lock()
        .unwrap()
        .fail_ready
        .insert("api".to_owned());
    running(&st, &run).await;
    let rec = st.apps.record(&run).unwrap();
    assert_eq!(rec.state, AppRunState::Failed);
    assert!(!st.apps.calls().contains(&"spawn:web".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn app_run_lain_menghentikan_app_sebelumnya() {
    let (lark, clock) = setup();
    let (st, run_a) = single(&lark, &clock).await;
    running(&st, &run_a).await;
    st.svc.start("rec2", "repo1", None).await.unwrap();
    st.agent.last().done();
    let run_b = st
        .runs
        .all()
        .into_iter()
        .find(|r| r.record_id == "rec2")
        .unwrap()
        .id;
    eventually(|| {
        st.runs
            .all()
            .iter()
            .any(|r| r.id == run_b && !r.state.is_busy())
    })
    .await;
    running(&st, &run_b).await;
    assert_eq!(
        st.apps.record(&run_a).unwrap().state,
        AppRunState::StoppedByOtherRun
    );
    assert_eq!(st.apps.record(&run_b).unwrap().state, AppRunState::Running);
    assert!(st.apps.calls().contains(&format!("stop:{run_a}")));
    assert_eq!(
        st.apps.st.lock().unwrap().owner.as_deref(),
        Some(run_b.as_str())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_daemon_membunuh_group_yatim_dan_state_kembali_off() {
    let (lark, clock) = setup();
    let (st, run) = single(&lark, &clock).await;
    running(&st, &run).await;
    let pgid = st.apps.record(&run).unwrap().members[0].pgid.unwrap();
    st.svc.reap_apps().await.unwrap();
    assert_eq!(st.apps.st.lock().unwrap().reaped, [(run.clone(), pgid)]);
    assert_eq!(st.apps.record(&run).unwrap().state, AppRunState::Off);
    assert_eq!(st.apps.st.lock().unwrap().owner, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn repo_tanpa_perintah_start_tidak_bisa_dinyalakan() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.svc.start("rec1", "repo1", None).await.unwrap();
    let run = st.runs.only().id;
    assert!(matches!(
        st.svc.app_changes(&run).await,
        Err(RunError::Port(PortError::Rejected(_)))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn app_ditolak_saat_agent_bekerja_dan_dibatalkan_bila_kode_berubah() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.repos.repos.lock().unwrap().clear();
    st.repos.add(repo("repo1", web(5173, None)));
    st.svc.start("rec1", "repo1", None).await.unwrap();
    let run = st.runs.only().id;
    // Agent masih bekerja (giliran Start belum selesai).
    let snap = confirmed(&st, &run);
    assert!(matches!(
        st.svc.app_run(&run, &snap).await,
        Err(RunError::InvalidState(_))
    ));
    st.agent.last().done();
    eventually(|| !st.runs.only().state.is_busy()).await;

    // Kode berubah setelah konfirmasi tetapi sebelum spawn: dibatalkan.
    let wt = st.runs.only().worktree.unwrap();
    st.apps.st.lock().unwrap().change_tree_on_setup =
        Some((st.git.state.clone(), wt, "tree-baru".to_owned()));
    st.repos.repos.lock().unwrap()[0].app.setup_command = Some("bun install".to_owned());
    running(&st, &run).await;
    let rec = st.apps.record(&run).unwrap();
    assert_eq!(rec.state, AppRunState::Failed);
    assert!(
        rec.error
            .as_deref()
            .unwrap()
            .contains("changed after you confirmed"),
        "{rec:?}"
    );
    assert!(!st.apps.calls().iter().any(|c| c.starts_with("spawn:")));
}
