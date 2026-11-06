//! Alur permintaan izin (U4 plan 2026-11-06-001).

use std::sync::Arc;
use std::time::Duration;

use mandor_domain::permit::{Decision, TurnKind, redact_json, redact_text, request_key};
use serde_json::Value;

use crate::run::RunService;
use crate::run_ports::{
    ApprovalAnswer, ApprovalCard, ApprovalStatus, ApprovalStore, ApprovalWaiter, ChatItem,
    PermitFacts, PermitRef, RunDecision, RunStore, WaitOutcome,
};
use crate::{Deps, Ids};

/// Batas waktu kartu menunggu jawaban (R8).
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Kartu Pending maksimum per run (KTD-A15).
pub const MAX_PENDING: usize = 5;
/// Ukuran input terserialisasi maksimum (KTD-A15).
pub const MAX_INPUT_BYTES: usize = 64 * 1024;

/// Permintaan izin dari agent lewat server MCP `mandor_permit`.
#[derive(Debug, Clone, PartialEq)]
pub struct PermitRequest {
    pub run_id: String,
    pub turn: TurnKind,
    pub permit: PermitRef,
    pub tool: String,
    pub input: Value,
    pub tool_use_id: String,
}

/// Jawaban untuk Claude Code.
#[derive(Debug, Clone, PartialEq)]
pub enum PermitAnswer {
    /// Izinkan dengan input asli tanpa perubahan (R18).
    Allow(Value),
    Deny(String),
}

/// Error jawaban dev.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ApprovalError {
    #[error("approval request not found")]
    NotFound,
    #[error("this approval request is no longer pending")]
    NoLongerPending,
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<crate::PortError> for ApprovalError {
    fn from(e: crate::PortError) -> Self {
        Self::Storage(e.to_string())
    }
}

/// Ringkasan permintaan untuk Chat, tersamarkan (R22, R25).
fn summary(tool: &str, input: &Value) -> String {
    let shown = input
        .get("command")
        .and_then(Value::as_str)
        .map(redact_text)
        .or_else(|| input.get("url").and_then(Value::as_str).map(redact_text))
        .unwrap_or_else(|| redact_json(input).to_string());
    format!("{tool} `{shown}`")
}

impl<D: Deps> RunService<D> {
    /// Memutuskan permintaan izin agent; menunggu jawaban dev bila perlu.
    pub async fn permit_request(self: &Arc<Self>, req: PermitRequest) -> PermitAnswer {
        match self.permit_decide(&req).await {
            Ok(answer) => answer,
            Err(reason) => {
                self.approval_note(
                    &req.run_id,
                    &format!("Denied {}: {reason}", summary(&req.tool, &req.input)),
                )
                .await;
                PermitAnswer::Deny(reason)
            }
        }
    }

    /// `Err(alasan)` untuk penolakan tanpa kartu yang dicatat di Chat.
    async fn permit_decide(self: &Arc<Self>, req: &PermitRequest) -> Result<PermitAnswer, String> {
        if req.turn == TurnKind::Review {
            return Err("the AC reviewer cannot ask for approval".to_owned());
        }
        let Some(cwd) = self.permits.evidence(&req.permit, &req.tool_use_id).await else {
            return Err("Mandor did not finish checking this request".to_owned());
        };
        match self
            .permits
            .recheck(&req.permit, &req.tool, &req.input, &cwd)
            .await
        {
            Ok(Decision::Deny(reason)) => return Err(reason),
            Ok(Decision::Allow | Decision::Undecided(_)) => {}
            Err(e) => return Err(format!("Mandor could not check this request ({e})")),
        }
        if req.input.to_string().len() > MAX_INPUT_BYTES {
            return Err("the request is larger than 64 KiB".to_owned());
        }
        let run_key = request_key(&req.tool, &req.input, &cwd, None);
        let turn_key = request_key(&req.tool, &req.input, &cwd, Some(req.turn));
        let store_err = |e: crate::PortError| format!("approvals are unavailable ({e})");
        if self
            .approvals
            .run_decision(&req.run_id, &run_key)
            .await
            .map_err(store_err)?
            == Some(RunDecision::Deny)
        {
            return Err("you denied this exact request earlier in this run".to_owned());
        }
        if self
            .approvals
            .run_decision(&req.run_id, &turn_key)
            .await
            .map_err(store_err)?
            == Some(RunDecision::Allow)
        {
            return Ok(PermitAnswer::Allow(req.input.clone()));
        }
        if self
            .approvals
            .deny_mode(&req.run_id)
            .await
            .map_err(store_err)?
        {
            return Err(
                "an earlier approval request expired; open this run's Chat tab to resume approvals"
                    .to_owned(),
            );
        }
        if self
            .approvals
            .pending_approvals(&req.run_id)
            .await
            .map_err(store_err)?
            >= MAX_PENDING
        {
            return Err("this run already has 5 approval requests waiting".to_owned());
        }
        self.ask_dev(req, cwd).await
    }

    /// Kartu Pending lalu menunggu jawaban, expired, atau pembatalan.
    async fn ask_dev(
        self: &Arc<Self>,
        req: &PermitRequest,
        cwd: String,
    ) -> Result<PermitAnswer, String> {
        let now = self.now();
        let timeout_ms = i64::try_from(APPROVAL_TIMEOUT.as_millis()).unwrap_or(i64::MAX);
        let card = ApprovalCard {
            id: self.sync.ids.nonce(),
            run_id: req.run_id.clone(),
            tool_use_id: req.tool_use_id.clone(),
            turn: req.turn,
            tool: req.tool.clone(),
            input: req.input.clone(),
            cwd,
            status: ApprovalStatus::Pending,
            answer: None,
            created_ms: now,
            deadline_ms: now.saturating_add(timeout_ms),
            closed_ms: None,
        };
        let ticket = self.waiter.register(&card.id, &card.run_id);
        self.approvals
            .create_approval(&card)
            .await
            .map_err(|e| format!("approvals are unavailable ({e})"))?;
        self.approval_note(
            &req.run_id,
            &format!("Approval requested: {}", summary(&req.tool, &req.input)),
        )
        .await;
        let outcome = self.waiter.wait(ticket, APPROVAL_TIMEOUT).await;
        let answer = match outcome {
            WaitOutcome::Answered(a) => Some(a),
            WaitOutcome::TimedOut => {
                if self
                    .approvals
                    .expire_approval(&card.id, self.now())
                    .await
                    .unwrap_or(false)
                {
                    self.approval_expired(&req.run_id, &card).await;
                    return Err("the approval request expired after 10 minutes".to_owned());
                }
                // Jawaban menang tepat di tenggat.
                self.approvals
                    .approval(&card.id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|c| c.answer)
            }
            WaitOutcome::Cancelled => None,
        };
        match answer {
            Some(ApprovalAnswer::AllowOnce | ApprovalAnswer::AllowRun) => {
                Ok(PermitAnswer::Allow(req.input.clone()))
            }
            Some(ApprovalAnswer::Deny) => Err("the developer denied this request".to_owned()),
            None => Err("the run stopped before the request was answered".to_owned()),
        }
    }

    async fn approval_expired(&self, run_id: &str, card: &ApprovalCard) {
        let was_on = self.approvals.deny_mode(run_id).await.unwrap_or(false);
        if let Err(e) = self.approvals.set_deny_mode(run_id, true, self.now()).await {
            tracing::warn!(error = %e, run_id, "mode tolak langsung tidak tersimpan");
        }
        if !was_on {
            self.approval_note(
                run_id,
                &format!(
                    "Approval for {} expired. Later approval requests in this run are denied until you open this Chat tab.",
                    summary(&card.tool, &card.input)
                ),
            )
            .await;
        }
    }

    /// Jawaban dev dari koneksi aplikasi (R18).
    ///
    /// # Errors
    /// [`ApprovalError::NotFound`] bila kartu bukan milik run;
    /// [`ApprovalError::NoLongerPending`] bila sudah tidak menunggu.
    pub async fn answer_approval(
        &self,
        run_id: &str,
        card_id: &str,
        answer: ApprovalAnswer,
    ) -> Result<(), ApprovalError> {
        let card = self
            .approvals
            .approval(card_id)
            .await?
            .filter(|c| c.run_id == run_id)
            .ok_or(ApprovalError::NotFound)?;
        if card.status != ApprovalStatus::Pending {
            return Err(ApprovalError::NoLongerPending);
        }
        let grant = request_key(&card.tool, &card.input, &card.cwd, Some(card.turn));
        let deny = request_key(&card.tool, &card.input, &card.cwd, None);
        if !self
            .approvals
            .answer_approval(card_id, answer, &grant, &deny, self.now())
            .await?
        {
            return Err(ApprovalError::NoLongerPending);
        }
        self.waiter.answered(card_id, answer);
        let verb = match answer {
            ApprovalAnswer::Deny => "Denied",
            ApprovalAnswer::AllowOnce => "Allowed once",
            ApprovalAnswer::AllowRun => "Allowed for this run",
        };
        self.approval_note(
            run_id,
            &format!("{verb}: {}", summary(&card.tool, &card.input)),
        )
        .await;
        Ok(())
    }

    /// Dev membuka tab Chat run: mode tolak langsung berakhir (R19).
    ///
    /// # Errors
    /// [`ApprovalError::Storage`] bila penyimpanan gagal.
    pub async fn approvals_seen(&self, run_id: &str) -> Result<(), ApprovalError> {
        if !self.approvals.deny_mode(run_id).await? {
            return Ok(());
        }
        self.approvals
            .set_deny_mode(run_id, false, self.now())
            .await?;
        self.approval_note(run_id, "Approval requests resumed.")
            .await;
        Ok(())
    }

    /// Kartu run, urut dari yang terlama (R24).
    ///
    /// # Errors
    /// [`ApprovalError::Storage`] bila penyimpanan gagal.
    pub async fn list_approvals(&self, run_id: &str) -> Result<Vec<ApprovalCard>, ApprovalError> {
        Ok(self.approvals.approvals(run_id).await?)
    }

    /// Run berhenti, ditutup, atau gilirannya selesai: kartu Pending batal.
    pub async fn cancel_run_approvals(&self, run_id: &str) {
        match self
            .approvals
            .cancel_approvals(Some(run_id), self.now())
            .await
        {
            Ok(ids) if !ids.is_empty() => {
                self.approval_note(run_id, "Waiting approval requests were cancelled.")
                    .await;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, run_id, "kartu persetujuan tidak dibatalkan"),
        }
        self.waiter.cancel_run(run_id);
    }

    /// Daemon start: kartu Pending dari proses sebelumnya batal.
    ///
    /// # Errors
    /// [`ApprovalError::Storage`] bila penyimpanan gagal.
    pub async fn cancel_stale_approvals(&self) -> Result<usize, ApprovalError> {
        Ok(self
            .approvals
            .cancel_approvals(None, self.now())
            .await?
            .len())
    }

    async fn approval_note(&self, run_id: &str, text: &str) {
        if let Err(e) = self
            .runs
            .append(run_id, &ChatItem::System(text.to_owned()), self.now())
            .await
        {
            tracing::warn!(error = %e, run_id, "catatan persetujuan tidak tersimpan");
        }
    }
}
