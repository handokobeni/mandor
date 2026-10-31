//! State alur kerja per record di laptop pemilik (diagram di rencana) dan nama
//! branch task.

/// State run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunState {
    Preparing,
    Running,
    AwaitingDev,
    Publishing,
    InReviewLoop,
    WfAwaitingComment,
    WfQuiet,
    WfRunning,
    /// Sesi reviewer AC berjalan; agent pelaksana dikunci (KTD-29).
    Reviewing,
    Failed,
    Orphaned,
    /// Record dihapus atau tidak lagi di-assign: agent dihentikan, dev menutup run.
    Stopped,
    /// Kartu lembur VM tertahan dengan alasan (KTD-60 plan VM): menunggu
    /// jawaban atau keputusan dev, lalu diantre ulang. Bukan parkir dan tidak
    /// sibuk.
    Held,
}

impl RunState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::AwaitingDev => "awaiting_dev",
            Self::Publishing => "publishing",
            Self::InReviewLoop => "in_review_loop",
            Self::WfAwaitingComment => "wf_awaiting_comment",
            Self::WfQuiet => "wf_quiet",
            Self::WfRunning => "wf_running",
            Self::Reviewing => "reviewing",
            Self::Failed => "failed",
            Self::Orphaned => "orphaned",
            Self::Stopped => "stopped",
            Self::Held => "held",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "preparing" => Self::Preparing,
            "running" => Self::Running,
            "awaiting_dev" => Self::AwaitingDev,
            "publishing" => Self::Publishing,
            "in_review_loop" => Self::InReviewLoop,
            "wf_awaiting_comment" => Self::WfAwaitingComment,
            "wf_quiet" => Self::WfQuiet,
            "wf_running" => Self::WfRunning,
            "reviewing" => Self::Reviewing,
            "failed" => Self::Failed,
            "orphaned" => Self::Orphaned,
            "stopped" => Self::Stopped,
            "held" => Self::Held,
            _ => return None,
        })
    }

    /// Agent sedang memproses giliran.
    #[must_use]
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Running | Self::WfRunning | Self::Reviewing)
    }

    /// Run sedang bekerja dan tidak boleh diputus oleh restart daemon: agent
    /// atau reviewer berjalan, worktree sedang disiapkan, atau push/PR sedang
    /// berlangsung. Sama dengan state yang dianggap terputus oleh rekonsiliasi.
    #[must_use]
    pub fn is_working(self) -> bool {
        self.is_busy() || matches!(self, Self::Preparing | Self::Publishing)
    }

    /// Run diparkir (klaim hilang atau record hilang): tidak menerima pesan,
    /// tidak di-push, menunggu dev menutupnya.
    #[must_use]
    pub fn is_parked(self) -> bool {
        matches!(self, Self::Orphaned | Self::Stopped)
    }

    /// Record sedang dalam loop perbaikan Waiting Fix (KTD-6).
    #[must_use]
    pub fn in_fix_loop(self) -> bool {
        matches!(
            self,
            Self::WfAwaitingComment | Self::WfQuiet | Self::WfRunning
        )
    }
}

/// Alasan kartu VM tertahan (KTD-60 plan VM).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldReason {
    /// Agent buntu dan meninggalkan pertanyaan (R15).
    Stuck,
    /// Gerbang AC tidak siap: AC Fail atau Can't verify belum diputuskan (R12).
    AcGate,
    /// Push butuh konfirmasi dev karena alasan lain, mis. mengubah CI (R12).
    NeedsConfirmation,
    /// Batas pemakaian siklus atau kuota langganan habis (R10).
    UsageLimit,
    /// Kartu grup berubah di Lark sebelum gilirannya (R9 plan grup).
    CardRemoved,
    /// Jam lembur habis di antara kartu grup; dilanjutkan siklus berikutnya
    /// (R21 plan grup).
    WindowEnded,
    /// Grup memberi jalan ke Waiting Fix kartu VM lain (R20 plan grup).
    YieldToFix,
}

impl HoldReason {
    /// Nama stabil untuk penyimpanan dan protokol.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stuck => "stuck",
            Self::AcGate => "ac_gate",
            Self::NeedsConfirmation => "needs_confirmation",
            Self::UsageLimit => "usage_limit",
            Self::CardRemoved => "card_removed",
            Self::WindowEnded => "window_ended",
            Self::YieldToFix => "yield_to_fix",
        }
    }

    /// Dev perlu bertindak. Jeda grup (`WindowEnded`, `YieldToFix`) berlanjut
    /// sendiri sehingga tidak memanggil dev (KTD-G12 plan grup).
    #[must_use]
    pub fn needs_attention(self) -> bool {
        !matches!(self, Self::WindowEnded | Self::YieldToFix)
    }

    /// Kebalikan [`HoldReason::as_str`]; nilai tak dikenal menjadi `None`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "stuck" => Self::Stuck,
            "ac_gate" => Self::AcGate,
            "needs_confirmation" => Self::NeedsConfirmation,
            "usage_limit" => Self::UsageLimit,
            "card_removed" => Self::CardRemoved,
            "window_ended" => Self::WindowEnded,
            "yield_to_fix" => Self::YieldToFix,
            _ => return None,
        })
    }
}

/// Ada run yang sedang bekerja di antara state ini (sinyal `busy` daemon).
#[must_use]
pub fn any_working(states: impl IntoIterator<Item = RunState>) -> bool {
    states.into_iter().any(RunState::is_working)
}

/// Alasan klaim run tidak berlaku lagi (heartbeat).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimLoss {
    /// Record dihapus di Lark.
    Deleted,
    /// Record tidak lagi di-assign ke pengguna, walau klaim masih miliknya.
    Unassigned,
    /// Klaim diambil dev lain (ambil alih).
    TakenByOther,
}

impl ClaimLoss {
    /// State run setelah klaim hilang: pekerjaan dev lain membuat run diparkir
    /// (Orphaned); record yang hilang dari tangan pengguna menghentikan run (Stopped).
    #[must_use]
    pub fn run_state(self) -> RunState {
        match self {
            Self::Deleted | Self::Unassigned => RunState::Stopped,
            Self::TakenByOther => RunState::Orphaned,
        }
    }

    /// Penjelasan untuk notifikasi dev.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Deleted => "Task was deleted in Lark; run stopped",
            Self::Unassigned => "Task is no longer assigned to you; run stopped",
            Self::TakenByOther => "Task claim lost; run stopped",
        }
    }
}

/// Perintah skill dari chat dev: `/nama argumen`. Nama hanya huruf, angka,
/// `:`, `-`, `_`, `.`, dan harus diikuti spasi atau akhir teks, sehingga path
/// seperti `/home/u/a.rs` bukan perintah.
#[must_use]
pub fn slash_command(text: &str) -> Option<(&str, &str)> {
    let rest = text.trim_start().strip_prefix('/')?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.')))
        .unwrap_or(rest.len());
    let (name, tail) = rest.split_at(end);
    if name.is_empty() || !(tail.is_empty() || tail.starts_with(char::is_whitespace)) {
        return None;
    }
    Some((name, tail.trim()))
}

/// Nama branch task: `mandor/<key>-<slug judul>` (huruf kecil, maks 60 karakter).
#[must_use]
pub fn branch_name(key: Option<&str>, record_id: &str, title: &str) -> String {
    let head = key.unwrap_or(record_id).to_lowercase();
    let mut slug = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let mut name = format!("mandor/{head}");
    let slug = slug.trim_end_matches('-');
    if !slug.is_empty() {
        name.push('-');
        name.push_str(slug);
    }
    if name.len() > 60 {
        name.truncate(60);
        while name.ends_with('-') {
            name.pop();
        }
    }
    name
}

/// Path yang membuat push otomatis ditahan untuk konfirmasi dev (KTD-19).
const SENSITIVE_PREFIXES: &[&str] = &[
    ".github/workflows/",
    ".claude/",
    ".husky/",
    ".githooks/",
    ".git/",
];
const SENSITIVE_FILES: &[&str] = &[
    "CLAUDE.md",
    "lefthook.yml",
    "lefthook.yaml",
    ".pre-commit-config.yaml",
];

/// `true` bila salah satu path menyentuh CI, konfigurasi agent, atau hook git.
#[must_use]
pub fn touches_sensitive_path<'a>(paths: impl IntoIterator<Item = &'a str>) -> bool {
    paths.into_iter().any(|p| {
        let p = p.trim_start_matches("./");
        SENSITIVE_PREFIXES.iter().any(|pre| p.starts_with(pre))
            || SENSITIVE_FILES
                .iter()
                .any(|f| p == *f || p.ends_with(&format!("/{f}")))
    })
}

#[cfg(test)]
mod slash_tests {
    use super::slash_command;

    #[test]
    fn perintah_skill_dan_bukan_perintah() {
        assert_eq!(
            slash_command("/compound-engineering:ce-plan fokus ke migrasi\nbaris 2"),
            Some(("compound-engineering:ce-plan", "fokus ke migrasi\nbaris 2"))
        );
        assert_eq!(slash_command("  /tdd"), Some(("tdd", "")));
        assert_eq!(slash_command("/home/u/a.rs perlu dicek"), None);
        assert_eq!(slash_command("/"), None);
        assert_eq!(slash_command("tolong /tdd"), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_bekerja_mencakup_persiapan_dan_publish() {
        let bekerja = [
            RunState::Running,
            RunState::WfRunning,
            RunState::Reviewing,
            RunState::Preparing,
            RunState::Publishing,
        ];
        for s in bekerja {
            assert!(s.is_working(), "{s:?}");
        }
        for s in [
            RunState::AwaitingDev,
            RunState::InReviewLoop,
            RunState::WfAwaitingComment,
            RunState::WfQuiet,
            RunState::Failed,
            RunState::Orphaned,
            RunState::Stopped,
            RunState::Held,
        ] {
            assert!(!s.is_working(), "{s:?}");
        }
    }

    #[test]
    fn ada_run_bekerja_dari_kumpulan_state() {
        assert!(!any_working([]));
        assert!(!any_working([RunState::AwaitingDev, RunState::Failed]));
        assert!(any_working([RunState::AwaitingDev, RunState::Publishing]));
    }

    #[test]
    fn state_bolak_balik() {
        for s in [
            RunState::Preparing,
            RunState::Running,
            RunState::AwaitingDev,
            RunState::Publishing,
            RunState::InReviewLoop,
            RunState::WfAwaitingComment,
            RunState::WfQuiet,
            RunState::WfRunning,
            RunState::Failed,
            RunState::Orphaned,
            RunState::Stopped,
            RunState::Held,
        ] {
            assert_eq!(RunState::parse(s.as_str()), Some(s));
        }
        assert!(!RunState::Held.is_parked() && !RunState::Held.is_busy());
        for r in [
            HoldReason::Stuck,
            HoldReason::AcGate,
            HoldReason::NeedsConfirmation,
            HoldReason::UsageLimit,
            HoldReason::CardRemoved,
            HoldReason::WindowEnded,
            HoldReason::YieldToFix,
        ] {
            assert_eq!(HoldReason::parse(r.as_str()), Some(r));
        }
        assert!(HoldReason::Stuck.needs_attention() && HoldReason::CardRemoved.needs_attention());
        assert!(
            !HoldReason::WindowEnded.needs_attention() && !HoldReason::YieldToFix.needs_attention(),
            "jeda grup tidak memanggil dev"
        );
        assert_eq!(HoldReason::parse("x"), None);
        assert_eq!(RunState::parse("x"), None);
        assert!(RunState::WfQuiet.in_fix_loop());
        assert!(!RunState::Running.in_fix_loop());
        assert!(RunState::Stopped.is_parked() && RunState::Orphaned.is_parked());
        assert!(!RunState::Failed.is_parked());
    }

    #[test]
    fn klaim_hilang_karena_dev_lain_memarkir_karena_record_hilang_menghentikan() {
        assert_eq!(ClaimLoss::TakenByOther.run_state(), RunState::Orphaned);
        assert_eq!(ClaimLoss::Deleted.run_state(), RunState::Stopped);
        assert_eq!(ClaimLoss::Unassigned.run_state(), RunState::Stopped);
        assert!(
            ClaimLoss::Unassigned
                .describe()
                .contains("no longer assigned")
        );
    }

    #[test]
    fn mendeteksi_path_sensitif() {
        for (path, want) in [
            (".github/workflows/ci.yml", true),
            (".claude/settings.json", true),
            ("CLAUDE.md", true),
            ("docs/CLAUDE.md", true),
            (".husky/pre-push", true),
            ("lefthook.yml", true),
            ("src/main.rs", false),
            ("github/workflows.md", false),
        ] {
            assert_eq!(touches_sensitive_path([path]), want, "{path}");
        }
    }

    #[test]
    fn nama_branch_dari_kunci_dan_judul() {
        assert_eq!(
            branch_name(Some("PGS-1388"), "rec1", "Profile Data: Tab Data"),
            "mandor/pgs-1388-profile-data-tab-data"
        );
        assert_eq!(branch_name(None, "recX", "!!!"), "mandor/recx");
        let long = branch_name(Some("PGS-1"), "r", &"kata panjang ".repeat(20));
        assert!(long.len() <= 60 && !long.ends_with('-'), "{long}");
    }
}
