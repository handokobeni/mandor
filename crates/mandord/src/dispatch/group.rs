//! Method run grup (plan run grup U8): kandidat, start, pindah kartu, dan
//! keputusan kartu yang berubah di Lark.

use std::collections::HashMap;

use mandor_domain::group::CardDecision;
use mandor_protocol::{
    ErrorCode, GateCardDecisionParams, GateCardDecisionResult, GroupCandidateView,
    GroupCandidatesParams, GroupCardRef, GroupCardView, RpcError, RunRef, StartGroupParams,
    StartProgressView, event, method,
};
use mandor_usecase::run::CardOutcome;
use mandor_usecase::run_ports::RunRecord;
use mandor_usecase::{Planning, StartStep, StartTarget};
use serde_json::{Value, json};

use super::{AppDispatch, ok, params, planning_of, run_error};
use crate::prod::emit;
use crate::session::LarkSession;
use crate::views::{group_card_view, run_view};

impl AppDispatch {
    pub(super) fn is_group_method(name: &str) -> bool {
        [
            method::GROUPS_CANDIDATES,
            method::RUNS_START_GROUP,
            method::RUNS_NEXT_CARD,
            method::RUNS_CONTINUE_GROUP,
            method::RUNS_REMOVE_CARD,
            method::RUNS_STOP_GROUP,
            method::RUNS_GATE_CARD_DECISION,
        ]
        .contains(&name)
    }

    pub(super) async fn group_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let runs = &s.runs;
        match name {
            method::GROUPS_CANDIDATES => {
                let p: GroupCandidatesParams = params(v)?;
                let list = runs
                    .group_candidates(&p.record_id, &p.queued.unwrap_or_default())
                    .await
                    .map_err(run_error)?;
                let views: Vec<GroupCandidateView> = list
                    .into_iter()
                    .map(|c| GroupCandidateView {
                        record_id: c.record_id,
                        key: c.key,
                        title: c.title,
                        blocked: c.blocked.map(|b| b.to_string()),
                    })
                    .collect();
                ok(&views)
            }
            method::RUNS_START_GROUP => self.start_group(&s, params(v)?).await,
            method::RUNS_NEXT_CARD => {
                let p: RunRef = params(v)?;
                runs.next_card(&p.run_id).await.map_err(run_error)?;
                ok(&json!({}))
            }
            method::RUNS_CONTINUE_GROUP => {
                let p: RunRef = params(v)?;
                runs.continue_group(&p.run_id).await.map_err(run_error)?;
                ok(&json!({}))
            }
            method::RUNS_REMOVE_CARD => {
                let p: GroupCardRef = params(v)?;
                runs.remove_card(&p.run_id, &p.record_id)
                    .await
                    .map_err(run_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            method::RUNS_STOP_GROUP => {
                let p: RunRef = params(v)?;
                runs.stop_group(&p.run_id).await.map_err(run_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            method::RUNS_GATE_CARD_DECISION => {
                let p: GateCardDecisionParams = params(v)?;
                let decision = card_decision(&p)?;
                let out = runs
                    .gate_card_decision(&p.run_id, &p.record_id, &decision)
                    .await
                    .map_err(run_error)?;
                let outcome = match out {
                    CardOutcome::Kept => "kept",
                    CardOutcome::Reverted => "reverted",
                    CardOutcome::RevertConflict => "revert_conflict",
                };
                ok(&GateCardDecisionResult {
                    outcome: outcome.to_owned(),
                })
            }
            other => Err(RpcError::new(
                ErrorCode::MethodNotFound,
                format!("unknown method: {other}"),
            )),
        }
    }

    async fn start_group(&self, s: &LarkSession, p: StartGroupParams) -> Result<Value, RpcError> {
        let events = self.app.events.clone();
        let record_id = p.record_ids.first().cloned().unwrap_or_default();
        let progress = move |step: StartStep| {
            let view = StartProgressView {
                record_id: record_id.clone(),
                step: step.as_str().to_owned(),
            };
            emit(
                &events,
                event::START_PROGRESS,
                serde_json::to_value(view).unwrap_or(Value::Null),
            );
        };
        let target = match p.workspace {
            Some(w) => StartTarget::Workspace {
                id: w.id,
                repo_ids: w.repo_ids,
            },
            None => StartTarget::Repo(p.repo_id.clone()),
        };
        let planning = planning_of(p.planning.as_ref())?;
        let mode = group_start(&planning, p.one_plan.unwrap_or(false))?;
        let design = self
            .design_roots(p.design_sources.as_deref().unwrap_or_default())
            .await?;
        let started = match mode {
            GroupStart::OnePlan { skill } => {
                s.runs
                    .start_one_plan_group(
                        &p.record_ids,
                        &target,
                        p.note.as_deref(),
                        skill,
                        &design,
                        progress,
                    )
                    .await
            }
            GroupStart::PerCard => {
                s.runs
                    .start_group(
                        &p.record_ids,
                        &target,
                        p.note.as_deref(),
                        &planning,
                        &design,
                        progress,
                    )
                    .await
            }
        };
        let run = started.map_err(run_error)?;
        if let Some(skill) = p.reviewer_skill.as_deref().filter(|k| !k.trim().is_empty()) {
            s.runs
                .set_reviewer_skill(&run.id, Some(skill))
                .await
                .map_err(run_error)?;
        }
        s.poke.notify_one();
        ok(&run_view(&run, None))
    }
}

/// Cara grup dimulai: plan per kartu, atau satu plan untuk semua kartu (R25).
#[derive(Debug, PartialEq, Eq)]
enum GroupStart {
    PerCard,
    OnePlan { skill: Option<String> },
}

/// Mode "One plan for all cards" hanya sah dengan planning `plan`.
fn group_start(planning: &Planning, one_plan: bool) -> Result<GroupStart, RpcError> {
    if !one_plan {
        return Ok(GroupStart::PerCard);
    }
    match planning {
        Planning::Plan { skill } => Ok(GroupStart::OnePlan {
            skill: skill.clone(),
        }),
        _ => Err(RpcError::new(
            ErrorCode::InvalidParams,
            "one plan for all cards needs the Plan choice",
        )),
    }
}

fn card_decision(p: &GateCardDecisionParams) -> Result<CardDecision, RpcError> {
    match p.decision.as_str() {
        "revert" => Ok(CardDecision::Revert),
        "keep" => Ok(CardDecision::Keep {
            reason: p.reason.clone().unwrap_or_default(),
        }),
        other => Err(RpcError::new(
            ErrorCode::InvalidParams,
            format!("unknown decision: {other}"),
        )),
    }
}

/// Kartu setiap run grup aktif untuk papan dan daftar run; run tunggal tidak
/// punya entri. Kunci dan judul kartu dari cache task.
pub(super) async fn group_views(
    s: &LarkSession,
    runs: &[RunRecord],
) -> Result<HashMap<String, Vec<GroupCardView>>, RpcError> {
    let mut out = HashMap::new();
    for r in runs {
        let cards = s.runs.group_cards(&r.id).await.map_err(run_error)?;
        if cards.is_empty() {
            continue;
        }
        let mut views = Vec::with_capacity(cards.len());
        for c in &cards {
            let task = s
                .sync
                .cached(&c.record_id)
                .await
                .map_err(super::sync_error)?;
            views.push(group_card_view(c, task.as_ref().map(|t| &t.task)));
        }
        out.insert(r.id.clone(), views);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn satu_plan_grup_hanya_dengan_pilihan_plan() {
        let plan = Planning::Plan {
            skill: Some("ce-plan".to_owned()),
        };
        assert_eq!(group_start(&plan, false).ok(), Some(GroupStart::PerCard));
        assert_eq!(
            group_start(&plan, true).ok(),
            Some(GroupStart::OnePlan {
                skill: Some("ce-plan".to_owned())
            })
        );
        assert!(group_start(&Planning::Skip, true).is_err());
        let brainstorm = Planning::Brainstorm {
            brainstorm: None,
            plan: None,
        };
        assert!(group_start(&brainstorm, true).is_err());
    }

    #[test]
    fn keputusan_kartu_dari_parameter() {
        let p = |d: &str, reason: Option<&str>| GateCardDecisionParams {
            run_id: "g".to_owned(),
            record_id: "c1".to_owned(),
            decision: d.to_owned(),
            reason: reason.map(str::to_owned),
        };
        assert_eq!(
            card_decision(&p("revert", None)).ok(),
            Some(CardDecision::Revert)
        );
        assert_eq!(
            card_decision(&p("keep", Some("QA"))).ok(),
            Some(CardDecision::Keep {
                reason: "QA".to_owned()
            })
        );
        assert!(card_decision(&p("hapus", None)).is_err());
    }
}
