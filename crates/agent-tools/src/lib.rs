//! `agent-tools` — executable tool runtimes and registry.
//!
//! v0 ships the [`Tool`] trait, an in-memory [`ToolRegistry`], and a
//! deterministic [`FakeTool`] for tests/demos. Sandboxing, permissions, and
//! the wider runtime variants (`process`, `wasm`, `mcp`, `subagent`, …) from
//! `specs/architecture.md` §4.1 land in later slices.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::time::{Duration, timeout};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolId(pub String);

impl<S: Into<String>> From<S> for ToolId {
    fn from(s: S) -> Self {
        Self(s.into())
    }
}

/// v0 cut of `ToolDescriptor` from `specs/architecture.md` §4.1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub id: ToolId,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub permissions: ToolPermissions,
    #[serde(default)]
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolPermissions {
    pub shell: bool,
    pub file_read: bool,
    pub file_write: bool,
    pub network: bool,
    pub secrets: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("tool not found: {0:?}")]
    NotFound(ToolId),
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone)]
pub struct ShellToolConfig {
    pub default_timeout_ms: u64,
    pub max_output_bytes: usize,
    pub default_cwd: Option<PathBuf>,
}

impl Default for ShellToolConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30_000,
            max_output_bytes: 64 * 1024,
            default_cwd: None,
        }
    }
}

/// Shell-string runner for Stage 1. It is intentionally not registered by
/// default; callers must explicitly install it in a registry.
pub struct ShellTool {
    config: ShellToolConfig,
}

impl ShellTool {
    pub fn new(config: ShellToolConfig) -> Self {
        Self { config }
    }

    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("shell"),
            name: "Shell".into(),
            description: "Runs a shell command with timeout and captured stdout/stderr.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command string. Uses /bin/sh -c on Unix and cmd /C on Windows."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Optional working directory."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional timeout override."
                    },
                    "max_output_bytes": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional stdout/stderr truncation limit per stream."
                    }
                }
            }),
            permissions: ToolPermissions {
                shell: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
        }
    }
}

#[async_trait]
impl Tool for ShellTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let command = input
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidInput("missing string field `command`".into()))?;
        let timeout_ms = input
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(self.config.default_timeout_ms);
        let max_output_bytes = input
            .get("max_output_bytes")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(self.config.max_output_bytes);
        let cwd = input
            .get("cwd")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .or_else(|| self.config.default_cwd.clone());

        let mut cmd = shell_command(command);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let started = Instant::now();
        let child = cmd
            .spawn()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let output =
            match timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
                Ok(result) => result.map_err(|e| ToolError::Execution(e.to_string()))?,
                Err(_) => {
                    return Ok(json!({
                        "status": "timeout",
                        "exit_code": null,
                        "stdout": "",
                        "stderr": "",
                        "duration_ms": started.elapsed().as_millis() as u64,
                        "timed_out": true,
                        "truncated_stdout": false,
                        "truncated_stderr": false
                    }));
                }
            };

        let (stdout, truncated_stdout) = decode_and_truncate(&output.stdout, max_output_bytes);
        let (stderr, truncated_stderr) = decode_and_truncate(&output.stderr, max_output_bytes);
        let exit_code = output.status.code();
        let status = if output.status.success() {
            "success"
        } else {
            "exit"
        };

        Ok(json!({
            "status": status,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "duration_ms": started.elapsed().as_millis() as u64,
            "timed_out": false,
            "truncated_stdout": truncated_stdout,
            "truncated_stderr": truncated_stderr
        }))
    }
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg(command);
        cmd
    }
}

fn decode_and_truncate(bytes: &[u8], max_bytes: usize) -> (String, bool) {
    let truncated = bytes.len() > max_bytes;
    let end = if truncated { max_bytes } else { bytes.len() };
    let text = String::from_utf8_lossy(&bytes[..end]).to_string();
    (text, truncated)
}

#[async_trait]
pub trait Tool: Send + Sync {
    async fn execute(&self, input: Value) -> Result<Value, ToolError>;
}

/// In-memory tool registry. Pluggable runtimes (process, wasm, MCP, hermes,
/// openclaw) land later as additional `Tool` implementations registered here.
pub struct ToolRegistry {
    tools: HashMap<ToolId, Entry>,
}

struct Entry {
    descriptor: ToolDescriptor,
    runner: Arc<dyn Tool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, descriptor: ToolDescriptor, runner: Arc<dyn Tool>) {
        self.tools
            .insert(descriptor.id.clone(), Entry { descriptor, runner });
    }

    pub fn descriptor(&self, id: &ToolId) -> Option<&ToolDescriptor> {
        self.tools.get(id).map(|e| &e.descriptor)
    }

    pub fn descriptors(&self) -> impl Iterator<Item = &ToolDescriptor> {
        self.tools.values().map(|e| &e.descriptor)
    }

    pub fn contains(&self, id: &ToolId) -> bool {
        self.tools.contains_key(id)
    }

    pub async fn execute(&self, id: &ToolId, input: Value) -> Result<Value, ToolError> {
        let entry = self
            .tools
            .get(id)
            .ok_or_else(|| ToolError::NotFound(id.clone()))?;
        entry.runner.execute(input).await
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Deterministic fake tool. Three modes:
/// - [`FakeTool::echo`] returns its input unchanged.
/// - [`FakeTool::constant`] returns a fixed JSON value.
/// - [`FakeTool::failing`] always returns [`ToolError::Execution`].
pub struct FakeTool {
    behavior: Behavior,
}

enum Behavior {
    Echo,
    Constant(Value),
    Failing(String),
}

impl FakeTool {
    pub fn echo_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("echo"),
            name: "Echo".into(),
            description: "Returns text input unchanged.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to echo."
                    }
                },
                "additionalProperties": false
            }),
            permissions: ToolPermissions::default(),
            requires_approval: false,
        }
    }

    pub fn echo() -> Self {
        Self {
            behavior: Behavior::Echo,
        }
    }
    pub fn constant(value: Value) -> Self {
        Self {
            behavior: Behavior::Constant(value),
        }
    }
    pub fn failing(msg: impl Into<String>) -> Self {
        Self {
            behavior: Behavior::Failing(msg.into()),
        }
    }
}

#[async_trait]
impl Tool for FakeTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match &self.behavior {
            Behavior::Echo => Ok(input),
            Behavior::Constant(v) => Ok(v.clone()),
            Behavior::Failing(msg) => Err(ToolError::Execution(msg.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn descriptor(id: &str) -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from(id),
            name: id.into(),
            description: "test".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                }
            }),
            permissions: ToolPermissions::default(),
            requires_approval: false,
        }
    }

    #[tokio::test]
    async fn registry_register_and_execute_echo() {
        let mut reg = ToolRegistry::new();
        reg.register(FakeTool::echo_descriptor(), Arc::new(FakeTool::echo()));

        let out = reg
            .execute(&ToolId::from("echo"), json!({"hi": "there"}))
            .await
            .unwrap();
        assert_eq!(out, json!({"hi": "there"}));
    }

    #[test]
    fn echo_descriptor_is_openai_compatible_object_schema() {
        let schema = FakeTool::echo_descriptor().input_schema;
        assert_eq!(schema["type"], "object");
        assert!(
            schema
                .get("properties")
                .and_then(Value::as_object)
                .is_some()
        );
        assert!(schema["properties"].get("text").is_some());
    }

    #[tokio::test]
    async fn registry_returns_not_found_for_unknown() {
        let reg = ToolRegistry::new();
        let err = reg
            .execute(&ToolId::from("nope"), json!({}))
            .await
            .unwrap_err();
        matches!(err, ToolError::NotFound(_));
    }

    #[tokio::test]
    async fn failing_tool_propagates_error() {
        let mut reg = ToolRegistry::new();
        reg.register(
            descriptor("bad"),
            Arc::new(FakeTool::failing("simulated failure")),
        );
        let err = reg
            .execute(&ToolId::from("bad"), json!({}))
            .await
            .unwrap_err();
        match err {
            ToolError::Execution(msg) => assert_eq!(msg, "simulated failure"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn descriptors_iterator_returns_registered_tools() {
        let mut reg = ToolRegistry::new();
        reg.register(descriptor("a"), Arc::new(FakeTool::echo()));
        reg.register(descriptor("b"), Arc::new(FakeTool::echo()));

        let mut ids: Vec<&str> = reg.descriptors().map(|d| d.id.0.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn shell_tool_captures_success_output() {
        let tool = ShellTool::new(ShellToolConfig::default());
        let out = tool
            .execute(json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert_eq!(out["status"], "success");
        assert_eq!(out["exit_code"], 0);
        assert_eq!(out["stdout"].as_str().unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn shell_tool_captures_non_zero_exit() {
        let tool = ShellTool::new(ShellToolConfig::default());
        let command = if cfg!(windows) {
            "echo nope 1>&2 & exit /b 7"
        } else {
            "printf nope >&2; exit 7"
        };
        let out = tool.execute(json!({"command": command})).await.unwrap();
        assert_eq!(out["status"], "exit");
        assert_eq!(out["exit_code"], 7);
        assert_eq!(out["stderr"].as_str().unwrap().trim(), "nope");
    }

    #[tokio::test]
    async fn shell_tool_truncates_streams() {
        let tool = ShellTool::new(ShellToolConfig::default());
        let out = tool
            .execute(json!({
                "command": "echo 123456",
                "max_output_bytes": 3
            }))
            .await
            .unwrap();
        assert_eq!(out["stdout"], "123");
        assert_eq!(out["truncated_stdout"], true);
    }

    #[tokio::test]
    async fn shell_tool_times_out() {
        let tool = ShellTool::new(ShellToolConfig::default());
        let command = if cfg!(windows) {
            "ping -n 2 127.0.0.1 > nul"
        } else {
            "sleep 1"
        };
        let out = tool
            .execute(json!({
                "command": command,
                "timeout_ms": 1
            }))
            .await
            .unwrap();
        assert_eq!(out["status"], "timeout");
        assert_eq!(out["timed_out"], true);
    }
}
