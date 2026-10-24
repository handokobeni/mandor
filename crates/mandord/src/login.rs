//! `mandord login lark`: login Lark dari terminal SSH (KTD-66 plan VM).
//!
//! Berjalan sebagai client daemon yang sudah berjalan: memanggil
//! `lark.login.begin`, melayani callback OAuth di loopback pada port milik
//! developer, lalu meneruskan code ke `lark.login.complete`, sehingga daemon
//! menyimpan token dan memulai sesi tanpa restart. Developer membuka URL di
//! browser laptop dengan port callback diteruskan lewat `ssh -L`.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, bail};
use mandor_api::DaemonClient;
use mandor_protocol::{LoginBegin, LoginComplete, method};
use secrecy::SecretString;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use crate::config::Config;
use crate::discovery;

/// Batas waktu menunggu callback Lark.
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(300);
/// Batas waktu membaca satu permintaan callback.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Ukuran maksimum kepala permintaan callback.
const MAX_REQUEST: usize = 8 * 1024;

/// Menjalankan `mandord login <layanan>`.
///
/// # Errors
/// Daemon tidak berjalan, login ditolak, atau callback tidak datang.
pub async fn run(cfg: &Config, args: &[String]) -> anyhow::Result<()> {
    match args.first().map(String::as_str) {
        Some("lark") => lark(cfg).await,
        _ => bail!("usage: mandord login lark"),
    }
}

async fn lark(cfg: &Config) -> anyhow::Result<()> {
    let info = discovery::read(&cfg.home).await.context(
        "the Mandor daemon is not running; start it with `systemctl --user start mandord`",
    )?;
    let client = DaemonClient::new(
        SocketAddr::from((Ipv4Addr::LOCALHOST, info.port)),
        SecretString::from(info.token),
        "mandord-login",
    );
    let begin: LoginBegin = serde_json::from_value(
        client
            .request(method::LARK_LOGIN_BEGIN, serde_json::Value::Null)
            .await
            .map_err(|e| anyhow::anyhow!(e.message))?,
    )
    .context("reading the login URL")?;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, begin.port))
        .await
        .with_context(|| format!("port {} for the Lark callback is in use", begin.port))?;
    say(&instructions(&begin)).await?;
    let code = wait_callback(&listener, &begin.state, CALLBACK_TIMEOUT).await?;
    drop(listener);
    let complete = LoginComplete {
        code,
        state: begin.state,
    };
    client
        .request(
            method::LARK_LOGIN_COMPLETE,
            serde_json::to_value(complete).context("encoding the login code")?,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.message))?;
    say("Logged in to Lark. The daemon started syncing.\n").await
}

fn instructions(begin: &LoginBegin) -> String {
    let port = begin.port;
    format!(
        "Open this URL in the browser on your laptop:\n\n  {url}\n\n\
         The Lark callback goes to port {port} on this machine. When you are \
         connected over SSH, forward it first from your laptop:\n\n  \
         ssh -L {port}:127.0.0.1:{port} <this-vm>\n\n\
         Waiting for the Lark callback (5 minutes)...\n",
        url = begin.url,
    )
}

async fn say(text: &str) -> anyhow::Result<()> {
    let mut out = tokio::io::stdout();
    out.write_all(text.as_bytes())
        .await
        .context("writing to the terminal")?;
    out.flush().await.context("flush terminal")
}

/// Hasil satu permintaan ke listener callback.
#[derive(Debug, PartialEq, Eq)]
enum Callback {
    Code(String),
    /// Lark mengembalikan `error` (mis. developer menolak izin).
    Denied(String),
    /// Bukan callback login ini; ditolak lalu terus menunggu.
    Ignored,
}

/// Menunggu callback dengan `state` yang cocok sampai `timeout`. Permintaan
/// lain (path salah, state salah) dijawab dengan error dan diabaikan, karena
/// user lain di VM bersama bisa menjangkau port loopback ini.
async fn wait_callback(
    listener: &TcpListener,
    state: &str,
    timeout: Duration,
) -> anyhow::Result<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let accepted = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "no Lark callback within {} minutes; run `mandord login lark` again",
                    timeout.as_secs() / 60
                )
            })?;
        let Ok((stream, _)) = accepted else {
            continue;
        };
        match handle(stream, state).await {
            Callback::Code(code) => return Ok(code),
            Callback::Denied(reason) => bail!("Lark login was not completed: {reason}"),
            Callback::Ignored => {}
        }
    }
}

async fn handle(mut stream: TcpStream, state: &str) -> Callback {
    let Ok(Some(target)) = tokio::time::timeout(REQUEST_TIMEOUT, read_target(&mut stream))
        .await
        .map(Result::ok)
        .map(Option::flatten)
    else {
        respond(&mut stream, 400, "Bad request").await;
        return Callback::Ignored;
    };
    let outcome = parse_callback(&target, state);
    let (code, body) = match &outcome {
        Callback::Code(_) => (200, "Lark login complete. You can close this tab."),
        Callback::Denied(_) => (400, "Lark login was not completed."),
        Callback::Ignored if target.starts_with("/callback") => (400, "Login state mismatch."),
        Callback::Ignored => (404, "Not found"),
    };
    respond(&mut stream, code, body).await;
    outcome
}

/// Target permintaan (`/callback?...`) dari baris pertama kepala HTTP.
async fn read_target(stream: &mut TcpStream) -> std::io::Result<Option<String>> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < MAX_REQUEST {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&buf);
    let mut first = head.lines().next().unwrap_or_default().split_whitespace();
    Ok(match (first.next(), first.next()) {
        (Some("GET"), Some(target)) => Some(target.to_owned()),
        _ => None,
    })
}

fn parse_callback(target: &str, state: &str) -> Callback {
    let Some(query) = target.strip_prefix("/callback?") else {
        return Callback::Ignored;
    };
    let param = |name: &str| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == name)
            .and_then(|(_, v)| percent_decode(v))
    };
    if param("state").as_deref() != Some(state) {
        return Callback::Ignored;
    }
    if let Some(err) = param("error") {
        return Callback::Denied(err);
    }
    match param("code") {
        Some(code) if !code.is_empty() => Callback::Code(code),
        _ => Callback::Ignored,
    }
}

/// Dekode `%XX` dan `+`; `None` bila bentuknya rusak.
fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = raw.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
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
    String::from_utf8(out).ok()
}

async fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let page = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = tokio::time::timeout(REQUEST_TIMEOUT, stream.write_all(page.as_bytes())).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_hanya_diterima_dengan_state_yang_cocok() {
        assert_eq!(
            parse_callback("/callback?code=ab%2Fc&state=s1", "s1"),
            Callback::Code("ab/c".to_owned())
        );
        assert_eq!(
            parse_callback("/callback?code=abc&state=lain", "s1"),
            Callback::Ignored
        );
        assert_eq!(
            parse_callback("/callback?code=abc", "s1"),
            Callback::Ignored
        );
        assert_eq!(
            parse_callback("/callback?error=access_denied&state=s1", "s1"),
            Callback::Denied("access_denied".to_owned())
        );
        assert_eq!(parse_callback("/favicon.ico", "s1"), Callback::Ignored);
        assert_eq!(percent_decode("a%zz"), None);
    }

    #[tokio::test]
    async fn tanpa_callback_listener_berhenti_setelah_batas_waktu() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let err = wait_callback(&listener, "s1", Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no Lark callback"), "{err}");
    }
}
