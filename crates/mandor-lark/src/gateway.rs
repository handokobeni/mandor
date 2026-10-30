//! Implementasi port [`LarkTasks`] di atas klien Lark: pemetaan field Sprint
//! Tasks ke [`Task`], manajemen token user, dan validasi opsi `SingleSelect`.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use mandor_domain::{AppField, Claim, Reply, Task, TaskStatus};
use mandor_usecase::{AcItem, EpicItem, LarkTasks, PortError, SprintItem};
use secrecy::SecretString;
use serde_json::{Map, Value, json};
use tokio::sync::Mutex;

use crate::fields::{link_ids, select_text, text_of, url_text, user_ids};
use crate::{
    FileTokenStore, LarkClient, LarkError, RawRecord, RetryPolicy, SearchFilter, TokenSet,
    validate_select, with_retry,
};

/// Nama field di tabel Sprint Tasks.
pub mod field {
    pub const TITLE: &str = "Task name";
    pub const KEY: &str = "ID";
    pub const DESCRIPTION: &str = "Description";
    pub const STATUS: &str = "Task Status";
    pub const READINESS: &str = "Development Readiness";
    pub const ASSIGNEE: &str = "Assignee (PIC)";
    pub const REPORTER: &str = "Reporter/Request";
    pub const AC: &str = "Acceptance Criteria";
    pub const SPRINT: &str = "Sprint";
    pub const EPIC: &str = "Epic";
    pub const KANBAN: &str = "kanban_column";
    pub const BRANCH: &str = "mandor_branch";
    pub const PR: &str = "mandor_pr";
    pub const OWNER: &str = "mandor_owner";
    pub const CURSOR: &str = "mandor_comment_cursor";
    pub const MODIFIED: &str = "mandor_modified";
    /// Selisih desain per AC untuk view tim (KTD-34).
    pub const DESIGN_GAPS: &str = "mandor_design_gaps";
    /// Field tampilan milik app yang sudah ada di Base.
    pub const REPO_ID: &str = "repo_id";
    pub const FIX_NOTES: &str = "Fix Notes";
    /// Field di tabel Acceptance Criteria.
    pub const AC_TITLE: &str = "Acceptance Criteria";
    pub const AC_DESCRIPTION: &str = "Description";
    pub const AC_ID: &str = "AC id";
    pub const AC_TYPE: &str = "AC Type";
    /// Nilai AC Type untuk AC yang dibatalkan (ejaan sesuai Base).
    pub const AC_DROPPED: &str = "Droped";
    /// Catatan QA per AC; sumber Waiting Fix selain komentar (R11f).
    pub const AC_FEEDBACK_QA: &str = "Feedback QA";
    /// Field di tabel sprint tertaut.
    pub const EPIC_NAME: &str = "Epic Name";
    pub const EPIC_KEY: &str = "ID";
    pub const SPRINT_NAME: &str = "Sprint";
    pub const SPRINT_STATUS: &str = "Sprint Status";
    pub const SPRINT_START: &str = "Start Date";
    pub const SPRINT_END: &str = "End Date";
}

/// Access token diperbarui bila habis dalam 5 menit.
const REFRESH_MARGIN_MS: i64 = 5 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Tabel Bitable yang disinkronkan.
#[derive(Debug, Clone)]
pub struct TableRef {
    pub app_token: String,
    pub table_id: String,
    /// Tabel Acceptance Criteria tertaut.
    pub ac_table_id: String,
}

/// Adapter Sprint Tasks untuk satu pengguna.
#[derive(Debug)]
pub struct LarkGateway {
    client: LarkClient,
    table: TableRef,
    tokens: FileTokenStore,
    current: Mutex<Option<TokenSet>>,
    schema: Mutex<Option<Vec<Value>>>,
    retry: RetryPolicy,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl From<LarkError> for PortError {
    fn from(e: LarkError) -> Self {
        match e {
            LarkError::ReauthRequired => Self::Reauth,
            LarkError::RateLimited => Self::Throttled,
            LarkError::WriteConflict => Self::Conflict,
            LarkError::Http(m) => Self::Transient(m),
            LarkError::Io(m) => Self::Storage(m),
            other @ (LarkError::Api { .. }
            | LarkError::Parse(_)
            | LarkError::InvalidOption { .. }) => Self::Rejected(other.to_string()),
        }
    }
}

/// Memetakan record Bitable ke [`Task`].
#[must_use]
pub fn to_task(r: &RawRecord) -> Task {
    let f = |name: &str| r.fields.get(name);
    let mut t = Task::new(
        &r.record_id,
        TaskStatus::parse(&select_text(f(field::STATUS)).unwrap_or_default()),
    );
    t.key = text_of(f(field::KEY));
    t.title = text_of(f(field::TITLE)).unwrap_or_default();
    t.description = text_of(f(field::DESCRIPTION)).unwrap_or_default();
    t.readiness = select_text(f(field::READINESS));
    t.assignees = user_ids(f(field::ASSIGNEE));
    t.reporter = user_ids(f(field::REPORTER)).into_iter().next();
    t.ac_ids = link_ids(f(field::AC));
    t.sprint_ids = link_ids(f(field::SPRINT));
    t.epic_ids = link_ids(f(field::EPIC));
    t.branch = text_of(f(field::BRANCH));
    t.pr_url = url_text(f(field::PR));
    t.claim = text_of(f(field::OWNER)).as_deref().and_then(Claim::parse);
    t.comment_cursor = text_of(f(field::CURSOR));
    t.kanban_mirror = select_text(f(field::KANBAN));
    t.fix_notes = text_of(f(field::FIX_NOTES));
    t.repo_id = text_of(f(field::REPO_ID));
    t.last_modified_ms = r.last_modified_ms;
    t.last_modified_by.clone_from(&r.last_modified_by);
    t
}

/// Memetakan record tabel AC.
#[must_use]
pub fn to_ac(r: &RawRecord) -> AcItem {
    let f = |name: &str| r.fields.get(name);
    let title = text_of(f(field::AC_TITLE)).unwrap_or_default();
    let text = match text_of(f(field::AC_DESCRIPTION)) {
        Some(d) if !d.trim().is_empty() => format!("{title}\n{d}"),
        _ => title,
    };
    AcItem {
        record_id: r.record_id.clone(),
        label: text_of(f(field::AC_ID)),
        text,
        dropped: select_text(f(field::AC_TYPE)).as_deref() == Some(field::AC_DROPPED),
        feedback_qa: text_of(f(field::AC_FEEDBACK_QA)).filter(|t| !t.trim().is_empty()),
        last_modified_ms: r.last_modified_ms,
        last_modified_by: r.last_modified_by.clone(),
    }
}

/// Memetakan record tabel epic ke [`EpicItem`]. Nama kosong memakai ID record.
#[must_use]
pub fn to_epic(r: &RawRecord) -> EpicItem {
    let f = |name: &str| r.fields.get(name);
    EpicItem {
        record_id: r.record_id.clone(),
        key: text_of(f(field::EPIC_KEY)),
        name: text_of(f(field::EPIC_NAME)).unwrap_or_else(|| r.record_id.clone()),
    }
}

/// Memetakan record tabel sprint ke [`SprintItem`]. Nama kosong memakai ID record.
#[must_use]
pub fn to_sprint(r: &RawRecord) -> SprintItem {
    let f = |name: &str| r.fields.get(name);
    SprintItem {
        record_id: r.record_id.clone(),
        name: text_of(f(field::SPRINT_NAME)).unwrap_or_else(|| r.record_id.clone()),
        start_ms: f(field::SPRINT_START).and_then(Value::as_i64),
        end_ms: f(field::SPRINT_END).and_then(Value::as_i64),
        status: select_text(f(field::SPRINT_STATUS)),
    }
}

/// `table_id` tujuan field Link bernama `name` dari skema tabel.
fn linked_table(schema: &[Value], name: &str) -> Option<String> {
    schema
        .iter()
        .find(|f| f.get("field_name").and_then(Value::as_str) == Some(name))
        .and_then(|f| f.get("property")?.get("table_id")?.as_str())
        .map(str::to_owned)
}

/// Body `fields` untuk PUT. Hanya field milik app dan Task Status.
#[must_use]
pub fn write_body(status: Option<&TaskStatus>, fields: &[AppField]) -> Value {
    let mut out = Map::new();
    if let Some(s) = status {
        out.insert(field::STATUS.to_owned(), json!(s.as_str()));
    }
    for f in fields {
        let (name, value) = match f {
            AppField::Branch(b) => (field::BRANCH, json!(b)),
            // Run workspace: satu PR per baris; link field URL hanya bisa satu.
            AppField::PrUrl(u) => {
                let first = u.lines().next().unwrap_or_default();
                (field::PR, json!({"text": u, "link": first}))
            }
            AppField::Claim(Some(c)) => (field::OWNER, json!(c.encode())),
            AppField::Claim(None) => (field::OWNER, Value::Null),
            AppField::CommentCursor(c) => (field::CURSOR, json!(c)),
            AppField::KanbanMirror(m) => (field::KANBAN, json!(m)),
            AppField::RepoId(r) => (field::REPO_ID, json!(r)),
            // Kosong mengosongkan field, sama seperti klaim yang dilepas.
            AppField::DesignGaps(g) if g.is_empty() => (field::DESIGN_GAPS, Value::Null),
            AppField::DesignGaps(g) => (field::DESIGN_GAPS, json!(g)),
        };
        out.insert(name.to_owned(), value);
    }
    Value::Object(out)
}

impl LarkGateway {
    #[must_use]
    pub fn new(client: LarkClient, table: TableRef, tokens: FileTokenStore) -> Self {
        Self {
            client,
            table,
            tokens,
            current: Mutex::new(None),
            schema: Mutex::new(None),
            retry: RetryPolicy::default(),
        }
    }

    /// Mengganti kebijakan retry (test memakai jeda pendek).
    #[must_use]
    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Access token yang masih berlaku; diperbarui dan disimpan atomik bila perlu.
    async fn access(&self) -> Result<SecretString, LarkError> {
        let mut guard = self.current.lock().await;
        if guard.is_none() {
            *guard = self.tokens.load().await?;
        }
        let Some(tokens) = guard.as_ref() else {
            return Err(LarkError::ReauthRequired);
        };
        let now = now_ms();
        if !tokens.access_expiring(now, REFRESH_MARGIN_MS) {
            return Ok(tokens.access_token.clone());
        }
        let fresh = self.client.refresh(tokens, now).await?;
        // Refresh token lama tidak berlaku lagi: simpan pasangan baru dulu.
        self.tokens.save(&fresh).await?;
        let access = fresh.access_token.clone();
        *guard = Some(fresh);
        Ok(access)
    }

    /// Bootstrap field milik app (idempoten).
    ///
    /// # Errors
    /// [`LarkError`] bila Lark menolak.
    pub async fn bootstrap(&self) -> Result<Vec<&'static str>, LarkError> {
        let access = self.access().await?;
        let created = self
            .client
            .ensure_app_fields(&access, &self.table.app_token, &self.table.table_id)
            .await?;
        // Skema lama tidak memuat field yang baru dibuat.
        if !created.is_empty() {
            *self.schema.lock().await = None;
        }
        Ok(created)
    }

    /// Skema field Sprint Tasks, dibaca sekali per sesi.
    async fn schema(&self, access: &SecretString) -> Result<Vec<Value>, LarkError> {
        let mut schema = self.schema.lock().await;
        if let Some(s) = schema.as_ref() {
            return Ok(s.clone());
        }
        let fresh = self
            .client
            .list_fields(access, &self.table.app_token, &self.table.table_id)
            .await?;
        *schema = Some(fresh.clone());
        Ok(fresh)
    }

    async fn check_options(&self, access: &SecretString, body: &Value) -> Result<(), LarkError> {
        let selects = [field::STATUS, field::KANBAN];
        if !selects
            .iter()
            .any(|n| body.get(n).is_some_and(Value::is_string))
        {
            return Ok(());
        }
        let fields = self.schema(access).await?;
        for name in selects {
            if let Some(option) = body.get(name).and_then(Value::as_str) {
                validate_select(&fields, name, option)?;
            }
        }
        Ok(())
    }
}

impl LarkGateway {
    /// Pencarian record tabel task dengan filter, semua halaman, dengan retry.
    async fn search_with(&self, filter: &SearchFilter) -> Result<Vec<RawRecord>, PortError> {
        with_retry(self.retry, || async {
            let access = self.access().await?;
            self.client
                .search_records(
                    &access,
                    &self.table.app_token,
                    &self.table.table_id,
                    filter,
                    &[],
                )
                .await
        })
        .await
        .map_err(Into::into)
    }

    async fn has_modified_field(&self) -> Result<bool, PortError> {
        with_retry(self.retry, || async {
            let access = self.access().await?;
            Ok(self
                .schema(&access)
                .await?
                .iter()
                .any(|f| f.get("field_name").and_then(Value::as_str) == Some(field::MODIFIED)))
        })
        .await
        .map_err(Into::into)
    }
}

impl LarkTasks for LarkGateway {
    async fn search_assigned(
        &self,
        me: &str,
        modified_since_ms: Option<i64>,
    ) -> Result<Vec<Task>, PortError> {
        let records = with_retry(self.retry, || async {
            let access = self.access().await?;
            let mut filter =
                SearchFilter::default().and(field::ASSIGNEE, "contains", vec![me.to_owned()]);
            // Tanpa field `mandor_modified` (bootstrap gagal atau belum jalan), Lark
            // menolak filternya; pencarian penuh tetap benar, hanya lebih berat.
            let has_modified = self
                .schema(&access)
                .await?
                .iter()
                .any(|f| f.get("field_name").and_then(Value::as_str) == Some(field::MODIFIED));
            if let Some(since) = modified_since_ms.filter(|_| has_modified) {
                // Filter tanggal Lark berbutir hari: "lebih besar dari sehari sebelumnya"
                // berarti "sejak hari itu".
                filter = filter.and(
                    field::MODIFIED,
                    "isGreater",
                    vec!["ExactDate".to_owned(), (since - DAY_MS).to_string()],
                );
            }
            self.client
                .search_records(
                    &access,
                    &self.table.app_token,
                    &self.table.table_id,
                    &filter,
                    &[],
                )
                .await
        })
        .await?;
        Ok(records.iter().map(to_task).collect())
    }

    async fn search_team(&self, done_since_ms: i64) -> Result<Vec<Task>, PortError> {
        let done = [TaskStatus::WaitingDeploy, TaskStatus::Delivered];
        let active = done.iter().fold(SearchFilter::default(), |f, s| {
            f.and(field::STATUS, "isNot", vec![s.as_str().to_owned()])
        });
        let mut records = self.search_with(&active).await?;
        // Kartu Done terbaru lewat waktu ubah; tanpa field itu hanya kartu aktif.
        if self.has_modified_field().await? {
            let recent = SearchFilter::default().and(
                field::MODIFIED,
                "isGreater",
                vec!["ExactDate".to_owned(), (done_since_ms - DAY_MS).to_string()],
            );
            for r in self.search_with(&recent).await? {
                if !records.iter().any(|x| x.record_id == r.record_id) {
                    records.push(r);
                }
            }
        }
        Ok(records.iter().map(to_task).collect())
    }

    async fn fetch(&self, ids: &[String]) -> Result<Vec<Task>, PortError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let records = with_retry(self.retry, || async {
            let access = self.access().await?;
            self.client
                .batch_get(&access, &self.table.app_token, &self.table.table_id, ids)
                .await
        })
        .await?;
        Ok(records.iter().map(to_task).collect())
    }

    async fn record_replies(&self, stop_before: Option<i64>) -> Result<Vec<Reply>, PortError> {
        let replies = with_retry(self.retry, || async {
            let access = self.access().await?;
            self.client
                .record_replies(&access, &self.table.app_token, stop_before)
                .await
        })
        .await?;
        Ok(replies
            .into_iter()
            .map(|r| Reply {
                record_id: r.record_id,
                comment_id: r.comment_id,
                reply_id: r.reply_id,
                author: r.author_open_id,
                create_time: r.create_time,
                text: r.text,
            })
            .collect())
    }

    async fn message_me(&self, text: &str) -> Result<(), PortError> {
        let me = {
            let guard = self.current.lock().await;
            guard.as_ref().and_then(|t| t.open_id.clone())
        };
        let me = match me {
            Some(id) => id,
            None => self
                .tokens
                .load()
                .await
                .map_err(PortError::from)?
                .and_then(|t| t.open_id)
                .ok_or(PortError::Reauth)?,
        };
        Ok(with_retry(self.retry, || async {
            let tenant = self.client.tenant_token().await?;
            self.client.send_text(&tenant, &me, text).await
        })
        .await?)
    }

    async fn user_names(&self, open_ids: &[String]) -> Result<HashMap<String, String>, PortError> {
        if open_ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(with_retry(self.retry, || async {
            let access = self.access().await?;
            self.client.user_names(&access, open_ids).await
        })
        .await?)
    }

    async fn sprints(&self, ids: &[String]) -> Result<Vec<SprintItem>, PortError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let records = with_retry(self.retry, || async {
            let access = self.access().await?;
            let schema = self.schema(&access).await?;
            let Some(table_id) = linked_table(&schema, field::SPRINT) else {
                return Ok(Vec::new());
            };
            self.client
                .batch_get(&access, &self.table.app_token, &table_id, ids)
                .await
        })
        .await?;
        Ok(ids
            .iter()
            .filter_map(|id| records.iter().find(|r| &r.record_id == id))
            .map(to_sprint)
            .collect())
    }

    async fn epics(&self, ids: &[String]) -> Result<Vec<EpicItem>, PortError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let records = with_retry(self.retry, || async {
            let access = self.access().await?;
            let schema = self.schema(&access).await?;
            let Some(table_id) = linked_table(&schema, field::EPIC) else {
                return Ok(Vec::new());
            };
            self.client
                .batch_get(&access, &self.table.app_token, &table_id, ids)
                .await
        })
        .await?;
        Ok(ids
            .iter()
            .filter_map(|id| records.iter().find(|r| &r.record_id == id))
            .map(to_epic)
            .collect())
    }

    async fn acceptance_criteria(&self, ids: &[String]) -> Result<Vec<AcItem>, PortError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let records = with_retry(self.retry, || async {
            let access = self.access().await?;
            self.client
                .batch_get(&access, &self.table.app_token, &self.table.ac_table_id, ids)
                .await
        })
        .await?;
        // Urut sesuai tautan di task.
        Ok(ids
            .iter()
            .filter_map(|id| records.iter().find(|r| &r.record_id == id))
            .map(to_ac)
            .collect())
    }

    async fn write(
        &self,
        record_id: &str,
        status: Option<&TaskStatus>,
        fields: &[AppField],
    ) -> Result<Task, PortError> {
        let body = write_body(status, fields);
        let record = with_retry(self.retry, || async {
            let access = self.access().await?;
            self.check_options(&access, &body).await?;
            self.client
                .update_record(
                    &access,
                    &self.table.app_token,
                    &self.table.table_id,
                    record_id,
                    &body,
                )
                .await
        })
        .await?;
        Ok(to_task(&record))
    }
}
