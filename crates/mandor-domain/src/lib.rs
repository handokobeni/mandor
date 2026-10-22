//! Aturan bisnis murni mandor.
//!
//! Crate ini tidak boleh bergantung pada crate I/O (tokio, sqlx, axum, reqwest)
//! maupun crate workspace lain. Lihat `CLAUDE.md`.

pub mod ac_ledger;
pub mod agent;
pub mod app_run;
pub mod claim;
pub mod cursor;
pub mod gate;
pub mod hook;
pub mod hook_review;
pub mod kanban;
pub mod push_gate;
pub mod run;
pub mod task;
pub mod trust;

pub use ac_ledger::{AcRow, Design, FetchedAc, RowStatus, Verdict};
pub use agent::{AgentEvent, Usage};
pub use claim::{Claim, ClaimRefused};
pub use cursor::{Cursor, Reply};
pub use gate::GateRejection;
pub use kanban::{Column, TaskStatus, column_of};
pub use push_gate::{GateDecision, GateMode, GateOutcome};
pub use run::{ClaimLoss, RunState};
pub use task::{AppField, CardFlag, FlagKind, Task, WriteOp, WriteRejected};
