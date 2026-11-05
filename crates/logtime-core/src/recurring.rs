//! Recurring work (stand-up, weekly meeting): fixed hours on fixed weekdays,
//! proposed for each reported day before the report's hours are split.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::Settings;

const EPS: f64 = 1e-6;

/// One recurring task due on one day.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FixedEntry {
    /// `RecurringTask::id`.
    pub id: String,
    pub date: NaiveDate,
    pub issue_id: u32,
    pub label: String,
    pub hours: f64,
    pub activity_id: Option<u32>,
    pub comments: String,
}

/// The recurring work due on each of `days` (date, free hours): every task
/// on its weekdays, on workdays only, unless the day already has time on
/// its issue (`logged`: date, issue id). A day takes them in list order as
/// long as they fit in its free hours.
pub fn due(
    settings: &Settings,
    days: &[(NaiveDate, f64)],
    logged: &[(NaiveDate, u32)],
) -> Vec<FixedEntry> {
    let mut out = Vec::new();
    for &(date, free) in days {
        if !settings.is_workday(date) {
            continue;
        }
        let mut left = free;
        for task in &settings.recurring {
            if !task.on(date)
                || task.issue_id == 0
                || task.hours <= EPS
                || logged.contains(&(date, task.issue_id))
                || task.hours > left + EPS
            {
                continue;
            }
            left -= task.hours;
            out.push(FixedEntry {
                id: task.id.clone(),
                date,
                issue_id: task.issue_id,
                label: task.label.clone(),
                hours: task.hours,
                activity_id: task.activity_id,
                comments: task.comments.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecurringTask;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn task(id: &str, issue: u32, hours: f64, weekdays: &[u8]) -> RecurringTask {
        RecurringTask {
            id: id.into(),
            issue_id: issue,
            label: format!("#{issue}"),
            hours,
            weekdays: weekdays.to_vec(),
            ..Default::default()
        }
    }

    fn settings(tasks: Vec<RecurringTask>) -> Settings {
        Settings {
            recurring: tasks,
            holidays: vec![d(28)],
            ..Settings::default()
        }
    }

    fn ids(found: &[FixedEntry], day: u32) -> Vec<&str> {
        found
            .iter()
            .filter(|f| f.date == d(day))
            .map(|f| f.id.as_str())
            .collect()
    }

    #[test]
    fn due_on_its_weekdays_and_workdays_only() {
        // Mon 26 … Sun 1/11; Wed 28 is a holiday.
        let s = settings(vec![
            task("daily", 10, 0.25, &[]),
            task("sprint", 11, 1.0, &[1]),
        ]);
        let days: Vec<_> = (26..=31).map(|x| (d(x), 8.0)).collect();
        let found = due(&s, &days, &[]);
        assert_eq!(ids(&found, 26), vec!["daily", "sprint"]);
        assert_eq!(ids(&found, 27), vec!["daily"]);
        assert!(ids(&found, 28).is_empty(), "holiday");
        assert!(ids(&found, 31).is_empty(), "Saturday");
        assert_eq!(found[0].hours, 0.25);
    }

    #[test]
    fn skips_paused_logged_and_what_does_not_fit() {
        let mut paused = task("paused", 12, 1.0, &[]);
        paused.paused = true;
        let s = settings(vec![
            task("big", 10, 2.0, &[]),
            task("small", 11, 0.5, &[]),
            paused,
        ]);
        // 26: already logged on #10; 27: only 1h free, so "big" does not fit.
        let found = due(
            &s,
            &[(d(26), 8.0), (d(27), 1.0), (d(29), 0.0)],
            &[(d(26), 10)],
        );
        assert_eq!(ids(&found, 26), vec!["small"]);
        assert_eq!(ids(&found, 27), vec!["small"]);
        assert!(ids(&found, 29).is_empty());
    }
}
