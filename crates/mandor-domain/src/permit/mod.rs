//! Izin agent tanpa penolakan diam-diam (plan 2026-11-06-001): pengurai
//! perintah ketat, pencocok allowlist giliran, dan keputusan izin bersama
//! hook dan daemon.

pub mod allowlist;
pub mod shell;

pub use allowlist::BashAllowlist;
pub use shell::{Redirect, SimpleCommand, Unparseable, parse};
