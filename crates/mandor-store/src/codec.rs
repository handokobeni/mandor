//! Pemetaan tipe domain ke JSON yang disimpan. Bentuk JSON ini milik adapter;
//! domain dan usecase tidak bergantung pada serde.

use mandor_domain::{AppField, Claim, Task, TaskStatus, WriteOp};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct TaskJson {
    record_id: String,
    key: Option<String>,
    title: String,
    description: String,
    status: String,
    readiness: Option<String>,
    assignees: Vec<String>,
    reporter: Option<String>,
    ac_ids: Vec<String>,
    sprint_ids: Vec<String>,
    branch: Option<String>,
    pr_url: Option<String>,
    claim: Option<String>,
    comment_cursor: Option<String>,
    kanban_mirror: Option<String>,
    fix_notes: Option<String>,
    #[serde(default)]
    repo_id: Option<String>,
    last_modified_ms: i64,
    last_modified_by: Option<String>,
}

impl From<&Task> for TaskJson {
    fn from(t: &Task) -> Self {
        Self {
            record_id: t.record_id.clone(),
            key: t.key.clone(),
            title: t.title.clone(),
            description: t.description.clone(),
            status: t.status.as_str().to_owned(),
            readiness: t.readiness.clone(),
            assignees: t.assignees.clone(),
            reporter: t.reporter.clone(),
            ac_ids: t.ac_ids.clone(),
            sprint_ids: t.sprint_ids.clone(),
            branch: t.branch.clone(),
            pr_url: t.pr_url.clone(),
            claim: t.claim.as_ref().map(Claim::encode),
            comment_cursor: t.comment_cursor.clone(),
            kanban_mirror: t.kanban_mirror.clone(),
            fix_notes: t.fix_notes.clone(),
            repo_id: t.repo_id.clone(),
            last_modified_ms: t.last_modified_ms,
            last_modified_by: t.last_modified_by.clone(),
        }
    }
}

impl From<TaskJson> for Task {
    fn from(j: TaskJson) -> Self {
        Self {
            record_id: j.record_id,
            key: j.key,
            title: j.title,
            description: j.description,
            status: TaskStatus::parse(&j.status),
            readiness: j.readiness,
            assignees: j.assignees,
            reporter: j.reporter,
            ac_ids: j.ac_ids,
            sprint_ids: j.sprint_ids,
            branch: j.branch,
            pr_url: j.pr_url,
            claim: j.claim.as_deref().and_then(Claim::parse),
            comment_cursor: j.comment_cursor,
            kanban_mirror: j.kanban_mirror,
            fix_notes: j.fix_notes,
            repo_id: j.repo_id,
            last_modified_ms: j.last_modified_ms,
            last_modified_by: j.last_modified_by,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "snake_case")]
pub(crate) enum FieldJson {
    Branch(String),
    PrUrl(String),
    Claim(Option<String>),
    CommentCursor(String),
    KanbanMirror(String),
    RepoId(String),
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum WriteOpJson {
    Transition {
        from: String,
        to: String,
        fields: Vec<FieldJson>,
    },
    Fields {
        fields: Vec<FieldJson>,
    },
}

fn field_to(f: &AppField) -> FieldJson {
    match f {
        AppField::Branch(v) => FieldJson::Branch(v.clone()),
        AppField::PrUrl(v) => FieldJson::PrUrl(v.clone()),
        AppField::Claim(c) => FieldJson::Claim(c.as_ref().map(Claim::encode)),
        AppField::CommentCursor(v) => FieldJson::CommentCursor(v.clone()),
        AppField::KanbanMirror(v) => FieldJson::KanbanMirror(v.clone()),
        AppField::RepoId(v) => FieldJson::RepoId(v.clone()),
    }
}

fn field_from(f: FieldJson) -> AppField {
    match f {
        FieldJson::Branch(v) => AppField::Branch(v),
        FieldJson::PrUrl(v) => AppField::PrUrl(v),
        FieldJson::Claim(c) => AppField::Claim(c.as_deref().and_then(Claim::parse)),
        FieldJson::CommentCursor(v) => AppField::CommentCursor(v),
        FieldJson::KanbanMirror(v) => AppField::KanbanMirror(v),
        FieldJson::RepoId(v) => AppField::RepoId(v),
    }
}

impl From<&WriteOp> for WriteOpJson {
    fn from(op: &WriteOp) -> Self {
        match op {
            WriteOp::Transition { from, to, fields } => Self::Transition {
                from: from.as_str().to_owned(),
                to: to.as_str().to_owned(),
                fields: fields.iter().map(field_to).collect(),
            },
            WriteOp::Fields(fields) => Self::Fields {
                fields: fields.iter().map(field_to).collect(),
            },
        }
    }
}

impl From<WriteOpJson> for WriteOp {
    fn from(j: WriteOpJson) -> Self {
        match j {
            WriteOpJson::Transition { from, to, fields } => Self::Transition {
                from: TaskStatus::parse(&from),
                to: TaskStatus::parse(&to),
                fields: fields.into_iter().map(field_from).collect(),
            },
            WriteOpJson::Fields { fields } => {
                Self::Fields(fields.into_iter().map(field_from).collect())
            }
        }
    }
}

/// Peran dan isi JSON pesan chat run. Bentuk `body` stabil karena dibaca UI.
pub(crate) fn chat_to_json(
    item: &mandor_usecase::run_ports::ChatItem,
) -> (&'static str, serde_json::Value) {
    use mandor_domain::AgentEvent as E;
    use mandor_usecase::run_ports::ChatItem;
    use serde_json::json;
    match item {
        ChatItem::Dev(t) => ("dev", json!({"type": "text", "text": t})),
        ChatItem::System(t) => ("system", json!({"type": "text", "text": t})),
        ChatItem::Agent(e) => (
            "agent",
            match e {
                E::SessionStarted { session_id, model } => {
                    json!({"type": "session_started", "session_id": session_id, "model": model})
                }
                E::AssistantMessage { text } => json!({"type": "assistant_message", "text": text}),
                E::ToolUse { id, name, preview } => {
                    json!({"type": "tool_use", "id": id, "name": name, "preview": preview})
                }
                E::ToolResult {
                    tool_use_id,
                    is_error,
                    preview,
                } => {
                    json!({"type": "tool_result", "tool_use_id": tool_use_id, "is_error": is_error, "preview": preview})
                }
                E::PermissionDenied {
                    tool_name,
                    tool_use_id,
                    reason,
                } => {
                    json!({"type": "permission_denied", "tool_name": tool_name, "tool_use_id": tool_use_id, "reason": reason})
                }
                E::RateLimit {
                    status,
                    utilization,
                } => {
                    json!({"type": "rate_limit", "status": status, "utilization": utilization})
                }
                E::Usage(u) => json!({
                    "type": "usage",
                    "input_tokens": u.input_tokens,
                    "output_tokens": u.output_tokens,
                    "cache_creation_input_tokens": u.cache_creation_input_tokens,
                    "cache_read_input_tokens": u.cache_read_input_tokens
                }),
                E::Done {
                    is_error,
                    result,
                    num_turns,
                    denials,
                } => {
                    json!({"type": "done", "is_error": is_error, "result": result, "num_turns": num_turns, "denials": denials})
                }
                E::Failed { message } => json!({"type": "failed", "message": message}),
            },
        ),
    }
}
