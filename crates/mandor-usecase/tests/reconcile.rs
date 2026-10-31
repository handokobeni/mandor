//! Skenario U10: rekonsiliasi saat daemon dimulai ulang (KTD-13).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{FakeClock, FakeLark, station};
use mandor_domain::RunState;
use mandor_usecase::run_ports::{RunRecord, SessionRef};
use pretty_assertions::assert_eq;

fn run(id: &str, record: &str, state: RunState, pid: Option<i64>) -> RunRecord {
    let mut r = RunRecord::new(id.to_owned(), "repo1".to_owned(), record.to_owned());
    r.state = state;
    r.generation = 3;
    r.pid = pid;
    r.branch = Some(format!("mandor/{record}"));
    r.worktree = Some(format!("/wt/mandor/{record}").into());
    r.session_id = Some("sess".to_owned());
    r.session_started = true;
    r
}

#[tokio::test]
async fn run_berjalan_saat_restart_menjadi_gagal_dan_proses_lama_dihentikan() {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    let st = station(&lark, &clock, "ou_me");
    st.runs
        .insert(run("a", "rec1", RunState::Running, Some(111)));
    st.runs
        .insert(run("b", "rec2", RunState::AwaitingDev, None));
    st.runs
        .insert(run("c", "rec3", RunState::WfRunning, Some(222)));

    let mut marked = st.svc.reconcile().await.unwrap();
    marked.sort();
    assert_eq!(marked, ["a", "c"]);
    let mut reaped = st.agent.reaped.lock().unwrap().clone();
    reaped.sort_unstable();
    assert_eq!(reaped, [111, 222]);
    let a = st.runs.rows.lock().unwrap()["a"].0.clone();
    assert_eq!((a.state, a.generation, a.pid), (RunState::Failed, 4, None));
    assert!(a.last_error.unwrap().contains("daemon restarted"));
    assert_eq!(
        st.runs.rows.lock().unwrap()["b"].0.state,
        RunState::AwaitingDev
    );
}

#[tokio::test]
async fn run_gagal_bisa_dilanjutkan_dengan_resume_dan_worktree_dipulihkan() {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    let st = station(&lark, &clock, "ou_me");
    st.runs
        .insert(run("a", "rec1", RunState::Running, Some(111)));
    st.svc.reconcile().await.unwrap();
    // Worktree hilang (mis. dihapus manual): dibuat ulang dari branch task.
    st.svc.resume_failed("a", None).await.unwrap();
    assert_eq!(st.git.st().restored, ["mandor/rec1"]);
    let req = st.agent.last().req;
    assert_eq!(req.session, SessionRef::Resume("sess".to_owned()));
    assert_eq!(st.runs.rows.lock().unwrap()["a"].0.state, RunState::Running);
}

mod ownership {
    use std::sync::atomic::Ordering;

    use super::common::{FakeClock, FakeLark, ready_task, station};
    use mandor_domain::claim::STALE_MS;
    use mandor_domain::{Claim, RunState, TaskStatus};
    use mandor_usecase::RunError;
    use mandor_usecase::run_ports::RunRecord;
    use pretty_assertions::assert_eq;

    const NOW: i64 = 10 * 24 * 60 * 60 * 1000;

    fn base() -> (FakeLark, FakeClock) {
        let lark = FakeLark::default();
        let clock = FakeClock::default();
        clock.set(NOW);
        lark.clock.store(NOW, Ordering::SeqCst);
        let mut t = ready_task("rec1", &["ou_me", "ou_b"]);
        t.status = TaskStatus::InProgress;
        t.branch = Some("mandor/rec1".to_owned());
        t.last_modified_ms = NOW;
        lark.put(t);
        (lark, clock)
    }

    #[tokio::test]
    async fn klaim_hilang_membuat_run_orphaned() {
        let (lark, clock) = base();
        lark.edit("rec1", |t| {
            t.claim = Some(Claim {
                open_id: "ou_b".to_owned(),
                ts_ms: NOW,
                nonce: "x".to_owned(),
            });
        });
        let st = station(&lark, &clock, "ou_me");
        let mut r = RunRecord::new("a".to_owned(), "repo1".to_owned(), "rec1".to_owned());
        r.state = RunState::AwaitingDev;
        st.runs.insert(r);
        assert_eq!(st.svc.heartbeat_all().await.unwrap(), ["a"]);
        assert_eq!(st.runs.only().state, RunState::Orphaned);
        // Tidak diproses ulang.
        assert!(st.svc.heartbeat_all().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn record_dihapus_menghentikan_run_menjadi_stopped() {
        let (lark, clock) = base();
        lark.edit("rec1", |t| {
            t.claim = Some(Claim {
                open_id: "ou_me".to_owned(),
                ts_ms: NOW,
                nonce: "n".to_owned(),
            });
        });
        let st = station(&lark, &clock, "ou_me");
        let mut r = RunRecord::new("a".to_owned(), "repo1".to_owned(), "rec1".to_owned());
        r.state = RunState::Running;
        st.runs.insert(r);
        lark.remove("rec1");
        assert_eq!(st.svc.heartbeat_all().await.unwrap(), ["a"]);
        assert_eq!(st.runs.only().state, RunState::Stopped);
        // Tidak diproses ulang.
        assert!(st.svc.heartbeat_all().await.unwrap().is_empty());
        // Run yang berhenti tidak menerima pesan dan tidak bisa di-push.
        assert_eq!(
            st.svc.send_message("a", "lanjut").await.unwrap_err(),
            RunError::InvalidState("stopped")
        );
        assert_eq!(
            st.svc.publish("a").await.unwrap_err(),
            RunError::InvalidState("stopped")
        );
    }

    #[tokio::test]
    async fn ambil_alih_klaim_stale_memulihkan_worktree_lalu_tutup_run_melepas_klaim() {
        let (lark, clock) = base();
        lark.edit("rec1", |t| {
            t.claim = Some(Claim {
                open_id: "ou_b".to_owned(),
                ts_ms: NOW - STALE_MS - 1,
                nonce: "x".to_owned(),
            });
        });
        let st = station(&lark, &clock, "ou_me");
        let run = st
            .svc
            .takeover(
                "rec1",
                &mandor_usecase::StartTarget::Repo("repo1".to_owned()),
                None,
                &mandor_usecase::VmCarry::default(),
            )
            .await
            .unwrap();
        assert_eq!(run.state, RunState::AwaitingDev);
        assert_eq!(st.git.st().restored, ["mandor/rec1"]);
        assert_eq!(lark.get("rec1").unwrap().claim.unwrap().open_id, "ou_me");

        st.svc.close_run(&run.id).await.unwrap();
        assert!(st.svc.active_runs().await.unwrap().is_empty());
        assert_eq!(lark.get("rec1").unwrap().claim, None);
        assert_eq!(st.git.st().discarded, ["mandor/rec1"]);
    }
}
