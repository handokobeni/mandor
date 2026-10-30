//! Jam lembur, siklus pemakaian, dan keputusan penjadwal VM (KTD-57, KTD-58
//! plan VM). Fungsi murni atas waktu epoch dan offset UTC tetap.

use std::fmt::Write as _;

const MIN_MS: i64 = 60_000;
const DAY_MIN: i64 = 24 * 60;

/// Kesalahan isian jam lembur.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WindowError {
    #[error("time must look like 19:00, got {0:?}")]
    Time(String),
    #[error("UTC offset must look like +07:00, got {0:?}")]
    Offset(String),
    #[error("the overnight window must not start and end at the same time")]
    Empty,
}

/// Jam lembur developer dalam zona waktunya.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Menit sejak tengah malam waktu lokal.
    start: i64,
    /// Menit sejak tengah malam waktu lokal.
    end: i64,
    /// Offset UTC dalam menit, mis. +420 untuk +07:00.
    offset: i64,
}

/// `HH:MM` menjadi menit sejak tengah malam.
fn parse_time(s: &str) -> Result<i64, WindowError> {
    let bad = || WindowError::Time(s.to_owned());
    let (h, m) = s.trim().split_once(':').ok_or_else(bad)?;
    let h: i64 = h.parse().map_err(|_| bad())?;
    let m: i64 = m.parse().map_err(|_| bad())?;
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return Err(bad());
    }
    Ok(h * 60 + m)
}

/// `+07:00` atau `-05:30` menjadi menit.
fn parse_offset(s: &str) -> Result<i64, WindowError> {
    let bad = || WindowError::Offset(s.to_owned());
    let t = s.trim();
    let (sign, rest) = match t.chars().next() {
        Some('+') => (1, &t[1..]),
        Some('-') => (-1, &t[1..]),
        _ => return Err(bad()),
    };
    let (h, m) = rest.split_once(':').ok_or_else(bad)?;
    let h: i64 = h.parse().map_err(|_| bad())?;
    let m: i64 = m.parse().map_err(|_| bad())?;
    if h > 14 || !(0..60).contains(&m) {
        return Err(bad());
    }
    Ok(sign * (h * 60 + m))
}

impl Window {
    /// Jam lembur dari isian developer.
    ///
    /// # Errors
    /// [`WindowError`] bila jam atau offset tidak valid, atau mulai sama dengan
    /// selesai.
    pub fn parse(start: &str, end: &str, offset: &str) -> Result<Self, WindowError> {
        let w = Self {
            start: parse_time(start)?,
            end: parse_time(end)?,
            offset: parse_offset(offset)?,
        };
        if w.start == w.end {
            return Err(WindowError::Empty);
        }
        Ok(w)
    }

    /// Menit lokal sejak epoch.
    fn local_min(&self, now_ms: i64) -> i64 {
        now_ms.div_euclid(MIN_MS) + self.offset
    }

    /// `true` bila `now_ms` di dalam jam lembur.
    #[must_use]
    pub fn is_open(&self, now_ms: i64) -> bool {
        let m = self.local_min(now_ms).rem_euclid(DAY_MIN);
        if self.start < self.end {
            (self.start..self.end).contains(&m)
        } else {
            m >= self.start || m < self.end
        }
    }

    /// Awal siklus yang memuat `now_ms` (ms epoch): jam mulai lembur terakhir
    /// pada atau sebelum `now_ms` (R10).
    #[must_use]
    pub fn cycle_start(&self, now_ms: i64) -> i64 {
        let local = self.local_min(now_ms);
        let mut start = local.div_euclid(DAY_MIN) * DAY_MIN + self.start;
        if start > local {
            start -= DAY_MIN;
        }
        (start - self.offset) * MIN_MS
    }
}

/// Ketersediaan VM untuk kartu baru.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Ada run VM yang sedang bekerja.
    Busy,
    /// Batas siklus belum diisi atau sudah habis, atau login Claude tidak valid.
    NotReady,
    /// Kosong dan siap.
    Free,
}

impl Window {
    /// Akhir jam lembur pada siklus yang dimulai `cycle_start_ms`.
    #[must_use]
    pub fn cycle_end(&self, cycle_start_ms: i64) -> i64 {
        let len = (self.end - self.start).rem_euclid(DAY_MIN);
        cycle_start_ms + len * MIN_MS
    }

    /// DM laporan siklus berjalan boleh dikirim: jam lembur sudah berakhir dan
    /// tidak ada run VM yang masih bekerja (R24).
    #[must_use]
    pub fn report_due(&self, now_ms: i64, running: bool) -> bool {
        !running && now_ms >= self.cycle_end(self.cycle_start(now_ms))
    }
}

/// Ringkasan satu siklus untuk DM laporan (R24). Hanya kunci kartu dan
/// jumlah; tidak pernah isi kode atau komentar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CycleReport {
    pub drafts: Vec<String>,
    pub held_ac_gate: Vec<String>,
    pub held_other: Vec<String>,
    pub dequeued: Vec<String>,
    pub failed: Vec<String>,
    /// Token terhitung siklus dan batasnya.
    pub used: u64,
    pub limit: Option<u64>,
}

/// Tautan ke tab Overnight di app.
pub const OVERNIGHT_LINK: &str = "mandor://overnight";

/// Teks DM laporan siklus.
#[must_use]
pub fn report_text(r: &CycleReport) -> String {
    let mut out = String::from("Mandor overnight report\n");
    let ran = r.drafts.len() + r.held_ac_gate.len() + r.held_other.len() + r.failed.len();
    if ran == 0 && r.dequeued.is_empty() {
        out.push_str("No cards ran this cycle.\n");
    } else {
        for (label, cards) in [
            ("Draft PRs ready", &r.drafts),
            ("Parked (AC gate)", &r.held_ac_gate),
            ("Parked (other)", &r.held_other),
            ("Removed from queue", &r.dequeued),
            ("Failed", &r.failed),
        ] {
            let _ = writeln!(out, "{label}: {}", cards.len());
        }
        let decide: Vec<&str> = r
            .drafts
            .iter()
            .chain(&r.held_ac_gate)
            .chain(&r.held_other)
            .map(String::as_str)
            .collect();
        if !decide.is_empty() {
            let _ = writeln!(out, "Needs your decision: {}", decide.join(", "));
        }
    }
    match r.limit {
        Some(l) => {
            let _ = writeln!(out, "Usage: {} of {l} tokens", r.used);
        }
        None => {
            let _ = writeln!(out, "Usage: {} tokens", r.used);
        }
    }
    let _ = write!(out, "Open: {OVERNIGHT_LINK}");
    out
}

/// Bahan keputusan penjadwal pada satu tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickState<'a> {
    pub slot: Slot,
    /// Ada perbaikan Waiting Fix yang siap dimulai.
    pub fix_ready: bool,
    /// Kartu antrean yang diminta "Start now".
    pub start_now: Option<&'a str>,
    /// Sekarang di dalam jam lembur.
    pub in_window: bool,
    /// Kartu teratas antrean.
    pub queue_head: Option<&'a str>,
}

/// Keputusan penjadwal (tabel "boleh mulai" plan VM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next<'a> {
    /// Ada run berjalan, atau tidak ada yang perlu dimulai.
    Wait,
    /// Batas atau login belum siap: tidak ada yang dimulai.
    NotReady,
    /// Waiting Fix mendapat giliran (dijalankan mesin Waiting Fix).
    Fix,
    /// Mulai kartu antrean ini.
    Start(&'a str),
}

/// Keputusan "boleh mulai kartu berikutnya?" (R7, R8, R10, R21).
#[must_use]
pub fn decide<'a>(s: &TickState<'a>) -> Next<'a> {
    match s.slot {
        Slot::Busy => return Next::Wait,
        Slot::NotReady => return Next::NotReady,
        Slot::Free => {}
    }
    if s.fix_ready {
        return Next::Fix;
    }
    if let Some(id) = s.start_now {
        return Next::Start(id);
    }
    match s.queue_head {
        Some(id) if s.in_window => Next::Start(id),
        _ => Next::Wait,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ms epoch untuk tanggal 2026-10-23 pukul `h:m` waktu +07:00.
    fn wib(day: i64, h: i64, m: i64) -> i64 {
        // 2026-10-23T00:00:00Z
        let base = 1_792_713_600_000;
        base + day * DAY_MIN * MIN_MS + ((h * 60 + m) - 7 * 60) * MIN_MS
    }

    #[test]
    fn jendela_lewat_tengah_malam() {
        let w = Window::parse("19:00", "06:00", "+07:00").unwrap();
        assert!(!w.is_open(wib(0, 18, 59)));
        assert!(w.is_open(wib(0, 19, 0)));
        assert!(w.is_open(wib(1, 5, 59)));
        assert!(!w.is_open(wib(1, 6, 0)));
        assert!(!w.is_open(wib(1, 14, 0)));
    }

    #[test]
    fn jendela_tanpa_lewat_tengah_malam_dan_offset_negatif() {
        let w = Window::parse("01:00", "05:00", "-05:30").unwrap();
        let utc = |h: i64, m: i64| 1_792_713_600_000 + (h * 60 + m) * MIN_MS;
        // 01:00 -05:30 = 06:30 UTC.
        assert!(!w.is_open(utc(6, 29)));
        assert!(w.is_open(utc(6, 30)));
        assert!(w.is_open(utc(10, 29)));
        assert!(!w.is_open(utc(10, 30)));
    }

    #[test]
    fn isian_tidak_valid_ditolak() {
        assert_eq!(
            Window::parse("19:00", "19:00", "+07:00"),
            Err(WindowError::Empty)
        );
        assert!(matches!(
            Window::parse("24:00", "06:00", "+07:00"),
            Err(WindowError::Time(_))
        ));
        assert!(matches!(
            Window::parse("19:00", "06:00", "07:00"),
            Err(WindowError::Offset(_))
        ));
    }

    #[test]
    fn siklus_mulai_saat_jam_lembur_mulai() {
        let w = Window::parse("19:00", "06:00", "+07:00").unwrap();
        assert_eq!(w.cycle_start(wib(0, 19, 0)), wib(0, 19, 0));
        assert_eq!(w.cycle_start(wib(1, 2, 0)), wib(0, 19, 0));
        assert_eq!(
            w.cycle_start(wib(1, 14, 0)),
            wib(0, 19, 0),
            "run siang masuk siklus yang dimulai kemarin malam"
        );
        assert_eq!(w.cycle_start(wib(1, 18, 59)), wib(0, 19, 0));
        assert_eq!(w.cycle_start(wib(1, 19, 0)), wib(1, 19, 0));
    }

    #[test]
    fn laporan_jatuh_tempo_setelah_jam_lembur_dan_run_selesai() {
        let w = Window::parse("19:00", "06:00", "+07:00").unwrap();
        assert_eq!(w.cycle_end(wib(0, 19, 0)), wib(1, 6, 0));
        assert!(!w.report_due(wib(1, 5, 59), false), "masih jam lembur");
        assert!(
            !w.report_due(wib(1, 6, 0), true),
            "kartu terakhir masih jalan"
        );
        assert!(w.report_due(wib(1, 6, 20), false));
        assert!(w.report_due(wib(1, 18, 59), false));
        assert!(!w.report_due(wib(1, 19, 0), false), "siklus baru dimulai");
    }

    #[test]
    fn teks_laporan_tanpa_isi_kode() {
        let r = CycleReport {
            drafts: vec!["PGS-1".to_owned()],
            held_ac_gate: vec!["PGS-2".to_owned()],
            held_other: vec![],
            dequeued: vec!["PGS-3".to_owned()],
            failed: vec![],
            used: 1200,
            limit: Some(5000),
        };
        let t = report_text(&r);
        assert!(t.contains("Draft PRs ready: 1"));
        assert!(t.contains("Parked (AC gate): 1"));
        assert!(t.contains("Removed from queue: 1"));
        assert!(t.contains("Needs your decision: PGS-1, PGS-2"));
        assert!(t.contains("Usage: 1200 of 5000 tokens"));
        assert!(t.ends_with("Open: mandor://overnight"));
        let empty = report_text(&CycleReport::default());
        assert!(empty.contains("No cards ran this cycle."));
    }

    fn state<'a>() -> TickState<'a> {
        TickState {
            slot: Slot::Free,
            fix_ready: false,
            start_now: None,
            in_window: true,
            queue_head: Some("a"),
        }
    }

    #[test]
    fn tabel_keputusan_boleh_mulai() {
        assert_eq!(
            decide(&TickState {
                slot: Slot::Busy,
                ..state()
            }),
            Next::Wait
        );
        assert_eq!(
            decide(&TickState {
                slot: Slot::NotReady,
                ..state()
            }),
            Next::NotReady
        );
        assert_eq!(
            decide(&TickState {
                slot: Slot::NotReady,
                start_now: Some("b"),
                ..state()
            }),
            Next::NotReady,
            "Start now juga butuh batas"
        );
        assert_eq!(
            decide(&TickState {
                fix_ready: true,
                start_now: Some("b"),
                ..state()
            }),
            Next::Fix
        );
        assert_eq!(
            decide(&TickState {
                start_now: Some("b"),
                in_window: false,
                ..state()
            }),
            Next::Start("b")
        );
        assert_eq!(decide(&state()), Next::Start("a"));
        assert_eq!(
            decide(&TickState {
                in_window: false,
                ..state()
            }),
            Next::Wait
        );
        assert_eq!(
            decide(&TickState {
                queue_head: None,
                ..state()
            }),
            Next::Wait
        );
    }
}
