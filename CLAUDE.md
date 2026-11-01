# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

Contributor setup, Docker Redmine, data locations and release steps are in `docs/development.md`.

## Commands beyond AGENTS.md

- Single Rust test: `cargo test -p ai-bridge tidy_report` (crate + test name filter).
- Mock UI: `make mock`, then open `http://localhost:1430/?page=calendar`. Other pages are `review`, `leave`, `history` and `settings`; `?setup=1` shows the first-run screen. Mock flags: `&gchat=0` (Google Chat not connected) and `&writefail=1` (the last time entry of each write fails, to exercise retry).
- In mock, "today" is pinned to 2026-10-29 (`TODAY` in `src/mock.ts`).
- Real-Redmine examples (Docker only): `cargo run -p logtime-engine --example local_e2e` and `cargo run -p ai-bridge --example local_ai`. The second one uses Claude quota.
- Real OS keychain round trip (throwaway item): `cargo test -p redmine-logtime real_keychain -- --ignored`.
- Mock flag `&slow=1` makes every mock answer ~15× slower, to see the loading skeletons.
- Mock flag `&memory=0` starts with no remembered tasks (Khớp nhanh then explains why it cannot match).

## Architecture

### Frontend ↔ backend

- `src/api.ts` holds typed wrappers whose shapes mirror the Rust serde structs (snake_case). It calls `src/bridge.ts`, which sends `invoke`/`listen` to Tauri, or to `src/mock.ts` when `VITE_MOCK=1`.
- Adding or changing a command touches four places:
  - the Rust fn in `src-tauri/src/*_commands.rs`
  - `generate_handler!` in `src-tauri/src/lib.rs`
  - `src/api.ts`
  - `mockInvoke` in `src/mock.ts`
- `src-tauri/src/state.rs` (`AppState`):
  - settings JSON and the SQLite `Store`
  - credentials (`secrets.rs`, `credentials.json` 0600, which holds the Redmine API key and the Google refresh token)
  - busy flags so only one write and one AI run happen at a time
  - the in-memory Google access token

### Report → Redmine (the core flow)

1. **Input.** The user pastes reports, or imports their own Google Chat messages via `GchatImport` → `gchat_messages`.
2. **Analysis (`analyze_report`).** `ai-bridge::report::tidy_report` dedupes and orders the "[Gửi lúc …]" blocks. `prompt()` + `schema()` then go to `ClaudeRunner` (`runner.rs`), which runs `claude -p` with `mcp-redmine` (pinned version, `REDMINE_READ_ONLY=1`, allow-listed tools).
   - Claude returns the tasks, dates, candidate parent issues and per-day weights. It never sets the hours.
3. **Hours (`plan_report`).** `report::plan` → `logtime-core::planner::allocate` splits each day up to `hours_per_day`, minus existing Redmine hours and leave. `remove_duplicates` drops work already logged.
4. **Review (`src/reportFlow.tsx`).** `useReportFlow` holds the user's edits (parent, sub-task, subject, hours, unticked entries). It re-plans when entries are unticked; the write button stays disabled while `planning`.
   - `ReportResult` renders one card per day.
   - The same hook and view serve `ReportPage` (multi-day) and the calendar `DayPanel` (one day, `compact`).
5. **Write.** `buildPlan` produces a `WritePlan { subtasks, items }`. `WriteDialog` is the mandatory confirm step and then shows live progress through the `batch-progress` event.
   - `write_plan` stores a batch in SQLite (`logtime-engine::store`), and `exec::run_batch` runs its ops.
   - Failed ops stay failed until `retry_batch` (`Store::reset_failed`).
   - A new sub-task's `key` makes re-runs idempotent.
   - Undo deletes only the time entries; sub-tasks remain.
   - `write_plan` re-validates with `logtime_engine::plan::check_plan` (hours, targets, ≤ 24h per day including Redmine's hours) before creating the batch.
   - A failure that may have reached Redmine (network, timeout, 5xx) is marked `uncertain`; `reset_failed` turns it into `running`, so the retry looks up the existing entry/sub-task before creating.
6. **Learn.** After a write, `afterWrite` sends each task's chosen parent/sub-task to `remember_tasks` (`ai-bridge::memory`, kv key `task_memory`). The memory is applied over Claude's guesses in `analyze_report`, listed in the prompt, and drives `quick_report` (“Khớp nhanh”: date headers + memory, no Claude).
   - Manual add (`AddEntryForm`) and leave (`LeaveForm` + `src/leave.ts::buildLeave`) build a `WritePlan` too and go through the same dialog. There is no queue or draft.

### Other pieces

- **Desktop shell (`src-tauri/src/lib.rs`).** Plugins: log (file in the OS log dir), notification, autostart (`--hidden` starts in the tray), updater. The main window starts hidden and is shown in `setup`. With `reminder_enabled`, closing the window hides it to the tray (only if the tray exists). `reminder.rs` checks once a minute and notifies on workdays after `reminder_time` when today or the week is short.
- **Credentials (`secrets.rs`).** One JSON item in the OS keychain via `keyring`; falls back to `credentials.json` (0600) when the keychain fails, and migrates an old file into the keychain.
- **In-app update (`update_commands.rs`).** `install_update` reads `latest.json` from the latest GitHub release (signed by `scripts/ci/sign-update.sh` and `latest-json.sh` in `release.yml` when the `TAURI_SIGNING_PRIVATE_KEY` secret is set); otherwise the About card falls back to the install command.

- **Redmine profile.** "Phân tích cấu trúc Redmine" (`discover_profile`, `ai-bridge::profile`) writes `redmine-profile.md`. It is passed to mcp-redmine as request instructions, to improve parent-task matching.
- **Leave.** Each leave type maps to a project with a monthly parent issue created by HR. The app creates one child issue per leave day and logs time on it (`leave_commands.rs`, `logtime-core::leave`, `src/leave.ts`).
- **Google Chat.**
  - Sign-in is installed-app OAuth: a loopback redirect on 127.0.0.1 with PKCE (`google-chat::oauth`).
  - Messages are filtered to the signed-in user by matching the OIDC `sub` to the sender `users/{id}`.
  - Scopes are read-only.

## Conventions not obvious from the code

- Icons come from Lucide (`lucide-react`), mapped by role in `src/icons.tsx` and used as `<Icon name="…" />`; do not hand-draw SVG icons. The logo is the Redmine logo mark recoloured in the app's blues on a white tile (`BrandMark`, `src-tauri/icons/source.svg`); it is CC BY-SA 2.5 (© 2009 Martin Herr), so keep the attribution in those files and the README.
- All user-facing text (UI, errors, README) is Vietnamese. Code, comments and AGENTS/CLAUDE files are English.
- Dialogs are native `<dialog>` elements wired through `useModal(onClose, locked)`. They close on Esc and on a backdrop click unless `locked`, e.g. while writing. Popovers such as `DateRangePicker` and `IssuePicker` close on an outside `pointerdown` and on Esc.
- The report prompt in `crates/ai-bridge/src/report.rs` is an `r#"…"#` raw string: never write `"#` inside it. Keep it principle-based rather than tied to one team's report format.
- Never write to a real Redmine while testing. Use mock mode, or Docker Redmine for writes.
- README screenshots in `docs/images/` are captured from mock mode.
