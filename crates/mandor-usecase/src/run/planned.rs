//! Kartu Planned yang dipulihkan di sesi baru (plan 2026-10-30-001, KTD-3,
//! KTD-12): buku AC dari Lark ditimpa kolom Design paket, dan pesan pertama
//! membawa plan yang disetujui.

use std::path::PathBuf;
use std::sync::Arc;

use mandor_domain::planned::{DesignMark, PlannedWork};
use mandor_domain::{Design, RunState, Task};

use super::members::group_name;
use super::start::ac_texts;
use super::{RunError, RunService, StartTarget};
use crate::ports::{Deps, Ids, LarkTasks};
use crate::review::DesignRoot;
use crate::run_ports::{
    GitPort, LedgerStore, Prompts, RepoInfo, RunMember, RunPhase, RunRecord, RunStore, RunWorkspace,
};

/// Paket plan untuk Take over ke laptop: sumber desain laptop yang masih ada
/// sudah di-resolve oleh adapter; yang hilang hanya dicatat namanya.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakeoverPlan {
    pub work: PlannedWork,
    pub design: Vec<DesignRoot>,
    pub missing_design: Vec<String>,
}

/// Ringkasan chat saat VM memulai kartu Planned.
const START_SUMMARY: &str = "Started: implement the approved plan.";

impl<D: Deps> RunService<D> {
    /// Run baru (belum disimpan) dari branch kartu yang sudah di-push: worktree
    /// tiap repo target dipulihkan; run workspace mendapat folder run dan
    /// anggotanya. Mengembalikan run beserta worktree yang dipulihkan.
    pub(super) async fn restore_run(
        &self,
        record_id: &str,
        target: &StartTarget,
        branch: &str,
    ) -> Result<(RunRecord, Vec<(RepoInfo, PathBuf)>), RunError> {
        let plan = self.resolve(target).await?;
        let group = plan.workspace.as_ref().map(|w| group_name(&w.id, branch));
        let mut restored = Vec::with_capacity(plan.repos.len());
        for repo in &plan.repos {
            let wt = self
                .git
                .restore_worktree(repo, branch, group.as_deref())
                .await?;
            restored.push((repo.clone(), wt));
        }
        let Some((primary_repo, primary_wt)) = restored.first().cloned() else {
            return Err(RunError::InvalidState("no repo selected"));
        };
        let mut run = RunRecord::new(self.sync.ids.nonce(), primary_repo.id, record_id.to_owned());
        run.state = RunState::AwaitingDev;
        run.branch = Some(branch.to_owned());
        if let (Some(ws), Some(root)) = (&plan.workspace, primary_wt.parent()) {
            run.workspace = Some(RunWorkspace {
                id: ws.id.clone(),
                root: root.to_path_buf(),
            });
            run.extra = restored[1..]
                .iter()
                .map(|(repo, wt)| RunMember {
                    repo_id: repo.id.clone(),
                    worktree: wt.clone(),
                    pr_url: None,
                })
                .collect();
        }
        run.worktree = Some(primary_wt);
        run.session_id = Some(self.sync.ids.nonce());
        Ok((run, restored))
    }

    /// Memulai kartu Planned di VM (KTD-3): worktree dari branch yang di-push,
    /// run fase implementasi, buku AC dengan kolom Design dari paket, lalu
    /// sesi baru dengan plan yang disetujui. Klaim dicek ulang tepat sebelum
    /// agent dijalankan; kegagalan membuang worktree tanpa melepas klaim.
    ///
    /// # Errors
    /// Target tidak valid, git, klaim berpindah, atau agent gagal.
    pub(crate) async fn start_from_plan(
        self: &Arc<Self>,
        record_id: &str,
        target: &StartTarget,
        work: &PlannedWork,
        design: &[DesignRoot],
        design_notes: &[String],
    ) -> Result<RunRecord, RunError> {
        let _work = self.enter_work()?;
        let task = self.task_of(record_id).await?;
        let (mut run, restored) = self.restore_run(record_id, target, &work.branch).await?;
        run.phase = RunPhase::Implement;
        run.plan = Some(work.plan.clone());
        run.design = design.to_vec();
        run.design_notes = design_notes.to_vec();
        run.pr_url.clone_from(&task.pr_url);
        let root = run.workspace.as_ref().map(|w| w.root.clone());
        let started = async {
            self.runs.create(&run, self.now()).await?;
            self.seed_planned_ledger(&run.id, &task, &work.design)
                .await?;
            let held = self
                .sync
                .fetch_one(record_id)
                .await?
                .is_some_and(|t| t.held_here(self.sync.identity()));
            if !held {
                return Err(RunError::InvalidState(
                    "the claim moved while the card was starting",
                ));
            }
            let message = self.approved_plan_turn(&run, &work.plan).await?;
            let req = self.resume_request(&mut run, message).await?;
            self.spawn_turn(&mut run, req, Some(START_SUMMARY.to_owned()))
                .await
        };
        if let Err(e) = started.await {
            self.discard_members(&restored, &work.branch, root.as_deref())
                .await;
            if let Err(err) = self.runs.close(&run.id, self.now()).await {
                tracing::warn!(run_id = %run.id, error = %err, "gagal menutup run setelah mulai gagal");
            }
            return Err(e);
        }
        self.load(&run.id).await
    }

    /// Run Planned yang terputus sebelum pesan pertamanya sampai: kirim ulang
    /// plan ke sesi baru, bukan pesan lanjutan umum.
    pub(crate) async fn resend_approved_plan(
        self: &Arc<Self>,
        run_id: &str,
    ) -> Result<(), RunError> {
        let _work = self.enter_work()?;
        let mut run = self.load(run_id).await?;
        let plan = run
            .plan
            .clone()
            .ok_or(RunError::InvalidState("no plan to implement"))?;
        let message = self.approved_plan_turn(&run, &plan).await?;
        let req = self.resume_request(&mut run, message).await?;
        self.spawn_turn(&mut run, req, Some(START_SUMMARY.to_owned()))
            .await
    }

    /// Pesan implementasi ditambah sumber desain run sebagai acuan, bila ada.
    pub(crate) fn with_design_reference(&self, run: &RunRecord, message: String) -> String {
        if run.design.is_empty() {
            return message;
        }
        format!(
            "{message}\n\n{}",
            self.prompts.design_reference_message(&run.design)
        )
    }

    /// Mengisi buku AC run dari Lark lalu menimpa kolom Design dengan hasil
    /// tahap plan. Baris paket untuk AC yang sudah tidak ada diabaikan.
    pub(super) async fn seed_planned_ledger(
        &self,
        run_id: &str,
        task: &Task,
        marks: &[DesignMark],
    ) -> Result<(), RunError> {
        let acs = self.sync.lark.acceptance_criteria(&task.ac_ids).await?;
        self.init_ledger(run_id, &acs).await;
        let mut rows = self.reviews.ac_rows(run_id).await?;
        for row in &mut rows {
            if let Some(mark) = marks.iter().find(|m| m.ac_id == row.ac_id) {
                row.design = mark.design;
                row.design_note.clone_from(&mark.note);
            }
        }
        self.reviews.save_ac_rows(run_id, &rows).await?;
        Ok(())
    }

    /// Pesan pertama sesi baru untuk plan yang disetujui di sesi lain: tugas,
    /// AC, plan, AC Follow design, dan sumber desain run bila ada.
    pub(super) async fn approved_plan_turn(
        &self,
        run: &RunRecord,
        plan: &str,
    ) -> Result<String, RunError> {
        let task = self.task_of(&run.record_id).await?;
        let acs = ac_texts(&self.sync.lark.acceptance_criteria(&task.ac_ids).await?);
        let followed: Vec<String> = self
            .reviews
            .ac_rows(&run.id)
            .await?
            .iter()
            .filter(|r| r.design == Design::FollowDesign)
            .map(|r| format!("{}: {}", r.display_label(), r.text))
            .collect();
        let message = self
            .prompts
            .approved_plan_message(&task, &acs, plan, &followed);
        Ok(self.with_design_reference(run, message))
    }
}
