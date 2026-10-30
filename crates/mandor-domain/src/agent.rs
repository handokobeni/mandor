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

/// Penghitung pemakaian per pesan: pesan dengan `message_id` sama dihitung
/// sekali, memakai nilai terbesar tiap kolom (KTD-59 plan VM).
#[derive(Debug, Default)]
pub struct UsageTracker {
    seen: std::collections::HashMap<String, Usage>,
}

impl UsageTracker {
    /// Tambahan pemakaian dari satu event pesan.
    pub fn observe(&mut self, message_id: &str, usage: &Usage) -> Usage {
        let prev = self.seen.entry(message_id.to_owned()).or_default();
        let grow = |old: &mut u64, new: u64| {
            let d = new.saturating_sub(*old);
            *old = (*old).max(new);
            d
        };
        Usage {
            input_tokens: grow(&mut prev.input_tokens, usage.input_tokens),
            output_tokens: grow(&mut prev.output_tokens, usage.output_tokens),
            cache_creation_input_tokens: grow(
                &mut prev.cache_creation_input_tokens,
                usage.cache_creation_input_tokens,
            ),
            cache_read_input_tokens: grow(
                &mut prev.cache_read_input_tokens,
                usage.cache_read_input_tokens,
            ),
        }
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
    /// Pemakaian satu pesan assistant (bukan subagent), untuk batas siklus VM
    /// (KTD-59 plan VM). Pesan yang sama bisa muncul di beberapa baris.
    MessageUsage {
        message_id: String,
        usage: Usage,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn u(i: u64, o: u64) -> Usage {
        Usage {
            input_tokens: i,
            output_tokens: o,
            ..Usage::default()
        }
    }

    #[test]
    fn pesan_yang_sama_dihitung_sekali() {
        let mut t = UsageTracker::default();
        assert_eq!(t.observe("m1", &u(10, 2)), u(10, 2));
        assert_eq!(
            t.observe("m1", &u(10, 2)),
            u(0, 0),
            "baris kedua pesan sama"
        );
        assert_eq!(t.observe("m1", &u(10, 5)), u(0, 3), "output bertambah");
        assert_eq!(t.observe("m2", &u(4, 1)), u(4, 1));
    }
}
