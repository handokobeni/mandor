//! Konversi DTO kartu Planned dan target antrean (plan 2026-10-30-001, U6).

use mandor_domain::Design;
use mandor_domain::planned::{DesignMark, PlannedWork};
use mandor_protocol::{DesignMarkDto, PlannedWorkDto, RpcError, StartWorkspace};
use mandor_usecase::StartTarget;

use super::rejected;

/// Paket plan menjadi DTO.
pub(super) fn planned_dto(w: &PlannedWork) -> PlannedWorkDto {
    PlannedWorkDto {
        plan: w.plan.clone(),
        branch: w.branch.clone(),
        design: w
            .design
            .iter()
            .map(|d| DesignMarkDto {
                ac_id: d.ac_id.clone(),
                design: d.design.as_str().to_owned(),
                note: d.note.clone(),
            })
            .collect(),
        laptop_design: w.laptop_design.clone(),
    }
}

/// DTO menjadi paket plan; nilai Design yang tidak dikenal ditolak.
pub(super) fn planned_of(d: PlannedWorkDto) -> Result<PlannedWork, RpcError> {
    let mut design = Vec::with_capacity(d.design.len());
    for m in d.design {
        let value = Design::parse(&m.design)
            .ok_or_else(|| rejected(format!("unknown design value: {}", m.design)))?;
        design.push(DesignMark {
            ac_id: m.ac_id,
            design: value,
            note: m.note,
        });
    }
    Ok(PlannedWork {
        plan: d.plan,
        branch: d.branch,
        design,
        laptop_design: d.laptop_design,
    })
}

/// Target Start dari parameter: workspace bila diisi, selain itu satu repo.
pub(super) fn target_of(
    repo_id: Option<String>,
    workspace: Option<StartWorkspace>,
) -> Result<StartTarget, RpcError> {
    match (workspace, repo_id) {
        (Some(w), _) => Ok(StartTarget::Workspace {
            id: w.id,
            repo_ids: w.repo_ids,
        }),
        (None, Some(repo)) => Ok(StartTarget::Repo(repo)),
        (None, None) => Err(rejected("choose a repo or workspace")),
    }
}

/// Target menjadi pasangan field DTO `(repo_id, workspace)`.
pub(super) fn target_view(t: &StartTarget) -> (Option<String>, Option<StartWorkspace>) {
    match t {
        StartTarget::Repo(r) => (Some(r.clone()), None),
        StartTarget::Workspace { id, repo_ids } => (
            None,
            Some(StartWorkspace {
                id: id.clone(),
                repo_ids: repo_ids.clone(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn paket_plan_bolak_balik_dan_design_tak_dikenal_ditolak() {
        let w = PlannedWork {
            plan: "## Plan".to_owned(),
            branch: "mandor/pgs-1".to_owned(),
            design: vec![DesignMark {
                ac_id: "ac1".to_owned(),
                design: Design::FollowDesign,
                note: Some("x".to_owned()),
            }],
            laptop_design: vec!["ds1".to_owned()],
        };
        let dto = planned_dto(&w);
        let json = serde_json::to_string(&dto).unwrap();
        let back: PlannedWorkDto = serde_json::from_str(&json).unwrap();
        assert_eq!(planned_of(back).unwrap(), w);
        let mut bad = planned_dto(&w);
        bad.design[0].design = "maybe".to_owned();
        assert!(planned_of(bad).is_err());
    }

    #[test]
    fn target_workspace_didahulukan() {
        let ws = StartWorkspace {
            id: "k".to_owned(),
            repo_ids: vec!["a".to_owned()],
        };
        let t = target_of(Some("r".to_owned()), Some(ws.clone())).unwrap();
        assert_eq!(target_view(&t), (None, Some(ws)));
        assert!(target_of(None, None).is_err());
    }
}
