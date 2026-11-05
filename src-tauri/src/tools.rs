//! Detects the external CLIs the AI features need: the agents (`claude`,
//! `agy`, `opencode`) and `uvx` for the Redmine MCP server.
//!
//! GUI apps launched from Finder/the dock do not inherit the user's shell
//! PATH, so we ask the login shell for it once and reuse it.

use std::process::Command;
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ToolStatus {
    pub name: &'static str,
    pub version: Option<String>,
    pub install_hint: &'static str,
}

pub fn login_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        Command::new(shell)
            .args(["-ilc", "printf '__PATH__%s' \"$PATH\""])
            .output()
            .ok()
            .and_then(|o| {
                let out = String::from_utf8_lossy(&o.stdout).into_owned();
                out.rsplit_once("__PATH__")
                    .map(|(_, p)| p.trim().to_string())
            })
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| std::env::var("PATH").unwrap_or_default())
    })
}

fn version_of(bin: &str) -> Option<String> {
    let out = Command::new(bin)
        .arg("--version")
        .env("PATH", login_path())
        .output()
        .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    })
}

pub fn check_all() -> Vec<ToolStatus> {
    // Each `--version` is its own process; ask them all at once.
    let agents: Vec<_> = ai_bridge::AiAgent::ALL
        .iter()
        .map(|a| ai_bridge::agents::info(*a))
        .map(|info| {
            std::thread::spawn(move || ToolStatus {
                name: info.program,
                version: version_of(info.program),
                install_hint: info.install_hint,
            })
        })
        .collect();
    let mut out: Vec<ToolStatus> = agents.into_iter().filter_map(|t| t.join().ok()).collect();
    out.push(ToolStatus {
        name: "uvx",
        version: version_of("uvx"),
        install_hint: "curl -LsSf https://astral.sh/uv/install.sh | sh",
    });
    out
}
