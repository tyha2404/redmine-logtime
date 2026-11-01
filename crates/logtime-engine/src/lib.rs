//! Write plans, execution batches, retry and undo, backed by a local SQLite store.

pub mod exec;
pub mod model;
pub mod plan;
mod store;

pub use store::Store;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Redmine(#[from] redmine_client::Error),
    #[error("Lỗi cơ sở dữ liệu cục bộ: {0}")]
    Store(#[from] rusqlite::Error),
    #[error("Dữ liệu cục bộ bị hỏng: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// The request may have been applied even though no answer came back:
    /// the connection dropped, it timed out, or the server failed (5xx).
    pub fn may_have_reached_redmine(&self) -> bool {
        match self {
            Error::Redmine(redmine_client::Error::Network(_)) => true,
            Error::Redmine(redmine_client::Error::Http { status, .. }) => *status >= 500,
            _ => false,
        }
    }
}
