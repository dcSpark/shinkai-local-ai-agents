//! `agent-tools` — executable tool runtimes and registry.
//!
//! v0 ships the [`Tool`] trait, an in-memory [`ToolRegistry`], and a
//! deterministic [`FakeTool`] for tests/demos. It also includes native shell
//! execution, stdio/HTTP MCP wrappers, secret-handle env/header resolution, and
//! minimal process environment scrubbing. Stronger OS isolation and future
//! runtime variants such as Wasm continue to land in later slices.

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

use agent_adapters::{
    AdapterKind, AdapterRegistry, CapabilityKind, NormalizedCapability, NormalizedPackage,
};
use agent_secrets::{SecretHandle, SecretId, SecretStore, SecretStoreError, default_secret_store};
use agent_storage::StoragePaths;
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    pub input_schema: Value,
    #[serde(default)]
    pub output_interpretation_guidance: Option<String>,
    #[serde(default)]
    pub permissions: ToolPermissions,
    #[serde(default)]
    pub requires_approval: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolPermissions {
    pub shell: bool,
    #[serde(default)]
    pub shell_restricted: bool,
    pub file_read: bool,
    pub file_write: bool,
    pub network: bool,
    pub secrets: bool,
    #[serde(default)]
    pub wallet: bool,
    #[serde(default)]
    pub payment: bool,
    #[serde(default)]
    pub browser_profile: bool,
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
    pub allowed_commands: Vec<String>,
    pub sandbox: Option<CodeSandboxConfig>,
    pub sandbox_required: bool,
}

impl Default for ShellToolConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30_000,
            max_output_bytes: 64 * 1024,
            default_cwd: None,
            allowed_commands: Vec::new(),
            sandbox: None,
            sandbox_required: false,
        }
    }
}

impl ShellToolConfig {
    pub fn from_env() -> Self {
        let allowed_commands = std::env::var("AGENT_SHELL_ALLOWLIST")
            .ok()
            .into_iter()
            .flat_map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect();
        Self {
            allowed_commands,
            sandbox: CodeSandboxConfig::shell_from_env(),
            sandbox_required: env_flag("AGENT_SHELL_SANDBOX_REQUIRED"),
            ..Self::default()
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
        Self::descriptor_for_config(&ShellToolConfig::default())
    }

    pub fn descriptor_for_config(config: &ShellToolConfig) -> ToolDescriptor {
        let sandbox_posture = if config.sandbox_required {
            "a configured sandbox wrapper is required"
        } else if config.sandbox.is_some() {
            "a configured sandbox wrapper"
        } else {
            "a minimal inherited environment"
        };
        ToolDescriptor {
            id: ToolId::from("shell"),
            name: "Shell".into(),
            description: if config.allowed_commands.is_empty() {
                format!(
                    "Runs a shell command with timeout, captured stdout/stderr, and {sandbox_posture}."
                )
            } else {
                format!(
                    "Runs a restricted command from the allowlist with {sandbox_posture}: {}.",
                    config.allowed_commands.join(", "),
                )
            },
            categories: vec!["system".into(), "shell".into()],
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
            output_interpretation_guidance: Some(
                "Preserve stdout, stderr, exit status, timeout, and truncation flags exactly when interpreting shell results.".into(),
            ),
            permissions: ToolPermissions {
                shell: true,
                shell_restricted: !config.allowed_commands.is_empty(),
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: None,
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
        if self.config.sandbox_required && self.config.sandbox.is_none() {
            return Err(ToolError::Execution(
                "shell sandbox is required but neither AGENT_SHELL_SANDBOX_COMMAND nor an available AGENT_SHELL_SANDBOX_AUTO wrapper is configured".into(),
            ));
        }

        let run_cwd = cwd
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let sandbox_label = self
            .config
            .sandbox
            .as_ref()
            .map(|sandbox| sandbox.label.clone())
            .unwrap_or_else(|| "minimal_env".into());
        let sandbox_command = self
            .config
            .sandbox
            .as_ref()
            .map(|sandbox| sandbox.command.clone());
        let (program, args) = if self.config.allowed_commands.is_empty() {
            shell_program_args(command)
        } else {
            restricted_program_args(command, &self.config.allowed_commands)?
        };
        let mut cmd = if let Some(sandbox) = &self.config.sandbox {
            let mut cmd = Command::new(&sandbox.command);
            cmd.args(sandbox.args_for_context(&run_cwd, &std::env::temp_dir()));
            cmd.arg(program);
            cmd.args(args);
            cmd
        } else {
            let mut cmd = Command::new(program);
            cmd.args(args);
            cmd
        };
        apply_minimal_env(&mut cmd);
        cmd.current_dir(&run_cwd);
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
                        "truncated_stderr": false,
                        "sandbox": sandbox_label,
                        "sandbox_command": sandbox_command
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
            "truncated_stderr": truncated_stderr,
            "sandbox": sandbox_label,
            "sandbox_command": sandbox_command
        }))
    }
}

#[derive(Debug, Clone)]
pub struct CodeExecutionConfig {
    pub default_timeout_ms: u64,
    pub max_output_bytes: usize,
    pub default_cwd: Option<PathBuf>,
    pub python_command: String,
    pub typescript_command: String,
    pub sandbox: Option<CodeSandboxConfig>,
    pub sandbox_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSandboxConfig {
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
}

impl CodeSandboxConfig {
    fn from_env() -> Option<Self> {
        Self::from_env_vars(
            "AGENT_CODE_SANDBOX_COMMAND",
            "AGENT_CODE_SANDBOX_ARGS_JSON",
            "AGENT_CODE_SANDBOX_LABEL",
            "AGENT_CODE_SANDBOX_AUTO",
        )
    }

    fn shell_from_env() -> Option<Self> {
        Self::from_env_vars(
            "AGENT_SHELL_SANDBOX_COMMAND",
            "AGENT_SHELL_SANDBOX_ARGS_JSON",
            "AGENT_SHELL_SANDBOX_LABEL",
            "AGENT_SHELL_SANDBOX_AUTO",
        )
    }

    fn from_env_vars(
        command_var: &str,
        args_var: &str,
        label_var: &str,
        auto_var: &str,
    ) -> Option<Self> {
        let Some(command) = std::env::var(command_var).ok().and_then(clean_non_empty) else {
            return env_flag(auto_var).then(|| {
                builtin_code_sandbox_for_platform(std::env::consts::OS, command_in_path)
            })?;
        };
        let args = std::env::var(args_var)
            .ok()
            .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
            .unwrap_or_default();
        let label = std::env::var(label_var)
            .ok()
            .and_then(clean_non_empty)
            .unwrap_or_else(|| {
                Path::new(&command)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("configured_sandbox")
                    .to_string()
            });
        Some(Self {
            label,
            command,
            args,
        })
    }

    fn args_for_context(&self, cwd: &Path, temp_dir: &Path) -> Vec<OsString> {
        let cwd = cwd.to_string_lossy();
        let temp_dir = temp_dir.to_string_lossy();
        let args = self
            .args
            .iter()
            .map(|arg| {
                OsString::from(
                    arg.replace("{cwd}", cwd.as_ref())
                        .replace("{temp_dir}", temp_dir.as_ref()),
                )
            })
            .collect::<Vec<_>>();
        dedupe_repeated_bind_args(args)
    }
}

fn dedupe_repeated_bind_args(args: Vec<OsString>) -> Vec<OsString> {
    let mut deduped = Vec::with_capacity(args.len());
    let mut seen_binds = Vec::<(OsString, OsString)>::new();
    let mut index = 0;
    while index < args.len() {
        if args[index].as_os_str() == "--bind" && index + 2 < args.len() {
            let key = (args[index + 1].clone(), args[index + 2].clone());
            if seen_binds.contains(&key) {
                index += 3;
                continue;
            }
            seen_binds.push(key);
            deduped.extend_from_slice(&args[index..index + 3]);
            index += 3;
            continue;
        }
        deduped.push(args[index].clone());
        index += 1;
    }
    deduped
}

fn builtin_code_sandbox_for_platform(
    platform: &str,
    command_available: impl Fn(&str) -> bool,
) -> Option<CodeSandboxConfig> {
    match platform {
        "macos" if command_available("sandbox-exec") => Some(CodeSandboxConfig {
            label: "macos_sandbox_exec_auto".into(),
            command: "sandbox-exec".into(),
            args: vec![
                "-p".into(),
                [
                    "(version 1)",
                    "(deny default)",
                    "(allow process*)",
                    "(allow signal*)",
                    "(allow sysctl-read)",
                    "(allow file-read*)",
                    "(allow file-write* (subpath \"{temp_dir}\") (subpath \"{cwd}\"))",
                    "(allow network*)",
                ]
                .join("\n"),
            ],
        }),
        "linux" if command_available("bwrap") => Some(CodeSandboxConfig {
            label: "linux_bubblewrap_auto".into(),
            command: "bwrap".into(),
            args: vec![
                "--die-with-parent".into(),
                "--new-session".into(),
                "--dev".into(),
                "/dev".into(),
                "--proc".into(),
                "/proc".into(),
                "--ro-bind".into(),
                "/".into(),
                "/".into(),
                "--tmpfs".into(),
                "/tmp".into(),
                "--bind".into(),
                "{temp_dir}".into(),
                "{temp_dir}".into(),
                "--bind".into(),
                "{cwd}".into(),
                "{cwd}".into(),
                "--chdir".into(),
                "{cwd}".into(),
                "--".into(),
            ],
        }),
        _ => None,
    }
}

fn command_in_path(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|dir| dir.join(command).is_file())
}

impl CodeExecutionConfig {
    pub fn from_shell_config(config: &ShellToolConfig) -> Self {
        Self {
            default_timeout_ms: config.default_timeout_ms,
            max_output_bytes: config.max_output_bytes,
            default_cwd: config.default_cwd.clone(),
            python_command: std::env::var("AGENT_PYTHON_COMMAND")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(default_python_command),
            typescript_command: std::env::var("AGENT_TYPESCRIPT_COMMAND")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "deno".into()),
            sandbox: CodeSandboxConfig::from_env(),
            sandbox_required: env_flag("AGENT_CODE_SANDBOX_REQUIRED"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum CodeLanguage {
    Python,
    TypeScript,
}

impl CodeLanguage {
    fn id(self) -> &'static str {
        match self {
            Self::Python => "code_python",
            Self::TypeScript => "code_typescript",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Python => "Python Code",
            Self::TypeScript => "TypeScript Code",
        }
    }

    fn category(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::TypeScript => "typescript",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Python => "py",
            Self::TypeScript => "ts",
        }
    }
}

pub struct CodeExecutionTool {
    language: CodeLanguage,
    config: CodeExecutionConfig,
}

impl CodeExecutionTool {
    fn new(language: CodeLanguage, config: CodeExecutionConfig) -> Self {
        Self { language, config }
    }

    pub fn python(config: CodeExecutionConfig) -> Self {
        Self::new(CodeLanguage::Python, config)
    }

    pub fn typescript(config: CodeExecutionConfig) -> Self {
        Self::new(CodeLanguage::TypeScript, config)
    }

    fn descriptor(language: CodeLanguage, sandbox_required: bool) -> ToolDescriptor {
        let code_kind = language.category();
        let sandbox_posture = if sandbox_required {
            "a configured sandbox wrapper is required"
        } else {
            "an optional configured sandbox wrapper"
        };
        ToolDescriptor {
            id: ToolId::from(language.id()),
            name: language.name().into(),
            description: format!(
                "Runs a {code_kind} snippet from a temporary file with timeout, captured stdout/stderr, a minimal inherited environment, and {sandbox_posture}."
            ),
            categories: vec!["code".into(), code_kind.into(), "shell".into()],
            input_schema: json!({
                "type": "object",
                "required": ["code"],
                "properties": {
                    "code": {
                        "type": "string",
                        "description": format!("{code_kind} source code to execute.")
                    },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional positional arguments passed after the temporary source file."
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Optional working directory. Defaults to the temporary source directory."
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
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Preserve language, runner, stdout, stderr, exit status, timeout, and truncation flags exactly when interpreting code execution results.".into(),
            ),
            permissions: ToolPermissions {
                shell: true,
                file_read: true,
                file_write: true,
                network: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some(format!("native:{}", language.id())),
        }
    }

    pub fn python_descriptor() -> ToolDescriptor {
        Self::descriptor(CodeLanguage::Python, false)
    }

    pub fn typescript_descriptor() -> ToolDescriptor {
        Self::descriptor(CodeLanguage::TypeScript, false)
    }

    fn descriptor_for_config(
        language: CodeLanguage,
        config: &CodeExecutionConfig,
    ) -> ToolDescriptor {
        Self::descriptor(language, config.sandbox_required)
    }

    fn runner_command(&self) -> &str {
        match self.language {
            CodeLanguage::Python => &self.config.python_command,
            CodeLanguage::TypeScript => &self.config.typescript_command,
        }
    }
}

#[async_trait]
impl Tool for CodeExecutionTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let code = input
            .get("code")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::InvalidInput("missing string field `code`".into()))?;
        let args = optional_string_array(input.get("args"), "args")?;
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
        if self.config.sandbox_required && self.config.sandbox.is_none() {
            return Err(ToolError::Execution(
                "code sandbox is required but neither AGENT_CODE_SANDBOX_COMMAND nor an available AGENT_CODE_SANDBOX_AUTO wrapper is configured".into(),
            ));
        }

        let temp_dir = std::env::temp_dir().join(format!(
            "agent-code-{}-{}",
            self.language.category(),
            unique_temp_suffix()
        ));
        std::fs::create_dir_all(&temp_dir).map_err(|e| ToolError::Execution(e.to_string()))?;
        let script_path = temp_dir.join(format!("snippet.{}", self.language.extension()));
        if let Err(error) = std::fs::write(&script_path, code) {
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err(ToolError::Execution(error.to_string()));
        }

        let mut runner_args = Vec::<OsString>::new();
        match self.language {
            CodeLanguage::Python => {
                runner_args.push(script_path.clone().into_os_string());
            }
            CodeLanguage::TypeScript => {
                runner_args.push("run".into());
                runner_args.push("--quiet".into());
                runner_args.push("--no-prompt".into());
                runner_args.push(script_path.clone().into_os_string());
            }
        }
        runner_args.extend(args.into_iter().map(OsString::from));
        let run_cwd = cwd.unwrap_or_else(|| temp_dir.clone());
        let sandbox_label = self
            .config
            .sandbox
            .as_ref()
            .map(|sandbox| sandbox.label.clone())
            .unwrap_or_else(|| "temporary_cwd_minimal_env".into());
        let sandbox_command = self
            .config
            .sandbox
            .as_ref()
            .map(|sandbox| sandbox.command.clone());
        let mut cmd = if let Some(sandbox) = &self.config.sandbox {
            let mut cmd = Command::new(&sandbox.command);
            cmd.args(sandbox.args_for_context(&run_cwd, &temp_dir));
            cmd.arg(self.runner_command());
            cmd.args(&runner_args);
            cmd
        } else {
            let mut cmd = Command::new(self.runner_command());
            cmd.args(&runner_args);
            cmd
        };
        apply_minimal_env(&mut cmd);
        cmd.current_dir(&run_cwd);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let started = Instant::now();
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&temp_dir);
                return Err(ToolError::Execution(error.to_string()));
            }
        };
        let output =
            match timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
                Ok(result) => result.map_err(|e| ToolError::Execution(e.to_string()))?,
                Err(_) => {
                    let _ = std::fs::remove_dir_all(&temp_dir);
                    return Ok(json!({
                        "language": self.language.category(),
                        "runner": self.runner_command(),
                        "status": "timeout",
                        "exit_code": null,
                        "stdout": "",
                        "stderr": "",
                        "duration_ms": started.elapsed().as_millis() as u64,
                        "timed_out": true,
                        "truncated_stdout": false,
                        "truncated_stderr": false,
                        "sandbox": sandbox_label,
                        "sandbox_command": sandbox_command
                    }));
                }
            };

        let _ = std::fs::remove_dir_all(&temp_dir);
        let (stdout, truncated_stdout) = decode_and_truncate(&output.stdout, max_output_bytes);
        let (stderr, truncated_stderr) = decode_and_truncate(&output.stderr, max_output_bytes);
        let exit_code = output.status.code();
        let status = if output.status.success() {
            "success"
        } else {
            "exit"
        };

        Ok(json!({
            "language": self.language.category(),
            "runner": self.runner_command(),
            "status": status,
            "exit_code": exit_code,
            "stdout": stdout,
            "stderr": stderr,
            "duration_ms": started.elapsed().as_millis() as u64,
            "timed_out": false,
            "truncated_stdout": truncated_stdout,
            "truncated_stderr": truncated_stderr,
            "sandbox": sandbox_label,
            "sandbox_command": sandbox_command
        }))
    }
}

pub fn register_code_execution_tools(
    registry: &mut ToolRegistry,
    shell_config: &ShellToolConfig,
) -> usize {
    let config = CodeExecutionConfig::from_shell_config(shell_config);
    registry.register(
        CodeExecutionTool::descriptor_for_config(CodeLanguage::Python, &config),
        Arc::new(CodeExecutionTool::python(config.clone())),
    );
    registry.register(
        CodeExecutionTool::descriptor_for_config(CodeLanguage::TypeScript, &config),
        Arc::new(CodeExecutionTool::typescript(config)),
    );
    2
}

pub fn is_shell_runtime_tool_id(id: &str) -> bool {
    matches!(id, "shell" | "code_python" | "code_typescript")
}

#[derive(Debug, Clone)]
pub struct PaymentX402Config {
    pub default_timeout_ms: u64,
    pub max_response_bytes: usize,
    pub max_amount: Option<f64>,
    pub signature_env: String,
    pub signature_secret: Option<String>,
    pub facilitator_url: Option<String>,
    pub wallet_command: Option<PaymentWalletCommandConfig>,
}

#[derive(Debug, Clone)]
pub struct PaymentWalletCommandConfig {
    pub command: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

impl PaymentX402Config {
    pub fn from_env() -> Self {
        let default_timeout_ms = env_u64("AGENT_PAYMENT_TIMEOUT_MS").unwrap_or(30_000);
        Self {
            default_timeout_ms,
            max_response_bytes: env_usize("AGENT_PAYMENT_MAX_RESPONSE_BYTES").unwrap_or(64 * 1024),
            max_amount: std::env::var("AGENT_PAYMENT_MAX_AMOUNT")
                .ok()
                .and_then(|value| value.trim().parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 0.0),
            signature_env: std::env::var("AGENT_X402_SIGNATURE_ENV")
                .ok()
                .and_then(clean_non_empty)
                .unwrap_or_else(|| "AGENT_X402_PAYMENT_SIGNATURE".into()),
            signature_secret: std::env::var("AGENT_X402_SIGNATURE_SECRET")
                .ok()
                .and_then(clean_non_empty),
            facilitator_url: std::env::var("AGENT_X402_FACILITATOR_URL")
                .ok()
                .and_then(clean_non_empty),
            wallet_command: payment_wallet_command_from_env(default_timeout_ms),
        }
    }
}

fn payment_wallet_command_from_env(default_timeout_ms: u64) -> Option<PaymentWalletCommandConfig> {
    let command = std::env::var("AGENT_X402_WALLET_COMMAND")
        .ok()
        .and_then(clean_non_empty)?;
    let args = std::env::var("AGENT_X402_WALLET_ARGS_JSON")
        .ok()
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .unwrap_or_default();
    let timeout_ms = env_u64("AGENT_X402_WALLET_TIMEOUT_MS").unwrap_or(default_timeout_ms);
    Some(PaymentWalletCommandConfig {
        command,
        args,
        timeout_ms,
    })
}

pub struct PaymentX402Tool {
    config: PaymentX402Config,
    secret_store: Arc<dyn SecretStore>,
}

impl PaymentX402Tool {
    pub fn new(config: PaymentX402Config) -> Self {
        Self {
            config,
            secret_store: default_secret_store(),
        }
    }

    pub fn new_with_secret_store(
        config: PaymentX402Config,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            config,
            secret_store,
        }
    }

    pub fn from_env() -> Self {
        Self::new(PaymentX402Config::from_env())
    }

    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("payment_x402_request"),
            name: "x402 Payment Request".into(),
            description: "Probes an x402 HTTP endpoint, parses PAYMENT-REQUIRED instructions, and can retry with a configured PAYMENT-SIGNATURE under an explicit spend limit.".into(),
            categories: vec!["payment".into(), "wallet".into(), "network".into(), "x402".into()],
            input_schema: json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "HTTP or HTTPS resource URL to request."
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST"],
                        "description": "HTTP method. Defaults to GET."
                    },
                    "headers": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                        "description": "Optional non-payment request headers."
                    },
                    "body": {
                        "type": "string",
                        "description": "Optional request body for POST."
                    },
                    "auto_pay": {
                        "type": "boolean",
                        "description": "Retry with PAYMENT-SIGNATURE after a 402 response. Requires a payment signature, secret, env signature, or configured wallet command plus max_amount."
                    },
                    "payment_signature": {
                        "type": "string",
                        "description": "Base64 x402 PAYMENT-SIGNATURE payload. Prefer payment_signature_secret for stored credentials."
                    },
                    "payment_signature_secret": {
                        "type": "string",
                        "description": "secret:// handle or active secret id containing the PAYMENT-SIGNATURE payload. Overrides AGENT_X402_SIGNATURE_SECRET."
                    },
                    "max_amount": {
                        "type": "number",
                        "minimum": 0,
                        "description": "Raw protocol amount ceiling for retrying payment. Overrides AGENT_PAYMENT_MAX_AMOUNT."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1
                    },
                    "max_response_bytes": {
                        "type": "integer",
                        "minimum": 1
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Report HTTP status, parsed PAYMENT-REQUIRED/PAYMENT-RESPONSE objects, spend-limit decision, retry status, and response truncation exactly."
                    .into(),
            ),
            permissions: ToolPermissions {
                network: true,
                wallet: true,
                payment: true,
                secrets: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("native:payment_x402_request".into()),
        }
    }
}

#[async_trait]
impl Tool for PaymentX402Tool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let url = input
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::InvalidInput("missing string field `url`".into()))?;
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(ToolError::InvalidInput(
                "payment URL must start with http:// or https://".into(),
            ));
        }
        let method = input
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_ascii_uppercase)
            .unwrap_or_else(|| "GET".into());
        if !matches!(method.as_str(), "GET" | "POST") {
            return Err(ToolError::InvalidInput(
                "payment method must be GET or POST".into(),
            ));
        }
        let headers = optional_header_map(input.get("headers"))?;
        let body = input
            .get("body")
            .and_then(Value::as_str)
            .map(str::to_string);
        let auto_pay = input
            .get("auto_pay")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let timeout_ms = input
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(self.config.default_timeout_ms);
        let max_response_bytes = input
            .get("max_response_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(self.config.max_response_bytes);
        let max_amount = input
            .get("max_amount")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .or(self.config.max_amount);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let first = send_payment_request(
            &client,
            &method,
            url,
            &headers,
            body.as_deref(),
            None,
            max_response_bytes,
        )
        .await?;
        let payment_required = first.payment_required.clone();
        if first.status_code != 402 || !auto_pay {
            return Ok(first.into_json(false, None));
        }

        let Some(payment_required) = payment_required.as_ref() else {
            return Err(ToolError::Execution(
                "x402 retry requested but PAYMENT-REQUIRED was missing or invalid".into(),
            ));
        };
        let Some(max_amount) = max_amount else {
            return Err(ToolError::InvalidInput(
                "x402 retry requires max_amount or AGENT_PAYMENT_MAX_AMOUNT".into(),
            ));
        };
        let spend = assess_payment_required_amount(payment_required, max_amount)?;
        if !spend.within_limit {
            return Ok(first.into_json(false, Some(spend)));
        }
        let signature = payment_signature_material(
            &input,
            &self.config,
            self.secret_store.as_ref(),
            payment_required,
            url,
            &method,
            max_amount,
        )
        .await?;
        let second = send_payment_request(
            &client,
            &method,
            url,
            &headers,
            body.as_deref(),
            Some(&signature.value),
            max_response_bytes,
        )
        .await?;
        Ok(json!({
            "status": "retried",
            "initial": first.into_json(true, Some(spend)),
            "retry": second.into_json(true, None),
            "wallet": {
                "signature_source": signature.source
            }
        }))
    }
}

struct PaymentSignatureMaterial {
    value: String,
    source: &'static str,
}

async fn payment_signature_material(
    input: &Value,
    config: &PaymentX402Config,
    secret_store: &dyn SecretStore,
    payment_required: &Value,
    url: &str,
    method: &str,
    max_amount: f64,
) -> Result<PaymentSignatureMaterial, ToolError> {
    if let Some(value) = input
        .get("payment_signature")
        .and_then(Value::as_str)
        .and_then(|value| clean_non_empty(value.to_string()))
    {
        return Ok(PaymentSignatureMaterial {
            value,
            source: "input",
        });
    }

    if let Some(reference) = input
        .get("payment_signature_secret")
        .and_then(Value::as_str)
        .and_then(|value| clean_non_empty(value.to_string()))
    {
        return Ok(PaymentSignatureMaterial {
            value: resolve_secret_reference_value(secret_store, &reference)?,
            source: "input_secret",
        });
    }

    if let Some(reference) = config.signature_secret.as_deref() {
        return Ok(PaymentSignatureMaterial {
            value: resolve_secret_reference_value(secret_store, reference)?,
            source: "configured_secret",
        });
    }

    if let Some(value) = std::env::var(&config.signature_env)
        .ok()
        .and_then(clean_non_empty)
    {
        return Ok(PaymentSignatureMaterial {
            value,
            source: "env",
        });
    }

    if let Some(wallet_command) = config.wallet_command.as_ref() {
        return payment_signature_from_wallet_command(
            wallet_command,
            payment_required,
            url,
            method,
            max_amount,
        )
        .await;
    }

    Err(ToolError::InvalidInput(format!(
        "x402 retry requires `payment_signature`, `payment_signature_secret`, AGENT_X402_SIGNATURE_SECRET, {}, or AGENT_X402_WALLET_COMMAND",
        config.signature_env
    )))
}

async fn payment_signature_from_wallet_command(
    wallet_command: &PaymentWalletCommandConfig,
    payment_required: &Value,
    url: &str,
    method: &str,
    max_amount: f64,
) -> Result<PaymentSignatureMaterial, ToolError> {
    let payload = json!({
        "payment_required": payment_required,
        "url": url,
        "method": method,
        "max_amount": max_amount,
    });
    let mut child = Command::new(&wallet_command.command)
        .args(&wallet_command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| ToolError::Execution(format!("failed to start x402 wallet command: {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| ToolError::Execution("x402 wallet command stdin unavailable".into()))?;
    stdin
        .write_all(payload.to_string().as_bytes())
        .await
        .map_err(|e| ToolError::Execution(format!("failed to write x402 wallet request: {e}")))?;
    drop(stdin);
    let output = timeout(
        Duration::from_millis(wallet_command.timeout_ms),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| ToolError::Execution("x402 wallet command timed out".into()))?
    .map_err(|e| ToolError::Execution(format!("failed to run x402 wallet command: {e}")))?;
    if !output.status.success() {
        let status = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".into());
        return Err(ToolError::Execution(format!(
            "x402 wallet command exited with status {status}"
        )));
    }
    if output.stdout.len() > 16 * 1024 {
        return Err(ToolError::Execution(
            "x402 wallet command returned too much output".into(),
        ));
    }
    Ok(PaymentSignatureMaterial {
        value: payment_signature_from_wallet_output(&output.stdout)?,
        source: "wallet_command",
    })
}

fn payment_signature_from_wallet_output(stdout: &[u8]) -> Result<String, ToolError> {
    let text = std::str::from_utf8(stdout).map_err(|_| {
        ToolError::Execution("x402 wallet command returned non-UTF-8 output".into())
    })?;
    let text = text.trim();
    if text.is_empty() {
        return Err(ToolError::Execution(
            "x402 wallet command returned an empty signature".into(),
        ));
    }
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        let signature = match value {
            Value::String(signature) => Some(signature),
            Value::Object(map) => map
                .get("payment_signature")
                .or_else(|| map.get("signature"))
                .or_else(|| map.get("paymentSignature"))
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        }
        .and_then(clean_non_empty)
        .ok_or_else(|| {
            ToolError::Execution("x402 wallet command JSON must contain payment_signature".into())
        })?;
        return Ok(signature);
    }
    clean_non_empty(text.to_string()).ok_or_else(|| {
        ToolError::Execution("x402 wallet command returned an empty signature".into())
    })
}

pub struct PaymentX402RequiredTool;

impl PaymentX402RequiredTool {
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("payment_x402_required"),
            name: "x402 Payment Required Response".into(),
            description: "Builds a 402 PAYMENT-REQUIRED header/body payload for a protected x402 resource.".into(),
            categories: vec!["payment".into(), "wallet".into(), "x402".into()],
            input_schema: json!({
                "type": "object",
                "properties": {
                    "payment_required": {
                        "type": "object",
                        "description": "Complete x402 PAYMENT-REQUIRED object. If supplied, accepts/x402_version/error are ignored."
                    },
                    "accepts": {
                        "type": "array",
                        "items": { "type": "object" },
                        "description": "Accepted payment requirements for this protected resource."
                    },
                    "x402_version": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "x402 protocol version. Defaults to 1."
                    },
                    "error": {
                        "type": "string",
                        "description": "Optional error text to include in the x402 challenge."
                    },
                    "body": {
                        "type": "string",
                        "description": "Optional HTTP response body text. Defaults to Payment Required."
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Return the status code, PAYMENT-REQUIRED header value, decoded payment_required object, and body text exactly."
                    .into(),
            ),
            permissions: ToolPermissions {
                wallet: true,
                payment: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("native:payment_x402_required".into()),
        }
    }
}

#[async_trait]
impl Tool for PaymentX402RequiredTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let payment_required = payment_required_from_input(&input)?;
        let body = input
            .get("body")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Payment Required".into());
        let header = encode_payment_header_value(&payment_required)?;
        Ok(json!({
            "status": "payment_required",
            "status_code": 402,
            "headers": {
                "PAYMENT-REQUIRED": header
            },
            "payment_required": payment_required,
            "body": body
        }))
    }
}

pub struct PaymentX402SettleTool {
    config: PaymentX402Config,
}

impl PaymentX402SettleTool {
    pub fn new(config: PaymentX402Config) -> Self {
        Self { config }
    }

    pub fn from_env() -> Self {
        Self::new(PaymentX402Config::from_env())
    }

    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("payment_x402_settle"),
            name: "x402 Verify and Settle".into(),
            description: "Verifies and optionally settles an incoming x402 PAYMENT-SIGNATURE payload through a configured facilitator.".into(),
            categories: vec!["payment".into(), "wallet".into(), "network".into(), "x402".into()],
            input_schema: json!({
                "type": "object",
                "properties": {
                    "facilitator_url": {
                        "type": "string",
                        "description": "HTTP or HTTPS facilitator base URL. Defaults to AGENT_X402_FACILITATOR_URL."
                    },
                    "payment_signature": {
                        "type": "string",
                        "description": "Incoming x402 PAYMENT-SIGNATURE header value."
                    },
                    "payment_payload": {
                        "type": "object",
                        "description": "Decoded incoming x402 payment payload. Used when payment_signature is omitted."
                    },
                    "payment_required": {
                        "type": "object",
                        "description": "Full PAYMENT-REQUIRED object previously sent by payment_x402_required."
                    },
                    "payment_requirements": {
                        "type": "object",
                        "description": "Selected payment requirements object. Overrides payment_required.accepts[accept_index]."
                    },
                    "accept_index": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Index into payment_required.accepts when payment_requirements is omitted. Defaults to 0."
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["verify", "settle", "verify_and_settle"],
                        "description": "Facilitator action. Defaults to verify_and_settle."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1
                    },
                    "max_response_bytes": {
                        "type": "integer",
                        "minimum": 1
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Report facilitator verify/settle status, HTTP status codes, parsed response bodies, and PAYMENT-RESPONSE header value without exposing the incoming payment signature."
                    .into(),
            ),
            permissions: ToolPermissions {
                network: true,
                wallet: true,
                payment: true,
                secrets: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("native:payment_x402_settle".into()),
        }
    }
}

#[async_trait]
impl Tool for PaymentX402SettleTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let facilitator_url = input
            .get("facilitator_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| self.config.facilitator_url.clone())
            .ok_or_else(|| {
                ToolError::InvalidInput(
                    "x402 settlement requires facilitator_url or AGENT_X402_FACILITATOR_URL".into(),
                )
            })?;
        if !(facilitator_url.starts_with("http://") || facilitator_url.starts_with("https://")) {
            return Err(ToolError::InvalidInput(
                "x402 facilitator_url must start with http:// or https://".into(),
            ));
        }
        let mode = input
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("verify_and_settle");
        if !matches!(mode, "verify" | "settle" | "verify_and_settle") {
            return Err(ToolError::InvalidInput(
                "x402 settlement mode must be verify, settle, or verify_and_settle".into(),
            ));
        }
        let timeout_ms = input
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(self.config.default_timeout_ms);
        let max_response_bytes = input
            .get("max_response_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(self.config.max_response_bytes);
        let payment_payload = payment_payload_from_input(&input)?;
        let payment_requirements = payment_requirements_from_input(&input)?;
        let x402_version = payment_x402_version(&input, &payment_payload);
        let request_body = json!({
            "x402Version": x402_version,
            "paymentPayload": payment_payload,
            "paymentRequirements": payment_requirements
        });
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        match mode {
            "verify" => {
                let verify = post_x402_facilitator(
                    &client,
                    &facilitator_url,
                    "verify",
                    &request_body,
                    max_response_bytes,
                )
                .await?;
                Ok(json!({
                    "status": if verify.is_valid() { "verified" } else { "verification_failed" },
                    "x402_version": x402_version,
                    "verify": verify.into_json()
                }))
            }
            "settle" => {
                let settle = post_x402_facilitator(
                    &client,
                    &facilitator_url,
                    "settle",
                    &request_body,
                    max_response_bytes,
                )
                .await?;
                let settled = settle.is_http_success();
                let payment_response = settled
                    .then(|| encode_payment_header_value(&settle.body))
                    .transpose()?;
                Ok(json!({
                    "status": if settled { "settled" } else { "settlement_failed" },
                    "x402_version": x402_version,
                    "settle": settle.into_json(),
                    "headers": {
                        "PAYMENT-RESPONSE": payment_response
                    }
                }))
            }
            _ => {
                let verify = post_x402_facilitator(
                    &client,
                    &facilitator_url,
                    "verify",
                    &request_body,
                    max_response_bytes,
                )
                .await?;
                if !verify.is_valid() {
                    return Ok(json!({
                        "status": "verification_failed",
                        "x402_version": x402_version,
                        "verify": verify.into_json()
                    }));
                }
                let settle = post_x402_facilitator(
                    &client,
                    &facilitator_url,
                    "settle",
                    &request_body,
                    max_response_bytes,
                )
                .await?;
                let settled = settle.is_http_success();
                let payment_response = settled
                    .then(|| encode_payment_header_value(&settle.body))
                    .transpose()?;
                Ok(json!({
                    "status": if settled { "settled" } else { "settlement_failed" },
                    "x402_version": x402_version,
                    "verify": verify.into_json(),
                    "settle": settle.into_json(),
                    "headers": {
                        "PAYMENT-RESPONSE": payment_response
                    }
                }))
            }
        }
    }
}

pub fn register_payment_tools_from_env(registry: &mut ToolRegistry) -> usize {
    if !env_flag("AGENT_PAYMENT_TOOLS") {
        return 0;
    }
    registry.register(
        PaymentX402Tool::descriptor(),
        Arc::new(PaymentX402Tool::from_env()),
    );
    registry.register(
        PaymentX402RequiredTool::descriptor(),
        Arc::new(PaymentX402RequiredTool),
    );
    registry.register(
        PaymentX402SettleTool::descriptor(),
        Arc::new(PaymentX402SettleTool::from_env()),
    );
    3
}

pub struct ArtifactTool {
    output_dir: PathBuf,
    quota_paths: Option<StoragePaths>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceRuntimeConfig {
    pub input_enabled: bool,
    pub output_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tts_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tts_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedArtifact {
    pub id: String,
    pub format: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedArtifactDataUrl {
    pub artifact: GeneratedArtifact,
    pub media_type: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedArtifactExport {
    pub artifact: GeneratedArtifact,
    pub output_path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactGenerateInput {
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedArtifactFile {
    pub artifact: GeneratedArtifact,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl ArtifactTool {
    pub fn from_env() -> Self {
        let paths = StoragePaths::from_env();
        Self {
            output_dir: paths.artifacts_dir(),
            quota_paths: Some(paths),
        }
    }

    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            quota_paths: None,
        }
    }

    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("artifact_generate"),
            name: "Artifact Generator".into(),
            description:
                "Generates scoped document and image artifacts under the harness artifact cache."
                    .into(),
            categories: vec!["documents".into(), "artifacts".into()],
            input_schema: json!({
                "type": "object",
                "required": ["format"],
                "properties": {
                    "format": {
                        "type": "string",
                        "enum": ["txt", "md", "csv", "json", "html", "svg", "pdf", "docx", "xlsx", "pptx"]
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional artifact title used in document formats."
                    },
                    "content": {
                        "type": "string",
                        "description": "Text content for text, markdown, HTML, SVG, PDF, DOCX, and PPTX outputs."
                    },
                    "rows": {
                        "type": "array",
                        "description": "Optional tabular rows for CSV/XLSX. Each row can be an array or object."
                    },
                    "filename": {
                        "type": "string",
                        "description": "Optional base filename. Path separators are ignored."
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Return the artifact id, path, format, byte size, and any limitations of the generated file."
                    .into(),
            ),
            permissions: ToolPermissions {
                file_write: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("native:artifact_generate".into()),
        }
    }
}

#[async_trait]
impl Tool for ArtifactTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let input: ArtifactGenerateInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
        let artifact = generate_artifact(&self.output_dir, self.quota_paths.as_ref(), input)?;

        Ok(json!({
            "artifact_id": artifact.id,
            "format": artifact.format,
            "path": artifact.path.display().to_string(),
            "bytes": artifact.bytes,
            "scoped": true
        }))
    }
}

pub fn register_voice_tools(registry: &mut ToolRegistry, config: VoiceRuntimeConfig) -> usize {
    let mut registered = 0;
    if config.input_enabled {
        registry.register(
            VoiceTranscribeTool::descriptor(&config),
            Arc::new(VoiceTranscribeTool::new(config.clone())),
        );
        registered += 1;
    }
    if config.output_enabled {
        registry.register(
            VoiceSpeakTool::descriptor(&config),
            Arc::new(VoiceSpeakTool::from_env(config)),
        );
        registered += 1;
    }
    registered
}

pub struct VoiceTranscribeTool {
    config: VoiceRuntimeConfig,
}

impl VoiceTranscribeTool {
    pub fn new(config: VoiceRuntimeConfig) -> Self {
        Self { config }
    }

    pub fn descriptor(config: &VoiceRuntimeConfig) -> ToolDescriptor {
        let backend = config
            .input_backend
            .clone()
            .and_then(clean_voice_string)
            .unwrap_or_else(|| {
                if config.input_provider.as_deref() == Some("openai") {
                    "cloud".into()
                } else {
                    "local".into()
                }
            });
        ToolDescriptor {
            id: ToolId::from("voice_transcribe"),
            name: "Voice Transcription".into(),
            description: format!(
                "Transcribes an audio file using the resolved voice input settings (backend: {backend})."
            ),
            categories: vec!["voice".into(), "audio".into(), "input".into()],
            input_schema: json!({
                "type": "object",
                "required": ["audio_path"],
                "properties": {
                    "audio_path": {
                        "type": "string",
                        "description": "Path to an audio file readable by the harness."
                    },
                    "backend": {
                        "type": "string",
                        "enum": ["local", "cloud"],
                        "description": "Optional override for the resolved input backend."
                    },
                    "provider": {
                        "type": "string",
                        "description": "Optional override for the resolved input provider. `command` and `openai` are supported."
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional speech-to-text model override."
                    },
                    "language": {
                        "type": "string",
                        "description": "Optional BCP-47 or provider-specific language hint."
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Optional provider-specific transcription prompt."
                    },
                    "command": {
                        "type": "string",
                        "description": "Local backend command override. Defaults to AGENT_VOICE_STT_COMMAND."
                    },
                    "api_base_url": {
                        "type": "string",
                        "description": "Cloud backend base URL. Defaults to https://api.openai.com/v1."
                    },
                    "api_key_env": {
                        "type": "string",
                        "description": "Cloud backend API key environment variable. Defaults to OPENAI_API_KEY."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Return the transcript text and preserve backend/provider/model provenance.".into(),
            ),
            permissions: ToolPermissions {
                shell: true,
                file_read: true,
                network: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("native:voice_transcribe".into()),
        }
    }
}

#[async_trait]
impl Tool for VoiceTranscribeTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let audio_path = input
            .get("audio_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::InvalidInput("missing string field `audio_path`".into()))?;
        let timeout_ms = voice_timeout_ms(&input);
        let backend = voice_value(&input, "backend", &self.config.input_backend)
            .unwrap_or_else(|| "local".into())
            .to_ascii_lowercase();
        match backend.as_str() {
            "local" => self.transcribe_local(audio_path, &input, timeout_ms).await,
            "cloud" => self.transcribe_cloud(audio_path, &input, timeout_ms).await,
            other => Err(ToolError::InvalidInput(format!(
                "unsupported voice input backend `{other}`"
            ))),
        }
    }
}

impl VoiceTranscribeTool {
    async fn transcribe_local(
        &self,
        audio_path: &str,
        input: &Value,
        timeout_ms: u64,
    ) -> Result<Value, ToolError> {
        let command = voice_command(input, "command", "AGENT_VOICE_STT_COMMAND")?;
        let provider = voice_value(input, "provider", &self.config.input_provider)
            .unwrap_or_else(|| "command".into());
        let model = voice_value(input, "model", &self.config.input_model);
        let language = input.get("language").and_then(Value::as_str).unwrap_or("");
        let prompt = input.get("prompt").and_then(Value::as_str).unwrap_or("");
        let output = run_voice_command(
            &command,
            &[
                ("AGENT_VOICE_INPUT_PATH", audio_path),
                ("AGENT_VOICE_PROVIDER", &provider),
                ("AGENT_VOICE_MODEL", model.as_deref().unwrap_or("")),
                ("AGENT_VOICE_LANGUAGE", language),
                ("AGENT_VOICE_PROMPT", prompt),
            ],
            timeout_ms,
        )
        .await?;
        let transcript = output.stdout.trim().to_string();
        if transcript.is_empty() {
            return Err(ToolError::Execution(
                "voice transcription command returned an empty transcript".into(),
            ));
        }
        Ok(json!({
            "transcript": transcript,
            "backend": "local",
            "provider": provider,
            "model": model,
            "duration_ms": output.duration_ms,
            "stderr": output.stderr,
            "truncated_stderr": output.truncated_stderr
        }))
    }

    async fn transcribe_cloud(
        &self,
        audio_path: &str,
        input: &Value,
        timeout_ms: u64,
    ) -> Result<Value, ToolError> {
        let provider = voice_value(input, "provider", &self.config.input_provider)
            .unwrap_or_else(|| "openai".into());
        if !provider.eq_ignore_ascii_case("openai") {
            return Err(ToolError::InvalidInput(format!(
                "unsupported cloud voice input provider `{provider}`"
            )));
        }
        let model = voice_value(input, "model", &self.config.input_model)
            .unwrap_or_else(|| "gpt-4o-transcribe".into());
        let base_url = voice_api_base_url(input);
        let api_key = voice_api_key(input)?;
        let started = Instant::now();
        let bytes = std::fs::read(audio_path).map_err(|e| ToolError::Execution(e.to_string()))?;
        let filename = Path::new(audio_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio");
        let part = reqwest::multipart::Part::bytes(bytes).file_name(filename.to_string());
        let mut form = reqwest::multipart::Form::new()
            .text("model", model.clone())
            .text("response_format", "json")
            .part("file", part);
        if let Some(language) = input
            .get("language")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            form = form.text("language", language.to_string());
        }
        if let Some(prompt) = input
            .get("prompt")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            form = form.text("prompt", prompt.to_string());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let response = client
            .post(format!(
                "{}/audio/transcriptions",
                base_url.trim_end_matches('/')
            ))
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        if !status.is_success() {
            return Err(ToolError::Execution(format!(
                "voice transcription request failed with {status}: {body}"
            )));
        }
        let transcript = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(body);
        Ok(json!({
            "transcript": transcript,
            "backend": "cloud",
            "provider": provider,
            "model": model,
            "duration_ms": started.elapsed().as_millis() as u64
        }))
    }
}

pub struct VoiceSpeakTool {
    config: VoiceRuntimeConfig,
    output_dir: PathBuf,
    quota_paths: Option<StoragePaths>,
}

impl VoiceSpeakTool {
    pub fn from_env(config: VoiceRuntimeConfig) -> Self {
        let paths = StoragePaths::from_env();
        Self {
            config,
            output_dir: paths.artifacts_dir(),
            quota_paths: Some(paths),
        }
    }

    pub fn new(config: VoiceRuntimeConfig, output_dir: impl Into<PathBuf>) -> Self {
        Self {
            config,
            output_dir: output_dir.into(),
            quota_paths: None,
        }
    }

    pub fn descriptor(config: &VoiceRuntimeConfig) -> ToolDescriptor {
        let backend = config
            .output_backend
            .clone()
            .and_then(clean_voice_string)
            .unwrap_or_else(|| {
                if config.tts_provider.as_deref() == Some("openai") {
                    "cloud".into()
                } else {
                    "local".into()
                }
            });
        ToolDescriptor {
            id: ToolId::from("voice_speak"),
            name: "Voice Speech".into(),
            description: format!(
                "Synthesizes speech audio using the resolved voice output settings (backend: {backend})."
            ),
            categories: vec!["voice".into(), "audio".into(), "output".into()],
            input_schema: json!({
                "type": "object",
                "required": ["text"],
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to synthesize as speech."
                    },
                    "backend": {
                        "type": "string",
                        "enum": ["local", "cloud"],
                        "description": "Optional override for the resolved output backend."
                    },
                    "provider": {
                        "type": "string",
                        "description": "Optional override for the resolved TTS provider. `command` and `openai` are supported."
                    },
                    "model": {
                        "type": "string",
                        "description": "Optional TTS model override."
                    },
                    "voice": {
                        "type": "string",
                        "description": "Optional provider-specific voice name."
                    },
                    "tone": {
                        "type": "string",
                        "description": "Optional style/tone guidance."
                    },
                    "format": {
                        "type": "string",
                        "enum": ["mp3", "wav", "opus", "aac", "flac", "pcm"],
                        "description": "Audio output format. Defaults to mp3 for cloud and wav for local."
                    },
                    "filename": {
                        "type": "string",
                        "description": "Optional artifact filename stem."
                    },
                    "command": {
                        "type": "string",
                        "description": "Local backend command override. Defaults to AGENT_VOICE_TTS_COMMAND."
                    },
                    "api_base_url": {
                        "type": "string",
                        "description": "Cloud backend base URL. Defaults to https://api.openai.com/v1."
                    },
                    "api_key_env": {
                        "type": "string",
                        "description": "Cloud backend API key environment variable. Defaults to OPENAI_API_KEY."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Return the generated audio artifact path, backend/provider/model, voice, tone, and any command stderr.".into(),
            ),
            permissions: ToolPermissions {
                shell: true,
                file_write: true,
                network: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("native:voice_speak".into()),
        }
    }
}

#[async_trait]
impl Tool for VoiceSpeakTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let text = input
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ToolError::InvalidInput("missing string field `text`".into()))?;
        let timeout_ms = voice_timeout_ms(&input);
        let backend = voice_value(&input, "backend", &self.config.output_backend)
            .unwrap_or_else(|| "local".into())
            .to_ascii_lowercase();
        match backend.as_str() {
            "local" => self.speak_local(text, &input, timeout_ms).await,
            "cloud" => self.speak_cloud(text, &input, timeout_ms).await,
            other => Err(ToolError::InvalidInput(format!(
                "unsupported voice output backend `{other}`"
            ))),
        }
    }
}

impl VoiceSpeakTool {
    async fn speak_local(
        &self,
        text: &str,
        input: &Value,
        timeout_ms: u64,
    ) -> Result<Value, ToolError> {
        let command = voice_command(input, "command", "AGENT_VOICE_TTS_COMMAND")?;
        let provider = voice_value(input, "provider", &self.config.tts_provider)
            .unwrap_or_else(|| "command".into());
        let model = voice_value(input, "model", &self.config.tts_model);
        let voice = voice_value(input, "voice", &self.config.voice);
        let tone = voice_value(input, "tone", &self.config.tone);
        let format = voice_output_format(input, "wav")?;
        let (artifact_id, path) = self.voice_output_path(input, &format)?;
        ensure_artifact_quota(self.quota_paths.as_ref(), 0)?;
        std::fs::create_dir_all(&self.output_dir)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let output_path = path.display().to_string();
        let output = run_voice_command(
            &command,
            &[
                ("AGENT_VOICE_TEXT", text),
                ("AGENT_VOICE_OUTPUT_PATH", &output_path),
                ("AGENT_VOICE_PROVIDER", &provider),
                ("AGENT_VOICE_MODEL", model.as_deref().unwrap_or("")),
                ("AGENT_VOICE_NAME", voice.as_deref().unwrap_or("")),
                ("AGENT_VOICE_TONE", tone.as_deref().unwrap_or("")),
                ("AGENT_VOICE_FORMAT", &format),
            ],
            timeout_ms,
        )
        .await?;
        if !path.exists() && !output.stdout_bytes.is_empty() {
            ensure_artifact_quota(self.quota_paths.as_ref(), output.stdout_bytes.len())?;
            std::fs::write(&path, &output.stdout_bytes)
                .map_err(|e| ToolError::Execution(e.to_string()))?;
        }
        let bytes = std::fs::metadata(&path)
            .map_err(|e| {
                ToolError::Execution(format!(
                    "voice speech command did not create `{}`: {e}",
                    path.display()
                ))
            })?
            .len();
        ensure_artifact_quota_after_external_write(self.quota_paths.as_ref(), &path)?;
        Ok(json!({
            "artifact_id": artifact_id,
            "format": format,
            "path": path.display().to_string(),
            "bytes": bytes,
            "backend": "local",
            "provider": provider,
            "model": model,
            "voice": voice,
            "tone": tone,
            "duration_ms": output.duration_ms,
            "stderr": output.stderr,
            "truncated_stderr": output.truncated_stderr,
            "scoped": true
        }))
    }

    async fn speak_cloud(
        &self,
        text: &str,
        input: &Value,
        timeout_ms: u64,
    ) -> Result<Value, ToolError> {
        let provider = voice_value(input, "provider", &self.config.tts_provider)
            .unwrap_or_else(|| "openai".into());
        if !provider.eq_ignore_ascii_case("openai") {
            return Err(ToolError::InvalidInput(format!(
                "unsupported cloud voice output provider `{provider}`"
            )));
        }
        let model = voice_value(input, "model", &self.config.tts_model)
            .unwrap_or_else(|| "gpt-4o-mini-tts".into());
        let voice =
            voice_value(input, "voice", &self.config.voice).unwrap_or_else(|| "alloy".into());
        let tone = voice_value(input, "tone", &self.config.tone);
        let format = voice_output_format(input, "mp3")?;
        let (artifact_id, path) = self.voice_output_path(input, &format)?;
        let base_url = voice_api_base_url(input);
        let api_key = voice_api_key(input)?;
        let started = Instant::now();
        let mut payload = json!({
            "model": model,
            "input": text,
            "voice": voice,
            "response_format": format
        });
        if let Some(tone) = tone.as_deref() {
            payload["instructions"] = Value::String(tone.to_string());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let response = client
            .post(format!("{}/audio/speech", base_url.trim_end_matches('/')))
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(ToolError::Execution(format!(
                "voice speech request failed with {status}: {body}"
            )));
        }
        ensure_artifact_quota(self.quota_paths.as_ref(), bytes.len())?;
        std::fs::create_dir_all(&self.output_dir)
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        std::fs::write(&path, &bytes).map_err(|e| ToolError::Execution(e.to_string()))?;
        Ok(json!({
            "artifact_id": artifact_id,
            "format": format,
            "path": path.display().to_string(),
            "bytes": bytes.len(),
            "backend": "cloud",
            "provider": provider,
            "model": model,
            "voice": payload["voice"].clone(),
            "tone": tone,
            "duration_ms": started.elapsed().as_millis() as u64,
            "scoped": true
        }))
    }

    fn voice_output_path(
        &self,
        input: &Value,
        format: &str,
    ) -> Result<(String, PathBuf), ToolError> {
        let base = input
            .get("filename")
            .and_then(Value::as_str)
            .map(safe_artifact_basename)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "voice-output".into());
        let artifact_id = format!(
            "{}-{}",
            chrono_like_timestamp(),
            base.trim_end_matches(&format!(".{format}"))
        );
        let filename = format!("{artifact_id}.{format}");
        Ok((
            artifact_id,
            scoped_artifact_path(&self.output_dir, &filename)?,
        ))
    }
}

struct VoiceCommandOutput {
    stdout: String,
    stdout_bytes: Vec<u8>,
    stderr: String,
    truncated_stderr: bool,
    duration_ms: u64,
}

async fn run_voice_command(
    command: &str,
    env: &[(&str, &str)],
    timeout_ms: u64,
) -> Result<VoiceCommandOutput, ToolError> {
    let mut cmd = shell_command(command);
    apply_minimal_env(&mut cmd);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.kill_on_drop(true);
    let started = Instant::now();
    let child = cmd
        .spawn()
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let output = match timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Ok(result) => result.map_err(|e| ToolError::Execution(e.to_string()))?,
        Err(_) => {
            return Err(ToolError::Execution(
                "voice command timed out before producing output".into(),
            ));
        }
    };
    let (stdout, _) = decode_and_truncate(&output.stdout, 8 * 1024);
    let (stderr, truncated_stderr) = decode_and_truncate(&output.stderr, 8 * 1024);
    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "voice command exited with status {}: {}",
            output.status, stderr
        )));
    }
    Ok(VoiceCommandOutput {
        stdout,
        stdout_bytes: output.stdout,
        stderr,
        truncated_stderr,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn voice_value(input: &Value, key: &str, configured: &Option<String>) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| configured.clone())
        .and_then(clean_voice_string)
}

fn clean_voice_string(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn voice_timeout_ms(input: &Value) -> u64 {
    input
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(60_000)
}

fn voice_command(input: &Value, key: &str, env_key: &str) -> Result<String, ToolError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| std::env::var(env_key).ok())
        .and_then(clean_voice_string)
        .ok_or_else(|| {
            ToolError::InvalidInput(format!(
                "local voice backend requires `{key}` or environment variable `{env_key}`"
            ))
        })
}

fn voice_api_base_url(input: &Value) -> String {
    input
        .get("api_base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/')
        .to_string()
}

fn voice_api_key(input: &Value) -> Result<String, ToolError> {
    let env_key = input
        .get("api_key_env")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("OPENAI_API_KEY");
    std::env::var(env_key).map_err(|_| {
        ToolError::InvalidInput(format!(
            "cloud voice backend requires API key env var `{env_key}`"
        ))
    })
}

fn voice_output_format(input: &Value, default: &str) -> Result<String, ToolError> {
    let format = input
        .get("format")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
        .to_ascii_lowercase();
    match format.as_str() {
        "mp3" | "wav" | "opus" | "aac" | "flac" | "pcm" => Ok(format),
        other => Err(ToolError::InvalidInput(format!(
            "unsupported voice output format `{other}`"
        ))),
    }
}

pub fn list_generated_artifacts_from_env() -> Result<Vec<GeneratedArtifact>, ToolError> {
    list_generated_artifacts(StoragePaths::from_env().artifacts_dir())
}

pub fn generate_artifact_from_env(
    input: ArtifactGenerateInput,
) -> Result<GeneratedArtifact, ToolError> {
    let paths = StoragePaths::from_env();
    generate_artifact(&paths.artifacts_dir(), Some(&paths), input)
}

pub fn generate_artifact(
    output_dir: &Path,
    quota_paths: Option<&StoragePaths>,
    input: ArtifactGenerateInput,
) -> Result<GeneratedArtifact, ToolError> {
    let format = input.format.trim().to_ascii_lowercase();
    let title = input
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Generated Artifact");
    let content = input.content.as_deref().unwrap_or_default();
    let extension = artifact_extension(&format)?;
    let base = input
        .filename
        .as_deref()
        .map(safe_artifact_basename)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| safe_artifact_basename(title));
    let artifact_id = format!(
        "{}-{}",
        chrono_like_timestamp(),
        base.trim_end_matches(&format!(".{extension}"))
    );
    let filename = format!("{artifact_id}.{extension}");
    let path = scoped_artifact_path(output_dir, &filename)?;
    let bytes = render_artifact(&format, title, content, input.rows.as_ref())?;
    ensure_artifact_quota(quota_paths, bytes.len())?;

    std::fs::create_dir_all(output_dir).map_err(|e| ToolError::Execution(e.to_string()))?;
    std::fs::write(&path, &bytes).map_err(|e| ToolError::Execution(e.to_string()))?;
    show_generated_artifact(output_dir, &artifact_id)
}

pub fn list_generated_artifacts(
    output_dir: impl AsRef<Path>,
) -> Result<Vec<GeneratedArtifact>, ToolError> {
    let output_dir = output_dir.as_ref();
    let mut artifacts = Vec::new();
    let entries = match std::fs::read_dir(output_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(artifacts),
        Err(err) => return Err(ToolError::Execution(err.to_string())),
    };
    for entry in entries {
        let entry = entry.map_err(|e| ToolError::Execution(e.to_string()))?;
        let file_type = entry
            .file_type()
            .map_err(|e| ToolError::Execution(e.to_string()))?;
        if !file_type.is_file() {
            continue;
        }
        if let Some(artifact) = generated_artifact_from_path(entry.path())? {
            artifacts.push(artifact);
        }
    }
    artifacts.sort_by(|left, right| {
        right
            .modified_ms
            .cmp(&left.modified_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(artifacts)
}

pub fn show_generated_artifact_from_env(
    id_or_filename: &str,
) -> Result<GeneratedArtifact, ToolError> {
    show_generated_artifact(StoragePaths::from_env().artifacts_dir(), id_or_filename)
}

pub fn show_generated_artifact(
    output_dir: impl AsRef<Path>,
    id_or_filename: &str,
) -> Result<GeneratedArtifact, ToolError> {
    let output_dir = output_dir.as_ref();
    let reference = safe_artifact_reference(id_or_filename)?;
    list_generated_artifacts(output_dir)?
        .into_iter()
        .find(|artifact| {
            artifact.id == reference
                || artifact
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == reference)
        })
        .ok_or_else(|| ToolError::InvalidInput(format!("artifact `{reference}` not found")))
}

pub fn open_generated_artifact_from_env(
    id_or_filename: &str,
) -> Result<GeneratedArtifact, ToolError> {
    open_generated_artifact(StoragePaths::from_env().artifacts_dir(), id_or_filename)
}

pub fn open_generated_artifact(
    output_dir: impl AsRef<Path>,
    id_or_filename: &str,
) -> Result<GeneratedArtifact, ToolError> {
    let output_dir = output_dir.as_ref();
    let artifact = show_generated_artifact(output_dir, id_or_filename)?;
    ensure_artifact_stays_scoped(output_dir, &artifact.path)?;
    spawn_os_open(&artifact.path)?;
    Ok(artifact)
}

pub fn export_generated_artifact_from_env(
    id_or_filename: &str,
    destination: impl AsRef<Path>,
) -> Result<GeneratedArtifactExport, ToolError> {
    export_generated_artifact(
        StoragePaths::from_env().artifacts_dir(),
        id_or_filename,
        destination,
    )
}

pub fn export_generated_artifact(
    output_dir: impl AsRef<Path>,
    id_or_filename: &str,
    destination: impl AsRef<Path>,
) -> Result<GeneratedArtifactExport, ToolError> {
    let output_dir = output_dir.as_ref();
    let artifact = show_generated_artifact(output_dir, id_or_filename)?;
    ensure_artifact_stays_scoped(output_dir, &artifact.path)?;
    let destination = artifact_export_destination(destination.as_ref(), &artifact)?;
    if destination.exists() {
        return Err(ToolError::InvalidInput(format!(
            "export destination already exists: {}",
            destination.display()
        )));
    }
    if let Some(parent) = destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|e| ToolError::Execution(e.to_string()))?;
    }
    let bytes = std::fs::copy(&artifact.path, &destination)
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    Ok(GeneratedArtifactExport {
        artifact,
        output_path: destination,
        bytes,
    })
}

pub fn delete_generated_artifact_from_env(
    id_or_filename: &str,
) -> Result<GeneratedArtifact, ToolError> {
    delete_generated_artifact(StoragePaths::from_env().artifacts_dir(), id_or_filename)
}

pub fn delete_generated_artifacts_by_ids_from_env(
    ids: &[String],
) -> Result<Vec<String>, ToolError> {
    delete_generated_artifacts_by_ids(StoragePaths::from_env().artifacts_dir(), ids)
}

pub fn delete_generated_artifact(
    output_dir: impl AsRef<Path>,
    id_or_filename: &str,
) -> Result<GeneratedArtifact, ToolError> {
    let output_dir = output_dir.as_ref();
    let artifact = show_generated_artifact(output_dir, id_or_filename)?;
    ensure_artifact_stays_scoped(output_dir, &artifact.path)?;
    std::fs::remove_file(&artifact.path).map_err(|e| ToolError::Execution(e.to_string()))?;
    Ok(artifact)
}

pub fn delete_generated_artifacts_by_ids(
    output_dir: impl AsRef<Path>,
    ids: &[String],
) -> Result<Vec<String>, ToolError> {
    let output_dir = output_dir.as_ref();
    let mut deleted = Vec::new();
    for id in ids.iter().collect::<BTreeSet<_>>() {
        if safe_artifact_reference(id).is_err() {
            continue;
        }
        match delete_generated_artifact(output_dir, id) {
            Ok(artifact) => deleted.push(artifact.id),
            Err(ToolError::InvalidInput(message)) if message.contains("not found") => {}
            Err(err) => return Err(err),
        }
    }
    deleted.sort();
    Ok(deleted)
}

pub fn generated_artifact_ids_in_value(value: &Value) -> Vec<String> {
    fn collect(value: &Value, ids: &mut BTreeSet<String>) {
        match value {
            Value::Object(object) => {
                if let Some(id) = object
                    .get("artifact_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                {
                    ids.insert(id.to_string());
                }
                if let Some(values) = object.get("artifact_ids").and_then(Value::as_array) {
                    for id in values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                    {
                        ids.insert(id.to_string());
                    }
                }
                for child in object.values() {
                    collect(child, ids);
                }
            }
            Value::Array(values) => {
                for child in values {
                    collect(child, ids);
                }
            }
            _ => {}
        }
    }

    let mut ids = BTreeSet::new();
    collect(value, &mut ids);
    ids.into_iter().collect()
}

pub fn generated_artifact_data_url_from_env(
    id_or_filename: &str,
) -> Result<GeneratedArtifactDataUrl, ToolError> {
    generated_artifact_data_url(StoragePaths::from_env().artifacts_dir(), id_or_filename)
}

pub fn generated_artifact_data_url(
    output_dir: impl AsRef<Path>,
    id_or_filename: &str,
) -> Result<GeneratedArtifactDataUrl, ToolError> {
    let output_dir = output_dir.as_ref();
    let artifact = show_generated_artifact(output_dir, id_or_filename)?;
    ensure_artifact_stays_scoped(output_dir, &artifact.path)?;
    let media_type = artifact_media_type(&artifact.format)?;
    let bytes = std::fs::read(&artifact.path).map_err(|e| ToolError::Execution(e.to_string()))?;
    let data_url = format!(
        "data:{media_type};base64,{}",
        general_purpose::STANDARD.encode(bytes)
    );
    Ok(GeneratedArtifactDataUrl {
        artifact,
        media_type: media_type.into(),
        data_url,
    })
}

pub fn generated_artifact_file_from_env(
    id_or_filename: &str,
) -> Result<GeneratedArtifactFile, ToolError> {
    generated_artifact_file(StoragePaths::from_env().artifacts_dir(), id_or_filename)
}

pub fn generated_artifact_file(
    output_dir: impl AsRef<Path>,
    id_or_filename: &str,
) -> Result<GeneratedArtifactFile, ToolError> {
    let output_dir = output_dir.as_ref();
    let artifact = show_generated_artifact(output_dir, id_or_filename)?;
    ensure_artifact_stays_scoped(output_dir, &artifact.path)?;
    let media_type = artifact_media_type(&artifact.format)?;
    let bytes = std::fs::read(&artifact.path).map_err(|e| ToolError::Execution(e.to_string()))?;
    Ok(GeneratedArtifactFile {
        artifact,
        media_type: media_type.into(),
        bytes,
    })
}

pub fn save_voice_capture_from_env(
    data_url: &str,
    filename: Option<&str>,
) -> Result<GeneratedArtifact, ToolError> {
    let paths = StoragePaths::from_env();
    save_voice_capture_with_quota(paths.artifacts_dir(), data_url, filename, Some(&paths))
}

pub fn save_voice_capture(
    output_dir: impl AsRef<Path>,
    data_url: &str,
    filename: Option<&str>,
) -> Result<GeneratedArtifact, ToolError> {
    save_voice_capture_with_quota(output_dir, data_url, filename, None)
}

fn save_voice_capture_with_quota(
    output_dir: impl AsRef<Path>,
    data_url: &str,
    filename: Option<&str>,
    quota_paths: Option<&StoragePaths>,
) -> Result<GeneratedArtifact, ToolError> {
    let output_dir = output_dir.as_ref();
    let (bytes, extension) = decode_audio_data_url(data_url)?;
    ensure_artifact_quota(quota_paths, bytes.len())?;
    std::fs::create_dir_all(output_dir).map_err(|e| ToolError::Execution(e.to_string()))?;
    let stem = filename
        .and_then(|name| Path::new(name).file_stem())
        .and_then(|stem| stem.to_str())
        .map(safe_artifact_basename)
        .unwrap_or_else(|| "voice-input".into());
    let artifact_id = format!("{}-{}", stem, uuid::Uuid::new_v4());
    let filename = format!("{artifact_id}.{extension}");
    let path = scoped_artifact_path(output_dir, &filename)?;
    std::fs::write(&path, bytes).map_err(|e| ToolError::Execution(e.to_string()))?;
    show_generated_artifact(output_dir, &artifact_id)
}

fn ensure_artifact_quota(
    quota_paths: Option<&StoragePaths>,
    attempted_bytes: usize,
) -> Result<(), ToolError> {
    let Some(paths) = quota_paths else {
        return Ok(());
    };
    let attempted_bytes = u64::try_from(attempted_bytes).unwrap_or(u64::MAX);
    paths
        .ensure_quota_for_write(attempted_bytes)
        .map_err(|e| ToolError::Execution(e.to_string()))
}

fn ensure_artifact_quota_after_external_write(
    quota_paths: Option<&StoragePaths>,
    path: &Path,
) -> Result<(), ToolError> {
    let Some(paths) = quota_paths else {
        return Ok(());
    };
    if let Err(err) = paths.ensure_quota_for_write(0) {
        let _ = std::fs::remove_file(path);
        return Err(ToolError::Execution(err.to_string()));
    }
    Ok(())
}

fn decode_audio_data_url(data_url: &str) -> Result<(Vec<u8>, &'static str), ToolError> {
    let (metadata, encoded) = data_url
        .split_once(',')
        .ok_or_else(|| ToolError::InvalidInput("voice capture must be a data URL".into()))?;
    let metadata = metadata
        .strip_prefix("data:")
        .ok_or_else(|| ToolError::InvalidInput("voice capture must be a data URL".into()))?;
    let mut parts = metadata.split(';');
    let media_type = parts
        .next()
        .map(str::to_ascii_lowercase)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::InvalidInput("voice capture media type is missing".into()))?;
    if !parts.any(|part| part.eq_ignore_ascii_case("base64")) {
        return Err(ToolError::InvalidInput(
            "voice capture data URL must be base64 encoded".into(),
        ));
    }
    let extension = audio_media_extension(&media_type)?;
    let bytes = general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| ToolError::InvalidInput(format!("invalid voice capture base64: {e}")))?;
    if bytes.is_empty() {
        return Err(ToolError::InvalidInput("voice capture is empty".into()));
    }
    Ok((bytes, extension))
}

fn audio_media_extension(media_type: &str) -> Result<&'static str, ToolError> {
    match media_type {
        "audio/webm" => Ok("webm"),
        "audio/wav" | "audio/x-wav" => Ok("wav"),
        "audio/mpeg" | "audio/mp3" => Ok("mp3"),
        "audio/mp4" | "audio/x-m4a" => Ok("m4a"),
        "audio/ogg" => Ok("ogg"),
        other => Err(ToolError::InvalidInput(format!(
            "unsupported voice capture media type `{other}`"
        ))),
    }
}

fn shell_command(command: &str) -> Command {
    let (program, args) = shell_program_args(command);
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd
}

fn shell_program_args(command: &str) -> (String, Vec<OsString>) {
    #[cfg(windows)]
    {
        ("cmd".into(), vec!["/C".into(), command.into()])
    }
    #[cfg(not(windows))]
    {
        ("/bin/sh".into(), vec!["-c".into(), command.into()])
    }
}

fn restricted_program_args(
    command: &str,
    allowlist: &[String],
) -> Result<(String, Vec<OsString>), ToolError> {
    let parsed = parse_restricted_command(command, allowlist)?;
    Ok((
        parsed.program,
        parsed.args.into_iter().map(OsString::from).collect(),
    ))
}

fn apply_minimal_env(cmd: &mut Command) {
    cmd.env_clear();
    for key in minimal_env_keys() {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
}

#[cfg(windows)]
fn minimal_env_keys() -> &'static [&'static str] {
    &["Path", "PATH", "SystemRoot", "ComSpec", "TEMP", "TMP"]
}

#[cfg(not(windows))]
fn minimal_env_keys() -> &'static [&'static str] {
    &["PATH", "LANG", "LC_ALL", "TMPDIR"]
}

#[cfg(windows)]
fn default_python_command() -> String {
    "python".into()
}

#[cfg(not(windows))]
fn default_python_command() -> String {
    "python3".into()
}

fn optional_string_array(value: Option<&Value>, field: &str) -> Result<Vec<String>, ToolError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| ToolError::InvalidInput(format!("field `{field}` must be an array")))?;
    items
        .iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                ToolError::InvalidInput(format!("field `{field}` must contain only strings"))
            })
        })
        .collect()
}

fn clean_non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
}

fn env_usize(key: &str) -> Option<usize> {
    env_u64(key).and_then(|value| usize::try_from(value).ok())
}

fn optional_header_map(value: Option<&Value>) -> Result<Vec<(String, String)>, ToolError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| ToolError::InvalidInput("field `headers` must be an object".into()))?;
    let mut headers = Vec::new();
    for (key, value) in object {
        if key.eq_ignore_ascii_case("payment-signature")
            || key.eq_ignore_ascii_case("payment-required")
            || key.eq_ignore_ascii_case("payment-response")
        {
            return Err(ToolError::InvalidInput(
                "payment headers are managed by the x402 payment tool".into(),
            ));
        }
        let value = value.as_str().ok_or_else(|| {
            ToolError::InvalidInput("field `headers` must contain only string values".into())
        })?;
        headers.push((key.clone(), value.to_string()));
    }
    Ok(headers)
}

fn payment_required_from_input(input: &Value) -> Result<Value, ToolError> {
    if let Some(payment_required) = input.get("payment_required") {
        let object = payment_required.as_object().ok_or_else(|| {
            ToolError::InvalidInput("field `payment_required` must be an object".into())
        })?;
        let accepts = object
            .get("accepts")
            .and_then(Value::as_array)
            .filter(|accepts| !accepts.is_empty())
            .ok_or_else(|| {
                ToolError::InvalidInput("field `payment_required.accepts` must be non-empty".into())
            })?;
        if accepts.iter().any(|value| !value.is_object()) {
            return Err(ToolError::InvalidInput(
                "field `payment_required.accepts` must contain only objects".into(),
            ));
        }
        return Ok(payment_required.clone());
    }

    let accepts = input
        .get("accepts")
        .and_then(Value::as_array)
        .filter(|accepts| !accepts.is_empty())
        .ok_or_else(|| ToolError::InvalidInput("field `accepts` must be non-empty".into()))?;
    if accepts.iter().any(|value| !value.is_object()) {
        return Err(ToolError::InvalidInput(
            "field `accepts` must contain only objects".into(),
        ));
    }
    let x402_version = input
        .get("x402_version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let mut payment_required = json!({
        "x402Version": x402_version,
        "accepts": accepts
    });
    if let Some(error) = input.get("error").and_then(Value::as_str) {
        payment_required["error"] = Value::String(error.to_string());
    }
    Ok(payment_required)
}

fn payment_payload_from_input(input: &Value) -> Result<Value, ToolError> {
    if let Some(payment_payload) = input.get("payment_payload") {
        if !payment_payload.is_object() {
            return Err(ToolError::InvalidInput(
                "field `payment_payload` must be an object".into(),
            ));
        }
        return Ok(payment_payload.clone());
    }
    let signature = input
        .get("payment_signature")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ToolError::InvalidInput(
                "x402 settlement requires `payment_signature` or `payment_payload`".into(),
            )
        })?;
    decode_payment_header_value(signature)
}

fn payment_requirements_from_input(input: &Value) -> Result<Value, ToolError> {
    if let Some(payment_requirements) = input.get("payment_requirements") {
        if !payment_requirements.is_object() {
            return Err(ToolError::InvalidInput(
                "field `payment_requirements` must be an object".into(),
            ));
        }
        return Ok(payment_requirements.clone());
    }
    let payment_required = input.get("payment_required").ok_or_else(|| {
        ToolError::InvalidInput(
            "x402 settlement requires `payment_requirements` or `payment_required`".into(),
        )
    })?;
    let accepts = payment_required
        .get("accepts")
        .and_then(Value::as_array)
        .filter(|accepts| !accepts.is_empty())
        .ok_or_else(|| {
            ToolError::InvalidInput("field `payment_required.accepts` must be non-empty".into())
        })?;
    let accept_index = input
        .get("accept_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let Some(payment_requirements) = accepts.get(accept_index) else {
        return Err(ToolError::InvalidInput(format!(
            "accept_index {accept_index} is out of range"
        )));
    };
    if !payment_requirements.is_object() {
        return Err(ToolError::InvalidInput(
            "selected payment requirements must be an object".into(),
        ));
    }
    Ok(payment_requirements.clone())
}

fn payment_x402_version(input: &Value, payment_payload: &Value) -> u64 {
    input
        .get("x402_version")
        .and_then(Value::as_u64)
        .or_else(|| payment_payload.get("x402Version").and_then(Value::as_u64))
        .or_else(|| {
            input
                .get("payment_required")
                .and_then(|value| value.get("x402Version"))
                .and_then(Value::as_u64)
        })
        .unwrap_or(1)
}

#[derive(Debug, Clone)]
struct PaymentFacilitatorResult {
    status_code: u16,
    body: Value,
    body_text: String,
    truncated_body: bool,
}

impl PaymentFacilitatorResult {
    fn is_http_success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }

    fn is_valid(&self) -> bool {
        self.is_http_success()
            && self
                .body
                .get("isValid")
                .or_else(|| self.body.get("valid"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
    }

    fn into_json(self) -> Value {
        json!({
            "status_code": self.status_code,
            "body": self.body,
            "body_text": self.body_text,
            "truncated_body": self.truncated_body
        })
    }
}

async fn post_x402_facilitator(
    client: &reqwest::Client,
    facilitator_url: &str,
    endpoint: &str,
    body: &Value,
    max_response_bytes: usize,
) -> Result<PaymentFacilitatorResult, ToolError> {
    let url = x402_facilitator_endpoint(facilitator_url, endpoint);
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let status_code = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let (body_text, truncated_body) = decode_and_truncate(&bytes, max_response_bytes);
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(body_text.clone()));
    Ok(PaymentFacilitatorResult {
        status_code,
        body,
        body_text,
        truncated_body,
    })
}

fn x402_facilitator_endpoint(base_url: &str, endpoint: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), endpoint)
}

#[derive(Debug, Clone, Serialize)]
struct SpendAssessment {
    max_amount: f64,
    required_amounts: Vec<f64>,
    within_limit: bool,
}

fn assess_payment_required_amount(
    payment_required: &Value,
    max_amount: f64,
) -> Result<SpendAssessment, ToolError> {
    let mut required_amounts = Vec::new();
    collect_payment_amounts(payment_required, &mut required_amounts);
    if required_amounts.is_empty() {
        return Err(ToolError::InvalidInput(
            "x402 retry could not find a numeric required amount to compare with max_amount".into(),
        ));
    }
    let within_limit = required_amounts
        .iter()
        .copied()
        .all(|amount| amount <= max_amount);
    Ok(SpendAssessment {
        max_amount,
        required_amounts,
        within_limit,
    })
}

fn collect_payment_amounts(value: &Value, out: &mut Vec<f64>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if is_payment_amount_key(key)
                    && let Some(amount) = payment_amount_value(value)
                {
                    out.push(amount);
                }
                collect_payment_amounts(value, out);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_payment_amounts(value, out);
            }
        }
        _ => {}
    }
}

fn is_payment_amount_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "amount"
            | "amountrequired"
            | "amount_required"
            | "maxamount"
            | "max_amount"
            | "maxamountrequired"
            | "max_amount_required"
            | "price"
            | "maxprice"
            | "max_price"
    )
}

fn payment_amount_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|value| value.is_finite() && *value >= 0.0)
}

#[derive(Debug, Clone)]
struct PaymentHttpResult {
    status_code: u16,
    body: String,
    truncated_body: bool,
    payment_required: Option<Value>,
    payment_response: Option<Value>,
}

impl PaymentHttpResult {
    fn into_json(self, retried: bool, spend: Option<SpendAssessment>) -> Value {
        let status = if self.status_code == 402 {
            "payment_required"
        } else if (200..300).contains(&self.status_code) {
            "success"
        } else {
            "http_error"
        };
        json!({
            "status": status,
            "status_code": self.status_code,
            "retried": retried,
            "body": self.body,
            "truncated_body": self.truncated_body,
            "payment_required": self.payment_required,
            "payment_response": self.payment_response,
            "spend": spend
        })
    }
}

async fn send_payment_request(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&str>,
    payment_signature: Option<&str>,
    max_response_bytes: usize,
) -> Result<PaymentHttpResult, ToolError> {
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
    let mut request = client.request(method, url);
    for (key, value) in headers {
        request = request.header(key, value);
    }
    if let Some(payment_signature) = payment_signature {
        request = request.header("PAYMENT-SIGNATURE", payment_signature);
    }
    if let Some(body) = body {
        request = request.body(body.to_string());
    }
    let response = request
        .send()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let status_code = response.status().as_u16();
    let payment_required = decode_payment_header(response.headers(), "PAYMENT-REQUIRED");
    let payment_response = decode_payment_header(response.headers(), "PAYMENT-RESPONSE");
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let (body, truncated_body) = decode_and_truncate(&bytes, max_response_bytes);
    Ok(PaymentHttpResult {
        status_code,
        body,
        truncated_body,
        payment_required,
        payment_response,
    })
}

fn decode_payment_header(headers: &reqwest::header::HeaderMap, name: &str) -> Option<Value> {
    let value = headers.get(name)?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }
    decode_payment_header_value(value).ok()
}

fn decode_payment_header_value(value: &str) -> Result<Value, ToolError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ToolError::InvalidInput(
            "x402 payment header value must not be empty".into(),
        ));
    }
    let decoded = general_purpose::STANDARD
        .decode(value)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(value))
        .map_err(|e| ToolError::InvalidInput(format!("invalid x402 payment header: {e}")))?;
    serde_json::from_slice(&decoded)
        .map_err(|e| ToolError::InvalidInput(format!("invalid x402 payment header JSON: {e}")))
}

fn encode_payment_header_value(value: &Value) -> Result<String, ToolError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| ToolError::Execution(format!("failed to encode x402 payment header: {e}")))?;
    Ok(general_purpose::STANDARD.encode(bytes))
}

fn artifact_extension(format: &str) -> Result<&'static str, ToolError> {
    match format {
        "txt" => Ok("txt"),
        "md" => Ok("md"),
        "csv" => Ok("csv"),
        "json" => Ok("json"),
        "html" => Ok("html"),
        "svg" => Ok("svg"),
        "png" => Ok("png"),
        "jpg" | "jpeg" => Ok("jpg"),
        "gif" => Ok("gif"),
        "webp" => Ok("webp"),
        "pdf" => Ok("pdf"),
        "docx" => Ok("docx"),
        "xlsx" => Ok("xlsx"),
        "pptx" => Ok("pptx"),
        "mp3" => Ok("mp3"),
        "wav" => Ok("wav"),
        "webm" => Ok("webm"),
        "m4a" => Ok("m4a"),
        "ogg" => Ok("ogg"),
        other => Err(ToolError::InvalidInput(format!(
            "unsupported artifact format `{other}`"
        ))),
    }
}

fn artifact_media_type(format: &str) -> Result<&'static str, ToolError> {
    match artifact_extension(format)? {
        "txt" => Ok("text/plain"),
        "md" => Ok("text/markdown"),
        "csv" => Ok("text/csv"),
        "json" => Ok("application/json"),
        "html" => Ok("text/html"),
        "svg" => Ok("image/svg+xml"),
        "png" => Ok("image/png"),
        "jpg" => Ok("image/jpeg"),
        "gif" => Ok("image/gif"),
        "webp" => Ok("image/webp"),
        "pdf" => Ok("application/pdf"),
        "docx" => Ok("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xlsx" => Ok("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "pptx" => Ok("application/vnd.openxmlformats-officedocument.presentationml.presentation"),
        "mp3" => Ok("audio/mpeg"),
        "wav" => Ok("audio/wav"),
        "webm" => Ok("audio/webm"),
        "m4a" => Ok("audio/mp4"),
        "ogg" => Ok("audio/ogg"),
        _ => Err(ToolError::InvalidInput(format!(
            "unsupported artifact format `{format}`"
        ))),
    }
}

fn safe_artifact_basename(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else if ch.is_whitespace() && !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 64 {
            break;
        }
    }
    let out = out.trim_matches(['-', '.', '_']).to_string();
    if out.is_empty() {
        "artifact".into()
    } else {
        out
    }
}

fn scoped_artifact_path(output_dir: &Path, filename: &str) -> Result<PathBuf, ToolError> {
    if filename.contains('/') || filename.contains('\\') || filename == "." || filename == ".." {
        return Err(ToolError::InvalidInput("invalid artifact filename".into()));
    }
    Ok(output_dir.join(filename))
}

fn artifact_export_destination(
    destination: &Path,
    artifact: &GeneratedArtifact,
) -> Result<PathBuf, ToolError> {
    if destination.as_os_str().is_empty() {
        return Err(ToolError::InvalidInput(
            "export destination must not be empty".into(),
        ));
    }
    if destination.is_dir() {
        let filename = artifact
            .path
            .file_name()
            .ok_or_else(|| ToolError::InvalidInput("artifact filename is invalid".into()))?;
        Ok(destination.join(filename))
    } else {
        Ok(destination.to_path_buf())
    }
}

fn chrono_like_timestamp() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{millis}")
}

static UNIQUE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_suffix() -> String {
    let counter = UNIQUE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{}-{counter}",
        std::process::id(),
        chrono_like_timestamp()
    )
}

fn generated_artifact_from_path(path: PathBuf) -> Result<Option<GeneratedArtifact>, ToolError> {
    let Some(format) = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return Ok(None);
    };
    if artifact_extension(&format).is_err() {
        return Ok(None);
    }
    let Some(id) = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_string)
    else {
        return Ok(None);
    };
    let metadata = std::fs::metadata(&path).map_err(|e| ToolError::Execution(e.to_string()))?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64);
    Ok(Some(GeneratedArtifact {
        id,
        format,
        path,
        bytes: metadata.len(),
        modified_ms,
    }))
}

fn safe_artifact_reference(value: &str) -> Result<String, ToolError> {
    let value = value.trim();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(ToolError::InvalidInput("invalid artifact id".into()));
    }
    Ok(value.to_string())
}

fn ensure_artifact_stays_scoped(output_dir: &Path, path: &Path) -> Result<(), ToolError> {
    let base = output_dir
        .canonicalize()
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    let artifact = path
        .canonicalize()
        .map_err(|e| ToolError::Execution(e.to_string()))?;
    if artifact.starts_with(base) {
        Ok(())
    } else {
        Err(ToolError::InvalidInput(
            "artifact path escaped artifact cache".into(),
        ))
    }
}

fn spawn_os_open(path: &Path) -> Result<(), ToolError> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    let status = command
        .status()
        .map_err(|e| ToolError::Execution(format!("failed to open artifact: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ToolError::Execution(format!(
            "artifact opener exited with status {status}"
        )))
    }
}

fn render_artifact(
    format: &str,
    title: &str,
    content: &str,
    rows: Option<&Value>,
) -> Result<Vec<u8>, ToolError> {
    match format {
        "txt" | "md" => Ok(content.as_bytes().to_vec()),
        "csv" => Ok(render_csv(rows, content).into_bytes()),
        "json" => {
            let value = json!({
                "title": title,
                "content": content,
                "rows": rows.cloned().unwrap_or(Value::Null)
            });
            serde_json::to_vec_pretty(&value).map_err(|e| ToolError::Execution(e.to_string()))
        }
        "html" => Ok(render_html(title, content).into_bytes()),
        "svg" => Ok(render_svg(title, content).into_bytes()),
        "pdf" => Ok(render_pdf(title, content)),
        "docx" => Ok(render_docx(title, content)),
        "xlsx" => Ok(render_xlsx(title, rows, content)),
        "pptx" => Ok(render_pptx(title, content)),
        other => Err(ToolError::InvalidInput(format!(
            "unsupported artifact format `{other}`"
        ))),
    }
}

fn render_csv(rows: Option<&Value>, fallback: &str) -> String {
    let Some(Value::Array(rows)) = rows else {
        return fallback.to_string();
    };
    let mut keys = Vec::<String>::new();
    for row in rows {
        if let Value::Object(map) = row {
            for key in map.keys() {
                if !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
        }
    }
    let mut out = String::new();
    if !keys.is_empty() {
        out.push_str(&csv_row(keys.iter().map(String::as_str)));
        for row in rows {
            let values = keys
                .iter()
                .map(|key| row.get(key).map(cell_to_string).unwrap_or_default());
            out.push_str(&csv_row(values));
        }
        return out;
    }
    for row in rows {
        match row {
            Value::Array(cells) => out.push_str(&csv_row(cells.iter().map(cell_to_string))),
            other => out.push_str(&csv_row([cell_to_string(other)])),
        }
    }
    out
}

fn csv_row<I>(cells: I) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut row = cells
        .into_iter()
        .map(|cell| csv_escape(cell.as_ref()))
        .collect::<Vec<_>>()
        .join(",");
    row.push('\n');
    row
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn render_html(title: &str, content: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title></head><body><main><h1>{}</h1><pre>{}</pre></main></body></html>",
        xml_escape(title),
        xml_escape(title),
        xml_escape(content)
    )
}

fn render_svg(title: &str, content: &str) -> String {
    let mut svg = String::from(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540" role="img">"#,
    );
    let _ = write!(svg, "<title>{}</title>", xml_escape(title));
    svg.push_str(r##"<rect width="960" height="540" rx="24" fill="#f8fafc"/>"##);
    svg.push_str(r##"<rect x="48" y="48" width="864" height="444" rx="18" fill="#ffffff" stroke="#cbd5e1" stroke-width="2"/>"##);
    let _ = write!(
        svg,
        r##"<text x="80" y="112" fill="#0f172a" font-family="Arial, sans-serif" font-size="32" font-weight="700">{}</text>"##,
        xml_escape(title)
    );
    let mut y = 170;
    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(10)
    {
        let _ = write!(
            svg,
            r##"<text x="80" y="{y}" fill="#334155" font-family="Arial, sans-serif" font-size="22">{}</text>"##,
            xml_escape(line)
        );
        y += 34;
    }
    if y == 170 {
        svg.push_str(
            r##"<text x="80" y="170" fill="#64748b" font-family="Arial, sans-serif" font-size="22">Generated SVG artifact</text>"##,
        );
    }
    svg.push_str("</svg>");
    svg
}

fn render_pdf(title: &str, content: &str) -> Vec<u8> {
    let lines = std::iter::once(title)
        .chain(content.lines())
        .take(42)
        .collect::<Vec<_>>();
    let mut stream = String::from("BT /F1 12 Tf 72 760 Td 16 TL ");
    for line in lines {
        let _ = write!(stream, "({}) Tj T* ", pdf_escape(line));
    }
    stream.push_str("ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (idx, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", idx + 1, object).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            xref
        )
        .as_bytes(),
    );
    pdf
}

fn render_docx(title: &str, content: &str) -> Vec<u8> {
    let mut body = String::new();
    body.push_str(&word_paragraph(title, true));
    for line in content.lines() {
        body.push_str(&word_paragraph(line, false));
    }
    zip_store(vec![
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec(),
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "word/document.xml",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}<w:sectPr/></w:body></w:document>"#,
                body
            )
            .into_bytes(),
        ),
    ])
}

fn word_paragraph(text: &str, bold: bool) -> String {
    let run_props = if bold { "<w:rPr><w:b/></w:rPr>" } else { "" };
    format!(
        "<w:p><w:r>{run_props}<w:t>{}</w:t></w:r></w:p>",
        xml_escape(text)
    )
}

fn render_xlsx(title: &str, rows: Option<&Value>, fallback: &str) -> Vec<u8> {
    let table = rows_to_table(rows, fallback);
    let mut sheet = String::new();
    for (row_idx, row) in table.iter().enumerate() {
        let _ = write!(sheet, "<row r=\"{}\">", row_idx + 1);
        for (col_idx, cell) in row.iter().enumerate() {
            let _ = write!(
                sheet,
                "<c r=\"{}{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                column_name(col_idx),
                row_idx + 1,
                xml_escape(cell)
            );
        }
        sheet.push_str("</row>");
    }
    if sheet.is_empty() {
        sheet = format!(
            "<row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>{}</t></is></c></row>",
            xml_escape(title)
        );
    }
    zip_store(vec![
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.to_vec(),
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "xl/workbook.xml",
            br#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec(),
        ),
        (
            "xl/_rels/workbook.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "xl/worksheets/sheet1.xml",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{}</sheetData></worksheet>"#,
                sheet
            )
            .into_bytes(),
        ),
    ])
}

fn rows_to_table(rows: Option<&Value>, fallback: &str) -> Vec<Vec<String>> {
    let Some(Value::Array(rows)) = rows else {
        return fallback
            .lines()
            .map(|line| vec![line.to_string()])
            .collect();
    };
    let mut keys = Vec::<String>::new();
    for row in rows {
        if let Value::Object(map) = row {
            for key in map.keys() {
                if !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
        }
    }
    if !keys.is_empty() {
        let mut table = vec![keys.clone()];
        for row in rows {
            table.push(
                keys.iter()
                    .map(|key| row.get(key).map(cell_to_string).unwrap_or_default())
                    .collect(),
            );
        }
        return table;
    }
    rows.iter()
        .map(|row| match row {
            Value::Array(cells) => cells.iter().map(cell_to_string).collect(),
            other => vec![cell_to_string(other)],
        })
        .collect()
}

fn cell_to_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn column_name(mut index: usize) -> String {
    let mut name = String::new();
    loop {
        let rem = index % 26;
        name.insert(0, char::from(b'A' + rem as u8));
        if index < 26 {
            return name;
        }
        index = index / 26 - 1;
    }
}

fn render_pptx(title: &str, content: &str) -> Vec<u8> {
    let slide_text = format!(
        "{}\n{}",
        title,
        content.lines().take(12).collect::<Vec<_>>().join("\n")
    );
    zip_store(vec![
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/></Types>"#.to_vec(),
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "ppt/presentation.xml",
            br#"<?xml version="1.0" encoding="UTF-8"?><p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#.to_vec(),
        ),
        (
            "ppt/_rels/presentation.xml.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#.to_vec(),
        ),
        (
            "ppt/slides/slide1.xml",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id="2" name="Content"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
                xml_escape(&slide_text)
            )
            .into_bytes(),
        ),
    ])
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn pdf_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '(' | ')' | '\\' => vec!['\\', ch],
            '\r' | '\n' => vec![' '],
            ch if ch.is_ascii() => vec![ch],
            _ => vec!['?'],
        })
        .collect()
}

fn zip_store(files: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content) in files {
        let offset = out.len() as u32;
        let crc = crc32(&content);
        let name_bytes = name.as_bytes();
        write_u32(&mut out, 0x0403_4b50);
        write_u16(&mut out, 20);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        write_u32(&mut out, crc);
        write_u32(&mut out, content.len() as u32);
        write_u32(&mut out, content.len() as u32);
        write_u16(&mut out, name_bytes.len() as u16);
        write_u16(&mut out, 0);
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(&content);

        write_u32(&mut central, 0x0201_4b50);
        write_u16(&mut central, 20);
        write_u16(&mut central, 20);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u32(&mut central, crc);
        write_u32(&mut central, content.len() as u32);
        write_u32(&mut central, content.len() as u32);
        write_u16(&mut central, name_bytes.len() as u16);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u16(&mut central, 0);
        write_u32(&mut central, 0);
        write_u32(&mut central, offset);
        central.extend_from_slice(name_bytes);
    }
    let central_offset = out.len() as u32;
    let central_len = central.len() as u32;
    let entries = count_zip_entries(&central);
    out.extend_from_slice(&central);
    write_u32(&mut out, 0x0605_4b50);
    write_u16(&mut out, 0);
    write_u16(&mut out, 0);
    write_u16(&mut out, entries);
    write_u16(&mut out, entries);
    write_u32(&mut out, central_len);
    write_u32(&mut out, central_offset);
    write_u16(&mut out, 0);
    out
}

fn count_zip_entries(central: &[u8]) -> u16 {
    central
        .windows(4)
        .filter(|window| *window == [0x50, 0x4b, 0x01, 0x02])
        .count() as u16
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

struct RestrictedCommand {
    program: String,
    args: Vec<String>,
}

fn parse_restricted_command(
    command: &str,
    allowlist: &[String],
) -> Result<RestrictedCommand, ToolError> {
    let command = command.trim();
    if command.is_empty() {
        return Err(ToolError::InvalidInput("empty command".into()));
    }
    if command
        .chars()
        .any(|ch| matches!(ch, ';' | '&' | '|' | '<' | '>' | '`' | '$' | '\n' | '\r'))
    {
        return Err(ToolError::InvalidInput(
            "restricted shell commands cannot contain shell metacharacters".into(),
        ));
    }
    let mut parts = command.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| ToolError::InvalidInput("empty command".into()))?;
    if program.contains('/') || program.contains('\\') {
        return Err(ToolError::InvalidInput(
            "restricted shell command must use an allowlisted command name, not a path".into(),
        ));
    }
    if !allowlist.iter().any(|allowed| allowed == program) {
        return Err(ToolError::InvalidInput(format!(
            "command `{program}` is not in the shell allowlist"
        )));
    }
    Ok(RestrictedCommand {
        program: program.into(),
        args: parts.map(str::to_string).collect(),
    })
}

fn decode_and_truncate(bytes: &[u8], max_bytes: usize) -> (String, bool) {
    let truncated = bytes.len() > max_bytes;
    let end = if truncated { max_bytes } else { bytes.len() };
    let text = String::from_utf8_lossy(&bytes[..end]).to_string();
    (text, truncated)
}

#[derive(Debug, Clone)]
pub struct McpServerSpec {
    pub id: ToolId,
    pub name: String,
    pub description: String,
    pub transport: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub url: Option<String>,
    pub permissions: ToolPermissions,
}

pub struct McpServerTool {
    spec: McpServerSpec,
    secret_store: Arc<dyn SecretStore>,
}

impl McpServerTool {
    pub fn new(spec: McpServerSpec) -> Self {
        Self {
            spec,
            secret_store: default_secret_store(),
        }
    }

    pub fn new_with_secret_store(spec: McpServerSpec, secret_store: Arc<dyn SecretStore>) -> Self {
        Self { spec, secret_store }
    }

    pub fn descriptor(spec: &McpServerSpec) -> ToolDescriptor {
        let mut categories = vec!["mcp".into()];
        if let Some(transport) = mcp_transport_label(spec) {
            categories.push(format!("mcp-{transport}"));
        }
        ToolDescriptor {
            id: spec.id.clone(),
            name: spec.name.clone(),
            description: spec.description.clone(),
            categories,
            input_schema: json!({
                "type": "object",
                "required": ["tool_name"],
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Tool name exposed by the MCP server."
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Arguments passed to the MCP tool.",
                        "additionalProperties": true
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional per-call timeout for the MCP exchange."
                    },
                    "headers": {
                        "type": "object",
                        "description": "Optional per-call HTTP headers for HTTP MCP servers. Secret-shaped keys are redacted in traces.",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Preserve the MCP JSON-RPC result, content array, and any structuredContent exactly when interpreting MCP tool results.".into(),
            ),
            permissions: spec.permissions.clone(),
            requires_approval: true,
            provenance: None,
        }
    }

    async fn execute_stdio(&self, input: Value) -> Result<Value, ToolError> {
        let command = self
            .spec
            .command
            .as_deref()
            .ok_or_else(|| ToolError::Execution("MCP server has no stdio command".into()))?;
        let (tool_name, arguments, timeout_ms) = mcp_call_input(&input)?;

        let mut cmd = Command::new(command);
        cmd.args(&self.spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_minimal_env(&mut cmd);
        if let Some(cwd) = &self.spec.cwd {
            cmd.current_dir(cwd);
        }
        for (key, value) in &self.spec.env {
            cmd.env(key, self.resolve_env_value(value)?);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| ToolError::Execution(format!("failed to spawn MCP server: {e}")))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::Execution("failed to open MCP stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ToolError::Execution("failed to open MCP stdout".into()))?;
        let mut lines = BufReader::new(stdout).lines();

        let run = async {
            write_json_rpc(
                &mut stdin,
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "clientInfo": {
                            "name": "shinkai",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                }),
            )
            .await?;
            let _ = read_json_rpc_response(&mut lines, 1).await?;
            write_json_rpc(
                &mut stdin,
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized"
                }),
            )
            .await?;
            write_json_rpc(
                &mut stdin,
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": tool_name,
                        "arguments": arguments
                    }
                }),
            )
            .await?;
            read_json_rpc_response(&mut lines, 2).await
        };

        let response = match timeout(Duration::from_millis(timeout_ms), run).await {
            Ok(result) => result?,
            Err(_) => {
                let _ = child.kill().await;
                return Err(ToolError::Execution(format!(
                    "MCP call timed out after {timeout_ms} ms"
                )));
            }
        };

        let _ = child.kill().await;
        if let Some(error) = response.get("error") {
            return Err(ToolError::Execution(format!("MCP error: {error}")));
        }
        Ok(response.get("result").cloned().unwrap_or(response))
    }

    async fn execute_http(&self, input: Value) -> Result<Value, ToolError> {
        let url = self
            .spec
            .url
            .as_deref()
            .ok_or_else(|| ToolError::Execution("MCP server has no HTTP URL".into()))?;
        let (tool_name, arguments, timeout_ms) = mcp_call_input(&input)?;
        let mut headers = self.resolved_headers()?;
        headers.extend(string_map(&input, "headers")?);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(format!("failed to build HTTP client: {e}")))?;
        let _ = post_json_rpc_http(
            &client,
            url,
            &headers,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "shinkai",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }),
        )
        .await?;
        let response = post_json_rpc_http(
            &client,
            url,
            &headers,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments
                }
            }),
        )
        .await?;
        Ok(response.get("result").cloned().unwrap_or(response))
    }

    async fn execute_sse(&self, input: Value) -> Result<Value, ToolError> {
        let url = self
            .spec
            .url
            .as_deref()
            .ok_or_else(|| ToolError::Execution("MCP SSE server has no URL".into()))?;
        let (tool_name, arguments, timeout_ms) = mcp_call_input(&input)?;
        let mut headers = self.resolved_headers()?;
        headers.extend(string_map(&input, "headers")?);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(format!("failed to build HTTP client: {e}")))?;

        let run = async {
            let mut request = client.get(url);
            for (key, value) in &headers {
                request = request.header(key.as_str(), value.as_str());
            }
            let response = request
                .send()
                .await
                .map_err(|e| ToolError::Execution(format!("SSE MCP connect failed: {e}")))?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(ToolError::Execution(format!(
                    "SSE MCP endpoint returned {status}: {body}"
                )));
            }
            let mut stream = SseReader::new(response);
            let message_url = resolve_sse_endpoint_url(url, &stream.read_endpoint().await?)?;

            post_sse_message(
                &client,
                &message_url,
                &headers,
                json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "clientInfo": {
                            "name": "shinkai",
                            "version": env!("CARGO_PKG_VERSION")
                        }
                    }
                }),
            )
            .await?;
            let _ = stream.read_json_rpc_response(1).await?;
            post_sse_message(
                &client,
                &message_url,
                &headers,
                json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/initialized"
                }),
            )
            .await?;
            post_sse_message(
                &client,
                &message_url,
                &headers,
                json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {
                        "name": tool_name,
                        "arguments": arguments
                    }
                }),
            )
            .await?;
            stream.read_json_rpc_response(2).await
        };

        let response = match timeout(Duration::from_millis(timeout_ms), run).await {
            Ok(result) => result?,
            Err(_) => {
                return Err(ToolError::Execution(format!(
                    "SSE MCP call timed out after {timeout_ms} ms"
                )));
            }
        };
        if let Some(error) = response.get("error") {
            return Err(ToolError::Execution(format!("MCP error: {error}")));
        }
        Ok(response.get("result").cloned().unwrap_or(response))
    }

    fn resolved_headers(&self) -> Result<Vec<(String, String)>, ToolError> {
        self.spec
            .headers
            .iter()
            .map(|(key, value)| Ok((key.clone(), self.resolve_env_value(value)?)))
            .collect()
    }

    fn resolve_env_value(&self, value: &str) -> Result<String, ToolError> {
        resolve_secret_value(self.secret_store.as_ref(), value)
    }
}

#[async_trait]
impl Tool for McpServerTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        if self.spec.url.is_some() {
            if mcp_spec_uses_sse(&self.spec) {
                return self.execute_sse(input).await;
            }
            return self.execute_http(input).await;
        }
        self.execute_stdio(input).await
    }
}

fn mcp_call_input(input: &Value) -> Result<(String, Value, u64), ToolError> {
    let tool_name = input
        .get("tool_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ToolError::InvalidInput("missing string field `tool_name`".into()))?;
    let arguments = input.get("arguments").cloned().unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err(ToolError::InvalidInput(
            "`arguments` must be a JSON object".into(),
        ));
    }
    let timeout_ms = input
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(30_000);
    Ok((tool_name.to_string(), arguments, timeout_ms))
}

async fn post_json_rpc_http(
    client: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
    body: Value,
) -> Result<Value, ToolError> {
    let mut request = client.post(url).json(&body);
    for (key, value) in headers {
        request = request.header(key.as_str(), value.as_str());
    }
    let response = request
        .send()
        .await
        .map_err(|e| ToolError::Execution(format!("HTTP MCP request failed: {e}")))?;
    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|e| ToolError::Execution(format!("invalid HTTP MCP JSON response: {e}")))?;
    if !status.is_success() {
        return Err(ToolError::Execution(format!(
            "HTTP MCP server returned {status}: {value}"
        )));
    }
    if let Some(error) = value.get("error") {
        return Err(ToolError::Execution(format!("MCP error: {error}")));
    }
    Ok(value)
}

async fn post_sse_message(
    client: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
    body: Value,
) -> Result<(), ToolError> {
    let mut request = client.post(url).json(&body);
    for (key, value) in headers {
        request = request.header(key.as_str(), value.as_str());
    }
    let response = request
        .send()
        .await
        .map_err(|e| ToolError::Execution(format!("SSE MCP POST failed: {e}")))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    Err(ToolError::Execution(format!(
        "SSE MCP message endpoint returned {status}: {body}"
    )))
}

struct SseReader {
    response: reqwest::Response,
    buffer: String,
}

impl SseReader {
    fn new(response: reqwest::Response) -> Self {
        Self {
            response,
            buffer: String::new(),
        }
    }

    async fn read_endpoint(&mut self) -> Result<String, ToolError> {
        while let Some(event) = self.next_event().await? {
            let data = event.data.trim();
            if event.event.as_deref() == Some("endpoint")
                || data.starts_with("http://")
                || data.starts_with("https://")
                || data.starts_with('/')
            {
                return Ok(data.to_string());
            }
        }
        Err(ToolError::Execution(
            "SSE MCP stream closed before endpoint event".into(),
        ))
    }

    async fn read_json_rpc_response(&mut self, id: u64) -> Result<Value, ToolError> {
        while let Some(event) = self.next_event().await? {
            let data = event.data.trim();
            if data.is_empty() || event.event.as_deref() == Some("endpoint") {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(value);
            }
        }
        Err(ToolError::Execution(format!(
            "SSE MCP stream closed before response id {id}"
        )))
    }

    async fn next_event(&mut self) -> Result<Option<SseEvent>, ToolError> {
        loop {
            if let Some(event) = take_next_sse_event(&mut self.buffer) {
                return Ok(Some(event));
            }
            let Some(chunk) =
                self.response.chunk().await.map_err(|e| {
                    ToolError::Execution(format!("failed to read SSE MCP stream: {e}"))
                })?
            else {
                return Ok(None);
            };
            let text = std::str::from_utf8(&chunk).map_err(|_| {
                ToolError::Execution("SSE MCP stream returned non-UTF-8 data".into())
            })?;
            self.buffer.push_str(text);
        }
    }
}

struct SseEvent {
    event: Option<String>,
    data: String,
}

fn take_next_sse_event(buffer: &mut String) -> Option<SseEvent> {
    let (index, separator_len) = next_sse_separator(buffer)?;
    let raw = buffer[..index].to_string();
    buffer.drain(..index + separator_len);
    parse_sse_event(&raw)
}

fn next_sse_separator(buffer: &str) -> Option<(usize, usize)> {
    let crlf = buffer.find("\r\n\r\n").map(|index| (index, 4));
    let lf = buffer.find("\n\n").map(|index| (index, 2));
    match (crlf, lf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn parse_sse_event(raw: &str) -> Option<SseEvent> {
    let mut event = None;
    let mut data = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(sse_field_value(value).to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(sse_field_value(value).to_string());
        }
    }
    if event.is_none() && data.is_empty() {
        return None;
    }
    Some(SseEvent {
        event,
        data: data.join("\n"),
    })
}

fn sse_field_value(value: &str) -> &str {
    value.strip_prefix(' ').unwrap_or(value)
}

fn resolve_sse_endpoint_url(base: &str, endpoint: &str) -> Result<String, ToolError> {
    let endpoint = endpoint.trim();
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return Ok(endpoint.to_string());
    }
    let base = reqwest::Url::parse(base)
        .map_err(|e| ToolError::InvalidInput(format!("invalid SSE MCP URL: {e}")))?;
    base.join(endpoint)
        .map(|url| url.to_string())
        .map_err(|e| ToolError::InvalidInput(format!("invalid SSE MCP endpoint URL: {e}")))
}

#[derive(Debug, Clone)]
pub struct ExternalAgentSpec {
    pub id: ToolId,
    pub name: String,
    pub description: String,
    pub transport: String,
    pub endpoint: String,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
    pub auth_schemes: Vec<String>,
    pub header_keys: Vec<String>,
}

pub struct ExternalAgentTool {
    spec: ExternalAgentSpec,
    secret_store: Arc<dyn SecretStore>,
}

impl ExternalAgentTool {
    pub fn new(spec: ExternalAgentSpec) -> Self {
        Self {
            spec,
            secret_store: default_secret_store(),
        }
    }

    pub fn new_with_secret_store(
        spec: ExternalAgentSpec,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self { spec, secret_store }
    }

    pub fn descriptor(spec: &ExternalAgentSpec) -> ToolDescriptor {
        let mut description = spec.description.clone();
        let mut hints = Vec::new();
        let a2a = external_agent_transport_is_a2a(&spec.transport);
        if !spec.transport.trim().is_empty() {
            hints.push(format!("transport: {}", spec.transport));
        }
        if !spec.input_modes.is_empty() {
            hints.push(format!("input modes: {}", spec.input_modes.join(",")));
        }
        if !spec.output_modes.is_empty() {
            hints.push(format!("output modes: {}", spec.output_modes.join(",")));
        }
        if !spec.header_keys.is_empty() {
            hints.push(format!("header keys: {}", spec.header_keys.join(",")));
        }
        if !hints.is_empty() {
            description.push('\n');
            description.push_str(&hints.join("; "));
        }
        let mut categories = vec!["agent".into(), "subagent".into(), "external-agent".into()];
        if a2a {
            categories.push("a2a".into());
        } else {
            categories.push("http-json".into());
        }
        let input_schema = if a2a {
            json!({
                "type": "object",
                "required": ["prompt"],
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Task prompt to send to the external agent."
                    },
                    "message_id": {
                        "type": "string",
                        "description": "Optional A2A message id. A new id is generated when omitted."
                    },
                    "task_id": {
                        "type": "string",
                        "description": "Optional A2A task id for continuation."
                    },
                    "context_id": {
                        "type": "string",
                        "description": "Optional A2A context id for continuation."
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional metadata object forwarded in the JSON-RPC params.",
                        "additionalProperties": true
                    },
                    "headers": {
                        "type": "object",
                        "description": "Optional per-call HTTP headers such as Authorization. Secret-shaped keys are redacted in traces.",
                        "additionalProperties": { "type": "string" }
                    },
                    "method": {
                        "type": "string",
                        "description": "Optional JSON-RPC method override. Defaults to message/send."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional request timeout in milliseconds."
                    }
                },
                "additionalProperties": false
            })
        } else {
            json!({
                "type": "object",
                "required": ["prompt"],
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Task prompt to send to the external agent."
                    },
                    "body": {
                        "type": "object",
                        "description": "Optional JSON request body override. The prompt is inserted when the body omits it.",
                        "additionalProperties": true
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional metadata object forwarded in the JSON body.",
                        "additionalProperties": true
                    },
                    "headers": {
                        "type": "object",
                        "description": "Optional per-call HTTP headers such as Authorization. Secret-shaped keys are redacted in traces.",
                        "additionalProperties": { "type": "string" }
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional request timeout in milliseconds."
                    }
                },
                "additionalProperties": false
            })
        };
        let output_guidance = if a2a {
            "Preserve the external agent JSON-RPC result, task/message ids, status, and returned artifacts exactly when interpreting A2A results."
        } else {
            "Preserve the external agent HTTP JSON response exactly, including ids, status fields, artifacts, citations, and provider metadata."
        };
        ToolDescriptor {
            id: spec.id.clone(),
            name: spec.name.clone(),
            description,
            categories,
            input_schema,
            output_interpretation_guidance: Some(output_guidance.into()),
            permissions: ToolPermissions {
                network: true,
                secrets: !spec.auth_schemes.is_empty() || !spec.header_keys.is_empty(),
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: None,
        }
    }

    fn resolved_headers(
        &self,
        headers: Vec<(String, String)>,
    ) -> Result<Vec<(String, String)>, ToolError> {
        let mut resolved = headers
            .into_iter()
            .map(|(key, value)| {
                resolve_secret_value(self.secret_store.as_ref(), &value).map(|value| (key, value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for header_key in &self.spec.header_keys {
            if resolved
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case(header_key))
            {
                continue;
            }
            if let Some(value) = self.resolve_declared_header(header_key)? {
                resolved.push((header_key.clone(), value));
            }
        }
        Ok(resolved)
    }

    fn resolve_declared_header(&self, header_key: &str) -> Result<Option<String>, ToolError> {
        let Ok(id) = SecretId::new(header_key) else {
            return Ok(None);
        };
        let record = match self.secret_store.show(&id) {
            Ok(record) => record,
            Err(SecretStoreError::NotFound(_)) => return Ok(None),
            Err(err) => {
                return Err(ToolError::Execution(format!(
                    "failed to inspect secret for header `{header_key}`: {err}"
                )));
            }
        };
        resolve_secret_value(
            self.secret_store.as_ref(),
            &SecretHandle {
                id: record.id,
                version: record.current_version,
            }
            .to_string(),
        )
        .map(Some)
    }
}

#[async_trait]
impl Tool for ExternalAgentTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let a2a = external_agent_transport_is_a2a(&self.spec.transport);
        let (body, headers, timeout_ms) = if a2a {
            external_agent_a2a_call_input(&input)?
        } else {
            external_agent_http_json_call_input(&input)?
        };
        let headers = self.resolved_headers(headers)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|e| ToolError::Execution(format!("failed to build HTTP client: {e}")))?;
        let mut request = client.post(&self.spec.endpoint).json(&body);
        for (key, value) in headers {
            request = request.header(key, value);
        }
        let response = request.send().await.map_err(|e| {
            ToolError::Execution(format!(
                "{} request failed: {e}",
                external_agent_transport_label(a2a)
            ))
        })?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(|e| {
            ToolError::Execution(format!(
                "invalid {} JSON response: {e}",
                external_agent_transport_label(a2a)
            ))
        })?;
        if !status.is_success() {
            return Err(ToolError::Execution(format!(
                "{} endpoint returned {status}: {value}",
                external_agent_transport_label(a2a)
            )));
        }
        if let Some(error) = value.get("error") {
            return Err(ToolError::Execution(format!(
                "{} error: {error}",
                external_agent_transport_label(a2a)
            )));
        }
        if a2a {
            Ok(value.get("result").cloned().unwrap_or(value))
        } else {
            Ok(value)
        }
    }
}

fn external_agent_transport_is_a2a(transport: &str) -> bool {
    transport
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|part| part == "a2a")
}

fn external_agent_transport_label(a2a: bool) -> &'static str {
    if a2a { "A2A" } else { "external agent" }
}

fn resolve_secret_value(secret_store: &dyn SecretStore, value: &str) -> Result<String, ToolError> {
    if !value.trim().starts_with("secret://") {
        return Ok(value.to_string());
    }
    let handle = value
        .parse::<SecretHandle>()
        .map_err(|err| ToolError::InvalidInput(format!("invalid secret handle: {err}")))?;
    let secret = secret_store
        .resolve(&handle)
        .map_err(|err| ToolError::Execution(format!("failed to resolve secret handle: {err}")))?;
    Ok(secret.expose().to_string())
}

fn resolve_secret_reference_value(
    secret_store: &dyn SecretStore,
    reference: &str,
) -> Result<String, ToolError> {
    let reference = reference.trim();
    if reference.starts_with("secret://") {
        return resolve_secret_value(secret_store, reference);
    }
    let id = SecretId::new(reference)
        .map_err(|err| ToolError::InvalidInput(format!("invalid secret id: {err}")))?;
    let record = secret_store
        .show(&id)
        .map_err(|err| ToolError::Execution(format!("failed to inspect secret id: {err}")))?;
    resolve_secret_value(
        secret_store,
        &SecretHandle {
            id: record.id,
            version: record.current_version,
        }
        .to_string(),
    )
}

type ExternalAgentCallInput = (Value, Vec<(String, String)>, u64);

fn external_agent_a2a_call_input(input: &Value) -> Result<ExternalAgentCallInput, ToolError> {
    let prompt = input
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .ok_or_else(|| ToolError::InvalidInput("missing string field `prompt`".into()))?;
    let method = input
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|method| !method.is_empty())
        .unwrap_or("message/send");
    let message_id = input
        .get("message_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("msg-{}", uuid::Uuid::new_v4()));
    let mut message = json!({
        "kind": "message",
        "messageId": message_id,
        "role": "user",
        "parts": [{ "kind": "text", "text": prompt }]
    });
    if let Some(task_id) = optional_string(input, "task_id") {
        message["taskId"] = json!(task_id);
    }
    if let Some(context_id) = optional_string(input, "context_id") {
        message["contextId"] = json!(context_id);
    }
    let mut params = json!({ "message": message });
    if let Some(metadata) = optional_object(input, "metadata")? {
        params["metadata"] = metadata;
    }
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let headers = string_map(input, "headers")?;
    let timeout_ms = input
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(30_000);
    Ok((body, headers, timeout_ms))
}

fn external_agent_http_json_call_input(input: &Value) -> Result<ExternalAgentCallInput, ToolError> {
    let prompt = input
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .ok_or_else(|| ToolError::InvalidInput("missing string field `prompt`".into()))?;
    let mut body = optional_object(input, "body")?.unwrap_or_else(|| json!({}));
    if body.get("prompt").is_none() {
        body["prompt"] = json!(prompt);
    }
    if let Some(metadata) = optional_object(input, "metadata")? {
        body["metadata"] = metadata;
    }
    let headers = string_map(input, "headers")?;
    let timeout_ms = input
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(30_000);
    Ok((body, headers, timeout_ms))
}

fn optional_string(input: &Value, field: &str) -> Option<String> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn optional_object(input: &Value, field: &str) -> Result<Option<Value>, ToolError> {
    match input.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) if value.is_object() => Ok(Some(value.clone())),
        Some(_) => Err(ToolError::InvalidInput(format!(
            "`{field}` must be a JSON object"
        ))),
    }
}

fn string_map(input: &Value, field: &str) -> Result<Vec<(String, String)>, ToolError> {
    let Some(value) = input.get(field) else {
        return Ok(Vec::new());
    };
    let map = value
        .as_object()
        .ok_or_else(|| ToolError::InvalidInput(format!("`{field}` must be a JSON object")))?;
    let mut items = Vec::new();
    for (key, value) in map {
        let Some(value) = value.as_str() else {
            return Err(ToolError::InvalidInput(format!(
                "`{field}.{key}` must be a string"
            )));
        };
        items.push((key.clone(), value.to_string()));
    }
    Ok(items)
}

async fn write_json_rpc(
    stdin: &mut tokio::process::ChildStdin,
    value: Value,
) -> Result<(), ToolError> {
    let mut line = serde_json::to_vec(&value)
        .map_err(|e| ToolError::Execution(format!("failed to serialize MCP message: {e}")))?;
    line.push(b'\n');
    stdin
        .write_all(&line)
        .await
        .map_err(|e| ToolError::Execution(format!("failed to write MCP message: {e}")))?;
    stdin
        .flush()
        .await
        .map_err(|e| ToolError::Execution(format!("failed to flush MCP message: {e}")))
}

async fn read_json_rpc_response(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    id: u64,
) -> Result<Value, ToolError> {
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| ToolError::Execution(format!("failed to read MCP response: {e}")))?
    {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("id").and_then(Value::as_u64) == Some(id) {
            return Ok(value);
        }
    }
    Err(ToolError::Execution(format!(
        "MCP server closed stdout before response id {id}"
    )))
}

pub fn register_allowed_mcp_tools_from_env(registry: &mut ToolRegistry) -> usize {
    let Ok(packages) = AdapterRegistry::from_env().list() else {
        return 0;
    };
    register_allowed_mcp_tools(registry, packages)
}

pub fn register_allowed_adapter_tools_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    extra_provenance: Option<&str>,
) -> usize {
    let packages = packages.into_iter().collect::<Vec<_>>();
    register_allowed_mcp_tools_with_provenance(registry, packages.clone(), extra_provenance)
        + register_allowed_external_agent_tools_with_provenance(
            registry,
            packages,
            extra_provenance,
        )
}

pub fn register_allowed_adapter_tools_for_resource_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    resource: &str,
    extra_provenance: Option<&str>,
) -> usize {
    let packages = packages.into_iter().collect::<Vec<_>>();
    register_allowed_mcp_tools_for_resource_with_provenance(
        registry,
        packages.clone(),
        resource,
        extra_provenance,
    ) + register_allowed_external_agent_tools_for_resource_with_provenance(
        registry,
        packages,
        resource,
        extra_provenance,
    )
}

pub fn register_allowed_adapter_tools_for_category_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    category: &str,
    extra_provenance: Option<&str>,
) -> usize {
    let packages = packages.into_iter().collect::<Vec<_>>();
    register_allowed_mcp_tools_for_category_with_provenance(
        registry,
        packages.clone(),
        category,
        extra_provenance,
    ) + register_allowed_external_agent_tools_for_category_with_provenance(
        registry,
        packages,
        category,
        extra_provenance,
    )
}

pub fn register_allowed_mcp_tools(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
) -> usize {
    register_allowed_mcp_tools_impl(registry, packages, |_, _| true, false, None)
}

pub fn register_allowed_mcp_tools_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    extra_provenance: Option<&str>,
) -> usize {
    register_allowed_mcp_tools_impl(registry, packages, |_, _| true, false, extra_provenance)
}

pub fn register_allowed_mcp_tools_for_resource(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    resource: &str,
) -> usize {
    register_allowed_mcp_tools_for_resource_with_provenance(registry, packages, resource, None)
}

pub fn register_allowed_mcp_tools_for_resource_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    resource: &str,
    extra_provenance: Option<&str>,
) -> usize {
    register_allowed_mcp_tools_impl(
        registry,
        packages,
        |package, spec| mcp_resource_matches(package, spec, resource),
        true,
        extra_provenance,
    )
}

pub fn register_allowed_mcp_tools_for_category_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    category: &str,
    extra_provenance: Option<&str>,
) -> usize {
    register_allowed_mcp_tools_impl(
        registry,
        packages,
        |package, spec| mcp_category_matches(package, spec, category),
        true,
        extra_provenance,
    )
}

fn register_allowed_mcp_tools_impl(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    include: impl Fn(&NormalizedPackage, &McpServerSpec) -> bool,
    skip_existing: bool,
    extra_provenance: Option<&str>,
) -> usize {
    let mut registered = 0;
    for package in packages {
        if package.quarantined || package.adapter != AdapterKind::Mcp {
            continue;
        }
        let Ok(specs) = mcp_specs_from_package(&package) else {
            continue;
        };
        for spec in specs {
            if !mcp_spec_has_supported_runtime(&spec) {
                continue;
            }
            if !include(&package, &spec) {
                continue;
            }
            let mut descriptor = McpServerTool::descriptor(&spec);
            descriptor.provenance = Some(mcp_tool_provenance(&package, extra_provenance));
            if skip_existing && registry.contains(&descriptor.id) {
                continue;
            }
            registry.register(descriptor, Arc::new(McpServerTool::new(spec)));
            registered += 1;
        }
    }
    registered
}

fn mcp_tool_provenance(package: &NormalizedPackage, extra_provenance: Option<&str>) -> String {
    let mut parts = vec![format!("adapter_package={}", package.id)];
    if let Some(provenance) = package.provenance.as_deref().map(str::trim)
        && !provenance.is_empty()
    {
        parts.push(provenance.to_string());
    }
    if let Some(provenance) = extra_provenance.map(str::trim)
        && !provenance.is_empty()
    {
        parts.push(provenance.to_string());
    }
    parts.join("; ")
}

fn mcp_resource_matches(package: &NormalizedPackage, spec: &McpServerSpec, resource: &str) -> bool {
    if resource == "*" || package.id == resource || spec.id.0 == resource || spec.name == resource {
        return true;
    }
    let short_name = spec.name.strip_prefix("MCP: ").unwrap_or(&spec.name);
    package.capabilities.iter().any(|capability| {
        capability.kind == CapabilityKind::Tool
            && (capability.id == resource || capability.name == resource)
            && (capability.id == spec.id.0 || capability.name == short_name)
    })
}

fn mcp_category_matches(package: &NormalizedPackage, spec: &McpServerSpec, category: &str) -> bool {
    let category = category.trim();
    if category == "*" || category.eq_ignore_ascii_case("mcp") || package.id == category {
        return true;
    }
    mcp_transport_label(spec)
        .map(|transport| format!("mcp-{transport}"))
        .is_some_and(|transport_category| transport_category.eq_ignore_ascii_case(category))
}

fn mcp_spec_uses_sse(spec: &McpServerSpec) -> bool {
    spec.transport.as_deref().is_some_and(|transport| {
        transport
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part == "sse")
    })
}

fn mcp_spec_has_supported_runtime(spec: &McpServerSpec) -> bool {
    if spec.command.is_some() && spec.url.is_some() {
        return false;
    }
    if spec.command.is_some() {
        return true;
    }
    if spec.url.is_none() {
        return false;
    }
    let transport = spec.transport.as_deref();
    mcp_transport_is_sse(transport)
        || mcp_transport_is_streamable_http(transport)
        || mcp_transport_is_http(transport)
}

fn mcp_transport_label(spec: &McpServerSpec) -> Option<&'static str> {
    if spec.command.is_some() {
        Some("stdio")
    } else if mcp_spec_uses_sse(spec) {
        Some("sse")
    } else if mcp_transport_is_streamable_http(spec.transport.as_deref()) {
        Some("streamable-http")
    } else if spec.url.is_some() {
        Some("http")
    } else {
        None
    }
}

pub fn register_allowed_external_agent_tools(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
) -> usize {
    register_allowed_external_agent_tools_impl(registry, packages, |_, _, _| true, false, None)
}

pub fn register_allowed_external_agent_tools_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    extra_provenance: Option<&str>,
) -> usize {
    register_allowed_external_agent_tools_impl(
        registry,
        packages,
        |_, _, _| true,
        false,
        extra_provenance,
    )
}

pub fn register_allowed_external_agent_tools_for_resource_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    resource: &str,
    extra_provenance: Option<&str>,
) -> usize {
    register_allowed_external_agent_tools_impl(
        registry,
        packages,
        |package, capability, spec| {
            external_agent_resource_matches(package, capability, spec, resource)
        },
        true,
        extra_provenance,
    )
}

pub fn register_allowed_external_agent_tools_for_category_with_provenance(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    category: &str,
    extra_provenance: Option<&str>,
) -> usize {
    register_allowed_external_agent_tools_impl(
        registry,
        packages,
        |package, _, _| external_agent_category_matches(package, category),
        true,
        extra_provenance,
    )
}

fn register_allowed_external_agent_tools_impl(
    registry: &mut ToolRegistry,
    packages: impl IntoIterator<Item = NormalizedPackage>,
    include: impl Fn(&NormalizedPackage, &NormalizedCapability, &ExternalAgentSpec) -> bool,
    skip_existing: bool,
    extra_provenance: Option<&str>,
) -> usize {
    let mut registered = 0;
    for package in packages {
        if package.quarantined || !external_agent_adapter_supported(&package) {
            continue;
        }
        for capability in package.capabilities.iter().filter(|capability| {
            capability.kind == CapabilityKind::ExternalAgent && !capability.quarantined
        }) {
            if !external_agent_capability_supported(&package, capability) {
                continue;
            }
            let Some(spec) = external_agent_spec_from_capability(&package, capability) else {
                continue;
            };
            if !include(&package, capability, &spec) {
                continue;
            }
            let mut descriptor = ExternalAgentTool::descriptor(&spec);
            descriptor.provenance = Some(external_agent_tool_provenance(
                &package,
                capability,
                extra_provenance,
            ));
            if skip_existing && registry.contains(&descriptor.id) {
                continue;
            }
            registry.register(descriptor, Arc::new(ExternalAgentTool::new(spec)));
            registered += 1;
        }
    }
    registered
}

fn external_agent_adapter_supported(package: &NormalizedPackage) -> bool {
    matches!(
        package.adapter,
        AdapterKind::A2a | AdapterKind::HermesPlugin | AdapterKind::HermesExternalAgent
    )
}

fn external_agent_capability_supported(
    package: &NormalizedPackage,
    capability: &NormalizedCapability,
) -> bool {
    if package.adapter == AdapterKind::A2a {
        return true;
    }
    capability
        .runtime
        .as_ref()
        .is_some_and(external_agent_runtime_supported)
}

fn external_agent_runtime_supported(runtime: &agent_adapters::NormalizedRuntime) -> bool {
    let transport = runtime.transport.to_ascii_lowercase();
    let has_http_endpoint = runtime.endpoint.as_deref().is_some_and(|endpoint| {
        endpoint.starts_with("http://") || endpoint.starts_with("https://")
    });
    if external_agent_transport_is_a2a(&transport) {
        return has_http_endpoint;
    }
    has_http_endpoint
        && transport
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| {
                matches!(
                    part,
                    "http" | "https" | "json" | "rest" | "webhook" | "unknown"
                )
            })
}

fn external_agent_spec_from_capability(
    package: &NormalizedPackage,
    capability: &NormalizedCapability,
) -> Option<ExternalAgentSpec> {
    let runtime = capability.runtime.as_ref()?;
    let endpoint = runtime.endpoint.as_deref()?.trim();
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return None;
    }
    let a2a =
        package.adapter == AdapterKind::A2a || external_agent_transport_is_a2a(&runtime.transport);
    Some(ExternalAgentSpec {
        id: ToolId::from(format!(
            "{}-{}",
            if a2a { "a2a" } else { "external-agent" },
            slugify(&capability.id)
        )),
        name: format!("External agent: {}", capability.name),
        description: capability.description.clone(),
        transport: if a2a {
            "a2a".into()
        } else {
            runtime.transport.clone()
        },
        endpoint: endpoint.to_string(),
        input_modes: runtime.input_modes.clone(),
        output_modes: runtime.output_modes.clone(),
        auth_schemes: runtime.auth_schemes.clone(),
        header_keys: runtime.header_keys.clone(),
    })
}

fn external_agent_tool_provenance(
    package: &NormalizedPackage,
    capability: &NormalizedCapability,
    extra_provenance: Option<&str>,
) -> String {
    let mut parts = vec![
        format!("adapter_package={}", package.id),
        format!("capability={}", capability.id),
    ];
    if let Some(provenance) = package.provenance.as_deref().map(str::trim)
        && !provenance.is_empty()
    {
        parts.push(provenance.to_string());
    }
    if let Some(provenance) = extra_provenance.map(str::trim)
        && !provenance.is_empty()
    {
        parts.push(provenance.to_string());
    }
    parts.join("; ")
}

fn external_agent_resource_matches(
    package: &NormalizedPackage,
    capability: &NormalizedCapability,
    spec: &ExternalAgentSpec,
    resource: &str,
) -> bool {
    resource == "*"
        || package.id == resource
        || capability.id == resource
        || capability.name == resource
        || spec.id.0 == resource
        || spec.name == resource
}

fn external_agent_category_matches(package: &NormalizedPackage, category: &str) -> bool {
    category == "*"
        || category == "agent"
        || category == "subagent"
        || category == "a2a"
        || category == "http-json"
        || category == "external-agent"
        || package.id == category
}

fn mcp_specs_from_package(package: &NormalizedPackage) -> Result<Vec<McpServerSpec>, ToolError> {
    let manifest = read_mcp_manifest(&package.source)?;
    let servers = manifest
        .get("mcpServers")
        .or_else(|| manifest.get("servers"))
        .and_then(Value::as_object)
        .ok_or_else(|| ToolError::InvalidInput("MCP manifest has no servers".into()))?;
    let mut specs = Vec::new();
    for (name, server) in servers {
        let server = server.as_object().ok_or_else(|| {
            ToolError::InvalidInput(format!("MCP server `{name}` must be an object"))
        })?;
        let id = ToolId::from(format!("mcp-{}", slugify(name)));
        let command = server
            .get("command")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let url = mcp_server_endpoint(server);
        let transport = mcp_server_transport(server, command.as_deref(), url.as_deref());
        let args = server
            .get("args")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let env = server
            .get("env")
            .and_then(Value::as_object)
            .map(|env| {
                env.iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_string()))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let headers = server
            .get("headers")
            .and_then(Value::as_object)
            .map(|headers| {
                headers
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_string()))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let cwd = server.get("cwd").and_then(Value::as_str).map(PathBuf::from);
        let permissions = ToolPermissions {
            shell: command.is_some(),
            network: url.is_some(),
            secrets: !env.is_empty() || !headers.is_empty(),
            file_read: server_text_has_file_marker(server, "read"),
            file_write: server_text_has_file_marker(server, "write")
                || server_text_has_file_marker(server, "edit"),
            wallet: server_text_has_marker(server, "wallet"),
            payment: server_text_has_marker(server, "payment"),
            browser_profile: server_text_has_marker(server, "browser profile")
                || server_text_has_marker(server, "browser_profile"),
            ..ToolPermissions::default()
        };
        let description = if let Some(command) = &command {
            format!("Allowed stdio MCP server `{name}` via command `{command}`.")
        } else if mcp_transport_is_sse(transport.as_deref()) {
            format!(
                "Allowed SSE MCP server `{name}` at {}.",
                url.as_deref().unwrap_or("")
            )
        } else if let Some(url) = &url {
            format!("Allowed HTTP MCP server `{name}` at {url}.")
        } else {
            format!("Allowed MCP server `{name}`.")
        };
        specs.push(McpServerSpec {
            id,
            name: format!("MCP: {name}"),
            description,
            transport,
            command,
            args,
            env,
            headers,
            cwd,
            url,
            permissions,
        });
    }
    Ok(specs)
}

fn mcp_server_endpoint(server: &serde_json::Map<String, Value>) -> Option<String> {
    server
        .get("url")
        .or_else(|| server.get("endpoint"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn mcp_server_transport(
    server: &serde_json::Map<String, Value>,
    command: Option<&str>,
    url: Option<&str>,
) -> Option<String> {
    if command.is_some() {
        return Some("stdio".into());
    }
    let declared = server
        .get("transport")
        .or_else(|| server.get("type"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if mcp_transport_is_sse(declared) {
        return Some("sse".into());
    }
    if mcp_transport_is_streamable_http(declared) {
        return Some("streamable_http".into());
    }
    if url.is_some() {
        return Some(
            declared
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "http".into()),
        );
    }
    declared.map(ToOwned::to_owned)
}

fn mcp_transport_is_sse(transport: Option<&str>) -> bool {
    transport.is_some_and(|transport| {
        transport
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part == "sse")
    })
}

fn mcp_transport_is_streamable_http(transport: Option<&str>) -> bool {
    transport.is_some_and(|transport| {
        let lower = transport.to_ascii_lowercase();
        lower.contains("streamable") || lower.contains("streaming_http")
    })
}

fn mcp_transport_is_http(transport: Option<&str>) -> bool {
    transport.is_none_or(|transport| {
        transport
            .to_ascii_lowercase()
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| matches!(part, "http" | "https"))
    })
}

fn read_mcp_manifest(source: &Path) -> Result<Value, ToolError> {
    let path = if source.is_dir() {
        source.join("mcp.json")
    } else {
        source.to_path_buf()
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ToolError::Execution(format!("failed to read MCP manifest: {e}")))?;
    serde_json::from_str(&text)
        .map_err(|e| ToolError::InvalidInput(format!("invalid MCP manifest JSON: {e}")))
}

fn server_text_has_file_marker(server: &serde_json::Map<String, Value>, marker: &str) -> bool {
    let text = Value::Object(server.clone())
        .to_string()
        .to_ascii_lowercase();
    (text.contains("filesystem") || text.contains("file-system")) && text.contains(marker)
}

fn server_text_has_marker(server: &serde_json::Map<String, Value>, marker: &str) -> bool {
    Value::Object(server.clone())
        .to_string()
        .to_ascii_lowercase()
        .contains(marker)
}

fn slugify(s: &str) -> String {
    let slug: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug.split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Descriptor/placeholder for the harness-owned subagent runtime. The actual
/// child run is executed by `agent-core` so it can emit linked child events.
pub struct SubagentTool;

impl SubagentTool {
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("subagent"),
            name: "Subagent".into(),
            description: "Runs a focused child agent call and returns its output.".into(),
            categories: vec!["agent".into(), "subagent".into()],
            input_schema: json!({
                "type": "object",
                "required": ["prompt"],
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Task prompt for the child run."
                    },
                    "agent_id": {
                        "type": "string",
                        "description": "Optional child agent label for trace provenance."
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Summarize the child run output while preserving child_run_id, agent_id, and final_output provenance.".into(),
            ),
            permissions: ToolPermissions::default(),
            requires_approval: false,
            provenance: None,
        }
    }

    pub fn descriptor_with_agent_options(agent_ids: Vec<String>) -> ToolDescriptor {
        let mut descriptor = Self::descriptor();
        let mut agent_ids = agent_ids
            .into_iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect::<Vec<_>>();
        agent_ids.sort();
        agent_ids.dedup();
        if agent_ids.is_empty() {
            return descriptor;
        }
        if let Some(agent_id_schema) = descriptor
            .input_schema
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|properties| properties.get_mut("agent_id"))
        {
            agent_id_schema["enum"] = json!(agent_ids);
            agent_id_schema["description"] =
                json!("Optional saved child agent id. Omit to use the parent-derived subagent.");
        }
        descriptor.provenance = Some("native:agent-core; saved_agent_selection=true".into());
        descriptor
    }
}

#[async_trait]
impl Tool for SubagentTool {
    async fn execute(&self, _input: Value) -> Result<Value, ToolError> {
        Err(ToolError::Execution(
            "subagent execution is owned by agent-core".into(),
        ))
    }
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
            categories: vec!["demo".into()],
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
            output_interpretation_guidance: Some(
                "Echo output is already final; keep the returned text unchanged unless the user asked for a transformation.".into(),
            ),
            permissions: ToolPermissions::default(),
            requires_approval: false,
            provenance: None,
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
    use agent_secrets::{FileSecretStore, SecretId, SecretStore, SecretValue};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn descriptor(id: &str) -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from(id),
            name: id.into(),
            description: "test".into(),
            categories: vec!["test".into()],
            input_schema: json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                }
            }),
            output_interpretation_guidance: None,
            permissions: ToolPermissions::default(),
            requires_approval: false,
            provenance: None,
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "agent-tools-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn subagent_descriptor_can_advertise_saved_agent_options() {
        let descriptor = SubagentTool::descriptor_with_agent_options(vec![
            "worker".into(),
            " critic ".into(),
            "worker".into(),
        ]);
        let agent_id_schema = descriptor
            .input_schema
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .and_then(|properties| properties.get("agent_id"))
            .expect("agent_id schema");

        assert_eq!(
            agent_id_schema
                .get("enum")
                .and_then(serde_json::Value::as_array),
            Some(&vec![json!("critic"), json!("worker")])
        );
        assert_eq!(
            descriptor.provenance.as_deref(),
            Some("native:agent-core; saved_agent_selection=true")
        );
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

    #[tokio::test]
    async fn artifact_tool_writes_scoped_text_artifact() {
        let dir = temp_dir("artifact-text");
        let tool = ArtifactTool::new(&dir);

        let output = tool
            .execute(json!({
                "format": "txt",
                "title": "Run Report",
                "filename": "../../final.txt",
                "content": "hello artifact"
            }))
            .await
            .unwrap();

        let path = PathBuf::from(output["path"].as_str().unwrap());
        assert_eq!(output["format"], "txt");
        assert_eq!(output["scoped"], true);
        assert!(path.starts_with(&dir));
        assert_eq!(path.parent(), Some(dir.as_path()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello artifact");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn artifact_tool_generates_common_document_formats() {
        let dir = temp_dir("artifact-formats");
        let tool = ArtifactTool::new(&dir);
        let rows = json!([["name", "count"], ["Ada", 3], ["Comma, Cell", true]]);

        for (format, magic) in [
            ("svg", b"<svg".as_slice()),
            ("pdf", b"%PDF-1.4".as_slice()),
            ("docx", b"PK\x03\x04".as_slice()),
            ("xlsx", b"PK\x03\x04".as_slice()),
            ("pptx", b"PK\x03\x04".as_slice()),
        ] {
            let output = tool
                .execute(json!({
                    "format": format,
                    "title": "Smoke",
                    "content": "Hello",
                    "rows": rows.clone()
                }))
                .await
                .unwrap();
            let path = PathBuf::from(output["path"].as_str().unwrap());
            let bytes = std::fs::read(path).unwrap();
            assert!(
                bytes.starts_with(magic),
                "{format} did not use expected file signature"
            );
        }

        let output = tool
            .execute(json!({
                "format": "csv",
                "filename": "table",
                "rows": rows.clone()
            }))
            .await
            .unwrap();
        let csv = std::fs::read_to_string(output["path"].as_str().unwrap()).unwrap();
        assert_eq!(csv, "name,count\nAda,3\n\"Comma, Cell\",true\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn artifact_listing_resolves_only_scoped_artifacts() {
        let dir = temp_dir("artifact-list");
        let tool = ArtifactTool::new(&dir);
        let output = tool
            .execute(json!({
                "format": "md",
                "filename": "notes",
                "content": "# Notes"
            }))
            .await
            .unwrap();
        let artifact_id = output["artifact_id"].as_str().unwrap();
        let filename = PathBuf::from(output["path"].as_str().unwrap())
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let artifacts = list_generated_artifacts(&dir).unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].id, artifact_id);
        assert_eq!(artifacts[0].format, "md");
        assert_eq!(artifacts[0].bytes, 7);
        assert!(artifacts[0].modified_ms.is_some());
        assert_eq!(
            show_generated_artifact(&dir, artifact_id).unwrap().path,
            artifacts[0].path
        );
        assert_eq!(
            show_generated_artifact(&dir, &filename).unwrap().id,
            artifact_id
        );
        let deleted = delete_generated_artifact(&dir, artifact_id).unwrap();
        assert_eq!(deleted.id, artifact_id);
        assert!(!deleted.path.exists());
        assert!(list_generated_artifacts(&dir).unwrap().is_empty());

        let err =
            show_generated_artifact(&dir, "../notes").expect_err("path escapes must be rejected");
        assert!(err.to_string().contains("invalid artifact id"));
        let err = delete_generated_artifact(&dir, "../notes")
            .expect_err("delete path escapes must be rejected");
        assert!(err.to_string().contains("invalid artifact id"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn generated_artifact_ids_can_be_collected_and_deleted_in_bulk() {
        let dir = temp_dir("artifact-bulk-delete");
        let tool = ArtifactTool::new(&dir);
        let first = tool
            .execute(json!({
                "format": "txt",
                "filename": "first",
                "content": "one"
            }))
            .await
            .unwrap();
        let second = tool
            .execute(json!({
                "format": "md",
                "filename": "second",
                "content": "two"
            }))
            .await
            .unwrap();
        let first_id = first["artifact_id"].as_str().unwrap().to_string();
        let second_id = second["artifact_id"].as_str().unwrap().to_string();
        let ids = generated_artifact_ids_in_value(&json!({
            "artifact_id": first_id,
            "nested": [{ "artifact_ids": [second_id, "../escape", ""] }]
        }));

        let deleted = delete_generated_artifacts_by_ids(&dir, &ids).unwrap();

        assert_eq!(deleted.len(), 2);
        assert!(list_generated_artifacts(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn artifact_export_copies_scoped_artifact_without_overwrite() {
        let dir = temp_dir("artifact-export");
        let export_dir = temp_dir("artifact-export-out");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&export_dir).unwrap();
        std::fs::write(dir.join("report.txt"), b"export me").unwrap();

        let exported = export_generated_artifact(&dir, "report", &export_dir).unwrap();
        assert_eq!(exported.artifact.id, "report");
        assert_eq!(exported.bytes, 9);
        assert_eq!(exported.output_path, export_dir.join("report.txt"));
        assert_eq!(std::fs::read(&exported.output_path).unwrap(), b"export me");

        let err = export_generated_artifact(&dir, "report", &export_dir)
            .expect_err("export must not overwrite existing files");
        assert!(err.to_string().contains("already exists"));
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(export_dir);
    }

    #[test]
    fn generated_artifact_data_url_reads_scoped_audio() {
        let dir = temp_dir("artifact-data-url");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("voice-output.mp3"), b"AUDIO").unwrap();

        let preview = generated_artifact_data_url(&dir, "voice-output").unwrap();

        assert_eq!(preview.artifact.id, "voice-output");
        assert_eq!(preview.media_type, "audio/mpeg");
        assert_eq!(preview.data_url, "data:audio/mpeg;base64,QVVESU8=");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn generated_artifact_data_url_reads_scoped_svg_image() {
        let dir = temp_dir("artifact-svg-data-url");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("diagram.svg"), b"<svg></svg>").unwrap();

        let preview = generated_artifact_data_url(&dir, "diagram").unwrap();

        assert_eq!(preview.artifact.id, "diagram");
        assert_eq!(preview.media_type, "image/svg+xml");
        assert_eq!(
            preview.data_url,
            "data:image/svg+xml;base64,PHN2Zz48L3N2Zz4="
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn generated_artifact_data_url_reads_scoped_raster_image() {
        let dir = temp_dir("artifact-raster-data-url");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("screenshot.png"), b"\x89PNG").unwrap();

        let preview = generated_artifact_data_url(&dir, "screenshot").unwrap();

        assert_eq!(preview.artifact.id, "screenshot");
        assert_eq!(preview.artifact.format, "png");
        assert_eq!(preview.media_type, "image/png");
        assert_eq!(preview.data_url, "data:image/png;base64,iVBORw==");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn generated_artifact_file_reads_scoped_bytes_and_media_type() {
        let dir = temp_dir("artifact-file");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("report.pdf"), b"%PDF\xff").unwrap();

        let file = generated_artifact_file(&dir, "report").unwrap();

        assert_eq!(file.artifact.id, "report");
        assert_eq!(file.media_type, "application/pdf");
        assert_eq!(file.bytes, b"%PDF\xff");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn voice_capture_data_url_is_saved_as_scoped_audio_artifact() {
        let dir = temp_dir("voice-capture");

        let artifact = save_voice_capture(
            &dir,
            "data:audio/webm;codecs=opus;base64,aGVsbG8=",
            Some("../meeting.webm"),
        )
        .unwrap();

        assert_eq!(artifact.format, "webm");
        assert_eq!(artifact.bytes, 5);
        assert_eq!(std::fs::read(&artifact.path).unwrap(), b"hello");
        assert!(artifact.path.starts_with(&dir));
        assert!(artifact.id.starts_with("meeting-"));
        assert_eq!(list_generated_artifacts(&dir).unwrap().len(), 1);

        let err = save_voice_capture(&dir, "data:text/plain;base64,aGVsbG8=", None)
            .expect_err("non-audio captures must be rejected");
        assert!(
            err.to_string()
                .contains("unsupported voice capture media type")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn artifact_tool_rejects_unknown_formats() {
        let dir = temp_dir("artifact-invalid");
        let tool = ArtifactTool::new(&dir);

        let err = tool.execute(json!({"format": "exe"})).await.unwrap_err();

        assert!(err.to_string().contains("unsupported artifact format"));
        assert!(!dir.exists());
    }

    #[test]
    fn voice_tools_register_only_enabled_directions() {
        let mut registry = ToolRegistry::new();
        let count = register_voice_tools(
            &mut registry,
            VoiceRuntimeConfig {
                output_enabled: true,
                output_backend: Some("local".into()),
                tts_provider: Some("command".into()),
                ..VoiceRuntimeConfig::default()
            },
        );

        assert_eq!(count, 1);
        assert!(registry.descriptor(&ToolId::from("voice_speak")).is_some());
        assert!(
            registry
                .descriptor(&ToolId::from("voice_transcribe"))
                .is_none()
        );
        let descriptor = registry.descriptor(&ToolId::from("voice_speak")).unwrap();
        assert!(descriptor.requires_approval);
        assert!(descriptor.permissions.shell);
        assert!(descriptor.permissions.file_write);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_voice_speech_command_writes_scoped_artifact() {
        let dir = temp_dir("voice-speech-local");
        let tool = VoiceSpeakTool::new(
            VoiceRuntimeConfig {
                output_enabled: true,
                output_backend: Some("local".into()),
                tts_provider: Some("command".into()),
                voice: Some("test-voice".into()),
                tone: Some("calm".into()),
                ..VoiceRuntimeConfig::default()
            },
            &dir,
        );

        let output = tool
            .execute(json!({
                "text": "hello voice",
                "filename": "../voice",
                "format": "wav",
                "command": "printf '%s' \"$AGENT_VOICE_TEXT\" > \"$AGENT_VOICE_OUTPUT_PATH\""
            }))
            .await
            .unwrap();

        let path = PathBuf::from(output["path"].as_str().unwrap());
        assert!(path.starts_with(&dir));
        assert_eq!(output["backend"], "local");
        assert_eq!(output["provider"], "command");
        assert_eq!(output["voice"], "test-voice");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello voice");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn local_voice_transcription_command_returns_transcript() {
        let tool = VoiceTranscribeTool::new(VoiceRuntimeConfig {
            input_enabled: true,
            input_backend: Some("local".into()),
            input_provider: Some("command".into()),
            input_model: Some("tiny".into()),
            ..VoiceRuntimeConfig::default()
        });

        let output = tool
            .execute(json!({
                "audio_path": "/tmp/audio.wav",
                "command": "printf 'heard voice'"
            }))
            .await
            .unwrap();

        assert_eq!(output["transcript"], "heard voice");
        assert_eq!(output["backend"], "local");
        assert_eq!(output["model"], "tiny");
    }

    #[tokio::test]
    async fn cloud_voice_speech_posts_to_api_and_writes_artifact() {
        use std::io::Write;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let body = read_http_body(&mut stream);
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["model"], "tts-test");
            assert_eq!(body["input"], "hello cloud");
            assert_eq!(body["voice"], "nova");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: audio/mpeg\r\ncontent-length: 5\r\nconnection: close\r\n\r\nAUDIO"
            )
            .unwrap();
        });
        let dir = temp_dir("voice-speech-cloud");
        let tool = VoiceSpeakTool::new(
            VoiceRuntimeConfig {
                output_enabled: true,
                output_backend: Some("cloud".into()),
                tts_provider: Some("openai".into()),
                tts_model: Some("tts-test".into()),
                voice: Some("nova".into()),
                ..VoiceRuntimeConfig::default()
            },
            &dir,
        );
        unsafe {
            std::env::set_var("AGENT_TEST_OPENAI_KEY", "test-key");
        }

        let output = tool
            .execute(json!({
                "text": "hello cloud",
                "api_base_url": format!("http://{addr}/v1"),
                "api_key_env": "AGENT_TEST_OPENAI_KEY",
                "filename": "cloud",
                "format": "mp3"
            }))
            .await
            .unwrap();

        unsafe {
            std::env::remove_var("AGENT_TEST_OPENAI_KEY");
        }
        server.join().unwrap();
        let path = PathBuf::from(output["path"].as_str().unwrap());
        assert_eq!(output["backend"], "cloud");
        assert_eq!(output["provider"], "openai");
        assert_eq!(std::fs::read(&path).unwrap(), b"AUDIO");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn allowed_mcp_packages_register_native_approval_gated_tools() {
        let dir = temp_dir("mcp-register");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("mcp.json");
        std::fs::write(
            &source,
            r#"{
              "mcpServers": {
                "filesystem": {
                  "command": "fake-mcp-filesystem",
                  "args": ["--root", "/tmp"],
                  "env": { "API_KEY": "from-env" },
                  "capabilities": ["filesystem", "read", "write"]
                },
                "search": { "url": "https://example.invalid/mcp" }
              }
            }"#,
        )
        .unwrap();
        let package = agent_adapters::inspect_source(&source).unwrap();
        let mut quarantined_registry = ToolRegistry::new();

        assert_eq!(
            register_allowed_mcp_tools(&mut quarantined_registry, [package.clone()]),
            0
        );

        let mut allowed_package = package;
        allowed_package.quarantined = false;
        let mut registry = ToolRegistry::new();
        assert_eq!(
            register_allowed_mcp_tools(&mut registry, [allowed_package]),
            2
        );
        let filesystem = registry
            .descriptor(&ToolId::from("mcp-filesystem"))
            .unwrap();
        let search = registry.descriptor(&ToolId::from("mcp-search")).unwrap();

        assert!(filesystem.requires_approval);
        assert!(filesystem.permissions.shell);
        assert!(filesystem.permissions.secrets);
        assert!(filesystem.permissions.file_read);
        assert!(filesystem.permissions.file_write);
        assert!(search.requires_approval);
        assert!(search.permissions.network);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn promoted_agent_created_mcp_package_registers_runtime_tools() {
        let dir = temp_dir("agent-created-mcp-register");
        let registry = AdapterRegistry::new(StoragePaths::new(dir.join("home")));
        let package = registry
            .promote_agent_created_tool(
                "draft-weather-tool",
                "Weather Tool",
                r#"{
                  "mcpServers": {
                    "weather-cli": {
                      "command": "fake-weather-mcp",
                      "args": ["--stdio", "--city", "Santiago"],
                      "env": { "WEATHER_API_KEY": "secret://mcp.weather_api" }
                    },
                    "weather-http": {
                      "url": "https://example.invalid/mcp",
                      "headers": { "Authorization": "secret://mcp.weather_auth" }
                    }
                  }
                }"#,
                "agent",
                "test:capability",
            )
            .unwrap();

        assert!(!package.quarantined);
        let specs = mcp_specs_from_package(&package).unwrap();
        let cli = specs
            .iter()
            .find(|spec| spec.id == ToolId::from("mcp-weather-cli"))
            .unwrap();
        assert_eq!(cli.command.as_deref(), Some("fake-weather-mcp"));
        assert_eq!(
            cli.args,
            vec![
                "--stdio".to_string(),
                "--city".to_string(),
                "Santiago".to_string()
            ]
        );
        assert_eq!(
            cli.env.get("WEATHER_API_KEY").map(String::as_str),
            Some("secret://mcp.weather_api")
        );
        let http = specs
            .iter()
            .find(|spec| spec.id == ToolId::from("mcp-weather-http"))
            .unwrap();
        assert_eq!(http.url.as_deref(), Some("https://example.invalid/mcp"));
        assert_eq!(
            http.headers.get("Authorization").map(String::as_str),
            Some("secret://mcp.weather_auth")
        );

        let mut tool_registry = ToolRegistry::new();
        assert_eq!(
            register_allowed_mcp_tools_with_provenance(
                &mut tool_registry,
                [package],
                Some("conversation=weather-demo"),
            ),
            2
        );
        let cli_descriptor = tool_registry
            .descriptor(&ToolId::from("mcp-weather-cli"))
            .unwrap();
        assert!(cli_descriptor.requires_approval);
        assert!(cli_descriptor.permissions.shell);
        assert!(cli_descriptor.permissions.secrets);
        assert!(
            cli_descriptor
                .provenance
                .as_deref()
                .is_some_and(|provenance| {
                    provenance.contains("adapter_package=agent-tool-draft-weather-tool")
                        && provenance.contains("draft_id=draft-weather-tool")
                        && provenance.contains("conversation=weather-demo")
                })
        );
        let http_descriptor = tool_registry
            .descriptor(&ToolId::from("mcp-weather-http"))
            .unwrap();
        assert!(http_descriptor.permissions.network);
        assert!(http_descriptor.permissions.secrets);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcp_package_registration_can_be_limited_to_one_granted_resource() {
        let dir = temp_dir("mcp-grant-resource");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("mcp.json");
        std::fs::write(
            &source,
            r#"{
              "mcpServers": {
                "filesystem": { "command": "fake-mcp-filesystem" },
                "search": { "url": "https://example.invalid/mcp" }
              }
            }"#,
        )
        .unwrap();
        let mut package = agent_adapters::inspect_source(&source).unwrap();
        package.quarantined = false;
        for capability in &mut package.capabilities {
            capability.quarantined = false;
        }
        let mut registry = ToolRegistry::new();

        assert_eq!(
            register_allowed_mcp_tools_for_resource(&mut registry, [package], "mcp-search"),
            1
        );

        let search = registry.descriptor(&ToolId::from("mcp-search")).unwrap();
        assert!(
            search
                .provenance
                .as_deref()
                .is_some_and(|provenance| provenance.starts_with("adapter_package=adapter-"))
        );
        assert!(
            registry
                .descriptor(&ToolId::from("mcp-filesystem"))
                .is_none()
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn allowed_mcp_sse_packages_register_native_approval_gated_tools() {
        let dir = temp_dir("mcp-sse-register");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("mcp.json");
        std::fs::write(
            &source,
            r#"{
              "mcpServers": {
                "events": {
                  "type": "sse",
                  "endpoint": "https://example.invalid/sse",
                  "headers": { "Authorization": "secret://mcp.events_auth" }
                }
              }
            }"#,
        )
        .unwrap();
        let mut package = agent_adapters::inspect_source(&source).unwrap();
        package.quarantined = false;
        for capability in &mut package.capabilities {
            capability.quarantined = false;
        }
        let mut registry = ToolRegistry::new();

        assert_eq!(
            register_allowed_mcp_tools(&mut registry, [package.clone()]),
            1
        );

        let descriptor = registry.descriptor(&ToolId::from("mcp-events")).unwrap();
        assert!(descriptor.requires_approval);
        assert!(descriptor.permissions.network);
        assert!(descriptor.permissions.secrets);
        assert!(
            descriptor
                .categories
                .iter()
                .any(|category| category == "mcp")
        );
        assert!(
            descriptor
                .categories
                .iter()
                .any(|category| category == "mcp-sse")
        );
        let spec = mcp_specs_from_package(&package)
            .unwrap()
            .into_iter()
            .find(|spec| spec.id == ToolId::from("mcp-events"))
            .unwrap();
        assert_eq!(spec.transport.as_deref(), Some("sse"));
        assert_eq!(spec.url.as_deref(), Some("https://example.invalid/sse"));
        assert_eq!(
            spec.headers.get("Authorization").map(String::as_str),
            Some("secret://mcp.events_auth")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcp_transport_category_grants_register_only_matching_tools() {
        let dir = temp_dir("mcp-transport-category");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("mcp.json");
        std::fs::write(
            &source,
            r#"{
              "mcpServers": {
                "filesystem": { "command": "fake-mcp-filesystem" },
                "search": { "url": "https://example.invalid/mcp" },
                "events": { "type": "sse", "url": "https://example.invalid/sse" }
              }
            }"#,
        )
        .unwrap();
        let mut package = agent_adapters::inspect_source(&source).unwrap();
        package.quarantined = false;
        for capability in &mut package.capabilities {
            capability.quarantined = false;
        }
        let mut registry = ToolRegistry::new();

        assert_eq!(
            register_allowed_mcp_tools_for_category_with_provenance(
                &mut registry,
                [package],
                "mcp-sse",
                Some("grant=transport")
            ),
            1
        );

        let descriptor = registry.descriptor(&ToolId::from("mcp-events")).unwrap();
        assert!(
            descriptor
                .categories
                .iter()
                .any(|category| category == "mcp-sse")
        );
        assert!(
            descriptor
                .provenance
                .as_deref()
                .is_some_and(|provenance| provenance.contains("grant=transport"))
        );
        assert!(
            registry
                .descriptor(&ToolId::from("mcp-filesystem"))
                .is_none()
        );
        assert!(registry.descriptor(&ToolId::from("mcp-search")).is_none());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mcp_registration_skips_servers_without_supported_runtime() {
        let dir = temp_dir("mcp-supported-runtime");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("mcp.json");
        std::fs::write(
            &source,
            r#"{
              "mcpServers": {
                "metadata": { "description": "Catalog-only entry" },
                "socket": {
                  "transport": "websocket",
                  "url": "https://example.invalid/ws"
                },
                "ambiguous": {
                  "command": "fake-mcp",
                  "url": "https://example.invalid/mcp"
                },
                "search": { "url": "https://example.invalid/mcp" }
              }
            }"#,
        )
        .unwrap();
        let mut package = agent_adapters::inspect_source(&source).unwrap();
        package.quarantined = false;
        for capability in &mut package.capabilities {
            capability.quarantined = false;
        }
        let mut registry = ToolRegistry::new();

        assert_eq!(register_allowed_mcp_tools(&mut registry, [package]), 1);
        assert!(registry.descriptor(&ToolId::from("mcp-search")).is_some());
        assert!(registry.descriptor(&ToolId::from("mcp-metadata")).is_none());
        assert!(registry.descriptor(&ToolId::from("mcp-socket")).is_none());
        assert!(
            registry
                .descriptor(&ToolId::from("mcp-ambiguous"))
                .is_none()
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mcp_stdio_tool_calls_fake_json_rpc_server() {
        let dir = temp_dir("mcp-stdio");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("fake-mcp.sh");
        std::fs::write(
            &script,
            r#"while IFS= read -r line; do
  case "$line" in
    *'"id":1'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fake","version":"0"}}}'
      ;;
    *'"id":2'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"ok"}],"structuredContent":{"ok":true}}}'
      exit 0
      ;;
  esac
done
"#,
        )
        .unwrap();
        let tool = McpServerTool::new(McpServerSpec {
            id: ToolId::from("mcp-fake"),
            name: "MCP: fake".into(),
            description: "Fake MCP server.".into(),
            transport: Some("stdio".into()),
            command: Some("/bin/sh".into()),
            args: vec![script.display().to_string()],
            env: HashMap::new(),
            headers: HashMap::new(),
            cwd: None,
            url: None,
            permissions: ToolPermissions {
                shell: true,
                ..ToolPermissions::default()
            },
        });

        let output = tool
            .execute(json!({
                "tool_name": "echo",
                "arguments": { "text": "hello" },
                "timeout_ms": 1000
            }))
            .await
            .unwrap();

        assert_eq!(output["content"][0]["text"], "ok");
        assert_eq!(output["structuredContent"]["ok"], true);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mcp_stdio_tool_resolves_secret_handles_into_env() {
        let dir = temp_dir("mcp-secret-env");
        std::fs::create_dir_all(&dir).unwrap();
        let secret_store = Arc::new(FileSecretStore::new(dir.join("secrets.json")));
        let handle = secret_store
            .set(
                SecretId::new("mcp.api_key").unwrap(),
                SecretValue::new("resolved-secret"),
                None,
            )
            .unwrap();
        let script = dir.join("fake-mcp-secret.sh");
        std::fs::write(
            &script,
            r#"while IFS= read -r line; do
  case "$line" in
    *'"id":1'*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"fake","version":"0"}}}'
      ;;
    *'"id":2'*)
      printf '{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"%s"}]}}\n' "$SECRET_VALUE"
      exit 0
      ;;
  esac
done
"#,
        )
        .unwrap();
        let tool = McpServerTool::new_with_secret_store(
            McpServerSpec {
                id: ToolId::from("mcp-fake-secret"),
                name: "MCP: fake secret".into(),
                description: "Fake MCP server with secret env.".into(),
                transport: Some("stdio".into()),
                command: Some("/bin/sh".into()),
                args: vec![script.display().to_string()],
                env: HashMap::from([("SECRET_VALUE".into(), handle.to_string())]),
                headers: HashMap::new(),
                cwd: None,
                url: None,
                permissions: ToolPermissions {
                    shell: true,
                    secrets: true,
                    ..ToolPermissions::default()
                },
            },
            secret_store,
        );

        let output = tool
            .execute(json!({
                "tool_name": "echo",
                "arguments": {},
                "timeout_ms": 1000
            }))
            .await
            .unwrap();

        assert_eq!(output["content"][0]["text"], "resolved-secret");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn mcp_http_tool_posts_json_rpc_call() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let initialize = read_http_body(&mut stream);
            let initialize_json: Value = serde_json::from_str(&initialize).unwrap();
            assert_eq!(initialize_json["method"], "initialize");
            write_http_json(&mut stream, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);

            let (mut stream, _) = listener.accept().unwrap();
            let call = read_http_body(&mut stream);
            let call_json: Value = serde_json::from_str(&call).unwrap();
            assert_eq!(call_json["method"], "tools/call");
            assert_eq!(call_json["params"]["name"], "search");
            assert_eq!(call_json["params"]["arguments"]["q"], "rust");
            write_http_json(
                &mut stream,
                r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"ok"}]}}"#,
            );
        });
        let tool = McpServerTool::new(McpServerSpec {
            id: ToolId::from("mcp-http"),
            name: "MCP: http".into(),
            description: "HTTP MCP server".into(),
            transport: Some("http".into()),
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            headers: HashMap::new(),
            cwd: None,
            url: Some(format!("http://{addr}/mcp")),
            permissions: ToolPermissions {
                network: true,
                ..ToolPermissions::default()
            },
        });

        let output = tool
            .execute(json!({
                "tool_name": "search",
                "arguments": { "q": "rust" },
                "timeout_ms": 5_000
            }))
            .await
            .unwrap();

        assert_eq!(output["content"][0]["text"], "ok");
        server.join().unwrap();
    }

    #[tokio::test]
    async fn mcp_http_tool_sends_manifest_headers_and_secret_handles() {
        let dir = temp_dir("mcp-http-headers");
        let secret_store = Arc::new(FileSecretStore::new(dir.join("secrets.json")));
        let handle = secret_store
            .set(
                SecretId::new("mcp.http_auth").unwrap(),
                SecretValue::new("Bearer resolved-token"),
                None,
            )
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for id in [1, 2] {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                let lower = request.to_ascii_lowercase();
                assert!(lower.contains("authorization: bearer resolved-token"));
                assert!(lower.contains("x-mcp-client: shinkai-test"));
                let body = request.split_once("\r\n\r\n").unwrap().1;
                let body: Value = serde_json::from_str(body).unwrap();
                if id == 1 {
                    assert_eq!(body["method"], "initialize");
                    write_http_json(&mut stream, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
                } else {
                    assert_eq!(body["method"], "tools/call");
                    write_http_json(
                        &mut stream,
                        r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"secret ok"}]}}"#,
                    );
                }
            }
        });
        let tool = McpServerTool::new_with_secret_store(
            McpServerSpec {
                id: ToolId::from("mcp-http-secret"),
                name: "MCP: http secret".into(),
                description: "HTTP MCP server with auth headers".into(),
                transport: Some("http".into()),
                command: None,
                args: Vec::new(),
                env: HashMap::new(),
                headers: HashMap::from([
                    ("Authorization".into(), handle.to_string()),
                    ("X-MCP-Client".into(), "shinkai-test".into()),
                ]),
                cwd: None,
                url: Some(format!("http://{addr}/mcp")),
                permissions: ToolPermissions {
                    network: true,
                    secrets: true,
                    ..ToolPermissions::default()
                },
            },
            secret_store,
        );

        let output = tool
            .execute(json!({
                "tool_name": "search",
                "arguments": {},
                "timeout_ms": 5_000
            }))
            .await
            .unwrap();

        assert_eq!(output["content"][0]["text"], "secret ok");
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn mcp_sse_tool_posts_messages_and_reads_streamed_responses() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            use std::io::Write;

            let (mut sse_stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut sse_stream);
            assert!(request.starts_with("GET /sse HTTP/1.1"));
            write!(
                sse_stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: keep-alive\r\n\r\n"
            )
            .unwrap();
            write_sse_event(
                &mut sse_stream,
                "endpoint",
                &format!("http://{addr}/messages"),
            );

            let (mut initialize_stream, _) = listener.accept().unwrap();
            let initialize = read_http_body(&mut initialize_stream);
            let initialize_json: Value = serde_json::from_str(&initialize).unwrap();
            assert_eq!(initialize_json["method"], "initialize");
            write_http_empty(&mut initialize_stream, 202);
            write_sse_event(
                &mut sse_stream,
                "message",
                r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{}}}"#,
            );

            let (mut initialized_stream, _) = listener.accept().unwrap();
            let initialized = read_http_body(&mut initialized_stream);
            let initialized_json: Value = serde_json::from_str(&initialized).unwrap();
            assert_eq!(initialized_json["method"], "notifications/initialized");
            write_http_empty(&mut initialized_stream, 202);

            let (mut call_stream, _) = listener.accept().unwrap();
            let call = read_http_body(&mut call_stream);
            let call_json: Value = serde_json::from_str(&call).unwrap();
            assert_eq!(call_json["method"], "tools/call");
            assert_eq!(call_json["params"]["name"], "events.search");
            assert_eq!(call_json["params"]["arguments"]["q"], "rust");
            write_http_empty(&mut call_stream, 202);
            write_sse_event(
                &mut sse_stream,
                "message",
                r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"sse ok"}]}}"#,
            );
        });
        let tool = McpServerTool::new(McpServerSpec {
            id: ToolId::from("mcp-sse"),
            name: "MCP: sse".into(),
            description: "SSE MCP server".into(),
            transport: Some("sse".into()),
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            headers: HashMap::new(),
            cwd: None,
            url: Some(format!("http://{addr}/sse")),
            permissions: ToolPermissions {
                network: true,
                ..ToolPermissions::default()
            },
        });

        let output = tool
            .execute(json!({
                "tool_name": "events.search",
                "arguments": { "q": "rust" },
                "timeout_ms": 5_000
            }))
            .await
            .unwrap();

        assert_eq!(output["content"][0]["text"], "sse ok");
        server.join().unwrap();
    }

    #[tokio::test]
    async fn a2a_external_agent_tool_posts_message_send() {
        let dir = temp_dir("a2a-external-secret-header");
        std::fs::create_dir_all(&dir).unwrap();
        let secret_store = Arc::new(FileSecretStore::new(dir.join("secrets.json")));
        secret_store
            .set(
                SecretId::new("Authorization").unwrap(),
                SecretValue::new("Bearer test-token"),
                Some("A2A authorization header".into()),
            )
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-token")
            );
            let body = request.split_once("\r\n\r\n").unwrap().1;
            let body: Value = serde_json::from_str(body).unwrap();
            assert_eq!(body["method"], "message/send");
            assert_eq!(body["params"]["message"]["role"], "user");
            assert_eq!(body["params"]["message"]["parts"][0]["kind"], "text");
            assert_eq!(body["params"]["message"]["parts"][0]["text"], "review this");
            assert_eq!(body["params"]["message"]["contextId"], "ctx-1");
            assert_eq!(body["params"]["metadata"]["priority"], "high");
            write_http_json(
                &mut stream,
                r#"{"jsonrpc":"2.0","id":1,"result":{"taskId":"task-1","status":{"state":"completed"},"artifacts":[{"parts":[{"kind":"text","text":"done"}]}]}}"#,
            );
        });
        let tool = ExternalAgentTool::new_with_secret_store(
            ExternalAgentSpec {
                id: ToolId::from("a2a-reviewer"),
                name: "External agent: Reviewer".into(),
                description: "Remote review agent".into(),
                transport: "a2a".into(),
                endpoint: format!("http://{addr}/a2a"),
                input_modes: vec!["text/plain".into()],
                output_modes: vec!["text/plain".into()],
                auth_schemes: vec!["bearerAuth".into()],
                header_keys: vec!["Authorization".into()],
            },
            secret_store,
        );

        let output = tool
            .execute(json!({
                "prompt": "review this",
                "context_id": "ctx-1",
                "metadata": { "priority": "high" },
                "timeout_ms": 5_000
            }))
            .await
            .unwrap();

        assert_eq!(output["taskId"], "task-1");
        assert_eq!(output["status"]["state"], "completed");
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn http_json_external_agent_tool_posts_prompt_body() {
        let dir = temp_dir("http-json-external-secret-header");
        std::fs::create_dir_all(&dir).unwrap();
        let secret_store = Arc::new(FileSecretStore::new(dir.join("secrets.json")));
        let header_handle = secret_store
            .set(
                SecretId::new("agent.token").unwrap(),
                SecretValue::new("test-token"),
                Some("HTTP external agent token".into()),
            )
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("x-agent-token: test-token")
            );
            let body = request.split_once("\r\n\r\n").unwrap().1;
            let body: Value = serde_json::from_str(body).unwrap();
            assert_eq!(body["prompt"], "review this");
            assert_eq!(body["metadata"]["priority"], "high");
            write_http_json(
                &mut stream,
                r#"{"id":"job-1","status":"completed","answer":"done"}"#,
            );
        });
        let tool = ExternalAgentTool::new_with_secret_store(
            ExternalAgentSpec {
                id: ToolId::from("external-agent-reviewer"),
                name: "External agent: Reviewer".into(),
                description: "Remote review agent".into(),
                transport: "http-json".into(),
                endpoint: format!("http://{addr}/invoke"),
                input_modes: vec!["application/json".into()],
                output_modes: vec!["application/json".into()],
                auth_schemes: vec!["apiKey".into()],
                header_keys: Vec::new(),
            },
            secret_store,
        );

        let output = tool
            .execute(json!({
                "prompt": "review this",
                "metadata": { "priority": "high" },
                "headers": { "X-Agent-Token": header_handle.to_string() },
                "timeout_ms": 5_000
            }))
            .await
            .unwrap();

        assert_eq!(output["id"], "job-1");
        assert_eq!(output["answer"], "done");
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn allowed_a2a_packages_register_external_agent_tools() {
        let dir = temp_dir("a2a-register");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("agent-card.json");
        std::fs::write(
            &source,
            r#"{
              "name": "Remote Reviewer",
              "description": "Reviews documents.",
              "url": "https://agents.example.test/a2a",
              "preferredTransport": "JSONRPC",
              "defaultInputModes": ["text/plain"],
              "defaultOutputModes": ["text/plain"],
              "securitySchemes": {
                "bearerAuth": { "type": "http", "scheme": "bearer" }
              },
              "security": [{ "bearerAuth": [] }],
              "skills": [
                { "id": "review", "name": "Review", "description": "Review text." }
              ]
            }"#,
        )
        .unwrap();
        let package = agent_adapters::inspect_source(&source).unwrap();
        let mut quarantined_registry = ToolRegistry::new();

        assert_eq!(
            register_allowed_external_agent_tools(&mut quarantined_registry, [package.clone()]),
            0
        );

        let mut allowed_package = package;
        allowed_package.quarantined = false;
        for capability in &mut allowed_package.capabilities {
            capability.quarantined = false;
        }
        let mut registry = ToolRegistry::new();
        assert_eq!(
            register_allowed_external_agent_tools(&mut registry, [allowed_package.clone()]),
            1
        );
        let review = registry.descriptor(&ToolId::from("a2a-review")).unwrap();

        assert!(review.requires_approval);
        assert!(review.permissions.network);
        assert!(review.permissions.secrets);
        assert_eq!(
            review.categories,
            vec![
                "agent".to_string(),
                "subagent".to_string(),
                "external-agent".to_string(),
                "a2a".to_string()
            ]
        );

        let mut granted = ToolRegistry::new();
        assert_eq!(
            register_allowed_external_agent_tools_for_resource_with_provenance(
                &mut granted,
                [allowed_package.clone()],
                "review",
                Some("profile=research"),
            ),
            1
        );
        assert!(granted.descriptor(&ToolId::from("a2a-review")).is_some());
        let mut granted_by_category = ToolRegistry::new();
        assert_eq!(
            register_allowed_external_agent_tools_for_category_with_provenance(
                &mut granted_by_category,
                [allowed_package],
                "subagent",
                Some("profile=research"),
            ),
            1
        );
        let review_by_category = granted_by_category
            .descriptor(&ToolId::from("a2a-review"))
            .unwrap();
        assert!(
            review_by_category
                .provenance
                .as_deref()
                .is_some_and(|provenance| provenance.contains("profile=research"))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn allowed_hermes_a2a_external_agents_register_external_agent_tools() {
        let dir = temp_dir("hermes-a2a-register");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("plugin.yaml");
        std::fs::write(
            &source,
            r#"
name: hermes-review-pack
external_agents:
  - name: remote_reviewer
    endpoint: https://agents.example.test/a2a
    transport: a2a
    input_modes: [text/plain]
    output_modes: [text/plain]
    auth: [bearerAuth]
"#,
        )
        .unwrap();
        let mut package = agent_adapters::inspect_source(&source).unwrap();
        package.quarantined = false;
        for capability in &mut package.capabilities {
            capability.quarantined = false;
        }

        let mut registry = ToolRegistry::new();
        assert_eq!(
            register_allowed_external_agent_tools(&mut registry, [package]),
            1
        );
        let reviewer = registry
            .descriptor(&ToolId::from("a2a-remote-reviewer"))
            .unwrap();
        assert!(reviewer.requires_approval);
        assert!(reviewer.permissions.network);
        assert!(reviewer.permissions.secrets);
        assert!(
            reviewer
                .provenance
                .as_deref()
                .unwrap()
                .contains("adapter_package=")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn allowed_hermes_http_external_agents_register_external_agent_tools() {
        let dir = temp_dir("hermes-http-register");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("plugin.yaml");
        std::fs::write(
            &source,
            r#"
name: hermes-review-pack
external_agents:
  - name: remote_reviewer
    endpoint: https://agents.example.test/invoke
    transport: http-json
    input_modes: [application/json]
    output_modes: [application/json]
    auth: [apiKey]
    headers: [Authorization, X-Agent-Trace]
"#,
        )
        .unwrap();
        let mut package = agent_adapters::inspect_source(&source).unwrap();
        package.quarantined = false;
        for capability in &mut package.capabilities {
            capability.quarantined = false;
        }

        let mut registry = ToolRegistry::new();
        assert_eq!(
            register_allowed_external_agent_tools(&mut registry, [package]),
            1
        );
        let reviewer = registry
            .descriptor(&ToolId::from("external-agent-remote-reviewer"))
            .unwrap();
        assert!(reviewer.requires_approval);
        assert!(reviewer.permissions.network);
        assert!(reviewer.permissions.secrets);
        assert_eq!(
            reviewer.categories,
            vec![
                "agent".to_string(),
                "subagent".to_string(),
                "external-agent".to_string(),
                "http-json".to_string()
            ]
        );
        assert!(
            reviewer
                .description
                .contains("header keys: Authorization,X-Agent-Trace")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        use std::io::Read;

        let mut buffer = Vec::new();
        let mut temp = [0u8; 1024];
        loop {
            let read = stream.read(&mut temp).unwrap();
            assert!(read > 0, "HTTP client closed before headers");
            buffer.extend_from_slice(&temp[..read]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        let header_end = buffer
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap()
            + 4;
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        while buffer.len() < header_end + content_length {
            let read = stream.read(&mut temp).unwrap();
            assert!(read > 0, "HTTP client closed before body");
            buffer.extend_from_slice(&temp[..read]);
        }
        String::from_utf8(buffer[..header_end + content_length].to_vec()).unwrap()
    }

    fn read_http_body(stream: &mut std::net::TcpStream) -> String {
        let request = read_http_request(stream);
        request
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_string())
            .unwrap()
    }

    fn write_http_json(stream: &mut std::net::TcpStream, body: &str) {
        use std::io::Write;

        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    }

    fn write_http_empty(stream: &mut std::net::TcpStream, status: u16) {
        use std::io::Write;

        let reason = match status {
            200 => "OK",
            202 => "Accepted",
            _ => "Status",
        };
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        )
        .unwrap();
    }

    fn write_sse_event(stream: &mut std::net::TcpStream, event: &str, data: &str) {
        use std::io::Write;

        write!(stream, "event: {event}\ndata: {data}\n\n").unwrap();
        stream.flush().unwrap();
    }

    fn write_http_payment(
        stream: &mut std::net::TcpStream,
        status: u16,
        headers: &[(&str, &str)],
        body: &str,
    ) {
        use std::io::Write;

        let reason = match status {
            200 => "OK",
            402 => "Payment Required",
            _ => "Status",
        };
        write!(stream, "HTTP/1.1 {status} {reason}\r\n").unwrap();
        for (name, value) in headers {
            write!(stream, "{name}: {value}\r\n").unwrap();
        }
        write!(
            stream,
            "content-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    }

    fn encoded_payment_header(value: &Value) -> String {
        general_purpose::STANDARD.encode(serde_json::to_vec(value).unwrap())
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

    #[test]
    fn code_execution_tools_register_as_approval_gated_shell_runtime_tools() {
        let mut reg = ToolRegistry::new();
        assert_eq!(
            register_code_execution_tools(&mut reg, &ShellToolConfig::default()),
            2
        );

        for id in ["code_python", "code_typescript"] {
            let descriptor = reg.descriptor(&ToolId::from(id)).unwrap();
            assert!(descriptor.requires_approval);
            assert!(descriptor.permissions.shell);
            assert!(descriptor.permissions.file_read);
            assert!(descriptor.permissions.file_write);
            assert!(descriptor.permissions.network);
            assert!(is_shell_runtime_tool_id(id));
        }
    }

    #[test]
    fn x402_payment_descriptor_declares_wallet_payment_permissions() {
        let descriptor = PaymentX402Tool::descriptor();
        assert_eq!(descriptor.id.0, "payment_x402_request");
        assert!(descriptor.requires_approval);
        assert!(descriptor.permissions.network);
        assert!(descriptor.permissions.wallet);
        assert!(descriptor.permissions.payment);
        assert!(descriptor.permissions.secrets);
        assert!(
            descriptor.input_schema["properties"]
                .get("payment_signature_secret")
                .is_some()
        );

        let descriptor = PaymentX402RequiredTool::descriptor();
        assert_eq!(descriptor.id.0, "payment_x402_required");
        assert!(descriptor.requires_approval);
        assert!(descriptor.permissions.wallet);
        assert!(descriptor.permissions.payment);

        let descriptor = PaymentX402SettleTool::descriptor();
        assert_eq!(descriptor.id.0, "payment_x402_settle");
        assert!(descriptor.requires_approval);
        assert!(descriptor.permissions.network);
        assert!(descriptor.permissions.wallet);
        assert!(descriptor.permissions.payment);
        assert!(descriptor.permissions.secrets);
    }

    #[tokio::test]
    async fn x402_payment_required_tool_builds_header_response() {
        let accepts = json!([{
            "scheme": "exact",
            "network": "base-sepolia",
            "maxAmountRequired": "10",
            "payTo": "0x0000000000000000000000000000000000000001",
            "asset": "0x0000000000000000000000000000000000000002",
            "resource": "https://service.example/paid"
        }]);
        let output = PaymentX402RequiredTool
            .execute(json!({
                "x402_version": 1,
                "accepts": accepts,
                "error": "payment required",
                "body": "pay first"
            }))
            .await
            .unwrap();

        let header = output["headers"]["PAYMENT-REQUIRED"].as_str().unwrap();
        let decoded = decode_payment_header_value(header).unwrap();
        assert_eq!(output["status"], "payment_required");
        assert_eq!(output["status_code"], 402);
        assert_eq!(output["body"], "pay first");
        assert_eq!(decoded, output["payment_required"]);
        assert_eq!(decoded["accepts"], accepts);
    }

    #[tokio::test]
    async fn x402_payment_settle_tool_verifies_and_settles() {
        let payment_payload = json!({
            "x402Version": 1,
            "scheme": "exact",
            "network": "base-sepolia",
            "payload": { "authorization": "signed" }
        });
        let payment_requirements = json!({
            "scheme": "exact",
            "network": "base-sepolia",
            "maxAmountRequired": "5",
            "payTo": "0x0000000000000000000000000000000000000001",
            "asset": "0x0000000000000000000000000000000000000002",
            "resource": "https://service.example/paid"
        });
        let payment_required = json!({
            "x402Version": 1,
            "accepts": [payment_requirements.clone()]
        });
        let settlement = json!({
            "success": true,
            "transaction": "0xtx"
        });
        let payment_signature = encoded_payment_header(&payment_payload);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn({
            let payment_payload = payment_payload.clone();
            let payment_requirements = payment_requirements.clone();
            let settlement = settlement.clone();
            move || {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                assert!(request.starts_with("POST /verify HTTP/1.1"));
                let (_, body) = request.split_once("\r\n\r\n").unwrap();
                let body: Value = serde_json::from_str(body).unwrap();
                assert_eq!(body["paymentPayload"], payment_payload);
                assert_eq!(body["paymentRequirements"], payment_requirements);
                write_http_payment(&mut stream, 200, &[], r#"{"isValid":true}"#);

                let (mut stream, _) = listener.accept().unwrap();
                let request = read_http_request(&mut stream);
                assert!(request.starts_with("POST /settle HTTP/1.1"));
                let (_, body) = request.split_once("\r\n\r\n").unwrap();
                let body: Value = serde_json::from_str(body).unwrap();
                assert_eq!(body["paymentPayload"], payment_payload);
                assert_eq!(body["paymentRequirements"], payment_requirements);
                write_http_payment(&mut stream, 200, &[], &settlement.to_string());
            }
        });
        let tool = PaymentX402SettleTool::new(PaymentX402Config {
            default_timeout_ms: 5_000,
            max_response_bytes: 64 * 1024,
            max_amount: None,
            signature_env: "AGENT_TEST_X402_SIGNATURE".into(),
            signature_secret: None,
            facilitator_url: Some(format!("http://{addr}")),
            wallet_command: None,
        });

        let output = tool
            .execute(json!({
                "payment_signature": payment_signature,
                "payment_required": payment_required
            }))
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(output["status"], "settled");
        assert_eq!(output["verify"]["body"]["isValid"], true);
        assert_eq!(output["settle"]["body"], settlement);
        let response_header = output["headers"]["PAYMENT-RESPONSE"].as_str().unwrap();
        assert_eq!(
            decode_payment_header_value(response_header).unwrap(),
            settlement
        );
    }

    #[tokio::test]
    async fn x402_payment_settle_tool_does_not_settle_failed_verify() {
        let payment_payload = json!({
            "x402Version": 1,
            "scheme": "exact",
            "network": "base-sepolia",
            "payload": { "authorization": "signed" }
        });
        let payment_required = json!({
            "x402Version": 1,
            "accepts": [{
                "scheme": "exact",
                "network": "base-sepolia",
                "maxAmountRequired": "5",
                "resource": "https://service.example/paid"
            }]
        });
        let payment_signature = encoded_payment_header(&payment_payload);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(request.starts_with("POST /verify HTTP/1.1"));
            write_http_payment(
                &mut stream,
                200,
                &[],
                r#"{"isValid":false,"invalidReason":"underpaid"}"#,
            );
        });
        let tool = PaymentX402SettleTool::new(PaymentX402Config {
            default_timeout_ms: 5_000,
            max_response_bytes: 64 * 1024,
            max_amount: None,
            signature_env: "AGENT_TEST_X402_SIGNATURE".into(),
            signature_secret: None,
            facilitator_url: Some(format!("http://{addr}")),
            wallet_command: None,
        });

        let output = tool
            .execute(json!({
                "payment_signature": payment_signature,
                "payment_required": payment_required
            }))
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(output["status"], "verification_failed");
        assert_eq!(output["verify"]["body"]["isValid"], false);
    }

    #[tokio::test]
    async fn x402_payment_tool_parses_payment_required_header() {
        let required = json!({
            "x402Version": 1,
            "accepts": [{ "scheme": "exact", "maxAmountRequired": "5" }]
        });
        let header = encoded_payment_header(&required);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_body(&mut stream);
            write_http_payment(
                &mut stream,
                402,
                &[("PAYMENT-REQUIRED", &header)],
                "payment required",
            );
        });
        let tool = PaymentX402Tool::new(PaymentX402Config {
            default_timeout_ms: 5_000,
            max_response_bytes: 64 * 1024,
            max_amount: None,
            signature_env: "AGENT_TEST_X402_SIGNATURE".into(),
            signature_secret: None,
            facilitator_url: None,
            wallet_command: None,
        });

        let output = tool
            .execute(json!({
                "url": format!("http://{addr}/paid"),
                "method": "POST",
                "body": "probe"
            }))
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(output["status"], "payment_required");
        assert_eq!(output["status_code"], 402);
        assert_eq!(output["payment_required"], required);
        assert_eq!(output["retried"], false);
    }

    #[tokio::test]
    async fn x402_payment_tool_retries_with_signature_under_spend_limit() {
        let required = json!({
            "x402Version": 1,
            "accepts": [{ "scheme": "exact", "maxAmountRequired": "5" }]
        });
        let response = json!({ "transaction": "tx-test" });
        let required_header = encoded_payment_header(&required);
        let response_header = encoded_payment_header(&response);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_body(&mut stream);
            write_http_payment(
                &mut stream,
                402,
                &[("PAYMENT-REQUIRED", &required_header)],
                "payment required",
            );

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("payment-signature: signed-test")
            );
            write_http_payment(
                &mut stream,
                200,
                &[("PAYMENT-RESPONSE", &response_header)],
                "paid",
            );
        });
        let tool = PaymentX402Tool::new(PaymentX402Config {
            default_timeout_ms: 5_000,
            max_response_bytes: 64 * 1024,
            max_amount: None,
            signature_env: "AGENT_TEST_X402_SIGNATURE".into(),
            signature_secret: None,
            facilitator_url: None,
            wallet_command: None,
        });

        let output = tool
            .execute(json!({
                "url": format!("http://{addr}/paid"),
                "method": "POST",
                "body": "probe",
                "auto_pay": true,
                "payment_signature": "signed-test",
                "max_amount": 5
            }))
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(output["status"], "retried");
        assert_eq!(output["initial"]["spend"]["within_limit"], true);
        assert_eq!(output["retry"]["status"], "success");
        assert_eq!(output["retry"]["payment_response"], response);
        assert_eq!(output["wallet"]["signature_source"], "input");
    }

    #[tokio::test]
    async fn x402_payment_tool_retries_with_secret_signature() {
        let required = json!({
            "x402Version": 1,
            "accepts": [{ "scheme": "exact", "maxAmountRequired": "5" }]
        });
        let response = json!({ "transaction": "tx-secret" });
        let required_header = encoded_payment_header(&required);
        let response_header = encoded_payment_header(&response);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_body(&mut stream);
            write_http_payment(
                &mut stream,
                402,
                &[("PAYMENT-REQUIRED", &required_header)],
                "payment required",
            );

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("payment-signature: signed-from-secret")
            );
            write_http_payment(
                &mut stream,
                200,
                &[("PAYMENT-RESPONSE", &response_header)],
                "paid",
            );
        });
        let secret_store = Arc::new(FileSecretStore::new(
            temp_dir("x402-payment-secret").join("secrets.json"),
        ));
        secret_store
            .set(
                SecretId::new("payment.signature").unwrap(),
                SecretValue::new("signed-from-secret"),
                Some("test payment signature".into()),
            )
            .unwrap();
        let tool = PaymentX402Tool::new_with_secret_store(
            PaymentX402Config {
                default_timeout_ms: 5_000,
                max_response_bytes: 64 * 1024,
                max_amount: None,
                signature_env: "AGENT_TEST_X402_SIGNATURE".into(),
                signature_secret: None,
                facilitator_url: None,
                wallet_command: None,
            },
            secret_store,
        );

        let output = tool
            .execute(json!({
                "url": format!("http://{addr}/paid"),
                "method": "POST",
                "body": "probe",
                "auto_pay": true,
                "payment_signature_secret": "payment.signature",
                "max_amount": 5
            }))
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(output["status"], "retried");
        assert_eq!(output["wallet"]["signature_source"], "input_secret");
        assert_eq!(output["retry"]["payment_response"], response);
        assert!(!output.to_string().contains("signed-from-secret"));
    }

    #[tokio::test]
    async fn x402_payment_tool_retries_with_wallet_command() {
        let python = default_python_command();
        if !command_available(&python) {
            return;
        }
        let required = json!({
            "x402Version": 1,
            "accepts": [{ "scheme": "exact", "maxAmountRequired": "5" }]
        });
        let response = json!({ "transaction": "tx-wallet" });
        let required_header = encoded_payment_header(&required);
        let response_header = encoded_payment_header(&response);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_body(&mut stream);
            write_http_payment(
                &mut stream,
                402,
                &[("PAYMENT-REQUIRED", &required_header)],
                "payment required",
            );

            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("payment-signature: wallet-signed")
            );
            write_http_payment(
                &mut stream,
                200,
                &[("PAYMENT-RESPONSE", &response_header)],
                "paid",
            );
        });
        let wallet_dir = temp_dir("x402-wallet-command");
        std::fs::create_dir_all(&wallet_dir).unwrap();
        let wallet_path = wallet_dir.join("wallet.py");
        std::fs::write(
            &wallet_path,
            r#"
import json
import sys

payload = json.load(sys.stdin)
assert payload["payment_required"]["accepts"][0]["maxAmountRequired"] == "5"
assert payload["url"].endswith("/paid")
assert payload["method"] == "POST"
assert payload["max_amount"] == 5
print(json.dumps({"payment_signature": "wallet-signed"}))
"#,
        )
        .unwrap();
        let tool = PaymentX402Tool::new(PaymentX402Config {
            default_timeout_ms: 5_000,
            max_response_bytes: 64 * 1024,
            max_amount: None,
            signature_env: "AGENT_TEST_X402_SIGNATURE".into(),
            signature_secret: None,
            facilitator_url: None,
            wallet_command: Some(PaymentWalletCommandConfig {
                command: python,
                args: vec![wallet_path.to_string_lossy().to_string()],
                timeout_ms: 5_000,
            }),
        });

        let output = tool
            .execute(json!({
                "url": format!("http://{addr}/paid"),
                "method": "POST",
                "body": "probe",
                "auto_pay": true,
                "max_amount": 5
            }))
            .await
            .unwrap();

        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&wallet_dir);
        assert_eq!(output["status"], "retried");
        assert_eq!(output["wallet"]["signature_source"], "wallet_command");
        assert_eq!(output["retry"]["payment_response"], response);
        assert!(!output.to_string().contains("wallet-signed"));
    }

    #[tokio::test]
    async fn x402_payment_tool_blocks_retry_above_spend_limit() {
        let required = json!({
            "x402Version": 1,
            "accepts": [{ "scheme": "exact", "maxAmountRequired": "6" }]
        });
        let header = encoded_payment_header(&required);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_body(&mut stream);
            write_http_payment(
                &mut stream,
                402,
                &[("PAYMENT-REQUIRED", &header)],
                "payment required",
            );
        });
        let tool = PaymentX402Tool::new(PaymentX402Config {
            default_timeout_ms: 5_000,
            max_response_bytes: 64 * 1024,
            max_amount: None,
            signature_env: "AGENT_TEST_X402_SIGNATURE".into(),
            signature_secret: None,
            facilitator_url: None,
            wallet_command: None,
        });

        let output = tool
            .execute(json!({
                "url": format!("http://{addr}/paid"),
                "method": "POST",
                "body": "probe",
                "auto_pay": true,
                "payment_signature": "signed-test",
                "max_amount": 5
            }))
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(output["status"], "payment_required");
        assert_eq!(output["spend"]["within_limit"], false);
        assert_eq!(output["retried"], false);
    }

    #[tokio::test]
    async fn python_code_tool_runs_temp_file_with_minimal_env() {
        let python = default_python_command();
        if !command_available(&python) {
            return;
        }
        unsafe {
            std::env::set_var("AGENT_CODE_TEST_SECRET", "leak-me");
        }
        let tool = CodeExecutionTool::python(CodeExecutionConfig {
            default_timeout_ms: 30_000,
            max_output_bytes: 64 * 1024,
            default_cwd: None,
            python_command: python,
            typescript_command: "deno".into(),
            sandbox: None,
            sandbox_required: false,
        });

        let output = tool
            .execute(json!({
                "code": "import os, sys\nprint('secret=' + os.environ.get('AGENT_CODE_TEST_SECRET', ''))\nprint('arg=' + sys.argv[1])",
                "args": ["ok"]
            }))
            .await
            .unwrap();
        unsafe {
            std::env::remove_var("AGENT_CODE_TEST_SECRET");
        }

        assert_eq!(output["language"], "python");
        assert_eq!(output["status"], "success");
        assert_eq!(output["sandbox"], "temporary_cwd_minimal_env");
        let stdout = output["stdout"].as_str().unwrap();
        assert!(stdout.contains("secret=\n"));
        assert!(stdout.contains("arg=ok\n"));
    }

    #[tokio::test]
    async fn code_tool_rejects_execution_when_required_sandbox_is_missing() {
        let tool = CodeExecutionTool::python(CodeExecutionConfig {
            default_timeout_ms: 30_000,
            max_output_bytes: 64 * 1024,
            default_cwd: None,
            python_command: default_python_command(),
            typescript_command: "deno".into(),
            sandbox: None,
            sandbox_required: true,
        });

        let err = tool
            .execute(json!({ "code": "print('should not run')" }))
            .await
            .unwrap_err();

        assert!(err.to_string().contains("code sandbox is required"));
    }

    #[test]
    fn built_in_code_sandbox_configs_are_platform_scoped_and_contextual() {
        let linux =
            builtin_code_sandbox_for_platform("linux", |command| command == "bwrap").unwrap();
        assert_eq!(linux.label, "linux_bubblewrap_auto");
        assert_eq!(linux.command, "bwrap");
        let linux_args =
            linux.args_for_context(Path::new("/work/project"), Path::new("/tmp/agent-code"));
        assert!(linux_args.iter().any(|arg| arg == "/work/project"));
        assert!(linux_args.iter().any(|arg| arg == "/tmp/agent-code"));
        assert!(
            !linux_args
                .iter()
                .any(|arg| arg.to_string_lossy().contains("{cwd}"))
        );
        let duplicate_default_cwd_args =
            linux.args_for_context(Path::new("/tmp/agent-code"), Path::new("/tmp/agent-code"));
        assert_eq!(
            duplicate_default_cwd_args
                .iter()
                .filter(|arg| arg.as_os_str() == "--bind")
                .count(),
            1
        );

        let macos = builtin_code_sandbox_for_platform("macos", |command| command == "sandbox-exec")
            .unwrap();
        assert_eq!(macos.label, "macos_sandbox_exec_auto");
        assert_eq!(macos.command, "sandbox-exec");
        let macos_args =
            macos.args_for_context(Path::new("/work/project"), Path::new("/tmp/agent-code"));
        let profile = macos_args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .find(|arg| arg.contains("(version 1)"))
            .unwrap();
        assert!(profile.contains("/work/project"));
        assert!(profile.contains("/tmp/agent-code"));

        assert!(builtin_code_sandbox_for_platform("linux", |_| false).is_none());
        assert!(builtin_code_sandbox_for_platform("windows", |_| true).is_none());
    }

    #[tokio::test]
    async fn python_code_tool_can_run_through_configured_sandbox_wrapper() {
        let python = default_python_command();
        if !command_available(&python) {
            return;
        }
        let wrapper_dir =
            std::env::temp_dir().join(format!("agent-code-sandbox-test-{}", unique_temp_suffix()));
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let wrapper_path = wrapper_dir.join("wrapper.py");
        std::fs::write(
            &wrapper_path,
            r#"
import os
import subprocess
import sys

os.environ["AGENT_CODE_SANDBOX_MARKER"] = "wrapped"
raise SystemExit(subprocess.run(sys.argv[1:]).returncode)
"#,
        )
        .unwrap();
        let tool = CodeExecutionTool::python(CodeExecutionConfig {
            default_timeout_ms: 30_000,
            max_output_bytes: 64 * 1024,
            default_cwd: None,
            python_command: python.clone(),
            typescript_command: "deno".into(),
            sandbox: Some(CodeSandboxConfig {
                label: "test_sandbox_wrapper".into(),
                command: python.clone(),
                args: vec![wrapper_path.to_string_lossy().to_string()],
            }),
            sandbox_required: true,
        });

        let output = tool
            .execute(json!({
                "code": "import os, sys\nprint('marker=' + os.environ.get('AGENT_CODE_SANDBOX_MARKER', ''))\nprint('arg=' + sys.argv[1])",
                "args": ["ok"]
            }))
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(&wrapper_dir);

        assert_eq!(output["language"], "python");
        assert_eq!(output["status"], "success");
        assert_eq!(output["sandbox"], "test_sandbox_wrapper");
        assert_eq!(output["sandbox_command"], python);
        let stdout = output["stdout"].as_str().unwrap();
        assert!(stdout.contains("marker=wrapped\n"));
        assert!(stdout.contains("arg=ok\n"));
    }

    #[tokio::test]
    async fn typescript_code_tool_runs_with_deno_when_available() {
        if !command_available("deno") {
            return;
        }
        let tool = CodeExecutionTool::typescript(CodeExecutionConfig {
            default_timeout_ms: 30_000,
            max_output_bytes: 64 * 1024,
            default_cwd: None,
            python_command: default_python_command(),
            typescript_command: "deno".into(),
            sandbox: None,
            sandbox_required: false,
        });

        let output = tool
            .execute(json!({
                "code": "const value: string = Deno.args[0]; console.log(`ts=${value}`);",
                "args": ["ok"]
            }))
            .await
            .unwrap();

        assert_eq!(output["language"], "typescript");
        assert_eq!(output["status"], "success");
        assert_eq!(output["stdout"], "ts=ok\n");
    }

    fn command_available(command: &str) -> bool {
        std::process::Command::new(command)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
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
    async fn restricted_shell_allows_only_allowlisted_direct_commands() {
        let tool = ShellTool::new(ShellToolConfig {
            allowed_commands: vec!["echo".into()],
            ..ShellToolConfig::default()
        });
        let descriptor = ShellTool::descriptor_for_config(&tool.config);
        assert!(descriptor.permissions.shell_restricted);

        let out = tool
            .execute(json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert_eq!(out["status"], "success");
        assert_eq!(out["stdout"].as_str().unwrap().trim(), "hello");
    }

    #[tokio::test]
    async fn restricted_shell_rejects_unlisted_commands_and_metacharacters() {
        let tool = ShellTool::new(ShellToolConfig {
            allowed_commands: vec!["echo".into()],
            ..ShellToolConfig::default()
        });

        let err = tool.execute(json!({"command": "uname"})).await.unwrap_err();
        assert!(err.to_string().contains("not in the shell allowlist"));

        let err = tool
            .execute(json!({"command": "echo hi; uname"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("metacharacters"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_tool_does_not_inherit_unlisted_environment() {
        // This test uses a unique variable and restores it immediately after
        // the child process check; no other test relies on this key.
        unsafe {
            std::env::set_var("AGENT_TOOL_ENV_TEST_SECRET", "leak-me");
        }
        let tool = ShellTool::new(ShellToolConfig::default());
        let out = tool
            .execute(json!({"command": "printf \"$AGENT_TOOL_ENV_TEST_SECRET\""}))
            .await
            .unwrap();
        unsafe {
            std::env::remove_var("AGENT_TOOL_ENV_TEST_SECRET");
        }

        assert_eq!(out["status"], "success");
        assert_eq!(out["stdout"], "");
        assert_eq!(out["sandbox"], "minimal_env");
    }

    #[tokio::test]
    async fn shell_tool_rejects_execution_when_required_sandbox_is_missing() {
        let tool = ShellTool::new(ShellToolConfig {
            sandbox_required: true,
            sandbox: None,
            ..ShellToolConfig::default()
        });

        let err = tool
            .execute(json!({"command": "echo should-not-run"}))
            .await
            .unwrap_err();

        assert!(err.to_string().contains("shell sandbox is required"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn shell_tool_can_run_through_configured_sandbox_wrapper() {
        let python = default_python_command();
        if !command_available(&python) {
            return;
        }
        let wrapper_dir =
            std::env::temp_dir().join(format!("agent-shell-sandbox-test-{}", unique_temp_suffix()));
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let wrapper_path = wrapper_dir.join("wrapper.py");
        std::fs::write(
            &wrapper_path,
            r#"
import os
import subprocess
import sys

os.environ["AGENT_SHELL_SANDBOX_MARKER"] = "wrapped"
raise SystemExit(subprocess.run(sys.argv[1:]).returncode)
"#,
        )
        .unwrap();
        let tool = ShellTool::new(ShellToolConfig {
            sandbox: Some(CodeSandboxConfig {
                label: "test_shell_sandbox_wrapper".into(),
                command: python.clone(),
                args: vec![wrapper_path.to_string_lossy().to_string()],
            }),
            sandbox_required: true,
            ..ShellToolConfig::default()
        });

        let output = tool
            .execute(json!({
                "command": "printf \"$AGENT_SHELL_SANDBOX_MARKER\""
            }))
            .await
            .unwrap();
        let _ = std::fs::remove_dir_all(&wrapper_dir);

        assert_eq!(output["status"], "success");
        assert_eq!(output["sandbox"], "test_shell_sandbox_wrapper");
        assert_eq!(output["sandbox_command"], python);
        assert_eq!(output["stdout"], "wrapped");
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
