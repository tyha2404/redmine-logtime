//! Leave as HR runs it: each kind of leave has a project, HR opens a parent
//! issue there every month, and a day off is a sub-task of that parent.

use chrono::{Datelike, NaiveDate};
use logtime_core::leave::{classify_project, mentions_month, name_from_subject};
use logtime_core::{LeaveType, default_allowance};
use redmine_client::TimeEntry;
use serde::Serialize;
use tauri::State;

use crate::commands::client_for;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct LeaveSetup {
    types: Vec<LeaveType>,
    /// Name prefix learnt from the user's own past leave issues, if any.
    name: Option<String>,
}

/// Finds the leave projects (named "… Leave" or "Nghỉ …"), the activity
/// matching each, and how the user names themselves in leave subjects.
#[tauri::command]
pub async fn discover_leave(state: State<'_, AppState>) -> CmdResult<LeaveSetup> {
    let client = client_for(&state.credentials, &state.settings(), None)?;
    let projects = client.projects().await.map_err(err)?;
    let activities = client.time_entry_activities().await.map_err(err)?;

    let mut types: Vec<LeaveType> = projects
        .iter()
        .filter_map(|p| {
            let (key, label, verb) = classify_project(&p.name)?;
            let activity = activities
                .iter()
                .filter(|a| a.active)
                .find(|a| a.name.eq_ignore_ascii_case(&p.name))
                .or_else(|| {
                    activities
                        .iter()
                        .filter(|a| a.active)
                        .find(|a| classify_project(&a.name).map(|c| c.0) == Some(key))
                });
            Some(LeaveType {
                key: key.into(),
                label: label.into(),
                project_id: p.id,
                project_name: p.name.clone(),
                activity_id: activity.map(|a| a.id),
                verb: verb.into(),
                days_per_year: default_allowance(key),
            })
        })
        .collect();
    if types.is_empty() {
        return Err(
            "Không thấy project nghỉ phép nào (tên có “Leave” hoặc “Nghỉ”) trong các project bạn xem được."
                .into(),
        );
    }
    let order = |k: &str| {
        ["annual", "sick", "unpaid"]
            .iter()
            .position(|x| *x == k)
            .unwrap_or(9)
    };
    types.sort_by_key(|t| order(&t.key));

    let mut name = None;
    for t in &types {
        let issues = client.my_recent_issues(t.project_id).await.map_err(err)?;
        if let Some(n) = issues
            .iter()
            .filter(|i| i.parent.is_some())
            .find_map(|i| name_from_subject(&i.subject))
        {
            name = Some(n);
            break;
        }
    }
    Ok(LeaveSetup { types, name })
}

#[derive(Serialize)]
pub struct LeaveParent {
    id: u32,
    subject: String,
    project_id: u32,
    project_name: String,
    /// Leave issues use the parent's tracker.
    tracker_id: u32,
}

/// The parent issue HR opened in `project_id` for the month of `date`.
#[tauri::command]
pub async fn leave_parent(
    state: State<'_, AppState>,
    project_id: u32,
    date: NaiveDate,
) -> CmdResult<LeaveParent> {
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let found = client
        .top_level_issues(project_id)
        .await
        .map_err(err)?
        .into_iter()
        .find(|i| mentions_month(&i.subject, date.year(), date.month()));
    found
        .map(|i| LeaveParent {
            id: i.id,
            subject: i.subject,
            project_id: i.project.id,
            project_name: i.project.name,
            tracker_id: i.tracker.id,
        })
        .ok_or_else(|| {
        let project = settings
            .leave_types
            .iter()
            .find(|t| t.project_id == project_id)
            .map(|t| t.project_name.clone())
            .unwrap_or_else(|| format!("project #{project_id}"));
        format!(
            "Chưa có task cha nghỉ phép tháng {:02}/{} trong {project}. HR thường tạo vào đầu tháng; khi có rồi thử lại.",
            date.month(),
            date.year()
        )
    })
}

/// The user's time on leave projects during `year`, oldest first.
#[tauri::command]
pub async fn leave_entries(state: State<'_, AppState>, year: i32) -> CmdResult<Vec<TimeEntry>> {
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let from = NaiveDate::from_ymd_opt(year, 1, 1).ok_or("Năm không hợp lệ.")?;
    let to = NaiveDate::from_ymd_opt(year, 12, 31).ok_or("Năm không hợp lệ.")?;
    let mut entries = Vec::new();
    for t in &settings.leave_types {
        let found = client
            .my_time_entries_in(Some(t.project_id), from, to)
            .await
            .map_err(err)?;
        // The filter includes sub-projects; keep only this kind's own project.
        entries.extend(found.into_iter().filter(|e| e.project.id == t.project_id));
    }
    entries.sort_by_key(|e| e.spent_on);
    Ok(entries)
}
