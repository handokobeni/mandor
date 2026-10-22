//! Subcommand `mandord hook`: hook `PreToolUse` untuk agent (KTD-10).
//!
//! Membaca input JSON dari stdin, memutuskan dengan aturan domain, dan keluar
//! dengan 0 (izinkan) atau 2 (tolak, alasan di stderr). Tidak membaca
//! konfigurasi dan tidak memakai runtime async agar jalur ini sesederhana mungkin.
//! MCP server yang diizinkan untuk run datang dari argumen `--mcp a,b`, yang
//! ditulis daemon di `--settings` agent dan tidak bisa diubah agent. Akar
//! sumber desain datang dari `--design <path>` (boleh berulang).

use std::path::Path;

use mandor_agent::claude::guardrails::run_hook;
use mandor_domain::hook::{Verdict, check_design_path, classify_planning, classify_with_mcp};

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

/// Akar sumber desain dari argumen `--design`, sudah dikanonisasi.
fn design_roots(args: &[String]) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == "--design")
        .map(|w| canonical(&w[1]))
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
    let stdin = std::io::stdin();
    let mut stderr = std::io::stderr();
    run_hook(stdin.lock(), &mut stderr, |call| {
        if !roots.is_empty() {
            let denied = call
                .paths
                .iter()
                .find_map(|p| check_design_path(&call.tool_name, p, &canonical(p), &roots));
            if let Some(reason) = denied {
                return Verdict::Deny(reason);
            }
        }
        if plan {
            classify_planning(call, &allowed)
        } else {
            classify_with_mcp(call, &allowed)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(roots[1], "/tidak/ada");
    }
}
