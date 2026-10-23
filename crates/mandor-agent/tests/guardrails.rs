//! Environment agent memakai config git/gh terisolasi: credential helper global
//! tidak terpakai sehingga push dari environment agent tidak bisa autentikasi.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use mandor_agent::claude::guardrails::{agent_env, prepare_agent_home};
use mandor_proc::{Cmd, EnvPolicy, run};

#[tokio::test]
async fn credential_helper_global_tidak_terlihat_dari_environment_agent() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join(".gitconfig"),
        "[credential]\n\thelper = store\n[url \"https://x-access-token:ghp_rahasia@github.com/\"]\n\tinsteadOf = https://github.com/\n",
    )
    .unwrap();
    std::fs::write(
        home.join(".git-credentials"),
        "https://dev:ghp_rahasia@example.test\n",
    )
    .unwrap();

    let mut login = BTreeMap::new();
    login.insert("HOME".to_owned(), home.display().to_string());
    login.insert("PATH".to_owned(), std::env::var("PATH").unwrap());
    login.insert("GH_TOKEN".to_owned(), "ghp_rahasia".to_owned());

    // Pembanding: dengan env login biasa, helper global terlihat. Config sistem
    // diabaikan agar pembanding tidak bergantung pada mesin (mis. macOS
    // membawa `credential.helper=osxkeychain` dari Xcode).
    let mut plain: Vec<(String, String)> = login.clone().into_iter().collect();
    plain.push(("GIT_CONFIG_NOSYSTEM".to_owned(), "1".to_owned()));
    let out = run(&Cmd::new("git")
        .args(["config", "--get-all", "credential.helper"])
        .env_policy(EnvPolicy::Explicit(plain)))
    .await
    .unwrap();
    assert_eq!(out.stdout.trim(), "store");

    let agent_home = prepare_agent_home(&dir.path().join("mandor"))
        .await
        .unwrap();
    let env = agent_env(&login, &agent_home);
    assert!(!env.iter().any(|(k, _)| k == "GH_TOKEN"));

    let out = run(&Cmd::new("git")
        .args(["config", "--get-all", "credential.helper"])
        .env_policy(EnvPolicy::Explicit(env.clone())))
    .await
    .unwrap();
    assert!(!out.stdout.contains("store"), "{}", out.stdout);
    let out = run(&Cmd::new("git")
        .args(["config", "--get-regexp", "^url\\."])
        .env_policy(EnvPolicy::Explicit(env.clone())))
    .await
    .unwrap();
    assert!(!out.stdout.contains("ghp_rahasia"));

    // Meminta kredensial secara eksplisit gagal: tidak ada helper dan prompt dimatikan.
    let fill = Cmd::new("git")
        .args(["credential", "fill"])
        .env_policy(EnvPolicy::Explicit(env))
        .stdin_bytes("protocol=https\nhost=example.test\n\n");
    let out = run(&fill).await.unwrap();
    assert!(!out.success());
    assert!(!out.stdout.contains("ghp_rahasia"));
}

/// A3: input hook Claude in Chrome yang direkam dari `claude -p --chrome`
/// (Claude Code 2.1.307). Setiap respons tool yang memakai tab memuat
/// "Tab Context" dengan alamat tab, tetapi respons klik bisa masih
/// menampilkan alamat sebelum navigasi selesai.
fn fixture(kind: &str) -> Vec<mandor_domain::hook::ToolCall> {
    let path = format!(
        "{}/tests/stream_fixtures/chrome_hooks/{kind}.jsonl",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| mandor_agent::claude::guardrails::parse_hook_input(l.as_bytes()).unwrap())
        .collect()
}

#[test]
fn fixture_browser_terbaca_dan_pagar_review_menolak_bacaan_di_luar_origin() {
    use mandor_domain::hook::Verdict;
    use mandor_domain::hook_review::{
        ReviewState, classify_review, mark_before, origin_of, record_post,
    };

    let pre = fixture("pre");
    let post = fixture("post");
    assert_eq!(pre.len(), post.len());
    // Setiap tool pada tab membawa tabId; respons mencantumkan alamat tab itu.
    for p in post
        .iter()
        .filter(|p| !p.tool_name.ends_with("tabs_close_mcp"))
    {
        if let Some(tab) = p.tab_id {
            let text = p.response.as_deref().unwrap();
            assert!(text.contains(&format!("tabId {tab}:")), "{}", p.tool_name);
        }
    }
    assert_eq!(
        pre[2].url.as_deref(),
        Some("http://localhost:47811/index.html")
    );
    assert_eq!(pre[6].action.as_deref(), Some("left_click"));

    let mut state = ReviewState {
        origins: vec![origin_of("http://localhost:47811").unwrap()],
        ..ReviewState::default()
    };
    // Diputar ulang seperti yang benar-benar dieksekusi; yang dicek adalah
    // keputusan pagar di setiap langkah.
    let mut verdicts = Vec::new();
    for (p, q) in pre.iter().zip(&post) {
        let v = classify_review(p, &state, &[]);
        verdicts.push(matches!(v, Verdict::Allow));
        mark_before(p, &mut state);
        record_post(q, &mut state);
    }
    let names: Vec<String> = pre
        .iter()
        .map(|c| {
            let short = c.tool_name.trim_start_matches("mcp__claude-in-chrome__");
            match &c.action {
                Some(a) => format!("{short}:{a}"),
                None => short.to_owned(),
            }
        })
        .collect();
    let expected = [
        ("tabs_context_mcp", true),
        ("tabs_create_mcp", true),
        ("navigate", true),
        ("find", true),
        ("form_input", true),
        // Setiap interaksi membuat alamat belum terkonfirmasi.
        ("find", false),
        ("computer:left_click", false),
        // Bacaan dan interaksi menunggu `computer wait` atau
        // `tabs_context_mcp` (rekaman tidak memanggilnya).
        ("computer:screenshot", false),
        ("get_page_text", false),
        ("read_page", false),
        ("computer:left_click", false),
        ("get_page_text", false),
        ("navigate", true),
        ("computer:wait", true),
        // Redirect JavaScript tertunda tertangkap oleh post `wait`.
        ("get_page_text", false),
        ("tabs_close_mcp", true),
    ];
    let got: Vec<(String, bool)> = names.into_iter().zip(verdicts).collect();
    let want: Vec<(String, bool)> = expected
        .iter()
        .map(|(n, a)| ((*n).to_owned(), *a))
        .collect();
    assert_eq!(got, want);
    assert!(state.tabs.is_empty(), "tab ditutup");
}
