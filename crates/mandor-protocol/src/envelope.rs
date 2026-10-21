use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Permintaan client → daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Request {
    pub jsonrpc: String,
    #[ts(type = "number")]
    pub id: u64,
    pub method: String,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
}

impl Request {
    #[must_use]
    pub fn new(id: u64, method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id,
            method: method.to_owned(),
            params,
        }
    }
}

/// Jawaban daemon → client. Tepat satu dari `result` atau `error` terisi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Response {
    pub jsonrpc: String,
    #[ts(type = "number")]
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown", optional)]
    pub result: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<RpcError>,
}

impl Response {
    #[must_use]
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(result),
            error: None,
        }
    }

    #[must_use]
    pub fn err(id: u64, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// Event daemon → client tanpa `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
}

impl Notification {
    #[must_use]
    pub fn new(method: &str, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            method: method.to_owned(),
            params,
        }
    }
}

/// Kode error yang stabil; client mencocokkan kode, bukan pesan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    Unauthorized,
    VersionMismatch,
    NotFound,
    Conflict,
    Rejected,
    Internal,
}

/// Error terstruktur. `message` ditujukan untuk manusia dan tidak pernah memuat detail internal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown", optional)]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

/// Pesan masuk yang belum diketahui jenisnya (dipakai sisi client/relay).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Incoming {
    Response(Response),
    Notification(Notification),
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn response_ok_tidak_menyertakan_error() {
        let v = serde_json::to_value(Response::ok(1, json!({"a": 1}))).unwrap();
        assert_eq!(v, json!({"jsonrpc": "2.0", "id": 1, "result": {"a": 1}}));
    }

    #[test]
    fn response_err_memakai_kode_snake_case() {
        let v = serde_json::to_value(Response::err(
            2,
            RpcError::new(ErrorCode::MethodNotFound, "missing"),
        ))
        .unwrap();
        assert_eq!(
            v,
            json!({"jsonrpc": "2.0", "id": 2, "error": {"code": "method_not_found", "message": "missing"}})
        );
    }

    #[test]
    fn incoming_membedakan_response_dan_notification() {
        let r: Incoming =
            serde_json::from_value(json!({"jsonrpc":"2.0","id":3,"result":null})).unwrap();
        assert!(matches!(r, Incoming::Response(_)));
        let n: Incoming =
            serde_json::from_value(json!({"jsonrpc":"2.0","method":"card.updated","params":{}}))
                .unwrap();
        assert!(matches!(n, Incoming::Notification(_)));
    }
}
