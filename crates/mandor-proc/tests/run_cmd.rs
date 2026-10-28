#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration tests: clippy.toml allowances miss #[tokio::test]
#![cfg(unix)]

use std::time::{Duration, Instant};

use mandor_proc::{Cmd, EnvPolicy, ProcError, run};

fn sh(script: &str) -> Cmd {
    Cmd::new("sh").arg("-c").arg(script)
}

#[tokio::test]
async fn menjalankan_perintah_dan_mengembalikan_output() {
    let out = run(&sh("echo halo; echo galat 1>&2; exit 3"))
        .await
        .unwrap();
    assert_eq!(out.code, Some(3));
    assert_eq!(out.stdout.trim(), "halo");
    assert_eq!(out.stderr.trim(), "galat");
    assert!(!out.success());
}

#[tokio::test]
async fn timeout_menghentikan_proses_dan_anaknya() {
    let marker = tempfile_path("anak");
    // Anak di background menulis marker setelah 2 detik bila masih hidup.
    let script = format!("(sleep 2; touch {marker}) & sleep 30");
    let started = Instant::now();
    let err = run(&sh(&script).timeout(Duration::from_millis(300)))
        .await
        .unwrap_err();
    assert!(matches!(err, ProcError::Timeout { .. }), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        !std::path::Path::new(&marker).exists(),
        "proses anak masih hidup setelah timeout"
    );
}

#[tokio::test]
async fn output_diredaksi_sebelum_dikembalikan() {
    let out = run(&sh(
        "echo 'fatal: https://x-access-token:ghp_abcdefghijklmnopqrstuvwxyz0123@github.com/o/r' 1>&2; exit 1",
    ))
    .await
    .unwrap();
    assert!(!out.stderr.contains("ghp_abcdefghijklmnopqrstuvwxyz0123"));
    assert!(out.stderr.contains("[REDACTED]"));
}

#[tokio::test]
async fn kebijakan_env_bersih_menghapus_token() {
    // SAFETY-free: env diset di proses anak lewat Cmd, bukan di proses test.
    let cmd = sh("echo \"gh=${GH_TOKEN:-kosong} git=${GITHUB_TOKEN:-kosong} x=${KEEP:-kosong}\"")
        .env_policy(EnvPolicy::Explicit(vec![
            ("GH_TOKEN".into(), "rahasia".into()),
            ("GITHUB_TOKEN".into(), "rahasia".into()),
            ("KEEP".into(), "ada".into()),
            ("PATH".into(), std::env::var("PATH").unwrap()),
        ]))
        .scrub_tokens();
    let out = run(&cmd).await.unwrap();
    assert_eq!(out.stdout.trim(), "gh=kosong git=kosong x=ada");
}

#[tokio::test]
async fn program_tidak_ada_menghasilkan_error_spawn() {
    let err = run(&Cmd::new("program-yang-tidak-ada-xyz"))
        .await
        .unwrap_err();
    assert!(matches!(err, ProcError::Spawn { .. }), "{err:?}");
}

fn tempfile_path(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("mandor-proc-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_file(&dir);
    dir.to_string_lossy().into_owned()
}

#[tokio::test]
async fn stop_bertahap_menghentikan_group_dengan_sigint() {
    use mandor_proc::{StopOutcome, Supervised};
    let mut p = Supervised::spawn(&sh("trap 'exit 0' INT; sleep 30 & wait")).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let outcome = p.stop(Duration::from_secs(2)).await;
    assert!(
        matches!(outcome, StopOutcome::Interrupted | StopOutcome::Terminated),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn stop_membunuh_paksa_proses_yang_mengabaikan_sinyal() {
    use mandor_proc::{StopOutcome, Supervised};
    let mut p = Supervised::spawn(&sh("trap '' INT TERM; while true; do sleep 1; done")).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let outcome = p.stop(Duration::from_millis(300)).await;
    assert_eq!(outcome, StopOutcome::Killed);
}

#[tokio::test]
async fn stdin_diteruskan_ke_proses() {
    let out = run(&Cmd::new("cat").stdin_bytes("masukan")).await.unwrap();
    assert_eq!(out.stdout, "masukan");
}

#[tokio::test]
async fn variabel_eksplisit_tetap_ada_walau_scrub() {
    let cmd = sh("echo \"gh=${GH_TOKEN:-kosong}\"")
        .env_policy(EnvPolicy::Explicit(vec![
            ("GH_TOKEN".into(), "warisan".into()),
            ("PATH".into(), std::env::var("PATH").unwrap()),
        ]))
        .scrub_tokens()
        .env("GH_TOKEN", "milik-daemon");
    let out = run(&cmd).await.unwrap();
    assert_eq!(out.stdout.trim(), "gh=milik-daemon");
}

#[tokio::test]
async fn stdout_rahasia_tidak_diredaksi_tetapi_stderr_tetap() {
    use secrecy::ExposeSecret;
    let token = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let out = mandor_proc::run_secret(&sh(&format!("echo {token}; echo {token} 1>&2")))
        .await
        .unwrap();
    assert!(out.success());
    assert_eq!(out.stdout.expose_secret().trim(), token);
    assert!(!out.stderr.contains(token));
}
