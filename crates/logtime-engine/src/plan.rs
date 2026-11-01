//! Pure planning helpers: turning a write plan into operations and
//! previewing the API calls.

use serde::Serialize;
use serde_json::{Value, json};

use crate::model::{OpPayload, PlanItem, Target, WritePlan};
use crate::{Error, Result};

/// Operations for a plan: each new sub-task once (before its entries), then
/// the time entries ordered by date.
pub fn build_ops(plan: &WritePlan) -> Result<Vec<OpPayload>> {
    let mut items: Vec<&PlanItem> = plan.items.iter().collect();
    if items.is_empty() {
        return Err(Error::Invalid("Chưa chọn logtime nào để ghi.".into()));
    }
    items.sort_by_key(|i| i.date);

    let mut ops = Vec::new();
    let mut seen_keys: Vec<&str> = Vec::new();
    for item in &items {
        if !(item.hours > 0.0 && item.hours <= 24.0) {
            return Err(Error::Invalid(format!(
                "Logtime ngày {} có số giờ không hợp lệ ({}).",
                item.date, item.hours
            )));
        }
        if let Target::NewSubtask { key } = &item.target {
            if seen_keys.contains(&key.as_str()) {
                continue;
            }
            let sub = plan.subtask(key).ok_or_else(|| {
                Error::Invalid(format!(
                    "Không tìm thấy task con cho logtime ngày {}.",
                    item.date
                ))
            })?;
            seen_keys.push(key);
            ops.push(OpPayload::CreateSubtask(sub.clone()));
        }
    }
    ops.extend(
        items
            .into_iter()
            .map(|i| OpPayload::CreateTimeEntry(i.clone())),
    );
    Ok(ops)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ApiCall {
    pub method: &'static str,
    pub path: &'static str,
    pub body: Value,
}

/// The requests a batch would send, for the dry-run view.
pub fn preview(ops: &[OpPayload], plan: &WritePlan, assignee_id: Option<u32>) -> Vec<ApiCall> {
    ops.iter()
        .map(|op| match op {
            OpPayload::CreateSubtask(s) => ApiCall {
                method: "POST",
                path: "/issues.json",
                body: json!({"issue": {
                    "project_id": s.project_id,
                    "tracker_id": s.tracker_id,
                    "subject": s.subject,
                    "parent_issue_id": s.parent_id,
                    "assigned_to_id": assignee_id,
                    "start_date": s.start_date,
                    "due_date": s.start_date,
                }}),
            },
            OpPayload::CreateTimeEntry(i) => {
                let issue_id = match &i.target {
                    Target::Issue { issue_id, .. } => json!(issue_id),
                    Target::NewSubtask { key } => {
                        let subject = plan
                            .subtask(key)
                            .map(|s| s.subject.as_str())
                            .unwrap_or(key);
                        json!(format!("<id task con \"{subject}\">"))
                    }
                };
                ApiCall {
                    method: "POST",
                    path: "/time_entries.json",
                    body: json!({"time_entry": {
                        "issue_id": issue_id,
                        "spent_on": i.date,
                        "hours": i.hours,
                        "activity_id": i.activity_id,
                        "comments": i.comments,
                    }}),
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemKind, SubtaskDraft};
    use chrono::NaiveDate;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn item(id: &str, day: u32, target: Target, hours: f64) -> PlanItem {
        PlanItem {
            id: id.into(),
            date: d(day),
            target,
            hours,
            activity_id: Some(9),
            comments: format!("c-{id}"),
            kind: ItemKind::Work,
        }
    }

    fn sub(key: &str) -> SubtaskDraft {
        SubtaskDraft {
            key: key.into(),
            parent_id: 1,
            parent_subject: "Công nợ".into(),
            project_id: 3,
            project_name: "ABC".into(),
            tracker_id: 4,
            subject: format!("Task {key}"),
            start_date: None,
        }
    }

    fn plan() -> WritePlan {
        let new_a = Target::NewSubtask { key: "a".into() };
        WritePlan {
            subtasks: vec![sub("a"), sub("unused")],
            items: vec![
                item("2", 28, new_a.clone(), 3.0),
                item("1", 27, new_a, 5.0),
                item(
                    "3",
                    27,
                    Target::Issue {
                        issue_id: 50,
                        label: "#50".into(),
                    },
                    3.0,
                ),
            ],
        }
    }

    #[test]
    fn build_ops_creates_each_used_subtask_once_before_entries() {
        let ops = build_ops(&plan()).unwrap();
        assert_eq!(ops.len(), 4);
        assert!(matches!(&ops[0], OpPayload::CreateSubtask(s) if s.key == "a"));
        let dates: Vec<_> = ops[1..]
            .iter()
            .map(|o| match o {
                OpPayload::CreateTimeEntry(i) => i.date,
                _ => panic!("expected entry"),
            })
            .collect();
        assert_eq!(dates, vec![d(27), d(27), d(28)]);
    }

    #[test]
    fn build_ops_rejects_empty_bad_hours_and_missing_subtask() {
        assert!(build_ops(&WritePlan::default()).is_err());
        let mut bad = plan();
        bad.items[2].hours = 0.0;
        assert!(build_ops(&bad).is_err());
        let mut orphan = plan();
        orphan.subtasks.clear();
        assert!(build_ops(&orphan).is_err());
    }

    #[test]
    fn preview_uses_placeholder_for_new_subtask() {
        let mut p = plan();
        p.items.retain(|i| i.id == "1");
        let ops = build_ops(&p).unwrap();
        let calls = preview(&ops, &p, Some(7));
        assert_eq!(calls[0].path, "/issues.json");
        assert_eq!(calls[0].body["issue"]["parent_issue_id"], 1);
        assert_eq!(
            calls[1].body["time_entry"]["issue_id"],
            "<id task con \"Task a\">"
        );
    }
}
