//! Penyimpanan kartu persetujuan yang memberi tahu client saat kartu run
//! berubah (`approvals.updated`, plan 2026-11-06-001 U6).

use mandor_protocol::Notification;
use mandor_protocol::{RunEventView, event};
use mandor_store::ApprovalsStore;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{ApprovalAnswer, ApprovalCard, ApprovalStore, RunDecision};
use serde_json::Value;
use tokio::sync::broadcast;

/// [`ApprovalsStore`] dengan event `approvals.updated` (dan `run.updated`
/// untuk badge papan) setelah setiap perubahan.
#[derive(Debug, Clone)]
pub struct BroadcastApprovals {
    inner: ApprovalsStore,
    events: broadcast::Sender<Notification>,
}

impl BroadcastApprovals {
    pub fn new(inner: ApprovalsStore, events: broadcast::Sender<Notification>) -> Self {
        Self { inner, events }
    }

    fn changed(&self, run_id: &str) {
        let view = serde_json::to_value(RunEventView {
            run_id: run_id.to_owned(),
        })
        .unwrap_or(Value::Null);
        crate::prod::emit(&self.events, event::APPROVALS_UPDATED, view.clone());
        crate::prod::emit(&self.events, event::RUN_UPDATED, view);
    }

    async fn run_of(&self, id: &str) -> Option<String> {
        self.inner
            .approval(id)
            .await
            .ok()
            .flatten()
            .map(|c| c.run_id)
    }
}

impl ApprovalStore for BroadcastApprovals {
    async fn create_approval(&self, card: &ApprovalCard) -> Result<(), PortError> {
        self.inner.create_approval(card).await?;
        self.changed(&card.run_id);
        Ok(())
    }

    async fn approval(&self, id: &str) -> Result<Option<ApprovalCard>, PortError> {
        self.inner.approval(id).await
    }

    async fn approvals(&self, run_id: &str) -> Result<Vec<ApprovalCard>, PortError> {
        self.inner.approvals(run_id).await
    }

    async fn pending_approvals(&self, run_id: &str) -> Result<usize, PortError> {
        self.inner.pending_approvals(run_id).await
    }

    async fn answer_approval(
        &self,
        id: &str,
        answer: ApprovalAnswer,
        grant_key: &str,
        deny_key: &str,
        now_ms: i64,
    ) -> Result<bool, PortError> {
        let won = self
            .inner
            .answer_approval(id, answer, grant_key, deny_key, now_ms)
            .await?;
        if let (true, Some(run)) = (won, self.run_of(id).await) {
            self.changed(&run);
        }
        Ok(won)
    }

    async fn expire_approval(&self, id: &str, now_ms: i64) -> Result<bool, PortError> {
        let won = self.inner.expire_approval(id, now_ms).await?;
        if let (true, Some(run)) = (won, self.run_of(id).await) {
            self.changed(&run);
        }
        Ok(won)
    }

    async fn cancel_approvals(
        &self,
        run_id: Option<&str>,
        now_ms: i64,
    ) -> Result<Vec<String>, PortError> {
        let ids = self.inner.cancel_approvals(run_id, now_ms).await?;
        if let (Some(run), false) = (run_id, ids.is_empty()) {
            self.changed(run);
        }
        Ok(ids)
    }

    async fn run_decision(
        &self,
        run_id: &str,
        key: &str,
    ) -> Result<Option<RunDecision>, PortError> {
        self.inner.run_decision(run_id, key).await
    }

    async fn deny_mode(&self, run_id: &str) -> Result<bool, PortError> {
        self.inner.deny_mode(run_id).await
    }

    async fn set_deny_mode(&self, run_id: &str, on: bool, now_ms: i64) -> Result<(), PortError> {
        self.inner.set_deny_mode(run_id, on, now_ms).await?;
        self.changed(run_id);
        Ok(())
    }
}
