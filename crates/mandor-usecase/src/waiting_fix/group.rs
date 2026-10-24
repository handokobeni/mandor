//! Waiting Fix run grup (U7 plan run grup, KTD-G8): loop perbaikan per kartu
//! di satu run, satu giliran untuk semua kartu yang siap, dan run lanjutan dari
//! branch default bila PR grup sudah merged (R17, R19).

use std::collections::HashMap;
use std::sync::Arc;

use mandor_domain::group::{CardState, RemovedReason};
use mandor_domain::run::branch_name;
use mandor_domain::{Reply, RunState, Task, TaskStatus};

use crate::notify::notice;
use crate::ports::{Clock, Deps, Ids};
use crate::run::{RunError, RunService};
use crate::run_ports::{
    ChatItem, GitPort, GroupCardRow, Journal, NoticeKind, Notifier, RepoInfo, RunRecord, RunStore,
};

/// Hasil satu putaran untuk satu run grup.
#[derive(Debug, Default)]
pub(super) struct GroupStep {
    pub entered: Vec<String>,
    pub started: Vec<String>,
}

impl<D: Deps> RunService<D> {
    /// Memisahkan kandidat milik run grup (dikelompokkan per run) dari kandidat
    /// run tunggal.
    pub(super) async fn split_group_candidates(
        &self,
        candidates: Vec<Task>,
    ) -> Result<(Vec<(RunRecord, Vec<Task>)>, Vec<Task>), RunError> {
        let mut groups: Vec<(RunRecord, Vec<Task>)> = Vec::new();
        let mut single = Vec::new();
        let mut is_group: HashMap<String, bool> = HashMap::new();
        for task in candidates {
            let Some(run) = self.runs.active_for(&task.record_id).await? else {
                single.push(task);
                continue;
            };
            let grouped = if let Some(g) = is_group.get(&run.id) {
                *g
            } else {
                let g = !self.runs.cards(&run.id).await?.is_empty();
                is_group.insert(run.id.clone(), g);
                g
            };
            if !grouped {
                single.push(task);
            } else if let Some((_, tasks)) = groups.iter_mut().find(|(r, _)| r.id == run.id) {
                tasks.push(task);
            } else {
                groups.push((run, vec![task]));
            }
        }
        Ok((groups, single))
    }

    /// Satu putaran Waiting Fix untuk run grup: kartu masuk dan keluar loop
    /// sendiri-sendiri, jeda tenang per kartu, lalu satu giliran untuk semua
    /// kartu yang siap.
    pub(super) async fn process_group(
        self: &Arc<Self>,
        mut run: RunRecord,
        tasks: Vec<Task>,
        replies: &[Reply],
        quiet_secs: i64,
        comments_unavailable: bool,
    ) -> Result<GroupStep, RunError> {
        let mut cards = self.runs.cards(&run.id).await?;
        let step = self.update_fix_cards(&run, &mut cards, &tasks).await?;
        if run.state.is_working() || run.state.is_parked() {
            // Kartu baru ikut giliran berikutnya setelah giliran ini selesai.
            return Ok(step);
        }
        let fixing: Vec<usize> = cards
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_open() && c.in_fix && waiting(&tasks, &c.record_id))
            .map(|(i, _)| i)
            .collect();
        if fixing.is_empty() {
            if matches!(run.state, RunState::WfAwaitingComment | RunState::WfQuiet) {
                run.state = RunState::InReviewLoop;
                run.quiet_until_ms = None;
                self.runs.save(&run, run.generation, self.now()).await?;
            }
            return Ok(step);
        }
        if self.group_pr_merged(&run).await {
            for record_id in fixing.iter().map(|&i| cards[i].record_id.clone()) {
                self.split_follow_up(&mut run, &record_id, false).await?;
            }
            if matches!(run.state, RunState::WfAwaitingComment | RunState::WfQuiet) {
                run.state = RunState::InReviewLoop;
                self.runs.save(&run, run.generation, self.now()).await?;
            }
            return Ok(step);
        }
        if !step.entered.is_empty() {
            self.refresh_group_bodies(&mut run).await?;
        }
        let repo = self.repo_of(&run).await?;
        let ctx = FixRound {
            tasks: &tasks,
            replies,
            quiet_secs,
            comments_unavailable,
            repo: &repo,
        };
        let (ready, until) = self
            .ready_cards(&mut run, &mut cards, &fixing, &ctx)
            .await?;
        if ready.is_empty() {
            self.runs.save_cards(&run.id, &cards, None).await?;
            let state = if until.is_some() {
                RunState::WfQuiet
            } else {
                RunState::WfAwaitingComment
            };
            if run.state != state || run.quiet_until_ms != until {
                run.state = state;
                run.quiet_until_ms = until;
                run.automatic = true;
                self.runs.save(&run, run.generation, self.now()).await?;
            }
            return Ok(step);
        }
        let started = ready.iter().map(|(t, _)| t.record_id.clone()).collect();
        self.start_fix_turn(run, ready, &repo, None).await?;
        Ok(GroupStep {
            entered: step.entered,
            started,
        })
    }

    /// Komentar siap kirim per kartu yang sedang diperbaiki; kartu yang masih
    /// dalam jeda tenang mencatat batasnya. Mengembalikan batch siap dan batas
    /// jeda tenang terdekat.
    async fn ready_cards(
        &self,
        run: &mut RunRecord,
        cards: &mut [GroupCardRow],
        fixing: &[usize],
        ctx: &FixRound<'_>,
    ) -> Result<(Vec<(Task, Vec<Reply>)>, Option<i64>), RunError> {
        let now_secs = self.sync.clock.now_ms() / 1000;
        let mut ready = Vec::new();
        let mut until: Option<i64> = None;
        for &i in fixing {
            let Some(task) = ctx.tasks.iter().find(|t| t.record_id == cards[i].record_id) else {
                continue;
            };
            let mut batch = self
                .unsent_trusted(task, ctx.replies, &ctx.repo.trusted_reviewers)
                .await?;
            if batch.is_empty()
                && ctx.comments_unavailable
                && let Some(r) = self.fix_notes_reply(task, ctx.repo, run).await?
            {
                batch.push(r);
            }
            if batch.is_empty() {
                self.reconcile_cursor(task, ctx.replies).await?;
                continue;
            }
            let resend = !self
                .runs
                .journal_pending_list(&task.record_id)
                .await?
                .is_empty();
            let since = batch.iter().map(|r| r.create_time).min().unwrap_or(0);
            if !resend && now_secs - since < ctx.quiet_secs {
                let card_until = (since + ctx.quiet_secs) * 1000;
                cards[i].quiet_until_ms = Some(card_until);
                until = Some(until.map_or(card_until, |u| u.min(card_until)));
                continue;
            }
            ready.push((task.clone(), batch));
        }
        Ok((ready, until))
    }

    /// Kartu masuk loop perbaikan saat berstatus Waiting Fix (dengan pemindahnya
    /// dicatat sekali), dan keluar bila manusia memindahnya ke status lain selain
    /// In Progress. Satu notifikasi per kartu yang masuk.
    async fn update_fix_cards(
        &self,
        run: &RunRecord,
        cards: &mut [GroupCardRow],
        tasks: &[Task],
    ) -> Result<GroupStep, RunError> {
        let mut step = GroupStep::default();
        let mut changed = false;
        for task in tasks {
            let Some(card) = cards
                .iter_mut()
                .find(|c| c.record_id == task.record_id && c.is_open())
            else {
                continue;
            };
            if task.status == TaskStatus::WaitingFix && !card.in_fix {
                card.in_fix = true;
                card.fix_requested_by.clone_from(&task.last_modified_by);
                card.quiet_until_ms = None;
                card.fix_replies.clear();
                changed = true;
                step.entered.push(task.record_id.clone());
            } else if card.in_fix
                && !matches!(task.status, TaskStatus::WaitingFix | TaskStatus::InProgress)
            {
                card.in_fix = false;
                card.quiet_until_ms = None;
                changed = true;
            }
        }
        if changed {
            self.runs.save_cards(&run.id, cards, None).await?;
        }
        for record_id in &step.entered {
            let mut about = run.clone();
            about.record_id.clone_from(record_id);
            self.notifier
                .notify(
                    notice(
                        self,
                        NoticeKind::WaitingFix,
                        &about,
                        "The reviewer requested fixes",
                    )
                    .await,
                )
                .await;
        }
        Ok(step)
    }

    /// "Mulai perbaikan" pada kartu grup: kartu masuk loop bila belum, dan run
    /// grup siap menerima giliran perbaikan.
    pub(super) async fn enter_group_card(
        &self,
        mut run: RunRecord,
        task: &Task,
    ) -> Result<RunRecord, RunError> {
        let mut cards = self.runs.cards(&run.id).await?;
        let card = cards
            .iter_mut()
            .find(|c| c.record_id == task.record_id && c.is_open())
            .ok_or(RunError::NotFound("card"))?;
        if !card.in_fix {
            if task.status != TaskStatus::WaitingFix {
                return Err(RunError::InvalidState("not in Waiting Fix"));
            }
            card.in_fix = true;
            card.fix_requested_by.clone_from(&task.last_modified_by);
            self.runs.save_cards(&run.id, &cards, None).await?;
        }
        if !run.state.in_fix_loop() {
            run.state = RunState::WfAwaitingComment;
            self.runs.save(&run, run.generation, self.now()).await?;
        }
        Ok(run)
    }

    /// Komentar tepercaya yang belum terkirim untuk setiap kartu grup yang masih
    /// Waiting Fix, termasuk kartu yang masuk saat giliran berjalan (R17).
    pub(super) async fn group_unsent(
        &self,
        run: &RunRecord,
        repo: &RepoInfo,
        replies: &[Reply],
    ) -> Result<Vec<(Task, Vec<Reply>)>, RunError> {
        let mut out = Vec::new();
        for card in self
            .runs
            .cards(&run.id)
            .await?
            .into_iter()
            .filter(|c| c.is_open() && c.in_fix)
        {
            let task = self.task_of(&card.record_id).await?;
            if task.status != TaskStatus::WaitingFix {
                continue;
            }
            let batch = self
                .unsent_trusted(&task, replies, &repo.trusted_reviewers)
                .await?;
            if !batch.is_empty() {
                out.push((task, batch));
            }
        }
        Ok(out)
    }

    /// PR grup (salah satu anggota) sudah merged. Kegagalan membaca PR hanya
    /// dicatat dan dianggap belum merged.
    pub(super) async fn group_pr_merged(&self, run: &RunRecord) -> bool {
        let Some(branch) = run.branch.as_deref() else {
            return false;
        };
        let members = match self.members(run).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(run_id = %run.id, error = %e, "anggota run tidak terbaca; PR dianggap belum merged");
                return false;
            }
        };
        for m in members.iter().filter(|m| m.pr_url.is_some()) {
            match self.git.find_pr(&m.repo, branch).await {
                Ok(Some(pr)) if pr.merged => return true,
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(run_id = %run.id, error = %e, "status PR grup tidak terbaca");
                }
            }
        }
        false
    }

    /// PR grup merged saat perbaikan berjalan: setiap kartu yang sedang
    /// diperbaiki pindah ke run lanjutan dengan komentar yang sudah dikirim
    /// dikembalikan ke `pending`; tidak ada push ke branch grup.
    pub(super) async fn follow_up_fixing(&self, mut run: RunRecord) -> Result<(), RunError> {
        let fixing: Vec<String> = self
            .runs
            .cards(&run.id)
            .await?
            .into_iter()
            .filter(|c| c.is_open() && c.in_fix)
            .map(|c| c.record_id)
            .collect();
        for record_id in &fixing {
            self.split_follow_up(&mut run, record_id, true).await?;
        }
        self.flag_fix(
            run,
            RunState::InReviewLoop,
            "The group pull request merged during the fix; the fix moved to a follow-up pull request",
        )
        .await
    }

    /// Mengeluarkan kartu dari grup ke run tunggal baru dari branch default (R19,
    /// KTD-G8). `requeue` mengembalikan komentar siklus ini ke `pending` agar run
    /// lanjutan menerimanya.
    async fn split_follow_up(
        &self,
        run: &mut RunRecord,
        record_id: &str,
        requeue: bool,
    ) -> Result<(), RunError> {
        let mut cards = self.runs.cards(&run.id).await?;
        let Some(idx) = cards.iter().position(|c| c.record_id == record_id) else {
            return Ok(());
        };
        let task = self.task_of(record_id).await?;
        if requeue {
            for reply in &cards[idx].fix_replies {
                self.runs.journal_requeue(record_id, reply).await?;
            }
        }
        let repo = self.repo_of(run).await?;
        let branch = branch_name(task.key.as_deref(), &task.record_id, &task.title);
        let worktree = self.git.prepare_worktree(&repo, &branch, None).await?;
        let mut follow = RunRecord::new(
            Ids::nonce(&self.sync.ids),
            repo.id.clone(),
            record_id.to_owned(),
        );
        follow.branch = Some(branch);
        follow.worktree = Some(worktree);
        follow.state = RunState::WfAwaitingComment;
        follow.automatic = true;
        follow
            .fix_requested_by
            .clone_from(&cards[idx].fix_requested_by);
        cards[idx].state = CardState::Removed(RemovedReason::FollowUp);
        cards[idx].in_fix = false;
        cards[idx].fix_replies.clear();
        self.save_group(run, &cards).await?;
        self.runs.create(&follow, self.now()).await?;
        let key = task.key.as_deref().unwrap_or(record_id);
        self.runs
            .append(
                &run.id,
                &ChatItem::System(format!(
                    "The group pull request already merged; {key} is fixed in a follow-up run."
                )),
                self.now(),
            )
            .await?;
        Ok(())
    }

    /// Menulis ulang badan PR grup (peringatan "do not merge", R19). Badan yang
    /// sudah disunting orang tidak disentuh.
    async fn refresh_group_bodies(&self, run: &mut RunRecord) -> Result<(), RunError> {
        let members = self.members(run).await?;
        let mut changed = false;
        for m in &members {
            if let Some(url) = m.pr_url.clone() {
                changed |= self.refresh_pr_body(run, m, &url).await;
            }
        }
        if changed {
            self.runs.save(run, run.generation, self.now()).await?;
        }
        Ok(())
    }
}

/// Bahan satu putaran Waiting Fix grup.
struct FixRound<'a> {
    tasks: &'a [Task],
    replies: &'a [Reply],
    quiet_secs: i64,
    comments_unavailable: bool,
    repo: &'a RepoInfo,
}

/// Kartu `record_id` berstatus Waiting Fix di daftar task putaran ini.
fn waiting(tasks: &[Task], record_id: &str) -> bool {
    tasks
        .iter()
        .any(|t| t.record_id == record_id && t.status == TaskStatus::WaitingFix)
}
