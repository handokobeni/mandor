//! DTO method dan event aplikasi (papan, run, Lark, repo, notifikasi).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Status koneksi Lark.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LarkStatus {
    /// App ID, secret, dan tabel sudah diatur.
    pub configured: bool,
    pub logged_in: bool,
    pub open_id: Option<String>,
    /// Nama akun Lark yang login; `None` pada login lama (login ulang mengisinya).
    pub name: Option<String>,
    /// Refresh token tidak berlaku lagi; login ulang diperlukan.
    pub reauth_required: bool,
}

/// Hasil `status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StatusResult {
    pub version: String,
    #[ts(type = "number")]
    pub uptime_ms: u64,
    #[ts(type = "number")]
    pub repos: u64,
    pub lark: LarkStatus,
}

/// Pengaturan Lark yang ditampilkan (tanpa secret).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LarkSettingsView {
    pub app_id: String,
    pub has_secret: bool,
    pub app_token: String,
    pub table_id: String,
    pub ac_table_id: String,
    pub redirect_port: u16,
    /// Domain Open Platform, mis. `https://open.larksuite.com`.
    pub open_base: String,
    pub accounts_base: String,
    #[ts(type = "number")]
    pub quiet_secs: i64,
}

/// Parameter `lark.settings.set`. `app_secret` kosong mempertahankan secret lama.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LarkSettingsInput {
    pub app_id: String,
    pub app_secret: Option<String>,
    pub app_token: String,
    pub table_id: String,
    pub ac_table_id: String,
    pub redirect_port: u16,
    pub open_base: Option<String>,
    pub accounts_base: Option<String>,
    #[ts(type = "number | null")]
    pub quiet_secs: Option<i64>,
}

/// Parameter `waiting_fix.quiet.set`: jeda tenang dalam detik (30–900).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QuietPeriodSet {
    #[ts(type = "number")]
    pub secs: i64,
}

/// Hasil `lark.login.begin`: client membuka `url` dan menunggu callback di `port`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LoginBegin {
    pub url: String,
    pub state: String,
    pub port: u16,
}

/// Parameter `lark.login.complete`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LoginComplete {
    pub code: String,
    pub state: String,
}

/// Penanda kartu (R6, R6a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FlagView {
    /// `overwritten` | `sync_failed` | `deleted` | `unassigned`.
    pub kind: String,
    pub detail: String,
}

/// Ringkasan run untuk kartu dan panel agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunView {
    pub id: String,
    pub record_id: String,
    pub repo_id: String,
    pub key: Option<String>,
    pub title: String,
    /// State alur kerja, mis. `running`, `awaiting_dev`, `wf_quiet`.
    pub state: String,
    pub branch: Option<String>,
    /// Semua link PR run, satu per baris.
    pub pr_url: Option<String>,
    /// Workspace run multi-repo; `None` untuk run satu repo.
    pub workspace_id: Option<String>,
    /// Repo run beserta link PR-nya, repo utama pertama.
    pub repos: Vec<RunRepoView>,
    /// `brainstorm` | `plan` (menunggu persetujuan plan) | `implement`.
    pub phase: String,
    /// Plan terakhir dari agent (Markdown).
    pub plan: Option<String>,
    pub last_error: Option<String>,
    pub note: Option<String>,
    pub queued: Vec<String>,
    pub automatic: bool,
    /// Akhir jeda tenang (ms epoch) saat state `wf_quiet`.
    #[ts(type = "number | null")]
    pub quiet_until_ms: Option<i64>,
    #[ts(type = "number")]
    pub started_at_ms: i64,
    #[ts(type = "number")]
    pub updated_at_ms: i64,
}

/// Satu repo dalam run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunRepoView {
    pub repo_id: String,
    pub pr_url: Option<String>,
}

/// Satu kartu papan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "DTO wire: setiap bool adalah field JSON terpisah yang dibaca client, bukan state tersamar"
)]
pub struct CardView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    pub status: String,
    /// `todo` | `in_progress` | `review` | `done` | `unknown`.
    pub column: String,
    pub readiness: Option<String>,
    /// Alasan Start dinonaktifkan; `None` bila Start boleh.
    pub start_blocked: Option<String>,
    pub sprint_ids: Vec<String>,
    /// `open_id` assignee (PIC) di Lark.
    pub assignees: Vec<String>,
    /// Kartu rekan di papan Team: hanya baca, tidak bisa di-Start dari sini.
    pub team: bool,
    pub owner: Option<String>,
    pub owner_active: bool,
    pub mine: bool,
    pub can_takeover: bool,
    pub flag: Option<FlagView>,
    pub run: Option<RunView>,
    pub pr_url: Option<String>,
    pub branch: Option<String>,
    pub repo_id: Option<String>,
    #[ts(type = "number")]
    pub last_modified_ms: i64,
}

/// Hasil `board.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BoardView {
    pub me: Option<String>,
    pub cards: Vec<CardView>,
    /// Sprint yang dirujuk kartu (`sprint_ids`), untuk nama dan filter.
    pub sprints: Vec<SprintView>,
    /// Poll Lark terakhir yang berhasil (ms epoch); `None` sebelum poll pertama.
    #[ts(type = "number | null")]
    pub synced_at_ms: Option<i64>,
    /// Error poll terakhir, kosong bila poll terakhir berhasil.
    pub sync_error: Option<String>,
}

/// Satu sprint dari tabel tertaut field Sprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SprintView {
    pub id: String,
    pub name: String,
    #[ts(type = "number | null")]
    pub start_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub end_ms: Option<i64>,
    /// Status sprint di Lark, mis. "In Progress", "Done".
    pub status: Option<String>,
}

/// Satu pesan chat run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MessageView {
    #[ts(type = "number")]
    pub id: i64,
    /// `dev` | `agent` | `system`.
    pub role: String,
    /// Isi pesan; untuk agent berisi `{type, ...}` sesuai jenis event.
    #[ts(type = "Record<string, unknown>")]
    pub body: Value,
    #[ts(type = "number")]
    pub created_at_ms: i64,
}

/// Parameter `runs.messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MessagesQuery {
    pub run_id: String,
    #[ts(type = "number")]
    pub after_id: i64,
    #[ts(type = "number")]
    pub limit: i64,
}

/// Parameter `runs.start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StartParams {
    pub record_id: String,
    /// Repo untuk Start satu repo; diabaikan bila `workspace` diisi.
    pub repo_id: String,
    pub note: Option<String>,
    /// Start beberapa repo dari satu workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace: Option<StartWorkspace>,
    /// Planning sebelum implementasi; tidak diisi = langsung implementasi.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub planning: Option<StartPlanning>,
}

/// Pilihan planning saat Start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StartPlanning {
    /// `skip` | `plan` | `brainstorm` (brainstorm lalu plan).
    pub kind: String,
    /// Skill brainstorm (`plugin:skill`); `None` = prompt bawaan mandor.
    pub brainstorm_skill: Option<String>,
    /// Skill plan (`plugin:skill`); `None` = prompt bawaan mandor.
    pub plan_skill: Option<String>,
}

/// Skill Claude untuk planning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SkillView {
    pub name: String,
    pub description: String,
}

/// Target Start workspace: repo yang dicentang dev.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StartWorkspace {
    pub id: String,
    pub repo_ids: Vec<String>,
}

/// Parameter aksi pada satu run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunRef {
    pub run_id: String,
}

/// Parameter aksi pada satu record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecordRef {
    pub record_id: String,
}

/// Parameter `runs.send` dan `runs.send_now`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SendParams {
    pub run_id: String,
    pub text: String,
}

/// Hasil `runs.send`: `sent` atau `queued`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SendResult {
    pub outcome: String,
}

/// Parameter `runs.takeover`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TakeoverParams {
    pub record_id: String,
    pub repo_id: String,
}

/// Parameter `runs.start_fix` ("Mulai perbaikan").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StartFixParams {
    pub record_id: String,
    pub instruction: String,
    /// Komentar (boleh tidak tepercaya) yang dipilih dev.
    pub reply_ids: Vec<String>,
}

/// Hasil `runs.publish`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PublishResult {
    pub pr_url: String,
}

/// Satu berkas yang berubah di worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileChangeView {
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}

/// Parameter `runs.file_diff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileDiffParams {
    pub run_id: String,
    pub path: String,
}

/// Satu Acceptance Criteria di tab Task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AcView {
    pub id: String,
    /// `AC-xxx`.
    pub label: Option<String>,
    pub text: String,
    /// AC Type "Droped": ditampilkan dicoret, tidak dikirim ke agent.
    pub dropped: bool,
}

/// Hasil `card.details`: isi tab Task (hanya baca; Lark pemiliknya).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TaskDetailsView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub readiness: Option<String>,
    pub assignees: Vec<String>,
    pub reporter: Option<String>,
    pub sprint_ids: Vec<String>,
    pub acs: Vec<AcView>,
}

/// Komentar record untuk kartu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommentView {
    pub reply_id: String,
    pub author: String,
    #[ts(type = "number")]
    pub create_time: i64,
    pub text: String,
    pub trusted: bool,
    /// Sudah diteruskan ke agent.
    pub sent: bool,
}

/// Repo yang terhubung beserta pengaturannya.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RepoView {
    pub id: String,
    pub path: String,
    pub default_branch: Option<String>,
    pub gh_profile: Option<String>,
    pub companions: Vec<String>,
    pub trusted_reviewers: Vec<String>,
    pub extra_allowed: Vec<String>,
}

/// Parameter `repos.add`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RepoAdd {
    pub path: String,
    pub gh_profile: Option<String>,
}

/// Parameter `repos.update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RepoUpdate {
    pub id: String,
    pub gh_profile: Option<String>,
    pub companions: Vec<String>,
    pub trusted_reviewers: Vec<String>,
    pub extra_allowed: Vec<String>,
}

/// Workspace multi-repo: folder induk dan repo terdaftar di dalamnya.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceView {
    pub id: String,
    pub name: String,
    pub root: String,
    pub repo_ids: Vec<String>,
    /// MCP server dari `.mcp.json` folder induk yang boleh dipakai agent.
    pub mcp_servers: Vec<String>,
}

/// Parameter `workspaces.add`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceAdd {
    pub name: String,
    pub root: String,
    pub repo_ids: Vec<String>,
    pub mcp_servers: Vec<String>,
}

/// Parameter `workspaces.update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceUpdate {
    pub id: String,
    pub repo_ids: Vec<String>,
    pub mcp_servers: Vec<String>,
}

/// Parameter `workspaces.mcp_servers`: folder induk (path absolut).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct McpServersQuery {
    pub root: String,
}

/// Parameter `repos.remove` dan `workspaces.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RepoRef {
    pub id: String,
}

/// Notifikasi untuk dev; juga dikirim sebagai event `notice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NoticeView {
    /// ID riwayat; 0 pada payload yang disimpan.
    #[ts(type = "number")]
    #[serde(default)]
    pub id: i64,
    /// Sudah dibaca di notification center.
    #[serde(default)]
    pub read: bool,
    /// `awaiting_answer` | `failed` | `waiting_fix` | `needs_attention` | `pr_created`.
    pub kind: String,
    pub record_id: String,
    pub run_id: Option<String>,
    pub title: String,
    pub body: String,
    #[ts(type = "number")]
    pub created_at_ms: i64,
}

/// Parameter `notifications.read`: ID tertentu, atau semuanya bila `ids` kosong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NotificationsRead {
    #[ts(type = "Array<number> | null")]
    pub ids: Option<Vec<i64>>,
}

/// Satu prasyarat di mesin daemon (onboarding).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PrereqView {
    /// `git` | `gh` | `gh login` | `claude`.
    pub name: String,
    pub ok: bool,
    /// Versi, atau alasan gagal.
    pub detail: String,
}

/// Parameter `people.names`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PeopleQuery {
    pub open_ids: Vec<String>,
}

/// Nama tampilan satu pengguna Lark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PersonView {
    pub open_id: String,
    pub name: String,
}

/// Event `sync.status`: dikirim setiap poll Lark selesai.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SyncStatusView {
    #[ts(type = "number | null")]
    pub synced_at_ms: Option<i64>,
    pub sync_error: Option<String>,
}

/// Saran repo di mesin daemon (`repos.discover`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RepoCandidateView {
    pub path: String,
    pub name: String,
    pub origin: Option<String>,
    /// Sudah ditambahkan ke Mandor.
    pub added: bool,
}

/// Parameter `fs.list_dirs`; `path` kosong = home pengguna di mesin daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DirQuery {
    pub path: Option<String>,
}

/// Satu subfolder di penjelajah folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DirEntryView {
    pub name: String,
    pub path: String,
    pub is_repo: bool,
}

/// Hasil `fs.list_dirs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DirListingView {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<DirEntryView>,
}

/// Event `start.progress`: langkah Start yang baru dimulai.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StartProgressView {
    pub record_id: String,
    /// `claiming` | `creating_worktree` | `starting_agent` | `moving_status`.
    pub step: String,
}

/// Event `run.updated` dan `run.message`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunEventView {
    pub run_id: String,
}
