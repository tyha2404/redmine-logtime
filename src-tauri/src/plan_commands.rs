//! Commands for writing to Redmine: issue lookups, write batches, retry,
//! history and undo.

use logtime_engine::model::{Batch, Op, OpStatus, WritePlan};
use logtime_engine::plan::{self, ApiCall};
use logtime_engine::{Store, exec};
use redmine_client::{Issue, RedmineClient, TimeEntry, Tracker};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::client_for;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

const BUSY: &str = "Đang ghi một lô khác, đợi lô đó xong đã.";

#[tauri::command]
pub async fn lookup_issue(state: State<'_, AppState>, id: u32) -> CmdResult<Issue> {
    client_for(&state.credentials, &state.settings(), None)?
        .issue(id)
        .await
        .map_err(|e| match e {
            redmine_client::Error::NotFound(_) => {
                format!("Không tìm thấy issue #{id} hoặc bạn không có quyền xem.")
            }
            e => e.to_string(),
        })
}

/// Issues for a picker: `#123` or `123` finds that issue, any other text
/// searches open issues by subject.
#[tauri::command]
pub async fn search_issues(state: State<'_, AppState>, query: String) -> CmdResult<Vec<Issue>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let client = client_for(&state.credentials, &state.settings(), None)?;
    if let Ok(id) = q.trim_start_matches('#').parse::<u32>() {
        return match client.issue(id).await {
            Ok(i) => Ok(vec![i]),
            Err(redmine_client::Error::NotFound(_)) => Ok(Vec::new()),
            Err(e) => Err(err(e)),
        };
    }
    client.search_issues(q, 20).await.map_err(err)
}

/// How far back "Task cha gần đây" looks for the user's time entries.
const RECENT_DAYS: i64 = 90;

/// Open parent tasks of the issues the user logged time on lately, the most
/// recently used first: the list a parent picker opens with.
#[tauri::command]
pub async fn recent_parents(state: State<'_, AppState>) -> CmdResult<Vec<Issue>> {
    let client = client_for(&state.credentials, &state.settings(), None)?;
    let today = chrono::Local::now().date_naive();
    let entries = client
        .my_time_entries(today - chrono::Duration::days(RECENT_DAYS), today)
        .await
        .map_err(err)?;
    let logged = recent_issue_ids(&entries);
    let issues = client.issues_by_ids(&logged).await.map_err(err)?;
    let parents = parents_in_order(&logged, &issues);
    let mut found = client.open_issues_by_ids(&parents).await.map_err(err)?;
    found.sort_by_key(|i| parents.iter().position(|&p| p == i.id));
    Ok(found)
}

/// Issues the entries were logged on, latest first, each once.
fn recent_issue_ids(entries: &[TimeEntry]) -> Vec<u32> {
    let mut sorted: Vec<&TimeEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| b.spent_on.cmp(&a.spent_on));
    let mut ids = Vec::new();
    for id in sorted.iter().filter_map(|e| e.issue.as_ref().map(|i| i.id)) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Parents of `issues`, in the order of `logged`, each once.
fn parents_in_order(logged: &[u32], issues: &[Issue]) -> Vec<u32> {
    let mut parents = Vec::new();
    for id in logged {
        let parent = issues
            .iter()
            .find(|i| i.id == *id)
            .and_then(|i| i.parent.as_ref());
        if let Some(p) = parent
            && !parents.contains(&p.id)
        {
            parents.push(p.id);
        }
    }
    parents
}

/// The current user's sub-tasks of `parent_id`, to log on one that exists.
#[tauri::command]
pub async fn list_children(state: State<'_, AppState>, parent_id: u32) -> CmdResult<Vec<Issue>> {
    client_for(&state.credentials, &state.settings(), None)?
        .my_children(parent_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn list_trackers(state: State<'_, AppState>) -> CmdResult<Vec<Tracker>> {
    client_for(&state.credentials, &state.settings(), None)?
        .trackers()
        .await
        .map_err(err)
}

/// Dry run: the requests that writing this plan would send.
#[tauri::command]
pub async fn preview_plan(state: State<'_, AppState>, plan: WritePlan) -> CmdResult<Vec<ApiCall>> {
    let ops = plan::build_ops(&plan).map_err(err)?;
    let me = client_for(&state.credentials, &state.settings(), None)?
        .current_user()
        .await
        .map_err(err)?;
    Ok(plan::preview(&ops, &plan, Some(me.id)))
}

#[derive(Clone, Serialize)]
struct ProgressEvent {
    batch_id: i64,
    op: Op,
}

/// Records the plan as a batch, then writes it to Redmine. The user has
/// confirmed it in the app first. Failures stay in the batch for a retry.
#[tauri::command]
pub async fn write_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    plan: WritePlan,
    label: String,
) -> CmdResult<Batch> {
    let _guard = state.begin_execution().ok_or(BUSY)?;
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let me = client.current_user().await.map_err(err)?;
    // Re-check on this side of the bridge, with the hours Redmine holds now.
    if let (Some(from), Some(to)) = (
        plan.items.iter().map(|i| i.date).min(),
        plan.items.iter().map(|i| i.date).max(),
    ) {
        let logged: Vec<_> = client
            .my_time_entries(from, to)
            .await
            .map_err(err)?
            .into_iter()
            .map(|e| (e.spent_on, e.hours))
            .collect();
        plan::check_plan(&plan, &logged).map_err(err)?;
    }
    let ops = plan::build_ops(&plan).map_err(err)?;
    let batch_id = state
        .store
        .create_batch(&label, &settings.redmine_url, &ops)
        .map_err(err)?;
    run(
        &app,
        &state.store,
        &client,
        batch_id,
        &settings.redmine_url,
        me.id,
    )
    .await
}

/// Sends a batch's failed ops again; done ones are never repeated.
#[tauri::command]
pub async fn retry_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    batch_id: i64,
) -> CmdResult<Batch> {
    let _guard = state.begin_execution().ok_or(BUSY)?;
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let me = client.current_user().await.map_err(err)?;
    state.store.reset_failed(batch_id).map_err(err)?;
    run(
        &app,
        &state.store,
        &client,
        batch_id,
        &settings.redmine_url,
        me.id,
    )
    .await
}

/// Continues a batch that was interrupted (e.g. the app closed mid-way).
#[tauri::command]
pub async fn resume_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    batch_id: i64,
) -> CmdResult<Batch> {
    let _guard = state.begin_execution().ok_or(BUSY)?;
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let me = client.current_user().await.map_err(err)?;
    run(
        &app,
        &state.store,
        &client,
        batch_id,
        &settings.redmine_url,
        me.id,
    )
    .await
}

async fn run(
    app: &AppHandle,
    store: &Store,
    client: &RedmineClient,
    batch_id: i64,
    url: &str,
    assignee_id: u32,
) -> CmdResult<Batch> {
    let batch = exec::run_batch(store, client, batch_id, url, assignee_id, &mut |op: &Op| {
        if let Some(e) = &op.error {
            log::warn!("Lô {batch_id}, thao tác {}: {e}", op.seq);
        }
        let _ = app.emit(
            "batch-progress",
            ProgressEvent {
                batch_id,
                op: op.clone(),
            },
        );
    })
    .await;
    // Some ops may have reached Redmine even when the batch stopped.
    crate::tray::refresh_soon(app);
    let batch = batch.map_err(|e| {
        log::error!("Lô {batch_id} dừng: {e}");
        err(e)
    })?;
    let count = |s: OpStatus| batch.ops.iter().filter(|o| o.status == s).count();
    log::info!(
        "Lô {batch_id} “{}”: {} xong, {} lỗi / {} thao tác",
        batch.label,
        count(OpStatus::Done),
        count(OpStatus::Failed),
        batch.ops.len()
    );
    Ok(batch)
}

/// Deletes the time entries a batch created (sub-tasks are kept).
#[tauri::command]
pub async fn undo_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    batch_id: i64,
) -> CmdResult<Batch> {
    let _guard = state.begin_execution().ok_or(BUSY)?;
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let batch = exec::undo_batch(
        &state.store,
        &client,
        batch_id,
        &settings.redmine_url,
        &mut |op: &Op| {
            let _ = app.emit(
                "batch-progress",
                ProgressEvent {
                    batch_id,
                    op: op.clone(),
                },
            );
        },
    )
    .await;
    crate::tray::refresh_soon(&app);
    batch.map_err(err)
}

#[tauri::command]
pub fn list_batches(state: State<'_, AppState>) -> CmdResult<Vec<Batch>> {
    state.store.list_batches(50).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(issue: u32, spent_on: &str) -> TimeEntry {
        serde_json::from_value(json!({
            "id": 1, "project": {"id": 1, "name": "ABC"}, "issue": {"id": issue},
            "user": {"id": 7, "name": "Dev"}, "activity": {"id": 9, "name": "Dev"},
            "hours": 1.0, "spent_on": spent_on
        }))
        .unwrap()
    }

    fn issue(id: u32, parent: Option<u32>) -> Issue {
        serde_json::from_value(json!({
            "id": id, "subject": "x", "project": {"id": 1, "name": "ABC"},
            "tracker": {"id": 2, "name": "Task"}, "status": {"id": 1, "name": "New"},
            "author": {"id": 7, "name": "Dev"},
            "parent": parent.map(|p| json!({"id": p}))
        }))
        .unwrap()
    }

    #[test]
    fn recent_parents_follow_the_latest_entry() {
        let entries = [
            entry(10, "2026-10-01"),
            entry(20, "2026-10-20"),
            entry(11, "2026-10-15"),
            entry(10, "2026-10-02"),
        ];
        let logged = recent_issue_ids(&entries);
        assert_eq!(logged, vec![20, 11, 10]);
        // 20 and 10 share parent 2; 11 has none.
        let issues = [issue(10, Some(2)), issue(11, None), issue(20, Some(2))];
        assert_eq!(parents_in_order(&logged, &issues), vec![2]);
        let issues = [issue(10, Some(3)), issue(11, Some(1)), issue(20, Some(2))];
        assert_eq!(parents_in_order(&logged, &issues), vec![2, 1, 3]);
    }
}
