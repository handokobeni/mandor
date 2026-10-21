/// Error penyimpanan; `sqlx::Error` tidak pernah bocor keluar crate.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("data not found")]
    NotFound,
    #[error("konflik: {0}")]
    Conflict(&'static str),
    #[error("storage error: {0}")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl StoreError {
    pub(crate) fn internal(e: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(e))
    }

    /// Menerjemahkan error `sqlx`; `conflict` dipakai untuk pelanggaran unik.
    pub(crate) fn from_sqlx(e: sqlx::Error, conflict: &'static str) -> Self {
        match &e {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::Database(d) if d.is_unique_violation() => Self::Conflict(conflict),
            _ => Self::internal(e),
        }
    }
}
