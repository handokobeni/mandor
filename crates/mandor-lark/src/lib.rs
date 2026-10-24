//! Adapter Lark untuk mandord (U7, KTD-5, KTD-6).
//!
//! - OAuth per user dengan PKCE; app bertindak atas nama dev.
//! - Bitable: cari, ambil, ubah record, dan bootstrap field milik app.
//! - Komentar record lewat API komentar drive (`file_type=bitable`, belum
//!   terdokumentasi resmi), dipetakan ke record lewat metadata balasan.
//! - Error API diterjemahkan ke [`LarkError`]; retry dengan backoff + jitter.

mod bitable;
mod client;
mod comments;
mod contact;
mod error;
mod fields;
mod fields_bootstrap;
mod gateway;
mod messages;
mod oauth;
mod rate_limit;
mod token_store;

pub use bitable::{RawRecord, SearchFilter};
pub use client::{LarkClient, LarkConfig};
pub use comments::{RecordReply, map_record_replies};
pub use error::LarkError;
pub use fields::{link_ids, select_text, text_of, user_ids};
pub use fields_bootstrap::{APP_FIELDS, FieldDef, validate_select};
pub use gateway::{LarkGateway, TableRef, field, to_ac, to_epic, to_sprint, to_task, write_body};
pub use oauth::{Pkce, SCOPES, TokenSet, UserIdentity, authorize_url, new_state};
pub use rate_limit::{RetryPolicy, with_retry};
pub use token_store::FileTokenStore;
