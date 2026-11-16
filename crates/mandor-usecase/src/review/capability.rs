//! Token kapabilitas per giliran untuk tool ledger milik daemon (KTD-26).
//!
//! Token hanya berlaku untuk satu run dan satu peran, hidup selama satu
//! giliran, dan disimpan di memori: restart menghentikan semua sesi (KTD-13),
//! jadi token lama tidak perlu dipulihkan.

use std::sync::{Arc, PoisonError};

use mandor_domain::ac_ledger::{Visual, valid_page_path};
use mandor_domain::lab::{
    Evidence, Finding, LabNeed, LookAdvice, LookRecommendation, Simulated, check_lab_need,
};
use mandor_domain::{AcRow, Design, Verdict};
use secrecy::{ExposeSecret, SecretString};

use crate::ports::{Deps, Ids};
use crate::run::RunService;
use crate::run_ports::{
    AgentPort, AppRecord, AppRecords, LabPort, LedgerStore, ReviewHistory, RunStore,
};

/// Batas panjang bukti reviewer per AC.
const EVIDENCE_MAX: usize = 2_000;
const TRUNCATED: &str = " [truncated]";

/// Peran token: giliran plan mengisi Design, reviewer mengisi Verdict, alat
/// persetujuan meminta izin (plan 2026-11-06-001), dan giliran brainstorm/plan
/// tanpa sumber desain hanya bertanya (plan 2026-11-07-001, KTD-Q4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityRole {
    Design,
    Verdict,
    Permit,
    Ask,
    /// Pengusul perlengkapan lab (KTD-L14); lingkupnya kunci repo, bukan run.
    LabSetup,
}

impl CapabilityRole {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Verdict => "verdict",
            Self::Permit => "permit",
            Self::Ask => "ask",
            Self::LabSetup => "lab_setup",
        }
    }
}

/// Token kapabilitas. `Debug` tidak pernah mencetak isinya; perbandingan
/// berjalan dalam waktu konstan.
#[derive(Clone)]
pub struct CapabilityToken(SecretString);

impl CapabilityToken {
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Token tetap untuk test adapter; tidak pernah terdaftar di registri.
    #[must_use]
    pub fn for_test(raw: &str) -> Self {
        Self(SecretString::from(raw.to_owned()))
    }
}

impl std::fmt::Debug for CapabilityToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CapabilityToken(..)")
    }
}

impl PartialEq for CapabilityToken {
    fn eq(&self, other: &Self) -> bool {
        ct_eq(self.expose().as_bytes(), other.expose().as_bytes())
    }
}

impl Eq for CapabilityToken {}

/// Akses ledger untuk satu giliran agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerAccess {
    pub role: CapabilityRole,
    pub token: CapabilityToken,
}

/// Lingkup koneksi yang memakai token kapabilitas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityScope {
    pub run_id: String,
    pub role: CapabilityRole,
}

/// Kesalahan operasi ledger dari agent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerToolError {
    #[error("this session cannot change that column")]
    WrongRole,
    #[error("unknown acceptance criterion: {0}")]
    UnknownAc(String),
    #[error("value not allowed here: {0}")]
    NotAllowed(&'static str),
    /// Isian ditolak dengan alasan yang bisa diperbaiki agent.
    #[error("{0}")]
    Invalid(String),
    #[error("storage error: {0}")]
    Storage(String),
}

impl From<crate::PortError> for LedgerToolError {
    fn from(e: crate::PortError) -> Self {
        Self::Storage(e.to_string())
    }
}

/// Laporan verdict satu baris dari reviewer (alat `ac_set_verdict`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerdictReport {
    pub evidence: Option<String>,
    pub lab_need: Option<LabNeed>,
    pub steps: Vec<String>,
    pub screenshots: Vec<String>,
    pub prepared: Option<String>,
    pub simulated: Simulated,
}

/// Konteks review berjalan untuk tool ledger.
#[derive(Debug, Clone, Default)]
struct ReviewContext {
    review_id: String,
    repos: Vec<String>,
    browser: bool,
}

/// Teks bukti setelah disamarkan.
struct RedactedTexts {
    evidence: Option<String>,
    prepared: Option<String>,
    steps: Vec<String>,
}

/// Langkah bernomor sebagai teks bukti lama.
fn numbered(steps: &[String]) -> String {
    steps
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}. {s}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Laporan tampilan satu baris dari reviewer (alat `ac_set_visual`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VisualReport {
    pub description: Option<String>,
    pub app_path: Option<String>,
    pub prototype_path: Option<String>,
    /// Nama berkas salinan daemon (run VM); diisi dispatch setelah diperiksa.
    pub screenshots: Vec<String>,
}

/// Anggota prototype yang menyala di record app: `(url, revisi)`.
fn running_prototype(rec: &AppRecord) -> Option<(String, String)> {
    rec.members
        .iter()
        .filter(|m| {
            m.prototype.is_some() && m.state == mandor_domain::app_run::AppRunState::Running
        })
        .find_map(|m| Some((m.url.clone()?, rec.snapshot.get(&m.repo_id)?.clone())))
}

/// Perbandingan byte dalam waktu yang tidak bergantung pada isi.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn truncate_evidence(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= EVIDENCE_MAX {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(EVIDENCE_MAX).collect();
    out.push_str(TRUNCATED);
    out
}

/// Token yang berlaku: (token, lingkup).
pub(crate) type Issued = Vec<(CapabilityToken, CapabilityScope)>;

/// Giliran yang memegang token `Permit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermitGrant {
    pub turn: mandor_domain::permit::TurnKind,
    pub permit: crate::run_ports::PermitRef,
}

impl<D: Deps> RunService<D> {
    /// Membuat token untuk run dan peran.
    pub fn issue_capability(&self, run_id: &str, role: CapabilityRole) -> CapabilityToken {
        let raw = format!("{}{}", self.sync.ids.nonce(), self.sync.ids.nonce()).replace('-', "");
        let token = CapabilityToken(SecretString::from(raw));
        self.capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((
                token.clone(),
                CapabilityScope {
                    run_id: run_id.to_owned(),
                    role,
                },
            ));
        token
    }

    /// Lingkup token bila masih berlaku. Semua token dibandingkan tanpa berhenti
    /// lebih awal.
    #[must_use]
    pub fn verify_capability(&self, token: &str) -> Option<CapabilityScope> {
        if token.is_empty() {
            return None;
        }
        let issued = self
            .capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut found = None;
        for (t, scope) in issued.iter() {
            if ct_eq(t.expose().as_bytes(), token.as_bytes()) {
                found = Some(scope.clone());
            }
        }
        found
    }

    /// Mencabut semua token run (giliran selesai atau run ditutup).
    pub fn revoke_capabilities(&self, run_id: &str) {
        self.capabilities
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(_, s)| s.run_id != run_id);
        self.permit_grants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(run_id);
    }

    /// Token `Permit` untuk giliran implementasi atau plan, beserta berkas
    /// izin dan jenis gilirannya (KTD-A10).
    pub(crate) fn issue_permit(&self, run_id: &str, grant: PermitGrant) -> CapabilityToken {
        self.permit_grants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run_id.to_owned(), grant);
        self.issue_capability(run_id, CapabilityRole::Permit)
    }

    /// Berkas izin dan jenis giliran untuk token `Permit` run.
    #[must_use]
    pub fn permit_grant(&self, run_id: &str) -> Option<PermitGrant> {
        self.permit_grants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(run_id)
            .cloned()
    }

    /// Buku AC untuk tool `ac_ledger_read`.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn ledger_read(
        &self,
        scope: &CapabilityScope,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        Ok(self.reviews.ac_rows(&scope.run_id).await?)
    }

    /// Tool `ac_set_design` (peran Design). Follow design hanya dari dev (R8a).
    ///
    /// # Errors
    /// Peran salah, AC tidak dikenal, nilai tidak diizinkan, atau penyimpanan gagal.
    pub async fn ledger_set_design(
        &self,
        scope: &CapabilityScope,
        ac_id: &str,
        design: Design,
        note: Option<&str>,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        if scope.role != CapabilityRole::Design {
            return Err(LedgerToolError::WrongRole);
        }
        if design == Design::FollowDesign {
            return Err(LedgerToolError::NotAllowed("follow_design"));
        }
        self.update_row(&scope.run_id, ac_id, |r| {
            r.design = design;
            r.design_note = note.map(truncate_evidence).filter(|n| !n.is_empty());
        })
        .await
    }

    /// Tool `ac_set_verdict` (peran Verdict): lulus, gagal, atau tidak bisa
    /// dipastikan, dengan bukti teks biasa.
    ///
    /// # Errors
    /// Peran salah, AC tidak dikenal atau di luar gerbang, nilai tidak
    /// diizinkan, atau penyimpanan gagal.
    pub async fn ledger_set_verdict(
        &self,
        scope: &CapabilityScope,
        ac_id: &str,
        verdict: Verdict,
        report: VerdictReport,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        if scope.role != CapabilityRole::Verdict {
            return Err(LedgerToolError::WrongRole);
        }
        self.lab_scope(&scope.run_id, ac_id)?;
        if !matches!(verdict, Verdict::Pass | Verdict::Fail | Verdict::CantVerify) {
            return Err(LedgerToolError::NotAllowed(verdict.as_str()));
        }
        check_lab_need(verdict, report.lab_need)
            .map_err(|_| LedgerToolError::NotAllowed("lab_need is only for cant_verify"))?;
        if report.simulated == Simulated::Sql {
            return Err(LedgerToolError::NotAllowed(
                "simulated sql is set by Mandor",
            ));
        }
        let rows = self.reviews.ac_rows(&scope.run_id).await?;
        let Some(row) = rows.iter().find(|r| r.ac_id == ac_id) else {
            return Err(LedgerToolError::UnknownAc(ac_id.to_owned()));
        };
        if !row.in_gate() {
            return Err(LedgerToolError::NotAllowed("ac outside the gate"));
        }
        let ctx = self.review_context(&scope.run_id).await;
        if verdict == Verdict::Pass && ctx.browser && report.steps.is_empty() {
            return Err(LedgerToolError::NotAllowed(
                "a pass checked in the browser needs numbered steps",
            ));
        }
        let mut verdict = verdict;
        let mut lab_need = report.lab_need;
        let mut evidence_text = report.evidence.clone();
        // Diambil setelah baris dipastikan ada, agar penolakan tidak hilang.
        let mut hits = self.lab.take_blocked(&ctx.review_id);
        // URL disimpan tanpa query string, lalu disamarkan seperti teks bukti.
        let urls: Vec<String> = hits
            .iter()
            .map(|h| {
                h.request
                    .url
                    .split(['?', '#'])
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        let urls = self.lab.redact(&ctx.repos, &ctx.review_id, urls).await;
        for (h, url) in hits.iter_mut().zip(urls) {
            h.request.url = url;
        }
        // Error dari request tulis yang diblokir proxy bukan perilaku app
        // (KTD-L6): verdict menjadi Can't verify yang perlu lab data.
        if let Some(w) = hits.iter().find(|h| h.write)
            && matches!(verdict, Verdict::Pass | Verdict::Fail)
        {
            verdict = Verdict::CantVerify;
            lab_need = Some(LabNeed::Data);
            evidence_text = Some(format!(
                "The review proxy blocked a save request ({} {}); this check needs prepared data.",
                w.request.method, w.request.url
            ));
        }
        let screenshots = self
            .keep_screenshots(&scope.run_id, ac_id, &report.screenshots)
            .await;
        let texts = self
            .redacted(&ctx, evidence_text, report.prepared.clone(), &report.steps)
            .await;
        let detail = Evidence {
            steps: texts.steps,
            screenshots,
            prepared: texts.prepared,
            simulated: report.simulated,
            blocked_requests: hits.into_iter().map(|h| h.request).collect(),
            lab: self.lab_running(&scope.run_id),
        };
        let text = texts
            .evidence
            .or_else(|| (!detail.steps.is_empty()).then(|| numbered(&detail.steps)));
        let has_detail = detail != Evidence::default();
        self.update_row(&scope.run_id, ac_id, |r| {
            r.verdict = verdict;
            r.evidence = text
                .as_deref()
                .map(truncate_evidence)
                .filter(|e| !e.is_empty());
            r.lab_need = lab_need;
            r.evidence_detail = has_detail.then_some(detail);
            r.stale = false;
            // Keputusan dev lama tetap di baris; review run grup memulihkannya
            // bila verdict tidak berubah (R18). Run tunggal sudah mengosongkannya
            // saat review dimulai.
        })
        .await
    }

    /// Tool `ac_report_finding` (peran Verdict): temuan di luar AC disimpan
    /// terpisah dan tidak mengubah verdict mana pun (R19).
    ///
    /// # Errors
    /// Peran salah, judul kosong, atau penyimpanan gagal.
    pub async fn ledger_report_finding(
        &self,
        scope: &CapabilityScope,
        finding: Finding,
    ) -> Result<(), LedgerToolError> {
        if scope.role != CapabilityRole::Verdict {
            return Err(LedgerToolError::WrongRole);
        }
        if finding.title.trim().is_empty() {
            return Err(LedgerToolError::NotAllowed("empty title"));
        }
        let ctx = self.review_context(&scope.run_id).await;
        let mut texts = self
            .lab
            .redact(
                &ctx.repos,
                &ctx.review_id,
                vec![finding.title.clone(), finding.detail.clone()],
            )
            .await
            .into_iter();
        let steps = self
            .lab
            .redact(&ctx.repos, &ctx.review_id, finding.evidence.steps.clone())
            .await;
        let screenshots = self
            .keep_screenshots(&scope.run_id, "finding", &finding.evidence.screenshots)
            .await;
        let clean = Finding {
            card: finding.card,
            title: truncate_evidence(&texts.next().unwrap_or_default()),
            detail: truncate_evidence(&texts.next().unwrap_or_default()),
            evidence: Evidence {
                steps: steps.iter().map(|s| truncate_evidence(s)).collect(),
                screenshots,
                ..Evidence::default()
            },
        };
        self.reviews
            .save_finding(&ctx.review_id, &clean, self.now())
            .await?;
        Ok(())
    }

    /// Tool `ac_recommend_look` (peran Verdict): rekomendasi untuk baris
    /// *looks different*; developer yang memutuskan (R18).
    ///
    /// # Errors
    /// Peran salah, baris bukan *looks different*, atau penyimpanan gagal.
    pub async fn ledger_recommend_look(
        &self,
        scope: &CapabilityScope,
        ac_id: &str,
        advice: LookAdvice,
        reason: &str,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        if scope.role != CapabilityRole::Verdict {
            return Err(LedgerToolError::WrongRole);
        }
        self.lab_scope(&scope.run_id, ac_id)?;
        let rows = self.reviews.ac_rows(&scope.run_id).await?;
        if rows
            .iter()
            .any(|r| r.ac_id == ac_id && r.visual != Visual::Differs)
        {
            return Err(LedgerToolError::NotAllowed(
                "recommendations are only for rows that look different",
            ));
        }
        let ctx = self.review_context(&scope.run_id).await;
        let reason = self
            .lab
            .redact(&ctx.repos, &ctx.review_id, vec![reason.to_owned()])
            .await
            .into_iter()
            .next()
            .unwrap_or_default();
        self.update_row(&scope.run_id, ac_id, |r| {
            r.look_recommendation = Some(LookRecommendation {
                advice,
                reason: truncate_evidence(&reason),
            });
        })
        .await
    }

    /// Review berjalan untuk run: id, repo anggota, dan apakah browser dipakai.
    async fn review_context(&self, run_id: &str) -> ReviewContext {
        let Ok(Some(run)) = self.runs.get(run_id).await else {
            return ReviewContext::default();
        };
        let review_id = run.review_id.clone().unwrap_or_default();
        let browser = self
            .reviews
            .reviews(run_id)
            .await
            .ok()
            .and_then(|all| all.into_iter().find(|r| r.id == review_id))
            .is_some_and(|r| r.browser_used);
        let repos = self.members(&run).await.map_or_else(
            |_| vec![run.repo_id.clone()],
            |m| m.iter().map(|m| m.repo.id.clone()).collect(),
        );
        ReviewContext {
            review_id,
            repos,
            browser,
        }
    }

    /// Teks bukti yang sudah disamarkan: bukti (dan data yang disiapkan),
    /// serta langkah.
    async fn redacted(
        &self,
        ctx: &ReviewContext,
        evidence: Option<String>,
        prepared: Option<String>,
        steps: &[String],
    ) -> RedactedTexts {
        let (has_evidence, has_prepared) = (evidence.is_some(), prepared.is_some());
        let all: Vec<String> = evidence
            .into_iter()
            .chain(prepared)
            .chain(steps.iter().cloned())
            .collect();
        let mut out = self
            .lab
            .redact(&ctx.repos, &ctx.review_id, all)
            .await
            .into_iter();
        let evidence = if has_evidence { out.next() } else { None };
        let prepared = if has_prepared { out.next() } else { None };
        RedactedTexts {
            evidence,
            prepared: prepared.map(|p| truncate_evidence(&p)),
            steps: out.map(|s| truncate_evidence(&s)).collect(),
        }
    }

    /// URL dan revisi prototype yang sedang menyala untuk run ini, bila ada.
    pub(crate) async fn running_prototype(&self, run_id: &str) -> Option<(String, String)> {
        if self.apps.app_owner().await.ok().flatten().as_deref() != Some(run_id) {
            return None;
        }
        running_prototype(&self.apps.app_record(run_id).await.ok().flatten()?)
    }

    /// URL prototype yang bisa dibandingkan reviewer untuk run ini (R16).
    pub async fn visual_prototype_url(&self, run_id: &str) -> Option<String> {
        self.running_prototype(run_id).await.map(|(url, _)| url)
    }

    /// Tool `ac_set_visual` (peran Verdict): hasil perbandingan tampilan satu
    /// baris yang ada di desain, dengan revisi prototype yang menyala (R17,
    /// R21). Keputusan dev (Accepted, Checked manually) tidak bisa ditulis
    /// agent (R25).
    ///
    /// # Errors
    /// Peran salah, AC tidak dikenal atau tidak dibandingkan, nilai atau path
    /// tidak diizinkan, prototype tidak menyala, atau penyimpanan gagal.
    pub async fn ledger_set_visual(
        &self,
        scope: &CapabilityScope,
        ac_id: &str,
        result: Visual,
        report: VisualReport,
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        if scope.role != CapabilityRole::Verdict {
            return Err(LedgerToolError::WrongRole);
        }
        self.lab_scope(&scope.run_id, ac_id)?;
        if !matches!(result, Visual::Match | Visual::Differs | Visual::CantVerify) {
            return Err(LedgerToolError::NotAllowed(result.as_str()));
        }
        let paths = [&report.app_path, &report.prototype_path];
        if paths.into_iter().flatten().any(|p| !valid_page_path(p)) {
            return Err(LedgerToolError::NotAllowed("page path"));
        }
        let Some((_, rev)) = self.running_prototype(&scope.run_id).await else {
            return Err(LedgerToolError::NotAllowed(
                "no prototype is running for this run",
            ));
        };
        let rows = self.reviews.ac_rows(&scope.run_id).await?;
        if rows.iter().any(|r| r.ac_id == ac_id && !r.compares_look()) {
            return Err(LedgerToolError::NotAllowed("ac not in the design"));
        }
        let screenshots = self
            .keep_screenshots(&scope.run_id, ac_id, &report.screenshots)
            .await;
        self.update_row(&scope.run_id, ac_id, |r| {
            r.visual = result;
            let d = &mut r.visual_detail;
            d.description = report
                .description
                .as_deref()
                .map(truncate_evidence)
                .filter(|t| !t.is_empty());
            d.app_path = report.app_path;
            d.prototype_path = report.prototype_path;
            d.prototype_rev = Some(rev);
            d.screenshots = screenshots;
        })
        .await
    }

    /// Salinan screenshot reviewer milik daemon, di laptop dan VM (R16).
    /// Salinan yang gagal hanya dicatat; hasil tetap disimpan.
    async fn keep_screenshots(&self, run_id: &str, ac_id: &str, names: &[String]) -> Vec<String> {
        if names.is_empty() {
            return Vec::new();
        }
        let review_id = match self.runs.get(run_id).await {
            Ok(Some(run)) => run.review_id.unwrap_or_default(),
            _ => return Vec::new(),
        };
        match self
            .agent
            .keep_screenshots(&review_id, run_id, ac_id, names)
            .await
        {
            Ok(kept) => kept,
            Err(e) => {
                tracing::warn!(run_id, ac_id, error = %e, "screenshot tampilan tidak disimpan");
                Vec::new()
            }
        }
    }

    /// Isi screenshot ke-`index` hasil tampilan baris `ac_id` (run VM).
    ///
    /// # Errors
    /// Baris atau screenshot tidak ada, atau berkas tidak terbaca.
    pub async fn visual_screenshot(
        &self,
        run_id: &str,
        ac_id: &str,
        index: usize,
    ) -> Result<Vec<u8>, crate::RunError> {
        let rows = self.reviews.ac_rows(run_id).await?;
        let name = rows
            .iter()
            .find(|r| r.ac_id == ac_id)
            .and_then(|r| r.visual_detail.screenshots.get(index).cloned())
            .ok_or(crate::RunError::NotFound("screenshot"))?;
        Ok(self.agent.screenshot(run_id, &name).await?)
    }

    /// Screenshot bukti verdict ke-`index` milik baris (R16).
    ///
    /// # Errors
    /// Baris atau screenshot tidak ada, atau salinan tidak terbaca.
    pub async fn evidence_screenshot(
        &self,
        run_id: &str,
        ac_id: &str,
        index: usize,
    ) -> Result<Vec<u8>, crate::RunError> {
        let rows = self.reviews.ac_rows(run_id).await?;
        let name = rows
            .iter()
            .find(|r| r.ac_id == ac_id)
            .and_then(|r| r.evidence_detail.as_ref())
            .and_then(|d| d.screenshots.get(index).cloned())
            .ok_or(crate::RunError::NotFound("screenshot"))?;
        Ok(self.agent.screenshot(run_id, &name).await?)
    }

    /// Keputusan dev atas rekomendasi reviewer untuk baris *looks different*
    /// (R18, KTD-L13): Approve pada `accept` menerima perbedaan; Reject dan
    /// `follow_prototype` membiarkan baris tetap menahan push.
    ///
    /// # Errors
    /// Baris tidak ada atau tanpa rekomendasi, review sedang berjalan, atau
    /// penyimpanan gagal.
    pub async fn decide_look_recommendation(
        &self,
        run_id: &str,
        ac_id: &str,
        approve: bool,
        by: Option<&str>,
    ) -> Result<Vec<AcRow>, crate::RunError> {
        let run = self.load(run_id).await?;
        if run.state == mandor_domain::RunState::Reviewing {
            return Err(crate::RunError::InvalidState("review is running"));
        }
        let mut rows = self.reviews.ac_rows(run_id).await?;
        let row = rows
            .iter_mut()
            .find(|r| r.ac_id == ac_id)
            .ok_or(crate::RunError::NotFound("acceptance criterion"))?;
        if !mandor_domain::ac_ledger::decide_look(row, approve, by) {
            return Err(crate::RunError::InvalidState("no recommendation to decide"));
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(rows)
    }

    /// Temuan reviewer di luar AC untuk run (R19): `(review_id, temuan)`.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn run_findings(
        &self,
        run_id: &str,
    ) -> Result<Vec<(String, Finding)>, crate::RunError> {
        Ok(self.reviews.findings(run_id).await?)
    }

    async fn update_row(
        &self,
        run_id: &str,
        ac_id: &str,
        change: impl FnOnce(&mut AcRow),
    ) -> Result<Vec<AcRow>, LedgerToolError> {
        let mut rows = self.reviews.ac_rows(run_id).await?;
        let row = rows
            .iter_mut()
            .find(|r| r.ac_id == ac_id)
            .ok_or_else(|| LedgerToolError::UnknownAc(ac_id.to_owned()))?;
        change(row);
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(rows)
    }
}

/// Registri kosong untuk [`RunService::new`].
pub(crate) fn empty() -> Arc<std::sync::Mutex<Issued>> {
    Arc::new(std::sync::Mutex::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perbandingan_konstan_dan_debug_tidak_membocorkan() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        let t = CapabilityToken(SecretString::from("rahasia".to_owned()));
        assert_eq!(format!("{t:?}"), "CapabilityToken(..)");
        assert_eq!(t, t.clone());
        assert_eq!(CapabilityRole::Design.as_str(), "design");
        assert_eq!(CapabilityRole::Verdict.as_str(), "verdict");
    }

    #[test]
    fn bukti_dipotong_dengan_penanda() {
        assert_eq!(truncate_evidence("  ok  "), "ok");
        let long = truncate_evidence(&"y".repeat(3_000));
        assert!(long.ends_with(TRUNCATED));
        assert_eq!(
            long.chars().count(),
            EVIDENCE_MAX + TRUNCATED.chars().count()
        );
    }
}
