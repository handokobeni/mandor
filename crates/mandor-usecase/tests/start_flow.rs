//! Skenario U10: Start, sesi agent, publish, dan pemeriksaan PR.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{AppField, Claim, RunState, TaskStatus, WriteOp};
use mandor_usecase::run_ports::{NoticeKind, SessionRef, WorkChanges};
use mandor_usecase::{AcItem, RunError, SendOutcome, StartStep, SyncError};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock.store(10 * DAY, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Tab Data".clone_into(&mut t.title);
    "Tampilkan data profil".clone_into(&mut t.description);
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned(), "ac3".to_owned()];
    t.kanban_mirror = Some("todo".to_owned());
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    let mut acs = lark.acs.lock().unwrap();
    for (id, text, dropped) in [
        ("ac1", "Nama tampil", false),
        ("ac2", "Email tampil", false),
        ("ac3", "Lama", true),
    ] {
        acs.insert(
            id.to_owned(),
            AcItem {
                record_id: id.to_owned(),
                label: Some(format!("AC-{id}")),
                text: text.to_owned(),
                dropped,
            },
        );
    }
    drop(acs);
    (lark, clock)
}

async fn started(lark: &FakeLark, clock: &FakeClock) -> Station {
    let st = station(lark, clock, "ou_me");
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st
}

#[tokio::test]
async fn f1_start_membuat_worktree_menjalankan_agent_lalu_in_progress_dan_branch() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;

    assert_eq!(st.git.st().prepared, ["mandor/pgs-7-tab-data"]);
    let req = st.agent.last().req;
    assert!(matches!(req.session, SessionRef::New(_)));
    assert!(req.message.contains("Tab Data"));
    assert!(req.message.contains("Tampilkan data profil"));
    assert!(
        req.message
            .contains("AC-ac1: Nama tampil|AC-ac2: Email tampil")
    );
    assert!(!req.message.contains("Lama"), "AC Droped tidak dikirim");

    let run = st.runs.only();
    assert_eq!(run.state, RunState::Running);
    assert_eq!(run.generation, 1);
    // Status ditulis paling akhir lewat outbox.
    assert_eq!(
        st.outbox.ops(),
        [WriteOp::Transition {
            from: TaskStatus::ToDo,
            to: TaskStatus::InProgress,
            fields: vec![
                AppField::Branch("mandor/pgs-7-tab-data".to_owned()),
                AppField::RepoId("repo1".to_owned())
            ]
        }]
    );
    st.svc.sync().drain_outbox(10).await.unwrap();
    let rec = lark.get("rec1").unwrap();
    assert_eq!(rec.status, TaskStatus::InProgress);
    assert_eq!(rec.branch.as_deref(), Some("mandor/pgs-7-tab-data"));
    assert_eq!(rec.claim.unwrap().open_id, "ou_me");
}

#[tokio::test]
async fn gagal_spawn_menghapus_worktree_melepas_klaim_dan_status_tetap_to_do() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    *st.agent.fail_spawn.lock().unwrap() = true;
    let err = st.svc.start("rec1", "repo1", None).await.unwrap_err();
    assert!(matches!(err, RunError::Port(_)), "{err:?}");
    assert_eq!(st.git.st().discarded, ["mandor/pgs-7-tab-data"]);
    let rec = lark.get("rec1").unwrap();
    assert_eq!(rec.status, TaskStatus::ToDo);
    assert_eq!(rec.claim, None);
    assert!(st.outbox.ops().is_empty());
    assert!(st.svc.active_runs().await.unwrap().is_empty());
}

#[tokio::test]
async fn kalah_klaim_membatalkan_start_tanpa_worktree() {
    let (lark, clock) = setup();
    lark.edit("rec1", |t| {
        t.assignees.push("ou_other".to_owned());
        t.claim = Some(Claim {
            open_id: "ou_other".to_owned(),
            ts_ms: 10 * DAY,
            nonce: "x".to_owned(),
        });
    });
    let st = station(&lark, &clock, "ou_me");
    let err = st.svc.start("rec1", "repo1", None).await.unwrap_err();
    assert!(
        matches!(err, RunError::Sync(SyncError::Claim(_))),
        "{err:?}"
    );
    assert!(st.git.st().prepared.is_empty());
    assert_eq!(st.agent.count(), 0);
}

#[tokio::test]
async fn giliran_selesai_menunggu_dev_dan_notifikasi() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent
        .last()
        .emit(mandor_domain::AgentEvent::AssistantMessage {
            text: "Sudah".to_owned(),
        });
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let sent = st.notifier.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].kind, NoticeKind::AwaitingAnswer);
    assert!(sent[0].title.contains("PGS-7"));
    assert!(st.runs.chat.lock().unwrap().len() >= 2);
}

#[tokio::test]
async fn publish_commit_push_pr_lalu_waiting_review_dengan_link() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.svc.sync().drain_outbox(10).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let wt = st.runs.only().worktree.unwrap();
    st.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/a.rs".to_owned()],
            diff: "+fn a(){}".to_owned(),
        },
    );

    let url = st.svc.publish(&st.runs.only().id).await.unwrap();
    assert_eq!(url, "https://github.com/o/repo1/pull/1");
    assert_eq!(*st.agent.commit_diffs.lock().unwrap(), ["+fn a(){}"]);
    assert_eq!(st.git.st().commits[0].1, "feat: implementasi task");
    assert_eq!(st.git.st().pushes, ["mandor/pgs-7-tab-data"]);
    assert_eq!(st.git.st().created_prs[0].1, "PGS-7: Tab Data");

    st.svc.sync().drain_outbox(10).await.unwrap();
    let rec = lark.get("rec1").unwrap();
    assert_eq!(rec.status, TaskStatus::WaitingReview);
    assert_eq!(rec.pr_url.as_deref(), Some(url.as_str()));
    assert!(
        rec.comment_cursor.is_some(),
        "kursor ditulis bersama Waiting Review"
    );
    assert_eq!(st.runs.only().state, RunState::InReviewLoop);
}

#[tokio::test]
async fn publish_tanpa_perubahan_dan_tanpa_pr_tetap_membuat_pr() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.svc.publish(&st.runs.only().id).await.unwrap();
    assert!(st.git.st().commits.is_empty());
    assert!(st.git.st().pushes.is_empty());
    // Publish kedua tanpa perubahan apa pun: ditolak, state dipulihkan.
    let err = st.svc.publish(&st.runs.only().id).await.unwrap_err();
    assert_eq!(err, RunError::NoChanges);
    assert_eq!(st.runs.only().state, RunState::InReviewLoop);
}

#[tokio::test]
async fn pr_buatan_luar_app_memicu_waiting_review_kecuali_saat_loop_perbaikan() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.svc.sync().drain_outbox(10).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    // Loop perbaikan: pemeriksaan PR tidak menyentuh record.
    let mut run = st.runs.only();
    run.state = RunState::WfAwaitingComment;
    st.runs.insert(run.clone());
    st.git.st().prs.insert(
        "repo1:mandor/pgs-7-tab-data".to_owned(),
        "https://github.com/o/r/pull/9".to_owned(),
    );
    assert!(st.svc.check_prs().await.unwrap().is_empty());

    run.state = RunState::AwaitingDev;
    st.runs.insert(run);
    assert_eq!(st.svc.check_prs().await.unwrap(), ["rec1"]);
    st.svc.sync().drain_outbox(10).await.unwrap();
    let rec = lark.get("rec1").unwrap();
    assert_eq!(rec.status, TaskStatus::WaitingReview);
    assert_eq!(rec.pr_url.as_deref(), Some("https://github.com/o/r/pull/9"));
    // Sudah punya link: tidak diperiksa ulang.
    assert!(st.svc.check_prs().await.unwrap().is_empty());
}

#[tokio::test]
async fn pesan_saat_sibuk_diantrekan_lalu_dikirim_setelah_giliran_selesai() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    let id = st.runs.only().id;
    assert_eq!(
        st.svc.send_message(&id, "tambahkan test").await.unwrap(),
        SendOutcome::Queued
    );
    assert_eq!(st.agent.count(), 1);
    st.agent.run(0).done();
    eventually(|| st.agent.count() == 2).await;
    let second = st.agent.run(1).req;
    assert!(matches!(second.session, SessionRef::Resume(_)));
    assert_eq!(second.message, "tambahkan test");
    let run = st.runs.only();
    assert_eq!((run.state, run.generation), (RunState::Running, 2));
    assert!(run.queue.is_empty());
    assert!(
        st.runs
            .dev_messages()
            .contains(&"tambahkan test".to_owned())
    );
}

#[tokio::test]
async fn kirim_sekarang_menghentikan_giliran_lama_dan_event_lama_diabaikan() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    let id = st.runs.only().id;
    assert_eq!(
        st.svc.send_now(&id, "ganti pendekatan").await.unwrap(),
        SendOutcome::Sent
    );
    assert!(st.agent.run(0).is_stopped());
    assert_eq!(st.agent.count(), 2);
    let run = st.runs.only();
    assert_eq!(run.state, RunState::Running);
    // Giliran lama sudah menutup stream dengan Failed; tidak mengubah run baru.
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let after = st.runs.only();
    assert_eq!(
        (after.state, after.generation),
        (RunState::Running, run.generation)
    );
    assert!(
        st.notifier
            .sent
            .lock()
            .unwrap()
            .iter()
            .all(|n| n.kind != NoticeKind::Failed)
    );
}

#[tokio::test]
async fn stop_membuat_run_menunggu_dev() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    let id = st.runs.only().id;
    st.svc.stop(&id).await.unwrap();
    assert!(st.agent.run(0).is_stopped());
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
}

#[tokio::test]
async fn crash_agent_menandai_gagal_dan_notifikasi() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    st.agent.last().crash("exit 137");
    eventually(|| st.runs.only().state == RunState::Failed).await;
    assert_eq!(st.runs.only().last_error.as_deref(), Some("exit 137"));
    assert!(
        st.notifier
            .sent
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.kind == NoticeKind::Failed)
    );
}

#[tokio::test]
async fn discard_berkas_hanya_untuk_berkas_berubah_dan_saat_agent_diam() {
    let (lark, clock) = setup();
    let st = started(&lark, &clock).await;
    let id = st.runs.only().id;
    let wt = st.runs.only().worktree.unwrap();
    st.git.set_changes(
        &wt,
        WorkChanges {
            dirty: true,
            files: vec!["src/a.rs".to_owned()],
            ..WorkChanges::default()
        },
    );
    // Agent masih bekerja: berkas bisa sedang diubah.
    assert_eq!(
        st.svc.discard_file(&id, "src/a.rs").await.unwrap_err(),
        RunError::InvalidState("running")
    );
    st.agent.run(0).done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    assert_eq!(
        st.svc.discard_file(&id, "../etc/passwd").await.unwrap_err(),
        RunError::NotFound("changed file")
    );
    st.svc.discard_file(&id, "src/a.rs").await.unwrap();
    assert_eq!(st.git.st().reverted, ["src/a.rs"]);
}

#[tokio::test]
async fn start_melaporkan_setiap_langkah_berurutan() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let steps = std::sync::Mutex::new(Vec::new());
    st.svc
        .start_with_progress(
            "rec1",
            &mandor_usecase::StartTarget::Repo("repo1".to_owned()),
            None,
            |s| steps.lock().unwrap().push(s),
        )
        .await
        .unwrap();
    assert_eq!(
        steps.into_inner().unwrap(),
        [
            StartStep::Claiming,
            StartStep::CreatingWorktree,
            StartStep::StartingAgent,
            StartStep::MovingStatus
        ]
    );
}
