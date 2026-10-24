//! Port antrean, siklus, dan setelan lembur VM (KTD-57 plan VM).

use std::future::Future;

use crate::StartTarget;
use crate::ports::PortError;

/// Satu kartu di antrean VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub record_id: String,
    /// Repo atau workspace yang dipilih saat mengantre.
    pub target: StartTarget,
    /// Catatan atau jawaban dev untuk agent (kartu `Held` yang diantre ulang).
    pub note: Option<String>,
    /// Dev meminta "Start now".
    pub start_now: bool,
    pub added_at_ms: i64,
}

/// Kartu yang keluar dari antrean tanpa dikerjakan, untuk laporan pagi (R9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dequeued {
    pub record_id: String,
    pub reason: String,
    pub at_ms: i64,
}

/// Setelan lembur developer (R7, R10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OvernightSettings {
    /// Jam mulai lembur, `HH:MM` waktu lokal.
    pub start: String,
    /// Jam selesai lembur, `HH:MM` waktu lokal.
    pub end: String,
    /// Offset UTC tetap, mis. `+07:00` (KTD-58).
    pub offset: String,
    /// Batas token (input + output) per siklus; `None` = belum diisi, VM tidak
    /// memulai apa pun.
    pub token_limit: Option<u64>,
}

/// Pemakaian token satu siklus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CycleUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Dicatat terpisah untuk laporan; tidak dihitung ke batas.
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl CycleUsage {
    /// Token yang dihitung ke batas siklus (KTD-59).
    #[must_use]
    pub fn counted(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// Status DM laporan satu siklus (R24).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportState {
    pub sent: bool,
    /// Percobaan kirim yang sudah dilakukan.
    pub attempts: u32,
}

/// Penyimpanan antrean, siklus, dan setelan lembur.
pub trait OvernightStore: Send + Sync {
    /// Antrean aktif, urut posisi.
    fn queue(&self) -> impl Future<Output = Result<Vec<QueueItem>, PortError>> + Send;
    /// Menambah kartu di akhir antrean; `Conflict` bila sudah ada.
    fn enqueue(&self, item: &QueueItem) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Mengeluarkan kartu. `reason` dicatat untuk laporan pagi; `None` berarti
    /// kartu mulai dikerjakan atau dikeluarkan dev.
    fn dequeue(
        &self,
        record_id: &str,
        reason: Option<&str>,
        now_ms: i64,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Mengurutkan ulang antrean sesuai `record_ids`; kartu lain tetap di akhir.
    fn reorder(&self, record_ids: &[String]) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Menandai "Start now"; `NotFound` bila kartu tidak di antrean.
    fn set_start_now(&self, record_id: &str) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Kartu yang dikeluarkan dengan alasan sejak `since_ms`.
    fn dequeued_since(
        &self,
        since_ms: i64,
    ) -> impl Future<Output = Result<Vec<Dequeued>, PortError>> + Send;
    /// Kartu yang sudah diserahkan ke laptop dan tidak boleh diantre lagi.
    fn laptop_only(&self, record_id: &str) -> impl Future<Output = Result<bool, PortError>> + Send;
    fn set_laptop_only(
        &self,
        record_id: &str,
        now_ms: i64,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    fn settings(&self)
    -> impl Future<Output = Result<Option<OvernightSettings>, PortError>> + Send;
    fn set_settings(
        &self,
        settings: &OvernightSettings,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Pemakaian siklus yang dimulai `cycle_start_ms`; nol bila belum ada.
    fn cycle_usage(
        &self,
        cycle_start_ms: i64,
    ) -> impl Future<Output = Result<CycleUsage, PortError>> + Send;
    /// Status DM laporan siklus.
    fn report_state(
        &self,
        cycle_start_ms: i64,
    ) -> impl Future<Output = Result<ReportState, PortError>> + Send;
    /// Mencatat satu percobaan kirim DM laporan; `sent` bila berhasil.
    fn record_report(
        &self,
        cycle_start_ms: i64,
        sent: bool,
        now_ms: i64,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
    /// Menambah pemakaian ke siklus.
    fn add_cycle_usage(
        &self,
        cycle_start_ms: i64,
        usage: &CycleUsage,
    ) -> impl Future<Output = Result<(), PortError>> + Send;
}
