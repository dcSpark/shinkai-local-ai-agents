//! `agent-api-client` — transport boundary for `HarnessApi`.
//!
//! v0 provides an in-process client used by local UIs plus a small HTTP client
//! for the daemon routes that mirror the `HarnessApi` surface.

use std::sync::Arc;
use std::{
    io::{Read, Write},
    net::TcpStream,
};

use agent_core::{
    AgentConfig, ApprovalMode, ConfigExplanation, ContextSnapshot, HarnessApi, HarnessError,
    RunResult, ToolCallResult, ToolOutputMode, ToolView, UserInput,
};
use agent_tools::ToolId;
use agent_tracing::{RunEvent, RunId};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HarnessTransport {
    InProcess,
    Daemon { base_url: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ApiClientError {
    #[error("invalid daemon url: {0}")]
    InvalidUrl(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("daemon http {status}: {body}")]
    Http { status: u16, body: String },
}

#[derive(Debug, Clone)]
pub struct DaemonHttpClient {
    base_url: String,
}

impl DaemonHttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub fn get_json(&self, path: &str) -> Result<Value, ApiClientError> {
        self.request_json("GET", path, None)
    }

    pub fn post_json(&self, path: &str, body: Value) -> Result<Value, ApiClientError> {
        self.request_json("POST", path, Some(body))
    }

    fn request_json(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, ApiClientError> {
        let target = parse_http_url(&self.base_url)?;
        let body = body.map(|value| value.to_string()).unwrap_or_default();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nhost: {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            target.host_header,
            body.len()
        );
        let mut stream = TcpStream::connect((&*target.host, target.port))?;
        stream.write_all(request.as_bytes())?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .unwrap_or(500);
        if !(200..300).contains(&status) {
            return Err(ApiClientError::Http {
                status,
                body: body.to_string(),
            });
        }
        Ok(serde_json::from_str(body)?)
    }
}

struct HttpTarget {
    host: String,
    port: u16,
    host_header: String,
}

fn parse_http_url(url: &str) -> Result<HttpTarget, ApiClientError> {
    let Some(rest) = url.strip_prefix("http://") else {
        return Err(ApiClientError::InvalidUrl(url.into()));
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.is_empty() {
        return Err(ApiClientError::InvalidUrl(url.into()));
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map(|(host, port)| {
            Ok::<_, ApiClientError>((
                host.to_string(),
                port.parse::<u16>()
                    .map_err(|_| ApiClientError::InvalidUrl(url.into()))?,
            ))
        })
        .transpose()?
        .unwrap_or_else(|| (authority.to_string(), 80));
    Ok(HttpTarget {
        host,
        port,
        host_header: authority.to_string(),
    })
}

pub struct InProcessHarnessClient {
    inner: Arc<dyn HarnessApi>,
}

pub struct DaemonHarnessClient {
    http: DaemonHttpClient,
}

impl DaemonHarnessClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: DaemonHttpClient::new(base_url),
        }
    }

    pub fn from_http(http: DaemonHttpClient) -> Self {
        Self { http }
    }
}

impl InProcessHarnessClient {
    pub fn new(inner: Arc<dyn HarnessApi>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl HarnessApi for InProcessHarnessClient {
    async fn run(&self, agent: &AgentConfig, input: UserInput) -> Result<RunResult, HarnessError> {
        self.inner.run(agent, input).await
    }

    async fn call_tool(
        &self,
        agent: &AgentConfig,
        tool_id: ToolId,
        input: Value,
    ) -> Result<ToolCallResult, HarnessError> {
        self.inner.call_tool(agent, tool_id, input).await
    }

    fn preview_context(&self, agent: &AgentConfig, input: UserInput) -> ContextSnapshot {
        self.inner.preview_context(agent, input)
    }

    fn explain_config(&self, agent: &AgentConfig) -> ConfigExplanation {
        self.inner.explain_config(agent)
    }

    fn explain_tools(&self, agent: &AgentConfig) -> Vec<ToolView> {
        self.inner.explain_tools(agent)
    }

    fn events(&self, run_id: RunId) -> Vec<RunEvent> {
        self.inner.events(run_id)
    }
}

#[async_trait]
impl HarnessApi for DaemonHarnessClient {
    async fn run(&self, agent: &AgentConfig, input: UserInput) -> Result<RunResult, HarnessError> {
        let value = self
            .http
            .post_json(
                "/run",
                daemon_options_from_agent(agent, Some(input.text), None),
            )
            .map_err(transport_error)?;
        Ok(RunResult {
            run_id: parse_run_id(&value, "run_id")?,
            final_output: value
                .get("final_output")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }

    async fn call_tool(
        &self,
        agent: &AgentConfig,
        tool_id: ToolId,
        input: Value,
    ) -> Result<ToolCallResult, HarnessError> {
        let mut body = input;
        if let Some(map) = body.as_object_mut()
            && agent.tool_policy.approval_mode == ApprovalMode::AutoApprove
        {
            map.insert("__auto_approve".into(), Value::Bool(true));
        }
        let value = self
            .http
            .post_json(&format!("/tool/{}", tool_id.0), body)
            .map_err(transport_error)?;
        Ok(ToolCallResult {
            run_id: parse_run_id(&value, "run_id")?,
            output: value.get("output").cloned().unwrap_or(Value::Null),
            duration_ms: value
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        })
    }

    fn preview_context(&self, agent: &AgentConfig, input: UserInput) -> ContextSnapshot {
        let value = self
            .http
            .post_json(
                "/preview-context",
                daemon_options_from_agent(agent, Some(input.text), None),
            )
            .expect("daemon preview-context request failed");
        serde_json::from_value(value).expect("daemon returned invalid ContextSnapshot")
    }

    fn explain_config(&self, agent: &AgentConfig) -> ConfigExplanation {
        let value = self
            .http
            .post_json(
                "/explain-config",
                daemon_options_from_agent(agent, None, None),
            )
            .expect("daemon explain-config request failed");
        serde_json::from_value(value).expect("daemon returned invalid ConfigExplanation")
    }

    fn explain_tools(&self, agent: &AgentConfig) -> Vec<ToolView> {
        let value = self
            .http
            .post_json(
                "/explain-tools",
                daemon_options_from_agent(agent, None, None),
            )
            .expect("daemon explain-tools request failed");
        serde_json::from_value(value).expect("daemon returned invalid tool list")
    }

    fn events(&self, run_id: RunId) -> Vec<RunEvent> {
        let value = self
            .http
            .get_json(&format!("/trace/{}", run_id.0))
            .expect("daemon trace request failed");
        serde_json::from_value(value).expect("daemon returned invalid trace")
    }
}

fn daemon_options_from_agent(
    agent: &AgentConfig,
    input: Option<String>,
    demo: Option<&str>,
) -> Value {
    let uses_shell = agent_uses_tool(agent, "shell");
    let uses_subagent = agent_uses_tool(agent, "subagent");
    let uses_capability_drafts = agent_uses_tool(agent, "capability_draft");
    let mut value = serde_json::json!({
        "agent_id": &agent.id,
        "model": &agent.model.0,
        "max_tool_calls": agent.tool_policy.max_calls,
        "allowed_tool_categories": &agent.tool_policy.allowed_categories,
        "tool_visibility": agent.tool_policy.visibility,
        "enable_shell": uses_shell,
        "enable_subagent": uses_subagent,
        "enable_capability_drafts": uses_capability_drafts,
        "load_memory": !agent.memory_fragments.is_empty(),
        "load_skills": !agent.skill_views.is_empty(),
        "raw_tool_output": agent.tool_policy.output_mode == ToolOutputMode::Raw,
        "require_approval": agent.tool_policy.approval_mode == ApprovalMode::RequireExplicit,
        "auto_approve": agent.tool_policy.approval_mode == ApprovalMode::AutoApprove,
        "input_cost_per_million": agent.cost_policy.input_cost_per_million,
        "output_cost_per_million": agent.cost_policy.output_cost_per_million,
        "compacted_context": &agent.compacted_context,
    });
    if let Some(input) = input {
        value["input"] = Value::String(input);
    }
    if let Some(demo) = demo {
        value["demo"] = Value::String(demo.to_string());
    }
    value
}

fn agent_uses_tool(agent: &AgentConfig, id: &str) -> bool {
    agent
        .tool_policy
        .allowed_tools
        .iter()
        .any(|tool| tool.0 == id)
        || agent
            .tool_policy
            .required_tool
            .as_ref()
            .is_some_and(|tool| tool.0 == id)
}

fn parse_run_id(value: &Value, key: &str) -> Result<RunId, HarnessError> {
    let raw = value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| HarnessError::Transport(format!("daemon response missing `{key}`")))?;
    uuid::Uuid::parse_str(raw)
        .map(RunId)
        .map_err(|err| HarnessError::Transport(format!("invalid daemon run id: {err}")))
}

fn transport_error(err: ApiClientError) -> HarnessError {
    HarnessError::Transport(err.to_string())
}
