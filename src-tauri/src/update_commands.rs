//! In-app update: checks the signed release for this platform on GitHub
//! (`latest.json`), then downloads and installs it and restarts.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

/// The release found by the last check, installed by `install_update`.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    current_version: String,
    /// "What's new", from `latest.json`.
    notes: Option<String>,
}

#[derive(Clone, Serialize)]
struct UpdateProgress {
    downloaded: u64,
    total: Option<u64>,
}

fn unavailable(e: &dyn std::fmt::Display) -> String {
    log::warn!("Cập nhật trong app không được: {e}");
    format!("Chưa cập nhật trong app được ({e}). Dùng lệnh cài trong Cài đặt → Ứng dụng.")
}

/// Asks GitHub for a newer signed release this build can install. None when
/// this is the newest; an error when the check cannot run (offline, no
/// signed release for this platform).
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> CmdResult<Option<UpdateInfo>> {
    let found = app
        .updater()
        .map_err(|e| unavailable(&e))?
        .check()
        .await
        .map_err(|e| unavailable(&e))?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone().filter(|n| !n.trim().is_empty()),
    });
    *pending.0.lock().unwrap() = found;
    Ok(info)
}

/// Installs the release found by `check_update` and restarts the app. Waits
/// for no write or Claude run, so a restart never cuts one off.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingUpdate>,
) -> CmdResult<()> {
    let _write = state
        .begin_execution()
        .ok_or("Đang ghi lên Redmine. Đợi ghi xong rồi cập nhật.")?;
    let _ai = state
        .begin_ai()
        .ok_or("Claude đang phân tích. Đợi xong rồi cập nhật.")?;
    let found = pending.0.lock().unwrap().clone();
    let update = match found {
        Some(u) => u,
        None => app
            .updater()
            .map_err(|e| unavailable(&e))?
            .check()
            .await
            .map_err(|e| unavailable(&e))?
            .ok_or("Đang là bản mới nhất.")?,
    };
    log::info!(
        "Cập nhật lên {} từ {}",
        update.version,
        update.current_version
    );
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit("update-progress", UpdateProgress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| unavailable(&e))?;
    log::info!("Đã cài bản {}, khởi động lại", update.version);
    app.restart();
}
