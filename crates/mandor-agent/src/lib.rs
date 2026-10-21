//! Adapter agent mandor: menjalankan `claude -p` tanpa modifikasi dengan pagar
//! izin di luar prompt (KTD-10, KTD-11, KTD-15, KTD-17).

pub mod backend;
pub mod claude;
pub mod port;
pub mod prompt;
pub mod skills;

pub use backend::{AgentBackend, AgentRun, ClaudeBackend, RunEvent, RunStopper};
pub use claude::command::{Session, SessionSpec};
pub use port::{ClaudeAgent, ClaudeConfig, ClaudeRun, mcp_server_names};
pub use prompt::ClaudePrompts;
pub use skills::{SkillInfo, available_skills};

/// Kegagalan adapter agent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AgentError {
    #[error("cannot start the agent: {0}")]
    Spawn(String),
    #[error("agent file: {0}")]
    Io(String),
}

/// ID sesi baru (UUID v4) untuk `--session-id`.
#[must_use]
pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
