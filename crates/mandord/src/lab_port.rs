//! Port review lab produksi (plan 2026-11-10-001): perlengkapan dari store,
//! password dari `lab_secrets`, proxy per review, dan login akun uji lewat
//! Playwright MCP milik daemon.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mandor_agent::playwright_login::{self, LoginCategory, LoginRequest};
use mandor_domain::lab::{BlockedRequest, LabEquipment, ProxyBlock, ProxyPolicy};
use mandor_store::Store;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{LabLogin, LabPort, LoginFailure, ProxyHit};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::lab_proxy::LabProxy;
use crate::lab_secrets;

/// Batas operasi berkas sesi.
const FILE_TIMEOUT: Duration = Duration::from_secs(5);

/// Adapter [`LabPort`] milik daemon.
#[derive(Clone)]
pub struct ProdLab {
    store: Store,
    lab_root: PathBuf,
    playwright: Option<PathBuf>,
    env: Vec<(String, String)>,
    tracker: TaskTracker,
    shutdown: CancellationToken,
    proxies: Arc<Mutex<HashMap<String, LabProxy>>>,
}

impl std::fmt::Debug for ProdLab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProdLab")
            .field("lab_root", &self.lab_root)
            .finish_non_exhaustive()
    }
}

impl ProdLab {
    #[must_use]
    pub fn new(
        store: Store,
        lab_root: PathBuf,
        playwright: Option<PathBuf>,
        env: Vec<(String, String)>,
        tracker: TaskTracker,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            store,
            lab_root,
            playwright,
            env,
            tracker,
            shutdown,
            proxies: Arc::default(),
        }
    }

    /// Folder sesi login satu review, milik daemon (di bawah `~/.mandor`).
    fn sessions_dir(&self, review_id: &str) -> PathBuf {
        self.lab_root.join("sessions").join(safe_name(review_id))
    }
}

/// Nama berkas aman dari ID review atau role (hanya huruf, angka, `-`, `_`).
fn safe_name(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl LabPort for ProdLab {
    fn playwright_installed(&self) -> bool {
        self.playwright.is_some()
    }

    async fn approved_equipment(&self, repo_id: &str) -> Result<Option<LabEquipment>, PortError> {
        Ok(self
            .store
            .repo_lab(repo_id)
            .await?
            .filter(|l| l.approved_digest.is_some())
            .map(|l| l.equipment))
    }

    async fn start_proxy(&self, review_id: &str, policy: ProxyPolicy) -> Result<String, PortError> {
        let proxy = LabProxy::start(policy, &self.tracker, &self.shutdown)
            .await
            .map_err(|e| PortError::Storage(e.to_string()))?;
        let url = proxy.url();
        if let Ok(mut map) = self.proxies.lock() {
            map.insert(review_id.to_owned(), proxy);
        }
        Ok(url)
    }

    fn take_blocked(&self, review_id: &str) -> Vec<ProxyHit> {
        let Ok(map) = self.proxies.lock() else {
            return Vec::new();
        };
        map.get(review_id)
            .map(|p| {
                p.take_blocked()
                    .into_iter()
                    .map(|h| ProxyHit {
                        at_ms: h.at_ms,
                        write: h.reason == ProxyBlock::Write,
                        request: BlockedRequest {
                            method: h.method,
                            url: h.url,
                        },
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn set_proxy_writes(&self, review_id: &str, allowed: bool) {
        if let Ok(map) = self.proxies.lock()
            && let Some(p) = map.get(review_id)
        {
            p.set_write_allowed(allowed);
        }
    }

    async fn login(&self, req: LabLogin<'_>) -> Result<PathBuf, LoginFailure> {
        let Some(bin) = &self.playwright else {
            return Err(LoginFailure::NotInstalled);
        };
        let secrets = lab_secrets::load(&self.lab_root, req.repo_id)
            .await
            .map_err(|_| LoginFailure::NoPassword)?;
        let Some(password) = secrets.password(req.role) else {
            return Err(LoginFailure::NoPassword);
        };
        let Some(account) = req.equipment.accounts.iter().find(|a| a.role == req.role) else {
            return Err(LoginFailure::NoPassword);
        };
        let dir = self.sessions_dir(req.review_id);
        let work = dir.join(format!("work-{}", safe_name(req.role)));
        let out = dir.join(format!("{}.json", safe_name(req.role)));
        private_dirs(&[&dir, &work])
            .await
            .map_err(|_| LoginFailure::Failed)?;
        let result = playwright_login::login(&LoginRequest {
            bin,
            form: &req.equipment.login,
            username: &account.username,
            password,
            origins: req.origins,
            proxy: req.proxy,
            work_dir: &work,
            out: &out,
            env: self.env.clone(),
        })
        .await;
        remove_dir(work).await;
        match result {
            Ok(()) => {
                tracing::info!(
                    review = req.review_id,
                    role = req.role,
                    "login akun uji berhasil"
                );
                Ok(out)
            }
            Err(c) => {
                tracing::warn!(review = req.review_id, role = req.role, reason = %c, "login akun uji gagal");
                Err(failure(c))
            }
        }
    }

    async fn redact(
        &self,
        repo_ids: &[String],
        review_id: &str,
        texts: Vec<String>,
    ) -> Vec<String> {
        let mut values: Vec<String> = Vec::new();
        for repo in repo_ids {
            if let Ok(s) = lab_secrets::load(&self.lab_root, repo).await {
                values.extend(s.exposed_values());
            }
        }
        values.extend(session_values(&self.sessions_dir(review_id)).await);
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        texts
            .iter()
            .map(|t| mandor_domain::lab::redact_values(t, &refs))
            .collect()
    }

    async fn end_review(&self, review_id: &str) {
        let proxy = self
            .proxies
            .lock()
            .ok()
            .and_then(|mut m| m.remove(review_id));
        if let Some(p) = proxy {
            p.stop();
        }
        remove_dir(self.sessions_dir(review_id)).await;
    }
}

/// Nilai cookie dan `localStorage` di berkas sesi review, untuk penyamaran.
async fn session_values(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(Ok(mut entries)) = tokio::time::timeout(FILE_TIMEOUT, tokio::fs::read_dir(dir)).await
    else {
        return out;
    };
    while let Ok(Ok(Some(e))) = tokio::time::timeout(FILE_TIMEOUT, entries.next_entry()).await {
        if e.path().extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(Ok(body)) = tokio::time::timeout(FILE_TIMEOUT, tokio::fs::read(e.path())).await
        else {
            continue;
        };
        out.extend(state_values(&body));
    }
    out
}

/// Nilai cookie dan `localStorage` dari satu `storageState`.
fn state_values(body: &[u8]) -> Vec<String> {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let cookies = v["cookies"].as_array().into_iter().flatten();
    let storage = v["origins"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|o| o["localStorage"].as_array().into_iter().flatten());
    cookies
        .chain(storage)
        .filter_map(|c| c["value"].as_str().map(str::to_owned))
        .collect()
}

fn failure(c: LoginCategory) -> LoginFailure {
    match c {
        LoginCategory::NotStarted => LoginFailure::NotInstalled,
        LoginCategory::Timeout => LoginFailure::Timeout,
        LoginCategory::FormNotFound => LoginFailure::FormNotFound,
        LoginCategory::NotSignedIn => LoginFailure::NotSignedIn,
        LoginCategory::SessionNotKept => LoginFailure::SessionNotKept,
        LoginCategory::Failed => LoginFailure::Failed,
    }
}

/// Folder 0700 (berkas sesi berisi cookie akun uji).
async fn private_dirs(dirs: &[&Path]) -> std::io::Result<()> {
    for d in dirs {
        let mut b = tokio::fs::DirBuilder::new();
        b.recursive(true);
        #[cfg(unix)]
        b.mode(0o700);
        tokio::time::timeout(FILE_TIMEOUT, b.create(d))
            .await
            .map_err(|_| std::io::Error::other("creating the lab folder timed out"))??;
    }
    Ok(())
}

async fn remove_dir(dir: PathBuf) {
    let removal = tokio::fs::remove_dir_all(&dir);
    match tokio::time::timeout(FILE_TIMEOUT, removal).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
        Ok(Err(e)) => {
            tracing::warn!(error = %e, dir = %dir.display(), "folder sesi lab tidak terhapus");
        }
        Err(_) => {
            tracing::warn!(dir = %dir.display(), "penghapusan folder sesi lab melewati batas waktu");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nama_aman_untuk_path() {
        assert_eq!(safe_name("rv-1_a"), "rv-1_a");
        assert_eq!(safe_name("../x"), "___x");
        assert_eq!(safe_name("a/b c"), "a_b_c");
    }

    #[test]
    fn nilai_sesi_dibaca_dari_storage_state() {
        let body = br#"{"cookies":[{"name":"sesi","value":"tok-abc-123"}],
            "origins":[{"origin":"http://x","localStorage":[{"name":"jwt","value":"eyJhbGciOi"}]}]}"#;
        assert_eq!(state_values(body), ["tok-abc-123", "eyJhbGciOi"]);
        assert!(state_values(b"bukan json").is_empty());
    }

    #[test]
    fn kategori_login_dipetakan_tanpa_teks() {
        assert_eq!(
            failure(LoginCategory::NotSignedIn),
            LoginFailure::NotSignedIn
        );
        assert_eq!(
            failure(LoginCategory::NotStarted),
            LoginFailure::NotInstalled
        );
    }

    #[tokio::test]
    async fn login_tanpa_password_tidak_menjalankan_playwright() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let store = Store::open(&dir.path().join("m.db"))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let lab = ProdLab::new(
            store,
            dir.path().join("lab"),
            Some("/bin/false".into()),
            Vec::new(),
            TaskTracker::new(),
            CancellationToken::new(),
        );
        let eq = LabEquipment {
            accounts: vec![mandor_domain::lab::LabAccount {
                role: "admin".to_owned(),
                username: "a@uji.test".to_owned(),
                default: true,
            }],
            login: mandor_domain::lab::LoginForm {
                url: "http://localhost:3000/login".to_owned(),
                username_field: "#email".to_owned(),
                password_field: "#password".to_owned(),
                submit: "text=Sign in".to_owned(),
                submit_path: "/api/login".to_owned(),
                success: mandor_domain::lab::SuccessMarker::PathPrefix("/admin".to_owned()),
                authenticated_url: "http://localhost:3000/admin".to_owned(),
            },
            safe_posts: Vec::new(),
            fixtures_dir: None,
        };
        let r = lab
            .login(LabLogin {
                review_id: "rv1",
                repo_id: "repo1",
                equipment: &eq,
                role: "admin",
                origins: &[],
                proxy: None,
            })
            .await;
        assert_eq!(r, Err(LoginFailure::NoPassword));
        let none = ProdLab::new(
            Store::open(&dir.path().join("n.db"))
                .await
                .unwrap_or_else(|e| panic!("{e}")),
            dir.path().join("lab2"),
            None,
            Vec::new(),
            TaskTracker::new(),
            CancellationToken::new(),
        );
        assert!(!none.playwright_installed());
    }

    #[tokio::test]
    async fn proxy_dimulai_dan_dihentikan_per_review() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let store = Store::open(&dir.path().join("m.db"))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let lab = ProdLab::new(
            store,
            dir.path().join("lab"),
            None,
            Vec::new(),
            TaskTracker::new(),
            CancellationToken::new(),
        );
        let url = lab
            .start_proxy(
                "rv1",
                ProxyPolicy::new(&["http://localhost:3000".to_owned()], &[]),
            )
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(url.starts_with("http://127.0.0.1:"));
        assert!(lab.take_blocked("rv1").is_empty());
        lab.end_review("rv1").await;
        assert!(lab.proxies.lock().is_ok_and(|m| m.is_empty()));
    }
}
