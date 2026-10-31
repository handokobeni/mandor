//! Start task (KTD-9): gerbang → klaim → worktree → agent → status terakhir.

use std::path::PathBuf;
use std::sync::Arc;

use mandor_domain::run::branch_name;
use mandor_domain::{AppField, TaskStatus, WriteOp};

use super::members::group_name;
use super::{RunError, RunService};
use crate::ports::{Deps, Ids, LarkTasks};
use crate::review::DesignRoot;
use crate::run_ports::{
    GitPort, Prompts, RepoInfo, Repos, RunMember, RunPhase, RunRecord, RunStore, RunWorkspace,
    SessionRef, WorkspaceInfo,
};

/// Yang dikerjakan sebuah Start: satu repo, atau beberapa repo dari satu workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartTarget {
    Repo(String),
    Workspace { id: String, repo_ids: Vec<String> },
}

impl StartTarget {
    /// Repo yang dikerjakan target ini.
    #[must_use]
    pub fn repo_ids(&self) -> Vec<String> {
        match self {
            Self::Repo(id) => vec![id.clone()],
            Self::Workspace { repo_ids, .. } => repo_ids.clone(),
        }
    }
}

/// Planning sebelum implementasi, dipilih dev saat Start.
#[derive(Debug, Clone, PartialEq, Eq)]
/// Skill `None` berarti prompt bawaan mandor; selain itu skill plugin Claude,
/// mis. `compound-engineering:ce-plan`.
pub enum Planning {
    /// Langsung implementasi.
    Skip,
    /// Plan, lalu menunggu persetujuan.
    Plan { skill: Option<String> },
    /// Brainstorm (tanya jawab lewat chat), lalu plan, lalu menunggu persetujuan.
    Brainstorm {
        brainstorm: Option<String>,
        plan: Option<String>,
    },
}

/// Teks AC yang masih berlaku, dengan label, untuk pesan agent.
pub(super) fn ac_texts(items: &[crate::AcItem]) -> Vec<String> {
    items
        .iter()
        .filter(|ac| !ac.dropped)
        .map(|ac| match &ac.label {
            Some(l) => format!("{l}: {}", ac.text),
            None => ac.text.clone(),
        })
        .collect()
}

/// Ringkasan Start untuk chat dev. Prompt lengkap (dengan blok data Lark)
/// hanya dikirim ke agent.
pub(super) fn start_summary(planning: &Planning, dev_note: Option<&str>) -> String {
    let skill = |s: &Option<String>| s.clone().unwrap_or_else(|| "Mandor built-in".to_owned());
    let mut out = match planning {
        Planning::Skip => "Started: implement the task.".to_owned(),
        Planning::Plan { skill: s } => format!("Started: plan with {}.", skill(s)),
        Planning::Brainstorm { brainstorm, plan } => format!(
            "Started: brainstorm with {}, then plan with {}.",
            skill(brainstorm),
            skill(plan)
        ),
    };
    if let Some(note) = dev_note.map(str::trim).filter(|n| !n.is_empty()) {
        out.push_str("\n\nNote: ");
        out.push_str(note);
    }
    out
}

/// Target yang sudah divalidasi: repo dalam urutan workspace, repo utama pertama.
pub(crate) struct Plan {
    pub(super) repos: Vec<RepoInfo>,
    pub(super) workspace: Option<WorkspaceInfo>,
    pub(super) design: Vec<DesignRoot>,
    pub(super) design_notes: Vec<String>,
}

/// Langkah Start yang dilaporkan ke client (dialog Start).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartStep {
    Claiming,
    CreatingWorktree,
    StartingAgent,
    MovingStatus,
}

impl StartStep {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claiming => "claiming",
            Self::CreatingWorktree => "creating_worktree",
            Self::StartingAgent => "starting_agent",
            Self::MovingStatus => "moving_status",
        }
    }
}

impl<D: Deps> RunService<D> {
    /// [`Self::start_with_progress`] satu repo tanpa laporan langkah.
    ///
    /// # Errors
    /// Gerbang, klaim, git, atau agent gagal.
    pub async fn start(
        self: &Arc<Self>,
        record_id: &str,
        repo_id: &str,
        dev_note: Option<&str>,
    ) -> Result<RunRecord, RunError> {
        let target = StartTarget::Repo(repo_id.to_owned());
        self.start_with_progress(record_id, &target, dev_note, |_| {})
            .await
    }

    /// Memulai task: klaim pemilik, worktree dari base terbaru untuk setiap repo
    /// target, agent dengan judul, deskripsi, dan AC, lalu In Progress + nama
    /// branch lewat outbox. Kegagalan sebelum tulis status me-rollback worktree,
    /// branch, folder run, dan klaim.
    ///
    /// # Errors
    /// Target tidak valid, gerbang, klaim, git, atau agent gagal.
    pub async fn start_with_progress(
        self: &Arc<Self>,
        record_id: &str,
        target: &StartTarget,
        dev_note: Option<&str>,
        progress: impl Fn(StartStep) + Send + Sync,
    ) -> Result<RunRecord, RunError> {
        self.start_planned(record_id, target, dev_note, &Planning::Skip, &[], progress)
            .await
    }

    /// [`Self::start_with_progress`] dengan fase plan: selain [`Planning::Skip`],
    /// giliran pertama hanya membaca kode dan menyusun plan, lalu run menunggu
    /// persetujuan dev ([`Self::approve_plan`]). `design`: sumber desain yang
    /// sudah diperiksa adapter; disimpan di run untuk semua giliran.
    ///
    /// # Errors
    /// Target tidak valid, gerbang, klaim, git, atau agent gagal.
    pub async fn start_planned(
        self: &Arc<Self>,
        record_id: &str,
        target: &StartTarget,
        dev_note: Option<&str>,
        planning: &Planning,
        design: &[DesignRoot],
        progress: impl Fn(StartStep) + Send + Sync,
    ) -> Result<RunRecord, RunError> {
        let mut plan = self.resolve(target).await?;
        plan.design = design.to_vec();
        self.start_resolved(record_id, &plan, dev_note, planning, progress)
            .await
    }

    /// Kartu No plan dari antrean VM: tanpa plan, dengan sumber desain dan
    /// catatan desain untuk laporan pagi (R12 plan 2026-10-30-001).
    pub(crate) async fn start_queued_card(
        self: &Arc<Self>,
        record_id: &str,
        target: &StartTarget,
        design: &[DesignRoot],
        design_notes: &[String],
    ) -> Result<RunRecord, RunError> {
        let mut plan = self.resolve(target).await?;
        plan.design = design.to_vec();
        plan.design_notes = design_notes.to_vec();
        self.start_resolved(record_id, &plan, None, &Planning::Skip, |_| {})
            .await
    }

    async fn start_resolved(
        self: &Arc<Self>,
        record_id: &str,
        plan: &Plan,
        dev_note: Option<&str>,
        planning: &Planning,
        progress: impl Fn(StartStep) + Send + Sync,
    ) -> Result<RunRecord, RunError> {
        let _work = self.enter_work()?;
        // Gerbang dan klaim dari baca segar; kalah klaim berhenti sebelum worktree.
        progress(StartStep::Claiming);
        self.sync.claim(record_id, false).await?;

        match self
            .prepare_and_spawn(record_id, plan, dev_note, planning, &progress)
            .await
        {
            Ok(run) => Ok(run),
            Err(err) => {
                if let Err(e) = self.sync.release_claim(record_id).await {
                    tracing::warn!(record_id, error = %e, "gagal melepas klaim setelah Start gagal");
                }
                Err(err)
            }
        }
    }

    pub(crate) async fn resolve(&self, target: &StartTarget) -> Result<Plan, RunError> {
        let (ids, workspace) = match target {
            StartTarget::Repo(id) => (vec![id.clone()], None),
            StartTarget::Workspace { id, repo_ids } => {
                let ws = self
                    .repos
                    .workspace(id)
                    .await?
                    .ok_or(RunError::NotFound("workspace"))?;
                if repo_ids.iter().any(|r| !ws.repo_ids.contains(r)) {
                    return Err(RunError::InvalidState("repo is not in the workspace"));
                }
                let ordered: Vec<String> = ws
                    .repo_ids
                    .iter()
                    .filter(|r| repo_ids.contains(r))
                    .cloned()
                    .collect();
                if ordered.is_empty() {
                    return Err(RunError::InvalidState("no repo selected"));
                }
                (ordered, Some(ws))
            }
        };
        let mut repos = Vec::with_capacity(ids.len());
        for id in &ids {
            repos.push(
                self.repos
                    .repo(id)
                    .await?
                    .ok_or(RunError::NotFound("repo"))?,
            );
        }
        Ok(Plan {
            repos,
            workspace,
            design: Vec::new(),
            design_notes: Vec::new(),
        })
    }

    /// Pesan brainstorm/plan ditambah instruksi kolom Design bila run punya
    /// sumber desain.
    pub(crate) fn with_design(&self, run: &RunRecord, message: String) -> String {
        if run.design.is_empty() {
            return message;
        }
        format!("{message}\n\n{}", self.prompts.design_message(&run.design))
    }

    async fn prepare_and_spawn(
        self: &Arc<Self>,
        record_id: &str,
        plan: &Plan,
        dev_note: Option<&str>,
        planning: &Planning,
        progress: &(impl Fn(StartStep) + Send + Sync),
    ) -> Result<RunRecord, RunError> {
        let task = self.task_of(record_id).await?;
        let ac_items = self.sync.lark.acceptance_criteria(&task.ac_ids).await?;
        let acs = ac_texts(&ac_items);
        let branch = branch_name(task.key.as_deref(), &task.record_id, &task.title);
        progress(StartStep::CreatingWorktree);
        let group = plan.workspace.as_ref().map(|w| group_name(&w.id, &branch));
        let prepared = self
            .prepare_all(&plan.repos, &branch, group.as_deref())
            .await?;
        let root = group
            .as_ref()
            .and_then(|_| prepared[0].1.parent().map(std::path::Path::to_path_buf));

        let primary = &prepared[0];
        let mut run = RunRecord::new(
            self.sync.ids.nonce(),
            primary.0.id.clone(),
            record_id.to_owned(),
        );
        run.branch = Some(branch.clone());
        run.worktree = Some(primary.1.clone());
        run.session_id = Some(self.sync.ids.nonce());
        if let (Some(ws), Some(root)) = (&plan.workspace, &root) {
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
        let now = self.now();
        if let Err(e) = self.runs.create(&run, now).await {
            self.discard_members(&prepared, &branch, root.as_deref())
                .await;
            return Err(e.into());
        }
        self.init_ledger(&run.id, &ac_items).await;

        let message = match planning {
            Planning::Skip => {
                let msg = self.prompts.task_message(&task, &acs, dev_note);
                // Laptop: desain hanya untuk reviewer, seperti sebelumnya.
                if self.is_vm() {
                    self.with_design_reference(&run, msg)
                } else {
                    msg
                }
            }
            Planning::Plan { skill } => {
                run.phase = RunPhase::Plan;
                let msg = self
                    .prompts
                    .plan_message(&task, &acs, dev_note, skill.as_deref());
                self.with_design(&run, msg)
            }
            Planning::Brainstorm { brainstorm, plan } => {
                run.phase = RunPhase::Brainstorm;
                run.plan_skill.clone_from(plan);
                let msg =
                    self.prompts
                        .brainstorm_message(&task, &acs, dev_note, brainstorm.as_deref());
                self.with_design(&run, msg)
            }
        };
        let session = SessionRef::New(run.session_id.clone().unwrap_or_default());
        progress(StartStep::StartingAgent);
        let spawned = match self.agent_request(&mut run, session, message.clone()).await {
            Ok(req) => {
                let summary = start_summary(planning, dev_note);
                self.spawn_turn(&mut run, req, Some(summary)).await
            }
            Err(e) => Err(e),
        };
        if let Err(e) = spawned {
            self.discard_members(&prepared, &branch, root.as_deref())
                .await;
            if let Err(err) = self.runs.close(&run.id, self.now()).await {
                tracing::warn!(run_id = %run.id, error = %err, "gagal menutup run setelah spawn gagal");
            }
            return Err(e);
        }

        // Status Lark ditulis paling akhir (KTD-9).
        progress(StartStep::MovingStatus);
        let op = WriteOp::Transition {
            from: TaskStatus::ToDo,
            to: TaskStatus::InProgress,
            fields: vec![
                AppField::Branch(branch),
                AppField::RepoId(primary.0.id.clone()),
            ],
        };
        self.sync.request_write(record_id, &op).await?;
        Ok(run)
    }

    /// Worktree untuk setiap repo; kegagalan menghapus yang sudah dibuat.
    pub(super) async fn prepare_all(
        &self,
        repos: &[RepoInfo],
        branch: &str,
        group: Option<&str>,
    ) -> Result<Vec<(RepoInfo, PathBuf)>, RunError> {
        let mut prepared: Vec<(RepoInfo, PathBuf)> = Vec::with_capacity(repos.len());
        for repo in repos {
            match self.git.prepare_worktree(repo, branch, group).await {
                Ok(wt) => prepared.push((repo.clone(), wt)),
                Err(e) => {
                    let root = group
                        .and_then(|_| prepared.first())
                        .and_then(|(_, wt)| wt.parent().map(std::path::Path::to_path_buf));
                    self.discard_members(&prepared, branch, root.as_deref())
                        .await;
                    return Err(e.into());
                }
            }
        }
        Ok(prepared)
    }
}
