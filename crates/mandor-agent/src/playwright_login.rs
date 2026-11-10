//! Login akun uji oleh daemon lewat sesi Playwright MCP miliknya sendiri
//! (plan 2026-11-10-001, KTD-L4).
//!
//! Skrip `browser_run_code_unsafe` adalah template tetap; URL, selector, tanda
//! berhasil, username, dan password hanya masuk sebagai satu literal JSON. Teks
//! balasan dan error MCP mengulang kode beserta password, jadi tidak pernah
//! diteruskan ke log atau pemanggil: hasil hanya [`LoginCategory`].

use std::path::Path;
use std::time::Duration;

use mandor_domain::lab::{LoginForm, SuccessMarker};
use mandor_proc::{Cmd, EnvPolicy, Supervised};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

/// Batas total satu login, termasuk menyalakan Chromium.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(60);
/// Batas tiap permintaan MCP selain `browser_run_code_unsafe`.
const RPC_TIMEOUT: Duration = Duration::from_secs(20);
const STOP_GRACE: Duration = Duration::from_secs(5);
/// Nama tool kode Playwright di `@playwright/mcp` 0.0.83.
const RUN_CODE: &str = "browser_run_code_unsafe";

/// Hasil login tanpa teks dari halaman maupun MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LoginCategory {
    #[error("Playwright could not start")]
    NotStarted,
    #[error("the login took too long")]
    Timeout,
    #[error("the login form was not found")]
    FormNotFound,
    #[error("the signed-in page did not appear after submitting the form")]
    NotSignedIn,
    #[error("the saved session did not stay signed in")]
    SessionNotKept,
    #[error("Playwright reported an error")]
    Failed,
}

/// Masukan satu login. `work_dir` adalah folder kosong milik daemon untuk cwd
/// dan output Playwright; dihapus pemanggil setelah login.
pub struct LoginRequest<'a> {
    pub bin: &'a Path,
    pub form: &'a LoginForm,
    pub username: &'a str,
    pub password: &'a SecretString,
    /// Origin yang boleh dibuka (`--allowed-origins`).
    pub origins: &'a [String],
    /// Proxy review (`--proxy-server`), bila ada.
    pub proxy: Option<&'a str>,
    pub work_dir: &'a Path,
    /// Tujuan berkas `storageState`.
    pub out: &'a Path,
    /// Environment bersih untuk Playwright (PATH, HOME untuk cache browser).
    pub env: Vec<(String, String)>,
}

impl std::fmt::Debug for LoginRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("out", &self.out)
            .finish_non_exhaustive()
    }
}

/// Login dan menyimpan `storageState` ke `req.out`.
///
/// # Errors
/// [`LoginCategory`] tanpa teks dari halaman atau MCP.
pub async fn login(req: &LoginRequest<'_>) -> Result<(), LoginCategory> {
    let mut cmd = Cmd::new(req.bin)
        .args(server_args(req))
        .cwd(req.work_dir)
        .env_policy(EnvPolicy::Explicit(req.env.clone()))
        // Stdin tetap terbuka untuk percakapan JSON-RPC (bukan data sekali kirim).
        .stdin_bytes(Vec::new());
    cmd = cmd.scrub_tokens();
    let mut child = Supervised::spawn(&cmd).map_err(|_| LoginCategory::NotStarted)?;
    let (Some(stdin), Some(stdout)) = (child.take_stdin(), child.take_stdout()) else {
        child.stop(STOP_GRACE).await;
        return Err(LoginCategory::NotStarted);
    };
    let script = login_script(req);
    let result = tokio::time::timeout(LOGIN_TIMEOUT, converse(stdin, stdout, &script)).await;
    child.stop(STOP_GRACE).await;
    match result {
        Err(_) => Err(LoginCategory::Timeout),
        Ok(r) => r,
    }
}

fn server_args(req: &LoginRequest<'_>) -> Vec<String> {
    let mut args = vec![
        "--headless".to_owned(),
        "--isolated".to_owned(),
        "--browser".to_owned(),
        "chromium".to_owned(),
        "--block-service-workers".to_owned(),
        "--output-dir".to_owned(),
        req.work_dir.display().to_string(),
        "--allowed-origins".to_owned(),
        req.origins.join(";"),
    ];
    if let Some(p) = req.proxy {
        args.push("--proxy-server".to_owned());
        args.push(p.to_owned());
    }
    args
}

async fn converse(
    stdin: ChildStdin,
    stdout: ChildStdout,
    script: &str,
) -> Result<(), LoginCategory> {
    let mut rpc = Rpc {
        stdin,
        lines: BufReader::new(stdout).lines(),
        next: 0,
    };
    rpc.call(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "mandord-login", "version": env!("CARGO_PKG_VERSION")},
        }),
        RPC_TIMEOUT,
    )
    .await?;
    rpc.notify("notifications/initialized").await?;
    let reply = rpc
        .call(
            "tools/call",
            json!({"name": RUN_CODE, "arguments": {"code": script}}),
            LOGIN_TIMEOUT,
        )
        .await?;
    outcome(&reply)
}

struct Rpc {
    stdin: ChildStdin,
    lines: tokio::io::Lines<BufReader<ChildStdout>>,
    next: u64,
}

impl Rpc {
    async fn send(&mut self, msg: &Value) -> Result<(), LoginCategory> {
        let mut line = msg.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|_| LoginCategory::NotStarted)
    }

    async fn notify(&mut self, method: &str) -> Result<(), LoginCategory> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": {}}))
            .await
    }

    async fn call(
        &mut self,
        method: &str,
        params: Value,
        limit: Duration,
    ) -> Result<Value, LoginCategory> {
        self.next += 1;
        let id = self.next;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await?;
        tokio::time::timeout(limit, async {
            loop {
                let Ok(Some(line)) = self.lines.next_line().await else {
                    return Err(LoginCategory::NotStarted);
                };
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if v.get("id").and_then(Value::as_u64) == Some(id) {
                    return Ok(v);
                }
            }
        })
        .await
        .map_err(|_| LoginCategory::Timeout)?
    }
}

/// Kata hasil dari template; hanya kata yang dikenal yang dibaca.
fn outcome(reply: &Value) -> Result<(), LoginCategory> {
    let result = reply.get("result").ok_or(LoginCategory::Failed)?;
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    match result_word(text) {
        Some("ok") => Ok(()),
        Some("form_not_found") => Err(LoginCategory::FormNotFound),
        Some("not_signed_in") => Err(LoginCategory::NotSignedIn),
        Some("session_not_kept") => Err(LoginCategory::SessionNotKept),
        _ => Err(LoginCategory::Failed),
    }
}

/// Kata hasil di baris setelah `### Result`, mis. `"ok"`.
fn result_word(text: &str) -> Option<&'static str> {
    let after = text.split_once("### Result")?.1;
    let line = after.lines().map(str::trim).find(|l| !l.is_empty())?;
    ["ok", "form_not_found", "not_signed_in", "session_not_kept"]
        .into_iter()
        .find(|w| line == format!("\"{w}\""))
}

/// Template tetap; seluruh nilai dari perlengkapan dan secret masuk lewat
/// satu literal JSON `d`.
fn login_script(req: &LoginRequest<'_>) -> String {
    let (kind, marker) = match &req.form.success {
        SuccessMarker::PathPrefix(p) => ("path", p.as_str()),
        SuccessMarker::Text(t) => ("text", t.as_str()),
    };
    let data = json!({
        "url": req.form.url,
        "user_field": req.form.username_field,
        "pass_field": req.form.password_field,
        "submit": req.form.submit,
        "username": req.username,
        "password": req.password.expose_secret(),
        "kind": kind,
        "marker": marker,
        "authenticated": req.form.authenticated_url,
        "out": req.out.display().to_string(),
    });
    format!("async (page) => {{ const d = {data}; {LOGIN_BODY} }}")
}

const LOGIN_BODY: &str = r"
const sel = (p, s) => s.startsWith('text=')
  ? p.getByText(s.slice(5), { exact: true }).first()
  : p.locator(s).first();
const loginPath = new URL(d.url).pathname;
const signedIn = async (p) => d.kind === 'path'
  ? new URL(p.url()).pathname.startsWith(d.marker)
  : (await p.getByText(d.marker).count()) > 0;
const waitFor = async (p, ms) => {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await signedIn(p)) return true;
    await p.waitForTimeout(250);
  }
  return false;
};
await page.goto(d.url, { waitUntil: 'domcontentloaded' });
try {
  await sel(page, d.user_field).fill(d.username, { timeout: 10000 });
  await sel(page, d.pass_field).fill(d.password, { timeout: 5000 });
  await sel(page, d.submit).click({ timeout: 5000 });
} catch (e) {
  return 'form_not_found';
}
if (!(await waitFor(page, 20000))) return 'not_signed_in';
const state = await page.context().storageState();
const browser = page.context().browser();
if (!browser) return 'session_not_kept';
const ctx = await browser.newContext({ storageState: state });
const check = await ctx.newPage();
await check.goto(d.authenticated, { waitUntil: 'domcontentloaded' });
const kept = new URL(check.url()).pathname !== loginPath
  && (d.kind === 'path' || (await waitFor(check, 10000)));
await ctx.close();
if (!kept) return 'session_not_kept';
await page.context().storageState({ path: d.out });
return 'ok';
";

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> LoginForm {
        LoginForm {
            url: "http://localhost:3000/login".to_owned(),
            username_field: "#email".to_owned(),
            password_field: "[name=password]".to_owned(),
            submit: "text=Sign in".to_owned(),
            submit_path: "/api/login".to_owned(),
            success: SuccessMarker::PathPrefix("/admin".to_owned()),
            authenticated_url: "http://localhost:3000/admin".to_owned(),
        }
    }

    fn req<'a>(f: &'a LoginForm, pw: &'a SecretString, origins: &'a [String]) -> LoginRequest<'a> {
        LoginRequest {
            bin: Path::new("/bin/false"),
            form: f,
            username: "a@uji.test",
            password: pw,
            origins,
            proxy: Some("http://127.0.0.1:9"),
            work_dir: Path::new("/tmp/w"),
            out: Path::new("/tmp/w/state.json"),
            env: Vec::new(),
        }
    }

    #[test]
    fn template_tetap_dan_nilai_hanya_data_json() {
        let f = form();
        let origins = vec!["http://localhost:3000".to_owned()];
        let pw = SecretString::from("p\"; process.exit(1); //`${x}".to_owned());
        let script = login_script(&req(&f, &pw, &origins));
        assert!(script.starts_with("async (page) => { const d = {"));
        assert!(script.ends_with(&format!("{LOGIN_BODY} }}")));
        // Password hanya muncul sebagai string JSON yang di-escape.
        assert!(script.contains(r#""password":"p\"; process.exit(1); //`${x}""#));
        let mut other = f.clone();
        other.submit = "text=Masuk".to_owned();
        let pw2 = SecretString::from("lain".to_owned());
        let script2 = login_script(&req(&other, &pw2, &origins));
        let body = |s: &str| s.split_once("; \n").map(|(_, b)| b.to_owned());
        assert_eq!(body(&script), body(&script2));
    }

    #[test]
    fn argumen_server_memuat_origin_proxy_dan_folder_kerja() {
        let f = form();
        let origins = vec![
            "http://localhost:3000".to_owned(),
            "http://api.localhost:8080".to_owned(),
        ];
        let pw = SecretString::from("x".to_owned());
        let args = server_args(&req(&f, &pw, &origins));
        let joined = args.join(" ");
        assert!(joined.contains("--headless --isolated"));
        assert!(
            joined.contains("--allowed-origins http://localhost:3000;http://api.localhost:8080")
        );
        assert!(joined.contains("--proxy-server http://127.0.0.1:9"));
        assert!(joined.contains("--output-dir /tmp/w"));
        assert!(!joined.contains("unrestricted"));
    }

    #[test]
    fn hasil_hanya_kata_yang_dikenal() {
        let reply = |text: &str| json!({"result": {"content": [{"type": "text", "text": text}]}});
        assert_eq!(
            outcome(&reply("### Result\n\"ok\"\n### Ran Playwright code\n...")),
            Ok(())
        );
        assert_eq!(
            outcome(&reply("### Result\n\"not_signed_in\"")),
            Err(LoginCategory::NotSignedIn)
        );
        assert_eq!(
            outcome(&reply("### Result\n\"form_not_found\"")),
            Err(LoginCategory::FormNotFound)
        );
        assert_eq!(
            outcome(&reply("### Result\n\"session_not_kept\"")),
            Err(LoginCategory::SessionNotKept)
        );
        assert_eq!(
            outcome(&reply("### Error\nTimeoutError: rahasia-123")),
            Err(LoginCategory::Failed)
        );
        assert_eq!(
            outcome(&json!({"error": {"message": "x"}})),
            Err(LoginCategory::Failed)
        );
    }

    #[test]
    fn debug_dan_kategori_tidak_memuat_password() {
        let f = form();
        let origins = Vec::new();
        let pw = SecretString::from("rahasia-123".to_owned());
        let r = req(&f, &pw, &origins);
        assert!(!format!("{r:?}").contains("rahasia-123"));
        for c in [
            LoginCategory::NotStarted,
            LoginCategory::Timeout,
            LoginCategory::FormNotFound,
            LoginCategory::NotSignedIn,
            LoginCategory::SessionNotKept,
            LoginCategory::Failed,
        ] {
            assert!(!c.to_string().contains("rahasia"));
        }
    }
}
