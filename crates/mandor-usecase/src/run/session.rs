//! Giliran agent: spawn, driver event, antrean pesan, kirim sekarang, stop.
//!
//! Setiap spawn menaikkan `generation`. Driver hanya boleh mengubah run bila
//! generasinya masih yang tersimpan, sehingga event giliran lama tidak menimpa
//! giliran baru (perbaikan bug race stop→send).

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
        while let Some(event) = handle.next_event().await {
            observe(&event, &mut end, &mut stats);
            // Sesi reviewer tidak masuk chat pelaksana; progresnya lewat buku AC.
            match self.current_state(&run_id, generation).await {
                Some(current) if current != RunState::Reviewing => {}
                _ => continue,
            }
            if let Err(e) = self
                .runs
                .append(&run_id, &ChatItem::Agent(event), self.now())
                .await
            {
                tracing::warn!(run_id, error = %e, "gagal menyimpan event agent");
            }
        }
        if let Err(e) = self.finish_turn(&run_id, generation, end, stats).await {
            tracing::warn!(run_id, error = %e, "gagal menyelesaikan giliran agent");
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
                // VM: kartu dikerjakan sampai PR atau tertahan dengan alasan (U4 plan VM).
                // Run grup di VM menyusul di U10 plan run grup.
                if self.is_vm()
                    && run.phase == RunPhase::Implement
                    && self.runs.cards(&run.id).await?.is_empty()
                {
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
            .filter(|r| follow_design.contains(&r.ac_id))
            .map(|r| format!("{}: {}", r.display_label(), r.text))
            .collect();
        run.phase = RunPhase::Implement;
        let message = self.prompts.approve_message(&followed);
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
    async fn interrupt(&self, run_id: &str) -> Result<bool, RunError> {
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
fn follow_design_rows(mut rows: Vec<AcRow>, ids: &[String]) -> Result<Vec<AcRow>, RunError> {
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

fn rejected(message: String) -> RunError {
    RunError::Port(PortError::Rejected(message))
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
