//! `agent-api-client` — transport boundary for `HarnessApi`.
//!
//! v0 provides an in-process client used by local UIs. The remote variant is a
//! typed placeholder so CLI/Tauri can grow daemon transport without changing
//! call sites.

use std::sync::Arc;
use std::{
    io::{Read, Write},
    net::TcpStream,
};

use agent_core::{
    AgentConfig, ConfigExplanation, ContextSnapshot, HarnessApi, HarnessError, RunResult,
    ToolCallResult, ToolView, UserInput,
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
    #[error("remote daemon transport is not implemented yet: {0}")]
    RemoteNotImplemented(String),
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
