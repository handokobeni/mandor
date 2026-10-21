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
    AgentPort, AgentRequest, AgentRunHandle, ChatItem, GitPort, Journal, Notice, Notifier, PrRef,
    RepoInfo, Repos, RunRecord, RunStore, Spawner, Stopper, WorkChanges, WorkspaceInfo,
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
    pub commits: Vec<(PathBuf, String)>,
    pub pushes: Vec<String>,
    pub prs: HashMap<String, String>,
    pub created_prs: Vec<(String, String)>,
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
    async fn commit_all(&self, wt: &Path, message: &str) -> Result<(), PortError> {
        let mut st = self.state.lock().unwrap();
        st.commits.push((wt.to_path_buf(), message.to_owned()));
        if let Some(c) = st.changes.get_mut(wt) {
            c.dirty = false;
            c.unpushed += 1;
        }
        Ok(())
    }
    async fn push(&self, _r: &RepoInfo, wt: &Path, branch: &str) -> Result<(), PortError> {
        let mut st = self.state.lock().unwrap();
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
        Ok(self
            .state
            .lock()
            .unwrap()
            .prs
            .get(&pr_key(r, branch))
            .map(|u| PrRef {
                url: u.clone(),
                open: true,
            }))
    }
    async fn create_pr(
        &self,
        r: &RepoInfo,
        _wt: &Path,
        branch: &str,
        title: &str,
        _body: &str,
    ) -> Result<String, PortError> {
        let mut st = self.state.lock().unwrap();
        if st.fail_create_pr.contains(&r.id) {
            return Err(PortError::Transient(format!("gh pr create {}", r.id)));
        }
        let url = format!(
            "https://github.com/o/{}/pull/{}",
            r.id,
            st.created_prs.len() + 1
        );
        st.created_prs.push((branch.to_owned(), title.to_owned()));
        st.prs.insert(pr_key(r, branch), url.clone());
        Ok(url)
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
    /// Giliran selesai normal dengan pesan akhir `result`.
    pub fn finish(&self, result: &str) {
        self.emit(AgentEvent::Done {
            is_error: false,
            result: result.to_owned(),
            num_turns: 1,
            denials: 0,
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
    async fn reap(&self, pid: u32) {
        self.reaped.lock().unwrap().push(pid);
    }
}

#[derive(Clone, Default)]
pub struct FakeRuns {
    pub rows: Arc<Mutex<HashMap<String, (RunRecord, bool)>>>,
    pub chat: Arc<Mutex<Vec<(String, ChatItem)>>>,
    /// (record, reply) → `pending` / `delivered`.
    pub journal: Arc<Mutex<Vec<(String, String, String)>>>,
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
    async fn create(&self, run: &RunRecord, _now: i64) -> Result<(), PortError> {
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
        Ok(self
            .rows
            .lock()
            .unwrap()
            .values()
            .find(|(r, c)| !c && r.record_id == record_id)
            .map(|(r, _)| r.clone()))
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
