//! Work report (weekly + daily) → tasks with parent issues and per-day
//! weights. Hours are NOT decided by the model; see `logtime_core::planner`.

use chrono::NaiveDate;
use std::collections::BTreeMap;

use logtime_core::planner::{self, Allocation, Demand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DayWeight {
    pub date: NaiveDate,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportTask {
    /// Same work on several days shares one key → one sub-task.
    pub key: String,
    pub text: String,
    #[serde(default)]
    pub project_hint: String,
    pub parent_issue_id: Option<u32>,
    #[serde(default)]
    pub parent_confidence: f64,
    #[serde(default)]
    pub parent_reason: String,
    /// The developer's own sub-task under that parent, when one already exists.
    pub existing_subtask_id: Option<u32>,
    #[serde(default)]
    pub subtask_subject: String,
    #[serde(default)]
    pub comment: String,
    pub days: Vec<DayWeight>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LeaveDay {
    pub date: NaiveDate,
    pub hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportAnalysis {
    pub period_from: NaiveDate,
    pub period_to: NaiveDate,
    #[serde(default)]
    pub leave: Vec<LeaveDay>,
    #[serde(default)]
    pub questions: Vec<String>,
    pub tasks: Vec<ReportTask>,
}

impl ReportAnalysis {
    /// Planner input for the tasks that are not excluded.
    pub fn demands(&self, excluded: &[String]) -> Vec<Demand> {
        self.tasks
            .iter()
            .filter(|t| !excluded.contains(&t.key))
            .flat_map(|t| {
                t.days.iter().map(|d| Demand {
                    task: t.key.clone(),
                    date: d.date,
                    weight: d.weight,
                })
            })
            .collect()
    }
}

/// Free hours of one day in the report period, after what is already logged
/// and any leave.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DayCapacity {
    pub date: NaiveDate,
    pub workday: bool,
    pub existing_hours: f64,
    /// Hours already waiting in the review queue for this day.
    #[serde(default)]
    pub queued_hours: f64,
    pub leave_hours: f64,
    pub capacity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DayPlan {
    pub date: NaiveDate,
    pub capacity: f64,
    pub existing_hours: f64,
    pub queued_hours: f64,
    pub leave_hours: f64,
    pub allocations: Vec<Allocation>,
    pub warnings: Vec<String>,
}

/// Splits each day's free hours across the report tasks done that day.
pub fn plan(analysis: &ReportAnalysis, days: &[DayCapacity], excluded: &[String]) -> Vec<DayPlan> {
    let capacity: BTreeMap<NaiveDate, f64> = days
        .iter()
        .filter(|d| d.workday)
        .map(|d| (d.date, d.capacity))
        .collect();
    let demands = analysis.demands(excluded);
    let allocations = planner::allocate(&capacity, &demands, 0.5);

    days.iter()
        .map(|d| {
            let mine: Vec<Allocation> = allocations
                .iter()
                .filter(|a| a.date == d.date)
                .cloned()
                .collect();
            let used: f64 = mine.iter().map(|a| a.hours).sum();
            let wanted = demands.iter().any(|x| x.date == d.date);
            let mut warnings = Vec::new();
            if !d.workday && wanted {
                warnings.push("Báo cáo có việc vào ngày nghỉ; đã bỏ qua.".to_string());
            } else if d.workday && d.capacity > 0.0 && !wanted {
                warnings.push(format!(
                    "Không có việc nào trong báo cáo cho ngày này, còn trống {}h.",
                    fmt_h(d.capacity)
                ));
            } else if d.workday && d.capacity - used >= 0.25 && wanted {
                warnings.push(format!(
                    "Còn trống {}h chưa phân bổ.",
                    fmt_h(d.capacity - used)
                ));
            }
            DayPlan {
                date: d.date,
                capacity: d.capacity,
                existing_hours: d.existing_hours,
                queued_hours: d.queued_hours,
                leave_hours: d.leave_hours,
                allocations: mine,
                warnings,
            }
        })
        .collect()
}

fn fmt_h(h: f64) -> String {
    let s = format!("{h:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub struct ReportContext<'a> {
    pub today: NaiveDate,
    pub hours_per_day: f64,
    /// ISO weekday numbers, Monday = 1.
    pub workdays: &'a [u8],
    /// Markdown from `profile::to_instructions`, if discovery has run.
    pub profile: Option<&'a str>,
    /// Set when the user reports one specific calendar day.
    pub day: Option<NaiveDate>,
}

pub fn schema() -> Value {
    let date = json!({"type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$"});
    json!({
        "type": "object",
        "required": ["period_from", "period_to", "leave", "questions", "tasks"],
        "properties": {
            "period_from": date, "period_to": date,
            "leave": {"type": "array", "items": {
                "type": "object", "required": ["date", "hours"],
                "properties": {"date": date, "hours": {"type": "number"}}
            }},
            "questions": {"type": "array", "items": {"type": "string"}},
            "tasks": {"type": "array", "items": {
                "type": "object",
                "required": ["key", "text", "project_hint", "parent_issue_id", "parent_confidence",
                             "parent_reason", "existing_subtask_id", "subtask_subject", "comment", "days"],
                "properties": {
                    "key": {"type": "string"},
                    "text": {"type": "string"},
                    "project_hint": {"type": "string"},
                    "parent_issue_id": {"type": ["integer", "null"]},
                    "parent_confidence": {"type": "number", "minimum": 0, "maximum": 1},
                    "parent_reason": {"type": "string"},
                    "existing_subtask_id": {"type": ["integer", "null"]},
                    "subtask_subject": {"type": "string"},
                    "comment": {"type": "string"},
                    "days": {"type": "array", "items": {
                        "type": "object", "required": ["date", "weight"],
                        "properties": {"date": date, "weight": {"type": "number", "minimum": 0}}
                    }}
                }
            }}
        }
    })
}

pub fn prompt(report: &str, ctx: &ReportContext) -> String {
    let profile = ctx.profile.unwrap_or(
        "(Chưa phân tích cấu trúc Redmine. Hãy tự tìm hiểu nhanh qua tracker và người tạo issue.)",
    );
    format!(
        r#"You turn a Vietnamese developer's work report into Redmine time-logging data, using the `redmine` MCP tools (read-only) to find the right issues.

Today is {today} ({weekday}). Working days (ISO, Monday=1): {workdays:?}. A full day is {hpd}h.

## How this Redmine is organised
{profile}

## Report format
- A WEEKLY report covers the whole week (e.g. "báo cáo công việc tuần này 26/10/2026" → Monday 26/10 to Friday 30/10).
- DAILY reports cover one date each (headers like "Thứ 2 26/10", "Ngày 27/10", "T3:").
- Lines under a project header (e.g. "1. Davipharm - Salex") belong to that project.
- Each report line is its own task. The same work appearing on several days is ONE task (same `key`).

## What to produce
- period_from / period_to: the dates the report covers.
- leave: days the report says the developer was off ("nghỉ phép", "nghỉ ốm", "off", "nghỉ nửa ngày" = {half}h).
- tasks: one per distinct line of work:
  - days: the working dates it was done on. A line in a DAILY report → that date only. Dates with no daily report → the WEEKLY lines. Skip leave-only days. `weight` = relative effort on that date (1 = normal; bigger = more of the day). Do not compute hours.
  - parent_issue_id: the BA parent task this work belongs to. Search with GET /issues.json (project_id, tracker_id, status_id=open or *, subject=~keyword, limit=100) and follow the rules above. Prefer open parents in the hinted project. null if nothing fits — never invent ids.
  - parent_confidence 0–1 and parent_reason (Vietnamese, one sentence).
  - existing_subtask_id: if the parent already has a child assigned to the current user (GET /issues.json?parent_id=…&status_id=*) that matches this work, its id; else null.
  - subtask_subject: Vietnamese sub-task title following the naming conventions, from the report line.
  - comment: short Vietnamese time-entry comment.
- questions: Vietnamese questions for anything ambiguous (unclear dates, unknown project). Empty if none.

{day_rule}Keep it under ~40 Redmine requests. The report follows between the markers.

<report>
{report}
</report>"#,
        today = ctx.today,
        weekday = chrono::Datelike::weekday(&ctx.today),
        workdays = ctx.workdays,
        hpd = ctx.hours_per_day,
        half = ctx.hours_per_day / 2.0,
        profile = profile,
        report = report.trim(),
        day_rule = ctx
            .day
            .map(|d| format!(
                "## This is a report for ONE day\nEvery line was done on {d} ({wd}). Set period_from = period_to = {d} and give every task exactly that one day. Ignore other dates mentioned.\n\n",
                wd = chrono::Datelike::weekday(&d)
            ))
            .unwrap_or_default(),
    )
}

impl ReportAnalysis {
    /// Pins a single-day report to `day`, whatever dates the model chose.
    pub fn pin_to_day(&mut self, day: NaiveDate) {
        self.period_from = day;
        self.period_to = day;
        self.leave.retain(|l| l.date == day);
        for t in &mut self.tasks {
            let weight = t.days.iter().map(|d| d.weight).fold(0.0, f64::max);
            t.days = vec![DayWeight {
                date: day,
                weight: if weight > 0.0 { weight } else { 1.0 },
            }];
        }
    }
}

/// Work that already exists for a date: logged on Redmine or waiting in the
/// review queue.
#[derive(Debug, Clone, PartialEq)]
pub struct KnownWork {
    pub date: NaiveDate,
    /// Issue the hours are (or will be) logged on.
    pub issue_id: Option<u32>,
    /// For a queued new sub-task: its parent and title.
    pub parent_id: Option<u32>,
    pub text: String,
    pub hours: f64,
    pub queued: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Duplicate {
    pub date: NaiveDate,
    pub reason: String,
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drops the days on which a task's work is already logged or queued, so it
/// is not reported twice. Returns, per task key, the days that were dropped.
pub fn remove_duplicates(
    analysis: &mut ReportAnalysis,
    known: &[KnownWork],
) -> BTreeMap<String, Vec<Duplicate>> {
    let mut out: BTreeMap<String, Vec<Duplicate>> = BTreeMap::new();
    for t in &mut analysis.tasks {
        let names = [norm(&t.text), norm(&t.subtask_subject), norm(&t.comment)];
        let same = |w: &KnownWork| {
            let by_issue = t.existing_subtask_id.is_some() && w.issue_id == t.existing_subtask_id;
            let by_parent = w.parent_id.is_some()
                && w.parent_id == t.parent_issue_id
                && names.contains(&norm(&w.text));
            let by_text = !w.text.trim().is_empty() && names.contains(&norm(&w.text));
            by_issue || by_parent || by_text
        };
        t.days.retain(|d| {
            let Some(w) = known.iter().find(|w| w.date == d.date && same(w)) else {
                return true;
            };
            let target = w.issue_id.map(|i| format!(" vào #{i}")).unwrap_or_default();
            let reason = if w.queued {
                format!("Đã có trong hàng chờ{target} ({}h).", fmt_h(w.hours))
            } else {
                format!("Đã log {}h{target} trên Redmine.", fmt_h(w.hours))
            };
            out.entry(t.key.clone()).or_default().push(Duplicate {
                date: d.date,
                reason,
            });
            false
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn demands_skip_excluded_tasks() {
        let task = |key: &str, days: &[u32]| ReportTask {
            key: key.into(),
            text: key.into(),
            project_hint: String::new(),
            parent_issue_id: Some(1),
            parent_confidence: 0.9,
            parent_reason: String::new(),
            existing_subtask_id: None,
            subtask_subject: key.into(),
            comment: String::new(),
            days: days
                .iter()
                .map(|&x| DayWeight {
                    date: d(x),
                    weight: 1.0,
                })
                .collect(),
        };
        let a = ReportAnalysis {
            period_from: d(26),
            period_to: d(30),
            leave: vec![],
            questions: vec![],
            tasks: vec![task("a", &[26, 27]), task("b", &[27])],
        };
        assert_eq!(a.demands(&[]).len(), 3);
        let only_a = a.demands(&["b".into()]);
        assert!(only_a.iter().all(|x| x.task == "a"));
    }

    #[test]
    fn plan_fills_free_hours_and_warns() {
        let task = |key: &str, days: &[(u32, f64)]| ReportTask {
            key: key.into(),
            text: key.into(),
            project_hint: String::new(),
            parent_issue_id: Some(1),
            parent_confidence: 0.9,
            parent_reason: String::new(),
            existing_subtask_id: None,
            subtask_subject: key.into(),
            comment: String::new(),
            days: days
                .iter()
                .map(|&(x, w)| DayWeight {
                    date: d(x),
                    weight: w,
                })
                .collect(),
        };
        let a = ReportAnalysis {
            period_from: d(26),
            period_to: d(28),
            leave: vec![],
            questions: vec![],
            tasks: vec![task("a", &[(26, 3.0), (27, 1.0)]), task("b", &[(26, 1.0)])],
        };
        let cap = |x: u32, existing: f64| DayCapacity {
            date: d(x),
            workday: true,
            existing_hours: existing,
            queued_hours: 0.0,
            leave_hours: 0.0,
            capacity: 8.0 - existing,
        };
        let days = vec![cap(26, 0.0), cap(27, 3.0), cap(28, 0.0)];
        let p = plan(&a, &days, &[]);

        let hours = |i: usize| {
            p[i].allocations
                .iter()
                .map(|x| (x.task.as_str(), x.hours))
                .collect::<Vec<_>>()
        };
        assert_eq!(hours(0), vec![("a", 6.0), ("b", 2.0)]);
        assert_eq!(hours(1), vec![("a", 5.0)]);
        assert!(p[2].warnings[0].contains("còn trống 8h"));

        let without_b = plan(&a, &days, &["b".into()]);
        assert_eq!(without_b[0].allocations.len(), 1);
        assert_eq!(without_b[0].allocations[0].hours, 8.0);
    }

    fn task(key: &str, text: &str, subtask: Option<u32>, days: &[u32]) -> ReportTask {
        ReportTask {
            key: key.into(),
            text: text.into(),
            project_hint: String::new(),
            parent_issue_id: Some(1),
            parent_confidence: 0.9,
            parent_reason: String::new(),
            existing_subtask_id: subtask,
            subtask_subject: text.into(),
            comment: text.into(),
            days: days
                .iter()
                .map(|&x| DayWeight {
                    date: d(x),
                    weight: 1.0,
                })
                .collect(),
        }
    }

    fn known(
        day: u32,
        issue: Option<u32>,
        parent: Option<u32>,
        text: &str,
        queued: bool,
    ) -> KnownWork {
        KnownWork {
            date: d(day),
            issue_id: issue,
            parent_id: parent,
            text: text.into(),
            hours: 4.0,
            queued,
        }
    }

    #[test]
    fn removes_days_already_logged_or_queued() {
        let mut a = ReportAnalysis {
            period_from: d(26),
            period_to: d(28),
            leave: vec![],
            questions: vec![],
            tasks: vec![
                task("a", "Sửa feedback công nợ", Some(4), &[26, 27]),
                task("b", "Review Module Sự kiện", None, &[27, 28]),
                task("c", "Họp sprint", None, &[28]),
            ],
        };
        let dups = remove_duplicates(
            &mut a,
            &[
                known(26, Some(4), None, "khác", false), // same sub-task, logged
                known(28, None, Some(1), "review module sự kiện!", true), // same new sub-task, queued
                known(28, Some(9), None, "Họp sprint", false),            // same comment, logged
                known(27, Some(99), None, "việc khác", false),            // unrelated
            ],
        );
        assert_eq!(
            a.tasks[0].days.iter().map(|x| x.date).collect::<Vec<_>>(),
            vec![d(27)]
        );
        assert_eq!(
            a.tasks[1].days.iter().map(|x| x.date).collect::<Vec<_>>(),
            vec![d(27)]
        );
        assert!(a.tasks[2].days.is_empty());
        assert!(dups["a"][0].reason.contains("Đã log 4h vào #4"));
        assert!(dups["b"][0].reason.contains("hàng chờ"));
        assert_eq!(dups.len(), 3);
    }

    #[test]
    fn pin_to_day_forces_single_date() {
        let mut a = ReportAnalysis {
            period_from: d(26),
            period_to: d(30),
            leave: vec![
                LeaveDay {
                    date: d(27),
                    hours: 8.0,
                },
                LeaveDay {
                    date: d(29),
                    hours: 4.0,
                },
            ],
            questions: vec![],
            tasks: vec![task("a", "x", None, &[26, 27])],
        };
        a.pin_to_day(d(29));
        assert_eq!((a.period_from, a.period_to), (d(29), d(29)));
        assert_eq!(a.leave.len(), 1);
        assert_eq!(
            a.tasks[0].days,
            vec![DayWeight {
                date: d(29),
                weight: 1.0
            }]
        );
    }

    #[test]
    fn prompt_embeds_report_and_profile() {
        let p = prompt(
            "1. Davipharm - Salex\nSửa feedback công nợ",
            &ReportContext {
                today: d(28),
                hours_per_day: 8.0,
                workdays: &[1, 2, 3, 4, 5],
                profile: Some("Task cha là Feature."),
                day: None,
            },
        );
        assert!(p.contains("Today is 2026-10-28 (Wed)"));
        assert!(p.contains("Task cha là Feature."));
        assert!(p.contains("<report>\n1. Davipharm - Salex\nSửa feedback công nợ\n</report>"));
        assert!(!p.contains("ONE day"));
    }
}
