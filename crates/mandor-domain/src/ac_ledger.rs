//! Buku AC per run (KTD-25): satu baris per acceptance criterion, dengan kolom
//! Design (diisi saat brainstorm/plan) dan Verdict (diisi reviewer).
//!
//! Baris dikunci oleh `record_id` AC di Lark. Teks AC disimpan agar perubahan di
//! tracker terdeteksi (KTD-27).

/// Nilai kolom Design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Design {
    NotChecked,
    InDesign,
    Partly,
    Missing,
    Conflicts,
    /// Dev menandai AC usang: agent mengikuti desain, baris keluar dari gerbang.
    FollowDesign,
}

impl Design {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotChecked => "not_checked",
            Self::InDesign => "in_design",
            Self::Partly => "partly",
            Self::Missing => "missing",
            Self::Conflicts => "conflicts",
            Self::FollowDesign => "follow_design",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "not_checked" => Self::NotChecked,
            "in_design" => Self::InDesign,
            "partly" => Self::Partly,
            "missing" => Self::Missing,
            "conflicts" => Self::Conflicts,
            "follow_design" => Self::FollowDesign,
            _ => return None,
        })
    }

    /// Selisih yang boleh ditandai "ikuti desain" saat menyetujui plan (R8a).
    #[must_use]
    pub fn is_gap(self) -> bool {
        matches!(self, Self::Partly | Self::Missing | Self::Conflicts)
    }

    fn label(self) -> &'static str {
        match self {
            Self::NotChecked => "Not checked",
            Self::InDesign => "In design",
            Self::Partly => "Partly in design",
            Self::Missing => "Missing from design",
            Self::Conflicts => "Conflicts with design",
            Self::FollowDesign => "Follow design",
        }
    }
}

/// Nilai kolom Verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Verdict {
    NotReviewed,
    Pass,
    Fail,
    CantVerify,
    CheckedManually,
    Skipped,
    /// Baris Fail yang dikirim ke agent; menunggu review berikutnya.
    FixInProgress,
}

impl Verdict {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotReviewed => "not_reviewed",
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::CantVerify => "cant_verify",
            Self::CheckedManually => "checked_manually",
            Self::Skipped => "skipped",
            Self::FixInProgress => "fix_in_progress",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "not_reviewed" => Self::NotReviewed,
            "pass" => Self::Pass,
            "fail" => Self::Fail,
            "cant_verify" => Self::CantVerify,
            "checked_manually" => Self::CheckedManually,
            "skipped" => Self::Skipped,
            "fix_in_progress" => Self::FixInProgress,
            _ => return None,
        })
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::NotReviewed => "Not reviewed",
            Self::Pass => "Pass",
            Self::Fail => "Fail",
            Self::CantVerify => "Can't verify",
            Self::CheckedManually => "Checked manually",
            Self::Skipped => "Skipped",
            Self::FixInProgress => "Fix in progress",
        }
    }
}

/// Status baris terhadap AC di tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RowStatus {
    Active,
    /// AC ditambahkan setelah buku AC dibuat.
    New,
    /// Teks AC berubah di tracker.
    Changed,
    /// AC di-drop atau dilepas dari task; keluar dari gerbang.
    Removed,
}

impl RowStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::New => "new",
            Self::Changed => "changed",
            Self::Removed => "removed",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "active" => Self::Active,
            "new" => Self::New,
            "changed" => Self::Changed,
            "removed" => Self::Removed,
            _ => return None,
        })
    }
}

/// Satu baris buku AC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcRow {
    /// `record_id` AC di Lark.
    pub ac_id: String,
    /// `AC-xxx`, bila ada.
    pub label: Option<String>,
    pub text: String,
    pub design: Design,
    /// Bagian desain yang ditemukan, atau catatan selisih.
    pub design_note: Option<String>,
    pub verdict: Verdict,
    /// Bukti reviewer (teks biasa; tidak pernah isi halaman).
    pub evidence: Option<String>,
    /// Kode berubah setelah verdict ini diberikan.
    pub stale: bool,
    pub status: RowStatus,
    /// Alasan Push anyway (Skipped).
    pub reason: Option<String>,
    /// Nama orang yang mengonfirmasi atau melewati baris ini.
    pub decided_by: Option<String>,
}

impl AcRow {
    /// Baris baru tanpa Design dan Verdict.
    #[must_use]
    pub fn new(ac_id: &str, label: Option<&str>, text: &str, status: RowStatus) -> Self {
        Self {
            ac_id: ac_id.to_owned(),
            label: label.map(str::to_owned),
            text: text.to_owned(),
            design: Design::NotChecked,
            design_note: None,
            verdict: Verdict::NotReviewed,
            evidence: None,
            stale: false,
            status,
            reason: None,
            decided_by: None,
        }
    }

    /// Label untuk teks keluaran: `AC-xxx`, atau `record_id` bila tidak ada.
    #[must_use]
    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.ac_id)
    }

    /// Baris ikut gerbang dan review: tidak Removed dan tidak mengikuti desain.
    #[must_use]
    pub fn in_gate(&self) -> bool {
        self.status != RowStatus::Removed && self.design != Design::FollowDesign
    }

    /// Boleh dipilih untuk Send to agent (R17).
    #[must_use]
    pub fn selectable(&self) -> bool {
        self.in_gate() && self.verdict == Verdict::Fail && !self.stale
    }
}

/// AC seperti yang dibaca dari tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchedAc {
    pub ac_id: String,
    pub label: Option<String>,
    pub text: String,
    pub dropped: bool,
}

/// Buku AC awal saat Start: semua AC aktif tanpa Design dan Verdict.
#[must_use]
pub fn initial_rows(fetched: &[FetchedAc]) -> Vec<AcRow> {
    fetched
        .iter()
        .filter(|a| !a.dropped)
        .map(|a| AcRow::new(&a.ac_id, a.label.as_deref(), &a.text, RowStatus::Active))
        .collect()
}

/// Menyelaraskan buku AC dengan AC terbaru di tracker (KTD-27).
///
/// AC baru menjadi New, teks berubah menjadi Changed (keduanya Not reviewed),
/// AC yang di-drop atau dilepas menjadi Removed. Urutan mengikuti tracker; baris
/// Removed di akhir.
#[must_use]
pub fn reconcile_rows(existing: &[AcRow], fetched: &[FetchedAc]) -> Vec<AcRow> {
    let mut out: Vec<AcRow> = Vec::with_capacity(existing.len().max(fetched.len()));
    for ac in fetched.iter().filter(|a| !a.dropped) {
        let row = match existing.iter().find(|r| r.ac_id == ac.ac_id) {
            None => AcRow::new(&ac.ac_id, ac.label.as_deref(), &ac.text, RowStatus::New),
            Some(old) if old.text != ac.text => {
                let mut r = old.clone();
                r.text.clone_from(&ac.text);
                r.status = RowStatus::Changed;
                r.verdict = Verdict::NotReviewed;
                r.evidence = None;
                r.reason = None;
                r.decided_by = None;
                r.stale = false;
                r
            }
            Some(old) => {
                let mut r = old.clone();
                if r.status == RowStatus::Removed {
                    r.status = RowStatus::Changed;
                    r.verdict = Verdict::NotReviewed;
                }
                r
            }
        };
        let mut row = row;
        row.label.clone_from(&ac.label);
        out.push(row);
    }
    for old in existing {
        if !out.iter().any(|r| r.ac_id == old.ac_id) {
            let mut r = old.clone();
            r.status = RowStatus::Removed;
            out.push(r);
        }
    }
    out
}

/// Menandai semua verdict usang karena kode berubah setelah review (R16).
pub fn mark_stale(rows: &mut [AcRow]) {
    for r in rows
        .iter_mut()
        .filter(|r| r.verdict != Verdict::NotReviewed)
    {
        r.stale = true;
    }
}

/// Alasan AC yang tidak dilaporkan reviewer (KTD-38).
pub const NOT_REPORTED: &str = "reviewer did not report";

/// Awal review: verdict baris di gerbang kembali Not reviewed. Baris Follow
/// design dan Removed tidak disentuh.
pub fn reset_for_review(rows: &mut [AcRow]) {
    for r in rows.iter_mut().filter(|r| r.in_gate()) {
        r.verdict = Verdict::NotReviewed;
        r.evidence = None;
        r.stale = false;
        r.reason = None;
        r.decided_by = None;
    }
}

/// Memulihkan keputusan dev dari review sebelumnya (R18 plan run grup): baris
/// yang dulu di-Push anyway (Skipped) dan kini tetap Fail, atau dulu
/// dikonfirmasi manual (Checked manually) dan kini tetap Can't verify, kembali
/// ke keputusan itu. Baris yang verdict-nya berubah tidak disentuh.
pub fn restore_decisions(rows: &mut [AcRow], previous: &[AcRow]) {
    for r in rows.iter_mut().filter(|r| r.in_gate() && !r.stale) {
        let Some(p) = previous.iter().find(|p| p.ac_id == r.ac_id) else {
            continue;
        };
        let restored = match (&p.verdict, &r.verdict) {
            (Verdict::Skipped, Verdict::Fail) => Verdict::Skipped,
            (Verdict::CheckedManually, Verdict::CantVerify) => Verdict::CheckedManually,
            _ => continue,
        };
        r.verdict = restored;
        r.reason.clone_from(&p.reason);
        r.decided_by.clone_from(&p.decided_by);
    }
}

/// Review selesai normal: baris di gerbang yang tidak dilaporkan menjadi
/// Can't verify dengan alasan [`NOT_REPORTED`].
pub fn fill_unreported(rows: &mut [AcRow]) {
    for r in rows
        .iter_mut()
        .filter(|r| r.in_gate() && r.verdict == Verdict::NotReviewed)
    {
        r.verdict = Verdict::CantVerify;
        r.evidence = Some(NOT_REPORTED.to_owned());
    }
}

/// Jumlah per status baris di gerbang.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReviewCounts {
    pub pass: u32,
    pub fail: u32,
    pub cant_verify: u32,
    pub not_reviewed: u32,
}

impl ReviewCounts {
    #[must_use]
    pub fn of(rows: &[AcRow]) -> Self {
        let mut c = Self::default();
        for r in rows.iter().filter(|r| r.in_gate()) {
            match r.verdict {
                Verdict::Pass | Verdict::CheckedManually => c.pass += 1,
                Verdict::Fail | Verdict::FixInProgress => c.fail += 1,
                Verdict::CantVerify => c.cant_verify += 1,
                Verdict::NotReviewed | Verdict::Skipped => c.not_reviewed += 1,
            }
        }
        c
    }

    /// Ringkasan untuk notifikasi, mis. "2 passed, 1 failed".
    #[must_use]
    pub fn summary(&self) -> String {
        let parts: Vec<String> = [
            (self.pass, "passed"),
            (self.fail, "failed"),
            (self.cant_verify, "can't verify"),
            (self.not_reviewed, "not reviewed"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, l)| format!("{n} {l}"))
        .collect();
        if parts.is_empty() {
            "no acceptance criteria".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

/// Teks selisih desain untuk field tracker `mandor_design_gaps` (KTD-34).
/// Baris dipotong di batas baris; sisanya diringkas "+N more in Mandor".
#[must_use]
pub fn design_gaps_text(rows: &[AcRow], limit: usize) -> String {
    let lines: Vec<String> = rows
        .iter()
        .filter(|r| r.status != RowStatus::Removed)
        .filter_map(|r| {
            let label = r.display_label();
            match r.design {
                Design::FollowDesign => {
                    Some(format!("{label}: Follow design: PM please update the AC"))
                }
                d if d.is_gap() => Some(match r.design_note.as_deref() {
                    Some(n) if !n.trim().is_empty() => {
                        format!("{label}: {}: {}", d.label(), n.trim())
                    }
                    _ => format!("{label}: {}", d.label()),
                }),
                _ => None,
            }
        })
        .collect();
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let rest = lines.len() - i;
        let tail = format!("\n+{rest} more in Mandor");
        let needed = usize::from(!out.is_empty()) + line.chars().count();
        let reserve = if rest > 1 { tail.chars().count() } else { 0 };
        if out.chars().count() + needed + reserve > limit {
            out.push_str(&tail);
            return out;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    #[test]
    fn keputusan_lama_dipulihkan_bila_verdict_tidak_berubah() {
        let mut skipped = AcRow::new("a", None, "A", RowStatus::Active);
        skipped.verdict = Verdict::Skipped;
        skipped.reason = Some("bug lama".to_owned());
        skipped.decided_by = Some("Beni".to_owned());
        let mut manual = AcRow::new("b", None, "B", RowStatus::Active);
        manual.verdict = Verdict::CheckedManually;
        manual.decided_by = Some("Beni".to_owned());
        let mut was_skipped_now_pass = skipped.clone();
        was_skipped_now_pass.ac_id = "c".to_owned();
        let previous = vec![skipped, manual, was_skipped_now_pass];

        let mut rows = vec![
            AcRow::new("a", None, "A", RowStatus::Active),
            AcRow::new("b", None, "B", RowStatus::Active),
            AcRow::new("c", None, "C", RowStatus::Active),
            AcRow::new("d", None, "D", RowStatus::Active),
        ];
        rows[0].verdict = Verdict::Fail;
        rows[1].verdict = Verdict::CantVerify;
        rows[2].verdict = Verdict::Pass;
        rows[3].verdict = Verdict::Fail;
        restore_decisions(&mut rows, &previous);
        assert_eq!(rows[0].verdict, Verdict::Skipped);
        assert_eq!(rows[0].reason.as_deref(), Some("bug lama"));
        assert_eq!(rows[1].verdict, Verdict::CheckedManually);
        assert_eq!(rows[1].decided_by.as_deref(), Some("Beni"));
        assert_eq!(
            rows[2].verdict,
            Verdict::Pass,
            "verdict berubah: tidak disentuh"
        );
        assert_eq!(
            rows[3].verdict,
            Verdict::Fail,
            "tanpa keputusan lama tetap menahan"
        );
    }

    #[test]
    fn review_mereset_lalu_melengkapi_ac_yang_tidak_dilaporkan() {
        let mut rows = vec![
            AcRow::new("a1", Some("AC-1"), "x", RowStatus::Active),
            AcRow::new("a2", Some("AC-2"), "y", RowStatus::Active),
            AcRow::new("a3", Some("AC-3"), "z", RowStatus::Active),
        ];
        rows[0].verdict = Verdict::Fail;
        rows[0].evidence = Some("lama".to_owned());
        rows[2].design = Design::FollowDesign;
        rows[2].verdict = Verdict::Pass;
        reset_for_review(&mut rows);
        assert_eq!(rows[0].verdict, Verdict::NotReviewed);
        assert_eq!(rows[0].evidence, None);
        assert_eq!(
            rows[2].verdict,
            Verdict::Pass,
            "follow design tidak disentuh"
        );
        rows[0].verdict = Verdict::Pass;
        fill_unreported(&mut rows);
        assert_eq!(rows[1].verdict, Verdict::CantVerify);
        assert_eq!(rows[1].evidence.as_deref(), Some(NOT_REPORTED));
        let c = ReviewCounts::of(&rows);
        assert_eq!((c.pass, c.cant_verify), (1, 1));
        assert_eq!(c.summary(), "1 passed, 1 can't verify");
        assert_eq!(ReviewCounts::default().summary(), "no acceptance criteria");
    }

    use super::*;

    fn fetched(id: &str, text: &str) -> FetchedAc {
        FetchedAc {
            ac_id: id.to_owned(),
            label: Some(format!("AC-{id}")),
            text: text.to_owned(),
            dropped: false,
        }
    }

    fn row(id: &str, verdict: Verdict) -> AcRow {
        let mut r = AcRow::new(id, Some(&format!("AC-{id}")), "teks", RowStatus::Active);
        r.verdict = verdict;
        r
    }

    #[test]
    fn nilai_design_verdict_dan_status_bolak_balik_lewat_string() {
        for d in [
            Design::NotChecked,
            Design::InDesign,
            Design::Partly,
            Design::Missing,
            Design::Conflicts,
            Design::FollowDesign,
        ] {
            assert_eq!(Design::parse(d.as_str()), Some(d));
        }
        for v in [
            Verdict::NotReviewed,
            Verdict::Pass,
            Verdict::Fail,
            Verdict::CantVerify,
            Verdict::CheckedManually,
            Verdict::Skipped,
            Verdict::FixInProgress,
        ] {
            assert_eq!(Verdict::parse(v.as_str()), Some(v));
        }
        for s in [
            RowStatus::Active,
            RowStatus::New,
            RowStatus::Changed,
            RowStatus::Removed,
        ] {
            assert_eq!(RowStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(Design::parse("x"), None);
        assert_eq!(Verdict::parse("x"), None);
        assert_eq!(RowStatus::parse("x"), None);
    }

    #[test]
    fn buku_awal_melewati_ac_yang_di_drop() {
        let mut dropped = fetched("2", "b");
        dropped.dropped = true;
        let rows = initial_rows(&[fetched("1", "a"), dropped]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, RowStatus::Active);
        assert_eq!(rows[0].design, Design::NotChecked);
        assert_eq!(rows[0].verdict, Verdict::NotReviewed);
    }

    #[test]
    fn ae7_ac_baru_setelah_semua_lulus_menjadi_new_dan_not_reviewed() {
        let existing = vec![row("1", Verdict::Pass)];
        let rows = reconcile_rows(&existing, &[fetched("1", "teks"), fetched("2", "baru")]);
        assert_eq!(rows[0].verdict, Verdict::Pass);
        assert_eq!(rows[0].status, RowStatus::Active);
        assert_eq!(rows[1].status, RowStatus::New);
        assert_eq!(rows[1].verdict, Verdict::NotReviewed);
    }

    #[test]
    fn teks_berubah_menjadi_changed_dan_ac_hilang_menjadi_removed() {
        let mut fail = row("1", Verdict::Fail);
        fail.evidence = Some("bukti".to_owned());
        let existing = vec![fail, row("2", Verdict::Pass), row("3", Verdict::Pass)];
        let mut dropped = fetched("3", "teks");
        dropped.dropped = true;
        let rows = reconcile_rows(&existing, &[fetched("1", "teks baru"), dropped]);
        assert_eq!(rows[0].status, RowStatus::Changed);
        assert_eq!(rows[0].verdict, Verdict::NotReviewed);
        assert_eq!(rows[0].evidence, None);
        assert_eq!(rows[0].text, "teks baru");
        let removed: Vec<&str> = rows
            .iter()
            .filter(|r| r.status == RowStatus::Removed)
            .map(|r| r.ac_id.as_str())
            .collect();
        assert_eq!(removed, ["2", "3"]);
        assert!(
            rows.iter()
                .filter(|r| r.status == RowStatus::Removed)
                .all(|r| !r.in_gate())
        );
    }

    #[test]
    fn ac_yang_kembali_setelah_dilepas_harus_direview_ulang() {
        let mut gone = row("1", Verdict::Pass);
        gone.status = RowStatus::Removed;
        let rows = reconcile_rows(&[gone], &[fetched("1", "teks")]);
        assert_eq!(rows[0].status, RowStatus::Changed);
        assert_eq!(rows[0].verdict, Verdict::NotReviewed);
    }

    #[test]
    fn ae10_follow_design_keluar_dari_gerbang_dan_tidak_bisa_dipilih() {
        let mut r = row("4", Verdict::Fail);
        r.design = Design::FollowDesign;
        assert!(!r.in_gate());
        assert!(!r.selectable());
        assert!(row("5", Verdict::Fail).selectable());
        assert!(!row("6", Verdict::Pass).selectable());
    }

    #[test]
    fn tanda_usang_hanya_untuk_baris_yang_sudah_punya_verdict() {
        let mut rows = vec![row("1", Verdict::Pass), row("2", Verdict::NotReviewed)];
        mark_stale(&mut rows);
        assert!(rows[0].stale);
        assert_eq!(rows[0].verdict, Verdict::Pass, "verdict tetap terlihat");
        assert!(!rows[1].stale);
        assert!(!rows[0].selectable());
    }

    #[test]
    fn selisih_desain_berlabel_dan_follow_design_meminta_update_ac() {
        let mut missing = row("3", Verdict::NotReviewed);
        missing.design = Design::Missing;
        missing.design_note = Some("The reject dialog has no reason field.".to_owned());
        let mut conflicts = row("5", Verdict::NotReviewed);
        conflicts.design = Design::Conflicts;
        let mut follow = row("7", Verdict::NotReviewed);
        follow.design = Design::FollowDesign;
        let mut ok = row("1", Verdict::NotReviewed);
        ok.design = Design::InDesign;
        let text = design_gaps_text(&[ok, missing, conflicts, follow], 4_000);
        assert_eq!(
            text,
            "AC-3: Missing from design: The reject dialog has no reason field.\n\
             AC-5: Conflicts with design\n\
             AC-7: Follow design: PM please update the AC"
        );
        assert_eq!(design_gaps_text(&[row("1", Verdict::Pass)], 4_000), "");
    }

    #[test]
    fn selisih_desain_dipotong_di_batas_baris_dengan_penutup() {
        let rows: Vec<AcRow> = (0..300)
            .map(|i| {
                let mut r = row(&i.to_string(), Verdict::NotReviewed);
                r.design = Design::Missing;
                r.design_note = Some("x".repeat(30));
                r
            })
            .collect();
        let text = design_gaps_text(&rows, 4_000);
        assert!(text.chars().count() <= 4_000, "{}", text.chars().count());
        let last = text.lines().last().unwrap_or_default();
        assert!(
            last.starts_with('+') && last.ends_with("more in Mandor"),
            "{last}"
        );
        let shown = text.lines().count() - 1;
        let more: usize = last
            .trim_start_matches('+')
            .split(' ')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        assert_eq!(shown + more, 300);
    }
}
