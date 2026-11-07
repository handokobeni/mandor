//! Kartu persetujuan izin agent (plan 2026-11-06-001): permintaan izin dari
//! server MCP `mandor_permit`, keputusan run, mode tolak langsung, dan
//! penantian jawaban dev.

mod service;

pub use service::{
    APPROVAL_TIMEOUT, ApprovalError, MAX_INPUT_BYTES, MAX_PENDING, PermitAnswer, PermitRequest,
};
