//! Menutup run kartu yang sudah selesai (plan `2026-11-03-001`, U2).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station};
use mandor_domain::cleanup::SkipReason;
use mandor_domain::{RunState, TaskStatus};
use mandor_usecase::run::CleanupSettings;
use mandor_usecase::run_ports::{NoticeKind, RunRecord, WorkChanges};
use mandor_usecase::{Planning, StartTarget};
use pretty_assertions::assert_eq;

const MIN: i64 = 60_000;

fn setup(ids: &[&str]) -> (FakeLark, FakeClock, Station) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(100 * MIN);
    for (i, id) in ids.iter().enumerate() {
        let mut t = ready_task(id, &["ou_me"]);
        t.key = Some(format!("PGS-{}", i + 1));
        t.epic_ids = vec!["ep1".to_owned()];
        lark.put(t);
    }
    let st = station(&lark, &clock, "ou_me");
    (lark, clock, st)
}

/// Run kartu tunggal yang menunggu dev dengan worktree bersih.
async fn waiting_run(st: &Station, id: &str) -> RunRecord {
    let run = st.svc.start(id, "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| run_of(st, &run.id).state == RunState::AwaitingDev).await;
    run_of(st, &run.id)
}

fn run_of(st: &Station, id: &str) -> RunRecord {
    st.runs.all().into_iter().find(|r| r.id == id).unwrap()
}

async fn set_status(lark: &FakeLark, st: &Station, id: &str, status: TaskStatus) {
    lark.edit(id, |t| t.status = status);
    st.svc.sync().poll(true, &[]).await.unwrap();
}

fn set_size(st: &Station, run: &RunRecord, bytes: u64) {
    st.git
        .st()
        .sizes
        .insert(run.worktree.clone().unwrap(), bytes);
}

fn unpushed(st: &Station, run: &RunRecord) {
    st.git.set_changes(
        run.worktree.as_ref().unwrap(),
        WorkChanges {
            dirty: false,
            unpushed: 1,
            files: Vec::new(),
            diff: String::new(),
        },
    );
}

fn is_open(st: &Station, id: &str) -> bool {
    !st.runs.rows.lock().unwrap()[id].1
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae1_hanya_run_selesai_dan_bersih_yang_ditutup() {
    let (lark, _clock, st) = setup(&["c1", "c2", "c3"]);
    let r1 = waiting_run(&st, "c1").await;
    let r2 = waiting_run(&st, "c2").await;
    let r3 = waiting_run(&st, "c3").await;
    set_size(&st, &r1, 2_000_000);
    set_status(&lark, &st, "c1", TaskStatus::Delivered).await;
    set_status(&lark, &st, "c2", TaskStatus::WaitingDeploy).await;
    unpushed(&st, &r2);

    let found = st.svc.finished_runs().await.unwrap();
    let closable: Vec<&str> = found.closable.iter().map(|r| r.run_id.as_str()).collect();
    assert_eq!(closable, [r1.id.as_str()]);
    assert_eq!(found.closable[0].keys, ["PGS-1"]);
    assert_eq!(found.closable[0].bytes, Some(2_000_000));
    assert_eq!(found.skipped.len(), 1);
    assert_eq!(found.skipped[0].run.run_id, r2.id);
    assert_eq!(found.skipped[0].reason, SkipReason::UnpushedChanges);

    let report = st
        .svc
        .close_finished(&[r1.id.clone(), r2.id.clone()])
        .await
        .unwrap();
    assert_eq!(report.closed, [r1.id.as_str()]);
    assert_eq!(report.freed_bytes, 2_000_000);
    assert_eq!(report.skipped[0].reason, SkipReason::UnpushedChanges);
    assert!(!is_open(&st, &r1.id), "run PGS-1 tertutup");
    assert!(
        !st.git
            .st()
            .worktrees
            .contains(&r1.worktree.clone().unwrap()),
        "worktree PGS-1 dihapus"
    );
    assert!(is_open(&st, &r2.id), "PGS-2 belum di-push, tetap terbuka");
    assert!(is_open(&st, &r3.id), "PGS-3 belum selesai");
}

#[tokio::test(flavor = "multi_thread")]
async fn konfirmasi_memeriksa_ulang_kartu_yang_kembali_ke_waiting_fix() {
    let (lark, _clock, st) = setup(&["c1"]);
    let r1 = waiting_run(&st, "c1").await;
    set_status(&lark, &st, "c1", TaskStatus::WaitingDeploy).await;
    assert_eq!(st.svc.finished_runs().await.unwrap().closable.len(), 1);
    set_status(&lark, &st, "c1", TaskStatus::WaitingFix).await;

    let report = st
        .svc
        .close_finished(std::slice::from_ref(&r1.id))
        .await
        .unwrap();
    assert!(report.closed.is_empty());
    assert!(is_open(&st, &r1.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae4_run_yang_sedang_bekerja_tidak_ditutup() {
    let (lark, _clock, st) = setup(&["c1"]);
    let run = st.svc.start("c1", "repo1", None).await.unwrap();
    set_status(&lark, &st, "c1", TaskStatus::Delivered).await;
    let found = st.svc.finished_runs().await.unwrap();
    assert!(found.closable.is_empty());
    assert_eq!(found.skipped[0].reason, SkipReason::Working);
    assert!(
        st.svc
            .close_finished(std::slice::from_ref(&run.id))
            .await
            .unwrap()
            .closed
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae2_run_grup_selesai_bila_semua_kartu_yang_disertakan_selesai() {
    let (lark, _clock, st) = setup(&["c1", "c2", "c3"]);
    let run = st
        .svc
        .start_group(
            &["c1".to_owned(), "c2".to_owned(), "c3".to_owned()],
            &StartTarget::Repo("repo1".to_owned()),
            None,
            &Planning::Skip,
            &[],
            |_| {},
        )
        .await
        .unwrap();
    st.svc.sync().drain_outbox(20).await.unwrap();
    st.agent.last().done();
    eventually(|| run_of(&st, &run.id).state == RunState::AwaitingDev).await;
    st.svc.remove_card(&run.id, "c2").await.unwrap();
    set_status(&lark, &st, "c1", TaskStatus::Delivered).await;
    set_status(&lark, &st, "c3", TaskStatus::WaitingReview).await;
    assert!(st.svc.finished_runs().await.unwrap().closable.is_empty());

    set_status(&lark, &st, "c3", TaskStatus::Delivered).await;
    let found = st.svc.finished_runs().await.unwrap();
    assert_eq!(found.closable.len(), 1);
    assert_eq!(found.closable[0].keys, ["PGS-1", "PGS-3"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae3_otomatis_hanya_bila_dinyalakan_dan_paling_sering_sepuluh_menit() {
    let (lark, clock, st) = setup(&["c1", "c2"]);
    let r1 = waiting_run(&st, "c1").await;
    let r2 = waiting_run(&st, "c2").await;
    set_size(&st, &r1, 3_000);
    set_status(&lark, &st, "c1", TaskStatus::Delivered).await;

    assert!(
        st.svc.auto_close_finished().await.unwrap().is_none(),
        "mati"
    );
    assert!(is_open(&st, &r1.id));

    st.svc
        .set_cleanup_settings(&CleanupSettings { auto_close: true })
        .await
        .unwrap();
    assert!(st.svc.cleanup_settings().await.auto_close);
    let report = st.svc.auto_close_finished().await.unwrap().unwrap();
    assert_eq!(report.closed, [r1.id.as_str()]);
    assert!(!is_open(&st, &r1.id));
    let notices: Vec<_> = st
        .notifier
        .sent
        .lock()
        .unwrap()
        .iter()
        .filter(|n| n.kind == NoticeKind::RunsClosed)
        .cloned()
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0].body.contains("Closed 1 finished run"),
        "{notices:?}"
    );

    set_status(&lark, &st, "c2", TaskStatus::Delivered).await;
    clock.set(100 * MIN + 5 * MIN);
    assert!(
        st.svc.auto_close_finished().await.unwrap().is_none(),
        "belum 10 menit"
    );
    clock.set(100 * MIN + 11 * MIN);
    let report = st.svc.auto_close_finished().await.unwrap().unwrap();
    assert_eq!(report.closed, [r2.id.as_str()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn ukuran_gagal_diukur_tidak_menghalangi_penutupan() {
    let (lark, _clock, st) = setup(&["c1"]);
    let r1 = waiting_run(&st, "c1").await;
    st.git.st().size_error = Some("du timed out".to_owned());
    set_status(&lark, &st, "c1", TaskStatus::Delivered).await;
    let found = st.svc.finished_runs().await.unwrap();
    assert_eq!(found.closable[0].bytes, None);
    let report = st
        .svc
        .close_finished(std::slice::from_ref(&r1.id))
        .await
        .unwrap();
    assert_eq!(report.closed, [r1.id.as_str()]);
    assert_eq!(report.freed_bytes, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae7_daftar_storage_semua_run_terbuka_urut_ukuran() {
    let (lark, _clock, st) = setup(&["c1", "c2", "c3"]);
    let r1 = waiting_run(&st, "c1").await;
    let r2 = waiting_run(&st, "c2").await;
    let r3 = waiting_run(&st, "c3").await;
    set_size(&st, &r1, 10);
    set_size(&st, &r2, 1_000_000_000);
    set_size(&st, &r3, 500);
    set_status(&lark, &st, "c3", TaskStatus::Delivered).await;

    let rows = st.svc.run_storage().await.unwrap();
    let order: Vec<&str> = rows.iter().map(|r| r.run_id.as_str()).collect();
    assert_eq!(order, [r2.id.as_str(), r3.id.as_str(), r1.id.as_str()]);
    assert_eq!(rows[0].keys, ["PGS-2"]);
    assert_eq!(rows[0].bytes, Some(1_000_000_000));
    assert_eq!(rows[0].state, RunState::AwaitingDev);
    assert_eq!(rows[1].card_status, [TaskStatus::Delivered]);
}
