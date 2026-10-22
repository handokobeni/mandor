//! Orkestrasi mandor. Mendefinisikan trait port yang dibutuhkan use case;
//! adapter mengimplementasikannya. Lihat `CLAUDE.md`.

pub mod app_run;
mod hash;
mod notify;
pub mod ports;
pub mod review;
pub mod run;
pub mod run_ports;
pub mod sync;
pub mod waiting_fix;

pub use ports::{
    AcItem, CachedTask, Clock, Deps, Ids, LarkTasks, Outbox, PortError, QueuedWrite, SprintItem,
    TaskCache,
};
pub use run::{Planning, RunError, RunPorts, RunService, SendOutcome, StartStep, StartTarget};
pub use sync::{DrainReport, Heartbeat, PollReport, SyncError, SyncService, TaskDetails};
