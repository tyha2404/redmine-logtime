//! Menu bar / tray icon: today's hours at a glance, and shortcuts that open
//! the window where the user reviews and confirms. The tray itself never
//! writes to Redmine.

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use logtime_core::Settings;
use redmine_client::TimeEntry;
use tauri::async_runtime::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::commands::{add_issue_subjects, client_for};
use crate::reminder::{fmt_h, shortfall, week_start};
use crate::state::AppState;

const ID: &str = "main";
/// Sent to the window with the action's id; `App.tsx` opens the page.
const EVENT: &str = "tray-action";
const REFRESH_EVERY: Duration = Duration::from_secs(15 * 60);
/// Today's entries listed in the menu; the rest are counted.
const MAX_ENTRIES: usize = 3;
const MAX_LABEL: usize = 42;

/// Shortcuts into the app, handled by the window (see `App.tsx`).
const ACTIONS: [(&str, &str); 4] = [
    ("today", "Ghi logtime hôm nay…"),
    ("quick", "Khớp nhanh hôm nay từ Google Chat"),
    ("leave", "Xin nghỉ…"),
    ("history", "Lịch sử ghi"),
];

/// What the tray shows: the text beside the icon and read-only menu lines.
#[derive(Debug, PartialEq)]
pub struct Status {
    pub title: Option<String>,
    pub lines: Vec<String>,
}

pub fn exists(app: &AppHandle) -> bool {
    app.tray_by_id(ID).is_some()
}

/// Builds the tray, then keeps its hours fresh.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(ID)
        .tooltip("Redmine Logtime")
        .menu(&menu(app, None)?)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => crate::show_main(app),
            "refresh" => refresh_soon(app),
            "quit" => app.exit(0),
            id if ACTIONS.iter().any(|(a, _)| *a == id) => {
                crate::show_main(app);
                let _ = app.emit_to("main", EVENT, id);
            }
            _ => {}
        });
    // macOS: a one-colour template icon that follows the menu bar's light or
    // dark look. Elsewhere the coloured app icon (Ubuntu's top bar is dark).
    #[cfg(target_os = "macos")]
    {
        let glyph = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
        builder = builder.icon(glyph).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&app).await;
            tokio::time::sleep(REFRESH_EVERY).await;
        }
    });
    Ok(())
}

/// Re-reads the hours in the background, e.g. after a write or undo.
pub fn refresh_soon(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { refresh(&app).await });
}

async fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(ID) else {
        return;
    };
    // One at a time, so a slow older read never overwrites a newer one.
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _turn = LOCK.get_or_init(|| Mutex::new(())).lock().await;

    let status = match read(app).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Thanh menu chưa đọc được giờ: {e}");
            e.into_status()
        }
    };
    let title = status.as_ref().and_then(|s| s.title.clone());
    let tooltip = match &title {
        Some(t) => format!("Redmine Logtime · hôm nay {t}"),
        None => "Redmine Logtime".into(),
    };
    let result = menu(app, status.as_ref()).and_then(|m| {
        tray.set_menu(Some(m))?;
        tray.set_title(title)?;
        tray.set_tooltip(Some(tooltip))
    });
    if let Err(e) = result {
        log::warn!("Không cập nhật được thanh menu: {e}");
    }
}

enum ReadError {
    /// No URL or key yet: show no hours at all.
    NotSetUp,
    Redmine(String),
}

impl ReadError {
    fn into_status(self) -> Option<Status> {
        match self {
            Self::NotSetUp => None,
            Self::Redmine(_) => Some(Status {
                title: None,
                lines: vec!["Chưa đọc được giờ từ Redmine".into()],
            }),
        }
    }
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSetUp => f.write_str("chưa cấu hình Redmine"),
            Self::Redmine(e) => f.write_str(e),
        }
    }
}

async fn read(app: &AppHandle) -> Result<Option<Status>, ReadError> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let client =
        client_for(&state.credentials, &settings, None).map_err(|_| ReadError::NotSetUp)?;
    let today = Local::now().date_naive();
    let week = client
        .my_time_entries(week_start(today), today)
        .await
        .map_err(|e| ReadError::Redmine(e.to_string()))?;
    // Subjects only for the lines shown: today's entries.
    let (mut entries, mut earlier): (Vec<TimeEntry>, Vec<TimeEntry>) =
        week.into_iter().partition(|e| e.spent_on == today);
    add_issue_subjects(&client, &mut entries).await;
    entries.append(&mut earlier);
    Ok(Some(status(&settings, today, &entries)))
}

/// The tray's text from this week's entries (leave included).
pub fn status(settings: &Settings, today: NaiveDate, entries: &[TimeEntry]) -> Status {
    let hpd = settings.hours_per_day;
    let logged: Vec<(NaiveDate, f64)> = entries.iter().map(|e| (e.spent_on, e.hours)).collect();
    let (_, week_missing) = shortfall(settings, today, &logged);
    let mine: Vec<&TimeEntry> = entries.iter().filter(|e| e.spent_on == today).collect();
    let today_hours: f64 = mine.iter().map(|e| e.hours).sum();
    let workday = settings.is_workday(today);

    let title = if workday {
        Some(format!(
            "{}/{}",
            fmt_h(today_hours).trim_end_matches('h'),
            fmt_h(hpd)
        ))
    } else {
        (today_hours > 0.0).then(|| fmt_h(today_hours))
    };
    let mut lines = vec![if workday {
        format!("Hôm nay: {} / {}", fmt_h(today_hours), fmt_h(hpd))
    } else {
        format!("Hôm nay: ngày nghỉ, đã log {}", fmt_h(today_hours))
    }];
    lines.push(if week_missing > 1e-6 {
        format!("Tuần này: còn thiếu {}", fmt_h(week_missing))
    } else {
        "Tuần này: đã đủ giờ".into()
    });
    for e in mine.iter().take(MAX_ENTRIES) {
        lines.push(format!("    {} · {}", short(&label(e)), fmt_h(e.hours)));
    }
    if mine.len() > MAX_ENTRIES {
        lines.push(format!("    và {} mục khác", mine.len() - MAX_ENTRIES));
    }
    Status { title, lines }
}

fn label(e: &TimeEntry) -> String {
    match &e.issue {
        Some(i) => i.subject.clone().unwrap_or_else(|| format!("#{}", i.id)),
        None => e.project.name.clone(),
    }
}

fn short(s: &str) -> String {
    if s.chars().count() <= MAX_LABEL {
        return s.to_string();
    }
    let cut: String = s.chars().take(MAX_LABEL - 1).collect();
    format!("{}…", cut.trim_end())
}

fn menu(app: &AppHandle, status: Option<&Status>) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    if let Some(s) = status {
        for line in &s.lines {
            menu.append(&MenuItem::new(app, line, false, None::<&str>)?)?;
        }
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    for (id, text) in ACTIONS {
        menu.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        "Mở Redmine Logtime",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "refresh",
        "Làm mới số giờ",
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Thoát",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use redmine_client::{IdName, IdRef};

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    fn entry(day: u32, hours: f64, subject: Option<&str>) -> TimeEntry {
        let name = |s: &str| IdName {
            id: 1,
            name: s.into(),
        };
        TimeEntry {
            id: 1,
            project: name("Dự án ABC"),
            issue: subject.map(|s| IdRef {
                id: 12,
                subject: (!s.is_empty()).then(|| s.into()),
                parent: None,
            }),
            user: name("Dev A"),
            activity: name("Development"),
            hours,
            comments: None,
            spent_on: d(day),
        }
    }

    #[test]
    fn status_shows_today_the_week_and_todays_entries() {
        let s = Settings::default();
        // Thursday 29/10: Mon 8h, Tue 8h, Wed 5h, today 2h + 1.5h.
        let entries = [
            entry(26, 8.0, None),
            entry(27, 8.0, None),
            entry(28, 5.0, None),
            entry(29, 2.0, Some("Review Module Sự kiện")),
            entry(29, 1.5, Some("")),
        ];
        let st = status(&s, d(29), &entries);
        assert_eq!(st.title.as_deref(), Some("3.5/8h"));
        assert_eq!(
            st.lines,
            [
                "Hôm nay: 3.5h / 8h",
                "Tuần này: còn thiếu 7.5h",
                "    Review Module Sự kiện · 2h",
                "    #12 · 1.5h",
            ]
        );
    }

    #[test]
    fn status_counts_extra_entries_and_full_weeks() {
        let s = Settings::default();
        let mut entries: Vec<_> = (26..29).map(|day| entry(day, 8.0, None)).collect();
        entries.extend((0..4).map(|_| entry(29, 2.0, None)));
        let st = status(&s, d(29), &entries);
        assert_eq!(st.lines[1], "Tuần này: đã đủ giờ");
        assert_eq!(st.lines[2], "    Dự án ABC · 2h");
        assert_eq!(st.lines.last().unwrap(), "    và 1 mục khác");
    }

    #[test]
    fn days_off_show_hours_only_when_logged() {
        let s = Settings::default();
        // Saturday 31/10.
        assert_eq!(status(&s, d(31), &[]).title, None);
        assert_eq!(
            status(&s, d(31), &[entry(31, 2.0, None)]).title.as_deref(),
            Some("2h")
        );
    }

    #[test]
    fn long_labels_are_cut() {
        let long = "a".repeat(60);
        assert_eq!(short(&long).chars().count(), MAX_LABEL);
        assert!(short(&long).ends_with('…'));
        assert_eq!(short("ngắn"), "ngắn");
    }
}
