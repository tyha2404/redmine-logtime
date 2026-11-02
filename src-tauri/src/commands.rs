use chrono::Local;
use logtime_core::Settings;
use logtime_core::calendar::{self, MonthSummary};
use redmine_client::{Activity, ClientOptions, IdRef, Issue, RedmineClient, TimeEntry, User};
use serde::Serialize;
use std::collections::HashMap;
use tauri::{Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::secrets::Credentials;
use crate::state::AppState;
use crate::tools;

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
pub struct SettingsView {
    settings: Settings,
    has_api_key: bool,
    /// Where the API key and Google sign-in are kept.
    secret_storage: crate::secrets::Storage,
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
        secret_storage: state.credentials.storage(),
    })
}

#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> CmdResult<()> {
    if settings.hours_per_day <= 0.0 || settings.hours_per_day > 24.0 {
        return Err("Số giờ mỗi ngày phải lớn hơn 0 và không quá 24.".into());
    }
    if settings.reminder_enabled && crate::reminder::parse_time(&settings.reminder_time).is_none() {
        return Err("Giờ nhắc phải có dạng HH:MM, ví dụ 17:30.".into());
    }
    let login = settings.start_at_login;
    state
        .save_settings(settings)
        .map_err(|e| format!("Không lưu được cài đặt: {e}"))?;
    // Hours per day, workdays or the Redmine account may have changed.
    crate::tray::refresh_soon(&app);
    let autostart = app.autolaunch();
    if autostart.is_enabled().unwrap_or(!login) != login {
        let done = if login {
            autostart.enable()
        } else {
            autostart.disable()
        };
        done.map_err(|e| format!("Đã lưu, nhưng chưa đổi được việc mở cùng máy: {e}"))?;
    }
    Ok(())
}

/// Opens the folder holding the app's log files, to attach to a bug report.
#[tauri::command]
pub fn open_log_dir(app: tauri::AppHandle) -> CmdResult<()> {
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open_in_browser(&dir.to_string_lossy())
}

/// Stores the key, or removes it when `api_key` is empty.
#[tauri::command]
pub fn save_api_key(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    api_key: String,
) -> CmdResult<()> {
    if api_key.trim().is_empty() {
        state.credentials.delete_api_key()?;
    } else {
        state.credentials.set_api_key(&api_key)?;
    }
    crate::tray::refresh_soon(&app);
    Ok(())
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
    let client = client_for(&state.credentials, &settings, None)?;
    let mut entries = client
        .my_time_entries(from, to)
        .await
        .map_err(|e| e.to_string())?;
    add_issue_subjects(&client, &mut entries).await;
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

/// Time entries carry only the issue id; look up each issue's subject and
/// parent task (one request per 100 issues, plus one for the parents) so the
/// calendar can say what was logged. Best effort: the month still shows if a
/// lookup fails.
async fn add_issue_subjects(client: &RedmineClient, entries: &mut [TimeEntry]) {
    let mut ids: Vec<u32> = entries
        .iter()
        .filter_map(|e| e.issue.as_ref().map(|i| i.id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return;
    }
    let mut issues: HashMap<u32, Issue> = match client.issues_by_ids(&ids).await {
        Ok(found) => found.into_iter().map(|i| (i.id, i)).collect(),
        Err(e) => {
            log::warn!("issue subjects for the month: {e}");
            return;
        }
    };
    let mut parents: Vec<u32> = issues
        .values()
        .filter_map(|i| i.parent.as_ref().map(|p| p.id))
        .filter(|id| !issues.contains_key(id))
        .collect();
    parents.sort_unstable();
    parents.dedup();
    if !parents.is_empty() {
        match client.issues_by_ids(&parents).await {
            Ok(found) => issues.extend(found.into_iter().map(|i| (i.id, i))),
            Err(e) => log::warn!("parent tasks for the month: {e}"),
        }
    }
    for issue in entries.iter_mut().filter_map(|e| e.issue.as_mut()) {
        let Some(found) = issues.get(&issue.id) else {
            continue;
        };
        issue.subject = Some(found.subject.clone());
        issue.parent = found.parent.as_ref().map(|p| {
            Box::new(IdRef {
                id: p.id,
                subject: issues.get(&p.id).map(|i| i.subject.clone()),
                parent: None,
            })
        });
    }
}

pub(crate) fn open_in_browser(url: &str) -> CmdResult<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Không mở được trình duyệt: {e}"))
}
