//! `agent-llm` — LLM provider abstraction.
//!
//! v0 ships the trait + a deterministic `FakeProvider` (echo / canned /
//! scripted-sequence modes) so the rest of the runtime can be developed and
//! tested without paid LLM calls (see `specs/architecture.md` §22).
//! Stage 1 adds an opt-in rig-core/OpenAI-compatible provider with native
//! tool-call parsing; the harness still owns actual tool execution.

use std::sync::Mutex;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use rig::{
    OneOrMany,
    client::CompletionClient,
    completion::{self, CompletionModel as _, GetTokenUsage as _},
    message::{
        Document, DocumentMediaType, DocumentSourceKind, ImageDetail, ImageMediaType, UserContent,
    },
    providers::{anthropic, gemini, openai},
    streaming::StreamedAssistantContent,
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
    UserWithAttachments {
        content: String,
        #[serde(default)]
        attachments: Vec<LlmAttachment>,
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

    pub fn user_with_attachments(
        content: impl Into<String>,
        attachments: Vec<LlmAttachment>,
    ) -> Self {
        Message::UserWithAttachments {
            content: content.into(),
            attachments,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LlmAttachment {
    Image {
        data_base64: String,
        media_type: LlmImageMediaType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<LlmImageDetail>,
    },
    Document {
        data_base64: String,
        media_type: LlmDocumentMediaType,
    },
}

impl LlmAttachment {
    pub fn image_base64(data_base64: impl Into<String>, media_type: LlmImageMediaType) -> Self {
        Self::Image {
            data_base64: data_base64.into(),
            media_type,
            detail: Some(LlmImageDetail::Auto),
        }
    }

    pub fn document_base64(
        data_base64: impl Into<String>,
        media_type: LlmDocumentMediaType,
    ) -> Self {
        Self::Document {
            data_base64: data_base64.into(),
            media_type,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmImageMediaType {
    Jpeg,
    Png,
    Gif,
    Webp,
    Svg,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmDocumentMediaType {
    Pdf,
    Txt,
    Markdown,
    Csv,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmImageDetail {
    Low,
    High,
    Auto,
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

    async fn complete_streaming(
        &self,
        req: LlmRequest,
        _on_delta: LlmStreamCallback<'_>,
    ) -> Result<LlmResponse, LlmError> {
        self.complete(req).await
    }
}

pub type LlmStreamCallback<'a> = &'a mut (dyn FnMut(String) + Send);

/// Configuration for the first real provider slice. Rig reads OpenAI-compatible
/// endpoints through the OpenAI provider; base URL and API key env var are
/// explicit so local/open-router style backends can be used without code
/// changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigProviderConfig {
    pub api_base_url: Option<String>,
    pub api_key_env: String,
    #[serde(default)]
    pub allow_missing_api_key: bool,
    pub model: ModelRef,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_params: Option<Value>,
}

impl Default for RigProviderConfig {
    fn default() -> Self {
        Self {
            api_base_url: None,
            api_key_env: "OPENAI_API_KEY".into(),
            allow_missing_api_key: false,
            model: ModelRef::from("gpt-4o-mini"),
            max_output_tokens: None,
            temperature: None,
            additional_params: None,
        }
    }
}

impl RigProviderConfig {
    pub fn ollama(model: ModelRef) -> Self {
        Self {
            api_base_url: Some(
                std::env::var("OLLAMA_OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:11434/v1".into()),
            ),
            api_key_env: "OLLAMA_API_KEY".into(),
            allow_missing_api_key: true,
            model,
            max_output_tokens: None,
            temperature: None,
            additional_params: None,
        }
    }

    pub fn llama_cpp(model: ModelRef) -> Self {
        Self {
            api_base_url: Some(
                std::env::var("LLAMA_CPP_OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:8080/v1".into()),
            ),
            api_key_env: "LLAMA_CPP_API_KEY".into(),
            allow_missing_api_key: true,
            model,
            max_output_tokens: None,
            temperature: None,
            additional_params: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeProviderConfig {
    pub api_key_env: String,
    #[serde(default)]
    pub allow_missing_api_key: bool,
    pub model: ModelRef,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_params: Option<Value>,
}

impl NativeProviderConfig {
    pub fn anthropic(model: ModelRef) -> Self {
        Self {
            api_key_env: "ANTHROPIC_API_KEY".into(),
            allow_missing_api_key: false,
            model,
            max_output_tokens: None,
            temperature: None,
            additional_params: None,
        }
    }

    pub fn gemini(model: ModelRef) -> Self {
        Self {
            api_key_env: "GEMINI_API_KEY".into(),
            allow_missing_api_key: false,
            model,
            max_output_tokens: None,
            temperature: None,
            additional_params: None,
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
        let api_key = api_key_for_config(
            &config.api_key_env,
            config.allow_missing_api_key,
            api_key_override,
        )?;
        Ok(Self { config, api_key })
    }
}

pub struct AnthropicProvider {
    config: NativeProviderConfig,
    api_key: String,
}

impl AnthropicProvider {
    pub fn from_config(config: NativeProviderConfig) -> Result<Self, LlmError> {
        Self::from_config_with_api_key_override(config, None)
    }

    pub fn from_config_with_api_key_override(
        config: NativeProviderConfig,
        api_key_override: Option<String>,
    ) -> Result<Self, LlmError> {
        let api_key = api_key_for_config(
            &config.api_key_env,
            config.allow_missing_api_key,
            api_key_override,
        )?;
        Ok(Self { config, api_key })
    }
}

pub struct GeminiProvider {
    config: NativeProviderConfig,
    api_key: String,
}

impl GeminiProvider {
    pub fn from_config(config: NativeProviderConfig) -> Result<Self, LlmError> {
        Self::from_config_with_api_key_override(config, None)
    }

    pub fn from_config_with_api_key_override(
        config: NativeProviderConfig,
        api_key_override: Option<String>,
    ) -> Result<Self, LlmError> {
        let api_key = api_key_for_config(
            &config.api_key_env,
            config.allow_missing_api_key,
            api_key_override,
        )?;
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

fn api_key_for_config(
    api_key_env: &str,
    allow_missing_api_key: bool,
    api_key_override: Option<String>,
) -> Result<String, LlmError> {
    match api_key_override.and_then(non_empty_secret) {
        Some(api_key) => Ok(api_key),
        None => match std::env::var(api_key_env) {
            Ok(value) => Ok(value),
            Err(_) if allow_missing_api_key => Ok("local-provider".into()),
            Err(_) => Err(LlmError::Config(format!(
                "environment variable {api_key_env} is not set"
            ))),
        },
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
            .additional_params_opt(self.config.additional_params.clone())
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        Ok(rig_response_to_llm(response.choice, response.usage))
    }

    async fn complete_streaming(
        &self,
        req: LlmRequest,
        on_delta: LlmStreamCallback<'_>,
    ) -> Result<LlmResponse, LlmError> {
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
        let mut stream = model
            .completion_request(prompt)
            .messages(history)
            .tools(tools)
            .max_tokens_opt(self.config.max_output_tokens)
            .temperature_opt(self.config.temperature)
            .additional_params_opt(self.config.additional_params.clone())
            .stream()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        while let Some(chunk) = stream.next().await {
            match chunk.map_err(|e| LlmError::Provider(e.to_string()))? {
                StreamedAssistantContent::Text(text) => {
                    if !text.text.is_empty() {
                        on_delta(text.text);
                    }
                }
                StreamedAssistantContent::Reasoning(reasoning) => {
                    let text = reasoning.display_text();
                    if !text.is_empty() {
                        on_delta(text);
                    }
                }
                StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                    if !reasoning.is_empty() {
                        on_delta(reasoning);
                    }
                }
                StreamedAssistantContent::ToolCall { .. }
                | StreamedAssistantContent::ToolCallDelta { .. }
                | StreamedAssistantContent::Final(_) => {}
            }
        }

        let usage = stream
            .response
            .as_ref()
            .and_then(|response| response.token_usage())
            .unwrap_or_default();
        Ok(rig_response_to_llm(stream.choice, usage))
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let client =
            anthropic::Client::new(&self.api_key).map_err(|e| LlmError::Provider(e.to_string()))?;
        let model = client.completion_model(model_id_for_request(&self.config.model, &req));
        let (prompt, history) = rig_prompt_and_history(&req.messages)?;
        let tools = rig_tool_definitions(req.tools);
        let response = model
            .completion_request(prompt)
            .messages(history)
            .tools(tools)
            .max_tokens_opt(self.config.max_output_tokens)
            .temperature_opt(self.config.temperature)
            .additional_params_opt(self.config.additional_params.clone())
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        Ok(rig_response_to_llm(response.choice, response.usage))
    }

    async fn complete_streaming(
        &self,
        req: LlmRequest,
        on_delta: LlmStreamCallback<'_>,
    ) -> Result<LlmResponse, LlmError> {
        let client =
            anthropic::Client::new(&self.api_key).map_err(|e| LlmError::Provider(e.to_string()))?;
        let model = client.completion_model(model_id_for_request(&self.config.model, &req));
        let (prompt, history) = rig_prompt_and_history(&req.messages)?;
        let tools = rig_tool_definitions(req.tools);
        let mut stream = model
            .completion_request(prompt)
            .messages(history)
            .tools(tools)
            .max_tokens_opt(self.config.max_output_tokens)
            .temperature_opt(self.config.temperature)
            .additional_params_opt(self.config.additional_params.clone())
            .stream()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        drain_rig_stream(&mut stream, on_delta).await?;
        let usage = stream
            .response
            .as_ref()
            .and_then(|response| response.token_usage())
            .unwrap_or_default();
        Ok(rig_response_to_llm(stream.choice, usage))
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        let client =
            gemini::Client::new(&self.api_key).map_err(|e| LlmError::Provider(e.to_string()))?;
        let model = client.completion_model(model_id_for_request(&self.config.model, &req));
        let (prompt, history) = rig_prompt_and_history(&req.messages)?;
        let tools = rig_tool_definitions(req.tools);
        let response = model
            .completion_request(prompt)
            .messages(history)
            .tools(tools)
            .max_tokens_opt(self.config.max_output_tokens)
            .temperature_opt(self.config.temperature)
            .additional_params_opt(self.config.additional_params.clone())
            .send()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        Ok(rig_response_to_llm(response.choice, response.usage))
    }

    async fn complete_streaming(
        &self,
        req: LlmRequest,
        on_delta: LlmStreamCallback<'_>,
    ) -> Result<LlmResponse, LlmError> {
        let client =
            gemini::Client::new(&self.api_key).map_err(|e| LlmError::Provider(e.to_string()))?;
        let model = client.completion_model(model_id_for_request(&self.config.model, &req));
        let (prompt, history) = rig_prompt_and_history(&req.messages)?;
        let tools = rig_tool_definitions(req.tools);
        let mut stream = model
            .completion_request(prompt)
            .messages(history)
            .tools(tools)
            .max_tokens_opt(self.config.max_output_tokens)
            .temperature_opt(self.config.temperature)
            .additional_params_opt(self.config.additional_params.clone())
            .stream()
            .await
            .map_err(|e| LlmError::Provider(e.to_string()))?;

        drain_rig_stream(&mut stream, on_delta).await?;
        let usage = stream
            .response
            .as_ref()
            .and_then(|response| response.token_usage())
            .unwrap_or_default();
        Ok(rig_response_to_llm(stream.choice, usage))
    }
}

async fn drain_rig_stream<S, E, R>(
    stream: &mut S,
    on_delta: LlmStreamCallback<'_>,
) -> Result<(), LlmError>
where
    S: Stream<Item = Result<StreamedAssistantContent<R>, E>> + Unpin,
    E: std::fmt::Display,
{
    while let Some(chunk) = stream.next().await {
        match chunk.map_err(|e| LlmError::Provider(e.to_string()))? {
            StreamedAssistantContent::Text(text) => {
                if !text.text.is_empty() {
                    on_delta(text.text);
                }
            }
            StreamedAssistantContent::Reasoning(reasoning) => {
                let text = reasoning.display_text();
                if !text.is_empty() {
                    on_delta(text);
                }
            }
            StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                if !reasoning.is_empty() {
                    on_delta(reasoning);
                }
            }
            StreamedAssistantContent::ToolCall { .. }
            | StreamedAssistantContent::ToolCallDelta { .. }
            | StreamedAssistantContent::Final(_) => {}
        }
    }
    Ok(())
}

/// One step in a scripted [`FakeProvider`] sequence.
#[derive(Debug, Clone)]
pub enum FakeStep {
    /// Reply with a final text answer (terminates the run loop).
    Reply(String),
    /// Reply with text chunks that should be surfaced through streaming event
    /// callbacks before the final response is returned.
    StreamReply(Vec<String>),
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
        let (response, _) = self.response_for_request(req);
        Ok(response)
    }

    async fn complete_streaming(
        &self,
        req: LlmRequest,
        on_delta: LlmStreamCallback<'_>,
    ) -> Result<LlmResponse, LlmError> {
        let (response, deltas) = self.response_for_request(req);
        for delta in deltas {
            if !delta.is_empty() {
                on_delta(delta);
            }
        }
        Ok(response)
    }
}

impl FakeProvider {
    fn response_for_request(&self, req: LlmRequest) -> (LlmResponse, Vec<String>) {
        let (mut response, deltas) = {
            let mut g = self.inner.lock().expect("fake provider mutex poisoned");
            match &mut *g {
                FakeInner::Echo => {
                    let text = last_user_text(&req.messages)
                        .map(|t| format!("[fake] {t}"))
                        .unwrap_or_else(|| "[fake] (no input)".into());
                    (text_response(text), Vec::new())
                }
                FakeInner::Canned(s) => (text_response(s.clone()), Vec::new()),
                FakeInner::Sequence { steps, cursor } => {
                    let step = steps
                        .get(*cursor)
                        .cloned()
                        .unwrap_or_else(|| FakeStep::Reply("[fake] (script exhausted)".into()));
                    *cursor += 1;
                    match step {
                        FakeStep::Reply(text) => (text_response(text), Vec::new()),
                        FakeStep::StreamReply(deltas) => (text_response(deltas.concat()), deltas),
                        FakeStep::CallTool { id, tool, input } => (
                            LlmResponse {
                                content: None,
                                tool_calls: vec![LlmToolCall {
                                    id,
                                    tool_name: tool,
                                    input,
                                }],
                                tokens_in: 0,
                                tokens_out: 0,
                            },
                            Vec::new(),
                        ),
                        FakeStep::CallTools(tool_calls) => (
                            LlmResponse {
                                content: None,
                                tool_calls,
                                tokens_in: 0,
                                tokens_out: 0,
                            },
                            Vec::new(),
                        ),
                    }
                }
            }
        };

        response.tokens_in = (req.messages.iter().map(message_chars).sum::<usize>() / 4) as u32;
        response.tokens_out = (response.content.as_deref().map(str::len).unwrap_or(0) / 4) as u32;
        (response, deltas)
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
        Message::UserWithAttachments { content, .. } => Some(content.as_str()),
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

fn model_id_for_request(configured_model: &ModelRef, req: &LlmRequest) -> String {
    if req.model.0 == configured_model.0 {
        configured_model.0.clone()
    } else {
        req.model.0.clone()
    }
}

fn rig_tool_definitions(tools: Vec<ToolSchema>) -> Vec<completion::ToolDefinition> {
    tools
        .into_iter()
        .map(|tool| completion::ToolDefinition {
            name: tool.name,
            description: tool.description,
            parameters: tool.input_schema,
        })
        .collect()
}

fn to_rig_message(message: &Message) -> Result<completion::Message, LlmError> {
    match message {
        Message::System { content } => Ok(completion::Message::system(content.clone())),
        Message::User { content } => Ok(completion::Message::user(content.clone())),
        Message::UserWithAttachments {
            content,
            attachments,
        } => {
            let mut parts = Vec::with_capacity(attachments.len() + 1);
            parts.push(UserContent::text(content.clone()));
            for attachment in attachments {
                parts.push(to_rig_attachment(attachment));
            }
            let content = OneOrMany::many(parts)
                .map_err(|_| LlmError::Config("user message has no content".into()))?;
            Ok(completion::Message::User { content })
        }
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

fn to_rig_attachment(attachment: &LlmAttachment) -> UserContent {
    match attachment {
        LlmAttachment::Image {
            data_base64,
            media_type,
            detail,
        } => UserContent::image_base64(
            data_base64.clone(),
            Some(to_rig_image_media_type(*media_type)),
            Some(to_rig_image_detail(detail.unwrap_or(LlmImageDetail::Auto))),
        ),
        LlmAttachment::Document {
            data_base64,
            media_type,
        } => UserContent::Document(Document {
            data: DocumentSourceKind::Base64(data_base64.clone()),
            media_type: Some(to_rig_document_media_type(*media_type)),
            additional_params: None,
        }),
    }
}

fn to_rig_image_media_type(media_type: LlmImageMediaType) -> ImageMediaType {
    match media_type {
        LlmImageMediaType::Jpeg => ImageMediaType::JPEG,
        LlmImageMediaType::Png => ImageMediaType::PNG,
        LlmImageMediaType::Gif => ImageMediaType::GIF,
        LlmImageMediaType::Webp => ImageMediaType::WEBP,
        LlmImageMediaType::Svg => ImageMediaType::SVG,
    }
}

fn to_rig_document_media_type(media_type: LlmDocumentMediaType) -> DocumentMediaType {
    match media_type {
        LlmDocumentMediaType::Pdf => DocumentMediaType::PDF,
        LlmDocumentMediaType::Txt => DocumentMediaType::TXT,
        LlmDocumentMediaType::Markdown => DocumentMediaType::MARKDOWN,
        LlmDocumentMediaType::Csv => DocumentMediaType::CSV,
    }
}

fn to_rig_image_detail(detail: LlmImageDetail) -> ImageDetail {
    match detail {
        LlmImageDetail::Low => ImageDetail::Low,
        LlmImageDetail::High => ImageDetail::High,
        LlmImageDetail::Auto => ImageDetail::Auto,
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
        Message::UserWithAttachments {
            content,
            attachments,
        } => {
            content.len()
                + attachments
                    .iter()
                    .map(|attachment| match attachment {
                        LlmAttachment::Image { data_base64, .. }
                        | LlmAttachment::Document { data_base64, .. } => data_base64.len() / 16,
                    })
                    .sum::<usize>()
        }
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
    async fn fake_provider_stream_reply_calls_delta_sink() {
        let p =
            FakeProvider::sequence(vec![FakeStep::StreamReply(vec!["hel".into(), "lo".into()])]);
        let mut deltas = Vec::new();
        let r = p
            .complete_streaming(req_user("go"), &mut |delta| deltas.push(delta))
            .await
            .unwrap();

        assert_eq!(r.content.as_deref(), Some("hello"));
        assert_eq!(deltas, vec!["hel", "lo"]);
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
    fn native_provider_configs_use_expected_api_key_envs() {
        let anthropic = NativeProviderConfig::anthropic(ModelRef::from("claude-sonnet-4-5"));
        assert_eq!(anthropic.api_key_env, "ANTHROPIC_API_KEY");
        assert!(
            AnthropicProvider::from_config_with_api_key_override(
                anthropic,
                Some(" anthropic-key ".into()),
            )
            .is_ok()
        );

        let gemini = NativeProviderConfig::gemini(ModelRef::from("gemini-2.5-flash"));
        assert_eq!(gemini.api_key_env, "GEMINI_API_KEY");
        assert!(
            GeminiProvider::from_config_with_api_key_override(gemini, Some(" gemini-key ".into()))
                .is_ok()
        );
    }

    #[test]
    fn local_openai_compatible_configs_allow_missing_api_key() {
        let ollama = RigProviderConfig::ollama(ModelRef::from("llama3.1"));
        if std::env::var("OLLAMA_OPENAI_BASE_URL").is_err() {
            assert_eq!(
                ollama.api_base_url.as_deref(),
                Some("http://127.0.0.1:11434/v1")
            );
        }
        assert!(ollama.allow_missing_api_key);
        assert!(RigProvider::from_config(ollama).is_ok());

        let llama_cpp = RigProviderConfig::llama_cpp(ModelRef::from("local-model"));
        if std::env::var("LLAMA_CPP_OPENAI_BASE_URL").is_err() {
            assert_eq!(
                llama_cpp.api_base_url.as_deref(),
                Some("http://127.0.0.1:8080/v1")
            );
        }
        assert!(llama_cpp.allow_missing_api_key);
        assert!(RigProvider::from_config(llama_cpp).is_ok());
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

    #[test]
    fn rig_message_conversion_preserves_user_attachments() {
        let messages = vec![Message::user_with_attachments(
            "extract the receipt",
            vec![
                LlmAttachment::image_base64("aW1hZ2U=", LlmImageMediaType::Png),
                LlmAttachment::document_base64("JVBERi0xLjQ=", LlmDocumentMediaType::Pdf),
            ],
        )];
        let (prompt, history) = rig_prompt_and_history(&messages).unwrap();

        assert!(history.is_empty());
        match prompt {
            completion::Message::User { content } => {
                let parts = content.iter().collect::<Vec<_>>();
                assert_eq!(parts.len(), 3);
                assert!(matches!(
                    parts[0],
                    completion::message::UserContent::Text(_)
                ));
                assert!(matches!(
                    parts[1],
                    completion::message::UserContent::Image(_)
                ));
                assert!(matches!(
                    parts[2],
                    completion::message::UserContent::Document(_)
                ));
            }
            other => panic!("expected user prompt, got {other:?}"),
        }
    }
}
