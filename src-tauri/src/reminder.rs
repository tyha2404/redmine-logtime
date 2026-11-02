//! End-of-day reminder: on workdays at the chosen time, a system
//! notification when today (or this week so far) is short of hours. The
//! app keeps running in the menu bar / tray for it; see `lib.rs`.

use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use logtime_core::Settings;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::commands::client_for;
use crate::state::AppState;

const EPS: f64 = 1e-6;

/// "17:30" → 17:30; anything else → None (reminder off).
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// Whether to check now: reminders on, a workday, past the time, and not
/// yet done today.
pub fn due(settings: &Settings, now: NaiveDateTime, last: Option<NaiveDate>) -> bool {
    let Some(at) = parse_time(&settings.reminder_time) else {
        return false;
    };
    let today = now.date();
    settings.reminder_enabled
        && settings.is_workday(today)
        && now.time() >= at
        && last != Some(today)
}

/// Monday of `date`'s week.
pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - chrono::Duration::days(date.weekday().num_days_from_monday() as i64)
}

/// Hours still missing, from (date, hours) pairs logged on Redmine (leave
/// included): today's, and the week's workdays up to today.
pub fn shortfall(settings: &Settings, today: NaiveDate, logged: &[(NaiveDate, f64)]) -> (f64, f64) {
    let hpd = settings.hours_per_day;
    let on = |d: NaiveDate| logged.iter().filter(|(x, _)| *x == d).map(|(_, h)| h).sum::<f64>();
    let today_missing = if settings.is_workday(today) {
        (hpd - on(today)).max(0.0)
    } else {
        0.0
    };
    let week_missing = week_start(today)
        .iter_days()
        .take_while(|d| *d <= today)
        .filter(|d| settings.is_workday(*d))
        .map(|d| (hpd - on(d)).max(0.0))
        .sum();
    (today_missing, week_missing)
}

pub fn fmt_h(h: f64) -> String {
    let s = format!("{h:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string() + "h"
}

/// Title and body of the notification, or None when nothing is missing.
/// `always` gives an answer even then (the tray's "check now").
pub fn message(settings: &Settings, today_missing: f64, week_missing: f64, always: bool) -> Option<(String, String)> {
    let hpd = settings.hours_per_day;
    if today_missing < EPS && week_missing < EPS {
        return always.then(|| ("Đã log đủ giờ".into(), format!("Hôm nay và cả tuần đều đủ {}.", fmt_h(hpd))));
    }
    let mut body = if today_missing > EPS {
        format!("Hôm nay mới log {} trên {}.", fmt_h(hpd - today_missing), fmt_h(hpd))
    } else {
        "Hôm nay đã đủ giờ.".to_string()
    };
    if week_missing > today_missing + EPS {
        body += &format!(" Tuần này còn thiếu {}.", fmt_h(week_missing));
    }
    body += " Mở app để ghi.";
    Some(("Chưa log đủ giờ".into(), body))
}

/// Reads this week's hours from Redmine and notifies when short.
pub async fn check(app: &AppHandle, always: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let today = Local::now().date_naive();
    let client = client_for(&state.credentials, &settings, None)?;
    let logged: Vec<(NaiveDate, f64)> = client
        .my_time_entries(week_start(today), today)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|e| (e.spent_on, e.hours))
        .collect();
    let (day, week) = shortfall(&settings, today, &logged);
    log::info!("Nhắc giờ: hôm nay thiếu {day}h, tuần thiếu {week}h");
    if let Some((title, body)) = message(&settings, day, week, always) {
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Checks once a minute whether today's reminder is due.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<NaiveDate> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let settings = app.state::<AppState>().settings();
            let now = Local::now().naive_local();
            if !due(&settings, now, last) {
                continue;
            }
            match check(&app, false).await {
                Ok(()) => last = Some(now.date()),
                // Offline or no key yet: try again next minute, quietly.
                Err(e) => log::warn!("Nhắc giờ chưa kiểm tra được: {e}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn at(day: u32, h: u32, m: u32) -> NaiveDateTime {
        d(day).and_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn due_on_workdays_after_the_time_once_a_day() {
        let s = Settings::default(); // 17:30, Monday–Friday
        assert!(!due(&s, at(29, 17, 29), None));
        assert!(due(&s, at(29, 17, 30), None));
        assert!(!due(&s, at(29, 18, 0), Some(d(29))));
        assert!(due(&s, at(30, 9, 0).with_hour_min(17, 45), Some(d(29))));
        // Saturday 31/10 and a holiday are skipped.
        assert!(!due(&s, at(31, 18, 0), None));
        let holiday = Settings { holidays: vec![d(29)], ..Settings::default() };
        assert!(!due(&holiday, at(29, 18, 0), None));
        let off = Settings { reminder_enabled: false, ..Settings::default() };
        assert!(!due(&off, at(29, 18, 0), None));
        let bad = Settings { reminder_time: "5pm".into(), ..Settings::default() };
        assert!(!due(&bad, at(29, 18, 0), None));
    }

    #[test]
    fn shortfall_counts_today_and_the_week_so_far() {
        let s = Settings::default();
        // Week of Mon 26/10: Mon 8h, Tue 4h + 4h leave, Wed 5h, Thu 3h (today).
        let logged = [(d(26), 8.0), (d(27), 4.0), (d(27), 4.0), (d(28), 5.0), (d(29), 3.0)];
        assert_eq!(shortfall(&s, d(29), &logged), (5.0, 8.0));
        assert_eq!(week_start(d(29)), d(26));
        // Saturday: nothing is due today; the week still counts Mon–Fri.
        assert_eq!(shortfall(&s, d(31), &logged).0, 0.0);
    }

    #[test]
    fn message_says_what_is_missing() {
        let s = Settings::default();
        let (title, body) = message(&s, 5.0, 8.0, false).unwrap();
        assert_eq!(title, "Chưa log đủ giờ");
        assert_eq!(body, "Hôm nay mới log 3h trên 8h. Tuần này còn thiếu 8h. Mở app để ghi.");
        let (_, body) = message(&s, 0.0, 2.5, false).unwrap();
        assert!(body.starts_with("Hôm nay đã đủ giờ. Tuần này còn thiếu 2.5h."));
        assert!(message(&s, 0.0, 0.0, false).is_none());
        assert_eq!(message(&s, 0.0, 0.0, true).unwrap().0, "Đã log đủ giờ");
    }

    trait WithHourMin {
        fn with_hour_min(self, h: u32, m: u32) -> NaiveDateTime;
    }
    impl WithHourMin for NaiveDateTime {
        fn with_hour_min(self, h: u32, m: u32) -> NaiveDateTime {
            self.date().and_hms_opt(h, m, 0).unwrap()
        }
    }
}
