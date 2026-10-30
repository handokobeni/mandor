//! "Take over to laptop" (U5 plan VM): kerja malam di-push, klaim pindah ke
//! daemon laptop, dan kartu tidak bisa kembali ke antrean VM.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::{FakeClock, FakeLark, Station, ready_task, station, vm_station};
use mandor_domain::{Claim, RunState, TaskStatus};
use mandor_usecase::StartTarget;
use mandor_usecase::run_ports::{RunRecord, WorkChanges};
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
    let branch = on_vm.svc.handoff("a", &to).await.unwrap();
    assert_eq!(branch, BRANCH);
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
    let run = laptop.svc.takeover("a", "repo1").await.unwrap();
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
