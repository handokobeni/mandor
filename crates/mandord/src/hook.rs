//! Subcommand `mandord hook`: hook `PreToolUse` untuk agent (KTD-10).
//!
//! Membaca input JSON dari stdin, memutuskan dengan aturan domain, dan keluar
//! dengan 0 (izinkan) atau 2 (tolak, alasan di stderr). Tidak membaca
//! konfigurasi dan tidak memakai runtime async agar jalur ini sesederhana mungkin.
//! MCP server yang diizinkan untuk run datang dari argumen `--mcp a,b`, yang
//! ditulis daemon di `--settings` agent dan tidak bisa diubah agent. Akar
//! sumber desain datang dari `--design <path>` (boleh berulang).
//!
//! Sesi reviewer memakai `--review <berkas state>` (KTD-31): `PreToolUse`
//! memutuskan dengan state pagar, dan `--post` (`PostToolUse`) mencatat tab
//! serta alamatnya. State hilang, rusak, atau terkunci menolak.

use std::path::{Path, PathBuf};

use mandor_agent::claude::guardrails::run_hook;
use mandor_domain::hook::{
    DesignRoot, ToolCall, Verdict, check_design_path, classify_planning, classify_with_mcp,
};
use mandor_domain::hook_review::{classify_review, mark_before, record_post};

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

/// `--origin <origin>` (berulang): origin app review dari daemon. Menang atas
/// isi berkas state, yang di VM bisa ditulis user agent.
fn review_origins(args: &[String]) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == "--origin")
        .filter_map(|w| mandor_domain::hook_review::origin_of(&w[1]))
        .collect()
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

/// Origin dari argv menggantikan isi berkas state bila ada.
fn pin_origins(state: &mut mandor_domain::hook_review::ReviewState, origins: &[String]) {
    if !origins.is_empty() {
        origins.clone_into(&mut state.origins);
    }
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

pub fn run(args: &[String]) -> u8 {
    let allowed = allowed_mcp(args);
    let plan = planning(args);
    let roots = design_roots(args);
    let review = review_state_path(args);
    let origins = review_origins(args);
    let post = args.iter().any(|a| a == "--post");
    let stdin = std::io::stdin();
    let mut stderr = std::io::stderr();
    run_hook(stdin.lock(), &mut stderr, |call| {
        if post {
            return match &review {
                Some(path) => review_post(path, &origins, call),
                None => Verdict::Deny("mandor: --post needs --review".to_owned()),
            };
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
