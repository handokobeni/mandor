//! Method lembur VM (plan VM): antrean, setelan, laporan siklus, keputusan
//! gerbang AC, Send to review, dan serah-terima.

use mandor_domain::Task;
use std::collections::HashMap;

use mandor_protocol::{
    DequeuedView, ErrorCode, HandoffParams, HandoffResult, HeldPlannedView, OvernightQueueView,
    OvernightReportView, OvernightSettingsView, QueueAddParams, QueueItemView, QueueReorderParams,
    RecordRef, ReportCardView, RpcError, RunRef, method,
};
use mandor_usecase::StartTarget;
use mandor_usecase::overnight::{
    CardOutcome, Dequeued, HeldPlanned, OvernightReport, OvernightSettings, QueueItem,
    QueueRequest, ReportCard,
};
use serde_json::{Value, json};

use super::planned::{planned_dto, planned_of, target_of, target_view};
use super::{AppDispatch, ok, params, rejected, run_error, store_error, sync_error};
use crate::session::LarkSession;

impl AppDispatch {
    pub(super) fn is_overnight_method(name: &str) -> bool {
        [
            method::OVERNIGHT_GATE_DECIDE,
            method::OVERNIGHT_SEND_TO_REVIEW,
            method::RUNS_HANDOFF,
            method::OVERNIGHT_QUEUE_LIST,
            method::OVERNIGHT_QUEUE_ADD,
            method::OVERNIGHT_QUEUE_REMOVE,
            method::OVERNIGHT_QUEUE_REORDER,
            method::OVERNIGHT_QUEUE_START_NOW,
            method::OVERNIGHT_SETTINGS_GET,
            method::OVERNIGHT_SETTINGS_SET,
            method::OVERNIGHT_REPORT,
        ]
        .contains(&name)
    }

    pub(super) async fn overnight_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let s = self.session().await?;
        match name {
            // Keputusan dev atas kartu `ac_gate` memakai gerbang Push & PR yang sama (R12a).
            method::OVERNIGHT_GATE_DECIDE => self.publish(&s, params(v)?).await,
            method::OVERNIGHT_SEND_TO_REVIEW => {
                let p: RunRef = params(v)?;
                s.runs.send_to_review(&p.run_id).await.map_err(run_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            method::RUNS_HANDOFF => {
                let p: HandoffParams = params(v)?;
                let back = s
                    .runs
                    .handoff(&p.record_id, &p.to_daemon_id)
                    .await
                    .map_err(run_error)?;
                s.poke.notify_one();
                let (repo_id, workspace) = back.target.as_ref().map_or((None, None), target_view);
                ok(&HandoffResult {
                    branch: back.branch,
                    repo_id,
                    workspace,
                    planned: back.planned.as_ref().map(planned_dto),
                    draft: back.draft,
                })
            }
            other => self.overnight_queue_call(&s, other, v).await,
        }
    }

    async fn overnight_queue_call(
        &self,
        s: &LarkSession,
        name: &str,
        v: Value,
    ) -> Result<Value, RpcError> {
        let runs = &s.runs;
        match name {
            method::OVERNIGHT_QUEUE_LIST => self.queue_list(s).await,
            method::OVERNIGHT_QUEUE_ADD => {
                let p: QueueAddParams = params(v)?;
                let planned = match (p.planned, p.origin_daemon) {
                    (Some(w), Some(origin)) => Some((planned_of(w)?, origin)),
                    (Some(_), None) => {
                        return Err(rejected("a planned card needs the laptop daemon id"));
                    }
                    (None, _) => None,
                };
                let req = QueueRequest {
                    target: target_of(p.repo_id, p.workspace)?,
                    note: p.note,
                    design_sources: p.design_sources.unwrap_or_default(),
                    planned,
                };
                runs.enqueue_card(&p.record_id, &req)
                    .await
                    .map_err(run_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            method::OVERNIGHT_QUEUE_REMOVE => {
                let p: RecordRef = params(v)?;
                runs.queue_remove(&p.record_id).await.map_err(run_error)?;
                ok(&json!({}))
            }
            method::OVERNIGHT_QUEUE_REORDER => {
                let p: QueueReorderParams = params(v)?;
                runs.queue_reorder(&p.record_ids).await.map_err(run_error)?;
                ok(&json!({}))
            }
            method::OVERNIGHT_QUEUE_START_NOW => {
                let p: RecordRef = params(v)?;
                runs.queue_start_now(&p.record_id)
                    .await
                    .map_err(run_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            method::OVERNIGHT_SETTINGS_GET => ok(&runs
                .overnight_settings()
                .await
                .map_err(run_error)?
                .map(|s| settings_view(&s))),
            method::OVERNIGHT_SETTINGS_SET => {
                let p: OvernightSettingsView = params(v)?;
                runs.set_overnight_settings(&settings_of(p))
                    .await
                    .map_err(run_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            method::OVERNIGHT_REPORT => self.report(s).await,
            other => Err(RpcError::new(
                ErrorCode::MethodNotFound,
                format!("unknown method: {other}"),
            )),
        }
    }
}

impl AppDispatch {
    async fn queue_list(&self, s: &LarkSession) -> Result<Value, RpcError> {
        let runs = &s.runs;
        let items = runs.overnight_queue().await.map_err(run_error)?;
        let report = runs.overnight_report().await.map_err(run_error)?;
        let mut view = OvernightQueueView {
            items: Vec::new(),
            dequeued: Vec::new(),
        };
        let names = self.design_names().await?;
        for item in &items {
            let task = cached_task(s, &item.record_id).await?;
            let held = report.cards.iter().any(|c| {
                c.record_id == item.record_id
                    && matches!(c.outcome, CardOutcome::Held(_) | CardOutcome::Failed)
            });
            view.items
                .push(queue_item_view(item, task.as_ref(), held, &names));
        }
        for d in &report.dequeued {
            view.dequeued.push(dequeued_view(
                d,
                cached_task(s, &d.record_id).await?.as_ref(),
            ));
        }
        ok(&view)
    }

    async fn report(&self, s: &LarkSession) -> Result<Value, RpcError> {
        let runs = &s.runs;
        let report = runs.overnight_report().await.map_err(run_error)?;
        let mut cards = Vec::new();
        for c in &report.cards {
            cards.push(report_card_view(
                c,
                cached_task(s, &c.record_id).await?.as_ref(),
            ));
        }
        let mut dequeued = Vec::new();
        for d in &report.dequeued {
            dequeued.push(dequeued_view(
                d,
                cached_task(s, &d.record_id).await?.as_ref(),
            ));
        }
        let queue = runs.overnight_queue().await.map_err(run_error)?;
        let mut planned_held = Vec::new();
        for h in &report.planned_held {
            planned_held.push(held_planned_view(
                h,
                cached_task(s, &h.record_id).await?.as_ref(),
                items_target(&queue, h),
            ));
        }
        ok(&report_view(&report, cards, dequeued, planned_held))
    }

    /// Nama sumber desain daemon per id.
    async fn design_names(&self) -> Result<HashMap<String, String>, RpcError> {
        Ok(self
            .app
            .store
            .design_sources()
            .await
            .map_err(store_error)?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect())
    }
}

/// Repo item antrean untuk kartu Planned yang ditahan.
fn items_target(queue: &[QueueItem], h: &HeldPlanned) -> Option<String> {
    queue
        .iter()
        .find(|i| i.record_id == h.record_id)
        .and_then(|i| target_view(&i.target).0)
}

fn held_planned_view(
    h: &HeldPlanned,
    task: Option<&Task>,
    repo_id: Option<String>,
) -> HeldPlannedView {
    HeldPlannedView {
        record_id: h.record_id.clone(),
        key: task.and_then(|t| t.key.clone()),
        title: task.map(|t| t.title.clone()).unwrap_or_default(),
        repo_id,
        reason: h.reason.clone(),
    }
}

async fn cached_task(s: &LarkSession, record_id: &str) -> Result<Option<Task>, RpcError> {
    Ok(s.sync
        .cached(record_id)
        .await
        .map_err(sync_error)?
        .map(|c| c.task))
}

fn queue_item_view(
    item: &QueueItem,
    task: Option<&Task>,
    held: bool,
    names: &HashMap<String, String>,
) -> QueueItemView {
    let (repo_id, workspace_id) = match &item.target {
        StartTarget::Repo(r) => (Some(r.clone()), None),
        StartTarget::Workspace { id, .. } => (None, Some(id.clone())),
    };
    QueueItemView {
        record_id: item.record_id.clone(),
        key: task.and_then(|t| t.key.clone()),
        title: task.map(|t| t.title.clone()).unwrap_or_default(),
        repo_id,
        workspace_id,
        note: item.note.clone(),
        start_now: item.start_now,
        added_at_ms: item.added_at_ms,
        held,
        planned_state: item.planned.as_ref().map(|p| p.state.as_str().to_owned()),
        planned_reason: item.planned.as_ref().and_then(|p| p.reason.clone()),
        design_sources: item
            .design_sources
            .iter()
            .map(|id| names.get(id).cloned().unwrap_or_else(|| id.clone()))
            .collect(),
    }
}

fn dequeued_view(d: &Dequeued, task: Option<&Task>) -> DequeuedView {
    DequeuedView {
        record_id: d.record_id.clone(),
        key: task.and_then(|t| t.key.clone()),
        title: task.map(|t| t.title.clone()).unwrap_or_default(),
        reason: d.reason.clone(),
        at_ms: d.at_ms,
    }
}

fn settings_view(s: &OvernightSettings) -> OvernightSettingsView {
    OvernightSettingsView {
        start: s.start.clone(),
        end: s.end.clone(),
        offset: s.offset.clone(),
        token_limit: s.token_limit,
    }
}

fn settings_of(v: OvernightSettingsView) -> OvernightSettings {
    OvernightSettings {
        start: v.start,
        end: v.end,
        offset: v.offset,
        token_limit: v.token_limit.filter(|l| *l > 0),
    }
}

fn report_card_view(c: &ReportCard, task: Option<&Task>) -> ReportCardView {
    let (outcome, hold_reason) = match &c.outcome {
        CardOutcome::Draft => ("draft", None),
        CardOutcome::SentToReview => ("sent_to_review", None),
        CardOutcome::Held(r) => ("held", Some(r.as_str().to_owned())),
        CardOutcome::Failed => ("failed", None),
        CardOutcome::Working => ("working", None),
    };
    ReportCardView {
        record_id: c.record_id.clone(),
        key: task.and_then(|t| t.key.clone()),
        title: task.map(|t| t.title.clone()).unwrap_or_default(),
        run_id: c.run_id.clone(),
        repo_id: c.repo_id.clone(),
        outcome: outcome.to_owned(),
        hold_reason,
        hold_note: c.hold_note.clone(),
        pr_url: c.pr_url.clone(),
        ac_pass: c.ac_pass,
        ac_fail: c.ac_fail,
        ac_cant_verify: c.ac_cant_verify,
        entered_waiting_fix: c.entered_waiting_fix,
        design_notes: c.design_notes.clone(),
    }
}

fn report_view(
    r: &OvernightReport,
    cards: Vec<ReportCardView>,
    dequeued: Vec<DequeuedView>,
    planned_held: Vec<HeldPlannedView>,
) -> OvernightReportView {
    OvernightReportView {
        cycle_start_ms: r.cycle_start_ms,
        used: r.usage.counted(),
        limit: r.limit,
        cache_read_tokens: r.usage.cache_read_tokens,
        cards,
        dequeued,
        reviewed_without_fix: r.reviewed_without_fix,
        planned_held,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandor_domain::HoldReason;

    #[test]
    fn hasil_kartu_dan_setelan_dipetakan_ke_view() {
        let mut task = Task::new("rec1", mandor_domain::TaskStatus::InProgress);
        task.key = Some("PGS-1".to_owned());
        task.title = "Form".to_owned();
        let card = ReportCard {
            record_id: "rec1".to_owned(),
            run_id: "run-1".to_owned(),
            repo_id: "repo1".to_owned(),
            outcome: CardOutcome::Held(HoldReason::AcGate),
            hold_note: Some("AC-2 can't be checked".to_owned()),
            pr_url: None,
            ac_pass: 2,
            ac_fail: 0,
            ac_cant_verify: 1,
            entered_waiting_fix: false,
            design_notes: vec![],
        };
        let v = report_card_view(&card, Some(&task));
        assert_eq!(
            (v.outcome.as_str(), v.hold_reason.as_deref()),
            ("held", Some("ac_gate"))
        );
        assert_eq!(v.key.as_deref(), Some("PGS-1"));
        let s = settings_of(OvernightSettingsView {
            start: "19:00".to_owned(),
            end: "06:00".to_owned(),
            offset: "+07:00".to_owned(),
            token_limit: Some(0),
        });
        assert_eq!(s.token_limit, None, "batas 0 berarti belum diisi");
        assert_eq!(settings_view(&s).start, "19:00");
        let item = QueueItem {
            record_id: "rec1".to_owned(),
            target: StartTarget::Workspace {
                id: "kelola".to_owned(),
                repo_ids: vec!["api".to_owned()],
            },
            note: None,
            start_now: true,
            added_at_ms: 5,
            design_sources: vec!["ds1".to_owned(), "hilang".to_owned()],
            planned: None,
        };
        let names = HashMap::from([("ds1".to_owned(), "kelola-mock".to_owned())]);
        let q = queue_item_view(&item, None, true, &names);
        assert_eq!(
            (q.repo_id, q.workspace_id.as_deref()),
            (None, Some("kelola"))
        );
        assert!(q.held && q.start_now);
        assert_eq!(q.design_sources, ["kelola-mock", "hilang"]);
        assert_eq!(q.planned_state, None);
    }
}
