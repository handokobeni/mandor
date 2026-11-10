//! Sesi reviewer AC (U9 Rilis 2): mulai, verdict lewat tool ledger, akhir
//! normal atau gagal, antrean pesan dev, kirim temuan, dan restart.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use common::run_fakes::eventually;
use common::{FakeClock, FakeLark, Station, ready_task, station, vm_station};
use mandor_domain::app_run::{AppRunState, AppSettings, TestMode};
use mandor_domain::{RunState, Verdict};
use mandor_usecase::run_ports::{
    AppMemberRecord, AppRecord, AppRecords, ReviewHistory, ReviewStatus, ReviewTrigger,
};
use mandor_usecase::{AcItem, PortError, RunError};
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
    t.ac_ids = vec!["ac1".to_owned(), "ac2".to_owned(), "ac3".to_owned()];
    t.last_modified_ms = 10 * DAY;
    lark.put(t);
    for id in ["ac1", "ac2", "ac3"] {
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

/// Run yang sudah selesai giliran pertamanya dan menunggu dev.
async fn waiting(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let id = st.runs.only().id;
    (st, id)
}

fn verdict(st: &Station, ac: &str, v: Verdict, evidence: &str) {
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    let rt = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        let mut report = common::report(Some(evidence));
        // Pass lewat browser butuh langkah bernomor (R16).
        report.steps = vec![evidence.to_owned()];
        rt.block_on(st.svc.ledger_set_verdict(&scope, ac, v, report))
    })
    .unwrap();
}

async fn reviewed(st: &Station, run: &str) -> mandor_usecase::run_ports::ReviewRecord {
    eventually(|| st.runs.only().state != RunState::Reviewing).await;
    st.reviews.reviews(run).await.unwrap().remove(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn review_semua_ac_tersimpan_dan_run_kembali_ke_dev() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let rec = st
        .svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    assert_eq!(st.runs.only().state, RunState::Reviewing);
    let req = st.agent.last().req;
    let review = req.review.clone().unwrap();
    assert_eq!(review.review_id, rec.id);
    assert!(!review.browser, "repo tanpa app");
    assert_eq!(review.budget_cents, 200);
    assert!(matches!(
        req.session,
        mandor_usecase::run_ports::SessionRef::New(_)
    ));
    assert!(req.message.contains("REVIEW"), "{}", req.message);
    assert!(req.message.contains("REASON:No app for this run"));
    // Sesi pelaksana tidak berubah.
    assert_eq!(st.runs.only().session_id, st.runs.all()[0].session_id);

    verdict(&st, "ac1", Verdict::Pass, "src/a.rs:10");
    verdict(&st, "ac2", Verdict::Fail, "test b gagal");
    verdict(&st, "ac3", Verdict::CantVerify, "butuh browser");
    st.agent.last().done();
    let done = reviewed(&st, &run).await;
    assert_eq!(done.status, ReviewStatus::Finished);
    assert_eq!(done.verdicts.len(), 3);
    assert!(!done.stale);
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
    assert_eq!(st.runs.only().review_id, None);
    assert_eq!(st.agent.ended_reviews.lock().unwrap().clone(), [rec.id]);
    let notes = st.notifier.sent.lock().unwrap().clone();
    assert!(
        notes
            .last()
            .unwrap()
            .body
            .contains("1 passed, 1 failed, 1 can't verify"),
        "{notes:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ac_yang_tidak_dilaporkan_menjadi_cant_verify() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Pass, "ok");
    st.agent.last().done();
    reviewed(&st, &run).await;
    let rows = st.svc.ledger(&run).await.unwrap();
    assert_eq!(rows[2].verdict, Verdict::CantVerify);
    assert_eq!(rows[2].evidence.as_deref(), Some("reviewer did not report"));
}

#[tokio::test(flavor = "multi_thread")]
async fn batas_biaya_membuat_review_failed_dan_ac_sisa_tetap_not_reviewed() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Pass, "ok");
    st.agent.last().budget_stop();
    let done = reviewed(&st, &run).await;
    assert_eq!(done.status, ReviewStatus::Failed);
    assert_eq!(
        done.error.as_deref(),
        Some("Review stopped at the cost limit")
    );
    assert_eq!(done.cost_usd, Some(2.01));
    assert_eq!(done.usage.output_tokens, 5);
    let rows = st.svc.ledger(&run).await.unwrap();
    assert_eq!(rows[2].verdict, Verdict::NotReviewed);
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae4_agent_yang_menulis_berkas_saat_review_ditandai_stale() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    let wt = st.runs.only().worktree.unwrap();
    st.git.st().trees.insert(wt, "tree-diubah".to_owned());
    st.agent.last().done();
    let done = reviewed(&st, &run).await;
    assert!(done.stale);
    assert!(st.svc.ledger(&run).await.unwrap()[0].stale);
}

#[tokio::test(flavor = "multi_thread")]
async fn pesan_dev_selama_review_diantrekan_dan_push_ditolak() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let out = st.svc.send_message(&run, "tambah test").await.unwrap();
    assert_eq!(out, mandor_usecase::SendOutcome::Queued);
    assert!(matches!(
        st.svc.publish(&run).await,
        Err(RunError::InvalidState(_))
    ));
    let spawned = st.agent.runs.lock().unwrap().len();
    st.agent.last().done();
    eventually(|| st.agent.runs.lock().unwrap().len() == spawned + 1).await;
    let next = st.agent.last().req;
    assert!(next.review.is_none());
    assert_eq!(next.message, "tambah test");
    assert!(matches!(
        next.session,
        mandor_usecase::run_ports::SessionRef::Resume(_)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn kirim_temuan_hanya_untuk_baris_fail() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Fail, "salah");
    st.agent.last().done();
    reviewed(&st, &run).await;
    let err = st
        .svc
        .send_findings(&run, &["ac1".to_owned()])
        .await
        .unwrap_err();
    assert!(matches!(err, RunError::Port(PortError::Rejected(_))));
    st.svc
        .send_findings(&run, &["ac2".to_owned()])
        .await
        .unwrap();
    assert_eq!(st.agent.last().req.message, "FINDINGS ac2");
    assert_eq!(
        st.svc.ledger(&run).await.unwrap()[1].verdict,
        Verdict::FixInProgress
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn batal_review_dan_restart_daemon_membuat_review_tidak_selesai() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    st.svc.cancel_review(&run).await.unwrap();
    let done = reviewed(&st, &run).await;
    assert_eq!(done.status, ReviewStatus::Cancelled);
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);

    // Restart di tengah review: Failed, run menunggu dev, proses dihentikan.
    let second = st
        .svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    st.runs.rows.lock().unwrap().get_mut(&run).unwrap().0.pid = Some(4321);
    let marked = st.svc.reconcile().await.unwrap();
    assert_eq!(marked, std::slice::from_ref(&run));
    assert_eq!(st.runs.only().state, RunState::AwaitingDev);
    let last = st
        .reviews
        .reviews(&run)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.id == second.id)
        .unwrap();
    assert_eq!(last.status, ReviewStatus::Failed);
    assert!(st.agent.reaped.lock().unwrap().contains(&4321));
}

/// Satu review yang langsung selesai.
async fn cycle(st: &Station, run: &str) {
    st.svc
        .start_review(run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae5_browser_hanya_bila_semua_syarat_kesiapan_terpenuhi() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        ..AppSettings::default()
    };
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only().id;
    let wt: PathBuf = st.runs.only().worktree.unwrap();

    let reason = |st: &Station| {
        let msg = st.agent.last().req.message;
        msg.split("REASON:")
            .nth(1)
            .unwrap()
            .split(" SKILL:")
            .next()
            .unwrap()
            .to_owned()
    };
    // App tidak menyala.
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "The app is not running");

    let mut app = AppRecord {
        run_id: run.clone(),
        state: AppRunState::Running,
        members: vec![AppMemberRecord {
            repo_id: "repo1".to_owned(),
            url: Some("http://localhost:5173".to_owned()),
            pgid: Some(1),
            state: AppRunState::Running,
            prototype: None,
        }],
        snapshot: [("repo1".to_owned(), "tree-lama".to_owned())].into(),
        ..AppRecord::default()
    };
    st.apps.save_app_record(&app).await.unwrap();
    st.apps.set_app_owner(Some(&run)).await.unwrap();
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "app running older code");

    st.git.st().trees.insert(wt, "tree-s2".to_owned());
    app.snapshot
        .insert("repo1".to_owned(), "tree-s2".to_owned());
    st.apps.save_app_record(&app).await.unwrap();
    cycle(&st, &run).await;
    assert_eq!(
        reason(&st),
        "No test login is prepared for unattended review"
    );
    assert!(!st.agent.last().req.review.unwrap().browser);

    *st.lab.installed.lock().unwrap() = false;
    lab_ready(&st);
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "Playwright is not installed");

    *st.lab.installed.lock().unwrap() = true;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, Some("qa:review"))
        .await
        .unwrap();
    let req = st.agent.last().req;
    let review = req.review.unwrap();
    assert!(review.browser);
    assert_eq!(review.origins, ["http://localhost:5173"]);
    assert!(req.message.contains("SKILL:qa:review"));
    // Covers R15: review biasa login dengan akun default lewat proxy.
    let proxy = review.proxy.clone().unwrap();
    assert_eq!(
        st.lab.logins.lock().unwrap().last().unwrap(),
        &(
            review.review_id.clone(),
            "repo1".to_owned(),
            "admin".to_owned(),
            Some(proxy)
        )
    );
    assert_eq!(review.sessions.len(), 1);
    assert_eq!(review.sessions[0].server, "playwright");
    let (_, policy) = st.lab.proxies.lock().unwrap().last().cloned().unwrap();
    assert!(!policy.write_allowed);
    assert_eq!(policy.origins, ["http://localhost:5173"]);
    assert!(policy.safe_posts.contains(&"/api/login".to_owned()));

    // Uji browser dimatikan untuk repo.
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    st.repos.repos.lock().unwrap()[0].app.browser_tests = false;
    cycle(&st, &run).await;
    assert_eq!(reason(&st), "Browser tests are off");
}

#[tokio::test(flavor = "multi_thread")]
async fn setelan_reviewer_baru_dipakai_review_berikutnya_dan_prasyarat_dicek() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.reviews
        .set_reviewer_settings(&mandor_usecase::run_ports::ReviewerSettings {
            model: Some("haiku".to_owned()),
            budget_cents: 50,
            timeout_secs: 120,
        })
        .await
        .unwrap();
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let review = st.agent.last().req.review.unwrap();
    assert_eq!(review.model.as_deref(), Some("haiku"));
    assert_eq!(review.budget_cents, 50);
    assert_eq!(review.timeout.as_secs(), 120);
    // Review kedua saat yang pertama berjalan ditolak.
    assert!(matches!(
        st.svc.start_review(&run, ReviewTrigger::Manual, None).await,
        Err(RunError::InvalidState(_))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn kesiapan_per_item_menyebut_pemilik_app_lain_dan_dialog_hanya_untuk_web() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    let r = st.svc.review_readiness(&run).await.unwrap();
    assert!(!r.needs_dialog(), "repo tanpa app web");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        ..AppSettings::default()
    };
    st.apps.set_app_owner(Some("run-lain")).await.unwrap();
    let r = st.svc.review_readiness(&run).await.unwrap();
    assert!(r.needs_dialog());
    assert_eq!(r.app, mandor_usecase::review::AppReadiness::NotRunning);
    assert_eq!(r.other_owner.as_deref(), Some("run-lain"));
    assert_eq!(r.facts().reason.as_deref(), Some("The app is not running"));
}

#[tokio::test(flavor = "multi_thread")]
async fn skill_reviewer_dari_start_dipakai_kecuali_ada_skill_eksplisit() {
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .set_reviewer_skill(&run, Some("ac-check"))
        .await
        .unwrap();
    let rec = st
        .svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    assert_eq!(rec.skill.as_deref(), Some("ac-check"));
    assert!(st.agent.last().req.message.contains("SKILL:ac-check"));
    st.agent.last().done();
    reviewed(&st, &run).await;
    let notes = st.notifier.sent.lock().unwrap().clone();
    assert_eq!(
        notes.last().unwrap().kind,
        mandor_usecase::run_ports::NoticeKind::ReviewFinished
    );

    let rec = st
        .svc
        .start_review(&run, ReviewTrigger::Manual, Some("lain"))
        .await
        .unwrap();
    assert_eq!(rec.skill.as_deref(), Some("lain"));
}

/// Run Web yang menunggu dev, app-nya pernah menyala dari kode lama dengan
/// login dikonfirmasi, dan ekstensi tersambung. `auto` = setelan restart
/// otomatis repo.
async fn app_lama(auto: bool) -> (FakeLark, FakeClock, Station, String) {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        auto_restart: auto,
        ..AppSettings::default()
    };
    lab_ready(&st);
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only().id;
    let wt: PathBuf = st.runs.only().worktree.unwrap();
    st.git.st().trees.insert(wt, "tree-baru".to_owned());
    let app = AppRecord {
        run_id: run.clone(),
        state: AppRunState::Running,
        members: vec![AppMemberRecord {
            repo_id: "repo1".to_owned(),
            url: Some("http://localhost:5173".to_owned()),
            pgid: Some(1),
            state: AppRunState::Running,
            prototype: None,
        }],
        snapshot: [("repo1".to_owned(), "tree-lama".to_owned())].into(),
        login_confirmed: true,
        ..AppRecord::default()
    };
    st.apps.save_app_record(&app).await.unwrap();
    st.apps.set_app_owner(Some(&run)).await.unwrap();
    (lark, clock, st, run)
}

fn system_notes(st: &Station) -> Vec<String> {
    st.runs
        .chat
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(_, item)| match item {
            mandor_usecase::run_ports::ChatItem::System(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_otomatis_menyalakan_ulang_app_dari_kode_terbaru_sebelum_review() {
    let (_lark, _clock, st, run) = app_lama(true).await;
    // Review otomatis setelah Waiting Fix memakai jalur yang sama.
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let calls = st.apps.calls();
    assert!(calls.contains(&format!("stop:{run}")), "{calls:?}");
    assert!(calls.contains(&"spawn:repo1".to_owned()), "{calls:?}");
    let rec = st.apps.record(&run).unwrap();
    assert_eq!(rec.state, AppRunState::Running);
    assert_eq!(
        rec.snapshot.get("repo1").map(String::as_str),
        Some("tree-baru")
    );
    assert!(
        rec.login_confirmed,
        "restart oleh Mandor tidak mereset login"
    );
    let review = st.agent.last().req.review.unwrap();
    assert!(review.browser, "browser dipakai setelah restart");
    assert!(
        system_notes(&st)
            .iter()
            .any(|n| n.contains("Restarted the app")),
        "{:?}",
        system_notes(&st)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tanpa_setelan_restart_otomatis_app_lama_tidak_disentuh() {
    let (_lark, _clock, st, run) = app_lama(false).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    assert!(!st.apps.calls().iter().any(|c| c.starts_with("spawn:")));
    assert!(
        st.agent
            .last()
            .req
            .message
            .contains("REASON:app running older code")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_otomatis_tidak_merebut_app_run_lain_dan_butuh_sumber_login() {
    // App run lain sedang menyala: tidak dihentikan.
    let (_lark, _clock, st, run) = app_lama(true).await;
    st.apps.set_app_owner(Some("run-lain")).await.unwrap();
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let calls = st.apps.calls();
    assert!(!calls.iter().any(|c| c.starts_with("spawn:")), "{calls:?}");
    assert!(!calls.contains(&"stop:run-lain".to_owned()), "{calls:?}");
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;

    // Tidak ada sumber login (perlengkapan lab belum disetujui): restart
    // tidak berguna, dilewati.
    let (_lark, _clock, st, run) = app_lama(true).await;
    st.lab.equipment.lock().unwrap().clear();
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    assert!(!st.apps.calls().iter().any(|c| c.starts_with("spawn:")));
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_otomatis_gagal_review_tetap_berjalan_tanpa_browser() {
    let (_lark, _clock, st, run) = app_lama(true).await;
    st.apps
        .st
        .lock()
        .unwrap()
        .fail_ready
        .insert("repo1".to_owned());
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let review = st.agent.last().req.review.unwrap();
    assert!(!review.browser);
    assert!(
        system_notes(&st)
            .iter()
            .any(|n| n.contains("Could not restart the app")),
        "{:?}",
        system_notes(&st)
    );
}

/// Run VM yang agent-nya baru selesai dengan perubahan kode; review dimulai
/// otomatis lewat gerbang push. `before` dijalankan sebelum giliran berakhir.
async fn vm_review(before: impl AsyncFnOnce(&Station)) -> (FakeLark, FakeClock, Station, String) {
    let (lark, clock) = setup();
    let st = vm_station(&lark, &clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("python3 -m http.server 5150".to_owned()),
        port: Some(5150),
        browser_tests: true,
        auto_restart: true,
        unattended_login: true,
        ..AppSettings::default()
    };
    st.svc.start("rec1", "repo1", None).await.unwrap();
    let wt: PathBuf = st.runs.only().worktree.unwrap();
    st.git.set_changes(
        &wt,
        mandor_usecase::run_ports::WorkChanges {
            dirty: true,
            unpushed: 0,
            files: vec!["web/bmi.html".to_owned()],
            diff: "+bmi".to_owned(),
        },
    );
    before(&st).await;
    st.agent.last().done();
    eventually(|| st.agent.last().req.review.is_some()).await;
    let run = st.runs.only().id;
    (lark, clock, st, run)
}

/// Di VM tidak ada dev yang menekan Run app: review headless menyalakan app
/// run ini sendiri bila belum menyala (R11), selama restart otomatis aktif.
#[tokio::test(flavor = "multi_thread")]
async fn vm_menyalakan_app_yang_belum_menyala_sebelum_review_headless() {
    let (_lark, _clock, st, run) = vm_review(async |_| {}).await;
    let calls = st.apps.calls();
    assert!(calls.contains(&"spawn:repo1".to_owned()), "{calls:?}");
    assert_eq!(
        st.apps.app_owner().await.unwrap().as_deref(),
        Some(run.as_str())
    );
    assert!(st.agent.last().req.review.unwrap().browser);
    assert!(
        system_notes(&st)
            .iter()
            .any(|n| n.contains("Started the app")),
        "{:?}",
        system_notes(&st)
    );
}

/// Kartu lembur dikerjakan satu per satu: app kartu sebelumnya dihentikan
/// agar kartu berikutnya bisa di-review dengan browser.
#[tokio::test(flavor = "multi_thread")]
async fn vm_menghentikan_app_kartu_sebelumnya_sebelum_review_headless() {
    let (_lark, _clock, st, run) = vm_review(async |st| {
        st.apps
            .save_app_record(&AppRecord {
                run_id: "run-lain".to_owned(),
                state: AppRunState::Running,
                ..AppRecord::default()
            })
            .await
            .unwrap();
        st.apps.set_app_owner(Some("run-lain")).await.unwrap();
    })
    .await;
    let calls = st.apps.calls();
    assert!(calls.contains(&"stop:run-lain".to_owned()), "{calls:?}");
    assert!(calls.contains(&"spawn:repo1".to_owned()), "{calls:?}");
    assert_eq!(
        st.apps.app_owner().await.unwrap().as_deref(),
        Some(run.as_str())
    );
    assert_eq!(
        st.apps.record("run-lain").unwrap().state,
        AppRunState::StoppedByOtherRun
    );
    assert!(st.agent.last().req.review.unwrap().browser);
}

#[tokio::test(flavor = "multi_thread")]
async fn laptop_tidak_menyalakan_app_yang_belum_menyala() {
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        auto_restart: true,
        ..AppSettings::default()
    };
    lab_ready(&st);
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only().id;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    assert!(!st.apps.calls().iter().any(|c| c.starts_with("spawn:")));
}

/// Sumber desain `d1` yang bisa dijalankan dipasang ke run VM sebelum review.
fn vm_prototype(st: &Station, confirmed: Option<&str>, current: &str) {
    st.design.add_prototype(
        "d1",
        "kelola-web-prototype",
        AppSettings {
            test_mode: TestMode::Web,
            start_command: Some("pnpm dev".to_owned()),
            port: Some(5151),
            ..AppSettings::default()
        },
        confirmed,
    );
    st.design.set_revision("kelola-web-prototype", current);
    let mut r = st.runs.only();
    r.design = vec![mandor_usecase::review::DesignRoot {
        id: "d1".to_owned(),
        name: "kelola-web-prototype".to_owned(),
        path: PathBuf::from("/design/kelola-web-prototype"),
    }];
    st.runs.insert(r);
}

/// Plan 2026-11-03-002, R28/AE15: VM hanya menyalakan prototype pada revisi
/// yang dikonfirmasi dev.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae15_vm_menyalakan_prototype_hanya_pada_revisi_terkonfirmasi() {
    let (_lark, _clock, st, _run) =
        vm_review(async |st| vm_prototype(st, Some("git:a"), "git:a")).await;
    assert!(
        st.apps.calls().contains(&"spawn:prototype:d1".to_owned()),
        "{:?}",
        st.apps.calls()
    );

    let (_lark, _clock, st, run) =
        vm_review(async |st| vm_prototype(st, Some("git:a"), "git:b")).await;
    let calls = st.apps.calls();
    assert!(calls.contains(&"spawn:repo1".to_owned()), "{calls:?}");
    assert!(!calls.iter().any(|c| c.contains("prototype")), "{calls:?}");
    assert_eq!(st.apps.record(&run).unwrap().members.len(), 1);
}

/// Plan 2026-11-03-002, R23: baris yang hanya berbeda tampilannya bisa
/// dikirim ke agent, dan hasil tampilannya dibandingkan ulang.
#[tokio::test(flavor = "multi_thread")]
async fn kirim_temuan_untuk_baris_yang_tampilannya_berbeda() {
    use mandor_domain::ac_ledger::Visual;
    use mandor_usecase::run_ports::LedgerStore as _;
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Pass, "ok");
    st.agent.last().done();
    reviewed(&st, &run).await;
    let mut rows = st.svc.ledger(&run).await.unwrap();
    rows[0].visual = Visual::Differs;
    rows[0].visual_detail.description = Some("tombol abu-abu, prototype biru".to_owned());
    st.reviews.save_ac_rows(&run, &rows).await.unwrap();
    assert!(
        st.svc
            .send_findings(&run, &["ac2".to_owned()])
            .await
            .is_err()
    );
    st.svc
        .send_findings(&run, &["ac1".to_owned()])
        .await
        .unwrap();
    let row = &st.svc.ledger(&run).await.unwrap()[0];
    assert_eq!(row.verdict, Verdict::FixInProgress);
    assert_eq!(row.visual, Visual::NotCompared);
}

/// Run laptop yang app dan prototype-nya menyala dari kode terbaru, login
/// dikonfirmasi, ekstensi tersambung, ac1 In design dan ac2 Conflicts.
async fn with_running_prototype() -> (FakeLark, FakeClock, Station, String) {
    use mandor_domain::Design;
    use mandor_usecase::run_ports::LedgerStore as _;
    let (lark, clock) = setup();
    let st = station(&lark, &clock, "ou_me");
    let app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        ..AppSettings::default()
    };
    st.repos.repos.lock().unwrap()[0].app = app.clone();
    lab_ready(&st);
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only().id;
    st.design.add_prototype(
        "d1",
        "kelola-web-prototype",
        AppSettings {
            port: Some(3001),
            app_url: Some("http://127.0.0.1:3001".to_owned()),
            ..app
        },
        Some("git:a"),
    );
    st.design.set_revision("kelola-web-prototype", "git:a");
    let mut r = st.runs.only();
    r.design = vec![mandor_usecase::review::DesignRoot {
        id: "d1".to_owned(),
        name: "kelola-web-prototype".to_owned(),
        path: PathBuf::from("/design/kelola-web-prototype"),
    }];
    st.runs.insert(r);
    let wt: PathBuf = st.runs.only().worktree.unwrap();
    st.git.st().trees.insert(wt, "tree-1".to_owned());
    let rec = AppRecord {
        run_id: run.clone(),
        state: AppRunState::Running,
        members: vec![
            AppMemberRecord {
                repo_id: "repo1".to_owned(),
                url: Some("http://localhost:5173".to_owned()),
                pgid: Some(1),
                state: AppRunState::Running,
                prototype: None,
            },
            AppMemberRecord {
                repo_id: "prototype:d1".to_owned(),
                url: Some("http://127.0.0.1:3001".to_owned()),
                pgid: Some(2),
                state: AppRunState::Running,
                prototype: Some("kelola-web-prototype".to_owned()),
            },
        ],
        snapshot: [
            ("repo1".to_owned(), "tree-1".to_owned()),
            ("prototype:d1".to_owned(), "git:a".to_owned()),
        ]
        .into(),
        login_confirmed: true,
        ..AppRecord::default()
    };
    st.apps.save_app_record(&rec).await.unwrap();
    st.apps.set_app_owner(Some(&run)).await.unwrap();
    let mut rows = st.svc.ledger(&run).await.unwrap();
    rows[0].design = Design::InDesign;
    rows[1].design = Design::Conflicts;
    st.reviews.save_ac_rows(&run, &rows).await.unwrap();
    (lark, clock, st, run)
}

/// Plan 2026-11-03-002, R16/R19: reviewer mendapat URL prototype dan hanya
/// origin app serta prototype.
#[tokio::test(flavor = "multi_thread")]
async fn reviewer_mendapat_prototype_dan_origin_app_serta_prototype() {
    let (_lark, _clock, st, run) = with_running_prototype().await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let req = st.agent.last().req;
    let review = req.review.unwrap();
    assert!(review.browser, "{}", req.message);
    assert_eq!(
        review.origins,
        ["http://localhost:5173", "http://127.0.0.1:3001"]
    );
    assert!(
        req.message.contains("PROTO:http://127.0.0.1:3001 LOOK:ac1"),
        "{}",
        req.message
    );
}

/// Plan 2026-11-03-002, R24: baris di desain yang tidak dibandingkan
/// reviewer menjadi Can't verify untuk tampilan; baris Conflicts tidak.
#[tokio::test(flavor = "multi_thread")]
async fn baris_desain_yang_tidak_dibandingkan_menjadi_cant_verify() {
    use mandor_domain::ac_ledger::Visual;
    let (_lark, _clock, st, run) = with_running_prototype().await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Pass, "ok");
    st.agent.last().done();
    reviewed(&st, &run).await;
    let rows = st.svc.ledger(&run).await.unwrap();
    assert_eq!(rows[0].visual, Visual::CantVerify);
    assert!(rows[0].visual_detail.description.is_some());
    assert_eq!(rows[1].visual, Visual::NotCompared);
}

/// Plan 2026-11-03-002, R22/AE14: prototype berubah setelah review membuat
/// hasil review usang, seperti kode yang berubah.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae14_prototype_berubah_membuat_hasil_review_usang() {
    let (_lark, _clock, st, run) = with_running_prototype().await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    verdict(&st, "ac1", Verdict::Pass, "ok");
    verdict(&st, "ac2", Verdict::Pass, "ok");
    st.agent.last().done();
    reviewed(&st, &run).await;
    assert!(st.runs.only().review_snapshot.contains_key("prototype:d1"));
    let decision = mandor_domain::push_gate::GateDecision::default();
    assert!(matches!(
        st.svc.publish_gated(&run, &decision).await.unwrap(),
        mandor_usecase::run::PublishOutcome::NeedsConfirmation { .. }
    ));
    st.design.set_revision("kelola-web-prototype", "git:b");
    assert!(matches!(
        st.svc.publish_gated(&run, &decision).await.unwrap(),
        mandor_usecase::run::PublishOutcome::ReviewStarted { .. }
    ));
}

/// Plan 2026-11-03-002, U7/R21/R26: run VM menyimpan salinan screenshot,
/// berkas yang ditolak tidak menggagalkan hasil, dan salinan dihapus saat
/// run ditutup.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae15_screenshot_vm_disimpan_lalu_dihapus_saat_run_ditutup() {
    use mandor_domain::Design;
    use mandor_domain::ac_ledger::Visual;
    use mandor_usecase::run_ports::LedgerStore as _;
    let (_lark, _clock, st, run) =
        vm_review(async |st| vm_prototype(st, Some("git:a"), "git:a")).await;
    let mut rows = st.svc.ledger(&run).await.unwrap();
    rows[0].design = Design::InDesign;
    st.reviews.save_ac_rows(&run, &rows).await.unwrap();
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    let report = |shots: &[&str]| mandor_usecase::review::VisualReport {
        description: Some("beda".to_owned()),
        app_path: Some("/a".to_owned()),
        prototype_path: Some("/a".to_owned()),
        screenshots: shots.iter().map(|s| (*s).to_owned()).collect(),
    };
    let rows = st
        .svc
        .ledger_set_visual(
            &scope,
            "ac1",
            Visual::Differs,
            report(&["app.png", "proto.png"]),
        )
        .await
        .unwrap();
    assert_eq!(
        rows[0].visual_detail.screenshots,
        ["ac1-0.png", "ac1-1.png"]
    );
    let review_id = st.runs.only().review_id.unwrap();
    assert_eq!(st.agent.kept.lock().unwrap()[0].0, review_id);
    assert_eq!(
        st.svc.visual_screenshot(&run, "ac1", 1).await.unwrap(),
        format!("{run}/ac1-1.png").into_bytes()
    );
    assert!(st.svc.visual_screenshot(&run, "ac1", 2).await.is_err());
    assert!(st.svc.visual_screenshot(&run, "ac2", 0).await.is_err());

    let rows = st
        .svc
        .ledger_set_visual(&scope, "ac1", Visual::Differs, report(&["besar.png"]))
        .await
        .unwrap();
    assert_eq!(rows[0].visual, Visual::Differs, "hasil tetap tersimpan");
    assert!(rows[0].visual_detail.screenshots.is_empty());

    st.svc.close_run(&run).await.unwrap();
    assert_eq!(*st.agent.forgotten.lock().unwrap(), [run]);
}

/// Plan 2026-11-06-001, AE5: reviewer tidak mendapat kartu, dan perintah di
/// luar allowlist yang ditolak tercatat tepat satu item Chat walau event sesi
/// reviewer tidak masuk Chat.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae5_penolakan_reviewer_tercatat_satu_item_chat_tanpa_kartu() {
    use mandor_usecase::run_ports::ChatItem;
    let (lark, clock) = setup();
    let (st, run) = waiting(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let rv = st.agent.last();
    assert!(rv.req.permit.is_none(), "reviewer tanpa alat persetujuan");
    rv.emit(mandor_domain::AgentEvent::PermissionDenied {
        tool_name: "Bash".to_owned(),
        tool_use_id: "t1".to_owned(),
        reason: "not allowed".to_owned(),
    });
    rv.finish_denied(vec![mandor_domain::Denial {
        tool_name: "Bash".to_owned(),
        tool_use_id: "t1".to_owned(),
        input: "docker ps".to_owned(),
    }]);
    reviewed(&st, &run).await;
    let denied: Vec<String> = st
        .runs
        .chat
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(_, i)| match i {
            ChatItem::System(t) if t.contains("docker ps") => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(denied.len(), 1, "{denied:?}");
    assert!(st.approvals.cards.lock().unwrap().is_empty(), "tanpa kartu");
}

/// Perlengkapan lab disetujui untuk repo1 dengan akun default `admin`.
fn lab_ready(st: &Station) {
    use mandor_domain::lab::{LabAccount, LabEquipment, LoginForm, SuccessMarker};
    st.lab.equipment.lock().unwrap().insert(
        "repo1".to_owned(),
        LabEquipment {
            accounts: vec![LabAccount {
                role: "admin".to_owned(),
                username: "admin@uji.test".to_owned(),
                default: true,
            }],
            login: LoginForm {
                url: "http://localhost:5173/login".to_owned(),
                username_field: "#email".to_owned(),
                password_field: "#password".to_owned(),
                submit: "text=Sign in".to_owned(),
                submit_path: "/api/login".to_owned(),
                success: SuccessMarker::PathPrefix("/admin".to_owned()),
                authenticated_url: "http://localhost:5173/admin".to_owned(),
            },
            safe_posts: vec![],
            fixtures_dir: None,
        },
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_gagal_membuat_review_tanpa_browser_dan_proxy_dibersihkan() {
    let (lark, clock) = setup();
    let (st, run) = waiting_with_app(&lark, &clock).await;
    lab_ready(&st);
    *st.lab.login_failure.lock().unwrap() =
        Some(mandor_usecase::run_ports::LoginFailure::NotSignedIn);
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let req = st.agent.last().req;
    let review = req.review.clone().unwrap();
    assert!(!review.browser);
    assert!(review.sessions.is_empty());
    assert!(
        req.message
            .contains("Test login failed for admin: the signed-in page did not appear"),
        "{}",
        req.message
    );
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(st.lab.ended.lock().unwrap().contains(&review.review_id));
}

#[tokio::test(flavor = "multi_thread")]
async fn app_bukan_loopback_tidak_login_dengan_akun_uji() {
    let (lark, clock) = setup();
    let (st, run) = waiting_with_app(&lark, &clock).await;
    lab_ready(&st);
    let mut app = st.apps.app_record(&run).await.unwrap().unwrap();
    app.members[0].url = Some("https://staging.example.com".to_owned());
    st.apps.save_app_record(&app).await.unwrap();
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let review = st.agent.last().req.review.unwrap();
    assert!(!review.browser);
    assert!(st.lab.logins.lock().unwrap().is_empty());
}

/// Run Web menunggu dev dengan app menyala dari kode terbaru.
async fn waiting_with_app(lark: &FakeLark, clock: &FakeClock) -> (Station, String) {
    let st = station(lark, clock, "ou_me");
    st.repos.repos.lock().unwrap()[0].app = AppSettings {
        test_mode: TestMode::Web,
        start_command: Some("bun run dev".to_owned()),
        port: Some(5173),
        browser_tests: true,
        ..AppSettings::default()
    };
    st.svc.start("rec1", "repo1", None).await.unwrap();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let run = st.runs.only().id;
    let wt: PathBuf = st.runs.only().worktree.unwrap();
    st.git.st().trees.insert(wt, "tree-s2".to_owned());
    let app = AppRecord {
        run_id: run.clone(),
        state: AppRunState::Running,
        members: vec![AppMemberRecord {
            repo_id: "repo1".to_owned(),
            url: Some("http://localhost:5173".to_owned()),
            pgid: Some(1),
            state: AppRunState::Running,
            prototype: None,
        }],
        snapshot: [("repo1".to_owned(), "tree-s2".to_owned())].into(),
        ..AppRecord::default()
    };
    st.apps.save_app_record(&app).await.unwrap();
    st.apps.set_app_owner(Some(&run)).await.unwrap();
    (st, run)
}

/// Review berjalan dengan browser (perlengkapan lab disetujui) dan token
/// verdict.
async fn browser_review(
    lark: &FakeLark,
    clock: &FakeClock,
) -> (Station, String, mandor_usecase::review::CapabilityScope) {
    let (st, run) = waiting_with_app(lark, clock).await;
    lab_ready(&st);
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    assert!(st.agent.last().req.review.unwrap().browser);
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    (st, run, scope)
}

fn report_with(steps: &[&str]) -> mandor_usecase::review::VerdictReport {
    mandor_usecase::review::VerdictReport {
        evidence: Some("diperiksa di browser".to_owned()),
        steps: steps.iter().map(|s| (*s).to_owned()).collect(),
        ..mandor_usecase::review::VerdictReport::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pass_lewat_browser_butuh_langkah_dan_bukti_tersimpan() {
    use mandor_domain::lab::Simulated;
    use mandor_usecase::review::LedgerToolError;
    let (lark, clock) = setup();
    let (st, run, scope) = browser_review(&lark, &clock).await;
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    assert!(matches!(
        st.svc
            .ledger_set_verdict(&scope, &ac, Verdict::Pass, report_with(&[]))
            .await,
        Err(LedgerToolError::NotAllowed(_))
    ));
    let mut r = report_with(&["Buka /admin", "Klik Save"]);
    r.evidence = None;
    r.screenshots = vec!["admin.png".to_owned()];
    r.simulated = Simulated::Request;
    r.prepared = Some("akun admin; POST /api/kb digagalkan 500".to_owned());
    let rows = st
        .svc
        .ledger_set_verdict(&scope, &ac, Verdict::Pass, r)
        .await
        .unwrap();
    let row = rows.iter().find(|x| x.ac_id == ac).unwrap();
    assert_eq!(row.verdict, Verdict::Pass);
    assert_eq!(
        row.evidence.as_deref(),
        Some("1. Buka /admin\n2. Klik Save")
    );
    let detail = row.evidence_detail.clone().unwrap();
    assert_eq!(detail.simulated, Simulated::Request);
    assert_eq!(detail.steps.len(), 2);
    assert!(!detail.screenshots.is_empty());
    assert!(detail.prepared.unwrap().contains("POST /api/kb"));
}

#[tokio::test(flavor = "multi_thread")]
async fn lab_need_dan_sql_dari_agent_ditolak() {
    use mandor_domain::lab::{LabNeed, Simulated};
    use mandor_usecase::review::LedgerToolError;
    let (lark, clock) = setup();
    let (st, run, scope) = browser_review(&lark, &clock).await;
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    let mut r = report_with(&["x"]);
    r.lab_need = Some(LabNeed::Role);
    assert!(matches!(
        st.svc
            .ledger_set_verdict(&scope, &ac, Verdict::Pass, r)
            .await,
        Err(LedgerToolError::NotAllowed(_))
    ));
    let mut r = report_with(&["x"]);
    r.simulated = Simulated::Sql;
    assert!(matches!(
        st.svc
            .ledger_set_verdict(&scope, &ac, Verdict::Fail, r)
            .await,
        Err(LedgerToolError::NotAllowed(_))
    ));
    let mut r = report_with(&[]);
    r.lab_need = Some(LabNeed::Role);
    let rows = st
        .svc
        .ledger_set_verdict(&scope, &ac, Verdict::CantVerify, r)
        .await
        .unwrap();
    assert_eq!(
        rows.iter().find(|x| x.ac_id == ac).unwrap().lab_need,
        Some(LabNeed::Role)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tulis_yang_diblokir_proxy_membuat_cant_verify_tetapi_origin_lain_tidak() {
    use mandor_domain::lab::{BlockedRequest, LabNeed};
    use mandor_usecase::run_ports::ProxyHit;
    let (lark, clock) = setup();
    let (st, run, scope) = browser_review(&lark, &clock).await;
    let review_id = st.agent.last().req.review.unwrap().review_id;
    let rows = st.svc.ledger(&run).await.unwrap();
    let (a1, a2) = (rows[0].ac_id.clone(), rows[1].ac_id.clone());
    let hit = |method: &str, url: &str, write: bool| ProxyHit {
        at_ms: 1,
        request: BlockedRequest {
            method: method.to_owned(),
            url: url.to_owned(),
        },
        write,
    };
    st.lab.blocked.lock().unwrap().insert(
        review_id.clone(),
        vec![hit("GET", "https://fonts.example.com/a.css", false)],
    );
    let rows = st
        .svc
        .ledger_set_verdict(&scope, &a1, Verdict::Fail, report_with(&["Klik Save"]))
        .await
        .unwrap();
    let r1 = rows.iter().find(|x| x.ac_id == a1).unwrap();
    assert_eq!(
        r1.verdict,
        Verdict::Fail,
        "blokir origin lain tidak mengubah verdict"
    );
    assert_eq!(
        r1.evidence_detail.clone().unwrap().blocked_requests.len(),
        1
    );

    st.lab.blocked.lock().unwrap().insert(
        review_id,
        vec![hit("POST", "http://localhost:5173/api/kb", true)],
    );
    let rows = st
        .svc
        .ledger_set_verdict(&scope, &a2, Verdict::Pass, report_with(&["Klik Save"]))
        .await
        .unwrap();
    let r2 = rows.iter().find(|x| x.ac_id == a2).unwrap();
    assert_eq!(r2.verdict, Verdict::CantVerify);
    assert_eq!(r2.lab_need, Some(LabNeed::Data));
    assert!(
        r2.evidence
            .as_deref()
            .unwrap()
            .contains("POST http://localhost:5173/api/kb")
    );

    // URL tanpa query string; AC tak dikenal tidak menghabiskan penolakan.
    st.lab.blocked.lock().unwrap().insert(
        st.agent.last().req.review.unwrap().review_id,
        vec![hit(
            "POST",
            "http://localhost:5173/api/kb?token=abc123",
            true,
        )],
    );
    assert!(
        st.svc
            .ledger_set_verdict(&scope, "tidak-ada", Verdict::Fail, report_with(&["x"]))
            .await
            .is_err()
    );
    let rows = st
        .svc
        .ledger_set_verdict(&scope, &a1, Verdict::CantVerify, report_with(&["x"]))
        .await
        .unwrap();
    let r1 = rows.iter().find(|x| x.ac_id == a1).unwrap();
    let blocked = r1.evidence_detail.clone().unwrap().blocked_requests;
    assert_eq!(blocked[0].url, "http://localhost:5173/api/kb");
}

#[tokio::test(flavor = "multi_thread")]
async fn password_akun_uji_disamarkan_di_bukti_dan_temuan() {
    let (lark, clock) = setup();
    let (st, run, scope) = browser_review(&lark, &clock).await;
    st.lab
        .secrets
        .lock()
        .unwrap()
        .push("rahasia-uji-9".to_owned());
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    let rows = st
        .svc
        .ledger_set_verdict(
            &scope,
            &ac,
            Verdict::Pass,
            report_with(&["Isi password rahasia-uji-9", "Klik Sign in"]),
        )
        .await
        .unwrap();
    let row = rows.iter().find(|x| x.ac_id == ac).unwrap();
    let steps = row.evidence_detail.clone().unwrap().steps;
    assert_eq!(steps[0], "Isi password [redacted]");
    assert!(
        !row.evidence
            .clone()
            .unwrap_or_default()
            .contains("rahasia-uji-9")
    );

    let before = st.svc.ledger(&run).await.unwrap();
    st.svc
        .ledger_report_finding(
            &scope,
            mandor_domain::lab::Finding {
                card: None,
                title: "Tombol Save tidak aktif".to_owned(),
                detail: "Setelah login rahasia-uji-9 tombol mati".to_owned(),
                evidence: mandor_domain::lab::Evidence::default(),
            },
        )
        .await
        .unwrap();
    let findings = st.reviews.findings.lock().unwrap().clone();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].1.detail, "Setelah login [redacted] tombol mati");
    assert_eq!(
        st.svc.ledger(&run).await.unwrap(),
        before,
        "temuan tidak mengubah verdict"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rekomendasi_tampilan_hanya_untuk_baris_berbeda() {
    use mandor_domain::ac_ledger::Visual;
    use mandor_domain::lab::LookAdvice;
    use mandor_usecase::review::LedgerToolError;
    let (lark, clock) = setup();
    let (st, run, scope) = browser_review(&lark, &clock).await;
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    assert!(matches!(
        st.svc
            .ledger_recommend_look(&scope, &ac, LookAdvice::Accept, "ikut AC")
            .await,
        Err(LedgerToolError::NotAllowed(_))
    ));
    {
        let mut ledgers = st.reviews.ledgers.lock().unwrap();
        let rows = ledgers.get_mut(&run).unwrap();
        rows[0].visual = Visual::Differs;
    }
    let rows = st
        .svc
        .ledger_recommend_look(&scope, &ac, LookAdvice::Accept, "AC menulis Key Behavior")
        .await
        .unwrap();
    let rec = rows[0].look_recommendation.clone().unwrap();
    assert_eq!(rec.advice, LookAdvice::Accept);
    assert_eq!(rec.reason, "AC menulis Key Behavior");
}

/// Verdict dari sesi agent terakhir (review biasa atau lab).
async fn set_from_last(
    st: &Station,
    ac: &str,
    v: Verdict,
    report: mandor_usecase::review::VerdictReport,
) {
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    st.svc
        .ledger_set_verdict(&scope, ac, v, report)
        .await
        .unwrap();
}

fn needs(need: mandor_domain::lab::LabNeed) -> mandor_usecase::review::VerdictReport {
    mandor_usecase::review::VerdictReport {
        evidence: Some("butuh role lain".to_owned()),
        lab_need: Some(need),
        ..mandor_usecase::review::VerdictReport::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae1_lab_menguji_calon_setelah_review_biasa() {
    use mandor_domain::lab::LabNeed;
    let (lark, clock) = setup();
    let (st, run, _scope) = browser_review(&lark, &clock).await;
    let review_id = st.agent.last().req.review.unwrap().review_id;
    let rows = st.svc.ledger(&run).await.unwrap();
    let (a1, a2) = (rows[0].ac_id.clone(), rows[1].ac_id.clone());
    set_from_last(&st, &a1, Verdict::CantVerify, needs(LabNeed::Role)).await;
    set_from_last(&st, &a2, Verdict::CantVerify, needs(LabNeed::Simulation)).await;
    st.agent.last().done();
    eventually(|| st.agent.last().req.review.is_some_and(|r| r.lab.is_some())).await;
    let lab_req = st.agent.last().req;
    let lab = lab_req.review.clone().unwrap();
    assert_eq!(lab.review_id, format!("{review_id}-lab0"));
    assert_eq!(lab.sessions[0].server, "playwright_admin");
    assert!(lab.browser);
    assert_eq!(lab.budget_cents, 300);
    assert!(
        lab_req.message.contains(&format!("AC:{a1}|{a2}")),
        "{}",
        lab_req.message
    );
    assert!(lab_req.message.contains("ROLES:admin"));
    assert_eq!(st.runs.only().state, RunState::Reviewing);

    // Baris di luar calon lab kartu ini tertutup bagi sesi lab.
    let a3 = rows[2].ac_id.clone();
    let token = st.agent.last().req.ledger.unwrap().token;
    let scope = st.svc.verify_capability(token.expose()).unwrap();
    assert!(matches!(
        st.svc
            .ledger_set_verdict(&scope, &a3, Verdict::Pass, report_with(&["x"]))
            .await,
        Err(mandor_usecase::review::LedgerToolError::NotAllowed(_))
    ));
    let mut pass = report_with(&["Login sebagai admin", "Buka /x"]);
    pass.prepared = Some("akun admin".to_owned());
    set_from_last(&st, &a1, Verdict::Pass, pass).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let rows = st.svc.ledger(&run).await.unwrap();
    let r1 = rows.iter().find(|r| r.ac_id == a1).unwrap();
    let r2 = rows.iter().find(|r| r.ac_id == a2).unwrap();
    assert_eq!(r1.verdict, Verdict::Pass);
    assert_eq!(r2.verdict, Verdict::CantVerify);
    assert_eq!(r2.lab_need, Some(LabNeed::Simulation));
    assert!(
        r2.evidence.as_deref().unwrap().contains("did not report"),
        "{:?}",
        r2.evidence
    );
    let record = st.reviews.reviews(&run).await.unwrap().remove(0);
    assert_eq!(record.status, ReviewStatus::Finished);
    assert!(st.lab.ended.lock().unwrap().contains(&review_id));
    // Pass lab tidak butuh konfirmasi manual: ia Pass penuh.
    assert_eq!(r1.effective_verdict(), Verdict::Pass);
    // Badge "Lab" hanya untuk verdict dari giliran lab.
    assert!(r1.evidence_detail.as_ref().is_some_and(|d| d.lab));
    assert!(!r2.evidence_detail.as_ref().is_some_and(|d| d.lab));
}

#[tokio::test(flavor = "multi_thread")]
async fn covers_ae7_tanpa_perlengkapan_lab_tidak_jalan_dan_ada_catatan() {
    use mandor_domain::lab::LabNeed;
    let (lark, clock) = setup();
    let (st, run) = waiting_with_app(&lark, &clock).await;
    st.svc
        .start_review(&run, ReviewTrigger::Manual, None)
        .await
        .unwrap();
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    set_from_last(&st, &ac, Verdict::CantVerify, needs(LabNeed::Role)).await;
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert!(st.agent.last().req.review.unwrap().lab.is_none());
    let chat = st.runs.chat.lock().unwrap().clone();
    assert!(
        chat.iter()
            .any(|(_, c)| matches!(c, mandor_usecase::run_ports::ChatItem::System(t) if t.contains("need the review lab"))),
        "{chat:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn batas_lab_per_run_membuat_kartu_sisa_tetap_cant_verify() {
    use mandor_domain::lab::LabNeed;
    let (lark, clock) = setup();
    let (st, run, _scope) = browser_review(&lark, &clock).await;
    st.reviews
        .set_lab_settings(&mandor_usecase::run_ports::LabSettings {
            run_budget_cents: 0,
            ..mandor_usecase::run_ports::LabSettings::default()
        })
        .await
        .unwrap();
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    set_from_last(&st, &ac, Verdict::CantVerify, needs(LabNeed::File)).await;
    let before = st.agent.runs.lock().unwrap().len();
    st.agent.last().done();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    assert_eq!(
        st.agent.runs.lock().unwrap().len(),
        before,
        "tidak ada sesi lab"
    );
    let row = st.svc.ledger(&run).await.unwrap()[0].clone();
    assert_eq!(row.verdict, Verdict::CantVerify);
    assert!(row.evidence.unwrap().contains("run limit"));
}

#[tokio::test(flavor = "multi_thread")]
async fn batal_di_tengah_lab_menutup_review_sebagai_cancelled() {
    use mandor_domain::lab::LabNeed;
    let (lark, clock) = setup();
    let (st, run, _scope) = browser_review(&lark, &clock).await;
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    set_from_last(&st, &ac, Verdict::CantVerify, needs(LabNeed::Role)).await;
    st.agent.last().done();
    eventually(|| st.agent.last().req.review.is_some_and(|r| r.lab.is_some())).await;
    st.svc.cancel_review(&run).await.unwrap();
    eventually(|| st.runs.only().state == RunState::AwaitingDev).await;
    let record = st.reviews.reviews(&run).await.unwrap().remove(0);
    assert_eq!(record.status, ReviewStatus::Cancelled);
    let row = st.svc.ledger(&run).await.unwrap()[0].clone();
    assert_eq!(
        row.verdict,
        Verdict::CantVerify,
        "hasil review biasa dipulihkan"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn login_role_gagal_disebut_di_pesan_lab() {
    use mandor_domain::lab::LabNeed;
    let (lark, clock) = setup();
    let (st, run, _scope) = browser_review(&lark, &clock).await;
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    set_from_last(&st, &ac, Verdict::CantVerify, needs(LabNeed::Role)).await;
    *st.lab.login_failure.lock().unwrap() =
        Some(mandor_usecase::run_ports::LoginFailure::FormNotFound);
    st.agent.last().done();
    eventually(|| st.agent.last().req.review.is_some_and(|r| r.lab.is_some())).await;
    let req = st.agent.last().req;
    assert!(req.message.contains("UNAVAILABLE:admin"), "{}", req.message);
    assert!(!req.review.unwrap().browser);
}

/// R18, AE6 (KTD-L13): Approve rekomendasi accept menerima perbedaan; Reject
/// membiarkan baris *looks different*; baris tanpa rekomendasi ditolak.
#[tokio::test(flavor = "multi_thread")]
async fn covers_ae6_keputusan_rekomendasi_tampilan() {
    use mandor_domain::ac_ledger::Visual;
    use mandor_domain::lab::{LookAdvice, LookRecommendation};
    let (lark, clock) = setup();
    let (st, run) = waiting_with_app(&lark, &clock).await;
    let mut rows = st.svc.ledger(&run).await.unwrap();
    for r in rows.iter_mut().take(2) {
        r.visual = Visual::Differs;
        r.look_recommendation = Some(LookRecommendation {
            advice: LookAdvice::Accept,
            reason: "App follows the AC: Key Behavior".to_owned(),
        });
    }
    rows[2].visual = Visual::Differs;
    let (a1, a2, a3) = (
        rows[0].ac_id.clone(),
        rows[1].ac_id.clone(),
        rows[2].ac_id.clone(),
    );
    st.reviews.ledgers.lock().unwrap().insert(run.clone(), rows);

    let rows = st
        .svc
        .decide_look_recommendation(&run, &a1, true, Some("Beni"))
        .await
        .unwrap();
    let r1 = rows.iter().find(|r| r.ac_id == a1).unwrap();
    assert_eq!(r1.visual, Visual::Accepted);
    assert_eq!(r1.visual_detail.decided_by.as_deref(), Some("Beni"));

    let rows = st
        .svc
        .decide_look_recommendation(&run, &a2, false, Some("Beni"))
        .await
        .unwrap();
    let r2 = rows.iter().find(|r| r.ac_id == a2).unwrap();
    assert_eq!(r2.visual, Visual::Differs, "push tetap tertahan");
    assert!(r2.look_recommendation.is_none());

    assert!(matches!(
        st.svc
            .decide_look_recommendation(&run, &a3, true, None)
            .await,
        Err(RunError::InvalidState(_))
    ));
    assert!(matches!(
        st.svc
            .decide_look_recommendation(&run, "tidak-ada", true, None)
            .await,
        Err(RunError::NotFound(_))
    ));
}

/// R16, R19: screenshot bukti verdict bisa dibaca dan temuan run terdaftar.
#[tokio::test(flavor = "multi_thread")]
async fn screenshot_bukti_dan_temuan_run() {
    let (lark, clock) = setup();
    let (st, run, scope) = browser_review(&lark, &clock).await;
    let ac = st.svc.ledger(&run).await.unwrap()[0].ac_id.clone();
    let mut r = report_with(&["Buka /x"]);
    r.screenshots = vec!["page.png".to_owned()];
    st.svc
        .ledger_set_verdict(&scope, &ac, Verdict::Pass, r)
        .await
        .unwrap();
    let bytes = st.svc.evidence_screenshot(&run, &ac, 0).await.unwrap();
    assert_eq!(bytes, format!("{run}/{ac}-0.png").into_bytes());
    assert!(matches!(
        st.svc.evidence_screenshot(&run, &ac, 1).await,
        Err(RunError::NotFound(_))
    ));
    st.svc
        .ledger_report_finding(
            &scope,
            mandor_domain::lab::Finding {
                card: None,
                title: "Toast overlaps the header".to_owned(),
                detail: "Seen while saving".to_owned(),
                evidence: mandor_domain::lab::Evidence::default(),
            },
        )
        .await
        .unwrap();
    let findings = st.svc.run_findings(&run).await.unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].1.title, "Toast overlaps the header");
}
