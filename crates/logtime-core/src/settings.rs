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
    /// Issue that leave days are logged against (8h/day).
    pub leave_issue_id: Option<u32>,
    pub leave_activity_id: Option<u32>,
    pub default_activity_id: Option<u32>,
    /// Tracker used when creating sub-tasks under a BA's parent issue.
    pub subtask_tracker_id: Option<u32>,
    /// Run `claude` without ~/.claude/settings.json (e.g. when it sets an
    /// ANTHROPIC_API_KEY that does not work headless), using the claude.ai login.
    pub claude_ignore_user_settings: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            redmine_url: String::new(),
            accept_invalid_certs: false,
            hours_per_day: 8.0,
            workdays: vec![1, 2, 3, 4, 5],
            holidays: Vec::new(),
            leave_issue_id: None,
            leave_activity_id: None,
            default_activity_id: None,
            subtask_tracker_id: None,
            claude_ignore_user_settings: false,
        }
    }
}

impl Settings {
    pub fn is_workday(&self, date: NaiveDate) -> bool {
        let iso = date.weekday().number_from_monday() as u8;
        self.workdays.contains(&iso) && !self.holidays.contains(&date)
    }

    pub fn is_holiday(&self, date: NaiveDate) -> bool {
        self.holidays.contains(&date)
    }
}
