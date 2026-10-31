//! Runs `claude -p` headless with the read-only Redmine MCP server and a JSON
//! schema for the answer, streaming tool calls back as progress.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::{Error, Result};

/// Pinned so upstream changes cannot alter behaviour silently.
pub const MCP_REDMINE_PACKAGE: &str = "mcp-redmine==2026.10.15.204909";

/// Redmine tools the model may use. Writes are refused by the server anyway
/// (REDMINE_READ_ONLY=1); uploads/downloads are not offered at all.
const ALLOWED_TOOLS: &str = "mcp__redmine__redmine_request,mcp__redmine__redmine_paths_list,mcp__redmine__redmine_paths_info";

pub struct ClaudeRunner {
    pub program: String,
    /// PATH for the child (GUI apps do not inherit the login shell's PATH).
    pub path_env: String,
    /// Scratch dir for the temporary MCP config; also the child's cwd.
    pub workdir: PathBuf,
    pub timeout: Duration,
    /// Use the claude.ai login: skip ~/.claude/settings.json and inherited
    /// API keys (they may hold a key that fails headless).
    pub ignore_user_settings: bool,
}

pub struct RedmineMcp {
    pub url: String,
    pub api_key: String,
    pub accept_invalid_certs: bool,
    /// Extra guidance for the model (REDMINE_REQUEST_INSTRUCTIONS).
    pub instructions: Option<PathBuf>,
}

/// Something worth showing while the model works.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Progress {
    pub text: String,
}

impl ClaudeRunner {
    pub async fn run<T: DeserializeOwned>(
        &self,
        prompt: &str,
        schema: &Value,
        mcp: Option<&RedmineMcp>,
        cancel: &AtomicBool,
        on_progress: &mut (dyn FnMut(Progress) + Send),
    ) -> Result<T> {
        fs::create_dir_all(&self.workdir)?;
        let config = mcp
            .map(|m| TempConfig::write(&self.workdir, m))
            .transpose()?;

        let mut cmd = Command::new(&self.program);
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
        if self.ignore_user_settings {
            // The workdir has no project settings, so nothing is loaded; drop
            // inherited API credentials too so the claude.ai login is used.
            cmd.args(["--setting-sources", "project"]);
            cmd.env_remove("ANTHROPIC_API_KEY");
            cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
        }
        if let Some(cfg) = &config {
            cmd.arg("--mcp-config").arg(&cfg.0);
            cmd.args(["--allowedTools", ALLOWED_TOOLS]);
        }
        cmd.current_dir(&self.workdir)
            .env("PATH", &self.path_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Don't let a parent Claude Code session leak into the child.
        for (key, _) in std::env::vars() {
            if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") {
                cmd.env_remove(key);
            }
        }

        let mut child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotInstalled,
            _ => Error::Io(e),
        })?;

        let mut stdin = child.stdin.take().expect("piped stdin");
        stdin.write_all(prompt.as_bytes()).await?;
        drop(stdin);

        let mut stderr = child.stderr.take().expect("piped stderr");
        let stderr_task = tokio::spawn(async move {
            let mut s = String::new();
            let _ = stderr.read_to_string(&mut s).await;
            s
        });

        let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
        let deadline = Instant::now() + self.timeout;
        let mut result: Option<Value> = None;

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Some(line) = line? else { break };
                    let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
                    if event["type"] == "result" {
                        result = Some(event);
                    } else if let Some(p) = describe_event(&event) {
                        on_progress(p);
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    if cancel.load(Ordering::Acquire) {
                        let _ = child.kill().await;
                        return Err(Error::Cancelled);
                    }
                    if Instant::now() > deadline {
                        let _ = child.kill().await;
                        return Err(Error::Timeout(self.timeout.as_secs()));
                    }
                }
            }
        }

        let _ = child.wait().await;
        let stderr = stderr_task.await.unwrap_or_default();
        drop(config);

        let result = result.ok_or_else(|| Error::Failed(tail(&stderr)))?;
        parse_result(result)
    }
}

/// Converts the final `result` event into the requested type.
fn parse_result<T: DeserializeOwned>(result: Value) -> Result<T> {
    if result["is_error"].as_bool().unwrap_or(false) {
        let msg = result["result"]
            .as_str()
            .unwrap_or("không rõ lỗi")
            .to_string();
        return Err(Error::Claude(msg));
    }
    let value = match result.get("structured_output") {
        Some(v) if !v.is_null() => v.clone(),
        _ => extract_json(result["result"].as_str().unwrap_or(""))
            .ok_or_else(|| Error::BadOutput("không thấy JSON trong câu trả lời".into()))?,
    };
    serde_json::from_value(value).map_err(|e| Error::BadOutput(e.to_string()))
}

/// The first JSON object in free text (handles ```json fences).
pub fn extract_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

/// Human-readable line for a tool call, e.g. "GET /issues.json?parent_id=12".
fn describe_event(event: &Value) -> Option<Progress> {
    if event["type"] != "assistant" {
        return None;
    }
    let blocks = event["message"]["content"].as_array()?;
    let calls: Vec<String> = blocks
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| {
            let input = &b["input"];
            match b["name"].as_str().unwrap_or("") {
                "mcp__redmine__redmine_request" => {
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
                }
                "mcp__redmine__redmine_paths_list" => "Đọc danh sách API của Redmine".into(),
                "mcp__redmine__redmine_paths_info" => "Đọc mô tả API của Redmine".into(),
                other => other.to_string(),
            }
        })
        .collect();
    (!calls.is_empty()).then(|| Progress {
        text: calls.join(", "),
    })
}

fn tail(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(5);
    let t = lines[start..].join("\n");
    if t.is_empty() {
        "Claude CLI dừng mà không trả kết quả.".into()
    } else {
        t
    }
}

/// MCP config holding the API key: private file, removed on drop.
struct TempConfig(PathBuf);

impl TempConfig {
    fn write(dir: &Path, mcp: &RedmineMcp) -> Result<Self> {
        let mut env = json!({
            "REDMINE_URL": mcp.url,
            "REDMINE_API_KEY": mcp.api_key,
            "REDMINE_READ_ONLY": "1",
            "REDMINE_RESPONSE_FORMAT": "json",
        });
        if mcp.accept_invalid_certs {
            env["REDMINE_DANGEROUSLY_ACCEPT_INVALID_CERTS"] = json!("1");
        }
        if let Some(p) = mcp.instructions.as_ref().filter(|p| p.exists()) {
            env["REDMINE_REQUEST_INSTRUCTIONS"] = json!(p);
        }
        let config = json!({"mcpServers": {"redmine": {
            "command": "uvx",
            "args": ["--from", MCP_REDMINE_PACKAGE, "mcp-redmine"],
            "env": env,
        }}});

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = dir.join(format!("mcp-{}-{nanos}.json", std::process::id()));
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        opts.open(&path)?.write_all(config.to_string().as_bytes())?;
        Ok(Self(path))
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Answer {
        name: String,
    }

    #[test]
    fn extracts_json_from_fenced_text() {
        let v = extract_json("Kết quả:\n```json\n{\"name\": \"Bob\"}\n```").unwrap();
        assert_eq!(v["name"], "Bob");
        assert!(extract_json("không có").is_none());
    }

    #[test]
    fn describes_redmine_calls() {
        let ev = json!({"type": "assistant", "message": {"content": [
            {"type": "text", "text": "..."},
            {"type": "tool_use", "name": "mcp__redmine__redmine_request",
             "input": {"path": "/issues.json", "params": {"parent_id": 12}}}
        ]}});
        assert_eq!(
            describe_event(&ev).unwrap().text,
            "GET /issues.json?parent_id=12"
        );
    }

    fn fake_claude(dir: &Path, body: &str) -> String {
        let path = dir.join("claude");
        fs::write(&path, format!("#!/bin/sh\ncat > /dev/null\n{body}\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        path.to_string_lossy().into_owned()
    }

    fn runner(dir: &Path, program: String, timeout: Duration) -> ClaudeRunner {
        ClaudeRunner {
            program,
            path_env: std::env::var("PATH").unwrap_or_default(),
            workdir: dir.join("work"),
            timeout,
            ignore_user_settings: false,
        }
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ai-bridge-{name}-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn mcp() -> RedmineMcp {
        RedmineMcp {
            url: "http://localhost:3000".into(),
            api_key: "secret".into(),
            accept_invalid_certs: false,
            instructions: None,
        }
    }

    #[tokio::test]
    async fn streams_progress_and_reads_structured_output() {
        let dir = tmpdir("ok");
        let program = fake_claude(
            &dir,
            r#"echo '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__redmine__redmine_request","input":{"path":"/trackers.json"}}]}}'
echo 'not json'
echo '{"type":"result","is_error":false,"result":"","structured_output":{"name":"Bob"}}'"#,
        );
        let mut seen = Vec::new();
        let ans: Answer = runner(&dir, program, Duration::from_secs(10))
            .run(
                "hi",
                &json!({}),
                Some(&mcp()),
                &AtomicBool::new(false),
                &mut |p| seen.push(p.text),
            )
            .await
            .unwrap();
        assert_eq!(ans, Answer { name: "Bob".into() });
        assert_eq!(seen, vec!["GET /trackers.json"]);
        // The config with the API key is gone.
        assert_eq!(fs::read_dir(dir.join("work")).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn reports_claude_errors() {
        let dir = tmpdir("err");
        let program = fake_claude(
            &dir,
            r#"echo '{"type":"result","is_error":true,"result":"Invalid API key"}'"#,
        );
        let err = runner(&dir, program, Duration::from_secs(10))
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Claude(ref m) if m.contains("Invalid API key")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn cancels_and_times_out() {
        let dir = tmpdir("slow");
        let program = fake_claude(&dir, "sleep 5");
        let r = runner(&dir, program.clone(), Duration::from_secs(10));
        let err = r
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(true), &mut |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Cancelled));

        let r = runner(&dir, program, Duration::from_millis(300));
        let err = r
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Timeout(_)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn missing_binary_is_reported() {
        let dir = tmpdir("missing");
        let err = runner(&dir, "/nonexistent/claude".into(), Duration::from_secs(1))
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotInstalled));
        fs::remove_dir_all(&dir).unwrap();
    }
}
