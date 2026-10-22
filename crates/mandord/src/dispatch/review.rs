//! Method Rilis 2: buku AC (`ledger.*`).

use mandor_domain::AcRow;
use mandor_protocol::{AcLedgerView, AcRowView, RpcError, RunRef, method};
use serde_json::Value;

use super::{AppDispatch, ok, params, run_error};

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
