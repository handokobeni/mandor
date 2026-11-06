//! Giliran agent: spawn, driver event, antrean pesan, kirim sekarang, stop.
//!
//! Setiap spawn menaikkan `generation`. Driver hanya boleh mengubah run bila
//! generasinya masih yang tersimpan, sehingga event giliran lama tidak menimpa
//! giliran baru (perbaikan bug race stop→send).

use crate::run_ports::ApprovalStore as _;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use mandor_domain::ac_ledger::design_gaps_text;
use mandor_domain::{AcRow, AgentEvent, AppField, Design, RunState, WriteOp};

use super::members::session_of;
use super::{RunError, RunService};
use crate::notify::notice;
use crate::ports::{Deps, PortError};
use crate::review::service::ReviewEnd;
use crate::run_ports::{
    AgentPort, AgentRequest, AgentRunHandle, ChatItem, LedgerStore, NoticeKind, Notifier, Prompts,
    RunPhase, RunRecord, RunStore, Spawner, Stopper,
};

/// Hasil akhir satu giliran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TurnEnd {
    /// Selesai normal dengan pesan akhir agent.
    Ok(String),
    Error(String),
}

/// Hasil pengiriman pesan dev.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    Queued,
}

impl<D: Deps> RunService<D> {
    /// Menjalankan satu giliran agent untuk `run` dan menyerahkan event-nya ke
    /// driver latar. `run` diperbarui (generasi, pid, state) dan disimpan.
    pub(crate) async fn spawn_turn(
        self: &Arc<Self>,
        run: &mut RunRecord,
        req: AgentRequest,
        dev_message: Option<String>,
    ) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        let expected = run.generation;
        let mut req = req;
        // Giliran implementasi dan plan di laptop meminta izin lewat kartu;
        // reviewer dan semua giliran VM tidak (R12, R13).
        if req.review.is_none() && !self.is_vm() {
            if let Some(permit) = self.agent.permit_ref(&req) {
                let turn = if req.plan_mode {
                    mandor_domain::permit::TurnKind::Plan
                } else {
                    mandor_domain::permit::TurnKind::Implement
                };
                req.permit =
                    Some(self.issue_permit(&run.id, crate::review::PermitGrant { turn, permit }));
            }
        }
        let handle = self.agent.spawn(&req).await?;
        run.generation += 1;
        run.state = if req.review.is_some() {
            RunState::Reviewing
        } else if run.automatic {
            RunState::WfRunning
        } else {
            RunState::Running
        };
        run.session_started = true;
        run.last_error = None;
        run.pid = handle.pid().map(i64::from);
        if let Err(e) = self.runs.save(run, expected, self.now()).await {
            handle.stopper().stop().await;
            return Err(e.into());
        }
        if let Some(m) = dev_message {
            self.runs
                .append(&run.id, &ChatItem::Dev(m), self.now())
                .await?;
        }
        self.set_stopper(&run.id, run.generation, handle.stopper());
        let this = Arc::clone(self);
        let run_id = run.id.clone();
        let generation = run.generation;
        self.spawner
            .spawn(async move { this.drive(run_id, generation, handle).await });
        Ok(())
    }

    /// Versi ter-box dari [`Self::spawn_turn`] untuk memutus siklus tipe async
    /// (driver → akhir giliran → giliran berikutnya).
    pub(crate) fn spawn_turn_boxed(
        self: &Arc<Self>,
        mut run: RunRecord,
        req: AgentRequest,
        message: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), RunError>> + Send + '_>> {
        Box::pin(async move { self.spawn_turn(&mut run, req, Some(message)).await })
    }

    async fn drive(
        self: Arc<Self>,
        run_id: String,
        generation: i64,
        mut handle: <D::Agent as AgentPort>::Run,
    ) {
        let mut end = TurnEnd::Error("agent stopped without a result".to_owned());
        let mut stats = ReviewEnd::default();
        let mut tracker = mandor_domain::UsageTracker::default();
        let mut limited = false;
        // Penolakan yang sudah tampil di Chat lewat event stream (KTD-A17).
        let mut shown = std::collections::HashSet::new();
        let mut denied = Vec::new();
        while let Some(event) = handle.next_event().await {
            observe(&event, &mut end, &mut stats);
            // Batas siklus VM dihitung per pesan agar bisa berhenti di tengah
            // giliran (KTD-59 plan VM).
            if self.is_vm() && !limited && self.vm_count_usage(&event, &mut tracker).await {
                limited = true;
                handle.stopper().stop().await;
            }
            let denial_id = shown_denial(&event);
            if let AgentEvent::Done { denied: d, .. } = &event {
                denied.clone_from(d);
            }
            if self.record_event(&run_id, generation, event).await {
                shown.extend(denial_id);
            }
        }
        self.record_denials(&run_id, generation, denied, &shown)
            .await;
        let finished = if limited {
            self.hold_for_limit(&run_id, generation).await
        } else {
            self.finish_turn(&run_id, generation, end, stats).await
        };
        if let Err(e) = finished {
            tracing::warn!(run_id, error = %e, "gagal menyelesaikan giliran agent");
        }
    }

    /// Menyimpan event agent ke chat run. Pemakaian per pesan dan sesi reviewer
    /// tidak masuk chat pelaksana; progres review lewat buku AC.
    /// `true` bila event tersimpan di chat.
    async fn record_event(&self, run_id: &str, generation: i64, event: AgentEvent) -> bool {
        if matches!(event, AgentEvent::MessageUsage { .. }) {
            return false;
        }
        match self.current_state(run_id, generation).await {
            Some(current) if current != RunState::Reviewing => {}
            _ => return false,
        }
        if let Err(e) = self
            .runs
            .append(run_id, &ChatItem::Agent(event), self.now())
            .await
        {
            tracing::warn!(run_id, error = %e, "gagal menyimpan event agent");
            return false;
        }
        true
    }

    /// Penolakan Claude Code yang belum tampil (bukan kartu, bukan event
    /// penolakan, bukan hasil hook): satu item Chat di laptop, atau catatan
    /// laporan pagi di VM (KTD-A17, R13, R22).
    async fn record_denials(
        &self,
        run_id: &str,
        generation: i64,
        denied: Vec<mandor_domain::Denial>,
        shown: &std::collections::HashSet<String>,
    ) {
        let handled = self
            .permit_handled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(run_id)
            .unwrap_or_default();
        if denied.is_empty() {
            return;
        }
        let carded: std::collections::HashSet<String> = self
            .approvals
            .approvals(run_id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.tool_use_id)
            .collect();
        let fresh: Vec<String> = denied
            .into_iter()
            .filter(|d| {
                !shown.contains(&d.tool_use_id)
                    && !carded.contains(&d.tool_use_id)
                    && !handled.contains(&d.tool_use_id)
            })
            .map(|d| format!("{} `{}`", d.tool_name, d.input))
            .collect();
        if fresh.is_empty() {
            return;
        }
        if self.is_vm() {
            self.note_vm_denials(run_id, generation, fresh).await;
            return;
        }
        for text in fresh {
            let item =
                ChatItem::System(format!("Denied by Claude Code's permission rules: {text}"));
            if let Err(e) = self.runs.append(run_id, &item, self.now()).await {
                tracing::warn!(run_id, error = %e, "penolakan tidak tercatat");
            }
        }
    }

    /// Penolakan run VM untuk laporan pagi, dibatasi agar run tidak membengkak.
    async fn note_vm_denials(&self, run_id: &str, generation: i64, fresh: Vec<String>) {
        const KEPT: usize = 50;
        let Ok(Some(mut run)) = self.runs.get(run_id).await else {
            return;
        };
        if run.generation != generation {
            return;
        }
        run.denials.extend(fresh);
        let over = run.denials.len().saturating_sub(KEPT);
        run.denials.drain(..over);
        let expected = run.generation;
        if let Err(e) = self.runs.save(&run, expected, self.now()).await {
            tracing::warn!(run_id, error = %e, "penolakan VM tidak tercatat");
        }
    }

    /// State run bila generasi giliran ini masih yang berlaku.
    async fn current_state(&self, run_id: &str, generation: i64) -> Option<RunState> {
        match self.runs.get(run_id).await {
            Ok(Some(r)) if r.generation == generation => Some(r.state),
            _ => None,
        }
    }

    async fn finish_turn(
        self: &Arc<Self>,
        run_id: &str,
        generation: i64,
        end: TurnEnd,
        stats: ReviewEnd,
    ) -> Result<(), RunError> {
        self.clear_stopper(run_id, generation);
        self.revoke_capabilities(run_id);
        // Giliran selesai atau dihentikan: kartu yang menunggu tidak punya
        // agent lagi (R20).
        self.cancel_run_approvals(run_id).await;
        let mut run = self.load(run_id).await?;
        if run.generation != generation {
            return Ok(());
        }
        run.pid = None;
        self.mark_stale_if_changed(&run).await;
        if run.state == RunState::Reviewing {
            let (ok, message) = match end {
                TurnEnd::Ok(m) => (true, m),
                TurnEnd::Error(m) => (false, m),
            };
            let end = ReviewEnd {
                ok,
                message,
                ..stats
            };
            return self.finish_review(run, generation, end).await;
        }
        if run.state == RunState::WfRunning {
            return self.finish_fix_turn(run, end).await;
        }
        match end {
            TurnEnd::Ok(result) => {
                if run.phase == RunPhase::Plan && !result.trim().is_empty() {
                    run.plan = Some(result.clone());
                }
                if !run.queue.is_empty() {
                    let next = run.queue.remove(0);
                    let req = self.resume_request(&mut run, next.clone()).await?;
                    return self.spawn_turn_boxed(run, req, next).await;
                }
                // VM: kartu (atau grup, U12 plan grup) dikerjakan sampai PR atau
                // tertahan dengan alasan (U4 plan VM).
                if self.is_vm() && run.phase == RunPhase::Implement {
                    return self.vm_after_turn(run, result).await;
                }
                run.state = RunState::AwaitingDev;
                self.runs.save(&run, generation, self.now()).await?;
                let body = match run.phase {
                    RunPhase::Plan => "Plan ready: approve it to start coding, or reply to revise",
                    RunPhase::Brainstorm => {
                        "The agent has a question or a summary: answer in chat, or continue to plan"
                    }
                    RunPhase::Implement => "Agent finished and is waiting for you",
                };
                self.notifier
                    .notify(notice(self, NoticeKind::AwaitingAnswer, &run, body).await)
                    .await;
            }
            TurnEnd::Error(msg) => {
                run.state = RunState::Failed;
                run.last_error = Some(msg.clone());
                self.runs.save(&run, generation, self.now()).await?;
                self.notifier
                    .notify(notice(self, NoticeKind::Failed, &run, &msg).await)
                    .await;
            }
        }
        Ok(())
    }

    /// Permintaan giliran lanjutan pada sesi yang sama; worktree yang hilang
    /// dibuat ulang dan disimpan di `run`. Pesan `/skill argumen` menjadi
    /// instruksi menjalankan skill itu.
    pub(crate) async fn resume_request(
        &self,
        run: &mut RunRecord,
        message: String,
    ) -> Result<AgentRequest, RunError> {
        let message = match mandor_domain::run::slash_command(&message) {
            Some((name, args)) => self.prompts.command_message(name, args),
            None => message,
        };
        self.agent_request(run, session_of(run), message).await
    }

    /// "Approve plan & implement": agent mengerjakan plan terakhir di sesi yang sama.
    /// `follow_design`: AC berselisih desain yang mengikuti desain (R8a); daftar
    /// ini berlaku saat approve, jadi undo cukup dengan mengirim ulang daftar.
    /// Setelah agent berjalan, selisih desain ditulis ke tracker (KTD-34).
    ///
    /// # Errors
    /// Run tidak ada, agent masih bekerja, belum ada plan, AC follow design
    /// tidak valid, atau agent gagal.
    pub async fn approve_plan(
        self: &Arc<Self>,
        run_id: &str,
        follow_design: &[String],
    ) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        if run.phase != RunPhase::Plan || run.plan.is_none() {
            return Err(RunError::InvalidState("no plan to approve"));
        }
        let rows = follow_design_rows(self.reviews.ac_rows(run_id).await?, follow_design)?;
        let followed: Vec<String> = rows
            .iter()
            .filter(|r| r.design == Design::FollowDesign)
            .map(|r| format!("{}: {}", r.display_label(), r.text))
            .collect();
        run.phase = RunPhase::Implement;
        let mut message = if run.session_started {
            self.prompts.approve_message(&followed)
        } else {
            // Run hasil Take over: sesi baru belum pernah melihat plan-nya.
            self.reviews.save_ac_rows(run_id, &rows).await?;
            let plan = run.plan.clone().unwrap_or_default();
            self.approved_plan_turn(&run, &plan).await?
        };
        if run.one_plan.is_some() {
            // Plan grup (R25): cuplikan AC saat disetujui, lalu kartu aktif saja.
            run.one_plan = Some(self.one_plan_snapshot(run_id).await?);
            if let Some(key) = self.active_key(run_id).await? {
                message.push_str("\n\n");
                message.push_str(&super::one_plan::scope_note(&key));
            }
        }
        let req = self.resume_request(&mut run, message).await?;
        self.spawn_turn(
            &mut run,
            req,
            Some("Plan approved; implement it".to_owned()),
        )
        .await?;
        if run.design.is_empty() {
            return Ok(());
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;
        // Run grup: selisih desain milik kartu yang sedang dikerjakan.
        let active = self
            .runs
            .cards(run_id)
            .await?
            .into_iter()
            .find(|c| c.state == mandor_domain::group::CardState::Active)
            .map(|c| c.record_id);
        let (target, card_rows): (String, Vec<_>) = match &active {
            Some(card) => (
                card.clone(),
                rows.iter()
                    .filter(|r| r.card.as_deref() == Some(card.as_str()))
                    .cloned()
                    .collect(),
            ),
            None => (run.record_id.clone(), rows),
        };
        let gaps = design_gaps_text(&card_rows, DESIGN_GAPS_LIMIT);
        let op = WriteOp::Fields(vec![AppField::DesignGaps(gaps)]);
        self.sync.request_write(&target, &op).await?;
        Ok(())
    }

    /// "Continue to plan": dari brainstorm ke plan di sesi yang sama, memakai
    /// skill plan yang dipilih saat Start.
    ///
    /// # Errors
    /// Run tidak ada, agent masih bekerja, bukan fase brainstorm, atau agent gagal.
    pub async fn continue_to_plan(self: &Arc<Self>, run_id: &str) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        if run.phase != RunPhase::Brainstorm {
            return Err(RunError::InvalidState("not brainstorming"));
        }
        run.phase = RunPhase::Plan;
        let message = self.with_design(
            &run,
            self.prompts
                .continue_plan_message(run.plan_skill.as_deref()),
        );
        let req = self.resume_request(&mut run, message).await?;
        self.spawn_turn(&mut run, req, Some("Continue to plan".to_owned()))
            .await
    }

    /// Pesan dev: diantrekan bila agent sibuk, selain itu dikirim sebagai giliran baru.
    ///
    /// # Errors
    /// Run tidak ada, atau agent/penyimpanan gagal.
    pub async fn send_message(
        self: &Arc<Self>,
        run_id: &str,
        text: &str,
    ) -> Result<SendOutcome, RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        if run.state.is_busy() {
            run.queue.push(text.to_owned());
            self.runs.save(&run, run.generation, self.now()).await?;
            return Ok(SendOutcome::Queued);
        }
        if run.state.is_parked() || matches!(run.state, RunState::Preparing | RunState::Publishing)
        {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let req = self.resume_request(&mut run, text.to_owned()).await?;
        self.spawn_turn(&mut run, req, Some(text.to_owned()))
            .await?;
        Ok(SendOutcome::Sent)
    }

    /// "Kirim sekarang": menghentikan giliran berjalan secara aman lalu mengirim.
    ///
    /// # Errors
    /// Run tidak ada, atau agent/penyimpanan gagal.
    pub async fn send_now(
        self: &Arc<Self>,
        run_id: &str,
        text: &str,
    ) -> Result<SendOutcome, RunError> {
        let _work = self.enter_work()?;
        self.interrupt(run_id).await?;
        self.send_message(run_id, text).await
    }

    /// Menghentikan giliran berjalan; run menunggu dev.
    ///
    /// # Errors
    /// Run tidak ada atau penyimpanan gagal.
    pub async fn stop(self: &Arc<Self>, run_id: &str) -> Result<(), RunError> {
        if self.interrupt(run_id).await? {
            self.runs
                .append(
                    run_id,
                    &ChatItem::System("Turn stopped by the developer".to_owned()),
                    self.now(),
                )
                .await?;
        }
        Ok(())
    }

    /// Menaikkan generasi lebih dulu (event giliran lama jadi basi), lalu
    /// menghentikan prosesnya. Mengembalikan `true` bila ada giliran yang dihentikan.
    pub(crate) async fn interrupt(&self, run_id: &str) -> Result<bool, RunError> {
        let mut run = self.load(run_id).await?;
        if !run.state.is_busy() {
            return Ok(false);
        }
        let expected = run.generation;
        run.generation += 1;
        run.state = if run.automatic {
            RunState::InReviewLoop
        } else {
            RunState::AwaitingDev
        };
        run.pid = None;
        self.runs.save(&run, expected, self.now()).await?;
        if let Some(stopper) = self.take_stopper(run_id) {
            stopper.stop().await;
        }
        Ok(true)
    }
}

/// Batas panjang field `mandor_design_gaps` (KTD-34).
const DESIGN_GAPS_LIMIT: usize = 4_000;

/// Buku AC dengan baris follow design diterapkan. Hanya baris Missing, Partly,
/// atau Conflicts milik run yang boleh mengikuti desain.
pub(super) fn follow_design_rows(
    mut rows: Vec<AcRow>,
    ids: &[String],
) -> Result<Vec<AcRow>, RunError> {
    for id in ids {
        let row = rows
            .iter_mut()
            .find(|r| &r.ac_id == id)
            .ok_or_else(|| rejected(format!("unknown acceptance criterion: {id}")))?;
        if !row.design.is_gap() {
            return Err(rejected(format!(
                "{} is not missing from or in conflict with the design",
                row.display_label()
            )));
        }
        row.design = Design::FollowDesign;
    }
    Ok(rows)
}

pub(super) fn rejected(message: String) -> RunError {
    RunError::Port(PortError::Rejected(message))
}

/// `tool_use_id` penolakan yang tampil di Chat lewat event ini: event
/// penolakan, atau hasil tool dari hook Mandor.
fn shown_denial(event: &AgentEvent) -> Option<String> {
    match event {
        AgentEvent::PermissionDenied { tool_use_id, .. } => Some(tool_use_id.clone()),
        AgentEvent::ToolResult {
            tool_use_id,
            is_error: true,
            preview,
        } if preview.starts_with("PreToolUse:") && preview.contains("mandor:") => {
            Some(tool_use_id.clone())
        }
        _ => None,
    }
}

/// Mencatat hasil giliran dan pemakaian dari satu event agent.
fn observe(event: &AgentEvent, end: &mut TurnEnd, stats: &mut ReviewEnd) {
    match event {
        AgentEvent::Done {
            is_error,
            result,
            cost_usd,
            limit,
            ..
        } => {
            *end = if *is_error {
                TurnEnd::Error(result.clone())
            } else {
                TurnEnd::Ok(result.clone())
            };
            stats.cost_usd = *cost_usd;
            stats.limit.clone_from(limit);
        }
        AgentEvent::Usage(u) => stats.usage = *u,
        AgentEvent::Failed { message } => *end = TurnEnd::Error(message.clone()),
        _ => {}
    }
}
