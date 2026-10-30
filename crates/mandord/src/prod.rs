//! Wiring produksi: bundel [`Deps`] dan adapter kecil milik daemon.

use std::time::Duration;

use mandor_agent::{ClaudeAgent, ClaudePrompts};
use mandor_domain::{CardFlag, Task};
use mandor_git::GitAdapter;
use mandor_lark::LarkGateway;
use mandor_protocol::{NoticeView, Notification, RunEventView, event};
use mandor_store::{ReposStore, ReviewsStore, RunsStore, Store, TaskStore};
use mandor_usecase::run_ports::{
    ChatItem, GroupCardRow, Journal, LedgerStore, Notice, Notifier, RunRecord, RunStore, Spawner,
};
use mandor_usecase::{CachedTask, Clock, Deps, Ids, PortError, TaskCache};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_util::task::TaskTracker;

/// Bundel port produksi.
#[derive(Debug)]
pub struct Prod;

impl Deps for Prod {
    type Lark = LarkGateway;
    type Cache = BroadcastTasks;
    type Outbox = TaskStore;
    type Clock = SystemClock;
    type Ids = UuidIds;
    type Git = GitAdapter;
    type Agent = ClaudeAgent;
    type Reviews = BroadcastReviews;
    type Apps = crate::apps::BroadcastApps;
    type Runs = BroadcastRuns;
    type Notifier = DaemonNotifier;
    type Spawner = TrackerSpawner;
    type Repos = ReposStore;
    type Prompts = ClaudePrompts;
    type Overnight = mandor_store::OvernightsStore;
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Jam sistem.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        now_ms()
    }

    async fn sleep(&self, d: Duration) {
        tokio::time::sleep(d).await;
    }
}

/// ID acak (UUID v4).
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidIds;

impl Ids for UuidIds {
    fn nonce(&self) -> String {
        mandor_agent::new_session_id()
    }
}

/// Spawner dengan pemilik `TaskTracker` daemon.
#[derive(Debug, Clone)]
pub struct TrackerSpawner {
    tracker: TaskTracker,
}

impl TrackerSpawner {
    pub fn new(tracker: TaskTracker) -> Self {
        Self { tracker }
    }
}

impl Spawner for TrackerSpawner {
    fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.tracker.spawn(fut);
    }
}

/// Mengirim event ke semua client tersambung (gagal kirim = tidak ada client).
pub fn emit(events: &broadcast::Sender<Notification>, method: &str, params: Value) {
    let _ = events.send(Notification::new(method, params));
}

/// Notifikasi: disimpan untuk client yang belum tersambung, lalu di-broadcast.
#[derive(Debug, Clone)]
pub struct DaemonNotifier {
    store: Store,
    events: broadcast::Sender<Notification>,
}

impl DaemonNotifier {
    pub fn new(store: Store, events: broadcast::Sender<Notification>) -> Self {
        Self { store, events }
    }
}

impl Notifier for DaemonNotifier {
    async fn notify(&self, n: Notice) {
        let mut view = NoticeView {
            id: 0,
            read: false,
            kind: n.kind.as_str().to_owned(),
            record_id: n.record_id,
            run_id: n.run_id,
            title: n.title,
            body: n.body,
            created_at_ms: now_ms(),
        };
        let payload = serde_json::to_value(&view).unwrap_or(Value::Null);
        // Selalu masuk riwayat; tanpa client, notifikasi diambil saat tersambung.
        let connected = self.events.receiver_count() > 0;
        match self
            .store
            .push_notification(&view.kind, &payload, view.created_at_ms, connected)
            .await
        {
            Ok(id) => view.id = id,
            Err(e) => tracing::warn!(error = %e, "notifikasi tidak tersimpan"),
        }
        if connected {
            emit(
                &self.events,
                event::NOTICE,
                serde_json::to_value(&view).unwrap_or(Value::Null),
            );
        }
    }
}

/// Cache task yang memberi tahu client saat papan berubah.
#[derive(Debug, Clone)]
pub struct BroadcastTasks {
    inner: TaskStore,
    events: broadcast::Sender<Notification>,
}

impl BroadcastTasks {
    pub fn new(inner: TaskStore, events: broadcast::Sender<Notification>) -> Self {
        Self { inner, events }
    }

    fn changed(&self) {
        emit(&self.events, event::BOARD_UPDATED, json!({}));
    }
}

impl TaskCache for BroadcastTasks {
    async fn upsert(&self, task: &Task) -> Result<(), PortError> {
        self.inner.upsert(task).await?;
        self.changed();
        Ok(())
    }

    async fn get(&self, record_id: &str) -> Result<Option<CachedTask>, PortError> {
        self.inner.get(record_id).await
    }

    async fn list(&self) -> Result<Vec<CachedTask>, PortError> {
        self.inner.list().await
    }

    async fn set_flag(&self, record_id: &str, flag: Option<&CardFlag>) -> Result<(), PortError> {
        self.inner.set_flag(record_id, flag).await?;
        self.changed();
        Ok(())
    }
}

/// Penyimpanan run yang memberi tahu client saat run atau chat berubah.
#[derive(Debug, Clone)]
pub struct BroadcastRuns {
    inner: RunsStore,
    events: broadcast::Sender<Notification>,
}

impl BroadcastRuns {
    pub fn new(inner: RunsStore, events: broadcast::Sender<Notification>) -> Self {
        Self { inner, events }
    }

    fn run_event(&self, method: &str, run_id: &str) {
        let view = RunEventView {
            run_id: run_id.to_owned(),
        };
        emit(
            &self.events,
            method,
            serde_json::to_value(view).unwrap_or(Value::Null),
        );
    }
}

impl RunStore for BroadcastRuns {
    async fn create(&self, run: &RunRecord, now_ms: i64) -> Result<(), PortError> {
        self.inner.create(run, now_ms).await?;
        self.run_event(event::RUN_UPDATED, &run.id);
        Ok(())
    }

    async fn create_group(
        &self,
        run: &RunRecord,
        cards: &[GroupCardRow],
        now_ms: i64,
    ) -> Result<(), PortError> {
        self.inner.create_group(run, cards, now_ms).await?;
        self.run_event(event::RUN_UPDATED, &run.id);
        Ok(())
    }

    async fn cards(&self, run_id: &str) -> Result<Vec<GroupCardRow>, PortError> {
        self.inner.cards(run_id).await
    }

    async fn save_cards(
        &self,
        run_id: &str,
        cards: &[GroupCardRow],
        primary: Option<&str>,
    ) -> Result<(), PortError> {
        self.inner.save_cards(run_id, cards, primary).await?;
        self.run_event(event::RUN_UPDATED, run_id);
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<RunRecord>, PortError> {
        self.inner.get(id).await
    }

    async fn active_for(&self, record_id: &str) -> Result<Option<RunRecord>, PortError> {
        self.inner.active_for(record_id).await
    }

    async fn list_active(&self) -> Result<Vec<RunRecord>, PortError> {
        self.inner.list_active().await
    }

    async fn save(&self, run: &RunRecord, expected: i64, now_ms: i64) -> Result<(), PortError> {
        self.inner.save(run, expected, now_ms).await?;
        self.run_event(event::RUN_UPDATED, &run.id);
        Ok(())
    }

    async fn close(&self, id: &str, now_ms: i64) -> Result<(), PortError> {
        self.inner.close(id, now_ms).await?;
        self.run_event(event::RUN_UPDATED, id);
        Ok(())
    }

    async fn append(&self, run_id: &str, item: &ChatItem, now_ms: i64) -> Result<(), PortError> {
        self.inner.append(run_id, item, now_ms).await?;
        self.run_event(event::RUN_MESSAGE, run_id);
        Ok(())
    }
}

impl Journal for BroadcastRuns {
    async fn journal_pending(
        &self,
        record_id: &str,
        reply_id: &str,
        run_id: &str,
        now_ms: i64,
    ) -> Result<(), PortError> {
        self.inner
            .journal_pending(record_id, reply_id, run_id, now_ms)
            .await
    }

    async fn journal_delivered(&self, record_id: &str, reply_id: &str) -> Result<(), PortError> {
        self.inner.journal_delivered(record_id, reply_id).await
    }

    async fn journal_requeue(&self, record_id: &str, reply_id: &str) -> Result<(), PortError> {
        self.inner.journal_requeue(record_id, reply_id).await
    }

    async fn journal_pending_list(&self, record_id: &str) -> Result<Vec<String>, PortError> {
        self.inner.journal_pending_list(record_id).await
    }

    async fn journal_delivered_list(&self, record_id: &str) -> Result<Vec<String>, PortError> {
        self.inner.journal_delivered_list(record_id).await
    }
}

/// Penyimpanan Rilis 2 yang memancarkan `ledger.updated` setiap buku AC berubah.
#[derive(Debug, Clone)]
pub struct BroadcastReviews {
    inner: ReviewsStore,
    events: broadcast::Sender<Notification>,
}

impl BroadcastReviews {
    pub fn new(inner: ReviewsStore, events: broadcast::Sender<Notification>) -> Self {
        Self { inner, events }
    }
}

impl LedgerStore for BroadcastReviews {
    async fn ac_rows(&self, run_id: &str) -> Result<Vec<mandor_domain::AcRow>, PortError> {
        self.inner.ac_rows(run_id).await
    }

    async fn save_ac_rows(
        &self,
        run_id: &str,
        rows: &[mandor_domain::AcRow],
    ) -> Result<(), PortError> {
        self.inner.save_ac_rows(run_id, rows).await?;
        let view = mandor_protocol::LedgerRowsEvent {
            run_id: run_id.to_owned(),
            rows: rows.iter().map(crate::views::ac_row_view).collect(),
        };
        emit(
            &self.events,
            event::LEDGER_UPDATED,
            serde_json::to_value(view).unwrap_or(Value::Null),
        );
        Ok(())
    }
}

impl mandor_usecase::run_ports::ReviewHistory for BroadcastReviews {
    async fn save_review(
        &self,
        r: &mandor_usecase::run_ports::ReviewRecord,
    ) -> Result<(), PortError> {
        self.inner.save_review(r).await?;
        let view = RunEventView {
            run_id: r.run_id.clone(),
        };
        emit(
            &self.events,
            event::REVIEW_PROGRESS,
            serde_json::to_value(view).unwrap_or(Value::Null),
        );
        Ok(())
    }
    async fn reviews(
        &self,
        run_id: &str,
    ) -> Result<Vec<mandor_usecase::run_ports::ReviewRecord>, PortError> {
        self.inner.reviews(run_id).await
    }
    async fn reviewer_settings(&self) -> mandor_usecase::run_ports::ReviewerSettings {
        self.inner.reviewer_settings().await
    }
    async fn set_reviewer_settings(
        &self,
        s: &mandor_usecase::run_ports::ReviewerSettings,
    ) -> Result<(), PortError> {
        self.inner.set_reviewer_settings(s).await
    }
    async fn chrome_connected(&self) -> bool {
        self.inner.chrome_connected().await
    }
}
