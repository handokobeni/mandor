//! Fake port untuk test use case. Lock `std` tidak pernah ditahan melewati `.await`.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mandor_domain::{AppField, CardFlag, Task, TaskStatus, WriteOp};
use mandor_usecase::{
    AcItem, CachedTask, Clock, Deps, Ids, LarkTasks, Outbox, PortError, QueuedWrite, SprintItem,
    SyncService, TaskCache,
};

/// Satu tulisan yang diterima Lark tiruan.
pub type Write = (String, Option<TaskStatus>, Vec<AppField>);

/// Base Lark tiruan yang dibagi beberapa "laptop".
#[derive(Clone, Default)]
pub struct FakeLark {
    pub records: Arc<Mutex<HashMap<String, Task>>>,
    pub writes: Arc<Mutex<Vec<Write>>>,
    pub searches: Arc<Mutex<Vec<Option<i64>>>>,
    /// Error yang dikembalikan oleh `write` berikutnya (FIFO).
    pub write_errors: Arc<Mutex<Vec<PortError>>>,
    pub clock: Arc<AtomicI64>,
    pub acs: Arc<Mutex<HashMap<String, AcItem>>>,
    /// `true`: tabel AC tidak bisa dibaca.
    pub acs_fail: Arc<Mutex<bool>>,
    pub replies: Arc<Mutex<Vec<mandor_domain::Reply>>>,
    /// `true`: API komentar gagal (fallback Fix Notes).
    pub replies_fail: Arc<Mutex<bool>>,
    pub names: Arc<Mutex<HashMap<String, String>>>,
    pub sprints: Arc<Mutex<HashMap<String, SprintItem>>>,
    /// ID sprint yang diminta per panggilan.
    pub sprint_calls: Arc<Mutex<Vec<Vec<String>>>>,
    /// `true`: tabel sprint tidak bisa dibaca.
    pub sprints_fail: Arc<Mutex<bool>>,
    /// ID yang diminta ke Contact API per panggilan.
    pub name_calls: Arc<Mutex<Vec<Vec<String>>>>,
    /// `true`: Contact API gagal (scope belum disetujui).
    pub names_fail: Arc<Mutex<bool>>,
    seq: Arc<AtomicI64>,
}

impl FakeLark {
    pub fn put(&self, t: Task) {
        self.records.lock().unwrap().insert(t.record_id.clone(), t);
    }
    pub fn get(&self, id: &str) -> Option<Task> {
        self.records.lock().unwrap().get(id).cloned()
    }
    pub fn remove(&self, id: &str) {
        self.records.lock().unwrap().remove(id);
    }
    pub fn edit(&self, id: &str, f: impl FnOnce(&mut Task)) {
        let mut recs = self.records.lock().unwrap();
        let t = recs.get_mut(id).unwrap();
        f(t);
        t.last_modified_ms += 1;
    }
    pub fn writes(&self) -> Vec<Write> {
        self.writes.lock().unwrap().clone()
    }
}

impl LarkTasks for FakeLark {
    async fn sprints(&self, ids: &[String]) -> Result<Vec<SprintItem>, PortError> {
        self.sprint_calls.lock().unwrap().push(ids.to_vec());
        if *self.sprints_fail.lock().unwrap() {
            return Err(PortError::Rejected("sprint table".to_owned()));
        }
        let s = self.sprints.lock().unwrap();
        Ok(ids.iter().filter_map(|i| s.get(i).cloned()).collect())
    }
    async fn user_names(&self, ids: &[String]) -> Result<HashMap<String, String>, PortError> {
        self.name_calls.lock().unwrap().push(ids.to_vec());
        if *self.names_fail.lock().unwrap() {
            return Err(PortError::Rejected("contact scope".to_owned()));
        }
        let names = self.names.lock().unwrap();
        Ok(ids
            .iter()
            .filter_map(|i| names.get(i).map(|n| (i.clone(), n.clone())))
            .collect())
    }
    async fn record_replies(
        &self,
        _stop_before: Option<i64>,
    ) -> Result<Vec<mandor_domain::Reply>, PortError> {
        if *self.replies_fail.lock().unwrap() {
            return Err(PortError::Rejected("comment api".to_owned()));
        }
        Ok(self.replies.lock().unwrap().clone())
    }
    async fn acceptance_criteria(&self, ids: &[String]) -> Result<Vec<AcItem>, PortError> {
        if *self.acs_fail.lock().unwrap() {
            return Err(PortError::Transient("AC table timeout".to_owned()));
        }
        let acs = self.acs.lock().unwrap();
        Ok(ids.iter().filter_map(|i| acs.get(i).cloned()).collect())
    }

    async fn search_assigned(&self, me: &str, since: Option<i64>) -> Result<Vec<Task>, PortError> {
        self.searches.lock().unwrap().push(since);
        let out = self
            .records
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.assigned_to(me))
            .filter(|t| since.is_none_or(|s| t.last_modified_ms >= s))
            .cloned()
            .collect();
        Ok(out)
    }

    async fn search_team(&self, done_since_ms: i64) -> Result<Vec<Task>, PortError> {
        let out = self
            .records
            .lock()
            .unwrap()
            .values()
            .filter(|t| {
                t.column() != mandor_domain::Column::Done || t.last_modified_ms >= done_since_ms
            })
            .cloned()
            .collect();
        Ok(out)
    }

    async fn fetch(&self, ids: &[String]) -> Result<Vec<Task>, PortError> {
        let out = {
            let recs = self.records.lock().unwrap();
            ids.iter().filter_map(|i| recs.get(i).cloned()).collect()
        };
        // Beri kesempatan "laptop" lain berjalan di antara baca dan tulis.
        tokio::task::yield_now().await;
        Ok(out)
    }

    async fn write(
        &self,
        record_id: &str,
        status: Option<&TaskStatus>,
        fields: &[AppField],
    ) -> Result<Task, PortError> {
        {
            let mut errs = self.write_errors.lock().unwrap();
            if !errs.is_empty() {
                return Err(errs.remove(0));
            }
        }
        self.writes
            .lock()
            .unwrap()
            .push((record_id.to_owned(), status.cloned(), fields.to_vec()));
        let mut recs = self.records.lock().unwrap();
        let t = recs
            .get_mut(record_id)
            .ok_or(PortError::Rejected("RecordIdNotFound".into()))?;
        t.apply_write(status, fields);
        t.last_modified_ms =
            self.clock.load(Ordering::SeqCst) + self.seq.fetch_add(1, Ordering::SeqCst);
        // Seperti Bitable: respons update hanya memuat field yang ditulis.
        let mut partial = Task::new(
            record_id,
            status
                .cloned()
                .unwrap_or(TaskStatus::Unknown(String::new())),
        );
        partial.apply_write(None, fields);
        partial.last_modified_ms = t.last_modified_ms;
        Ok(partial)
    }
}

#[derive(Clone, Default)]
pub struct FakeCache {
    pub rows: Arc<Mutex<HashMap<String, CachedTask>>>,
}

impl FakeCache {
    pub fn flag(&self, id: &str) -> Option<CardFlag> {
        self.rows
            .lock()
            .unwrap()
            .get(id)
            .and_then(|c| c.flag.clone())
    }
    pub fn task(&self, id: &str) -> Option<Task> {
        self.rows.lock().unwrap().get(id).map(|c| c.task.clone())
    }
}

impl TaskCache for FakeCache {
    async fn upsert(&self, task: &Task) -> Result<(), PortError> {
        let mut rows = self.rows.lock().unwrap();
        let flag = rows.get(&task.record_id).and_then(|c| c.flag.clone());
        rows.insert(
            task.record_id.clone(),
            CachedTask {
                task: task.clone(),
                flag,
            },
        );
        Ok(())
    }
    async fn get(&self, id: &str) -> Result<Option<CachedTask>, PortError> {
        Ok(self.rows.lock().unwrap().get(id).cloned())
    }
    async fn list(&self) -> Result<Vec<CachedTask>, PortError> {
        Ok(self.rows.lock().unwrap().values().cloned().collect())
    }
    async fn set_flag(&self, id: &str, flag: Option<&CardFlag>) -> Result<(), PortError> {
        if let Some(c) = self.rows.lock().unwrap().get_mut(id) {
            c.flag = flag.cloned();
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutState {
    Pending { next_at: i64 },
    Done,
    Cancelled(String),
    Failed(String),
}

#[derive(Clone, Default)]
pub struct FakeOutbox {
    pub items: Arc<Mutex<Vec<(QueuedWrite, OutState)>>>,
}

impl FakeOutbox {
    pub fn states(&self) -> Vec<(String, OutState)> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .map(|(q, s)| (q.record_id.clone(), s.clone()))
            .collect()
    }
    pub fn ops(&self) -> Vec<WriteOp> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .map(|(q, _)| q.op.clone())
            .collect()
    }
    fn update(&self, id: i64, f: impl FnOnce(&mut QueuedWrite, &mut OutState)) {
        let mut items = self.items.lock().unwrap();
        let (q, s) = items.iter_mut().find(|(q, _)| q.id == id).unwrap();
        f(q, s);
    }
}

impl Outbox for FakeOutbox {
    async fn enqueue(&self, record_id: &str, op: &WriteOp, now: i64) -> Result<i64, PortError> {
        let mut items = self.items.lock().unwrap();
        let id = i64::try_from(items.len()).unwrap() + 1;
        let q = QueuedWrite {
            id,
            record_id: record_id.to_owned(),
            op: op.clone(),
            attempts: 0,
        };
        items.push((q, OutState::Pending { next_at: now }));
        Ok(id)
    }
    async fn due(&self, now: i64, limit: u32) -> Result<Vec<QueuedWrite>, PortError> {
        Ok(self
            .items
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| matches!(s, OutState::Pending { next_at } if *next_at <= now))
            .take(limit as usize)
            .map(|(q, _)| q.clone())
            .collect())
    }
    async fn complete(&self, id: i64) -> Result<(), PortError> {
        self.update(id, |_, s| *s = OutState::Done);
        Ok(())
    }
    async fn cancel(&self, id: i64, reason: &str) -> Result<(), PortError> {
        self.update(id, |_, s| *s = OutState::Cancelled(reason.to_owned()));
        Ok(())
    }
    async fn retry_later(
        &self,
        id: i64,
        attempts: u32,
        next: i64,
        _e: &str,
    ) -> Result<(), PortError> {
        self.update(id, |q, s| {
            q.attempts = attempts;
            *s = OutState::Pending { next_at: next };
        });
        Ok(())
    }
    async fn fail(&self, id: i64, e: &str) -> Result<(), PortError> {
        self.update(id, |_, s| *s = OutState::Failed(e.to_owned()));
        Ok(())
    }
    async fn requeue_failed(&self, record_id: &str, now: i64) -> Result<u32, PortError> {
        let mut n = 0;
        for (q, s) in self.items.lock().unwrap().iter_mut() {
            if q.record_id == record_id && matches!(s, OutState::Failed(_)) {
                q.attempts = 0;
                *s = OutState::Pending { next_at: now };
                n += 1;
            }
        }
        Ok(n)
    }
}

/// Jam tiruan: `sleep` memajukan waktu lalu memberi giliran ke task lain.
#[derive(Clone, Default)]
pub struct FakeClock {
    pub now: Arc<AtomicI64>,
    pub slept: Arc<Mutex<Vec<Duration>>>,
}

impl FakeClock {
    pub fn set(&self, ms: i64) {
        self.now.store(ms, Ordering::SeqCst);
    }
    pub fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
    async fn sleep(&self, d: Duration) {
        self.slept.lock().unwrap().push(d);
        self.now
            .fetch_add(i64::try_from(d.as_millis()).unwrap(), Ordering::SeqCst);
        tokio::task::yield_now().await;
    }
}

#[derive(Clone, Default)]
pub struct FakeIds {
    pub prefix: String,
    n: Arc<AtomicU32>,
}

impl FakeIds {
    pub fn with_prefix(p: &str) -> Self {
        Self {
            prefix: p.to_owned(),
            n: Arc::default(),
        }
    }
}

impl Ids for FakeIds {
    fn nonce(&self) -> String {
        format!("{}{}", self.prefix, self.n.fetch_add(1, Ordering::SeqCst))
    }
}

pub struct FakeDeps;

impl Deps for FakeDeps {
    type Lark = FakeLark;
    type Cache = FakeCache;
    type Outbox = FakeOutbox;
    type Clock = FakeClock;
    type Ids = FakeIds;
    type Git = run_fakes::FakeGit;
    type Agent = run_fakes::FakeAgent;
    type Runs = run_fakes::FakeRuns;
    type Notifier = run_fakes::FakeNotifier;
    type Spawner = run_fakes::TokioSpawner;
    type Repos = run_fakes::FakeRepos;
    type Prompts = PlainMessage;
    type Reviews = run_fakes::FakeReviews;
}

pub mod run_fakes;

/// Satu "laptop": layanan sinkron beserta fake miliknya; Lark dan jam dibagi.
pub struct Laptop {
    pub svc: SyncService<FakeDeps>,
    pub cache: FakeCache,
    pub outbox: FakeOutbox,
}

pub fn laptop(lark: &FakeLark, clock: &FakeClock, me: &str) -> Laptop {
    let cache = FakeCache::default();
    let outbox = FakeOutbox::default();
    let svc = SyncService::new(
        lark.clone(),
        cache.clone(),
        outbox.clone(),
        clock.clone(),
        FakeIds::with_prefix(me),
        me.to_owned(),
    );
    Laptop { svc, cache, outbox }
}

/// Task siap dikerjakan, di-assign ke `who`.
pub fn ready_task(id: &str, who: &[&str]) -> Task {
    let mut t = Task::new(id, TaskStatus::ToDo);
    t.title = format!("Judul {id}");
    t.readiness = Some("Ready To Dev".to_owned());
    t.assignees = who.iter().map(|s| (*s).to_owned()).collect();
    t.last_modified_ms = 1;
    t
}

/// Satu laptop lengkap dengan layanan run dan fake miliknya.
pub struct Station {
    pub svc: Arc<mandor_usecase::RunService<FakeDeps>>,
    pub cache: FakeCache,
    pub outbox: FakeOutbox,
    pub git: run_fakes::FakeGit,
    pub agent: run_fakes::FakeAgent,
    pub runs: run_fakes::FakeRuns,
    pub notifier: run_fakes::FakeNotifier,
    pub repos: run_fakes::FakeRepos,
    pub reviews: run_fakes::FakeReviews,
}

pub fn station(lark: &FakeLark, clock: &FakeClock, me: &str) -> Station {
    let Laptop { svc, cache, outbox } = laptop(lark, clock, me);
    let git = run_fakes::FakeGit::default();
    let agent = run_fakes::FakeAgent::default();
    let runs = run_fakes::FakeRuns::default();
    let notifier = run_fakes::FakeNotifier::default();
    let repos = run_fakes::FakeRepos::default();
    let reviews = run_fakes::FakeReviews::default();
    let ports = mandor_usecase::RunPorts::<FakeDeps> {
        git: git.clone(),
        agent: agent.clone(),
        runs: runs.clone(),
        notifier: notifier.clone(),
        spawner: run_fakes::TokioSpawner::default(),
        repos: repos.clone(),
        prompts: PlainMessage,
        reviews: reviews.clone(),
    };
    let svc = Arc::new(mandor_usecase::RunService::new(Arc::new(svc), ports));
    Station {
        svc,
        cache,
        outbox,
        git,
        agent,
        runs,
        notifier,
        repos,
        reviews,
    }
}

/// Penyusun pesan task sederhana untuk test.
pub struct PlainMessage;

impl mandor_usecase::run_ports::Prompts for PlainMessage {
    fn task_message(&self, task: &Task, acs: &[String], dev_note: Option<&str>) -> String {
        format!(
            "{}\n{}\nAC:{}\n{}",
            task.title,
            task.description,
            acs.join("|"),
            dev_note.unwrap_or("")
        )
    }

    fn plan_message(
        &self,
        task: &Task,
        _acs: &[String],
        dev_note: Option<&str>,
        skill: Option<&str>,
    ) -> String {
        format!(
            "PLAN {}\n{}\nSKILL:{}",
            task.title,
            dev_note.unwrap_or(""),
            skill.unwrap_or("-")
        )
    }

    fn approve_message(&self) -> String {
        "APPROVED".to_owned()
    }

    fn brainstorm_message(
        &self,
        task: &Task,
        _acs: &[String],
        dev_note: Option<&str>,
        skill: Option<&str>,
    ) -> String {
        format!(
            "BRAINSTORM {}\n{}\nSKILL:{}",
            task.title,
            dev_note.unwrap_or(""),
            skill.unwrap_or("-")
        )
    }

    fn command_message(&self, name: &str, args: &str) -> String {
        format!("COMMAND {name} ARGS {args}")
    }

    fn continue_plan_message(&self, skill: Option<&str>) -> String {
        format!("CONTINUE SKILL:{}", skill.unwrap_or("-"))
    }

    fn fix_message(&self, ctx: &mandor_usecase::run_ports::FixContext<'_>) -> String {
        let comments: Vec<String> = ctx
            .comments
            .iter()
            .map(|c| format!("[{}] {}", c.author, c.text))
            .collect();
        format!(
            "FIX {}\nAC:{}\nDIFF:{}\nKOMENTAR:{}\nINSTRUKSI:{}",
            ctx.task.title,
            ctx.acs.join("|"),
            ctx.diff.unwrap_or("-"),
            comments.join("|"),
            ctx.instruction.unwrap_or("-")
        )
    }
}
