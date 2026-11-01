use std::collections::BTreeMap;

use chrono::{Datelike, Days, NaiveDate};
use redmine_client::TimeEntry;
use serde::Serialize;

use crate::Settings;

const EPS: f64 = 1e-6;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DayStatus {
    /// Non-working weekday per settings.
    Weekend,
    Holiday,
    /// Leave logged for the whole day.
    Leave,
    Full,
    Over,
    Partial,
    /// Past (or today) working day with nothing logged.
    Missing,
    /// Future working day with nothing logged yet.
    Future,
}

#[derive(Debug, Clone, Serialize)]
pub struct DaySummary {
    pub date: NaiveDate,
    pub status: DayStatus,
    pub hours: f64,
    pub leave_hours: f64,
    pub entries: Vec<TimeEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthSummary {
    pub year: i32,
    pub month: u32,
    pub days: Vec<DaySummary>,
    pub total_hours: f64,
    /// Working days × hours per day for the whole month.
    pub expected_hours: f64,
    /// Hours still missing on working days up to and including `today`.
    pub missing_hours: f64,
    pub missing_days: u32,
}

/// First and last day of a month, or `None` for an invalid month.
pub fn month_range(year: i32, month: u32) -> Option<(NaiveDate, NaiveDate)> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    Some((first, next - Days::new(1)))
}

pub fn summarize_month(
    year: i32,
    month: u32,
    entries: Vec<TimeEntry>,
    settings: &Settings,
    today: NaiveDate,
) -> Option<MonthSummary> {
    let (first, last) = month_range(year, month)?;
    let hpd = settings.hours_per_day;

    let mut by_day: BTreeMap<NaiveDate, Vec<TimeEntry>> = BTreeMap::new();
    for e in entries {
        if e.spent_on >= first && e.spent_on <= last {
            by_day.entry(e.spent_on).or_default().push(e);
        }
    }

    let mut days = Vec::new();
    let (mut total, mut expected, mut missing_hours, mut missing_days) = (0.0, 0.0, 0.0, 0u32);

    for date in first.iter_days().take_while(|d| *d <= last) {
        let entries = by_day.remove(&date).unwrap_or_default();
        let hours: f64 = entries.iter().map(|e| e.hours).sum();
        let leave_hours: f64 = entries
            .iter()
            .filter(|e| is_leave(e, settings))
            .map(|e| e.hours)
            .sum();
        total += hours;

        let status = if settings.is_holiday(date) {
            DayStatus::Holiday
        } else if !settings.is_workday(date) {
            DayStatus::Weekend
        } else {
            expected += hpd;
            let status = classify(hours, leave_hours, hpd, date > today);
            if date <= today && hours + EPS < hpd {
                missing_hours += hpd - hours;
                missing_days += 1;
            }
            status
        };

        days.push(DaySummary {
            date,
            status,
            hours,
            leave_hours,
            entries,
        });
    }

    Some(MonthSummary {
        year: first.year(),
        month: first.month(),
        days,
        total_hours: total,
        expected_hours: expected,
        missing_hours,
        missing_days,
    })
}

fn is_leave(e: &TimeEntry, settings: &Settings) -> bool {
    settings.is_leave_project(e.project.id)
}

fn classify(hours: f64, leave_hours: f64, hpd: f64, future: bool) -> DayStatus {
    if leave_hours + EPS >= hpd {
        DayStatus::Leave
    } else if hours > hpd + EPS {
        DayStatus::Over
    } else if hours + EPS >= hpd {
        DayStatus::Full
    } else if hours > EPS {
        DayStatus::Partial
    } else if future {
        DayStatus::Future
    } else {
        DayStatus::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redmine_client::{IdName, IdRef};

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn entry(day: u32, issue: u32, hours: f64) -> TimeEntry {
        TimeEntry {
            id: day * 100 + issue,
            // Issue 999 lives in the leave project.
            project: IdName {
                id: if issue == 999 { 108 } else { 1 },
                name: "P".into(),
            },
            issue: Some(IdRef { id: issue, subject: None, parent: None }),
            user: IdName {
                id: 1,
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

    fn settings() -> Settings {
        Settings {
            holidays: vec![d(2)],
            leave_types: vec![crate::LeaveType {
                key: "annual".into(),
                project_id: 108,
                ..Default::default()
            }],
            ..Settings::default()
        }
    }

    fn status_of(m: &MonthSummary, day: u32) -> DayStatus {
        m.days[(day - 1) as usize].status
    }

    #[test]
    fn month_range_handles_december() {
        let (a, b) = month_range(2026, 12).unwrap();
        assert_eq!((a.day(), b.day(), b.month()), (1, 31, 12));
        assert!(month_range(2026, 13).is_none());
    }

    #[test]
    fn classifies_days() {
        // 2026-10-26 is a Monday; today = Wed 28.
        let entries = vec![
            entry(26, 1, 5.0),
            entry(26, 2, 3.0),   // Mon: full
            entry(27, 999, 8.0), // Tue: leave
            entry(28, 1, 4.0),   // Wed (today): partial
            entry(23, 1, 9.0),   // Fri: over
            entry(25, 1, 2.0),   // Sun: weekend with hours
        ];
        let m = summarize_month(2026, 10, entries, &settings(), d(28)).unwrap();

        assert_eq!(m.days.len(), 31);
        assert_eq!(status_of(&m, 26), DayStatus::Full);
        assert_eq!(status_of(&m, 27), DayStatus::Leave);
        assert_eq!(status_of(&m, 28), DayStatus::Partial);
        assert_eq!(status_of(&m, 23), DayStatus::Over);
        assert_eq!(status_of(&m, 25), DayStatus::Weekend);
        assert_eq!(status_of(&m, 2), DayStatus::Holiday);
        assert_eq!(status_of(&m, 22), DayStatus::Missing);
        assert_eq!(status_of(&m, 29), DayStatus::Future);
        assert_eq!(m.days[24].hours, 2.0);
    }

    #[test]
    fn totals() {
        let entries = vec![entry(26, 1, 8.0), entry(28, 1, 4.0)];
        let m = summarize_month(2026, 10, entries, &settings(), d(28)).unwrap();
        // October 2026: 22 weekdays, minus holiday on the 2nd = 21.
        assert_eq!(m.expected_hours, 21.0 * 8.0);
        assert_eq!(m.total_hours, 12.0);
        // Workdays 1..=28 excluding holiday: 19; one full → 18 short days.
        assert_eq!(m.missing_days, 18);
        assert_eq!(m.missing_hours, 17.0 * 8.0 + 4.0);
    }
}
