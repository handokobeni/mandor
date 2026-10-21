/// Kegagalan akses Lark.
#[derive(Debug, thiserror::Error)]
pub enum LarkError {
    #[error("HTTP request to Lark failed: {0}")]
    Http(String),
    #[error("Lark rejected the request (code {code}): {msg}")]
    Api { code: i64, msg: String },
    #[error("Lark session expired; please log in again")]
    ReauthRequired,
    #[error("Lark API quota exceeded; try again later")]
    RateLimited,
    #[error("write conflict in the Lark table; try again")]
    WriteConflict,
    #[error("cannot parse Lark response: {0}")]
    Parse(String),
    #[error("option \"{option}\" does not exist in field {field}")]
    InvalidOption { field: String, option: String },
    #[error("token file: {0}")]
    Io(String),
}

impl LarkError {
    /// `true` bila error sementara dan aman dicoba ulang.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::WriteConflict | Self::Http(_)
        )
    }

    /// Menerjemahkan kode error API Lark.
    pub(crate) fn from_code(code: i64, msg: &str) -> Self {
        match code {
            1_254_290 | 99_991_400 => Self::RateLimited,
            1_254_291 | 1_254_607 => Self::WriteConflict,
            // Token tidak valid/kedaluwarsa, refresh token dipakai ulang/dicabut.
            99_991_663 | 99_991_668 | 99_991_677 | 20_037 | 20_064 | 20_073 | 20_074 => {
                Self::ReauthRequired
            }
            _ => Self::Api {
                code,
                msg: msg.to_owned(),
            },
        }
    }
}
