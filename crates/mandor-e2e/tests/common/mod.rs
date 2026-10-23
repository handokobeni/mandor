//! Helper bersama smoke test: konfigurasi dari environment, relay
//! `mandord connect`, dan penantian berbatas waktu.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    dead_code
)]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// Konfigurasi smoke dari environment.
pub struct Env {
    pub bin: PathBuf,
    pub home: PathBuf,
    pub record: String,
    pub repo: String,
}

pub fn env() -> Option<Env> {
    if std::env::var("MANDOR_E2E").ok().as_deref() != Some("1") {
        eprintln!("lewati: set MANDOR_E2E=1 untuk menjalankan smoke test");
        return None;
    }
    let var = |k: &str| {
        std::env::var(k).unwrap_or_else(|_| panic!("{k} wajib diisi (lihat README mandor-e2e)"))
    };
    let bin = std::env::var("MANDOR_E2E_BIN").map_or_else(
        |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mandord"),
        PathBuf::from,
    );
    Some(Env {
        bin,
        home: PathBuf::from(var("MANDOR_E2E_HOME")),
        record: var("MANDOR_E2E_RECORD"),
        repo: var("MANDOR_E2E_REPO"),
    })
}

/// Relay `mandord connect` yang sudah melakukan `hello`.
pub struct Relay {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Relay {
    pub fn open(e: &Env) -> Self {
        let token = Command::new(&e.bin)
            .arg("token")
            .env("MANDOR_HOME", &e.home)
            .output()
            .unwrap();
        assert!(token.status.success(), "mandord token gagal");
        let token = String::from_utf8(token.stdout).unwrap().trim().to_owned();
        let mut child = Command::new(&e.bin)
            .arg("connect")
            .env("MANDOR_HOME", &e.home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut relay = Self {
            child,
            stdin,
            stdout,
            next: 1,
        };
        relay.call(
            "hello",
            &json!({"token": token, "protocol": "1.0", "client": "mandor-e2e"}),
        );
        relay
    }

    /// Memanggil method; event (tanpa `id`) dilewati.
    pub fn call(&mut self, method: &str, params: &Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .unwrap();
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "relay tertutup"
            );
            let v: Value = serde_json::from_str(&line).unwrap();
            if v.get("id").and_then(Value::as_u64) == Some(id) {
                assert!(v.get("error").is_none(), "{method} gagal: {}", v["error"]);
                return v["result"].clone();
            }
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn card(relay: &mut Relay, record: &str) -> Value {
    let board = relay.call("board.list", &Value::Null);
    board["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["record_id"] == record)
        .cloned()
        .unwrap_or_else(|| panic!("record {record} tidak ada di papan pengguna"))
}

pub fn wait_for(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    panic!("menunggu {what} melewati batas waktu");
}

/// Run milik record ini di `runs.list`.
pub fn run_of(relay: &mut Relay, record: &str) -> Option<Value> {
    relay
        .call("runs.list", &Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["record_id"] == record)
        .cloned()
}

/// Menunggu run kembali ke dev; run Failed menggagalkan smoke.
pub fn wait_awaiting_dev(relay: &mut Relay, record: &str, what: &str, timeout: Duration) -> Value {
    let mut last = Value::Null;
    wait_for(what, timeout, || {
        last = run_of(relay, record).unwrap_or(Value::Null);
        assert_ne!(last["state"], "failed", "run gagal: {}", last["last_error"]);
        last["state"] == "awaiting_dev"
    });
    last
}
