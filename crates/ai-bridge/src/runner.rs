//! Runs a CLI agent headless with the read-only Redmine MCP server and a JSON
//! schema for the answer, streaming its tool calls back as progress. What
//! differs per agent lives in `agents.rs`.

use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::agents::{self, AgentInfo, Launch, Options, Outcome};
use crate::{AiAgent, Error, Result};

/// Pinned so upstream changes cannot alter behaviour silently.
pub const MCP_REDMINE_PACKAGE: &str = "mcp-redmine==2026.10.15.204909";

pub struct AgentRunner {
    pub agent: AiAgent,
    /// Model for the agent; None uses its own default.
    pub model: Option<String>,
    /// The executable; None runs the agent's usual command from PATH.
    pub program: Option<String>,
    /// PATH for the child (GUI apps do not inherit the login shell's PATH).
    pub path_env: String,
    /// Scratch dir for the temporary MCP config; also the child's cwd.
    pub workdir: PathBuf,
    pub timeout: Duration,
    /// The user's home (agent sign-ins, uv's cache); None reads `HOME`.
    pub home: Option<PathBuf>,
    /// Claude only: use the claude.ai login, skipping ~/.claude/settings.json
    /// and inherited API keys (they may hold a key that fails headless).
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

impl AgentRunner {
    pub fn info(&self) -> AgentInfo {
        agents::info(self.agent)
    }

    pub async fn run<T: DeserializeOwned>(
        &self,
        prompt: &str,
        schema: &Value,
        mcp: Option<&RedmineMcp>,
        cancel: &AtomicBool,
        on_progress: &mut (dyn FnMut(Progress) + Send),
    ) -> Result<T> {
        let info = self.info();
        let name = info.name;
        let program = self.program.as_deref().unwrap_or(info.program);
        fs::create_dir_all(&self.workdir)?;
        let home = self
            .home
            .clone()
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        let Launch {
            mut cmd,
            stdin: input,
            mut stream,
            fatal,
            temp,
            cwd,
        } = agents::launch(
            self.agent,
            &Options {
                program,
                home: home.as_deref(),
                workdir: &self.workdir,
                model: self
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|m| !m.is_empty()),
                ignore_user_settings: self.ignore_user_settings,
            },
            prompt,
            schema,
            mcp,
        )?;

        cmd.current_dir(cwd.as_deref().unwrap_or(&self.workdir))
            .env("PATH", &self.path_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotInstalled {
                name,
                hint: info.install_hint,
            },
            _ => Error::Io(e),
        })?;

        let mut stdin = child.stdin.take().expect("piped stdin");
        stdin.write_all(input.as_bytes()).await?;
        drop(stdin);

        // Stderr is kept for the error message, and watched for lines that
        // mean the run cannot succeed.
        let refused: Arc<Mutex<Option<String>>> = Arc::default();
        let stderr_task = {
            let refused = refused.clone();
            let mut lines = BufReader::new(child.stderr.take().expect("piped stderr")).lines();
            tokio::spawn(async move {
                let mut all = String::new();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(msg) = fatal.and_then(|f| f(&line)) {
                        refused.lock().unwrap().get_or_insert(msg);
                    }
                    all.push_str(&line);
                    all.push('\n');
                }
                all
            })
        };

        let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
        let deadline = Instant::now() + self.timeout;
        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Some(line) = line? else { break };
                    let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
                    if let Some(p) = stream.event(&event) {
                        on_progress(p);
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    if cancel.load(Ordering::Acquire) {
                        let _ = child.kill().await;
                        self.cleanup(program, stream.cleanup()).await;
                        return Err(Error::Cancelled);
                    }
                    if Instant::now() > deadline {
                        let _ = child.kill().await;
                        self.cleanup(program, stream.cleanup()).await;
                        return Err(Error::Timeout { name, secs: self.timeout.as_secs() });
                    }
                    let msg = refused.lock().unwrap().take();
                    if let Some(msg) = msg {
                        let _ = child.kill().await;
                        self.cleanup(program, stream.cleanup()).await;
                        return Err(Error::Agent { name, msg });
                    }
                }
            }
        }

        let _ = child.wait().await;
        let stderr = stderr_task.await.unwrap_or_default();
        self.cleanup(program, stream.cleanup()).await;
        drop(temp);

        match stream.finish() {
            Some(Outcome::Answer(value)) => {
                serde_json::from_value(value).map_err(|e| Error::BadOutput {
                    name,
                    msg: e.to_string(),
                })
            }
            Some(Outcome::AgentError(msg)) => Err(Error::Agent { name, msg }),
            Some(Outcome::BadOutput(msg)) => Err(Error::BadOutput { name, msg }),
            None => Err(Error::Failed {
                name,
                msg: tail(&stderr),
            }),
        }
    }

    /// Best effort: a follow-up run that tidies up (e.g. deletes the session).
    async fn cleanup(&self, program: &str, args: Option<Vec<String>>) {
        let Some(args) = args else { return };
        let run = Command::new(program)
            .args(&args)
            .current_dir(&self.workdir)
            .env("PATH", &self.path_env)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status();
        let _ = tokio::time::timeout(Duration::from_secs(10), run).await;
    }
}

/// The first JSON object in free text (handles ```json fences).
pub fn extract_json(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

fn tail(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(5);
    let t = lines[start..].join("\n");
    if t.is_empty() {
        "dừng mà không trả kết quả.".into()
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;
    use std::path::Path;

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

    /// A stand-in for the agent's CLI: logs its arguments and stdin, then runs `body`.
    fn fake_agent(dir: &Path, body: &str) -> String {
        let path = dir.join("agent");
        fs::write(
            &path,
            format!("#!/bin/sh\necho \"$@\" >> \"$(dirname \"$0\")/args.log\"\ncat > \"$(dirname \"$0\")/stdin.log\"\n{body}\n"),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        path.to_string_lossy().into_owned()
    }

    fn runner(dir: &Path, agent: AiAgent, program: String, timeout: Duration) -> AgentRunner {
        AgentRunner {
            agent,
            model: None,
            program: Some(program),
            path_env: std::env::var("PATH").unwrap_or_default(),
            workdir: dir.join("work"),
            timeout,
            home: None,
            ignore_user_settings: false,
        }
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ai-bridge-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
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
        let program = fake_agent(
            &dir,
            r#"echo '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__redmine__redmine_request","input":{"path":"/trackers.json"}}]}}'
echo 'not json'
echo '{"type":"result","is_error":false,"result":"","structured_output":{"name":"Bob"}}'"#,
        );
        let mut seen = Vec::new();
        let ans: Answer = runner(&dir, AiAgent::Claude, program, Duration::from_secs(10))
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
    async fn reports_agent_errors() {
        let dir = tmpdir("err");
        let program = fake_agent(
            &dir,
            r#"echo '{"type":"result","is_error":true,"result":"Invalid API key"}'"#,
        );
        let err = runner(&dir, AiAgent::Claude, program, Duration::from_secs(10))
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::Agent { name: "Claude", ref msg } if msg.contains("Invalid API key"))
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn agy_runs_with_its_own_home_and_only_redmine_allowed() {
        let dir = tmpdir("agy");
        let user = dir.join("user");
        fs::create_dir_all(user.join(".gemini/antigravity-cli")).unwrap();
        fs::write(user.join(agents::AGY_TOKEN), "token").unwrap();
        // The throwaway HOME is removed after the run; copy out what it held.
        let program = fake_agent(
            &dir,
            r#"out="$(dirname "$0")"
cp "$HOME/.gemini/antigravity-cli/settings.json" "$out/settings.json"
cp "$HOME/.gemini/config/mcp_config.json" "$out/mcp_config.json"
cat "$HOME/.gemini/antigravity-cli/antigravity-oauth-token" > "$out/token"
echo "$REDMINE_API_KEY" > "$out/key"; pwd > "$out/cwd"
echo '{"event":"step_update","step_update":{"step_type":"tool","state":"ACTIVE","tool_name":"call_mcp_tool","tool_info":{"parameters":{"ToolName":"redmine_request","Arguments":{"path":"/trackers.json"}}}}}'
echo '{"event":"result","result":{"status":"SUCCESS","response":"","structured_output":{"name":"Bob"}}}'"#,
        );
        let mut r = runner(&dir, AiAgent::Agy, program, Duration::from_secs(10));
        r.home = Some(user.clone());
        r.model = Some("gemini-3.8-flash".into());
        let mut seen = Vec::new();
        let ans: Answer = r
            .run(
                "hi",
                &json!({"type": "object"}),
                Some(&mcp()),
                &AtomicBool::new(false),
                &mut |p| seen.push(p.text),
            )
            .await
            .unwrap();
        assert_eq!(ans.name, "Bob");
        assert_eq!(seen, vec!["GET /trackers.json"]);
        let args = fs::read_to_string(dir.join("args.log")).unwrap();
        assert!(
            args.contains("--input-format stream-json --output-format stream-json"),
            "{args}"
        );
        assert!(args.contains("--model gemini-3.8-flash"), "{args}");
        let stdin: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("stdin.log")).unwrap()).unwrap();
        assert_eq!(stdin["event"], "user");
        assert_eq!(stdin["message"]["content"][0]["text"], "hi");
        let read = |f: &str| fs::read_to_string(dir.join(f)).unwrap();
        let settings: Value = serde_json::from_str(&read("settings.json")).unwrap();
        assert_eq!(settings["toolPermission"], "request-review");
        assert_eq!(
            settings["permissions"]["allow"][0],
            "mcp(redmine/redmine_request)"
        );
        // The key reaches the server through the environment, never a file.
        let config = read("mcp_config.json");
        assert!(!config.contains("secret"), "{config}");
        assert_eq!(read("key").trim(), "secret");
        assert_eq!(read("token"), "token");
        assert!(read("cwd").trim().ends_with("/work"));
        // The throwaway HOME is gone and the user's token is untouched.
        assert_eq!(fs::read_dir(dir.join("work")).unwrap().count(), 0);
        assert_eq!(
            fs::read_to_string(user.join(agents::AGY_TOKEN)).unwrap(),
            "token"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn agy_without_a_sign_in_says_so() {
        let dir = tmpdir("agy-login");
        let mut r = runner(
            &dir,
            AiAgent::Agy,
            fake_agent(&dir, "exit 1"),
            Duration::from_secs(5),
        );
        r.home = Some(dir.join("nobody"));
        let err = r
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::Agent { name: "Antigravity", ref msg } if msg.contains("đăng nhập"))
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn opencode_runs_isolated_and_deletes_its_session() {
        let dir = tmpdir("opencode");
        let program = fake_agent(
            &dir,
            r#"[ "$1" = session ] && exit 0
cp "$XDG_CONFIG_HOME/opencode/opencode.json" "$(dirname "$0")/config.json"
echo '{"type":"tool_use","sessionID":"ses_9","part":{"tool":"redmine_redmine_request","state":{"input":{"path":"/trackers.json"}}}}'
echo '{"type":"text","sessionID":"ses_9","part":{"id":"p1","text":"{\"name\": \"Bob\"}"}}'"#,
        );
        let mut seen = Vec::new();
        let ans: Answer = runner(&dir, AiAgent::Opencode, program, Duration::from_secs(10))
            .run(
                "hi",
                &json!({}),
                Some(&mcp()),
                &AtomicBool::new(false),
                &mut |p| seen.push(p.text),
            )
            .await
            .unwrap();
        assert_eq!(ans.name, "Bob");
        assert_eq!(seen, vec!["GET /trackers.json"]);
        let args = fs::read_to_string(dir.join("args.log")).unwrap();
        assert!(args.lines().any(|l| l == "session delete ses_9"), "{args}");
        let config: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("config.json")).unwrap()).unwrap();
        assert_eq!(config["tools"]["bash"], false);
        assert_eq!(
            config["mcp"]["redmine"]["environment"]["REDMINE_READ_ONLY"],
            "1"
        );
        assert_eq!(fs::read_dir(dir.join("work")).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn opencode_stops_when_the_model_call_is_refused() {
        let dir = tmpdir("refused");
        let program = fake_agent(
            &dir,
            r#"[ "$1" = session ] && exit 0
echo 'level=ERROR message="stream error" error.error="Free usage limit reached."' >&2
sleep 5"#,
        );
        let started = Instant::now();
        let err = runner(&dir, AiAgent::Opencode, program, Duration::from_secs(10))
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(
            matches!(err, Error::Agent { name: "opencode", ref msg } if msg == "Free usage limit reached.")
        );
        assert!(started.elapsed() < Duration::from_secs(4));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn cancels_and_times_out() {
        let dir = tmpdir("slow");
        let program = fake_agent(&dir, "sleep 5");
        let r = runner(
            &dir,
            AiAgent::Claude,
            program.clone(),
            Duration::from_secs(10),
        );
        let err = r
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(true), &mut |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Cancelled));

        let r = runner(&dir, AiAgent::Claude, program, Duration::from_millis(300));
        let err = r
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Timeout { .. }));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn missing_binary_is_reported() {
        let dir = tmpdir("missing");
        // Signed in, so the launch gets as far as spawning the program.
        let user = dir.join("user");
        fs::create_dir_all(user.join(agents::AGY_TOKEN).parent().unwrap()).unwrap();
        fs::write(user.join(agents::AGY_TOKEN), "token").unwrap();
        let mut r = runner(
            &dir,
            AiAgent::Agy,
            "/nonexistent/agy".into(),
            Duration::from_secs(1),
        );
        r.home = Some(user);
        let err = r
            .run::<Answer>("hi", &json!({}), None, &AtomicBool::new(false), &mut |_| {})
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            Error::NotInstalled {
                name: "Antigravity",
                ..
            }
        ));
        fs::remove_dir_all(&dir).unwrap();
    }
}
