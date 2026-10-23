//! `mandord install` dan `mandord uninstall` (KTD-43, KTD-52, KTD-53 plan
//! service OS): memasang biner ini ke `~/.local/bin` dan unit systemd user,
//! aman terhadap pekerjaan yang sedang berjalan. Keputusan ada di
//! [`mandor_domain::service`]; modul ini hanya I/O. Keluaran satu baris JSON
//! untuk app, tanpa token.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use mandor_api::DaemonClient;
use mandor_domain::service::{
    BinaryState, InstallPlan, InstallResult, Notice, Running, ServiceState, Start, Stop,
    compare_versions, parse_version_line, plan_install, unit_text,
};
use mandor_proc::Cmd;
use mandor_protocol::{ErrorCode, method};
use secrecy::SecretString;
use serde::Serialize;
use serde_json::Value;

use crate::VERSION;
use crate::config::Config;
use crate::discovery;

const UNIT_NAME: &str = "mandord.service";
/// Batas waktu perintah singkat (`systemctl`, `loginctl`, `--version`, `kill`).
const PROC_TIMEOUT: Duration = Duration::from_secs(10);
/// Batas waktu `systemctl restart` dan `enable --now`.
const SERVICE_TIMEOUT: Duration = Duration::from_secs(30);
/// Batas waktu baca/tulis biner.
const FILE_TIMEOUT: Duration = Duration::from_secs(30);
/// Batas waktu menunggu daemon lama melepas lock setelah SIGTERM (masa
/// tenggang shutdown daemon 30 detik).
const LOCK_RELEASE_TIMEOUT: Duration = Duration::from_secs(40);
/// Run yang bekerja menurut daemon lama tanpa `daemon.drain`.
const WORKING_STATES: [&str; 5] = [
    "running",
    "wf_running",
    "reviewing",
    "preparing",
    "publishing",
];

/// Keluaran `install` dan `uninstall`.
#[derive(Debug, Serialize)]
struct Report {
    result: &'static str,
    service: &'static str,
    linger: bool,
    apps_stopped: bool,
    warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Keluaran `install --check`.
#[derive(Debug, Serialize)]
struct CheckReport {
    binary: &'static str,
    installed_version: Option<String>,
    service: &'static str,
    systemd: bool,
    linger: bool,
    running: &'static str,
}

/// Opsi baris perintah `install`.
#[derive(Debug, Default)]
struct InstallArgs {
    check: bool,
    keep_awake_cmd: Option<String>,
}

fn parse_install_args(args: &[String]) -> anyhow::Result<InstallArgs> {
    let mut out = InstallArgs::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--check" => out.check = true,
            "--keep-awake-cmd" => {
                let v = it.next().context("--keep-awake-cmd needs a value")?;
                out.keep_awake_cmd = Some(v.clone()).filter(|v| !v.is_empty());
            }
            other => bail!("unknown install option: {other}"),
        }
    }
    Ok(out)
}

/// Lokasi berkas yang dikelola `install`.
struct Paths {
    exe: PathBuf,
    target: PathBuf,
    unit: PathBuf,
}

impl Paths {
    fn new(cfg: &Config) -> Self {
        Self {
            exe: cfg.exe.clone(),
            target: cfg.bin_dir.join("mandord"),
            unit: cfg.systemd_user_dir.join(UNIT_NAME),
        }
    }
}

/// Daemon yang sedang berjalan untuk direktori data ini.
struct Live {
    running: Running,
    client: Option<DaemonClient>,
    pid: Option<u32>,
}

/// Subcommand `install`. Mengembalikan `false` bila hasilnya `error`.
///
/// # Errors
/// Opsi tidak dikenal atau keluaran tidak bisa ditulis.
pub async fn run_install(cfg: &Config, args: &[String]) -> anyhow::Result<bool> {
    let opts = parse_install_args(args)?;
    let paths = Paths::new(cfg);
    if opts.check {
        let report = check(cfg, &paths, opts.keep_awake_cmd.as_deref()).await?;
        print_json(&report).await?;
        return Ok(true);
    }
    let mut warnings = Vec::new();
    let report = match install(cfg, &paths, opts.keep_awake_cmd.as_deref(), &mut warnings).await {
        Ok(r) => r,
        Err(e) => Report {
            result: InstallResult::Error.as_str(),
            service: "none",
            linger: false,
            apps_stopped: false,
            warnings,
            message: Some(format!("{e:#}")),
        },
    };
    let ok = report.result != InstallResult::Error.as_str();
    print_json(&report).await?;
    Ok(ok)
}

/// Subcommand `uninstall [--force]`. Mengembalikan `false` bila hasilnya `error`.
///
/// # Errors
/// Opsi tidak dikenal atau keluaran tidak bisa ditulis.
pub async fn run_uninstall(cfg: &Config, args: &[String]) -> anyhow::Result<bool> {
    let force = match args {
        [] => false,
        [f] if f == "--force" => true,
        _ => bail!("usage: mandord uninstall [--force]"),
    };
    let paths = Paths::new(cfg);
    let report = match uninstall(cfg, &paths, force).await {
        Ok(r) => r,
        Err(e) => Report {
            result: InstallResult::Error.as_str(),
            service: "none",
            linger: false,
            apps_stopped: false,
            warnings: Vec::new(),
            message: Some(format!("{e:#}")),
        },
    };
    let ok = report.result != InstallResult::Error.as_str();
    print_json(&report).await?;
    Ok(ok)
}

async fn check(
    cfg: &Config,
    paths: &Paths,
    keep_awake: Option<&str>,
) -> anyhow::Result<CheckReport> {
    let (binary, installed_version) = observe_binary(paths).await?;
    let systemd = systemd_available(cfg).await;
    let expected = unit_text(&paths.target.display().to_string(), keep_awake)?;
    let service = observe_service(systemd, &paths.unit, &expected).await;
    let linger = systemd && linger_enabled(cfg).await;
    let live = observe_running(cfg).await;
    Ok(CheckReport {
        binary: binary_name(binary),
        installed_version,
        service: service_name(service),
        systemd,
        linger,
        running: running_name(live.running),
    })
}

async fn install(
    cfg: &Config,
    paths: &Paths,
    keep_awake: Option<&str>,
    warnings: &mut Vec<String>,
) -> anyhow::Result<Report> {
    ensure_safe_dir(&paths.target, &cfg.user_home).await?;
    let (binary, _) = observe_binary(paths).await?;
    let systemd = systemd_available(cfg).await;
    let expected = unit_text(&paths.target.display().to_string(), keep_awake)?;
    let service = observe_service(systemd, &paths.unit, &expected).await;
    let live = observe_running(cfg).await;
    let plan = plan_install(binary, service, live.running);
    if plan.notice == Some(Notice::NewerInstalled) {
        warnings
            .push("the installed daemon is newer than this app; it was not downgraded".to_owned());
    }
    let service_name = if systemd { "systemd" } else { "none" };
    if plan.result == InstallResult::UpToDate {
        return Ok(report(
            plan.result,
            service_name,
            systemd && linger_enabled(cfg).await,
            false,
            warnings,
        ));
    }
    if plan.needs_drain {
        match drain(&live).await {
            Drain::Idle => {}
            Drain::Busy => {
                return Ok(report(
                    InstallResult::Deferred,
                    service_name,
                    false,
                    false,
                    warnings,
                ));
            }
        }
    }
    let applied = apply(cfg, paths, &plan, &expected, &live).await;
    if let Err(e) = applied {
        if plan.needs_drain {
            undrain(&live).await;
        }
        return Err(e);
    }
    let linger = if systemd {
        ensure_linger(cfg, warnings).await
    } else {
        false
    };
    Ok(report(
        plan.result,
        service_name,
        linger,
        plan.stop != Stop::None,
        warnings,
    ))
}

fn report(
    result: InstallResult,
    service: &'static str,
    linger: bool,
    apps_stopped: bool,
    warnings: &mut Vec<String>,
) -> Report {
    Report {
        result: result.as_str(),
        service,
        linger,
        apps_stopped,
        warnings: std::mem::take(warnings),
        message: None,
    }
}

async fn apply(
    cfg: &Config,
    paths: &Paths,
    plan: &InstallPlan,
    expected_unit: &str,
    live: &Live,
) -> anyhow::Result<()> {
    if plan.copy_binary {
        let body = read_file(&paths.exe).await?;
        write_atomic(&paths.target, &body, 0o755, &cfg.user_home).await?;
    }
    if plan.write_unit {
        write_atomic(&paths.unit, expected_unit.as_bytes(), 0o644, &cfg.user_home).await?;
        systemctl(cfg, &["daemon-reload"], PROC_TIMEOUT).await?;
    }
    match plan.stop {
        Stop::None => {}
        Stop::RestartService => systemctl(cfg, &["restart", UNIT_NAME], SERVICE_TIMEOUT).await?,
        Stop::TerminateDetached => terminate_detached(cfg, live).await?,
    }
    if plan.start == Start::EnableNow {
        systemctl(cfg, &["enable", "--now", UNIT_NAME], SERVICE_TIMEOUT).await?;
    }
    Ok(())
}

async fn uninstall(cfg: &Config, paths: &Paths, force: bool) -> anyhow::Result<Report> {
    let systemd = systemd_available(cfg).await;
    let live = observe_running(cfg).await;
    let mut warnings = Vec::new();
    if live.running != Running::No && !force {
        if let Drain::Busy = drain(&live).await {
            return Ok(report(
                InstallResult::Deferred,
                if systemd { "systemd" } else { "none" },
                false,
                false,
                &mut warnings,
            ));
        }
    }
    let unit_exists = tokio::fs::symlink_metadata(&paths.unit).await.is_ok();
    if systemd && unit_exists {
        systemctl(cfg, &["disable", "--now", UNIT_NAME], SERVICE_TIMEOUT).await?;
    }
    if live.running == Running::Detached {
        terminate_detached(cfg, &live).await?;
    }
    remove_if_regular(&paths.unit).await?;
    if systemd && unit_exists {
        systemctl(cfg, &["daemon-reload"], PROC_TIMEOUT).await?;
    }
    remove_if_regular(&paths.target).await?;
    Ok(report(
        InstallResult::Uninstalled,
        if systemd { "systemd" } else { "none" },
        false,
        live.running != Running::No,
        &mut warnings,
    ))
}

// ---- pengamatan ----

async fn observe_binary(paths: &Paths) -> anyhow::Result<(BinaryState, Option<String>)> {
    let meta = match tokio::fs::symlink_metadata(&paths.target).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((BinaryState::Missing, None));
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", paths.target.display())),
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        bail!(
            "{} is not a regular file; refusing to replace it",
            paths.target.display()
        );
    }
    let version = installed_version(&paths.target).await;
    let mine = read_file(&paths.exe).await?;
    let theirs = read_file(&paths.target).await?;
    if mine == theirs {
        return Ok((BinaryState::Same, Some(VERSION.to_owned())));
    }
    let state = match version
        .as_deref()
        .and_then(|v| compare_versions(v, VERSION))
    {
        Some(std::cmp::Ordering::Less) => BinaryState::Older,
        Some(std::cmp::Ordering::Greater) => BinaryState::Newer,
        _ => BinaryState::Differs,
    };
    Ok((state, version))
}

async fn installed_version(target: &Path) -> Option<String> {
    let out = mandor_proc::run(&Cmd::new(target).arg("--version").timeout(PROC_TIMEOUT))
        .await
        .ok()?;
    if !out.success() {
        return None;
    }
    parse_version_line(&out.stdout).map(str::to_owned)
}

async fn systemd_available(cfg: &Config) -> bool {
    matches!(
        mandor_proc::run(
            &Cmd::new(&cfg.systemctl)
                .args(["--user", "show-environment"])
                .timeout(PROC_TIMEOUT)
        )
        .await,
        Ok(out) if out.success()
    )
}

async fn observe_service(systemd: bool, unit: &Path, expected: &str) -> ServiceState {
    if !systemd {
        return ServiceState::Unavailable;
    }
    match tokio::fs::read_to_string(unit).await {
        Ok(text) if text == expected => ServiceState::Matches,
        Ok(_) => ServiceState::Differs,
        Err(_) => ServiceState::Missing,
    }
}

async fn observe_running(cfg: &Config) -> Live {
    let none = Live {
        running: Running::No,
        client: None,
        pid: None,
    };
    if !lock_held(cfg).await {
        return none;
    }
    let Some(info) = discovery::read(&cfg.home).await else {
        // Lock dipegang tanpa daemon.json: anggap berjalan, tanpa client.
        return Live {
            running: Running::Detached,
            client: None,
            pid: None,
        };
    };
    let client = DaemonClient::new(
        SocketAddr::from((Ipv4Addr::LOCALHOST, info.port)),
        SecretString::from(info.token),
        "mandord-install",
    );
    let managed_by = match client.request(method::STATUS, Value::Null).await {
        Ok(status) => status
            .get("managed_by")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Err(_) => {
            return Live {
                running: Running::Detached,
                client: None,
                pid: Some(info.pid),
            };
        }
    };
    let running = if managed_by.as_deref() == Some("systemd") {
        Running::Systemd
    } else {
        Running::Detached
    };
    Live {
        running,
        client: Some(client),
        pid: Some(info.pid),
    }
}

async fn linger_enabled(cfg: &Config) -> bool {
    let Some(user) = cfg.user.as_deref() else {
        return false;
    };
    match mandor_proc::run(
        &Cmd::new(&cfg.loginctl)
            .args(["show-user", user, "--property=Linger", "--value"])
            .timeout(PROC_TIMEOUT),
    )
    .await
    {
        Ok(out) if out.success() => out.stdout.trim() == "yes",
        _ => false,
    }
}

async fn ensure_linger(cfg: &Config, warnings: &mut Vec<String>) -> bool {
    if linger_enabled(cfg).await {
        return true;
    }
    let enabled = matches!(
        mandor_proc::run(&Cmd::new(&cfg.loginctl).arg("enable-linger").timeout(PROC_TIMEOUT)).await,
        Ok(out) if out.success()
    );
    if !enabled {
        warnings.push(
            "linger could not be enabled; the daemon stops when your last session ends".to_owned(),
        );
    }
    enabled
}

// ---- drain ----

enum Drain {
    Idle,
    Busy,
}

/// Drain daemon yang berjalan. Daemon yang tidak bisa ditanya dianggap sibuk:
/// lebih baik menunda pembaruan daripada memutus pekerjaan.
async fn drain(live: &Live) -> Drain {
    let Some(client) = &live.client else {
        return Drain::Busy;
    };
    match client.request(method::DAEMON_DRAIN, Value::Null).await {
        Ok(v) if v.get("state").and_then(Value::as_str) == Some("idle") => Drain::Idle,
        Err(e) if e.code == ErrorCode::MethodNotFound => legacy_busy(client).await,
        Ok(_) | Err(_) => Drain::Busy,
    }
}

/// Daemon versi lama tanpa `daemon.drain`: sibuk dihitung dari `runs.list`.
/// Celah antara cek dan restart diterima sekali ini (Risks plan service OS).
async fn legacy_busy(client: &DaemonClient) -> Drain {
    match client.request(method::RUNS_LIST, Value::Null).await {
        Ok(Value::Array(runs)) => {
            let working = runs.iter().any(|r| {
                r.get("state")
                    .and_then(Value::as_str)
                    .is_some_and(|s| WORKING_STATES.contains(&s))
            });
            if working { Drain::Busy } else { Drain::Idle }
        }
        // Tanpa sesi Lark tidak ada run yang bisa berjalan.
        Err(e) if e.code == ErrorCode::Unauthorized => Drain::Idle,
        _ => Drain::Busy,
    }
}

async fn undrain(live: &Live) {
    if let Some(client) = &live.client {
        if let Err(e) = client.request(method::DAEMON_UNDRAIN, Value::Null).await {
            tracing::warn!(error = %e.message, "gagal membatalkan drain");
        }
    }
}

// ---- menghentikan daemon detached ----

async fn terminate_detached(cfg: &Config, live: &Live) -> anyhow::Result<()> {
    let pid = live
        .pid
        .context("the running daemon did not report its process id")?;
    if !is_mandord(pid).await {
        bail!("process {pid} in daemon.json is not mandord; refusing to signal it");
    }
    if !lock_held(cfg).await {
        return Ok(());
    }
    let out = mandor_proc::run(
        &Cmd::new("kill")
            .args(["-TERM", &pid.to_string()])
            .timeout(PROC_TIMEOUT),
    )
    .await
    .context("sending SIGTERM to the old daemon")?;
    if !out.success() {
        bail!("could not stop the old daemon (pid {pid})");
    }
    let deadline = Instant::now() + LOCK_RELEASE_TIMEOUT;
    while lock_held(cfg).await {
        if Instant::now() >= deadline {
            bail!("the old daemon did not stop in time");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

/// `/proc/<pid>/exe` menunjuk biner bernama `mandord` (termasuk biner lama
/// yang sudah diganti, ditandai `(deleted)`).
async fn is_mandord(pid: u32) -> bool {
    match tokio::fs::read_link(format!("/proc/{pid}/exe")).await {
        Ok(path) => path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "mandord" || n.starts_with("mandord ")),
        Err(_) => false,
    }
}

/// Lock single-instance sedang dipegang daemon lain.
async fn lock_held(cfg: &Config) -> bool {
    let path = cfg.home.join("daemon.lock");
    tokio::task::spawn_blocking(move || {
        let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        else {
            return false;
        };
        let mut lock = fd_lock::RwLock::new(file);
        lock.try_write().is_err()
    })
    .await
    .unwrap_or(false)
}

// ---- berkas ----

async fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    tokio::time::timeout(FILE_TIMEOUT, tokio::fs::read(path))
        .await
        .with_context(|| format!("reading {} timed out", path.display()))?
        .with_context(|| format!("reading {}", path.display()))
}

/// Direktori induk tujuan dibuat bila perlu, harus direktori biasa (bukan
/// symlink) milik pemilik home, dan tujuan tidak boleh symlink.
async fn ensure_safe_dir(target: &Path, home: &Path) -> anyhow::Result<()> {
    let dir = target.parent().context("target has no parent directory")?;
    let mut created = Vec::new();
    let mut cur = dir.to_path_buf();
    while tokio::fs::symlink_metadata(&cur).await.is_err() {
        created.push(cur.clone());
        if !cur.pop() {
            break;
        }
    }
    for d in created.iter().rev() {
        tokio::fs::create_dir(d)
            .await
            .with_context(|| format!("creating {}", d.display()))?;
        set_mode(d, 0o755).await?;
    }
    check_owned_dir(dir, home).await
}

async fn check_owned_dir(dir: &Path, home: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let meta = tokio::fs::symlink_metadata(dir)
        .await
        .with_context(|| format!("reading {}", dir.display()))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        bail!(
            "{} is not a regular directory; refusing to write there",
            dir.display()
        );
    }
    let owner = tokio::fs::metadata(home)
        .await
        .with_context(|| format!("reading {}", home.display()))?
        .uid();
    if meta.uid() != owner {
        bail!(
            "{} is not owned by you; refusing to write there",
            dir.display()
        );
    }
    Ok(())
}

/// Menulis atomik: berkas sementara baru (`create_new`) di direktori tujuan,
/// mode diatur, lalu rename. Tujuan yang berupa symlink ditolak.
async fn write_atomic(target: &Path, body: &[u8], mode: u32, home: &Path) -> anyhow::Result<()> {
    ensure_safe_dir(target, home).await?;
    if let Ok(meta) = tokio::fs::symlink_metadata(target).await {
        if meta.file_type().is_symlink() || !meta.is_file() {
            bail!(
                "{} is not a regular file; refusing to replace it",
                target.display()
            );
        }
    }
    let dir = target.parent().context("target has no parent directory")?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .context("target has no file name")?;
    let tmp = dir.join(format!(".{name}.tmp.{}", std::process::id()));
    let written = tokio::time::timeout(FILE_TIMEOUT, async {
        let mut opts = tokio::fs::OpenOptions::new();
        opts.write(true).create_new(true).mode(mode);
        let mut file = opts
            .open(&tmp)
            .await
            .with_context(|| format!("creating {}", tmp.display()))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, body)
            .await
            .with_context(|| format!("writing {}", tmp.display()))?;
        file.sync_all()
            .await
            .with_context(|| format!("syncing {}", tmp.display()))?;
        drop(file);
        set_mode(&tmp, mode).await?;
        tokio::fs::rename(&tmp, target)
            .await
            .with_context(|| format!("replacing {}", target.display()))
    })
    .await
    .with_context(|| format!("writing {} timed out", target.display()))
    .and_then(|r| r);
    if written.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    written
}

async fn set_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .await
        .with_context(|| format!("setting permissions on {}", path.display()))
}

async fn remove_if_regular(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(meta) if meta.is_file() => tokio::fs::remove_file(path)
            .await
            .with_context(|| format!("removing {}", path.display())),
        Ok(_) => bail!(
            "{} is not a regular file; refusing to remove it",
            path.display()
        ),
        Err(_) => Ok(()),
    }
}

async fn systemctl(cfg: &Config, args: &[&str], timeout: Duration) -> anyhow::Result<()> {
    let out = mandor_proc::run(
        &Cmd::new(&cfg.systemctl)
            .arg("--user")
            .args(args)
            .timeout(timeout),
    )
    .await
    .with_context(|| format!("systemctl --user {}", args.join(" ")))?;
    if !out.success() {
        bail!(
            "systemctl --user {} failed: {}",
            args.join(" "),
            out.stderr.trim()
        );
    }
    Ok(())
}

async fn print_json(value: &impl Serialize) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt as _;
    let line = serde_json::to_string(value).context("serializing the result")?;
    let mut out = tokio::io::stdout();
    out.write_all(format!("{line}\n").as_bytes())
        .await
        .context("writing the result")?;
    out.flush().await.context("flush result")
}

fn binary_name(b: BinaryState) -> &'static str {
    match b {
        BinaryState::Missing => "missing",
        BinaryState::Same => "same",
        BinaryState::Older => "older",
        BinaryState::Newer => "newer",
        BinaryState::Differs => "differs",
    }
}

fn service_name(s: ServiceState) -> &'static str {
    match s {
        ServiceState::Unavailable => "unavailable",
        ServiceState::Missing => "missing",
        ServiceState::Matches => "matches",
        ServiceState::Differs => "differs",
    }
}

fn running_name(r: Running) -> &'static str {
    match r {
        Running::No => "no",
        Running::Systemd => "systemd",
        Running::Detached => "detached",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opsi_install() {
        let a = parse_install_args(&["--check".to_owned()]).unwrap();
        assert!(a.check && a.keep_awake_cmd.is_none());
        let a = parse_install_args(&["--keep-awake-cmd".to_owned(), "[\"x\"]".to_owned()]).unwrap();
        assert_eq!(a.keep_awake_cmd.as_deref(), Some("[\"x\"]"));
        assert!(parse_install_args(&["--keep-awake-cmd".to_owned()]).is_err());
        assert!(parse_install_args(&["--lain".to_owned()]).is_err());
    }
}
