//! Kartu persetujuan untuk aplikasi dev (U6 plan 2026-11-06-001): daftar,
//! jawaban, dan tanda tab Chat dibuka. Hanya koneksi token penuh yang sampai
//! ke sini (KTD-A16); koneksi terbatas ditolak di `call_scoped`.

use mandor_domain::permit::redact_json;
use mandor_protocol::{ApprovalAnswerParams, ApprovalView, ErrorCode, RpcError, RunRef};
use mandor_usecase::approval::ApprovalError;
use mandor_usecase::run_ports::{ApprovalAnswer, ApprovalCard, ApprovalStatus};
use serde_json::Value;

use super::{AppDispatch, ok, params};

/// Kartu untuk aplikasi; input lengkap hanya selama Pending (R25).
pub(crate) fn approval_view(c: &ApprovalCard) -> ApprovalView {
    let input = if c.status == ApprovalStatus::Pending {
        c.input.clone()
    } else {
        redact_json(&c.input)
    };
    ApprovalView {
        id: c.id.clone(),
        run_id: c.run_id.clone(),
        turn: c.turn.as_str().to_owned(),
        tool: c.tool.clone(),
        description: input
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        input,
        cwd: c.cwd.clone(),
        status: c.status.as_str().to_owned(),
        answer: c.answer.map(|a| a.as_str().to_owned()),
        created_at_ms: c.created_ms,
        deadline_ms: c.deadline_ms,
        closed_at_ms: c.closed_ms,
    }
}

fn approval_error(e: &ApprovalError) -> RpcError {
    let code = match e {
        ApprovalError::NotFound => ErrorCode::NotFound,
        ApprovalError::NoLongerPending => ErrorCode::Rejected,
        ApprovalError::Storage(_) => ErrorCode::Internal,
    };
    RpcError::new(code, e.to_string())
}

impl AppDispatch {
    /// `approvals.list`, `approvals.answer`, dan `approvals.seen`.
    pub(super) async fn approvals_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        match name {
            mandor_protocol::method::APPROVALS_ANSWER => self.approvals_answer(v).await,
            mandor_protocol::method::APPROVALS_SEEN => self.approvals_seen(v).await,
            _ => self.approvals_list(v).await,
        }
    }

    async fn approvals_list(&self, v: Value) -> Result<Value, RpcError> {
        let p: RunRef = params(v)?;
        let s = self.session().await?;
        let cards = s
            .runs
            .list_approvals(&p.run_id)
            .await
            .map_err(|e| approval_error(&e))?;
        ok(&cards.iter().map(approval_view).collect::<Vec<_>>())
    }

    async fn approvals_answer(&self, v: Value) -> Result<Value, RpcError> {
        let p: ApprovalAnswerParams = params(v)?;
        let answer = ApprovalAnswer::parse(&p.answer)
            .ok_or_else(|| RpcError::new(ErrorCode::InvalidParams, "unknown approval answer"))?;
        let s = self.session().await?;
        s.runs
            .answer_approval(&p.run_id, &p.approval_id, answer)
            .await
            .map_err(|e| approval_error(&e))?;
        ok(&serde_json::json!({}))
    }

    async fn approvals_seen(&self, v: Value) -> Result<Value, RpcError> {
        let p: RunRef = params(v)?;
        let s = self.session().await?;
        s.runs
            .approvals_seen(&p.run_id)
            .await
            .map_err(|e| approval_error(&e))?;
        ok(&serde_json::json!({}))
    }

    /// Koneksi terbatas mencoba memakai method kartu (AE16): dicatat di Chat
    /// run token itu lalu ditolak.
    pub(super) async fn approvals_refused(
        &self,
        scope: &mandor_api::CapabilityScope,
        name: &str,
    ) -> RpcError {
        if let Ok(s) = self.session().await {
            s.runs
                .approval_answer_refused(&scope.run_id, &scope.role)
                .await;
        }
        RpcError::new(
            ErrorCode::Unauthorized,
            format!("{name} is not available on this connection"),
        )
    }
}

#[cfg(test)]
mod tests {
    use mandor_domain::permit::TurnKind;
    use serde_json::json;

    use super::*;

    fn card(status: ApprovalStatus) -> ApprovalCard {
        ApprovalCard {
            id: "a1".to_owned(),
            run_id: "r1".to_owned(),
            tool_use_id: "t1".to_owned(),
            turn: TurnKind::Implement,
            tool: "Bash".to_owned(),
            input: json!({"command": "curl -H 'Authorization: Bearer rahasia' x",
                "description": "Ambil data"}),
            cwd: "/w".to_owned(),
            status,
            answer: None,
            created_ms: 1,
            deadline_ms: 600_001,
            closed_ms: None,
        }
    }

    #[test]
    fn input_lengkap_hanya_selama_pending() {
        let pending = approval_view(&card(ApprovalStatus::Pending));
        assert!(pending.input.to_string().contains("rahasia"));
        assert_eq!(pending.description.as_deref(), Some("Ambil data"));
        assert_eq!(pending.deadline_ms, 600_001);
        let answered = approval_view(&card(ApprovalStatus::Answered));
        assert!(
            !answered.input.to_string().contains("rahasia"),
            "{}",
            answered.input
        );
        assert_eq!(answered.status, "answered");
    }

    #[test]
    fn error_jawaban_menjadi_kode_rpc() {
        assert_eq!(
            approval_error(&ApprovalError::NotFound).code,
            ErrorCode::NotFound
        );
        let late = approval_error(&ApprovalError::NoLongerPending);
        assert_eq!(late.code, ErrorCode::Rejected);
        assert!(late.message.contains("no longer pending"));
    }
}
