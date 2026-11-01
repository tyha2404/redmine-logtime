//! Read-only Google Chat access for importing work reports.
//!
//! Sign-in uses OAuth for installed apps (browser + loopback redirect + PKCE)
//! with read-only scopes; nothing here can send or change a message.

mod chat;
pub mod oauth;

pub use chat::{ChatClient, Message, Space};
pub use oauth::{Identity, Loopback, OAuthClient, Pkce, Tokens};

/// Where requests go; tests point these at a mock server.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub revoke: String,
    pub chat: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
            chat: "https://chat.googleapis.com".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Đã huỷ kết nối Google.")]
    Cancelled,
    #[error("Quá thời gian chờ đăng nhập Google. Thử kết nối lại.")]
    Timeout,
    #[error("Google từ chối đăng nhập: {0}")]
    Denied(String),
    #[error("Phiên Google đã hết hạn hoặc bị thu hồi. Vào Cài đặt → Google Chat để kết nối lại.")]
    Revoked,
    #[error("Google Chat từ chối truy cập (403): {0}")]
    Forbidden(String),
    #[error("Không tìm thấy group trên Google Chat. Có thể bạn đã rời group.")]
    NotFound,
    #[error("Google trả về lỗi {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Phản hồi của Google không hợp lệ: {0}")]
    Invalid(String),
    #[error("Lỗi kết nối Google: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Lỗi máy cục bộ: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Google's message from an error body, for either API style:
/// `{"error": {"message": …}}` or `{"error": "…", "error_description": …}`.
pub(crate) fn error_message(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let e = &v["error"];
    e["message"]
        .as_str()
        .or_else(|| v["error_description"].as_str())
        .or_else(|| e.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| body.chars().take(200).collect())
}

pub(crate) fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}
