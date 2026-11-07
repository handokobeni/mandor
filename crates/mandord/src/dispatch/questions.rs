//! Kartu pertanyaan agent (U5 plan 2026-11-07-001): alat `ask_developer` di
//! koneksi kapabilitas, serta daftar, jawaban, dan antrean yang ditahan untuk
//! aplikasi dev (hanya koneksi token penuh).

use mandor_domain::question::{QuestionDraft, QuestionOption};
use mandor_protocol::{
    AskToolQuestion, ErrorCode, QuestionAnswerParams, QuestionOptionView, QuestionView, RpcError,
    RunRef, method,
};
use mandor_usecase::AskError;
use mandor_usecase::run_ports::QuestionRecord;
use serde_json::{Value, json};

use super::review::usecase_scope;
use super::{AppDispatch, ok, params, run_error};

pub(crate) fn question_view(q: &QuestionRecord) -> QuestionView {
    QuestionView {
        id: q.id.clone(),
        run_id: q.run_id.clone(),
        question: q.question.clone(),
        options: q
            .options
            .iter()
            .map(|o| QuestionOptionView {
                label: o.label.clone(),
                description: o.description.clone(),
                recommended: o.recommended,
            })
            .collect(),
        status: q.status.as_str().to_owned(),
        answer: q.answer.clone(),
        created_at_ms: q.created_ms,
        closed_at_ms: q.closed_ms,
    }
}

fn draft(p: AskToolQuestion) -> QuestionDraft {
    QuestionDraft {
        question: p.question,
        options: p
            .options
            .into_iter()
            .map(|o| QuestionOption {
                label: o.label,
                description: o.description,
                recommended: o.recommended,
            })
            .collect(),
    }
}

fn ask_error(e: &AskError) -> RpcError {
    let code = match e {
        AskError::NotAllowed => ErrorCode::Unauthorized,
        AskError::NotAsking | AskError::AlreadyAsked => ErrorCode::Conflict,
        AskError::Invalid(_) => ErrorCode::InvalidParams,
        AskError::Storage(_) => ErrorCode::Internal,
    };
    RpcError::new(code, e.to_string())
}

impl AppDispatch {
    /// `ask.tool.ask` dari koneksi kapabilitas peran ask atau design.
    pub(super) async fn ask_tool_call(
        &self,
        scope: &mandor_api::CapabilityScope,
        v: Value,
    ) -> Result<Value, RpcError> {
        let scope = usecase_scope(scope)
            .ok_or_else(|| RpcError::new(ErrorCode::Unauthorized, "unknown capability role"))?;
        let p: AskToolQuestion = params(v)?;
        let s = self.session().await?;
        let reply = s
            .runs
            .ask_question(&scope, &draft(p))
            .await
            .map_err(|e| ask_error(&e))?;
        ok(&reply)
    }

    /// `questions.list`, `questions.answer`, `runs.queued.send`, dan
    /// `runs.queued.discard`.
    pub(super) async fn questions_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let s = self.session().await?;
        match name {
            method::QUESTIONS_ANSWER => {
                let p: QuestionAnswerParams = params(v)?;
                let option = usize::try_from(p.option).unwrap_or(usize::MAX);
                s.runs
                    .answer_question(&p.question_id, option)
                    .await
                    .map_err(run_error)?;
            }
            method::RUNS_QUEUED_SEND => {
                let p: RunRef = params(v)?;
                s.runs.send_queued(&p.run_id).await.map_err(run_error)?;
            }
            method::RUNS_QUEUED_DISCARD => {
                let p: RunRef = params(v)?;
                s.runs.discard_queued(&p.run_id).await.map_err(run_error)?;
            }
            _ => {
                let p: RunRef = params(v)?;
                let list = s.runs.questions(&p.run_id).await.map_err(run_error)?;
                return ok(&list.iter().map(question_view).collect::<Vec<_>>());
            }
        }
        ok(&json!({}))
    }

    pub(super) fn is_questions_method(name: &str) -> bool {
        matches!(
            name,
            method::QUESTIONS_LIST
                | method::QUESTIONS_ANSWER
                | method::RUNS_QUEUED_SEND
                | method::RUNS_QUEUED_DISCARD
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandor_domain::question::QuestionStatus;
    use pretty_assertions::assert_eq;

    #[test]
    fn view_membawa_pilihan_dan_status() {
        let q = QuestionRecord {
            id: "q-1".to_owned(),
            run_id: "run1".to_owned(),
            generation: 2,
            question: "Keep?".to_owned(),
            options: vec![QuestionOption {
                label: "Yes".to_owned(),
                description: "keep".to_owned(),
                recommended: true,
            }],
            status: QuestionStatus::AnsweredInChat,
            answer: Some("pakai dua".to_owned()),
            created_ms: 5,
            closed_ms: Some(9),
        };
        let v = question_view(&q);
        assert_eq!(v.status, "answered_in_chat");
        assert_eq!(v.options[0].label, "Yes");
        assert!(v.options[0].recommended);
        assert_eq!(v.closed_at_ms, Some(9));
    }

    #[test]
    fn error_alat_dipetakan_ke_kode_rpc() {
        assert_eq!(ask_error(&AskError::AlreadyAsked).code, ErrorCode::Conflict);
        assert_eq!(
            ask_error(&AskError::NotAllowed).code,
            ErrorCode::Unauthorized
        );
    }
}
