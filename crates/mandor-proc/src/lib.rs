//! Runner proses eksternal untuk mandor.
//!
//! Semua proses yang dijalankan adapter (git, gh, claude) lewat crate ini agar
//! selalu punya timeout, berjalan di process group sendiri, dan outputnya
//! diredaksi dari token sebelum keluar dari crate (KTD-11, KTD-17).

mod cmd;
mod git_auth;
mod reap;
mod redact;
mod shell_env;
mod supervised;

pub use cmd::{Cmd, EnvPolicy, Output, ProcError, run};
pub use git_auth::git_credential_env;
pub use reap::reap_group;
pub use redact::redact;
pub use shell_env::{ShellEnvError, capture_login_env, parse_env_block};
pub use supervised::{StopOutcome, Supervised};
