//! Commands for the review draft, write batches, history and undo.

use logtime_engine::model::{Batch, Draft, Op, OpStatus, SubtaskDraft};
use logtime_engine::plan::{self, ApiCall, DayReview};
use logtime_engine::{Store, exec};
use redmine_client::{Issue, Tracker};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::client_for;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_draft(state: State<'_, AppState>) -> CmdResult<Draft> {
    state.store.load_draft().map_err(err)
}

#[tauri::command]
pub fn save_draft(state: State<'_, AppState>, draft: Draft) -> CmdResult<()> {
    state.store.save_draft(&draft).map_err(err)
}

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

#[tauri::command]
pub async fn list_trackers(state: State<'_, AppState>) -> CmdResult<Vec<Tracker>> {
    client_for(&state.credentials, &state.settings(), None)?
        .trackers()
        .await
        .map_err(err)
}

#[derive(Serialize)]
pub struct ReviewView {
    days: Vec<DayReview>,
    subtasks: Vec<SubtaskDraft>,
    hours_per_day: f64,
}

/// The draft grouped by day, next to what is already logged on Redmine.
#[tauri::command]
pub async fn review_draft(state: State<'_, AppState>) -> CmdResult<ReviewView> {
    let settings = state.settings();
    let draft = state.store.load_draft().map_err(err)?;
    let dates = draft.items.iter().map(|i| i.date);
    let existing = match (dates.clone().min(), dates.max()) {
        (Some(from), Some(to)) => client_for(&state.credentials, &settings, None)?
            .my_time_entries(from, to)
            .await
            .map_err(err)?,
        _ => Vec::new(),
    };
    Ok(ReviewView {
        days: plan::review(&draft, &existing, &settings),
        subtasks: draft.subtasks,
        hours_per_day: settings.hours_per_day,
    })
}

/// Dry run: the requests that writing these items would send.
#[tauri::command]
pub async fn preview_items(
    state: State<'_, AppState>,
    item_ids: Vec<String>,
) -> CmdResult<Vec<ApiCall>> {
    let draft = state.store.load_draft().map_err(err)?;
    let ops = plan::build_ops(&draft, &item_ids).map_err(err)?;
    let me = client_for(&state.credentials, &state.settings(), None)?
        .current_user()
        .await
        .map_err(err)?;
    Ok(plan::preview(&ops, &draft, Some(me.id)))
}

#[derive(Clone, Serialize)]
struct ProgressEvent {
    batch_id: i64,
    op: Op,
}

/// Moves the selected items into a new batch and writes them to Redmine.
/// Items that fail go back to the draft with the error attached.
#[tauri::command]
pub async fn execute_items(
    app: AppHandle,
    state: State<'_, AppState>,
    item_ids: Vec<String>,
    label: String,
) -> CmdResult<Batch> {
    let _guard = state
        .begin_execution()
        .ok_or("Đang ghi một lô khác, đợi lô đó xong đã.")?;
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let me = client.current_user().await.map_err(err)?;
    let store = &state.store;

    let mut draft = store.load_draft().map_err(err)?;
    let ops = plan::build_ops(&draft, &item_ids).map_err(err)?;
    let batch_id = store
        .create_batch(&label, &settings.redmine_url, &ops)
        .map_err(err)?;
    draft.remove_items(&item_ids);
    store.save_draft(&draft).map_err(err)?;

    run_and_restore(&app, store, &client, batch_id, &settings.redmine_url, me.id).await
}

/// Continues a batch that was interrupted (e.g. the app closed mid-way).
#[tauri::command]
pub async fn resume_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    batch_id: i64,
) -> CmdResult<Batch> {
    let _guard = state
        .begin_execution()
        .ok_or("Đang ghi một lô khác, đợi lô đó xong đã.")?;
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let me = client.current_user().await.map_err(err)?;
    run_and_restore(
        &app,
        &state.store,
        &client,
        batch_id,
        &settings.redmine_url,
        me.id,
    )
    .await
}

async fn run_and_restore(
    app: &AppHandle,
    store: &Store,
    client: &redmine_client::RedmineClient,
    batch_id: i64,
    url: &str,
    assignee_id: u32,
) -> CmdResult<Batch> {
    let mut processed = Vec::new();
    let result = exec::run_batch(store, client, batch_id, url, assignee_id, &mut |op: &Op| {
        processed.push(op.id);
        let _ = app.emit(
            "batch-progress",
            ProgressEvent {
                batch_id,
                op: op.clone(),
            },
        );
    })
    .await;

    // Restore failures even if the run stopped early, so nothing is lost.
    let batch = store.batch(batch_id).map_err(err)?;
    if batch
        .ops
        .iter()
        .any(|o| o.status == OpStatus::Failed && processed.contains(&o.id))
    {
        let mut draft = store.load_draft().map_err(err)?;
        draft.restore_failed(&batch, &processed);
        store.save_draft(&draft).map_err(err)?;
    }
    result.map_err(err)
}

/// Deletes the time entries a batch created (sub-tasks are kept).
#[tauri::command]
pub async fn undo_batch(
    app: AppHandle,
    state: State<'_, AppState>,
    batch_id: i64,
) -> CmdResult<Batch> {
    let _guard = state
        .begin_execution()
        .ok_or("Đang ghi một lô khác, đợi lô đó xong đã.")?;
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    exec::undo_batch(
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
    .await
    .map_err(err)
}

#[tauri::command]
pub fn list_batches(state: State<'_, AppState>) -> CmdResult<Vec<Batch>> {
    state.store.list_batches(50).map_err(err)
}
