//! Event sesi agent yang sudah dinormalkan dari stream backend (KTD-10).

/// Pemakaian token dari event `result`, disimpan apa adanya.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl Usage {
    /// Total token masukan termasuk cache.
    #[must_use]
    pub fn total_input(&self) -> u64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
}

/// Satu event dari sesi agent.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// Sesi dimulai; `session_id` dipakai untuk `--resume`.
    SessionStarted {
        session_id: String,
        model: Option<String>,
    },
    AssistantMessage {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        /// Ringkasan input (perintah, path, pola), maksimal 200 karakter.
        preview: String,
    },
    ToolResult {
        tool_use_id: String,
        is_error: bool,
        preview: String,
    },
    /// Tool call ditolak pagar izin (allowlist atau tanpa approval surface).
    PermissionDenied {
        tool_name: String,
        tool_use_id: String,
        reason: String,
    },
    /// Status batas pemakaian langganan.
    RateLimit {
        status: String,
        utilization: Option<f64>,
    },
    Usage(Usage),
    /// Giliran selesai.
    Done {
        is_error: bool,
        result: String,
        num_turns: u32,
        denials: u32,
        /// Biaya giliran menurut CLI (`total_cost_usd`).
        cost_usd: Option<f64>,
        /// Batas yang menghentikan giliran: `budget` atau `turns`.
        limit: Option<String>,
    },
    /// Proses berakhir tanpa event `result`.
    Failed {
        message: String,
    },
}
