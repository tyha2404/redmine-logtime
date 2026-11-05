//! Headless CLI agents (Claude Code, Antigravity CLI, opencode): Redmine structure
//! discovery and work-report analysis through the read-only mcp-redmine
//! server.

pub mod agents;
pub mod memory;
pub mod profile;
pub mod report;
mod runner;

pub use agents::AgentInfo;
pub use logtime_core::AiAgent;
pub use runner::{AgentRunner, MCP_REDMINE_PACKAGE, Progress, RedmineMcp, extract_json};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Không tìm thấy {name}. Cài bằng: {hint}")]
    NotInstalled {
        name: &'static str,
        hint: &'static str,
    },
    /// The agent ran and reported an error (login, quota, model…).
    #[error("{name} báo lỗi: {msg}")]
    Agent { name: &'static str, msg: String },
    #[error("{name} trả kết quả không đúng định dạng: {msg}")]
    BadOutput { name: &'static str, msg: String },
    /// The agent stopped without an answer.
    #[error("{name} lỗi: {msg}")]
    Failed { name: &'static str, msg: String },
    #[error("Đã huỷ.")]
    Cancelled,
    #[error("Quá {secs} giây mà {name} chưa xong.")]
    Timeout { name: &'static str, secs: u64 },
    #[error("Lỗi hệ thống: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
