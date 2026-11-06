//! Subcommand `mandord hook`: hook `PreToolUse` untuk agent (KTD-10).
//!
//! Membaca input JSON dari stdin, memutuskan dengan aturan domain, dan keluar
//! dengan 0 (izinkan) atau 2 (tolak, alasan di stderr). Tidak membaca
//! konfigurasi dan tidak memakai runtime async agar jalur ini sesederhana mungkin.
//! MCP server yang diizinkan untuk run datang dari argumen `--mcp a,b`, yang
//! ditulis daemon di `--settings` agent dan tidak bisa diubah agent. Akar
//! sumber desain datang dari `--design <path>` (boleh berulang).
//!
//! Daemon VM menambah `--vm <run dir>`: pagar lembur (tanpa pesan, staging,
//! database jarak jauh, atau perubahan konfigurasi git) dan berkas run daemon
//! ditolak untuk semua sesi (U11 plan VM).
//!
//! Sesi reviewer memakai `--review <berkas state>` (KTD-31): `PreToolUse`
//! memutuskan dengan state pagar, dan `--post` (`PostToolUse`) mencatat tab
//! serta alamatnya. State hilang, rusak, atau terkunci menolak.
//!
//! Giliran agent membawa `--permit <berkas> --permit-sha <hex>` (KTD-A4 plan
//! 2026-11-06-001): hook memutuskan tolak, izinkan eksplisit, atau tanpa
//! keputusan dengan berkas izin itu dan fakta path dari `cwd`, dalam tenggat
//! 5 detik (KTD-A14). Untuk tanpa keputusan, catatan bukti `{tool_use_id,
//! cwd}` ditulis bila giliran punya kartu (KTD-A9). Berkas hilang, rusak,
//! atau digest tidak cocok menolak.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mandor_agent::claude::guardrails::{run_hook, run_pre_hook};
use mandor_agent::claude::permit::{self, Evidence, FsFacts};
use mandor_domain::hook::{
    DesignRoot, ToolCall, Verdict, check_design_path, classify_planning, classify_vm,
    classify_with_mcp,
};
use mandor_domain::hook_review::{classify_review, mark_before, record_post};
use mandor_domain::permit::{Decision, TurnKind, decide};

use crate::review_state;

/// MCP server yang diizinkan dari argumen hook; tanpa `--mcp`, tidak ada.
fn allowed_mcp(args: &[String]) -> Vec<String> {
    args.iter()
        .position(|a| a == "--mcp")
        .and_then(|i| args.get(i + 1))
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// `--plan`: giliran plan, tool penulis berkas ditolak.
fn planning(args: &[String]) -> bool {
    args.iter().any(|a| a == "--plan")
}

/// `--origin <origin>` (berulang): origin app review dari daemon, satu-satunya
/// sumber origin (berkas state di VM bisa ditulis user agent).
fn review_origins(args: &[String]) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == "--origin")
        .filter_map(|w| mandor_domain::hook_review::origin_of(&w[1]))
        .collect()
}

/// `--vm <run dir>`: daemon VM, pagar lembur berlaku dan berkas run daemon
/// ditolak (U11 plan VM).
fn vm_run_dir(args: &[String]) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == "--vm")
        .map(|w| w[1].clone())
}

/// Pagar VM dengan path tool yang dikanonisasi (`..`, symlink, path relatif
/// terhadap cwd hook), dibandingkan dengan direktori run apa adanya maupun
/// hasil kanonisasinya.
fn vm_violation(call: &ToolCall, run_dir: &str) -> Option<String> {
    let mut resolved = call.clone();
    resolved.paths = call
        .paths
        .iter()
        .flat_map(|p| [p.clone(), canonical(p)])
        .collect();
    classify_vm(&resolved, run_dir).or_else(|| classify_vm(&resolved, &canonical(run_dir)))
}

/// `--review <berkas>`: berkas state pagar sesi reviewer.
fn review_state_path(args: &[String]) -> Option<PathBuf> {
    args.windows(2)
        .find(|w| w[0] == "--review")
        .map(|w| PathBuf::from(&w[1]))
}

/// Keputusan `PreToolUse` sesi reviewer; tool yang diizinkan dan bisa
/// memindah halaman menandai tab belum terkonfirmasi sebelum berjalan.
fn review_pre(path: &Path, origins: &[String], call: &ToolCall, allowed: &[String]) -> Verdict {
    let outcome = review_state::with_locked(path, |state| {
        pin_origins(state, origins);
        let verdict = classify_review(call, state, allowed);
        if verdict == Verdict::Allow {
            mark_before(call, state);
        }
        verdict
    });
    outcome.unwrap_or_else(|e| Verdict::Deny(format!("mandor: {e}; tool call denied")))
}

/// Origin dari argv adalah satu-satunya sumber; tanpa `--origin`, semua
/// halaman ditolak (gagal-tertutup).
fn pin_origins(state: &mut mandor_domain::hook_review::ReviewState, origins: &[String]) {
    origins.clone_into(&mut state.origins);
}

/// `PostToolUse` sesi reviewer: mencatat hasil tool browser.
fn review_post(path: &Path, origins: &[String], call: &ToolCall) -> Verdict {
    match review_state::with_locked(path, |state| {
        pin_origins(state, origins);
        record_post(call, state);
    }) {
        Ok(()) => Verdict::Allow,
        Err(e) => Verdict::Deny(format!("mandor: {e}")),
    }
}

/// Akar sumber desain dari argumen `--design`: jalur yang diberikan dan
/// hasil kanonisasinya.
fn design_roots(args: &[String]) -> Vec<DesignRoot> {
    args.windows(2)
        .filter(|w| w[0] == "--design")
        .map(|w| DesignRoot {
            given: w[1].trim_end_matches('/').to_owned(),
            canonical: canonical(&w[1]),
        })
        .collect()
}

/// `realpath` dari path; untuk berkas yang belum ada, induknya dikanonisasi.
/// Hook berjalan sebagai proses sinkron tanpa runtime async.
fn canonical(raw: &str) -> String {
    let path = Path::new(raw);
    let resolved =
        std::fs::canonicalize(path).or_else(|e| match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => std::fs::canonicalize(parent).map(|p| p.join(name)),
            _ => Err(e),
        });
    resolved.map_or_else(|_| raw.to_owned(), |p| p.to_string_lossy().into_owned())
}

/// Tenggat keputusan hook (KTD-A14); `--deadline-ms` hanya untuk test.
const DEADLINE: Duration = Duration::from_secs(5);

/// `--permit <berkas> --permit-sha <hex>`.
fn permit_args(args: &[String]) -> Option<(PathBuf, String)> {
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    Some((PathBuf::from(value("--permit")?), value("--permit-sha")?))
}

fn deadline(args: &[String]) -> Duration {
    args.windows(2)
        .find(|w| w[0] == "--deadline-ms")
        .and_then(|w| w[1].parse().ok())
        .map_or(DEADLINE, Duration::from_millis)
}

/// Argumen hook yang dipakai keputusan dengan berkas izin.
struct PermitHook {
    path: PathBuf,
    sha: String,
    vm: Option<String>,
    review: Option<PathBuf>,
    origins: Vec<String>,
}

impl PermitHook {
    fn decide(&self, call: &mandor_domain::hook::ToolCall) -> Decision {
        let deny = |e: &str| Decision::Deny(format!("mandor: {e}; tool call denied"));
        let file = match permit::load(&self.path, &self.sha) {
            Ok(f) => f,
            Err(e) => return deny(&e),
        };
        let private = self
            .path
            .parent()
            .map(|d| vec![d.display().to_string()])
            .unwrap_or_default();
        let Some(scope) = file.scope(self.vm.clone(), private) else {
            return deny("the permit file has an unknown turn");
        };
        let Some(cwd) = call.cwd.as_deref() else {
            return deny("hook input has no cwd");
        };
        let decision = match (scope.turn, &self.review) {
            (TurnKind::Review, Some(state_path)) => {
                let outcome = review_state::with_locked(state_path, |state| {
                    pin_origins(state, &self.origins);
                    let d = decide(&scope, call, cwd, Some(state), &FsFacts);
                    if !matches!(d, Decision::Deny(_)) {
                        mark_before(call, state);
                    }
                    d
                });
                outcome.unwrap_or_else(|e| deny(&e))
            }
            (TurnKind::Review, None) | (_, Some(_)) => {
                return deny("the permit file does not match this session");
            }
            _ => decide(&scope, call, cwd, None, &FsFacts),
        };
        match (&decision, &file.evidence) {
            (Decision::Undecided(_), Some(dir)) => {
                let evidence = Evidence {
                    tool_use_id: call.tool_use_id.clone().unwrap_or_default(),
                    cwd: cwd.to_owned(),
                };
                match permit::write_evidence(Path::new(dir), &evidence) {
                    Ok(()) => decision,
                    Err(e) => deny(&format!("cannot record the permission check ({e})")),
                }
            }
            _ => decision,
        }
    }
}

pub fn run(args: &[String]) -> u8 {
    let allowed = allowed_mcp(args);
    let plan = planning(args);
    let roots = design_roots(args);
    let review = review_state_path(args);
    let origins = review_origins(args);
    let vm = vm_run_dir(args);
    let post = args.iter().any(|a| a == "--post");
    let mut stderr = std::io::stderr();
    if let (false, Some((path, sha))) = (post, permit_args(args)) {
        let hook = PermitHook {
            path,
            sha,
            vm,
            review,
            origins,
        };
        let mut stdout = std::io::stdout();
        return run_pre_hook(
            std::io::stdin(),
            &mut stdout,
            &mut stderr,
            deadline(args),
            move |call| hook.decide(call),
        );
    }
    let stdin = std::io::stdin();
    run_hook(stdin.lock(), &mut stderr, |call| {
        if post {
            return match &review {
                Some(path) => review_post(path, &origins, call),
                None => Verdict::Deny("mandor: --post needs --review".to_owned()),
            };
        }
        if let Some(reason) = vm.as_deref().and_then(|dir| vm_violation(call, dir)) {
            return Verdict::Deny(reason);
        }
        if !roots.is_empty() {
            let denied = call
                .paths
                .iter()
                .find_map(|p| check_design_path(&call.tool_name, p, &canonical(p), &roots));
            if let Some(reason) = denied {
                return Verdict::Deny(reason);
            }
        }
        match &review {
            Some(path) => review_pre(path, &origins, call, &allowed),
            None if plan => classify_planning(call, &allowed),
            None => classify_with_mcp(call, &allowed),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_review_dari_argumen_menang_atas_berkas_state() {
        let args: Vec<String> = [
            "--review",
            "/s.json",
            "--origin",
            "http://localhost:5173/x",
            "--origin",
            "bukan-url",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let origins = review_origins(&args);
        assert_eq!(origins, ["http://localhost:5173"]);
        let mut state = mandor_domain::hook_review::ReviewState {
            origins: vec!["https://evil.example:443".to_owned()],
            ..Default::default()
        };
        pin_origins(&mut state, &origins);
        assert_eq!(state.origins, ["http://localhost:5173"]);
        pin_origins(&mut state, &[]);
        assert!(state.origins.is_empty(), "tanpa --origin tidak ada halaman");
    }

    #[test]
    fn argumen_mcp_menjadi_daftar_server() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            allowed_mcp(&args(&["--mcp", "graft, docs"])),
            ["graft", "docs"]
        );
        assert!(allowed_mcp(&args(&[])).is_empty());
        assert!(allowed_mcp(&args(&["--mcp"])).is_empty());
        assert!(planning(&args(&["--mcp", "graft", "--plan"])));
        assert!(!planning(&args(&["--mcp", "graft"])));
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap().to_owned();
        let roots = design_roots(&args(&[
            "--design",
            &root,
            "--plan",
            "--design",
            "/tidak/ada",
        ]));
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].given, root);
        assert_eq!(
            roots[0].canonical,
            std::fs::canonicalize(&root).unwrap().to_string_lossy()
        );
        assert_eq!(roots[1].given, "/tidak/ada");
        assert_eq!(roots[1].canonical, "/tidak/ada");
    }
}
