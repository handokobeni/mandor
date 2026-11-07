//! Fake port kartu persetujuan (plan 2026-11-06-001, U4).
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mandor_domain::permit::Decision;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{
    ApprovalAnswer, ApprovalCard, ApprovalStatus, ApprovalStore, ApprovalWaiter, PermitFacts,
    PermitRef, RunDecision, WaitOutcome,
};
use tokio::sync::watch;

#[derive(Clone, Default)]
pub struct FakeApprovals {
    pub cards: Arc<Mutex<Vec<ApprovalCard>>>,
    /// (run, kunci mentah) → keputusan.
    pub decisions: Arc<Mutex<HashMap<(String, String), RunDecision>>>,
    pub deny_mode: Arc<Mutex<HashSet<String>>>,
}

impl FakeApprovals {
    pub fn pending(&self, run_id: &str) -> Vec<ApprovalCard> {
        self.cards
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.run_id == run_id && c.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    fn close(
        &self,
        id: &str,
        status: ApprovalStatus,
        answer: Option<ApprovalAnswer>,
        now: i64,
    ) -> bool {
        let mut cards = self.cards.lock().unwrap();
        let Some(card) = cards
            .iter_mut()
            .find(|c| c.id == id && c.status == ApprovalStatus::Pending)
        else {
            return false;
        };
        card.status = status;
        card.answer = answer;
        card.closed_ms = Some(now);
        true
    }
}

impl ApprovalStore for FakeApprovals {
    async fn create_approval(&self, card: &ApprovalCard) -> Result<(), PortError> {
        self.cards.lock().unwrap().push(card.clone());
        Ok(())
    }

    async fn approval(&self, id: &str) -> Result<Option<ApprovalCard>, PortError> {
        Ok(self
            .cards
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.id == id)
            .cloned())
    }

    async fn approvals(&self, run_id: &str) -> Result<Vec<ApprovalCard>, PortError> {
        Ok(self
            .cards
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.run_id == run_id)
            .cloned()
            .collect())
    }

    async fn pending_approvals(&self, run_id: &str) -> Result<usize, PortError> {
        Ok(self.pending(run_id).len())
    }

    async fn answer_approval(
        &self,
        id: &str,
        answer: ApprovalAnswer,
        grant_key: &str,
        deny_key: &str,
        now_ms: i64,
    ) -> Result<bool, PortError> {
        let run_id = match self.cards.lock().unwrap().iter().find(|c| c.id == id) {
            Some(c) => c.run_id.clone(),
            None => return Ok(false),
        };
        if !self.close(id, ApprovalStatus::Answered, Some(answer), now_ms) {
            return Ok(false);
        }
        let mut decisions = self.decisions.lock().unwrap();
        match answer {
            ApprovalAnswer::AllowRun => {
                decisions.insert((run_id, grant_key.to_owned()), RunDecision::Allow);
            }
            ApprovalAnswer::Deny => {
                decisions.insert((run_id, deny_key.to_owned()), RunDecision::Deny);
            }
            ApprovalAnswer::AllowOnce => {}
        }
        Ok(true)
    }

    async fn expire_approval(&self, id: &str, now_ms: i64) -> Result<bool, PortError> {
        Ok(self.close(id, ApprovalStatus::Expired, None, now_ms))
    }

    async fn cancel_approvals(
        &self,
        run_id: Option<&str>,
        now_ms: i64,
    ) -> Result<Vec<String>, PortError> {
        let ids: Vec<String> = self
            .cards
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.status == ApprovalStatus::Pending && run_id.is_none_or(|r| c.run_id == r))
            .map(|c| c.id.clone())
            .collect();
        for id in &ids {
            self.close(id, ApprovalStatus::Cancelled, None, now_ms);
        }
        Ok(ids)
    }

    async fn run_decision(
        &self,
        run_id: &str,
        key: &str,
    ) -> Result<Option<RunDecision>, PortError> {
        Ok(self
            .decisions
            .lock()
            .unwrap()
            .get(&(run_id.to_owned(), key.to_owned()))
            .copied())
    }

    async fn deny_mode(&self, run_id: &str) -> Result<bool, PortError> {
        Ok(self.deny_mode.lock().unwrap().contains(run_id))
    }

    async fn set_deny_mode(&self, run_id: &str, on: bool, _now_ms: i64) -> Result<(), PortError> {
        let mut set = self.deny_mode.lock().unwrap();
        if on {
            set.insert(run_id.to_owned());
        } else {
            set.remove(run_id);
        }
        Ok(())
    }
}

/// Penunggu satu kartu: run dan pengirim hasilnya.
type Slot = (String, watch::Sender<Option<WaitOutcome>>);

/// Waiter palsu: tenggat dipicu test lewat [`FakeWaiter::time_out`], bukan jam.
#[derive(Clone, Default)]
pub struct FakeWaiter {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    pub timeouts: Arc<Mutex<Vec<Duration>>>,
}

impl FakeWaiter {
    /// Tenggat semua kartu yang sedang ditunggu terlewati.
    pub fn time_out(&self, card_id: &str) {
        if let Some((_, tx)) = self.slots.lock().unwrap().get(card_id) {
            let _ = tx.send(Some(WaitOutcome::TimedOut));
        }
    }

    pub fn waiting(&self) -> usize {
        self.slots.lock().unwrap().len()
    }
}

impl ApprovalWaiter for FakeWaiter {
    type Ticket = (String, watch::Receiver<Option<WaitOutcome>>);

    fn register(&self, card_id: &str, run_id: &str) -> Self::Ticket {
        let (tx, rx) = watch::channel(None);
        self.slots
            .lock()
            .unwrap()
            .insert(card_id.to_owned(), (run_id.to_owned(), tx));
        (card_id.to_owned(), rx)
    }

    async fn wait(&self, ticket: Self::Ticket, timeout: Duration) -> WaitOutcome {
        self.timeouts.lock().unwrap().push(timeout);
        let (id, mut rx) = ticket;
        let outcome = loop {
            if let Some(o) = *rx.borrow_and_update() {
                break o;
            }
            if rx.changed().await.is_err() {
                break WaitOutcome::Cancelled;
            }
        };
        self.slots.lock().unwrap().remove(&id);
        outcome
    }

    fn answered(&self, card_id: &str, answer: ApprovalAnswer) {
        if let Some((_, tx)) = self.slots.lock().unwrap().get(card_id) {
            let _ = tx.send(Some(WaitOutcome::Answered(answer)));
        }
    }

    fn cancel_run(&self, run_id: &str) {
        for (run, tx) in self.slots.lock().unwrap().values() {
            if run == run_id {
                let _ = tx.send(Some(WaitOutcome::Cancelled));
            }
        }
    }
}

/// Fakta permit palsu: bukti hook per `tool_use_id`; pemeriksaan ulang
/// memakai klasifikasi hook domain (tolak atau tanpa keputusan).
#[derive(Clone, Default)]
pub struct FakePermits {
    pub evidence: Arc<Mutex<HashMap<String, String>>>,
}

impl FakePermits {
    pub fn checked(&self, tool_use_id: &str, cwd: &str) {
        self.evidence
            .lock()
            .unwrap()
            .insert(tool_use_id.to_owned(), cwd.to_owned());
    }
}

impl PermitFacts for FakePermits {
    async fn evidence(&self, _permit: &PermitRef, tool_use_id: &str) -> Option<String> {
        self.evidence.lock().unwrap().get(tool_use_id).cloned()
    }

    async fn recheck(
        &self,
        _permit: &PermitRef,
        tool: &str,
        input: &serde_json::Value,
        _cwd: &str,
    ) -> Result<Decision, String> {
        let call = mandor_domain::hook::ToolCall {
            tool_name: tool.to_owned(),
            command: input
                .get("command")
                .and_then(|c| c.as_str())
                .map(str::to_owned),
            ..Default::default()
        };
        Ok(match mandor_domain::hook::classify(&call) {
            mandor_domain::hook::Verdict::Deny(r) => Decision::Deny(r),
            mandor_domain::hook::Verdict::Allow => Decision::Undecided("card".to_owned()),
        })
    }
}

/// Fake kartu pertanyaan (plan 2026-11-07-001, U3).
#[derive(Clone, Default)]
pub struct FakeQuestions {
    pub rows: Arc<Mutex<Vec<mandor_usecase::run_ports::QuestionRecord>>>,
}

impl FakeQuestions {
    pub fn all(&self, run_id: &str) -> Vec<mandor_usecase::run_ports::QuestionRecord> {
        self.rows
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.run_id == run_id)
            .cloned()
            .collect()
    }
}

impl mandor_usecase::run_ports::QuestionStore for FakeQuestions {
    async fn create_question(
        &self,
        q: &mandor_usecase::run_ports::QuestionRecord,
    ) -> Result<bool, PortError> {
        let mut rows = self.rows.lock().unwrap();
        if rows
            .iter()
            .any(|r| r.run_id == q.run_id && r.generation == q.generation)
        {
            return Ok(false);
        }
        rows.push(q.clone());
        Ok(true)
    }

    async fn question(
        &self,
        id: &str,
    ) -> Result<Option<mandor_usecase::run_ports::QuestionRecord>, PortError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|q| q.id == id)
            .cloned())
    }

    async fn questions(
        &self,
        run_id: &str,
    ) -> Result<Vec<mandor_usecase::run_ports::QuestionRecord>, PortError> {
        Ok(self.all(run_id))
    }

    async fn pending_question(
        &self,
        run_id: &str,
    ) -> Result<Option<mandor_usecase::run_ports::QuestionRecord>, PortError> {
        Ok(self
            .all(run_id)
            .into_iter()
            .rev()
            .find(|q| q.status == mandor_domain::question::QuestionStatus::Pending))
    }

    async fn close_question(
        &self,
        id: &str,
        status: mandor_domain::question::QuestionStatus,
        answer: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, PortError> {
        let mut rows = self.rows.lock().unwrap();
        let Some(q) = rows
            .iter_mut()
            .find(|q| q.id == id && q.status == mandor_domain::question::QuestionStatus::Pending)
        else {
            return Ok(false);
        };
        q.status = status;
        q.answer = answer.map(str::to_owned);
        q.closed_ms = Some(now_ms);
        Ok(true)
    }
}
