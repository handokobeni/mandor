//! Proses `mandord mcp --permit`: alat persetujuan Claude Code
//! (`--permission-prompt-tool`) yang meneruskan permintaan izin ke daemon
//! (plan 2026-11-06-001, KTD-A10).
//!
//! Setiap `tools/call` berjalan sebagai task di [`JoinSet`] milik sesi dengan
//! koneksi daemon sendiri, dan satu task penulis memegang stdout, sehingga
//! beberapa kartu bisa menunggu bersamaan (R24). Error apa pun menjadi error
//! alat, yang ditolak Claude Code (S3b).

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use mandor_protocol::{PermitDecision, PermitRequestParams, RpcError, method};
use secrecy::SecretString;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::client::DaemonClient;
use crate::relay::RelayError;

/// Batas waktu satu permintaan izin: tenggat kartu 10 menit ditambah jeda.
pub const PERMIT_CALL_TIMEOUT: Duration = Duration::from_secs(11 * 60);
/// Versi protokol MCP bila client tidak menyebutkan.
const DEFAULT_MCP_VERSION: &str = "2025-06-18";
/// Balasan yang menunggu ditulis.
const OUTBOUND: usize = 64;

/// Pemanggil `permit.request` ke daemon.
pub trait PermitRpc: Send + Sync + 'static {
    fn request(&self, params: Value) -> impl Future<Output = Result<Value, RpcError>> + Send;
}

/// Koneksi daemon baru untuk setiap permintaan izin.
#[derive(Debug)]
pub struct DaemonPermit {
    addr: SocketAddr,
    token: SecretString,
}

impl DaemonPermit {
    #[must_use]
    pub fn new(addr: SocketAddr, token: SecretString) -> Self {
        Self { addr, token }
    }
}

impl PermitRpc for DaemonPermit {
    async fn request(&self, params: Value) -> Result<Value, RpcError> {
        DaemonClient::new(self.addr, self.token.clone(), "mandord-permit")
            .request_with_timeout(method::PERMIT_REQUEST, params, PERMIT_CALL_TIMEOUT)
            .await
    }
}

/// Melayani alat persetujuan dari `input` ke `output` sampai input habis.
///
/// # Errors
/// [`RelayError`] bila I/O stdio gagal.
pub async fn permit_lines<P, R, W>(rpc: Arc<P>, input: R, output: W) -> Result<(), RelayError>
where
    P: PermitRpc,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<String>(OUTBOUND);
    let writer = tokio::spawn(write_lines(rx, output));
    let mut calls = JoinSet::new();
    let mut lines = BufReader::new(input).lines();
    let read = loop {
        while calls.try_join_next().is_some() {}
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break Ok(()),
            Err(e) => break Err(RelayError::Io(e.to_string())),
        };
        if line.trim().is_empty() {
            continue;
        }
        match handle(&line) {
            Handled::Reply(reply) => {
                if tx.send(reply).await.is_err() {
                    break Err(RelayError::Io("stdout closed".to_owned()));
                }
            }
            Handled::Call { id, params } => {
                let rpc = Arc::clone(&rpc);
                let tx = tx.clone();
                calls.spawn(async move {
                    let result = call(rpc.as_ref(), &params).await;
                    let reply = json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string();
                    let _ = tx.send(reply).await;
                });
            }
            Handled::Ignore => {}
        }
    };
    // Agent menutup stdin: tidak ada lagi yang menunggu jawaban.
    calls.shutdown().await;
    drop(tx);
    let written = writer.await.map_err(|e| RelayError::Io(e.to_string()))?;
    read.and(written)
}

async fn write_lines<W: AsyncWrite + Unpin>(
    mut rx: mpsc::Receiver<String>,
    mut output: W,
) -> Result<(), RelayError> {
    while let Some(reply) = rx.recv().await {
        let io = |e: std::io::Error| RelayError::Io(e.to_string());
        output
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .map_err(io)?;
        output.flush().await.map_err(io)?;
    }
    Ok(())
}

enum Handled {
    Reply(String),
    Call { id: Value, params: Value },
    Ignore,
}

fn handle(line: &str) -> Handled {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Handled::Reply(error(&Value::Null, -32700, "parse error")),
    };
    let Some(id) = msg.get("id").cloned() else {
        return Handled::Ignore;
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match msg
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "initialize" => initialize(&params),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": [permit_tool()] }),
        "tools/call" => return Handled::Call { id, params },
        other => return Handled::Reply(error(&id, -32601, &format!("method not found: {other}"))),
    };
    Handled::Reply(json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string())
}

/// Hasil `tools/call`; izin selalu memakai input asli (R18).
async fn call<P: PermitRpc>(rpc: &P, params: &Value) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name != "permit" {
        return tool_error(&format!("unknown tool: {name}"));
    }
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let request: PermitRequestParams = match serde_json::from_value(args) {
        Ok(r) => r,
        Err(e) => return tool_error(&format!("invalid arguments: {e}")),
    };
    let original = request.input.clone();
    let body = match serde_json::to_value(&request) {
        Ok(v) => v,
        Err(e) => return tool_error(&e.to_string()),
    };
    let decision = match rpc.request(body).await {
        Ok(v) => serde_json::from_value::<PermitDecision>(v),
        Err(e) => return tool_error(&e.message),
    };
    let answer = match decision {
        Ok(PermitDecision::Allow { .. }) => PermitDecision::Allow {
            updated_input: original,
        },
        Ok(deny @ PermitDecision::Deny { .. }) => deny,
        Err(e) => return tool_error(&format!("unexpected daemon answer: {e}")),
    };
    match serde_json::to_string(&answer) {
        Ok(text) => json!({"content": [{"type": "text", "text": text}], "isError": false}),
        Err(e) => tool_error(&e.to_string()),
    }
}

fn initialize(params: &Value) -> Value {
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_MCP_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "mandor_permit", "version": env!("CARGO_PKG_VERSION")},
    })
}

fn permit_tool() -> Value {
    json!({
        "name": "permit",
        "description": "Ask the developer through Mandor whether a tool call may run.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "tool_name": {"type": "string"},
                "input": {"type": "object"},
                "tool_use_id": {"type": "string"},
            },
            "required": ["tool_name", "input", "tool_use_id"],
        },
    })
}

fn tool_error(message: &str) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

fn error(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use mandor_protocol::ErrorCode;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::sync::Semaphore;

    use super::*;

    /// Daemon palsu: setiap permintaan menunggu sampai dilepas test.
    struct Fake {
        seen: Mutex<Vec<Value>>,
        release: Semaphore,
    }

    impl Default for Fake {
        fn default() -> Self {
            Self {
                seen: Mutex::new(Vec::new()),
                release: Semaphore::new(0),
            }
        }
    }

    impl PermitRpc for Fake {
        async fn request(&self, params: Value) -> Result<Value, RpcError> {
            let cmd = params["input"]["command"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(params);
            let _permit = self.release.acquire().await;
            match cmd.as_str() {
                "deny" => {
                    Ok(json!({"behavior": "deny", "message": "the developer denied this request"}))
                }
                "error" => Err(RpcError::new(ErrorCode::Internal, "daemon unreachable")),
                // Daemon yang mengubah input tidak bisa mengubah perintah (R18).
                _ => Ok(json!({"behavior": "allow", "updatedInput": {"command": "rm -rf /"}})),
            }
        }
    }

    fn call_line(id: u64, cmd: &str) -> String {
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"permit",
            "arguments":{"tool_name":"Bash","input":{"command":cmd},"tool_use_id":format!("t{id}")}}})
        .to_string()
    }

    #[tokio::test]
    async fn panggilan_bersamaan_dan_jawaban_dalam_bentuk_alat_persetujuan() {
        let fake = Arc::new(Fake::default());
        let (mut agent_in, server_in) = tokio::io::duplex(64 * 1024);
        let (server_out, agent_out) = tokio::io::duplex(64 * 1024);
        let server = tokio::spawn(permit_lines(Arc::clone(&fake), server_in, server_out));
        let mut replies = BufReader::new(agent_out).lines();

        let list = json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}).to_string();
        for line in [
            list,
            call_line(1, "jq . a.json"),
            call_line(2, "deny"),
            call_line(3, "error"),
        ] {
            agent_in
                .write_all(format!("{line}\n").as_bytes())
                .await
                .unwrap();
        }
        let tools: Value =
            serde_json::from_str(&replies.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(tools["result"]["tools"][0]["name"], "permit");
        // Ketiga panggilan sampai di daemon sebelum satu pun dijawab.
        // Batas longgar: test berjalan di bawah instrumentasi coverage.
        for _ in 0..2_000 {
            if fake.seen.lock().unwrap().len() == 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(fake.seen.lock().unwrap().len(), 3);
        fake.release.add_permits(3);

        let mut by_id = std::collections::HashMap::new();
        for _ in 0..3 {
            let v: Value =
                serde_json::from_str(&replies.next_line().await.unwrap().unwrap()).unwrap();
            by_id.insert(v["id"].as_u64().unwrap(), v["result"].clone());
        }
        let allow: Value =
            serde_json::from_str(by_id[&1]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            allow,
            json!({"behavior": "allow", "updatedInput": {"command": "jq . a.json"}})
        );
        let deny: Value =
            serde_json::from_str(by_id[&2]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(deny["behavior"], "deny");
        assert_eq!(by_id[&3]["isError"], true);

        drop(agent_in);
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn argumen_rusak_dan_alat_lain_menjadi_error_alat() {
        let fake = Fake::default();
        fake.release.add_permits(10);
        let bad = call(
            &fake,
            &json!({"name": "permit", "arguments": {"tool_name": "Bash"}}),
        )
        .await;
        assert_eq!(bad["isError"], true);
        let other = call(&fake, &json!({"name": "rm", "arguments": {}})).await;
        assert_eq!(other["isError"], true);
        assert!(matches!(handle("{rusak"), Handled::Reply(r) if r.contains("-32700")));
        assert!(matches!(
            handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            Handled::Ignore
        ));
    }
}
