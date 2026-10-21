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

    // Pembanding: dengan env login biasa, helper global terlihat.
    let plain: Vec<(String, String)> = login.clone().into_iter().collect();
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
