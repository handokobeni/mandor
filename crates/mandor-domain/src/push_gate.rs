//! Gerbang Push & PR (R19–R22, KTD-35) dan ringkasan per AC untuk deskripsi PR
//! (R31, R33). Fungsi murni atas baris buku AC.

use crate::ac_ledger::{AcRow, Design, RowStatus, Verdict};

/// Ketatnya gerbang per workspace atau repo (R19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GateMode {
    #[default]
    Block,
    Warn,
}

impl GateMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Warn => "warn",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "block" => Some(Self::Block),
            "warn" => Some(Self::Warn),
            _ => None,
        }
    }

    /// Mode paling ketat di antara anggota run (KTD-37).
    #[must_use]
    pub fn strictest(modes: impl IntoIterator<Item = Self>) -> Self {
        if modes.into_iter().any(|m| m == Self::Block) {
            Self::Block
        } else {
            Self::Warn
        }
    }
}

/// Keputusan dev saat menekan Push & PR.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GateDecision {
    /// `ac_id` baris Can't verify yang dikonfirmasi "checked manually".
    pub confirmed: Vec<String>,
    /// Alasan Push anyway untuk semua baris Fail (mode blokir).
    pub push_anyway_reason: Option<String>,
    /// Nama dev yang memutuskan (dicatat di buku AC dan PR).
    pub decided_by: Option<String>,
}

/// Hasil evaluasi gerbang.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    /// Boleh push. `warnings`: label AC Fail atau Can't verify yang dibiarkan
    /// (mode peringatan).
    Ready { warnings: Vec<String> },
    /// Ada baris tanpa review yang berlaku: review harus berjalan dulu.
    NeedsReview,
    /// Mode blokir dengan AC gagal tanpa alasan Push anyway.
    Blocked { failed: Vec<String> },
    /// Mode blokir dengan AC Can't verify yang belum dikonfirmasi.
    NeedsConfirmation { unverified: Vec<String> },
}

/// Menerapkan keputusan dev ke baris: Fail menjadi Skipped dengan alasan, Can't
/// verify yang dikonfirmasi menjadi Checked manually.
pub fn apply_decision(rows: &mut [AcRow], decision: &GateDecision) {
    let reason = decision
        .push_anyway_reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty());
    for r in rows.iter_mut().filter(|r| r.in_gate() && !r.stale) {
        match r.verdict {
            Verdict::Fail => {
                if let Some(reason) = reason {
                    r.verdict = Verdict::Skipped;
                    r.reason = Some(reason.to_owned());
                    r.decided_by.clone_from(&decision.decided_by);
                }
            }
            Verdict::CantVerify if decision.confirmed.contains(&r.ac_id) => {
                r.verdict = Verdict::CheckedManually;
                r.decided_by.clone_from(&decision.decided_by);
            }
            _ => {}
        }
    }
}

/// Mengevaluasi gerbang atas baris setelah keputusan dev diterapkan.
#[must_use]
pub fn evaluate(rows: &[AcRow], mode: GateMode, decision: &GateDecision) -> GateOutcome {
    let mut rows = rows.to_vec();
    apply_decision(&mut rows, decision);
    let active: Vec<&AcRow> = rows.iter().filter(|r| r.in_gate()).collect();
    if active
        .iter()
        .any(|r| r.stale || matches!(r.verdict, Verdict::NotReviewed | Verdict::FixInProgress))
    {
        return GateOutcome::NeedsReview;
    }
    let labels = |v: Verdict| -> Vec<String> {
        active
            .iter()
            .filter(|r| r.verdict == v)
            .map(|r| r.display_label().to_owned())
            .collect()
    };
    let failed = labels(Verdict::Fail);
    let unverified = labels(Verdict::CantVerify);
    match mode {
        GateMode::Warn => GateOutcome::Ready {
            warnings: failed.into_iter().chain(unverified).collect(),
        },
        GateMode::Block if !failed.is_empty() => GateOutcome::Blocked { failed },
        GateMode::Block if !unverified.is_empty() => GateOutcome::NeedsConfirmation { unverified },
        GateMode::Block => GateOutcome::Ready {
            warnings: Vec::new(),
        },
    }
}

/// Ringkasan per AC untuk deskripsi PR. Hanya label, status, alasan, dan nama
/// pemutus; bukti dan isi halaman tidak pernah ikut (R33).
#[must_use]
pub fn pr_summary(rows: &[AcRow], ac_synced_at: Option<&str>) -> String {
    let mut out = String::from("## Acceptance criteria review\n\n");
    let shown: Vec<&AcRow> = rows
        .iter()
        .filter(|r| r.status != RowStatus::Removed)
        .collect();
    if shown.is_empty() {
        out.push_str("No acceptance criteria.\n");
    } else {
        for r in shown {
            out.push_str("- ");
            out.push_str(r.display_label());
            out.push_str(": ");
            out.push_str(&row_result(r));
            out.push('\n');
        }
        out.push_str("\nVerdicts are the AI reviewer's findings, not a sign-off.\n");
    }
    if let Some(at) = ac_synced_at {
        out.push_str("\nAC last synced at ");
        out.push_str(at);
        out.push_str(".\n");
    }
    out
}

fn row_result(r: &AcRow) -> String {
    if r.design == Design::FollowDesign {
        return "Follows design (the PM was asked to update this AC)".to_owned();
    }
    let by = r
        .decided_by
        .as_deref()
        .map(|n| format!(" by {n}"))
        .unwrap_or_default();
    let stale = if r.stale { " (stale)" } else { "" };
    match r.verdict {
        Verdict::Skipped => format!(
            "Skipped{by}: {}",
            r.reason.as_deref().unwrap_or("no reason given")
        ),
        Verdict::CheckedManually => format!("Checked manually{by}"),
        v => format!("{}{stale}", v.label()),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn row(label: &str, verdict: Verdict) -> AcRow {
        let mut r = AcRow::new(
            &format!("rec-{label}"),
            Some(label),
            "teks",
            RowStatus::Active,
        );
        r.verdict = verdict;
        r
    }

    fn ids(rows: &[AcRow], label: &str) -> Vec<String> {
        rows.iter()
            .filter(|r| r.display_label() == label)
            .map(|r| r.ac_id.clone())
            .collect()
    }

    #[test]
    fn ae1_blokir_lalu_konfirmasi_lalu_siap() {
        let mut rows = vec![
            row("AC-1", Verdict::Pass),
            row("AC-2", Verdict::CantVerify),
            row("AC-3", Verdict::Fail),
        ];
        let none = GateDecision::default();
        assert_eq!(
            evaluate(&rows, GateMode::Block, &none),
            GateOutcome::Blocked {
                failed: vec!["AC-3".to_owned()]
            }
        );
        rows[2].verdict = Verdict::Pass;
        assert_eq!(
            evaluate(&rows, GateMode::Block, &none),
            GateOutcome::NeedsConfirmation {
                unverified: vec!["AC-2".to_owned()]
            }
        );
        let confirm = GateDecision {
            confirmed: ids(&rows, "AC-2"),
            push_anyway_reason: None,
            decided_by: Some("Beni Handoko".to_owned()),
        };
        assert_eq!(
            evaluate(&rows, GateMode::Block, &confirm),
            GateOutcome::Ready { warnings: vec![] }
        );
        apply_decision(&mut rows, &confirm);
        assert!(pr_summary(&rows, None).contains("- AC-2: Checked manually by Beni Handoko"));
    }

    #[test]
    fn alasan_push_anyway_dan_konfirmasi_dalam_satu_keputusan() {
        let rows = vec![row("AC-2", Verdict::CantVerify), row("AC-3", Verdict::Fail)];
        let decision = GateDecision {
            confirmed: ids(&rows, "AC-2"),
            push_anyway_reason: Some("API dependency down".to_owned()),
            decided_by: None,
        };
        assert_eq!(
            evaluate(&rows, GateMode::Block, &decision),
            GateOutcome::Ready { warnings: vec![] }
        );
    }

    #[test]
    fn ae2_mode_peringatan_tidak_butuh_alasan_maupun_konfirmasi() {
        let rows = vec![row("AC-2", Verdict::CantVerify), row("AC-3", Verdict::Fail)];
        assert_eq!(
            evaluate(&rows, GateMode::Warn, &GateDecision::default()),
            GateOutcome::Ready {
                warnings: vec!["AC-3".to_owned(), "AC-2".to_owned()]
            }
        );
    }

    #[test]
    fn ae3_push_anyway_menjadikan_fail_skipped_dengan_alasan() {
        let mut rows = vec![row("AC-3", Verdict::Fail)];
        let decision = GateDecision {
            confirmed: vec![],
            push_anyway_reason: Some("  API dependency down ".to_owned()),
            decided_by: None,
        };
        assert_eq!(
            evaluate(&rows, GateMode::Block, &decision),
            GateOutcome::Ready { warnings: vec![] }
        );
        apply_decision(&mut rows, &decision);
        assert_eq!(rows[0].verdict, Verdict::Skipped);
        assert!(pr_summary(&rows, None).contains("- AC-3: Skipped: API dependency down"));
    }

    #[test]
    fn alasan_kosong_tidak_melewati_blokir() {
        let rows = vec![row("AC-3", Verdict::Fail)];
        let decision = GateDecision {
            push_anyway_reason: Some("   ".to_owned()),
            ..GateDecision::default()
        };
        assert!(matches!(
            evaluate(&rows, GateMode::Block, &decision),
            GateOutcome::Blocked { .. }
        ));
    }

    #[test]
    fn baris_tanpa_review_berlaku_meminta_review() {
        for v in [Verdict::NotReviewed, Verdict::FixInProgress] {
            let rows = vec![row("AC-1", Verdict::Pass), row("AC-2", v)];
            assert_eq!(
                evaluate(&rows, GateMode::Warn, &GateDecision::default()),
                GateOutcome::NeedsReview,
                "{v:?}"
            );
        }
        let mut stale = row("AC-1", Verdict::Pass);
        stale.stale = true;
        assert_eq!(
            evaluate(&[stale], GateMode::Block, &GateDecision::default()),
            GateOutcome::NeedsReview
        );
    }

    #[test]
    fn ae10_follow_design_dan_removed_diabaikan_gerbang() {
        let mut follow = row("AC-4", Verdict::NotReviewed);
        follow.design = Design::FollowDesign;
        let mut removed = row("AC-8", Verdict::Fail);
        removed.status = RowStatus::Removed;
        let rows = vec![row("AC-1", Verdict::Pass), follow, removed];
        assert_eq!(
            evaluate(&rows, GateMode::Block, &GateDecision::default()),
            GateOutcome::Ready { warnings: vec![] }
        );
        let summary = pr_summary(&rows, None);
        assert!(summary.contains("- AC-4: Follows design"), "{summary}");
        assert!(!summary.contains("AC-8"), "{summary}");
    }

    #[test]
    fn tanpa_ac_aktif_siap_dan_ringkasan_menyebutnya() {
        assert_eq!(
            evaluate(&[], GateMode::Block, &GateDecision::default()),
            GateOutcome::Ready { warnings: vec![] }
        );
        assert!(pr_summary(&[], None).contains("No acceptance criteria."));
    }

    #[test]
    fn ringkasan_mencatat_waktu_sinkron_dan_tanpa_bukti() {
        let mut r = row("AC-1", Verdict::Pass);
        r.evidence = Some("Opened /po as approver; page showed salary 12.000.000".to_owned());
        let summary = pr_summary(&[r], Some("2026-10-22 10:31"));
        assert!(
            summary.contains("AC last synced at 2026-10-22 10:31."),
            "{summary}"
        );
        assert!(summary.contains("- AC-1: Pass"));
        assert!(!summary.contains("12.000.000"), "bukti tidak ikut ke PR");
        assert!(summary.contains("not a sign-off"));
    }

    #[test]
    fn mode_paling_ketat_dan_string() {
        assert_eq!(
            GateMode::strictest([GateMode::Warn, GateMode::Block]),
            GateMode::Block
        );
        assert_eq!(GateMode::strictest([GateMode::Warn]), GateMode::Warn);
        assert_eq!(GateMode::strictest([]), GateMode::Warn);
        assert_eq!(
            GateMode::parse(GateMode::Warn.as_str()),
            Some(GateMode::Warn)
        );
        assert_eq!(GateMode::parse("x"), None);
        assert_eq!(GateMode::default(), GateMode::Block);
    }
}
