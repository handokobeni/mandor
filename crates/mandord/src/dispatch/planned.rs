//! Konversi DTO kartu Planned dan target antrean (plan 2026-10-30-001, U6).

use mandor_domain::group::{AcSnap, CardState};
use mandor_domain::planned::{DesignMark, PlannedWork};
use mandor_domain::{AcRow, Design, RowStatus, Verdict};
use mandor_protocol::{
    AcRowView, AcSnapDto, CardCommitDto, CarriedCardDto, CarriedReviewDto, DesignMarkDto,
    PlannedWorkDto, RpcError, StartWorkspace,
};
use mandor_usecase::run_ports::{CardCommit, GroupCardRow};
use mandor_usecase::{CarriedReview, StartTarget};

use crate::views::ac_row_view;

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
        repos: w.repos.clone(),
        group_acs: w.group_acs.as_deref().map(snaps_dto),
    }
}

/// Cuplikan AC plan grup menjadi DTO.
pub(super) fn snaps_dto(v: &[AcSnap]) -> Vec<AcSnapDto> {
    v.iter()
        .map(|a| AcSnapDto {
            card: a.card.clone(),
            ac_id: a.ac_id.clone(),
            text: a.text.clone(),
        })
        .collect()
}

/// DTO menjadi cuplikan AC plan grup.
pub(super) fn snaps_of(v: Vec<AcSnapDto>) -> Vec<AcSnap> {
    v.into_iter()
        .map(|a| AcSnap {
            card: a.card,
            ac_id: a.ac_id,
            text: a.text,
        })
        .collect()
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
        repos: d.repos,
        group_acs: d.group_acs.map(snaps_of),
    })
}

/// Hasil review AC VM menjadi DTO.
pub(super) fn review_dto(r: &CarriedReview) -> CarriedReviewDto {
    CarriedReviewDto {
        rows: r.rows.iter().map(ac_row_view).collect(),
        snapshot: r.snapshot.clone(),
    }
}

/// DTO menjadi hasil review AC; nilai yang tidak dikenal ditolak.
pub(super) fn review_of(d: CarriedReviewDto) -> Result<CarriedReview, RpcError> {
    let rows = d
        .rows
        .into_iter()
        .map(ac_row_of)
        .collect::<Result<_, _>>()?;
    Ok(CarriedReview {
        rows,
        snapshot: d.snapshot,
    })
}

fn ac_row_of(v: AcRowView) -> Result<AcRow, RpcError> {
    let unknown = |what: &str, value: &str| rejected(format!("unknown {what} value: {value}"));
    Ok(AcRow {
        design: Design::parse(&v.design).ok_or_else(|| unknown("design", &v.design))?,
        verdict: Verdict::parse(&v.verdict).ok_or_else(|| unknown("verdict", &v.verdict))?,
        status: RowStatus::parse(&v.status).ok_or_else(|| unknown("status", &v.status))?,
        ac_id: v.ac_id,
        label: v.label,
        text: v.text,
        design_note: v.design_note,
        evidence: v.evidence,
        stale: v.stale,
        reason: v.reason,
        decided_by: v.decided_by,
        card: v.card,
    })
}

/// Kartu grup menjadi DTO serah-terima (KTD-G15 plan grup).
pub(super) fn card_dto(c: &GroupCardRow) -> CarriedCardDto {
    CarriedCardDto {
        record_id: c.record_id.clone(),
        state: c.state.as_str().to_owned(),
        changed_in_lark: c.changed_in_lark,
        kept_reason: c.kept_reason.clone(),
        commits: c
            .commits
            .iter()
            .map(|m| CardCommitDto {
                repo_id: m.repo_id.clone(),
                sha: m.sha.clone(),
            })
            .collect(),
        plan: c.plan.clone(),
        fix_requested_by: c.fix_requested_by.clone(),
        quiet_until_ms: c.quiet_until_ms,
        in_fix: c.in_fix,
        fix_replies: c.fix_replies.clone(),
    }
}

/// DTO menjadi kartu grup; state yang tidak dikenal ditolak.
pub(super) fn card_of(d: CarriedCardDto) -> Result<GroupCardRow, RpcError> {
    let state = CardState::parse(&d.state)
        .ok_or_else(|| rejected(format!("unknown card state: {}", d.state)))?;
    Ok(GroupCardRow {
        record_id: d.record_id,
        state,
        changed_in_lark: d.changed_in_lark,
        kept_reason: d.kept_reason,
        commits: d
            .commits
            .into_iter()
            .map(|m| CardCommit {
                repo_id: m.repo_id,
                sha: m.sha,
            })
            .collect(),
        plan: d.plan,
        fix_requested_by: d.fix_requested_by,
        quiet_until_ms: d.quiet_until_ms,
        in_fix: d.in_fix,
        fix_replies: d.fix_replies,
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
    fn kartu_grup_bolak_balik_dan_state_tak_dikenal_ditolak() {
        let mut c = GroupCardRow::pending("c1");
        c.state = CardState::Done;
        c.changed_in_lark = true;
        c.commits = vec![CardCommit {
            repo_id: "web".to_owned(),
            sha: "abc".to_owned(),
        }];
        c.plan = Some("## Plan".to_owned());
        let json = serde_json::to_string(&card_dto(&c)).unwrap();
        let back: CarriedCardDto = serde_json::from_str(&json).unwrap();
        assert_eq!(card_of(back).unwrap(), c);
        let mut bad = card_dto(&c);
        bad.state = "maybe".to_owned();
        assert!(card_of(bad).is_err());
    }

    #[test]
    fn hasil_review_bolak_balik_dan_verdict_tak_dikenal_ditolak() {
        let mut row = AcRow::new("ac1", Some("AC-1"), "teks", RowStatus::Active);
        row.verdict = Verdict::Pass;
        row.evidence = Some("tampil".to_owned());
        let r = CarriedReview {
            rows: vec![row],
            snapshot: [("web".to_owned(), "t1".to_owned())].into(),
        };
        let json = serde_json::to_string(&review_dto(&r)).unwrap();
        let back: CarriedReviewDto = serde_json::from_str(&json).unwrap();
        assert_eq!(review_of(back).unwrap(), r);
        let mut bad = review_dto(&r);
        bad.rows[0].verdict = "maybe".to_owned();
        assert!(review_of(bad).is_err());
    }

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
            repos: vec!["web".to_owned(), "api".to_owned()],
            group_acs: Some(vec![AcSnap {
                card: "c2".to_owned(),
                ac_id: "ac2".to_owned(),
                text: "AC kartu 2".to_owned(),
            }]),
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
