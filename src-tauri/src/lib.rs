mod ai_commands;
mod commands;
mod gchat_commands;
mod leave_commands;
mod plan_commands;
mod reminder;
mod secrets;
mod state;
mod tools;
mod update_commands;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

/// Passed by the login item: start in the tray without a window.
const HIDDEN_ARG: &str = "--hidden";

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::LogDir {
                        file_name: Some("redmine-logtime".into()),
                    }),
                    Target::new(TargetKind::Stdout),
                ])
                .level(log::LevelFilter::Info)
                // Libraries are noisy at info; keep their warnings only.
                .level_for("reqwest", log::LevelFilter::Warn)
                .level_for("hyper_util", log::LevelFilter::Warn)
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .max_file_size(2_000_000)
                .rotation_strategy(RotationStrategy::KeepSome(3))
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            app.manage(state::AppState::load(config_dir, data_dir)?);
            log::info!("Redmine Logtime {} khởi động", app.package_info().version);

            // Without a tray (Linux lacking AppIndicator) the app still runs;
            // closing the window then quits instead of hiding.
            if let Err(e) = tray(app.handle()) {
                log::warn!("Không tạo được biểu tượng ở thanh menu/khay: {e}");
            }
            reminder::start(app.handle().clone());
            if !std::env::args().any(|a| a == HIDDEN_ARG) {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // With reminders on, closing only hides the window: the app stays
            // in the menu bar / tray to remind at the end of the day.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<state::AppState>();
                let has_tray = window.app_handle().tray_by_id("main").is_some();
                if has_tray && state.settings().reminder_enabled {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::save_api_key,
            commands::test_connection,
            commands::list_activities,
            commands::month_summary,
            commands::check_tools,
            commands::open_log_dir,
            plan_commands::lookup_issue,
            plan_commands::search_issues,
            plan_commands::list_children,
            plan_commands::list_trackers,
            plan_commands::preview_plan,
            plan_commands::write_plan,
            plan_commands::retry_batch,
            plan_commands::resume_batch,
            plan_commands::undo_batch,
            plan_commands::list_batches,
            ai_commands::ai_test,
            ai_commands::ai_cancel,
            ai_commands::get_profile,
            ai_commands::save_profile,
            ai_commands::discover_profile,
            ai_commands::get_last_report,
            ai_commands::clear_last_report,
            ai_commands::analyze_report,
            ai_commands::plan_report,
            ai_commands::quick_report,
            ai_commands::remember_tasks,
            ai_commands::memory_count,
            ai_commands::forget_memory,
            leave_commands::discover_leave,
            leave_commands::leave_parent,
            leave_commands::leave_entries,
            gchat_commands::gchat_status,
            gchat_commands::gchat_connect,
            gchat_commands::gchat_cancel_connect,
            gchat_commands::gchat_disconnect,
            gchat_commands::gchat_spaces,
            gchat_commands::gchat_messages,
            update_commands::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        // macOS: clicking the Dock icon brings the hidden window back.
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = event {
            show_main(app);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Menu bar / tray icon: open the window, check today's hours, quit.
fn tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Mở Redmine Logtime", true, None::<&str>)?;
    let check = MenuItem::with_id(app, "check", "Kiểm tra giờ hôm nay", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &check, &sep, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Redmine Logtime")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "check" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = reminder::check(&app, true).await {
                        log::warn!("Kiểm tra giờ thất bại: {e}");
                        show_main(&app);
                    }
                });
            }
            "quit" => app.exit(0),
            _ => {}
        });
    // macOS: a one-colour template icon that follows the menu bar's light or
    // dark look. Elsewhere the coloured app icon (Ubuntu's top bar is dark).
    #[cfg(target_os = "macos")]
    {
        let glyph = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
        builder = builder.icon(glyph).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
