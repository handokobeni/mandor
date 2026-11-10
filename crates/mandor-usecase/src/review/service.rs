//! Sesi reviewer AC (U9, KTD-29–KTD-31, KTD-38): sesi Claude baru yang hanya
//! membaca, menilai setiap AC lewat tool ledger, lalu kembali ke dev.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use mandor_domain::RunState;
use mandor_domain::ac_ledger::{
    ReviewCounts, Verdict, fill_unreported, reset_for_group_review, reset_for_review,
    restore_decisions,
};
use mandor_domain::app_run::{AppRunState, TestMode};
use mandor_domain::hook_review::origin_of;

use super::capability::CapabilityRole;
use crate::notify::notice;
use crate::ports::{Deps, Ids, PortError};
use crate::run::{RunError, RunService};
use crate::run_ports::{
    AgentPort, AgentRequest, AppRecords, BrowserSession, ChatItem, LabLogin, LabPort, LedgerStore,
    NoticeKind, Notifier, Prompts, Repos, ReviewBrief, ReviewFacts, ReviewHistory, ReviewRecord,
    ReviewRequest, ReviewStatus, ReviewTrigger, RunPhase, RunRecord, RunStore, SessionRef, Stopper,
};

/// Nama server MCP Playwright review biasa.
pub const DEFAULT_BROWSER_SERVER: &str = "playwright";

/// Hasil akhir sesi reviewer yang dibaca dari event agent.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ReviewEnd {
    pub ok: bool,
    pub message: String,
    pub cost_usd: Option<f64>,
    pub usage: mandor_domain::Usage,
    /// Batas yang menghentikan sesi (`budget`, `turns`).
    pub limit: Option<String>,
}

fn rejected(message: &str) -> RunError {
    RunError::Port(PortError::Rejected(message.to_owned()))
}

impl<D: Deps> RunService<D> {
    /// Memulai review AC run. Manual dan Push & PR: run harus menunggu dev di
    /// fase implementasi; Automatic fix dipanggil engine Waiting Fix (KTD-40).
    ///
    /// # Errors
    /// Run tidak ada atau sibuk, tidak ada AC, snapshot gagal, atau agent gagal.
    pub async fn start_review(
        self: &Arc<Self>,
        run_id: &str,
        trigger: ReviewTrigger,
        skill: Option<&str>,
    ) -> Result<ReviewRecord, RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        let skill = skill.or(run.reviewer_skill.as_deref()).map(str::to_owned);
        let skill = skill.as_deref();
        if !self.review_allowed(&run, trigger) {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let mut rows = self.refresh_ledger(run_id).await?.rows;
        if !rows.iter().any(mandor_domain::AcRow::in_gate) {
            return Err(rejected("This task has no acceptance criteria to review"));
        }
        let snapshot = self.code_snapshot(&run).await?;
        // Run grup menyimpan keputusan dev agar pulih bila verdict tetap (R18).
        if self.runs.cards(run_id).await?.is_empty() {
            reset_for_review(&mut rows);
        } else {
            reset_for_group_review(&mut rows);
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;

        self.refresh_app_for_review(&run, &snapshot).await?;
        let review_id = self.sync.ids.nonce();
        let readiness = self.readiness(&run, &snapshot).await?;
        let browser = self.prepare_browser(&review_id, &readiness).await;
        let facts = browser.facts.clone();
        let settings = self.reviews.reviewer_settings().await;
        let task = self.task_of(&run.record_id).await?;
        let gate: Vec<_> = rows.iter().filter(|r| r.in_gate()).cloned().collect();
        let repos: Vec<String> = self
            .members(&run)
            .await?
            .iter()
            .map(|m| m.repo.id.clone())
            .collect();
        let cards = self.review_cards(&run).await?;
        let message = self.prompts.review_message(&ReviewBrief {
            task: &task,
            rows: &gate,
            facts: &facts,
            skill,
            repos: if run.workspace.is_some() { &repos } else { &[] },
            cards: &cards,
        });
        let record = ReviewRecord {
            id: review_id.clone(),
            run_id: run_id.to_owned(),
            trigger,
            status: ReviewStatus::Running,
            browser_used: facts.browser,
            browser_reason: facts.reason.clone(),
            error: None,
            cost_usd: None,
            usage: mandor_domain::Usage::default(),
            stale: false,
            skill: skill.map(str::to_owned),
            started_at_ms: self.now(),
            finished_at_ms: None,
            verdicts: Vec::new(),
        };
        if let Err(e) = self.reviews.save_review(&record).await {
            self.lab.end_review(&review_id).await;
            return Err(e.into());
        }

        let mut req = self
            .agent_request(&mut run, SessionRef::New(self.sync.ids.nonce()), message)
            .await?;
        req.plan_mode = false;
        req.ledger = Some(super::LedgerAccess {
            role: CapabilityRole::Verdict,
            token: self.issue_capability(run_id, CapabilityRole::Verdict),
        });
        let mut review = review_request(&record.id, &facts, &settings);
        review.proxy = browser.proxy;
        review.sessions = browser.sessions;
        req.review = Some(review);
        run.review_snapshot = snapshot;
        run.review_id = Some(record.id.clone());
        run.state = RunState::Reviewing;
        run.hold = None;
        let summary = match skill {
            Some(s) => format!("Review started with {s}."),
            None => "Review started.".to_owned(),
        };
        if let Err(e) = self.spawn_review(&mut run, req, summary).await {
            self.lab.end_review(&review_id).await;
            let mut failed = record;
            failed.status = ReviewStatus::Failed;
            failed.error = Some(e.to_string());
            failed.finished_at_ms = Some(self.now());
            self.reviews.save_review(&failed).await?;
            return Err(e);
        }
        Ok(record)
    }

    /// Review boleh dimulai dari keadaan run ini untuk pemicu ini.
    fn review_allowed(&self, run: &RunRecord, trigger: ReviewTrigger) -> bool {
        match trigger {
            ReviewTrigger::AutomaticFix => run.state == RunState::WfRunning,
            ReviewTrigger::Manual => {
                run.state == RunState::AwaitingDev && run.phase == RunPhase::Implement
            }
            // Di VM kartu `ac_gate` yang AC-nya belum di-review (review putus)
            // di-review lagi lewat keputusan dev di gerbang.
            ReviewTrigger::PushAndPr => {
                run.phase == RunPhase::Implement
                    && (run.state == RunState::AwaitingDev || self.held_at_ac_gate(run))
            }
        }
    }

    /// Versi ter-box dari [`Self::start_review`] untuk pemanggil di dalam akhir
    /// giliran (memutus siklus tipe async driver → review → giliran baru).
    pub(crate) fn start_review_boxed<'a>(
        self: &'a Arc<Self>,
        run_id: &'a str,
        trigger: ReviewTrigger,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ReviewRecord, RunError>> + Send + 'a>,
    > {
        Box::pin(self.start_review(run_id, trigger, None))
    }

    async fn spawn_review(
        self: &Arc<Self>,
        run: &mut RunRecord,
        req: AgentRequest,
        summary: String,
    ) -> Result<(), RunError> {
        self.runs
            .append(&run.id, &ChatItem::System(summary), self.now())
            .await?;
        self.spawn_turn(run, req, None).await
    }

    /// Kesiapan uji browser per item untuk dialog kesiapan (KTD-39).
    ///
    /// # Errors
    /// Run tidak ada, snapshot gagal, atau penyimpanan gagal.
    pub async fn review_readiness(&self, run_id: &str) -> Result<ReviewReadiness, RunError> {
        let run = self.load(run_id).await?;
        let snapshot = self.code_snapshot(&run).await?;
        self.readiness(&run, &snapshot).await
    }

    /// Kartu VM yang tertahan di gerbang AC.
    /// Run memakai sumber desain yang bisa dijalankan (R16 plan 2026-11-03-002).
    pub(crate) async fn expects_prototype(&self, run: &RunRecord) -> bool {
        for root in &run.design {
            if matches!(
                crate::review::DesignSources::prototype(&self.design, &root.id).await,
                Ok(Some(_))
            ) {
                return true;
            }
        }
        false
    }

    /// Akhir review normal: baris di desain yang tampilannya tidak
    /// dibandingkan menjadi Can't verify dengan alasannya (R24, R28). Run
    /// tanpa prototype tidak berubah (R16).
    async fn fill_uncompared_looks(&self, run: &RunRecord, rows: &mut [mandor_domain::AcRow]) {
        use mandor_domain::ac_ledger::Visual;
        if !self.expects_prototype(run).await {
            return;
        }
        let reason = if self.running_prototype(&run.id).await.is_some() {
            "The reviewer did not compare this page with the prototype"
        } else {
            "The prototype was not running at a confirmed revision"
        };
        for r in rows
            .iter_mut()
            .filter(|r| r.compares_look() && r.visual == Visual::NotCompared)
        {
            r.visual = Visual::CantVerify;
            r.visual_detail.description = Some(reason.to_owned());
        }
    }

    fn held_at_ac_gate(&self, run: &RunRecord) -> bool {
        self.is_vm()
            && run.state == RunState::Held
            && run
                .hold
                .as_ref()
                .is_some_and(|h| h.reason == mandor_domain::HoldReason::AcGate)
    }

    async fn readiness(
        &self,
        run: &RunRecord,
        snapshot: &BTreeMap<String, String>,
    ) -> Result<ReviewReadiness, RunError> {
        let members = self.members(run).await?;
        let web: Vec<_> = members
            .iter()
            .filter(|m| m.repo.app.test_mode == TestMode::Web)
            .collect();
        let browser_tests = match &run.workspace {
            Some(ws) => self
                .repos
                .workspace(&ws.id)
                .await?
                .is_none_or(|w| w.browser_tests),
            None => web.iter().all(|m| m.repo.app.browser_tests),
        };
        let owner = self.apps.app_owner().await?;
        let app = self
            .apps
            .app_record(&run.id)
            .await?
            .filter(|_| owner.as_deref() == Some(run.id.as_str()));
        let app_current = app.as_ref().is_some_and(|a| {
            web.iter()
                .all(|m| a.snapshot.get(&m.repo.id) == snapshot.get(&m.repo.id))
        });
        let app_state = match &app {
            Some(a) if a.state == AppRunState::Running && app_current => AppReadiness::Current,
            Some(a) if a.state == AppRunState::Running => AppReadiness::OlderCode,
            _ => AppReadiness::NotRunning,
        };
        let app_urls: Vec<String> = app
            .as_ref()
            .map(|a| {
                a.members
                    .iter()
                    .filter(|m| m.prototype.is_none())
                    .filter_map(|m| m.url.clone())
                    .collect()
            })
            .unwrap_or_default();
        let web_ids: Vec<String> = web.iter().map(|m| m.repo.id.clone()).collect();
        let equipment = self.login_equipment(&web_ids, &app_urls).await?;
        let unattended = self.is_vm();
        let login = if equipment.is_some() {
            LoginKind::Equipment
        } else if unattended && !web.is_empty() && web.iter().all(|m| m.repo.app.unattended_login) {
            LoginKind::RepoSetup
        } else {
            LoginKind::None
        };
        let driver = BrowserDriver {
            installed: self.lab.playwright_installed(),
            unattended,
            login,
        };
        Ok(ReviewReadiness {
            web: !web.is_empty(),
            browser_tests,
            driver,
            app: app_state,
            other_owner: owner.filter(|o| o != &run.id),
            confirmed: Confirmations {
                database: app.as_ref().is_some_and(|a| a.database_confirmed),
                login: app.as_ref().is_some_and(|a| a.login_confirmed),
            },
            prototype_url: app.as_ref().and_then(|a| {
                a.members
                    .iter()
                    .find(|m| m.prototype.is_some() && m.state == AppRunState::Running)
                    .and_then(|m| m.url.clone())
            }),
            app_urls,
            equipment,
        })
    }

    /// Perlengkapan lab yang disetujui untuk login review biasa: repo Web
    /// pertama yang punya perlengkapan disetujui, dan hanya bila semua URL app
    /// loopback (KTD-L6).
    async fn login_equipment(
        &self,
        web_repos: &[String],
        app_urls: &[String],
    ) -> Result<Option<(String, mandor_domain::lab::LabEquipment)>, RunError> {
        if app_urls.is_empty()
            || !app_urls
                .iter()
                .all(|u| mandor_domain::lab::is_loopback_origin(u))
        {
            return Ok(None);
        }
        for repo in web_repos {
            if let Some(eq) = self.lab.approved_equipment(repo).await? {
                return Ok(Some((repo.clone(), eq)));
            }
        }
        Ok(None)
    }

    /// Proxy dan login akun default untuk review biasa (R15, KTD-L4, KTD-L6).
    /// Login gagal membuat browser tidak dipakai dengan alasan yang jelas.
    async fn prepare_browser(
        &self,
        review_id: &str,
        readiness: &ReviewReadiness,
    ) -> PreparedBrowser {
        let mut facts = readiness.facts();
        if !facts.browser {
            return PreparedBrowser {
                facts,
                proxy: None,
                sessions: Vec::new(),
            };
        }
        let origins: Vec<String> = facts
            .app_urls
            .iter()
            .chain(&facts.prototype_url)
            .cloned()
            .collect();
        let mut safe_posts = Vec::new();
        if let Some((_, eq)) = &readiness.equipment {
            safe_posts.extend(eq.safe_posts.iter().cloned());
            safe_posts.push(eq.login.submit_path.clone());
        }
        let policy = mandor_domain::lab::ProxyPolicy::new(&origins, &safe_posts);
        let proxy = match self.lab.start_proxy(review_id, policy).await {
            Ok(url) => url,
            Err(e) => {
                facts.browser = false;
                facts.reason = Some(format!("The review proxy could not start: {e}"));
                facts.app_urls.clear();
                facts.prototype_url = None;
                return PreparedBrowser {
                    facts,
                    proxy: None,
                    sessions: Vec::new(),
                };
            }
        };
        let mut sessions = Vec::new();
        if let Some((repo_id, eq)) = &readiness.equipment {
            let Some(role) = eq
                .accounts
                .iter()
                .find(|a| a.default)
                .map(|a| a.role.clone())
            else {
                return PreparedBrowser {
                    facts,
                    proxy: Some(proxy),
                    sessions,
                };
            };
            let login = self
                .lab
                .login(LabLogin {
                    review_id,
                    repo_id,
                    equipment: eq,
                    role: &role,
                    origins: &origins,
                    proxy: Some(&proxy),
                })
                .await;
            match login {
                Ok(state) => sessions.push(BrowserSession {
                    server: DEFAULT_BROWSER_SERVER.to_owned(),
                    state,
                }),
                Err(f) => {
                    facts.browser = false;
                    facts.reason = Some(format!("Test login failed for {role}: {}", f.reason()));
                    facts.app_urls.clear();
                    facts.prototype_url = None;
                }
            }
        }
        PreparedBrowser {
            facts,
            proxy: Some(proxy),
            sessions,
        }
    }

    /// Restart otomatis (opt-in per repo): app run ini yang menyala dari kode
    /// lama dinyalakan ulang dari `snapshot` sebelum fakta browser dihitung,
    /// dan hasilnya dicatat di chat run.
    async fn refresh_app_for_review(
        &self,
        run: &RunRecord,
        snapshot: &BTreeMap<String, String>,
    ) -> Result<(), RunError> {
        let auto_restart = self
            .members(run)
            .await?
            .iter()
            .filter(|m| m.repo.app.test_mode == TestMode::Web)
            .all(|m| m.repo.app.auto_restart);
        if !self
            .readiness(run, snapshot)
            .await?
            .restarts_app(auto_restart)
        {
            return Ok(());
        }
        let note = self.restart_app_for_review(&run.id, snapshot).await;
        self.runs
            .append(&run.id, &ChatItem::System(note), self.now())
            .await?;
        Ok(())
    }

    /// Akhir giliran reviewer: verdict dilengkapi atau review gagal, riwayat
    /// disimpan, lalu run kembali ke dev dan antrean berjalan.
    pub(crate) async fn finish_review(
        self: &Arc<Self>,
        mut run: RunRecord,
        generation: i64,
        end: ReviewEnd,
    ) -> Result<(), RunError> {
        let review_id = run.review_id.take().unwrap_or_default();
        self.agent.end_review(&review_id).await;
        self.lab.end_review(&review_id).await;
        let mut record = self
            .reviews
            .reviews(&run.id)
            .await?
            .into_iter()
            .find(|r| r.id == review_id);
        let mut rows = self.reviews.ac_rows(&run.id).await?;
        let cancelled = record
            .as_ref()
            .is_some_and(|r| r.status == ReviewStatus::Cancelled);
        let status = if cancelled {
            ReviewStatus::Cancelled
        } else if end.ok {
            fill_unreported(&mut rows);
            self.fill_uncompared_looks(&run, &mut rows).await;
            restore_decisions(&mut rows);
            self.reviews.save_ac_rows(&run.id, &rows).await?;
            ReviewStatus::Finished
        } else {
            ReviewStatus::Failed
        };
        let stale = match self.code_snapshot(&run).await {
            Ok(now) => now != run.review_snapshot,
            Err(_) => true,
        };
        if let Some(r) = &mut record {
            r.status = status;
            r.error = match (status, end.limit.as_deref()) {
                (ReviewStatus::Failed, Some("budget")) => {
                    Some("Review stopped at the cost limit".to_owned())
                }
                (ReviewStatus::Failed, _) => Some(end.message.clone()),
                _ => r.error.clone(),
            };
            r.cost_usd = end.cost_usd;
            r.usage = end.usage;
            r.stale = stale;
            r.finished_at_ms = Some(self.now());
            r.verdicts = rows.iter().filter(|x| x.in_gate()).cloned().collect();
            self.reviews.save_review(r).await?;
        }
        let counts = ReviewCounts::of(&rows);
        let summary = match status {
            ReviewStatus::Finished => format!("Review finished: {}.", counts.summary()),
            ReviewStatus::Cancelled => "Review cancelled.".to_owned(),
            _ => format!(
                "Review failed: {}",
                record
                    .as_ref()
                    .and_then(|r| r.error.clone())
                    .unwrap_or_else(|| end.message.clone())
            ),
        };
        self.runs
            .append(&run.id, &ChatItem::System(summary.clone()), self.now())
            .await?;
        if record
            .as_ref()
            .is_some_and(|r| r.trigger == ReviewTrigger::AutomaticFix)
        {
            let failure = record.as_ref().and_then(|r| r.error.clone());
            return self
                .after_fix_review(run, status == ReviewStatus::Finished, failure)
                .await;
        }
        run.state = RunState::AwaitingDev;
        if self.is_vm()
            && run.queue.is_empty()
            && record
                .as_ref()
                .is_some_and(|r| r.trigger == ReviewTrigger::PushAndPr)
        {
            self.runs.save(&run, generation, self.now()).await?;
            let finished = status == ReviewStatus::Finished;
            return self.vm_after_review(run, finished, &summary).await;
        }
        if !run.queue.is_empty() {
            let next = run.queue.remove(0);
            let req = self.resume_request(&mut run, next.clone()).await?;
            return self.spawn_turn_boxed(run, req, next).await;
        }
        self.runs.save(&run, generation, self.now()).await?;
        let kind = if status == ReviewStatus::Finished {
            NoticeKind::ReviewFinished
        } else {
            NoticeKind::Failed
        };
        self.notifier
            .notify(notice(self, kind, &run, &summary).await)
            .await;
        Ok(())
    }

    /// Menghentikan review yang berjalan; AC yang belum dilaporkan tetap Not
    /// reviewed.
    ///
    /// # Errors
    /// Run tidak ada atau tidak sedang direview.
    pub async fn cancel_review(&self, run_id: &str) -> Result<(), RunError> {
        let run = self.load(run_id).await?;
        if run.state != RunState::Reviewing {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        if let Some(mut r) = self
            .reviews
            .reviews(run_id)
            .await?
            .into_iter()
            .find(|r| Some(&r.id) == run.review_id.as_ref())
        {
            r.status = ReviewStatus::Cancelled;
            self.reviews.save_review(&r).await?;
        }
        if let Some(stopper) = self.take_stopper(run_id) {
            stopper.stop().await;
        }
        Ok(())
    }

    /// Menyimpan skill reviewer pilihan dev saat Start (R14).
    ///
    /// # Errors
    /// Run tidak ada atau penyimpanan gagal.
    pub async fn set_reviewer_skill(
        &self,
        run_id: &str,
        skill: Option<&str>,
    ) -> Result<(), RunError> {
        let mut run = self.load(run_id).await?;
        run.reviewer_skill = skill.map(str::to_owned);
        self.runs.save(&run, run.generation, self.now()).await?;
        Ok(())
    }

    /// Catatan sistem di chat run (mis. skill reviewer yang tidak terpasang).
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn system_note(&self, run_id: &str, text: &str) -> Result<(), RunError> {
        Ok(self
            .runs
            .append(run_id, &ChatItem::System(text.to_owned()), self.now())
            .await?)
    }

    /// Setelan reviewer saat ini (dibaca setiap review dimulai).
    pub async fn reviewer_settings(&self) -> crate::run_ports::ReviewerSettings {
        self.reviews.reviewer_settings().await
    }

    /// Menyimpan setelan reviewer; berlaku untuk review berikutnya.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn set_reviewer_settings(
        &self,
        s: &crate::run_ports::ReviewerSettings,
    ) -> Result<(), RunError> {
        Ok(self.reviews.set_reviewer_settings(s).await?)
    }

    /// Riwayat review run, terbaru dulu.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn review_history(&self, run_id: &str) -> Result<Vec<ReviewRecord>, RunError> {
        Ok(self.reviews.reviews(run_id).await?)
    }

    /// Mengirim temuan Fail ke agent pelaksana (R17): giliran Implement di sesi
    /// yang sama, dan baris yang dikirim menjadi Fix in progress.
    ///
    /// # Errors
    /// Run tidak menunggu dev, baris bukan Fail, atau agent gagal.
    pub async fn send_findings(
        self: &Arc<Self>,
        run_id: &str,
        ac_ids: &[String],
    ) -> Result<(), RunError> {
        let mut run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        if ac_ids.is_empty() {
            return Err(rejected("Pick at least one failed criterion"));
        }
        let mut rows = self.reviews.ac_rows(run_id).await?;
        for id in ac_ids {
            // Perilaku gagal, atau tampilan berbeda dari prototype (R23 plan
            // 2026-11-03-002).
            let ok = rows.iter().any(|r| &r.ac_id == id && r.selectable());
            if !ok {
                return Err(rejected("Only failed criteria can be sent to the agent"));
            }
        }
        let picked: Vec<_> = rows
            .iter()
            .filter(|r| ac_ids.contains(&r.ac_id))
            .cloned()
            .collect();
        let message = self.prompts.findings_message(&picked);
        let req = self.resume_request(&mut run, message).await?;
        self.spawn_turn(
            &mut run,
            req,
            Some(format!(
                "Sent {} review finding(s) to the agent",
                picked.len()
            )),
        )
        .await?;
        for r in rows.iter_mut().filter(|r| ac_ids.contains(&r.ac_id)) {
            r.verdict = Verdict::FixInProgress;
            if r.visual == mandor_domain::ac_ledger::Visual::Differs {
                r.visual = mandor_domain::ac_ledger::Visual::NotCompared;
            }
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(())
    }
}

fn review_request(
    review_id: &str,
    facts: &ReviewFacts,
    settings: &crate::run_ports::ReviewerSettings,
) -> ReviewRequest {
    // Browser reviewer hanya boleh membuka app run dan prototype (R19 plan
    // 2026-11-03-002).
    let origins = facts
        .app_urls
        .iter()
        .chain(&facts.prototype_url)
        .filter_map(|u| origin_of(u))
        .collect();
    ReviewRequest {
        review_id: review_id.to_owned(),
        origins,
        browser: facts.browser,
        model: settings.model.clone(),
        budget_cents: settings.budget_cents.max(1),
        timeout: Duration::from_secs(settings.timeout_secs.max(60)),
        proxy: None,
        sessions: Vec::new(),
        lab: None,
    }
}

/// Browser review yang sudah disiapkan: fakta akhir, proxy, dan sesi login.
#[derive(Debug, Clone)]
struct PreparedBrowser {
    facts: ReviewFacts,
    proxy: Option<String>,
    sessions: Vec<BrowserSession>,
}

/// App run untuk uji browser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AppReadiness {
    #[default]
    NotRunning,
    /// Menyala dari kode yang lebih lama dari kode saat ini.
    OlderCode,
    Current,
}

impl AppReadiness {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRunning => "not_running",
            Self::OlderCode => "older_code",
            Self::Current => "current",
        }
    }
}

/// Konfirmasi dev yang disimpan di state app (KTD-36).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Confirmations {
    pub database: bool,
    pub login: bool,
}

/// Kesiapan uji browser per item (dialog kesiapan, KTD-39).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewReadiness {
    /// Run punya repo dengan cara uji Web.
    pub web: bool,
    pub browser_tests: bool,
    pub driver: BrowserDriver,
    pub app: AppReadiness,
    /// Run lain yang app-nya sedang menyala.
    pub other_owner: Option<String>,
    pub confirmed: Confirmations,
    pub app_urls: Vec<String>,
    /// URL prototype yang menyala untuk run ini (plan 2026-11-03-002).
    pub prototype_url: Option<String>,
    /// Perlengkapan lab disetujui untuk login review biasa: `(repo, perlengkapan)`.
    pub equipment: Option<(String, mandor_domain::lab::LabEquipment)>,
}

/// Sumber login browser review (R15, KTD-L1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoginKind {
    /// Akun default dari perlengkapan lab yang disetujui.
    Equipment,
    /// VM: setup command repo menyiapkan login sendiri (KTD-64 plan VM).
    RepoSetup,
    #[default]
    None,
}

/// Browser reviewer: Playwright headless di laptop dan VM (KTD-L1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BrowserDriver {
    /// Biner Playwright MCP tersedia.
    pub installed: bool,
    /// Daemon VM tanpa pengawas: app boleh dinyalakan dan diambil alih.
    pub unattended: bool,
    pub login: LoginKind,
}

impl ReviewReadiness {
    /// Dialog kesiapan hanya untuk run dengan repo Web dan uji browser aktif.
    #[must_use]
    pub fn needs_dialog(&self) -> bool {
        self.web && self.browser_tests
    }

    /// App run ini perlu dinyalakan ulang otomatis sebelum review: restart
    /// aktif di semua repo Web (`auto_restart`), app run ini menyala dari kode lama, tidak ada app run lain yang
    /// menyala, login pernah dikonfirmasi, dan ekstensi tersambung, sehingga
    /// restart membuat review bisa memakai browser. Di VM tidak ada dev yang
    /// menyalakan app, jadi app yang belum menyala juga dinyalakan (R11), dan
    /// app kartu sebelumnya dihentikan karena kartu lembur dikerjakan satu per
    /// satu.
    #[must_use]
    pub fn restarts_app(&self, auto_restart: bool) -> bool {
        let headless = self.driver.unattended;
        let stale = match self.app {
            AppReadiness::OlderCode => true,
            AppReadiness::NotRunning => headless,
            AppReadiness::Current => false,
        };
        self.needs_dialog()
            && auto_restart
            && stale
            && (self.other_owner.is_none() || headless)
            && self.login_ready()
            && self.driver_ready()
    }

    /// Playwright MCP terpasang.
    fn driver_ready(&self) -> bool {
        self.driver.installed
    }

    /// Ada sumber login: perlengkapan lab disetujui, atau setup command repo
    /// di VM.
    fn login_ready(&self) -> bool {
        self.driver.login != LoginKind::None
    }

    /// Fakta untuk prompt reviewer: browser hanya bila semua syarat terpenuhi.
    #[must_use]
    pub fn facts(&self) -> ReviewFacts {
        let reason = if !self.web {
            Some("No app for this run")
        } else if !self.browser_tests {
            Some("Browser tests are off")
        } else if self.app == AppReadiness::NotRunning {
            Some("The app is not running")
        } else if self.app == AppReadiness::OlderCode {
            Some("app running older code")
        } else if !self.driver_ready() {
            Some("Playwright is not installed")
        } else if !self.login_ready() {
            Some("No test login is prepared for unattended review")
        } else {
            None
        };
        ReviewFacts {
            browser: reason.is_none(),
            reason: reason.map(str::to_owned),
            app_urls: if reason.is_none() {
                self.app_urls.clone()
            } else {
                Vec::new()
            },
            prototype_url: self.prototype_url.clone().filter(|_| reason.is_none()),
        }
    }
}
