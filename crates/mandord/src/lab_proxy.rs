//! Proxy allowlist milik daemon untuk browser review dan lab (plan
//! 2026-11-10-001, KTD-L6, U17).
//!
//! Satu proxy per review, hanya mendengarkan di loopback. Keputusan ada di
//! [`mandor_domain::lab::proxy_decision`]; modul ini hanya I/O. Penolakan
//! dijawab `418` dengan header `X-Mandor-Proxy: blocked` dan dicatat agar
//! usecase bisa mengisi `blocked_requests` di bukti AC.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HOST, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use mandor_domain::lab::{ProxyBlock, ProxyPolicy, ProxyRequest, proxy_decision};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

/// Batas menyambung ke app.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Batas menunggu header respons app.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
/// Batas umur satu terowongan `CONNECT` (WebSocket, HTTPS).
const TUNNEL_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
/// Batas jumlah penolakan yang dicatat per review.
const MAX_BLOCKED: usize = 500;

/// Request yang ditolak proxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedHit {
    pub at_ms: i64,
    pub method: String,
    pub url: String,
    pub reason: ProxyBlock,
}

struct State {
    policy: RwLock<ProxyPolicy>,
    blocked: Mutex<Vec<BlockedHit>>,
}

/// Proxy satu review. Berhenti saat [`LabProxy::stop`] dipanggil, saat
/// token induk dibatalkan, atau saat nilai ini di-drop.
pub struct LabProxy {
    port: u16,
    state: Arc<State>,
    cancel: CancellationToken,
}

impl std::fmt::Debug for LabProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabProxy")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

type ProxyBody = BoxBody<Bytes, hyper::Error>;

impl LabProxy {
    /// Mendengarkan di `127.0.0.1:<port acak>`; tugas dimiliki `tracker`.
    ///
    /// # Errors
    /// Port loopback tidak bisa dibuka.
    pub async fn start(
        policy: ProxyPolicy,
        tracker: &TaskTracker,
        parent: &CancellationToken,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let state = Arc::new(State {
            policy: RwLock::new(policy),
            blocked: Mutex::new(Vec::new()),
        });
        let cancel = parent.child_token();
        let accept_state = Arc::clone(&state);
        let accept_cancel = cancel.clone();
        let conns = tracker.clone();
        tracker.spawn(async move {
            loop {
                let stream = tokio::select! {
                    () = accept_cancel.cancelled() => break,
                    r = listener.accept() => match r {
                        Ok((s, _)) => s,
                        Err(e) => {
                            tracing::warn!(error = %e, "proxy lab gagal menerima koneksi");
                            continue;
                        }
                    },
                };
                let st = Arc::clone(&accept_state);
                let c = accept_cancel.clone();
                let t = conns.clone();
                conns.spawn(serve_conn(stream, st, c, t));
            }
        });
        Ok(Self {
            port,
            state,
            cancel,
        })
    }

    #[cfg(test)]
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Alamat untuk `--proxy-server`.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Mengizinkan atau menolak request tulis (sesi lab data, KTD-L6).
    pub fn set_write_allowed(&self, allowed: bool) {
        if let Ok(mut p) = self.state.policy.write() {
            p.write_allowed = allowed;
        }
    }

    /// Penolakan sejak panggilan sebelumnya.
    #[must_use]
    pub fn take_blocked(&self) -> Vec<BlockedHit> {
        self.state
            .blocked
            .lock()
            .map(|mut b| std::mem::take(&mut *b))
            .unwrap_or_default()
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

impl Drop for LabProxy {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

async fn serve_conn(
    stream: TcpStream,
    state: Arc<State>,
    cancel: CancellationToken,
    tracker: TaskTracker,
) {
    let io = TokioIo::new(stream);
    let svc_cancel = cancel.clone();
    let service = hyper::service::service_fn(move |req| {
        handle(req, Arc::clone(&state), svc_cancel.clone(), tracker.clone())
    });
    let conn = hyper::server::conn::http1::Builder::new()
        .preserve_header_case(true)
        .serve_connection(io, service)
        .with_upgrades();
    tokio::select! {
        () = cancel.cancelled() => {}
        r = conn => {
            if let Err(e) = r {
                tracing::debug!(error = %e, "koneksi proxy lab berakhir");
            }
        }
    }
}

async fn handle(
    req: Request<Incoming>,
    state: Arc<State>,
    cancel: CancellationToken,
    tracker: TaskTracker,
) -> Result<Response<ProxyBody>, hyper::Error> {
    if req.method() == Method::CONNECT {
        return Ok(connect(req, &state, cancel, &tracker));
    }
    let url = req.uri().to_string();
    let method = req.method().as_str().to_owned();
    let decision = state
        .policy
        .read()
        .map_or(Err(ProxyBlock::OtherOrigin), |p| {
            proxy_decision(
                &p,
                ProxyRequest::Http {
                    method: &method,
                    url: &url,
                },
            )
        });
    if let Err(reason) = decision {
        record(&state, &method, &url, reason);
        return Ok(blocked());
    }
    Ok(forward(req, &tracker).await.unwrap_or_else(|e| {
        tracing::debug!(error = %e, url, "proxy lab gagal meneruskan");
        plain(StatusCode::BAD_GATEWAY, "upstream unavailable")
    }))
}

fn connect(
    req: Request<Incoming>,
    state: &State,
    cancel: CancellationToken,
    tracker: &TaskTracker,
) -> Response<ProxyBody> {
    let authority = req
        .uri()
        .authority()
        .map(ToString::to_string)
        .unwrap_or_default();
    let decision = state
        .policy
        .read()
        .map_or(Err(ProxyBlock::OtherOrigin), |p| {
            proxy_decision(
                &p,
                ProxyRequest::Connect {
                    authority: &authority,
                },
            )
        });
    if let Err(reason) = decision {
        record(state, "CONNECT", &authority, reason);
        return blocked();
    }
    tracker.spawn(async move {
        let upgraded = match hyper::upgrade::on(req).await {
            Ok(u) => u,
            Err(e) => {
                tracing::debug!(error = %e, "upgrade CONNECT gagal");
                return;
            }
        };
        let Ok(mut upstream) = dial(&authority).await else {
            return;
        };
        let mut client = TokioIo::new(upgraded);
        tokio::select! {
            () = cancel.cancelled() => {}
            r = tokio::time::timeout(
                TUNNEL_TIMEOUT,
                tokio::io::copy_bidirectional(&mut client, &mut upstream),
            ) => {
                if let Ok(Err(e)) = r {
                    tracing::debug!(error = %e, "terowongan proxy lab berakhir");
                }
            }
        }
    });
    plain(StatusCode::OK, "")
}

async fn forward(
    mut req: Request<Incoming>,
    tracker: &TaskTracker,
) -> anyhow::Result<Response<ProxyBody>> {
    let uri = req.uri().clone();
    let host = uri.host().ok_or_else(|| anyhow::anyhow!("no host"))?;
    let port = uri.port_u16().unwrap_or(80);
    let authority = format!("{host}:{port}");
    let stream = dial(&authority).await?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tracker.spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!(error = %e, "koneksi app dari proxy lab berakhir");
        }
    });
    let path = uri
        .path_and_query()
        .map_or_else(|| "/".to_owned(), ToString::to_string);
    *req.uri_mut() = path.parse()?;
    let headers = req.headers_mut();
    headers.remove("proxy-connection");
    headers.remove("proxy-authorization");
    if !headers.contains_key(HOST) {
        headers.insert(HOST, HeaderValue::from_str(&authority)?);
    }
    let resp = tokio::time::timeout(RESPONSE_TIMEOUT, sender.send_request(req)).await??;
    Ok(resp.map(BodyExt::boxed))
}

/// Menyambung ke `host:port`. Nama `*.localhost` selalu loopback (RFC 6761)
/// walau resolver sistem tidak mengenalnya.
async fn dial(authority: &str) -> anyhow::Result<TcpStream> {
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("authority without port"))?;
    let port: u16 = port.parse()?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let candidates: Vec<SocketAddr> = if host.ends_with(".localhost") || host == "localhost" {
        vec![
            ([127, 0, 0, 1], port).into(),
            (std::net::Ipv6Addr::LOCALHOST, port).into(),
        ]
    } else {
        tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::lookup_host((host, port)))
            .await??
            .collect()
    };
    let mut last = None;
    for addr in candidates {
        match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
            Ok(Ok(s)) => return Ok(s),
            Ok(Err(e)) => last = Some(anyhow::Error::from(e)),
            Err(e) => last = Some(anyhow::Error::from(e)),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("no address for {host}")))
}

fn record(state: &State, method: &str, url: &str, reason: ProxyBlock) {
    if let Ok(mut b) = state.blocked.lock()
        && b.len() < MAX_BLOCKED
    {
        b.push(BlockedHit {
            at_ms: crate::prod::now_ms(),
            method: method.to_owned(),
            url: url.to_owned(),
            reason,
        });
    }
}

fn blocked() -> Response<ProxyBody> {
    let mut r = plain(
        StatusCode::IM_A_TEAPOT,
        "blocked by the Mandor review proxy",
    );
    r.headers_mut()
        .insert("x-mandor-proxy", HeaderValue::from_static("blocked"));
    r
}

fn plain(status: StatusCode, body: &'static str) -> Response<ProxyBody> {
    let mut r = Response::new(
        Full::new(Bytes::from_static(body.as_bytes()))
            .map_err(|never| match never {})
            .boxed(),
    );
    *r.status_mut() = status;
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// App palsu: membalas `ok` untuk setiap request HTTP, atau menggemakan
    /// byte untuk koneksi yang bukan HTTP.
    async fn upstream(echo: bool) -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let port = listener
            .local_addr()
            .unwrap_or_else(|e| panic!("{e}"))
            .port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let log = Arc::clone(&log);
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let Ok(n) = s.read(&mut buf).await else {
                        return;
                    };
                    if echo {
                        let _ = s.write_all(&buf[..n]).await;
                        return;
                    }
                    let head = String::from_utf8_lossy(&buf[..n])
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    if let Ok(mut l) = log.lock() {
                        l.push(head);
                    }
                    let _ = s
                        .write_all(
                            b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                        )
                        .await;
                });
            }
        });
        (port, seen)
    }

    async fn send(proxy: u16, raw: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", proxy))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        s.write_all(raw.as_bytes())
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let mut buf = vec![0u8; 4096];
        let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf))
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .unwrap_or_else(|e| panic!("{e}"));
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }

    async fn proxy_for(port: u16, safe: &[&str]) -> (LabProxy, TaskTracker, CancellationToken) {
        let tracker = TaskTracker::new();
        let cancel = CancellationToken::new();
        let policy = ProxyPolicy::new(
            &[format!("http://127.0.0.1:{port}")],
            &safe.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
        );
        let p = LabProxy::start(policy, &tracker, &cancel)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        (p, tracker, cancel)
    }

    #[tokio::test]
    async fn get_ke_origin_run_diteruskan() {
        let (app, seen) = upstream(false).await;
        let (p, _t, _c) = proxy_for(app, &[]).await;
        let resp = send(
            p.port(),
            &format!(
                "GET http://127.0.0.1:{app}/admin?x=1 HTTP/1.1\r\nHost: 127.0.0.1:{app}\r\n\r\n"
            ),
        )
        .await;
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(resp.ends_with("ok"), "{resp}");
        assert_eq!(
            seen.lock().map(|l| l.clone()).unwrap_or_default(),
            vec!["GET /admin?x=1 HTTP/1.1".to_owned()]
        );
        assert!(p.take_blocked().is_empty());
    }

    #[tokio::test]
    async fn origin_lain_dan_port_mandord_ditolak_418() {
        let (app, _) = upstream(false).await;
        let (p, _t, _c) = proxy_for(app, &[]).await;
        for url in ["http://example.com/", "http://127.0.0.1:7878/rpc"] {
            let resp = send(p.port(), &format!("GET {url} HTTP/1.1\r\nHost: x\r\n\r\n")).await;
            assert!(resp.starts_with("HTTP/1.1 418"), "{resp}");
            assert!(
                resp.to_ascii_lowercase()
                    .contains("x-mandor-proxy: blocked"),
                "{resp}"
            );
        }
        let blocked = p.take_blocked();
        assert_eq!(blocked.len(), 2);
        assert_eq!(blocked[0].reason, ProxyBlock::OtherOrigin);
        assert!(p.take_blocked().is_empty());
    }

    #[tokio::test]
    async fn post_ditolak_kecuali_path_aman_atau_sesi_data() {
        let (app, seen) = upstream(false).await;
        let (p, _t, _c) = proxy_for(app, &["/api/search"]).await;
        let post = |path: &str| {
            format!(
                "POST http://127.0.0.1:{app}{path} HTTP/1.1\r\nHost: 127.0.0.1:{app}\r\ncontent-length: 0\r\n\r\n"
            )
        };
        let resp = send(p.port(), &post("/api/kb")).await;
        assert!(resp.starts_with("HTTP/1.1 418"), "{resp}");
        let blocked = p.take_blocked();
        assert_eq!(blocked[0].reason, ProxyBlock::Write);
        assert_eq!(blocked[0].method, "POST");
        assert!(
            send(p.port(), &post("/api/search"))
                .await
                .starts_with("HTTP/1.1 200")
        );
        p.set_write_allowed(true);
        assert!(
            send(p.port(), &post("/api/kb"))
                .await
                .starts_with("HTTP/1.1 200")
        );
        assert_eq!(seen.lock().map(|l| l.len()).unwrap_or_default(), 2);
    }

    #[tokio::test]
    async fn connect_ke_origin_run_menjadi_terowongan_saat_sesi_data() {
        let (app, _) = upstream(true).await;
        let (p, _t, _c) = proxy_for(app, &[]).await;
        let blocked = send(
            p.port(),
            &format!("CONNECT 127.0.0.1:{app} HTTP/1.1\r\nHost: 127.0.0.1:{app}\r\n\r\n"),
        )
        .await;
        assert!(blocked.starts_with("HTTP/1.1 418"), "{blocked}");
        p.set_write_allowed(true);
        let mut s = TcpStream::connect(("127.0.0.1", p.port()))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        s.write_all(
            format!("CONNECT 127.0.0.1:{app} HTTP/1.1\r\nHost: 127.0.0.1:{app}\r\n\r\n").as_bytes(),
        )
        .await
        .unwrap_or_else(|e| panic!("{e}"));
        let mut buf = vec![0u8; 1024];
        let n = s.read(&mut buf).await.unwrap_or_else(|e| panic!("{e}"));
        assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 200"));
        s.write_all(b"ping").await.unwrap_or_else(|e| panic!("{e}"));
        let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf))
            .await
            .unwrap_or_else(|e| panic!("{e}"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&buf[..n], b"ping");
    }

    #[tokio::test]
    async fn connect_ke_host_lain_ditolak() {
        let (app, _) = upstream(true).await;
        let (p, _t, _c) = proxy_for(app, &[]).await;
        let resp = send(
            p.port(),
            "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n",
        )
        .await;
        assert!(resp.starts_with("HTTP/1.1 418"), "{resp}");
        assert_eq!(p.take_blocked()[0].method, "CONNECT");
    }

    #[tokio::test]
    async fn berhenti_melepas_port() {
        let (app, _) = upstream(false).await;
        let (p, tracker, _c) = proxy_for(app, &[]).await;
        let port = p.port();
        p.stop();
        tracker.close();
        tokio::time::timeout(Duration::from_secs(5), tracker.wait())
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(TcpListener::bind(("127.0.0.1", port)).await.is_ok());
    }
}
