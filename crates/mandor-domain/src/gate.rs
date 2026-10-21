//! Gerbang Start (R16a) dan allowlist transisi status milik app (R13, KTD-7).

use crate::kanban::TaskStatus;

/// Nilai Development Readiness yang boleh diambil agent.
pub const READY_VALUES: [&str; 2] = ["Ready To Dev", "Commit To Sprint"];

/// Alasan Start ditolak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GateRejection {
    #[error("task is not ready: Development Readiness \"{0}\"")]
    NotReady(String),
    #[error("task is not ready: Development Readiness is empty")]
    ReadinessMissing,
    #[error("task is not in To Do (currently \"{0}\")")]
    NotToDo(String),
}

/// Memeriksa gerbang Start dari data segar.
///
/// # Errors
/// [`GateRejection`] beserta alasan yang ditampilkan ke dev.
pub fn check_start(status: &TaskStatus, readiness: Option<&str>) -> Result<(), GateRejection> {
    match readiness {
        None | Some("") => return Err(GateRejection::ReadinessMissing),
        Some(r) if !READY_VALUES.contains(&r) => {
            return Err(GateRejection::NotReady(r.to_owned()));
        }
        Some(_) => {}
    }
    if *status != TaskStatus::ToDo {
        return Err(GateRejection::NotToDo(status.as_str().to_owned()));
    }
    Ok(())
}

/// `true` bila transisi termasuk allowlist app: To Do → In Progress,
/// In Progress → Waiting Review, Waiting Fix → In Progress.
#[must_use]
pub fn transition_allowed(from: &TaskStatus, to: &TaskStatus) -> bool {
    match from {
        TaskStatus::ToDo | TaskStatus::WaitingFix => *to == TaskStatus::InProgress,
        TaskStatus::InProgress => *to == TaskStatus::WaitingReview,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 8] = [
        "To Do",
        "In Progress",
        "Waiting Review",
        "In Review",
        "Waiting Fix",
        "Waiting Deploy",
        "Delivered",
        "Lain",
    ];

    #[test]
    fn hanya_tiga_transisi_yang_diizinkan() {
        let mut allowed = Vec::new();
        for from in ALL {
            for to in ALL {
                if transition_allowed(&TaskStatus::parse(from), &TaskStatus::parse(to)) {
                    allowed.push((from, to));
                }
            }
        }
        assert_eq!(
            allowed,
            [
                ("To Do", "In Progress"),
                ("In Progress", "Waiting Review"),
                ("Waiting Fix", "In Progress"),
            ]
        );
    }

    #[test]
    fn transisi_ke_in_review_dan_dari_waiting_deploy_ditolak() {
        let s = TaskStatus::parse;
        assert!(!transition_allowed(&s("Waiting Review"), &s("In Review")));
        for to in ALL {
            assert!(!transition_allowed(&s("Waiting Deploy"), &s(to)));
            assert!(!transition_allowed(&s(to), &s("Delivered")));
        }
    }

    #[test]
    fn start_ditolak_bila_readiness_review_ac_by_pm() {
        let err = check_start(&TaskStatus::ToDo, Some("Review AC By PM")).unwrap_err();
        assert_eq!(err, GateRejection::NotReady("Review AC By PM".to_owned()));
        assert!(err.to_string().contains("not ready"));
        assert_eq!(
            check_start(&TaskStatus::ToDo, None),
            Err(GateRejection::ReadinessMissing)
        );
    }

    #[test]
    fn start_diterima_untuk_ready_to_dev_dan_commit_to_sprint_berstatus_to_do() {
        for r in READY_VALUES {
            assert_eq!(check_start(&TaskStatus::ToDo, Some(r)), Ok(()));
        }
        assert!(matches!(
            check_start(&TaskStatus::InProgress, Some("Ready To Dev")),
            Err(GateRejection::NotToDo(_))
        ));
    }
}
