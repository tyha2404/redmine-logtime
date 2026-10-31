//! Claude CLI integration: Redmine structure discovery and work-report
//! analysis through the read-only mcp-redmine server.

pub mod profile;
pub mod report;
mod runner;

pub use runner::{ClaudeRunner, MCP_REDMINE_PACKAGE, Progress, RedmineMcp, extract_json};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Không tìm thấy Claude CLI. Cài bằng: curl -fsSL https://claude.ai/install.sh | bash")]
    NotInstalled,
    #[error("Claude báo lỗi: {0}")]
    Claude(String),
    #[error("Claude trả kết quả không đúng định dạng: {0}")]
    BadOutput(String),
    #[error("Claude CLI lỗi: {0}")]
    Failed(String),
    #[error("Đã huỷ.")]
    Cancelled,
    #[error("Quá {0} giây mà Claude chưa xong.")]
    Timeout(u64),
    #[error("Lỗi hệ thống: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
