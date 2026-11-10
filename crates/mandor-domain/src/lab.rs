//! Review lab (plan 2026-11-10-001): perlengkapan lab per repo, kebutuhan lab
//! per AC, bukti terstruktur, temuan, dan rekomendasi tampilan.
//!
//! Modul ini hanya aturan murni. Password akun uji tidak pernah ada di sini:
//! perlengkapan hanya memuat nama role dan username (R11).

use std::net::Ipv4Addr;

use serde_json::{Value, json};

use crate::ac_ledger::{AcRow, Verdict};
use crate::hook_review::origin_of;

/// Alasan AC perlu lab, diisi reviewer biasa bersama *Can't verify* (KTD-L7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabNeed {
    /// Butuh login sebagai role lain.
    Role,
    /// Butuh simulasi request: lambat, gagal, 403, 5xx.
    Simulation,
    /// Butuh lampiran berkas uji.
    File,
    /// Butuh data uji di database lokal (fase 2).
    Data,
}

impl LabNeed {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Simulation => "simulation",
            Self::File => "file",
            Self::Data => "data",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "role" => Self::Role,
            "simulation" => Self::Simulation,
            "file" => Self::File,
            "data" => Self::Data,
            _ => return None,
        })
    }
}

/// `lab_need` hanya sah bersama verdict *Can't verify*.
///
/// # Errors
/// [`LabNeedError::NotCantVerify`] untuk verdict lain.
pub fn check_lab_need(verdict: Verdict, need: Option<LabNeed>) -> Result<(), LabNeedError> {
    match need {
        Some(_) if verdict != Verdict::CantVerify => Err(LabNeedError::NotCantVerify),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LabNeedError {
    #[error("lab_need is only allowed with the cant_verify verdict")]
    NotCantVerify,
}

/// Cara keadaan awal AC disiapkan (R17, R20). Pass bertanda simulasi tetap
/// Pass penuh; tanda hanya membuatnya terlihat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Simulated {
    #[default]
    None,
    /// Request dicegat atau digagalkan di halaman.
    Request,
    /// Data disiapkan lewat SQL atau seed (diisi daemon, KTD-L12).
    Sql,
}

impl Simulated {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Request => "request",
            Self::Sql => "sql",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Self::None,
            "request" => Self::Request,
            "sql" => Self::Sql,
            _ => return None,
        })
    }
}

/// Request yang ditolak proxy lab (KTD-L6); diisi daemon, bukan reviewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockedRequest {
    pub method: String,
    pub url: String,
}

/// Bukti terstruktur satu verdict (R16, R20, KTD-L12).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    /// Langkah bernomor, urut.
    pub steps: Vec<String>,
    /// Nama berkas screenshot salinan daemon.
    pub screenshots: Vec<String>,
    /// Data, akun, dan simulasi yang disiapkan.
    pub prepared: Option<String>,
    pub simulated: Simulated,
    pub blocked_requests: Vec<BlockedRequest>,
    /// Verdict dicatat oleh giliran reviewer lab (badge "Lab"); diisi daemon.
    pub lab: bool,
}

/// Temuan reviewer di luar AC (R19); tidak mengubah verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Kartu asal (`record_id`) di run grup.
    pub card: Option<String>,
    pub title: String,
    pub detail: String,
    pub evidence: Evidence,
}

/// Rekomendasi reviewer untuk baris *looks different* (R18, KTD-L13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookAdvice {
    /// App mengikuti AC; perbedaan dengan prototype diterima.
    Accept,
    /// App diubah mengikuti prototype.
    FollowPrototype,
}

impl LookAdvice {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::FollowPrototype => "follow_prototype",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "accept" => Self::Accept,
            "follow_prototype" => Self::FollowPrototype,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookRecommendation {
    pub advice: LookAdvice,
    pub reason: String,
}

/// Satu akun uji. Password disimpan terpisah oleh daemon (KTD-L2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabAccount {
    pub role: String,
    pub username: String,
    /// Akun yang dipakai review biasa (R15).
    pub default: bool,
}

/// Tanda login berhasil (KTD-L4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuccessMarker {
    /// Path halaman setelah login diawali nilai ini, mis. `/admin`.
    PathPrefix(String),
    /// Teks yang tampil setelah login.
    Text(String),
}

/// Form login satu halaman di origin app (KTD-L3, KTD-L4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginForm {
    pub url: String,
    pub username_field: String,
    pub password_field: String,
    pub submit: String,
    /// Path request POST yang dikirim form; otomatis lolos proxy.
    pub submit_path: String,
    pub success: SuccessMarker,
    /// Halaman yang hanya bisa dibuka setelah login, untuk verifikasi sesi.
    pub authenticated_url: String,
}

/// Perlengkapan lab per repo (KTD-L3). Bagian data (fase 2) ditambah kemudian.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabEquipment {
    pub accounts: Vec<LabAccount>,
    pub login: LoginForm,
    /// Path POST baca-saja yang boleh lewat proxy di luar sesi data (KTD-L6).
    pub safe_posts: Vec<String>,
    /// Subfolder repo berisi berkas uji (KTD-L11).
    pub fixtures_dir: Option<String>,
}

/// Batas jumlah akun per sesi lab (KTD-L5).
pub const MAX_ACCOUNTS: usize = 4;
/// Panjang maksimum selector dan teks penanda (KTD-L4).
pub const MAX_SELECTOR_LEN: usize = 120;
const MAX_ROLE_LEN: usize = 16;
const MAX_PATH_LEN: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EquipmentError {
    #[error("add at least one test account")]
    NoAccounts,
    #[error("use at most {MAX_ACCOUNTS} test accounts")]
    TooManyAccounts,
    #[error(
        "role name `{0}` must be 1-16 lowercase letters, digits or dashes, starting with a letter or digit"
    )]
    BadRole(String),
    #[error("role `{0}` is listed twice")]
    DuplicateRole(String),
    #[error("username for role `{0}` is empty")]
    EmptyUsername(String),
    #[error("mark exactly one account as the default for ordinary review")]
    DefaultCount,
    #[error("{0} must be a page of the app")]
    NotAppUrl(&'static str),
    #[error(
        "{0} must be `#id`, `[name=field]` or `text=Label`, at most {MAX_SELECTOR_LEN} characters"
    )]
    BadSelector(&'static str),
    #[error("{0} must be a path that starts with `/`, without `..`, `?` or `#`")]
    BadPath(&'static str),
    #[error("the test files folder must be a subfolder of the repo, without `..`")]
    BadFixturesDir,
}

/// Memeriksa perlengkapan sebelum disimpan atau disetujui.
///
/// # Errors
/// [`EquipmentError`] pertama yang ditemukan.
pub fn validate_equipment(eq: &LabEquipment, app_origins: &[String]) -> Result<(), EquipmentError> {
    check_accounts(&eq.accounts)?;
    check_login(&eq.login, app_origins)?;
    for p in &eq.safe_posts {
        check_path(p, "Each read-only POST path")?;
    }
    if let Some(dir) = &eq.fixtures_dir {
        check_fixtures_dir(dir)?;
    }
    Ok(())
}

fn check_accounts(accounts: &[LabAccount]) -> Result<(), EquipmentError> {
    if accounts.is_empty() {
        return Err(EquipmentError::NoAccounts);
    }
    if accounts.len() > MAX_ACCOUNTS {
        return Err(EquipmentError::TooManyAccounts);
    }
    for (i, a) in accounts.iter().enumerate() {
        if !valid_role(&a.role) {
            return Err(EquipmentError::BadRole(a.role.clone()));
        }
        if accounts[..i].iter().any(|b| b.role == a.role) {
            return Err(EquipmentError::DuplicateRole(a.role.clone()));
        }
        if a.username.trim().is_empty() {
            return Err(EquipmentError::EmptyUsername(a.role.clone()));
        }
    }
    if accounts.iter().filter(|a| a.default).count() != 1 {
        return Err(EquipmentError::DefaultCount);
    }
    Ok(())
}

/// `^[a-z0-9][a-z0-9-]{0,15}$`: aman sebagai nama berkas sesi dan bagian nama
/// server MCP `playwright_<role>` (tanpa `_`, nama tool tetap ≤ 64 karakter).
#[must_use]
pub fn valid_role(role: &str) -> bool {
    let mut chars = role.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && role.len() <= MAX_ROLE_LEN
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn check_login(login: &LoginForm, app_origins: &[String]) -> Result<(), EquipmentError> {
    if !on_app(&login.url, app_origins) {
        return Err(EquipmentError::NotAppUrl("The login page"));
    }
    if !on_app(&login.authenticated_url, app_origins) {
        return Err(EquipmentError::NotAppUrl("The signed-in page"));
    }
    for (value, what) in [
        (&login.username_field, "The username field"),
        (&login.password_field, "The password field"),
        (&login.submit, "The submit button"),
    ] {
        if !valid_selector(value) {
            return Err(EquipmentError::BadSelector(what));
        }
    }
    check_path(&login.submit_path, "The login submit path")?;
    match &login.success {
        SuccessMarker::PathPrefix(p) => check_path(p, "The signed-in path")?,
        SuccessMarker::Text(t) if !plain_text(t) => {
            return Err(EquipmentError::BadSelector("The signed-in text"));
        }
        SuccessMarker::Text(_) => {}
    }
    Ok(())
}

fn on_app(url: &str, app_origins: &[String]) -> bool {
    origin_of(url).is_some_and(|o| {
        app_origins
            .iter()
            .filter_map(|a| origin_of(a))
            .any(|a| a == o)
    })
}

/// Tata bahasa selector sempit (KTD-L4): `#id`, `[name=field]`, `text=Label`.
/// Nilai ini hanya masuk template login sebagai data JSON, tetapi tata bahasa
/// sempit tetap membatasi apa yang bisa dicari di halaman.
#[must_use]
pub fn valid_selector(s: &str) -> bool {
    if s.len() > MAX_SELECTOR_LEN {
        return false;
    }
    if let Some(id) = s.strip_prefix('#') {
        return ident(id);
    }
    if let Some(name) = s.strip_prefix("[name=").and_then(|r| r.strip_suffix(']')) {
        return ident(name);
    }
    s.strip_prefix("text=").is_some_and(plain_text)
}

fn ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn plain_text(s: &str) -> bool {
    !s.trim().is_empty()
        && s.len() <= MAX_SELECTOR_LEN
        && s.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | ','))
}

fn check_path(p: &str, what: &'static str) -> Result<(), EquipmentError> {
    let ok = p.starts_with('/')
        && !p.starts_with("//")
        && p.len() <= MAX_PATH_LEN
        && !p.contains(['?', '#', '\\', ' '])
        && !p.split('/').any(|seg| seg == "..");
    if ok {
        Ok(())
    } else {
        Err(EquipmentError::BadPath(what))
    }
}

fn check_fixtures_dir(dir: &str) -> Result<(), EquipmentError> {
    let ok = !dir.is_empty()
        && !dir.starts_with('/')
        && !dir.contains('\\')
        && dir
            .trim_end_matches('/')
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..");
    if ok {
        Ok(())
    } else {
        Err(EquipmentError::BadFixturesDir)
    }
}

/// String kanonis perlengkapan untuk digest persetujuan (KTD-L3): urutan akun
/// dan path tidak memengaruhi hasil; isi field apa pun memengaruhinya.
#[must_use]
pub fn equipment_canonical(eq: &LabEquipment) -> String {
    let mut accounts: Vec<Value> = eq
        .accounts
        .iter()
        .map(|a| json!({"role": a.role, "username": a.username, "default": a.default}))
        .collect();
    accounts.sort_by_key(|a| a["role"].as_str().map(str::to_owned));
    let mut posts = eq.safe_posts.clone();
    posts.sort();
    let (marker_kind, marker) = match &eq.login.success {
        SuccessMarker::PathPrefix(p) => ("path", p),
        SuccessMarker::Text(t) => ("text", t),
    };
    json!({
        "accounts": accounts,
        "login": {
            "url": eq.login.url,
            "username_field": eq.login.username_field,
            "password_field": eq.login.password_field,
            "submit": eq.login.submit,
            "submit_path": eq.login.submit_path,
            "success": {"kind": marker_kind, "value": marker},
            "authenticated_url": eq.login.authenticated_url,
        },
        "safe_posts": posts,
        "fixtures_dir": eq.fixtures_dir,
    })
    .to_string()
}

/// Origin `http://` atau `https://` di mesin sendiri: `localhost`,
/// `*.localhost`, `127.0.0.0/8`, atau `[::1]`.
#[must_use]
pub fn is_loopback_origin(url: &str) -> bool {
    host_of(url).is_some_and(|h| is_loopback_host(&h))
}

/// Origin HTTP loopback: syarat lab di fase mana pun (KTD-L6), karena blokir
/// tulis proxy tidak bisa melihat isi terowongan HTTPS.
#[must_use]
pub fn is_http_loopback_origin(url: &str) -> bool {
    origin_of(url).is_some_and(|o| o.starts_with("http://")) && is_loopback_origin(url)
}

fn host_of(url: &str) -> Option<String> {
    let origin = origin_of(url)?;
    let authority = origin.split_once("://")?.1;
    let (host, _) = authority.rsplit_once(':')?;
    Some(host.to_owned())
}

/// Host loopback, tanpa menebak dari awalan (`127.evil.com` bukan loopback).
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || host
            .strip_suffix(".localhost")
            .is_some_and(|sub| !sub.is_empty() && !sub.starts_with('.'))
        || host == "[::1]"
        || host == "::1"
        || host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

/// Penanda pengganti nilai rahasia di bukti.
pub const REDACTED: &str = "[redacted]";

/// Kunci usulan perlengkapan yang tidak boleh ada: usulan agent tidak pernah
/// membawa password, secret, token, atau cookie (R13). Selektor
/// `password_field` boleh karena hanya menunjuk kolom di form.
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "cookie",
    "credential",
];

/// Jalur kunci pertama di usulan JSON yang tampak seperti rahasia, mis.
/// `accounts[0].password`; `None` bila bersih.
#[must_use]
pub fn secret_field(value: &serde_json::Value) -> Option<String> {
    fn walk(v: &serde_json::Value, path: &str) -> Option<String> {
        match v {
            serde_json::Value::Object(map) => map.iter().find_map(|(k, child)| {
                let here = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                let lower = k.to_ascii_lowercase();
                let secret =
                    lower != "password_field" && SECRET_KEYS.iter().any(|s| lower.contains(s));
                if secret {
                    Some(here)
                } else {
                    walk(child, &here)
                }
            }),
            serde_json::Value::Array(items) => items
                .iter()
                .enumerate()
                .find_map(|(i, child)| walk(child, &format!("{path}[{i}]"))),
            _ => None,
        }
    }
    walk(value, "")
}

/// Mengganti setiap kemunculan nilai rahasia di `text` (KTD-L12). Nilai
/// pendek (< 4 karakter) dilewati agar teks biasa tidak rusak; nilai panjang
/// diganti lebih dulu agar nilai yang memuat nilai lain tetap tersamarkan.
#[must_use]
pub fn redact_values(text: &str, values: &[&str]) -> String {
    let mut sorted: Vec<&str> = values
        .iter()
        .copied()
        .filter(|v| v.chars().count() >= 4)
        .collect();
    sorted.sort_by_key(|v| std::cmp::Reverse(v.len()));
    let mut out = text.to_owned();
    for v in sorted {
        if out.contains(v) {
            out = out.replace(v, REDACTED);
        }
    }
    out
}

/// Kebijakan proxy lab satu review (KTD-L6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyPolicy {
    /// Origin run yang boleh dihubungi (bentuk [`origin_of`]).
    pub origins: Vec<String>,
    /// Request tulis ke app diizinkan: hanya selama sesi lab data `running`.
    pub write_allowed: bool,
    /// Path POST baca-saja yang disetujui, termasuk path submit login.
    pub safe_posts: Vec<String>,
}

impl ProxyPolicy {
    /// Kebijakan untuk origin run; origin tidak sah dibuang.
    #[must_use]
    pub fn new(origins: &[String], safe_posts: &[String]) -> Self {
        let mut normalized: Vec<String> = origins.iter().filter_map(|o| origin_of(o)).collect();
        normalized.sort();
        normalized.dedup();
        Self {
            origins: normalized,
            write_allowed: false,
            safe_posts: safe_posts.to_vec(),
        }
    }
}

/// Request yang sampai di proxy lab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyRequest<'a> {
    /// Request HTTP biasa dengan URL absolut.
    Http { method: &'a str, url: &'a str },
    /// Terowongan `CONNECT host:port` (WebSocket dan HTTPS).
    Connect { authority: &'a str },
}

/// Alasan proxy menolak request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyBlock {
    /// Tujuan bukan origin run.
    OtherOrigin,
    /// Request tulis ke app di luar sesi lab data.
    Write,
}

/// Keputusan proxy lab (KTD-L6): hanya origin run, dan di luar sesi data
/// hanya method baca serta path POST baca-saja yang disetujui.
///
/// # Errors
/// [`ProxyBlock`] bila request ditolak.
pub fn proxy_decision(policy: &ProxyPolicy, req: ProxyRequest<'_>) -> Result<(), ProxyBlock> {
    match req {
        ProxyRequest::Connect { authority } => {
            let target = authority.to_ascii_lowercase();
            let scheme = policy
                .origins
                .iter()
                .filter_map(|o| o.split_once("://"))
                .find(|(_, a)| *a == target)
                .map(|(s, _)| s);
            match scheme {
                None => Err(ProxyBlock::OtherOrigin),
                // Terowongan mentah ke origin HTTP tidak bisa diperiksa proxy,
                // jadi hanya dibuka saat tulis diizinkan (sesi lab data).
                // WebSocket app HTTP tidak tersambung di luar sesi data.
                Some("http") if !policy.write_allowed => Err(ProxyBlock::Write),
                Some(_) => Ok(()),
            }
        }
        ProxyRequest::Http { method, url } => {
            let Some(origin) = origin_of(url) else {
                return Err(ProxyBlock::OtherOrigin);
            };
            if !policy.origins.contains(&origin) {
                return Err(ProxyBlock::OtherOrigin);
            }
            let method = method.to_ascii_uppercase();
            if matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS") || policy.write_allowed {
                return Ok(());
            }
            let safe = method == "POST" && policy.safe_posts.iter().any(|p| p == path_of(url));
            if safe { Ok(()) } else { Err(ProxyBlock::Write) }
        }
    }
}

/// Path URL tanpa query dan fragmen; `/` bila kosong.
fn path_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.find('/').map_or("/", |i| &rest[i..]);
    path.split(['?', '#']).next().unwrap_or("/")
}

/// AC calon lab untuk satu kartu (`None` di run tunggal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardCandidates {
    pub card: Option<String>,
    pub ac_ids: Vec<String>,
}

/// Calon lab per kartu, urut kemunculan pertama kartu (R1, R2, R3).
///
/// Hanya baris di gerbang dengan *Can't verify* dan `lab_need`; AC yang sudah
/// terbukti tidak diuji ulang. Calon `data` hanya masuk bila lab data siap.
#[must_use]
pub fn lab_candidates(rows: &[AcRow], data_ready: bool) -> Vec<CardCandidates> {
    let mut out: Vec<CardCandidates> = Vec::new();
    for row in rows {
        let Some(need) = row.lab_need else { continue };
        if !row.in_gate() || row.verdict != Verdict::CantVerify {
            continue;
        }
        if need == LabNeed::Data && !data_ready {
            continue;
        }
        match out.iter_mut().find(|c| c.card == row.card) {
            Some(c) => c.ac_ids.push(row.ac_id.clone()),
            None => out.push(CardCandidates {
                card: row.card.clone(),
                ac_ids: vec![row.ac_id.clone()],
            }),
        }
    }
    out
}

#[cfg(test)]
mod secret_tests {
    use super::secret_field;
    use serde_json::json;

    #[test]
    fn usulan_dengan_field_rahasia_dikenali() {
        let clean = json!({
            "accounts": [{"role": "admin", "username": "a@uji.test", "default": true}],
            "login": {"password_field": "#password", "submit": "text=Sign in"},
        });
        assert_eq!(secret_field(&clean), None);
        let with = |extra: serde_json::Value| {
            let mut v = clean.clone();
            v["accounts"][0]
                .as_object_mut()
                .unwrap_or_else(|| panic!("objek"))
                .extend(extra.as_object().cloned().unwrap_or_default());
            secret_field(&v)
        };
        assert_eq!(
            with(json!({"password": "rahasia"})).as_deref(),
            Some("accounts[0].password")
        );
        assert!(with(json!({"Password": "x"})).is_some());
        assert!(with(json!({"api_token": "x"})).is_some());
        assert!(with(json!({"session_cookie": "x"})).is_some());
        assert!(secret_field(&json!({"login": {"admin_secret": 1}})).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ac_ledger::RowStatus;
    use pretty_assertions::assert_eq;

    fn row(id: &str, card: Option<&str>, verdict: Verdict, need: Option<LabNeed>) -> AcRow {
        let mut r = AcRow::new(id, None, "teks", RowStatus::Active);
        r.card = card.map(str::to_owned);
        r.verdict = verdict;
        r.lab_need = need;
        r
    }

    const APP: &str = "http://stagingintegro.localhost:3000";

    fn account(role: &str, default: bool) -> LabAccount {
        LabAccount {
            role: role.to_owned(),
            username: format!("{role}@uji.test"),
            default,
        }
    }

    fn equipment() -> LabEquipment {
        LabEquipment {
            accounts: vec![account("admin", true), account("manager", false)],
            login: LoginForm {
                url: format!("{APP}/login"),
                username_field: "#email".to_owned(),
                password_field: "[name=password]".to_owned(),
                submit: "text=Sign in".to_owned(),
                submit_path: "/api/auth/login".to_owned(),
                success: SuccessMarker::PathPrefix("/admin".to_owned()),
                authenticated_url: format!("{APP}/admin/profile"),
            },
            safe_posts: vec!["/api/search".to_owned()],
            fixtures_dir: Some("review-fixtures".to_owned()),
        }
    }

    fn app() -> Vec<String> {
        vec![APP.to_owned()]
    }

    #[test]
    fn calon_lab_hanya_cant_verify_dengan_lab_need() {
        let rows = [
            row("a", None, Verdict::CantVerify, Some(LabNeed::Simulation)),
            row("b", None, Verdict::Pass, None),
            row("c", None, Verdict::Fail, None),
            row("d", None, Verdict::CantVerify, None),
        ];
        assert_eq!(
            lab_candidates(&rows, true),
            vec![CardCandidates {
                card: None,
                ac_ids: vec!["a".to_owned()]
            }]
        );
    }

    #[test]
    fn calon_data_dikecualikan_bila_lab_data_belum_siap() {
        let rows = [
            row("a", None, Verdict::CantVerify, Some(LabNeed::Data)),
            row("b", None, Verdict::CantVerify, Some(LabNeed::File)),
        ];
        let ids = |c: Vec<CardCandidates>| c.into_iter().flat_map(|c| c.ac_ids).collect::<Vec<_>>();
        assert_eq!(ids(lab_candidates(&rows, false)), vec!["b"]);
        assert_eq!(ids(lab_candidates(&rows, true)), vec!["a", "b"]);
    }

    #[test]
    fn run_grup_dikelompokkan_per_kartu_urut_kemunculan() {
        let rows = [
            row("a1", Some("k2"), Verdict::CantVerify, Some(LabNeed::Role)),
            row("b1", Some("k1"), Verdict::CantVerify, Some(LabNeed::File)),
            row(
                "a2",
                Some("k2"),
                Verdict::CantVerify,
                Some(LabNeed::Simulation),
            ),
            row("c1", Some("k3"), Verdict::Pass, None),
        ];
        let got = lab_candidates(&rows, false);
        assert_eq!(
            got,
            vec![
                CardCandidates {
                    card: Some("k2".to_owned()),
                    ac_ids: vec!["a1".to_owned(), "a2".to_owned()]
                },
                CardCandidates {
                    card: Some("k1".to_owned()),
                    ac_ids: vec!["b1".to_owned()]
                },
            ]
        );
    }

    #[test]
    fn baris_di_luar_gerbang_tidak_jadi_calon() {
        let mut r = row("a", None, Verdict::CantVerify, Some(LabNeed::Role));
        r.status = RowStatus::Removed;
        assert!(lab_candidates(&[r], true).is_empty());
    }

    #[test]
    fn lab_need_hanya_dengan_cant_verify() {
        assert!(check_lab_need(Verdict::CantVerify, Some(LabNeed::Role)).is_ok());
        assert!(check_lab_need(Verdict::Pass, None).is_ok());
        assert_eq!(
            check_lab_need(Verdict::Pass, Some(LabNeed::Role)),
            Err(LabNeedError::NotCantVerify)
        );
        assert_eq!(
            check_lab_need(Verdict::Fail, Some(LabNeed::Data)),
            Err(LabNeedError::NotCantVerify)
        );
    }

    #[test]
    fn pass_bertanda_simulasi_tetap_pass_penuh() {
        let r = row("a", None, Verdict::Pass, None);
        let mut with_sim = r.clone();
        with_sim.evidence_detail = Some(Evidence {
            simulated: Simulated::Request,
            ..Evidence::default()
        });
        assert_eq!(with_sim.effective_verdict(), Verdict::Pass);
    }

    #[test]
    fn perlengkapan_valid_diterima() {
        assert_eq!(validate_equipment(&equipment(), &app()), Ok(()));
    }

    #[test]
    fn nama_role_dibatasi() {
        for bad in [
            "../x",
            "Super Admin",
            "a__b",
            "-admin",
            "",
            "abcdefghijklmnopq",
            "a_b",
        ] {
            let mut eq = equipment();
            eq.accounts[1].role = bad.to_owned();
            assert_eq!(
                validate_equipment(&eq, &app()),
                Err(EquipmentError::BadRole(bad.to_owned())),
                "role {bad:?}"
            );
        }
        for good in ["admin", "non-admin", "a", "landlord-admin-1"] {
            assert!(valid_role(good), "role {good:?}");
        }
    }

    #[test]
    fn nama_tool_terpanjang_tetap_dalam_batas() {
        let role = "a".repeat(16);
        assert!(valid_role(&role));
        let name = format!("mcp__playwright_{role}__browser_file_upload");
        assert!(name.len() <= 64, "{} karakter", name.len());
    }

    #[test]
    fn lima_akun_ditolak() {
        let mut eq = equipment();
        eq.accounts = (0..5).map(|i| account(&format!("r{i}"), i == 0)).collect();
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::TooManyAccounts)
        );
    }

    #[test]
    fn akun_default_harus_tepat_satu() {
        let mut eq = equipment();
        eq.accounts[1].default = true;
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::DefaultCount)
        );
        eq.accounts[0].default = false;
        eq.accounts[1].default = false;
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::DefaultCount)
        );
    }

    #[test]
    fn role_ganda_dan_username_kosong_ditolak() {
        let mut eq = equipment();
        eq.accounts[1].role = "admin".to_owned();
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::DuplicateRole("admin".to_owned()))
        );
        let mut eq = equipment();
        eq.accounts[1].username = "  ".to_owned();
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::EmptyUsername("manager".to_owned()))
        );
        let mut eq = equipment();
        eq.accounts.clear();
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::NoAccounts)
        );
    }

    #[test]
    fn url_login_harus_di_origin_app() {
        let mut eq = equipment();
        eq.login.url = "https://accounts.example.com/login".to_owned();
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::NotAppUrl("The login page"))
        );
        let mut eq = equipment();
        eq.login.authenticated_url = "http://localhost:9999/admin".to_owned();
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::NotAppUrl("The signed-in page"))
        );
    }

    #[test]
    fn selector_berbahaya_ditolak() {
        for bad in [
            "'); process.exit(",
            "#a b",
            "div > input",
            "[name=x] , #y",
            "text=<script>",
            &format!("#{}", "a".repeat(121)),
        ] {
            assert!(!valid_selector(bad), "selector {bad:?}");
        }
        for good in [
            "#email",
            "[name=password]",
            "text=Sign in",
            "#login-form.v2",
        ] {
            assert!(valid_selector(good), "selector {good:?}");
        }
        let mut eq = equipment();
        eq.login.submit = "'); process.exit(".to_owned();
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::BadSelector("The submit button"))
        );
    }

    #[test]
    fn path_post_dan_penanda_harus_path_bersih() {
        for bad in [
            "api/search",
            "//evil.com/x",
            "/a/../b",
            "/a?x=1",
            "/a#b",
            "/a b",
        ] {
            let mut eq = equipment();
            eq.safe_posts = vec![bad.to_owned()];
            assert_eq!(
                validate_equipment(&eq, &app()),
                Err(EquipmentError::BadPath("Each read-only POST path")),
                "path {bad:?}"
            );
        }
        let mut eq = equipment();
        eq.login.success = SuccessMarker::Text("Welcome <b>".to_owned());
        assert_eq!(
            validate_equipment(&eq, &app()),
            Err(EquipmentError::BadSelector("The signed-in text"))
        );
    }

    #[test]
    fn folder_berkas_uji_harus_subfolder_repo() {
        for bad in [".", "", "../x", "/etc", "a/../b", "./", "a\\b"] {
            let mut eq = equipment();
            eq.fixtures_dir = Some(bad.to_owned());
            assert_eq!(
                validate_equipment(&eq, &app()),
                Err(EquipmentError::BadFixturesDir),
                "dir {bad:?}"
            );
        }
        let mut eq = equipment();
        eq.fixtures_dir = Some("review-fixtures/kb/".to_owned());
        assert_eq!(validate_equipment(&eq, &app()), Ok(()));
    }

    #[test]
    fn kanonis_berubah_bila_field_berubah_tetapi_tidak_karena_urutan() {
        let base = equipment_canonical(&equipment());
        let mut reordered = equipment();
        reordered.accounts.reverse();
        reordered.safe_posts = vec!["/api/search".to_owned()];
        assert_eq!(equipment_canonical(&reordered), base);
        let mut changed = equipment();
        changed.login.submit_path = "/api/auth/login2".to_owned();
        assert_ne!(equipment_canonical(&changed), base);
        let mut changed = equipment();
        changed.accounts[1].username = "lain@uji.test".to_owned();
        assert_ne!(equipment_canonical(&changed), base);
    }

    #[test]
    fn origin_loopback() {
        for local in [
            "http://localhost:3000",
            "http://127.0.0.1:5173/x",
            "http://stagingintegro.localhost:3000",
            "http://[::1]:8080",
            "https://localhost",
        ] {
            assert!(is_loopback_origin(local), "{local}");
        }
        for remote in [
            "https://staging.example.com",
            "http://localhost.evil.com",
            "http://127.evil.com:80",
            "http://10.0.0.5",
            "file:///etc/passwd",
        ] {
            assert!(!is_loopback_origin(remote), "{remote}");
        }
        assert!(is_http_loopback_origin("http://localhost:3000"));
        assert!(!is_http_loopback_origin("https://localhost:3000"));
    }

    fn policy(write: bool) -> ProxyPolicy {
        let mut p = ProxyPolicy::new(
            &[
                "http://localhost:3000/".to_owned(),
                "http://api.localhost:8080".to_owned(),
            ],
            &["/api/search".to_owned(), "/api/auth/login".to_owned()],
        );
        p.write_allowed = write;
        p
    }

    fn http<'a>(method: &'a str, url: &'a str) -> ProxyRequest<'a> {
        ProxyRequest::Http { method, url }
    }

    #[test]
    fn proxy_hanya_origin_run() {
        let p = policy(false);
        assert_eq!(
            proxy_decision(&p, http("GET", "http://localhost:3000/admin?x=1")),
            Ok(())
        );
        assert_eq!(
            proxy_decision(&p, http("GET", "http://api.localhost:8080/v1")),
            Ok(())
        );
        for url in [
            "https://example.com/",
            "http://127.0.0.1:7878/",
            "http://localhost:3001/",
            "https://localhost:3000/",
            "ftp://localhost:3000/",
        ] {
            assert_eq!(
                proxy_decision(&p, http("GET", url)),
                Err(ProxyBlock::OtherOrigin),
                "{url}"
            );
        }
    }

    #[test]
    fn proxy_menolak_tulis_di_luar_sesi_data() {
        let p = policy(false);
        for m in ["POST", "PUT", "PATCH", "DELETE", "post"] {
            assert_eq!(
                proxy_decision(&p, http(m, "http://localhost:3000/api/kb")),
                Err(ProxyBlock::Write),
                "{m}"
            );
        }
        assert_eq!(
            proxy_decision(&p, http("HEAD", "http://localhost:3000/x")),
            Ok(())
        );
        assert_eq!(
            proxy_decision(&p, http("OPTIONS", "http://localhost:3000/x")),
            Ok(())
        );
        assert_eq!(
            proxy_decision(
                &policy(true),
                http("DELETE", "http://localhost:3000/api/kb")
            ),
            Ok(())
        );
    }

    #[test]
    fn proxy_meneruskan_post_baca_saja_yang_disetujui() {
        let p = policy(false);
        assert_eq!(
            proxy_decision(&p, http("POST", "http://localhost:3000/api/search?q=a")),
            Ok(())
        );
        assert_eq!(
            proxy_decision(&p, http("POST", "http://localhost:3000/api/auth/login")),
            Ok(())
        );
        assert_eq!(
            proxy_decision(&p, http("PUT", "http://localhost:3000/api/search")),
            Err(ProxyBlock::Write)
        );
        assert_eq!(
            proxy_decision(&p, http("POST", "http://localhost:3000/api/search/extra")),
            Err(ProxyBlock::Write)
        );
    }

    #[test]
    fn connect_hanya_ke_host_port_origin_run() {
        let p = policy(true);
        let connect = |authority| ProxyRequest::Connect { authority };
        assert_eq!(proxy_decision(&p, connect("localhost:3000")), Ok(()));
        assert_eq!(proxy_decision(&p, connect("API.localhost:8080")), Ok(()));
        for a in ["example.com:443", "localhost:7878", "127.0.0.1:3000"] {
            assert_eq!(
                proxy_decision(&p, connect(a)),
                Err(ProxyBlock::OtherOrigin),
                "{a}"
            );
        }
    }

    #[test]
    fn connect_ke_origin_http_ditolak_di_luar_sesi_data() {
        let connect = |authority| ProxyRequest::Connect { authority };
        let p = policy(false);
        assert_eq!(
            proxy_decision(&p, connect("localhost:3000")),
            Err(ProxyBlock::Write)
        );
        let https = ProxyPolicy::new(&["https://app.test".to_owned()], &[]);
        assert_eq!(proxy_decision(&https, connect("app.test:443")), Ok(()));
    }

    #[test]
    fn nilai_rahasia_disamarkan() {
        let out = redact_values(
            "login a@uji.test dengan rahasia-9 dan cookie abcd1234efgh",
            &["rahasia-9", "abcd1234efgh", "ab", "rahasia-9-panjang"],
        );
        assert_eq!(
            out,
            "login a@uji.test dengan [redacted] dan cookie [redacted]"
        );
        assert_eq!(
            redact_values("x rahasia-9-panjang", &["rahasia-9", "rahasia-9-panjang"]),
            "x [redacted]"
        );
        assert_eq!(redact_values("abc", &["ab"]), "abc");
    }

    #[test]
    fn enum_bolak_balik() {
        for n in [
            LabNeed::Role,
            LabNeed::Simulation,
            LabNeed::File,
            LabNeed::Data,
        ] {
            assert_eq!(LabNeed::parse(n.as_str()), Some(n));
        }
        for s in [Simulated::None, Simulated::Request, Simulated::Sql] {
            assert_eq!(Simulated::parse(s.as_str()), Some(s));
        }
        for a in [LookAdvice::Accept, LookAdvice::FollowPrototype] {
            assert_eq!(LookAdvice::parse(a.as_str()), Some(a));
        }
        assert_eq!(LabNeed::parse("x"), None);
        assert_eq!(Simulated::parse("x"), None);
        assert_eq!(LookAdvice::parse("x"), None);
    }
}
