//! Trait port yang dibutuhkan use case, dibundel dalam [`Deps`].

use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

use mandor_domain::{AppField, CardFlag, Reply, Task, TaskStatus, WriteOp};

/// Kegagalan adapter, sudah diterjemahkan dari error infrastruktur.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PortError {
    #[error("Lark session expired; please log in again")]
    Reauth,
    #[error("Lark is rate limiting requests")]
    Throttled,
    #[error("write conflict in Lark")]
    Conflict,
    #[error("temporary failure: {0}")]
    Transient(String),
    #[error("rejected: {0}")]
    Rejected(String),
    #[error("local storage: {0}")]
    Storage(String),
}

/// Akses record Sprint Tasks atas nama pengguna.
pub trait LarkTasks: Send + Sync {
    /// Record yang di-assign ke `me`; `modified_since_ms` membatasi ke record
    /// yang berubah sejak waktu itu (poll inkremental).
    fn search_assigned(
        &self,
        me: &str,
        modified_since_ms: Option<i64>,
    ) -> impl Future<Output = Result<Vec<Task>, PortError>> + Send;

    /// Kartu seluruh tim untuk papan Team: semua yang belum Done, ditambah yang
    /// Done dan berubah sejak `done_since_ms`.
    fn search_team(
        &self,
        done_since_ms: i64,
    ) -> impl Future<Output = Result<Vec<Task>, PortError>> + Send;

    /// Baca segar record berdasarkan ID; record yang tidak ada tidak dikembalikan.
    fn fetch(&self, ids: &[String]) -> impl Future<Output = Result<Vec<Task>, PortError>> + Send;

    /// Menulis status (opsional) dan field milik app dalam satu PUT.
    fn write(
        &self,
        record_id: &str,
        status: Option<&TaskStatus>,
        fields: &[AppField],
    ) -> impl Future<Output = Result<Task, PortError>> + Send;

    /// Balasan komentar di Base yang terpetakan ke record. Paging boleh berhenti
    /// saat seluruh komentar di halaman lebih lama dari `stop_before` (detik).
    fn record_replies(
        &self,
        stop_before: Option<i64>,
    ) -> impl Future<Output = Result<Vec<Reply>, PortError>> + Send;

    /// Teks Acceptance Criteria dari tabel tertaut, urut sesuai `ids`.
    fn acceptance_criteria(
        &self,
        ids: &[String],
    ) -> impl Future<Output = Result<Vec<AcItem>, PortError>> + Send;

    /// Sprint tertaut di field Sprint, urut sesuai `ids`; ID yang tidak ada dilewati.
    fn sprints(
        &self,
        ids: &[String],
    ) -> impl Future<Output = Result<Vec<SprintItem>, PortError>> + Send;

    /// Epic tertaut di field Epic, urut sesuai `ids`; ID yang tidak ada dilewati.
    fn epics(
        &self,
        ids: &[String],
    ) -> impl Future<Output = Result<Vec<EpicItem>, PortError>> + Send;

    /// DM dari bot app ke developer yang login di daemon ini, dan hanya ke dia
    /// (R13, R24 plan VM). Tidak ada parameter penerima.
    fn message_me(&self, text: &str) -> impl Future<Output = Result<(), PortError>> + Send;

    /// Nama tampilan per `open_id`; ID yang tidak dikenal tidak ada di hasil.
    fn user_names(
        &self,
        open_ids: &[String],
    ) -> impl Future<Output = Result<HashMap<String, String>, PortError>> + Send;
}

/// Satu sprint dari tabel tertaut field Sprint (mis. "Sprint Status").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SprintItem {
    pub record_id: String,
    pub name: String,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    /// Status sprint, mis. "In Progress", "Done".
    pub status: Option<String>,
}

/// Satu epic dari tabel tertaut field Epic (mis. "Epic Pegasus").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicItem {
    pub record_id: String,
    /// Kunci epic, mis. `E-PGS43`.
    pub key: Option<String>,
    pub name: String,
}

/// Satu Acceptance Criteria.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AcItem {
    pub record_id: String,
    /// `AC-xxx`.
    pub label: Option<String>,
    pub text: String,
    /// AC Type "Droped": tidak dikirim ke agent.
    pub dropped: bool,
    /// Isi field Feedback QA (R11f).
    pub feedback_qa: Option<String>,
    /// Waktu ubah terakhir record AC (ms epoch).
    pub last_modified_ms: i64,
    /// `open_id` pengubah terakhir record AC, bila Lark mengembalikannya.
    pub last_modified_by: Option<String>,
}

/// Record di cache lokal beserta penanda kartunya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedTask {
    pub task: Task,
    pub flag: Option<CardFlag>,
}

/// Cache record lokal.
pub trait TaskCache: Send + Sync {
    /// Menyimpan nilai terbaru dari Lark; penanda yang ada dipertahankan.
    fn upsert(&self, task: &Task) -> impl Future<Output = Result<(), PortError>> + Send;
    fn get(
        &self,
        record_id: &str,
    ) -> impl Future<Output = Result<Option<CachedTask>, PortError>> + Send;
    fn list(&self) -> impl Future<Output = Result<Vec<CachedTask>, PortError>> + Send;
    fn set_flag(
        &self,
        record_id: &str,
        flag: Option<&CardFlag>,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}

/// Tulisan yang menunggu di outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedWrite {
    pub id: i64,
    pub record_id: String,
    pub op: WriteOp,
    pub attempts: u32,
}

/// Antrean tulis Lark yang persisten.
pub trait Outbox: Send + Sync {
    fn enqueue(
        &self,
        record_id: &str,
        op: &WriteOp,
        now_ms: i64,
    ) -> impl Future<Output = Result<i64, PortError>> + Send;
    fn due(
        &self,
        now_ms: i64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<QueuedWrite>, PortError>> + Send;
    fn complete(&self, id: i64) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Dibatalkan oleh validasi ulang; tidak dicoba lagi.
    fn cancel(&self, id: i64, reason: &str) -> impl Future<Output = Result<(), PortError>> + Send;
    fn retry_later(
        &self,
        id: i64,
        attempts: u32,
        next_attempt_ms: i64,
        error: &str,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Gagal permanen sampai dev menekan coba lagi.
    fn fail(&self, id: i64, error: &str) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Mengantrekan ulang tulisan gagal untuk record; validasi ulang tetap berlaku.
    fn requeue_failed(
        &self,
        record_id: &str,
        now_ms: i64,
    ) -> impl Future<Output = Result<u32, PortError>> + Send;
}

/// Waktu dan jeda; disuntikkan agar test tidak menunggu sungguhan.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
    fn sleep(&self, d: Duration) -> impl Future<Output = ()> + Send;
}

/// Pembuat ID acak (nonce klaim, ID run).
pub trait Ids: Send + Sync {
    fn nonce(&self) -> String;
}

/// Bundel semua port.
pub trait Deps: Send + Sync + 'static {
    type Lark: LarkTasks + 'static;
    type Cache: TaskCache + 'static;
    type Outbox: Outbox + 'static;
    type Clock: Clock + 'static;
    type Ids: Ids + 'static;
    type Git: crate::run_ports::GitPort + 'static;
    type Agent: crate::run_ports::AgentPort + 'static;
    type Runs: crate::run_ports::RunStore + crate::run_ports::Journal + 'static;
    type Notifier: crate::run_ports::Notifier + 'static;
    type Spawner: crate::run_ports::Spawner + 'static;
    type Repos: crate::run_ports::Repos + 'static;
    type Prompts: crate::run_ports::Prompts + 'static;
    /// Penyimpanan Rilis 2: buku AC (dan, di unit berikutnya, review, sumber
    /// desain, setelan).
    type Reviews: crate::run_ports::LedgerStore + crate::run_ports::ReviewHistory + 'static;
    type Apps: crate::run_ports::AppProcesses + crate::run_ports::AppRecords + 'static;
    /// Antrean, siklus, dan setelan lembur VM (KTD-57 plan VM).
    type Overnight: crate::overnight::OvernightStore + 'static;
    /// Sumber desain milik daemon (plan 2026-10-30-001, KTD-8).
    type Design: crate::review::DesignSources + 'static;
}
