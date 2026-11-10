//! Login daemon lewat Playwright MCP sungguhan (plan 2026-11-10-001, U4).
//! Hanya berjalan dengan `MANDOR_E2E=1` dan `MANDOR_PLAYWRIGHT_MCP` menunjuk
//! biner `@playwright/mcp`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use mandor_agent::playwright_login::{LoginCategory, LoginRequest, login};
use mandor_domain::lab::{LoginForm, SuccessMarker};
use secrecy::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PASSWORD: &str = "rahasia-uji-9";

fn enabled() -> Option<PathBuf> {
    // Hanya test e2e; konfigurasi produksi dibaca di `Config::from_env`.
    #[allow(clippy::disallowed_methods)]
    let on = std::env::var("MANDOR_E2E").ok().as_deref() == Some("1");
    #[allow(clippy::disallowed_methods)]
    let bin = std::env::var("MANDOR_PLAYWRIGHT_MCP")
        .ok()
        .map(PathBuf::from);
    on.then_some(bin).flatten()
}

/// App palsu: form login, POST yang memasang cookie, dan /admin yang butuh cookie.
async fn app() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut s, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let first = req.lines().next().unwrap_or_default().to_owned();
                let has_cookie = req.to_ascii_lowercase().contains("cookie: sesi=ok");
                let resp = if first.starts_with("GET /login") {
                    let body = "<form method=post action=/api/login><input id=email name=email><input name=password type=password><button>Sign in</button></form>";
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else if first.starts_with("POST /api/login") {
                    let ok = req.contains(&format!("password={PASSWORD}"));
                    let cookie = if ok {
                        "set-cookie: sesi=ok; Path=/\r\n"
                    } else {
                        ""
                    };
                    let to = if ok { "/admin" } else { "/login" };
                    format!(
                        "HTTP/1.1 303 See Other\r\n{cookie}location: {to}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    )
                } else if first.starts_with("GET /admin") && has_cookie {
                    let body = "<h1>Dashboard</h1>";
                    format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    )
                } else {
                    "HTTP/1.1 302 Found\r\nlocation: /login\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_owned()
                };
                let _ = s.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

fn form(port: u16) -> LoginForm {
    LoginForm {
        url: format!("http://127.0.0.1:{port}/login"),
        username_field: "#email".to_owned(),
        password_field: "[name=password]".to_owned(),
        submit: "text=Sign in".to_owned(),
        submit_path: "/api/login".to_owned(),
        success: SuccessMarker::PathPrefix("/admin".to_owned()),
        authenticated_url: format!("http://127.0.0.1:{port}/admin"),
    }
}

fn env() -> Vec<(String, String)> {
    ["PATH", "HOME"]
        .into_iter()
        .filter_map(|k| {
            #[allow(clippy::disallowed_methods)]
            std::env::var(k).ok().map(|v| (k.to_owned(), v))
        })
        .collect()
}

#[tokio::test]
async fn login_menyimpan_sesi_dan_tidak_meninggalkan_password() {
    let Some(bin) = enabled() else { return };
    let port = app().await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("state.json");
    let f = form(port);
    let origins = vec![format!("http://127.0.0.1:{port}")];
    let pw = SecretString::from(PASSWORD.to_owned());
    let r = login(&LoginRequest {
        bin: &bin,
        form: &f,
        username: "a@uji.test",
        password: &pw,
        origins: &origins,
        proxy: None,
        work_dir: dir.path(),
        out: &out,
        env: env(),
    })
    .await;
    assert_eq!(r, Ok(()));
    let state = std::fs::read_to_string(&out).unwrap();
    assert!(state.contains("sesi"), "{state}");
    for entry in walk(dir.path()) {
        if entry == out {
            continue;
        }
        let body = std::fs::read(&entry).unwrap_or_default();
        assert!(
            !String::from_utf8_lossy(&body).contains(PASSWORD),
            "password di {}",
            entry.display()
        );
    }
}

#[tokio::test]
async fn password_salah_menjadi_not_signed_in() {
    let Some(bin) = enabled() else { return };
    let port = app().await;
    let dir = tempfile::tempdir().unwrap();
    let f = form(port);
    let origins = vec![format!("http://127.0.0.1:{port}")];
    let pw = SecretString::from("salah".to_owned());
    let r = login(&LoginRequest {
        bin: &bin,
        form: &f,
        username: "a@uji.test",
        password: &pw,
        origins: &origins,
        proxy: None,
        work_dir: dir.path(),
        out: &dir.path().join("state.json"),
        env: env(),
    })
    .await;
    assert_eq!(r, Err(LoginCategory::NotSignedIn));
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
