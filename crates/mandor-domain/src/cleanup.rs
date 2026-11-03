//! Penutupan run kartu yang sudah selesai (plan `2026-11-03-001`, R1, R2,
//! KTD-C1): kapan kartu dianggap selesai dan kapan run boleh ditutup.

use crate::{RunState, TaskStatus};

/// Alasan run selesai tidak ditutup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Agent, reviewer, atau push sedang berjalan.
    Working,
    /// Worktree punya perubahan atau commit yang belum di-push.
    UnpushedChanges,
}

impl SkipReason {
    /// Nama stabil untuk protokol.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::UnpushedChanges => "unpushed_changes",
        }
    }
}

/// Keputusan untuk satu run yang kartunya sudah selesai.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDecision {
    Close,
    Skip(SkipReason),
}

/// `true` bila status kartu dihitung selesai (R1).
#[must_use]
pub fn card_finished(status: &TaskStatus) -> bool {
    matches!(status, TaskStatus::Delivered | TaskStatus::WaitingDeploy)
}

/// `true` bila semua kartu run selesai: satu kartu untuk run tunggal, atau
/// kartu grup yang masih disertakan (R1). Daftar kosong tidak selesai.
#[must_use]
pub fn finished(statuses: &[TaskStatus]) -> bool {
    !statuses.is_empty() && statuses.iter().all(card_finished)
}

/// Run yang kartunya selesai boleh ditutup bila tidak sedang bekerja dan
/// worktree-nya bersih (R2).
#[must_use]
pub fn close_decision(state: RunState, clean: bool) -> CloseDecision {
    if state.is_working() {
        CloseDecision::Skip(SkipReason::Working)
    } else if !clean {
        CloseDecision::Skip(SkipReason::UnpushedChanges)
    } else {
        CloseDecision::Close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_dan_waiting_deploy_selesai_status_lain_tidak() {
        assert!(card_finished(&TaskStatus::Delivered));
        assert!(card_finished(&TaskStatus::WaitingDeploy));
        for s in [
            TaskStatus::ToDo,
            TaskStatus::InProgress,
            TaskStatus::WaitingReview,
            TaskStatus::InReview,
            TaskStatus::WaitingFix,
            TaskStatus::Unknown("Blocked".to_owned()),
        ] {
            assert!(!card_finished(&s), "{s:?}");
        }
    }

    #[test]
    fn grup_selesai_bila_semua_kartu_selesai() {
        assert!(finished(&[TaskStatus::Delivered]));
        assert!(finished(&[
            TaskStatus::Delivered,
            TaskStatus::WaitingDeploy
        ]));
        assert!(!finished(&[
            TaskStatus::Delivered,
            TaskStatus::WaitingReview
        ]));
        assert!(!finished(&[]), "tanpa kartu tidak dianggap selesai");
    }

    #[test]
    fn run_bekerja_atau_kotor_dilewati() {
        for s in [
            RunState::Preparing,
            RunState::Running,
            RunState::Reviewing,
            RunState::Publishing,
            RunState::WfRunning,
        ] {
            assert_eq!(
                close_decision(s, true),
                CloseDecision::Skip(SkipReason::Working),
                "{s:?}"
            );
        }
        assert_eq!(
            close_decision(RunState::AwaitingDev, false),
            CloseDecision::Skip(SkipReason::UnpushedChanges)
        );
        for s in [
            RunState::AwaitingDev,
            RunState::InReviewLoop,
            RunState::Held,
            RunState::Failed,
            RunState::Orphaned,
        ] {
            assert_eq!(close_decision(s, true), CloseDecision::Close, "{s:?}");
        }
        assert_eq!(SkipReason::UnpushedChanges.as_str(), "unpushed_changes");
    }
}
