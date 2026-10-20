use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    InsufficientQuota(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl AppError {
    /// `SQLITE_BUSY` (5) or `SQLITE_LOCKED` (6), including extended codes such as 517.
    pub fn is_db_busy(&self) -> bool {
        let AppError::Db(sqlx::Error::Database(db)) = self else {
            return false;
        };
        db.code()
            .and_then(|c| c.parse::<i32>().ok())
            .is_some_and(|c| matches!(c & 0xff, 5 | 6))
    }
}

pub type AppResult<T> = Result<T, AppError>;
