//! Kartu Planned di antrean VM: plan disetujui siang di laptop, dikerjakan VM
//! malam hari (plan 2026-10-30-001, KTD-2, KTD-4, KTD-5).

use crate::ac_ledger::Design;
use crate::claim::{ClaimIdentity, Holder, STALE_MS};
use crate::kanban::TaskStatus;
use crate::task::Task;

/// Status Design satu AC hasil tahap plan di laptop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignMark {
    /// `record_id` AC di Lark.
    pub ac_id: String,
    pub design: Design,
    pub note: Option<String>,
}

/// Plan yang disetujui di laptop dan dibawa ke VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWork {
    pub plan: String,
    /// Branch kartu yang sudah di-push laptop.
    pub branch: String,
    pub design: Vec<DesignMark>,
    /// Sumber desain laptop saat plan disusun; dipulihkan saat Take over.
    pub laptop_design: Vec<String>,
}

impl PlannedWork {
    /// `true` bila ada AC yang ditandai Follow design (R11).
    #[must_use]
    pub fn follows_design(&self) -> bool {
        self.design.iter().any(|d| d.design == Design::FollowDesign)
    }
}

/// Status item Planned di antrean VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedState {
    /// Diantre; klaim belum pindah dari laptop asal.
    HandoverPending,
    /// Klaim milik VM; menunggu jam lembur.
    Waiting,
    /// Jalur mulai sedang berjalan.
    Starting,
    /// Dikeluarkan tanpa dikerjakan, plan dan klaim tetap di VM (R15).
    Held,
}

impl PlannedState {
    /// Nama stabil untuk penyimpanan dan protokol.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HandoverPending => "handover_pending",
            Self::Waiting => "waiting",
            Self::Starting => "starting",
            Self::Held => "held",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "handover_pending" => Self::HandoverPending,
            "waiting" => Self::Waiting,
            "starting" => Self::Starting,
            "held" => Self::Held,
            _ => return None,
        })
    }
}

/// Folder tempat skill plan dan brainstorm menulis dokumennya.
const PLAN_DIRS: [&str; 2] = ["docs/plans/", "docs/brainstorms/"];

/// Berkas baru yang tidak boleh ikut ter-push saat kartu dikirim ke VM
/// (KTD-10): semua yang di luar folder plan dan brainstorm.
#[must_use]
pub fn stray_files(untracked: &[String]) -> Vec<String> {
    untracked
        .iter()
        .filter(|p| !PLAN_DIRS.iter().any(|d| p.starts_with(d)) || p.contains(".."))
        .cloned()
        .collect()
}

/// Alasan klaim masih di daemon laptop asal.
pub const CLAIM_ON_LAPTOP: &str = "the claim is still on the laptop";

/// Alasan bersama untuk kartu yang tidak lagi milik dev atau keluar dari In
/// Progress; `None` bila kartu masih layak.
fn card_gone(task: &Task, me: &ClaimIdentity) -> Option<String> {
    if !task.assigned_to(&me.open_id) {
        return Some("the card is no longer assigned to you".to_owned());
    }
    if task.status != TaskStatus::InProgress {
        return Some(format!(
            "the card moved to {} in Lark",
            task.status.as_str()
        ));
    }
    None
}

/// Syarat mulai kartu Planned di VM (KTD-4). `Err` berisi alasan untuk
/// laporan pagi.
///
/// # Errors
/// Kartu tidak lagi ditugaskan ke dev, bukan In Progress, atau klaimnya tidak
/// dipegang daemon ini.
pub fn check_planned_start(task: &Task, me: &ClaimIdentity) -> Result<(), String> {
    if let Some(reason) = card_gone(task, me) {
        return Err(reason);
    }
    match task.holder(me) {
        Some(Holder::ThisDaemon) => Ok(()),
        Some(Holder::OwnOtherDaemon) => Err(CLAIM_ON_LAPTOP.to_owned()),
        Some(Holder::OtherPerson) => Err("the card is held by someone else".to_owned()),
        None => Err("the card has no claim".to_owned()),
    }
}

/// Tindakan heartbeat untuk satu item Planned (KTD-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimAction {
    /// Serah-terima belum selesai; jangan sentuh klaim.
    Wait,
    /// Klaim sudah milik VM: item menjadi `Waiting` dan klaim diperbarui.
    Promote,
    /// Perbarui klaim milik VM.
    Renew,
    /// Tahan item (R15) dengan alasan; klaim tidak ditulis.
    Hold(String),
    /// Keluarkan item dengan alasan; klaim tidak ditulis.
    Drop(String),
    /// Jalur mulai yang memegang item; jangan sentuh.
    Leave,
}

/// Keputusan heartbeat item Planned dari baca segar kartu. Klaim tidak pernah
/// ditulis bila pemegangnya bukan daemon ini.
#[must_use]
pub fn planned_claim_action(
    state: PlannedState,
    origin_daemon: Option<&str>,
    added_at_ms: i64,
    task: &Task,
    me: &ClaimIdentity,
    now_ms: i64,
) -> ClaimAction {
    if state == PlannedState::Starting {
        return ClaimAction::Leave;
    }
    if let Some(reason) = card_gone(task, me) {
        return ClaimAction::Drop(reason);
    }
    let Some(claim) = &task.claim else {
        return ClaimAction::Drop("the card has no claim".to_owned());
    };
    let holder = me.holder(claim);
    if holder == Holder::ThisDaemon {
        return if state == PlannedState::HandoverPending {
            ClaimAction::Promote
        } else {
            ClaimAction::Renew
        };
    }
    let from_origin = holder == Holder::OwnOtherDaemon
        && origin_daemon.is_some_and(|o| claim.daemon_id() == Some(o));
    if state == PlannedState::HandoverPending && from_origin {
        return if now_ms - added_at_ms > STALE_MS {
            ClaimAction::Hold(CLAIM_ON_LAPTOP.to_owned())
        } else {
            ClaimAction::Wait
        };
    }
    ClaimAction::Drop("the claim was taken by another Mandor daemon".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::Claim;

    const ME: &str = "ou_dev";

    fn vm() -> ClaimIdentity {
        ClaimIdentity::new(ME, "vm", false)
    }

    fn card(status: TaskStatus, claim_daemon: Option<&str>) -> Task {
        let mut t = Task::new("rec1", status);
        t.assignees = vec![ME.to_owned()];
        t.claim = claim_daemon.map(|d| Claim {
            open_id: ME.to_owned(),
            ts_ms: 1_000,
            nonce: format!("n~{d}"),
        });
        t
    }

    #[test]
    fn mulai_hanya_bila_in_progress_milik_dev_dan_klaim_di_vm() {
        assert_eq!(
            check_planned_start(&card(TaskStatus::InProgress, Some("vm")), &vm()),
            Ok(())
        );
        let cases = [
            (
                card(TaskStatus::ToDo, Some("vm")),
                "the card moved to To Do in Lark",
            ),
            (
                card(TaskStatus::WaitingReview, Some("vm")),
                "the card moved to Waiting Review in Lark",
            ),
            (
                card(TaskStatus::InProgress, Some("laptop")),
                CLAIM_ON_LAPTOP,
            ),
            (card(TaskStatus::InProgress, None), "the card has no claim"),
        ];
        for (task, want) in cases {
            assert_eq!(check_planned_start(&task, &vm()), Err(want.to_owned()));
        }
        let mut other = card(TaskStatus::InProgress, Some("vm"));
        other.assignees = vec!["ou_lain".to_owned()];
        assert_eq!(
            check_planned_start(&other, &vm()),
            Err("the card is no longer assigned to you".to_owned())
        );
    }

    #[test]
    fn serah_terima_tertunda_menunggu_lalu_ditahan_setelah_basi() {
        let task = card(TaskStatus::InProgress, Some("laptop"));
        let act = |now| {
            planned_claim_action(
                PlannedState::HandoverPending,
                Some("laptop"),
                0,
                &task,
                &vm(),
                now,
            )
        };
        assert_eq!(act(STALE_MS - 1), ClaimAction::Wait);
        assert_eq!(
            act(STALE_MS + 1),
            ClaimAction::Hold(CLAIM_ON_LAPTOP.to_owned())
        );
    }

    #[test]
    fn klaim_milik_vm_dipromosikan_atau_diperbarui() {
        let task = card(TaskStatus::InProgress, Some("vm"));
        let act = |state| planned_claim_action(state, Some("laptop"), 0, &task, &vm(), 5);
        assert_eq!(act(PlannedState::HandoverPending), ClaimAction::Promote);
        assert_eq!(act(PlannedState::Waiting), ClaimAction::Renew);
        assert_eq!(act(PlannedState::Held), ClaimAction::Renew);
        assert_eq!(act(PlannedState::Starting), ClaimAction::Leave);
    }

    #[test]
    fn klaim_daemon_lain_tidak_pernah_ditimpa() {
        for (state, holder) in [
            (PlannedState::Waiting, "laptop"),
            (PlannedState::Held, "laptop2"),
            (PlannedState::HandoverPending, "laptop2"),
        ] {
            let task = card(TaskStatus::InProgress, Some(holder));
            let got = planned_claim_action(state, Some("laptop"), 0, &task, &vm(), 5);
            assert!(
                matches!(got, ClaimAction::Drop(_)),
                "{state:?} dengan klaim {holder}: {got:?}"
            );
        }
    }

    #[test]
    fn kartu_yang_pindah_status_dikeluarkan() {
        let task = card(TaskStatus::WaitingReview, Some("vm"));
        assert_eq!(
            planned_claim_action(PlannedState::Waiting, None, 0, &task, &vm(), 5),
            ClaimAction::Drop("the card moved to Waiting Review in Lark".to_owned())
        );
    }

    #[test]
    fn hanya_dokumen_plan_dan_brainstorm_yang_boleh_ikut() {
        let files = [
            "docs/plans/2026-10-30-001-plan.md",
            "docs/brainstorms/x-requirements.md",
            "notes.txt",
            ".env.local",
            "docs/plans/../../.env",
            "src/docs/plans/x.md",
        ]
        .map(str::to_owned);
        assert_eq!(
            stray_files(&files),
            [
                "notes.txt",
                ".env.local",
                "docs/plans/../../.env",
                "src/docs/plans/x.md"
            ]
        );
        assert!(stray_files(&[]).is_empty());
    }

    #[test]
    fn status_bolak_balik_dan_follow_design() {
        for s in [
            PlannedState::HandoverPending,
            PlannedState::Waiting,
            PlannedState::Starting,
            PlannedState::Held,
        ] {
            assert_eq!(PlannedState::parse(s.as_str()), Some(s));
        }
        assert_eq!(PlannedState::parse("x"), None);
        let mut work = PlannedWork {
            plan: "p".to_owned(),
            branch: "b".to_owned(),
            design: vec![DesignMark {
                ac_id: "ac1".to_owned(),
                design: Design::Missing,
                note: None,
            }],
            laptop_design: Vec::new(),
        };
        assert!(!work.follows_design());
        work.design[0].design = Design::FollowDesign;
        assert!(work.follows_design());
    }
}
