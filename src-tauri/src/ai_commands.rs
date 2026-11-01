//! Commands that run Claude: a smoke test, Redmine structure discovery and
//! work-report analysis.

use std::sync::atomic::Ordering;
use std::time::Duration;

use ai_bridge::profile::{self, Profile};
use ai_bridge::report::{
    self, DayCapacity, DayPlan, Duplicate, KnownWork, ReportAnalysis, ReportContext, ReportTask,
};
use ai_bridge::{ClaudeRunner, Progress, RedmineMcp};
use chrono::{Local, NaiveDate, Utc};
use logtime_core::Settings;
use logtime_engine::model::Target;
use redmine_client::Issue;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};

use crate::commands::client_for;
use crate::state::AppState;
use crate::tools;

type CmdResult<T> = Result<T, String>;

const PROFILE_KEY: &str = "profile";
const REPORT_KEY: &str = "last_report";

/// Where a report is kept: the report page has one slot, each calendar day
/// its own.
fn report_key(day: Option<NaiveDate>) -> String {
    match day {
        Some(d) => format!("day_report:{d}"),
        None => REPORT_KEY.to_string(),
    }
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn runner(state: &AppState, ignore_user_settings: bool) -> ClaudeRunner {
    ClaudeRunner {
        program: "claude".into(),
        path_env: tools::login_path().to_string(),
        workdir: state.ai_workdir.clone(),
        timeout: Duration::from_secs(15 * 60),
        ignore_user_settings,
    }
}

/// Claude rejected its credentials (e.g. a stale ANTHROPIC_API_KEY).
fn is_auth_error(msg: &str) -> bool {
    let m = msg.to_lowercase();
    ["api key", "authentication", "unauthorized", "401", "oauth"]
        .iter()
        .any(|k| m.contains(k))
}

/// Runs Claude. When the user's Claude config carries an API key that fails
/// headless, retries once with the claude.ai login and remembers that choice.
async fn run_claude<T: DeserializeOwned>(
    state: &AppState,
    prompt: &str,
    schema: &Value,
    mcp: Option<&RedmineMcp>,
    on_progress: &mut (dyn FnMut(Progress) + Send),
) -> CmdResult<T> {
    let ignore = state.settings().claude_ignore_user_settings;
    let first = runner(state, ignore)
        .run(prompt, schema, mcp, &state.ai_cancel, on_progress)
        .await;
    match first {
        Err(ai_bridge::Error::Claude(msg)) if !ignore && is_auth_error(&msg) => {
            let out = runner(state, true)
                .run(prompt, schema, mcp, &state.ai_cancel, on_progress)
                .await
                .map_err(|e| {
                    format!("{e} (Đã thử cả cấu hình Claude của bạn lẫn tài khoản claude.ai. Chạy `claude` trong terminal và đăng nhập bằng /login.)")
                })?;
            let mut settings = state.settings();
            settings.claude_ignore_user_settings = true;
            state.save_settings(settings).map_err(err)?;
            Ok(out)
        }
        other => other.map_err(err),
    }
}

fn redmine_mcp(state: &AppState, settings: &Settings) -> CmdResult<RedmineMcp> {
    let api_key = state
        .credentials
        .get_api_key()?
        .ok_or("Chưa có API key. Vào Cài đặt để nhập.")?;
    Ok(RedmineMcp {
        url: settings.redmine_url.clone(),
        api_key,
        accept_invalid_certs: settings.accept_invalid_certs,
        instructions: Some(state.profile_md_path.clone()),
    })
}

fn emitter(app: &AppHandle) -> impl FnMut(Progress) + Send + '_ {
    move |p| {
        let _ = app.emit("ai-progress", p);
    }
}

#[tauri::command]
pub fn ai_cancel(state: State<'_, AppState>) {
    state.ai_cancel.store(true, Ordering::Release);
}

/// Checks that `claude -p` works on this machine (install + login).
#[tauri::command]
pub async fn ai_test(state: State<'_, AppState>) -> CmdResult<String> {
    let _guard = state.begin_ai().ok_or("Claude đang chạy một việc khác.")?;
    #[derive(Deserialize)]
    struct Pong {
        reply: String,
    }
    let schema = json!({"type": "object", "required": ["reply"], "properties": {"reply": {"type": "string"}}});
    let pong: Pong = run_claude(
        &state,
        "Trả lời đúng một câu ngắn bằng tiếng Việt: \"Sẵn sàng phân tích báo cáo.\"",
        &schema,
        None,
        &mut |_| {},
    )
    .await?;
    Ok(pong.reply)
}

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>) -> CmdResult<Option<Profile>> {
    state.store.get_json(PROFILE_KEY).map_err(err)
}

/// Saves the profile and refreshes the instructions file mcp-redmine reads.
#[tauri::command]
pub fn save_profile(state: State<'_, AppState>, profile: Profile) -> CmdResult<()> {
    store_profile(&state, &profile)
}

fn store_profile(state: &AppState, profile: &Profile) -> CmdResult<()> {
    state.store.set_json(PROFILE_KEY, profile).map_err(err)?;
    std::fs::write(&state.profile_md_path, profile::to_instructions(profile))
        .map_err(|e| format!("Không ghi được file hướng dẫn cho MCP: {e}"))
}

/// Lets Claude explore Redmine (read-only) and work out how BA parent tasks
/// look. Keeps the user's own notes from the previous profile.
#[tauri::command]
pub async fn discover_profile(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Profile> {
    let _guard = state.begin_ai().ok_or("Claude đang chạy một việc khác.")?;
    let settings = state.settings();
    let mcp = redmine_mcp(&state, &settings)?;
    let mut found: Profile = run_claude(
        &state,
        &profile::discovery_prompt(),
        &profile::schema(),
        Some(&mcp),
        &mut emitter(&app),
    )
    .await?;

    let previous: Option<Profile> = state.store.get_json(PROFILE_KEY).map_err(err)?;
    found.user_notes = previous.map(|p| p.user_notes).unwrap_or_default();
    found.generated_at = Some(Utc::now().to_rfc3339());
    store_profile(&state, &found)?;
    Ok(found)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueBrief {
    id: u32,
    subject: String,
    project_id: u32,
    project_name: String,
    tracker: String,
    status: String,
    parent_id: Option<u32>,
}

impl From<Issue> for IssueBrief {
    fn from(i: Issue) -> Self {
        Self {
            id: i.id,
            subject: i.subject,
            project_id: i.project.id,
            project_name: i.project.name,
            tracker: i.tracker.name,
            status: i.status.name,
            parent_id: i.parent.map(|p| p.id),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskView {
    task: ReportTask,
    parent: Option<IssueBrief>,
    existing_subtask: Option<IssueBrief>,
    problems: Vec<String>,
    /// Days dropped because this work is already logged or queued.
    #[serde(default)]
    duplicates: Vec<Duplicate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportView {
    analysis: ReportAnalysis,
    tasks: Vec<TaskView>,
    days: Vec<DayCapacity>,
    /// Dates after today that were left out of the plan.
    skipped_future: Vec<NaiveDate>,
    /// The calendar day this report is for, when reported per day.
    #[serde(default)]
    day: Option<NaiveDate>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct LastReport {
    text: String,
    #[serde(default)]
    day: Option<NaiveDate>,
    view: Option<ReportView>,
}

#[tauri::command]
pub fn get_last_report(
    state: State<'_, AppState>,
    day: Option<NaiveDate>,
) -> CmdResult<LastReport> {
    Ok(state
        .store
        .get_json(&report_key(day))
        .map_err(err)?
        .unwrap_or_default())
}

/// Forgets the analysis once its plan is queued, keeping the text, so the
/// same report is not queued twice when the page opens again.
#[tauri::command]
pub fn clear_last_report(state: State<'_, AppState>, day: Option<NaiveDate>) -> CmdResult<()> {
    let key = report_key(day);
    let mut last: LastReport = state.store.get_json(&key).map_err(err)?.unwrap_or_default();
    last.view = None;
    state.store.set_json(&key, &last).map_err(err)
}

/// Report text → tasks with verified parent issues and each day's free hours.
/// With `day`, the text is that one calendar day's report. Work already
/// logged or queued for a day is dropped from the plan as a duplicate.
#[tauri::command]
pub async fn analyze_report(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    day: Option<NaiveDate>,
) -> CmdResult<ReportView> {
    if text.trim().is_empty() {
        return Err("Nhập báo cáo công việc vào trước đã.".into());
    }
    let _guard = state.begin_ai().ok_or("Claude đang chạy một việc khác.")?;
    let remember = |view: Option<ReportView>| {
        state.store.set_json(
            &report_key(day),
            &LastReport {
                text: text.clone(),
                day,
                view,
            },
        )
    };
    remember(None).map_err(err)?;

    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let mcp = redmine_mcp(&state, &settings)?;
    let profile: Option<Profile> = state.store.get_json(PROFILE_KEY).map_err(err)?;
    let profile_md = profile.as_ref().map(profile::to_instructions);
    let today = Local::now().date_naive();

    let prompt = report::prompt(
        &text,
        &ReportContext {
            today,
            hours_per_day: settings.hours_per_day,
            workdays: &settings.workdays,
            profile: profile_md.as_deref(),
            day,
        },
    );
    let mut analysis: ReportAnalysis = run_claude(
        &state,
        &prompt,
        &report::schema(),
        Some(&mcp),
        &mut emitter(&app),
    )
    .await?;
    if let Some(d) = day {
        analysis.pin_to_day(d);
    }

    let _ = app.emit(
        "ai-progress",
        Progress {
            text: "Kiểm tra lại issue và việc đã log trên Redmine".into(),
        },
    );
    // A day picked on the calendar is planned even if it is in the future.
    let last = if day.is_some() {
        analysis.period_to
    } else {
        analysis.period_to.min(today)
    };
    let existing = if analysis.period_from <= last {
        client
            .my_time_entries(analysis.period_from, last)
            .await
            .map_err(err)?
    } else {
        Vec::new()
    };
    let draft = state.store.load_draft().map_err(err)?;

    let mut known: Vec<KnownWork> = existing
        .iter()
        .map(|e| KnownWork {
            date: e.spent_on,
            issue_id: e.issue.as_ref().map(|i| i.id),
            parent_id: None,
            text: e.comments.clone().unwrap_or_default(),
            hours: e.hours,
            queued: false,
        })
        .collect();
    known.extend(draft.items.iter().map(|i| match &i.target {
        Target::Issue { issue_id, .. } => KnownWork {
            date: i.date,
            issue_id: Some(*issue_id),
            parent_id: None,
            text: i.comments.clone(),
            hours: i.hours,
            queued: true,
        },
        Target::NewSubtask { key } => {
            let sub = draft.subtask(key);
            KnownWork {
                date: i.date,
                issue_id: None,
                parent_id: sub.map(|s| s.parent_id),
                text: sub
                    .map(|s| s.subject.clone())
                    .unwrap_or_else(|| i.comments.clone()),
                hours: i.hours,
                queued: true,
            }
        }
    }));
    let mut duplicates = report::remove_duplicates(&mut analysis, &known);

    let mut tasks = Vec::new();
    for task in &analysis.tasks {
        let mut view = verify_task(&client, task.clone()).await;
        view.duplicates = duplicates.remove(&task.key).unwrap_or_default();
        tasks.push(view);
    }

    let mut days = Vec::new();
    let mut skipped_future = Vec::new();
    for date in analysis
        .period_from
        .iter_days()
        .take_while(|d| *d <= analysis.period_to)
    {
        if date > last {
            if settings.is_workday(date) {
                skipped_future.push(date);
            }
            continue;
        }
        let existing_hours: f64 = existing
            .iter()
            .filter(|e| e.spent_on == date)
            .map(|e| e.hours)
            .sum();
        let queued_hours: f64 = draft
            .items
            .iter()
            .filter(|i| i.date == date)
            .map(|i| i.hours)
            .sum();
        let leave_hours: f64 = analysis
            .leave
            .iter()
            .filter(|l| l.date == date)
            .map(|l| l.hours)
            .sum();
        let workday = settings.is_workday(date);
        days.push(DayCapacity {
            date,
            workday,
            existing_hours,
            queued_hours,
            leave_hours,
            capacity: if workday {
                (settings.hours_per_day - existing_hours - queued_hours - leave_hours).max(0.0)
            } else {
                0.0
            },
        });
    }

    let view = ReportView {
        analysis,
        tasks,
        days,
        skipped_future,
        day,
    };
    remember(Some(view.clone())).map_err(err)?;
    Ok(view)
}

/// Checks the ids the model proposed actually exist and fit together.
async fn verify_task(client: &redmine_client::RedmineClient, task: ReportTask) -> TaskView {
    let mut problems = Vec::new();
    let parent = match task.parent_issue_id {
        Some(id) => match client.issue(id).await {
            Ok(i) => Some(IssueBrief::from(i)),
            Err(e) => {
                problems.push(format!("Không đọc được task cha #{id}: {e}"));
                None
            }
        },
        None => {
            problems.push("Chưa tìm được task cha, hãy nhập số issue.".into());
            None
        }
    };
    let existing_subtask = match task.existing_subtask_id {
        Some(id) => match client.issue(id).await {
            Ok(i)
                if parent
                    .as_ref()
                    .is_some_and(|p| i.parent.as_ref().map(|x| x.id) == Some(p.id)) =>
            {
                Some(IssueBrief::from(i))
            }
            Ok(_) => {
                problems.push(format!(
                    "#{id} không phải task con của task cha đã chọn; sẽ tạo task con mới."
                ));
                None
            }
            Err(_) => None,
        },
        None => None,
    };
    if task.parent_confidence < 0.6 && parent.is_some() {
        problems.push("AI không chắc chắn về task cha này, hãy kiểm tra.".into());
    }
    TaskView {
        task,
        parent,
        existing_subtask,
        problems,
        duplicates: Vec::new(),
    }
}

/// Recomputes the hour split, e.g. after the user excludes some tasks.
#[tauri::command]
pub fn plan_report(view: ReportView, excluded: Vec<String>) -> Vec<DayPlan> {
    report::plan(&view.analysis, &view.days, &excluded)
}

#[cfg(test)]
mod tests {
    use super::is_auth_error;

    #[test]
    fn recognises_auth_failures() {
        assert!(is_auth_error("Invalid API key · Fix external API key"));
        assert!(is_auth_error("OAuth token has expired"));
        assert!(!is_auth_error("Không đọc được task cha #12"));
    }
}
