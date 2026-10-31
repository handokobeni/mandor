//! Run grup kartu satu flow (plan `2026-10-23-004`): kartu dikerjakan
//! berurutan di satu branch dengan satu PR di akhir. Aturan murni ada di
//! `mandor_domain::group`.

use std::sync::Arc;

use mandor_domain::group::{self, Candidate, CardState, Unselectable};
use mandor_domain::planned::PlannedWork;
use mandor_domain::{AcRow, AppField, HoldReason, RunState, Task, TaskStatus, WriteOp};

use super::members::{COMMIT_DIFF_MAX, group_name};
use super::start::{Plan, start_summary};
use super::{RunError, RunService};
use crate::ports::{AcItem, Deps, Ids, LarkTasks, TaskCache};
use crate::review::DesignRoot;
use crate::run_ports::{
    AgentPort, CardCommit, ChatItem, GitPort, GroupCardRow, LedgerStore, Prompts, RunMember,
    RunPhase, RunRecord, RunStore, RunWorkspace, SessionRef,
};
use crate::{Planning, StartStep, StartTarget};

/// AC aktif sebuah kartu dalam bentuk pesan agent.
fn ac_lines(items: &[AcItem]) -> Vec<String> {
    items
        .iter()
        .filter(|ac| !ac.dropped)
        .map(|ac| match &ac.label {
            Some(l) => format!("{l}: {}", ac.text),
            None => ac.text.clone(),
        })
        .collect()
}

/// Kunci tampilan kartu.
pub(super) fn key_of(t: &Task) -> &str {
    t.key.as_deref().unwrap_or(&t.record_id)
}

/// Catatan konteks flow untuk agent: kartu ini bagian dari flow berurutan.
fn flow_note(tasks: &[Task], current: usize, dev_note: Option<&str>) -> String {
    let list: Vec<String> = tasks
        .iter()
        .map(|t| format!("{} {}", key_of(t), t.title))
        .collect();
    let mut note = format!(
        "This task is card {} of {} in one flow on the same branch: {}. Work only on {} now; \
         the next cards follow on this branch after it is finished.",
        current + 1,
        tasks.len(),
        list.join("; "),
        key_of(&tasks[current]),
    );
    if let Some(n) = dev_note.map(str::trim).filter(|n| !n.is_empty()) {
        note.push_str("\n\n");
        note.push_str(n);
    }
    note
}

/// Baris buku AC awal grup: AC aktif tiap kartu, diberi label kartunya.
fn group_rows(per_card: &[(String, Vec<AcItem>)]) -> Vec<AcRow> {
    per_card
        .iter()
        .flat_map(|(card, items)| {
            items.iter().filter(|a| !a.dropped).map(move |a| {
                let mut row = AcRow::new(
                    &a.record_id,
                    a.label.as_deref(),
                    &a.text,
                    mandor_domain::RowStatus::Active,
                );
                row.card = Some(card.clone());
                row
            })
        })
        .collect()
}

impl<D: Deps> RunService<D> {
    /// Kartu run grup, urut posisi; kosong untuk run tunggal.
    ///
    /// # Errors
    /// Penyimpanan gagal.
    pub async fn group_cards(&self, run_id: &str) -> Result<Vec<GroupCardRow>, RunError> {
        Ok(self.runs.cards(run_id).await?)
    }

    /// Kandidat "Start together" untuk kartu `anchor` dari cache tugas (R1, R2).
    ///
    /// # Errors
    /// Kartu tidak ada di cache atau penyimpanan gagal.
    pub async fn group_candidates(&self, anchor: &str) -> Result<Vec<Candidate>, RunError> {
        let tasks: Vec<Task> = self
            .sync
            .cache
            .list()
            .await?
            .into_iter()
            .map(|c| c.task)
            .collect();
        let anchor_task = tasks
            .iter()
            .find(|t| t.record_id == anchor)
            .cloned()
            .ok_or(RunError::NotFound("task"))?;
        let mut busy = Vec::new();
        for t in &tasks {
            if self.runs.active_for(&t.record_id).await?.is_some() {
                busy.push(t.record_id.clone());
            }
        }
        let is_busy = |id: &str| busy.iter().any(|b| b == id).then_some(Unselectable::HasRun);
        Ok(group::candidates(
            &anchor_task,
            &tasks,
            self.sync.me(),
            &is_busy,
        ))
    }

    /// Memulai run grup (R1–R7): klaim semua kartu, satu worktree dari branch
    /// default, buku AC berlabel kartu, giliran pertama untuk kartu pertama,
    /// lalu semua kartu In Progress. Klaim yang sudah didapat dilepas bila
    /// langkah apa pun gagal.
    ///
    /// # Errors
    /// Kurang dari dua kartu, kartu punya run, gerbang, klaim, git, atau agent gagal.
    pub async fn start_group(
        self: &Arc<Self>,
        record_ids: &[String],
        target: &StartTarget,
        dev_note: Option<&str>,
        planning: &Planning,
        design: &[DesignRoot],
        progress: impl Fn(StartStep) + Send + Sync,
    ) -> Result<RunRecord, RunError> {
        if record_ids.len() < 2 {
            return Err(RunError::InvalidState("a group needs at least two cards"));
        }
        let _work = self.enter_work()?;
        for id in record_ids {
            if self.runs.active_for(id).await?.is_some() {
                return Err(RunError::InvalidState("a card already has an active run"));
            }
        }
        let mut plan = self.resolve(target).await?;
        plan.design = design.to_vec();
        self.start_group_plan(record_ids, &plan, dev_note, planning, progress)
            .await
    }

    /// Grup dari antrean VM (U12 plan grup): tanpa persetujuan plan (R6 VM),
    /// sumber desain item untuk seluruh grup, dan tanda Start now.
    pub(crate) async fn start_vm_group(
        self: &Arc<Self>,
        record_ids: &[String],
        target: &StartTarget,
        dev_note: Option<&str>,
        design: (&[DesignRoot], &[String]),
        start_now: bool,
    ) -> Result<RunRecord, RunError> {
        let _work = self.enter_work()?;
        let mut plan = self.resolve(target).await?;
        plan.design = design.0.to_vec();
        plan.design_notes = design.1.to_vec();
        plan.vm_start = crate::run_ports::VmStart::from_start_now(start_now);
        self.start_group_plan(record_ids, &plan, dev_note, &Planning::Skip, |_| {})
            .await
    }

    /// Grup Planned di VM (R24, KTD-G14 plan grup): worktree dari branch grup,
    /// run grup dengan kartu pertama aktif membawa plan yang disetujui, buku
    /// AC semua kartu dengan tanda Design dari laptop, lalu giliran
    /// implementasi kartu pertama. Kartu berikutnya tanpa persetujuan plan.
    pub(crate) async fn start_group_from_plan(
        self: &Arc<Self>,
        record_ids: &[String],
        target: &StartTarget,
        work: &PlannedWork,
        design: (&[DesignRoot], &[String]),
        start_now: bool,
    ) -> Result<RunRecord, RunError> {
        let _work = self.enter_work()?;
        let first = record_ids
            .first()
            .ok_or(RunError::InvalidState("a group needs at least two cards"))?;
        let input = self.load_group(record_ids).await?;
        let (mut run, restored) = self.restore_run(first, target, &work.branch).await?;
        run.phase = RunPhase::Implement;
        run.plan = Some(work.plan.clone());
        run.design = design.0.to_vec();
        run.design_notes = design.1.to_vec();
        run.vm_start = crate::run_ports::VmStart::from_start_now(start_now);
        let mut cards: Vec<GroupCardRow> = record_ids
            .iter()
            .map(|id| GroupCardRow::pending(id))
            .collect();
        cards[0].state = CardState::Active;
        let root = run.workspace.as_ref().map(|w| w.root.clone());
        let started = async {
            self.runs.create_group(&run, &cards, self.now()).await?;
            self.save_group_ledger(&run.id, &input.per_card).await;
            let mut rows = self.reviews.ac_rows(&run.id).await?;
            for row in &mut rows {
                if let Some(mark) = work.design.iter().find(|m| m.ac_id == row.ac_id) {
                    row.design = mark.design;
                    row.design_note.clone_from(&mark.note);
                }
            }
            self.reviews.save_ac_rows(&run.id, &rows).await?;
            for id in record_ids {
                let held = self
                    .sync
                    .fetch_one(id)
                    .await?
                    .is_some_and(|t| t.held_here(self.sync.identity()));
                if !held {
                    return Err(RunError::InvalidState(
                        "the claim moved while the group was starting",
                    ));
                }
            }
            let mut message = self.approved_plan_turn(&run, &work.plan).await?;
            message.push_str("\n\n");
            message.push_str(&flow_note(&input.tasks, 0, None));
            let req = self.resume_request(&mut run, message).await?;
            let summary = format!(
                "Started: implement the approved plan.\n\nGroup: {}",
                input
                    .tasks
                    .iter()
                    .map(key_of)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            self.spawn_turn(&mut run, req, Some(summary)).await
        };
        if let Err(e) = started.await {
            self.discard_members(&restored, &work.branch, root.as_deref())
                .await;
            self.close_quietly(&run.id).await;
            return Err(e);
        }
        self.load(&run.id).await
    }

    /// Klaim, worktree, dan giliran pertama grup untuk target yang sudah
    /// di-resolve (dipakai juga oleh antrean VM, U12 plan grup).
    pub(crate) async fn start_group_plan(
        self: &Arc<Self>,
        record_ids: &[String],
        plan: &Plan,
        dev_note: Option<&str>,
        planning: &Planning,
        progress: impl Fn(StartStep) + Send + Sync,
    ) -> Result<RunRecord, RunError> {
        progress(StartStep::Claiming);
        let mut claimed: Vec<&String> = Vec::with_capacity(record_ids.len());
        let mut result = Ok(());
        for id in record_ids {
            match self.sync.claim(id, false).await {
                Ok(_) => claimed.push(id),
                Err(e) => {
                    result = Err(RunError::from(e));
                    break;
                }
            }
        }
        let outcome = match result {
            Ok(()) => {
                self.prepare_group(record_ids, plan, dev_note, planning, &progress)
                    .await
            }
            Err(e) => Err(e),
        };
        if outcome.is_err() {
            for id in claimed {
                if let Err(e) = self.sync.release_claim(id).await {
                    tracing::warn!(record_id = %id, error = %e, "gagal melepas klaim setelah Start grup gagal");
                }
            }
        }
        outcome
    }

    /// Task dan AC tiap kartu, urut sesuai `record_ids`.
    async fn load_group(&self, record_ids: &[String]) -> Result<GroupInput, RunError> {
        let mut tasks = Vec::with_capacity(record_ids.len());
        let mut per_card = Vec::with_capacity(record_ids.len());
        for id in record_ids {
            let task = self.task_of(id).await?;
            let acs = self.sync.lark.acceptance_criteria(&task.ac_ids).await?;
            per_card.push((id.clone(), acs));
            tasks.push(task);
        }
        Ok(GroupInput { tasks, per_card })
    }

    /// Nama branch grup dari kunci kartu pertama dan terakhir serta judul epic
    /// (judul kartu pertama bila epic tidak terbaca).
    async fn group_branch(&self, tasks: &[Task]) -> String {
        let first = &tasks[0];
        let epic = match first.epic_ids.first() {
            Some(e) => self
                .sync
                .lark
                .epics(std::slice::from_ref(e))
                .await
                .ok()
                .and_then(|v| v.into_iter().next())
                .map(|e| e.name),
            None => None,
        };
        let last = &tasks[tasks.len() - 1];
        group::branch_name(
            first.key.as_deref(),
            last.key.as_deref(),
            &first.record_id,
            epic.as_deref().unwrap_or(&first.title),
        )
    }

    /// Pesan giliran pertama kartu `index` sesuai Planning; menyetel fase run.
    fn card_message(
        &self,
        run: &mut RunRecord,
        input: &GroupInput,
        index: usize,
        dev_note: Option<&str>,
        planning: &Planning,
    ) -> String {
        let task = &input.tasks[index];
        let note = flow_note(&input.tasks, index, dev_note);
        let acs = ac_lines(&input.per_card[index].1);
        match planning {
            Planning::Skip => self.prompts.task_message(task, &acs, Some(&note)),
            Planning::Plan { skill } => {
                run.phase = RunPhase::Plan;
                let msg = self
                    .prompts
                    .plan_message(task, &acs, Some(&note), skill.as_deref());
                self.with_design(run, msg)
            }
            Planning::Brainstorm { brainstorm, plan } => {
                run.phase = RunPhase::Brainstorm;
                run.plan_skill.clone_from(plan);
                let msg =
                    self.prompts
                        .brainstorm_message(task, &acs, Some(&note), brainstorm.as_deref());
                self.with_design(run, msg)
            }
        }
    }

    async fn prepare_group(
        self: &Arc<Self>,
        record_ids: &[String],
        plan: &Plan,
        dev_note: Option<&str>,
        planning: &Planning,
        progress: &(impl Fn(StartStep) + Send + Sync),
    ) -> Result<RunRecord, RunError> {
        let input = self.load_group(record_ids).await?;
        let branch = self.group_branch(&input.tasks).await;
        progress(StartStep::CreatingWorktree);
        let ws_group = plan.workspace.as_ref().map(|w| group_name(&w.id, &branch));
        let prepared = self
            .prepare_all(&plan.repos, &branch, ws_group.as_deref())
            .await?;
        let root = ws_group
            .as_ref()
            .and_then(|_| prepared[0].1.parent().map(std::path::Path::to_path_buf));
        let mut run = new_run(
            self.sync.ids.nonce(),
            self.sync.ids.nonce(),
            &input.tasks[0].record_id,
            &branch,
            plan,
            &prepared,
            root.as_ref(),
        );
        let mut cards: Vec<GroupCardRow> = record_ids
            .iter()
            .map(|id| GroupCardRow::pending(id))
            .collect();
        cards[0].state = CardState::Active;
        if let Err(e) = self.runs.create_group(&run, &cards, self.now()).await {
            self.discard_members(&prepared, &branch, root.as_deref())
                .await;
            return Err(e.into());
        }
        self.save_group_ledger(&run.id, &input.per_card).await;

        run.group_plan = !matches!(planning, Planning::Skip);
        if let Planning::Plan { skill } = planning {
            run.plan_skill.clone_from(skill);
        }
        let message = self.card_message(&mut run, &input, 0, dev_note, planning);
        let session = SessionRef::New(run.session_id.clone().unwrap_or_default());
        progress(StartStep::StartingAgent);
        let summary = format!(
            "{}\n\nGroup: {}",
            start_summary(planning, dev_note),
            input
                .tasks
                .iter()
                .map(key_of)
                .collect::<Vec<_>>()
                .join(", ")
        );
        let spawned = match self.agent_request(&mut run, session, message).await {
            Ok(req) => self.spawn_turn(&mut run, req, Some(summary)).await,
            Err(e) => Err(e),
        };
        if let Err(e) = spawned {
            self.discard_members(&prepared, &branch, root.as_deref())
                .await;
            self.close_quietly(&run.id).await;
            return Err(e);
        }
        progress(StartStep::MovingStatus);
        self.group_in_progress(record_ids, &branch, &prepared[0].0.id)
            .await?;
        Ok(run)
    }

    /// "Next card" (R5): commit lokal kartu aktif dengan kunci kartu di pesan
    /// commit, tandai selesai, lalu mulai kartu berikutnya, atau tunggu Push &
    /// PR bila semua kartu selesai.
    ///
    /// # Errors
    /// Bukan run grup, agent masih bekerja, plan belum disetujui, atau git/agent gagal.
    pub async fn next_card(self: &Arc<Self>, run_id: &str) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev || run.phase != RunPhase::Implement {
            return Err(RunError::InvalidState("the card is not finished yet"));
        }
        let mut cards = self.group_of(run_id).await?;
        self.finish_active_card(&mut run, &mut cards).await?;
        self.advance(&mut run, &mut cards, None).await
    }

    /// Commit lokal kartu aktif dengan kuncinya di pesan commit, lalu tandai
    /// selesai beserta plan-nya (KTD-G3).
    pub(crate) async fn finish_active_card(
        &self,
        run: &mut RunRecord,
        cards: &mut [GroupCardRow],
    ) -> Result<(), RunError> {
        let idx = cards
            .iter()
            .position(|c| c.state == CardState::Active)
            .ok_or(RunError::InvalidState("no active card"))?;
        let task = self.task_of(&cards[idx].record_id).await?;
        cards[idx].commits = self.commit_card(run, key_of(&task)).await?;
        cards[idx].state = CardState::Done;
        cards[idx].plan = run.plan.take();
        Ok(())
    }

    /// Melanjutkan grup setelah kartu dikeluarkan (R9): mulai kartu berikutnya.
    ///
    /// # Errors
    /// Bukan run grup, masih ada kartu aktif, agent bekerja, atau agent gagal.
    pub async fn continue_group(self: &Arc<Self>, run_id: &str) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        if run.state != RunState::AwaitingDev {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        let mut cards = self.group_of(run_id).await?;
        if group::active_card(&group_cards_domain(&cards)).is_some() {
            return Err(RunError::InvalidState("a card is still active"));
        }
        self.advance(&mut run, &mut cards, None).await
    }

    /// Mengeluarkan kartu yang belum dimulai (R10): klaim dilepas, dev diminta
    /// mengembalikannya ke To Do.
    ///
    /// # Errors
    /// Bukan run grup, kartu tidak ada atau sudah dimulai.
    pub async fn remove_card(&self, run_id: &str, record_id: &str) -> Result<(), RunError> {
        let mut run = self.load(run_id).await?;
        let mut cards = self.group_of(run_id).await?;
        let card = cards
            .iter_mut()
            .find(|c| c.record_id == record_id)
            .ok_or(RunError::NotFound("card"))?;
        if card.state != CardState::Pending {
            return Err(RunError::InvalidState(
                "only cards not started can be removed",
            ));
        }
        card.state = CardState::Removed(group::RemovedReason::ByDev);
        self.release_quietly(record_id).await;
        let key = self.key_or_id(record_id).await;
        run.note = Some(format!(
            "{key} was removed from the group; move it back to To Do in Lark."
        ));
        self.save_group(&mut run, &cards).await
    }

    /// Menghentikan grup (R10): commit kartu yang selesai di-push tanpa PR,
    /// klaim semua kartu dilepas, run ditutup.
    ///
    /// # Errors
    /// Bukan run grup, agent masih bekerja, atau git gagal.
    pub async fn stop_group(self: &Arc<Self>, run_id: &str) -> Result<(), RunError> {
        let run = self.load(run_id).await?;
        if run.state.is_working() {
            return Err(RunError::InvalidState(run.state.as_str()));
        }
        self.group_of(run_id).await?;
        let branch = run
            .branch
            .clone()
            .ok_or(RunError::InvalidState("no branch"))?;
        for (m, changes) in self.member_changes(&run).await? {
            if changes.unpushed > 0 {
                self.git.push(&m.repo, &m.worktree, &branch).await?;
            }
        }
        self.close_run(run_id).await
    }

    /// Kartu run grup; `InvalidState` untuk run tunggal.
    pub(crate) async fn group_of(&self, run_id: &str) -> Result<Vec<GroupCardRow>, RunError> {
        let cards = self.runs.cards(run_id).await?;
        if cards.is_empty() {
            return Err(RunError::InvalidState("not a group run"));
        }
        Ok(cards)
    }

    /// Commit lokal perubahan setiap repo run untuk kartu `key` (KTD-G3).
    async fn commit_card(&self, run: &RunRecord, key: &str) -> Result<Vec<CardCommit>, RunError> {
        let mut out = Vec::new();
        for (m, changes) in self.member_changes(run).await? {
            if !changes.dirty {
                continue;
            }
            let diff: String = changes.diff.chars().take(COMMIT_DIFF_MAX).collect();
            let summary = self.agent.commit_message(&m.worktree, &diff).await?;
            let message = format!("{key}: {summary}");
            if let Some(sha) = self.git.commit_all(&m.worktree, &message).await? {
                self.runs
                    .append(
                        &run.id,
                        &ChatItem::System(format!("Commit: {message}")),
                        self.now(),
                    )
                    .await?;
                out.push(CardCommit {
                    repo_id: m.repo.id.clone(),
                    sha,
                });
            }
        }
        Ok(out)
    }

    /// Memulai kartu berikutnya setelah memeriksa ulang kartu itu di Lark (R9),
    /// atau menunggu Push & PR bila tidak ada kartu lagi. `dev_note` ikut ke
    /// pesan kartu berikutnya (jawaban dev saat grup VM diantre ulang).
    pub(crate) async fn advance(
        self: &Arc<Self>,
        run: &mut RunRecord,
        cards: &mut [GroupCardRow],
        dev_note: Option<&str>,
    ) -> Result<(), RunError> {
        let Some(next) = cards.iter().position(|c| c.state == CardState::Pending) else {
            run.note = Some("All cards are done. Review the AC, then Push & PR.".to_owned());
            return self.save_group(run, cards).await;
        };
        let id = cards[next].record_id.clone();
        let fresh = self.sync.lark.fetch(std::slice::from_ref(&id)).await?;
        let task = fresh.into_iter().find(|t| t.record_id == id);
        if !group::still_ours(task.as_ref(), self.sync.me()) {
            cards[next].state = CardState::Removed(group::RemovedReason::ChangedInLark);
            self.release_quietly(&id).await;
            let key = task.as_ref().map_or(id.clone(), |t| key_of(t).to_owned());
            if self.is_vm() {
                // VM tidak menunggu dev: grup diparkir (R9, KTD-G13).
                self.save_group(run, cards).await?;
                let note = format!(
                    "{key} changed in Lark and was removed from the group. Queue the group again to continue without it."
                );
                return self.hold(run, HoldReason::CardRemoved, Some(note)).await;
            }
            run.note = Some(format!(
                "{key} changed in Lark and was removed from the group. Continue without it, or stop the group."
            ));
            return self.save_group(run, cards).await;
        }
        cards[next].state = CardState::Active;
        let active: Vec<String> = cards
            .iter()
            .filter(|c| !matches!(c.state, CardState::Removed(_)))
            .map(|c| c.record_id.clone())
            .collect();
        let input = self.load_group(&active).await?;
        let index = active.iter().position(|a| *a == id).unwrap_or_default();
        let planning = if run.group_plan {
            Planning::Plan {
                skill: run.plan_skill.clone(),
            }
        } else {
            Planning::Skip
        };
        run.note = None;
        run.phase = RunPhase::Implement;
        let message = self.card_message(run, &input, index, dev_note, &planning);
        self.save_group(run, cards).await?;
        let req = self.resume_request(run, message).await?;
        let summary = format!("Next card: {}", key_of(&input.tasks[index]));
        self.spawn_turn(run, req, Some(summary)).await
    }

    /// Menyimpan kartu (dengan pindah kartu utama bila perlu) lalu run.
    pub(crate) async fn save_group(
        &self,
        run: &mut RunRecord,
        cards: &[GroupCardRow],
    ) -> Result<(), RunError> {
        let primary = new_primary(run, cards);
        self.runs
            .save_cards(&run.id, cards, primary.as_deref())
            .await?;
        if let Some(p) = primary {
            run.record_id = p;
        }
        let expected = run.generation;
        run.generation += 1;
        self.runs.save(run, expected, self.now()).await?;
        Ok(())
    }

    /// Melepas klaim tanpa menggagalkan pemanggil.
    pub(super) async fn release_quietly(&self, record_id: &str) {
        if let Err(e) = self.sync.release_claim(record_id).await {
            tracing::warn!(record_id, error = %e, "gagal melepas klaim kartu grup");
        }
    }

    /// Kunci kartu dari cache, atau ID record.
    pub(super) async fn key_or_id(&self, record_id: &str) -> String {
        self.task_of(record_id)
            .await
            .ok()
            .and_then(|t| t.key)
            .unwrap_or_else(|| record_id.to_owned())
    }

    /// Buku AC awal grup; kegagalan hanya dicatat (refresh membangunnya ulang).
    async fn save_group_ledger(&self, run_id: &str, per_card: &[(String, Vec<AcItem>)]) {
        match self
            .reviews
            .save_ac_rows(run_id, &group_rows(per_card))
            .await
        {
            Ok(()) => self.mark_synced(run_id),
            Err(e) => tracing::warn!(run_id, error = %e, "buku AC grup tidak tersimpan"),
        }
    }

    /// Menutup run tanpa menggagalkan pemanggil.
    async fn close_quietly(&self, run_id: &str) {
        if let Err(err) = self.runs.close(run_id, self.now()).await {
            tracing::warn!(run_id, error = %err, "gagal menutup run grup");
        }
    }

    /// Semua kartu grup ke In Progress dengan branch grup (status terakhir, KTD-9).
    async fn group_in_progress(
        &self,
        record_ids: &[String],
        branch: &str,
        repo_id: &str,
    ) -> Result<(), RunError> {
        for id in record_ids {
            let op = WriteOp::Transition {
                from: TaskStatus::ToDo,
                to: TaskStatus::InProgress,
                fields: vec![
                    AppField::Branch(branch.to_owned()),
                    AppField::RepoId(repo_id.to_owned()),
                ],
            };
            self.sync.request_write(id, &op).await?;
        }
        Ok(())
    }
}

/// Kartu tersimpan sebagai kartu domain.
pub(super) fn group_cards_domain(cards: &[GroupCardRow]) -> Vec<group::GroupCard> {
    cards
        .iter()
        .map(|c| group::GroupCard {
            record_id: c.record_id.clone(),
            state: c.state.clone(),
            changed_in_lark: c.changed_in_lark,
            kept_reason: c.kept_reason.clone(),
        })
        .collect()
}

/// Kartu utama baru bila kartu utama run sudah dikeluarkan.
fn new_primary(run: &RunRecord, cards: &[GroupCardRow]) -> Option<String> {
    let primary_removed = cards
        .iter()
        .any(|c| c.record_id == run.record_id && matches!(c.state, CardState::Removed(_)));
    if !primary_removed {
        return None;
    }
    cards
        .iter()
        .find(|c| !matches!(c.state, CardState::Removed(_)))
        .map(|c| c.record_id.clone())
}

/// Task dan AC kartu grup.
struct GroupInput {
    tasks: Vec<Task>,
    per_card: Vec<(String, Vec<AcItem>)>,
}

/// Run baru untuk worktree yang sudah disiapkan (repo utama pertama).
fn new_run(
    id: String,
    session: String,
    record_id: &str,
    branch: &str,
    plan: &Plan,
    prepared: &[(crate::run_ports::RepoInfo, std::path::PathBuf)],
    root: Option<&std::path::PathBuf>,
) -> RunRecord {
    let mut run = RunRecord::new(id, prepared[0].0.id.clone(), record_id.to_owned());
    run.branch = Some(branch.to_owned());
    run.worktree = Some(prepared[0].1.clone());
    run.session_id = Some(session);
    if let (Some(ws), Some(root)) = (&plan.workspace, root) {
        run.workspace = Some(RunWorkspace {
            id: ws.id.clone(),
            root: root.clone(),
        });
        run.extra = prepared[1..]
            .iter()
            .map(|(repo, wt)| RunMember {
                repo_id: repo.id.clone(),
                worktree: wt.clone(),
                pr_url: None,
            })
            .collect();
    }
    run.design.clone_from(&plan.design);
    run.design_notes.clone_from(&plan.design_notes);
    run.vm_start = plan.vm_start;
    run
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catatan_flow_menyebut_semua_kartu_dan_kartu_aktif() {
        let mut a = Task::new("a", TaskStatus::ToDo);
        a.key = Some("PGS-1".to_owned());
        "Satu".clone_into(&mut a.title);
        let mut b = Task::new("b", TaskStatus::ToDo);
        b.key = Some("PGS-2".to_owned());
        "Dua".clone_into(&mut b.title);
        let note = flow_note(&[a, b], 1, Some(" catatan dev "));
        assert!(note.contains("card 2 of 2"));
        assert!(note.contains("PGS-1 Satu; PGS-2 Dua"));
        assert!(note.contains("Work only on PGS-2"));
        assert!(note.ends_with("catatan dev"));
    }
}
