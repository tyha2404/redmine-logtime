use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use logtime_core::Settings;
use logtime_engine::Store;

use crate::secrets::Credentials;

/// App-wide state: settings are persisted as JSON in the OS config dir, the
/// draft and execution history in SQLite in the data dir.
pub struct AppState {
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    pub store: Store,
    pub credentials: Credentials,
    /// Markdown profile handed to mcp-redmine as request instructions.
    pub profile_md_path: PathBuf,
    /// Scratch dir for Claude runs (temporary MCP configs).
    pub ai_workdir: PathBuf,
    executing: AtomicBool,
    ai_running: AtomicBool,
    pub ai_cancel: AtomicBool,
    /// Google access token and when it expires; refreshed from the stored
    /// refresh token, never written to disk.
    pub gchat_token: Mutex<Option<(String, Instant)>>,
    gchat_connecting: AtomicBool,
    pub gchat_cancel: AtomicBool,
}

impl AppState {
    pub fn load(
        config_dir: PathBuf,
        data_dir: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        fs::create_dir_all(&config_dir)?;
        restrict_dir(&config_dir)?;
        fs::create_dir_all(&data_dir)?;
        let settings_path = config_dir.join("settings.json");
        let settings = match fs::read_to_string(&settings_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self {
            settings_path,
            settings: Mutex::new(settings),
            store: Store::open(&data_dir.join("logtime.db"))?,
            credentials: Credentials::new(&config_dir),
            profile_md_path: config_dir.join("redmine-profile.md"),
            ai_workdir: data_dir.join("ai"),
            executing: AtomicBool::new(false),
            ai_running: AtomicBool::new(false),
            ai_cancel: AtomicBool::new(false),
            gchat_token: Mutex::new(None),
            gchat_connecting: AtomicBool::new(false),
            gchat_cancel: AtomicBool::new(false),
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn save_settings(&self, settings: Settings) -> io::Result<()> {
        let raw = serde_json::to_string_pretty(&settings)?;
        // Write-then-rename so a crash never leaves a half-written file.
        let tmp = self.settings_path.with_extension("json.tmp");
        fs::write(&tmp, raw)?;
        fs::rename(&tmp, &self.settings_path)?;
        *self.settings.lock().unwrap() = settings;
        Ok(())
    }

    /// Allows one write batch at a time; the guard releases on drop.
    pub fn begin_execution(&self) -> Option<ExecutionGuard<'_>> {
        claim(&self.executing)
    }

    /// Allows one Claude run at a time and clears any stale cancel request.
    pub fn begin_ai(&self) -> Option<ExecutionGuard<'_>> {
        let guard = claim(&self.ai_running)?;
        self.ai_cancel.store(false, Ordering::Release);
        Some(guard)
    }

    /// Allows one Google sign-in at a time and clears any stale cancel request.
    pub fn begin_gchat_connect(&self) -> Option<ExecutionGuard<'_>> {
        let guard = claim(&self.gchat_connecting)?;
        self.gchat_cancel.store(false, Ordering::Release);
        Some(guard)
    }
}

fn claim(flag: &AtomicBool) -> Option<ExecutionGuard<'_>> {
    flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .ok()
        .map(|_| ExecutionGuard(flag))
}

/// The config dir holds the API key file: keep it private to the user.
#[cfg(unix)]
fn restrict_dir(dir: &std::path::Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_dir(_: &std::path::Path) -> io::Result<()> {
    Ok(())
}

pub struct ExecutionGuard<'a>(&'a AtomicBool);

impl Drop for ExecutionGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
