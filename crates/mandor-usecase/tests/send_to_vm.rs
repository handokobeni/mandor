//! "Approve and queue for VM" di daemon laptop (U3 plan 2026-10-30-001):
//! commit dan push plan, paket plan, lalu klaim pindah ke VM.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{Design, RunState};
use mandor_usecase::review::DesignRoot;
use mandor_usecase::run_ports::{RepoInfo, RunPhase, WorkspaceInfo};
use mandor_usecase::{AcItem, Planning, StartTarget};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;
const PLAN: &str = "## Plan\n1. Tambah tombol";

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Tab Data".clone_into(&mut t.title);
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned()];
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    for id in ["ac1", "ac2"] {
        lark.acs.lock().unwrap().insert(
            id.to_owned(),
            AcItem {
                record_id: id.to_owned(),
                label: Some(format!("AC-{id}")),
                text: format!("teks {id}"),
                ..Default::default()
            },
        );
    }
    (lark, clock)
}

fn mock_design() -> DesignRoot {
    DesignRoot {
        id: "ds1".to_owned(),
        name: "kelola-mock".to_owned(),
        path: "/design/kelola-mock".into(),
    }
}

/// Run plan siap disetujui: ac2 bertentangan dengan desain.
async fn plan_ready(lark: &FakeLark, clock: &FakeClock, target: &StartTarget) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    if let StartTarget::Workspace { .. } = target {
        with_workspace(&st);
    }
    st.svc
        .start_planned(
            "rec1",
            target,
            None,
            &Planning::Plan { skill: None },
            &[mock_design()],
            &|_| {},
        )
        .await
        .unwrap();
    let scope = st
        .svc
        .verify_capability(st.agent.last().req.ledger.unwrap().token.expose())
        .unwrap();
    st.svc
        .ledger_set_design(&scope, "ac2", Design::Conflicts, Some("Header beda"))
        .await
        .unwrap();
    st.agent.last().finish(PLAN);
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let id = st.runs.only().id;
    (st, id)
}

fn repo1() -> StartTarget {
    StartTarget::Repo("repo1".to_owned())
}

fn worktree(st: &Station) -> PathBuf {
    st.runs.only().worktree.unwrap()
}

fn repo(id: &str) -> RepoInfo {
    RepoInfo {
        id: id.to_owned(),
        path: PathBuf::from(format!("/src/{id}")),
        default_branch: "main".to_owned(),
        gh_profile: None,
        companions: vec![],
        trusted_reviewers: vec![],
        extra_allowed: vec![],
        app: mandor_domain::app_run::AppSettings::default(),
        gate_mode: None,
    }
}

fn with_workspace(st: &Station) {
    st.repos.add(repo("api"));
    st.repos.add(repo("web"));
    st.repos.add_workspace(WorkspaceInfo {
        id: "kelola".to_owned(),
        name: "kelola".to_owned(),
        root: PathBuf::from("/src/kelola"),
        repo_ids: vec!["api".to_owned(), "web".to_owned()],
        mcp_servers: vec![],
        browser_tests: false,
        gate_mode: None,
    });
}

#[tokio::test]
async fn covers_ae1_plan_di_commit_dan_push_lalu_paket_berisi_plan_dan_follow_design() {
    let (lark, clock) = setup();
    let (st, run) = plan_ready(&lark, &clock, &repo1()).await;
    let wt = worktree(&st);
    st.git.st().untracked.insert(
        wt.clone(),
        vec!["docs/plans/2026-10-30-001-plan.md".to_owned()],
    );

    let out = st
        .svc
        .prepare_for_vm(&run, &["ac2".to_owned()])
        .await
        .unwrap();

    let git = st.git.st();
    assert_eq!(
        git.commits,
        [(wt.clone(), "PGS-7: approved plan".to_owned())]
    );
    assert_eq!(git.pushes.len(), 1);
    drop(git);
    assert_eq!(out.target, repo1());
    assert_eq!(out.work.plan, PLAN);
    assert_eq!(Some(out.work.branch.clone()), st.runs.only().branch);
    assert_eq!(out.work.laptop_design, ["ds1"]);
    let ac2 = out.work.design.iter().find(|d| d.ac_id == "ac2").unwrap();
    assert_eq!(ac2.design, Design::FollowDesign);
    assert_eq!(ac2.note.as_deref(), Some("Header beda"));

    let run_now = st.runs.only();
    assert_eq!(run_now.state, RunState::AwaitingDev, "run tetap terbuka");
    assert_eq!(run_now.phase, RunPhase::Plan);
    let claim = lark.get("rec1").unwrap().claim.unwrap();
    assert_eq!(claim.daemon_id(), Some("local"), "klaim belum pindah");
}

#[tokio::test]
async fn covers_ae10_berkas_baru_lain_menahan_pengiriman() {
    let (lark, clock) = setup();
    let (st, run) = plan_ready(&lark, &clock, &repo1()).await;
    let wt = worktree(&st);
    st.git.st().untracked.insert(
        wt,
        vec!["docs/plans/p.md".to_owned(), "notes.txt".to_owned()],
    );

    let err = st.svc.prepare_for_vm(&run, &[]).await.unwrap_err();
    assert!(format!("{err}").contains("notes.txt"), "{err}");
    let git = st.git.st();
    assert!(git.commits.is_empty());
    assert!(git.pushes.is_empty());
}

#[tokio::test]
async fn run_workspace_semua_anggota_di_push() {
    let (lark, clock) = setup();
    let target = StartTarget::Workspace {
        id: "kelola".to_owned(),
        repo_ids: vec!["api".to_owned(), "web".to_owned()],
    };
    let (st, run) = plan_ready(&lark, &clock, &target).await;

    let out = st.svc.prepare_for_vm(&run, &[]).await.unwrap();
    assert_eq!(out.target, target);
    assert_eq!(
        st.git.st().pushes.len(),
        2,
        "anggota tanpa perubahan ikut di-push"
    );
}

#[tokio::test]
async fn push_gagal_run_tetap_menunggu_persetujuan() {
    let (lark, clock) = setup();
    let (st, run) = plan_ready(&lark, &clock, &repo1()).await;
    st.git.st().fail_push = true;
    assert!(st.svc.prepare_for_vm(&run, &[]).await.is_err());
    let r = st.runs.only();
    assert_eq!((r.state, r.phase), (RunState::AwaitingDev, RunPhase::Plan));
    let claim = lark.get("rec1").unwrap().claim.unwrap();
    assert_eq!(claim.daemon_id(), Some("local"), "klaim tetap di laptop");
}

#[tokio::test]
async fn hanya_run_plan_yang_menunggu_persetujuan() {
    let (lark, clock) = setup();
    let (st, run) = plan_ready(&lark, &clock, &repo1()).await;
    st.svc.approve_plan(&run, &[]).await.unwrap();
    assert!(
        st.svc.prepare_for_vm(&run, &[]).await.is_err(),
        "fase implement"
    );
}

#[tokio::test]
async fn covers_ae2_tanpa_send_to_vm_run_laptop_masih_bisa_disetujui() {
    let (lark, clock) = setup();
    let (st, run) = plan_ready(&lark, &clock, &repo1()).await;
    st.svc.prepare_for_vm(&run, &[]).await.unwrap();
    st.svc.approve_plan(&run, &[]).await.unwrap();
    assert_eq!(st.runs.only().phase, RunPhase::Implement);
}

#[tokio::test]
async fn send_to_vm_memindah_klaim_dan_menutup_run_dan_bisa_diulang() {
    let (lark, clock) = setup();
    let (st, run) = plan_ready(&lark, &clock, &repo1()).await;
    st.svc.prepare_for_vm(&run, &[]).await.unwrap();
    let branch = st.runs.only().branch.unwrap();

    st.svc.send_to_vm(&run, "vm").await.unwrap();
    let claim = lark.get("rec1").unwrap().claim.unwrap();
    assert_eq!(claim.daemon_id(), Some("vm"));
    assert!(
        st.svc.active_runs().await.unwrap().is_empty(),
        "run ditutup"
    );
    assert_eq!(st.git.st().discarded, [branch], "worktree laptop dibuang");
    assert!(
        st.runs
            .system_messages()
            .iter()
            .any(|m| m.contains("Queued for the VM")),
        "{:?}",
        st.runs.system_messages()
    );
}

#[tokio::test]
async fn send_to_vm_diulang_setelah_klaim_pindah_menutup_run() {
    let (lark, clock) = setup();
    let (st, run) = plan_ready(&lark, &clock, &repo1()).await;
    st.svc.prepare_for_vm(&run, &[]).await.unwrap();
    // Klaim sudah pindah ke VM tetapi run belum ditutup (langkah 3 terputus).
    lark.edit("rec1", |t| {
        if let Some(c) = t.claim.as_mut() {
            c.nonce = "r~vm".to_owned();
        }
    });
    st.svc.sync().poll(true, &[]).await.unwrap();
    st.svc.send_to_vm(&run, "vm").await.unwrap();
    assert!(st.svc.active_runs().await.unwrap().is_empty());
}
