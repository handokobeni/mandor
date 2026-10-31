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
    /// Ada run yang bekerja (agent, review, persiapan worktree, atau publish);
    /// daemon tidak boleh di-restart. Daemon lama tidak mengirimnya.
    #[serde(default)]
    #[ts(optional)]
    pub busy: Option<bool>,
    /// Pengelola proses daemon: `systemd` atau `detached`.
    #[serde(default)]
    #[ts(optional)]
    pub managed_by: Option<String>,
    /// Daemon sedang bersiap untuk pembaruan dan menolak kerja baru.
    #[serde(default)]
    #[ts(optional)]
    pub draining: Option<bool>,
    /// Keadaan keep-awake: `not_configured`, `off`, `held`, atau `failing`.
    #[serde(default)]
    #[ts(optional)]
    pub keep_awake: Option<String>,
    /// Pesan error penjaga keep-awake terakhir (untuk `failing`).
    #[serde(default)]
    #[ts(optional)]
    pub keep_awake_error: Option<String>,
    /// Daemon VM: login Lark, gh, dan Claude user agent beserta langkah
    /// perbaikannya (KTD-66 plan VM). Daemon laptop tidak mengirimnya.
    #[serde(default)]
    #[ts(optional)]
    pub logins: Option<Vec<LoginCheck>>,
    /// Id daemon di klaim (KTD-55), untuk serah-terima VM → laptop.
    #[serde(default)]
    #[ts(optional)]
    pub daemon_id: Option<String>,
}

/// Satu login yang dibutuhkan daemon VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LoginCheck {
    /// `lark`, `gh`, atau `claude`.
    pub service: String,
    /// `ok`, `missing`, atau `expired`.
    pub state: String,
    /// Langkah perbaikan untuk developer; `None` bila `ok`.
    pub fix: Option<String>,
}

/// Hasil `daemon.drain`: `idle` (kerja baru ditolak sampai restart) atau
/// `busy` (ada run yang bekerja; tidak ada yang berubah).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DrainResult {
    pub state: String,
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
    /// PR masih draft dan menunggu "Send to review".
    pub draft: bool,
    pub queued: Vec<String>,
    pub automatic: bool,
    /// Akhir jeda tenang (ms epoch) saat state `wf_quiet`.
    #[ts(type = "number | null")]
    pub quiet_until_ms: Option<i64>,
    #[ts(type = "number")]
    pub started_at_ms: i64,
    #[ts(type = "number")]
    pub updated_at_ms: i64,
    /// Ringkasan buku AC (Rilis 2); `None` bila task tanpa AC.
    pub ac: Option<AcSummaryView>,
    /// Kartu run grup, urut posisi; kosong untuk run tunggal.
    pub cards: Vec<GroupCardView>,
}

/// Satu kartu run grup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GroupCardView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    /// `pending`, `active`, `done`, `removed_changed`, `removed_by_dev`,
    /// `removed_reverted`, atau `removed_follow_up`.
    pub state: String,
    /// Kartu selesai yang berubah di Lark; menunggu keputusan di gerbang.
    pub changed_in_lark: bool,
    /// Alasan dev tetap menyertakan kartu yang berubah.
    pub kept_reason: Option<String>,
    /// Kartu sedang dalam loop perbaikan Waiting Fix.
    pub in_fix: bool,
}

/// Parameter `groups.candidates`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GroupCandidatesParams {
    /// Kartu awal; kandidat diambil dari epic kartu ini.
    pub record_id: String,
    /// Kartu di antrean VM (termasuk anggota grup), dibaca app dari daemon VM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub queued: Option<Vec<String>>,
}

/// Satu kandidat grup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GroupCandidateView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    /// Alasan kartu tidak bisa dipilih; `None` bila bisa.
    pub blocked: Option<String>,
}

/// Parameter `runs.start_group`: kartu dalam urutan pengerjaan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StartGroupParams {
    pub record_ids: Vec<String>,
    /// Repo untuk grup satu repo; diabaikan bila `workspace` diisi.
    pub repo_id: String,
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace: Option<StartWorkspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub planning: Option<StartPlanning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub design_sources: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reviewer_skill: Option<String>,
}

/// Parameter aksi pada satu kartu run grup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GroupCardRef {
    pub run_id: String,
    pub record_id: String,
}

/// Parameter `runs.gate_card_decision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GateCardDecisionParams {
    pub run_id: String,
    pub record_id: String,
    /// `revert` atau `keep`.
    pub decision: String,
    /// Wajib untuk `keep`; dicatat di PR.
    pub reason: Option<String>,
}

/// Hasil `runs.gate_card_decision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GateCardDecisionResult {
    /// `kept`, `reverted`, atau `revert_conflict`.
    pub outcome: String,
}

/// Ringkasan buku AC untuk kartu dan sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AcSummaryView {
    /// Baris aktif (tanpa Removed).
    pub total: u32,
    /// Pass, Checked manually, Skipped, atau Follows design.
    pub ok: u32,
    pub failed: u32,
    pub unverified: u32,
    pub not_reviewed: u32,
    /// Ada verdict yang usang.
    pub stale: bool,
    /// Baris Missing, Partly, atau Conflicts di kolom Design.
    pub design_gaps: u32,
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
    /// Pemegang klaim dilihat dari daemon ini: `self`, `own_other_daemon`
    /// (mis. daemon VM, badge "On VM"), atau `other_person`; `None` tanpa klaim.
    pub holder: Option<String>,
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
    /// Id sumber desain yang dipilih (R5); kosong = tanpa sumber desain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub design_sources: Option<Vec<String>>,
    /// Skill reviewer (`plugin:skill`); tidak diisi = prompt bawaan (R14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reviewer_skill: Option<String>,
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

/// Parameter `runs.takeover`: satu repo, atau workspace bila diisi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TakeoverParams {
    pub record_id: String,
    pub repo_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace: Option<StartWorkspace>,
    /// Plan kartu Planned yang dikembalikan VM; run kembali menunggu
    /// persetujuan plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub planned: Option<PlannedWorkDto>,
    /// PR draft VM dipakai lagi; run menunggu Send to review.
    #[serde(default)]
    #[ts(optional)]
    pub draft: Option<bool>,
    /// Hasil review AC VM; usang bila kode laptop berbeda.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub review: Option<CarriedReviewDto>,
    /// Kartu grup dari VM; run laptop menjadi run grup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cards: Option<Vec<CarriedCardDto>>,
}

/// Status Design satu AC hasil tahap plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DesignMarkDto {
    pub ac_id: String,
    /// `not_checked`, `in_design`, `partly`, `missing`, `conflicts`, `follow_design`.
    pub design: String,
    pub note: Option<String>,
}

/// Plan yang disetujui di laptop dan dibawa ke VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlannedWorkDto {
    pub plan: String,
    pub branch: String,
    pub design: Vec<DesignMarkDto>,
    /// Sumber desain laptop saat plan disusun (id laptop).
    pub laptop_design: Vec<String>,
    /// Repo run laptop: satu repo, atau anggota workspace. VM menolak target
    /// dengan repo berbeda; kosong dari laptop versi lama.
    #[serde(default)]
    pub repos: Vec<String>,
}

/// Parameter `runs.prepare_for_vm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PrepareForVmParams {
    pub run_id: String,
    /// `ac_id` baris berselisih desain yang mengikuti desain (R8a).
    #[serde(default)]
    pub follow_design: Vec<String>,
}

/// Hasil `runs.prepare_for_vm`: paket plan dan target run laptop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VmHandoverView {
    pub planned: PlannedWorkDto,
    /// Target satu repo; `None` bila workspace.
    pub repo_id: Option<String>,
    pub workspace: Option<StartWorkspace>,
    /// Kartu grup urut pengerjaan; kosong untuk kartu tunggal (R24 plan grup).
    #[serde(default)]
    pub cards: Vec<String>,
}

/// Parameter `runs.send_to_vm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SendToVmParams {
    pub run_id: String,
    /// Id daemon VM penerima klaim (dari `status` daemon VM).
    pub vm_daemon_id: String,
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
    /// `published`, `review_started`, `blocked`, `needs_confirmation`, atau
    /// `cards_changed` (run grup).
    pub outcome: String,
    pub pr_url: Option<String>,
    /// Label AC gagal (Blocked).
    pub failed: Vec<String>,
    /// Label AC Can't verify yang butuh konfirmasi (`needs_confirmation`).
    pub unverified: Vec<String>,
    /// Label AC Fail/Can't verify yang dibiarkan (mode peringatan).
    pub warnings: Vec<String>,
    /// AC tidak bisa disegarkan dari tracker.
    pub sync_warning: Option<String>,
    /// Review yang dimulai (`review_started`).
    pub review_id: Option<String>,
    /// Kunci kartu grup yang berubah di Lark dan menunggu keputusan revert
    /// atau tetap (`cards_changed`).
    pub changed_cards: Vec<String>,
}

/// Parameter `runs.publish` dengan keputusan dev (KTD-35).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PublishParams {
    pub run_id: String,
    /// `ac_id` baris Can't verify yang dicek manual oleh dev.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub confirmed: Option<Vec<String>>,
    /// Alasan Push anyway untuk baris Fail (mode blokir).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub push_anyway_reason: Option<String>,
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
    /// Run app dan cara uji reviewer (KTD-37).
    pub app: RepoAppView,
    /// Override mode gerbang: `block`, `warn`, atau `null` (ikut workspace).
    pub gate_mode: Option<String>,
}

/// Pengaturan run app per repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RepoAppView {
    /// `web` atau `no_app`.
    pub test_mode: String,
    pub setup_command: Option<String>,
    pub start_command: Option<String>,
    pub port: Option<u16>,
    pub app_url: Option<String>,
    /// Kunci `.env` yang diteruskan ke app (nilai tidak pernah dikirim).
    pub env_keys: Vec<String>,
    pub browser_tests: bool,
    /// Restart app otomatis dari kode terbaru sebelum review berbrowser
    /// (opt-in; kode agent berjalan tanpa konfirmasi perubahan).
    #[serde(default)]
    pub auto_restart: bool,
    /// Setup command menyiapkan login uji sendiri; review headless di VM
    /// memakai browser tanpa konfirmasi login (KTD-64 plan VM).
    #[serde(default)]
    pub unattended_login: bool,
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
    /// Pengaturan run app; tidak diisi = tidak berubah.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub app: Option<RepoAppView>,
    /// `block`, `warn`, atau `inherit` (hapus override); tidak diisi = tidak berubah.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gate_mode: Option<String>,
}

/// State app satu repo anggota run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppMemberView {
    pub repo_id: String,
    /// URL app kanonik: alamat login dev dan origin reviewer.
    pub url: Option<String>,
    pub state: String,
}

/// State app milik run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppRunView {
    pub run_id: String,
    /// `off`, `setting_up`, `starting`, `waiting_for_port`, `running`,
    /// `failed`, atau `stopped_by_other_run`.
    pub state: String,
    pub members: Vec<AppMemberView>,
    pub database_confirmed: bool,
    pub login_confirmed: bool,
    pub error: Option<String>,
}

/// Berkas berubah di satu repo untuk konfirmasi sebelum app dijalankan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppChangesView {
    pub repo_id: String,
    /// Dikirim balik di `AppRunParams.snapshots`.
    pub snapshot: String,
    pub files: Vec<String>,
    /// Manifest paket, lockfile, skrip, dan berkas perintah start.
    pub flagged: Vec<String>,
}

/// Parameter `app.run`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppRunParams {
    pub run_id: String,
    /// Snapshot per repo yang ditampilkan ke dev (`AppChangesView.snapshot`).
    pub snapshots: std::collections::BTreeMap<String, String>,
}

/// Parameter `app.confirm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppConfirm {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub database: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub login: Option<bool>,
}

/// Log app satu repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppLogView {
    pub repo_id: String,
    pub text: String,
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
    /// Uji browser oleh reviewer (KTD-37).
    pub browser_tests: bool,
    /// `block` atau `warn`.
    pub gate_mode: String,
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
    /// Tidak diisi = tidak berubah.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub browser_tests: Option<bool>,
    /// `block` atau `warn`; tidak diisi = tidak berubah.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gate_mode: Option<String>,
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

/// Satu baris buku AC (Rilis 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AcRowView {
    pub ac_id: String,
    pub label: Option<String>,
    pub text: String,
    /// `not_checked`, `in_design`, `partly`, `missing`, `conflicts`, `follow_design`.
    pub design: String,
    pub design_note: Option<String>,
    /// `not_reviewed`, `pass`, `fail`, `cant_verify`, `checked_manually`, `skipped`,
    /// `fix_in_progress`.
    pub verdict: String,
    /// Bukti reviewer sebagai teks biasa.
    pub evidence: Option<String>,
    pub stale: bool,
    /// `active`, `new`, `changed`, `removed`.
    pub status: String,
    pub reason: Option<String>,
    pub decided_by: Option<String>,
    /// Ikut gerbang dan review (bukan Removed, bukan Follow design).
    pub in_gate: bool,
    /// Boleh dipilih untuk Send to agent.
    pub selectable: bool,
    /// Kartu asal AC di run grup (`record_id`, kunci ada di `RunView.cards`);
    /// `None` untuk run tunggal.
    pub card: Option<String>,
}

/// Hasil `ledger.get`: buku AC run setelah refresh dari tracker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AcLedgerView {
    pub run_id: String,
    pub rows: Vec<AcRowView>,
    /// Waktu sinkron AC terakhir yang berhasil (ms epoch).
    #[ts(type = "number | null")]
    pub synced_at_ms: Option<i64>,
    /// Alasan refresh dari tracker gagal; `rows` adalah data terakhir.
    pub sync_error: Option<String>,
    /// Nama sumber desain run; kosong = "No design source".
    pub design_sources: Vec<String>,
    /// Mode gerbang run: `block` atau `warn`.
    pub gate_mode: String,
}

/// Event `ledger.updated`: baris buku AC terbaru (progres review per baris).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LedgerRowsEvent {
    pub run_id: String,
    pub rows: Vec<AcRowView>,
}

/// Verdict satu AC dalam riwayat review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReviewVerdictView {
    pub ac_id: String,
    pub label: Option<String>,
    pub verdict: String,
    pub evidence: Option<String>,
}

/// Satu review AC (riwayat dan progres).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReviewView {
    pub id: String,
    pub run_id: String,
    /// `manual`, `push_and_pr`, atau `automatic_fix`.
    pub trigger: String,
    /// `running`, `finished`, `failed`, atau `cancelled`.
    pub status: String,
    pub browser_used: bool,
    /// Alasan tanpa browser, mis. "app running older code".
    pub browser_reason: Option<String>,
    pub error: Option<String>,
    pub cost_usd: Option<f64>,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    /// Kode berubah selama review.
    pub stale: bool,
    pub skill: Option<String>,
    #[ts(type = "number")]
    pub started_at_ms: i64,
    #[ts(type = "number | null")]
    pub finished_at_ms: Option<i64>,
    pub verdicts: Vec<ReviewVerdictView>,
}

/// Parameter `review.start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReviewStart {
    pub run_id: String,
    /// `manual` (bawaan) atau `push_and_pr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trigger: Option<String>,
    /// Skill reviewer (`plugin:skill`); tidak diisi = prompt bawaan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub skill: Option<String>,
}

/// Parameter `review.send_findings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReviewFindings {
    pub run_id: String,
    pub ac_ids: Vec<String>,
}

/// Setelan reviewer (Settings › General, KTD-30).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReviewerSettingsView {
    /// `None` = model agent pelaksana.
    pub model: Option<String>,
    /// Batas biaya per review dalam sen USD.
    pub budget_cents: u32,
    #[ts(type = "number")]
    pub timeout_secs: u64,
}

/// Kesiapan uji browser per item (dialog kesiapan, KTD-39).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReviewReadinessView {
    /// Dialog hanya untuk run dengan repo Web dan uji browser aktif.
    pub needs_dialog: bool,
    pub browser_tests: bool,
    /// `not_running`, `older_code`, atau `current`.
    pub app: String,
    /// Run lain yang app-nya sedang menyala.
    pub other_owner: Option<String>,
    pub confirmed: ReadinessConfirmations,
    pub app_urls: Vec<String>,
    pub chrome_connected: bool,
    /// Alasan Can't verify untuk AC tampilan bila review jalan sekarang.
    pub cant_verify_reason: Option<String>,
}

/// Konfirmasi dev untuk app run (database menyala, sudah login).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReadinessConfirmations {
    pub database: bool,
    pub login: bool,
}

/// Hasil `system.chrome_check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChromeCheckView {
    pub connected: bool,
    pub detail: String,
    #[ts(type = "number")]
    pub checked_at_ms: i64,
}

/// Parameter `system.chrome_check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChromeCheckParams {
    /// Abaikan cache 60 detik.
    #[serde(default)]
    pub force: bool,
}

/// Parameter `runs.approve_plan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApprovePlanParams {
    pub run_id: String,
    /// `ac_id` baris Missing, Partly, atau Conflicts yang mengikuti desain (R8a).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub follow_design: Option<Vec<String>>,
}

/// Sumber desain terdaftar (Settings → Design sources).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DesignSourceView {
    pub id: String,
    pub name: String,
    /// `folder` di 2.0.
    pub kind: String,
    pub path: String,
    /// Bisa dipakai saat ini.
    pub ready: bool,
    /// `Ready`, `Path missing`, `Not a folder`, atau alasan penolakan.
    pub status: String,
}

/// Parameter `design_sources.add`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DesignSourceAdd {
    pub name: String,
    pub path: String,
}

/// Parameter `design_sources.update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DesignSourceUpdate {
    pub id: String,
    pub name: String,
    pub path: String,
}

/// Parameter `design_sources.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DesignSourceRef {
    pub id: String,
}

/// Parameter `design_sources.test`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DesignSourceTest {
    pub path: String,
}

/// Hasil `design_sources.test`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DesignSourceCheckView {
    pub ready: bool,
    pub status: String,
}

/// Lingkup koneksi kapabilitas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerToolInfo {
    pub run_id: String,
    /// `design` atau `verdict`.
    pub role: String,
}

/// Parameter `ledger.tool.set_design`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerToolSetDesign {
    pub ac_id: String,
    /// `in_design`, `partly`, `missing`, `conflicts`, atau `not_checked`.
    pub design: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Parameter `ledger.tool.set_verdict`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerToolSetVerdict {
    pub ac_id: String,
    /// `pass`, `fail`, atau `cant_verify`.
    pub verdict: String,
    #[serde(default)]
    pub evidence: Option<String>,
}

/// Parameter `runs.handoff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HandoffParams {
    pub record_id: String,
    /// Id daemon laptop penerima klaim (dari `status` daemon laptop).
    pub to_daemon_id: String,
}

/// Hasil `runs.handoff`: branch kartu untuk dipulihkan laptop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HandoffResult {
    pub branch: String,
    /// Target kartu di VM; `None` bila workspace.
    #[serde(default)]
    pub repo_id: Option<String>,
    #[serde(default)]
    pub workspace: Option<StartWorkspace>,
    /// Plan kartu Planned yang belum dimulai VM.
    #[serde(default)]
    pub planned: Option<PlannedWorkDto>,
    /// PR VM masih draft; diteruskan ke `runs.takeover`.
    #[serde(default)]
    pub draft: bool,
    /// Hasil review AC VM; diteruskan ke `runs.takeover`.
    #[serde(default)]
    pub review: Option<CarriedReviewDto>,
    /// Kartu grup beserta state, commit, dan plan; diteruskan ke
    /// `runs.takeover` (KTD-G15 plan grup).
    #[serde(default)]
    pub cards: Vec<CarriedCardDto>,
}

/// Satu kartu grup yang dibawa dari VM ke laptop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CarriedCardDto {
    pub record_id: String,
    /// `pending`, `active`, `done`, atau `removed_*`.
    pub state: String,
    pub changed_in_lark: bool,
    pub kept_reason: Option<String>,
    pub commits: Vec<CardCommitDto>,
    pub plan: Option<String>,
    pub fix_requested_by: Option<String>,
    #[ts(type = "number | null")]
    pub quiet_until_ms: Option<i64>,
    pub in_fix: bool,
    pub fix_replies: Vec<String>,
}

/// Commit lokal satu kartu grup di satu repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardCommitDto {
    pub repo_id: String,
    pub sha: String,
}

/// Hasil review AC run VM beserta snapshot kode saat review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CarriedReviewDto {
    pub rows: Vec<AcRowView>,
    /// Tree id per repo saat review terakhir dimulai.
    pub snapshot: std::collections::BTreeMap<String, String>,
}

/// Satu kartu di antrean VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QueueItemView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    /// Target satu repo; `None` bila workspace.
    pub repo_id: Option<String>,
    pub workspace_id: Option<String>,
    /// Catatan atau jawaban dev untuk agent.
    pub note: Option<String>,
    pub start_now: bool,
    #[ts(type = "number")]
    pub added_at_ms: i64,
    /// Kartu tertahan yang diantre ulang (lanjut di branch yang sama).
    pub held: bool,
    /// Kartu Planned: `handover_pending`, `waiting`, `starting`, `held`;
    /// `None` = No plan.
    pub planned_state: Option<String>,
    /// Alasan item Planned ditahan.
    pub planned_reason: Option<String>,
    /// Nama sumber desain VM yang dipilih (id bila sudah dihapus).
    pub design_sources: Vec<String>,
    /// Kartu grup (`record_id`) urut pengerjaan; kosong untuk kartu tunggal.
    pub group: Vec<String>,
    /// Kunci kartu grup untuk tampilan (id bila tidak ada di cache).
    pub group_keys: Vec<String>,
}

/// Kartu yang keluar dari antrean tanpa dikerjakan, beserta alasannya.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DequeuedView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    pub reason: String,
    #[ts(type = "number")]
    pub at_ms: i64,
}

/// Hasil `overnight.queue.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OvernightQueueView {
    pub items: Vec<QueueItemView>,
    /// Dikeluarkan sejak awal siklus berjalan.
    pub dequeued: Vec<DequeuedView>,
}

/// Parameter `overnight.queue.add`: satu repo, atau workspace bila diisi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QueueAddParams {
    pub record_id: String,
    pub repo_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace: Option<StartWorkspace>,
    pub note: Option<String>,
    /// Id sumber desain VM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub design_sources: Option<Vec<String>>,
    /// Plan yang disetujui di laptop (kartu Planned).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub planned: Option<PlannedWorkDto>,
    /// Id daemon laptop yang menyerahkan kartu Planned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub origin_daemon: Option<String>,
    /// Kartu grup urut pengerjaan; `record_id` = kartu pertama (plan grup R20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub group: Option<Vec<String>>,
}

/// Parameter `overnight.queue.reorder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QueueReorderParams {
    pub record_ids: Vec<String>,
}

/// Setelan lembur (R7, R10): jam lokal `HH:MM`, offset `+07:00`, batas token
/// per siklus (`null` = belum diisi, VM tidak memulai apa pun).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OvernightSettingsView {
    pub start: String,
    pub end: String,
    pub offset: String,
    #[ts(type = "number | null")]
    pub token_limit: Option<u64>,
}

/// Satu kartu VM di laporan siklus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReportCardView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    pub run_id: String,
    pub repo_id: String,
    /// `draft`, `sent_to_review`, `held`, `failed`, atau `working`.
    pub outcome: String,
    /// Untuk `held`: `stuck`, `ac_gate`, `needs_confirmation`, `usage_limit`.
    pub hold_reason: Option<String>,
    /// Pertanyaan atau alasan tertahan.
    pub hold_note: Option<String>,
    pub pr_url: Option<String>,
    pub ac_pass: u32,
    pub ac_fail: u32,
    pub ac_cant_verify: u32,
    pub entered_waiting_fix: bool,
    /// Sumber desain yang gagal diperbarui saat kartu dimulai.
    pub design_notes: Vec<String>,
    /// Keadaan kartu di grupnya: `committed`, `active`, `pending`, `removed`;
    /// `None` untuk kartu tunggal.
    pub group_card: Option<String>,
    /// Kartu grup yang sudah di-commit tetapi berubah di Lark (R13): menunggu
    /// Keep atau Revert.
    pub changed_in_lark: bool,
}

/// Kartu Planned yang ditahan di antrean; plan dan klaimnya masih di VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HeldPlannedView {
    pub record_id: String,
    pub key: Option<String>,
    pub title: String,
    pub repo_id: Option<String>,
    pub reason: String,
}

/// Hasil `overnight.report` (R18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OvernightReportView {
    #[ts(type = "number | null")]
    pub cycle_start_ms: Option<i64>,
    /// Token terhitung (input + output) siklus ini.
    #[ts(type = "number")]
    pub used: u64,
    #[ts(type = "number | null")]
    pub limit: Option<u64>,
    #[ts(type = "number")]
    pub cache_read_tokens: u64,
    pub cards: Vec<ReportCardView>,
    pub dequeued: Vec<DequeuedView>,
    /// Kartu yang sampai Waiting Review tanpa pernah masuk Waiting Fix.
    pub reviewed_without_fix: u32,
    /// Kartu Planned yang ditahan (R15 plan 2026-10-30-001).
    pub planned_held: Vec<HeldPlannedView>,
}
