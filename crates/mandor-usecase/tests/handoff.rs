//! "Take over to laptop" (U5 plan VM): kerja malam di-push, klaim pindah ke
//! daemon laptop, dan kartu tidak bisa kembali ke antrean VM.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::{FakeClock, FakeLark, Station, ready_task, station, vm_station};
use mandor_domain::planned::{DesignMark, PlannedWork};
use mandor_domain::{Claim, Design, RunState, TaskStatus};
use mandor_usecase::run_ports::{RepoInfo, RunPhase, RunRecord, WorkChanges, WorkspaceInfo};
use mandor_usecase::{AcItem, CarriedReview, StartTarget, VmCarry};
use pretty_assertions::assert_eq;

const NOW: i64 = 1_792_800_000_000;
const BRANCH: &str = "mandor/pgs-9-job";

/// Kartu `a` dipegang daemon VM dengan run tertahan dan kerja malam yang
/// belum di-commit maupun belum di-push.
async fn held_on_vm() -> (FakeLark, FakeClock, Station, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(NOW);
    lark.clock.store(NOW, Ordering::SeqCst);
    let mut t = ready_task("a", &["ou_me"]);
    t.key = Some("PGS-9".to_owned());
    t.status = TaskStatus::InProgress;
    t.branch = Some(BRANCH.to_owned());
    t.repo_id = Some("repo1".to_owned());
    t.claim = Some(Claim {
        open_id: "ou_me".to_owned(),
        ts_ms: NOW,
        nonce: "r~vm".to_owned(),
    });
    lark.put(t);
    let on_vm = vm_station(&lark, &clock, "ou_me");
    let wt = PathBuf::from(format!("/wt/{BRANCH}"));
    let mut r = RunRecord::new("run-a".to_owned(), "repo1".to_owned(), "a".to_owned());
    r.state = RunState::Held;
    r.branch = Some(BRANCH.to_owned());
    r.worktree = Some(wt.clone());
    on_vm.runs.insert(r);
    on_vm.git.st().worktrees.insert(wt.clone());
    on_vm.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            unpushed: 2,
            files: vec!["src/a.rs".to_owned()],
            diff: "+a".to_owned(),
        },
    );
    on_vm.svc.sync().poll(true, &[]).await.unwrap();
    let laptop = station(&lark, &clock, "ou_me");
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    (lark, clock, on_vm, laptop)
}

#[tokio::test]
async fn covers_ae11_kerja_malam_di_push_klaim_pindah_dan_laptop_melanjutkan() {
    let (lark, _clock, on_vm, laptop) = held_on_vm().await;
    let to = laptop.svc.sync().identity().daemon_id.clone();
    let back = on_vm.svc.handoff("a", &to).await.unwrap();
    assert_eq!(back.branch, BRANCH);
    assert_eq!(back.target, Some(StartTarget::Repo("repo1".to_owned())));
    assert_eq!(back.planned, None);
    {
        let git = on_vm.git.st();
        assert!(
            git.commits
                .iter()
                .any(|(_, m)| m.starts_with("PGS-9: work in progress")),
            "{:?}",
            git.commits
        );
        assert_eq!(git.pushes, [BRANCH], "push tanpa PR");
        assert!(git.created_prs.is_empty());
    }
    let claim = lark.get("a").unwrap().claim.unwrap();
    assert_eq!(claim.daemon_id(), Some(to.as_str()));
    assert!(
        on_vm.svc.active_runs().await.unwrap().is_empty(),
        "run VM ditutup"
    );

    laptop.svc.sync().poll(true, &[]).await.unwrap();
    let run = laptop
        .svc
        .takeover(
            "a",
            &StartTarget::Repo("repo1".to_owned()),
            None,
            &VmCarry::default(),
        )
        .await
        .unwrap();
    assert_eq!(run.state, RunState::AwaitingDev);
    assert_eq!(run.branch.as_deref(), Some(BRANCH));
    assert_eq!(laptop.git.st().restored, [BRANCH]);

    on_vm.svc.sync().poll(true, &[]).await.unwrap();
    let err = on_vm
        .svc
        .queue_add("a", &StartTarget::Repo("repo1".to_owned()), None)
        .await
        .unwrap_err();
    assert!(
        format!("{err}").contains("taken over to the laptop"),
        "{err}"
    );
}

#[tokio::test]
async fn take_over_draft_pr_vm_lanjut_di_pr_yang_sama_lalu_send_to_review_dari_laptop() {
    let (lark, _clock, on_vm, laptop) = held_on_vm().await;
    let url = "https://github.com/o/r/pull/9".to_owned();
    let mut r = on_vm.runs.only();
    r.state = RunState::AwaitingDev;
    r.draft_pending = Some(NOW);
    r.pr_url = Some(url.clone());
    r.review_snapshot = [("repo1".to_owned(), "tree0".to_owned())].into();
    on_vm.runs.insert(r);
    on_vm
        .reviews
        .ledgers
        .lock()
        .unwrap()
        .insert("run-a".to_owned(), vec![passed("ac1")]);
    let to = laptop.svc.sync().identity().daemon_id.clone();
    let back = on_vm.svc.handoff("a", &to).await.unwrap();
    assert!(back.carry.draft, "VM menandai PR masih draft");
    assert_eq!(
        back.carry.review,
        Some(CarriedReview {
            rows: vec![passed("ac1")],
            snapshot: [("repo1".to_owned(), "tree0".to_owned())].into(),
        })
    );

    laptop.svc.sync().poll(true, &[]).await.unwrap();
    laptop
        .git
        .st()
        .prs
        .insert(format!("repo1:{BRANCH}"), url.clone());
    let run = laptop
        .svc
        .takeover(
            "a",
            &StartTarget::Repo("repo1".to_owned()),
            None,
            &back.carry,
        )
        .await
        .unwrap();
    assert_eq!(run.state, RunState::AwaitingDev);
    assert!(
        run.draft_pending.is_some(),
        "perubahan laptop tetap di PR draft"
    );
    assert_eq!(run.pr_url.as_deref(), Some(url.as_str()));
    let rows = laptop.reviews.ledgers.lock().unwrap()[&run.id].clone();
    assert_eq!(
        rows,
        [passed("ac1")],
        "hasil review VM ikut; kode sama, tidak usang"
    );

    laptop.svc.send_to_review(&run.id).await.unwrap();
    assert_eq!(laptop.git.st().ready, std::slice::from_ref(&url));
    laptop.svc.sync().drain_outbox(10).await.unwrap();
    assert_eq!(lark.get("a").unwrap().status, TaskStatus::WaitingReview);
}

fn passed(ac: &str) -> mandor_domain::AcRow {
    let mut row =
        mandor_domain::AcRow::new(ac, Some("AC-1"), "teks", mandor_domain::RowStatus::Active);
    row.verdict = mandor_domain::Verdict::Pass;
    row.evidence = Some("tampil".to_owned());
    row
}

#[tokio::test]
async fn hasil_review_vm_usang_bila_kode_laptop_berbeda() {
    let (_lark, _clock, laptop) = handed_to_laptop();
    laptop.git.st().trees.insert(
        PathBuf::from(format!("/wt/{BRANCH}")),
        "tree-lain".to_owned(),
    );
    let carry = VmCarry {
        draft: false,
        review: Some(CarriedReview {
            rows: vec![passed("ac1")],
            snapshot: [("repo1".to_owned(), "tree0".to_owned())].into(),
        }),
        cards: Vec::new(),
        one_plan: None,
    };
    let run = laptop
        .svc
        .takeover("a", &StartTarget::Repo("repo1".to_owned()), None, &carry)
        .await
        .unwrap();
    let rows = laptop.reviews.ledgers.lock().unwrap()[&run.id].clone();
    assert!(rows[0].stale, "kode berubah setelah review VM: review lagi");
}

#[tokio::test]
async fn take_over_bisa_diulang_bila_langkah_laptop_gagal() {
    let (_lark, _clock, on_vm, laptop) = held_on_vm().await;
    let to = laptop.svc.sync().identity().daemon_id.clone();
    let first = on_vm.svc.handoff("a", &to).await.unwrap();
    let pushes = on_vm.git.st().pushes.len();
    let again = on_vm.svc.handoff("a", &to).await.unwrap();
    assert_eq!(
        again, first,
        "serah-terima yang sudah selesai dikembalikan lagi"
    );
    assert_eq!(on_vm.git.st().pushes.len(), pushes, "tidak push ulang");
    let err = on_vm.svc.handoff("a", "lain").await.unwrap_err();
    assert!(format!("{err}").contains("another daemon"), "{err}");
}

#[tokio::test]
async fn push_gagal_klaim_tidak_pindah() {
    let (lark, _clock, on_vm, _laptop) = held_on_vm().await;
    on_vm.git.st().fail_push = true;
    assert!(on_vm.svc.handoff("a", "local").await.is_err());
    let claim = lark.get("a").unwrap().claim.unwrap();
    assert_eq!(claim.daemon_id(), Some("vm"), "klaim tetap di VM");
    assert_eq!(on_vm.svc.active_runs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn agent_yang_berjalan_dihentikan_dulu() {
    let (_lark, _clock, on_vm, _laptop) = held_on_vm().await;
    on_vm
        .svc
        .send_message("run-a", "Lanjutkan perbaikan")
        .await
        .unwrap();
    let turn = on_vm.agent.last();
    on_vm.svc.handoff("a", "local").await.unwrap();
    assert!(turn.is_stopped(), "agent dihentikan sebelum serah-terima");
    assert_eq!(on_vm.git.st().pushes, [BRANCH]);
}

#[tokio::test]
async fn kartu_yang_dipegang_daemon_lain_atau_di_laptop_ditolak() {
    let (lark, _clock, on_vm, laptop) = held_on_vm().await;
    lark.edit("a", |t| {
        t.claim = Some(Claim {
            open_id: "ou_me".to_owned(),
            ts_ms: NOW,
            nonce: "r~local".to_owned(),
        });
    });
    on_vm.svc.sync().poll(true, &[]).await.unwrap();
    assert!(on_vm.svc.handoff("a", "local").await.is_err());
    assert!(
        laptop.svc.handoff("a", "local").await.is_err(),
        "serah-terima hanya dari daemon VM"
    );
}

/// Kartu `a` sudah diserahkan VM ke daemon laptop (`local`).
fn handed_to_laptop() -> (FakeLark, FakeClock, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(NOW);
    lark.clock.store(NOW, Ordering::SeqCst);
    let mut t = ready_task("a", &["ou_me"]);
    t.key = Some("PGS-9".to_owned());
    "Job".clone_into(&mut t.title);
    t.status = TaskStatus::InProgress;
    t.branch = Some(BRANCH.to_owned());
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned()];
    t.claim = Some(Claim {
        open_id: "ou_me".to_owned(),
        ts_ms: NOW,
        nonce: "r~local".to_owned(),
    });
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
    let laptop = station(&lark, &clock, "ou_me");
    (lark, clock, laptop)
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

#[tokio::test]
async fn covers_ae9_take_over_workspace_memulihkan_folder_run_dan_semua_anggota() {
    let (_lark, _clock, laptop) = handed_to_laptop();
    laptop.repos.add(repo("api"));
    laptop.repos.add(repo("web"));
    laptop.repos.add_workspace(WorkspaceInfo {
        id: "kelola".to_owned(),
        name: "kelola".to_owned(),
        root: PathBuf::from("/src/kelola"),
        repo_ids: vec!["api".to_owned(), "web".to_owned()],
        mcp_servers: vec![],
        browser_tests: false,
        gate_mode: None,
    });
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    let target = StartTarget::Workspace {
        id: "kelola".to_owned(),
        repo_ids: vec!["api".to_owned(), "web".to_owned()],
    };
    let run = laptop
        .svc
        .takeover("a", &target, None, &VmCarry::default())
        .await
        .unwrap();

    assert_eq!(run.state, RunState::AwaitingDev);
    assert_eq!(laptop.git.st().restored, [BRANCH, BRANCH]);
    assert_eq!(run.repo_id, "api");
    let ws = run.workspace.clone().unwrap();
    assert_eq!(ws.id, "kelola");
    assert_eq!(
        run.worktree.as_deref().unwrap().parent(),
        Some(ws.root.as_path())
    );
    assert_eq!(run.extra.len(), 1);
    assert_eq!(run.extra[0].repo_id, "web");
}

fn package() -> PlannedWork {
    PlannedWork {
        plan: "## Plan\n1. Ubah".to_owned(),
        branch: BRANCH.to_owned(),
        design: vec![DesignMark {
            ac_id: "ac2".to_owned(),
            design: Design::FollowDesign,
            note: Some("Header beda".to_owned()),
        }],
        laptop_design: vec!["ds1".to_owned(), "old-mock".to_owned()],
        repos: vec!["repo1".to_owned()],
        group_acs: None,
    }
}

#[tokio::test]
async fn covers_ae3_take_over_dengan_paket_kembali_ke_plan_dan_approve_mengirim_plan() {
    let (_lark, _clock, laptop) = handed_to_laptop();
    laptop.design.add("ds1", "kelola-mock");
    laptop.svc.sync().poll(true, &[]).await.unwrap();
    let run = laptop
        .svc
        .takeover(
            "a",
            &StartTarget::Repo("repo1".to_owned()),
            Some(&package()),
            &VmCarry::default(),
        )
        .await
        .unwrap();

    assert_eq!(
        (run.state, run.phase),
        (RunState::AwaitingDev, RunPhase::Plan)
    );
    assert_eq!(run.plan.as_deref(), Some("## Plan\n1. Ubah"));
    assert_eq!(run.design.len(), 1);
    let rows = laptop.svc.ledger(&run.id).await.unwrap();
    assert_eq!(rows.len(), 2, "buku AC diisi dari Lark");
    assert_eq!(rows[1].design, Design::FollowDesign);
    assert_eq!(rows[1].design_note.as_deref(), Some("Header beda"));
    assert!(
        laptop
            .runs
            .system_messages()
            .iter()
            .any(|m| m.contains("old-mock")),
        "sumber desain yang hilang dicatat"
    );

    laptop.svc.approve_plan(&run.id, &[]).await.unwrap();
    let req = laptop.agent.last().req;
    assert!(
        req.message.starts_with("IMPLEMENT PLAN Job"),
        "{}",
        req.message
    );
    assert!(req.message.contains("PLAN:## Plan"));
    assert!(
        req.message.contains("FOLLOW:AC-ac2: teks ac2"),
        "{}",
        req.message
    );
    assert!(
        req.message.contains("DESIGN REF kelola-mock"),
        "{}",
        req.message
    );
    assert!(matches!(
        req.session,
        mandor_usecase::run_ports::SessionRef::New(_)
    ));
}
