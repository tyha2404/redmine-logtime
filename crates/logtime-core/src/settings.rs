use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

/// User configuration (everything except the API key, which has its own private file).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub redmine_url: String,
    pub accept_invalid_certs: bool,
    pub hours_per_day: f64,
    /// ISO weekday numbers, Monday = 1 … Sunday = 7.
    pub workdays: Vec<u8>,
    pub holidays: Vec<NaiveDate>,
    /// Kinds of leave. Each lives in its own project, where HR opens a parent
    /// issue every month; a day off is a new sub-task of that month's parent.
    pub leave_types: Vec<LeaveType>,
    /// How you appear at the start of a leave issue's subject, e.g. "BE - Tên".
    pub leave_name: String,
    pub default_activity_id: Option<u32>,
    /// Tracker used when creating sub-tasks under a BA's parent issue.
    pub subtask_tracker_id: Option<u32>,
    /// Run `claude` without ~/.claude/settings.json (e.g. when it sets an
    /// ANTHROPIC_API_KEY that does not work headless), using the claude.ai login.
    pub claude_ignore_user_settings: bool,
    /// Notify on workdays at `reminder_time` when today or this week is short.
    /// While on, closing the window keeps the app in the menu bar / tray.
    pub reminder_enabled: bool,
    /// "HH:MM", local time.
    pub reminder_time: String,
    /// Start (hidden, in the tray) when the user logs in.
    pub start_at_login: bool,
    /// Work done the same way on fixed weekdays (stand-up, weekly meeting),
    /// logged with its own hours before a report's hours are split.
    pub recurring: Vec<RecurringTask>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            redmine_url: String::new(),
            accept_invalid_certs: false,
            hours_per_day: 8.0,
            workdays: vec![1, 2, 3, 4, 5],
            holidays: Vec::new(),
            leave_types: Vec::new(),
            leave_name: String::new(),
            default_activity_id: None,
            subtask_tracker_id: None,
            claude_ignore_user_settings: false,
            reminder_enabled: true,
            reminder_time: "17:30".into(),
            start_at_login: false,
            recurring: Vec::new(),
        }
    }
}

/// One piece of recurring work, logged on an existing issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct RecurringTask {
    /// Stable local id.
    pub id: String,
    pub issue_id: u32,
    /// "#123 Họp daily", for display.
    pub label: String,
    pub hours: f64,
    /// ISO weekdays it happens on (Monday = 1); empty means every workday.
    pub weekdays: Vec<u8>,
    /// None uses the default activity.
    pub activity_id: Option<u32>,
    /// The time entry's comment; the issue subject when empty.
    pub comments: String,
    /// Kept in the list but not proposed.
    pub paused: bool,
}

impl RecurringTask {
    /// Whether it is proposed on `date` (a workday, see `Settings::is_workday`).
    pub fn on(&self, date: NaiveDate) -> bool {
        let iso = date.weekday().number_from_monday() as u8;
        !self.paused && (self.weekdays.is_empty() || self.weekdays.contains(&iso))
    }
}

/// One kind of leave (annual, unplanned, unpaid…).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct LeaveType {
    /// "annual", "sick", "unpaid" or "other".
    pub key: String,
    /// Shown in the app, e.g. "Nghỉ phép năm".
    pub label: String,
    /// The project holding this kind's monthly parents and leave issues.
    pub project_id: u32,
    pub project_name: String,
    pub activity_id: Option<u32>,
    /// Goes after the name in a leave issue's subject, e.g. "xin nghỉ phép".
    pub verb: String,
    /// Days allowed per year; `None` means no limit (unpaid leave).
    pub days_per_year: Option<f64>,
}

/// The usual yearly allowance for a kind of leave.
pub fn default_allowance(key: &str) -> Option<f64> {
    match key {
        "annual" => Some(12.0),
        "sick" => Some(4.0),
        _ => None,
    }
}

impl Settings {
    /// Time on a leave project counts as leave on the calendar.
    pub fn is_leave_project(&self, project_id: u32) -> bool {
        self.leave_types.iter().any(|t| t.project_id == project_id)
    }

    pub fn is_workday(&self, date: NaiveDate) -> bool {
        let iso = date.weekday().number_from_monday() as u8;
        self.workdays.contains(&iso) && !self.holidays.contains(&date)
    }

    pub fn is_holiday(&self, date: NaiveDate) -> bool {
        self.holidays.contains(&date)
    }
}
