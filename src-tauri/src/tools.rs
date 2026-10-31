//! Detects the external CLIs the AI features need (`claude`, `uvx`).
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
    vec![
        ToolStatus {
            name: "claude",
            version: version_of("claude"),
            install_hint: "curl -fsSL https://claude.ai/install.sh | bash",
        },
        ToolStatus {
            name: "uvx",
            version: version_of("uvx"),
            install_hint: "curl -LsSf https://astral.sh/uv/install.sh | sh",
        },
    ]
}
