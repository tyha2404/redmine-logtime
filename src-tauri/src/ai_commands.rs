//! Commands that run the AI agent (Claude, Antigravity or opencode): a smoke test, Redmine structure discovery and
//! work-report analysis.

use std::sync::atomic::Ordering;
use std::time::Duration;

use ai_bridge::memory::{self, Memory, Remembered};
use ai_bridge::profile::{self, Profile};
use ai_bridge::report::{
    self, DayCapacity, DayPlan, Duplicate, KnownWork, ReportAnalysis, ReportContext, ReportTask,
};
use ai_bridge::{AgentRunner, AiAgent, Progress, RedmineMcp};
use chrono::{Local, NaiveDate, Utc};
use logtime_core::Settings;
use logtime_core::recurring::{self, FixedEntry};
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
const MEMORY_KEY: &str = "task_memory";

fn load_memory(state: &AppState) -> CmdResult<Memory> {
    Ok(state
        .store
        .get_json(MEMORY_KEY)
        .map_err(err)?
        .unwrap_or_default())
}

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

fn runner(state: &AppState, settings: &Settings, ignore_user_settings: bool) -> AgentRunner {
    AgentRunner {
        agent: settings.ai_agent,
        model: Some(settings.ai_model.clone()),
        program: None,
        home: None,
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

/// Runs the chosen agent. When the user's Claude config carries an API key
/// that fails headless, retries once with the claude.ai login and remembers
/// that choice.
async fn run_agent<T: DeserializeOwned>(
    state: &AppState,
    prompt: &str,
    schema: &Value,
    mcp: Option<&RedmineMcp>,
    on_progress: &mut (dyn FnMut(Progress) + Send),
) -> CmdResult<T> {
    let settings = state.settings();
    let ignore = settings.claude_ignore_user_settings;
    let first = runner(state, &settings, ignore)
        .run(prompt, schema, mcp, &state.ai_cancel, on_progress)
        .await;
    match first {
        Err(ai_bridge::Error::Agent { msg, .. })
            if settings.ai_agent == AiAgent::Claude && !ignore && is_auth_error(&msg) =>
        {
            let out = runner(state, &settings, true)
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
        other => other.map_err(|e| {
            log::warn!("AI lỗi: {e}");
            err(e)
        }),
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

/// Checks that the chosen agent works on this machine (install + login).
#[tauri::command]
pub async fn ai_test(state: State<'_, AppState>) -> CmdResult<String> {
    let _guard = state.begin_ai().ok_or("AI đang chạy một việc khác.")?;
    #[derive(Deserialize)]
    struct Pong {
        reply: String,
    }
    let schema = json!({"type": "object", "required": ["reply"], "properties": {"reply": {"type": "string"}}});
    let pong: Pong = run_agent(
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
    let _guard = state.begin_ai().ok_or("AI đang chạy một việc khác.")?;
    let settings = state.settings();
    let mcp = redmine_mcp(&state, &settings)?;
    let mut found: Profile = run_agent(
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
    /// Days dropped because this work is already logged.
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
    /// Recurring work due on the reported days, logged before the split.
    #[serde(default)]
    recurring: Vec<FixedEntry>,
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

/// Forgets the analysis once its plan is written, keeping the text, so the
/// same report is not written twice when the page opens again.
#[tauri::command]
pub fn clear_last_report(state: State<'_, AppState>, day: Option<NaiveDate>) -> CmdResult<()> {
    let key = report_key(day);
    let mut last: LastReport = state.store.get_json(&key).map_err(err)?.unwrap_or_default();
    last.view = None;
    state.store.set_json(&key, &last).map_err(err)
}

/// Report text → tasks with verified parent issues and each day's free hours.
/// With `day`, the text is that one calendar day's report. Work already
/// logged for a day is dropped from the plan as a duplicate.
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
    let _guard = state.begin_ai().ok_or("AI đang chạy một việc khác.")?;
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
    let memory = load_memory(&state)?;
    let known_tasks = memory.prompt_hint();
    let today = Local::now().date_naive();

    let prompt = report::prompt(
        &report::tidy_report(&text),
        &ReportContext {
            today,
            hours_per_day: settings.hours_per_day,
            workdays: &settings.workdays,
            profile: profile_md.as_deref(),
            day,
            known_tasks: known_tasks.as_deref(),
        },
    );
    let mut analysis: ReportAnalysis = run_agent(
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
    // The user's earlier choice beats a fresh guess for the same work.
    let remembered = memory.apply(&mut analysis);
    log::info!(
        "Phân tích báo cáo: {} việc ({} lấy từ ghi nhớ), {} ngày nghỉ, {} câu hỏi",
        analysis.tasks.len(),
        remembered,
        analysis.leave.len(),
        analysis.questions.len()
    );

    let _ = app.emit(
        "ai-progress",
        Progress {
            text: "Kiểm tra lại issue và việc đã log trên Redmine".into(),
        },
    );
    let view = build_view(&client, &settings, analysis, today, day).await?;
    remember(Some(view.clone())).map_err(err)?;
    Ok(view)
}

/// "Khớp nhanh": the report matched against tasks logged before, without
/// Claude. Lines it does not recognise are listed as questions.
#[tauri::command]
pub async fn quick_report(
    state: State<'_, AppState>,
    text: String,
    day: Option<NaiveDate>,
) -> CmdResult<ReportView> {
    if text.trim().is_empty() {
        return Err("Nhập báo cáo công việc vào trước đã.".into());
    }
    let settings = state.settings();
    let client = client_for(&state.credentials, &settings, None)?;
    let today = Local::now().date_naive();
    let analysis = memory::quick_analysis(&load_memory(&state)?, &text, today, day)?;
    log::info!(
        "Khớp nhanh: {} việc, {} dòng cần xem lại",
        analysis.tasks.len(),
        analysis.questions.len()
    );
    let view = build_view(&client, &settings, analysis, today, day).await?;
    state
        .store
        .set_json(
            &report_key(day),
            &LastReport {
                text,
                day,
                view: Some(view.clone()),
            },
        )
        .map_err(err)?;
    Ok(view)
}

/// What the user wrote for one task, sent after a successful write.
#[derive(Debug, Deserialize)]
pub struct Learned {
    pub text: String,
    pub parent_id: u32,
    pub parent_subject: String,
    pub subtask_id: Option<u32>,
    pub subtask_subject: String,
    #[serde(default)]
    pub project_hint: String,
    #[serde(default)]
    pub comment: String,
}

#[tauri::command]
pub fn remember_tasks(state: State<'_, AppState>, tasks: Vec<Learned>) -> CmdResult<usize> {
    let mut memory = load_memory(&state)?;
    let today = Local::now().date_naive();
    for t in tasks {
        memory.learn(Remembered {
            text: t.text,
            parent_id: t.parent_id,
            parent_subject: t.parent_subject,
            subtask_id: t.subtask_id,
            subtask_subject: t.subtask_subject,
            project_hint: t.project_hint,
            comment: t.comment,
            uses: 0,
            last_used: today,
        });
    }
    state.store.set_json(MEMORY_KEY, &memory).map_err(err)?;
    Ok(memory.len())
}

/// How many tasks are remembered.
#[tauri::command]
pub fn memory_count(state: State<'_, AppState>) -> CmdResult<usize> {
    Ok(load_memory(&state)?.len())
}

#[tauri::command]
pub fn forget_memory(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .store
        .set_json(MEMORY_KEY, &Memory::default())
        .map_err(err)
}

/// Checks an analysis against Redmine: drops work already logged, verifies
/// the issues, and works out each day's free hours.
async fn build_view(
    client: &redmine_client::RedmineClient,
    settings: &Settings,
    mut analysis: ReportAnalysis,
    today: NaiveDate,
    day: Option<NaiveDate>,
) -> CmdResult<ReportView> {
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

    let known: Vec<KnownWork> = existing
        .iter()
        .map(|e| KnownWork {
            date: e.spent_on,
            issue_id: e.issue.as_ref().map(|i| i.id),
            parent_id: None,
            text: e.comments.clone().unwrap_or_default(),
            hours: e.hours,
        })
        .collect();
    let mut duplicates = report::remove_duplicates(&mut analysis, &known);

    let mut tasks = Vec::new();
    for task in &analysis.tasks {
        let mut view = verify_task(client, task.clone()).await;
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
            leave_hours,
            capacity: if workday {
                (settings.hours_per_day - existing_hours - leave_hours).max(0.0)
            } else {
                0.0
            },
        });
    }

    let free: Vec<_> = days.iter().map(|d| (d.date, d.capacity)).collect();
    let logged: Vec<_> = existing
        .iter()
        .filter_map(|e| e.issue.as_ref().map(|i| (e.spent_on, i.id)))
        .collect();
    let recurring = recurring::due(settings, &free, &logged);

    Ok(ReportView {
        analysis,
        tasks,
        days,
        skipped_future,
        day,
        recurring,
    })
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

/// Recomputes the hour split, e.g. after the user excludes some tasks or
/// recurring entries (the view holds the ticked ones only).
#[tauri::command]
pub fn plan_report(view: ReportView, excluded: Vec<String>) -> Vec<DayPlan> {
    report::plan(&view.analysis, &view.days, &view.recurring, &excluded)
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
