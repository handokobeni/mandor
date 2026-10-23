//! Run grup kartu satu flow (plan `2026-10-23-004`): kartu dikerjakan
//! berurutan di satu branch dengan satu PR di akhir. Aturan murni ada di
//! `mandor_domain::group`.

use std::sync::Arc;

use mandor_domain::group::{self, Candidate, CardState, Unselectable};
use mandor_domain::{AcRow, AppField, Task, TaskStatus, WriteOp};

use super::members::group_name;
use super::start::{Plan, start_summary};
use super::{RunError, RunService};
use crate::ports::{AcItem, Deps, Ids, LarkTasks, TaskCache};
use crate::review::DesignRoot;
use crate::run_ports::{
    GroupCardRow, LedgerStore, Prompts, RunMember, RunPhase, RunRecord, RunStore, RunWorkspace,
    SessionRef,
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
fn key_of(t: &Task) -> &str {
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
                self.prepare_group(record_ids, &plan, dev_note, planning, &progress)
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
