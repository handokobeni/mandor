//! Penerima redirect OAuth Lark di loopback (KTD-5).
//!
//! Browser dibuka di mesin tempat UI berjalan (Windows pada mode WSL), jadi
//! callback diterima di sini lalu kodenya diteruskan ke daemon lewat RPC. Daemon
//! yang memegang verifier PKCE dan melakukan penukaran token.

use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Batas ukuran baris permintaan yang dibaca dari browser.
const MAX_REQUEST: usize = 8 * 1024;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum CallbackError {
    #[error(
        "callback port {0} is already in use by another application; close it and log in again"
    )]
    PortInUse(u16),
    #[error("login cancelled: state mismatch")]
    StateMismatch,
    #[error("Lark rejected the login: {0}")]
    Denied(String),
    #[error("no login response before the timeout")]
    Timeout,
    #[error("callback failed: {0}")]
    Io(String),
}

/// Mengikat port callback. Dipisah dari [`wait_for_code`] agar port yang sudah
/// terpakai dilaporkan sebelum browser dibuka.
///
/// # Errors
/// [`CallbackError::PortInUse`] bila port tidak bisa diikat.
pub async fn bind(port: u16) -> Result<TcpListener, CallbackError> {
    TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AddrInUse => CallbackError::PortInUse(port),
            _ => CallbackError::Io(e.to_string()),
        })
}

/// Menunggu satu redirect `/callback?code=..&state=..` dan mengembalikan kodenya.
/// Permintaan ke path lain (mis. favicon) diabaikan.
///
/// # Errors
/// [`CallbackError`] bila state tidak cocok, Lark menolak, atau waktu habis.
pub async fn wait_for_code(
    listener: TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<String, CallbackError> {
    tokio::time::timeout(timeout, accept_loop(&listener, expected_state))
        .await
        .map_err(|_| CallbackError::Timeout)?
}

async fn accept_loop(
    listener: &TcpListener,
    expected_state: &str,
) -> Result<String, CallbackError> {
    loop {
        let (mut sock, _) = listener
            .accept()
            .await
            .map_err(|e| CallbackError::Io(e.to_string()))?;
        let Some(target) = read_target(&mut sock).await else {
            continue;
        };
        let Some(query) = target.strip_prefix("/callback") else {
            respond(&mut sock, 404, "<!doctype html><title>Not found</title>").await;
            continue;
        };
        let outcome = check_query(query.trim_start_matches('?'), expected_state);
        respond(
            &mut sock,
            if outcome.is_ok() { 200 } else { 400 },
            &page(&outcome),
        )
        .await;
        return outcome;
    }
}

fn check_query(query: &str, expected_state: &str) -> Result<String, CallbackError> {
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let v = percent_decode(v);
        match k {
            "code" => code = Some(v),
            "state" => state = Some(v),
            "error" => error = Some(v),
            _ => {}
        }
    }
    // State diperiksa lebih dulu: respons tanpa state yang benar tidak dipercaya.
    if state.as_deref() != Some(expected_state) {
        return Err(CallbackError::StateMismatch);
    }
    if let Some(err) = error {
        return Err(CallbackError::Denied(err));
    }
    code.filter(|c| !c.is_empty())
        .ok_or_else(|| CallbackError::Denied("authorization code missing".to_owned()))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(b);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Membaca baris pertama permintaan HTTP dan mengembalikan target-nya.
async fn read_target(sock: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(2).any(|w| w == b"\r\n") {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 || buf.len() > MAX_REQUEST {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let line = String::from_utf8_lossy(&buf);
    let mut parts = line.split(' ');
    (parts.next()? == "GET").then_some(())?;
    parts.next().map(str::to_owned)
}

const TEMPLATE: &str = include_str!("callback.html");
const CHECK_ICON: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/></svg>"#;
const CROSS_ICON: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/></svg>"#;

/// Halaman hasil login (desain handoff), tanpa skrip; teks dari Lark di-escape.
fn page(outcome: &Result<String, CallbackError>) -> String {
    let (class, icon, title, heading, body) = match outcome {
        Ok(_) => (
            "ok",
            CHECK_ICON,
            "Mandor · Logged in to Lark",
            "Logged in to Lark",
            "<p>You can return to Mandor.</p>".to_owned(),
        ),
        Err(e) => {
            let reason = match e {
                CallbackError::StateMismatch => {
                    "This response does not match the login Mandor started (state mismatch), so it was ignored."
                        .to_owned()
                }
                CallbackError::Denied(d) if d == "access_denied" => {
                    "You declined access in Lark. Mandor needs permission to read Sprint Tasks and update Task Status."
                        .to_owned()
                }
                other => escape_html(&other.to_string()),
            };
            (
                "fail",
                CROSS_ICON,
                "Mandor · Lark login failed",
                "Lark login failed",
                format!(
                    "<p>{reason}</p><p>Go back to Mandor and press <b>Log in with Lark</b> again.</p>"
                ),
            )
        }
    };
    TEMPLATE
        .replace("{{TITLE}}", title)
        .replace("{{CLASS}}", class)
        .replace("{{ICON}}", icon)
        .replace("{{HEADING}}", heading)
        .replace("{{BODY}}", &body)
}

async fn respond(sock: &mut TcpStream, status: u16, body: &str) {
    let resp = format!(
        "HTTP/1.1 {status} OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-security-policy: default-src 'none'; style-src 'unsafe-inline'\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = sock.write_all(resp.as_bytes()).await;
    let _ = sock.shutdown().await;
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    async fn hit(port: u16, path: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET {path} HTTP/1.1\r\nhost: x\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }

    async fn listener() -> (TcpListener, u16) {
        let l = bind(0).await.unwrap();
        let port = l.local_addr().unwrap().port();
        (l, port)
    }

    #[tokio::test]
    async fn kode_dikembalikan_bila_state_cocok() {
        let (l, port) = listener().await;
        let wait =
            tokio::spawn(async move { wait_for_code(l, "s1", Duration::from_secs(5)).await });
        let favicon = hit(port, "/favicon.ico").await;
        assert!(favicon.starts_with("HTTP/1.1 404"));
        let page = hit(port, "/callback?code=ab%2Fc&state=s1").await;
        assert!(page.starts_with("HTTP/1.1 200"));
        assert!(
            page.contains("<h1>Logged in to Lark</h1>") && page.contains("You can close this tab.")
        );
        // Halaman mandiri: tidak memuat skrip atau sumber luar.
        assert!(page.contains("content-security-policy: default-src 'none'"));
        assert!(!page.contains("<script"));
        assert_eq!(wait.await.unwrap().unwrap(), "ab/c");
    }

    #[tokio::test]
    async fn state_tidak_cocok_ditolak() {
        let (l, port) = listener().await;
        let wait =
            tokio::spawn(async move { wait_for_code(l, "s1", Duration::from_secs(5)).await });
        let page = hit(port, "/callback?code=x&state=palsu").await;
        assert!(page.starts_with("HTTP/1.1 400"));
        assert!(page.contains("<h1>Lark login failed</h1>") && page.contains("does not match"));
        assert!(matches!(
            wait.await.unwrap(),
            Err(CallbackError::StateMismatch)
        ));
    }

    #[tokio::test]
    async fn penolakan_pengguna_dilaporkan() {
        let (l, port) = listener().await;
        let wait =
            tokio::spawn(async move { wait_for_code(l, "s1", Duration::from_secs(5)).await });
        let page = hit(port, "/callback?error=access_denied&state=s1").await;
        assert!(page.contains("You declined access in Lark"));
        assert!(
            matches!(wait.await.unwrap(), Err(CallbackError::Denied(e)) if e == "access_denied")
        );
    }

    #[tokio::test]
    async fn port_terpakai_memberi_pesan_jelas() {
        let (_held, port) = listener().await;
        let err = bind(port).await.unwrap_err();
        assert!(matches!(err, CallbackError::PortInUse(p) if p == port));
        assert!(err.to_string().contains("already in use"));
    }

    #[tokio::test]
    async fn waktu_habis_tanpa_redirect() {
        let (l, _) = listener().await;
        let r = wait_for_code(l, "s1", Duration::from_millis(50)).await;
        assert!(matches!(r, Err(CallbackError::Timeout)));
    }

    #[test]
    fn error_lark_lain_di_escape() {
        let html = page(&Err(CallbackError::Denied("<b>x</b>".to_owned())));
        assert!(html.contains("&lt;b&gt;x&lt;/b&gt;") && !html.contains("<b>x</b>"));
    }

    #[test]
    fn dekode_persen_tahan_input_rusak() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
