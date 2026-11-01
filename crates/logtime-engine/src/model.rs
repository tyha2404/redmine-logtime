use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Where a planned time entry goes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Target {
    /// An issue that already exists on Redmine.
    Issue { issue_id: u32, label: String },
    /// A sub-task that will be created first (see [`SubtaskDraft::key`]).
    NewSubtask { key: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    #[default]
    Work,
    Leave,
}

/// One time entry to write.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanItem {
    pub id: String,
    pub date: NaiveDate,
    pub target: Target,
    pub hours: f64,
    pub activity_id: Option<u32>,
    #[serde(default)]
    pub comments: String,
    #[serde(default)]
    pub kind: ItemKind,
}

/// A sub-task to create under a BA-owned parent issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubtaskDraft {
    /// Stable local id; also the idempotency key once created.
    pub key: String,
    pub parent_id: u32,
    #[serde(default)]
    pub parent_subject: String,
    pub project_id: u32,
    #[serde(default)]
    pub project_name: String,
    pub tracker_id: u32,
    pub subject: String,
    /// Start and due date of the new issue (a leave issue's day), if any.
    #[serde(default)]
    pub start_date: Option<NaiveDate>,
}

/// What one write sends to Redmine: the sub-tasks to create and the time
/// entries, as the user confirmed them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WritePlan {
    pub subtasks: Vec<SubtaskDraft>,
    pub items: Vec<PlanItem>,
}

impl WritePlan {
    pub fn subtask(&self, key: &str) -> Option<&SubtaskDraft> {
        self.subtasks.iter().find(|s| s.key == key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum OpPayload {
    CreateSubtask(SubtaskDraft),
    CreateTimeEntry(PlanItem),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpStatus {
    Pending,
    /// Request sent but the outcome was not recorded (e.g. the app crashed).
    /// Retrying first checks Redmine for the result to avoid duplicates.
    Running,
    Done,
    Failed,
    Undone,
}

impl OpStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OpStatus::Pending => "pending",
            OpStatus::Running => "running",
            OpStatus::Done => "done",
            OpStatus::Failed => "failed",
            OpStatus::Undone => "undone",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "running" => OpStatus::Running,
            "done" => OpStatus::Done,
            "failed" => OpStatus::Failed,
            "undone" => OpStatus::Undone,
            _ => OpStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Op {
    pub id: i64,
    pub seq: i64,
    pub payload: OpPayload,
    pub status: OpStatus,
    /// Created issue or time entry id on Redmine.
    pub result_id: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Batch {
    pub id: i64,
    pub created_at: String,
    pub label: String,
    pub redmine_url: String,
    pub ops: Vec<Op>,
}
