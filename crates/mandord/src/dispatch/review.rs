//! Method Rilis 2: buku AC (`ledger.*`).

use mandor_domain::{Design, Verdict};
use mandor_protocol::{
    AcLedgerView, ChromeCheckParams, ChromeCheckView, ErrorCode, LedgerToolInfo,
    LedgerToolSetDesign, LedgerToolSetVerdict, ReadinessConfirmations, ReviewFindings,
    ReviewReadinessView, ReviewStart, ReviewVerdictView, ReviewView, ReviewerSettingsView,
    RpcError, RunRef, method,
};
use mandor_usecase::review::{CapabilityRole, CapabilityScope, LedgerToolError};
use mandor_usecase::run_ports::{ReviewRecord, ReviewTrigger, ReviewerSettings};
use serde_json::Value;

use super::{AppDispatch, ok, params, run_error};
use crate::views::ac_row_view;

/// Satu panggilan tool ledger yang sudah diurai dari koneksi kapabilitas.
#[derive(Debug, PartialEq)]
pub(super) enum LedgerTool {
    Info,
    Read,
    SetDesign {
        ac_id: String,
        design: Design,
        note: Option<String>,
    },
    SetVerdict {
        ac_id: String,
        verdict: Verdict,
        evidence: Option<String>,
    },
}

/// Mengurai method dan parameter tool ledger; method lain ditolak.
pub(super) fn ledger_tool(name: &str, v: Value) -> Result<LedgerTool, RpcError> {
    match name {
        method::LEDGER_TOOL_INFO => Ok(LedgerTool::Info),
        method::LEDGER_TOOL_READ => Ok(LedgerTool::Read),
        method::LEDGER_TOOL_SET_DESIGN => {
            let p: LedgerToolSetDesign = params(v)?;
            let design = Design::parse(&p.design).ok_or_else(|| invalid("design", &p.design))?;
            Ok(LedgerTool::SetDesign {
                ac_id: p.ac_id,
                design,
                note: p.note,
            })
        }
        method::LEDGER_TOOL_SET_VERDICT => {
            let p: LedgerToolSetVerdict = params(v)?;
            let verdict =
                Verdict::parse(&p.verdict).ok_or_else(|| invalid("verdict", &p.verdict))?;
            Ok(LedgerTool::SetVerdict {
                ac_id: p.ac_id,
                verdict,
                evidence: p.evidence,
            })
        }
        other => Err(RpcError::new(
            ErrorCode::Unauthorized,
            format!("{other} is not available on this connection"),
        )),
    }
}

fn invalid(field: &str, value: &str) -> RpcError {
    RpcError::new(
        ErrorCode::InvalidParams,
        format!("unknown {field} value: {value}"),
    )
}

fn tool_error(e: &LedgerToolError) -> RpcError {
    let code = match e {
        LedgerToolError::WrongRole => ErrorCode::Unauthorized,
        LedgerToolError::UnknownAc(_) => ErrorCode::NotFound,
        LedgerToolError::NotAllowed(_) => ErrorCode::Rejected,
        LedgerToolError::Storage(_) => ErrorCode::Internal,
    };
    RpcError::new(code, e.to_string())
}

fn api_scope(s: &CapabilityScope) -> mandor_api::CapabilityScope {
    mandor_api::CapabilityScope {
        run_id: s.run_id.clone(),
        role: s.role.as_str().to_owned(),
    }
}

fn usecase_scope(s: &mandor_api::CapabilityScope) -> Option<CapabilityScope> {
    let role = match s.role.as_str() {
        "design" => CapabilityRole::Design,
        "verdict" => CapabilityRole::Verdict,
        _ => return None,
    };
    Some(CapabilityScope {
        run_id: s.run_id.clone(),
        role,
    })
}

/// Kunci hasil cek ekstensi terakhir.
const CHROME_CHECKED_KEY: &str = "chrome_check";
/// Umur cache cek ekstensi.
const CHROME_CACHE_MS: i64 = 60_000;

fn readiness_view(r: mandor_usecase::review::ReviewReadiness) -> ReviewReadinessView {
    ReviewReadinessView {
        needs_dialog: r.needs_dialog(),
        cant_verify_reason: r.facts().reason,
        browser_tests: r.browser_tests,
        app: r.app.as_str().to_owned(),
        other_owner: r.other_owner,
        confirmed: ReadinessConfirmations {
            database: r.confirmed.database,
            login: r.confirmed.login,
        },
        app_urls: r.app_urls,
        chrome_connected: matches!(
            r.driver,
            mandor_usecase::review::BrowserDriver::Chrome { connected: true }
        ),
    }
}

fn review_view(r: &ReviewRecord) -> ReviewView {
    ReviewView {
        id: r.id.clone(),
        run_id: r.run_id.clone(),
        trigger: r.trigger.as_str().to_owned(),
        status: r.status.as_str().to_owned(),
        browser_used: r.browser_used,
        browser_reason: r.browser_reason.clone(),
        error: r.error.clone(),
        cost_usd: r.cost_usd,
        input_tokens: r.usage.total_input(),
        output_tokens: r.usage.output_tokens,
        stale: r.stale,
        skill: r.skill.clone(),
        started_at_ms: r.started_at_ms,
        finished_at_ms: r.finished_at_ms,
        verdicts: r
            .verdicts
            .iter()
            .map(|v| ReviewVerdictView {
                ac_id: v.ac_id.clone(),
                label: v.label.clone(),
                verdict: v.verdict.as_str().to_owned(),
                evidence: v.evidence.clone(),
            })
            .collect(),
    }
}

fn settings_view(s: &ReviewerSettings) -> ReviewerSettingsView {
    ReviewerSettingsView {
        model: s.model.clone(),
        budget_cents: s.budget_cents,
        timeout_secs: s.timeout_secs,
    }
}

/// Batas biaya 0,10–50 USD dan batas waktu 1–120 menit.
fn reviewer_settings(v: &ReviewerSettingsView) -> Result<ReviewerSettings, RpcError> {
    if !(10..=5_000).contains(&v.budget_cents) {
        return Err(RpcError::new(
            ErrorCode::InvalidParams,
            "The cost limit must be between $0.10 and $50",
        ));
    }
    if !(60..=7_200).contains(&v.timeout_secs) {
        return Err(RpcError::new(
            ErrorCode::InvalidParams,
            "The time limit must be between 1 and 120 minutes",
        ));
    }
    Ok(ReviewerSettings {
        model: v
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(str::to_owned),
        budget_cents: v.budget_cents,
        timeout_secs: v.timeout_secs,
    })
}

impl AppDispatch {
    pub(super) async fn capability(&self, token: &str) -> Option<mandor_api::CapabilityScope> {
        let s = self.app.session().await?;
        s.runs.verify_capability(token).as_ref().map(api_scope)
    }

    pub(super) async fn ledger_tool_call(
        &self,
        scope: &mandor_api::CapabilityScope,
        name: &str,
        v: Value,
    ) -> Result<Value, RpcError> {
        let tool = ledger_tool(name, v)?;
        let scope = usecase_scope(scope)
            .ok_or_else(|| RpcError::new(ErrorCode::Unauthorized, "unknown capability role"))?;
        let s = self.session().await?;
        let rows = match tool {
            LedgerTool::Info => {
                return ok(&LedgerToolInfo {
                    run_id: scope.run_id,
                    role: scope.role.as_str().to_owned(),
                });
            }
            LedgerTool::Read => s.runs.ledger_read(&scope).await,
            LedgerTool::SetDesign {
                ac_id,
                design,
                note,
            } => {
                s.runs
                    .ledger_set_design(&scope, &ac_id, design, note.as_deref())
                    .await
            }
            LedgerTool::SetVerdict {
                ac_id,
                verdict,
                evidence,
            } => {
                s.runs
                    .ledger_set_verdict(&scope, &ac_id, verdict, evidence.as_deref())
                    .await
            }
        }
        .map_err(|e| tool_error(&e))?;
        ok(&rows.iter().map(ac_row_view).collect::<Vec<_>>())
    }

    /// Status ekstensi Claude in Chrome: cache 60 detik di tabel `settings`;
    /// hasilnya juga dibaca saat review dimulai (KTD-32).
    async fn chrome_check(
        &self,
        s: &crate::session::LarkSession,
        force: bool,
    ) -> Result<ChromeCheckView, RpcError> {
        let store = &self.app.store;
        let now = crate::prod::now_ms();
        let cached = store
            .setting(CHROME_CHECKED_KEY)
            .await
            .map_err(super::store_error)?
            .and_then(|raw| serde_json::from_str::<ChromeCheckView>(&raw).ok());
        if let Some(c) = cached.filter(|c| !force && now - c.checked_at_ms < CHROME_CACHE_MS) {
            return Ok(c);
        }
        let cwd = self.app.cfg.home.join("agent");
        let check = s.agent.chrome_check(&cwd).await;
        let view = ChromeCheckView {
            connected: check.connected,
            detail: check.detail,
            checked_at_ms: now,
        };
        let raw = serde_json::to_string(&view).map_err(super::rejected)?;
        store
            .set_setting(CHROME_CHECKED_KEY, &raw)
            .await
            .map_err(super::store_error)?;
        store
            .set_setting(
                mandor_store::CHROME_KEY,
                if view.connected { "true" } else { "false" },
            )
            .await
            .map_err(super::store_error)?;
        Ok(view)
    }

    /// Skill plugin atau user masih terpasang.
    async fn skill_installed(&self, name: &str) -> bool {
        mandor_agent::available_skills(&self.app.cfg.user_home.join(".claude"))
            .await
            .iter()
            .any(|s| s.name == name)
    }

    pub(super) fn is_review_method(name: &str) -> bool {
        [
            method::LEDGER_GET,
            method::REVIEW_START,
            method::REVIEW_CANCEL,
            method::REVIEW_READINESS,
            method::SYSTEM_CHROME_CHECK,
            method::REVIEW_HISTORY,
            method::REVIEW_SEND_FINDINGS,
            method::SETTINGS_REVIEWER_GET,
            method::SETTINGS_REVIEWER_SET,
        ]
        .contains(&name)
    }

    pub(super) async fn review_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        let s = self.session().await?;
        match name {
            method::LEDGER_GET => {
                let p: RunRef = params(v)?;
                let r = s.runs.refresh_ledger(&p.run_id).await.map_err(run_error)?;
                let mode = s.runs.gate_mode_for(&p.run_id).await.map_err(run_error)?;
                ok(&AcLedgerView {
                    run_id: p.run_id,
                    rows: r.rows.iter().map(ac_row_view).collect(),
                    synced_at_ms: r.synced_at_ms,
                    sync_error: r.error,
                    design_sources: r.design_sources,
                    gate_mode: mode.as_str().to_owned(),
                })
            }
            method::REVIEW_START => {
                let p: ReviewStart = params(v)?;
                let trigger = match p.trigger.as_deref() {
                    None | Some("manual") => ReviewTrigger::Manual,
                    Some("push_and_pr") => ReviewTrigger::PushAndPr,
                    Some(other) => {
                        return Err(RpcError::new(
                            ErrorCode::InvalidParams,
                            format!("unknown review trigger: {other}"),
                        ));
                    }
                };
                let wanted = p.skill.as_deref().filter(|s| !s.trim().is_empty());
                let skill = match wanted {
                    Some(name) if !self.skill_installed(name).await => None,
                    other => other,
                };
                let rec = s
                    .runs
                    .start_review(&p.run_id, trigger, skill)
                    .await
                    .map_err(run_error)?;
                if let (Some(name), None) = (wanted, skill) {
                    let note = format!(
                        "Reviewer skill {name} is not installed; using Mandor's built-in reviewer."
                    );
                    s.runs
                        .system_note(&p.run_id, &note)
                        .await
                        .map_err(run_error)?;
                }
                ok(&review_view(&rec))
            }
            method::REVIEW_READINESS => {
                let p: RunRef = params(v)?;
                let r = s
                    .runs
                    .review_readiness(&p.run_id)
                    .await
                    .map_err(run_error)?;
                ok(&readiness_view(r))
            }
            method::SYSTEM_CHROME_CHECK => {
                let p: ChromeCheckParams = params(v)?;
                ok(&self.chrome_check(&s, p.force).await?)
            }
            method::REVIEW_CANCEL => {
                let p: RunRef = params(v)?;
                s.runs.cancel_review(&p.run_id).await.map_err(run_error)?;
                ok(&serde_json::json!({}))
            }
            method::REVIEW_HISTORY => {
                let p: RunRef = params(v)?;
                let all = s.runs.review_history(&p.run_id).await.map_err(run_error)?;
                ok(&all.iter().map(review_view).collect::<Vec<_>>())
            }
            method::REVIEW_SEND_FINDINGS => {
                let p: ReviewFindings = params(v)?;
                s.runs
                    .send_findings(&p.run_id, &p.ac_ids)
                    .await
                    .map_err(run_error)?;
                ok(&serde_json::json!({}))
            }
            method::SETTINGS_REVIEWER_GET => ok(&settings_view(&s.runs.reviewer_settings().await)),
            method::SETTINGS_REVIEWER_SET => {
                let p: ReviewerSettingsView = params(v)?;
                let settings = reviewer_settings(&p)?;
                s.runs
                    .set_reviewer_settings(&settings)
                    .await
                    .map_err(run_error)?;
                ok(&settings_view(&settings))
            }
            _ => Err(RpcError::new(
                mandor_protocol::ErrorCode::MethodNotFound,
                format!("unknown method: {name}"),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use mandor_domain::{AcRow, Design, RowStatus, Verdict};

    use super::*;

    #[test]
    fn tool_ledger_diurai_dan_method_lain_ditolak() {
        let t = ledger_tool(
            method::LEDGER_TOOL_SET_VERDICT,
            serde_json::json!({"ac_id": "rec1", "verdict": "cant_verify", "evidence": "x"}),
        )
        .unwrap();
        assert_eq!(
            t,
            LedgerTool::SetVerdict {
                ac_id: "rec1".to_owned(),
                verdict: Verdict::CantVerify,
                evidence: Some("x".to_owned()),
            }
        );
        let t = ledger_tool(
            method::LEDGER_TOOL_SET_DESIGN,
            serde_json::json!({"ac_id": "rec1", "design": "in_design"}),
        )
        .unwrap();
        assert!(matches!(
            t,
            LedgerTool::SetDesign {
                design: Design::InDesign,
                note: None,
                ..
            }
        ));
        let bad = ledger_tool(
            method::LEDGER_TOOL_SET_VERDICT,
            serde_json::json!({"ac_id": "rec1", "verdict": "lulus"}),
        )
        .unwrap_err();
        assert_eq!(bad.code, ErrorCode::InvalidParams);
        let denied = ledger_tool(method::STATUS, Value::Null).unwrap_err();
        assert_eq!(denied.code, ErrorCode::Unauthorized);
        assert_eq!(
            ledger_tool(method::LEDGER_TOOL_READ, Value::Null).unwrap(),
            LedgerTool::Read
        );
    }

    #[test]
    fn lingkup_kapabilitas_bolak_balik_dan_peran_asing_ditolak() {
        let s = CapabilityScope {
            run_id: "r1".to_owned(),
            role: CapabilityRole::Design,
        };
        assert_eq!(usecase_scope(&api_scope(&s)), Some(s));
        let odd = mandor_api::CapabilityScope {
            run_id: "r1".to_owned(),
            role: "admin".to_owned(),
        };
        assert_eq!(usecase_scope(&odd), None);
        assert_eq!(
            tool_error(&LedgerToolError::WrongRole).code,
            ErrorCode::Unauthorized
        );
        assert_eq!(
            tool_error(&LedgerToolError::UnknownAc("x".to_owned())).code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn setelan_reviewer_divalidasi() {
        let mut v = ReviewerSettingsView {
            model: Some(" ".to_owned()),
            budget_cents: 200,
            timeout_secs: 900,
        };
        assert_eq!(reviewer_settings(&v).unwrap().model, None);
        v.budget_cents = 5;
        assert!(reviewer_settings(&v).is_err());
        v.budget_cents = 200;
        v.timeout_secs = 10;
        assert!(reviewer_settings(&v).is_err());
    }

    #[test]
    fn baris_follow_design_tidak_ikut_gerbang_dan_fail_bisa_dipilih() {
        let mut r = AcRow::new("rec1", Some("AC-1"), "Nama tampil", RowStatus::New);
        r.verdict = Verdict::Fail;
        let v = ac_row_view(&r);
        assert_eq!(v.verdict, "fail");
        assert_eq!(v.status, "new");
        assert!(v.in_gate && v.selectable);
        r.design = Design::FollowDesign;
        let v = ac_row_view(&r);
        assert_eq!(v.design, "follow_design");
        assert!(!v.in_gate && !v.selectable);
    }
}
