//! In-app update: downloads the signed release for this platform from
//! GitHub (`latest.json`), installs it and restarts.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

type CmdResult<T> = Result<T, String>;

#[derive(Clone, Serialize)]
struct UpdateProgress {
    downloaded: u64,
    total: Option<u64>,
}

/// Installs the newest release and restarts the app. Fails with a hint to
/// the install command when this build cannot update itself (no signed
/// release yet, or a package type the updater does not handle).
#[tauri::command]
pub async fn install_update(app: AppHandle) -> CmdResult<()> {
    let fallback = |e: &dyn std::fmt::Display| {
        log::warn!("Cập nhật trong app không được: {e}");
        format!("Chưa cập nhật trong app được ({e}). Dùng lệnh cài bên dưới.")
    };
    let update = app
        .updater()
        .map_err(|e| fallback(&e))?
        .check()
        .await
        .map_err(|e| fallback(&e))?
        .ok_or("Đang là bản mới nhất có chữ ký cập nhật.")?;
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
        .map_err(|e| fallback(&e))?;
    log::info!("Đã cài bản {}, khởi động lại", update.version);
    app.restart();
}
