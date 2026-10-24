//! `mandord login lark` (U8 plan VM): login Lark dari terminal SSH sebagai
//! client daemon yang sedang berjalan, dengan callback di loopback VM.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration tests
#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_mandord");

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Server Lark palsu: tukar kode, info pengguna, dan jawaban kosong untuk
/// semua panggilan lain dari sesi yang baru dimulai.
fn fake_lark() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || answer(stream));
        }
    });
    base
}

fn answer(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() {
        return;
    }
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let path = first.split_whitespace().nth(1).unwrap_or_default();
    let reply = if path.starts_with("/open-apis/authen/v2/oauth/token") {
        json!({"code": 0, "access_token": "u-1", "refresh_token": "r-1",
               "expires_in": 7200, "refresh_token_expires_in": 604_800})
    } else if path.starts_with("/open-apis/authen/v1/user_info") {
        json!({"code": 0, "data": {"open_id": "ou_me", "name": "Saya"}})
    } else {
        json!({"code": 0, "data": {"items": [], "has_more": false}})
    };
    let text = reply.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
}

fn serve(home: &Path, env: &[(&str, &str)]) -> Child {
    let mut cmd = Command::new(BIN);
    cmd.arg("serve")
        .env("MANDOR_HOME", home)
        .env_remove("INVOCATION_ID")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.spawn().unwrap()
}

fn wait_for_token(home: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(body) = std::fs::read(home.join("daemon.json")) {
            let info: Value = serde_json::from_slice(&body).unwrap();
            return info["token"].as_str().unwrap().to_owned();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon.json tidak muncul");
}

fn rpc(home: &Path, calls: &[(&str, Value)]) -> Vec<Value> {
    let token = wait_for_token(home);
    let mut child = Command::new(BIN)
        .arg("connect")
        .env("MANDOR_HOME", home)
        .env_remove("INVOCATION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello",
        "params": {"token": token, "protocol": "1.0", "client": "test"}});
    let mut replies = Vec::new();
    let lines = std::iter::once(hello).chain(
        calls
            .iter()
            .enumerate()
            .map(|(i, (m, p))| json!({"jsonrpc": "2.0", "id": i + 1, "method": m, "params": p})),
    );
    for line in lines {
        writeln!(stdin, "{line}").unwrap();
        let mut buf = String::new();
        out.read_line(&mut buf).unwrap();
        replies.push(serde_json::from_str(&buf).unwrap());
    }
    drop(stdin);
    let _ = child.wait();
    replies.remove(0);
    replies
}

fn stop(mut daemon: Child) {
    let _ = Command::new("kill")
        .args(["-TERM", &daemon.id().to_string()])
        .status();
    let _ = daemon.wait();
}

fn settings(callback: u16, lark: &str) -> Value {
    json!({
        "app_id": "cli_uji", "app_secret": "rahasia-app", "app_token": "appX", "table_id": "tblX",
        "ac_table_id": "tblAC", "redirect_port": callback, "open_base": lark,
        "accounts_base": lark, "quiet_secs": null
    })
}

/// Permintaan GET ke callback; mengembalikan kode status HTTP.
fn get(port: u16, path: &str) -> u16 {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
    let mut head = String::new();
    BufReader::new(s).read_line(&mut head).unwrap();
    head.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn login_lark_dari_terminal_menyimpan_token_dan_memulai_sesi_tanpa_restart() {
    let home = tempfile::tempdir().unwrap();
    let lark = fake_lark();
    let callback = free_port();
    let daemon = serve(home.path(), &[]);
    let r = rpc(
        home.path(),
        &[("lark.settings.set", settings(callback, &lark))],
    );
    assert!(r[0].get("error").is_none(), "{r:?}");

    let mut login = Command::new(BIN)
        .args(["login", "lark"])
        .env("MANDOR_HOME", home.path())
        .env_remove("INVOCATION_ID")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(login.stdout.take().unwrap());
    let mut text = String::new();
    let url = loop {
        let mut line = String::new();
        assert!(out.read_line(&mut line).unwrap() > 0, "{text}");
        text.push_str(&line);
        if let Some(u) = line.split_whitespace().find(|w| w.starts_with("http")) {
            if u.contains("authorize") {
                break u.to_owned();
            }
        }
    };
    let state = url
        .split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("state="))
        .unwrap()
        .to_owned();
    // Teks bantuan menyebut port forward SSH ke port callback.
    while !text.contains("Waiting") {
        let mut line = String::new();
        assert!(out.read_line(&mut line).unwrap() > 0, "{text}");
        text.push_str(&line);
    }
    assert!(
        text.contains(&format!("ssh -L {callback}:127.0.0.1:{callback}")),
        "{text}"
    );

    assert_eq!(get(callback, "/favicon.ico"), 404);
    assert_eq!(get(callback, "/callback?code=kode-1&state=salah"), 400);
    assert_eq!(
        get(callback, &format!("/callback?code=kode-1&state={state}")),
        200
    );
    let status = login.wait().unwrap();
    let mut err = String::new();
    login
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    assert!(status.success(), "{err}");

    let r = rpc(home.path(), &[("status", Value::Null)]);
    assert_eq!(r[0]["result"]["lark"]["logged_in"], true, "{r:?}");
    assert_eq!(r[0]["result"]["lark"]["open_id"], "ou_me");
    stop(daemon);
}

#[test]
fn login_lark_tanpa_daemon_berjalan_gagal_dengan_petunjuk() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(BIN)
        .args(["login", "lark"])
        .env("MANDOR_HOME", home.path())
        .env_remove("INVOCATION_ID")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("not running"), "{err}");
}

#[test]
fn daemon_vm_menolak_port_di_luar_rentang_developer() {
    let home = tempfile::tempdir().unwrap();
    let group = tempfile::tempdir().unwrap();
    let daemon = serve(
        home.path(),
        &[
            ("MANDOR_ROLE", "vm"),
            ("MANDOR_AGENT_USER", "beni-agent"),
            ("MANDOR_GROUP_DIR", group.path().to_str().unwrap()),
            ("MANDOR_PORT_RANGE", "5100-5199"),
        ],
    );
    let lark = fake_lark();
    let r = rpc(
        home.path(),
        &[
            ("lark.settings.set", settings(53_682, &lark)),
            ("lark.settings.set", settings(5150, &lark)),
        ],
    );
    assert_eq!(r[0]["error"]["code"], "rejected", "{r:?}");
    assert!(
        r[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("5100-5199")
    );
    assert!(r[1].get("error").is_none(), "{r:?}");
    stop(daemon);
}

#[test]
fn status_vm_memuat_tiga_login_beserta_langkah_perbaikan() {
    let home = tempfile::tempdir().unwrap();
    let group = tempfile::tempdir().unwrap();
    let daemon = serve(
        home.path(),
        &[
            ("MANDOR_ROLE", "vm"),
            ("MANDOR_AGENT_USER", "mandor-uji-tidak-ada"),
            ("MANDOR_GROUP_DIR", group.path().to_str().unwrap()),
            ("MANDOR_PORT_RANGE", "5100-5199"),
            ("MANDOR_CLAUDE_BIN", "/tidak/ada/claude"),
            ("PATH", "/usr/bin:/bin"),
            ("MANDOR_GH_BIN", "/tidak/ada/gh"),
        ],
    );
    let r = rpc(home.path(), &[("status", Value::Null)]);
    let logins = r[0]["result"]["logins"].as_array().unwrap().clone();
    let services: Vec<&str> = logins
        .iter()
        .map(|l| l["service"].as_str().unwrap())
        .collect();
    assert_eq!(services, ["lark", "gh", "claude"]);
    for l in &logins {
        assert_eq!(l["state"], "missing", "{l}");
        assert!(!l["fix"].as_str().unwrap().is_empty(), "{l}");
    }
    assert!(
        logins[0]["fix"]
            .as_str()
            .unwrap()
            .contains("mandord login lark")
    );
    assert!(
        logins[2]["fix"]
            .as_str()
            .unwrap()
            .contains("mandor-uji-tidak-ada")
    );
    stop(daemon);

    // Daemon laptop tidak memeriksa login VM.
    let home = tempfile::tempdir().unwrap();
    let daemon = serve(home.path(), &[]);
    let r = rpc(home.path(), &[("status", Value::Null)]);
    assert!(r[0]["result"].get("logins").is_none_or(Value::is_null));
    stop(daemon);
}
