//! Pure planning helpers: turning a write plan into operations and
//! previewing the API calls.

use std::collections::{BTreeMap, HashSet};

use chrono::NaiveDate;
use redmine_client::Issue;
use serde::Serialize;
use serde_json::{Value, json};

use crate::model::{OpPayload, PlanItem, Target, WritePlan};
use crate::{Error, Result};

/// Operations for a plan: issue renames first, then each new sub-task once
/// (before its entries), then the new entries ordered by date.
pub fn build_ops(plan: &WritePlan) -> Result<Vec<OpPayload>> {
    if plan.is_empty() {
        return Err(Error::Invalid("Chưa chọn logtime nào để ghi.".into()));
    }
    let mut items: Vec<&PlanItem> = plan.items.iter().collect();
    items.sort_by_key(|i| i.date);

    let mut ops = Vec::new();
    let mut renamed = HashSet::new();
    for r in &plan.renames {
        if !renamed.insert(r.issue_id) {
            return Err(Error::Invalid(format!(
                "Issue #{} được đổi tên hai lần trong lô ghi.",
                r.issue_id
            )));
        }
        if r.after.trim().is_empty() {
            return Err(Error::Invalid(format!(
                "Tên mới của issue #{} đang trống.",
                r.issue_id
            )));
        }
        ops.push(OpPayload::RenameIssue(r.clone()));
    }

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

/// Hours one day can hold, whatever the UI allowed.
pub const MAX_HOURS_PER_DAY: f64 = 24.0;
const EPS: f64 = 1e-6;

/// Sets each rename's `before` to the name Redmine holds now (`current`),
/// so undo restores the real previous name. Fails when an issue cannot be
/// seen any more.
pub fn refresh_renames(plan: &mut WritePlan, current: &[Issue]) -> Result<()> {
    for r in &mut plan.renames {
        let issue = current.iter().find(|i| i.id == r.issue_id).ok_or_else(|| {
            Error::Invalid(format!(
                "Không đọc được issue #{} trên Redmine. Tải lại lịch rồi thử lại.",
                r.issue_id
            ))
        })?;
        r.before = issue.subject.clone();
    }
    Ok(())
}

/// The last check before anything is sent, independent of the UI: every
/// entry has sane hours and a target, every new sub-task is complete, every
/// rename has a name, and no day ends up above 24h counting what Redmine
/// already holds (`logged`).
pub fn check_plan(plan: &WritePlan, logged: &[(NaiveDate, f64)]) -> Result<()> {
    let bad = |msg: String| Err(Error::Invalid(msg));
    if plan.is_empty() {
        return bad("Chưa chọn logtime nào để ghi.".into());
    }
    for r in &plan.renames {
        if r.issue_id == 0 || r.after.trim().is_empty() {
            return bad(format!("Tên mới của issue #{} đang trống.", r.issue_id));
        }
    }
    let mut per_day: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    for (date, hours) in logged {
        *per_day.entry(*date).or_default() += hours;
    }
    let mut ids = HashSet::new();
    for item in &plan.items {
        let day = item.date.format("%d/%m/%Y");
        if !ids.insert(item.id.as_str()) {
            return bad(format!("Logtime ngày {day} bị lặp trong lô ghi."));
        }
        if !(item.hours.is_finite() && item.hours > 0.0 && item.hours <= MAX_HOURS_PER_DAY) {
            return bad(format!(
                "Logtime ngày {day} có số giờ không hợp lệ ({}).",
                item.hours
            ));
        }
        match &item.target {
            Target::Issue { issue_id: 0, .. } => {
                return bad(format!("Logtime ngày {day} chưa có issue."));
            }
            Target::Issue { .. } => {}
            Target::NewSubtask { key } => {
                let Some(sub) = plan.subtask(key) else {
                    return bad(format!("Không tìm thấy task con cho logtime ngày {day}."));
                };
                if sub.subject.trim().is_empty() {
                    return bad(format!("Task con của logtime ngày {day} chưa có tên."));
                }
                if sub.parent_id == 0 || sub.project_id == 0 || sub.tracker_id == 0 {
                    return bad(format!(
                        "Task con “{}” thiếu task cha, project hoặc tracker.",
                        sub.subject
                    ));
                }
            }
        }
        *per_day.entry(item.date).or_default() += item.hours;
    }
    for (date, total) in per_day {
        // Only days this plan touches can be pushed over the limit.
        if total > MAX_HOURS_PER_DAY + EPS && plan.items.iter().any(|i| i.date == date) {
            return bad(format!(
                "Ngày {} sẽ có {}h, vượt {}h. Giảm số giờ rồi ghi lại.",
                date.format("%d/%m/%Y"),
                (total * 100.0).round() / 100.0,
                MAX_HOURS_PER_DAY
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ApiCall {
    pub method: &'static str,
    pub path: String,
    pub body: Value,
}

/// The requests a batch would send, for the dry-run view.
pub fn preview(ops: &[OpPayload], plan: &WritePlan, assignee_id: Option<u32>) -> Vec<ApiCall> {
    ops.iter()
        .map(|op| match op {
            OpPayload::CreateSubtask(s) => ApiCall {
                method: "POST",
                path: "/issues.json".into(),
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
                        let subject = plan.subtask(key).map(|s| s.subject.as_str()).unwrap_or(key);
                        json!(format!("<id task con \"{subject}\">"))
                    }
                };
                ApiCall {
                    method: "POST",
                    path: "/time_entries.json".into(),
                    body: json!({"time_entry": {
                        "issue_id": issue_id,
                        "spent_on": i.date,
                        "hours": i.hours,
                        "activity_id": i.activity_id,
                        "comments": i.comments,
                    }}),
                }
            }
            OpPayload::RenameIssue(r) => ApiCall {
                method: "PUT",
                path: format!("/issues/{}.json", r.issue_id),
                body: json!({"issue": {"subject": r.after}}),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{IssueRename, ItemKind, SubtaskDraft};
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
            ..Default::default()
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

    fn check_err(p: &WritePlan, logged: &[(NaiveDate, f64)]) -> String {
        check_plan(p, logged).unwrap_err().to_string()
    }

    #[test]
    fn check_plan_accepts_a_full_day_with_what_redmine_has() {
        // 27/10: 8h planned + 16h already logged = exactly 24h.
        assert!(check_plan(&plan(), &[(d(27), 16.0), (d(29), 30.0)]).is_ok());
    }

    #[test]
    fn check_plan_rejects_a_day_above_24h_counting_redmine() {
        let msg = check_err(&plan(), &[(d(27), 10.0), (d(27), 6.5)]);
        assert!(msg.contains("27/10/2026") && msg.contains("24.5h"), "{msg}");
        // Entries of 13h each, none above 24h on its own.
        let mut p = plan();
        p.items[1].hours = 13.0;
        p.items[2].hours = 13.0;
        assert!(check_err(&p, &[]).contains("vượt 24h"));
    }

    #[test]
    fn check_plan_rejects_bad_hours_targets_and_subtasks() {
        assert!(check_err(&WritePlan::default(), &[]).contains("Chưa chọn"));
        for hours in [0.0, -1.0, 25.0, f64::NAN, f64::INFINITY] {
            let mut p = plan();
            p.items[2].hours = hours;
            assert!(check_err(&p, &[]).contains("số giờ"), "{hours}");
        }
        let mut no_issue = plan();
        no_issue.items[2].target = Target::Issue {
            issue_id: 0,
            label: String::new(),
        };
        assert!(check_err(&no_issue, &[]).contains("chưa có issue"));
        let mut blank = plan();
        blank.subtasks[0].subject = "  ".into();
        assert!(check_err(&blank, &[]).contains("chưa có tên"));
        let mut no_tracker = plan();
        no_tracker.subtasks[0].tracker_id = 0;
        assert!(check_err(&no_tracker, &[]).contains("tracker"));
        let mut twice = plan();
        twice.items[1].id = "2".into();
        assert!(check_err(&twice, &[]).contains("bị lặp"));
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

    fn renames() -> WritePlan {
        WritePlan {
            renames: vec![IssueRename {
                issue_id: 77,
                before: String::new(),
                after: "[FE] Lọc theo vùng".into(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn build_ops_renames_first_and_once() {
        let mut p = plan();
        p.renames = renames().renames;
        let ops = build_ops(&p).unwrap();
        assert!(matches!(&ops[0], OpPayload::RenameIssue(r) if r.issue_id == 77));
        assert!(matches!(&ops[1], OpPayload::CreateSubtask(_)));
        // Renames alone are a plan; the same issue twice or a blank name is not.
        assert_eq!(build_ops(&renames()).unwrap().len(), 1);
        let mut twice = renames();
        twice.renames.push(twice.renames[0].clone());
        assert!(
            build_ops(&twice)
                .unwrap_err()
                .to_string()
                .contains("hai lần")
        );
        let mut blank = renames();
        blank.renames[0].after = "  ".into();
        assert!(build_ops(&blank).is_err());
        assert!(check_err(&blank, &[]).contains("trống"));
        assert!(check_plan(&renames(), &[]).is_ok());
    }

    fn issue(id: u32, subject: &str) -> Issue {
        serde_json::from_value(json!({
            "id": id, "subject": subject, "project": {"id": 3, "name": "ABC"},
            "tracker": {"id": 4, "name": "Task"}, "status": {"id": 1, "name": "New"},
            "author": {"id": 7, "name": "Me"}
        }))
        .unwrap()
    }

    #[test]
    fn refresh_renames_takes_the_name_redmine_holds_now() {
        let mut p = renames();
        refresh_renames(&mut p, &[issue(77, "Lọc vùng (sửa trên web)")]).unwrap();
        assert_eq!(p.renames[0].before, "Lọc vùng (sửa trên web)");
        let err = refresh_renames(&mut renames(), &[issue(78, "x")]).unwrap_err();
        assert!(err.to_string().contains("#77"));
    }

    #[test]
    fn preview_shows_the_rename() {
        let p = renames();
        let calls = preview(&build_ops(&p).unwrap(), &p, None);
        assert_eq!(
            (calls[0].method, calls[0].path.as_str()),
            ("PUT", "/issues/77.json")
        );
        assert_eq!(calls[0].body["issue"]["subject"], "[FE] Lọc theo vùng");
    }
}
