//! Importing work reports from Google Chat (read-only).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use google_chat::{ChatClient, Endpoints, Loopback, Message, OAuthClient, Pkce, Space, oauth};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::open_in_browser;
use crate::secrets::GoogleAuth;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const MAX_RANGE_DAYS: i64 = 62;
const NOT_CONNECTED: &str = "Chưa kết nối Google Chat. Vào Cài đặt → Google Chat để kết nối.";

#[derive(Serialize)]
pub struct GchatStatus {
    connected: bool,
    email: Option<String>,
    /// Pre-fills the form when reconnecting.
    client_id: Option<String>,
}

fn status(state: &AppState) -> GchatStatus {
    match state.credentials.get_google() {
        Ok(Some(a)) => GchatStatus {
            connected: true,
            email: Some(a.email),
            client_id: Some(a.client_id),
        },
        _ => GchatStatus {
            connected: false,
            email: None,
            client_id: None,
        },
    }
}

#[tauri::command]
pub fn gchat_status(state: State<'_, AppState>) -> GchatStatus {
    status(&state)
}

/// Opens Google's consent page in the browser and waits for the redirect.
#[tauri::command]
pub async fn gchat_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    client_id: String,
    client_secret: String,
) -> CmdResult<GchatStatus> {
    let (client_id, client_secret) = (client_id.trim().to_string(), client_secret.trim().to_string());
    if client_id.is_empty() || client_secret.is_empty() {
        return Err("Nhập Client ID và Client secret của OAuth client (loại Desktop app).".into());
    }
    if !client_id.ends_with(".apps.googleusercontent.com") {
        return Err("Client ID có dạng …apps.googleusercontent.com.".into());
    }
    let _guard = state
        .begin_gchat_connect()
        .ok_or("Đang chờ đăng nhập Google trong trình duyệt.")?;

    let err = |e: google_chat::Error| e.to_string();
    let oauth = OAuthClient::new(&client_id, &client_secret, Endpoints::default()).map_err(err)?;
    let loopback = Loopback::bind().map_err(err)?;
    let redirect = loopback.redirect_uri().map_err(err)?;
    let pkce = Pkce::new();
    let csrf = oauth::new_state();
    open_in_browser(&oauth.auth_url(&redirect, &csrf, &pkce).map_err(err)?)?;

    let code = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        loopback.wait_for_code(&csrf, SIGN_IN_TIMEOUT, &state.gchat_cancel)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)?;

    let tokens = oauth.exchange_code(&code, &pkce.verifier, &redirect).await.map_err(err)?;
    let refresh_token = tokens
        .refresh_token
        .ok_or("Google không trả về refresh token. Thử kết nối lại.")?;
    let id_token = tokens.id_token.ok_or("Google không cho biết tài khoản đã đăng nhập.")?;
    let who = oauth::identity(&id_token).map_err(err)?;

    state.credentials.set_google(Some(GoogleAuth {
        client_id,
        client_secret,
        refresh_token,
        user_id: who.sub,
        email: who.email,
    }))?;
    cache_token(&state, tokens.access_token, tokens.expires_in);
    Ok(status(&state))
}

#[tauri::command]
pub fn gchat_cancel_connect(state: State<'_, AppState>) {
    state.gchat_cancel.store(true, Ordering::Release);
}

/// Forgets the sign-in here and asks Google to revoke it.
#[tauri::command]
pub async fn gchat_disconnect(state: State<'_, AppState>) -> CmdResult<GchatStatus> {
    let auth = state.credentials.get_google().ok().flatten();
    state.credentials.set_google(None)?;
    *state.gchat_token.lock().unwrap() = None;
    if let Some(a) = auth
        && let Ok(oauth) = OAuthClient::new(&a.client_id, &a.client_secret, Endpoints::default())
    {
        let _ = oauth.revoke(&a.refresh_token).await;
    }
    Ok(status(&state))
}

#[tauri::command]
pub async fn gchat_spaces(state: State<'_, AppState>) -> CmdResult<Vec<Space>> {
    let (client, _) = chat(&state, false).await?;
    match client.spaces().await {
        Err(google_chat::Error::Revoked) => chat(&state, true).await?.0.spaces().await,
        r => r,
    }
    .map_err(|e| e.to_string())
}

/// The signed-in user's own messages in `space`, from the start of `from` to
/// the end of `to` in local time.
#[tauri::command]
pub async fn gchat_messages(
    state: State<'_, AppState>,
    space: String,
    from: NaiveDate,
    to: NaiveDate,
) -> CmdResult<Vec<Message>> {
    if to < from {
        return Err("Ngày bắt đầu phải trước ngày kết thúc.".into());
    }
    if (to - from).num_days() >= MAX_RANGE_DAYS {
        return Err(format!("Chỉ lấy tối đa {MAX_RANGE_DAYS} ngày mỗi lần."));
    }
    let (start, end) = (local_midnight(from)?, local_midnight(to + chrono::Days::new(1))?);
    let (client, auth) = chat(&state, false).await?;
    match client.my_messages(&space, &auth.user_id, start, end).await {
        Err(google_chat::Error::Revoked) => {
            let (client, auth) = chat(&state, true).await?;
            client.my_messages(&space, &auth.user_id, start, end).await
        }
        r => r,
    }
    .map_err(|e| e.to_string())
}

fn local_midnight(day: NaiveDate) -> CmdResult<DateTime<Utc>> {
    let midnight = day.and_hms_opt(0, 0, 0).ok_or("Ngày không hợp lệ.")?;
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .ok_or_else(|| "Ngày không hợp lệ.".into())
}

/// A Chat client with a valid access token; `fresh` skips the cached one
/// (after Google rejected it).
async fn chat(state: &AppState, fresh: bool) -> CmdResult<(ChatClient, GoogleAuth)> {
    let auth = state.credentials.get_google()?.ok_or(NOT_CONNECTED)?;
    let cached = state.gchat_token.lock().unwrap().clone();
    let token = match cached {
        Some((t, exp)) if !fresh && exp > Instant::now() + Duration::from_secs(60) => t,
        _ => {
            let oauth = OAuthClient::new(&auth.client_id, &auth.client_secret, Endpoints::default())
                .map_err(|e| e.to_string())?;
            let tokens = oauth.refresh(&auth.refresh_token).await.map_err(|e| e.to_string())?;
            cache_token(state, tokens.access_token.clone(), tokens.expires_in);
            tokens.access_token
        }
    };
    let client = ChatClient::new(&token, &Endpoints::default()).map_err(|e| e.to_string())?;
    Ok((client, auth))
}

fn cache_token(state: &AppState, token: String, expires_in: u64) {
    let expires = Instant::now() + Duration::from_secs(expires_in.max(60));
    *state.gchat_token.lock().unwrap() = Some((token, expires));
}
