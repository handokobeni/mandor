//! Tahap review lab (plan 2026-11-10-001, U7): setelah review biasa, AC yang
//! tersisa *Can't verify* dengan `lab_need` diuji di sesi lab per kartu,
//! berurutan, dengan akun uji per role, proxy review yang sama, dan berkas uji
//! dari staging.
//!
//! Progres lab hanya ada di memori. Setelah daemon restart, review yang
//! terputus ditangani rekonsiliasi seperti review biasa.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use mandor_domain::lab::{CardCandidates, LabEquipment, lab_candidates};
use mandor_domain::{AcRow, RunState, Verdict};

use super::capability::CapabilityRole;
use super::service::ReviewEnd;
use crate::ports::{Deps, Ids};
use crate::run::{RunError, RunService};
use crate::run_ports::{
    AgentPort, BrowserSession, ChatItem, LabBrief, LabLogin, LabPort, LabTurn, LedgerStore,
    Prompts, ReviewHistory, ReviewRequest, ReviewStatus, RunRecord, RunStore, SessionRef,
};

/// Teks yang ditambahkan ke bukti AC yang tidak dilaporkan lab.
const NOT_REPORTED: &str = "The review lab did not report this criterion.";
/// Teks yang ditambahkan ke bukti AC yang lewat batas lab per run.
const RUN_LIMIT: &str = "The review lab run limit was reached before this criterion.";

/// Rencana lab yang dicatat saat review biasa mulai dengan login perlengkapan
/// (KTD-L7): perlengkapan, origin, dan proxy review yang sama.
#[derive(Debug, Clone)]
pub(crate) struct LabPlan {
    pub repo_id: String,
    pub equipment: LabEquipment,
    pub origins: Vec<String>,
    pub proxy: Option<String>,
    /// Lab data boleh berjalan: VM, atau developer memilih Run lab di laptop
    /// (KTD-L23).
    pub data_allowed: bool,
}

/// Progres lab satu review.
#[derive(Debug, Clone)]
pub(crate) struct LabProgress {
    pub(crate) review_id: String,
    pub(crate) plan: LabPlan,
    pub(crate) cards: Vec<CardCandidates>,
    index: usize,
    started_ms: i64,
    /// Biaya review biasa ditambah semua sesi lab.
    cost_usd: f64,
    /// Akhir review biasa; dipakai ekor review dengan biaya total.
    review_end: ReviewEnd,
    /// ID berkas sesi lab yang berjalan (`<review>-lab<n>`).
    session: String,
    /// Baris asli calon lab kartu yang berjalan.
    originals: HashMap<String, AcRow>,
    fixtures: Option<PathBuf>,
    fixtures_staged: bool,
    /// Sesi lab data yang berjalan (U14).
    pub(crate) data: Option<super::lab_data::DataSession>,
    pub(crate) data_attempted: bool,
    /// Reviewer lab mengubah data lewat `lab_sql`/`lab_seed` di kartu ini.
    pub(crate) data_changed: bool,
}

impl<D: Deps> RunService<D> {
    /// Mencatat rencana lab untuk run (dipanggil saat review biasa mulai).
    pub(crate) fn remember_lab_plan(&self, run_id: &str, plan: LabPlan) {
        self.lab_plans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run_id.to_owned(), plan);
    }

    /// Sesi lab sedang berjalan untuk run.
    pub(crate) fn lab_running(&self, run_id: &str) -> bool {
        self.lab_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(run_id)
    }

    /// Selama sesi lab, tool ledger hanya boleh menyentuh calon lab kartu
    /// yang sedang diuji; baris lain (mis. Fail dari review biasa) tertutup.
    pub(crate) fn lab_scope(
        &self,
        run_id: &str,
        ac_id: &str,
    ) -> Result<(), super::LedgerToolError> {
        let runs = self.lab_runs.lock().unwrap_or_else(PoisonError::into_inner);
        match runs.get(run_id) {
            Some(p) if !p.originals.contains_key(ac_id) => Err(super::LedgerToolError::NotAllowed(
                "this lab session only reports the criteria it was given",
            )),
            _ => Ok(()),
        }
    }

    /// Progres lab bila ada calon lab dan perlengkapan; tanpa perlengkapan,
    /// developer diberi tahu cara menyiapkannya (AE7).
    pub(crate) async fn plan_lab(
        &self,
        run: &RunRecord,
        review_id: &str,
        rows: &[AcRow],
        end: &ReviewEnd,
    ) -> Result<Option<LabProgress>, RunError> {
        let plan = self
            .lab_plans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&run.id);
        // Calon `data` hanya bila perlengkapan punya bagian data (fase 2).
        let data = plan.as_ref().is_some_and(|p| p.equipment.data.is_some());
        let cards = lab_candidates(rows, data);
        if cards.is_empty() {
            return Ok(None);
        }
        let Some(plan) = plan else {
            let n: usize = cards.iter().map(|c| c.ac_ids.len()).sum();
            let note = format!(
                "{n} criteria need the review lab. Approve lab equipment in the repo settings so Mandor can test them."
            );
            self.runs
                .append(&run.id, &ChatItem::System(note), self.now())
                .await?;
            return Ok(None);
        };
        Ok(Some(LabProgress {
            review_id: review_id.to_owned(),
            plan,
            cards,
            index: 0,
            started_ms: self.now(),
            cost_usd: end.cost_usd.unwrap_or_default(),
            review_end: end.clone(),
            session: String::new(),
            originals: HashMap::new(),
            fixtures: None,
            fixtures_staged: false,
            data: None,
            data_attempted: false,
            data_changed: false,
        }))
    }

    /// Memulai sesi lab kartu berikutnya, atau menutup review bila semua
    /// kartu selesai atau batas lab run tercapai.
    pub(crate) async fn start_lab_card(
        self: &Arc<Self>,
        mut run: RunRecord,
        generation: i64,
        mut progress: LabProgress,
    ) -> Result<(), RunError> {
        self.begin_data(&run, &mut progress).await?;
        if progress.index >= progress.cards.len() {
            return self.finish_lab(run, generation, progress).await;
        }
        let settings = self.reviews.lab_settings().await;
        let spent = progress.cost_usd * 100.0;
        let elapsed = self.now().saturating_sub(progress.started_ms);
        let over_budget = spent >= f64::from(settings.run_budget_cents);
        let over_time =
            u64::try_from(elapsed).unwrap_or(u64::MAX) >= settings.run_timeout_secs * 1000;
        if over_budget || over_time {
            let rest: Vec<String> = progress.cards[progress.index..]
                .iter()
                .flat_map(|c| c.ac_ids.clone())
                .collect();
            self.note_rows(&run.id, &rest, RUN_LIMIT).await?;
            progress.index = progress.cards.len();
            return self.finish_lab(run, generation, progress).await;
        }
        let card = progress.cards[progress.index].clone();
        let rows = self
            .prepare_card_rows(&run.id, &card, &mut progress)
            .await?;
        let (roles, sessions, unavailable) = self.lab_logins(&progress).await;
        self.stage_lab_fixtures(&run, &mut progress).await;
        let task_id = card.card.clone().unwrap_or_else(|| run.record_id.clone());
        let task = self.task_of(&task_id).await?;
        let message = self.prompts.lab_message(&LabBrief {
            task: &task,
            rows: &rows,
            roles: &roles,
            unavailable: &unavailable,
            app_urls: &progress.plan.origins,
            fixtures: progress.fixtures.as_deref(),
            data: progress
                .data
                .as_ref()
                .map(|d| (d.ready.sql_stores.as_slice(), d.ready.seeds.as_slice())),
        });
        let session = format!("{}-lab{}", progress.review_id, progress.index);
        let reviewer = self.reviews.reviewer_settings().await;
        let mut req = self
            .agent_request(&mut run, SessionRef::New(self.sync.ids.nonce()), message)
            .await?;
        req.plan_mode = false;
        req.ledger = Some(super::LedgerAccess {
            role: CapabilityRole::Verdict,
            token: self.issue_capability(&run.id, CapabilityRole::Verdict),
        });
        req.review = Some(ReviewRequest {
            review_id: session.clone(),
            origins: progress.plan.origins.clone(),
            browser: !sessions.is_empty(),
            model: settings.model.clone().or(reviewer.model),
            budget_cents: settings.budget_cents.max(1),
            timeout: Duration::from_secs(settings.timeout_secs.max(60)),
            proxy: progress.plan.proxy.clone(),
            sessions,
            lab: Some(LabTurn {
                fixtures: progress.fixtures.clone(),
                setup: false,
            }),
        });
        progress.session = session;
        let note = format!(
            "Review lab started for {} ({} criteria).",
            task.key.as_deref().unwrap_or(&task.title),
            rows.len()
        );
        self.runs
            .append(&run.id, &ChatItem::System(note), self.now())
            .await?;
        self.lab_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(run.id.clone(), progress);
        run.state = RunState::Reviewing;
        let run_id = run.id.clone();
        if let Err(e) = self.spawn_lab_turn_boxed(run, req).await {
            let progress = self
                .lab_runs
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&run_id);
            if let Some(p) = progress {
                tracing::warn!(run_id, error = %e, "sesi lab tidak bisa dimulai");
                self.restore_originals(&run_id, &p, NOT_REPORTED).await?;
                let run = self.load(&run_id).await?;
                return self.finish_lab(run, generation, p).await;
            }
            return Err(e);
        }
        Ok(())
    }

    /// Versi ter-box dari `spawn_turn` untuk memutus siklus tipe async (akhir
    /// giliran lab → sesi lab berikutnya).
    fn spawn_lab_turn_boxed(
        self: &Arc<Self>,
        mut run: RunRecord,
        req: crate::run_ports::AgentRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), RunError>> + Send + '_>>
    {
        Box::pin(async move { self.spawn_turn(&mut run, req, None).await })
    }

    /// Akhir satu giliran lab: baris yang tidak dilaporkan kembali ke hasil
    /// review biasa dengan catatan, lalu kartu berikutnya atau ekor review
    /// (KTD-L9).
    pub(crate) async fn finish_lab_turn(
        self: &Arc<Self>,
        run: RunRecord,
        generation: i64,
        end: ReviewEnd,
    ) -> Result<(), RunError> {
        let progress = self
            .lab_runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&run.id);
        let Some(mut progress) = progress else {
            return self.finish_review(run, generation, end).await;
        };
        self.agent.end_review(&progress.session).await;
        progress.cost_usd += end.cost_usd.unwrap_or_default();
        self.restore_originals(&run.id, &progress, NOT_REPORTED)
            .await?;
        if self.review_cancelled(&run.id, &progress.review_id).await {
            progress.index = progress.cards.len();
            return self.finish_lab(run, generation, progress).await;
        }
        progress.index += 1;
        progress.originals.clear();
        progress.data_changed = false;
        if progress.index < progress.cards.len()
            && let Some(session) = progress.data.clone()
            && !self.data_between_cards(&run.id, &session).await
        {
            progress.data = None;
            let rows = self.reviews.ac_rows(&run.id).await?;
            let rest: Vec<String> = progress.cards[progress.index..]
                .iter()
                .flat_map(|c| c.ac_ids.clone())
                .filter(|id| {
                    rows.iter().any(|r| {
                        &r.ac_id == id && r.lab_need == Some(mandor_domain::lab::LabNeed::Data)
                    })
                })
                .collect();
            self.note_rows(
                &run.id,
                &rest,
                "Can't verify: the lab data could not be restored between cards.",
            )
            .await?;
            for c in &mut progress.cards[progress.index..] {
                c.ac_ids.retain(|id| !rest.contains(id));
            }
        }
        self.start_lab_card(run, generation, progress).await
    }

    /// Ekor review setelah lab: biaya total review biasa ditambah lab.
    async fn finish_lab(
        self: &Arc<Self>,
        run: RunRecord,
        generation: i64,
        mut progress: LabProgress,
    ) -> Result<(), RunError> {
        self.end_data(&run.id, &mut progress).await;
        let mut end = progress.review_end;
        end.cost_usd = Some(progress.cost_usd);
        let status = if self.review_cancelled(&run.id, &progress.review_id).await {
            ReviewStatus::Cancelled
        } else {
            ReviewStatus::Finished
        };
        self.finish_review_tail(run, generation, end, status).await
    }

    async fn review_cancelled(&self, run_id: &str, review_id: &str) -> bool {
        self.reviews.reviews(run_id).await.is_ok_and(|all| {
            all.iter()
                .any(|r| r.id == review_id && r.status == ReviewStatus::Cancelled)
        })
    }

    /// Baris calon kartu ini dikosongkan menjadi Not reviewed agar baris yang
    /// tidak dilaporkan lab terdeteksi; aslinya disimpan.
    async fn prepare_card_rows(
        &self,
        run_id: &str,
        card: &CardCandidates,
        progress: &mut LabProgress,
    ) -> Result<Vec<AcRow>, RunError> {
        let mut rows = self.reviews.ac_rows(run_id).await?;
        let mut picked = Vec::new();
        for r in rows.iter_mut().filter(|r| card.ac_ids.contains(&r.ac_id)) {
            progress.originals.insert(r.ac_id.clone(), r.clone());
            picked.push(r.clone());
            r.verdict = Verdict::NotReviewed;
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(picked)
    }

    /// Baris yang masih Not reviewed kembali ke hasil review biasa dengan
    /// catatan.
    async fn restore_originals(
        &self,
        run_id: &str,
        progress: &LabProgress,
        note: &str,
    ) -> Result<(), RunError> {
        let mut rows = self.reviews.ac_rows(run_id).await?;
        let mut changed = false;
        for r in rows
            .iter_mut()
            .filter(|r| r.verdict == Verdict::NotReviewed)
        {
            if let Some(orig) = progress.originals.get(&r.ac_id) {
                *r = orig.clone();
                r.evidence = Some(with_note(orig.evidence.as_deref(), note));
                changed = true;
            }
        }
        if changed {
            self.reviews.save_ac_rows(run_id, &rows).await?;
        }
        Ok(())
    }

    /// Menambahkan catatan ke bukti baris `ac_ids` (verdict tetap).
    pub(crate) async fn note_rows(
        &self,
        run_id: &str,
        ac_ids: &[String],
        note: &str,
    ) -> Result<(), RunError> {
        let mut rows = self.reviews.ac_rows(run_id).await?;
        for r in rows.iter_mut().filter(|r| ac_ids.contains(&r.ac_id)) {
            r.evidence = Some(with_note(r.evidence.as_deref(), note));
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(())
    }

    /// Login semua akun perlengkapan: role yang berhasil, sesinya, dan role
    /// yang gagal beserta alasannya.
    async fn lab_logins(
        &self,
        progress: &LabProgress,
    ) -> (Vec<String>, Vec<BrowserSession>, Vec<(String, String)>) {
        let mut roles = Vec::new();
        let mut sessions = Vec::new();
        let mut unavailable = Vec::new();
        for account in &progress.plan.equipment.accounts {
            let login = self
                .lab
                .login(LabLogin {
                    review_id: &progress.review_id,
                    repo_id: &progress.plan.repo_id,
                    equipment: &progress.plan.equipment,
                    role: &account.role,
                    origins: &progress.plan.origins,
                    proxy: progress.plan.proxy.as_deref(),
                })
                .await;
            match login {
                Ok(state) => {
                    roles.push(account.role.clone());
                    sessions.push(BrowserSession {
                        server: format!("playwright_{}", account.role),
                        state,
                    });
                }
                Err(f) => unavailable.push((
                    account.role.clone(),
                    format!("test login failed: {}", f.reason()),
                )),
            }
        }
        (roles, sessions, unavailable)
    }

    /// Menyalin folder berkas uji repo ke staging sekali per review (KTD-L11).
    async fn stage_lab_fixtures(&self, run: &RunRecord, progress: &mut LabProgress) {
        if progress.fixtures_staged {
            return;
        }
        progress.fixtures_staged = true;
        let Some(dir) = progress.plan.equipment.fixtures_dir.clone() else {
            return;
        };
        let worktree = match self.members(run).await {
            Ok(members) => members
                .into_iter()
                .find(|m| m.repo.id == progress.plan.repo_id)
                .map(|m| m.worktree),
            Err(_) => None,
        };
        let Some(worktree) = worktree.or_else(|| run.worktree.clone()) else {
            return;
        };
        match self
            .lab
            .stage_fixtures(&progress.review_id, &worktree, &dir)
            .await
        {
            Ok(staged) => progress.fixtures = staged,
            Err(e) => tracing::warn!(run_id = %run.id, error = %e, "berkas uji lab tidak disalin"),
        }
    }
}

fn with_note(evidence: Option<&str>, note: &str) -> String {
    match evidence.map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => format!("{e} {note}"),
        None => note.to_owned(),
    }
}
