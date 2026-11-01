# Repository Guidelines

## Project Structure & Module Organization

Redmine Logtime combines Tauri 2, React/TypeScript, and a Rust workspace.

- `src/`: React pages, shared components, hooks (`useAiRun.ts`, `useModal.ts`), API bridge, mock data, and CSS.
- `src-tauri/`: desktop commands, application state, credentials (OS keychain with file fallback), tray and end-of-day reminder, in-app updater, logging, and Tauri configuration.
- `crates/google-chat/`: read-only Google Chat client and OAuth sign-in.
- `crates/redmine-client/`: Redmine REST client.
- `crates/logtime-core/`: settings, calendar, leave, and planning logic.
- `crates/logtime-engine/`: SQLite-backed write batches, retry, deduplication, and undo.
- `crates/ai-bridge/`: Claude CLI integration, report analysis, and remembered task choices (`memory.rs`, also used by the no-AI quick match).
- `docker/`: local Redmine and seed data; `scripts/`: installation and releases; `docs/`: development notes and screenshots.

## Build, Test, and Development Commands

Use Node 22 (matching CI), stable Rust, and the platform dependencies in [docs/development.md](docs/development.md). Install Node dependencies with `npm ci`.

- `make dev`: launch the desktop app with hot reload.
- `make mock`: run the browser UI with sample data at `http://localhost:1430/?page=calendar`.
- `make test`: run Rust workspace tests and TypeScript type checking.
- `npm run build`: type-check and build frontend assets.
- `make build`: build the desktop app without installing it.
- `cargo test -p logtime-engine`: run tests for one crate.

## Coding Style & Naming Conventions

Match existing code: TypeScript uses two-space indentation, double quotes, and semicolons; Rust uses four-space indentation and standard rustfmt conventions. Use PascalCase for React component files, `useCamelCase` for hooks, and snake_case for Rust modules and functions. Keep filenames in English. TypeScript strict checking is enabled; no dedicated frontend formatter or lint command is configured.

## Testing Guidelines

Rust tests use inline `mod tests` modules, `#[test]`, `#[tokio::test]`, and Wiremock. Name tests after behavior, such as `unauthorized_maps_to_error`. Cover changed planning, persistence, and API behavior. No coverage threshold or frontend test runner is configured. Run `make test`; inspect UI changes with `make mock`.

## Commit & Pull Request Guidelines

Follow the history's Conventional Commit prefixes: `feat:`, `fix:`, `docs:`, and `chore:`. Keep subjects concise and imperative. PRs should describe behavior changes, link relevant issues, report validation, and include screenshots for UI changes.

## Security & Configuration

Never commit API keys or credentials. Test writes against Docker Redmine, following `docs/development.md`; its seed does not cover the full monthly leave workflow. Preserve explicit user review before Redmine writes and the read-only access of the AI and Google Chat integrations. Undo removes time entries, retaining subtasks.

## Documentation

Keep [README.md](README.md) concise and in Vietnamese for macOS/Ubuntu users. Keep contributor setup and release steps in `docs/development.md`. Verify commands against scripts and button names against UI source.
