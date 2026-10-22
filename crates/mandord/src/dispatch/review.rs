//! Method Rilis 2: buku AC (`ledger.*`).

use mandor_domain::{AcRow, Design, Verdict};
use mandor_protocol::{
    AcLedgerView, AcRowView, ErrorCode, LedgerToolInfo, LedgerToolSetDesign, LedgerToolSetVerdict,
    RpcError, RunRef, method,
};
use mandor_usecase::review::{CapabilityRole, CapabilityScope, LedgerToolError};
use serde_json::Value;

use super::{AppDispatch, ok, params, run_error};

/// Satu panggilan tool ledger yang sudah diurai dari koneksi kapabilitas.
#[derive(Debug, PartialEq)]
pub(super) enum LedgerTool {
    Info,
    Read,
    SetDesign {
        ac_id: String,
        design: Design,
        note: Option<String>,
    },
    SetVerdict {
        ac_id: String,
        verdict: Verdict,
        evidence: Option<String>,
    },
}

/// Mengurai method dan parameter tool ledger; method lain ditolak.
pub(super) fn ledger_tool(name: &str, v: Value) -> Result<LedgerTool, RpcError> {
    match name {
        method::LEDGER_TOOL_INFO => Ok(LedgerTool::Info),
        method::LEDGER_TOOL_READ => Ok(LedgerTool::Read),
        method::LEDGER_TOOL_SET_DESIGN => {
            let p: LedgerToolSetDesign = params(v)?;
            let design = Design::parse(&p.design).ok_or_else(|| invalid("design", &p.design))?;
            Ok(LedgerTool::SetDesign {
                ac_id: p.ac_id,
                design,
                note: p.note,
            })
        }
        method::LEDGER_TOOL_SET_VERDICT => {
            let p: LedgerToolSetVerdict = params(v)?;
            let verdict =
                Verdict::parse(&p.verdict).ok_or_else(|| invalid("verdict", &p.verdict))?;
            Ok(LedgerTool::SetVerdict {
                ac_id: p.ac_id,
                verdict,
                evidence: p.evidence,
            })
        }
        other => Err(RpcError::new(
            ErrorCode::Unauthorized,
            format!("{other} is not available on this connection"),
        )),
    }
}

fn invalid(field: &str, value: &str) -> RpcError {
    RpcError::new(
        ErrorCode::InvalidParams,
        format!("unknown {field} value: {value}"),
    )
}

fn tool_error(e: &LedgerToolError) -> RpcError {
    let code = match e {
        LedgerToolError::WrongRole => ErrorCode::Unauthorized,
        LedgerToolError::UnknownAc(_) => ErrorCode::NotFound,
        LedgerToolError::NotAllowed(_) => ErrorCode::Rejected,
        LedgerToolError::Storage(_) => ErrorCode::Internal,
    };
    RpcError::new(code, e.to_string())
}

fn api_scope(s: &CapabilityScope) -> mandor_api::CapabilityScope {
    mandor_api::CapabilityScope {
        run_id: s.run_id.clone(),
        role: s.role.as_str().to_owned(),
    }
}

fn usecase_scope(s: &mandor_api::CapabilityScope) -> Option<CapabilityScope> {
    let role = match s.role.as_str() {
        "design" => CapabilityRole::Design,
        "verdict" => CapabilityRole::Verdict,
        _ => return None,
    };
    Some(CapabilityScope {
        run_id: s.run_id.clone(),
        role,
    })
}

pub(super) fn ac_row_view(r: &AcRow) -> AcRowView {
    AcRowView {
        ac_id: r.ac_id.clone(),
        label: r.label.clone(),
        text: r.text.clone(),
        design: r.design.as_str().to_owned(),
        design_note: r.design_note.clone(),
        verdict: r.verdict.as_str().to_owned(),
        evidence: r.evidence.clone(),
        stale: r.stale,
        status: r.status.as_str().to_owned(),
        reason: r.reason.clone(),
        decided_by: r.decided_by.clone(),
        in_gate: r.in_gate(),
        selectable: r.selectable(),
    }
}

impl AppDispatch {
    pub(super) async fn capability(&self, token: &str) -> Option<mandor_api::CapabilityScope> {
        let s = self.app.session().await?;
        s.runs.verify_capability(token).as_ref().map(api_scope)
    }

    pub(super) async fn ledger_tool_call(
        &self,
        scope: &mandor_api::CapabilityScope,
        name: &str,
        v: Value,
    ) -> Result<Value, RpcError> {
        let tool = ledger_tool(name, v)?;
        let scope = usecase_scope(scope)
            .ok_or_else(|| RpcError::new(ErrorCode::Unauthorized, "unknown capability role"))?;
        let s = self.session().await?;
        let rows = match tool {
            LedgerTool::Info => {
                return ok(&LedgerToolInfo {
                    run_id: scope.run_id,
                    role: scope.role.as_str().to_owned(),
                });
            }
            LedgerTool::Read => s.runs.ledger_read(&scope).await,
            LedgerTool::SetDesign {
                ac_id,
                design,
                note,
            } => {
                s.runs
                    .ledger_set_design(&scope, &ac_id, design, note.as_deref())
                    .await
            }
            LedgerTool::SetVerdict {
                ac_id,
                verdict,
                evidence,
            } => {
                s.runs
                    .ledger_set_verdict(&scope, &ac_id, verdict, evidence.as_deref())
                    .await
            }
        }
        .map_err(|e| tool_error(&e))?;
        ok(&rows.iter().map(ac_row_view).collect::<Vec<_>>())
    }

    pub(super) fn is_review_method(name: &str) -> bool {
        [method::LEDGER_GET].contains(&name)
    }

    pub(super) async fn review_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let s = self.session().await?;
        match name {
            method::LEDGER_GET => {
                let p: RunRef = params(v)?;
                let r = s.runs.refresh_ledger(&p.run_id).await.map_err(run_error)?;
                ok(&AcLedgerView {
                    run_id: p.run_id,
                    rows: r.rows.iter().map(ac_row_view).collect(),
                    synced_at_ms: r.synced_at_ms,
                    sync_error: r.error,
                    design_sources: r.design_sources,
                })
            }
            _ => Err(RpcError::new(
                mandor_protocol::ErrorCode::MethodNotFound,
                format!("unknown method: {name}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use mandor_domain::{Design, RowStatus, Verdict};

    use super::*;

    #[test]
    fn tool_ledger_diurai_dan_method_lain_ditolak() {
        let t = ledger_tool(
            method::LEDGER_TOOL_SET_VERDICT,
            serde_json::json!({"ac_id": "rec1", "verdict": "cant_verify", "evidence": "x"}),
        )
        .unwrap();
        assert_eq!(
            t,
            LedgerTool::SetVerdict {
                ac_id: "rec1".to_owned(),
                verdict: Verdict::CantVerify,
                evidence: Some("x".to_owned()),
            }
        );
        let t = ledger_tool(
            method::LEDGER_TOOL_SET_DESIGN,
            serde_json::json!({"ac_id": "rec1", "design": "in_design"}),
        )
        .unwrap();
        assert!(matches!(
            t,
            LedgerTool::SetDesign {
                design: Design::InDesign,
                note: None,
                ..
            }
        ));
        let bad = ledger_tool(
            method::LEDGER_TOOL_SET_VERDICT,
            serde_json::json!({"ac_id": "rec1", "verdict": "lulus"}),
        )
        .unwrap_err();
        assert_eq!(bad.code, ErrorCode::InvalidParams);
        let denied = ledger_tool(method::STATUS, Value::Null).unwrap_err();
        assert_eq!(denied.code, ErrorCode::Unauthorized);
        assert_eq!(
            ledger_tool(method::LEDGER_TOOL_READ, Value::Null).unwrap(),
            LedgerTool::Read
        );
    }

    #[test]
    fn lingkup_kapabilitas_bolak_balik_dan_peran_asing_ditolak() {
        let s = CapabilityScope {
            run_id: "r1".to_owned(),
            role: CapabilityRole::Design,
        };
        assert_eq!(usecase_scope(&api_scope(&s)), Some(s));
        let odd = mandor_api::CapabilityScope {
            run_id: "r1".to_owned(),
            role: "admin".to_owned(),
        };
        assert_eq!(usecase_scope(&odd), None);
        assert_eq!(
            tool_error(&LedgerToolError::WrongRole).code,
            ErrorCode::Unauthorized
        );
        assert_eq!(
            tool_error(&LedgerToolError::UnknownAc("x".to_owned())).code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn baris_follow_design_tidak_ikut_gerbang_dan_fail_bisa_dipilih() {
        let mut r = AcRow::new("rec1", Some("AC-1"), "Nama tampil", RowStatus::New);
        r.verdict = Verdict::Fail;
        let v = ac_row_view(&r);
        assert_eq!(v.verdict, "fail");
        assert_eq!(v.status, "new");
        assert!(v.in_gate && v.selectable);
        r.design = Design::FollowDesign;
        let v = ac_row_view(&r);
        assert_eq!(v.design, "follow_design");
        assert!(!v.in_gate && !v.selectable);
    }
}
