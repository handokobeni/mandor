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
    pub const SYSTEM_CHECK: &str = "system.check";
    pub const LARK_SETTINGS_GET: &str = "lark.settings.get";
    pub const LARK_SETTINGS_SET: &str = "lark.settings.set";
    /// Mengubah jeda tenang Waiting Fix tanpa menyentuh pengaturan Lark lain.
    pub const WAITING_FIX_QUIET_SET: &str = "waiting_fix.quiet.set";
    /// Buku AC run (`RunRef` → `AcLedgerView`), di-refresh dari tracker dulu.
    pub const LEDGER_GET: &str = "ledger.get";
    pub const LARK_LOGIN_BEGIN: &str = "lark.login.begin";
    pub const LARK_LOGIN_COMPLETE: &str = "lark.login.complete";
    pub const LARK_LOGOUT: &str = "lark.logout";
    pub const BOARD_LIST: &str = "board.list";
    pub const BOARD_REFRESH: &str = "board.refresh";
    pub const CARD_RETRY: &str = "card.retry";
    pub const CARD_COMMENTS: &str = "card.comments";
    pub const CARD_DETAILS: &str = "card.details";
    pub const PEOPLE_NAMES: &str = "people.names";
    pub const RUNS_LIST: &str = "runs.list";
    pub const RUNS_MESSAGES: &str = "runs.messages";
    pub const RUNS_START: &str = "runs.start";
    pub const RUNS_SEND: &str = "runs.send";
    pub const RUNS_SEND_NOW: &str = "runs.send_now";
    pub const RUNS_STOP: &str = "runs.stop";
    pub const RUNS_PUBLISH: &str = "runs.publish";
    pub const RUNS_RESUME: &str = "runs.resume";
    pub const RUNS_START_FIX: &str = "runs.start_fix";
    pub const RUNS_TAKEOVER: &str = "runs.takeover";
    pub const RUNS_CLOSE: &str = "runs.close";
    pub const RUNS_CHANGES: &str = "runs.changes";
    pub const RUNS_FILE_DIFF: &str = "runs.file_diff";
    pub const RUNS_DISCARD_FILE: &str = "runs.discard_file";
    /// Menyetujui plan agent dan memulai implementasi.
    pub const RUNS_APPROVE_PLAN: &str = "runs.approve_plan";
    /// Mengakhiri brainstorm dan meminta agent menulis plan.
    pub const RUNS_CONTINUE_TO_PLAN: &str = "runs.continue_to_plan";
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
    /// Buku AC run berubah (`RunEventView`).
    pub const LEDGER_UPDATED: &str = "ledger.updated";
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
    Ok(vec![
        one("Hello.ts", Hello::export_to_string(&cfg))?,
        one("HelloOk.ts", HelloOk::export_to_string(&cfg))?,
        one("RpcError.ts", RpcError::export_to_string(&cfg))?,
        one("ErrorCode.ts", ErrorCode::export_to_string(&cfg))?,
        one("Notification.ts", Notification::export_to_string(&cfg))?,
        one("Request.ts", Request::export_to_string(&cfg))?,
        one("Response.ts", Response::export_to_string(&cfg))?,
        one("LarkStatus.ts", LarkStatus::export_to_string(&cfg))?,
        one("StatusResult.ts", StatusResult::export_to_string(&cfg))?,
        one(
            "LarkSettingsView.ts",
            LarkSettingsView::export_to_string(&cfg),
        )?,
        one(
            "LarkSettingsInput.ts",
            LarkSettingsInput::export_to_string(&cfg),
        )?,
        one("QuietPeriodSet.ts", QuietPeriodSet::export_to_string(&cfg))?,
        one("AcRowView.ts", AcRowView::export_to_string(&cfg))?,
        one("AcLedgerView.ts", AcLedgerView::export_to_string(&cfg))?,
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
    ])
}
