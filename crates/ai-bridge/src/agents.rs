//! What differs between the CLI agents: how each one is started (arguments,
//! the Redmine MCP config, isolation from the user's own setup) and how its
//! event stream reads. `runner.rs` drives the process the same way for all.
//!
//! Every agent gets the read-only mcp-redmine server and nothing else: no
//! shell, no file or web tools, none of the user's own MCP servers.
//! Agents are: Claude Code, Antigravity CLI (`agy`) and opencode.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use tokio::process::Command;

use crate::runner::{MCP_REDMINE_PACKAGE, Progress, RedmineMcp, extract_json};
use crate::{AiAgent, Error, Result};

/// How an agent is shown and installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentInfo {
    pub agent: AiAgent,
    pub name: &'static str,
    pub program: &'static str,
    pub install_hint: &'static str,
}

pub fn info(agent: AiAgent) -> AgentInfo {
    match agent {
        AiAgent::Claude => AgentInfo {
            agent,
            name: "Claude",
            program: "claude",
            install_hint: "curl -fsSL https://claude.ai/install.sh | bash",
        },
        AiAgent::Agy => AgentInfo {
            agent,
            name: "Antigravity",
            program: "agy",
            install_hint: "curl -fsSL https://antigravity.google/cli/install.sh | bash",
        },
        AiAgent::Opencode => AgentInfo {
            agent,
            name: "opencode",
            program: "opencode",
            install_hint: "curl -fsSL https://opencode.ai/install | bash",
        },
    }
}

/// mcp-redmine's tools the model may use. Writes are refused by the server
/// anyway (REDMINE_READ_ONLY=1); uploads/downloads are not offered at all.
const REDMINE_TOOLS: [&str; 3] = [
    "redmine_request",
    "redmine_paths_list",
    "redmine_paths_info",
];

/// opencode's built-in tools, all switched off.
const OPENCODE_TOOLS: [&str; 17] = [
    "bash",
    "edit",
    "write",
    "patch",
    "read",
    "glob",
    "grep",
    "list",
    "lsp",
    "webfetch",
    "websearch",
    "codesearch",
    "todowrite",
    "todoread",
    "task",
    "skill",
    "question",
];

pub(crate) struct Options<'a> {
    pub program: &'a str,
    /// The user's home: sign-ins and caches live there.
    pub home: Option<&'a Path>,
    pub workdir: &'a Path,
    pub model: Option<&'a str>,
    pub ignore_user_settings: bool,
}

/// A command ready to spawn, the prompt for its stdin, and how to read it.
pub(crate) struct Launch {
    pub cmd: Command,
    pub stdin: String,
    pub stream: Box<dyn Stream>,
    /// A stderr line that means the run cannot succeed (opencode retries a
    /// refused model call forever without a word on stdout).
    pub fatal: Option<fn(&str) -> Option<String>>,
    /// Config files holding the API key; removed on drop.
    pub temp: Vec<TempPath>,
    /// Where the agent runs, when not in the workdir itself.
    pub cwd: Option<PathBuf>,
}

/// What the agent's output ended with.
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Answer(Value),
    AgentError(String),
    BadOutput(String),
}

/// Reads one agent's JSON event stream.
pub(crate) trait Stream: Send {
    /// One stdout event; what to show the user, if anything.
    fn event(&mut self, event: &Value) -> Option<Progress>;
    /// The result once stdout has closed; None when the agent stopped
    /// without saying anything (the caller then reports stderr).
    fn finish(&mut self) -> Option<Outcome>;
    /// Arguments of a follow-up run that tidies up after this one.
    fn cleanup(&self) -> Option<Vec<String>> {
        None
    }
}

pub(crate) fn launch(
    agent: AiAgent,
    opts: &Options,
    prompt: &str,
    schema: &Value,
    mcp: Option<&RedmineMcp>,
) -> Result<Launch> {
    match agent {
        AiAgent::Claude => claude(opts, prompt, schema, mcp),
        AiAgent::Agy => agy(opts, prompt, schema, mcp),
        AiAgent::Opencode => opencode(opts, prompt, schema, mcp),
    }
}

// ---------------------------------------------------------------------------
// Claude Code: `claude -p`, which enforces the JSON schema itself.

fn claude(
    opts: &Options,
    prompt: &str,
    schema: &Value,
    mcp: Option<&RedmineMcp>,
) -> Result<Launch> {
    let mut cmd = Command::new(opts.program);
    cmd.args([
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--no-session-persistence",
        "--tools",
        "",
        "--strict-mcp-config",
        "--json-schema",
        &schema.to_string(),
    ]);
    if let Some(model) = opts.model {
        cmd.args(["--model", model]);
    }
    if opts.ignore_user_settings {
        // The workdir has no project settings, so nothing is loaded; drop
        // inherited API credentials too so the claude.ai login is used.
        cmd.args(["--setting-sources", "project"]);
        cmd.env_remove("ANTHROPIC_API_KEY");
        cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
    }
    let mut temp = Vec::new();
    if let Some(m) = mcp {
        let config = json!({"mcpServers": {"redmine": {
            "command": "uvx",
            "args": mcp_args(),
            "env": mcp_env(m),
        }}});
        let file = TempPath::file(opts.workdir, "mcp", &config)?;
        let allowed: Vec<String> = REDMINE_TOOLS
            .iter()
            .map(|t| format!("mcp__redmine__{t}"))
            .collect();
        cmd.arg("--mcp-config").arg(&file.0);
        cmd.args(["--allowedTools", &allowed.join(",")]);
        temp.push(file);
    }
    // Don't let a parent Claude Code session leak into the child.
    for (key, _) in std::env::vars() {
        if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") {
            cmd.env_remove(key);
        }
    }
    Ok(Launch {
        cmd,
        stdin: prompt.to_string(),
        stream: Box::new(ClaudeStream::default()),
        fatal: None,
        temp,
        cwd: None,
    })
}

#[derive(Default)]
struct ClaudeStream {
    result: Option<Value>,
}

impl Stream for ClaudeStream {
    fn event(&mut self, event: &Value) -> Option<Progress> {
        match event["type"].as_str() {
            Some("result") => {
                self.result = Some(event.clone());
                None
            }
            Some("assistant") => {
                let calls: Vec<String> = event["message"]["content"]
                    .as_array()?
                    .iter()
                    .filter(|b| b["type"] == "tool_use")
                    .map(|b| describe_tool(b["name"].as_str().unwrap_or(""), &b["input"]))
                    .collect();
                progress(calls)
            }
            _ => None,
        }
    }

    fn finish(&mut self) -> Option<Outcome> {
        let result = self.result.take()?;
        if result["is_error"].as_bool().unwrap_or(false) {
            let msg = result["result"].as_str().unwrap_or("không rõ lỗi");
            return Some(Outcome::AgentError(msg.to_string()));
        }
        Some(match result.get("structured_output") {
            Some(v) if !v.is_null() => Outcome::Answer(v.clone()),
            _ => answer(&[result["result"].as_str().unwrap_or("").to_string()]),
        })
    }
}

// ---------------------------------------------------------------------------
// Antigravity CLI: `agy` with JSON in and out and its own `--json-schema`.
// It has no flag for a config file, so it runs with a throwaway HOME that
// holds only the Redmine server, a permission list allowing just its read
// tools, and a link to the user's sign-in token. The user's MCP servers,
// plugins, hooks and `always-proceed` setting stay out; anything else the
// model tries (commands, edits, files outside its empty cwd) is denied, as
// headless runs cannot ask. The API key is passed in the environment, which
// the MCP server inherits, so it is in no file the model could read.

/// Where agy keeps its sign-in, relative to HOME.
pub(crate) const AGY_TOKEN: &str = ".gemini/antigravity-cli/antigravity-oauth-token";

fn agy(opts: &Options, prompt: &str, schema: &Value, mcp: Option<&RedmineMcp>) -> Result<Launch> {
    let real_home = opts.home;
    let token = real_home.map(|h| h.join(AGY_TOKEN)).filter(|t| t.exists());
    let Some(token) = token else {
        return Err(Error::Agent {
            name: info(AiAgent::Agy).name,
            msg: "chưa đăng nhập. Chạy agy trong Terminal để đăng nhập rồi thử lại.".into(),
        });
    };

    let root = TempPath::dir(opts.workdir, "agy")?;
    // agy resolves symlinks (macOS's /var → /private/var) before it checks
    // whether a path is inside its own home, so hand it the resolved one.
    let base = fs::canonicalize(&root.0)?;
    let home = base.join("home");
    let cwd = base.join("work");
    let cli = home.join(".gemini/antigravity-cli");
    let config = home.join(".gemini/config");
    for dir in [&cli, &config, &cwd] {
        fs::create_dir_all(dir)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&token, home.join(AGY_TOKEN))?;

    let allow: Vec<String> = REDMINE_TOOLS
        .iter()
        .map(|t| format!("mcp(redmine/{t})"))
        .collect();
    write_private(
        &cli.join("settings.json"),
        &json!({"toolPermission": "request-review", "permissions": {"allow": allow}}),
    )?;
    let mut servers = Map::new();
    if let Some(m) = mcp {
        let mut env = mcp_options(m);
        // uv keeps its cache and Python builds under the real HOME.
        if let Some(h) = real_home {
            env["HOME"] = json!(h);
        }
        servers.insert(
            "redmine".into(),
            json!({
                "command": "uvx",
                "args": mcp_args(),
                "env": env,
                "enabledTools": REDMINE_TOOLS,
            }),
        );
    }
    write_private(
        &config.join("mcp_config.json"),
        &json!({"mcpServers": servers}),
    )?;

    let mut cmd = Command::new(opts.program);
    cmd.args([
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--json-schema",
        &schema.to_string(),
    ]);
    if let Some(model) = opts.model {
        cmd.args(["--model", model]);
    }
    cmd.env("HOME", &home)
        .env("AGY_CLI_DISABLE_AUTO_UPDATE", "1");
    if let Some(m) = mcp {
        cmd.env("REDMINE_URL", &m.url)
            .env("REDMINE_API_KEY", &m.api_key);
    }
    let message = json!({
        "event": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": prompt}]},
    });
    Ok(Launch {
        cmd,
        stdin: format!("{message}\n"),
        stream: Box::new(AgyStream::default()),
        fatal: None,
        temp: vec![root],
        cwd: Some(cwd),
    })
}

#[derive(Default)]
struct AgyStream {
    done: Option<Outcome>,
}

impl Stream for AgyStream {
    fn event(&mut self, event: &Value) -> Option<Progress> {
        match event["event"].as_str()? {
            "step_update" => {
                let step = &event["step_update"];
                if step["step_type"] != "tool"
                    || step["state"] != "ACTIVE"
                    || step["tool_name"] != "call_mcp_tool"
                {
                    return None;
                }
                let p = &step["tool_info"]["parameters"];
                progress(vec![describe_tool(
                    p["ToolName"].as_str().unwrap_or(""),
                    &p["Arguments"],
                )])
            }
            "result" => {
                let r = &event["result"];
                self.done = Some(if r["status"] != "SUCCESS" {
                    Outcome::AgentError(r["error"].as_str().unwrap_or("không rõ lỗi").to_string())
                } else if !r["structured_output"].is_null() {
                    Outcome::Answer(r["structured_output"].clone())
                } else if let Some(denied) =
                    r["denied_actions"].as_array().filter(|d| !d.is_empty())
                {
                    // The model reached for a tool it is not allowed and stopped there.
                    let names: Vec<&str> = denied
                        .iter()
                        .filter_map(|d| d["display_name"].as_str())
                        .collect();
                    Outcome::AgentError(format!(
                        "dừng vì cần quyền app không cấp ({}). Thử lại; nếu lặp lại, đổi model.",
                        names.join(", ")
                    ))
                } else {
                    answer(&[r["response"].as_str().unwrap_or("").to_string()])
                });
                None
            }
            _ => None,
        }
    }

    fn finish(&mut self) -> Option<Outcome> {
        self.done.take()
    }
}

// ---------------------------------------------------------------------------
// opencode: `opencode run --format json` with its own config home, so the
// user's MCP servers, plugins and instructions stay out. Their providers and
// default model are copied over; logins live in the data dir and still work.
// The schema goes into the prompt.

fn opencode(
    opts: &Options,
    prompt: &str,
    schema: &Value,
    mcp: Option<&RedmineMcp>,
) -> Result<Launch> {
    let mut config = user_config_home()
        .map(|home| opencode_providers(&home.join("opencode")))
        .unwrap_or_default();
    config.insert("$schema".into(), json!("https://opencode.ai/config.json"));
    config.insert("autoupdate".into(), json!(false));
    config.insert("share".into(), json!("disabled"));
    let tools: Map<String, Value> = OPENCODE_TOOLS
        .iter()
        .map(|t| (t.to_string(), json!(false)))
        .collect();
    config.insert("tools".into(), Value::Object(tools));
    config.insert(
        "permission".into(),
        json!({"edit": "deny", "bash": "deny", "webfetch": "deny"}),
    );
    if let Some(m) = mcp {
        let mut command = vec![json!("uvx")];
        command.extend(mcp_args().iter().map(|a| json!(a)));
        config.insert(
            "mcp".into(),
            json!({"redmine": {
                "type": "local",
                "command": command,
                "environment": mcp_env(m),
                "enabled": true,
            }}),
        );
    }
    let home = TempPath::dir(opts.workdir, "opencode")?;
    let dir = home.0.join("opencode");
    fs::create_dir(&dir)?;
    write_private(&dir.join("opencode.json"), &Value::Object(config))?;

    let mut cmd = Command::new(opts.program);
    cmd.args([
        "run",
        "--format",
        "json",
        "--print-logs",
        "--log-level",
        "ERROR",
    ]);
    if let Some(model) = opts.model {
        cmd.args(["--model", model]);
    }
    cmd.env("XDG_CONFIG_HOME", &home.0);
    for flag in [
        "OPENCODE_DISABLE_PROJECT_CONFIG",
        "OPENCODE_DISABLE_CLAUDE_CODE",
        "OPENCODE_DISABLE_DEFAULT_PLUGINS",
        "OPENCODE_DISABLE_EXTERNAL_SKILLS",
        "OPENCODE_DISABLE_AUTOCOMPACT",
        "OPENCODE_DISABLE_AUTOUPDATE",
        "OPENCODE_DISABLE_SHARE",
    ] {
        cmd.env(flag, "1");
    }
    for key in [
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_CONTENT",
        "OPENCODE_CONFIG_DIR",
    ] {
        cmd.env_remove(key);
    }
    Ok(Launch {
        cmd,
        stdin: with_schema(prompt, schema),
        stream: Box::new(OpencodeStream::default()),
        fatal: Some(opencode_fatal),
        temp: vec![home],
        cwd: None,
    })
}

/// Where the user's own config lives (`~/.config` unless XDG says otherwise).
fn user_config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

/// The provider and model settings of the user's opencode config.
fn opencode_providers(dir: &Path) -> Map<String, Value> {
    const KEEP: [&str; 5] = [
        "provider",
        "model",
        "small_model",
        "enabled_providers",
        "disabled_providers",
    ];
    let mut out = Map::new();
    for name in ["config.json", "opencode.json", "opencode.jsonc"] {
        let Ok(text) = fs::read_to_string(dir.join(name)) else {
            continue;
        };
        let Ok(Value::Object(cfg)) = serde_json::from_str::<Value>(&strip_jsonc(&text)) else {
            continue;
        };
        for (key, value) in cfg {
            if KEEP.contains(&key.as_str()) {
                out.insert(key, value);
            }
        }
    }
    out
}

/// JSON with comments and trailing commas → plain JSON.
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
            continue;
        } else if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if !matches!(next, Some('}') | Some(']')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// `level=ERROR … message="stream error" … error.error="…"`: the model call
/// was refused (quota, login, unknown model) and opencode would only retry.
fn opencode_fatal(line: &str) -> Option<String> {
    if !line.contains("level=ERROR") || !line.contains("message=\"stream error\"") {
        return None;
    }
    let msg = line.split_once("error.error=").map(|(_, rest)| rest)?;
    let msg = msg
        .strip_prefix('"')
        .map_or(msg, |m| m.split('"').next().unwrap_or(m));
    Some(msg.trim().to_string())
}

#[derive(Default)]
struct OpencodeStream {
    /// Text parts by id, in order.
    texts: Vec<(String, String)>,
    error: Option<String>,
    session: Option<String>,
    events: usize,
}

impl Stream for OpencodeStream {
    fn event(&mut self, event: &Value) -> Option<Progress> {
        self.events += 1;
        if let Some(id) = event["sessionID"].as_str() {
            self.session = Some(id.to_string());
        }
        let part = &event["part"];
        match event["type"].as_str()? {
            "text" if part["synthetic"] != true => {
                let id = part["id"].as_str().unwrap_or("").to_string();
                let text = part["text"].as_str().unwrap_or("").to_string();
                match self.texts.iter_mut().find(|(k, _)| *k == id) {
                    Some(slot) => slot.1 = text,
                    None => self.texts.push((id, text)),
                }
                None
            }
            "tool_use" => {
                let name = part["tool"].as_str().unwrap_or("");
                progress(vec![describe_tool(name, &part["state"]["input"])])
            }
            "error" => {
                let e = &event["error"];
                let msg = e["data"]["message"]
                    .as_str()
                    .or(e["message"].as_str())
                    .or(e["name"].as_str())
                    .unwrap_or("không rõ lỗi");
                self.error = Some(msg.to_string());
                None
            }
            _ => None,
        }
    }

    fn finish(&mut self) -> Option<Outcome> {
        if let Some(msg) = self.error.take() {
            return Some(Outcome::AgentError(msg));
        }
        if self.events == 0 {
            return None;
        }
        let texts: Vec<String> = self.texts.drain(..).map(|(_, t)| t).collect();
        Some(answer(&texts))
    }

    /// Runs leave no session behind in the user's opencode history.
    fn cleanup(&self) -> Option<Vec<String>> {
        let id = self.session.as_ref()?;
        Some(vec!["session".into(), "delete".into(), id.clone()])
    }
}

// ---------------------------------------------------------------------------
// Shared pieces.

fn mcp_args() -> [&'static str; 3] {
    ["--from", MCP_REDMINE_PACKAGE, "mcp-redmine"]
}

/// The server's environment, including the URL and API key.
fn mcp_env(mcp: &RedmineMcp) -> Value {
    let mut env = mcp_options(mcp);
    env["REDMINE_URL"] = json!(mcp.url);
    env["REDMINE_API_KEY"] = json!(mcp.api_key);
    env
}

/// The server's settings that are not secret.
fn mcp_options(mcp: &RedmineMcp) -> Value {
    let mut env = json!({
        "REDMINE_READ_ONLY": "1",
        "REDMINE_RESPONSE_FORMAT": "json",
    });
    if mcp.accept_invalid_certs {
        env["REDMINE_DANGEROUSLY_ACCEPT_INVALID_CERTS"] = json!("1");
    }
    if let Some(p) = mcp.instructions.as_ref().filter(|p| p.exists()) {
        env["REDMINE_REQUEST_INSTRUCTIONS"] = json!(p);
    }
    env
}

/// For agents without a schema option: the schema is spelled out in the
/// prompt and the answer is checked when it is parsed.
fn with_schema(prompt: &str, schema: &Value) -> String {
    let schema = serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());
    format!(
        "{prompt}\n\n## Định dạng câu trả lời\n\nCâu trả lời cuối cùng chỉ gồm một object JSON hợp lệ theo JSON Schema dưới đây: không thêm chữ nào khác, không bọc trong ```.\n\n{schema}\n"
    )
}

/// The JSON answer: the last text that holds one (earlier text may be the
/// model thinking aloud before its tool calls).
fn answer(texts: &[String]) -> Outcome {
    texts
        .iter()
        .rev()
        .find_map(|t| extract_json(t))
        .or_else(|| extract_json(&texts.concat()))
        .map(Outcome::Answer)
        .unwrap_or_else(|| Outcome::BadOutput("không thấy JSON trong câu trả lời".into()))
}

fn progress(calls: Vec<String>) -> Option<Progress> {
    (!calls.is_empty()).then(|| Progress {
        text: calls.join(", "),
    })
}

/// Human-readable line for a tool call, e.g. "GET /issues.json?parent_id=12".
/// Agents prefix MCP tool names differently (`mcp__redmine__…`,
/// `redmine_…`), so they are matched on their ending.
pub(crate) fn describe_tool(name: &str, input: &Value) -> String {
    if name.ends_with("redmine_request") {
        let method = input["method"].as_str().unwrap_or("get").to_uppercase();
        let path = input["path"].as_str().unwrap_or("?");
        match input["params"].as_object().filter(|p| !p.is_empty()) {
            Some(p) => {
                let q: Vec<String> = p
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{k}={}",
                            v.as_str().map(String::from).unwrap_or(v.to_string())
                        )
                    })
                    .collect();
                format!("{method} {path}?{}", q.join("&"))
            }
            None => format!("{method} {path}"),
        }
    } else if name.ends_with("redmine_paths_list") {
        "Đọc danh sách API của Redmine".into()
    } else if name.ends_with("redmine_paths_info") {
        "Đọc mô tả API của Redmine".into()
    } else {
        name.to_string()
    }
}

/// A private file or directory in the workdir, removed on drop.
pub(crate) struct TempPath(pub PathBuf);

impl TempPath {
    fn unique(dir: &Path, prefix: &str, ext: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        dir.join(format!("{prefix}-{}-{nanos}{ext}", std::process::id()))
    }

    fn file(dir: &Path, prefix: &str, content: &Value) -> Result<Self> {
        let path = Self::unique(dir, prefix, ".json");
        write_private(&path, content)?;
        Ok(Self(path))
    }

    fn dir(dir: &Path, prefix: &str) -> Result<Self> {
        let path = Self::unique(dir, prefix, "");
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if self.0.is_dir() {
            let _ = fs::remove_dir_all(&self.0);
        } else {
            let _ = fs::remove_file(&self.0);
        }
    }
}

fn write_private(path: &Path, content: &Value) -> Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(path)?.write_all(content.to_string().as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_redmine_calls_whatever_the_prefix() {
        let input = json!({"path": "/issues.json", "params": {"parent_id": 12}});
        for name in [
            "mcp__redmine__redmine_request",
            "redmine_redmine_request",
            "redmine_request",
        ] {
            assert_eq!(describe_tool(name, &input), "GET /issues.json?parent_id=12");
        }
        assert_eq!(
            describe_tool("redmine_redmine_paths_list", &json!({})),
            "Đọc danh sách API của Redmine"
        );
    }

    #[test]
    fn answer_prefers_the_last_text_with_json() {
        let texts = vec![
            "Tôi sẽ đọc {trackers} trước.".to_string(),
            "```json\n{\"name\": \"Bob\"}\n```".to_string(),
        ];
        assert_eq!(answer(&texts), Outcome::Answer(json!({"name": "Bob"})));
        assert!(matches!(
            answer(&["không có".into()]),
            Outcome::BadOutput(_)
        ));
    }

    #[test]
    fn agy_stream_reads_mcp_calls_and_the_structured_result() {
        let step = |state: &str, tool: &str, params: Value| {
            json!({"event": "step_update", "step_update": {"step_type": "tool", "state": state,
                "tool_name": tool, "tool_info": {"name": tool, "parameters": params}}})
        };
        let call = json!({"ServerName": "redmine", "ToolName": "redmine_request", "Arguments": {"path": "/trackers.json"}});
        let mut s = AgyStream::default();
        let events = [
            json!({"event": "init", "init": {"cwd": "/tmp", "permission_mode": "request-review"}}),
            step(
                "ACTIVE",
                "read_file",
                json!({"Path": "/x/mcp/redmine/redmine_request.json"}),
            ),
            step("ACTIVE", "call_mcp_tool", call.clone()),
            step("DONE", "call_mcp_tool", call),
            json!({"event": "step_update", "step_update": {"step_type": "agent_response", "state": "DONE", "text_delta": "{}"}}),
            json!({"event": "result", "result": {"status": "SUCCESS", "response": "{\"name\":\"Bob\"}\n",
                "structured_output": {"name": "Bob"}}}),
        ];
        let seen: Vec<_> = events
            .iter()
            .filter_map(|e| s.event(e))
            .map(|p| p.text)
            .collect();
        assert_eq!(seen, vec!["GET /trackers.json"]);
        assert_eq!(s.finish(), Some(Outcome::Answer(json!({"name": "Bob"}))));

        let mut s = AgyStream::default();
        s.event(
            &json!({"event": "result", "result": {"status": "SUCCESS", "response": "",
            "denied_actions": [{"action": "command", "display_name": "RunCommand"}]}}),
        );
        assert!(matches!(s.finish(), Some(Outcome::AgentError(m)) if m.contains("RunCommand")));

        let mut s = AgyStream::default();
        s.event(&json!({"event": "result", "result": {"status": "ERROR", "error": "authentication failed or timed out"}}));
        assert_eq!(
            s.finish(),
            Some(Outcome::AgentError(
                "authentication failed or timed out".into()
            ))
        );
        assert_eq!(AgyStream::default().finish(), None);
    }

    #[test]
    fn opencode_stream_skips_synthetic_text_and_keeps_the_session() {
        let mut s = OpencodeStream::default();
        let events = [
            json!({"type": "step_start", "sessionID": "ses_1", "part": {"type": "step-start"}}),
            json!({"type": "tool_use", "sessionID": "ses_1", "part": {"type": "tool", "tool": "redmine_redmine_request",
                "state": {"status": "completed", "input": {"path": "/issues.json", "params": {"status_id": "open"}}}}}),
            json!({"type": "text", "sessionID": "ses_1", "part": {"id": "p1", "type": "text", "text": "{\"name\": \"Bob\"}"}}),
            json!({"type": "text", "sessionID": "ses_1", "part": {"id": "p2", "type": "text", "synthetic": true, "text": "Continue"}}),
            json!({"type": "step_finish", "sessionID": "ses_1", "part": {"reason": "stop"}}),
        ];
        let seen: Vec<_> = events
            .iter()
            .filter_map(|e| s.event(e))
            .map(|p| p.text)
            .collect();
        assert_eq!(seen, vec!["GET /issues.json?status_id=open"]);
        assert_eq!(s.finish(), Some(Outcome::Answer(json!({"name": "Bob"}))));
        assert_eq!(
            s.cleanup(),
            Some(vec!["session".into(), "delete".into(), "ses_1".into()])
        );

        let mut s = OpencodeStream::default();
        s.event(&json!({"type": "error", "sessionID": "ses_2", "error": {"name": "APIError", "data": {"message": "invalid x-api-key"}}}));
        assert_eq!(
            s.finish(),
            Some(Outcome::AgentError("invalid x-api-key".into()))
        );
        assert_eq!(OpencodeStream::default().finish(), None);
    }

    #[test]
    fn opencode_refused_model_call_is_fatal() {
        let line = r#"timestamp=2026-11-05T15:05:07Z level=ERROR run=a message="stream error" providerID=opencode error.error="AI_APICallError: Free usage limit reached. Subscribe" extra=1"#;
        assert_eq!(
            opencode_fatal(line).as_deref(),
            Some("AI_APICallError: Free usage limit reached. Subscribe")
        );
        assert_eq!(opencode_fatal(r#"level=ERROR message="mcp failed""#), None);
        assert_eq!(opencode_fatal("level=INFO message=\"stream error\""), None);
    }

    #[test]
    fn copies_only_provider_settings_from_jsonc() {
        let dir = std::env::temp_dir().join(format!("ai-bridge-oc-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("opencode.jsonc"),
            r#"{
  // local models
  "provider": {"ollama": {"options": {"baseURL": "http://localhost:11434/v1"}}},
  /* the default */ "model": "ollama/qwen3",
  "mcp": {"rea": {"type": "local", "command": ["rea"]}},
  "plugin": ["x"],
}"#,
        )
        .unwrap();
        let cfg = opencode_providers(&dir);
        assert_eq!(cfg["model"], "ollama/qwen3");
        assert_eq!(
            cfg["provider"]["ollama"]["options"]["baseURL"],
            "http://localhost:11434/v1"
        );
        assert!(!cfg.contains_key("mcp") && !cfg.contains_key("plugin"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn strip_jsonc_keeps_strings_intact() {
        let s = strip_jsonc(r#"{"url": "http://a//b", "x": "/* no */", "a": [1, 2,],}"#);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["url"], "http://a//b");
        assert_eq!(v["x"], "/* no */");
        assert_eq!(v["a"], json!([1, 2]));
    }
}
