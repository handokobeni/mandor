//! Mesin Waiting Fix (U11, KTD-12): deteksi, komentar tepercaya, jeda tenang,
//! perbaikan otomatis, dan kembali ke Waiting Review secara idempoten.
//!
//! Urutan per kiriman: jurnal `pending` → kirim ke agent → jurnal `delivered` →
//! kursor Lark → status. Kursor ditulis sebelum status.

use std::collections::HashSet;
use std::sync::{Arc, PoisonError};

use mandor_domain::cursor::newer_than;
use mandor_domain::run::touches_sensitive_path;
use mandor_domain::trust::is_trusted;
use mandor_domain::{AppField, Cursor, Reply, RunState, Task, TaskStatus, WriteOp};

use super::feedback_qa::feedback_qa_replies;
use crate::notify::notice;
use crate::ports::{Clock, Deps, LarkTasks, TaskCache};
use crate::run::{RunError, RunService, TurnEnd};
use crate::run_ports::{
    FixContext, GitPort, Journal, NoticeKind, Notifier, Prompts, RepoInfo, RunRecord, RunStore,
    SessionRef,
};

/// Jeda tenang default sejak komentar tepercaya pertama.
pub const DEFAULT_QUIET_SECS: i64 = 120;

/// Ringkasan satu putaran mesin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FixReport {
    /// Record yang baru masuk loop perbaikan.
    pub entered: Vec<String>,
    /// Record yang perbaikannya dimulai pada putaran ini.
    pub started: Vec<String>,
    /// `true` bila komentar gagal dibaca dan fallback Fix Notes dipakai.
    pub comments_unavailable: bool,
}

/// Komentar record untuk ditampilkan di kartu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentView {
    pub reply: Reply,
    pub trusted: bool,
    /// Sudah diteruskan ke agent (journal `delivered`).
    pub sent: bool,
}

impl<D: Deps> RunService<D> {
    /// Satu putaran (setiap poll komentar 20 detik).
    ///
    /// # Errors
    /// Error penyimpanan; kegagalan per record hanya dicatat.
    pub async fn tick_waiting_fix(
        self: &Arc<Self>,
        quiet_secs: i64,
    ) -> Result<FixReport, RunError> {
        let mut report = FixReport::default();
        let candidates = self.fix_candidates().await?;
        if candidates.is_empty() {
            return Ok(report);
        }
        let oldest = candidates
            .iter()
            .filter_map(|t| t.comment_cursor.as_deref().and_then(Cursor::parse))
            .map(|c| c.create_time)
            .min();
        let mut replies = match self.sync.lark.record_replies(oldest).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "komentar Lark tidak terbaca; memakai Fix Notes");
                report.comments_unavailable = true;
                Vec::new()
            }
        };
        replies.extend(self.feedback_qa_of(&candidates).await);
        self.replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone_from(&replies);

        for task in candidates {
            let record_id = task.record_id.clone();
            match self
                .process_record(task, &replies, quiet_secs, report.comments_unavailable)
                .await
            {
                Ok(Step::Entered) => report.entered.push(record_id),
                Ok(Step::Started) => report.started.push(record_id),
                Ok(Step::Idle) => {}
                Err(e) => {
                    tracing::warn!(record_id, error = %e, "mesin Waiting Fix gagal untuk record");
                }
            }
        }
        Ok(report)
    }

    /// Feedback QA dari AC setiap kandidat sebagai balasan sintetis (R11f). AC
    /// yang gagal dibaca hanya dicatat; komentar tetap diproses.
    async fn feedback_qa_of(&self, candidates: &[Task]) -> Vec<Reply> {
        let mut out = Vec::new();
        for task in candidates.iter().filter(|t| !t.ac_ids.is_empty()) {
            let requester = match self.fix_requester(task).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(record_id = %task.record_id, error = %e, "run tidak terbaca; Feedback QA dilewati");
                    continue;
                }
            };
            match self.sync.lark.acceptance_criteria(&task.ac_ids).await {
                Ok(acs) => out.extend(feedback_qa_replies(task, &acs, requester.as_deref())),
                Err(e) => tracing::warn!(
                    record_id = %task.record_id,
                    error = %e,
                    "Feedback QA tidak terbaca; komentar tetap diproses"
                ),
            }
        }
        out
    }

    /// Orang yang memindahkan task ke Waiting Fix pada siklus ini: yang dicatat
    /// run bila loop perbaikan sudah berjalan, atau pengubah terakhir task bila
    /// loop baru akan dimulai pada putaran ini.
    async fn fix_requester(&self, task: &Task) -> Result<Option<String>, RunError> {
        let run = self.runs.active_for(&task.record_id).await?;
        Ok(match run {
            Some(r) if r.state.in_fix_loop() => r.fix_requested_by,
            _ => task.last_modified_by.clone(),
        })
    }

    /// Record milik pengguna (pemilik klaim) yang berstatus Waiting Fix atau
    /// sedang dalam loop perbaikan.
    async fn fix_candidates(&self) -> Result<Vec<Task>, RunError> {
        let looped: HashSet<String> = self.fix_loop_records().await?.into_iter().collect();
        let me = self.sync.me();
        Ok(self
            .sync
            .cache
            .list()
            .await?
            .into_iter()
            .map(|c| c.task)
            .filter(|t| {
                looped.contains(&t.record_id)
                    || (t.status == TaskStatus::WaitingFix && t.claimed_by(me) && t.assigned_to(me))
            })
            .collect())
    }

    /// Komentar terakhir yang diketahui untuk record (untuk kartu), beserta
    /// apakah sudah diteruskan ke agent.
    ///
    /// # Errors
    /// Journal tidak bisa dibaca.
    pub async fn record_comments(
        &self,
        task: &Task,
        reviewers: &[String],
    ) -> Result<Vec<CommentView>, RunError> {
        let known: Vec<Reply> = {
            let replies = self.replies.lock().unwrap_or_else(PoisonError::into_inner);
            newer_than(&replies, &task.record_id, None)
                .into_iter()
                .cloned()
                .collect()
        };
        let delivered = self.runs.journal_delivered_list(&task.record_id).await?;
        let mut out: Vec<CommentView> = known
            .into_iter()
            .map(|r| CommentView {
                trusted: is_trusted(&r.author, task, reviewers),
                sent: delivered.contains(&r.reply_id),
                reply: r,
            })
            .collect();
        out.reverse();
        Ok(out)
    }

    async fn process_record(
        self: &Arc<Self>,
        task: Task,
        replies: &[Reply],
        quiet_secs: i64,
        comments_unavailable: bool,
    ) -> Result<Step, RunError> {
        let existing = self.runs.active_for(&task.record_id).await?;
        let mut entered = false;
        let mut run = match existing {
            Some(r) if r.state.in_fix_loop() => r,
            Some(r) if r.state.is_busy() || r.state == RunState::Publishing => {
                return Ok(Step::Idle);
            }
            other if task.status == TaskStatus::WaitingFix => {
                entered = true;
                match self.enter_fix_loop(&task, other).await? {
                    Some(r) => r,
                    None => return Ok(Step::Idle),
                }
            }
            _ => return Ok(Step::Idle),
        };
        if run.state == RunState::WfRunning {
            return Ok(Step::Idle);
        }
        // Status diubah manusia di luar Waiting Fix sebelum perbaikan dimulai.
        if task.status != TaskStatus::WaitingFix {
            run.state = RunState::InReviewLoop;
            self.runs.save(&run, run.generation, self.now()).await?;
            return Ok(Step::Idle);
        }
        let repo = self.repo_of(&run).await?;
        let mut batch = self
            .unsent_trusted(&task, replies, &repo.trusted_reviewers)
            .await?;
        if batch.is_empty() && comments_unavailable {
            if let Some(r) = self.fix_notes_reply(&task, &repo, &mut run).await? {
                batch.push(r);
            }
        }
        let pending_resend = !self
            .runs
            .journal_pending_list(&task.record_id)
            .await?
            .is_empty();
        if batch.is_empty() {
            self.reconcile_cursor(&task, replies).await?;
            return Ok(if entered { Step::Entered } else { Step::Idle });
        }
        let quiet_since = batch.iter().map(|r| r.create_time).min().unwrap_or(0);
        let now_secs = self.sync.clock.now_ms() / 1000;
        if !pending_resend && now_secs - quiet_since < quiet_secs {
            let until = Some((quiet_since + quiet_secs) * 1000);
            if run.state != RunState::WfQuiet || run.quiet_until_ms != until {
                run.state = RunState::WfQuiet;
                run.quiet_until_ms = until;
                self.runs.save(&run, run.generation, self.now()).await?;
            }
            return Ok(if entered { Step::Entered } else { Step::Idle });
        }
        self.start_fix_turn(run, &task, &repo, batch, None).await?;
        Ok(Step::Started)
    }

    /// Masuk loop perbaikan: memakai run yang ada atau membuat run otomatis dari
    /// branch task (termasuk PR yang dibuat di laptop lain atau oleh lembur).
    async fn enter_fix_loop(
        &self,
        task: &Task,
        existing: Option<RunRecord>,
    ) -> Result<Option<RunRecord>, RunError> {
        let mut run = if let Some(r) = existing {
            r
        } else {
            let (Some(branch), Some(repo_id)) = (task.branch.clone(), task.repo_id.clone()) else {
                tracing::warn!(record_id = %task.record_id, "Waiting Fix tanpa branch atau repo; dilewati");
                return Ok(None);
            };
            let mut r = RunRecord::new(
                crate::ports::Ids::nonce(&self.sync.ids),
                repo_id,
                task.record_id.clone(),
            );
            r.branch = Some(branch);
            r.pr_url.clone_from(&task.pr_url);
            r.state = RunState::WfAwaitingComment;
            self.runs.create(&r, self.now()).await?;
            r
        };
        run.state = RunState::WfAwaitingComment;
        run.automatic = true;
        run.note = None;
        run.fix_requested_by.clone_from(&task.last_modified_by);
        self.runs.save(&run, run.generation, self.now()).await?;
        self.notifier
            .notify(
                notice(
                    self,
                    NoticeKind::WaitingFix,
                    &run,
                    "The reviewer requested fixes",
                )
                .await,
            )
            .await;
        Ok(Some(run))
    }

    /// Balasan tepercaya yang lebih baru dari kursor dan belum `delivered`,
    /// ditambah balasan `pending` yang harus dikirim ulang setelah crash.
    async fn unsent_trusted(
        &self,
        task: &Task,
        replies: &[Reply],
        reviewers: &[String],
    ) -> Result<Vec<Reply>, RunError> {
        let cursor = task.comment_cursor.as_deref().and_then(Cursor::parse);
        let delivered: HashSet<String> = self
            .runs
            .journal_delivered_list(&task.record_id)
            .await?
            .into_iter()
            .collect();
        let pending: HashSet<String> = self
            .runs
            .journal_pending_list(&task.record_id)
            .await?
            .into_iter()
            .collect();
        Ok(newer_than(replies, &task.record_id, cursor.as_ref())
            .into_iter()
            .chain(
                replies
                    .iter()
                    .filter(|r| r.record_id == task.record_id && pending.contains(&r.reply_id)),
            )
            .filter(|r| !delivered.contains(&r.reply_id))
            .filter(|r| is_trusted(&r.author, task, reviewers))
            .fold(Vec::<Reply>::new(), |mut acc, r| {
                if !acc.iter().any(|x| x.reply_id == r.reply_id) {
                    acc.push(r.clone());
                }
                acc
            }))
    }

    /// Kursor Lark tertinggal dari balasan yang sudah `delivered` (crash setelah
    /// kirim sebelum kursor ditulis): antrekan kursor tanpa mengirim ulang.
    async fn reconcile_cursor(&self, task: &Task, replies: &[Reply]) -> Result<(), RunError> {
        let cursor = task.comment_cursor.as_deref().and_then(Cursor::parse);
        let delivered: HashSet<String> = self
            .runs
            .journal_delivered_list(&task.record_id)
            .await?
            .into_iter()
            .collect();
        let Some(max) = newer_than(replies, &task.record_id, cursor.as_ref())
            .into_iter()
            .filter(|r| delivered.contains(&r.reply_id))
            .map(Cursor::of)
            .max()
        else {
            return Ok(());
        };
        self.enqueue_cursor(&task.record_id, &max).await
    }

    async fn enqueue_cursor(&self, record_id: &str, cursor: &Cursor) -> Result<(), RunError> {
        let text = cursor.encode();
        {
            let mut sent = self
                .cursor_sent
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if sent.get(record_id) == Some(&text) {
                return Ok(());
            }
            sent.insert(record_id.to_owned(), text.clone());
        }
        let op = WriteOp::Fields(vec![AppField::CommentCursor(text)]);
        self.sync.request_write(record_id, &op).await?;
        Ok(())
    }

    /// Menjalankan giliran perbaikan dengan `batch` komentar (dan instruksi dev).
    async fn start_fix_turn(
        self: &Arc<Self>,
        mut run: RunRecord,
        task: &Task,
        repo: &RepoInfo,
        batch: Vec<Reply>,
        instruction: Option<&str>,
    ) -> Result<(), RunError> {
        // Perbaikan selalu mengerjakan kode, termasuk run yang masih di fase plan.
        run.phase = crate::run_ports::RunPhase::Implement;
        // Worktree yang hilang dibuat ulang sebelum diff diambil.
        let mut req = self
            .agent_request(&mut run, SessionRef::New(String::new()), String::new())
            .await?;
        let worktree = run
            .worktree
            .clone()
            .ok_or(RunError::InvalidState("no worktree"))?;
        let resumable = run.session_started && run.session_id.is_some();
        let message = if resumable {
            self.prompts.fix_message(&FixContext {
                task,
                acs: &[],
                diff: None,
                comments: &batch,
                instruction,
            })
        } else {
            let acs = self.ac_texts(task).await?;
            let diff = self
                .git
                .branch_diff(repo, &worktree)
                .await
                .unwrap_or_default();
            run.session_id = Some(crate::ports::Ids::nonce(&self.sync.ids));
            self.prompts.fix_message(&FixContext {
                task,
                acs: &acs,
                diff: Some(&diff),
                comments: &batch,
                instruction,
            })
        };
        let session_id = run.session_id.clone().unwrap_or_default();
        req.session = if resumable {
            SessionRef::Resume(session_id)
        } else {
            SessionRef::New(session_id)
        };
        req.message = message;
        for r in &batch {
            self.runs
                .journal_pending(&task.record_id, &r.reply_id, &run.id, self.now())
                .await?;
        }
        run.automatic = true;
        let summary = instruction.map_or_else(
            || format!("{} reviewer comment(s) forwarded to the agent", batch.len()),
            str::to_owned,
        );
        self.spawn_turn(&mut run, req, Some(summary)).await?;
        for r in &batch {
            self.runs
                .journal_delivered(&task.record_id, &r.reply_id)
                .await?;
        }
        // Kursor sebelum status.
        if let Some(max) = batch.iter().map(Cursor::of).max() {
            self.enqueue_cursor(&task.record_id, &max).await?;
        }
        if task.status == TaskStatus::WaitingFix {
            let op = WriteOp::Transition {
                from: TaskStatus::WaitingFix,
                to: TaskStatus::InProgress,
                fields: vec![],
            };
            self.sync.request_write(&task.record_id, &op).await?;
        }
        Ok(())
    }

    pub(crate) async fn ac_texts(&self, task: &Task) -> Result<Vec<String>, RunError> {
        Ok(self
            .sync
            .lark
            .acceptance_criteria(&task.ac_ids)
            .await?
            .into_iter()
            .filter(|ac| !ac.dropped)
            .map(|ac| match ac.label {
                Some(l) => format!("{l}: {}", ac.text),
                None => ac.text,
            })
            .collect())
    }

    /// "Mulai perbaikan" (R11d): instruksi dev ditambah komentar pilihan dev
    /// (boleh tidak tepercaya) dan komentar tepercaya yang belum terkirim.
    ///
    /// # Errors
    /// Record tidak dalam Waiting Fix atau loop perbaikan, atau agent gagal.
    pub async fn start_fix(
        self: &Arc<Self>,
        record_id: &str,
        instruction: &str,
        chosen: &[String],
    ) -> Result<(), RunError> {
        let task = self.task_of(record_id).await?;
        self.sync.cache.upsert(&task).await?;
        let existing = self.runs.active_for(record_id).await?;
        let run = match existing {
            Some(r) if r.state.in_fix_loop() && r.state != RunState::WfRunning => r,
            Some(r) if r.state.is_busy() => return Err(RunError::InvalidState(r.state.as_str())),
            other if task.status == TaskStatus::WaitingFix => self
                .enter_fix_loop(&task, other)
                .await?
                .ok_or(RunError::InvalidState("no branch"))?,
            _ => return Err(RunError::InvalidState("not in Waiting Fix")),
        };
        let repo = self.repo_of(&run).await?;
        let replies = self
            .replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let mut batch = self
            .unsent_trusted(&task, &replies, &repo.trusted_reviewers)
            .await?;
        for r in replies
            .iter()
            .filter(|r| r.record_id == record_id && chosen.contains(&r.reply_id))
        {
            if !batch.iter().any(|b| b.reply_id == r.reply_id) {
                batch.push(r.clone());
            }
        }
        // Instruksi opsional bila ada komentar yang dikirim; tanpa keduanya tidak ada
        // yang bisa dikerjakan agent.
        let instruction = Some(instruction.trim()).filter(|i| !i.is_empty());
        if instruction.is_none() && batch.is_empty() {
            return Err(RunError::InvalidState("no instruction or comment to send"));
        }
        self.start_fix_turn(run, &task, &repo, batch, instruction)
            .await
    }

    /// Akhir giliran perbaikan: lanjut dengan komentar baru, atau commit, push,
    /// dan Waiting Review. Tanpa perubahan atau dengan perubahan sensitif, kartu
    /// ditandai dan status tidak diubah.
    pub(crate) async fn finish_fix_turn(
        self: &Arc<Self>,
        mut run: RunRecord,
        end: TurnEnd,
    ) -> Result<(), RunError> {
        let generation = run.generation;
        if let TurnEnd::Error(msg) = end {
            run.state = RunState::Failed;
            run.last_error = Some(msg.clone());
            self.runs.save(&run, generation, self.now()).await?;
            self.notifier
                .notify(notice(self, NoticeKind::Failed, &run, &msg).await)
                .await;
            return Ok(());
        }
        let task = self.task_of(&run.record_id).await?;
        let repo = self.repo_of(&run).await?;
        let replies = self
            .replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let more = self
            .unsent_trusted(&task, &replies, &repo.trusted_reviewers)
            .await?;
        if !more.is_empty() {
            // Komentar yang tiba saat agent berjalan: giliran berikutnya, sesi sama.
            return self.continue_fix(run, task, repo, more).await;
        }
        let work = self.member_changes(&run).await?;
        if work.iter().all(|(_, c)| c.is_empty()) {
            return self
                .flag_fix(
                    run,
                    RunState::InReviewLoop,
                    "Fix finished without changes; status left unchanged",
                )
                .await;
        }
        let files = work
            .iter()
            .flat_map(|(_, c)| c.files.iter().map(String::as_str));
        if touches_sensitive_path(files) {
            return self
                .flag_fix(
                    run,
                    RunState::AwaitingDev,
                    "Changes touch CI, agent config, or git hooks; push needs developer confirmation",
                )
                .await;
        }
        run.state = RunState::Publishing;
        self.runs.save(&run, generation, self.now()).await?;
        let url = self.publish_members(&mut run, &work).await?;
        self.mark_waiting_review(&mut run, &url, false).await
    }

    fn continue_fix(
        self: &Arc<Self>,
        run: RunRecord,
        task: Task,
        repo: RepoInfo,
        batch: Vec<Reply>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), RunError>> + Send + '_>>
    {
        Box::pin(async move { self.start_fix_turn(run, &task, &repo, batch, None).await })
    }

    async fn flag_fix(
        &self,
        mut run: RunRecord,
        state: RunState,
        note: &str,
    ) -> Result<(), RunError> {
        run.state = state;
        run.note = Some(note.to_owned());
        self.runs.save(&run, run.generation, self.now()).await?;
        self.notifier
            .notify(notice(self, NoticeKind::NeedsAttention, &run, note).await)
            .await;
        Ok(())
    }
}

/// Hasil pemrosesan satu record.
enum Step {
    Idle,
    Entered,
    Started,
}
