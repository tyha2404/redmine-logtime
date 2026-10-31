//! Review draft, execution batches and undo, backed by a local SQLite store.

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
