mod ai_commands;
mod commands;
mod leave_commands;
mod plan_commands;
mod secrets;
mod state;
mod tools;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            app.manage(state::AppState::load(config_dir, data_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::save_api_key,
            commands::test_connection,
            commands::list_activities,
            commands::month_summary,
            commands::check_tools,
            commands::open_issue,
            plan_commands::get_draft,
            plan_commands::save_draft,
            plan_commands::lookup_issue,
            plan_commands::list_trackers,
            plan_commands::review_draft,
            plan_commands::preview_items,
            plan_commands::execute_items,
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
            leave_commands::discover_leave,
            leave_commands::leave_parent,
            leave_commands::leave_entries,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
