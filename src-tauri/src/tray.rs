//! Menu bar / tray icon: today's hours at a glance, and shortcuts that open
//! the window where the user reviews and confirms. The tray itself never
//! writes to Redmine.

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use logtime_core::Settings;
use tauri::async_runtime::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::commands::client_for;
use crate::reminder::{fmt_h, shortfall, week_start};
use crate::state::AppState;

const ID: &str = "main";
/// Sent to the window with the action's id; `App.tsx` opens the page.
const EVENT: &str = "tray-action";
/// No manual refresh in the menu, so re-read often (one small request).
const REFRESH_EVERY: Duration = Duration::from_secs(5 * 60);

/// Shortcuts into the app, handled by the window (see `App.tsx`).
const ACTIONS: [(&str, &str); 2] = [
    ("today", "Ghi logtime hôm nay…"),
    ("quick", "Khớp nhanh từ Google Chat"),
];

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
    let tooltip = match &status {
        Some(s) => format!("Redmine Logtime · {s}"),
        None => "Redmine Logtime".into(),
    };
    let result = menu(app, status.as_deref()).and_then(|m| {
        tray.set_menu(Some(m))?;
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
    fn into_status(self) -> Option<String> {
        match self {
            Self::NotSetUp => None,
            Self::Redmine(_) => Some("Chưa đọc được giờ từ Redmine".into()),
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

async fn read(app: &AppHandle) -> Result<Option<String>, ReadError> {
    let state = app.state::<AppState>();
    let settings = state.settings();
    let client =
        client_for(&state.credentials, &settings, None).map_err(|_| ReadError::NotSetUp)?;
    let today = Local::now().date_naive();
    let logged: Vec<(NaiveDate, f64)> = client
        .my_time_entries(week_start(today), today)
        .await
        .map_err(|e| ReadError::Redmine(e.to_string()))?
        .into_iter()
        .map(|e| (e.spent_on, e.hours))
        .collect();
    Ok(Some(status(&settings, today, &logged)))
}

/// The menu's status line from this week's (date, hours) on Redmine, leave included.
pub fn status(settings: &Settings, today: NaiveDate, logged: &[(NaiveDate, f64)]) -> String {
    let hpd = settings.hours_per_day;
    let (today_missing, week_missing) = shortfall(settings, today, logged);
    let today_hours: f64 = logged
        .iter()
        .filter(|(d, _)| *d == today)
        .map(|(_, h)| h)
        .sum();
    let mut line = if settings.is_workday(today) {
        format!("Hôm nay {} / {}", fmt_h(today_hours), fmt_h(hpd))
    } else if today_hours > 0.0 {
        format!("Hôm nay nghỉ, đã log {}", fmt_h(today_hours))
    } else {
        "Hôm nay nghỉ".into()
    };
    // Earlier days of the week still short, beyond today.
    if week_missing > today_missing + 1e-6 {
        line += &format!(" · tuần còn thiếu {}", fmt_h(week_missing));
    }
    line
}

fn menu(app: &AppHandle, status: Option<&str>) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    if let Some(line) = status {
        menu.append(&MenuItem::new(app, line, false, None::<&str>)?)?;
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    for (id, text) in ACTIONS {
        menu.append(&item(id, text)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&item("open", "Mở Redmine Logtime")?)?;
    menu.append(&item("quit", "Thoát")?)?;
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn status_shows_today_and_what_the_week_still_misses() {
        let s = Settings::default();
        // Thursday 29/10: Mon 8h, Tue 8h, Wed 5h, today 2h + 1.5h.
        let logged = [
            (d(26), 8.0),
            (d(27), 8.0),
            (d(28), 5.0),
            (d(29), 2.0),
            (d(29), 1.5),
        ];
        assert_eq!(
            status(&s, d(29), &logged),
            "Hôm nay 3.5h / 8h · tuần còn thiếu 7.5h"
        );
        // Nothing logged yet today.
        assert_eq!(
            status(&s, d(29), &logged[..3]),
            "Hôm nay 0h / 8h · tuần còn thiếu 11h"
        );
        // Only today short: the week adds nothing.
        let logged = [(d(26), 8.0), (d(27), 8.0), (d(28), 8.0), (d(29), 6.0)];
        assert_eq!(status(&s, d(29), &logged), "Hôm nay 6h / 8h");
    }

    #[test]
    fn days_off_mention_hours_only_when_logged() {
        let s = Settings::default();
        // Saturday 31/10, the week before it full.
        let mut logged: Vec<_> = (26..=30).map(|day| (d(day), 8.0)).collect();
        assert_eq!(status(&s, d(31), &logged), "Hôm nay nghỉ");
        logged.push((d(31), 2.0));
        assert_eq!(status(&s, d(31), &logged), "Hôm nay nghỉ, đã log 2h");
    }
}
