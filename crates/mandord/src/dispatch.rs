//! Penghubung method JSON-RPC ke use case.

use std::collections::HashMap;
use std::sync::Arc;

use mandor_api::Dispatch;
use mandor_protocol::{
    ApprovePlanParams, BoardView, CommentView, DirEntryView, DirListingView, DirQuery, DrainResult,
    ErrorCode, FileChangeView, FileDiffParams, LarkSettingsInput, LoginComplete, MessageView,
    MessagesQuery, NoticeView, NotificationsRead, PeopleQuery, PersonView, PublishParams,
    PublishResult, QuietPeriodSet, RecordRef, RepoAdd, RepoCandidateView, RepoRef, RepoUpdate,
    RepoView, RpcError, RunRef, SendParams, SendResult, SkillView, StartFixParams, StartParams,
    StartPlanning, StartProgressView, StatusResult, TakeoverParams, event, method,
};
use mandor_store::{NewRepo, RunsStore, StoreError};
use mandor_usecase::run::{DrainOutcome, PublishOutcome};
use mandor_usecase::run_ports::RunStore as _;
use mandor_usecase::{
    Planning, PortError, RunError, SendOutcome, StartStep, StartTarget, SyncError,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::VERSION;
use crate::app::App;
use crate::prod::{emit, now_ms};
use crate::session::LarkSession;
use crate::views::{
    apply_group, apply_holder, card_view, notice_view, run_view, sprint_view, task_details_view,
    team_card_view,
};

mod app_run;
mod design;
mod group;
mod overnight;
mod review;
mod workspaces;

/// Method yang membutuhkan sesi Lark dan mengubah run atau kartu.
const RUN_METHODS: &[&str] = &[
    method::RUNS_START,
    method::RUNS_SEND,
    method::RUNS_SEND_NOW,
    method::RUNS_STOP,
    method::RUNS_PUBLISH,
    method::RUNS_RESUME,
    method::RUNS_START_FIX,
    method::RUNS_TAKEOVER,
    method::RUNS_CLOSE,
    method::RUNS_DISCARD_FILE,
    method::RUNS_APPROVE_PLAN,
    method::RUNS_CONTINUE_TO_PLAN,
    method::CARD_RETRY,
];

/// Kedalaman folder yang dipindai untuk saran repo (mis. `~/Work/tim/repo`).
const DISCOVER_DEPTH: usize = 3;
/// Batas jumlah saran repo.
const DISCOVER_MAX: usize = 50;
/// Batas `open_id` per permintaan `people.names`.
const PEOPLE_MAX: usize = 200;
/// Notifikasi yang ditampilkan di notification center.
const NOTIFICATIONS_SHOWN: i64 = 50;
/// Batas ID per permintaan `notifications.read`.
const NOTIFICATIONS_KEPT_IDS: usize = 500;

/// Batas pesan per halaman.
const MESSAGES_MAX: i64 = 500;

/// Pilihan planning dari client; tanpa pilihan = langsung implementasi.
fn planning_of(p: Option<&StartPlanning>) -> Result<Planning, RpcError> {
    let Some(p) = p else {
        return Ok(Planning::Skip);
    };
    let skill = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    match p.kind.as_str() {
        "skip" => Ok(Planning::Skip),
        "plan" => Ok(Planning::Plan {
            skill: skill(&p.plan_skill),
        }),
        "brainstorm" => Ok(Planning::Brainstorm {
            brainstorm: skill(&p.brainstorm_skill),
            plan: skill(&p.plan_skill),
        }),
        _ => Err(rejected("unknown planning choice")),
    }
}

/// Worktree satu repo dalam run.
struct RunTree {
    repo_id: String,
    worktree: std::path::PathBuf,
    base: String,
}

/// Dispatcher daemon.
#[derive(Debug)]
pub struct AppDispatch {
    app: Arc<App>,
}

impl AppDispatch {
    pub fn new(app: Arc<App>) -> Self {
        Self { app }
    }
}

fn params<T: DeserializeOwned>(v: Value) -> Result<T, RpcError> {
    serde_json::from_value(v).map_err(|e| RpcError::new(ErrorCode::InvalidParams, e.to_string()))
}

fn ok<T: Serialize>(v: &T) -> Result<Value, RpcError> {
    serde_json::to_value(v).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))
}

fn internal(e: impl std::fmt::Display) -> RpcError {
    tracing::error!(error = %e, "rpc gagal");
    RpcError::new(ErrorCode::Internal, e.to_string())
}

fn rejected(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(ErrorCode::Rejected, e.to_string())
}

fn port_error(e: &PortError) -> RpcError {
    match e {
        PortError::Reauth => RpcError::new(ErrorCode::Unauthorized, e.to_string()),
        PortError::Conflict => RpcError::new(ErrorCode::Conflict, e.to_string()),
        PortError::Storage(_) => internal(e),
        _ => rejected(e),
    }
}

fn run_error(e: RunError) -> RpcError {
    match &e {
        RunError::NotFound(_) => RpcError::new(ErrorCode::NotFound, e.to_string()),
        RunError::InvalidState(_) | RunError::Draining => {
            RpcError::new(ErrorCode::Conflict, e.to_string())
        }
        RunError::Port(p) | RunError::Sync(SyncError::Port(p)) => port_error(p),
        _ => rejected(e),
    }
}

fn sync_error(e: SyncError) -> RpcError {
    match &e {
        SyncError::Port(p) => port_error(p),
        _ => rejected(e),
    }
}

fn store_error(e: StoreError) -> RpcError {
    match e {
        StoreError::NotFound => RpcError::new(ErrorCode::NotFound, e.to_string()),
        StoreError::Conflict(_) => RpcError::new(ErrorCode::Conflict, e.to_string()),
        StoreError::Internal(_) => internal(e),
    }
}

impl AppDispatch {
    async fn session(&self) -> Result<Arc<LarkSession>, RpcError> {
        self.app
            .session()
            .await
            .ok_or_else(|| RpcError::new(ErrorCode::Unauthorized, "log in to Lark first"))
    }

    async fn status(&self) -> Result<Value, RpcError> {
        let repos = self.app.store.list_repos().await.map_err(store_error)?;
        let active = RunsStore::new(self.app.store.clone())
            .list_active()
            .await
            .map_err(internal)?;
        let power = self.app.power.get();
        let lark = self.app.lark_status().await;
        let logins = match &self.app.cfg.vm_agent {
            Some(vm) => Some(crate::logins::check(&self.app.cfg, vm, &lark).await),
            None => None,
        };
        ok(&StatusResult {
            version: VERSION.to_owned(),
            uptime_ms: u64::try_from(self.app.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            repos: repos.len() as u64,
            lark,
            busy: Some(mandor_domain::run::any_working(
                active.iter().map(|r| r.state),
            )),
            managed_by: Some(self.app.cfg.managed_by.as_str().to_owned()),
            draining: Some(self.app.drain.is_draining(now_ms())),
            keep_awake: Some(power.state.as_str().to_owned()),
            keep_awake_error: power.error,
            logins,
        })
    }

    async fn drain(&self) -> Result<Value, RpcError> {
        let outcome = self
            .app
            .drain
            .drain(&RunsStore::new(self.app.store.clone()), now_ms())
            .await
            .map_err(|e| port_error(&e))?;
        let state = match outcome {
            DrainOutcome::Idle => "idle",
            DrainOutcome::Busy => "busy",
        };
        ok(&DrainResult {
            state: state.to_owned(),
        })
    }

    async fn board(&self) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let cached = s.sync.board().await.map_err(sync_error)?;
        let active = s.runs.active_runs().await.map_err(run_error)?;
        let groups = group::group_views(&s, &active).await?;
        // Run grup tampil di setiap kartunya yang masih disertakan.
        let mut runs: HashMap<String, _> = HashMap::new();
        for r in active {
            if let Some(cards) = groups.get(&r.id) {
                for c in cards.iter().filter(|c| !c.state.starts_with("removed")) {
                    runs.insert(c.record_id.clone(), r.clone());
                }
            }
            runs.insert(r.record_id.clone(), r);
        }
        let now = now_ms();
        let team = s.sync.team_board();
        let summaries = self.ac_summaries().await?;
        let mut cards: Vec<_> = cached
            .iter()
            .map(|c| {
                let run = runs.get(&c.task.record_id);
                let mut card = card_view(c, run, &s.me, now);
                apply_holder(&mut card, &c.task, s.sync.identity());
                if let (Some(view), Some(r)) = (card.run.as_mut(), run) {
                    view.ac = summaries.get(&r.id).cloned();
                }
                if let Some(cards) = run.and_then(|r| groups.get(&r.id)) {
                    apply_group(&mut card, cards);
                }
                card
            })
            .collect();
        cards.extend(
            team.iter()
                .filter(|t| !cached.iter().any(|c| c.task.record_id == t.record_id))
                .map(|t| team_card_view(t, &s.me, now)),
        );
        let sprint_ids: Vec<String> = cached
            .iter()
            .map(|c| &c.task)
            .chain(team.iter())
            .flat_map(|t| t.sprint_ids.iter().cloned())
            .collect();
        let sprints = s
            .sync
            .sprints(&sprint_ids)
            .await
            .iter()
            .map(sprint_view)
            .collect();
        let sync = s.sync_status.snapshot();
        ok(&BoardView {
            me: Some(s.me.clone()),
            cards,
            sprints,
            synced_at_ms: sync.synced_at_ms,
            sync_error: sync.error,
        })
    }

    async fn runs_list(&self) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let summaries = self.ac_summaries().await?;
        let active = s.runs.active_runs().await.map_err(run_error)?;
        let mut groups = group::group_views(&s, &active).await?;
        let mut out = Vec::new();
        for r in active {
            let task = s.sync.cached(&r.record_id).await.map_err(sync_error)?;
            let mut view = run_view(&r, task.as_ref().map(|c| &c.task));
            view.ac = summaries.get(&r.id).cloned();
            view.cards = groups.remove(&r.id).unwrap_or_default();
            out.push(view);
        }
        ok(&out)
    }

    /// Ringkasan buku AC semua run aktif (satu query).
    async fn ac_summaries(
        &self,
    ) -> Result<HashMap<String, mandor_protocol::AcSummaryView>, RpcError> {
        let mut by_run: HashMap<String, Vec<mandor_domain::AcRow>> = HashMap::new();
        for (run_id, row) in self.app.store.active_ac_rows().await.map_err(store_error)? {
            by_run.entry(run_id).or_default().push(row);
        }
        Ok(by_run
            .into_iter()
            .filter_map(|(id, rows)| crate::views::ac_summary(&rows).map(|s| (id, s)))
            .collect())
    }

    async fn messages(&self, q: MessagesQuery) -> Result<Value, RpcError> {
        let limit = q.limit.clamp(1, MESSAGES_MAX);
        let msgs = self
            .app
            .store
            .list_messages(&q.run_id, q.after_id, limit)
            .await
            .map_err(store_error)?;
        let out: Vec<MessageView> = msgs
            .into_iter()
            .map(|m| MessageView {
                id: m.id,
                role: m.role,
                body: m.body,
                created_at_ms: m.created_at_ms,
            })
            .collect();
        ok(&out)
    }

    /// Worktree setiap repo run beserta base branch-nya, repo utama pertama.
    /// Run workspace menampilkan path berkas dengan awalan `<repo>/`.
    async fn run_worktrees(&self, run_id: &str) -> Result<(bool, Vec<RunTree>), RpcError> {
        let s = self.session().await?;
        let run = s
            .runs
            .active_runs()
            .await
            .map_err(run_error)?
            .into_iter()
            .find(|r| r.id == run_id)
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "run not found"))?;
        let Some(primary) = run.worktree.clone() else {
            return Ok((false, Vec::new()));
        };
        let mut trees = vec![(run.repo_id.clone(), primary)];
        trees.extend(
            run.extra
                .iter()
                .map(|m| (m.repo_id.clone(), m.worktree.clone())),
        );
        let mut out = Vec::with_capacity(trees.len());
        for (repo_id, worktree) in trees {
            let repo = self
                .app
                .store
                .get_repo(&repo_id)
                .await
                .map_err(store_error)?;
            out.push(RunTree {
                base: repo.default_branch.unwrap_or_else(|| "main".to_owned()),
                repo_id,
                worktree,
            });
        }
        Ok((run.workspace.is_some(), out))
    }

    async fn changes(&self, r: RunRef) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let (multi, trees) = self.run_worktrees(&r.run_id).await?;
        let mut out: Vec<FileChangeView> = Vec::new();
        for t in &trees {
            let prefix = |p: String| {
                if multi {
                    format!("{}/{p}", t.repo_id)
                } else {
                    p
                }
            };
            let files = s
                .git
                .changed_files(&t.worktree, &t.base)
                .await
                .map_err(rejected)?;
            out.extend(files.into_iter().map(|f| FileChangeView {
                path: prefix(f.path),
                old_path: f.old_path.map(prefix),
                status: format!("{:?}", f.status).to_lowercase(),
                additions: f.additions,
                deletions: f.deletions,
            }));
        }
        ok(&out)
    }

    async fn file_diff(&self, p: FileDiffParams) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let (multi, trees) = self.run_worktrees(&p.run_id).await?;
        let target = if multi {
            p.path.split_once('/').and_then(|(repo, rest)| {
                trees.iter().find(|t| t.repo_id == repo).map(|t| (t, rest))
            })
        } else {
            trees.first().map(|t| (t, p.path.as_str()))
        };
        let (member, file) = target.ok_or_else(|| rejected("run has no worktree"))?;
        let patch = s
            .git
            .file_patch(&member.worktree, &member.base, file)
            .await
            .map_err(rejected)?;
        ok(&patch)
    }

    async fn people(&self, q: PeopleQuery) -> Result<Value, RpcError> {
        if q.open_ids.len() > PEOPLE_MAX {
            return Err(rejected("too many open_ids"));
        }
        let s = self.session().await?;
        let mut out: Vec<PersonView> = s
            .sync
            .user_names(&q.open_ids)
            .await
            .into_iter()
            .map(|(open_id, name)| PersonView { open_id, name })
            .collect();
        out.sort_by(|a, b| a.open_id.cmp(&b.open_id));
        ok(&out)
    }

    async fn details(&self, r: RecordRef) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let d = s
            .sync
            .task_details(&r.record_id)
            .await
            .map_err(sync_error)?;
        ok(&task_details_view(&d))
    }

    async fn comments(&self, r: RecordRef) -> Result<Value, RpcError> {
        let s = self.session().await?;
        let Some(cached) = s.sync.cached(&r.record_id).await.map_err(sync_error)? else {
            return Err(RpcError::new(ErrorCode::NotFound, "card not found"));
        };
        let reviewers = match &cached.task.repo_id {
            Some(id) => repo_settings(self.app.store.get_repo(id).await.ok().as_ref()).1,
            None => Vec::new(),
        };
        let out: Vec<CommentView> = s
            .runs
            .record_comments(&cached.task, &reviewers)
            .await
            .map_err(run_error)?
            .into_iter()
            .map(|c| CommentView {
                reply_id: c.reply.reply_id,
                author: c.reply.author,
                create_time: c.reply.create_time,
                text: c.reply.text,
                trusted: c.trusted,
                sent: c.sent,
            })
            .collect();
        ok(&out)
    }

    async fn repos_list(&self) -> Result<Value, RpcError> {
        let repos = self.app.store.list_repos().await.map_err(store_error)?;
        let out: Vec<RepoView> = repos.into_iter().map(repo_view).collect();
        ok(&out)
    }

    async fn repos_add(&self, p: RepoAdd) -> Result<Value, RpcError> {
        let path = tokio::fs::canonicalize(&p.path)
            .await
            .map_err(|e| rejected(format!("repo path not found: {e}")))?;
        let git = mandor_git::Git::new(mandor_git::GitConfig::new(self.app.cfg.workspaces_dir()));
        let default_branch = git.default_branch(&path).await.map_err(rejected)?;
        let base = path
            .file_name()
            .map_or_else(|| "repo".to_owned(), |n| n.to_string_lossy().to_lowercase());
        let slug: String = base
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let existing = self.app.store.list_repos().await.map_err(store_error)?;
        let mut id = slug.clone();
        let mut n = 2;
        while existing.iter().any(|r| r.id == id) {
            id = format!("{slug}-{n}");
            n += 1;
        }
        self.app
            .store
            .insert_repo(&NewRepo {
                id: id.clone(),
                path: path.display().to_string(),
                gh_profile: p.gh_profile.filter(|g| !g.is_empty()),
                default_branch: Some(default_branch),
                now_ms: now_ms(),
            })
            .await
            .map_err(store_error)?;
        ok(&repo_view(
            self.app.store.get_repo(&id).await.map_err(store_error)?,
        ))
    }

    async fn repos_update(&self, p: RepoUpdate) -> Result<Value, RpcError> {
        let current = self.app.store.get_repo(&p.id).await.map_err(store_error)?;
        let app = match &p.app {
            Some(view) => app_run::app_settings_from(view)?,
            None => mandor_store::app_settings_of(&current.settings),
        };
        app_run::check_app_port(self.app.cfg.vm_agent.as_ref().map(|v| v.ports), &app)?;
        let gate_mode = match p.gate_mode.as_deref() {
            None => current
                .settings
                .get("gate_mode")
                .and_then(Value::as_str)
                .map(str::to_owned),
            Some("inherit") => None,
            Some(m) if mandor_domain::GateMode::parse(m).is_some() => Some(m.to_owned()),
            Some(_) => return Err(rejected("gate mode must be block, warn, or inherit")),
        };
        let settings = json!({
            "companions": p.companions,
            "trusted_reviewers": p.trusted_reviewers,
            "extra_allowed": p.extra_allowed,
            "app": mandor_store::app_settings_json(&app),
            "gate_mode": gate_mode,
        });
        self.app
            .store
            .set_repo_settings(&p.id, &settings)
            .await
            .map_err(store_error)?;
        self.app
            .store
            .set_repo_gh_profile(&p.id, p.gh_profile.as_deref().filter(|g| !g.is_empty()))
            .await
            .map_err(store_error)?;
        ok(&repo_view(
            self.app.store.get_repo(&p.id).await.map_err(store_error)?,
        ))
    }

    async fn notifications(&self) -> Result<Value, RpcError> {
        let pending = self
            .app
            .store
            .take_notifications(100)
            .await
            .map_err(store_error)?;
        let out: Vec<NoticeView> = pending.iter().filter_map(notice_view).collect();
        ok(&out)
    }

    async fn repos_discover(&self) -> Result<Value, RpcError> {
        let added: Vec<String> = self
            .app
            .store
            .list_repos()
            .await
            .map_err(store_error)?
            .into_iter()
            .map(|r| r.path)
            .collect();
        let cfg = &self.app.cfg;
        let found = mandor_git::discover_repos(
            std::slice::from_ref(&cfg.user_home),
            std::slice::from_ref(&cfg.home),
            DISCOVER_DEPTH,
            DISCOVER_MAX,
        )
        .await;
        let out: Vec<RepoCandidateView> = found
            .into_iter()
            .map(|c| {
                let path = c.path.display().to_string();
                RepoCandidateView {
                    added: added.contains(&path),
                    path,
                    name: c.name,
                    origin: c.origin,
                }
            })
            .collect();
        ok(&out)
    }

    async fn list_dirs(&self, q: DirQuery) -> Result<Value, RpcError> {
        let path = q
            .path
            .filter(|p| !p.trim().is_empty())
            .map_or_else(|| self.app.cfg.user_home.clone(), std::path::PathBuf::from);
        let listing = mandor_git::list_dirs(&path).await.map_err(rejected)?;
        ok(&DirListingView {
            path: listing.path.display().to_string(),
            parent: listing.parent.map(|p| p.display().to_string()),
            entries: listing
                .entries
                .into_iter()
                .map(|e| DirEntryView {
                    name: e.name,
                    path: e.path.display().to_string(),
                    is_repo: e.is_repo,
                })
                .collect(),
        })
    }

    async fn notifications_list(&self) -> Result<Value, RpcError> {
        let list = self
            .app
            .store
            .list_notifications(NOTIFICATIONS_SHOWN)
            .await
            .map_err(store_error)?;
        let out: Vec<NoticeView> = list.iter().filter_map(notice_view).collect();
        ok(&out)
    }

    async fn notifications_read(&self, p: NotificationsRead) -> Result<Value, RpcError> {
        if p.ids
            .as_ref()
            .is_some_and(|ids| ids.len() > NOTIFICATIONS_KEPT_IDS)
        {
            return Err(rejected("too many notification ids"));
        }
        self.app
            .store
            .mark_notifications_read(p.ids.as_deref())
            .await
            .map_err(store_error)?;
        ok(&json!({}))
    }

    /// Start dengan event `start.progress` per langkah untuk dialog Start.
    /// Push & PR dengan gerbang AC; keputusan dev dicatat atas nama akun Lark
    /// yang login (KTD-35).
    pub(super) async fn publish(
        &self,
        s: &LarkSession,
        p: PublishParams,
    ) -> Result<Value, RpcError> {
        let decision = mandor_domain::GateDecision {
            confirmed: p.confirmed.unwrap_or_default(),
            push_anyway_reason: p.push_anyway_reason.filter(|r| !r.trim().is_empty()),
            decided_by: self.app.lark_status().await.name,
        };
        let out = s
            .runs
            .publish_gated(&p.run_id, &decision)
            .await
            .map_err(run_error)?;
        let mut view = PublishResult {
            outcome: String::new(),
            pr_url: None,
            failed: Vec::new(),
            unverified: Vec::new(),
            warnings: Vec::new(),
            sync_warning: None,
            review_id: None,
            changed_cards: Vec::new(),
        };
        match out {
            PublishOutcome::Published {
                url,
                warnings,
                sync_warning,
            } => {
                s.poke.notify_one();
                "published".clone_into(&mut view.outcome);
                view.pr_url = Some(url);
                view.warnings = warnings;
                view.sync_warning = sync_warning;
            }
            PublishOutcome::ReviewStarted { review_id } => {
                "review_started".clone_into(&mut view.outcome);
                view.review_id = Some(review_id);
            }
            PublishOutcome::Blocked { failed } => {
                "blocked".clone_into(&mut view.outcome);
                view.failed = failed;
            }
            PublishOutcome::NeedsConfirmation { unverified } => {
                "needs_confirmation".clone_into(&mut view.outcome);
                view.unverified = unverified;
            }
            PublishOutcome::CardsChanged { cards } => {
                "cards_changed".clone_into(&mut view.outcome);
                view.changed_cards = cards;
            }
        }
        ok(&view)
    }

    async fn start(&self, s: &LarkSession, p: StartParams) -> Result<Value, RpcError> {
        let events = self.app.events.clone();
        let record_id = p.record_id.clone();
        let progress = move |step: StartStep| {
            let view = StartProgressView {
                record_id: record_id.clone(),
                step: step.as_str().to_owned(),
            };
            emit(
                &events,
                event::START_PROGRESS,
                serde_json::to_value(view).unwrap_or(Value::Null),
            );
        };
        let target = match p.workspace {
            Some(w) => StartTarget::Workspace {
                id: w.id,
                repo_ids: w.repo_ids,
            },
            None => StartTarget::Repo(p.repo_id.clone()),
        };
        let planning = planning_of(p.planning.as_ref())?;
        let design = self
            .design_roots(p.design_sources.as_deref().unwrap_or_default())
            .await?;
        let run = s
            .runs
            .start_planned(
                &p.record_id,
                &target,
                p.note.as_deref(),
                &planning,
                &design,
                progress,
            )
            .await
            .map_err(run_error)?;
        if let Some(skill) = p.reviewer_skill.as_deref().filter(|k| !k.trim().is_empty()) {
            s.runs
                .set_reviewer_skill(&run.id, Some(skill))
                .await
                .map_err(run_error)?;
        }
        s.poke.notify_one();
        ok(&run_view(&run, None))
    }

    /// Method notifikasi dan pemeriksaan sistem (tanpa sesi Lark).
    async fn system_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        match name {
            method::NOTIFICATIONS_TAKE => self.notifications().await,
            method::NOTIFICATIONS_LIST => self.notifications_list().await,
            method::NOTIFICATIONS_READ => self.notifications_read(params(v)?).await,
            method::SYSTEM_CHECK => ok(&crate::prereq::check_all(&self.app.cfg.claude_bin).await),
            method::REPOS_DISCOVER => self.repos_discover().await,
            method::FS_LIST_DIRS => self.list_dirs(params(v)?).await,
            method::AGENT_SKILLS => {
                let skills =
                    mandor_agent::available_skills(&self.app.cfg.user_home.join(".claude"))
                        .await
                        .into_iter()
                        .map(|s| SkillView {
                            name: s.name,
                            description: s.description,
                        })
                        .collect::<Vec<_>>();
                ok(&skills)
            }
            other if Self::is_workspaces_method(other) => self.workspaces_call(other, v).await,
            other if Self::is_design_method(other) => self.design_call(other, v).await,
            other if Self::is_app_method(other) => self.app_call(other, v).await,
            other if Self::is_review_method(other) => self.review_call(other, v).await,
            other if Self::is_group_method(other) => self.group_call(other, v).await,
            other if Self::is_overnight_method(other) => self.overnight_call(other, v).await,
            other => self.run_action(other, v).await,
        }
    }

    async fn run_action(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        if !RUN_METHODS.contains(&name) {
            return Err(RpcError::new(
                ErrorCode::MethodNotFound,
                format!("unknown method: {name}"),
            ));
        }
        let s = self.session().await?;
        let runs = &s.runs;
        match name {
            method::RUNS_START => self.start(&s, params(v)?).await,
            method::RUNS_SEND | method::RUNS_SEND_NOW => {
                let p: SendParams = params(v)?;
                let out = if name == method::RUNS_SEND {
                    runs.send_message(&p.run_id, &p.text).await
                } else {
                    runs.send_now(&p.run_id, &p.text).await
                }
                .map_err(run_error)?;
                let outcome = match out {
                    SendOutcome::Sent => "sent",
                    SendOutcome::Queued => "queued",
                };
                ok(&SendResult {
                    outcome: outcome.to_owned(),
                })
            }
            method::RUNS_STOP => {
                let p: RunRef = params(v)?;
                runs.stop(&p.run_id).await.map_err(run_error)?;
                ok(&json!({}))
            }
            method::RUNS_PUBLISH => self.publish(&s, params(v)?).await,
            method::RUNS_RESUME => {
                let p: SendParams = params(v)?;
                let text = Some(p.text.as_str()).filter(|t| !t.trim().is_empty());
                runs.resume_failed(&p.run_id, text)
                    .await
                    .map_err(run_error)?;
                ok(&json!({}))
            }
            method::RUNS_START_FIX => {
                let p: StartFixParams = params(v)?;
                runs.start_fix(&p.record_id, &p.instruction, &p.reply_ids)
                    .await
                    .map_err(run_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            method::RUNS_TAKEOVER => {
                let p: TakeoverParams = params(v)?;
                let run = runs
                    .takeover(&p.record_id, &p.repo_id)
                    .await
                    .map_err(run_error)?;
                ok(&run_view(&run, None))
            }
            method::RUNS_DISCARD_FILE => {
                let p: FileDiffParams = params(v)?;
                runs.discard_file(&p.run_id, &p.path)
                    .await
                    .map_err(run_error)?;
                ok(&json!({}))
            }
            method::RUNS_APPROVE_PLAN => {
                let p: ApprovePlanParams = params(v)?;
                approve(runs, &p).await
            }
            method::RUNS_CONTINUE_TO_PLAN => {
                let p: RunRef = params(v)?;
                runs.continue_to_plan(&p.run_id).await.map_err(run_error)?;
                ok(&json!({}))
            }
            method::RUNS_CLOSE => {
                let p: RunRef = params(v)?;
                runs.close_run(&p.run_id).await.map_err(run_error)?;
                ok(&json!({}))
            }
            method::CARD_RETRY => {
                let p: RecordRef = params(v)?;
                s.sync
                    .retry_failed(&p.record_id)
                    .await
                    .map_err(sync_error)?;
                s.poke.notify_one();
                ok(&json!({}))
            }
            other => Err(RpcError::new(
                ErrorCode::MethodNotFound,
                format!("unknown method: {other}"),
            )),
        }
    }
}

/// (pendamping, reviewer, allowlist) dari pengaturan repo.
fn repo_settings(repo: Option<&mandor_store::Repo>) -> (Vec<String>, Vec<String>, Vec<String>) {
    let Some(r) = repo else {
        return Default::default();
    };
    let list = |k: &str| -> Vec<String> {
        r.settings
            .get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    (
        list("companions"),
        list("trusted_reviewers"),
        list("extra_allowed"),
    )
}

fn repo_view(r: mandor_store::Repo) -> RepoView {
    let (companions, trusted_reviewers, extra_allowed) = repo_settings(Some(&r));
    let app = mandor_store::app_settings_of(&r.settings);
    RepoView {
        id: r.id,
        path: r.path,
        default_branch: r.default_branch,
        gh_profile: r.gh_profile,
        companions,
        trusted_reviewers,
        extra_allowed,
        app: app_run::repo_app_view(&app),
        gate_mode: r
            .settings
            .get("gate_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

impl AppDispatch {
    /// Pengaturan Lark dan jeda tenang Waiting Fix (tanpa sesi Lark).
    async fn settings_call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        match name {
            method::LARK_SETTINGS_SET => {
                let input: LarkSettingsInput = params(v)?;
                self.app.set_settings(input).await.map_err(rejected)?;
                ok(&self.app.lark_status().await)
            }
            method::WAITING_FIX_QUIET_SET => {
                let p: QuietPeriodSet = params(v)?;
                ok(&self.app.set_quiet_secs(p.secs).await.map_err(rejected)?)
            }
            _ => ok(&self.app.settings_view().await),
        }
    }
}

async fn approve<D: mandor_usecase::Deps>(
    runs: &Arc<mandor_usecase::RunService<D>>,
    p: &ApprovePlanParams,
) -> Result<Value, RpcError> {
    let follow = p.follow_design.as_deref().unwrap_or_default();
    runs.approve_plan(&p.run_id, follow)
        .await
        .map_err(run_error)?;
    ok(&json!({}))
}

impl Dispatch for AppDispatch {
    async fn verify_capability(&self, token: &str) -> Option<mandor_api::CapabilityScope> {
        self.capability(token).await
    }

    async fn call_scoped(
        &self,
        scope: &mandor_api::CapabilityScope,
        name: &str,
        v: Value,
    ) -> Result<Value, RpcError> {
        self.ledger_tool_call(scope, name, v).await
    }

    async fn call(&self, name: &str, v: Value) -> Result<Value, RpcError> {
        match name {
            method::STATUS => self.status().await,
            method::DAEMON_DRAIN => self.drain().await,
            method::DAEMON_UNDRAIN => {
                self.app.drain.undrain();
                ok(&json!({}))
            }
            method::LARK_SETTINGS_GET
            | method::LARK_SETTINGS_SET
            | method::WAITING_FIX_QUIET_SET => self.settings_call(name, v).await,
            method::LARK_LOGIN_BEGIN => ok(&self.app.login_begin().await.map_err(rejected)?),
            method::LARK_LOGIN_COMPLETE => {
                let p: LoginComplete = params(v)?;
                self.app
                    .login_complete(&p.code, &p.state)
                    .await
                    .map_err(rejected)?;
                ok(&self.app.lark_status().await)
            }
            method::LARK_LOGOUT => {
                self.app.logout().await.map_err(internal)?;
                ok(&self.app.lark_status().await)
            }
            method::BOARD_LIST => self.board().await,
            method::BOARD_REFRESH => {
                self.session().await?.poke.notify_one();
                ok(&json!({}))
            }
            method::CARD_COMMENTS => self.comments(params(v)?).await,
            method::CARD_DETAILS => self.details(params(v)?).await,
            method::PEOPLE_NAMES => self.people(params(v)?).await,
            method::RUNS_LIST => self.runs_list().await,
            method::RUNS_MESSAGES => self.messages(params(v)?).await,
            method::RUNS_CHANGES => self.changes(params(v)?).await,
            method::RUNS_FILE_DIFF => self.file_diff(params(v)?).await,
            method::REPOS_LIST => self.repos_list().await,
            method::REPOS_ADD => self.repos_add(params(v)?).await,
            method::REPOS_UPDATE => self.repos_update(params(v)?).await,
            method::REPOS_REMOVE => {
                let p: RepoRef = params(v)?;
                self.app
                    .store
                    .remove_repo(&p.id)
                    .await
                    .map_err(store_error)?;
                ok(&json!({}))
            }
            other => self.system_call(other, v).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pilihan_planning_dari_client() {
        let p = |kind: &str, brainstorm: Option<&str>, plan: Option<&str>| StartPlanning {
            kind: kind.to_owned(),
            brainstorm_skill: brainstorm.map(str::to_owned),
            plan_skill: plan.map(str::to_owned),
        };
        assert_eq!(planning_of(None).unwrap(), Planning::Skip);
        assert_eq!(
            planning_of(Some(&p("plan", None, Some(" ")))).unwrap(),
            Planning::Plan { skill: None }
        );
        assert_eq!(
            planning_of(Some(&p(
                "brainstorm",
                Some("sp:brainstorming"),
                Some("ce:plan")
            )))
            .unwrap(),
            Planning::Brainstorm {
                brainstorm: Some("sp:brainstorming".to_owned()),
                plan: Some("ce:plan".to_owned()),
            }
        );
        assert!(planning_of(Some(&p("lain", None, None))).is_err());
    }
}
