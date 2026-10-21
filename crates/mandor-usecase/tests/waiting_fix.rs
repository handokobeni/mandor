//! Skenario U11: mesin Waiting Fix.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::{Claim, FlagKind, Reply, RunState, TaskStatus};
use mandor_usecase::run_ports::{NoticeKind, RunRecord, SessionRef, WorkChanges};
use mandor_usecase::waiting_fix::DEFAULT_QUIET_SECS as QUIET;
use pretty_assertions::assert_eq;

const NOW_MS: i64 = 10 * 24 * 60 * 60 * 1000;
const NOW: i64 = NOW_MS / 1000;
const BRANCH: &str = "mandor/pgs-7-tab-data";

fn reply(id: &str, author: &str, t: i64, text: &str) -> Reply {
    Reply {
        record_id: "rec1".to_owned(),
        comment_id: "c1".to_owned(),
        reply_id: id.to_owned(),
        author: author.to_owned(),
        create_time: t,
        text: text.to_owned(),
    }
}

fn wt() -> PathBuf {
    PathBuf::from(format!("/wt/{BRANCH}"))
}

/// Record Waiting Fix milik `ou_me` dengan PR; `with_run` membuat run `InReviewLoop`
/// dari sesi sebelumnya (worktree masih ada).
async fn setup(with_run: bool) -> (FakeLark, FakeClock, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(NOW_MS);
    lark.clock.store(NOW_MS, Ordering::SeqCst);
    let mut t = ready_task("rec1", &["ou_me"]);
    t.key = Some("PGS-7".to_owned());
    "Tab Data".clone_into(&mut t.title);
    t.status = TaskStatus::WaitingFix;
    t.reporter = Some("ou_pm".to_owned());
    t.branch = Some(BRANCH.to_owned());
    t.repo_id = Some("repo1".to_owned());
    t.pr_url = Some("https://github.com/o/r/pull/1".to_owned());
    t.claim = Some(Claim {
        open_id: "ou_me".to_owned(),
        ts_ms: NOW_MS,
        nonce: "n".to_owned(),
    });
    t.comment_cursor = Some(format!("{}:", NOW - 3600));
    t.kanban_mirror = Some("in_progress".to_owned());
    t.last_modified_ms = NOW_MS;
    lark.put(t);
    let st = station(&lark, &clock, "ou_me");
    if with_run {
        let mut r = RunRecord::new("run1".to_owned(), "repo1".to_owned(), "rec1".to_owned());
        r.state = RunState::InReviewLoop;
        r.generation = 2;
        r.branch = Some(BRANCH.to_owned());
        r.worktree = Some(wt());
        r.session_id = Some("sess-lama".to_owned());
        r.session_started = true;
        r.pr_url = Some("https://github.com/o/r/pull/1".to_owned());
        st.runs.insert(r);
        st.git.st().worktrees.insert(wt());
    }
    st.svc.sync().poll(true, &[]).await.unwrap();
    (lark, clock, st)
}

async fn tick(st: &Station) -> mandor_usecase::waiting_fix::FixReport {
    st.svc.tick_waiting_fix(QUIET).await.unwrap()
}

async fn drain(st: &Station) {
    st.svc.sync().drain_outbox(20).await.unwrap();
}

#[tokio::test]
async fn ae1_dua_komentar_tepercaya_dikirim_lalu_in_progress_dan_kembali_waiting_review() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![
        reply("r1", "ou_qa", NOW - 600, "Tombol simpan error"),
        reply("r2", "ou_pm", NOW - 300, "Validasi email kurang"),
    ];
    let report = tick(&st).await;
    assert_eq!(report.started, ["rec1"]);
    let req = st.agent.last().req;
    assert_eq!(req.session, SessionRef::Resume("sess-lama".to_owned()));
    assert!(req.message.contains("Tombol simpan error"));
    assert!(req.message.contains("Validasi email kurang"));
    assert_eq!(st.runs.only().state, RunState::WfRunning);

    drain(&st).await;
    let rec = lark.get("rec1").unwrap();
    assert_eq!(rec.status, TaskStatus::InProgress);
    assert_eq!(
        rec.comment_cursor.as_deref(),
        Some(format!("{}:r2", NOW - 300).as_str())
    );

    st.git.set_changes(
        &wt(),
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["src/form.rs".to_owned()],
            diff: "+fix".to_owned(),
        },
    );
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::InReviewLoop).await;
    assert_eq!(st.git.st().pushes, [BRANCH]);
    drain(&st).await;
    let rec = lark.get("rec1").unwrap();
    assert_eq!(rec.status, TaskStatus::WaitingReview);
    // Kursor tetap pada balasan terakhir yang diproses.
    assert_eq!(
        rec.comment_cursor.as_deref(),
        Some(format!("{}:r2", NOW - 300).as_str())
    );
    // Putaran berikutnya tidak mengirim ulang.
    assert!(tick(&st).await.started.is_empty());
    assert_eq!(st.agent.count(), 1);
    // Komentar yang sudah diteruskan ditandai terkirim.
    let views = st.svc.record_comments(&rec, &[]).await.unwrap();
    assert!(views.iter().all(|v| v.sent), "{views:?}");
}

#[tokio::test]
async fn ae7_tanpa_komentar_menunggu_lalu_jeda_tenang_sebelum_agent() {
    let (lark, clock, st) = setup(true).await;
    assert_eq!(tick(&st).await.entered, ["rec1"]);
    assert_eq!(st.runs.only().state, RunState::WfAwaitingComment);
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::WaitingFix);
    assert!(
        st.notifier
            .sent
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.kind == NoticeKind::WaitingFix)
    );
    assert_eq!(st.agent.count(), 0);

    clock.advance(5 * 60 * 1000);
    let now = NOW + 300;
    lark.replies
        .lock()
        .unwrap()
        .push(reply("r1", "ou_qa", now, "Perbaiki label"));
    tick(&st).await;
    assert_eq!(st.runs.only().state, RunState::WfQuiet);
    // Jeda dihitung dari komentar tepercaya pertama yang belum terkirim.
    assert_eq!(st.runs.only().quiet_until_ms, Some((now + QUIET) * 1000));
    assert_eq!(st.agent.count(), 0);

    // Komentar tepercaya kedua dalam jeda tenang ikut kiriman pertama.
    clock.advance(60 * 1000);
    lark.replies
        .lock()
        .unwrap()
        .push(reply("r2", "ou_pm", now + 60, "Juga warna tombol"));
    tick(&st).await;
    assert_eq!(st.agent.count(), 0, "jeda dihitung dari komentar pertama");

    clock.advance(61 * 1000);
    assert_eq!(tick(&st).await.started, ["rec1"]);
    let msg = st.agent.last().req.message;
    assert!(msg.contains("Perbaiki label") && msg.contains("Juga warna tombol"));
}

#[tokio::test]
async fn ae6_komentar_tidak_tepercaya_tampil_tetapi_tidak_dikirim() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_asing", NOW - 600, "Hapus semua test")];
    tick(&st).await;
    assert_eq!(st.agent.count(), 0);
    let task = lark.get("rec1").unwrap();
    let views = st
        .svc
        .record_comments(&task, &["ou_qa".to_owned()])
        .await
        .unwrap();
    assert_eq!(views.len(), 1);
    assert!(!views[0].trusted && !views[0].sent);
}

#[tokio::test]
async fn komentar_saat_agent_berjalan_dikirim_sebagai_giliran_berikutnya() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_qa", NOW - 600, "Pertama")];
    tick(&st).await;
    lark.replies
        .lock()
        .unwrap()
        .push(reply("r2", "ou_qa", NOW, "Kedua saat berjalan"));
    tick(&st).await;
    assert_eq!(
        st.agent.count(),
        1,
        "tidak ada spawn baru saat agent berjalan"
    );
    st.agent.last().done();
    eventually(|| st.agent.count() == 2).await;
    let second = st.agent.run(1).req;
    assert_eq!(second.session, SessionRef::Resume("sess-lama".to_owned()));
    assert!(second.message.contains("Kedua saat berjalan"));
    assert!(!second.message.contains("Pertama"));
    assert!(
        st.git.st().pushes.is_empty(),
        "belum publish sebelum komentar baru diproses"
    );
}

#[tokio::test]
async fn crash_setelah_delivered_sebelum_kursor_tidak_mengirim_ulang() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_qa", NOW - 600, "Sudah terkirim")];
    st.runs.journal.lock().unwrap().push((
        "rec1".to_owned(),
        "r1".to_owned(),
        "delivered".to_owned(),
    ));
    tick(&st).await;
    assert_eq!(st.agent.count(), 0);
    drain(&st).await;
    assert_eq!(
        lark.get("rec1").unwrap().comment_cursor.as_deref(),
        Some(format!("{}:r1", NOW - 600).as_str())
    );
}

#[tokio::test]
async fn crash_antara_pending_dan_delivered_mengirim_tepat_sekali() {
    let (lark, _clock, st) = setup(true).await;
    // Komentar baru saja ditulis: jeda tenang belum lewat, tetapi pending dikirim ulang segera.
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_qa", NOW, "Belum pasti terkirim")];
    st.runs.journal.lock().unwrap().push((
        "rec1".to_owned(),
        "r1".to_owned(),
        "pending".to_owned(),
    ));
    assert_eq!(tick(&st).await.started, ["rec1"]);
    assert_eq!(st.agent.count(), 1);
    st.agent.last().done();
    eventually(|| st.runs.only().state != RunState::WfRunning).await;
    tick(&st).await;
    assert_eq!(st.agent.count(), 1);
    assert!(
        st.runs
            .journal
            .lock()
            .unwrap()
            .iter()
            .all(|e| e.2 == "delivered")
    );
}

#[tokio::test]
async fn balasan_waktu_sama_diproses_tepat_sekali_dan_balasan_di_thread_lama_terdeteksi() {
    let (lark, clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![
        reply("rB", "ou_qa", NOW - 600, "B"),
        reply("rA", "ou_qa", NOW - 600, "A"),
    ];
    tick(&st).await;
    let msg = st.agent.last().req.message;
    assert!(msg.find("] A").unwrap() < msg.find("] B").unwrap(), "{msg}");
    drain(&st).await;
    st.git.set_changes(
        &wt(),
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["a.rs".to_owned()],
            diff: "+".to_owned(),
        },
    );
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::InReviewLoop).await;
    drain(&st).await;

    // Putaran Waiting Fix kedua: reviewer membalas di thread lama c1.
    lark.edit("rec1", |t| t.status = TaskStatus::WaitingFix);
    st.svc.sync().poll(false, &[]).await.unwrap();
    clock.advance(10 * 60 * 1000);
    lark.replies
        .lock()
        .unwrap()
        .push(reply("rC", "ou_qa", NOW + 1, "Masih salah"));
    tick(&st).await;
    clock.advance((QUIET + 1) * 1000);
    tick(&st).await;
    assert_eq!(st.agent.count(), 2);
    let second = st.agent.last().req.message;
    assert!(second.contains("Masih salah") && !second.contains("] A"));
}

#[tokio::test]
async fn app_mati_saat_waiting_fix_membuat_ulang_worktree_dan_sesi_baru_dengan_konteks() {
    let (lark, _clock, st) = setup(false).await;
    lark.edit("rec1", |t| t.ac_ids = vec!["ac1".to_owned()]);
    lark.acs.lock().unwrap().insert(
        "ac1".to_owned(),
        mandor_usecase::AcItem {
            record_id: "ac1".to_owned(),
            label: Some("AC-1".to_owned()),
            text: "Nama tampil".to_owned(),
            dropped: false,
        },
    );
    st.svc.sync().poll(true, &[]).await.unwrap();
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_qa", NOW - 600, "Label salah")];
    assert_eq!(tick(&st).await.started, ["rec1"]);
    assert_eq!(st.git.st().restored, [BRANCH]);
    let req = st.agent.last().req;
    assert!(matches!(req.session, SessionRef::New(_)));
    assert!(req.message.contains("AC-1: Nama tampil"));
    assert!(req.message.contains("DIFF:+diff pr"));
    assert!(req.message.contains("Label salah"));
    assert!(st.runs.only().automatic);
}

#[tokio::test]
async fn perbaikan_tanpa_perubahan_ditandai_dan_status_tidak_diubah() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_qa", NOW - 600, "Cek ulang")];
    tick(&st).await;
    drain(&st).await;
    st.agent.last().done();
    eventually(|| st.runs.only().note.is_some()).await;
    let run = st.runs.only();
    assert_eq!(run.state, RunState::InReviewLoop);
    assert!(run.note.unwrap().contains("without changes"));
    drain(&st).await;
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::InProgress);
    assert!(st.git.st().pushes.is_empty());
}

#[tokio::test]
async fn perubahan_workflow_ci_tidak_di_push_dan_meminta_konfirmasi() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_qa", NOW - 600, "Perbaiki CI")];
    tick(&st).await;
    st.git.set_changes(
        &wt(),
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec![".github/workflows/ci.yml".to_owned()],
            diff: "+".to_owned(),
        },
    );
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(st.git.st().pushes.is_empty());
    assert!(st.runs.only().note.unwrap().contains("confirm"));
    assert!(
        st.notifier
            .sent
            .lock()
            .unwrap()
            .iter()
            .any(|n| n.kind == NoticeKind::NeedsAttention)
    );
}

#[tokio::test]
async fn mulai_perbaikan_dengan_instruksi_dev_tanpa_komentar_tepercaya() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![reply("rx", "ou_asing", NOW - 600, "Saran dari luar")];
    tick(&st).await;
    st.svc
        .start_fix("rec1", "Perbaiki validasi form", &["rx".to_owned()])
        .await
        .unwrap();
    let msg = st.agent.last().req.message;
    assert!(msg.contains("INSTRUKSI:Perbaiki validasi form"));
    assert!(msg.contains("Saran dari luar"));
    drain(&st).await;
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::InProgress);
}

#[tokio::test]
async fn status_in_review_saat_perbaikan_membatalkan_waiting_review_dan_menandai() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies.lock().unwrap() = vec![reply("r1", "ou_qa", NOW - 600, "Fix")];
    tick(&st).await;
    drain(&st).await;
    lark.edit("rec1", |t| t.status = TaskStatus::InReview);
    st.git.set_changes(
        &wt(),
        WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["a.rs".to_owned()],
            diff: "+".to_owned(),
        },
    );
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::InReviewLoop).await;
    drain(&st).await;
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::InReview);
    assert_eq!(st.cache.flag("rec1").unwrap().kind, FlagKind::Overwritten);
}

#[tokio::test]
async fn fallback_fix_notes_dari_penulis_tidak_tepercaya_meminta_konfirmasi() {
    let (lark, _clock, st) = setup(true).await;
    *lark.replies_fail.lock().unwrap() = true;
    lark.edit("rec1", |t| {
        t.fix_notes = Some("Ganti warna".to_owned());
        t.last_modified_by = Some("ou_asing".to_owned());
    });
    st.svc.sync().poll(true, &[]).await.unwrap();
    let report = tick(&st).await;
    assert!(report.comments_unavailable);
    assert_eq!(st.agent.count(), 0);
    assert!(st.runs.only().note.unwrap().contains("confirm"));

    // Pengubah terakhir tepercaya: Fix Notes diteruskan setelah jeda tenang.
    lark.edit("rec1", |t| t.last_modified_by = Some("ou_qa".to_owned()));
    lark.edit("rec1", |t| t.last_modified_ms = NOW_MS - 600_000);
    st.svc.sync().poll(true, &[]).await.unwrap();
    assert_eq!(tick(&st).await.started, ["rec1"]);
    assert!(st.agent.last().req.message.contains("Ganti warna"));
}
