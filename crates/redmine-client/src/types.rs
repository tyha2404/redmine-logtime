use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdName {
    pub id: u32,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdRef {
    pub id: u32,
    /// Not sent by Redmine on time entries; the app fills it in for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// The issue's parent task, also filled in by the app for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Box<IdRef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: u32,
    pub login: String,
    #[serde(default)]
    pub firstname: String,
    #[serde(default)]
    pub lastname: String,
}

impl User {
    pub fn display_name(&self) -> String {
        format!("{} {}", self.firstname, self.lastname)
            .trim()
            .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeEntry {
    pub id: u32,
    pub project: IdName,
    pub issue: Option<IdRef>,
    pub user: IdName,
    pub activity: IdName,
    pub hours: f64,
    #[serde(default)]
    pub comments: Option<String>,
    pub spent_on: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Activity {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tracker {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub id: u32,
    pub project: IdName,
    pub tracker: IdName,
    pub status: IdName,
    pub subject: String,
    #[serde(default)]
    pub parent: Option<IdRef>,
    pub author: IdName,
    #[serde(default)]
    pub assigned_to: Option<IdName>,
}

/// Body for `POST /issues.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewIssue {
    pub project_id: u32,
    pub tracker_id: u32,
    pub subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_issue_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub parent: Option<IdName>,
}

/// Body for `POST /time_entries.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewTimeEntry {
    pub issue_id: u32,
    pub spent_on: NaiveDate,
    pub hours: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_id: Option<u32>,
    pub comments: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UserEnvelope {
    pub user: User,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TimeEntriesPage {
    pub time_entries: Vec<TimeEntry>,
    pub total_count: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ActivitiesEnvelope {
    pub time_entry_activities: Vec<Activity>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ErrorsEnvelope {
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TrackersEnvelope {
    pub trackers: Vec<Tracker>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct IssueEnvelope<T> {
    pub issue: T,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TimeEntryEnvelope<T> {
    pub time_entry: T,
}

#[derive(Debug, Deserialize)]
pub(crate) struct IssuesPage {
    pub issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectsPage {
    pub projects: Vec<Project>,
    #[serde(default)]
    pub total_count: u32,
}
