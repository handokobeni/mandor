//! Penyimpanan kartu pertanyaan yang memberi tahu client saat kartu run
//! berubah (`questions.updated`, plan 2026-11-07-001 U5).

use mandor_domain::question::QuestionStatus;
use mandor_protocol::Notification;
use mandor_protocol::{RunEventView, event};
use mandor_store::QuestionsStore;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{QuestionRecord, QuestionStore};
use serde_json::Value;
use tokio::sync::broadcast;

/// [`QuestionsStore`] dengan event `questions.updated` (dan `run.updated`
/// untuk tombol Approve) setelah setiap perubahan.
#[derive(Debug, Clone)]
pub struct BroadcastQuestions {
    inner: QuestionsStore,
    events: broadcast::Sender<Notification>,
}

impl BroadcastQuestions {
    pub fn new(inner: QuestionsStore, events: broadcast::Sender<Notification>) -> Self {
        Self { inner, events }
    }

    fn changed(&self, run_id: &str) {
        let view = serde_json::to_value(RunEventView {
            run_id: run_id.to_owned(),
        })
        .unwrap_or(Value::Null);
        crate::prod::emit(&self.events, event::QUESTIONS_UPDATED, view.clone());
        crate::prod::emit(&self.events, event::RUN_UPDATED, view);
    }
}

impl QuestionStore for BroadcastQuestions {
    async fn create_question(&self, q: &QuestionRecord) -> Result<bool, PortError> {
        let created = self.inner.create_question(q).await?;
        if created {
            self.changed(&q.run_id);
        }
        Ok(created)
    }

    async fn question(&self, id: &str) -> Result<Option<QuestionRecord>, PortError> {
        self.inner.question(id).await
    }

    async fn questions(&self, run_id: &str) -> Result<Vec<QuestionRecord>, PortError> {
        self.inner.questions(run_id).await
    }

    async fn pending_question(&self, run_id: &str) -> Result<Option<QuestionRecord>, PortError> {
        self.inner.pending_question(run_id).await
    }

    async fn close_question(
        &self,
        id: &str,
        status: QuestionStatus,
        answer: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, PortError> {
        let closed = self
            .inner
            .close_question(id, status, answer, now_ms)
            .await?;
        if closed {
            if let Some(q) = self.inner.question(id).await? {
                self.changed(&q.run_id);
            }
        }
        Ok(closed)
    }
}
