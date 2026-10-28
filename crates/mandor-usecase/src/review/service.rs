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
    AgentPort, AgentRequest, AppRecords, ChatItem, LedgerStore, NoticeKind, Notifier, Prompts,
    Repos, ReviewBrief, ReviewFacts, ReviewHistory, ReviewRecord, ReviewRequest, ReviewStatus,
    ReviewTrigger, RunPhase, RunRecord, RunStore, SessionRef, Stopper,
};

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
        let allowed = match trigger {
            ReviewTrigger::AutomaticFix => run.state == RunState::WfRunning,
            ReviewTrigger::Manual | ReviewTrigger::PushAndPr => {
                run.state == RunState::AwaitingDev && run.phase == RunPhase::Implement
            }
        };
        if !allowed {
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
        let facts = self.review_facts(&run, &snapshot).await?;
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
            id: self.sync.ids.nonce(),
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
        self.reviews.save_review(&record).await?;

        let mut req = self
            .agent_request(&mut run, SessionRef::New(self.sync.ids.nonce()), message)
            .await?;
        req.plan_mode = false;
        req.ledger = Some(super::LedgerAccess {
            role: CapabilityRole::Verdict,
            token: self.issue_capability(run_id, CapabilityRole::Verdict),
        });
        req.review = Some(review_request(&record.id, &facts, &settings));
        run.review_snapshot = snapshot;
        run.review_id = Some(record.id.clone());
        run.state = RunState::Reviewing;
        let summary = match skill {
            Some(s) => format!("Review started with {s}."),
            None => "Review started.".to_owned(),
        };
        if let Err(e) = self.spawn_review(&mut run, req, summary).await {
            let mut failed = record;
            failed.status = ReviewStatus::Failed;
            failed.error = Some(e.to_string());
            failed.finished_at_ms = Some(self.now());
            self.reviews.save_review(&failed).await?;
            return Err(e);
        }
        Ok(record)
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
        let driver = if self.is_vm() {
            BrowserDriver::Headless {
                unattended_login: !web.is_empty()
                    && web.iter().all(|m| m.repo.app.unattended_login),
            }
        } else {
            BrowserDriver::Chrome {
                connected: self.reviews.chrome_connected().await,
            }
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
            app_urls: app
                .map(|a| a.members.into_iter().filter_map(|m| m.url).collect())
                .unwrap_or_default(),
        })
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

    /// Fakta kesiapan uji browser: repo Web dengan uji browser aktif, app run
    /// ini menyala dari snapshot yang sama, login dikonfirmasi, dan ekstensi
    /// tersambung (R13, KTD-38).
    async fn review_facts(
        &self,
        run: &RunRecord,
        snapshot: &BTreeMap<String, String>,
    ) -> Result<ReviewFacts, RunError> {
        Ok(self.readiness(run, snapshot).await?.facts())
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
            let ok = rows
                .iter()
                .any(|r| &r.ac_id == id && r.selectable() && r.verdict == Verdict::Fail);
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
    let origins = facts.app_urls.iter().filter_map(|u| origin_of(u)).collect();
    ReviewRequest {
        review_id: review_id.to_owned(),
        origins,
        browser: facts.browser,
        model: settings.model.clone(),
        budget_cents: settings.budget_cents.max(1),
        timeout: Duration::from_secs(settings.timeout_secs.max(60)),
    }
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
}

/// Browser reviewer menurut peran daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserDriver {
    /// Laptop: ekstensi Claude in Chrome, login dikonfirmasi dev.
    Chrome { connected: bool },
    /// VM: Playwright headless; semua repo Web menyiapkan login uji sendiri
    /// lewat setup command (KTD-64 plan VM).
    Headless { unattended_login: bool },
}

impl Default for BrowserDriver {
    fn default() -> Self {
        Self::Chrome { connected: false }
    }
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
    /// menyalakan app, jadi app yang belum menyala juga dinyalakan (R11).
    #[must_use]
    pub fn restarts_app(&self, auto_restart: bool) -> bool {
        let stale = match self.app {
            AppReadiness::OlderCode => true,
            AppReadiness::NotRunning => matches!(self.driver, BrowserDriver::Headless { .. }),
            AppReadiness::Current => false,
        };
        self.needs_dialog()
            && auto_restart
            && stale
            && self.other_owner.is_none()
            && self.login_ready()
            && self.driver_ready()
    }

    /// Ekstensi Chrome tersambung; Playwright headless selalu tersedia.
    fn driver_ready(&self) -> bool {
        match self.driver {
            BrowserDriver::Chrome { connected } => connected,
            BrowserDriver::Headless { .. } => true,
        }
    }

    /// Login app siap: dikonfirmasi dev di laptop, disiapkan setup command
    /// repo di VM.
    fn login_ready(&self) -> bool {
        match self.driver {
            BrowserDriver::Chrome { .. } => self.confirmed.login,
            BrowserDriver::Headless { unattended_login } => unattended_login,
        }
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
        } else if self.driver
            == (BrowserDriver::Headless {
                unattended_login: false,
            })
        {
            Some("No test login is prepared for unattended review")
        } else if !self.login_ready() {
            Some("Login not confirmed")
        } else if !self.driver_ready() {
            Some("Claude in Chrome extension not connected")
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
        }
    }
}
