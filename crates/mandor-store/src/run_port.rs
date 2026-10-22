//! Port `RunStore` dan `Repos` usecase di atas tabel `runs`, `messages`, `repos`.

use std::path::PathBuf;

use mandor_domain::RunState;
use mandor_usecase::PortError;
use mandor_usecase::review::DesignRoot;
use mandor_usecase::run_ports::{
    ChatItem, Journal, RepoInfo, Repos, RunMember, RunPhase, RunRecord, RunStore, RunWorkspace,
    WorkspaceInfo,
};
use serde::{Deserialize, Serialize};

use crate::codec::chat_to_json;
use crate::{Store, StoreError, from_json};

/// Bagian run yang disimpan di `state_json`.
#[derive(Serialize, Deserialize, Default)]
struct RunExtra {
    #[serde(default)]
    session_started: bool,
    #[serde(default)]
    queue: Vec<String>,
    #[serde(default)]
    last_error: Option<String>,
    #[serde(default)]
    automatic: bool,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    quiet_until_ms: Option<i64>,
    #[serde(default)]
    fix_requested_by: Option<String>,
    #[serde(default)]
    pr_body_hashes: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    review_snapshot: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    design: Vec<StoredDesign>,
    #[serde(default)]
    workspace: Option<StoredWorkspace>,
    #[serde(default)]
    extra: Vec<StoredMember>,
    /// `plan` | `brainstorm`; tidak ada = implementasi.
    #[serde(default)]
    phase: Option<String>,
    /// Bentuk lama sebelum `phase`: `true` = fase plan.
    #[serde(default, skip_serializing)]
    planning: bool,
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    plan_skill: Option<String>,
}

/// Bentuk simpan [`RunWorkspace`].
#[derive(Serialize, Deserialize)]
struct StoredDesign {
    id: String,
    name: String,
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct StoredWorkspace {
    id: String,
    root: PathBuf,
}

/// Bentuk simpan [`RunMember`].
#[derive(Serialize, Deserialize)]
struct StoredMember {
    repo_id: String,
    worktree: PathBuf,
    pr_url: Option<String>,
}

/// Pengaturan repo yang dipakai orkestrasi (di `settings_json`).
#[derive(Serialize, Deserialize, Default)]
struct RepoSettings {
    #[serde(default)]
    companions: Vec<PathBuf>,
    #[serde(default)]
    trusted_reviewers: Vec<String>,
    #[serde(default)]
    extra_allowed: Vec<String>,
}

fn extra_of(r: &RunRecord) -> Result<String, StoreError> {
    serde_json::to_string(&RunExtra {
        session_started: r.session_started,
        queue: r.queue.clone(),
        last_error: r.last_error.clone(),
        automatic: r.automatic,
        note: r.note.clone(),
        quiet_until_ms: r.quiet_until_ms,
        fix_requested_by: r.fix_requested_by.clone(),
        pr_body_hashes: r.pr_body_hashes.clone(),
        review_snapshot: r.review_snapshot.clone(),
        design: r
            .design
            .iter()
            .map(|d| StoredDesign {
                id: d.id.clone(),
                name: d.name.clone(),
                path: d.path.clone(),
            })
            .collect(),
        workspace: r.workspace.as_ref().map(|w| StoredWorkspace {
            id: w.id.clone(),
            root: w.root.clone(),
        }),
        extra: r
            .extra
            .iter()
            .map(|m| StoredMember {
                repo_id: m.repo_id.clone(),
                worktree: m.worktree.clone(),
                pr_url: m.pr_url.clone(),
            })
            .collect(),
        phase: (r.phase != RunPhase::Implement).then(|| r.phase.as_str().to_owned()),
        planning: false,
        plan: r.plan.clone(),
        plan_skill: r.plan_skill.clone(),
    })
    .map_err(StoreError::internal)
}

/// Adapter run untuk usecase.
#[derive(Debug, Clone)]
pub struct RunsStore {
    store: Store,
}

impl RunsStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

struct Row {
    id: String,
    repo_id: String,
    record_id: String,
    state: String,
    state_json: String,
    generation: i64,
    branch: Option<String>,
    worktree_path: Option<String>,
    session_id: Option<String>,
    pid: Option<i64>,
    pr_url: Option<String>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

fn to_record(row: Row) -> Result<RunRecord, StoreError> {
    let extra: RunExtra = serde_json::from_value(from_json(&row.state_json)?).unwrap_or_default();
    Ok(RunRecord {
        id: row.id,
        repo_id: row.repo_id,
        record_id: row.record_id,
        // State tak dikenal (mis. dari versi lebih baru) diperlakukan gagal.
        state: RunState::parse(&row.state).unwrap_or(RunState::Failed),
        generation: row.generation,
        branch: row.branch,
        worktree: row.worktree_path.map(PathBuf::from),
        session_id: row.session_id,
        session_started: extra.session_started,
        pid: row.pid,
        pr_url: row.pr_url,
        queue: extra.queue,
        last_error: extra.last_error,
        automatic: extra.automatic,
        note: extra.note,
        quiet_until_ms: extra.quiet_until_ms,
        fix_requested_by: extra.fix_requested_by,
        pr_body_hashes: extra.pr_body_hashes,
        review_snapshot: extra.review_snapshot,
        design: extra
            .design
            .into_iter()
            .map(|d| DesignRoot {
                id: d.id,
                name: d.name,
                path: d.path,
            })
            .collect(),
        workspace: extra.workspace.map(|w| RunWorkspace {
            id: w.id,
            root: w.root,
        }),
        extra: extra
            .extra
            .into_iter()
            .map(|m| RunMember {
                repo_id: m.repo_id,
                worktree: m.worktree,
                pr_url: m.pr_url,
            })
            .collect(),
        phase: match extra.phase.as_deref() {
            Some("brainstorm") => RunPhase::Brainstorm,
            Some("plan") => RunPhase::Plan,
            _ if extra.planning => RunPhase::Plan,
            _ => RunPhase::Implement,
        },
        plan: extra.plan,
        plan_skill: extra.plan_skill,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    })
}

impl RunsStore {
    async fn fetch(&self, id: &str) -> Result<Option<RunRecord>, StoreError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, repo_id, record_id, state, state_json, generation, branch,
                    worktree_path, session_id, pid, pr_url, created_at_ms, updated_at_ms
             FROM runs WHERE id = ?",
            id
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        row.map(to_record).transpose()
    }
}

impl RunStore for RunsStore {
    async fn create(&self, r: &RunRecord, now_ms: i64) -> Result<(), PortError> {
        let state = r.state.as_str();
        let extra = extra_of(r)?;
        let worktree = r.worktree.as_ref().map(|p| p.display().to_string());
        sqlx::query!(
            "INSERT INTO runs (id, repo_id, record_id, state, state_json, generation, branch,
                               worktree_path, session_id, pid, pr_url, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            r.id,
            r.repo_id,
            r.record_id,
            state,
            extra,
            r.generation,
            r.branch,
            worktree,
            r.session_id,
            r.pid,
            r.pr_url,
            now_ms,
            now_ms
        )
        .execute(&self.store.pool)
        .await
        .map_err(|e| StoreError::from_sqlx(e, "record already has an active run"))?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<RunRecord>, PortError> {
        Ok(self.fetch(id).await?)
    }

    async fn active_for(&self, record_id: &str) -> Result<Option<RunRecord>, PortError> {
        let id = sqlx::query_scalar!(
            "SELECT id FROM runs WHERE record_id = ? AND closed = 0 ORDER BY created_at_ms DESC LIMIT 1",
            record_id
        )
        .fetch_optional(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        match id {
            Some(id) => Ok(self.fetch(&id).await?),
            None => Ok(None),
        }
    }

    async fn list_active(&self) -> Result<Vec<RunRecord>, PortError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repo_id, record_id, state, state_json, generation, branch,
                    worktree_path, session_id, pid, pr_url, created_at_ms, updated_at_ms
             FROM runs WHERE closed = 0 ORDER BY created_at_ms"
        )
        .fetch_all(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        Ok(rows.into_iter().map(to_record).collect::<Result<_, _>>()?)
    }

    async fn save(&self, r: &RunRecord, expected: i64, now_ms: i64) -> Result<(), PortError> {
        let state = r.state.as_str();
        let extra = extra_of(r)?;
        let worktree = r.worktree.as_ref().map(|p| p.display().to_string());
        let res = sqlx::query!(
            "UPDATE runs SET state = ?, state_json = ?, generation = ?, branch = ?,
                    worktree_path = ?, session_id = ?, pid = ?, pr_url = ?, updated_at_ms = ?
             WHERE id = ? AND generation = ? AND closed = 0",
            state,
            extra,
            r.generation,
            r.branch,
            worktree,
            r.session_id,
            r.pid,
            r.pr_url,
            now_ms,
            r.id,
            expected
        )
        .execute(&self.store.pool)
        .await
        .map_err(StoreError::internal)?;
        if res.rows_affected() == 0 {
            return Err(PortError::Conflict);
        }
        Ok(())
    }

    async fn close(&self, id: &str, now_ms: i64) -> Result<(), PortError> {
        Ok(self.store.close_run(id, now_ms).await?)
    }

    async fn append(&self, run_id: &str, item: &ChatItem, now_ms: i64) -> Result<(), PortError> {
        let (role, body) = chat_to_json(item);
        self.store
            .append_message(run_id, role, &body, now_ms)
            .await?;
        Ok(())
    }
}

impl Journal for RunsStore {
    async fn journal_pending(
        &self,
        record_id: &str,
        reply_id: &str,
        run_id: &str,
        now_ms: i64,
    ) -> Result<(), PortError> {
        Ok(self
            .store
            .journal_pending(record_id, reply_id, run_id, now_ms)
            .await?)
    }

    async fn journal_delivered(&self, record_id: &str, reply_id: &str) -> Result<(), PortError> {
        Ok(self.store.journal_delivered(record_id, reply_id).await?)
    }

    async fn journal_pending_list(&self, record_id: &str) -> Result<Vec<String>, PortError> {
        Ok(self.store.journal_pending_list(record_id).await?)
    }

    async fn journal_delivered_list(&self, record_id: &str) -> Result<Vec<String>, PortError> {
        Ok(self.store.journal_delivered_list(record_id).await?)
    }
}

/// Adapter daftar repo.
#[derive(Debug, Clone)]
pub struct ReposStore {
    store: Store,
}

impl ReposStore {
    #[must_use]
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl Repos for ReposStore {
    async fn workspace(&self, id: &str) -> Result<Option<WorkspaceInfo>, PortError> {
        match self.store.get_workspace(id).await {
            Ok(w) => Ok(Some(WorkspaceInfo {
                id: w.id,
                name: w.name,
                root: PathBuf::from(w.root_path),
                repo_ids: w.repo_ids,
                mcp_servers: w.mcp_servers,
            })),
            Err(StoreError::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn repo(&self, id: &str) -> Result<Option<RepoInfo>, PortError> {
        let repo = match self.store.get_repo(id).await {
            Ok(r) => r,
            Err(StoreError::NotFound) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let settings: RepoSettings = serde_json::from_value(repo.settings).unwrap_or_default();
        Ok(Some(RepoInfo {
            id: repo.id,
            path: PathBuf::from(repo.path),
            default_branch: repo.default_branch.unwrap_or_else(|| "main".to_owned()),
            gh_profile: repo.gh_profile,
            companions: settings.companions,
            trusted_reviewers: settings.trusted_reviewers,
            extra_allowed: settings.extra_allowed,
        }))
    }
}
