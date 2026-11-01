//! Pure planning helpers: turning the draft into operations, previewing the
//! API calls, and checking each day against the daily quota.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use logtime_core::Settings;
use redmine_client::TimeEntry;
use serde::Serialize;
use serde_json::{Value, json};

use crate::model::{Draft, ItemKind, OpPayload, PlanItem, Target};
use crate::{Error, Result};

const EPS: f64 = 1e-6;

/// Operations for the selected draft items: sub-tasks first, then time
/// entries ordered by date.
pub fn build_ops(draft: &Draft, item_ids: &[String]) -> Result<Vec<OpPayload>> {
    let mut items: Vec<&PlanItem> = draft
        .items
        .iter()
        .filter(|i| item_ids.contains(&i.id))
        .collect();
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
            let sub = draft.subtask(key).ok_or_else(|| {
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
pub fn preview(ops: &[OpPayload], draft: &Draft, assignee_id: Option<u32>) -> Vec<ApiCall> {
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
                        let subject = draft
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

#[derive(Debug, Clone, Serialize)]
pub struct DayReview {
    pub date: NaiveDate,
    pub is_workday: bool,
    pub existing_hours: f64,
    pub planned_hours: f64,
    pub total_hours: f64,
    pub existing: Vec<TimeEntry>,
    pub items: Vec<PlanItem>,
    pub warnings: Vec<String>,
}

/// One row per day that has draft items, with what is already on Redmine.
pub fn review(draft: &Draft, existing: &[TimeEntry], settings: &Settings) -> Vec<DayReview> {
    let hpd = settings.hours_per_day;
    let mut days: BTreeMap<NaiveDate, Vec<PlanItem>> = BTreeMap::new();
    for item in &draft.items {
        days.entry(item.date).or_default().push(item.clone());
    }

    days.into_iter()
        .map(|(date, items)| {
            let existing: Vec<TimeEntry> = existing
                .iter()
                .filter(|e| e.spent_on == date)
                .cloned()
                .collect();
            let existing_hours: f64 = existing.iter().map(|e| e.hours).sum();
            let planned_hours: f64 = items.iter().map(|i| i.hours).sum();
            let total = existing_hours + planned_hours;
            let is_workday = settings.is_workday(date);

            let mut warnings = Vec::new();
            if !is_workday {
                warnings.push("Ngày này là ngày nghỉ hoặc ngày lễ.".to_string());
            }
            if total > hpd + EPS {
                warnings.push(format!(
                    "Vượt {} so với {}.",
                    hours(total - hpd),
                    hours(hpd)
                ));
            } else if is_workday && total + EPS < hpd {
                warnings.push(format!(
                    "Còn thiếu {} để đủ {}.",
                    hours(hpd - total),
                    hours(hpd)
                ));
            }
            if items.iter().any(|i| i.kind == ItemKind::Leave) && existing_hours > EPS {
                warnings.push("Ngày nghỉ phép nhưng đã có logtime khác.".to_string());
            }
            if items.iter().any(|i| i.activity_id.is_none()) {
                warnings
                    .push("Có logtime chưa chọn hoạt động; Redmine thường sẽ từ chối.".to_string());
            }
            for item in &items {
                if let Target::Issue { issue_id, .. } = &item.target
                    && let Some(dup) = existing.iter().find(|e| {
                        e.issue.as_ref().is_some_and(|i| i.id == *issue_id)
                            && (e.hours - item.hours).abs() < EPS
                    })
                {
                    warnings.push(format!(
                        "Có thể trùng logtime đã có: #{issue_id} {} (entry {}).",
                        hours(dup.hours),
                        dup.id
                    ));
                }
            }

            DayReview {
                date,
                is_workday,
                existing_hours,
                planned_hours,
                total_hours: total,
                existing,
                items,
                warnings,
            }
        })
        .collect()
}

fn hours(h: f64) -> String {
    let s = format!("{h:.2}");
    format!("{}h", s.trim_end_matches('0').trim_end_matches('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SubtaskDraft;
    use redmine_client::{IdName, IdRef};

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
            last_error: None,
        }
    }

    fn sub(key: &str) -> SubtaskDraft {
        SubtaskDraft {
            key: key.into(),
            parent_id: 1,
            parent_subject: "Công nợ".into(),
            project_id: 3,
            project_name: "Salex".into(),
            tracker_id: 4,
            subject: format!("Task {key}"),
            start_date: None,
        }
    }

    fn existing(day: u32, issue: u32, hours: f64) -> TimeEntry {
        TimeEntry {
            id: 900 + day,
            project: IdName {
                id: 3,
                name: "Salex".into(),
            },
            issue: Some(IdRef { id: issue }),
            user: IdName {
                id: 7,
                name: "Me".into(),
            },
            activity: IdName {
                id: 9,
                name: "Dev".into(),
            },
            hours,
            comments: None,
            spent_on: d(day),
        }
    }

    fn draft() -> Draft {
        let new_a = Target::NewSubtask { key: "a".into() };
        Draft {
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
    fn build_ops_creates_each_subtask_once_before_entries() {
        let ids: Vec<String> = ["1", "2", "3"].map(String::from).to_vec();
        let ops = build_ops(&draft(), &ids).unwrap();
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
    fn build_ops_rejects_empty_and_bad_hours() {
        assert!(build_ops(&draft(), &[]).is_err());
        let mut bad = draft();
        bad.items[2].hours = 0.0;
        assert!(build_ops(&bad, &["3".into()]).is_err());
    }

    #[test]
    fn preview_uses_placeholder_for_new_subtask() {
        let dr = draft();
        let ops = build_ops(&dr, &["1".into()]).unwrap();
        let calls = preview(&ops, &dr, Some(7));
        assert_eq!(calls[0].path, "/issues.json");
        assert_eq!(calls[0].body["issue"]["parent_issue_id"], 1);
        assert_eq!(
            calls[1].body["time_entry"]["issue_id"],
            "<id task con \"Task a\">"
        );
    }

    #[test]
    fn review_flags_over_short_and_duplicates() {
        let settings = Settings::default();
        let rows = review(
            &draft(),
            &[existing(27, 50, 3.0), existing(28, 60, 1.0)],
            &settings,
        );
        assert_eq!(rows.len(), 2);

        let tue = &rows[0];
        assert_eq!(
            (tue.existing_hours, tue.planned_hours, tue.total_hours),
            (3.0, 8.0, 11.0)
        );
        assert!(tue.warnings.iter().any(|w| w.starts_with("Vượt 3h")));
        assert!(tue.warnings.iter().any(|w| w.contains("trùng")));

        assert!(!tue.warnings.iter().any(|w| w.contains("hoạt động")));

        let wed = &rows[1];
        assert_eq!(wed.total_hours, 4.0);
        assert!(wed.warnings.iter().any(|w| w.starts_with("Còn thiếu 4h")));
    }

    #[test]
    fn remove_items_drops_orphan_subtasks() {
        let mut dr = draft();
        dr.remove_items(&["1".into(), "2".into()]);
        assert_eq!(dr.items.len(), 1);
        assert!(dr.subtasks.is_empty());
    }
}
