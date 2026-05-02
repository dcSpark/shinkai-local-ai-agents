//! `agent-llm` — LLM provider abstraction.
//!
//! v0 ships the trait + a deterministic `FakeProvider` (echo / canned /
//! scripted-sequence modes) so the rest of the runtime can be developed and
//! tested without paid LLM calls (see `specs/architecture.md` §22).
//! Stage 1 adds an opt-in rig-core/OpenAI-compatible provider with native
//! tool-call parsing; the harness still owns actual tool execution.

use std::sync::Mutex;

use async_trait::async_trait;
use rig::{
    OneOrMany,
    client::CompletionClient,
    completion::{self, CompletionModel as _},
    providers::openai,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef(pub String);

impl<S: Into<String>> From<S> for ModelRef {
    fn from(s: S) -> Self {
        Self(s.into())
    }
}

/// Wire-format tool schema sent to the LLM. The runtime-level
/// `agent_tools::ToolDescriptor` is the authoritative type; this is the subset
/// the model itself sees in a request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// One tool call requested by the LLM in a single turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolCall {
    pub id: String,
    pub tool_name: String,
    pub input: Value,
}

/// Conversation message. Modeled after the OpenAI / Anthropic shape so we can
/// later map straight onto real provider SDKs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: Option<String>,
        #[serde(default)]
        tool_calls: Vec<LlmToolCall>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Message::System {
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Message::User {
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: ModelRef,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<LlmToolCall>,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("configuration error: {0}")]
    Config(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError>;
}

/// Configuration for the first real provider slice. Rig reads OpenAI-compatible
/// endpoints through the OpenAI provider; base URL and API key env var are
/// explicit so local/open-router style backends can be used without code
/// changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigProviderConfig {
    pub api_base_url: Option<String>,
    pub api_key_env: String,
    pub model: ModelRef,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
}

impl Default for RigProviderConfig {
    fn default() -> Self {
        Self {
            api_base_url: None,
            api_key_env: "OPENAI_API_KEY".into(),
            model: ModelRef::from("gpt-4o-mini"),
            max_output_tokens: None,
            temperature: None,
        }
    }
}

/// rig-core provider over OpenAI-compatible chat completions. It forwards tool
/// schemas and returns model-proposed tool calls, while the harness remains the
/// only component that executes tools.
pub struct RigProvider {
    config: RigProviderConfig,
    api_key: String,
}

impl RigProvider {
    pub fn from_config(config: RigProviderConfig) -> Result<Self, LlmError> {
        Self::from_config_with_api_key_override(config, None)
    }

    pub fn from_config_with_api_key_override(
        config: RigProviderConfig,
        api_key_override: Option<String>,
    ) -> Result<Self, LlmError> {
        let api_key = match api_key_override.and_then(non_empty_secret) {
            Some(api_key) => api_key,
            None => std::env::var(&config.api_key_env).map_err(|_| {
                LlmError::Config(format!(
                    "environment variable {} is not set",
                    config.api_key_env
                ))
            })?,
        };
        Ok(Self { config, api_key })
    }
}

fn non_empty_secret(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == value.len() {
        Some(value)
    } else {
        Some(trimmed.to_string())
    }
}

#[async_trait]
impl LlmProvider for RigProvider {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let model_id = if req.model.0 == self.config.model.0 {
            self.config.model.0.clone()
        } else {
            req.model.0.clone()
        };

        let mut client_builder = openai::Client::builder().api_key(&self.api_key);
        if let Some(base_url) = self.config.api_base_url.as_deref() {
            client_builder = client_builder.base_url(base_url);
        }
        let client = client_builder
            .build()
            .map_err(|e| LlmError::Provider(e.to_string()))?
            .completions_api();
        let model = client.completion_model(model_id);

        let (prompt, history) = rig_prompt_and_history(&req.messages)?;
        let tools = req
            .tools
            .into_iter()
            .map(|tool| completion::ToolDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
            })
            .collect();
        let response = model
            .completion_request(prompt)
            .messages(history)
            .tools(tools)
            .max_tokens_opt(self.config.max_output_tokens)
            .temperature_opt(self.config.temperature)
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        Ok(rig_response_to_llm(response.choice, response.usage))
    }
}

/// One step in a scripted [`FakeProvider`] sequence.
#[derive(Debug, Clone)]
pub enum FakeStep {
    /// Reply with a final text answer (terminates the run loop).
    Reply(String),
    /// Request a single tool call. The next step in the sequence handles the
    /// tool result.
    CallTool {
        id: String,
        tool: String,
        input: Value,
    },
    /// Request multiple tool calls in one model turn.
    CallTools(Vec<LlmToolCall>),
}

/// Deterministic fake LLM provider. Three modes:
/// - [`FakeProvider::echo`] — replies with `[fake] <last user message>`.
/// - [`FakeProvider::canned`] — replies with a fixed string.
/// - [`FakeProvider::sequence`] — walks a scripted list of [`FakeStep`]s, one
///   per `complete()` call. Used to drive the run loop in tests.
pub struct FakeProvider {
    inner: Mutex<FakeInner>,
}

enum FakeInner {
    Echo,
    Canned(String),
    Sequence { steps: Vec<FakeStep>, cursor: usize },
}

impl FakeProvider {
    pub fn echo() -> Self {
        Self {
            inner: Mutex::new(FakeInner::Echo),
        }
    }
    pub fn canned(s: impl Into<String>) -> Self {
        Self {
            inner: Mutex::new(FakeInner::Canned(s.into())),
        }
    }
    pub fn sequence(steps: Vec<FakeStep>) -> Self {
        Self {
            inner: Mutex::new(FakeInner::Sequence { steps, cursor: 0 }),
        }
    }
}

#[async_trait]
impl LlmProvider for FakeProvider {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let mut response = {
            let mut g = self.inner.lock().expect("fake provider mutex poisoned");
            match &mut *g {
                FakeInner::Echo => {
                    let text = last_user_text(&req.messages)
                        .map(|t| format!("[fake] {t}"))
                        .unwrap_or_else(|| "[fake] (no input)".into());
                    text_response(text)
                }
                FakeInner::Canned(s) => text_response(s.clone()),
                FakeInner::Sequence { steps, cursor } => {
                    let step = steps
                        .get(*cursor)
                        .cloned()
                        .unwrap_or_else(|| FakeStep::Reply("[fake] (script exhausted)".into()));
                    *cursor += 1;
                    match step {
                        FakeStep::Reply(text) => text_response(text),
                        FakeStep::CallTool { id, tool, input } => LlmResponse {
                            content: None,
                            tool_calls: vec![LlmToolCall {
                                id,
                                tool_name: tool,
                                input,
                            }],
                            tokens_in: 0,
                            tokens_out: 0,
                        },
                        FakeStep::CallTools(tool_calls) => LlmResponse {
                            content: None,
                            tool_calls,
                            tokens_in: 0,
                            tokens_out: 0,
                        },
                    }
                }
            }
        };

        response.tokens_in = (req.messages.iter().map(message_chars).sum::<usize>() / 4) as u32;
        response.tokens_out = (response.content.as_deref().map(str::len).unwrap_or(0) / 4) as u32;
        Ok(response)
    }
}

fn text_response(text: String) -> LlmResponse {
    LlmResponse {
        content: Some(text),
        tool_calls: vec![],
        tokens_in: 0,
        tokens_out: 0,
    }
}

fn last_user_text(messages: &[Message]) -> Option<&str> {
    messages.iter().rev().find_map(|m| match m {
        Message::User { content } => Some(content.as_str()),
        _ => None,
    })
}

fn rig_prompt_and_history(
    messages: &[Message],
) -> Result<(completion::Message, Vec<completion::Message>), LlmError> {
    let mut converted = messages
        .iter()
        .map(to_rig_message)
        .collect::<Result<Vec<_>, _>>()?;
    let prompt = converted
        .pop()
        .ok_or_else(|| LlmError::Config("LLM request has no messages".into()))?;
    Ok((prompt, converted))
}

fn to_rig_message(message: &Message) -> Result<completion::Message, LlmError> {
    match message {
        Message::System { content } => Ok(completion::Message::system(content.clone())),
        Message::User { content } => Ok(completion::Message::user(content.clone())),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let mut parts = Vec::<completion::AssistantContent>::new();
            if let Some(content) = content
                && !content.is_empty()
            {
                parts.push(completion::AssistantContent::text(content.clone()));
            }
            parts.extend(tool_calls.iter().map(|call| {
                completion::AssistantContent::tool_call(
                    call.id.clone(),
                    call.tool_name.clone(),
                    call.input.clone(),
                )
            }));
            let content = OneOrMany::many(parts)
                .map_err(|_| LlmError::Config("assistant message has no content".into()))?;
            Ok(completion::Message::Assistant { id: None, content })
        }
        Message::ToolResult {
            tool_call_id,
            content,
        } => Ok(completion::Message::tool_result(
            tool_call_id.clone(),
            content.clone(),
        )),
    }
}

fn rig_response_to_llm(
    choice: OneOrMany<completion::AssistantContent>,
    usage: completion::Usage,
) -> LlmResponse {
    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for item in choice {
        match item {
            completion::AssistantContent::Text(text) => content_parts.push(text.text),
            completion::AssistantContent::ToolCall(call) => {
                tool_calls.push(LlmToolCall {
                    id: call.id,
                    tool_name: call.function.name,
                    input: call.function.arguments,
                });
            }
            completion::AssistantContent::Reasoning(reasoning) => {
                let text = reasoning.display_text();
                if !text.is_empty() {
                    content_parts.push(text);
                }
            }
            completion::AssistantContent::Image(_) => {}
        }
    }
    let content = if content_parts.is_empty() {
        None
    } else {
        Some(content_parts.join("\n"))
    };
    LlmResponse {
        content,
        tool_calls,
        tokens_in: u64_to_u32(usage.input_tokens),
        tokens_out: u64_to_u32(usage.output_tokens),
    }
}

fn u64_to_u32(value: u64) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

fn message_chars(m: &Message) -> usize {
    match m {
        Message::System { content } | Message::User { content } => content.len(),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            content.as_deref().map(str::len).unwrap_or(0)
                + tool_calls.iter().map(|c| c.tool_name.len()).sum::<usize>()
        }
        Message::ToolResult { content, .. } => content.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req_user(text: &str) -> LlmRequest {
        LlmRequest {
            model: ModelRef::from("fake"),
            messages: vec![Message::system("sys"), Message::user(text)],
            tools: vec![],
        }
    }

    #[tokio::test]
    async fn echo_returns_last_user_message() {
        let p = FakeProvider::echo();
        let r = p.complete(req_user("hello")).await.unwrap();
        assert_eq!(r.content.as_deref(), Some("[fake] hello"));
        assert!(r.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn canned_response_is_constant() {
        let p = FakeProvider::canned("always");
        let r = p.complete(req_user("anything")).await.unwrap();
        assert_eq!(r.content.as_deref(), Some("always"));
    }

    #[tokio::test]
    async fn sequence_emits_tool_call_then_text() {
        let p = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"x": 1}),
            },
            FakeStep::Reply("done".into()),
        ]);

        let r1 = p.complete(req_user("go")).await.unwrap();
        assert!(r1.content.is_none());
        assert_eq!(r1.tool_calls.len(), 1);
        assert_eq!(r1.tool_calls[0].tool_name, "echo");
        assert_eq!(r1.tool_calls[0].input, json!({"x": 1}));

        let r2 = p.complete(req_user("go")).await.unwrap();
        assert_eq!(r2.content.as_deref(), Some("done"));
        assert!(r2.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn sequence_exhaustion_returns_fallback_text() {
        let p = FakeProvider::sequence(vec![FakeStep::Reply("only".into())]);
        let _ = p.complete(req_user("go")).await.unwrap();
        let r = p.complete(req_user("again")).await.unwrap();
        assert_eq!(r.content.as_deref(), Some("[fake] (script exhausted)"));
    }

    #[test]
    fn rig_provider_reports_missing_api_key_env() {
        let config = RigProviderConfig {
            api_key_env: "AGENT_HARNESS_TEST_MISSING_KEY".into(),
            ..RigProviderConfig::default()
        };
        match RigProvider::from_config(config) {
            Ok(_) => panic!("missing env var should fail provider construction"),
            Err(err) => assert!(matches!(err, LlmError::Config(_))),
        }
    }

    #[test]
    fn rig_provider_accepts_direct_api_key_override() {
        let config = RigProviderConfig {
            api_key_env: "AGENT_HARNESS_TEST_MISSING_KEY".into(),
            ..RigProviderConfig::default()
        };
        assert!(
            RigProvider::from_config_with_api_key_override(config, Some(" test-key ".into()))
                .is_ok()
        );
    }

    #[test]
    fn rig_choice_maps_text_and_tool_calls() {
        let choice = OneOrMany::many(vec![
            completion::AssistantContent::text("checking"),
            completion::AssistantContent::tool_call("call-1", "shell", json!({"command": "pwd"})),
        ])
        .unwrap();
        let response = rig_response_to_llm(
            choice,
            completion::Usage {
                input_tokens: 12,
                output_tokens: 7,
                total_tokens: 19,
                cached_input_tokens: 0,
                cache_creation_input_tokens: 0,
            },
        );

        assert_eq!(response.content.as_deref(), Some("checking"));
        assert_eq!(response.tokens_in, 12);
        assert_eq!(response.tokens_out, 7);
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].tool_name, "shell");
        assert_eq!(response.tool_calls[0].input, json!({"command": "pwd"}));
    }

    #[test]
    fn rig_message_conversion_preserves_tool_results() {
        let messages = vec![
            Message::system("sys"),
            Message::Assistant {
                content: None,
                tool_calls: vec![LlmToolCall {
                    id: "call-1".into(),
                    tool_name: "echo".into(),
                    input: json!({"x": 1}),
                }],
            },
            Message::ToolResult {
                tool_call_id: "call-1".into(),
                content: "{\"x\":1}".into(),
            },
        ];
        let (prompt, history) = rig_prompt_and_history(&messages).unwrap();

        assert_eq!(history.len(), 2);
        match prompt {
            completion::Message::User { content } => {
                let first = content.first_ref();
                assert!(matches!(
                    first,
                    completion::message::UserContent::ToolResult(_)
                ));
            }
            other => panic!("expected tool result prompt, got {other:?}"),
        }
    }
}
