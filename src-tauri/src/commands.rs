use chrono::Local;
use logtime_core::Settings;
use logtime_core::calendar::{self, MonthSummary};
use redmine_client::{Activity, ClientOptions, RedmineClient, User};
use serde::Serialize;
use tauri::State;

use crate::secrets::Credentials;
use crate::state::AppState;
use crate::tools;

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
pub struct SettingsView {
    settings: Settings,
    has_api_key: bool,
}

pub(crate) fn client_for(
    credentials: &Credentials,
    settings: &Settings,
    api_key: Option<String>,
) -> CmdResult<RedmineClient> {
    if settings.redmine_url.trim().is_empty() {
        return Err("Chưa cấu hình URL Redmine. Vào Cài đặt để nhập.".into());
    }
    let key = match api_key {
        Some(k) if !k.trim().is_empty() => k,
        _ => credentials
            .get_api_key()?
            .ok_or("Chưa có API key. Vào Cài đặt để nhập.")?,
    };
    let options = ClientOptions {
        accept_invalid_certs: settings.accept_invalid_certs,
    };
    RedmineClient::new(&settings.redmine_url, &key, options).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<SettingsView> {
    // An unreadable key file must not block the settings screen; the user
    // can re-enter the key there.
    Ok(SettingsView {
        settings: state.settings(),
        has_api_key: matches!(state.credentials.get_api_key(), Ok(Some(_))),
    })
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    if settings.hours_per_day <= 0.0 || settings.hours_per_day > 24.0 {
        return Err("Số giờ mỗi ngày phải trong khoảng 0–24.".into());
    }
    state
        .save_settings(settings)
        .map_err(|e| format!("Không lưu được cài đặt: {e}"))
}

/// Stores the key, or removes it when `api_key` is empty.
#[tauri::command]
pub fn save_api_key(state: State<'_, AppState>, api_key: String) -> CmdResult<()> {
    if api_key.trim().is_empty() {
        state.credentials.delete_api_key()
    } else {
        state.credentials.set_api_key(&api_key)
    }
}

/// Checks a URL/key pair before saving. Falls back to the stored key when
/// `api_key` is not given.
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    redmine_url: String,
    api_key: Option<String>,
    accept_invalid_certs: bool,
) -> CmdResult<User> {
    let settings = Settings {
        redmine_url,
        accept_invalid_certs,
        ..Settings::default()
    };
    client_for(&state.credentials, &settings, api_key)?
        .current_user()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_activities(state: State<'_, AppState>) -> CmdResult<Vec<Activity>> {
    client_for(&state.credentials, &state.settings(), None)?
        .time_entry_activities()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn month_summary(
    state: State<'_, AppState>,
    year: i32,
    month: u32,
) -> CmdResult<MonthSummary> {
    let settings = state.settings();
    let (from, to) = calendar::month_range(year, month).ok_or("Tháng không hợp lệ.")?;
    let entries = client_for(&state.credentials, &settings, None)?
        .my_time_entries(from, to)
        .await
        .map_err(|e| e.to_string())?;
    let today = Local::now().date_naive();
    calendar::summarize_month(year, month, entries, &settings, today)
        .ok_or_else(|| "Tháng không hợp lệ.".into())
}

#[tauri::command]
pub async fn check_tools() -> Vec<tools::ToolStatus> {
    tauri::async_runtime::spawn_blocking(tools::check_all)
        .await
        .unwrap_or_default()
}
