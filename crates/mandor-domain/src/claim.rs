//! Klaim pemilik record tanpa compare-and-set (KTD-8).
//!
//! Klaim ditulis ke field Lark sebagai teks `open_id|ts_ms|nonce`. Sejak plan
//! VM (KTD-55) nonce berisi `<acak>~<daemon_id>` sehingga dua daemon milik
//! developer yang sama bisa dibedakan, dan pembaca lama tetap bisa mengurainya.

/// Interval heartbeat klaim.
pub const HEARTBEAT_MS: i64 = 5 * 60 * 1000;
/// Klaim tanpa heartbeat selama ini dianggap tidak aktif.
pub const STALE_MS: i64 = 30 * 60 * 1000;
/// Jeda sebelum klaim dibaca ulang.
pub const READBACK_DELAY_MS: u64 = 3_000;

/// Klaim pemilik.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub open_id: String,
    pub ts_ms: i64,
    pub nonce: String,
}

impl Claim {
    /// Mengurai teks field; `None` bila kosong atau bentuknya salah.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.trim().splitn(3, '|');
        let open_id = parts.next().filter(|s| !s.is_empty())?.to_owned();
        let ts_ms = parts.next()?.parse().ok()?;
        let nonce = parts.next().filter(|s| !s.is_empty())?.to_owned();
        Some(Self {
            open_id,
            ts_ms,
            nonce,
        })
    }

    /// Bentuk teks yang ditulis ke Lark.
    #[must_use]
    pub fn encode(&self) -> String {
        format!("{}|{}|{}", self.open_id, self.ts_ms, self.nonce)
    }

    /// `true` bila heartbeat terakhir lebih dari [`STALE_MS`] yang lalu.
    #[must_use]
    pub fn is_stale(&self, now_ms: i64) -> bool {
        now_ms - self.ts_ms > STALE_MS
    }

    /// `true` bila klaim milik `open_id` dan masih aktif.
    #[must_use]
    pub fn held_by(&self, open_id: &str, now_ms: i64) -> bool {
        self.open_id == open_id && !self.is_stale(now_ms)
    }

    /// Daemon penulis klaim; `None` untuk klaim lama tanpa identitas daemon.
    #[must_use]
    pub fn daemon_id(&self) -> Option<&str> {
        self.nonce
            .rsplit_once(DAEMON_SEP)
            .map(|(_, d)| d)
            .filter(|d| !d.is_empty())
    }
}

/// Pemisah acak dan id daemon di nonce klaim.
const DAEMON_SEP: char = '~';

/// Identitas daemon ini untuk klaim (KTD-54, KTD-55).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimIdentity {
    pub open_id: String,
    pub daemon_id: String,
    /// Klaim lama milik `open_id` tanpa id daemon dianggap milik daemon ini.
    /// Hanya untuk daemon laptop, agar run yang sudah berjalan sebelum upgrade
    /// tetap dipegang; daemon VM tidak pernah menulis klaim lama.
    pub legacy_is_mine: bool,
}

impl ClaimIdentity {
    /// Identitas dengan id daemon yang sudah disaring: hanya huruf, angka,
    /// `.`, `_`, dan `-`; karena `|` dan `~` adalah pemisah klaim.
    #[must_use]
    pub fn new(open_id: &str, daemon_id: &str, legacy_is_mine: bool) -> Self {
        let clean: String = daemon_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            .collect();
        Self {
            open_id: open_id.to_owned(),
            daemon_id: if clean.is_empty() {
                "daemon".to_owned()
            } else {
                clean
            },
            legacy_is_mine,
        }
    }

    /// Nonce klaim baru dari bagian acak.
    #[must_use]
    pub fn nonce(&self, random: &str) -> String {
        format!("{random}{DAEMON_SEP}{}", self.daemon_id)
    }

    /// Pemegang klaim dilihat dari daemon ini.
    #[must_use]
    pub fn holder(&self, claim: &Claim) -> Holder {
        if claim.open_id != self.open_id {
            return Holder::OtherPerson;
        }
        match claim.daemon_id() {
            Some(d) if d == self.daemon_id => Holder::ThisDaemon,
            None if self.legacy_is_mine => Holder::ThisDaemon,
            _ => Holder::OwnOtherDaemon,
        }
    }

    /// `true` bila daemon ini yang memegang klaim (aktif atau tidak).
    #[must_use]
    pub fn holds(&self, claim: &Claim) -> bool {
        self.holder(claim) == Holder::ThisDaemon
    }
}

/// Pemegang klaim relatif terhadap daemon ini (KTD-56).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    ThisDaemon,
    /// Daemon lain milik developer yang sama (mis. daemon VM).
    OwnOtherDaemon,
    OtherPerson,
}

impl Holder {
    /// Nama stabil untuk protokol.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ThisDaemon => "self",
            Self::OwnOtherDaemon => "own_other_daemon",
            Self::OtherPerson => "other_person",
        }
    }
}

/// Alasan klaim tidak boleh ditulis.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimRefused {
    #[error("task is being worked on by {0}")]
    HeldByOther(String),
    #[error(
        "claim by {0} is still active; take over only works after 30 minutes without a heartbeat"
    )]
    NotStale(String),
    /// Kartu dipegang daemon lain milik developer yang sama (KTD-55).
    #[error("task is held by your other Mandor daemon ({0})")]
    OwnOtherDaemon(String),
}

/// Keputusan sebelum menulis klaim, dari baca segar.
///
/// Klaim milik daemon ini selalu boleh ditulis ulang. Klaim aktif milik daemon
/// lain dari developer yang sama (atau klaim lama tanpa id daemon pada daemon
/// VM) menolak sampai basi; serah-terima memakai jalur sendiri (KTD-63). Klaim
/// aktif milik dev lain menolak Start; klaim yang sudah basi hanya boleh diambil
/// lewat `takeover`.
///
/// # Errors
/// [`ClaimRefused`] bila klaim aktif milik daemon atau dev lain.
pub fn may_claim(
    current: Option<&Claim>,
    me: &ClaimIdentity,
    now_ms: i64,
    takeover: bool,
) -> Result<(), ClaimRefused> {
    let Some(c) = current else { return Ok(()) };
    match me.holder(c) {
        Holder::ThisDaemon => return Ok(()),
        Holder::OwnOtherDaemon => {
            return if c.is_stale(now_ms) {
                Ok(())
            } else {
                Err(ClaimRefused::OwnOtherDaemon(
                    c.daemon_id().unwrap_or("unknown").to_owned(),
                ))
            };
        }
        Holder::OtherPerson => {}
    }
    if !c.is_stale(now_ms) {
        return Err(if takeover {
            ClaimRefused::NotStale(c.open_id.clone())
        } else {
            ClaimRefused::HeldByOther(c.open_id.clone())
        });
    }
    if takeover {
        Ok(())
    } else {
        Err(ClaimRefused::HeldByOther(c.open_id.clone()))
    }
}

/// `true` bila baca ulang menunjukkan tepat nonce milik kita.
#[must_use]
pub fn won(readback: Option<&Claim>, my_nonce: &str, me: &str) -> bool {
    readback.is_some_and(|c| c.nonce == my_nonce && c.open_id == me)
}

/// `true` bila heartbeat perlu dikirim.
#[must_use]
pub fn heartbeat_due(claim: &Claim, now_ms: i64) -> bool {
    now_ms - claim.ts_ms >= HEARTBEAT_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me(who: &str) -> ClaimIdentity {
        ClaimIdentity::new(who, "laptop", true)
    }

    fn claim(who: &str, ts: i64, nonce: &str) -> Claim {
        Claim {
            open_id: who.to_owned(),
            ts_ms: ts,
            nonce: nonce.to_owned(),
        }
    }

    #[test]
    fn encode_parse_bolak_balik_dan_menolak_bentuk_rusak() {
        let c = claim("ou_a", 1_700, "n1");
        assert_eq!(Claim::parse(&c.encode()), Some(c));
        for bad in ["", "ou_a", "ou_a|x|n", "ou_a|1|", "|1|n"] {
            assert_eq!(Claim::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn klaim_aktif_dev_lain_menolak_start_dan_ambil_alih() {
        let c = claim("ou_b", 0, "n");
        let now = STALE_MS - 1;
        assert_eq!(
            may_claim(Some(&c), &me("ou_a"), now, false),
            Err(ClaimRefused::HeldByOther("ou_b".to_owned()))
        );
        assert_eq!(
            may_claim(Some(&c), &me("ou_a"), now, true),
            Err(ClaimRefused::NotStale("ou_b".to_owned()))
        );
    }

    #[test]
    fn klaim_stale_hanya_bisa_diambil_alih_secara_eksplisit() {
        let c = claim("ou_b", 0, "n");
        let now = STALE_MS + 1;
        assert!(may_claim(Some(&c), &me("ou_a"), now, false).is_err());
        assert_eq!(may_claim(Some(&c), &me("ou_a"), now, true), Ok(()));
        assert_eq!(may_claim(None, &me("ou_a"), now, false), Ok(()));
        assert_eq!(
            may_claim(Some(&claim("ou_a", 0, "x")), &me("ou_a"), now, false),
            Ok(())
        );
    }

    #[test]
    fn hanya_pemilik_nonce_yang_menang() {
        let last = claim("ou_b", 5, "nb");
        assert!(won(Some(&last), "nb", "ou_b"));
        assert!(!won(Some(&last), "na", "ou_a"));
        assert!(!won(None, "na", "ou_a"));
    }

    #[test]
    fn klaim_membawa_id_daemon_dan_klaim_lama_tetap_terbaca() {
        let vm = ClaimIdentity::new("ou_a", "vm|bad~id", false);
        assert_eq!(vm.daemon_id, "vmbadid", "pemisah klaim dibuang");
        let c = claim("ou_a", 5, &vm.nonce("r1"));
        let parsed = Claim::parse(&c.encode()).unwrap();
        assert_eq!(parsed.daemon_id(), Some("vmbadid"));
        assert_eq!(claim("ou_a", 5, "lama").daemon_id(), None);
        assert_eq!(claim("ou_a", 5, "x~").daemon_id(), None);
    }

    #[test]
    fn klaim_daemon_lain_milik_sendiri_ditolak_sampai_basi() {
        let laptop = ClaimIdentity::new("ou_a", "laptop", true);
        let vm = ClaimIdentity::new("ou_a", "vm", false);
        let held_by_vm = claim("ou_a", 0, &vm.nonce("r"));
        let now = STALE_MS - 1;
        assert_eq!(
            may_claim(Some(&held_by_vm), &laptop, now, false),
            Err(ClaimRefused::OwnOtherDaemon("vm".to_owned()))
        );
        assert_eq!(may_claim(Some(&held_by_vm), &vm, now, false), Ok(()));
        assert_eq!(
            may_claim(Some(&held_by_vm), &laptop, STALE_MS + 1, false),
            Ok(()),
            "basi boleh diambil"
        );
        // Klaim lama tanpa id: milik laptop, belum basi ditolak untuk VM.
        let legacy = claim("ou_a", 0, "lama");
        assert_eq!(may_claim(Some(&legacy), &laptop, now, false), Ok(()));
        assert_eq!(
            may_claim(Some(&legacy), &vm, now, false),
            Err(ClaimRefused::OwnOtherDaemon("unknown".to_owned()))
        );
        assert_eq!(may_claim(Some(&legacy), &vm, STALE_MS + 1, false), Ok(()));
    }

    #[test]
    fn pemegang_klaim_dari_sudut_daemon() {
        let laptop = ClaimIdentity::new("ou_a", "laptop", true);
        let vm = ClaimIdentity::new("ou_a", "vm", false);
        let by_vm = claim("ou_a", 0, &vm.nonce("r"));
        assert_eq!(laptop.holder(&by_vm), Holder::OwnOtherDaemon);
        assert_eq!(vm.holder(&by_vm), Holder::ThisDaemon);
        assert!(vm.holds(&by_vm) && !laptop.holds(&by_vm));
        assert_eq!(laptop.holder(&claim("ou_b", 0, "x")), Holder::OtherPerson);
        assert_eq!(Holder::OwnOtherDaemon.as_str(), "own_other_daemon");
    }

    #[test]
    fn heartbeat_tiap_lima_menit() {
        let c = claim("ou_a", 0, "n");
        assert!(!heartbeat_due(&c, HEARTBEAT_MS - 1));
        assert!(heartbeat_due(&c, HEARTBEAT_MS));
        assert!(c.held_by("ou_a", STALE_MS));
        assert!(!c.held_by("ou_a", STALE_MS + 1));
    }
}
