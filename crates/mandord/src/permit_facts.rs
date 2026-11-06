//! Fakta permit untuk pemeriksaan ulang di daemon (KTD-A9 plan
//! 2026-11-06-001): berkas izin giliran dan catatan bukti hook, dibaca di
//! `spawn_blocking` dengan batas waktu.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mandor_agent::claude::guardrails::parse_hook_input;
use mandor_agent::claude::permit::{self, FsFacts};
use mandor_domain::permit::{Decision, decide};
use mandor_usecase::run_ports::{PermitFacts, PermitRef};
use serde_json::{Value, json};

/// Batas waktu membaca berkas dan menyelesaikan path.
const FACTS_TIMEOUT: Duration = Duration::from_secs(10);

/// Fakta permit dari filesystem lokal.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiskPermits;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    match tokio::time::timeout(FACTS_TIMEOUT, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("the permission check timed out".to_owned()),
    }
}

/// Folder berkas izin: milik daemon, terlarang bagi agent.
fn private_of(path: &Path) -> Vec<String> {
    path.parent()
        .map(|d| vec![d.display().to_string()])
        .unwrap_or_default()
}

impl PermitFacts for DiskPermits {
    async fn evidence(&self, permit: &PermitRef, tool_use_id: &str) -> Option<String> {
        let path = PathBuf::from(&permit.path);
        let sha = permit.sha.clone();
        let id = tool_use_id.to_owned();
        blocking(move || {
            let file = permit::load(&path, &sha)?;
            let dir = file.evidence.ok_or("this turn has no approval cards")?;
            permit::read_evidence(Path::new(&dir), &id)
                .map(|e| e.cwd)
                .ok_or_else(|| "no check record".to_owned())
        })
        .await
        .ok()
    }

    async fn recheck(
        &self,
        permit: &PermitRef,
        tool: &str,
        input: &Value,
        cwd: &str,
    ) -> Result<Decision, String> {
        let path = PathBuf::from(&permit.path);
        let sha = permit.sha.clone();
        let raw = json!({"tool_name": tool, "tool_input": input, "cwd": cwd}).to_string();
        let cwd = cwd.to_owned();
        blocking(move || {
            let file = permit::load(&path, &sha)?;
            let scope = file
                .scope(None, private_of(&path))
                .ok_or("the permit file has an unknown turn")?;
            let call = parse_hook_input(raw.as_bytes())?;
            Ok(decide(&scope, &call, &cwd, None, &FsFacts))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mandor_agent::claude::permit::{Evidence, PermitFile, VERSION, digest, write_evidence};

    #[tokio::test]
    async fn bukti_dan_pemeriksaan_ulang_dari_berkas_izin() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let root = base.join("run");
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let ev = base.join("permit/run.d");
        std::fs::create_dir_all(&ev).unwrap();
        let file = PermitFile {
            version: VERSION,
            turn: "implement".to_owned(),
            allow: vec!["Bash(git diff*)".to_owned()],
            mcp: vec![],
            design: vec![],
            run_root: root.display().to_string(),
            members: vec![root.display().to_string()],
            evidence: Some(ev.display().to_string()),
        };
        let bytes = file.to_bytes().unwrap();
        let path = base.join("permit/run.json");
        std::fs::write(&path, &bytes).unwrap();
        let permit = PermitRef {
            path: path.display().to_string(),
            sha: digest(&bytes),
        };
        let cwd = root.display().to_string();
        assert_eq!(DiskPermits.evidence(&permit, "toolu_1").await, None);
        write_evidence(
            &ev,
            &Evidence {
                tool_use_id: "toolu_1".to_owned(),
                cwd: cwd.clone(),
            },
        )
        .unwrap();
        assert_eq!(
            DiskPermits.evidence(&permit, "toolu_1").await,
            Some(cwd.clone())
        );

        let push = DiskPermits
            .recheck(&permit, "Bash", &json!({"command": "git push"}), &cwd)
            .await
            .unwrap();
        assert!(matches!(push, Decision::Deny(_)));
        let docker = DiskPermits
            .recheck(&permit, "Bash", &json!({"command": "docker ps"}), &cwd)
            .await
            .unwrap();
        assert!(matches!(docker, Decision::Undecided(_)));

        let wrong = PermitRef {
            sha: "0".repeat(64),
            ..permit
        };
        assert!(
            DiskPermits
                .recheck(&wrong, "Bash", &json!({}), &cwd)
                .await
                .is_err()
        );
        assert_eq!(DiskPermits.evidence(&wrong, "toolu_1").await, None);
    }
}
