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

/// One time entry waiting for review.
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
    /// Why the last attempt to write this item failed, if it did.
    #[serde(default)]
    pub last_error: Option<String>,
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

/// Everything waiting for review. Persisted so it survives restarts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Draft {
    pub subtasks: Vec<SubtaskDraft>,
    pub items: Vec<PlanItem>,
}

impl Draft {
    pub fn subtask(&self, key: &str) -> Option<&SubtaskDraft> {
        self.subtasks.iter().find(|s| s.key == key)
    }

    /// Drops the given items, then any sub-task no remaining item refers to.
    pub fn remove_items(&mut self, ids: &[String]) {
        self.items.retain(|i| !ids.contains(&i.id));
        let used: Vec<&str> = self
            .items
            .iter()
            .filter_map(|i| match &i.target {
                Target::NewSubtask { key } => Some(key.as_str()),
                Target::Issue { .. } => None,
            })
            .collect();
        self.subtasks.retain(|s| used.contains(&s.key.as_str()));
    }

    /// Puts the time entries among `op_ids` that failed back for fixing,
    /// together with the sub-task drafts they still need. Pass only the ops
    /// processed in the latest run so earlier failures are not restored twice.
    pub fn restore_failed(&mut self, batch: &Batch, op_ids: &[i64]) {
        let subtask_op = |key: &str| {
            batch
                .ops
                .iter()
                .find(|o| matches!(&o.payload, OpPayload::CreateSubtask(s) if s.key == key))
        };
        for op in &batch.ops {
            let OpPayload::CreateTimeEntry(item) = &op.payload else {
                continue;
            };
            if op.status != OpStatus::Failed || !op_ids.contains(&op.id) {
                continue;
            }
            let mut item = item.clone();
            item.last_error = op.error.clone();
            if let Target::NewSubtask { key } = &item.target
                && let Some(sub_op) = subtask_op(key)
            {
                if sub_op.status == OpStatus::Failed {
                    item.last_error = sub_op.error.clone();
                }
                if let (OpPayload::CreateSubtask(sub), None) = (&sub_op.payload, self.subtask(key))
                {
                    self.subtasks.push(sub.clone());
                }
            }
            self.items.push(item);
        }
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
