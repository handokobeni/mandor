//! Skenario U3 (plan service OS): `mandord install`, `install --check`, dan
//! `mandord uninstall` dengan HOME sementara serta `systemctl`/`loginctl` palsu.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_mandord");

/// Lingkungan uji: `HOME`, `MANDOR_HOME`, dan program palsu.
struct Env {
    _root: tempfile::TempDir,
    home: PathBuf,
    data: PathBuf,
    log: PathBuf,
    systemctl: PathBuf,
    loginctl: PathBuf,
}

/// Program palsu: mencatat argumen; `FAKE_NO_SYSTEMD` membuat
/// `show-environment` gagal, `FAKE_LINGER` memberi nilai linger.
fn fake(path: &Path, log: &Path, body: &str) {
    let script = format!(
        "#!/bin/sh\necho \"$(basename \"$0\") $*\" >> '{}'\n{body}\nexit 0\n",
        log.display()
    );
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn env() -> Env {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let data = home.join(".mandor");
    std::fs::create_dir_all(&home).unwrap();
    let log = root.path().join("calls.log");
    let systemctl = root.path().join("systemctl");
    let loginctl = root.path().join("loginctl");
    fake(
        &systemctl,
        &log,
        "case \"$*\" in \
           *show-environment*) [ -n \"$FAKE_NO_SYSTEMD\" ] && exit 1;; \
           *'start mandord.service'*) [ -n \"$FAKE_START_FAIL\" ] && exit 1; \
             [ -n \"$FAKE_START_BIN\" ] && INVOCATION_ID=fake setsid \"$FAKE_START_BIN\" serve \
               </dev/null >/dev/null 2>&1 & ;; \
         esac",
    );
    fake(
        &loginctl,
        &log,
        "case \"$*\" in *show-user*) echo \"${FAKE_LINGER:-no}\";; *enable-linger*) [ -n \"$FAKE_LINGER_FAIL\" ] && exit 1;; esac",
    );
    Env {
        _root: root,
        home,
        data,
        log,
        systemctl,
        loginctl,
    }
}

impl Env {
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env("HOME", &self.home)
            .env("MANDOR_HOME", &self.data)
            .env("MANDOR_SYSTEMCTL", &self.systemctl)
            .env("MANDOR_LOGINCTL", &self.loginctl)
            .env("USER", "beni")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("INVOCATION_ID")
            .env_remove("FAKE_NO_SYSTEMD")
            .env_remove("FAKE_LINGER")
            .env_remove("FAKE_LINGER_FAIL")
            .env_remove("FAKE_START_FAIL")
            .env_remove("FAKE_START_BIN")
            .env_remove("MANDOR_KEEP_AWAKE_CMD");
        c
    }

    fn run(&self, args: &[&str], extra: &[(&str, &str)]) -> (Value, Output) {
        let mut c = self.cmd(args);
        for (k, v) in extra {
            c.env(k, v);
        }
        let out = c.output().unwrap();
        let line = String::from_utf8_lossy(&out.stdout);
        let json = serde_json::from_str(line.trim()).unwrap_or_else(|e| {
            panic!(
                "keluaran bukan JSON ({e}): {line}\nstderr: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (json, out)
    }

    fn installed(&self) -> PathBuf {
        self.home.join(".local/bin/mandord")
    }

    fn unit(&self) -> PathBuf {
        self.home.join(".config/systemd/user/mandord.service")
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn clear_calls(&self) {
        let _ = std::fs::remove_file(&self.log);
    }

    fn serve(&self, invocation_id: Option<&str>) -> Child {
        let mut c = self.cmd(&["serve"]);
        if let Some(id) = invocation_id {
            c.env("INVOCATION_ID", id);
        }
        let child = c
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self.data.join("daemon.json").exists() {
            assert!(Instant::now() < deadline, "daemon.json tidak muncul");
            std::thread::sleep(Duration::from_millis(50));
        }
        child
    }
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn pemasangan_pertama_menyalin_biner_menulis_unit_dan_menyalakan_service() {
    let e = env();
    let (out, raw) = e.run(&["install"], &[("FAKE_LINGER", "yes")]);
    assert!(raw.status.success());
    assert_eq!(out["result"], "installed");
    assert_eq!(out["service"], "systemd");
    assert_eq!(out["linger"], true);

    assert_eq!(
        std::fs::read(e.installed()).unwrap(),
        std::fs::read(BIN).unwrap()
    );
    assert_eq!(mode(&e.installed()), 0o755);
    assert_eq!(mode(&e.unit()), 0o644);
    let unit = std::fs::read_to_string(e.unit()).unwrap();
    assert!(unit.contains(&format!("ExecStart=\"{}\" serve", e.installed().display())));
    assert!(unit.contains("Restart=always"));

    let calls = e.calls();
    assert!(calls.contains("systemctl --user daemon-reload"), "{calls}");
    assert!(
        calls.contains("systemctl --user enable --now mandord.service"),
        "{calls}"
    );
    assert!(!out.to_string().contains("token"));
}

#[test]
fn dijalankan_dua_kali_hasil_kedua_up_to_date_tanpa_perubahan() {
    let e = env();
    e.run(&["install"], &[("FAKE_LINGER", "yes")]);
    e.clear_calls();
    let (out, _) = e.run(&["install"], &[("FAKE_LINGER", "yes")]);
    assert_eq!(out["result"], "up_to_date");
    let calls = e.calls();
    assert!(!calls.contains("enable --now"), "{calls}");
    assert!(!calls.contains("daemon-reload"), "{calls}");
}

#[test]
fn perintah_keep_awake_masuk_unit_dan_perubahan_menulis_ulang_service() {
    let e = env();
    e.run(&["install"], &[]);
    e.clear_calls();
    let cmd = r#"["/mnt/c/Users/A B/mandor-keepawake.exe","--keep-awake"]"#;
    let (out, _) = e.run(&["install", "--keep-awake-cmd", cmd], &[]);
    assert_eq!(out["result"], "service_only");
    let unit = std::fs::read_to_string(e.unit()).unwrap();
    assert!(unit.contains(r#"Environment="MANDOR_KEEP_AWAKE_CMD=[\"/mnt/c/Users/A B/mandor-keepawake.exe\",\"--keep-awake\"]""#));
    assert!(e.calls().contains("enable --now"));
}

#[test]
fn tanpa_systemd_hanya_menyalin_biner() {
    let e = env();
    let (out, raw) = e.run(&["install"], &[("FAKE_NO_SYSTEMD", "1")]);
    assert!(raw.status.success());
    assert_eq!(out["result"], "installed");
    assert_eq!(out["service"], "none");
    assert!(e.installed().exists());
    assert!(!e.unit().exists());
    assert!(!e.calls().contains("enable"));
}

#[test]
fn linger_gagal_tetap_sukses_dengan_peringatan() {
    let e = env();
    let (out, raw) = e.run(&["install"], &[("FAKE_LINGER_FAIL", "1")]);
    assert!(raw.status.success());
    assert_eq!(out["result"], "installed");
    assert_eq!(out["linger"], false);
    assert!(!out["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn tujuan_symlink_ditolak_tanpa_menulis_apa_pun() {
    let e = env();
    let bin = e.home.join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let elsewhere = e.home.join("lain");
    std::fs::write(&elsewhere, b"bukan mandord").unwrap();
    std::os::unix::fs::symlink(&elsewhere, bin.join("mandord")).unwrap();

    let (out, raw) = e.run(&["install"], &[]);
    assert!(!raw.status.success());
    assert_eq!(out["result"], "error");
    assert_eq!(std::fs::read(&elsewhere).unwrap(), b"bukan mandord");
    assert!(!e.unit().exists());
}

#[test]
fn direktori_tujuan_symlink_ditolak() {
    let e = env();
    let real = e.home.join("real-bin");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::create_dir_all(e.home.join(".local")).unwrap();
    std::os::unix::fs::symlink(&real, e.home.join(".local/bin")).unwrap();

    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "error");
    assert!(!real.join("mandord").exists());
}

#[test]
fn berkas_sementara_tertinggal_tidak_merusak_biner_lama() {
    let e = env();
    e.run(&["install"], &[]);
    let before = std::fs::read(e.installed()).unwrap();
    std::fs::write(e.home.join(".local/bin/.mandord.tmp.1"), b"sisa").unwrap();
    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "up_to_date");
    assert_eq!(std::fs::read(e.installed()).unwrap(), before);
}

#[test]
fn biner_terpasang_berbeda_diperbarui() {
    let e = env();
    e.run(&["install"], &[]);
    // Biner lama: skrip yang mencetak versi lebih lama.
    std::fs::write(e.installed(), "#!/bin/sh\necho 'mandord 0.0.1'\n").unwrap();
    e.clear_calls();
    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "updated");
    assert_eq!(
        std::fs::read(e.installed()).unwrap(),
        std::fs::read(BIN).unwrap()
    );
}

#[test]
fn biner_terpasang_lebih_baru_tidak_diturunkan() {
    let e = env();
    e.run(&["install"], &[]);
    let newer = "#!/bin/sh\necho 'mandord 999.0.0'\n";
    std::fs::write(e.installed(), newer).unwrap();
    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "up_to_date");
    assert!(!out["warnings"].as_array().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(e.installed()).unwrap(), newer);
}

#[test]
fn check_melaporkan_tanpa_mengubah_apa_pun() {
    let e = env();
    let (out, raw) = e.run(&["install", "--check"], &[]);
    assert!(raw.status.success());
    assert_eq!(out["binary"], "missing");
    assert_eq!(out["service"], "missing");
    assert_eq!(out["systemd"], true);
    assert_eq!(out["running"], "no");
    assert!(!e.installed().exists());
    assert!(!e.calls().contains("enable"));

    e.run(&["install"], &[]);
    let (out, _) = e.run(&["install", "--check"], &[]);
    assert_eq!(out["binary"], "same");
    assert_eq!(out["service"], "matches");
    assert_eq!(out["installed_version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn daemon_systemd_idle_di_drain_lalu_service_di_restart() {
    let e = env();
    e.run(&["install"], &[]);
    std::fs::write(e.installed(), "#!/bin/sh\necho 'mandord 0.0.1'\n").unwrap();
    let mut daemon = e.serve(Some("abc"));
    e.clear_calls();

    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "updated");
    assert_eq!(out["apps_stopped"], true);
    let calls = e.calls();
    assert!(
        calls.contains("systemctl --user restart mandord.service"),
        "{calls}"
    );

    // systemctl palsu tidak benar-benar me-restart: hentikan daemon uji.
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn daemon_detached_idle_dihentikan_lalu_service_dinyalakan() {
    let e = env();
    let mut daemon = e.serve(None);
    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "installed");
    let status = daemon.wait().unwrap();
    assert!(
        !status.success() || status.code() == Some(0),
        "daemon berhenti"
    );
    assert!(!e.data.join("daemon.json").exists());
    assert!(e.calls().contains("enable --now"));
}

#[test]
fn pid_daemon_json_bukan_mandord_menghasilkan_error_tanpa_sinyal() {
    let e = env();
    let mut daemon = e.serve(None);
    let info_path = e.data.join("daemon.json");
    let mut info: Value = serde_json::from_slice(&std::fs::read(&info_path).unwrap()).unwrap();
    let real_pid = info["pid"].clone();
    // pid proses lain milik pengguna (sleep), bukan mandord.
    let mut other = Command::new("sleep").arg("30").spawn().unwrap();
    info["pid"] = json!(other.id());
    std::fs::write(&info_path, serde_json::to_vec(&info).unwrap()).unwrap();

    let (out, raw) = e.run(&["install"], &[]);
    assert!(!raw.status.success());
    assert_eq!(out["result"], "error");
    assert!(
        other.try_wait().unwrap().is_none(),
        "proses lain tidak boleh disinyal"
    );
    assert!(!e.calls().contains("enable --now"));

    let _ = other.kill();
    let _ = other.wait();
    info["pid"] = real_pid;
    std::fs::write(&info_path, serde_json::to_vec(&info).unwrap()).unwrap();
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn uninstall_menghentikan_service_menghapus_unit_dan_biner() {
    let e = env();
    e.run(&["install"], &[]);
    std::fs::create_dir_all(&e.data).unwrap();
    std::fs::write(e.data.join("lark.json"), b"{}").unwrap();
    e.clear_calls();

    let (out, raw) = e.run(&["uninstall"], &[("FAKE_LINGER", "yes")]);
    assert!(raw.status.success());
    assert_eq!(out["result"], "uninstalled");
    assert_eq!(out["linger"], true, "linger tidak diubah uninstall");
    assert!(!e.installed().exists());
    assert!(!e.unit().exists());
    assert!(e.data.join("lark.json").exists(), "data pengguna tetap");
    assert!(
        e.calls()
            .contains("systemctl --user disable --now mandord.service")
    );

    let (out, raw) = e.run(&["uninstall"], &[]);
    assert!(raw.status.success());
    assert_eq!(out["result"], "uninstalled");
}

#[test]
fn lock_dipegang_tapi_daemon_tidak_menjawab_hasilnya_deferred() {
    let e = env();
    std::fs::create_dir_all(&e.data).unwrap();
    let lock = e.data.join("daemon.lock");
    std::fs::write(&lock, b"").unwrap();
    let mut holder = Command::new("flock")
        .arg(&lock)
        .args(["sleep", "30"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    std::fs::write(
        e.data.join("daemon.json"),
        serde_json::to_vec(&json!({
            "pid": holder.id(), "port": 1, "token": "x", "protocol": "1.0",
            "version": "0.0.1", "started_at_ms": 0
        }))
        .unwrap(),
    )
    .unwrap();

    let (out, raw) = e.run(&["install"], &[]);
    assert!(raw.status.success());
    assert_eq!(out["result"], "deferred");
    assert!(!e.installed().exists(), "tidak ada berkas yang disentuh");
    assert!(holder.try_wait().unwrap().is_none());
    let _ = holder.kill();
    let _ = holder.wait();
}

#[test]
fn error_setelah_drain_idle_membatalkan_drain() {
    let e = env();
    // Direktori unit berupa symlink: gagal setelah drain dan salin biner.
    let real = e.home.join("unit-asli");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::create_dir_all(e.home.join(".config/systemd")).unwrap();
    std::os::unix::fs::symlink(&real, e.home.join(".config/systemd/user")).unwrap();
    let mut daemon = e.serve(None);

    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "error");
    assert!(daemon.try_wait().unwrap().is_none(), "daemon tetap hidup");
    assert!(!real.join("mandord.service").exists());

    let info: Value =
        serde_json::from_slice(&std::fs::read(e.data.join("daemon.json")).unwrap()).unwrap();
    let status = status(&e, info["token"].as_str().unwrap());
    assert_eq!(status["draining"], false, "drain sudah dibatalkan");
    let _ = daemon.kill();
    let _ = daemon.wait();
}

/// `status` lewat `mandord connect`.
fn status(e: &Env, token: &str) -> Value {
    use std::io::{BufRead, BufReader, Write};
    let mut child = e
        .cmd(&["connect"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut reply = |line: Value| {
        writeln!(stdin, "{line}").unwrap();
        let mut buf = String::new();
        out.read_line(&mut buf).unwrap();
        serde_json::from_str::<Value>(&buf).unwrap()
    };
    reply(json!({"jsonrpc": "2.0", "id": 1, "method": "hello",
                 "params": {"token": token, "protocol": "1.0", "client": "test"}}));
    let s = reply(json!({"jsonrpc": "2.0", "id": 2, "method": "status", "params": null}));
    drop(stdin);
    let _ = child.wait();
    s["result"].clone()
}

fn stop_daemon(e: &Env) {
    let path = e.data.join("daemon.json");
    if let Ok(body) = std::fs::read(&path) {
        let info: Value = serde_json::from_slice(&body).unwrap();
        let pid = info["pid"].as_u64().unwrap().to_string();
        let _ = Command::new("kill").args(["-TERM", &pid]).status();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && path.exists() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// `connect` mengirim `status` lewat relay; daemon dinyalakan bila perlu.
fn connect_status(e: &Env, extra: &[(&str, &str)]) -> Value {
    use std::io::{BufRead, BufReader, Write};
    let mut c = e.cmd(&["connect"]);
    for (k, v) in extra {
        c.env(k, v);
    }
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    // Token dibaca setelah daemon siap: kirim hello salah dulu untuk memicu start.
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 1, "method": "hello",
        "params": {"token": "salah", "protocol": "1.0", "client": "test"}})
    )
    .unwrap();
    let mut buf = String::new();
    out.read_line(&mut buf).unwrap();
    drop(stdin);
    let _ = child.wait();
    let info: Value =
        serde_json::from_slice(&std::fs::read(e.data.join("daemon.json")).unwrap()).unwrap();
    status(e, info["token"].as_str().unwrap())
}

#[test]
fn connect_memakai_systemctl_bila_unit_ada() {
    let e = env();
    e.run(&["install"], &[]);
    e.clear_calls();
    let s = connect_status(&e, &[("FAKE_START_BIN", BIN)]);
    assert_eq!(s["managed_by"], "systemd");
    assert!(e.calls().contains("systemctl --user start mandord.service"));
    stop_daemon(&e);
}

#[test]
fn connect_tanpa_unit_memakai_setsid_tanpa_systemctl() {
    let e = env();
    let s = connect_status(&e, &[]);
    assert_eq!(s["managed_by"], "detached");
    assert!(!e.calls().contains("start mandord.service"));
    stop_daemon(&e);
}

#[test]
fn connect_jatuh_ke_setsid_bila_systemctl_gagal() {
    let e = env();
    e.run(&["install"], &[]);
    e.clear_calls();
    let s = connect_status(&e, &[("FAKE_START_FAIL", "1")]);
    assert_eq!(s["managed_by"], "detached");
    assert!(e.calls().contains("systemctl --user start mandord.service"));
    stop_daemon(&e);
}

#[test]
fn connect_ke_daemon_yang_sudah_berjalan_tidak_menyalakan_apa_pun() {
    let e = env();
    e.run(&["install"], &[]);
    let mut daemon = e.serve(None);
    e.clear_calls();
    let s = connect_status(&e, &[("FAKE_START_BIN", BIN)]);
    assert_eq!(s["managed_by"], "detached");
    assert!(!e.calls().contains("start mandord.service"));
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn connect_dengan_mandor_home_lain_tidak_memakai_service_global() {
    let e = env();
    e.run(&["install"], &[]);
    e.clear_calls();
    let other = e.home.join("data-lain");
    let mut c = e.cmd(&["token"]);
    let out = c.env("MANDOR_HOME", &other).output().unwrap();
    assert!(out.status.success());
    assert!(
        !e.calls().contains("start mandord.service"),
        "{}",
        e.calls()
    );
    let info: Value =
        serde_json::from_slice(&std::fs::read(other.join("daemon.json")).unwrap()).unwrap();
    let pid = info["pid"].as_u64().unwrap().to_string();
    let _ = Command::new("kill").args(["-TERM", &pid]).status();
}

#[test]
fn install_dengan_mandor_home_lain_tidak_menulis_service() {
    let e = env();
    let other = e.home.join("data-lain");
    let (out, raw) = e.run(&["install"], &[("MANDOR_HOME", other.to_str().unwrap())]);
    assert!(raw.status.success());
    assert_eq!(out["service"], "none");
    assert!(!out["warnings"].as_array().unwrap().is_empty());
    assert!(!e.unit().exists());
    assert!(e.installed().exists());
}

/// Database daemon dengan satu run berstatus `state`. Tanpa sesi Lark,
/// rekonsiliasi tidak berjalan sehingga state itu bertahan saat daemon start.
fn seed_run(data: &Path, state: mandor_domain::RunState) {
    use mandor_usecase::run_ports::{RunRecord, RunStore as _};
    std::fs::create_dir_all(data).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let store = mandor_store::Store::open(&data.join("mandor.db"))
            .await
            .unwrap();
        store
            .insert_repo(&mandor_store::NewRepo {
                id: "repo1".to_owned(),
                path: "/tmp/repo1".to_owned(),
                gh_profile: None,
                default_branch: None,
                now_ms: 1,
            })
            .await
            .unwrap();
        let mut run = RunRecord::new("run1".to_owned(), "repo1".to_owned(), "rec1".to_owned());
        run.state = state;
        mandor_store::RunsStore::new(store.clone())
            .create(&run, 1)
            .await
            .unwrap();
        drop(store);
    });
}

#[test]
fn install_ditunda_saat_daemon_melaporkan_run_bekerja() {
    let e = env();
    e.run(&["install"], &[]);
    std::fs::write(e.installed(), "#!/bin/sh\necho 'mandord 0.0.1'\n").unwrap();
    seed_run(&e.data, mandor_domain::RunState::Running);
    let mut daemon = e.serve(None);
    e.clear_calls();

    let (out, raw) = e.run(&["install"], &[]);
    assert!(raw.status.success());
    assert_eq!(out["result"], "deferred");
    assert_eq!(
        std::fs::read_to_string(e.installed()).unwrap(),
        "#!/bin/sh\necho 'mandord 0.0.1'\n",
        "biner tidak disentuh"
    );
    assert!(!e.calls().contains("restart") && !e.calls().contains("enable --now"));
    assert!(daemon.try_wait().unwrap().is_none(), "daemon tetap hidup");

    let info: Value =
        serde_json::from_slice(&std::fs::read(e.data.join("daemon.json")).unwrap()).unwrap();
    let s = status(&e, info["token"].as_str().unwrap());
    assert_eq!(s["busy"], true);
    assert_eq!(
        s["draining"], false,
        "drain yang busy tidak meninggalkan jendela"
    );

    let (out, _) = e.run(&["uninstall"], &[]);
    assert_eq!(out["result"], "deferred", "uninstall juga menunggu");
    assert!(e.installed().exists());

    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn run_publish_juga_menunda_install() {
    let e = env();
    seed_run(&e.data, mandor_domain::RunState::Publishing);
    let mut daemon = e.serve(None);
    let (out, _) = e.run(&["install"], &[]);
    assert_eq!(out["result"], "deferred");
    assert!(!e.installed().exists());
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn daemon_memegang_keep_awake_selama_run_bekerja_dan_melepas_saat_berhenti() {
    let e = env();
    seed_run(&e.data, mandor_domain::RunState::Running);
    let pidfile = e.home.join("penjaga.pid");
    let guard = e.home.join("penjaga.sh");
    std::fs::write(
        &guard,
        format!(
            "#!/bin/sh\necho $$ > '{}'\ncat > /dev/null\n",
            pidfile.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&guard, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cmd =
        serde_json::to_string(&[guard.display().to_string(), "--keep-awake".to_owned()]).unwrap();

    let mut c = e.cmd(&["serve"]);
    let mut daemon = c
        .env("MANDOR_KEEP_AWAKE_CMD", &cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !pidfile.exists() || !e.data.join("daemon.json").exists() {
        assert!(Instant::now() < deadline, "penjaga tidak dijalankan");
        std::thread::sleep(Duration::from_millis(50));
    }
    let guard_pid = std::fs::read_to_string(&pidfile).unwrap().trim().to_owned();
    let info: Value =
        serde_json::from_slice(&std::fs::read(e.data.join("daemon.json")).unwrap()).unwrap();
    let s = status(&e, info["token"].as_str().unwrap());
    assert_eq!(s["keep_awake"], "held");
    assert_eq!(s["busy"], true);

    let _ = Command::new("kill")
        .args(["-TERM", &info["pid"].as_u64().unwrap().to_string()])
        .status();
    let _ = daemon.wait();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !Path::new(&format!("/proc/{guard_pid}")).exists(),
        "penjaga berhenti bersama daemon"
    );
}

#[test]
fn tanpa_perintah_keep_awake_status_not_configured() {
    let e = env();
    let mut daemon = e.serve(None);
    let info: Value =
        serde_json::from_slice(&std::fs::read(e.data.join("daemon.json")).unwrap()).unwrap();
    let s = status(&e, info["token"].as_str().unwrap());
    assert_eq!(s["keep_awake"], "not_configured");
    let _ = daemon.kill();
    let _ = daemon.wait();
}
