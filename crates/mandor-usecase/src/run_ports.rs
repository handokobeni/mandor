//! Port untuk orkestrasi task run: git, agent, penyimpanan run, notifikasi.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};

use mandor_domain::{AcRow, AgentEvent, Reply, RunState, Task};

use crate::ports::PortError;

/// Repo lokal yang terhubung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub id: String,
    pub path: PathBuf,
    pub default_branch: String,
    pub gh_profile: Option<String>,
    /// Path pendamping read-only untuk agent (KTD-14).
    pub companions: Vec<PathBuf>,
    /// `open_id` reviewer/QA tepercaya tambahan (U11).
    pub trusted_reviewers: Vec<String>,
    /// Allowlist tool tambahan per repo untuk agent.
    pub extra_allowed: Vec<String>,
}

/// Daftar repo yang terhubung.
pub trait Repos: Send + Sync {
    fn repo(&self, id: &str) -> impl Future<Output = Result<Option<RepoInfo>, PortError>> + Send;
    fn workspace(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<WorkspaceInfo>, PortError>> + Send;
}

/// Workspace multi-repo: repo terdaftar di bawah satu folder induk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub id: String,
    pub name: String,
    /// Folder induk: sumber `CLAUDE.md` dan `.mcp.json` untuk agent.
    pub root: PathBuf,
    /// Repo anggota, berurutan.
    pub repo_ids: Vec<String>,
    /// MCP server dari `.mcp.json` folder induk yang diizinkan dev.
    pub mcp_servers: Vec<String>,
}

/// Perubahan di worktree terhadap branch remote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkChanges {
    /// Ada perubahan belum di-commit.
    pub dirty: bool,
    /// Commit lokal yang belum di-push.
    pub unpushed: u32,
    /// Path yang berubah (untuk deteksi path sensitif).
    pub files: Vec<String>,
    /// Diff untuk pembuat pesan commit (sudah dibatasi ukurannya oleh adapter).
    pub diff: String,
}

impl WorkChanges {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.dirty && self.unpushed == 0
    }
}

/// PR untuk branch task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRef {
    pub url: String,
    pub open: bool,
}

/// Buku AC per run (KTD-25).
pub trait LedgerStore: Send + Sync {
    fn ac_rows(&self, run_id: &str) -> impl Future<Output = Result<Vec<AcRow>, PortError>> + Send;
    fn save_ac_rows(
        &self,
        run_id: &str,
        rows: &[AcRow],
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}

/// Operasi git dan gh milik daemon (KTD-11).
pub trait GitPort: Send + Sync {
    /// Fetch lalu membuat worktree dan branch dari `origin/<base>` terbaru.
    /// `group`: folder run workspace; worktree diletakkan di `<group>/<repo>`.
    fn prepare_worktree(
        &self,
        repo: &RepoInfo,
        branch: &str,
        group: Option<&str>,
    ) -> impl Future<Output = Result<PathBuf, PortError>> + Send;
    /// Membuat ulang worktree dari branch task yang sudah di-push.
    fn restore_worktree(
        &self,
        repo: &RepoInfo,
        branch: &str,
        group: Option<&str>,
    ) -> impl Future<Output = Result<PathBuf, PortError>> + Send;
    /// Menghapus worktree dan branch lokal (rollback Start).
    fn discard_worktree(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        branch: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    fn worktree_exists(&self, worktree: &Path) -> impl Future<Output = bool> + Send;
    /// Menghapus folder run workspace setelah worktree-nya dihapus. Hanya folder
    /// di bawah direktori worktree milik daemon; tidak pernah gagal.
    fn remove_run_root(&self, root: &Path) -> impl Future<Output = ()> + Send;
    fn changes(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        branch: &str,
    ) -> impl Future<Output = Result<WorkChanges, PortError>> + Send;
    /// Mengembalikan satu berkas yang berubah ke isinya di base branch.
    fn discard_file(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        path: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    fn commit_all(
        &self,
        worktree: &Path,
        message: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    fn push(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        branch: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Diff branch task terhadap `origin/<base>` (konteks sesi perbaikan baru).
    fn branch_diff(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
    ) -> impl Future<Output = Result<String, PortError>> + Send;
    fn find_pr(
        &self,
        repo: &RepoInfo,
        branch: &str,
    ) -> impl Future<Output = Result<Option<PrRef>, PortError>> + Send;
    fn create_pr(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        branch: &str,
        title: &str,
        body: &str,
    ) -> impl Future<Output = Result<String, PortError>> + Send;
    /// Tree id isi worktree saat ini (KTD-28), tanpa mengubah index.
    fn snapshot(&self, worktree: &Path) -> impl Future<Output = Result<String, PortError>> + Send;
    /// Berkas yang berbeda antara dua tree.
    fn changed_between(
        &self,
        worktree: &Path,
        from: &str,
        to: &str,
    ) -> impl Future<Output = Result<Vec<String>, PortError>> + Send;
    /// Badan PR saat ini; `None` bila PR tidak ditemukan.
    fn pr_body(
        &self,
        repo: &RepoInfo,
        url: &str,
    ) -> impl Future<Output = Result<Option<String>, PortError>> + Send;
    /// Mengganti badan PR (`gh pr edit --body-file -`).
    fn set_pr_body(
        &self,
        repo: &RepoInfo,
        worktree: &Path,
        url: &str,
        body: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}

/// Sesi agent baru dengan ID pilihan daemon, atau lanjutan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRef {
    New(String),
    Resume(String),
}

/// Satu spawn agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRequest {
    /// Repo utama run.
    pub repo: RepoInfo,
    /// Folder kerja agent: worktree repo utama, atau folder run workspace.
    pub worktree: PathBuf,
    pub branch: String,
    pub session: SessionRef,
    pub message: String,
    /// Run workspace multi-repo; `None` untuk run satu repo.
    pub workspace: Option<AgentWorkspace>,
    /// Giliran plan: agent hanya membaca dan menulis plan di pesan akhirnya.
    pub plan_mode: bool,
    /// Tool ledger milik daemon untuk giliran ini (KTD-26).
    pub ledger: Option<crate::review::LedgerAccess>,
}

/// Konteks agent untuk run workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentWorkspace {
    /// Folder induk workspace: sumber `CLAUDE.md` dan `.mcp.json`.
    pub source: PathBuf,
    /// Semua repo run beserta worktree-nya, repo utama pertama.
    pub repos: Vec<(RepoInfo, PathBuf)>,
    /// MCP server dari `.mcp.json` yang diizinkan dev.
    pub mcp_servers: Vec<String>,
}

/// Menghentikan run agent dari luar task yang membaca event-nya.
pub trait Stopper: Send + Sync + Clone + 'static {
    fn stop(&self) -> impl Future<Output = ()> + Send;
}

/// Run agent yang sedang berjalan.
pub trait AgentRunHandle: Send + 'static {
    type Stopper: Stopper;
    fn next_event(&mut self) -> impl Future<Output = Option<AgentEvent>> + Send;
    fn pid(&self) -> Option<u32>;
    fn stopper(&self) -> Self::Stopper;
}

/// Backend agent.
pub trait AgentPort: Send + Sync {
    type Run: AgentRunHandle;
    fn spawn(
        &self,
        req: &AgentRequest,
    ) -> impl Future<Output = Result<Self::Run, PortError>> + Send;
    /// Pesan commit dari diff, dijalankan tanpa tool dan tanpa token (KTD-11).
    fn commit_message(
        &self,
        worktree: &Path,
        diff: &str,
    ) -> impl Future<Output = Result<String, PortError>> + Send;
    /// Deskripsi PR (Markdown) dari diff branch dan konteks task, dijalankan tanpa
    /// tool dan tanpa token seperti pesan commit (KTD-11).
    fn pr_description(
        &self,
        worktree: &Path,
        ctx: &PrContext<'_>,
    ) -> impl Future<Output = Result<String, PortError>> + Send;
    /// Menghentikan process group yatim dari daemon sebelumnya (KTD-13).
    fn reap(&self, pid: u32) -> impl Future<Output = ()> + Send;
}

/// Bahan deskripsi PR. Semua teks berasal dari Lark atau diff dan diperlakukan
/// sebagai data tidak tepercaya oleh adapter agent.
#[derive(Debug, Clone, Copy)]
pub struct PrContext<'a> {
    /// Judul PR, mis. `PGS-7: Tab Data`.
    pub title: &'a str,
    pub description: &'a str,
    /// AC aktif sebagai `label: teks`.
    pub acs: &'a [String],
    /// Plan yang disetujui dev, bila run melewati fase plan.
    pub plan: Option<&'a str>,
    /// Diff branch terhadap base, sudah dipotong.
    pub diff: &'a str,
}

/// Run untuk satu record di laptop pemilik.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub id: String,
    pub repo_id: String,
    pub record_id: String,
    pub state: RunState,
    /// Naik setiap spawn agent; event dari generasi lama diabaikan.
    pub generation: i64,
    pub branch: Option<String>,
    pub worktree: Option<PathBuf>,
    pub session_id: Option<String>,
    /// `true` setelah sesi pernah berjalan (spawn berikutnya memakai resume).
    pub session_started: bool,
    pub pid: Option<i64>,
    pub pr_url: Option<String>,
    /// Pesan dev yang menunggu giliran selesai.
    pub queue: Vec<String>,
    pub last_error: Option<String>,
    /// Sesi dimulai otomatis (Waiting Fix) tanpa dipicu dev.
    pub automatic: bool,
    /// Penanda kartu milik alur kerja, mis. "perbaikan tanpa perubahan".
    pub note: Option<String>,
    /// Run workspace multi-repo: folder run tempat agent bekerja. `None` = satu repo.
    pub workspace: Option<RunWorkspace>,
    /// Repo tambahan run workspace (repo utama ada di `repo_id`/`worktree`/`pr_url`).
    pub extra: Vec<RunMember>,
    /// Fase kerja: plan (menunggu persetujuan dev) atau implementasi.
    pub phase: RunPhase,
    /// Plan terakhir dari agent; tetap disimpan setelah disetujui.
    pub plan: Option<String>,
    /// Skill plan yang dipakai setelah brainstorm; `None` = prompt bawaan.
    pub plan_skill: Option<String>,
    /// Akhir jeda tenang (ms epoch); hanya bermakna saat state `WfQuiet`.
    pub quiet_until_ms: Option<i64>,
    /// `open_id` yang memindahkan task ke Waiting Fix pada siklus ini (pengubah
    /// terakhir record task saat loop perbaikan dimulai). Dasar kepercayaan
    /// Feedback QA (KTD-41).
    pub fix_requested_by: Option<String>,
    /// Hash badan PR yang terakhir ditulis Mandor, per URL PR. Badan yang hash-nya
    /// berbeda sudah disunting manusia dan tidak ditimpa.
    pub pr_body_hashes: BTreeMap<String, String>,
    /// Snapshot kode (repo → tree id) saat review terakhir dimulai; kosong bila
    /// belum pernah direview (KTD-28).
    pub review_snapshot: BTreeMap<String, String>,
    /// Diisi store saat membaca; diabaikan saat menyimpan.
    pub created_at_ms: i64,
    /// Diisi store saat membaca; diabaikan saat menyimpan.
    pub updated_at_ms: i64,
}

impl RunRecord {
    #[must_use]
    pub fn new(id: String, repo_id: String, record_id: String) -> Self {
        Self {
            id,
            repo_id,
            record_id,
            state: RunState::Preparing,
            generation: 0,
            branch: None,
            worktree: None,
            session_id: None,
            session_started: false,
            pid: None,
            pr_url: None,
            queue: Vec::new(),
            last_error: None,
            automatic: false,
            note: None,
            workspace: None,
            extra: Vec::new(),
            phase: RunPhase::Implement,
            plan: None,
            plan_skill: None,
            quiet_until_ms: None,
            fix_requested_by: None,
            pr_body_hashes: BTreeMap::new(),
            review_snapshot: BTreeMap::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }
}

/// Fase kerja run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RunPhase {
    /// Agent mengerjakan kode.
    #[default]
    Implement,
    /// Agent hanya membaca kode dan menyusun plan; kode ditulis setelah dev menyetujui.
    Plan,
    /// Sebelum plan: agent membaca kode dan menanyakan hal yang belum jelas lewat chat.
    Brainstorm,
}

impl RunPhase {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Implement => "implement",
            Self::Plan => "plan",
            Self::Brainstorm => "brainstorm",
        }
    }
}

/// Workspace sebuah run: id dan folder run berisi worktree setiap repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunWorkspace {
    pub id: String,
    pub root: PathBuf,
}

/// Repo tambahan dalam run workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunMember {
    pub repo_id: String,
    pub worktree: PathBuf,
    pub pr_url: Option<String>,
}

/// Isi chat yang disimpan per run.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatItem {
    Dev(String),
    Agent(AgentEvent),
    System(String),
}

/// Penyimpanan run dan chat.
pub trait RunStore: Send + Sync {
    /// Membuat run; `PortError::Conflict` bila record sudah punya run aktif.
    fn create(
        &self,
        run: &RunRecord,
        now_ms: i64,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    fn get(&self, id: &str) -> impl Future<Output = Result<Option<RunRecord>, PortError>> + Send;
    fn active_for(
        &self,
        record_id: &str,
    ) -> impl Future<Output = Result<Option<RunRecord>, PortError>> + Send;
    fn list_active(&self) -> impl Future<Output = Result<Vec<RunRecord>, PortError>> + Send;
    /// Menyimpan run bila generasi di penyimpanan masih `expected_generation`;
    /// selain itu `PortError::Conflict`.
    fn save(
        &self,
        run: &RunRecord,
        expected_generation: i64,
        now_ms: i64,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    fn close(&self, id: &str, now_ms: i64) -> impl Future<Output = Result<(), PortError>> + Send;
    fn append(
        &self,
        run_id: &str,
        item: &ChatItem,
        now_ms: i64,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}

/// Jurnal komentar dua tahap (KTD-12): `pending` sebelum dikirim ke agent,
/// `delivered` setelah giliran diterima proses agent.
pub trait Journal: Send + Sync {
    fn journal_pending(
        &self,
        record_id: &str,
        reply_id: &str,
        run_id: &str,
        now_ms: i64,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    fn journal_delivered(
        &self,
        record_id: &str,
        reply_id: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Balasan yang masih `pending` (terkirim belum pasti) untuk record.
    fn journal_pending_list(
        &self,
        record_id: &str,
    ) -> impl Future<Output = Result<Vec<String>, PortError>> + Send;
    /// Balasan yang sudah `delivered`.
    fn journal_delivered_list(
        &self,
        record_id: &str,
    ) -> impl Future<Output = Result<Vec<String>, PortError>> + Send;
}

/// Jenis notifikasi desktop (R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    AwaitingAnswer,
    Failed,
    WaitingFix,
    NeedsAttention,
    PrCreated,
}

impl NoticeKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingAnswer => "awaiting_answer",
            Self::Failed => "failed",
            Self::WaitingFix => "waiting_fix",
            Self::NeedsAttention => "needs_attention",
            Self::PrCreated => "pr_created",
        }
    }
}

/// Notifikasi untuk dev.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub record_id: String,
    pub run_id: Option<String>,
    pub title: String,
    pub body: String,
}

/// Pengirim notifikasi; antre bila tidak ada client tersambung.
pub trait Notifier: Send + Sync {
    fn notify(&self, notice: Notice) -> impl Future<Output = ()> + Send;
}

/// Menjalankan task latar yang punya pemilik (`TaskTracker` di daemon).
pub trait Spawner: Send + Sync {
    fn spawn<F>(&self, fut: F)
    where
        F: Future<Output = ()> + Send + 'static;
}

/// Konteks pesan sesi perbaikan Waiting Fix.
#[derive(Debug, Clone, Copy)]
pub struct FixContext<'a> {
    pub task: &'a Task,
    /// Teks AC; diisi hanya untuk sesi baru.
    pub acs: &'a [String],
    /// Diff PR; diisi hanya untuk sesi baru.
    pub diff: Option<&'a str>,
    pub comments: &'a [Reply],
    /// Instruksi dev (tepercaya), dari "Mulai perbaikan".
    pub instruction: Option<&'a str>,
}

/// Penyusun pesan agent: instruksi operator di luar blok, data Lark di dalam
/// blok berpembatas nonce (KTD-15).
pub trait Prompts: Send + Sync {
    fn task_message(&self, task: &Task, acs: &[String], dev_note: Option<&str>) -> String;
    fn fix_message(&self, ctx: &FixContext<'_>) -> String;
    /// Pesan giliran plan: baca kode, jangan ubah berkas, tulis plan di pesan
    /// akhir. `skill`: skill plugin yang dipakai untuk menyusun plan.
    fn plan_message(
        &self,
        task: &Task,
        acs: &[String],
        dev_note: Option<&str>,
        skill: Option<&str>,
    ) -> String;
    /// Pesan setelah dev menyetujui plan: kerjakan plan itu.
    fn approve_message(&self) -> String;
    /// Pesan giliran brainstorm: baca kode, tanyakan hal yang belum jelas satu
    /// per satu, lalu ringkas requirements yang disepakati.
    fn brainstorm_message(
        &self,
        task: &Task,
        acs: &[String],
        dev_note: Option<&str>,
        skill: Option<&str>,
    ) -> String;
    /// Pesan dev `/nama argumen`: jalankan skill `nama` dengan argumen itu.
    fn command_message(&self, name: &str, args: &str) -> String;
    /// Pesan dari brainstorm ke plan, di sesi yang sama.
    fn continue_plan_message(&self, skill: Option<&str>) -> String;
}
