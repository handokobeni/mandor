//! Proses app nyata memakai skrip kecil yang membuka port lokal (U10).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use mandor_proc::{AppNotReady, AppProcess, AppSpec, app_marker, port_open, run_setup};

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn listen_after(port: u16, delay: f32) -> String {
    format!(
        "python3 -c \"import socket,time,sys;print('booting');sys.stdout.flush();time.sleep({delay});\
         s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);\
         s.bind(('127.0.0.1',{port}));s.listen();print('ready');sys.stdout.flush();time.sleep(60)\""
    )
}

fn spec(command: String, env: Vec<(String, String)>) -> AppSpec {
    AppSpec {
        marker: app_marker("r1"),
        cwd: std::env::temp_dir(),
        command,
        env,
    }
}

fn base_env() -> Vec<(String, String)> {
    vec![("PATH".to_owned(), std::env::var("PATH").unwrap())]
}

#[tokio::test]
async fn skrip_yang_membuka_port_menjadi_siap_dan_berhenti_bersih() {
    let port = free_port();
    let mut app = AppProcess::spawn(&spec(listen_after(port, 0.5), base_env())).unwrap();
    app.wait_ready(port, Duration::from_secs(10), Duration::from_millis(100))
        .await
        .unwrap();
    assert!(app.log().contains("booting"));
    let pid = app.pid().unwrap();
    let ps = std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&ps.stdout).contains("mandor-app-r1"));
    app.stop().await;
    assert!(!port_open(port).await, "group mati, port tertutup");
}

#[tokio::test]
async fn port_tidak_pernah_terbuka_gagal_setelah_batas_waktu_dengan_log() {
    let port = free_port();
    let mut app =
        AppProcess::spawn(&spec("echo menunggu; sleep 30".to_owned(), base_env())).unwrap();
    let err = app
        .wait_ready(port, Duration::from_millis(600), Duration::from_millis(100))
        .await
        .unwrap_err();
    assert_eq!(err, AppNotReady::Timeout(port));
    app.stop().await;
    assert!(app.log().contains("menunggu"));
}

#[tokio::test]
async fn keluar_langsung_gagal_dengan_log_yang_diredaksi_dan_env_bersih() {
    let port = free_port();
    let env = vec![
        ("PATH".to_owned(), std::env::var("PATH").unwrap()),
        ("API_URL".to_owned(), "http://localhost:8000".to_owned()),
    ];
    let mut app = AppProcess::spawn(&spec(
        "echo \"api=$API_URL home=${HOME:-none} gh=${GH_TOKEN:-none}\"; \
         echo token ghp_abcdefghijklmnopqrstuvwxyz0123456789 >&2; exit 1"
            .to_owned(),
        env,
    ))
    .unwrap();
    let err = app
        .wait_ready(port, Duration::from_secs(5), Duration::from_millis(100))
        .await
        .unwrap_err();
    assert_eq!(err, AppNotReady::Exited(Some(1)));
    app.stop().await;
    let log = app.log();
    assert!(
        log.contains("api=http://localhost:8000 home=none gh=none"),
        "{log}"
    );
    assert!(
        !log.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789"),
        "{log}"
    );
}

#[tokio::test]
async fn perintah_setup_dijalankan_dengan_batas_waktu() {
    let dir = tempfile::tempdir().unwrap();
    let out = run_setup(
        dir.path().to_path_buf(),
        "echo dipasang > hasil.txt",
        base_env(),
        Duration::from_secs(10),
    )
    .await
    .unwrap();
    assert!(out.success());
    assert!(dir.path().join("hasil.txt").exists());
    let slow = run_setup(
        dir.path().to_path_buf(),
        "sleep 5",
        base_env(),
        Duration::from_millis(300),
    )
    .await;
    assert!(slow.is_err());
}
