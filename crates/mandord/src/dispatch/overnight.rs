//! Method lembur VM (plan VM): keputusan gerbang AC dan Send to review.

use mandor_protocol::{ErrorCode, RpcError, RunRef, method};
use serde_json::{Value, json};

use super::{AppDispatch, ok, params, run_error};

impl AppDispatch {
    pub(super) fn is_overnight_method(name: &str) -> bool {
        [
            method::OVERNIGHT_GATE_DECIDE,
            method::OVERNIGHT_SEND_TO_REVIEW,
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
            other => Err(RpcError::new(
                ErrorCode::MethodNotFound,
                format!("unknown method: {other}"),
            )),
        }
    }
}
