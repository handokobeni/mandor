//! Pagar hook mode review (KTD-31, R10, R29, R30): reviewer hanya membaca kode,
//! menjalankan test dari allowlist, dan memakai browser di tab miliknya pada
//! origin yang diizinkan.
//!
//! `PostToolUse` mencatat tab yang dibuat reviewer dan alamat terakhirnya dari
//! respons tool. Respons klik bisa masih menampilkan alamat lama karena
//! navigasi terjadi sesudahnya (A3), jadi setelah interaksi tab dianggap belum
//! terkonfirmasi: pembaca halaman ditolak sampai `computer wait` atau
//! `tabs_context_mcp` mencatat alamat yang diizinkan.

use std::collections::BTreeMap;

use crate::hook::{ToolCall, Verdict, classify_planning};

/// Awalan tool ekstensi Claude in Chrome.
pub const BROWSER_PREFIX: &str = "mcp__claude-in-chrome__";

/// Sub-perintah git yang hanya membaca; selain ini ditolak di mode review.
const GIT_READ_ONLY: &[&str] = &[
    "status",
    "diff",
    "log",
    "show",
    "blame",
    "ls-files",
    "rev-parse",
    "grep",
];

/// Keadaan satu tab milik reviewer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabState {
    /// Alamat terakhir yang terbaca; `None` = belum ada atau tidak diketahui.
    pub url: Option<String>,
    /// Alamat sudah dikonfirmasi setelah interaksi terakhir.
    pub settled: bool,
}

/// State pagar satu sesi review, milik daemon dan hook.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewState {
    /// Origin yang diizinkan, dinormalisasi lewat [`origin_of`].
    pub origins: Vec<String>,
    /// Tab yang dibuat reviewer.
    pub tabs: BTreeMap<u64, TabState>,
}

/// Jenis tool browser menurut pagar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Context,
    Create,
    Close,
    Navigate,
    /// Membaca isi halaman; screenshot termasuk.
    Reader,
    /// Bisa memicu navigasi.
    Interact,
    Wait,
    Denied,
}

fn kind(call: &ToolCall) -> Option<Kind> {
    let name = call.tool_name.strip_prefix(BROWSER_PREFIX)?;
    Some(match name {
        "tabs_context_mcp" => Kind::Context,
        "tabs_create_mcp" => Kind::Create,
        "tabs_close_mcp" => Kind::Close,
        "navigate" => Kind::Navigate,
        "get_page_text" | "read_page" | "find" => Kind::Reader,
        "form_input" => Kind::Interact,
        "computer" => match call.action.as_deref() {
            Some("screenshot" | "zoom") => Kind::Reader,
            Some("wait") => Kind::Wait,
            _ => Kind::Interact,
        },
        _ => Kind::Denied,
    })
}

/// Origin ketat `skema://host:port` dari URL `http`/`https`; port bawaan
/// ditulis eksplisit. URL dengan userinfo atau port tidak valid ditolak.
#[must_use]
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        let port = match after {
            "" => default_port,
            p => p.strip_prefix(':')?.parse::<u16>().ok()?,
        };
        (format!("[{h}]"), port)
    } else {
        match authority.split_once(':') {
            Some((h, p)) => (h.to_owned(), p.parse::<u16>().ok()?),
            None => (authority.to_owned(), default_port),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{}:{port}", host.to_ascii_lowercase()))
}

fn allowed(state: &ReviewState, url: Option<&str>) -> bool {
    url.and_then(origin_of)
        .is_some_and(|o| state.origins.contains(&o))
}

fn deny(reason: &str) -> Verdict {
    Verdict::Deny(format!("mandor: {reason}"))
}

/// Memutuskan tool call sesi reviewer.
#[must_use]
pub fn classify_review(call: &ToolCall, state: &ReviewState, allowed_mcp: &[String]) -> Verdict {
    if let Some(kind) = kind(call) {
        return browser_verdict(call, kind, state);
    }
    if let Some(cmd) = &call.command {
        if let Some(sub) = mutating_git(cmd) {
            return deny(&format!(
                "`git {sub}` changes the worktree; the review is read-only"
            ));
        }
    }
    match classify_planning(call, allowed_mcp) {
        Verdict::Deny(reason) if reason.contains("planning phase") => {
            deny("the review is read-only; report findings through the ledger tools")
        }
        other => other,
    }
}

fn browser_verdict(call: &ToolCall, kind: Kind, state: &ReviewState) -> Verdict {
    let tool = call.tool_name.trim_start_matches(BROWSER_PREFIX);
    match kind {
        Kind::Denied => return deny(&format!("`{tool}` is not available during review")),
        Kind::Context | Kind::Create => return Verdict::Allow,
        _ => {}
    }
    let Some(tab) = call.tab_id.and_then(|id| state.tabs.get(&id)) else {
        return deny("use only tabs you created with tabs_create_mcp");
    };
    match kind {
        Kind::Navigate if !allowed(state, call.url.as_deref()) => {
            deny("that address is outside the app allowed for this review")
        }
        Kind::Interact if !allowed(state, tab.url.as_deref()) => {
            deny("this tab is not on the allowed app; navigate to the app first")
        }
        Kind::Reader if !allowed(state, tab.url.as_deref()) => deny(
            "this tab is not on the allowed app (or its address is unknown); navigate to the app first",
        ),
        Kind::Reader if !tab.settled => deny(
            "the page may have changed after your last action; call computer wait or tabs_context_mcp first",
        ),
        _ => Verdict::Allow,
    }
}

/// Sub-perintah git yang mengubah state, bila ada.
fn mutating_git(cmd: &str) -> Option<String> {
    let lower = cmd.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || ";&|()".contains(c))
        .filter(|w| !w.is_empty())
        .collect();
    words.iter().enumerate().find_map(|(i, w)| {
        if w.rsplit('/').next() != Some("git") {
            return None;
        }
        let sub = words[i + 1..]
            .iter()
            .scan(false, |skip, w| {
                let out = if *skip { None } else { Some(*w) };
                *skip = matches!(*w, "-c" | "-C" | "--git-dir" | "--work-tree");
                Some(out)
            })
            .flatten()
            .find(|w| !w.starts_with('-'))?;
        (!GIT_READ_ONLY.contains(&sub)).then(|| sub.to_owned())
    })
}

/// Efek `PreToolUse` yang diizinkan: tool yang bisa memindah halaman membuat
/// tab belum terkonfirmasi sebelum tool berjalan, sehingga `PostToolUse` yang
/// gagal tetap gagal-tertutup.
pub fn mark_before(call: &ToolCall, state: &mut ReviewState) {
    if !matches!(kind(call), Some(Kind::Navigate | Kind::Interact)) {
        return;
    }
    if let Some(tab) = call.tab_id.and_then(|id| state.tabs.get_mut(&id)) {
        tab.settled = false;
    }
}

/// Mencatat hasil tool browser dari respons `PostToolUse`.
pub fn record_post(call: &ToolCall, state: &mut ReviewState) {
    let Some(kind) = kind(call) else {
        return;
    };
    let text = call.response.as_deref().unwrap_or_default();
    match kind {
        Kind::Create => {
            if let Some(id) = created_tab(text) {
                state.tabs.insert(id, TabState::default());
            }
        }
        Kind::Close => {
            if let Some(id) = call.tab_id {
                state.tabs.remove(&id);
            }
        }
        _ => {}
    }
    let listed = listed_tabs(text);
    for (id, tab) in &mut state.tabs {
        if let Some((_, url)) = listed.iter().find(|(t, _)| t == id) {
            tab.url = Some(url.clone()).filter(|u| !u.is_empty());
            if matches!(kind, Kind::Context | Kind::Navigate | Kind::Wait) {
                tab.settled = true;
            }
        } else if call.tab_id == Some(*id) {
            // Alamat tab yang dipakai tidak terbaca: gagal-tertutup.
            tab.url = None;
            tab.settled = false;
        }
    }
}

/// Id tab dari respons `tabs_create_mcp` ("Created new tab. Tab ID: 123").
fn created_tab(text: &str) -> Option<u64> {
    let rest = text.split("Tab ID:").nth(1)?;
    let digits: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Daftar tab dari bagian "Tab Context" respons:
/// `• tabId 123: "Judul" ("http://...")`.
fn listed_tabs(text: &str) -> Vec<(u64, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix('•')?.trim_start();
            let rest = rest.strip_prefix("tabId ")?;
            let (id, tail) = rest.split_once(':')?;
            let id = id.trim().parse().ok()?;
            let tail = tail.trim_end().strip_suffix("\")")?;
            let start = tail.rfind("(\"")?;
            Some((id, tail[start + 2..].to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "http://localhost:5173";

    fn state() -> ReviewState {
        ReviewState {
            origins: vec![origin_of(APP).unwrap()],
            tabs: BTreeMap::new(),
        }
    }

    fn browser(tool: &str, tab: Option<u64>) -> ToolCall {
        ToolCall {
            tool_name: format!("{BROWSER_PREFIX}{tool}"),
            tab_id: tab,
            ..ToolCall::default()
        }
    }

    fn context(tab: u64, url: &str) -> String {
        format!(
            "Done\n\nTab Context:\n- Executed on tabId: {tab}\n- Available tabs:\n  • tabId 1: \"New Tab\" (\"chrome://newtab/\")\n  • tabId {tab}: \"Judul (x)\" (\"{url}\")"
        )
    }

    fn post(call: &ToolCall, response: &str, s: &mut ReviewState) {
        let mut c = call.clone();
        c.response = Some(response.to_owned());
        record_post(&c, s);
    }

    /// Tab 7 milik reviewer, sudah di URL app dan terkonfirmasi.
    fn owned_on_app() -> ReviewState {
        let mut s = state();
        post(
            &browser("tabs_create_mcp", None),
            "Created new tab. Tab ID: 7",
            &mut s,
        );
        let mut nav = browser("navigate", Some(7));
        nav.url = Some(format!("{APP}/orders"));
        post(&nav, &context(7, &format!("{APP}/orders")), &mut s);
        s
    }

    #[test]
    fn origin_ketat_tanpa_pencocokan_awalan() {
        assert_eq!(origin_of("http://localhost:5173/x?y").as_deref(), Some(APP));
        assert_eq!(
            origin_of("HTTPS://Example.com/a").as_deref(),
            Some("https://example.com:443")
        );
        assert_eq!(origin_of("http://localhost:5173.evil.com/"), None);
        assert_eq!(origin_of("http://user@localhost:5173/"), None);
        assert_eq!(origin_of("javascript:alert(1)"), None);
        assert_eq!(origin_of("file:///etc/passwd"), None);
        assert_eq!(
            origin_of("http://[::1]:5173/").as_deref(),
            Some("http://[::1]:5173")
        );
        assert_ne!(origin_of("http://127.0.0.1:5173"), origin_of(APP));
        assert_ne!(origin_of("http://localhost:5174"), origin_of(APP));
    }

    #[test]
    fn covers_ae8_navigasi_hanya_ke_app_dan_hanya_di_tab_sendiri() {
        let s = owned_on_app();
        let mut nav = browser("navigate", Some(7));
        nav.url = Some("https://example.com".to_owned());
        assert!(matches!(classify_review(&nav, &s, &[]), Verdict::Deny(_)));
        for bad in [
            "http://localhost:5173.evil.com/",
            "http://localhost:5174/",
            "http://127.0.0.1:5173/",
        ] {
            nav.url = Some(bad.to_owned());
            assert!(
                matches!(classify_review(&nav, &s, &[]), Verdict::Deny(_)),
                "{bad}"
            );
        }
        nav.url = Some(format!("{APP}/settings"));
        assert_eq!(classify_review(&nav, &s, &[]), Verdict::Allow);
        // Tab yang tidak dibuat reviewer.
        let other = browser("get_page_text", Some(1));
        assert!(matches!(classify_review(&other, &s, &[]), Verdict::Deny(_)));
        assert!(matches!(
            classify_review(&browser("get_page_text", None), &s, &[]),
            Verdict::Deny(_)
        ));
        assert_eq!(
            classify_review(&browser("get_page_text", Some(7)), &s, &[]),
            Verdict::Allow
        );
    }

    #[test]
    fn tab_baru_diizinkan_setelah_tercatat_di_post() {
        let mut s = state();
        let mut nav = browser("navigate", Some(9));
        nav.url = Some(APP.to_owned());
        assert!(matches!(classify_review(&nav, &s, &[]), Verdict::Deny(_)));
        assert_eq!(
            classify_review(&browser("tabs_create_mcp", None), &s, &[]),
            Verdict::Allow
        );
        post(
            &browser("tabs_create_mcp", None),
            "Created new tab. Tab ID: 9",
            &mut s,
        );
        assert_eq!(classify_review(&nav, &s, &[]), Verdict::Allow);
        // Tab kosong belum di app: pembaca halaman ditolak.
        assert!(matches!(
            classify_review(&browser("read_page", Some(9)), &s, &[]),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn redirect_ke_luar_origin_menolak_pembaca_halaman() {
        let mut s = owned_on_app();
        let mut nav = browser("navigate", Some(7));
        nav.url = Some(format!("{APP}/redirect"));
        post(&nav, &context(7, "https://evil.example/landing"), &mut s);
        for reader in ["get_page_text", "read_page", "find"] {
            assert!(
                matches!(
                    classify_review(&browser(reader, Some(7)), &s, &[]),
                    Verdict::Deny(_)
                ),
                "{reader}"
            );
        }
    }

    #[test]
    fn klik_membuat_tab_belum_terkonfirmasi_sampai_wait() {
        let mut s = owned_on_app();
        let mut click = browser("computer", Some(7));
        click.action = Some("left_click".to_owned());
        assert_eq!(classify_review(&click, &s, &[]), Verdict::Allow);
        mark_before(&click, &mut s);
        // Respons klik masih menampilkan alamat lama (A3).
        post(&click, &context(7, &format!("{APP}/orders")), &mut s);
        let mut shot = browser("computer", Some(7));
        shot.action = Some("screenshot".to_owned());
        assert!(matches!(classify_review(&shot, &s, &[]), Verdict::Deny(_)));
        assert!(matches!(
            classify_review(&browser("get_page_text", Some(7)), &s, &[]),
            Verdict::Deny(_)
        ));
        // Wait mengonfirmasi alamat: navigasi klik ternyata keluar origin.
        let mut wait = browser("computer", Some(7));
        wait.action = Some("wait".to_owned());
        assert_eq!(classify_review(&wait, &s, &[]), Verdict::Allow);
        post(&wait, &context(7, "http://localhost:47812/other"), &mut s);
        assert!(matches!(
            classify_review(&browser("get_page_text", Some(7)), &s, &[]),
            Verdict::Deny(_)
        ));
        // Kembali ke app lewat navigate: pembaca halaman boleh lagi.
        let mut nav = browser("navigate", Some(7));
        nav.url = Some(APP.to_owned());
        mark_before(&nav, &mut s);
        post(&nav, &context(7, APP), &mut s);
        assert_eq!(classify_review(&shot, &s, &[]), Verdict::Allow);
    }

    #[test]
    fn respons_tanpa_alamat_menandai_tab_tidak_diketahui() {
        let mut s = owned_on_app();
        let form = browser("form_input", Some(7));
        mark_before(&form, &mut s);
        post(&form, "Set text value", &mut s);
        assert_eq!(s.tabs[&7].url, None);
        let mut wait = browser("computer", Some(7));
        wait.action = Some("wait".to_owned());
        post(&wait, "Waited", &mut s);
        assert!(matches!(
            classify_review(&browser("read_page", Some(7)), &s, &[]),
            Verdict::Deny(_)
        ));
        // Interaksi juga ditolak sampai navigasi ke app tercatat lagi.
        assert!(matches!(classify_review(&form, &s, &[]), Verdict::Deny(_)));
    }

    #[test]
    fn redirect_tertunda_tertangkap_post_berikutnya() {
        let mut s = owned_on_app();
        // Pembaca halaman berjalan saat redirect JS terjadi; post-nya mencatat
        // alamat baru, dan pembaca berikutnya ditolak.
        post(
            &browser("get_page_text", Some(7)),
            &context(7, "http://localhost:47812/other.html"),
            &mut s,
        );
        assert!(matches!(
            classify_review(&browser("get_page_text", Some(7)), &s, &[]),
            Verdict::Deny(_)
        ));
    }

    #[test]
    fn tool_browser_berbahaya_selalu_ditolak() {
        let s = owned_on_app();
        for tool in [
            "javascript_tool",
            "read_network_requests",
            "read_console_messages",
            "upload_image",
            "file_upload",
            "gif_creator",
            "shortcuts_execute",
            "switch_browser",
            "select_browser",
            "browser_batch",
            "tool_baru",
        ] {
            assert!(
                matches!(
                    classify_review(&browser(tool, Some(7)), &s, &[]),
                    Verdict::Deny(_)
                ),
                "{tool}"
            );
        }
        assert_eq!(
            classify_review(&browser("tabs_context_mcp", None), &s, &[]),
            Verdict::Allow
        );
    }

    #[test]
    fn mode_review_hanya_baca_kecuali_test() {
        let s = state();
        let bash = |cmd: &str| ToolCall {
            tool_name: "Bash".to_owned(),
            command: Some(cmd.to_owned()),
            ..ToolCall::default()
        };
        for cmd in [
            "git checkout main",
            "git stash",
            "cd web && git -C . reset --hard",
            "git add -A",
        ] {
            assert!(
                matches!(classify_review(&bash(cmd), &s, &[]), Verdict::Deny(_)),
                "{cmd}"
            );
        }
        for cmd in [
            "cargo test",
            "git diff main",
            "git -C web log -5",
            "pnpm test",
        ] {
            assert_eq!(
                classify_review(&bash(cmd), &s, &[]),
                Verdict::Allow,
                "{cmd}"
            );
        }
        let write = ToolCall {
            tool_name: "Edit".to_owned(),
            paths: vec!["/wt/a.rs".to_owned()],
            ..ToolCall::default()
        };
        let Verdict::Deny(reason) = classify_review(&write, &s, &[]) else {
            panic!("edit harus ditolak");
        };
        assert!(reason.contains("read-only"));
        let ledger = ToolCall {
            tool_name: "mcp__mandor__ac_set_verdict".to_owned(),
            ..ToolCall::default()
        };
        assert_eq!(
            classify_review(&ledger, &s, &["mandor".to_owned()]),
            Verdict::Allow
        );
    }

    #[test]
    fn daftar_tab_dari_respons_dengan_judul_berkurung() {
        let text = context(7, "http://localhost:5173/a?b=(1)");
        assert_eq!(
            listed_tabs(&text),
            [
                (1, "chrome://newtab/".to_owned()),
                (7, "http://localhost:5173/a?b=(1)".to_owned())
            ]
        );
        assert_eq!(
            created_tab("Created new tab. Tab ID: 1828590158"),
            Some(1_828_590_158)
        );
        assert_eq!(created_tab("gagal"), None);
    }
}
