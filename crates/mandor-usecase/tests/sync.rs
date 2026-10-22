//! Skenario U8: poller, outbox dengan validasi ulang, dan klaim pemilik.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{FakeClock, FakeLark, OutState, laptop, ready_task};
use mandor_domain::claim::{HEARTBEAT_MS, STALE_MS};
use mandor_domain::{
    AppField, Claim, ClaimLoss, ClaimRefused, FlagKind, GateRejection, TaskStatus, WriteOp,
};
use mandor_usecase::{AcItem, Heartbeat, PortError, SprintItem, SyncError};
use pretty_assertions::assert_eq;

const DAY: i64 = 24 * 60 * 60 * 1000;

fn setup() -> (FakeLark, FakeClock) {
    let lark = FakeLark::default();
    let clock = FakeClock::default();
    clock.set(10 * DAY);
    lark.clock
        .store(10 * DAY, std::sync::atomic::Ordering::SeqCst);
    (lark, clock)
}

fn now_task(id: &str, who: &[&str], clock: &FakeClock) -> mandor_domain::Task {
    let mut t = ready_task(id, who);
    t.last_modified_ms = mandor_usecase::Clock::now_ms(clock);
    t
}

fn claim_of(who: &str, ts: i64, nonce: &str) -> Claim {
    Claim {
        open_id: who.to_owned(),
        ts_ms: ts,
        nonce: nonce.to_owned(),
    }
}

fn start_op() -> WriteOp {
    WriteOp::Transition {
        from: TaskStatus::ToDo,
        to: TaskStatus::InProgress,
        fields: vec![AppField::Branch("mandor/pgs-1".to_owned())],
    }
}

#[tokio::test]
async fn outbox_menolak_transisi_di_luar_allowlist_di_tempat() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    let to_review = WriteOp::Transition {
        from: TaskStatus::WaitingReview,
        to: TaskStatus::InReview,
        fields: vec![],
    };
    let from_deploy = WriteOp::Transition {
        from: TaskStatus::WaitingDeploy,
        to: TaskStatus::InProgress,
        fields: vec![],
    };
    for op in [to_review, from_deploy] {
        let err = a.svc.request_write("rec1", &op).await.unwrap_err();
        assert!(matches!(err, SyncError::Rejected(_)), "{err:?}");
    }
    assert!(a.outbox.states().is_empty());
}

#[tokio::test]
async fn outbox_membatalkan_in_progress_bila_status_sudah_bukan_to_do() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    let mut t = now_task("rec1", &["ou_a"], &clock);
    t.claim = Some(claim_of("ou_a", 10 * DAY, "n"));
    lark.put(t);
    a.svc.poll(true, &[]).await.unwrap();
    a.svc.request_write("rec1", &start_op()).await.unwrap();
    // PM memindahkan status sebelum outbox sempat menulis.
    lark.edit("rec1", |t| t.status = TaskStatus::InReview);

    let report = a.svc.drain_outbox(10).await.unwrap();
    assert_eq!(report.cancelled, ["rec1"]);
    assert!(lark.writes().iter().all(|(_, s, _)| s.is_none()));
    assert_eq!(lark.get("rec1").unwrap().status, TaskStatus::InReview);
    let flag = a.cache.flag("rec1").unwrap();
    assert_eq!(flag.kind, FlagKind::Overwritten);
    assert!(flag.detail.contains("In Review"), "{}", flag.detail);
    assert!(matches!(
        a.outbox.states().last().unwrap().1,
        OutState::Cancelled(_)
    ));
}

#[tokio::test]
async fn cache_mempertahankan_key_judul_dan_assignee_setelah_outbox_menulis() {
    // Respons update Bitable hanya memuat field yang ditulis (lihat FakeLark).
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    let mut t = now_task("rec1", &["ou_a"], &clock);
    t.key = Some("PGS-1510".to_owned());
    t.title = "TEST Mandor".to_owned();
    t.claim = Some(claim_of("ou_a", 10 * DAY, "n"));
    lark.put(t);
    a.svc.poll(true, &[]).await.unwrap();
    a.svc.request_write("rec1", &start_op()).await.unwrap();

    let report = a.svc.drain_outbox(10).await.unwrap();
    // Poll juga mengantrekan cermin kanban, jadi rec1 bisa ditulis lebih dari sekali.
    assert!(report.written.iter().all(|r| r == "rec1"), "{report:?}");
    assert!(!report.written.is_empty());
    let cached = a.svc.cached("rec1").await.unwrap().unwrap().task;
    assert_eq!(cached.key.as_deref(), Some("PGS-1510"));
    assert_eq!(cached.title, "TEST Mandor");
    assert_eq!(cached.assignees, ["ou_a"]);
    assert_eq!(cached.status, TaskStatus::InProgress);
    assert_eq!(cached.branch.as_deref(), Some("mandor/pgs-1"));
}

#[tokio::test]
async fn outbox_menulis_transisi_dan_field_dengan_jeda_antar_tulis() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    for id in ["rec1", "rec2"] {
        let mut t = now_task(id, &["ou_a"], &clock);
        t.claim = Some(claim_of("ou_a", 10 * DAY, "n"));
        t.kanban_mirror = Some("todo".to_owned());
        lark.put(t);
        a.svc.request_write(id, &start_op()).await.unwrap();
    }
    let report = a.svc.drain_outbox(10).await.unwrap();
    assert_eq!(report.written.len(), 2);
    let rec1 = lark.get("rec1").unwrap();
    assert_eq!(rec1.status, TaskStatus::InProgress);
    assert_eq!(rec1.branch.as_deref(), Some("mandor/pgs-1"));
    assert_eq!(
        *clock.slept.lock().unwrap(),
        [std::time::Duration::from_millis(600)]
    );
    assert_eq!(a.cache.task("rec1").unwrap().status, TaskStatus::InProgress);
}

#[tokio::test]
async fn ae2_judul_dari_pm_muncul_di_cache_dan_app_tidak_pernah_menulis_judul() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    lark.put(now_task("rec1", &["ou_a"], &clock));
    a.svc.poll(false, &[]).await.unwrap();
    lark.edit("rec1", |t| t.title = "Judul baru dari PM".to_owned());
    let report = a.svc.poll(false, &[]).await.unwrap();
    assert_eq!(report.changed, ["rec1"]);
    assert_eq!(a.cache.task("rec1").unwrap().title, "Judul baru dari PM");
    // `AppField` tidak punya varian judul; tidak ada satu pun tulisan.
    assert!(lark.writes().is_empty());
    // Poll berikutnya tanpa perubahan tidak menandai apa pun.
    assert!(a.svc.poll(false, &[]).await.unwrap().changed.is_empty());
}

#[tokio::test]
async fn poll_inkremental_memakai_jendela_kemarin_dan_rekonsiliasi_penuh_tanpa_batas() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    a.svc.poll(false, &[]).await.unwrap();
    a.svc.poll(true, &[]).await.unwrap();
    assert_eq!(*lark.searches.lock().unwrap(), [Some(9 * DAY), None]);
}

#[tokio::test]
async fn ae3_start_ditolak_bila_readiness_review_ac_by_pm() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    let mut t = now_task("rec1", &["ou_a"], &clock);
    t.readiness = Some("Review AC By PM".to_owned());
    lark.put(t);
    let err = a.svc.claim("rec1", false).await.unwrap_err();
    assert_eq!(
        err,
        SyncError::Gate(GateRejection::NotReady("Review AC By PM".to_owned()))
    );
    assert!(lark.writes().is_empty());
}

#[tokio::test]
async fn dua_laptop_klaim_bersamaan_tepat_satu_menang() {
    let (lark, clock) = setup();
    lark.put(now_task("rec1", &["ou_a", "ou_b"], &clock));
    let a = laptop(&lark, &clock, "ou_a");
    let b = laptop(&lark, &clock, "ou_b");
    let (ra, rb) = tokio::join!(a.svc.claim("rec1", false), b.svc.claim("rec1", false));
    let wins = [ra.is_ok(), rb.is_ok()];
    assert_eq!(wins.iter().filter(|w| **w).count(), 1, "{ra:?} {rb:?}");
    let loser = if ra.is_ok() { rb } else { ra };
    assert_eq!(loser.unwrap_err(), SyncError::ClaimLost);
    // Keduanya sempat menulis (tanpa CAS); yang tersisa adalah nonce pemenang.
    assert_eq!(lark.writes().len(), 2);
}

#[tokio::test]
async fn start_batal_tanpa_menulis_bila_klaim_aktif_milik_dev_lain() {
    let (lark, clock) = setup();
    let mut t = now_task("rec1", &["ou_a", "ou_b"], &clock);
    t.claim = Some(claim_of("ou_b", 10 * DAY - 60_000, "nb"));
    lark.put(t);
    let a = laptop(&lark, &clock, "ou_a");
    let err = a.svc.claim("rec1", false).await.unwrap_err();
    assert_eq!(
        err,
        SyncError::Claim(ClaimRefused::HeldByOther("ou_b".to_owned()))
    );
    assert!(lark.writes().is_empty());
}

#[tokio::test]
async fn heartbeat_tiap_lima_menit_dan_klaim_stale_bisa_diambil_alih() {
    let (lark, clock) = setup();
    lark.put(now_task("rec1", &["ou_a", "ou_b"], &clock));
    let a = laptop(&lark, &clock, "ou_a");
    let b = laptop(&lark, &clock, "ou_b");
    let mine = a.svc.claim("rec1", false).await.unwrap();

    assert_eq!(a.svc.heartbeat("rec1").await.unwrap(), Heartbeat::Fresh);
    clock.advance(HEARTBEAT_MS);
    assert_eq!(a.svc.heartbeat("rec1").await.unwrap(), Heartbeat::Renewed);
    let renewed = lark.get("rec1").unwrap().claim.unwrap();
    assert_eq!(renewed.nonce, mine.nonce);
    assert!(renewed.ts_ms > mine.ts_ms);

    // Klaim masih aktif: ambil alih ditolak.
    let err = b.svc.claim("rec1", true).await.unwrap_err();
    assert_eq!(
        err,
        SyncError::Claim(ClaimRefused::NotStale("ou_a".to_owned()))
    );

    // Laptop A mati; 30 menit tanpa heartbeat.
    clock.advance(STALE_MS + 1);
    let taken = b.svc.claim("rec1", true).await.unwrap();
    assert_eq!(lark.get("rec1").unwrap().claim, Some(taken));
    assert_eq!(
        a.svc.heartbeat("rec1").await.unwrap(),
        Heartbeat::Lost(ClaimLoss::TakenByOther)
    );
}

#[tokio::test]
async fn heartbeat_membedakan_record_dihapus_dan_dialihkan() {
    let (lark, clock) = setup();
    lark.put(now_task("rec1", &["ou_a"], &clock));
    lark.put(now_task("rec2", &["ou_a"], &clock));
    let a = laptop(&lark, &clock, "ou_a");
    a.svc.claim("rec1", false).await.unwrap();
    a.svc.claim("rec2", false).await.unwrap();

    lark.remove("rec1");
    // Dialihkan ke dev lain sementara klaim masih milik kita.
    lark.edit("rec2", |t| t.assignees = vec!["ou_x".to_owned()]);
    assert_eq!(
        a.svc.heartbeat("rec1").await.unwrap(),
        Heartbeat::Lost(ClaimLoss::Deleted)
    );
    assert_eq!(
        a.svc.heartbeat("rec2").await.unwrap(),
        Heartbeat::Lost(ClaimLoss::Unassigned)
    );
}

#[tokio::test]
async fn record_dihapus_atau_dialihkan_ditandai_tidak_hilang() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    lark.put(now_task("rec1", &["ou_a"], &clock));
    lark.put(now_task("rec2", &["ou_a"], &clock));
    a.svc.poll(true, &[]).await.unwrap();

    lark.remove("rec1");
    lark.edit("rec2", |t| t.assignees = vec!["ou_x".to_owned()]);
    let mut report = a.svc.poll(true, &[]).await.unwrap();
    report.flagged.sort();
    assert_eq!(report.flagged, ["rec1", "rec2"]);
    assert_eq!(a.cache.flag("rec1").unwrap().kind, FlagKind::Deleted);
    assert_eq!(a.cache.flag("rec2").unwrap().kind, FlagKind::Unassigned);
    assert!(a.cache.task("rec1").is_some());
    // Tidak ditandai ulang pada rekonsiliasi berikutnya.
    assert!(a.svc.poll(true, &[]).await.unwrap().flagged.is_empty());

    // Di-assign kembali: penanda dihapus.
    lark.edit("rec2", |t| t.assignees = vec!["ou_a".to_owned()]);
    a.svc.poll(true, &[]).await.unwrap();
    assert!(a.cache.flag("rec2").is_none());
}

#[tokio::test]
async fn tiga_kali_gagal_menandai_gagal_sinkron_dan_coba_lagi_memvalidasi_ulang() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    let mut t = now_task("rec1", &["ou_a"], &clock);
    t.claim = Some(claim_of("ou_a", 10 * DAY, "n"));
    t.kanban_mirror = Some("todo".to_owned());
    lark.put(t);
    a.svc.poll(true, &[]).await.unwrap();
    a.svc.request_write("rec1", &start_op()).await.unwrap();
    *lark.write_errors.lock().unwrap() = vec![
        PortError::Conflict,
        PortError::Throttled,
        PortError::Transient("timeout".to_owned()),
    ];
    for expected_retry in [true, true, false] {
        let r = a.svc.drain_outbox(10).await.unwrap();
        assert_eq!(!r.retrying.is_empty(), expected_retry, "{r:?}");
        clock.advance(60_000);
    }
    let flag = a.cache.flag("rec1").unwrap();
    assert_eq!(flag.kind, FlagKind::SyncFailed);
    assert!(flag.detail.contains("timeout"));
    assert!(a.svc.drain_outbox(10).await.unwrap().written.is_empty());

    // Sementara itu status berubah; coba lagi tidak menulis tanpa validasi.
    lark.edit("rec1", |t| t.status = TaskStatus::WaitingReview);
    assert_eq!(a.svc.retry_failed("rec1").await.unwrap(), 1);
    assert!(a.cache.flag("rec1").is_none());
    let r = a.svc.drain_outbox(10).await.unwrap();
    assert_eq!(r.cancelled, ["rec1"]);
    assert!(lark.writes().is_empty());
}

#[tokio::test]
async fn reauth_menghentikan_pengurasan_tanpa_menghitung_percobaan() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    let mut t = now_task("rec1", &["ou_a"], &clock);
    t.claim = Some(claim_of("ou_a", 10 * DAY, "n"));
    lark.put(t);
    a.svc.request_write("rec1", &start_op()).await.unwrap();
    *lark.write_errors.lock().unwrap() = vec![PortError::Reauth];
    let err = a.svc.drain_outbox(10).await.unwrap_err();
    assert_eq!(err, SyncError::Port(PortError::Reauth));
    assert_eq!(a.outbox.items.lock().unwrap()[0].0.attempts, 0);
}

#[tokio::test]
async fn cermin_kanban_hanya_ditulis_pemilik_dan_hanya_bila_berubah() {
    let (lark, clock) = setup();
    let mut t = now_task("rec1", &["ou_a", "ou_b"], &clock);
    t.status = TaskStatus::WaitingFix;
    t.kanban_mirror = Some("review".to_owned());
    t.claim = Some(claim_of("ou_a", 10 * DAY, "n"));
    lark.put(t);
    let a = laptop(&lark, &clock, "ou_a");
    let b = laptop(&lark, &clock, "ou_b");
    b.svc.poll(true, &[]).await.unwrap();
    assert!(
        b.outbox.ops().is_empty(),
        "bukan pemilik tidak menulis cermin"
    );

    a.svc.poll(true, &[]).await.unwrap();
    assert_eq!(
        a.outbox.ops(),
        [WriteOp::Fields(vec![AppField::KanbanMirror(
            "in_progress".to_owned()
        )])]
    );
    a.svc.drain_outbox(10).await.unwrap();
    a.svc.poll(true, &[]).await.unwrap();
    assert_eq!(
        a.outbox.ops().len(),
        1,
        "cermin sudah sesuai; tidak ada tulisan baru"
    );
}

#[tokio::test]
async fn klaim_dikosongkan_bila_manusia_mengembalikan_status_ke_to_do() {
    let (lark, clock) = setup();
    let mut t = now_task("rec1", &["ou_a"], &clock);
    t.status = TaskStatus::InProgress;
    t.kanban_mirror = Some("in_progress".to_owned());
    t.claim = Some(claim_of("ou_a", 10 * DAY, "n"));
    lark.put(t);
    let a = laptop(&lark, &clock, "ou_a");
    a.svc.poll(true, &[]).await.unwrap();
    assert!(a.outbox.ops().is_empty());

    lark.edit("rec1", |t| t.status = TaskStatus::ToDo);
    a.svc.poll(false, &[]).await.unwrap();
    assert_eq!(
        a.outbox.ops(),
        [WriteOp::Fields(vec![
            AppField::Claim(None),
            AppField::KanbanMirror("todo".to_owned())
        ])]
    );
    a.svc.drain_outbox(10).await.unwrap();
    assert_eq!(lark.get("rec1").unwrap().claim, None);
}

#[tokio::test]
async fn komentar_hanya_diambil_saat_waiting_fix_atau_loop_perbaikan() {
    let (lark, clock) = setup();
    let a = laptop(&lark, &clock, "ou_a");
    let mut t = now_task("rec1", &["ou_a"], &clock);
    t.status = TaskStatus::InProgress;
    lark.put(t);
    assert!(!a.svc.poll(false, &[]).await.unwrap().comments_needed);
    // Perbaikan berjalan walau status Lark sudah In Progress.
    assert!(
        a.svc
            .poll(false, &["rec1".to_owned()])
            .await
            .unwrap()
            .comments_needed
    );
    lark.edit("rec1", |t| t.status = TaskStatus::WaitingFix);
    assert!(a.svc.poll(false, &[]).await.unwrap().comments_needed);
}

#[tokio::test]
async fn lepas_klaim_hanya_bila_masih_milik_sendiri() {
    let (lark, clock) = setup();
    let mut t = now_task("rec1", &["ou_a", "ou_b"], &clock);
    t.claim = Some(claim_of("ou_b", 10 * DAY, "nb"));
    lark.put(t);
    let a = laptop(&lark, &clock, "ou_a");
    a.svc.release_claim("rec1").await.unwrap();
    assert!(lark.writes().is_empty());
    let b = laptop(&lark, &clock, "ou_b");
    b.svc.release_claim("rec1").await.unwrap();
    assert_eq!(lark.get("rec1").unwrap().claim, None);
}

#[tokio::test]
async fn detail_task_dari_cache_dengan_ac_termasuk_yang_di_drop() {
    let (lark, clock) = setup();
    let mut t = now_task("rec1", &["ou_a"], &clock);
    "Tampilkan profil".clone_into(&mut t.description);
    t.ac_ids = vec!["ac2".to_owned(), "ac1".to_owned()];
    lark.put(t);
    {
        let mut acs = lark.acs.lock().unwrap();
        for (id, dropped) in [("ac1", false), ("ac2", true)] {
            acs.insert(
                id.to_owned(),
                AcItem {
                    record_id: id.to_owned(),
                    label: Some(format!("AC-{id}")),
                    text: format!("teks {id}"),
                    dropped,
                    ..Default::default()
                },
            );
        }
    }
    let a = laptop(&lark, &clock, "ou_a");
    a.svc.poll(true, &[]).await.unwrap();

    let d = a.svc.task_details("rec1").await.unwrap();
    assert_eq!(d.task.description, "Tampilkan profil");
    let acs: Vec<_> = d
        .acs
        .iter()
        .map(|ac| (ac.record_id.as_str(), ac.dropped))
        .collect();
    assert_eq!(acs, [("ac2", true), ("ac1", false)]);
    assert_eq!(
        a.svc.task_details("tidak-ada").await.unwrap_err(),
        SyncError::NotFound
    );
}

#[tokio::test]
async fn nama_orang_di_cache_dan_gagal_contact_tidak_merusak() {
    let (lark, clock) = setup();
    lark.names
        .lock()
        .unwrap()
        .insert("ou_a".to_owned(), "Beni".to_owned());
    let a = laptop(&lark, &clock, "ou_a");
    let ids = vec!["ou_a".to_owned(), "ou_tak_dikenal".to_owned()];
    let names = a.svc.user_names(&ids).await;
    assert_eq!(names.get("ou_a").map(String::as_str), Some("Beni"));
    assert!(!names.contains_key("ou_tak_dikenal"));
    // Nama yang sudah dikenal tidak diminta lagi.
    a.svc.user_names(&ids).await;
    assert_eq!(
        *lark.name_calls.lock().unwrap(),
        [ids.clone(), vec!["ou_tak_dikenal".to_owned()]]
    );
    // Contact API gagal: nama dari cache tetap dipakai, sisanya kosong.
    *lark.names_fail.lock().unwrap() = true;
    let names = a.svc.user_names(&ids).await;
    assert_eq!(names.len(), 1);
    // Gagal (mis. scope belum disetujui) tidak dicoba ulang selama 5 menit.
    let before = lark.name_calls.lock().unwrap().len();
    a.svc.user_names(&ids).await;
    assert_eq!(lark.name_calls.lock().unwrap().len(), before);
    clock.advance(5 * 60 * 1000);
    a.svc.user_names(&ids).await;
    assert_eq!(lark.name_calls.lock().unwrap().len(), before + 1);
}

#[tokio::test]
async fn sprint_di_cache_dan_tidak_diminta_ulang() {
    let (lark, clock) = setup();
    lark.sprints.lock().unwrap().insert(
        "spr1".to_owned(),
        SprintItem {
            record_id: "spr1".to_owned(),
            name: "Sprint 42".to_owned(),
            start_ms: Some(1),
            end_ms: Some(2),
            status: Some("In Progress".to_owned()),
        },
    );
    let a = laptop(&lark, &clock, "ou_a");
    let ids = vec!["spr1".to_owned(), "spr1".to_owned()];
    let list = a.svc.sprints(&ids).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Sprint 42");
    a.svc.sprints(&ids).await;
    assert_eq!(
        *lark.sprint_calls.lock().unwrap(),
        [vec!["spr1".to_owned()]]
    );
}

#[tokio::test]
async fn papan_tim_berisi_kartu_aktif_rekan_dan_done_14_hari_terakhir() {
    let (lark, clock) = setup();
    let mut mine = now_task("recMe", &["ou_me"], &clock);
    mine.title = "Punya saya".to_owned();
    lark.put(mine);
    lark.put(now_task("recB", &["ou_b"], &clock));
    let mut old_done = ready_task("recOld", &["ou_b"]);
    old_done.status = TaskStatus::Delivered;
    old_done.last_modified_ms = 10 * DAY - 15 * DAY;
    lark.put(old_done);
    let mut recent_done = ready_task("recDone", &["ou_c"]);
    recent_done.status = TaskStatus::WaitingDeploy;
    recent_done.last_modified_ms = 10 * DAY - 2 * DAY;
    lark.put(recent_done);
    let mut old_active = ready_task("recLama", &["ou_c"]);
    old_active.last_modified_ms = 0;
    lark.put(old_active);

    let me = laptop(&lark, &clock, "ou_me");
    assert!(me.svc.team_board().is_empty(), "belum diambil");
    assert!(me.svc.poll_team().await.unwrap(), "papan tim berubah");
    assert!(
        !me.svc.poll_team().await.unwrap(),
        "tanpa perubahan di Lark"
    );
    let mut ids: Vec<String> = me
        .svc
        .team_board()
        .into_iter()
        .map(|t| t.record_id)
        .collect();
    ids.sort();
    // Kartu saya tidak diulang (sudah di cache), Done lama tidak diambil.
    assert_eq!(ids, ["recB", "recDone", "recLama"]);
    // Cache kartu saya tidak tersentuh.
    assert!(
        mandor_usecase::TaskCache::list(&me.cache)
            .await
            .unwrap()
            .is_empty()
    );
}
