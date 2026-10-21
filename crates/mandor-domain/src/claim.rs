//! Klaim pemilik record tanpa compare-and-set (KTD-8).
//!
//! Klaim ditulis ke field Lark sebagai teks `open_id|ts_ms|nonce`.

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
}

/// Keputusan sebelum menulis klaim, dari baca segar.
///
/// Klaim milik sendiri selalu boleh ditulis ulang. Klaim aktif milik dev lain
/// menolak Start; klaim yang sudah stale hanya boleh diambil lewat `takeover`.
///
/// # Errors
/// [`ClaimRefused`] bila klaim aktif milik dev lain.
pub fn may_claim(
    current: Option<&Claim>,
    me: &str,
    now_ms: i64,
    takeover: bool,
) -> Result<(), ClaimRefused> {
    let Some(c) = current else { return Ok(()) };
    if c.open_id == me {
        return Ok(());
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
            may_claim(Some(&c), "ou_a", now, false),
            Err(ClaimRefused::HeldByOther("ou_b".to_owned()))
        );
        assert_eq!(
            may_claim(Some(&c), "ou_a", now, true),
            Err(ClaimRefused::NotStale("ou_b".to_owned()))
        );
    }

    #[test]
    fn klaim_stale_hanya_bisa_diambil_alih_secara_eksplisit() {
        let c = claim("ou_b", 0, "n");
        let now = STALE_MS + 1;
        assert!(may_claim(Some(&c), "ou_a", now, false).is_err());
        assert_eq!(may_claim(Some(&c), "ou_a", now, true), Ok(()));
        assert_eq!(may_claim(None, "ou_a", now, false), Ok(()));
        assert_eq!(
            may_claim(Some(&claim("ou_a", 0, "x")), "ou_a", now, false),
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
    fn heartbeat_tiap_lima_menit() {
        let c = claim("ou_a", 0, "n");
        assert!(!heartbeat_due(&c, HEARTBEAT_MS - 1));
        assert!(heartbeat_due(&c, HEARTBEAT_MS));
        assert!(c.held_by("ou_a", STALE_MS));
        assert!(!c.held_by("ou_a", STALE_MS + 1));
    }
}
