//! Fake port orkestrasi run: git, agent, run store, notifier, spawner, repo.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mandor_domain::AgentEvent;
use mandor_usecase::PortError;
use mandor_usecase::run_ports::{
    AgentPort, AgentRequest, AgentRunHandle, ChatItem, GitPort, GroupCardRow, Journal, Notice,
    Notifier, PrContext, PrRef, RepoInfo, Repos, RunRecord, RunStore, Spawner, Stopper,
    WorkChanges, WorkspaceInfo,
};
use tokio::sync::mpsc;

#[derive(Default)]
pub struct GitState {
    pub prepared: Vec<String>,
    pub restored: Vec<String>,
    pub discarded: Vec<String>,
    pub reverted: Vec<String>,
    pub removed_roots: Vec<PathBuf>,
    pub pushed_wts: Vec<PathBuf>,
    /// Repo yang gagal dibuatkan PR (uji kegagalan sebagian).
    pub fail_create_pr: Vec<String>,
    /// Repo yang gagal dibuat worktree-nya (uji rollback).
    pub fail_prepare: Vec<String>,
    pub worktrees: HashSet<PathBuf>,
    /// Diff branch per worktree; default `+diff pr`.
    pub branch_diffs: HashMap<PathBuf, String>,
    pub changes: HashMap<PathBuf, WorkChanges>,
    /// Berkas baru yang belum dilacak per worktree.
    pub untracked: HashMap<PathBuf, Vec<String>>,
    pub commits: Vec<(PathBuf, String)>,
    pub pushes: Vec<String>,
    pub prs: HashMap<String, String>,
    /// Commit yang di-revert (run grup).
    pub reverts: Vec<String>,
    /// Commit yang revert-nya konflik.
    pub revert_conflicts: Vec<String>,
    /// URL PR yang sudah di-merge.
    pub merged_prs: Vec<String>,
    pub created_prs: Vec<(String, String)>,
    /// PR yang dibuat sebagai draft (daemon VM).
    pub drafts: Vec<String>,
    /// PR yang ditandai siap (`gh pr ready`).
    pub ready: Vec<String>,
    pub fail_ready: bool,
    pub fail_push: bool,
    pub pr_bodies: Vec<String>,
    /// Badan PR saat ini per URL, seperti yang dikembalikan `gh pr view`.
    pub bodies: HashMap<String, String>,
    /// Jumlah `gh pr edit --body` yang dijalankan.
    pub body_edits: usize,
    /// Tree id snapshot per worktree; default `tree0`.
    pub trees: HashMap<PathBuf, String>,
    pub snapshot_fails: bool,
    /// Berkas berubah yang dikembalikan `changed_between` bila tree berbeda.
    pub diff_files: Vec<String>,
    /// Pasangan `(from, to)` yang diminta ke `changed_between`.
    pub calls_between: Arc<Mutex<Vec<(String, String)>>>,
}

/// Kunci PR palsu: repo dan branch (run workspace memakai branch yang sama).
pub fn pr_key(r: &RepoInfo, branch: &str) -> String {
    format!("{}:{branch}", r.id)
}

/// `/wt/<branch>` untuk satu repo, `/wt/<group>/<repo>` untuk run workspace.
fn fake_worktree(repo: &RepoInfo, branch: &str, group: Option<&str>) -> PathBuf {
    match group {
        Some(g) => PathBuf::from(format!("/wt/{g}/{}", repo.id)),
        None => PathBuf::from(format!("/wt/{branch}")),
    }
}

#[derive(Clone, Default)]
pub struct FakeGit {
    pub state: Arc<Mutex<GitState>>,
}

impl FakeGit {
    pub fn set_changes(&self, wt: &Path, c: WorkChanges) {
        self.state
            .lock()
            .unwrap()
            .changes
            .insert(wt.to_path_buf(), c);
    }
    pub fn st(&self) -> std::sync::MutexGuard<'_, GitState> {
        self.state.lock().unwrap()
    }
}

impl GitPort for FakeGit {
    async fn prepare_worktree(
        &self,
        repo: &RepoInfo,
        branch: &str,
        group: Option<&str>,
    ) -> Result<PathBuf, PortError> {
        if self.state.lock().unwrap().fail_prepare.contains(&repo.id) {
            return Err(PortError::Rejected(format!("worktree {}", repo.id)));
        }
        let wt = fake_worktree(repo, branch, group);
        let mut st = self.state.lock().unwrap();
        st.prepared.push(branch.to_owned());
        st.worktrees.insert(wt.clone());
        Ok(wt)
    }
    async fn restore_worktree(
        &self,
        repo: &RepoInfo,
        branch: &str,
        group: Option<&str>,
    ) -> Result<PathBuf, PortError> {
        let wt = fake_worktree(repo, branch, group);
        let mut st = self.state.lock().unwrap();
        st.restored.push(branch.to_owned());
        st.worktrees.insert(wt.clone());
        Ok(wt)
    }
    async fn discard_worktree(
        &self,
        _r: &RepoInfo,
        wt: &Path,
        branch: &str,
    ) -> Result<(), PortError> {
        let mut st = self.state.lock().unwrap();
        st.discarded.push(branch.to_owned());
        st.worktrees.remove(wt);
        Ok(())
    }
    async fn remove_run_root(&self, root: &Path) {
        self.state
            .lock()
            .unwrap()
            .removed_roots
            .push(root.to_path_buf());
    }
    async fn worktree_exists(&self, wt: &Path) -> bool {
        self.state.lock().unwrap().worktrees.contains(wt)
    }
    async fn changes(&self, _r: &RepoInfo, wt: &Path, _b: &str) -> Result<WorkChanges, PortError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .changes
            .get(wt)
            .cloned()
            .unwrap_or_default())
    }
    async fn discard_file(&self, _r: &RepoInfo, _wt: &Path, path: &str) -> Result<(), PortError> {
        self.state.lock().unwrap().reverted.push(path.to_owned());
        Ok(())
    }
    async fn commit_all(&self, wt: &Path, message: &str) -> Result<Option<String>, PortError> {
        let mut st = self.state.lock().unwrap();
        st.commits.push((wt.to_path_buf(), message.to_owned()));
        if let Some(c) = st.changes.get_mut(wt) {
            c.dirty = false;
            c.unpushed += 1;
        }
        Ok(Some(format!("sha{}", st.commits.len())))
    }
    async fn revert_commit(&self, _wt: &Path, sha: &str) -> Result<bool, PortError> {
        let mut st = self.state.lock().unwrap();
        if st.revert_conflicts.iter().any(|s| s == sha) {
            return Ok(false);
        }
        st.reverts.push(sha.to_owned());
        Ok(true)
    }
    async fn untracked(&self, wt: &Path) -> Result<Vec<String>, PortError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .untracked
            .get(wt)
            .cloned()
            .unwrap_or_default())
    }
    async fn push(&self, _r: &RepoInfo, wt: &Path, branch: &str) -> Result<(), PortError> {
        let mut st = self.state.lock().unwrap();
        if st.fail_push {
            return Err(PortError::Transient("git push rejected".to_owned()));
        }
        st.pushes.push(branch.to_owned());
        st.pushed_wts.push(wt.to_path_buf());
        if let Some(c) = st.changes.get_mut(wt) {
            c.unpushed = 0;
        }
        Ok(())
    }
    async fn branch_diff(&self, _r: &RepoInfo, wt: &Path) -> Result<String, PortError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .branch_diffs
            .get(wt)
            .cloned()
            .unwrap_or_else(|| "+diff pr".to_owned()))
    }
    async fn find_pr(&self, r: &RepoInfo, branch: &str) -> Result<Option<PrRef>, PortError> {
        let state = self.state.lock().unwrap();
        Ok(state.prs.get(&pr_key(r, branch)).map(|u| {
            let merged = state.merged_prs.contains(u);
            PrRef {
                url: u.clone(),
                open: !merged,
                merged,
            }
        }))
    }
    async fn create_pr(
        &self,
        r: &RepoInfo,
        _wt: &Path,
        branch: &str,
        title: &str,
        body: &str,
        draft: bool,
    ) -> Result<String, PortError> {
        let mut st = self.state.lock().unwrap();
        st.pr_bodies.push(body.to_owned());
        if st.fail_create_pr.contains(&r.id) {
            return Err(PortError::Transient(format!("gh pr create {}", r.id)));
        }
        let url = format!(
            "https://github.com/o/{}/pull/{}",
            r.id,
            st.created_prs.len() + 1
        );
        st.created_prs.push((branch.to_owned(), title.to_owned()));
        if draft {
            st.drafts.push(url.clone());
        }
        st.prs.insert(pr_key(r, branch), url.clone());
        st.bodies.insert(url.clone(), body.to_owned());
        Ok(url)
    }
    async fn mark_pr_ready(&self, _r: &RepoInfo, url: &str) -> Result<(), PortError> {
        let mut st = self.state.lock().unwrap();
        if st.fail_ready {
            return Err(PortError::Transient("gh pr ready timed out".to_owned()));
        }
        st.ready.push(url.to_owned());
        Ok(())
    }
    async fn snapshot(&self, wt: &Path) -> Result<String, PortError> {
        let st = self.state.lock().unwrap();
        if st.snapshot_fails {
            return Err(PortError::Transient("git write-tree".to_owned()));
        }
        Ok(st
            .trees
            .get(wt)
            .cloned()
            .unwrap_or_else(|| "tree0".to_owned()))
    }
    async fn changed_between(
        &self,
        _wt: &Path,
        a: &str,
        b: &str,
    ) -> Result<Vec<String>, PortError> {
        let st = self.state.lock().unwrap();
        st.calls_between
            .lock()
            .unwrap()
            .push((a.to_owned(), b.to_owned()));
        Ok(if a == b {
            vec![]
        } else if st.diff_files.is_empty() {
            vec!["src/changed.rs".to_owned()]
        } else {
            st.diff_files.clone()
        })
    }
    async fn pr_body(&self, _r: &RepoInfo, url: &str) -> Result<Option<String>, PortError> {
        Ok(self.state.lock().unwrap().bodies.get(url).cloned())
    }
    async fn set_pr_body(
        &self,
        _r: &RepoInfo,
        _wt: &Path,
        url: &str,
        body: &str,
    ) -> Result<(), PortError> {
        let mut st = self.state.lock().unwrap();
        st.bodies.insert(url.to_owned(), body.to_owned());
        st.body_edits += 1;
        Ok(())
    }
}

type Sender = Arc<Mutex<Option<mpsc::UnboundedSender<AgentEvent>>>>;

/// Kendali satu run agent palsu dari test.
#[derive(Clone)]
pub struct RunCtl {
    pub req: AgentRequest,
    tx: Sender,
    pub stopped: Arc<Mutex<bool>>,
}

impl RunCtl {
    pub fn emit(&self, e: AgentEvent) {
        if let Some(tx) = self.tx.lock().unwrap().as_ref() {
            let _ = tx.send(e);
        }
    }
    /// Sesi berhenti karena batas biaya (event `result` A4).
    pub fn budget_stop(&self) {
        self.emit(AgentEvent::Usage(mandor_domain::Usage {
            input_tokens: 10,
            output_tokens: 5,
            ..mandor_domain::Usage::default()
        }));
        self.emit(AgentEvent::Done {
            is_error: true,
            result: "Reached maximum budget ($2)".to_owned(),
            num_turns: 3,
            denials: 0,
            cost_usd: Some(2.01),
            limit: Some("budget".to_owned()),
        });
        self.tx.lock().unwrap().take();
    }
    /// Giliran selesai normal dengan pesan akhir `result`.
    pub fn finish(&self, result: &str) {
        self.emit(AgentEvent::Done {
            is_error: false,
            result: result.to_owned(),
            num_turns: 1,
            denials: 0,
            cost_usd: None,
            limit: None,
        });
        self.tx.lock().unwrap().take();
    }
    /// Giliran selesai normal.
    pub fn done(&self) {
        self.emit(AgentEvent::Done {
            is_error: false,
            result: "selesai".to_owned(),
            num_turns: 1,
            denials: 0,
            cost_usd: None,
            limit: None,
        });
        self.tx.lock().unwrap().take();
    }
    pub fn crash(&self, msg: &str) {
        self.emit(AgentEvent::Failed {
            message: msg.to_owned(),
        });
        self.tx.lock().unwrap().take();
    }
    pub fn is_stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }
}

#[derive(Clone)]
pub struct FakeStopper {
    tx: Sender,
    stopped: Arc<Mutex<bool>>,
}

impl Stopper for FakeStopper {
    async fn stop(&self) {
        *self.stopped.lock().unwrap() = true;
        let tx = self.tx.lock().unwrap().take();
        if let Some(tx) = tx {
            let _ = tx.send(AgentEvent::Failed {
                message: "dihentikan".to_owned(),
            });
        }
    }
}

pub struct FakeRun {
    rx: mpsc::UnboundedReceiver<AgentEvent>,
    stopper: FakeStopper,
}

impl AgentRunHandle for FakeRun {
    type Stopper = FakeStopper;
    async fn next_event(&mut self) -> Option<AgentEvent> {
        self.rx.recv().await
    }
    fn pid(&self) -> Option<u32> {
        Some(4242)
    }
    fn stopper(&self) -> FakeStopper {
        self.stopper.clone()
    }
}

#[derive(Clone, Default)]
pub struct FakeAgent {
    pub runs: Arc<Mutex<Vec<RunCtl>>>,
    pub fail_spawn: Arc<Mutex<bool>>,
    pub commit_diffs: Arc<Mutex<Vec<String>>>,
    pub reaped: Arc<Mutex<Vec<u32>>>,
    pub pr_contexts: Arc<Mutex<Vec<OwnedPrContext>>>,
    pub fail_pr_description: Arc<Mutex<bool>>,
    /// Review yang berkasnya dibersihkan (`end_review`).
    pub ended_reviews: Arc<Mutex<Vec<String>>>,
    /// Login Claude tidak valid (plan VM U3).
    pub logged_out: Arc<Mutex<bool>>,
}

/// Salinan [`PrContext`] yang diterima fake agent.
#[derive(Debug, Clone)]
pub struct OwnedPrContext {
    pub title: String,
    pub acs: Vec<String>,
    pub plan: Option<String>,
    pub diff: String,
    pub decided: Vec<String>,
}

impl FakeAgent {
    pub fn run(&self, i: usize) -> RunCtl {
        self.runs.lock().unwrap()[i].clone()
    }
    pub fn count(&self) -> usize {
        self.runs.lock().unwrap().len()
    }
    pub fn last(&self) -> RunCtl {
        self.runs.lock().unwrap().last().cloned().unwrap()
    }
}

impl AgentPort for FakeAgent {
    type Run = FakeRun;
    async fn spawn(&self, req: &AgentRequest) -> Result<FakeRun, PortError> {
        if *self.fail_spawn.lock().unwrap() {
            return Err(PortError::Transient("claude tidak ditemukan".to_owned()));
        }
        let (tx, rx) = mpsc::unbounded_channel();
        let tx: Sender = Arc::new(Mutex::new(Some(tx)));
        let stopped = Arc::new(Mutex::new(false));
        self.runs.lock().unwrap().push(RunCtl {
            req: req.clone(),
            tx: Arc::clone(&tx),
            stopped: Arc::clone(&stopped),
        });
        Ok(FakeRun {
            rx,
            stopper: FakeStopper { tx, stopped },
        })
    }
    async fn commit_message(&self, _wt: &Path, diff: &str) -> Result<String, PortError> {
        self.commit_diffs.lock().unwrap().push(diff.to_owned());
        Ok("feat: implementasi task".to_owned())
    }
    async fn pr_description(&self, _wt: &Path, ctx: &PrContext<'_>) -> Result<String, PortError> {
        self.pr_contexts.lock().unwrap().push(OwnedPrContext {
            title: ctx.title.to_owned(),
            acs: ctx.acs.to_vec(),
            plan: ctx.plan.map(str::to_owned),
            diff: ctx.diff.to_owned(),
            decided: ctx.decided.to_vec(),
        });
        if *self.fail_pr_description.lock().unwrap() {
            return Err(PortError::Transient("claude timeout".to_owned()));
        }
        Ok("## Summary\nAdds the tab.\n\n## Changes\n- src/a.rs".to_owned())
    }
    async fn reap(&self, pid: u32) {
        self.reaped.lock().unwrap().push(pid);
    }
    async fn logged_in(&self) -> bool {
        !*self.logged_out.lock().unwrap()
    }

    async fn end_review(&self, review_id: &str) {
        self.ended_reviews
            .lock()
            .unwrap()
            .push(review_id.to_owned());
    }
}

#[derive(Clone, Default)]
pub struct FakeRuns {
    pub rows: Arc<Mutex<HashMap<String, (RunRecord, bool)>>>,
    pub chat: Arc<Mutex<Vec<(String, ChatItem)>>>,
    /// (record, reply) → `pending` / `delivered`.
    pub journal: Arc<Mutex<Vec<(String, String, String)>>>,
    /// Kartu run grup per run.
    pub cards: Arc<Mutex<HashMap<String, Vec<GroupCardRow>>>>,
}

impl FakeRuns {
    /// Run terbuka yang memuat kartu terbuka `record_id`.
    fn group_of(&self, record_id: &str) -> Option<String> {
        let rows = self.rows.lock().unwrap();
        self.cards
            .lock()
            .unwrap()
            .iter()
            .find(|(run_id, cards)| {
                rows.get(*run_id).is_some_and(|(_, closed)| !closed)
                    && cards.iter().any(|c| {
                        c.record_id == record_id
                            && !matches!(c.state, mandor_domain::group::CardState::Removed(_))
                    })
            })
            .map(|(id, _)| id.clone())
    }
}

impl Journal for FakeRuns {
    async fn journal_pending(
        &self,
        record: &str,
        reply: &str,
        _run: &str,
        _now: i64,
    ) -> Result<(), PortError> {
        let mut j = self.journal.lock().unwrap();
        if !j.iter().any(|(r, id, _)| r == record && id == reply) {
            j.push((record.to_owned(), reply.to_owned(), "pending".to_owned()));
        }
        Ok(())
    }
    async fn journal_delivered(&self, record: &str, reply: &str) -> Result<(), PortError> {
        for e in self.journal.lock().unwrap().iter_mut() {
            if e.0 == record && e.1 == reply {
                "delivered".clone_into(&mut e.2);
            }
        }
        Ok(())
    }
    async fn journal_requeue(&self, record: &str, reply: &str) -> Result<(), PortError> {
        for e in self.journal.lock().unwrap().iter_mut() {
            if e.0 == record && e.1 == reply {
                "pending".clone_into(&mut e.2);
            }
        }
        Ok(())
    }
    async fn journal_pending_list(&self, record: &str) -> Result<Vec<String>, PortError> {
        Ok(self
            .journal
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.0 == record && e.2 == "pending")
            .map(|e| e.1.clone())
            .collect())
    }
    async fn journal_delivered_list(&self, record: &str) -> Result<Vec<String>, PortError> {
        Ok(self
            .journal
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.0 == record && e.2 == "delivered")
            .map(|e| e.1.clone())
            .collect())
    }
}

impl FakeRuns {
    pub fn all(&self) -> Vec<RunRecord> {
        self.rows
            .lock()
            .unwrap()
            .values()
            .map(|(r, _)| r.clone())
            .collect()
    }
    pub fn only(&self) -> RunRecord {
        let rows = self.rows.lock().unwrap();
        let active: Vec<_> = rows
            .values()
            .filter(|(_, closed)| !closed)
            .map(|(r, _)| r.clone())
            .collect();
        assert_eq!(active.len(), 1, "{active:?}");
        active[0].clone()
    }
    pub fn insert(&self, r: RunRecord) {
        self.rows.lock().unwrap().insert(r.id.clone(), (r, false));
    }
    pub fn system_messages(&self) -> Vec<String> {
        self.chat
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(_, c)| match c {
                ChatItem::System(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }
    pub fn dev_messages(&self) -> Vec<String> {
        self.chat
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(_, c)| match c {
                ChatItem::Dev(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }
}

impl RunStore for FakeRuns {
    async fn create_group(
        &self,
        run: &RunRecord,
        cards: &[GroupCardRow],
        _now: i64,
    ) -> Result<(), PortError> {
        for c in cards {
            if self.group_of(&c.record_id).is_some()
                || self
                    .rows
                    .lock()
                    .unwrap()
                    .values()
                    .any(|(r, closed)| !closed && r.record_id == c.record_id)
            {
                return Err(PortError::Conflict);
            }
        }
        self.rows
            .lock()
            .unwrap()
            .insert(run.id.clone(), (run.clone(), false));
        self.cards
            .lock()
            .unwrap()
            .insert(run.id.clone(), cards.to_vec());
        Ok(())
    }
    async fn cards(&self, run_id: &str) -> Result<Vec<GroupCardRow>, PortError> {
        Ok(self
            .cards
            .lock()
            .unwrap()
            .get(run_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn save_cards(
        &self,
        run_id: &str,
        cards: &[GroupCardRow],
        primary: Option<&str>,
    ) -> Result<(), PortError> {
        self.cards
            .lock()
            .unwrap()
            .insert(run_id.to_owned(), cards.to_vec());
        if let Some(p) = primary
            && let Some((r, _)) = self.rows.lock().unwrap().get_mut(run_id)
        {
            p.clone_into(&mut r.record_id);
        }
        Ok(())
    }
    async fn create(&self, run: &RunRecord, _now: i64) -> Result<(), PortError> {
        if self.group_of(&run.record_id).is_some() {
            return Err(PortError::Conflict);
        }
        let mut rows = self.rows.lock().unwrap();
        if rows
            .values()
            .any(|(r, closed)| !closed && r.record_id == run.record_id)
        {
            return Err(PortError::Conflict);
        }
        rows.insert(run.id.clone(), (run.clone(), false));
        Ok(())
    }
    async fn get(&self, id: &str) -> Result<Option<RunRecord>, PortError> {
        Ok(self.rows.lock().unwrap().get(id).map(|(r, _)| r.clone()))
    }
    async fn active_for(&self, record_id: &str) -> Result<Option<RunRecord>, PortError> {
        if let Some(id) = self.group_of(record_id) {
            return Ok(self.rows.lock().unwrap().get(&id).map(|(r, _)| r.clone()));
        }
        Ok(self
            .rows
            .lock()
            .unwrap()
            .values()
            .find(|(r, c)| !c && r.record_id == record_id)
            .map(|(r, _)| r.clone()))
    }
    async fn latest_for(&self, record_id: &str) -> Result<Option<RunRecord>, PortError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .values()
            .filter(|(r, _)| r.record_id == record_id)
            .map(|(r, _)| r.clone())
            .max_by(|a, b| a.id.cmp(&b.id)))
    }
    async fn list_active(&self) -> Result<Vec<RunRecord>, PortError> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .values()
            .filter(|(_, c)| !c)
            .map(|(r, _)| r.clone())
            .collect())
    }
    async fn save(&self, run: &RunRecord, expected: i64, _now: i64) -> Result<(), PortError> {
        let mut rows = self.rows.lock().unwrap();
        let (cur, _) = rows
            .get_mut(&run.id)
            .ok_or(PortError::Rejected("run tidak ada".into()))?;
        if cur.generation != expected {
            return Err(PortError::Conflict);
        }
        *cur = run.clone();
        Ok(())
    }
    async fn close(&self, id: &str, _now: i64) -> Result<(), PortError> {
        if let Some(row) = self.rows.lock().unwrap().get_mut(id) {
            row.1 = true;
        }
        Ok(())
    }
    async fn append(&self, run_id: &str, item: &ChatItem, _now: i64) -> Result<(), PortError> {
        self.chat
            .lock()
            .unwrap()
            .push((run_id.to_owned(), item.clone()));
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct FakeNotifier {
    pub sent: Arc<Mutex<Vec<Notice>>>,
}

impl Notifier for FakeNotifier {
    async fn notify(&self, n: Notice) {
        self.sent.lock().unwrap().push(n);
    }
}

#[derive(Clone, Default)]
pub struct TokioSpawner {
    pub spawned: Arc<AtomicU32>,
}

impl Spawner for TokioSpawner {
    fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.spawned.fetch_add(1, Ordering::SeqCst);
        tokio::spawn(fut);
    }
}

#[derive(Clone)]
pub struct FakeRepos {
    pub repos: Arc<Mutex<Vec<RepoInfo>>>,
    pub workspaces: Arc<Mutex<Vec<WorkspaceInfo>>>,
}

impl FakeRepos {
    pub fn add(&self, r: RepoInfo) {
        self.repos.lock().unwrap().push(r);
    }
    pub fn add_workspace(&self, w: WorkspaceInfo) {
        self.workspaces.lock().unwrap().push(w);
    }
    pub fn remove(&self, id: &str) {
        self.repos.lock().unwrap().retain(|r| r.id != id);
    }
}

impl Default for FakeRepos {
    fn default() -> Self {
        Self {
            workspaces: Arc::default(),
            repos: Arc::new(Mutex::new(vec![RepoInfo {
                id: "repo1".to_owned(),
                path: PathBuf::from("/repo"),
                default_branch: "main".to_owned(),
                gh_profile: None,
                companions: vec![],
                trusted_reviewers: vec!["ou_qa".to_owned()],
                extra_allowed: vec![],
                app: mandor_domain::app_run::AppSettings::default(),
                gate_mode: None,
            }])),
        }
    }
}

impl Repos for FakeRepos {
    async fn repo(&self, id: &str) -> Result<Option<RepoInfo>, PortError> {
        Ok(self
            .repos
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }
    async fn workspace(&self, id: &str) -> Result<Option<WorkspaceInfo>, PortError> {
        Ok(self
            .workspaces
            .lock()
            .unwrap()
            .iter()
            .find(|w| w.id == id)
            .cloned())
    }
}

/// Menunggu kondisi yang dipenuhi task latar.
pub async fn eventually(mut cond: impl FnMut() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(cond(), "kondisi tidak terpenuhi");
}

/// Penyimpanan Rilis 2 di memori.
#[derive(Clone, Default)]
pub struct FakeReviews {
    pub ledgers: Arc<Mutex<HashMap<String, Vec<mandor_domain::AcRow>>>>,
    pub history: Arc<Mutex<Vec<mandor_usecase::run_ports::ReviewRecord>>>,
    pub settings: Arc<Mutex<mandor_usecase::run_ports::ReviewerSettings>>,
    pub chrome: Arc<Mutex<bool>>,
}

impl FakeReviews {
    /// Menghapus buku AC run (meniru run lama sebelum Rilis 2).
    pub fn clear(&self, run_id: &str) {
        self.ledgers.lock().unwrap().remove(run_id);
    }
}

impl mandor_usecase::run_ports::LedgerStore for FakeReviews {
    async fn ac_rows(&self, run_id: &str) -> Result<Vec<mandor_domain::AcRow>, PortError> {
        Ok(self
            .ledgers
            .lock()
            .unwrap()
            .get(run_id)
            .cloned()
            .unwrap_or_default())
    }
    async fn save_ac_rows(
        &self,
        run_id: &str,
        rows: &[mandor_domain::AcRow],
    ) -> Result<(), PortError> {
        self.ledgers
            .lock()
            .unwrap()
            .insert(run_id.to_owned(), rows.to_vec());
        Ok(())
    }
}

/// State proses app palsu dan penyimpanannya.
#[derive(Default)]
pub struct AppsState {
    pub records: HashMap<String, mandor_usecase::run_ports::AppRecord>,
    pub owner: Option<String>,
    /// Port yang sudah dipakai program lain sebelum app dinyalakan.
    pub foreign_ports: HashSet<u16>,
    pub fail_setup: HashSet<String>,
    pub fail_ready: HashSet<String>,
    pub lockfile: Option<String>,
    /// Urutan panggilan: `setup:<repo>`, `spawn:<repo>`, `ready:<repo>`, `stop:<run>`.
    pub calls: Vec<String>,
    pub reaped: Vec<(String, u32)>,
    /// Setup yang mengubah tree worktree (meniru kode berubah saat start).
    pub change_tree_on_setup: Option<(Arc<Mutex<GitState>>, PathBuf, String)>,
    next_pgid: u32,
}

#[derive(Clone, Default)]
pub struct FakeApps {
    pub st: Arc<Mutex<AppsState>>,
}

impl FakeApps {
    pub fn calls(&self) -> Vec<String> {
        self.st.lock().unwrap().calls.clone()
    }
    pub fn record(&self, run_id: &str) -> Option<mandor_usecase::run_ports::AppRecord> {
        self.st.lock().unwrap().records.get(run_id).cloned()
    }
}

impl mandor_usecase::run_ports::AppProcesses for FakeApps {
    async fn port_open(&self, port: u16) -> bool {
        self.st.lock().unwrap().foreign_ports.contains(&port)
    }
    async fn lockfile_hash(&self, _wt: &Path) -> Option<String> {
        self.st.lock().unwrap().lockfile.clone()
    }
    async fn setup(
        &self,
        _run_id: &str,
        m: &mandor_usecase::run_ports::AppMember,
    ) -> Result<(), PortError> {
        let mut st = self.st.lock().unwrap();
        st.calls.push(format!("setup:{}", m.repo_id));
        if let Some((git, wt, tree)) = &st.change_tree_on_setup {
            git.lock().unwrap().trees.insert(wt.clone(), tree.clone());
        }
        if st.fail_setup.contains(&m.repo_id) {
            return Err(PortError::Rejected("setup failed: exit 1".to_owned()));
        }
        Ok(())
    }
    async fn spawn(
        &self,
        _run_id: &str,
        m: &mandor_usecase::run_ports::AppMember,
    ) -> Result<u32, PortError> {
        let mut st = self.st.lock().unwrap();
        st.calls.push(format!("spawn:{}", m.repo_id));
        st.next_pgid += 1;
        Ok(1000 + st.next_pgid)
    }
    async fn wait_ready(&self, _run_id: &str, repo_id: &str, _port: u16) -> Result<(), PortError> {
        let mut st = self.st.lock().unwrap();
        st.calls.push(format!("ready:{repo_id}"));
        if st.fail_ready.contains(repo_id) {
            return Err(PortError::Rejected(
                "the app did not open its port".to_owned(),
            ));
        }
        Ok(())
    }
    async fn stop(&self, run_id: &str) {
        self.st.lock().unwrap().calls.push(format!("stop:{run_id}"));
    }
    async fn logs(&self, run_id: &str) -> Vec<(String, String)> {
        vec![("repo1".to_owned(), format!("log {run_id}"))]
    }
    async fn reap(&self, run_id: &str, pgid: u32) {
        self.st
            .lock()
            .unwrap()
            .reaped
            .push((run_id.to_owned(), pgid));
    }
}

impl mandor_usecase::run_ports::AppRecords for FakeApps {
    async fn app_record(
        &self,
        run_id: &str,
    ) -> Result<Option<mandor_usecase::run_ports::AppRecord>, PortError> {
        Ok(self.record(run_id))
    }
    async fn save_app_record(
        &self,
        rec: &mandor_usecase::run_ports::AppRecord,
    ) -> Result<(), PortError> {
        self.st
            .lock()
            .unwrap()
            .records
            .insert(rec.run_id.clone(), rec.clone());
        Ok(())
    }
    async fn app_owner(&self) -> Result<Option<String>, PortError> {
        Ok(self.st.lock().unwrap().owner.clone())
    }
    async fn set_app_owner(&self, run_id: Option<&str>) -> Result<(), PortError> {
        self.st.lock().unwrap().owner = run_id.map(str::to_owned);
        Ok(())
    }
}

impl mandor_usecase::run_ports::ReviewHistory for FakeReviews {
    async fn save_review(
        &self,
        r: &mandor_usecase::run_ports::ReviewRecord,
    ) -> Result<(), PortError> {
        let mut h = self.history.lock().unwrap();
        h.retain(|x| x.id != r.id);
        h.push(r.clone());
        Ok(())
    }
    async fn reviews(
        &self,
        run_id: &str,
    ) -> Result<Vec<mandor_usecase::run_ports::ReviewRecord>, PortError> {
        let mut out: Vec<_> = self
            .history
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.run_id == run_id)
            .cloned()
            .collect();
        out.sort_by_key(|r| std::cmp::Reverse(r.started_at_ms));
        Ok(out)
    }
    async fn reviewer_settings(&self) -> mandor_usecase::run_ports::ReviewerSettings {
        self.settings.lock().unwrap().clone()
    }
    async fn set_reviewer_settings(
        &self,
        s: &mandor_usecase::run_ports::ReviewerSettings,
    ) -> Result<(), PortError> {
        *self.settings.lock().unwrap() = s.clone();
        Ok(())
    }
    async fn chrome_connected(&self) -> bool {
        *self.chrome.lock().unwrap()
    }
}

/// Antrean, siklus, dan setelan lembur di memori (plan VM U2).
#[derive(Clone, Default)]
pub struct FakeOvernight {
    pub queue: Arc<Mutex<Vec<mandor_usecase::overnight::QueueItem>>>,
    pub dequeued: Arc<Mutex<Vec<mandor_usecase::overnight::Dequeued>>>,
    pub laptop_only: Arc<Mutex<HashSet<String>>>,
    pub settings: Arc<Mutex<Option<mandor_usecase::overnight::OvernightSettings>>>,
    pub cycles: Arc<Mutex<HashMap<i64, mandor_usecase::overnight::CycleUsage>>>,
    pub reports: Arc<Mutex<HashMap<i64, mandor_usecase::overnight::ReportState>>>,
}

impl FakeOvernight {
    pub fn ids(&self) -> Vec<String> {
        self.queue
            .lock()
            .unwrap()
            .iter()
            .map(|i| i.record_id.clone())
            .collect()
    }
}

impl mandor_usecase::overnight::OvernightStore for FakeOvernight {
    async fn queue(&self) -> Result<Vec<mandor_usecase::overnight::QueueItem>, PortError> {
        Ok(self.queue.lock().unwrap().clone())
    }
    async fn enqueue(&self, item: &mandor_usecase::overnight::QueueItem) -> Result<(), PortError> {
        let mut q = self.queue.lock().unwrap();
        if q.iter().any(|i| i.record_id == item.record_id) {
            return Err(PortError::Conflict);
        }
        q.push(item.clone());
        Ok(())
    }
    async fn dequeue(
        &self,
        record_id: &str,
        reason: Option<&str>,
        now_ms: i64,
    ) -> Result<(), PortError> {
        self.queue
            .lock()
            .unwrap()
            .retain(|i| i.record_id != record_id);
        if let Some(r) = reason {
            self.dequeued
                .lock()
                .unwrap()
                .push(mandor_usecase::overnight::Dequeued {
                    record_id: record_id.to_owned(),
                    reason: r.to_owned(),
                    at_ms: now_ms,
                });
        }
        Ok(())
    }
    async fn reorder(&self, record_ids: &[String]) -> Result<(), PortError> {
        let mut q = self.queue.lock().unwrap();
        q.sort_by_key(|i| {
            record_ids
                .iter()
                .position(|r| *r == i.record_id)
                .unwrap_or(usize::MAX)
        });
        Ok(())
    }
    async fn set_planned_state(
        &self,
        record_id: &str,
        state: mandor_domain::planned::PlannedState,
        reason: Option<&str>,
    ) -> Result<(), PortError> {
        let mut q = self.queue.lock().unwrap();
        let p = q
            .iter_mut()
            .find(|i| i.record_id == record_id)
            .and_then(|i| i.planned.as_mut())
            .ok_or_else(|| {
                PortError::Rejected("the card is not a planned queue item".to_owned())
            })?;
        p.state = state;
        p.reason = reason.map(str::to_owned);
        Ok(())
    }
    async fn set_start_now(&self, record_id: &str) -> Result<(), PortError> {
        let mut q = self.queue.lock().unwrap();
        let item = q
            .iter_mut()
            .find(|i| i.record_id == record_id)
            .ok_or_else(|| PortError::Rejected("the card is not queued".to_owned()))?;
        item.start_now = true;
        Ok(())
    }
    async fn dequeued_since(
        &self,
        since_ms: i64,
    ) -> Result<Vec<mandor_usecase::overnight::Dequeued>, PortError> {
        Ok(self
            .dequeued
            .lock()
            .unwrap()
            .iter()
            .filter(|d| d.at_ms >= since_ms)
            .cloned()
            .collect())
    }
    async fn laptop_only(&self, record_id: &str) -> Result<bool, PortError> {
        Ok(self.laptop_only.lock().unwrap().contains(record_id))
    }
    async fn set_laptop_only(&self, record_id: &str, _now_ms: i64) -> Result<(), PortError> {
        self.laptop_only
            .lock()
            .unwrap()
            .insert(record_id.to_owned());
        Ok(())
    }
    async fn settings(
        &self,
    ) -> Result<Option<mandor_usecase::overnight::OvernightSettings>, PortError> {
        Ok(self.settings.lock().unwrap().clone())
    }
    async fn set_settings(
        &self,
        s: &mandor_usecase::overnight::OvernightSettings,
    ) -> Result<(), PortError> {
        *self.settings.lock().unwrap() = Some(s.clone());
        Ok(())
    }
    async fn cycle_usage(
        &self,
        start: i64,
    ) -> Result<mandor_usecase::overnight::CycleUsage, PortError> {
        Ok(self
            .cycles
            .lock()
            .unwrap()
            .get(&start)
            .copied()
            .unwrap_or_default())
    }
    async fn report_state(
        &self,
        start: i64,
    ) -> Result<mandor_usecase::overnight::ReportState, PortError> {
        Ok(self
            .reports
            .lock()
            .unwrap()
            .get(&start)
            .copied()
            .unwrap_or_default())
    }
    async fn record_report(&self, start: i64, sent: bool, _now: i64) -> Result<(), PortError> {
        let mut r = self.reports.lock().unwrap();
        let e = r.entry(start).or_default();
        e.attempts += 1;
        e.sent |= sent;
        Ok(())
    }
    async fn add_cycle_usage(
        &self,
        start: i64,
        u: &mandor_usecase::overnight::CycleUsage,
    ) -> Result<(), PortError> {
        let mut c = self.cycles.lock().unwrap();
        let e = c.entry(start).or_default();
        e.input_tokens += u.input_tokens;
        e.output_tokens += u.output_tokens;
        e.cache_read_tokens += u.cache_read_tokens;
        e.cache_write_tokens += u.cache_write_tokens;
        Ok(())
    }
}

/// Sumber desain daemon palsu: id → nama, dengan sumber yang hilang dan yang
/// gagal diperbarui.
#[derive(Clone, Default)]
pub struct FakeDesign {
    pub sources: Arc<Mutex<HashMap<String, String>>>,
    pub missing: Arc<Mutex<HashSet<String>>>,
    pub stale: Arc<Mutex<HashMap<String, String>>>,
    /// Id yang diminta diperbarui.
    pub refreshed: Arc<Mutex<Vec<String>>>,
}

impl FakeDesign {
    pub fn add(&self, id: &str, name: &str) {
        self.sources
            .lock()
            .unwrap()
            .insert(id.to_owned(), name.to_owned());
    }
}

impl mandor_usecase::review::DesignSources for FakeDesign {
    async fn prepare(
        &self,
        ids: &[String],
        refresh: bool,
    ) -> Result<mandor_usecase::review::DesignCheck, PortError> {
        let mut out = mandor_usecase::review::DesignCheck::default();
        for id in ids {
            let Some(name) = self.sources.lock().unwrap().get(id).cloned() else {
                out.unavailable
                    .push((id.clone(), "it was removed from the VM".to_owned()));
                continue;
            };
            if self.missing.lock().unwrap().contains(id) {
                out.unavailable.push((name, "Path missing".to_owned()));
                continue;
            }
            if refresh {
                self.refreshed.lock().unwrap().push(id.clone());
                if let Some(why) = self.stale.lock().unwrap().get(id) {
                    out.stale.push((name.clone(), why.clone()));
                }
            }
            out.roots.push(mandor_usecase::review::DesignRoot {
                id: id.clone(),
                path: PathBuf::from(format!("/design/{name}")),
                name,
            });
        }
        Ok(out)
    }
}
