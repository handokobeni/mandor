//! Protokol JSON-RPC 2.0 antara `mandord` dan client (KTD-2, KTD-16).
//!
//! Satu envelope yang konsisten untuk semua method. Tipe TypeScript untuk
//! client dihasilkan dari tipe di sini dengan `ts-rs`; test `ts_bindings`
//! memastikan file TypeScript tidak menyimpang.

mod dto;
mod envelope;
mod hello;

pub use dto::*;

pub use envelope::{ErrorCode, Incoming, Notification, Request, Response, RpcError};
pub use hello::{Hello, HelloOk, PROTOCOL_MAJOR, PROTOCOL_VERSION, major_of};

/// Nama method yang dikenal daemon.
pub mod method {
    /// Pesan pertama setiap koneksi: autentikasi token dan versi protokol.
    pub const HELLO: &str = "hello";
    /// Status daemon (pengganti `/healthz` untuk daemon lokal).
    pub const STATUS: &str = "status";
    /// Bersiap untuk pembaruan: menolak kerja baru bila tidak ada run yang
    /// bekerja (`DrainResult`). Jendela berakhir sendiri setelah 2 menit.
    pub const DAEMON_DRAIN: &str = "daemon.drain";
    /// Membatalkan drain (`{}`).
    pub const DAEMON_UNDRAIN: &str = "daemon.undrain";
    pub const SYSTEM_CHECK: &str = "system.check";
    pub const LARK_SETTINGS_GET: &str = "lark.settings.get";
    pub const LARK_SETTINGS_SET: &str = "lark.settings.set";
    /// Mengubah jeda tenang Waiting Fix tanpa menyentuh pengaturan Lark lain.
    pub const WAITING_FIX_QUIET_SET: &str = "waiting_fix.quiet.set";
    /// Buku AC run (`RunRef` → `AcLedgerView`), di-refresh dari tracker dulu.
    pub const LEDGER_GET: &str = "ledger.get";
    /// Koneksi kapabilitas (`mandord mcp`): lingkup token (`LedgerToolInfo`).
    pub const LEDGER_TOOL_INFO: &str = "ledger.tool.info";
    /// Koneksi kapabilitas: buku AC (`Vec<AcRowView>`).
    pub const LEDGER_TOOL_READ: &str = "ledger.tool.read";
    /// Koneksi kapabilitas, peran design: `LedgerToolSetDesign`.
    pub const LEDGER_TOOL_SET_DESIGN: &str = "ledger.tool.set_design";
    /// Koneksi kapabilitas, peran verdict: `LedgerToolSetVerdict`.
    pub const LEDGER_TOOL_SET_VERDICT: &str = "ledger.tool.set_verdict";
    /// Koneksi kapabilitas, peran verdict: `LedgerToolSetVisual`.
    pub const LEDGER_TOOL_SET_VISUAL: &str = "ledger.tool.set_visual";
    /// Koneksi kapabilitas, peran verdict: `LedgerToolReportFinding` (R19).
    pub const LEDGER_TOOL_REPORT_FINDING: &str = "ledger.tool.report_finding";
    /// Koneksi kapabilitas, peran verdict: `LedgerToolRecommendLook` (R18).
    pub const LEDGER_TOOL_RECOMMEND_LOOK: &str = "ledger.tool.recommend_look";
    /// Koneksi kapabilitas, peran `lab_setup`: usulan perlengkapan lab
    /// (`LabEquipmentDto` tanpa password) → `{"saved": true}` (KTD-L14).
    pub const LEDGER_TOOL_LAB_PROPOSE: &str = "ledger.tool.lab_propose";
    /// Koneksi kapabilitas, peran verdict selama sesi lab data:
    /// `LedgerToolLabSql` → `LabSqlView` (KTD-L16).
    pub const LEDGER_TOOL_LAB_SQL: &str = "ledger.tool.lab_sql";
    /// Koneksi kapabilitas, peran verdict selama sesi lab data:
    /// `LedgerToolLabSeed` → `{"output": ...}` (KTD-L17).
    pub const LEDGER_TOOL_LAB_SEED: &str = "ledger.tool.lab_seed";
    /// Approve atau Reject rekomendasi tampilan (`LookDecisionParams` →
    /// `Vec<AcRowView>`, R18).
    pub const LEDGER_LOOK_DECISION: &str = "ledger.look_decision";
    /// Temuan reviewer di luar AC (`RunRef` → `Vec<FindingView>`, R19).
    pub const REVIEW_FINDINGS_LIST: &str = "review.findings";
    /// Koneksi kapabilitas, peran ask atau design di laptop: `AskToolQuestion`
    /// → teks balasan alat (plan 2026-11-07-001).
    pub const ASK_TOOL_ASK: &str = "ask.tool.ask";
    /// Koneksi kapabilitas peran permit (`mandord mcp --permit`):
    /// `PermitRequestParams` → `PermitDecision`; boleh menunggu jawaban dev
    /// sampai 10 menit (plan 2026-11-06-001).
    pub const PERMIT_REQUEST: &str = "permit.request";
    /// Screenshot hasil tampilan run VM (`VisualScreenshotParams` →
    /// `VisualScreenshotView`).
    pub const REVIEW_SCREENSHOT: &str = "review.screenshot";
    /// Kesiapan uji browser (`RunRef` → `ReviewReadinessView`).
    pub const REVIEW_READINESS: &str = "review.readiness";
    /// Cek ekstensi Claude in Chrome (`ChromeCheckParams` → `ChromeCheckView`).
    pub const SYSTEM_CHROME_CHECK: &str = "system.chrome_check";
    /// App yang sedang menyala (`AppRunView` atau `null`).
    pub const APP_OWNER: &str = "app.owner";
    /// Memulai review AC (`ReviewStart` → `ReviewView`).
    pub const REVIEW_START: &str = "review.start";
    /// Membatalkan review yang berjalan (`RunRef` → `{}`).
    pub const REVIEW_CANCEL: &str = "review.cancel";
    /// Riwayat review run (`RunRef` → `Vec<ReviewView>`).
    pub const REVIEW_HISTORY: &str = "review.history";
    /// Mengirim baris Fail ke agent (`ReviewFindings` → `{}`).
    pub const REVIEW_SEND_FINDINGS: &str = "review.send_findings";
    /// Setelan reviewer (`ReviewerSettingsView`).
    pub const SETTINGS_REVIEWER_GET: &str = "settings.reviewer.get";
    /// Menyimpan setelan reviewer (`ReviewerSettingsView` → `ReviewerSettingsView`).
    pub const SETTINGS_REVIEWER_SET: &str = "settings.reviewer.set";
    /// State app run (`RunRef` → `AppRunView`).
    pub const APP_STATUS: &str = "app.status";
    /// Berkas berubah per repo untuk konfirmasi (`RunRef` → `Vec<AppChangesView>`).
    pub const APP_CHANGES: &str = "app.changes";
    /// Menyalakan app (`AppRunParams` → `AppRunView`).
    pub const APP_RUN: &str = "app.run";
    /// Menghentikan app (`RunRef` → `AppRunView`).
    pub const APP_STOP: &str = "app.stop";
    /// Konfirmasi database dan login (`AppConfirm` → `AppRunView`).
    pub const APP_CONFIRM: &str = "app.confirm";
    /// Log app per repo (`RunRef` → `Vec<AppLogView>`).
    pub const APP_LOGS: &str = "app.logs";
    /// Nama kunci `.env` folder asli (`EnvKeysParams` → `EnvKeysView`).
    pub const APP_ENV_KEYS: &str = "app.env_keys";
    /// Perlengkapan review lab repo (`LabRepoRef` → `LabView`); hanya koneksi
    /// token penuh (plan 2026-11-10-001, U3).
    pub const LAB_GET: &str = "lab.get";
    /// Menyimpan perlengkapan dan password akun uji (`LabSetParams` →
    /// `LabView`); persetujuan dikosongkan.
    pub const LAB_SET: &str = "lab.set";
    /// Menyetujui perlengkapan yang dilihat developer (`LabApproveParams` →
    /// `LabView`).
    pub const LAB_APPROVE: &str = "lab.approve";
    /// Meminta agent mengusulkan perlengkapan dari isi repo (`LabRepoRef` →
    /// `LabView`); hasilnya muncul di `LabView.proposal` (KTD-L14).
    pub const LAB_PROPOSE: &str = "lab.propose";
    /// Retry restore lab data yang gagal (`LabRepoRef` → `LabView`, KTD-L22).
    pub const LAB_RETRY_RESTORE: &str = "lab.retry_restore";
    /// Developer menyatakan data pulih (`LabDataRestored` → `LabView`, R6).
    pub const LAB_DATA_RESTORED: &str = "lab.data_restored";
    /// Run lab: review dengan lab data diizinkan (`RunRef` → `ReviewView`,
    /// KTD-L23).
    pub const REVIEW_LAB_START: &str = "review.lab_start";
    /// Sumber desain terdaftar (`Vec<DesignSourceView>`).
    pub const DESIGN_SOURCES_LIST: &str = "design_sources.list";
    /// `DesignSourceAdd` → `DesignSourceView`.
    pub const DESIGN_SOURCES_ADD: &str = "design_sources.add";
    /// Revisi prototype (`DesignSourceRef` → `PrototypeRevisionView`).
    pub const DESIGN_SOURCES_REVISION: &str = "design_sources.revision";
    /// Konfirmasi revisi prototype (`PrototypeConfirm` → `PrototypeRevisionView`).
    pub const DESIGN_SOURCES_CONFIRM: &str = "design_sources.confirm";
    /// `DesignSourceUpdate` → `DesignSourceView`.
    pub const DESIGN_SOURCES_UPDATE: &str = "design_sources.update";
    /// `DesignSourceRef` → `{}`.
    pub const DESIGN_SOURCES_REMOVE: &str = "design_sources.remove";
    /// `DesignSourceTest` → `DesignSourceCheckView`.
    pub const DESIGN_SOURCES_TEST: &str = "design_sources.test";
    pub const LARK_LOGIN_BEGIN: &str = "lark.login.begin";
    pub const LARK_LOGIN_COMPLETE: &str = "lark.login.complete";
    pub const LARK_LOGOUT: &str = "lark.logout";
    pub const BOARD_LIST: &str = "board.list";
    pub const BOARD_REFRESH: &str = "board.refresh";
    pub const CARD_RETRY: &str = "card.retry";
    pub const CARD_COMMENTS: &str = "card.comments";
    pub const CARD_DETAILS: &str = "card.details";
    pub const PEOPLE_NAMES: &str = "people.names";
    /// Orang yang muncul di kartu sendiri dan papan Team, untuk pemilih reviewer.
    pub const PEOPLE_KNOWN: &str = "people.known";
    pub const RUNS_LIST: &str = "runs.list";
    /// Kartu persetujuan run (`RunRef` → `Vec<ApprovalView>`), urut waktu
    /// dibuat; input tersamarkan kecuali kartu Pending (plan 2026-11-06-001).
    pub const APPROVALS_LIST: &str = "approvals.list";
    /// Jawaban dev (`ApprovalAnswerParams`); hanya koneksi token penuh.
    pub const APPROVALS_ANSWER: &str = "approvals.answer";
    /// Dev membuka tab Chat run (`RunRef`): mode tolak langsung berakhir.
    pub const APPROVALS_SEEN: &str = "approvals.seen";
    /// Kartu pertanyaan run (`RunRef` → `Vec<QuestionView>`), urut waktu
    /// (plan 2026-11-07-001).
    pub const QUESTIONS_LIST: &str = "questions.list";
    /// Klik pilihan kartu pertanyaan (`QuestionAnswerParams`); hanya koneksi
    /// token penuh.
    pub const QUESTIONS_ANSWER: &str = "questions.answer";
    /// Kirim pesan antrean yang ditahan sebagai satu pesan (`RunRef`).
    pub const RUNS_QUEUED_SEND: &str = "runs.queued.send";
    /// Buang pesan antrean yang ditahan (`RunRef`).
    pub const RUNS_QUEUED_DISCARD: &str = "runs.queued.discard";
    pub const RUNS_MESSAGES: &str = "runs.messages";
    pub const RUNS_START: &str = "runs.start";
    pub const RUNS_SEND: &str = "runs.send";
    pub const RUNS_SEND_NOW: &str = "runs.send_now";
    pub const RUNS_STOP: &str = "runs.stop";
    /// `PublishParams` → `PublishResult` (gerbang AC, KTD-35).
    pub const RUNS_PUBLISH: &str = "runs.publish";
    pub const RUNS_RESUME: &str = "runs.resume";
    pub const RUNS_START_FIX: &str = "runs.start_fix";
    pub const RUNS_TAKEOVER: &str = "runs.takeover";
    pub const RUNS_CLOSE: &str = "runs.close";
    /// Run kartu selesai yang bisa ditutup dan yang dilewati (plan
    /// `2026-11-03-001`).
    pub const RUNS_FINISHED: &str = "runs.finished";
    pub const RUNS_CLOSE_FINISHED: &str = "runs.close_finished";
    /// Semua run terbuka dengan ukuran worktree (R9).
    pub const RUNS_STORAGE: &str = "runs.storage";
    pub const SETTINGS_CLEANUP_GET: &str = "settings.cleanup.get";
    pub const SETTINGS_CLEANUP_SET: &str = "settings.cleanup.set";
    pub const RUNS_CHANGES: &str = "runs.changes";
    pub const RUNS_FILE_DIFF: &str = "runs.file_diff";
    pub const RUNS_DISCARD_FILE: &str = "runs.discard_file";
    /// Menyetujui plan agent dan memulai implementasi.
    /// `ApprovePlanParams` → `{}`.
    pub const RUNS_APPROVE_PLAN: &str = "runs.approve_plan";
    /// Mengakhiri brainstorm dan meminta agent menulis plan.
    pub const RUNS_CONTINUE_TO_PLAN: &str = "runs.continue_to_plan";
    /// Kandidat run grup dari epic kartu. `GroupCandidatesParams` →
    /// `GroupCandidateView[]`.
    pub const GROUPS_CANDIDATES: &str = "groups.candidates";
    /// `StartGroupParams` → `RunView`.
    pub const RUNS_START_GROUP: &str = "runs.start_group";
    /// Commit kartu aktif lalu kartu berikutnya. `RunRef` → `{}`.
    pub const RUNS_NEXT_CARD: &str = "runs.next_card";
    /// Lanjut setelah kartu dikeluarkan. `RunRef` → `{}`.
    pub const RUNS_CONTINUE_GROUP: &str = "runs.continue_group";
    /// Mengeluarkan kartu yang belum dimulai. `GroupCardRef` → `{}`.
    pub const RUNS_REMOVE_CARD: &str = "runs.remove_card";
    /// Menghentikan grup tanpa PR. `RunRef` → `{}`.
    pub const RUNS_STOP_GROUP: &str = "runs.stop_group";
    /// Revert atau tetap untuk kartu yang berubah di Lark.
    /// `GateCardDecisionParams` → `GateCardDecisionResult`.
    pub const RUNS_GATE_CARD_DECISION: &str = "runs.gate_card_decision";
    /// Keputusan AC dev untuk kartu VM yang tertahan `ac_gate` (R12a plan VM):
    /// push dan PR tanpa menjalankan agent. `PublishParams` → `PublishResult`.
    pub const OVERNIGHT_GATE_DECIDE: &str = "overnight.gate_decide";
    /// PR draft VM siap di-review dan kartu ke Waiting Review (R23 plan VM).
    /// `RunRef` → `{}`.
    pub const OVERNIGHT_SEND_TO_REVIEW: &str = "overnight.send_to_review";
    /// Serah-terima kartu VM ke daemon laptop (R22, KTD-63 plan VM); dipanggil
    /// ke daemon VM. `HandoffParams` → `HandoffResult`.
    pub const RUNS_HANDOFF: &str = "runs.handoff";
    /// Laptop: commit dan push plan, kembalikan paket plan (plan 2026-10-30-001).
    /// `PrepareForVmParams` → `VmHandoverView`.
    pub const RUNS_PREPARE_FOR_VM: &str = "runs.prepare_for_vm";
    /// Laptop: klaim ke daemon VM, tutup run (plan 2026-10-30-001).
    /// `SendToVmParams` → `{}`; aman diulang.
    pub const RUNS_SEND_TO_VM: &str = "runs.send_to_vm";
    /// Antrean VM dan kartu yang dikeluarkan siklus ini (R6 plan VM).
    /// `null` → `OvernightQueueView`.
    pub const OVERNIGHT_QUEUE_LIST: &str = "overnight.queue.list";
    /// `QueueAddParams` → `{}`; kartu To Do atau tertahan (R6, R20).
    pub const OVERNIGHT_QUEUE_ADD: &str = "overnight.queue.add";
    /// `RecordRef` → `{}`.
    pub const OVERNIGHT_QUEUE_REMOVE: &str = "overnight.queue.remove";
    /// `QueueReorderParams` → `{}`.
    pub const OVERNIGHT_QUEUE_REORDER: &str = "overnight.queue.reorder";
    /// "Start now" (R7). `RecordRef` → `{}`.
    pub const OVERNIGHT_QUEUE_START_NOW: &str = "overnight.queue.start_now";
    /// `null` → `OvernightSettingsView | null`.
    pub const OVERNIGHT_SETTINGS_GET: &str = "overnight.settings.get";
    /// `OvernightSettingsView` → `{}` (R7, R10).
    pub const OVERNIGHT_SETTINGS_SET: &str = "overnight.settings.set";
    /// Laporan siklus (R18). `null` → `OvernightReportView`.
    pub const OVERNIGHT_REPORT: &str = "overnight.report";
    /// Skill Claude yang bisa dipakai untuk planning (nama dan deskripsi).
    pub const AGENT_SKILLS: &str = "agent.skills";
    pub const REPOS_LIST: &str = "repos.list";
    pub const REPOS_ADD: &str = "repos.add";
    pub const REPOS_UPDATE: &str = "repos.update";
    pub const REPOS_REMOVE: &str = "repos.remove";
    pub const REPOS_DISCOVER: &str = "repos.discover";
    pub const FS_LIST_DIRS: &str = "fs.list_dirs";
    pub const WORKSPACES_LIST: &str = "workspaces.list";
    pub const WORKSPACES_ADD: &str = "workspaces.add";
    pub const WORKSPACES_UPDATE: &str = "workspaces.update";
    pub const WORKSPACES_REMOVE: &str = "workspaces.remove";
    /// Nama MCP server di `.mcp.json` folder induk (tanpa isi konfigurasinya).
    pub const WORKSPACES_MCP_SERVERS: &str = "workspaces.mcp_servers";
    pub const NOTIFICATIONS_TAKE: &str = "notifications.take";
    pub const NOTIFICATIONS_LIST: &str = "notifications.list";
    pub const NOTIFICATIONS_READ: &str = "notifications.read";
}

/// Nama event (notifikasi JSON-RPC) dari daemon.
pub mod event {
    /// Papan berubah; client memuat ulang `board.list`.
    pub const BOARD_UPDATED: &str = "board.updated";
    /// State run berubah (`RunEventView`).
    pub const RUN_UPDATED: &str = "run.updated";
    /// Buku AC run berubah (`LedgerRowsEvent`, baris terbaru).
    pub const LEDGER_UPDATED: &str = "ledger.updated";
    /// Kartu persetujuan run berubah (`RunEventView`); client memuat ulang
    /// `approvals.list`.
    pub const APPROVALS_UPDATED: &str = "approvals.updated";
    /// Kartu pertanyaan run berubah (`RunEventView`); client memuat ulang
    /// `questions.list`.
    pub const QUESTIONS_UPDATED: &str = "questions.updated";
    /// Review run berubah (`RunEventView`); client memuat ulang `review.history`.
    pub const REVIEW_PROGRESS: &str = "review.progress";
    /// State run app berubah (`RunEventView`); client memuat ulang `app.status`.
    pub const APP_UPDATED: &str = "app.updated";
    /// Pesan chat baru di run (`RunEventView`).
    pub const RUN_MESSAGE: &str = "run.message";
    /// Langkah Start yang sedang berjalan (`StartProgressView`).
    pub const START_PROGRESS: &str = "start.progress";
    /// Hasil poll Lark terakhir (`SyncStatusView`).
    pub const SYNC_STATUS: &str = "sync.status";
    /// Notifikasi untuk dev (`NoticeView`).
    pub const NOTICE: &str = "notice";
    /// Client tertinggal event; muat ulang semua.
    pub const RESYNC: &str = "resync";
}

/// Daftar (nama file, isi TypeScript) untuk semua tipe yang diekspor.
///
/// # Errors
/// Mengembalikan pesan bila `ts-rs` gagal membangkitkan deklarasi.
pub fn ts_bindings() -> Result<Vec<(&'static str, String)>, String> {
    use ts_rs::{Config, TS};
    let cfg = Config::new();
    let one = |name: &'static str, s: Result<String, ts_rs::ExportError>| {
        s.map(|body| (name, body)).map_err(|e| e.to_string())
    };
    Ok([
        one("Hello.ts", Hello::export_to_string(&cfg))?,
        one("HelloOk.ts", HelloOk::export_to_string(&cfg))?,
        one("RpcError.ts", RpcError::export_to_string(&cfg))?,
        one("ErrorCode.ts", ErrorCode::export_to_string(&cfg))?,
        one("Notification.ts", Notification::export_to_string(&cfg))?,
        one("Request.ts", Request::export_to_string(&cfg))?,
        one("Response.ts", Response::export_to_string(&cfg))?,
        one("LarkStatus.ts", LarkStatus::export_to_string(&cfg))?,
        one("StatusResult.ts", StatusResult::export_to_string(&cfg))?,
        one("DrainResult.ts", DrainResult::export_to_string(&cfg))?,
        one("LoginCheck.ts", LoginCheck::export_to_string(&cfg))?,
        one("DrainResult.ts", DrainResult::export_to_string(&cfg))?,
        one(
            "LarkSettingsView.ts",
            LarkSettingsView::export_to_string(&cfg),
        )?,
        one(
            "LarkSettingsInput.ts",
            LarkSettingsInput::export_to_string(&cfg),
        )?,
        one("QuietPeriodSet.ts", QuietPeriodSet::export_to_string(&cfg))?,
        one("LoginBegin.ts", LoginBegin::export_to_string(&cfg))?,
        one("LoginComplete.ts", LoginComplete::export_to_string(&cfg))?,
        one("FlagView.ts", FlagView::export_to_string(&cfg))?,
        one("RunView.ts", RunView::export_to_string(&cfg))?,
        one("CardView.ts", CardView::export_to_string(&cfg))?,
        one("BoardView.ts", BoardView::export_to_string(&cfg))?,
        one("SprintView.ts", SprintView::export_to_string(&cfg))?,
        one("MessageView.ts", MessageView::export_to_string(&cfg))?,
        one("MessagesQuery.ts", MessagesQuery::export_to_string(&cfg))?,
        one("StartParams.ts", StartParams::export_to_string(&cfg))?,
        one("RunRef.ts", RunRef::export_to_string(&cfg))?,
        one("RecordRef.ts", RecordRef::export_to_string(&cfg))?,
        one("SendParams.ts", SendParams::export_to_string(&cfg))?,
        one("SendResult.ts", SendResult::export_to_string(&cfg))?,
        one("TakeoverParams.ts", TakeoverParams::export_to_string(&cfg))?,
        one("StartFixParams.ts", StartFixParams::export_to_string(&cfg))?,
        one("PublishResult.ts", PublishResult::export_to_string(&cfg))?,
        one("FileChangeView.ts", FileChangeView::export_to_string(&cfg))?,
        one("FileDiffParams.ts", FileDiffParams::export_to_string(&cfg))?,
        one("CommentView.ts", CommentView::export_to_string(&cfg))?,
        one("AcView.ts", AcView::export_to_string(&cfg))?,
        one(
            "TaskDetailsView.ts",
            TaskDetailsView::export_to_string(&cfg),
        )?,
        one("RepoView.ts", RepoView::export_to_string(&cfg))?,
        one("RepoAdd.ts", RepoAdd::export_to_string(&cfg))?,
        one("RepoUpdate.ts", RepoUpdate::export_to_string(&cfg))?,
        one("RepoRef.ts", RepoRef::export_to_string(&cfg))?,
        one("NoticeView.ts", NoticeView::export_to_string(&cfg))?,
        one(
            "NotificationsRead.ts",
            NotificationsRead::export_to_string(&cfg),
        )?,
        one("RunEventView.ts", RunEventView::export_to_string(&cfg))?,
        one("PrereqView.ts", PrereqView::export_to_string(&cfg))?,
        one(
            "RepoCandidateView.ts",
            RepoCandidateView::export_to_string(&cfg),
        )?,
        one("DirQuery.ts", DirQuery::export_to_string(&cfg))?,
        one("DirEntryView.ts", DirEntryView::export_to_string(&cfg))?,
        one("DirListingView.ts", DirListingView::export_to_string(&cfg))?,
        one("SyncStatusView.ts", SyncStatusView::export_to_string(&cfg))?,
        one("PeopleQuery.ts", PeopleQuery::export_to_string(&cfg))?,
        one("PersonView.ts", PersonView::export_to_string(&cfg))?,
        one(
            "StartProgressView.ts",
            StartProgressView::export_to_string(&cfg),
        )?,
        one("StartWorkspace.ts", StartWorkspace::export_to_string(&cfg))?,
        one("RunRepoView.ts", RunRepoView::export_to_string(&cfg))?,
        one("WorkspaceView.ts", WorkspaceView::export_to_string(&cfg))?,
        one("WorkspaceAdd.ts", WorkspaceAdd::export_to_string(&cfg))?,
        one(
            "WorkspaceUpdate.ts",
            WorkspaceUpdate::export_to_string(&cfg),
        )?,
        one(
            "McpServersQuery.ts",
            McpServersQuery::export_to_string(&cfg),
        )?,
        one("StartPlanning.ts", StartPlanning::export_to_string(&cfg))?,
        one("SkillView.ts", SkillView::export_to_string(&cfg))?,
    ]
    .into_iter()
    .chain(review_bindings()?)
    .chain(group_bindings()?)
    .chain(prototype_bindings()?)
    .chain(approval_bindings()?)
    .chain(lab_bindings()?)
    .collect())
}

/// Tipe review lab (plan 2026-11-10-001).
fn lab_bindings() -> Result<Vec<(&'static str, String)>, String> {
    use ts_rs::{Config, TS};
    let cfg = Config::new();
    let one = |name: &'static str, s: Result<String, ts_rs::ExportError>| {
        s.map(|body| (name, body)).map_err(|e| e.to_string())
    };
    Ok(vec![
        one("LabRepoRef.ts", LabRepoRef::export_to_string(&cfg))?,
        one(
            "BlockedRequestView.ts",
            BlockedRequestView::export_to_string(&cfg),
        )?,
        one("EvidenceView.ts", EvidenceView::export_to_string(&cfg))?,
        one(
            "LookRecommendationView.ts",
            LookRecommendationView::export_to_string(&cfg),
        )?,
        one(
            "LookDecisionParams.ts",
            LookDecisionParams::export_to_string(&cfg),
        )?,
        one("FindingView.ts", FindingView::export_to_string(&cfg))?,
        one("LabView.ts", LabView::export_to_string(&cfg))?,
        one(
            "LabEquipmentDto.ts",
            LabEquipmentDto::export_to_string(&cfg),
        )?,
        one("LabAccountDto.ts", LabAccountDto::export_to_string(&cfg))?,
        one("LabDataDto.ts", LabDataDto::export_to_string(&cfg))?,
        one("LabStoreDto.ts", LabStoreDto::export_to_string(&cfg))?,
        one("LabSeedDto.ts", LabSeedDto::export_to_string(&cfg))?,
        one(
            "LabDataAlertView.ts",
            LabDataAlertView::export_to_string(&cfg),
        )?,
        one(
            "LabDataRestored.ts",
            LabDataRestored::export_to_string(&cfg),
        )?,
        one(
            "LabDataSessionView.ts",
            LabDataSessionView::export_to_string(&cfg),
        )?,
        one(
            "LabDbSecretInput.ts",
            LabDbSecretInput::export_to_string(&cfg),
        )?,
        one("LabLoginDto.ts", LabLoginDto::export_to_string(&cfg))?,
        one(
            "LabPasswordInput.ts",
            LabPasswordInput::export_to_string(&cfg),
        )?,
        one("LabSetParams.ts", LabSetParams::export_to_string(&cfg))?,
        one(
            "LabApproveParams.ts",
            LabApproveParams::export_to_string(&cfg),
        )?,
    ])
}

/// Tipe kartu persetujuan (plan 2026-11-06-001).
fn approval_bindings() -> Result<Vec<(&'static str, String)>, String> {
    use ts_rs::{Config, TS};
    let cfg = Config::new();
    let one = |name: &'static str, s: Result<String, ts_rs::ExportError>| {
        s.map(|body| (name, body)).map_err(|e| e.to_string())
    };
    Ok(vec![
        one("ApprovalView.ts", ApprovalView::export_to_string(&cfg))?,
        one(
            "ApprovalAnswerParams.ts",
            ApprovalAnswerParams::export_to_string(&cfg),
        )?,
        one("QuestionView.ts", QuestionView::export_to_string(&cfg))?,
        one(
            "QuestionOptionView.ts",
            QuestionOptionView::export_to_string(&cfg),
        )?,
        one(
            "QuestionAnswerParams.ts",
            QuestionAnswerParams::export_to_string(&cfg),
        )?,
    ])
}

/// Tipe Pick from .env dan prototype (plan 2026-11-03-002).
fn prototype_bindings() -> Result<Vec<(&'static str, String)>, String> {
    use ts_rs::{Config, TS};
    let cfg = Config::new();
    let one = |name: &'static str, s: Result<String, ts_rs::ExportError>| {
        s.map(|body| (name, body)).map_err(|e| e.to_string())
    };
    Ok(vec![
        one("EnvKeysParams.ts", EnvKeysParams::export_to_string(&cfg))?,
        one(
            "VisualDetailView.ts",
            VisualDetailView::export_to_string(&cfg),
        )?,
        one(
            "VisualScreenshotParams.ts",
            VisualScreenshotParams::export_to_string(&cfg),
        )?,
        one(
            "VisualScreenshotView.ts",
            VisualScreenshotView::export_to_string(&cfg),
        )?,
        one(
            "PrototypeRevisionView.ts",
            PrototypeRevisionView::export_to_string(&cfg),
        )?,
        one(
            "PrototypeConfirm.ts",
            PrototypeConfirm::export_to_string(&cfg),
        )?,
        one("EnvKeysView.ts", EnvKeysView::export_to_string(&cfg))?,
    ])
}

/// Tipe run grup (plan run grup U8).
fn group_bindings() -> Result<Vec<(&'static str, String)>, String> {
    use ts_rs::{Config, TS};
    let cfg = Config::new();
    let one = |name: &'static str, s: Result<String, ts_rs::ExportError>| {
        s.map(|body| (name, body)).map_err(|e| e.to_string())
    };
    Ok(vec![
        one("GroupCardView.ts", GroupCardView::export_to_string(&cfg))?,
        one(
            "GroupCandidatesParams.ts",
            GroupCandidatesParams::export_to_string(&cfg),
        )?,
        one(
            "GroupCandidateView.ts",
            GroupCandidateView::export_to_string(&cfg),
        )?,
        one(
            "StartGroupParams.ts",
            StartGroupParams::export_to_string(&cfg),
        )?,
        one("GroupCardRef.ts", GroupCardRef::export_to_string(&cfg))?,
        one("HandoffParams.ts", HandoffParams::export_to_string(&cfg))?,
        one("HandoffResult.ts", HandoffResult::export_to_string(&cfg))?,
        one("DesignMarkDto.ts", DesignMarkDto::export_to_string(&cfg))?,
        one("PlannedWorkDto.ts", PlannedWorkDto::export_to_string(&cfg))?,
        one(
            "CarriedReviewDto.ts",
            CarriedReviewDto::export_to_string(&cfg),
        )?,
        one("CarriedCardDto.ts", CarriedCardDto::export_to_string(&cfg))?,
        one("CardCommitDto.ts", CardCommitDto::export_to_string(&cfg))?,
        one("AcSnapDto.ts", AcSnapDto::export_to_string(&cfg))?,
        one(
            "FinishedRunView.ts",
            FinishedRunView::export_to_string(&cfg),
        )?,
        one("SkippedRunView.ts", SkippedRunView::export_to_string(&cfg))?,
        one(
            "FinishedRunsView.ts",
            FinishedRunsView::export_to_string(&cfg),
        )?,
        one(
            "CloseFinishedParams.ts",
            CloseFinishedParams::export_to_string(&cfg),
        )?,
        one(
            "CloseFinishedStarted.ts",
            CloseFinishedStarted::export_to_string(&cfg),
        )?,
        one("RunStorageView.ts", RunStorageView::export_to_string(&cfg))?,
        one(
            "CleanupSettingsView.ts",
            CleanupSettingsView::export_to_string(&cfg),
        )?,
        one(
            "PrepareForVmParams.ts",
            PrepareForVmParams::export_to_string(&cfg),
        )?,
        one("VmHandoverView.ts", VmHandoverView::export_to_string(&cfg))?,
        one("SendToVmParams.ts", SendToVmParams::export_to_string(&cfg))?,
        one(
            "HeldPlannedView.ts",
            HeldPlannedView::export_to_string(&cfg),
        )?,
        one(
            "OvernightQueueView.ts",
            OvernightQueueView::export_to_string(&cfg),
        )?,
        one("QueueItemView.ts", QueueItemView::export_to_string(&cfg))?,
        one("DequeuedView.ts", DequeuedView::export_to_string(&cfg))?,
        one("QueueAddParams.ts", QueueAddParams::export_to_string(&cfg))?,
        one(
            "QueueReorderParams.ts",
            QueueReorderParams::export_to_string(&cfg),
        )?,
        one(
            "OvernightSettingsView.ts",
            OvernightSettingsView::export_to_string(&cfg),
        )?,
        one(
            "OvernightReportView.ts",
            OvernightReportView::export_to_string(&cfg),
        )?,
        one("ReportCardView.ts", ReportCardView::export_to_string(&cfg))?,
        one(
            "GateCardDecisionParams.ts",
            GateCardDecisionParams::export_to_string(&cfg),
        )?,
        one(
            "GateCardDecisionResult.ts",
            GateCardDecisionResult::export_to_string(&cfg),
        )?,
    ])
}

/// Tipe Rilis 2: buku AC dan sumber desain.
fn review_bindings() -> Result<Vec<(&'static str, String)>, String> {
    use ts_rs::{Config, TS};
    let cfg = Config::new();
    let one = |name: &'static str, s: Result<String, ts_rs::ExportError>| {
        s.map(|body| (name, body)).map_err(|e| e.to_string())
    };
    Ok(vec![
        one("AcRowView.ts", AcRowView::export_to_string(&cfg))?,
        one("AcLedgerView.ts", AcLedgerView::export_to_string(&cfg))?,
        one(
            "ApprovePlanParams.ts",
            ApprovePlanParams::export_to_string(&cfg),
        )?,
        one("PublishParams.ts", PublishParams::export_to_string(&cfg))?,
        one(
            "DesignSourceView.ts",
            DesignSourceView::export_to_string(&cfg),
        )?,
        one(
            "DesignSourceAdd.ts",
            DesignSourceAdd::export_to_string(&cfg),
        )?,
        one("AppRunView.ts", AppRunView::export_to_string(&cfg))?,
        one("ReviewView.ts", ReviewView::export_to_string(&cfg))?,
        one("AcSummaryView.ts", AcSummaryView::export_to_string(&cfg))?,
        one(
            "ReviewReadinessView.ts",
            ReviewReadinessView::export_to_string(&cfg),
        )?,
        one(
            "ChromeCheckView.ts",
            ChromeCheckView::export_to_string(&cfg),
        )?,
        one(
            "ReadinessConfirmations.ts",
            ReadinessConfirmations::export_to_string(&cfg),
        )?,
        one(
            "ChromeCheckParams.ts",
            ChromeCheckParams::export_to_string(&cfg),
        )?,
        one(
            "LedgerRowsEvent.ts",
            LedgerRowsEvent::export_to_string(&cfg),
        )?,
        one(
            "ReviewVerdictView.ts",
            ReviewVerdictView::export_to_string(&cfg),
        )?,
        one("ReviewStart.ts", ReviewStart::export_to_string(&cfg))?,
        one("ReviewFindings.ts", ReviewFindings::export_to_string(&cfg))?,
        one(
            "ReviewerSettingsView.ts",
            ReviewerSettingsView::export_to_string(&cfg),
        )?,
        one("AppMemberView.ts", AppMemberView::export_to_string(&cfg))?,
        one("AppChangesView.ts", AppChangesView::export_to_string(&cfg))?,
        one("AppRunParams.ts", AppRunParams::export_to_string(&cfg))?,
        one("AppConfirm.ts", AppConfirm::export_to_string(&cfg))?,
        one("AppLogView.ts", AppLogView::export_to_string(&cfg))?,
        one("RepoAppView.ts", RepoAppView::export_to_string(&cfg))?,
        one(
            "DesignSourceUpdate.ts",
            DesignSourceUpdate::export_to_string(&cfg),
        )?,
        one(
            "DesignSourceRef.ts",
            DesignSourceRef::export_to_string(&cfg),
        )?,
        one(
            "DesignSourceTest.ts",
            DesignSourceTest::export_to_string(&cfg),
        )?,
        one(
            "DesignSourceCheckView.ts",
            DesignSourceCheckView::export_to_string(&cfg),
        )?,
    ])
}
