//! Keep-awake milik daemon (KTD-45 plan service OS): penjaga dipegang selama
//! ada run yang bekerja dan dilepas setelah tidak ada run yang bekerja selama
//! masa tenggang. Logika murni; menjalankan penjaga ada di `mandord`.

/// Masa tenggang sebelum keep-awake dilepas, agar laptop tidak sleep di celah
/// antar langkah run.
pub const DEFAULT_GRACE_MS: i64 = 5 * 60 * 1000;

/// Jeda pertama sebelum penjaga yang mati dijalankan ulang.
pub const RESPAWN_BASE_MS: i64 = 5_000;
/// Jeda terpanjang antar percobaan menjalankan ulang penjaga.
pub const RESPAWN_MAX_MS: i64 = 5 * 60 * 1000;
/// Kegagalan beruntun sebelum status menjadi `failing`.
pub const FAILING_AFTER: u32 = 3;

/// Perubahan yang harus dilakukan pada penjaga.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Hold,
    Release,
    Keep,
}

/// Status keep-awake yang dilaporkan `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepAwakeState {
    /// Tidak ada perintah penjaga yang dikonfigurasi.
    NotConfigured,
    /// Tidak ada run yang bekerja; pengaturan sleep tidak diubah.
    Off,
    /// Penjaga berjalan dan menahan sleep.
    Held,
    /// Penjaga gagal dijalankan berulang kali.
    Failing,
}

impl KeepAwakeState {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Off => "off",
            Self::Held => "held",
            Self::Failing => "failing",
        }
    }
}

/// Keputusan pegang/lepas dari riwayat sinyal `busy`.
#[derive(Debug, Clone, Default)]
pub struct KeepAwake {
    holding: bool,
    idle_since: Option<i64>,
}

impl KeepAwake {
    /// Mencatat `busy` saat ini dan mengembalikan perubahan yang diperlukan.
    pub fn observe(&mut self, busy: bool, now_ms: i64, grace_ms: i64) -> Change {
        if busy {
            self.idle_since = None;
            if self.holding {
                return Change::Keep;
            }
            self.holding = true;
            return Change::Hold;
        }
        if !self.holding {
            return Change::Keep;
        }
        let since = *self.idle_since.get_or_insert(now_ms);
        if now_ms.saturating_sub(since) >= grace_ms {
            self.holding = false;
            self.idle_since = None;
            return Change::Release;
        }
        Change::Keep
    }

    /// Keep-awake sedang diminta.
    #[must_use]
    pub fn holding(&self) -> bool {
        self.holding
    }
}

/// Jadwal menjalankan ulang penjaga yang keluar sendiri atau gagal dijalankan.
#[derive(Debug, Clone, Default)]
pub struct Respawn {
    failures: u32,
    next_at: Option<i64>,
}

impl Respawn {
    /// Penjaga keluar atau gagal dijalankan pada `now_ms`.
    pub fn failed(&mut self, now_ms: i64) {
        self.failures = self.failures.saturating_add(1);
        let shift = self.failures.saturating_sub(1).min(16);
        let delay = RESPAWN_BASE_MS
            .saturating_mul(1_i64 << shift)
            .min(RESPAWN_MAX_MS);
        self.next_at = Some(now_ms.saturating_add(delay));
    }

    /// Penjaga berjalan stabil; hitungan kegagalan direset.
    pub fn succeeded(&mut self) {
        self.failures = 0;
        self.next_at = None;
    }

    /// Penjaga boleh dicoba dijalankan lagi.
    #[must_use]
    pub fn due(&self, now_ms: i64) -> bool {
        self.next_at.is_none_or(|at| now_ms >= at)
    }

    /// Kegagalan beruntun sudah cukup untuk melapor `failing`.
    #[must_use]
    pub fn failing(&self) -> bool {
        self.failures >= FAILING_AFTER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: i64 = DEFAULT_GRACE_MS;

    #[test]
    fn busy_memegang_sekali() {
        let mut k = KeepAwake::default();
        assert_eq!(k.observe(true, 0, G), Change::Hold);
        assert_eq!(k.observe(true, 1_000, G), Change::Keep);
        assert!(k.holding());
    }

    #[test]
    fn tidak_busy_melepas_hanya_setelah_masa_tenggang() {
        let mut k = KeepAwake::default();
        k.observe(true, 0, G);
        assert_eq!(k.observe(false, 10_000, G), Change::Keep);
        assert_eq!(k.observe(false, 10_000 + G - 1, G), Change::Keep);
        assert_eq!(k.observe(false, 10_000 + G, G), Change::Release);
        assert!(!k.holding());
        assert_eq!(k.observe(false, 20_000 + G, G), Change::Keep);
    }

    #[test]
    fn busy_lagi_di_dalam_masa_tenggang_tidak_melepas() {
        let mut k = KeepAwake::default();
        k.observe(true, 0, G);
        k.observe(false, 1_000, G);
        assert_eq!(k.observe(true, 2_000, G), Change::Keep);
        // Masa tenggang dihitung ulang dari idle berikutnya.
        assert_eq!(k.observe(false, 3_000, G), Change::Keep);
        assert_eq!(k.observe(false, 1_000 + G, G), Change::Keep);
        assert_eq!(k.observe(false, 3_000 + G, G), Change::Release);
    }

    #[test]
    fn tanpa_busy_tidak_pernah_memegang() {
        let mut k = KeepAwake::default();
        assert_eq!(k.observe(false, 0, G), Change::Keep);
        assert!(!k.holding());
    }

    #[test]
    fn jeda_menjalankan_ulang_makin_panjang_dan_failing_setelah_tiga() {
        let mut r = Respawn::default();
        assert!(r.due(0));
        r.failed(0);
        assert!(!r.due(RESPAWN_BASE_MS - 1));
        assert!(r.due(RESPAWN_BASE_MS));
        assert!(!r.failing());
        r.failed(10_000);
        assert!(!r.due(10_000 + 2 * RESPAWN_BASE_MS - 1));
        assert!(r.due(10_000 + 2 * RESPAWN_BASE_MS));
        r.failed(100_000);
        assert!(r.failing());
        for _ in 0..40 {
            r.failed(0);
        }
        assert!(r.due(RESPAWN_MAX_MS), "jeda dibatasi");
        r.succeeded();
        assert!(!r.failing() && r.due(0));
    }

    #[test]
    fn nama_status() {
        assert_eq!(KeepAwakeState::NotConfigured.as_str(), "not_configured");
        assert_eq!(KeepAwakeState::Held.as_str(), "held");
        assert_eq!(KeepAwakeState::Failing.as_str(), "failing");
        assert_eq!(KeepAwakeState::Off.as_str(), "off");
    }
}
