//! `agent-core` — domain model + orchestration. Defines the `HarnessApi` trait
//! that every UI client (TUI, Tauri webapp, future daemon) calls.
//!
//! v0 surface: a tools-enabled run loop plus inspectable context snapshots.
//! The same context builder powers `preview_context` and each LLM request. The
//! LLM may propose tool calls; the runtime enforces an allowlist and a
//! max-calls budget, executes via the `ToolRegistry`, feeds results back to the
//! LLM, and iterates until the LLM stops calling tools or the budget runs out.
//! Memory, ingestion, approvals, batch, subagent tracing, streaming token
//! events, and observe-only run lifecycle hooks now land in early slices;
//! stronger sandboxing continues to follow `specs/architecture.md` §21.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{File, metadata, read_to_string, remove_file};
use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering as AtomicOrdering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use agent_llm::{LlmProvider, LlmRequest, LlmResponse, LlmToolCall, Message, ModelRef, ToolSchema};
use agent_sandbox::assess_permissions;
use agent_tools::{ToolId, ToolRegistry};
use agent_tracing::{EventId, EventStore, RunEvent, RunEventKind, RunId};

const MID_RUN_GUIDANCE_PREFIX: &str = "Mid-run user guidance:\n";
pub const DEFAULT_MEMORY_BACKEND_ID: &str = "local-markdown-v0";
pub const LOCAL_JSONL_MEMORY_BACKEND_ID: &str = "local-jsonl-v0";
pub const SUPPORTED_MEMORY_BACKEND_IDS: &[&str] =
    &[DEFAULT_MEMORY_BACKEND_ID, LOCAL_JSONL_MEMORY_BACKEND_ID];
const TOOL_OUTPUT_INTERPRETATION_SUMMARY_LIMIT: usize = 240;
const DEFAULT_HOOK_TIMEOUT_MS: u64 = 1_000;
const MAX_HOOK_TIMEOUT_MS: u64 = 5_000;
const MAX_HOOK_RETRY_ATTEMPTS: u32 = 3;
const MAX_HOOK_CONTEXT_FRAGMENTS: usize = 8;
const MAX_HOOK_CONTEXT_FRAGMENT_CHARS: usize = 4_000;
const MAX_HOOK_TOOL_INPUT_CHARS: usize = 16_000;
const MAX_HOOK_TOOL_OUTPUT_CHARS: usize = 16_000;
const MAX_HOOK_STDOUT_BYTES: u64 = 64 * 1024;
const DEFAULT_AUTO_COMPACTION_OUTPUT_TOKENS: u32 = 512;
pub const APPROVAL_UNLOCK_SHA256_ENV: &str = "AGENT_APPROVAL_UNLOCK_SHA256";
pub const APPROVAL_UNLOCK_ENV: &str = "AGENT_APPROVAL_UNLOCK";
pub const APPROVAL_SIGNATURE_SECRET_ENV: &str = "AGENT_APPROVAL_SIGNATURE_SECRET";
pub const APPROVAL_SIGNATURE_ENV: &str = "AGENT_APPROVAL_SIGNATURE";
static HOOK_STDOUT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApprovalUnlockError {
    #[error(
        "approval unlock is required; provide it through --unlock-env or AGENT_APPROVAL_UNLOCK"
    )]
    Missing,
    #[error("approval unlock did not match AGENT_APPROVAL_UNLOCK_SHA256")]
    Invalid,
    #[error("AGENT_APPROVAL_UNLOCK_SHA256 must be a sha256 hex digest")]
    InvalidHash,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApprovalSignatureError {
    #[error(
        "approval signature is required; provide it through --signature-env or AGENT_APPROVAL_SIGNATURE"
    )]
    Missing,
    #[error("approval signature did not match AGENT_APPROVAL_SIGNATURE_SECRET")]
    Invalid,
    #[error("approval signature must be a sha256 hex digest")]
    InvalidFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApprovalControllerError {
    #[error("approval {approval_id} was not found in the run trace")]
    RequestNotFound { approval_id: String },
    #[error("approval {approval_id} is not delegated to a controller agent")]
    NotDelegated { approval_id: String },
    #[error("approval {approval_id} is delegated to {expected}, not {provided}")]
    WrongController {
        approval_id: String,
        expected: String,
        provided: String,
    },
    #[error("approval controller model failed: {0}")]
    Model(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalControllerAssessment {
    pub approval_id: String,
    pub controller_agent: String,
    pub status: String,
    pub scope: Vec<String>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_output: Option<String>,
    #[serde(default)]
    pub tokens_in: u32,
    #[serde(default)]
    pub tokens_out: u32,
    #[serde(default)]
    pub duration_ms: u64,
}

pub fn approval_unlock_sha256(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    hex_digest(&digest)
}

pub fn verify_configured_approval_unlock(
    candidate: Option<&str>,
) -> Result<(), ApprovalUnlockError> {
    let Ok(expected_hash) = std::env::var(APPROVAL_UNLOCK_SHA256_ENV) else {
        return Ok(());
    };
    let fallback = std::env::var(APPROVAL_UNLOCK_ENV).ok();
    verify_approval_unlock_hash(&expected_hash, candidate.or(fallback.as_deref()))
}

pub fn approval_signature_payload(run_id: &str, approval_id: &str) -> String {
    format!("{}:{}", run_id.trim(), approval_id.trim())
}

pub fn approval_signature_hmac_sha256(secret: &str, run_id: &str, approval_id: &str) -> String {
    hmac_sha256_hex(
        secret.as_bytes(),
        approval_signature_payload(run_id, approval_id).as_bytes(),
    )
}

pub fn verify_configured_approval_signature(
    run_id: &str,
    approval_id: &str,
    signature: Option<&str>,
) -> Result<(), ApprovalSignatureError> {
    let Ok(secret) = std::env::var(APPROVAL_SIGNATURE_SECRET_ENV) else {
        return Ok(());
    };
    if secret.is_empty() {
        return Ok(());
    }
    let fallback = std::env::var(APPROVAL_SIGNATURE_ENV).ok();
    verify_approval_signature(
        &secret,
        run_id,
        approval_id,
        signature.or(fallback.as_deref()),
    )
}

pub fn verify_approval_controller_delegate(
    events: &[RunEvent],
    approval_id: &str,
    controller_agent: Option<&str>,
) -> Result<Option<String>, ApprovalControllerError> {
    Ok(
        assess_approval_controller_delegate(events, approval_id, controller_agent)?
            .map(|assessment| assessment.controller_agent),
    )
}

pub fn assess_approval_controller_delegate(
    events: &[RunEvent],
    approval_id: &str,
    controller_agent: Option<&str>,
) -> Result<Option<ApprovalControllerAssessment>, ApprovalControllerError> {
    let Some(provided) = controller_agent
        .map(str::trim)
        .filter(|controller| !controller.is_empty())
    else {
        return Ok(None);
    };
    let Some((expected, scope)) = events.iter().rev().find_map(|event| match &event.kind {
        RunEventKind::ApprovalRequested {
            approval_id: id,
            controller_agent,
            controller_scope,
            ..
        } if id == approval_id => Some((controller_agent.as_deref(), controller_scope.clone())),
        _ => None,
    }) else {
        return Err(ApprovalControllerError::RequestNotFound {
            approval_id: approval_id.to_string(),
        });
    };
    let Some(expected) = expected else {
        return Err(ApprovalControllerError::NotDelegated {
            approval_id: approval_id.to_string(),
        });
    };
    if expected == provided {
        Ok(Some(ApprovalControllerAssessment {
            approval_id: approval_id.to_string(),
            controller_agent: provided.to_string(),
            status: "scope_verified".into(),
            scope,
            reason:
                "controller matched the approval request delegate and is limited to the advertised scope"
                    .into(),
            model: None,
            recommendation: None,
            model_output: None,
            tokens_in: 0,
            tokens_out: 0,
            duration_ms: 0,
        }))
    } else {
        Err(ApprovalControllerError::WrongController {
            approval_id: approval_id.to_string(),
            expected: expected.to_string(),
            provided: provided.to_string(),
        })
    }
}

pub async fn assess_approval_controller_with_model(
    provider: &dyn LlmProvider,
    controller: &AgentConfig,
    events: &[RunEvent],
    approval_id: &str,
    controller_agent: Option<&str>,
) -> Result<ApprovalControllerAssessment, ApprovalControllerError> {
    let provided = controller_agent
        .map(str::trim)
        .filter(|agent| !agent.is_empty())
        .unwrap_or(controller.id.as_str());
    if controller.id != provided {
        return Err(ApprovalControllerError::WrongController {
            approval_id: approval_id.to_string(),
            expected: provided.to_string(),
            provided: controller.id.clone(),
        });
    }
    let scope_verified =
        assess_approval_controller_delegate(events, approval_id, Some(&controller.id))?
            .ok_or_else(|| ApprovalControllerError::NotDelegated {
                approval_id: approval_id.to_string(),
            })?;
    let context = approval_controller_context(events, approval_id)?;
    let user_prompt = approval_controller_prompt(approval_id, &scope_verified.scope, &context);
    let request = LlmRequest {
        model: controller.model.clone(),
        messages: vec![
            Message::System {
                content: controller.system_prompt.clone(),
            },
            Message::User {
                content: user_prompt,
            },
        ],
        tools: Vec::new(),
    };
    let started = Instant::now();
    let response = provider
        .complete(request)
        .await
        .map_err(|err| ApprovalControllerError::Model(err.to_string()))?;
    let duration_ms = started.elapsed().as_millis() as u64;
    let model_output = response.content.unwrap_or_default();
    let recommendation = approval_recommendation_from_text(&model_output);
    let summary = approval_controller_summary(&model_output);

    Ok(ApprovalControllerAssessment {
        approval_id: approval_id.to_string(),
        controller_agent: controller.id.clone(),
        status: "model_assessed".into(),
        scope: scope_verified.scope,
        reason: format!("controller model recommended {recommendation}: {summary}"),
        model: Some(controller.model.0.clone()),
        recommendation: Some(recommendation),
        model_output: Some(model_output),
        tokens_in: response.tokens_in,
        tokens_out: response.tokens_out,
        duration_ms,
    })
}

fn approval_controller_context(
    events: &[RunEvent],
    approval_id: &str,
) -> Result<Value, ApprovalControllerError> {
    let Some(request_event) = events.iter().rev().find(|event| {
        matches!(
            &event.kind,
            RunEventKind::ApprovalRequested { approval_id: id, .. } if id == approval_id
        )
    }) else {
        return Err(ApprovalControllerError::RequestNotFound {
            approval_id: approval_id.to_string(),
        });
    };
    let RunEventKind::ApprovalRequested {
        action,
        reason,
        controller_agent,
        controller_scope,
        ..
    } = &request_event.kind
    else {
        unreachable!("approval request event was matched above");
    };
    let tool_call = request_event.parent_event.and_then(|parent| {
        events.iter().find_map(|event| {
            if event.id != parent {
                return None;
            }
            match &event.kind {
                RunEventKind::ToolCallProposed {
                    call_id,
                    tool_id,
                    input,
                    model,
                    permissions,
                } => Some(json!({
                    "call_id": call_id,
                    "tool_id": tool_id,
                    "input": input,
                    "model": model,
                    "permissions": permissions
                })),
                _ => None,
            }
        })
    });
    let run_input = events.iter().find_map(|event| match &event.kind {
        RunEventKind::RunStarted { input, .. } => Some(input.clone()),
        _ => None,
    });
    Ok(json!({
        "approval_id": approval_id,
        "action": action,
        "reason": reason,
        "controller_agent": controller_agent,
        "controller_scope": controller_scope,
        "run_input": run_input,
        "tool_call": tool_call
    }))
}

fn approval_controller_prompt(approval_id: &str, scope: &[String], context: &Value) -> String {
    let context = serde_json::to_string_pretty(context).unwrap_or_else(|_| context.to_string());
    let scope = if scope.is_empty() {
        "none".into()
    } else {
        scope.join(", ")
    };
    format!(
        "Assess delegated approval request {approval_id}.\n\nAllowed controller scope: {scope}\n\nReturn a concise recommendation starting with exactly one of: APPROVE, REJECT, or NEEDS_HUMAN. Do not execute tools or make the final approval decision; only assess whether the request fits the delegated scope and appears safe from the trace context.\n\nTrace context:\n{context}"
    )
}

fn approval_recommendation_from_text(text: &str) -> String {
    let normalized = text.trim().to_ascii_lowercase();
    let first_word = normalized
        .split(|ch: char| !ch.is_ascii_alphabetic() && ch != '_')
        .find(|part| !part.is_empty())
        .unwrap_or_default();
    match first_word {
        "approve" | "approved" => "approve".into(),
        "reject" | "rejected" | "deny" | "denied" => "reject".into(),
        "needs_human" | "human" | "escalate" => "needs_human".into(),
        _ if normalized.contains("do not approve")
            || normalized.contains("don't approve")
            || normalized.contains("reject")
            || normalized.contains("deny") =>
        {
            "reject".into()
        }
        _ if normalized.contains("approve") => "approve".into(),
        _ => "needs_human".into(),
    }
}

fn approval_controller_summary(text: &str) -> String {
    let summary = normalize_inline(text);
    if summary.is_empty() {
        "model returned no textual rationale".into()
    } else {
        truncate_to_token_estimate(&summary, 80)
    }
}

fn verify_approval_unlock_hash(
    expected_hash: &str,
    candidate: Option<&str>,
) -> Result<(), ApprovalUnlockError> {
    let expected_hash = normalize_sha256_hex(expected_hash)?;
    let Some(candidate) = candidate.filter(|value| !value.is_empty()) else {
        return Err(ApprovalUnlockError::Missing);
    };
    let actual = approval_unlock_sha256(candidate);
    if constant_time_eq(expected_hash.as_bytes(), actual.as_bytes()) {
        Ok(())
    } else {
        Err(ApprovalUnlockError::Invalid)
    }
}

fn normalize_sha256_hex(value: &str) -> Result<String, ApprovalUnlockError> {
    let normalized = value
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or_else(|| value.trim())
        .to_ascii_lowercase();
    if normalized.len() == 64 && normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(normalized)
    } else {
        Err(ApprovalUnlockError::InvalidHash)
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    use std::fmt::Write as _;
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn verify_approval_signature(
    secret: &str,
    run_id: &str,
    approval_id: &str,
    signature: Option<&str>,
) -> Result<(), ApprovalSignatureError> {
    let Some(signature) = signature
        .map(normalize_signature_hex)
        .transpose()?
        .filter(|value| !value.is_empty())
    else {
        return Err(ApprovalSignatureError::Missing);
    };
    let expected = approval_signature_hmac_sha256(secret, run_id, approval_id);
    if constant_time_eq(expected.as_bytes(), signature.as_bytes()) {
        Ok(())
    } else {
        Err(ApprovalSignatureError::Invalid)
    }
}

fn normalize_signature_hex(value: &str) -> Result<String, ApprovalSignatureError> {
    let value = value.trim();
    let normalized = value
        .strip_prefix("sha256=")
        .or_else(|| value.strip_prefix("hmac-sha256="))
        .unwrap_or(value)
        .to_ascii_lowercase();
    if normalized.len() == 64 && normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(normalized)
    } else {
        Err(ApprovalSignatureError::InvalidFormat)
    }
}

fn hmac_sha256_hex(key: &[u8], payload: &[u8]) -> String {
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let digest = Sha256::digest(key);
        key_block[..digest.len()].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut outer = [0x5c; BLOCK_SIZE];
    let mut inner = [0x36; BLOCK_SIZE];
    for idx in 0..BLOCK_SIZE {
        outer[idx] ^= key_block[idx];
        inner[idx] ^= key_block[idx];
    }

    let mut inner_hasher = Sha256::new();
    inner_hasher.update(inner);
    inner_hasher.update(payload);
    let inner_digest = inner_hasher.finalize();

    let mut outer_hasher = Sha256::new();
    outer_hasher.update(outer);
    outer_hasher.update(inner_digest);
    let digest = outer_hasher.finalize();
    hex_digest(&digest)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalControllerPolicy {
    pub agent_id: String,
    pub allowed_tools: Vec<ToolId>,
    pub allowed_categories: Vec<String>,
}

impl ApprovalControllerPolicy {
    pub fn new(
        agent_id: impl Into<String>,
        allowed_tools: Vec<ToolId>,
        allowed_categories: Vec<String>,
    ) -> Option<Self> {
        let agent_id = agent_id.into().trim().to_string();
        if agent_id.is_empty() {
            return None;
        }
        Some(Self {
            agent_id,
            allowed_tools,
            allowed_categories,
        })
    }

    fn allows_descriptor(&self, descriptor: &agent_tools::ToolDescriptor) -> bool {
        self.allowed_tools.contains(&descriptor.id)
            || descriptor
                .categories
                .iter()
                .any(|category| self.allowed_categories.contains(category))
    }

    fn scope_labels(&self) -> Vec<String> {
        self.allowed_tools
            .iter()
            .map(|tool| format!("tool:{}", tool.0))
            .chain(
                self.allowed_categories
                    .iter()
                    .map(|category| format!("category:{category}")),
            )
            .collect()
    }
}

/// Tool-related policy slice. v0 cut of `specs/architecture.md` §4.5
/// `ToolPolicy`.
#[derive(Debug, Clone)]
pub struct ToolPolicy {
    /// Hard cap on the number of tool calls in a single run.
    pub max_calls: u32,
    /// Allowlist by tool id. Empty means "every registered tool is allowed."
    pub allowed_tools: Vec<ToolId>,
    /// Allowlist by category/pack. Empty means "all categories are eligible."
    pub allowed_categories: Vec<String>,
    /// Optional one-shot tool requirement for forced tool-call runs.
    pub required_tool: Option<ToolId>,
    /// How much detail the model/user context sees for registered tools.
    pub visibility: VisibilityLevel,
    /// Per-tool overrides for how much detail the model/user context sees.
    pub per_tool_visibility: HashMap<ToolId, VisibilityLevel>,
    /// How approval-required tools are handled.
    pub approval_mode: ApprovalMode,
    /// Optional delegated controller agent allowed to approve scoped tool calls.
    pub approval_controller: Option<ApprovalControllerPolicy>,
    /// Whether agents may create quarantined tool, skill, or agent drafts.
    pub capability_drafts_enabled: bool,
    /// Optional guidance shown to agents when capability drafting is enabled.
    pub capability_draft_guidance: Option<String>,
    /// Whether tool outputs are returned raw or fed back to the LLM.
    pub output_mode: ToolOutputMode,
    /// Optional model used for interpreted tool outputs instead of the agent model.
    pub output_interpretation_model: Option<ModelRef>,
    /// Per-tool overrides for output interpretation mode.
    pub per_tool_output_modes: HashMap<ToolId, ToolOutputMode>,
    /// Per-tool model overrides for interpreted tool outputs.
    pub per_tool_output_interpretation_models: HashMap<ToolId, ModelRef>,
    /// Per-tool overrides for output interpretation guidance sent to the model.
    pub per_tool_output_guidance: HashMap<ToolId, String>,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self {
            max_calls: 5,
            allowed_tools: Vec::new(),
            allowed_categories: Vec::new(),
            required_tool: None,
            visibility: VisibilityLevel::FullSchema,
            per_tool_visibility: HashMap::new(),
            approval_mode: ApprovalMode::RequireExplicit,
            approval_controller: None,
            capability_drafts_enabled: false,
            capability_draft_guidance: None,
            output_mode: ToolOutputMode::Interpreted,
            output_interpretation_model: None,
            per_tool_output_modes: HashMap::new(),
            per_tool_output_interpretation_models: HashMap::new(),
            per_tool_output_guidance: HashMap::new(),
        }
    }
}

impl ToolPolicy {
    pub fn allows_descriptor(&self, descriptor: &agent_tools::ToolDescriptor) -> bool {
        if self.allowed_tools.is_empty() && self.allowed_categories.is_empty() {
            return true;
        }
        self.allowed_tools.contains(&descriptor.id)
            || descriptor
                .categories
                .iter()
                .any(|category| self.allowed_categories.contains(category))
    }

    pub fn output_mode_for(&self, tool_id: &ToolId) -> ToolOutputMode {
        self.per_tool_output_modes
            .get(tool_id)
            .copied()
            .unwrap_or(self.output_mode)
    }

    pub fn visibility_for(&self, tool_id: &ToolId) -> VisibilityLevel {
        self.per_tool_visibility
            .get(tool_id)
            .copied()
            .unwrap_or(self.visibility)
    }

    pub fn output_interpretation_model_for(
        &self,
        tool_id: &ToolId,
        default_model: &ModelRef,
    ) -> ModelRef {
        self.per_tool_output_interpretation_models
            .get(tool_id)
            .cloned()
            .or_else(|| self.output_interpretation_model.clone())
            .unwrap_or_else(|| default_model.clone())
    }

    pub fn output_interpretation_guidance_for(
        &self,
        tool_id: &ToolId,
        default_guidance: Option<&str>,
    ) -> Option<String> {
        if self.output_mode_for(tool_id) == ToolOutputMode::Raw {
            return None;
        }
        self.per_tool_output_guidance
            .get(tool_id)
            .map(String::as_str)
            .or(default_guidance)
            .map(str::trim)
            .filter(|guidance| !guidance.is_empty())
            .map(ToOwned::to_owned)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    AutoApprove,
    RequireExplicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutputMode {
    Interpreted,
    Raw,
}

fn default_tool_view_output_mode() -> ToolOutputMode {
    ToolOutputMode::Interpreted
}

/// User/config-provided model pricing for trace cost estimates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostPolicy {
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    /// Number of child-run levels a root run may spawn. `1` means root -> child
    /// is allowed, while a child cannot spawn another child.
    pub max_subagent_depth: u32,
    /// Number of repeated appearances of an agent id in its own ancestry.
    /// `0` denies cycles; higher values opt into bounded recursion.
    pub max_recursion_depth: u32,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            max_subagent_depth: 1,
            max_recursion_depth: 0,
        }
    }
}

/// Minimal `AgentConfig` — v0 cut of `specs/architecture.md` §4.5.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub model: ModelRef,
    pub prompt_refinement: Option<PromptRefinement>,
    pub voice: VoiceConfig,
    pub tool_policy: ToolPolicy,
    pub context_policy: ContextPolicy,
    pub execution_policy: ExecutionPolicy,
    pub cost_policy: CostPolicy,
    pub conversation_history: Vec<Message>,
    pub compacted_context: Option<String>,
    pub memory_backend: String,
    pub memory_model: Option<ModelRef>,
    pub memory_fragments: Vec<MemoryFragment>,
    pub ingestion_artifacts: Vec<IngestedArtifactView>,
    pub allowed_skill_categories: Vec<String>,
    pub skill_visibility: VisibilityLevel,
    pub skill_visibility_overrides: HashMap<String, VisibilityLevel>,
    pub skill_views: Vec<SkillView>,
    pub subagent_configs: Vec<AgentConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRefinement {
    pub instructions: String,
    pub model: Option<ModelRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceConfig {
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextPolicy {
    pub compaction: ContextCompactionPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextCompactionPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_before_compaction: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
}

/// v0 cut of `ContextSnapshot` from `specs/architecture.md` §4.6 / §8.
/// Memory, compaction, skill loading, and richer provenance are represented by
/// empty slots so later crates can plug in without changing the preview/run
/// contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    pub system_prompt: String,
    pub conversation: Vec<Message>,
    pub compacted: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_review: Option<CompactionReview>,
    pub loaded_memory: Vec<MemoryFragment>,
    pub loaded_artifacts: Vec<IngestedArtifactView>,
    pub visible_tools: Vec<ToolView>,
    pub visible_skills: Vec<SkillView>,
    pub limits: RuntimeLimits,
    #[serde(default)]
    pub estimated_input_tokens: u32,
    pub provenance: Vec<ProvenanceRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionReview {
    pub mode: CompactionReviewMode,
    pub before_messages: Vec<String>,
    pub compacted_context: String,
    pub visible_messages: Vec<String>,
    pub before_tokens: u32,
    pub after_tokens: u32,
    #[serde(default)]
    pub withheld_before_messages: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionReviewMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFragment {
    pub id: String,
    pub content: String,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedArtifactView {
    pub id: String,
    pub source: String,
    pub sections: usize,
    pub content: String,
    #[serde(default)]
    pub findings: Vec<String>,
    pub provenance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub input_schema: Option<Value>,
    #[serde(default = "default_tool_view_output_mode")]
    pub output_mode: ToolOutputMode,
    #[serde(default)]
    pub output_interpretation_guidance: Option<String>,
    pub visibility: VisibilityLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default)]
    pub estimated_tokens: u32,
    pub visibility: VisibilityLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityLevel {
    FullSchema,
    NameAndDescription,
    NameOnly,
}

impl AgentConfig {
    pub fn skill_visibility_for(&self, skill: &SkillView) -> VisibilityLevel {
        self.skill_visibility_overrides
            .get(&skill.id)
            .copied()
            .unwrap_or(self.skill_visibility)
    }
}

pub fn apply_skill_visibility(skill: &mut SkillView, visibility: VisibilityLevel) {
    skill.visibility = visibility;
    match visibility {
        VisibilityLevel::FullSchema => {}
        VisibilityLevel::NameAndDescription => {
            skill.body = None;
            skill.estimated_tokens = 0;
        }
        VisibilityLevel::NameOnly => {
            skill.description = None;
            skill.body = None;
            skill.estimated_tokens = 0;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeLimits {
    pub max_tool_calls: u32,
    pub remaining_tool_calls: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub fragment: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigExplanation {
    pub agent_id: String,
    pub values: Vec<ConfigValueExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValueExplanation {
    pub key: String,
    pub value: Value,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct UserInput {
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookTrigger {
    RunStarted,
    BeforeContextBuilt,
    ContextBuilt,
    ToolProposed,
    BeforeToolCall,
    ToolCompleted,
    ToolOutputReady,
    RunCompleted,
    RunFailed,
}

impl HookTrigger {
    pub fn from_config_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "run_started" | "run_start" | "on_run_start" | "on_run_started" => {
                Some(Self::RunStarted)
            }
            "before_context_built" | "before_context" | "on_before_context_built" => {
                Some(Self::BeforeContextBuilt)
            }
            "context_built" | "on_context_built" => Some(Self::ContextBuilt),
            "tool_proposed" | "on_tool_proposed" => Some(Self::ToolProposed),
            "before_tool_call"
            | "tool_call_ready"
            | "before_tool_execution"
            | "on_before_tool_call" => Some(Self::BeforeToolCall),
            "tool_completed" | "on_tool_completed" => Some(Self::ToolCompleted),
            "tool_output_ready"
            | "after_tool_output"
            | "before_tool_result"
            | "on_tool_output_ready" => Some(Self::ToolOutputReady),
            "run_completed" | "run_complete" | "on_run_completed" | "on_run_complete" => {
                Some(Self::RunCompleted)
            }
            "run_failed" | "run_fail" | "on_run_failed" | "on_run_fail" => Some(Self::RunFailed),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            HookTrigger::RunStarted => "run_started",
            HookTrigger::BeforeContextBuilt => "before_context_built",
            HookTrigger::ContextBuilt => "context_built",
            HookTrigger::ToolProposed => "tool_proposed",
            HookTrigger::BeforeToolCall => "before_tool_call",
            HookTrigger::ToolCompleted => "tool_completed",
            HookTrigger::ToolOutputReady => "tool_output_ready",
            HookTrigger::RunCompleted => "run_completed",
            HookTrigger::RunFailed => "run_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLifecycleHook {
    pub id: String,
    pub triggers: Vec<HookTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<RunHookHandler>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunHookHandler {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub retry_attempts: u32,
}

impl RunHookHandler {
    pub fn command(command: impl Into<String>, args: Vec<String>, timeout_ms: Option<u64>) -> Self {
        Self {
            command: command.into(),
            args,
            timeout_ms,
            retry_attempts: 0,
        }
    }

    pub fn with_retry_attempts(mut self, retry_attempts: Option<u32>) -> Self {
        self.retry_attempts = retry_attempts
            .unwrap_or_default()
            .min(MAX_HOOK_RETRY_ATTEMPTS);
        self
    }

    fn bounded_timeout(&self) -> Duration {
        Duration::from_millis(
            self.timeout_ms
                .unwrap_or(DEFAULT_HOOK_TIMEOUT_MS)
                .clamp(1, MAX_HOOK_TIMEOUT_MS),
        )
    }
}

impl RunLifecycleHook {
    pub fn new(id: impl Into<String>, triggers: Vec<HookTrigger>) -> Self {
        Self {
            id: id.into(),
            triggers,
            handler: None,
        }
    }

    pub fn with_handler(mut self, handler: RunHookHandler) -> Self {
        self.handler = Some(handler);
        self
    }

    fn observes(&self, trigger: HookTrigger) -> bool {
        !self.id.trim().is_empty() && self.triggers.contains(&trigger)
    }
}

#[derive(Debug, Deserialize)]
struct HookContextOutput {
    #[serde(default, alias = "memory_fragments")]
    context_fragments: Vec<HookContextFragmentInput>,
}

#[derive(Debug, Deserialize)]
struct HookToolOutput {
    #[serde(default, alias = "visible_output", alias = "tool_result")]
    output: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct HookToolCallOutput {
    #[serde(default, alias = "arguments", alias = "tool_input")]
    input: Option<Value>,
    #[serde(default, alias = "deny", alias = "blocked")]
    denied: bool,
    #[serde(default)]
    allow: Option<bool>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug)]
struct HookToolCallDecision {
    input: Option<Value>,
    denial_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HookContextFragmentInput {
    #[serde(default)]
    id: Option<String>,
    content: String,
    #[serde(default)]
    provenance: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub run_id: RunId,
    pub final_output: String,
}

#[derive(Debug, Clone)]
pub struct ToolCallResult {
    pub run_id: RunId,
    pub output: Value,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
struct ToolExecutionResult {
    output: Value,
    cost_usd: Option<f64>,
}

#[derive(Debug, Clone)]
struct PreparedToolCall {
    call: LlmToolCall,
    tool_id: ToolId,
    proposed_event: EventId,
    started_event: EventId,
}

#[derive(Debug, Clone)]
struct ExecutedToolCall {
    prepared: PreparedToolCall,
    execution: ToolExecutionResult,
    duration_ms: u64,
}

#[derive(Debug, Clone)]
struct RunScope {
    run_id: RunId,
    parent_event: Option<EventId>,
    depth: u32,
    agent_chain: Vec<String>,
}

impl RunScope {
    fn root(agent_id: &str) -> Self {
        Self {
            run_id: RunId::new(),
            parent_event: None,
            depth: 0,
            agent_chain: vec![agent_id.to_string()],
        }
    }

    fn child(&self, run_id: RunId, parent_event: EventId, agent_id: String) -> Self {
        let mut agent_chain = self.agent_chain.clone();
        agent_chain.push(agent_id);
        Self {
            run_id,
            parent_event: Some(parent_event),
            depth: self.depth + 1,
            agent_chain,
        }
    }
}

#[derive(Debug, Clone)]
struct RunExecution {
    run_id: RunId,
    final_output: String,
    total_cost_usd: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum HarnessError {
    #[error(transparent)]
    Llm(#[from] agent_llm::LlmError),
    #[error(transparent)]
    Tool(#[from] agent_tools::ToolError),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("budget exhausted: {0}")]
    BudgetExhausted(String),
    #[error("run cancelled: {0}")]
    Cancelled(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("approval required: run_id={run_id} approval_id={approval_id} action={action}")]
    ApprovalRequired {
        run_id: RunId,
        approval_id: String,
        action: String,
    },
}

/// The single API trait every UI client (TUI, Tauri, daemon) consumes.
/// See `specs/architecture.md` §3 (three-surface architecture).
#[async_trait]
pub trait HarnessApi: Send + Sync {
    async fn run(&self, agent: &AgentConfig, input: UserInput) -> Result<RunResult, HarnessError>;

    async fn call_tool(
        &self,
        agent: &AgentConfig,
        tool_id: ToolId,
        input: Value,
    ) -> Result<ToolCallResult, HarnessError>;

    fn preview_context(&self, agent: &AgentConfig, input: UserInput) -> ContextSnapshot;

    fn explain_config(&self, agent: &AgentConfig) -> ConfigExplanation;

    fn explain_tools(&self, agent: &AgentConfig) -> Vec<ToolView>;

    fn events(&self, run_id: RunId) -> Vec<RunEvent>;
}

/// Default `HarnessApi` implementation.
pub struct Harness {
    provider: Arc<dyn LlmProvider>,
    events: Arc<dyn EventStore>,
    tools: Arc<ToolRegistry>,
    hooks: Vec<RunLifecycleHook>,
}

impl Harness {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        events: Arc<dyn EventStore>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            provider,
            events,
            tools,
            hooks: Vec::new(),
        }
    }

    pub fn with_hooks(mut self, hooks: Vec<RunLifecycleHook>) -> Self {
        self.hooks = hooks;
        self
    }

    fn initial_conversation(&self, agent: &AgentConfig, input: &UserInput) -> Vec<Message> {
        let mut conversation = agent.conversation_history.clone();
        conversation.push(Message::user(&input.text));
        conversation
    }

    fn refinement_request(
        &self,
        agent: &AgentConfig,
        refinement: &PromptRefinement,
        original_input: &str,
    ) -> LlmRequest {
        let instructions = refinement_instructions(refinement);
        LlmRequest {
            model: refinement
                .model
                .clone()
                .unwrap_or_else(|| agent.model.clone()),
            messages: vec![
                Message::system(instructions),
                Message::user(original_input.to_string()),
            ],
            tools: Vec::new(),
        }
    }

    fn build_context_snapshot(
        &self,
        agent: &AgentConfig,
        conversation: Vec<Message>,
        calls_used: u32,
    ) -> ContextSnapshot {
        self.build_context_snapshot_with_required_tool(
            agent,
            conversation,
            calls_used,
            agent.tool_policy.required_tool.as_ref(),
        )
    }

    fn build_context_snapshot_with_required_tool(
        &self,
        agent: &AgentConfig,
        conversation: Vec<Message>,
        calls_used: u32,
        required_tool: Option<&ToolId>,
    ) -> ContextSnapshot {
        self.build_context_snapshot_with_required_tool_and_hook_context(
            agent,
            conversation,
            calls_used,
            required_tool,
            Vec::new(),
        )
    }

    fn build_context_snapshot_with_required_tool_and_hook_context(
        &self,
        agent: &AgentConfig,
        conversation: Vec<Message>,
        calls_used: u32,
        required_tool: Option<&ToolId>,
        hook_context_fragments: Vec<MemoryFragment>,
    ) -> ContextSnapshot {
        ContextBuilder {
            agent,
            tools: &self.tools,
            conversation,
            calls_used,
            required_tool,
            hook_context_fragments,
        }
        .build()
    }

    fn llm_request_from_snapshot(
        &self,
        agent: &AgentConfig,
        snapshot: &ContextSnapshot,
    ) -> LlmRequest {
        let mut messages = Vec::with_capacity(snapshot.conversation.len() + 1);
        messages.push(Message::system(&snapshot.system_prompt));
        messages.extend(snapshot.conversation.clone());

        let tools = snapshot
            .visible_tools
            .iter()
            .filter_map(|t| {
                t.input_schema.as_ref().map(|input_schema| ToolSchema {
                    name: t.id.clone(),
                    description: tool_description_for_model(t),
                    input_schema: input_schema.clone(),
                })
            })
            .collect();

        LlmRequest {
            model: agent.model.clone(),
            messages,
            tools,
        }
    }

    fn llm_request_digest(&self, req: &LlmRequest) -> String {
        let payload = json!({
            "model": &req.model.0,
            "messages": &req.messages,
            "tools": &req.tools,
        });
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        let digest = Sha256::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn value_digest(payload: &Value) -> String {
        let bytes = serde_json::to_vec(payload).unwrap_or_default();
        let digest = Sha256::digest(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn fire_hooks(&self, run_id: RunId, parent: EventId, trigger: HookTrigger, payload: &Value) {
        if self.hooks.is_empty() {
            return;
        }
        let payload_digest = Self::value_digest(payload);
        for hook in self.hooks.iter().filter(|hook| hook.observes(trigger)) {
            let fired = self.events.append(
                run_id,
                Some(parent),
                RunEventKind::HookFired {
                    hook_id: hook.id.clone(),
                    trigger: trigger.as_str().to_string(),
                    payload_digest: payload_digest.clone(),
                },
            );
            if let Some(handler) = hook.handler.as_ref()
                && let Err(err) = self.execute_hook_handler_with_retries(
                    run_id,
                    fired.id,
                    handler,
                    &hook.id,
                    trigger,
                    payload,
                    &payload_digest,
                    false,
                )
            {
                self.events.append(
                    run_id,
                    Some(fired.id),
                    RunEventKind::PolicyDenied {
                        reason: format!("hook {} handler failed: {err}", hook.id),
                    },
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_hook_handler_with_retries(
        &self,
        run_id: RunId,
        parent: EventId,
        handler: &RunHookHandler,
        hook_id: &str,
        trigger: HookTrigger,
        payload: &Value,
        payload_digest: &str,
        capture_stdout: bool,
    ) -> Result<Option<String>, String> {
        let max_retries = handler.retry_attempts.min(MAX_HOOK_RETRY_ATTEMPTS);
        for retry_index in 0..=max_retries {
            let attempt = retry_index + 1;
            match execute_hook_handler(
                handler,
                hook_id,
                trigger,
                payload,
                payload_digest,
                capture_stdout,
            ) {
                Ok(output) => return Ok(output),
                Err(err) => {
                    let will_retry = retry_index < max_retries;
                    self.record_hook_failed(
                        run_id,
                        parent,
                        hook_id,
                        trigger,
                        err.clone(),
                        attempt,
                        will_retry,
                    );
                    if will_retry {
                        thread::sleep(Duration::from_millis(25 * u64::from(attempt)));
                    } else {
                        return Err(err);
                    }
                }
            }
        }
        Err("hook retry loop exhausted".into())
    }

    #[allow(clippy::too_many_arguments)]
    fn record_hook_failed(
        &self,
        run_id: RunId,
        parent: EventId,
        hook_id: &str,
        trigger: HookTrigger,
        error: String,
        attempt: u32,
        will_retry: bool,
    ) {
        self.events.append(
            run_id,
            Some(parent),
            RunEventKind::HookFailed {
                hook_id: hook_id.to_string(),
                trigger: trigger.as_str().to_string(),
                error,
                attempt,
                will_retry,
            },
        );
    }

    fn before_context_built_fragments(
        &self,
        run_id: RunId,
        parent: EventId,
        conversation: &[Message],
        calls_used: u32,
        required_tool: Option<&ToolId>,
    ) -> Vec<MemoryFragment> {
        if self.hooks.is_empty() {
            return Vec::new();
        }
        let payload = json!({
            "conversation": conversation,
            "calls_used": calls_used,
            "required_tool": required_tool.map(|tool| tool.0.as_str()),
        });
        let payload_digest = Self::value_digest(&payload);
        let mut fragments = Vec::new();
        for hook in self
            .hooks
            .iter()
            .filter(|hook| hook.observes(HookTrigger::BeforeContextBuilt))
        {
            let fired = self.events.append(
                run_id,
                Some(parent),
                RunEventKind::HookFired {
                    hook_id: hook.id.clone(),
                    trigger: HookTrigger::BeforeContextBuilt.as_str().to_string(),
                    payload_digest: payload_digest.clone(),
                },
            );
            let Some(handler) = hook.handler.as_ref() else {
                continue;
            };
            let output = match self.execute_hook_handler_with_retries(
                run_id,
                fired.id,
                handler,
                &hook.id,
                HookTrigger::BeforeContextBuilt,
                &payload,
                &payload_digest,
                true,
            ) {
                Ok(output) => output,
                Err(err) => {
                    self.events.append(
                        run_id,
                        Some(fired.id),
                        RunEventKind::PolicyDenied {
                            reason: format!("hook {} context extension failed: {err}", hook.id),
                        },
                    );
                    continue;
                }
            };
            match parse_hook_context_fragments(&hook.id, output.as_deref().unwrap_or_default()) {
                Ok(mut hook_fragments) => fragments.append(&mut hook_fragments),
                Err(err) => {
                    self.record_hook_failed(
                        run_id,
                        fired.id,
                        &hook.id,
                        HookTrigger::BeforeContextBuilt,
                        err.to_string(),
                        1,
                        false,
                    );
                    self.events.append(
                        run_id,
                        Some(fired.id),
                        RunEventKind::PolicyDenied {
                            reason: format!("hook {} context extension failed: {err}", hook.id),
                        },
                    );
                }
            }
        }
        fragments
    }

    fn before_tool_call(
        &self,
        run_id: RunId,
        parent: EventId,
        tool_id: &ToolId,
        call_id: &str,
        input: Value,
        model: Option<&str>,
    ) -> Result<Value, HarnessError> {
        if self.hooks.is_empty() {
            return Ok(input);
        }
        let mut current_input = input;
        for hook in self
            .hooks
            .iter()
            .filter(|hook| hook.observes(HookTrigger::BeforeToolCall))
        {
            let payload = json!({
                "tool_id": tool_id.0,
                "call_id": call_id,
                "input": self.redacted_tool_payload(tool_id, &current_input),
                "model": model,
                "permissions": self.tool_permissions_audit(tool_id),
            });
            let payload_digest = Self::value_digest(&payload);
            let fired = self.events.append(
                run_id,
                Some(parent),
                RunEventKind::HookFired {
                    hook_id: hook.id.clone(),
                    trigger: HookTrigger::BeforeToolCall.as_str().to_string(),
                    payload_digest: payload_digest.clone(),
                },
            );
            let Some(handler) = hook.handler.as_ref() else {
                continue;
            };
            let output = match self.execute_hook_handler_with_retries(
                run_id,
                fired.id,
                handler,
                &hook.id,
                HookTrigger::BeforeToolCall,
                &payload,
                &payload_digest,
                true,
            ) {
                Ok(output) => output,
                Err(err) => {
                    self.events.append(
                        run_id,
                        Some(fired.id),
                        RunEventKind::PolicyDenied {
                            reason: format!("hook {} tool call mutation failed: {err}", hook.id),
                        },
                    );
                    continue;
                }
            };
            match parse_hook_tool_call(&hook.id, output.as_deref().unwrap_or_default()) {
                Ok(decision) => {
                    if let Some(reason) = decision.denial_reason {
                        self.record_policy_denied(run_id, fired.id, &reason);
                        return Err(HarnessError::PolicyDenied(reason));
                    }
                    if let Some(input) = decision.input {
                        current_input = input;
                    }
                }
                Err(err) => {
                    self.record_hook_failed(
                        run_id,
                        fired.id,
                        &hook.id,
                        HookTrigger::BeforeToolCall,
                        err.to_string(),
                        1,
                        false,
                    );
                    self.events.append(
                        run_id,
                        Some(fired.id),
                        RunEventKind::PolicyDenied {
                            reason: format!("hook {} tool call mutation failed: {err}", hook.id),
                        },
                    );
                }
            }
        }
        Ok(current_input)
    }

    fn tool_output_ready(
        &self,
        run_id: RunId,
        parent: EventId,
        tool_id: &ToolId,
        call_id: &str,
        visible_output: Value,
    ) -> Value {
        if self.hooks.is_empty() {
            return visible_output;
        }
        let mut current_output = visible_output;
        for hook in self
            .hooks
            .iter()
            .filter(|hook| hook.observes(HookTrigger::ToolOutputReady))
        {
            let payload = json!({
                "tool_id": tool_id.0,
                "call_id": call_id,
                "visible_output": current_output.clone(),
            });
            let payload_digest = Self::value_digest(&payload);
            let fired = self.events.append(
                run_id,
                Some(parent),
                RunEventKind::HookFired {
                    hook_id: hook.id.clone(),
                    trigger: HookTrigger::ToolOutputReady.as_str().to_string(),
                    payload_digest: payload_digest.clone(),
                },
            );
            let Some(handler) = hook.handler.as_ref() else {
                continue;
            };
            let output = match self.execute_hook_handler_with_retries(
                run_id,
                fired.id,
                handler,
                &hook.id,
                HookTrigger::ToolOutputReady,
                &payload,
                &payload_digest,
                true,
            ) {
                Ok(output) => output,
                Err(err) => {
                    self.events.append(
                        run_id,
                        Some(fired.id),
                        RunEventKind::PolicyDenied {
                            reason: format!("hook {} tool output mutation failed: {err}", hook.id),
                        },
                    );
                    continue;
                }
            };
            match parse_hook_tool_output(&hook.id, output.as_deref().unwrap_or_default()) {
                Ok(Some(output)) => current_output = output,
                Ok(None) => {}
                Err(err) => {
                    self.record_hook_failed(
                        run_id,
                        fired.id,
                        &hook.id,
                        HookTrigger::ToolOutputReady,
                        err.to_string(),
                        1,
                        false,
                    );
                    self.events.append(
                        run_id,
                        Some(fired.id),
                        RunEventKind::PolicyDenied {
                            reason: format!("hook {} tool output mutation failed: {err}", hook.id),
                        },
                    );
                }
            }
        }
        current_output
    }

    fn record_run_started(
        &self,
        run_id: RunId,
        parent: Option<EventId>,
        agent_id: &str,
        input: &str,
    ) -> RunEvent {
        let event = self.events.append(
            run_id,
            parent,
            RunEventKind::RunStarted {
                agent_id: agent_id.to_string(),
                input: input.to_string(),
            },
        );
        self.fire_hooks(
            run_id,
            event.id,
            HookTrigger::RunStarted,
            &json!({ "agent_id": agent_id, "input": input }),
        );
        event
    }

    fn record_context_built(
        &self,
        run_id: RunId,
        parent: EventId,
        snapshot: &ContextSnapshot,
    ) -> RunEvent {
        let snapshot = serde_json::to_value(snapshot).unwrap_or(Value::Null);
        let event = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::ContextBuilt {
                snapshot: snapshot.clone(),
            },
        );
        self.fire_hooks(
            run_id,
            event.id,
            HookTrigger::ContextBuilt,
            &json!({ "snapshot": snapshot }),
        );
        event
    }

    #[allow(clippy::too_many_arguments)]
    fn record_tool_proposed(
        &self,
        run_id: RunId,
        parent: EventId,
        call_id: &str,
        tool_id: &str,
        input: Value,
        model: Option<String>,
        permissions: Option<Value>,
    ) -> RunEvent {
        let payload = json!({
            "call_id": call_id,
            "tool_id": tool_id,
            "input": &input,
            "model": &model,
            "permissions": &permissions,
        });
        let event = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::ToolCallProposed {
                call_id: call_id.to_string(),
                tool_id: tool_id.to_string(),
                input,
                model,
                permissions,
            },
        );
        self.fire_hooks(run_id, event.id, HookTrigger::ToolProposed, &payload);
        event
    }

    fn record_tool_completed(
        &self,
        run_id: RunId,
        parent: EventId,
        call_id: &str,
        output: Value,
        cost_usd: Option<f64>,
        duration_ms: u64,
    ) -> RunEvent {
        let payload = json!({
            "call_id": call_id,
            "output": &output,
            "cost_usd": cost_usd,
            "duration_ms": duration_ms,
        });
        let event = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::ToolCallCompleted {
                call_id: call_id.to_string(),
                output,
                cost_usd,
                duration_ms,
            },
        );
        self.fire_hooks(run_id, event.id, HookTrigger::ToolCompleted, &payload);
        event
    }

    fn record_run_completed(
        &self,
        run_id: RunId,
        parent: EventId,
        final_output: &str,
        total_cost_usd: Option<f64>,
        total_duration_ms: u64,
    ) -> RunEvent {
        let event = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::RunCompleted {
                final_output: final_output.to_string(),
                total_cost_usd,
                total_duration_ms,
            },
        );
        self.fire_hooks(
            run_id,
            event.id,
            HookTrigger::RunCompleted,
            &json!({
                "final_output": final_output,
                "total_cost_usd": total_cost_usd,
                "total_duration_ms": total_duration_ms,
            }),
        );
        event
    }

    fn record_run_failed(&self, run_id: RunId, parent: EventId, reason: &str) -> RunEvent {
        let event = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::RunFailed {
                reason: reason.to_string(),
            },
        );
        self.fire_hooks(
            run_id,
            event.id,
            HookTrigger::RunFailed,
            &json!({ "reason": reason }),
        );
        event
    }

    fn llm_cost_usd(&self, agent: &AgentConfig, tokens_in: u32, tokens_out: u32) -> Option<f64> {
        let input_cost = agent
            .cost_policy
            .input_cost_per_million
            .map(|rate| rate * f64::from(tokens_in) / 1_000_000.0);
        let output_cost = agent
            .cost_policy
            .output_cost_per_million
            .map(|rate| rate * f64::from(tokens_out) / 1_000_000.0);

        match (input_cost, output_cost) {
            (None, None) => None,
            (Some(input), None) => Some(input),
            (None, Some(output)) => Some(output),
            (Some(input), Some(output)) => Some(input + output),
        }
    }

    async fn complete_llm_with_trace(
        &self,
        req: LlmRequest,
        run_id: RunId,
        parent: EventId,
    ) -> Result<(LlmResponse, u64), HarnessError> {
        let t0 = Instant::now();
        let mut on_delta = |delta: String| {
            if !delta.is_empty() {
                self.events
                    .append(run_id, Some(parent), RunEventKind::LlmStreamToken { delta });
            }
        };
        let response = match self.provider.complete_streaming(req, &mut on_delta).await {
            Ok(response) => response,
            Err(err) => {
                self.record_run_failed(run_id, parent, &err.to_string());
                return Err(err.into());
            }
        };
        Ok((response, t0.elapsed().as_millis() as u64))
    }

    async fn refine_input_for_run(
        &self,
        agent: &AgentConfig,
        run_id: RunId,
        parent: EventId,
        original_input: &str,
    ) -> Result<(String, Option<f64>), HarnessError> {
        let Some(refinement) = agent.prompt_refinement.as_ref() else {
            return Ok((original_input.to_string(), None));
        };

        let req = self.refinement_request(agent, refinement, original_input);
        let model = req.model.0.clone();
        let instructions = refinement_instructions(refinement);
        let started = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::PromptRefinementStarted {
                model,
                original_input: original_input.to_string(),
                instructions,
            },
        );

        let t0 = Instant::now();
        let response = match self.provider.complete(req).await {
            Ok(response) => response,
            Err(err) => {
                self.record_run_failed(run_id, started.id, &err.to_string());
                return Err(err.into());
            }
        };
        let duration_ms = t0.elapsed().as_millis() as u64;
        let cost_usd = self.llm_cost_usd(agent, response.tokens_in, response.tokens_out);
        let refined_input = response.content.unwrap_or_default().trim().to_string();
        let refined_input = if refined_input.is_empty() {
            original_input.to_string()
        } else {
            refined_input
        };

        self.events.append(
            run_id,
            Some(started.id),
            RunEventKind::PromptRefinementCompleted {
                refined_input: refined_input.clone(),
                tokens_in: response.tokens_in,
                tokens_out: response.tokens_out,
                cost_usd,
                duration_ms,
            },
        );

        Ok((refined_input, cost_usd))
    }

    fn ensure_tool_allowed(
        &self,
        agent: &AgentConfig,
        tool_id: &ToolId,
    ) -> Result<(), HarnessError> {
        if let Some(descriptor) = self.tools.descriptor(tool_id) {
            if !agent.tool_policy.allows_descriptor(descriptor) {
                return Err(HarnessError::PolicyDenied(format!(
                    "tool {} not allowed by agent tool policy",
                    tool_id.0
                )));
            }
            return Ok(());
        }
        if (!agent.tool_policy.allowed_tools.is_empty()
            && !agent.tool_policy.allowed_tools.contains(tool_id))
            || !agent.tool_policy.allowed_categories.is_empty()
        {
            return Err(HarnessError::PolicyDenied(format!(
                "tool {} not allowed by agent tool policy",
                tool_id.0
            )));
        }
        Ok(())
    }

    fn stringify_tool_output(output: &Value) -> String {
        match output {
            Value::String(s) => s.clone(),
            other => serde_json::to_string(other).unwrap_or_else(|_| "<unserializable>".into()),
        }
    }

    fn redacted_tool_payload(&self, tool_id: &ToolId, payload: &Value) -> Value {
        let secret_scoped = self
            .tools
            .descriptor(tool_id)
            .is_some_and(|descriptor| descriptor.permissions.secrets);
        if secret_scoped || contains_secret_marker(payload) {
            redact_secret_markers(payload, secret_scoped)
        } else {
            payload.clone()
        }
    }

    fn tool_permissions_audit(&self, tool_id: &ToolId) -> Option<Value> {
        self.tools.descriptor(tool_id).map(|descriptor| {
            let sandbox = assess_permissions(&descriptor.permissions);
            json!({
                "shell": descriptor.permissions.shell,
                "shell_restricted": descriptor.permissions.shell_restricted,
                "file_read": descriptor.permissions.file_read,
                "file_write": descriptor.permissions.file_write,
                "network": descriptor.permissions.network,
                "secrets": descriptor.permissions.secrets,
                "wallet": descriptor.permissions.wallet,
                "payment": descriptor.permissions.payment,
                "browser_profile": descriptor.permissions.browser_profile,
                "approval_required": tool_requires_approval(descriptor),
                "sandbox": sandbox
            })
        })
    }

    fn tool_output_interpretation_summary(output: &Value) -> String {
        let output = Self::stringify_tool_output(output);
        let mut summary = String::new();
        for ch in output
            .chars()
            .take(TOOL_OUTPUT_INTERPRETATION_SUMMARY_LIMIT)
        {
            summary.push(ch);
        }
        if output.chars().count() > TOOL_OUTPUT_INTERPRETATION_SUMMARY_LIMIT {
            summary.push_str("...");
        }
        summary
    }

    fn record_tool_output_interpreted(
        &self,
        model: &ModelRef,
        run_id: RunId,
        parent: EventId,
        call_id: &str,
        output: &Value,
    ) {
        self.events.append(
            run_id,
            Some(parent),
            RunEventKind::ToolOutputInterpreted {
                call_id: call_id.to_string(),
                model: model.0.clone(),
                summary: Self::tool_output_interpretation_summary(output),
            },
        );
    }

    fn merge_pending_interpretation_model(
        pending: &mut Option<ModelRef>,
        model: ModelRef,
        agent: &AgentConfig,
    ) {
        match pending {
            None => *pending = Some(model),
            Some(existing) if existing == &model => {}
            Some(existing) => {
                *existing = agent
                    .tool_policy
                    .output_interpretation_model
                    .clone()
                    .unwrap_or_else(|| agent.model.clone());
            }
        }
    }

    fn record_approval_gate(
        &self,
        agent: &AgentConfig,
        run_id: RunId,
        parent: EventId,
        call_id: &str,
        tool_id: &ToolId,
    ) -> Result<(), HarnessError> {
        let Some(descriptor) = self.tools.descriptor(tool_id) else {
            return Ok(());
        };
        if !tool_requires_approval(descriptor) {
            return Ok(());
        }
        let approval_id = format!("approval-{call_id}");
        let action = format!("tool:{}", tool_id.0);
        let reason = permission_reason(descriptor);
        let controller = agent
            .tool_policy
            .approval_controller
            .as_ref()
            .filter(|controller| controller.allows_descriptor(descriptor));
        let requested = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::ApprovalRequested {
                approval_id: approval_id.clone(),
                action: action.clone(),
                reason,
                controller_agent: controller.map(|controller| controller.agent_id.clone()),
                controller_scope: controller
                    .map(ApprovalControllerPolicy::scope_labels)
                    .unwrap_or_default(),
            },
        );
        match agent.tool_policy.approval_mode {
            ApprovalMode::AutoApprove => {
                self.events.append(
                    run_id,
                    Some(requested.id),
                    RunEventKind::ApprovalResolved {
                        approval_id,
                        approved: true,
                        delegated_controller: None,
                    },
                );
                Ok(())
            }
            ApprovalMode::RequireExplicit => Err(HarnessError::ApprovalRequired {
                run_id,
                approval_id,
                action,
            }),
        }
    }

    fn record_context_references(
        &self,
        agent: &AgentConfig,
        run_id: RunId,
        parent: EventId,
        snapshot: &ContextSnapshot,
    ) {
        if !snapshot.loaded_memory.is_empty() {
            self.events.append(
                run_id,
                Some(parent),
                RunEventKind::MemoryRead {
                    backend: agent.memory_backend.clone(),
                    fragment_ids: snapshot
                        .loaded_memory
                        .iter()
                        .map(|fragment| fragment.id.clone())
                        .collect(),
                },
            );
        }
        for artifact in &snapshot.loaded_artifacts {
            self.events.append(
                run_id,
                Some(parent),
                RunEventKind::IngestionReferenced {
                    artifact_id: artifact.id.clone(),
                    source: artifact.source.clone(),
                },
            );
        }
    }

    fn record_policy_denied(&self, run_id: RunId, parent: EventId, reason: &str) {
        self.events.append(
            run_id,
            Some(parent),
            RunEventKind::PolicyDenied {
                reason: reason.to_string(),
            },
        );
    }

    fn cancellation_reason(&self, run_id: RunId) -> Option<String> {
        self.events
            .events(run_id)
            .into_iter()
            .rev()
            .find_map(|event| match event.kind {
                RunEventKind::RunCancelled { reason } => Some(reason),
                _ => None,
            })
    }

    fn check_cancelled(&self, run_id: RunId) -> Result<(), HarnessError> {
        match self.cancellation_reason(run_id) {
            Some(reason) => Err(HarnessError::Cancelled(reason)),
            None => Ok(()),
        }
    }

    fn append_pending_guidance(
        &self,
        run_id: RunId,
        conversation: &mut Vec<Message>,
        consumed_guidance_events: &mut HashSet<EventId>,
    ) {
        for event in self.events.events(run_id) {
            if consumed_guidance_events.contains(&event.id) {
                continue;
            }
            if let RunEventKind::GuidanceInjected { content } = event.kind {
                consumed_guidance_events.insert(event.id);
                conversation.push(Message::system(format!(
                    "{MID_RUN_GUIDANCE_PREFIX}{}",
                    content.trim()
                )));
            }
        }
    }

    fn tool_is_parallel_safe(&self, tool_id: &ToolId) -> bool {
        let Some(descriptor) = self.tools.descriptor(tool_id) else {
            return false;
        };
        !descriptor.requires_approval
            && !descriptor.permissions.shell
            && !descriptor.permissions.file_read
            && !descriptor.permissions.file_write
            && !descriptor.permissions.network
            && !descriptor.permissions.secrets
            && !descriptor.permissions.wallet
            && !descriptor.permissions.payment
            && !descriptor.permissions.browser_profile
            && tool_id.0 != "subagent"
    }

    async fn execute_prepared_tool_call(
        &self,
        agent: &AgentConfig,
        scope: &RunScope,
        prepared: PreparedToolCall,
    ) -> Result<ExecutedToolCall, (PreparedToolCall, HarnessError)> {
        let tool_t0 = Instant::now();
        let execution = self
            .execute_tool_or_subagent(
                agent,
                scope,
                prepared.started_event,
                &prepared.tool_id,
                prepared.call.input.clone(),
            )
            .await
            .map_err(|err| (prepared.clone(), err))?;
        Ok(ExecutedToolCall {
            prepared,
            execution,
            duration_ms: tool_t0.elapsed().as_millis() as u64,
        })
    }

    async fn execute_tool_or_subagent(
        &self,
        agent: &AgentConfig,
        scope: &RunScope,
        parent_event: EventId,
        tool_id: &ToolId,
        input: Value,
    ) -> Result<ToolExecutionResult, HarnessError> {
        if tool_id.0 == "subagent" {
            return self
                .execute_subagent(agent, scope, parent_event, tool_id, input)
                .await;
        }
        if let Some(child_agent_id) = self.external_agent_child_agent_id(tool_id) {
            return self
                .execute_external_agent_tool(scope, parent_event, tool_id, input, child_agent_id)
                .await;
        }
        let output = self
            .tools
            .execute(tool_id, input)
            .await
            .map_err(HarnessError::from)?;
        Ok(ToolExecutionResult {
            output,
            cost_usd: None,
        })
    }

    fn external_agent_child_agent_id(&self, tool_id: &ToolId) -> Option<String> {
        let descriptor = self.tools.descriptor(tool_id)?;
        descriptor
            .categories
            .iter()
            .any(|category| category == "external-agent")
            .then(|| format!("external-agent:{}", tool_id.0))
    }

    async fn execute_external_agent_tool(
        &self,
        scope: &RunScope,
        parent_event: EventId,
        tool_id: &ToolId,
        input: Value,
        child_agent_id: String,
    ) -> Result<ToolExecutionResult, HarnessError> {
        let child_run_id = RunId::new();
        let child_link = self.events.append(
            scope.run_id,
            Some(parent_event),
            RunEventKind::ChildRunStarted {
                child_run_id,
                agent_id: child_agent_id,
            },
        );

        let output = match self.tools.execute(tool_id, input).await {
            Ok(output) => output,
            Err(err) => {
                self.events.append(
                    scope.run_id,
                    Some(child_link.id),
                    RunEventKind::ChildRunCompleted {
                        child_run_id,
                        status: "failed".into(),
                    },
                );
                return Err(err.into());
            }
        };

        self.events.append(
            scope.run_id,
            Some(child_link.id),
            RunEventKind::ChildRunCompleted {
                child_run_id,
                status: "succeeded".into(),
            },
        );

        Ok(ToolExecutionResult {
            output,
            cost_usd: None,
        })
    }

    async fn execute_subagent(
        &self,
        agent: &AgentConfig,
        scope: &RunScope,
        parent_event: EventId,
        tool_id: &ToolId,
        input: Value,
    ) -> Result<ToolExecutionResult, HarnessError> {
        if !self.tools.contains(tool_id) {
            return Err(agent_tools::ToolError::NotFound(tool_id.clone()).into());
        }

        let (prompt, requested_agent_id) = parse_subagent_input(&input)?;
        if scope.depth >= agent.execution_policy.max_subagent_depth {
            let reason = format!(
                "subagent depth limit reached (depth={}, limit={})",
                scope.depth, agent.execution_policy.max_subagent_depth
            );
            self.record_policy_denied(scope.run_id, parent_event, &reason);
            return Err(HarnessError::PolicyDenied(reason));
        }

        let child_run_id = RunId::new();
        let child_agent_id = requested_agent_id.unwrap_or_else(|| format!("{}:subagent", agent.id));
        let existing_agent_occurrences = scope
            .agent_chain
            .iter()
            .filter(|ancestor| *ancestor == &child_agent_id)
            .count() as u32;
        if existing_agent_occurrences > agent.execution_policy.max_recursion_depth {
            let reason = format!(
                "subagent recursion denied for agent {child_agent_id} (seen={}, limit={})",
                existing_agent_occurrences, agent.execution_policy.max_recursion_depth
            );
            self.record_policy_denied(scope.run_id, parent_event, &reason);
            return Err(HarnessError::PolicyDenied(reason));
        }

        let selected_child_config = agent
            .subagent_configs
            .iter()
            .find(|candidate| candidate.id == child_agent_id)
            .cloned();
        let used_saved_agent_config = selected_child_config.is_some();

        let child_link = self.events.append(
            scope.run_id,
            Some(parent_event),
            RunEventKind::ChildRunStarted {
                child_run_id,
                agent_id: child_agent_id.clone(),
            },
        );

        let mut child_agent = selected_child_config.unwrap_or_else(|| {
            let mut child_agent = agent.clone();
            child_agent.id = child_agent_id.clone();
            child_agent.name = format!("{} Subagent", agent.name);
            child_agent
        });
        if child_agent.subagent_configs.is_empty() {
            child_agent.subagent_configs = agent.subagent_configs.clone();
        }

        let child_scope = scope.child(child_run_id, child_link.id, child_agent_id.clone());
        let execution = match self
            .run_scoped(
                &child_agent,
                UserInput {
                    text: prompt.clone(),
                },
                child_scope,
            )
            .await
        {
            Ok(execution) => execution,
            Err(err) => {
                self.events.append(
                    scope.run_id,
                    Some(child_link.id),
                    RunEventKind::ChildRunCompleted {
                        child_run_id,
                        status: "failed".into(),
                    },
                );
                return Err(err);
            }
        };
        debug_assert_eq!(execution.run_id, child_run_id);

        self.events.append(
            scope.run_id,
            Some(child_link.id),
            RunEventKind::ChildRunCompleted {
                child_run_id,
                status: "succeeded".into(),
            },
        );

        Ok(ToolExecutionResult {
            output: json!({
                "child_run_id": child_run_id.0,
                "agent_id": child_agent_id,
                "used_saved_agent_config": used_saved_agent_config,
                "final_output": execution.final_output,
                "total_cost_usd": execution.total_cost_usd
            }),
            cost_usd: execution.total_cost_usd,
        })
    }
}

fn parse_subagent_input(input: &Value) -> Result<(String, Option<String>), HarnessError> {
    let prompt = input
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .ok_or_else(|| agent_tools::ToolError::InvalidInput("missing prompt".into()))?
        .to_string();
    let agent_id = input
        .get("agent_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|agent_id| !agent_id.is_empty())
        .map(ToOwned::to_owned);
    Ok((prompt, agent_id))
}

fn tool_description_for_model(tool: &ToolView) -> String {
    let mut description = tool.description.clone().unwrap_or_default();
    if let Some(guidance) = &tool.output_interpretation_guidance {
        let guidance = guidance.trim();
        if !guidance.is_empty() {
            if !description.is_empty() {
                description.push_str("\n\n");
            }
            description.push_str("Output interpretation guidance: ");
            description.push_str(guidance);
        }
    }
    description
}

fn permission_reason(descriptor: &agent_tools::ToolDescriptor) -> String {
    let mut reasons = Vec::new();
    if descriptor.permissions.shell {
        reasons.push("shell");
    }
    if descriptor.permissions.file_read {
        reasons.push("file_read");
    }
    if descriptor.permissions.file_write {
        reasons.push("file_write");
    }
    if descriptor.permissions.network {
        reasons.push("network");
    }
    if descriptor.permissions.secrets {
        reasons.push("secrets");
    }
    if descriptor.permissions.wallet {
        reasons.push("wallet");
    }
    if descriptor.permissions.payment {
        reasons.push("payment");
    }
    if descriptor.permissions.browser_profile {
        reasons.push("browser_profile");
    }
    if reasons.is_empty() {
        "tool marked approval-required".into()
    } else {
        format!("sensitive permissions: {}", reasons.join(","))
    }
}

fn tool_requires_approval(descriptor: &agent_tools::ToolDescriptor) -> bool {
    descriptor.requires_approval
        || descriptor.permissions.shell
        || descriptor.permissions.file_read
        || descriptor.permissions.file_write
        || descriptor.permissions.network
        || descriptor.permissions.secrets
        || descriptor.permissions.wallet
        || descriptor.permissions.payment
        || descriptor.permissions.browser_profile
}

fn contains_secret_marker(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, value)| is_secret_key(key) || contains_secret_marker(value)),
        Value::Array(items) => items.iter().any(contains_secret_marker),
        _ => false,
    }
}

fn redact_secret_markers(value: &Value, secret_scoped: bool) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let value = if is_secret_key(key) {
                        redacted_value()
                    } else {
                        redact_secret_markers(value, false)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| redact_secret_markers(item, false))
                .collect(),
        ),
        Value::String(_) if secret_scoped => redacted_value(),
        other => other.clone(),
    }
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("secret")
        || key.contains("authorization")
        || key.contains("token")
        || key.contains("api_key")
        || key.contains("apikey")
        || key.contains("password")
        || key.contains("credential")
        || key.contains("private_key")
}

fn text_contains_secret_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("private_key")
        || lower.contains("password:")
        || lower.contains("password=")
        || lower.contains("credential:")
        || lower.contains("credential=")
        || lower.contains("secret:")
        || lower.contains("secret=")
        || lower.contains("token:")
        || lower.contains("token=")
        || lower.contains("bearer ")
        || lower.contains("sk-")
}

fn redacted_value() -> Value {
    Value::String("[REDACTED]".into())
}

fn execute_hook_handler(
    handler: &RunHookHandler,
    hook_id: &str,
    trigger: HookTrigger,
    payload: &Value,
    payload_digest: &str,
    capture_stdout: bool,
) -> Result<Option<String>, String> {
    let command = handler.command.trim();
    if command.is_empty() {
        return Err("empty hook command".into());
    }
    if is_shell_interpreter(command) {
        return Err(format!("shell interpreter is not allowed: {command}"));
    }
    let stdin_payload = serde_json::to_vec(&json!({
        "hook_id": hook_id,
        "trigger": trigger.as_str(),
        "payload_digest": payload_digest,
        "payload": payload,
    }))
    .map_err(|err| err.to_string())?;
    let stdout_path = capture_stdout.then(hook_stdout_path);
    let stdout = match stdout_path.as_ref() {
        Some(path) => Stdio::from(File::create(path).map_err(|err| err.to_string())?),
        None => Stdio::null(),
    };
    let mut child = Command::new(command)
        .args(&handler.args)
        .stdin(Stdio::piped())
        .stdout(stdout)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| {
            if let Some(path) = stdout_path.as_ref() {
                let _ = remove_file(path);
            }
            err.to_string()
        })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(&stdin_payload)
            .map_err(|err| err.to_string())?;
    }
    drop(child.stdin.take());

    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            return if status.success() {
                read_hook_stdout(stdout_path.as_deref())
            } else {
                if let Some(path) = stdout_path.as_ref() {
                    let _ = remove_file(path);
                }
                Err(format!("handler exited with {status}"))
            };
        }
        if started.elapsed() > handler.bounded_timeout() {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(path) = stdout_path.as_ref() {
                let _ = remove_file(path);
            }
            return Err(format!(
                "handler timed out after {}ms",
                handler.bounded_timeout().as_millis()
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn hook_stdout_path() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = HOOK_STDOUT_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
    std::env::temp_dir().join(format!(
        "agent-hook-{}-{nanos}-{counter}.stdout",
        std::process::id()
    ))
}

fn read_hook_stdout(path: Option<&std::path::Path>) -> Result<Option<String>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let len = metadata(path).map_err(|err| err.to_string())?.len();
    if len > MAX_HOOK_STDOUT_BYTES {
        let _ = remove_file(path);
        return Err(format!(
            "handler stdout exceeded {} bytes",
            MAX_HOOK_STDOUT_BYTES
        ));
    }
    let output = read_to_string(path).map_err(|err| err.to_string())?;
    let _ = remove_file(path);
    Ok(Some(output))
}

fn parse_hook_context_fragments(
    hook_id: &str,
    output: &str,
) -> Result<Vec<MemoryFragment>, String> {
    let output = output.trim();
    if output.is_empty() {
        return Ok(Vec::new());
    }
    let parsed: HookContextOutput = serde_json::from_str(output).map_err(|err| err.to_string())?;
    if parsed.context_fragments.len() > MAX_HOOK_CONTEXT_FRAGMENTS {
        return Err(format!(
            "too many context fragments: max {}",
            MAX_HOOK_CONTEXT_FRAGMENTS
        ));
    }
    parsed
        .context_fragments
        .into_iter()
        .enumerate()
        .map(|(index, fragment)| {
            let content = fragment.content.trim().to_string();
            if content.is_empty() {
                return Err("context fragment content is empty".into());
            }
            if content.chars().count() > MAX_HOOK_CONTEXT_FRAGMENT_CHARS {
                return Err(format!(
                    "context fragment content exceeds {} characters",
                    MAX_HOOK_CONTEXT_FRAGMENT_CHARS
                ));
            }
            if text_contains_secret_marker(&content) {
                return Err("context fragment content matched secret guardrail".into());
            }
            let id = fragment
                .id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("{hook_id}-{index}"));
            if !valid_hook_context_fragment_id(&id) {
                return Err(format!("invalid context fragment id: {id}"));
            }
            let provenance = fragment
                .provenance
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("hook:{hook_id}"));
            Ok(MemoryFragment {
                id,
                content,
                provenance,
            })
        })
        .collect()
}

fn parse_hook_tool_call(hook_id: &str, output: &str) -> Result<HookToolCallDecision, String> {
    let output = output.trim();
    if output.is_empty() {
        return Ok(HookToolCallDecision {
            input: None,
            denial_reason: None,
        });
    }
    let parsed: HookToolCallOutput = serde_json::from_str(output).map_err(|err| err.to_string())?;
    let denial_reason = if parsed.denied || parsed.allow == Some(false) {
        let reason = parsed
            .reason
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("hook {hook_id} denied tool call"));
        if reason.chars().count() > 512 {
            return Err(format!(
                "tool call denial reason from hook {hook_id} is too long"
            ));
        }
        if text_contains_secret_marker(&reason) {
            return Err(format!(
                "tool call denial reason from hook {hook_id} matched secret guardrail"
            ));
        }
        Some(reason)
    } else {
        None
    };
    if let Some(input) = parsed.input.as_ref() {
        let serialized = serde_json::to_string(input).map_err(|err| err.to_string())?;
        if serialized.chars().count() > MAX_HOOK_TOOL_INPUT_CHARS {
            return Err(format!(
                "tool input from hook {hook_id} exceeds {} characters",
                MAX_HOOK_TOOL_INPUT_CHARS
            ));
        }
        if contains_secret_marker(input) || text_contains_secret_marker(&serialized) {
            return Err(format!(
                "tool input from hook {hook_id} matched secret guardrail"
            ));
        }
    }
    Ok(HookToolCallDecision {
        input: parsed.input,
        denial_reason,
    })
}

fn parse_hook_tool_output(hook_id: &str, output: &str) -> Result<Option<Value>, String> {
    let output = output.trim();
    if output.is_empty() {
        return Ok(None);
    }
    let parsed: HookToolOutput = serde_json::from_str(output).map_err(|err| err.to_string())?;
    let Some(output) = parsed.output else {
        return Ok(None);
    };
    let serialized = serde_json::to_string(&output).map_err(|err| err.to_string())?;
    if serialized.chars().count() > MAX_HOOK_TOOL_OUTPUT_CHARS {
        return Err(format!(
            "tool output from hook {hook_id} exceeds {} characters",
            MAX_HOOK_TOOL_OUTPUT_CHARS
        ));
    }
    if contains_secret_marker(&output) || text_contains_secret_marker(&serialized) {
        return Err(format!(
            "tool output from hook {hook_id} matched secret guardrail"
        ));
    }
    Ok(Some(output))
}

fn valid_hook_context_fragment_id(id: &str) -> bool {
    id.len() <= 80
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
}

fn is_shell_interpreter(command: &str) -> bool {
    let Some(name) = command.rsplit(['/', '\\']).next() else {
        return false;
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "cmd"
            | "cmd.exe"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
    )
}

fn refinement_instructions(refinement: &PromptRefinement) -> String {
    let custom = refinement.instructions.trim();
    if custom.is_empty() {
        "Rewrite the user's prompt into a clearer instruction for the target agent. Preserve intent, constraints, and relevant details. Return only the rewritten prompt.".into()
    } else {
        custom.to_string()
    }
}

struct ContextBuilder<'a> {
    agent: &'a AgentConfig,
    tools: &'a ToolRegistry,
    conversation: Vec<Message>,
    calls_used: u32,
    required_tool: Option<&'a ToolId>,
    hook_context_fragments: Vec<MemoryFragment>,
}

impl ContextBuilder<'_> {
    fn build(self) -> ContextSnapshot {
        let max_tool_calls = self.agent.tool_policy.max_calls;
        let remaining_tool_calls = max_tool_calls.saturating_sub(self.calls_used);
        let original_conversation = self.conversation.clone();
        let original_conversation_tokens =
            original_conversation.iter().fold(0_u32, |total, message| {
                total.saturating_add(estimate_message_tokens(message))
            });
        let loaded_memory: Vec<MemoryFragment> = self
            .agent
            .memory_fragments
            .iter()
            .chain(self.hook_context_fragments.iter())
            .filter(|fragment| !text_contains_secret_marker(&fragment.content))
            .cloned()
            .collect();
        let withheld_memory_count = self
            .agent
            .memory_fragments
            .len()
            .saturating_add(self.hook_context_fragments.len())
            .saturating_sub(loaded_memory.len());
        let loaded_artifacts: Vec<IngestedArtifactView> = self
            .agent
            .ingestion_artifacts
            .iter()
            .filter(|artifact| !text_contains_secret_marker(&artifact.content))
            .cloned()
            .collect();
        let withheld_artifact_count = self
            .agent
            .ingestion_artifacts
            .len()
            .saturating_sub(loaded_artifacts.len());
        let raw_manual_compacted = self
            .agent
            .compacted_context
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let manual_compacted = raw_manual_compacted
            .filter(|text| !text_contains_secret_marker(text))
            .map(str::to_string);
        let manual_compacted_withheld =
            raw_manual_compacted.is_some() && manual_compacted.is_none();
        let (conversation, auto_compacted, auto_compacted_applied) = if manual_compacted.is_none() {
            auto_compact_conversation(self.conversation, &self.agent.context_policy.compaction)
        } else {
            (self.conversation, None, false)
        };
        let auto_compacted = auto_compacted.filter(|text| !text_contains_secret_marker(text));
        let auto_compacted_applied = auto_compacted_applied && auto_compacted.is_some();
        let compacted = manual_compacted.or(auto_compacted);
        let compaction_review = compacted.as_ref().map(|compacted_context| {
            build_compaction_review(
                if auto_compacted_applied {
                    CompactionReviewMode::Auto
                } else {
                    CompactionReviewMode::Manual
                },
                &original_conversation,
                &conversation,
                compacted_context,
                original_conversation_tokens,
            )
        });
        let mut visible_tools: Vec<ToolView> = self
            .tools
            .descriptors()
            .filter(|d| self.agent.tool_policy.allows_descriptor(d))
            .map(|d| {
                let output_mode = self.agent.tool_policy.output_mode_for(&d.id);
                let visibility = self.agent.tool_policy.visibility_for(&d.id);
                ToolView {
                    id: d.id.0.clone(),
                    name: d.name.clone(),
                    description: match visibility {
                        VisibilityLevel::FullSchema | VisibilityLevel::NameAndDescription => {
                            Some(d.description.clone())
                        }
                        VisibilityLevel::NameOnly => None,
                    },
                    categories: d.categories.clone(),
                    input_schema: match visibility {
                        VisibilityLevel::FullSchema => Some(d.input_schema.clone()),
                        VisibilityLevel::NameAndDescription | VisibilityLevel::NameOnly => None,
                    },
                    output_mode,
                    output_interpretation_guidance: self
                        .agent
                        .tool_policy
                        .output_interpretation_guidance_for(
                            &d.id,
                            d.output_interpretation_guidance.as_deref(),
                        ),
                    visibility,
                    provenance: d.provenance.clone(),
                }
            })
            .collect();
        visible_tools.sort_by(|a, b| a.id.cmp(&b.id));
        let visible_skills: Vec<SkillView> = self
            .agent
            .skill_views
            .iter()
            .filter(|skill| {
                self.agent.allowed_skill_categories.is_empty()
                    || skill
                        .categories
                        .iter()
                        .any(|category| self.agent.allowed_skill_categories.contains(category))
            })
            .map(|skill| {
                let mut skill = skill.clone();
                let visibility = self.agent.skill_visibility_for(&skill);
                apply_skill_visibility(&mut skill, visibility);
                skill
            })
            .collect();

        let mut system_prompt = format!(
            "{}\n\nRuntime limits:\n- tool calls remaining: {remaining_tool_calls}/{max_tool_calls}",
            self.agent.system_prompt
        );
        if remaining_tool_calls == 0 {
            system_prompt.push_str(
                "\n- tool-call budget exhausted: do not call tools; answer from available context or ask the user to raise the budget",
            );
        } else if remaining_tool_calls == 1 {
            system_prompt.push_str(
                "\n- tool-call budget warning: one tool call remains; choose the next call carefully or answer directly",
            );
        }
        if let Some(required_tool) = self.required_tool {
            system_prompt.push_str(&format!(
                "\n- required tool call: call `{}` exactly once before answering",
                required_tool.0
            ));
        }
        let raw_tool_ids: Vec<&str> = visible_tools
            .iter()
            .filter(|tool| tool.output_mode == ToolOutputMode::Raw)
            .map(|tool| tool.id.as_str())
            .collect();
        if self.agent.tool_policy.output_mode == ToolOutputMode::Raw
            && raw_tool_ids.len() == visible_tools.len()
        {
            system_prompt.push_str(
                "\n- tool output mode: raw; after a tool call, the runtime returns the tool output without an interpretation pass",
            );
        } else if !raw_tool_ids.is_empty() {
            system_prompt.push_str(&format!(
                "\n- raw tool output mode applies to: {}; after any of these tool calls, the runtime returns the tool output without an interpretation pass",
                raw_tool_ids.join(", ")
            ));
        }
        let guidance: Vec<&ToolView> = visible_tools
            .iter()
            .filter(|tool| {
                tool.output_interpretation_guidance
                    .as_deref()
                    .is_some_and(|text| !text.trim().is_empty())
            })
            .collect();
        if !guidance.is_empty() {
            system_prompt.push_str("\n\n<tool-output-guidance>");
            for tool in guidance {
                if let Some(text) = &tool.output_interpretation_guidance {
                    system_prompt.push_str(&format!(
                        "\n<tool id=\"{}\">\n{}\n</tool>",
                        tool.id,
                        text.trim()
                    ));
                }
            }
            system_prompt.push_str("\n</tool-output-guidance>");
        }
        system_prompt.push_str(
            "\n- side-effect-free, approval-free tool calls proposed in the same turn may execute in parallel",
        );
        if !loaded_memory.is_empty() {
            system_prompt.push_str("\n\n<memory-context>");
            for fragment in &loaded_memory {
                system_prompt.push_str(&format!(
                    "\n<memory id=\"{}\" provenance=\"{}\">\n{}\n</memory>",
                    fragment.id, fragment.provenance, fragment.content
                ));
            }
            system_prompt.push_str("\n</memory-context>");
        }
        if let Some(compacted_context) = &compacted {
            system_prompt.push_str(&format!(
                "\n\n<compacted-context>\n{}\n</compacted-context>",
                compacted_context
            ));
        }
        if !loaded_artifacts.is_empty() {
            system_prompt.push_str("\n\n<ingestion-context>");
            for artifact in &loaded_artifacts {
                system_prompt.push_str(&format!(
                    "\n<artifact id=\"{}\" source=\"{}\" sections=\"{}\" provenance=\"{}\">",
                    artifact.id, artifact.source, artifact.sections, artifact.provenance
                ));
                if !artifact.findings.is_empty() {
                    system_prompt.push_str("\n<findings>");
                    for finding in &artifact.findings {
                        system_prompt.push_str(&format!("\n- {finding}"));
                    }
                    system_prompt.push_str("\n</findings>");
                }
                if artifact.content.trim().is_empty() {
                    system_prompt.push_str("\n[artifact content withheld]");
                } else {
                    system_prompt.push_str(&format!("\n{}", artifact.content));
                }
                system_prompt.push_str("\n</artifact>");
            }
            system_prompt.push_str("\n</ingestion-context>");
        }
        if !visible_skills.is_empty() {
            system_prompt.push_str("\n\n<skill-context>");
            for skill in &visible_skills {
                let content = skill_visible_content(skill);
                let provenance = skill
                    .provenance
                    .as_deref()
                    .map(|value| format!(" provenance=\"{}\"", value))
                    .unwrap_or_default();
                system_prompt.push_str(&format!(
                    "\n<skill id=\"{}\" visibility=\"{:?}\"{}>\n{}\n</skill>",
                    skill.id, skill.visibility, provenance, content
                ));
            }
            system_prompt.push_str("\n</skill-context>");
        }

        let estimated_input_tokens = estimate_context_tokens(
            &system_prompt,
            &conversation,
            &loaded_memory,
            &loaded_artifacts,
            &visible_tools,
            &visible_skills,
        );
        let mut provenance = vec![
            ProvenanceRecord {
                fragment: "system_prompt".into(),
                source: format!("agent:{}", self.agent.id),
            },
            ProvenanceRecord {
                fragment: "runtime_limits".into(),
                source: "run".into(),
            },
            ProvenanceRecord {
                fragment: "visible_tools".into(),
                source: "tool_registry + agent.tool_policy".into(),
            },
            ProvenanceRecord {
                fragment: "loaded_memory".into(),
                source: "agent.memory_policy".into(),
            },
            ProvenanceRecord {
                fragment: "loaded_artifacts".into(),
                source: "agent.ingestion_policy".into(),
            },
            ProvenanceRecord {
                fragment: "visible_skills".into(),
                source: "agent.skill_policy".into(),
            },
        ];
        if conversation.iter().any(|message| {
            matches!(
                message,
                Message::System { content } if content.starts_with(MID_RUN_GUIDANCE_PREFIX)
            )
        }) {
            provenance.push(ProvenanceRecord {
                fragment: "mid_run_guidance".into(),
                source: "run.trace.GuidanceInjected".into(),
            });
        }
        if !self.agent.conversation_history.is_empty() {
            provenance.push(ProvenanceRecord {
                fragment: "conversation_history".into(),
                source: "profile.main.conversations".into(),
            });
        }
        if !self.hook_context_fragments.is_empty() {
            provenance.push(ProvenanceRecord {
                fragment: "hook_context".into(),
                source: "run.lifecycle.before_context_built".into(),
            });
        }
        if compacted.is_some() {
            provenance.push(ProvenanceRecord {
                fragment: "compacted_context".into(),
                source: if auto_compacted_applied {
                    "agent.context_policy.auto_compaction".into()
                } else {
                    "run.manual_compaction".into()
                },
            });
        }
        if withheld_memory_count > 0 {
            provenance.push(ProvenanceRecord {
                fragment: "withheld_memory".into(),
                source: "secret-pattern guardrail".into(),
            });
        }
        if withheld_artifact_count > 0 {
            provenance.push(ProvenanceRecord {
                fragment: "withheld_artifacts".into(),
                source: "secret-pattern guardrail".into(),
            });
        }
        if manual_compacted_withheld {
            provenance.push(ProvenanceRecord {
                fragment: "withheld_compacted_context".into(),
                source: "secret-pattern guardrail".into(),
            });
        }

        ContextSnapshot {
            system_prompt,
            conversation,
            compacted,
            compaction_review,
            loaded_memory,
            loaded_artifacts,
            visible_tools,
            visible_skills,
            limits: RuntimeLimits {
                max_tool_calls,
                remaining_tool_calls,
            },
            estimated_input_tokens,
            provenance,
        }
    }
}

fn estimate_context_tokens(
    system_prompt: &str,
    conversation: &[Message],
    loaded_memory: &[MemoryFragment],
    loaded_artifacts: &[IngestedArtifactView],
    visible_tools: &[ToolView],
    visible_skills: &[SkillView],
) -> u32 {
    let mut total = estimate_text_tokens(system_prompt) as u64;
    for message in conversation {
        total += estimate_message_tokens(message) as u64;
    }
    for fragment in loaded_memory {
        total += estimate_text_tokens(&fragment.content) as u64;
    }
    for artifact in loaded_artifacts {
        total += estimate_text_tokens(&artifact.content) as u64;
    }
    for tool in visible_tools {
        total += estimate_text_tokens(&tool.id) as u64;
        total += estimate_text_tokens(&tool.name) as u64;
        if let Some(description) = &tool.description {
            total += estimate_text_tokens(description) as u64;
        }
        if let Some(schema) = &tool.input_schema {
            total += estimate_text_tokens(&schema.to_string()) as u64;
        }
        if let Some(guidance) = &tool.output_interpretation_guidance {
            total += estimate_text_tokens(guidance) as u64;
        }
        if let Some(provenance) = &tool.provenance {
            total += estimate_text_tokens(provenance) as u64;
        }
    }
    for skill in visible_skills {
        total += u64::from(skill.estimated_tokens);
        total += estimate_text_tokens(&skill.id) as u64;
        total += estimate_text_tokens(&skill.name) as u64;
        if let Some(description) = &skill.description {
            total += estimate_text_tokens(description) as u64;
        }
        if let Some(provenance) = &skill.provenance {
            total += estimate_text_tokens(provenance) as u64;
        }
    }
    total.min(u64::from(u32::MAX)) as u32
}

fn auto_compact_conversation(
    conversation: Vec<Message>,
    policy: &ContextCompactionPolicy,
) -> (Vec<Message>, Option<String>, bool) {
    let Some(threshold) = policy
        .max_tokens_before_compaction
        .filter(|value| *value > 0)
    else {
        return (conversation, None, false);
    };
    let conversation_tokens = conversation.iter().fold(0_u32, |total, message| {
        total.saturating_add(estimate_message_tokens(message))
    });
    if conversation_tokens <= threshold {
        return (conversation, None, false);
    }
    let Some((latest, history)) = conversation.split_last() else {
        return (conversation, None, false);
    };
    if history.is_empty()
        || !matches!(
            latest,
            Message::User { .. } | Message::UserWithAttachments { .. }
        )
    {
        return (conversation, None, false);
    }
    let history_lines = history
        .iter()
        .map(message_compaction_line)
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if history_lines.is_empty() {
        return (conversation, None, false);
    }
    let max_output_tokens = policy
        .max_output_tokens
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_AUTO_COMPACTION_OUTPUT_TOKENS);
    let compacted = auto_compact_text(
        &history_lines,
        policy.guidance.as_deref(),
        max_output_tokens,
        conversation_tokens,
        threshold,
    );
    (vec![latest.clone()], Some(compacted), true)
}

fn build_compaction_review(
    mode: CompactionReviewMode,
    before: &[Message],
    visible: &[Message],
    compacted_context: &str,
    before_tokens: u32,
) -> CompactionReview {
    let (before_messages, withheld_before_messages) = compaction_review_lines(before);
    let (visible_messages, _) = compaction_review_lines(visible);
    let after_tokens = estimate_text_tokens(compacted_context).saturating_add(
        visible.iter().fold(0_u32, |total, message| {
            total.saturating_add(estimate_message_tokens(message))
        }),
    );
    CompactionReview {
        mode,
        before_messages,
        compacted_context: compacted_context.to_string(),
        visible_messages,
        before_tokens,
        after_tokens,
        withheld_before_messages,
    }
}

fn compaction_review_lines(messages: &[Message]) -> (Vec<String>, usize) {
    let mut withheld = 0;
    let lines = messages
        .iter()
        .filter_map(|message| {
            let line = message_compaction_line(message);
            if line.trim().is_empty() {
                None
            } else if text_contains_secret_marker(&line) {
                withheld += 1;
                None
            } else {
                Some(line)
            }
        })
        .collect();
    (lines, withheld)
}

fn message_compaction_line(message: &Message) -> String {
    match message {
        Message::System { content } => format!("System: {}", normalize_inline(content)),
        Message::User { content } => format!("User: {}", normalize_inline(content)),
        Message::UserWithAttachments {
            content,
            attachments,
        } => format!(
            "User: {} [{} attachment(s)]",
            normalize_inline(content),
            attachments.len()
        ),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let content = content
                .as_deref()
                .map(normalize_inline)
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "[no text]".into());
            if tool_calls.is_empty() {
                format!("Assistant: {content}")
            } else {
                let tools = tool_calls
                    .iter()
                    .map(|call| call.tool_name.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("Assistant: {content} [tool_calls: {tools}]")
            }
        }
        Message::ToolResult { content, .. } => {
            format!("Tool result: {}", normalize_inline(content))
        }
    }
}

fn auto_compact_text(
    lines: &[String],
    guidance: Option<&str>,
    max_output_tokens: u32,
    original_tokens: u32,
    threshold: u32,
) -> String {
    let guidance = guidance.map(str::trim).filter(|text| !text.is_empty());
    let header = if let Some(guidance) = guidance {
        format!(
            "<auto-compaction trigger=\"max_tokens_before_compaction\" original_tokens=\"{original_tokens}\" threshold=\"{threshold}\" max_output_tokens=\"{max_output_tokens}\">\nGuidance: {guidance}\nSummary:\n"
        )
    } else {
        format!(
            "<auto-compaction trigger=\"max_tokens_before_compaction\" original_tokens=\"{original_tokens}\" threshold=\"{threshold}\" max_output_tokens=\"{max_output_tokens}\">\nSummary:\n"
        )
    };
    let footer = "\n</auto-compaction>";
    let overhead = estimate_text_tokens(&header).saturating_add(estimate_text_tokens(footer));
    let body_budget = max_output_tokens.saturating_sub(overhead).max(1);
    let body = compact_lines_to_token_estimate(lines, body_budget);
    format!("{header}{body}{footer}")
}

fn compact_lines_to_token_estimate(lines: &[String], max_tokens: u32) -> String {
    let mut out = String::new();
    for line in lines {
        let candidate = format!("- {}\n", normalize_inline(line));
        if estimate_text_tokens(&out).saturating_add(estimate_text_tokens(&candidate)) > max_tokens
        {
            if out.is_empty() {
                let remaining = max_tokens.saturating_sub(estimate_text_tokens("- "));
                out.push_str("- ");
                out.push_str(&truncate_to_token_estimate(line, remaining));
                out.push('\n');
            }
            break;
        }
        out.push_str(&candidate);
    }
    out.trim_end().to_string()
}

fn normalize_inline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_to_token_estimate(text: &str, max_tokens: u32) -> String {
    let max_chars = (max_tokens as usize).saturating_mul(4).max(1);
    let normalized = normalize_inline(text);
    let mut out = String::new();
    for ch in normalized.chars().take(max_chars) {
        out.push(ch);
    }
    if normalized.chars().count() > out.chars().count() {
        out.push_str("...");
    }
    out
}

fn skill_visible_content(skill: &SkillView) -> &str {
    match skill.visibility {
        VisibilityLevel::FullSchema => skill
            .body
            .as_deref()
            .or(skill.description.as_deref())
            .unwrap_or(""),
        VisibilityLevel::NameAndDescription => skill.description.as_deref().unwrap_or(""),
        VisibilityLevel::NameOnly => "",
    }
}

fn estimate_message_tokens(message: &Message) -> u32 {
    match message {
        Message::System { content } | Message::User { content } => estimate_text_tokens(content),
        Message::UserWithAttachments {
            content,
            attachments,
        } => estimate_text_tokens(content).saturating_add((attachments.len() as u32) * 256),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            let content_tokens = content.as_deref().map(estimate_text_tokens).unwrap_or(0);
            let tool_tokens = tool_calls
                .iter()
                .map(|call| {
                    estimate_text_tokens(&call.tool_name)
                        + estimate_text_tokens(&call.input.to_string())
                })
                .sum::<u32>();
            content_tokens.saturating_add(tool_tokens)
        }
        Message::ToolResult { content, .. } => estimate_text_tokens(content),
    }
}

fn estimate_text_tokens(text: &str) -> u32 {
    if text.trim().is_empty() {
        return 0;
    }
    let chars = text.chars().count();
    let char_estimate = chars.div_ceil(4);
    let word_floor = text.split_whitespace().count();
    char_estimate.max(word_floor).min(u32::MAX as usize) as u32
}

impl Harness {
    fn run_scoped<'a>(
        &'a self,
        agent: &'a AgentConfig,
        input: UserInput,
        scope: RunScope,
    ) -> Pin<Box<dyn Future<Output = Result<RunExecution, HarnessError>> + Send + 'a>> {
        Box::pin(async move {
            let run_id = scope.run_id;
            let started_at = Instant::now();

            let run_started =
                self.record_run_started(run_id, scope.parent_event, &agent.id, &input.text);

            let mut calls_used: u32 = 0;
            let mut total_cost_usd = 0.0;
            let mut has_cost_usd = false;
            let (refined_input, refinement_cost_usd) = self
                .refine_input_for_run(agent, run_id, run_started.id, &input.text)
                .await?;
            if let Some(cost) = refinement_cost_usd {
                total_cost_usd += cost;
                has_cost_usd = true;
            }
            let mut conversation = self.initial_conversation(
                agent,
                &UserInput {
                    text: refined_input,
                },
            );
            let mut consumed_guidance_events = HashSet::new();
            let mut pending_interpretation_model: Option<ModelRef> = None;
            let mut required_tool_pending = agent.tool_policy.required_tool.clone();

            loop {
                self.check_cancelled(run_id)?;
                let hook_context_fragments = self.before_context_built_fragments(
                    run_id,
                    run_started.id,
                    &conversation,
                    calls_used,
                    required_tool_pending.as_ref(),
                );
                let snapshot = self.build_context_snapshot_with_required_tool_and_hook_context(
                    agent,
                    conversation.clone(),
                    calls_used,
                    required_tool_pending.as_ref(),
                    hook_context_fragments,
                );
                let context_built = self.record_context_built(run_id, run_started.id, &snapshot);
                self.record_context_references(agent, run_id, context_built.id, &snapshot);
                let mut req = self.llm_request_from_snapshot(agent, &snapshot);
                if let Some(model) = pending_interpretation_model.take() {
                    req.model = model;
                }
                let request_model = req.model.clone();
                let request_digest = self.llm_request_digest(&req);

                let llm_started = self.events.append(
                    run_id,
                    Some(context_built.id),
                    RunEventKind::LlmRequestStarted {
                        model: request_model.0.clone(),
                        request_digest: Some(request_digest),
                    },
                );

                let (response, llm_duration) = self
                    .complete_llm_with_trace(req, run_id, llm_started.id)
                    .await?;
                let llm_cost_usd =
                    self.llm_cost_usd(agent, response.tokens_in, response.tokens_out);
                if let Some(cost) = llm_cost_usd {
                    total_cost_usd += cost;
                    has_cost_usd = true;
                }

                let llm_completed = self.events.append(
                    run_id,
                    Some(llm_started.id),
                    RunEventKind::LlmRequestCompleted {
                        tokens_in: response.tokens_in,
                        tokens_out: response.tokens_out,
                        cost_usd: llm_cost_usd,
                        duration_ms: llm_duration,
                    },
                );

                self.check_cancelled(run_id)?;
                if let Some(required_tool) = required_tool_pending.as_ref() {
                    let violation = if response.tool_calls.is_empty() {
                        Some(format!("required tool {} was not called", required_tool.0))
                    } else if response.tool_calls.len() != 1
                        || response.tool_calls[0].tool_name != required_tool.0
                    {
                        Some(format!(
                            "required tool {} must be the only tool call in this turn",
                            required_tool.0
                        ))
                    } else {
                        None
                    };
                    if let Some(reason) = violation {
                        self.record_policy_denied(run_id, llm_completed.id, &reason);
                        self.record_run_failed(run_id, run_started.id, &reason);
                        return Err(HarnessError::PolicyDenied(reason));
                    }
                }

                if response.tool_calls.is_empty() {
                    let final_output = response.content.unwrap_or_default();
                    conversation.push(Message::Assistant {
                        content: Some(final_output.clone()),
                        tool_calls: vec![],
                    });

                    let total_cost_usd = has_cost_usd.then_some(total_cost_usd);
                    self.record_run_completed(
                        run_id,
                        run_started.id,
                        &final_output,
                        total_cost_usd,
                        started_at.elapsed().as_millis() as u64,
                    );
                    return Ok(RunExecution {
                        run_id,
                        final_output,
                        total_cost_usd,
                    });
                }

                conversation.push(Message::Assistant {
                    content: response.content.clone(),
                    tool_calls: response.tool_calls.clone(),
                });

                let mut prepared_calls = Vec::new();
                for mut tc in response.tool_calls {
                    let tool_id = ToolId::from(tc.tool_name.clone());

                    let proposed = self.record_tool_proposed(
                        run_id,
                        run_started.id,
                        &tc.id,
                        &tc.tool_name,
                        self.redacted_tool_payload(&tool_id, &tc.input),
                        Some(request_model.0.clone()),
                        self.tool_permissions_audit(&tool_id),
                    );

                    if let Err(err) = self.ensure_tool_allowed(agent, &tool_id) {
                        let reason = err.to_string();
                        self.record_policy_denied(run_id, proposed.id, &reason);
                        self.events.append(
                            run_id,
                            Some(proposed.id),
                            RunEventKind::ToolCallFailed {
                                call_id: tc.id.clone(),
                                error: reason.clone(),
                            },
                        );
                        self.record_run_failed(run_id, run_started.id, &reason);
                        return Err(err);
                    }

                    if calls_used + prepared_calls.len() as u32 >= agent.tool_policy.max_calls {
                        let reason = format!(
                            "tool-call budget exhausted (used={}, limit={})",
                            calls_used + prepared_calls.len() as u32,
                            agent.tool_policy.max_calls,
                        );
                        self.record_policy_denied(run_id, proposed.id, &reason);
                        self.events.append(
                            run_id,
                            Some(proposed.id),
                            RunEventKind::ToolCallFailed {
                                call_id: tc.id.clone(),
                                error: reason.clone(),
                            },
                        );
                        self.record_run_failed(run_id, run_started.id, &reason);
                        return Err(HarnessError::BudgetExhausted(reason));
                    }

                    match self.before_tool_call(
                        run_id,
                        proposed.id,
                        &tool_id,
                        &tc.id,
                        tc.input.clone(),
                        Some(&request_model.0),
                    ) {
                        Ok(input) => tc.input = input,
                        Err(err) => {
                            let reason = err.to_string();
                            self.events.append(
                                run_id,
                                Some(proposed.id),
                                RunEventKind::ToolCallFailed {
                                    call_id: tc.id.clone(),
                                    error: reason.clone(),
                                },
                            );
                            self.record_run_failed(run_id, run_started.id, &reason);
                            return Err(err);
                        }
                    }

                    if let Err(e) =
                        self.record_approval_gate(agent, run_id, proposed.id, &tc.id, &tool_id)
                    {
                        let reason = e.to_string();
                        self.events.append(
                            run_id,
                            Some(proposed.id),
                            RunEventKind::ToolCallFailed {
                                call_id: tc.id.clone(),
                                error: reason.clone(),
                            },
                        );
                        self.events.append(
                            run_id,
                            Some(run_started.id),
                            RunEventKind::RunPaused { reason },
                        );
                        return Err(e);
                    }

                    let tool_started = self.events.append(
                        run_id,
                        Some(proposed.id),
                        RunEventKind::ToolCallStarted {
                            call_id: tc.id.clone(),
                        },
                    );

                    prepared_calls.push(PreparedToolCall {
                        call: tc,
                        tool_id,
                        proposed_event: proposed.id,
                        started_event: tool_started.id,
                    });
                }

                let has_raw_tool_output = prepared_calls.iter().any(|prepared| {
                    agent.tool_policy.output_mode_for(&prepared.tool_id) == ToolOutputMode::Raw
                });
                let can_parallelize = !has_raw_tool_output
                    && prepared_calls.len() > 1
                    && prepared_calls
                        .iter()
                        .all(|prepared| self.tool_is_parallel_safe(&prepared.tool_id));

                if can_parallelize {
                    let executions =
                        join_all(prepared_calls.into_iter().map(|prepared| {
                            self.execute_prepared_tool_call(agent, &scope, prepared)
                        }))
                        .await;
                    let mut first_error = None;
                    for execution in executions {
                        match execution {
                            Ok(executed) => {
                                let visible_output = self.redacted_tool_payload(
                                    &executed.prepared.tool_id,
                                    &executed.execution.output,
                                );
                                let visible_output = self.tool_output_ready(
                                    run_id,
                                    executed.prepared.proposed_event,
                                    &executed.prepared.tool_id,
                                    &executed.prepared.call.id,
                                    visible_output,
                                );
                                let completed = self.record_tool_completed(
                                    run_id,
                                    executed.prepared.proposed_event,
                                    &executed.prepared.call.id,
                                    visible_output.clone(),
                                    executed.execution.cost_usd,
                                    executed.duration_ms,
                                );
                                let interpretation_model =
                                    agent.tool_policy.output_interpretation_model_for(
                                        &executed.prepared.tool_id,
                                        &agent.model,
                                    );
                                Self::merge_pending_interpretation_model(
                                    &mut pending_interpretation_model,
                                    interpretation_model.clone(),
                                    agent,
                                );
                                self.record_tool_output_interpreted(
                                    &interpretation_model,
                                    run_id,
                                    completed.id,
                                    &executed.prepared.call.id,
                                    &visible_output,
                                );
                                if let Some(cost) = executed.execution.cost_usd {
                                    total_cost_usd += cost;
                                    has_cost_usd = true;
                                }
                                conversation.push(Message::ToolResult {
                                    tool_call_id: executed.prepared.call.id,
                                    content: Self::stringify_tool_output(&visible_output),
                                });
                                calls_used += 1;
                            }
                            Err((prepared, err)) => {
                                let reason = err.to_string();
                                self.events.append(
                                    run_id,
                                    Some(prepared.proposed_event),
                                    RunEventKind::ToolCallFailed {
                                        call_id: prepared.call.id,
                                        error: reason.clone(),
                                    },
                                );
                                if first_error.is_none() {
                                    first_error = Some(err);
                                }
                            }
                        }
                    }
                    if let Some(err) = first_error {
                        self.record_run_failed(run_id, run_started.id, &err.to_string());
                        return Err(err);
                    }
                } else {
                    for prepared in prepared_calls {
                        let executed = match self
                            .execute_prepared_tool_call(agent, &scope, prepared.clone())
                            .await
                        {
                            Ok(executed) => executed,
                            Err((prepared, err)) => {
                                let reason = err.to_string();
                                self.events.append(
                                    run_id,
                                    Some(prepared.proposed_event),
                                    RunEventKind::ToolCallFailed {
                                        call_id: prepared.call.id,
                                        error: reason.clone(),
                                    },
                                );
                                self.record_run_failed(run_id, run_started.id, &reason);
                                return Err(err);
                            }
                        };
                        if let Some(cost) = executed.execution.cost_usd {
                            total_cost_usd += cost;
                            has_cost_usd = true;
                        }
                        let output = executed.execution.output;
                        let visible_output =
                            self.redacted_tool_payload(&executed.prepared.tool_id, &output);
                        let visible_output = self.tool_output_ready(
                            run_id,
                            executed.prepared.proposed_event,
                            &executed.prepared.tool_id,
                            &executed.prepared.call.id,
                            visible_output,
                        );
                        let completed = self.record_tool_completed(
                            run_id,
                            executed.prepared.proposed_event,
                            &executed.prepared.call.id,
                            visible_output.clone(),
                            executed.execution.cost_usd,
                            executed.duration_ms,
                        );
                        conversation.push(Message::ToolResult {
                            tool_call_id: executed.prepared.call.id.clone(),
                            content: Self::stringify_tool_output(&visible_output),
                        });
                        calls_used += 1;
                        if agent
                            .tool_policy
                            .output_mode_for(&executed.prepared.tool_id)
                            == ToolOutputMode::Raw
                        {
                            let final_output = Self::stringify_tool_output(&visible_output);
                            let total_cost_usd = has_cost_usd.then_some(total_cost_usd);
                            self.record_run_completed(
                                run_id,
                                run_started.id,
                                &final_output,
                                total_cost_usd,
                                started_at.elapsed().as_millis() as u64,
                            );
                            return Ok(RunExecution {
                                run_id,
                                final_output,
                                total_cost_usd,
                            });
                        }
                        let interpretation_model =
                            agent.tool_policy.output_interpretation_model_for(
                                &executed.prepared.tool_id,
                                &agent.model,
                            );
                        Self::merge_pending_interpretation_model(
                            &mut pending_interpretation_model,
                            interpretation_model.clone(),
                            agent,
                        );
                        self.record_tool_output_interpreted(
                            &interpretation_model,
                            run_id,
                            completed.id,
                            &executed.prepared.call.id,
                            &visible_output,
                        );
                        if required_tool_pending
                            .as_ref()
                            .is_some_and(|required| required == &executed.prepared.tool_id)
                        {
                            required_tool_pending = None;
                        }
                    }
                }
                self.check_cancelled(run_id)?;
                self.append_pending_guidance(
                    run_id,
                    &mut conversation,
                    &mut consumed_guidance_events,
                );
            }
        })
    }
}

#[async_trait]
impl HarnessApi for Harness {
    async fn run(&self, agent: &AgentConfig, input: UserInput) -> Result<RunResult, HarnessError> {
        let scope = RunScope::root(&agent.id);
        let run_id = scope.run_id;
        let started_at = Instant::now();

        let run_started = self.record_run_started(run_id, None, &agent.id, &input.text);

        let mut calls_used: u32 = 0;
        let mut total_cost_usd = 0.0;
        let mut has_cost_usd = false;
        let (refined_input, refinement_cost_usd) = self
            .refine_input_for_run(agent, run_id, run_started.id, &input.text)
            .await?;
        if let Some(cost) = refinement_cost_usd {
            total_cost_usd += cost;
            has_cost_usd = true;
        }
        let mut conversation = self.initial_conversation(
            agent,
            &UserInput {
                text: refined_input,
            },
        );
        let mut consumed_guidance_events = HashSet::new();
        let mut pending_interpretation_model: Option<ModelRef> = None;
        let mut required_tool_pending = agent.tool_policy.required_tool.clone();

        loop {
            self.check_cancelled(run_id)?;
            let hook_context_fragments = self.before_context_built_fragments(
                run_id,
                run_started.id,
                &conversation,
                calls_used,
                required_tool_pending.as_ref(),
            );
            let snapshot = self.build_context_snapshot_with_required_tool_and_hook_context(
                agent,
                conversation.clone(),
                calls_used,
                required_tool_pending.as_ref(),
                hook_context_fragments,
            );
            let context_built = self.record_context_built(run_id, run_started.id, &snapshot);
            self.record_context_references(agent, run_id, context_built.id, &snapshot);
            let mut req = self.llm_request_from_snapshot(agent, &snapshot);
            if let Some(model) = pending_interpretation_model.take() {
                req.model = model;
            }
            let request_model = req.model.clone();
            let request_digest = self.llm_request_digest(&req);

            let llm_started = self.events.append(
                run_id,
                Some(context_built.id),
                RunEventKind::LlmRequestStarted {
                    model: request_model.0.clone(),
                    request_digest: Some(request_digest),
                },
            );

            let (response, llm_duration) = self
                .complete_llm_with_trace(req, run_id, llm_started.id)
                .await?;
            let llm_cost_usd = self.llm_cost_usd(agent, response.tokens_in, response.tokens_out);
            if let Some(cost) = llm_cost_usd {
                total_cost_usd += cost;
                has_cost_usd = true;
            }

            let llm_completed = self.events.append(
                run_id,
                Some(llm_started.id),
                RunEventKind::LlmRequestCompleted {
                    tokens_in: response.tokens_in,
                    tokens_out: response.tokens_out,
                    cost_usd: llm_cost_usd,
                    duration_ms: llm_duration,
                },
            );

            self.check_cancelled(run_id)?;
            if let Some(required_tool) = required_tool_pending.as_ref() {
                let violation = if response.tool_calls.is_empty() {
                    Some(format!("required tool {} was not called", required_tool.0))
                } else if response.tool_calls.len() != 1
                    || response.tool_calls[0].tool_name != required_tool.0
                {
                    Some(format!(
                        "required tool {} must be the only tool call in this turn",
                        required_tool.0
                    ))
                } else {
                    None
                };
                if let Some(reason) = violation {
                    self.record_policy_denied(run_id, llm_completed.id, &reason);
                    self.record_run_failed(run_id, run_started.id, &reason);
                    return Err(HarnessError::PolicyDenied(reason));
                }
            }

            // No tool calls -> final answer.
            if response.tool_calls.is_empty() {
                let final_output = response.content.unwrap_or_default();
                conversation.push(Message::Assistant {
                    content: Some(final_output.clone()),
                    tool_calls: vec![],
                });

                let total_duration_ms = started_at.elapsed().as_millis() as u64;
                self.record_run_completed(
                    run_id,
                    run_started.id,
                    &final_output,
                    has_cost_usd.then_some(total_cost_usd),
                    total_duration_ms,
                );
                return Ok(RunResult {
                    run_id,
                    final_output,
                });
            }

            // Record the assistant turn that proposed tool calls.
            conversation.push(Message::Assistant {
                content: response.content.clone(),
                tool_calls: response.tool_calls.clone(),
            });

            let mut prepared_calls = Vec::new();
            for mut tc in response.tool_calls {
                let tool_id = ToolId::from(tc.tool_name.clone());

                let proposed = self.record_tool_proposed(
                    run_id,
                    run_started.id,
                    &tc.id,
                    &tc.tool_name,
                    self.redacted_tool_payload(&tool_id, &tc.input),
                    Some(request_model.0.clone()),
                    self.tool_permissions_audit(&tool_id),
                );

                // Allowlist check.
                if let Err(err) = self.ensure_tool_allowed(agent, &tool_id) {
                    let reason = err.to_string();
                    self.record_policy_denied(run_id, proposed.id, &reason);
                    self.events.append(
                        run_id,
                        Some(proposed.id),
                        RunEventKind::ToolCallFailed {
                            call_id: tc.id.clone(),
                            error: reason.clone(),
                        },
                    );
                    self.record_run_failed(run_id, run_started.id, &reason);
                    return Err(err);
                }

                // Budget check (per-call, before execution).
                if calls_used + prepared_calls.len() as u32 >= agent.tool_policy.max_calls {
                    let reason = format!(
                        "tool-call budget exhausted (used={}, limit={})",
                        calls_used + prepared_calls.len() as u32,
                        agent.tool_policy.max_calls,
                    );
                    self.record_policy_denied(run_id, proposed.id, &reason);
                    self.events.append(
                        run_id,
                        Some(proposed.id),
                        RunEventKind::ToolCallFailed {
                            call_id: tc.id.clone(),
                            error: reason.clone(),
                        },
                    );
                    self.record_run_failed(run_id, run_started.id, &reason);
                    return Err(HarnessError::BudgetExhausted(reason));
                }

                match self.before_tool_call(
                    run_id,
                    proposed.id,
                    &tool_id,
                    &tc.id,
                    tc.input.clone(),
                    Some(&request_model.0),
                ) {
                    Ok(input) => tc.input = input,
                    Err(err) => {
                        let reason = err.to_string();
                        self.events.append(
                            run_id,
                            Some(proposed.id),
                            RunEventKind::ToolCallFailed {
                                call_id: tc.id.clone(),
                                error: reason.clone(),
                            },
                        );
                        self.record_run_failed(run_id, run_started.id, &reason);
                        return Err(err);
                    }
                }

                if let Err(e) =
                    self.record_approval_gate(agent, run_id, proposed.id, &tc.id, &tool_id)
                {
                    let reason = e.to_string();
                    self.events.append(
                        run_id,
                        Some(proposed.id),
                        RunEventKind::ToolCallFailed {
                            call_id: tc.id.clone(),
                            error: reason.clone(),
                        },
                    );
                    self.events.append(
                        run_id,
                        Some(run_started.id),
                        RunEventKind::RunPaused { reason },
                    );
                    return Err(e);
                }

                let tool_started = self.events.append(
                    run_id,
                    Some(proposed.id),
                    RunEventKind::ToolCallStarted {
                        call_id: tc.id.clone(),
                    },
                );

                prepared_calls.push(PreparedToolCall {
                    call: tc,
                    tool_id,
                    proposed_event: proposed.id,
                    started_event: tool_started.id,
                });
            }

            let has_raw_tool_output = prepared_calls.iter().any(|prepared| {
                agent.tool_policy.output_mode_for(&prepared.tool_id) == ToolOutputMode::Raw
            });
            let can_parallelize = !has_raw_tool_output
                && prepared_calls.len() > 1
                && prepared_calls
                    .iter()
                    .all(|prepared| self.tool_is_parallel_safe(&prepared.tool_id));

            if can_parallelize {
                let executions = join_all(
                    prepared_calls
                        .into_iter()
                        .map(|prepared| self.execute_prepared_tool_call(agent, &scope, prepared)),
                )
                .await;
                let mut first_error = None;
                for execution in executions {
                    match execution {
                        Ok(executed) => {
                            let visible_output = self.redacted_tool_payload(
                                &executed.prepared.tool_id,
                                &executed.execution.output,
                            );
                            let visible_output = self.tool_output_ready(
                                run_id,
                                executed.prepared.proposed_event,
                                &executed.prepared.tool_id,
                                &executed.prepared.call.id,
                                visible_output,
                            );
                            let completed = self.record_tool_completed(
                                run_id,
                                executed.prepared.proposed_event,
                                &executed.prepared.call.id,
                                visible_output.clone(),
                                executed.execution.cost_usd,
                                executed.duration_ms,
                            );
                            let interpretation_model =
                                agent.tool_policy.output_interpretation_model_for(
                                    &executed.prepared.tool_id,
                                    &agent.model,
                                );
                            Self::merge_pending_interpretation_model(
                                &mut pending_interpretation_model,
                                interpretation_model.clone(),
                                agent,
                            );
                            self.record_tool_output_interpreted(
                                &interpretation_model,
                                run_id,
                                completed.id,
                                &executed.prepared.call.id,
                                &visible_output,
                            );
                            if let Some(cost) = executed.execution.cost_usd {
                                total_cost_usd += cost;
                                has_cost_usd = true;
                            }
                            conversation.push(Message::ToolResult {
                                tool_call_id: executed.prepared.call.id,
                                content: Self::stringify_tool_output(&visible_output),
                            });
                            calls_used += 1;
                        }
                        Err((prepared, err)) => {
                            let reason = err.to_string();
                            self.events.append(
                                run_id,
                                Some(prepared.proposed_event),
                                RunEventKind::ToolCallFailed {
                                    call_id: prepared.call.id,
                                    error: reason.clone(),
                                },
                            );
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                    }
                }
                if let Some(err) = first_error {
                    self.record_run_failed(run_id, run_started.id, &err.to_string());
                    return Err(err);
                }
            } else {
                for prepared in prepared_calls {
                    let executed = match self
                        .execute_prepared_tool_call(agent, &scope, prepared.clone())
                        .await
                    {
                        Ok(executed) => executed,
                        Err((prepared, err)) => {
                            let reason = err.to_string();
                            self.events.append(
                                run_id,
                                Some(prepared.proposed_event),
                                RunEventKind::ToolCallFailed {
                                    call_id: prepared.call.id,
                                    error: reason.clone(),
                                },
                            );
                            self.record_run_failed(run_id, run_started.id, &reason);
                            return Err(err);
                        }
                    };
                    if let Some(cost) = executed.execution.cost_usd {
                        total_cost_usd += cost;
                        has_cost_usd = true;
                    }
                    let output = executed.execution.output;
                    let visible_output =
                        self.redacted_tool_payload(&executed.prepared.tool_id, &output);
                    let visible_output = self.tool_output_ready(
                        run_id,
                        executed.prepared.proposed_event,
                        &executed.prepared.tool_id,
                        &executed.prepared.call.id,
                        visible_output,
                    );
                    let completed = self.record_tool_completed(
                        run_id,
                        executed.prepared.proposed_event,
                        &executed.prepared.call.id,
                        visible_output.clone(),
                        executed.execution.cost_usd,
                        executed.duration_ms,
                    );
                    conversation.push(Message::ToolResult {
                        tool_call_id: executed.prepared.call.id.clone(),
                        content: Self::stringify_tool_output(&visible_output),
                    });
                    calls_used += 1;
                    if agent
                        .tool_policy
                        .output_mode_for(&executed.prepared.tool_id)
                        == ToolOutputMode::Raw
                    {
                        let final_output = Self::stringify_tool_output(&visible_output);
                        self.record_run_completed(
                            run_id,
                            run_started.id,
                            &final_output,
                            has_cost_usd.then_some(total_cost_usd),
                            started_at.elapsed().as_millis() as u64,
                        );
                        return Ok(RunResult {
                            run_id,
                            final_output,
                        });
                    }
                    let interpretation_model = agent
                        .tool_policy
                        .output_interpretation_model_for(&executed.prepared.tool_id, &agent.model);
                    Self::merge_pending_interpretation_model(
                        &mut pending_interpretation_model,
                        interpretation_model.clone(),
                        agent,
                    );
                    self.record_tool_output_interpreted(
                        &interpretation_model,
                        run_id,
                        completed.id,
                        &executed.prepared.call.id,
                        &visible_output,
                    );
                    if required_tool_pending
                        .as_ref()
                        .is_some_and(|required| required == &executed.prepared.tool_id)
                    {
                        required_tool_pending = None;
                    }
                }
            }
            self.check_cancelled(run_id)?;
            self.append_pending_guidance(run_id, &mut conversation, &mut consumed_guidance_events);
        }
    }

    async fn call_tool(
        &self,
        agent: &AgentConfig,
        tool_id: ToolId,
        input: Value,
    ) -> Result<ToolCallResult, HarnessError> {
        let scope = RunScope::root(&agent.id);
        let run_id = scope.run_id;
        let started_at = Instant::now();
        let call_id = "manual-1".to_string();
        let redacted_input = self.redacted_tool_payload(&tool_id, &input);
        let run_input = format!("/tool! {} {}", tool_id.0, redacted_input);
        let mut input = input;

        let run_started = self.record_run_started(run_id, None, &agent.id, &run_input);

        let proposed = self.record_tool_proposed(
            run_id,
            run_started.id,
            &call_id,
            &tool_id.0,
            redacted_input,
            None,
            self.tool_permissions_audit(&tool_id),
        );

        if agent.tool_policy.max_calls == 0 {
            let reason = "tool-call budget exhausted (used=0, limit=0)".to_string();
            self.record_policy_denied(run_id, proposed.id, &reason);
            self.events.append(
                run_id,
                Some(proposed.id),
                RunEventKind::ToolCallFailed {
                    call_id: call_id.clone(),
                    error: reason.clone(),
                },
            );
            self.record_run_failed(run_id, run_started.id, &reason);
            return Err(HarnessError::BudgetExhausted(reason));
        }

        if let Err(e) = self.ensure_tool_allowed(agent, &tool_id) {
            let reason = e.to_string();
            self.record_policy_denied(run_id, proposed.id, &reason);
            self.events.append(
                run_id,
                Some(proposed.id),
                RunEventKind::ToolCallFailed {
                    call_id: call_id.clone(),
                    error: reason.clone(),
                },
            );
            self.record_run_failed(run_id, run_started.id, &reason);
            return Err(e);
        }

        match self.before_tool_call(run_id, proposed.id, &tool_id, &call_id, input.clone(), None) {
            Ok(mutated_input) => input = mutated_input,
            Err(err) => {
                let reason = err.to_string();
                self.events.append(
                    run_id,
                    Some(proposed.id),
                    RunEventKind::ToolCallFailed {
                        call_id: call_id.clone(),
                        error: reason.clone(),
                    },
                );
                self.record_run_failed(run_id, run_started.id, &reason);
                return Err(err);
            }
        }

        if let Err(e) = self.record_approval_gate(agent, run_id, proposed.id, &call_id, &tool_id) {
            let reason = e.to_string();
            self.events.append(
                run_id,
                Some(proposed.id),
                RunEventKind::ToolCallFailed {
                    call_id: call_id.clone(),
                    error: reason.clone(),
                },
            );
            self.events.append(
                run_id,
                Some(run_started.id),
                RunEventKind::RunPaused { reason },
            );
            return Err(e);
        }

        let tool_started = self.events.append(
            run_id,
            Some(proposed.id),
            RunEventKind::ToolCallStarted {
                call_id: call_id.clone(),
            },
        );

        let tool_t0 = Instant::now();
        let execution = match self
            .execute_tool_or_subagent(agent, &scope, tool_started.id, &tool_id, input)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                let err_msg = e.to_string();
                self.events.append(
                    run_id,
                    Some(proposed.id),
                    RunEventKind::ToolCallFailed {
                        call_id: call_id.clone(),
                        error: err_msg.clone(),
                    },
                );
                self.record_run_failed(run_id, run_started.id, &err_msg);
                return Err(e);
            }
        };
        let output = execution.output;
        let visible_output = self.redacted_tool_payload(&tool_id, &output);
        let visible_output =
            self.tool_output_ready(run_id, proposed.id, &tool_id, &call_id, visible_output);
        let duration_ms = tool_t0.elapsed().as_millis() as u64;

        self.record_tool_completed(
            run_id,
            proposed.id,
            &call_id,
            visible_output.clone(),
            execution.cost_usd,
            duration_ms,
        );
        self.check_cancelled(run_id)?;
        let final_output = Self::stringify_tool_output(&visible_output);
        self.record_run_completed(
            run_id,
            run_started.id,
            &final_output,
            execution.cost_usd,
            started_at.elapsed().as_millis() as u64,
        );

        Ok(ToolCallResult {
            run_id,
            output: visible_output,
            duration_ms,
        })
    }

    fn preview_context(&self, agent: &AgentConfig, input: UserInput) -> ContextSnapshot {
        self.build_context_snapshot(agent, self.initial_conversation(agent, &input), 0)
    }

    fn explain_config(&self, agent: &AgentConfig) -> ConfigExplanation {
        let per_tool_visibility: BTreeMap<String, VisibilityLevel> = agent
            .tool_policy
            .per_tool_visibility
            .iter()
            .map(|(tool_id, visibility)| (tool_id.0.clone(), *visibility))
            .collect();
        let per_tool_output_modes: BTreeMap<String, ToolOutputMode> = agent
            .tool_policy
            .per_tool_output_modes
            .iter()
            .map(|(tool_id, mode)| (tool_id.0.clone(), *mode))
            .collect();
        let per_tool_output_interpretation_models: BTreeMap<String, String> = agent
            .tool_policy
            .per_tool_output_interpretation_models
            .iter()
            .map(|(tool_id, model)| (tool_id.0.clone(), model.0.clone()))
            .collect();
        let per_tool_output_guidance: BTreeMap<String, String> = agent
            .tool_policy
            .per_tool_output_guidance
            .iter()
            .map(|(tool_id, guidance)| (tool_id.0.clone(), guidance.clone()))
            .collect();
        let skill_visibility_overrides: BTreeMap<String, VisibilityLevel> = agent
            .skill_visibility_overrides
            .iter()
            .map(|(skill_id, visibility)| (skill_id.clone(), *visibility))
            .collect();
        let approval_controller = agent.tool_policy.approval_controller.as_ref().map(|policy| {
            json!({
                "agent_id": policy.agent_id.clone(),
                "allowed_tools": policy.allowed_tools.iter().map(|tool| tool.0.clone()).collect::<Vec<_>>(),
                "allowed_categories": policy.allowed_categories.clone(),
            })
        });
        ConfigExplanation {
            agent_id: agent.id.clone(),
            values: vec![
                ConfigValueExplanation {
                    key: "agent.id".into(),
                    value: Value::String(agent.id.clone()),
                    source: "agent".into(),
                },
                ConfigValueExplanation {
                    key: "agent.name".into(),
                    value: Value::String(agent.name.clone()),
                    source: "agent".into(),
                },
                ConfigValueExplanation {
                    key: "agent.model.default".into(),
                    value: Value::String(agent.model.0.clone()),
                    source: "agent".into(),
                },
                ConfigValueExplanation {
                    key: "agent.prompt_refinement.enabled".into(),
                    value: Value::Bool(agent.prompt_refinement.is_some()),
                    source: "agent/run".into(),
                },
                ConfigValueExplanation {
                    key: "agent.prompt_refinement.model".into(),
                    value: agent
                        .prompt_refinement
                        .as_ref()
                        .and_then(|refinement| refinement.model.as_ref())
                        .map(|model| Value::String(model.0.clone()))
                        .unwrap_or(Value::Null),
                    source: "agent/run".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.max_calls".into(),
                    value: Value::from(agent.tool_policy.max_calls),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.execution_policy.max_subagent_depth".into(),
                    value: Value::from(agent.execution_policy.max_subagent_depth),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.execution_policy.max_recursion_depth".into(),
                    value: Value::from(agent.execution_policy.max_recursion_depth),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.allowed_tools".into(),
                    value: serde_json::to_value(&agent.tool_policy.allowed_tools)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.allowed_categories".into(),
                    value: serde_json::to_value(&agent.tool_policy.allowed_categories)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.approval_controller".into(),
                    value: approval_controller.unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.skill_policy.allowed_categories".into(),
                    value: serde_json::to_value(&agent.allowed_skill_categories)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.skill_policy.visibility".into(),
                    value: serde_json::to_value(agent.skill_visibility).unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.skill_policy.visibility_overrides".into(),
                    value: serde_json::to_value(skill_visibility_overrides).unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.required_tool".into(),
                    value: agent
                        .tool_policy
                        .required_tool
                        .as_ref()
                        .map(|tool| Value::String(tool.0.clone()))
                        .unwrap_or(Value::Null),
                    source: "agent/run".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.output_mode".into(),
                    value: serde_json::to_value(agent.tool_policy.output_mode)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.output_interpretation_model".into(),
                    value: agent
                        .tool_policy
                        .output_interpretation_model
                        .as_ref()
                        .map(|model| Value::String(model.0.clone()))
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.per_tool_output_modes".into(),
                    value: serde_json::to_value(per_tool_output_modes).unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.per_tool_output_interpretation_models".into(),
                    value: serde_json::to_value(per_tool_output_interpretation_models)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.per_tool_output_guidance".into(),
                    value: serde_json::to_value(per_tool_output_guidance).unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.per_tool_visibility".into(),
                    value: serde_json::to_value(per_tool_visibility).unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.visibility".into(),
                    value: serde_json::to_value(agent.tool_policy.visibility)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.cost_policy.input_cost_per_million".into(),
                    value: serde_json::to_value(agent.cost_policy.input_cost_per_million)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.cost_policy.output_cost_per_million".into(),
                    value: serde_json::to_value(agent.cost_policy.output_cost_per_million)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.context_policy.max_tokens_before_compaction".into(),
                    value: serde_json::to_value(
                        agent.context_policy.compaction.max_tokens_before_compaction,
                    )
                    .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.context_policy.max_compaction_output_tokens".into(),
                    value: serde_json::to_value(agent.context_policy.compaction.max_output_tokens)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
            ],
        }
    }

    fn explain_tools(&self, agent: &AgentConfig) -> Vec<ToolView> {
        self.build_context_snapshot(agent, Vec::new(), 0)
            .visible_tools
    }

    fn events(&self, run_id: RunId) -> Vec<RunEvent> {
        self.events.events(run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_llm::{FakeProvider, FakeStep, LlmResponse, LlmToolCall};
    use agent_tools::{FakeTool, SubagentTool, Tool, ToolDescriptor, ToolError, ToolPermissions};
    use agent_tracing::{
        InMemoryEventStore, PublishingEventStore, build_trace_tree, latest_event_id,
    };
    use serde_json::json;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn agent_with_tools(allowed: Vec<ToolId>, max_calls: u32) -> AgentConfig {
        AgentConfig {
            id: "fake-agent".into(),
            name: "Fake".into(),
            system_prompt: "be brief".into(),
            model: ModelRef::from("fake-model"),
            prompt_refinement: None,
            voice: VoiceConfig::default(),
            tool_policy: ToolPolicy {
                max_calls,
                allowed_tools: allowed,
                allowed_categories: Vec::new(),
                required_tool: None,
                visibility: VisibilityLevel::FullSchema,
                per_tool_visibility: HashMap::new(),
                approval_mode: ApprovalMode::AutoApprove,
                approval_controller: None,
                capability_drafts_enabled: false,
                capability_draft_guidance: None,
                output_mode: ToolOutputMode::Interpreted,
                output_interpretation_model: None,
                per_tool_output_modes: HashMap::new(),
                per_tool_output_interpretation_models: HashMap::new(),
                per_tool_output_guidance: HashMap::new(),
            },
            context_policy: ContextPolicy::default(),
            execution_policy: ExecutionPolicy::default(),
            cost_policy: CostPolicy::default(),
            conversation_history: Vec::new(),
            compacted_context: None,
            memory_backend: DEFAULT_MEMORY_BACKEND_ID.into(),
            memory_model: None,
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            allowed_skill_categories: Vec::new(),
            skill_visibility: VisibilityLevel::FullSchema,
            skill_visibility_overrides: HashMap::new(),
            skill_views: Vec::new(),
            subagent_configs: Vec::new(),
        }
    }

    fn echo_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("echo"),
            name: "Echo".into(),
            description: "Returns its input unchanged.".into(),
            categories: vec!["demo".into()],
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            }),
            output_interpretation_guidance: Some("Return echoed text verbatim.".into()),
            permissions: ToolPermissions::default(),
            requires_approval: false,
            provenance: None,
        }
    }

    fn sensitive_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("sensitive"),
            name: "Sensitive".into(),
            description: "Requires approval.".into(),
            categories: vec!["sensitive".into()],
            input_schema: json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                }
            }),
            output_interpretation_guidance: None,
            permissions: ToolPermissions {
                shell: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: None,
        }
    }

    #[test]
    fn approval_unlock_hash_accepts_matching_secret_only() {
        let expected = approval_unlock_sha256("correct horse");

        assert_eq!(
            verify_approval_unlock_hash(&expected, Some("correct horse")),
            Ok(())
        );
        assert_eq!(
            verify_approval_unlock_hash(&format!("sha256:{expected}"), Some("correct horse")),
            Ok(())
        );
        assert_eq!(
            verify_approval_unlock_hash(&expected, Some("wrong horse")),
            Err(ApprovalUnlockError::Invalid)
        );
        assert_eq!(
            verify_approval_unlock_hash(&expected, None),
            Err(ApprovalUnlockError::Missing)
        );
        assert_eq!(
            verify_approval_unlock_hash("not-a-sha256", Some("correct horse")),
            Err(ApprovalUnlockError::InvalidHash)
        );
    }

    #[test]
    fn approval_signature_accepts_matching_hmac_only() {
        let signature = approval_signature_hmac_sha256("controller-secret", "run-1", "approval-1");

        assert_eq!(
            verify_approval_signature("controller-secret", "run-1", "approval-1", Some(&signature),),
            Ok(())
        );
        assert_eq!(
            verify_approval_signature(
                "controller-secret",
                "run-1",
                "approval-1",
                Some(&format!("sha256={signature}")),
            ),
            Ok(())
        );
        assert_eq!(
            verify_approval_signature("controller-secret", "run-1", "approval-1", None),
            Err(ApprovalSignatureError::Missing)
        );
        assert_eq!(
            verify_approval_signature("controller-secret", "run-2", "approval-1", Some(&signature),),
            Err(ApprovalSignatureError::Invalid)
        );
        assert_eq!(
            verify_approval_signature(
                "controller-secret",
                "run-1",
                "approval-1",
                Some("not-a-signature"),
            ),
            Err(ApprovalSignatureError::InvalidFormat)
        );
    }

    struct SecretEchoTool;

    #[async_trait::async_trait]
    impl Tool for SecretEchoTool {
        async fn execute(&self, _input: Value) -> Result<Value, ToolError> {
            Ok(json!({
                "api_key": "sk-test-secret",
                "nested": {
                    "token": "nested-token",
                    "safe": "visible"
                },
                "safe": "visible"
            }))
        }
    }

    fn secret_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("secret_echo"),
            name: "Secret Echo".into(),
            description: "Returns secret-shaped output.".into(),
            categories: vec!["sensitive".into()],
            input_schema: json!({"type": "object"}),
            output_interpretation_guidance: None,
            permissions: ToolPermissions {
                secrets: true,
                ..ToolPermissions::default()
            },
            requires_approval: false,
            provenance: None,
        }
    }

    fn registry_with_echo() -> Arc<ToolRegistry> {
        let mut reg = ToolRegistry::new();
        reg.register(echo_descriptor(), Arc::new(FakeTool::echo()));
        Arc::new(reg)
    }

    #[cfg(unix)]
    fn write_executable_hook_script(name: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let script = std::env::temp_dir().join(format!(
            "agent-{name}-hook-test-{}-{nanos}.sh",
            std::process::id()
        ));
        std::fs::write(&script, body).unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();
        script
    }

    fn registry_with_secret_tool() -> Arc<ToolRegistry> {
        let mut reg = ToolRegistry::new();
        reg.register(secret_descriptor(), Arc::new(SecretEchoTool));
        Arc::new(reg)
    }

    #[test]
    fn wallet_payment_and_browser_permissions_require_approval() {
        for (field, permissions) in [
            (
                "wallet",
                ToolPermissions {
                    wallet: true,
                    ..ToolPermissions::default()
                },
            ),
            (
                "payment",
                ToolPermissions {
                    payment: true,
                    ..ToolPermissions::default()
                },
            ),
            (
                "browser_profile",
                ToolPermissions {
                    browser_profile: true,
                    ..ToolPermissions::default()
                },
            ),
        ] {
            let mut descriptor = echo_descriptor();
            descriptor.permissions = permissions;
            assert!(tool_requires_approval(&descriptor));
            assert!(permission_reason(&descriptor).contains(field));
        }
    }

    struct CapturingEventStore {
        inner: InMemoryEventStore,
        latest_run_id: Mutex<Option<RunId>>,
    }

    impl CapturingEventStore {
        fn new() -> Self {
            Self {
                inner: InMemoryEventStore::new(),
                latest_run_id: Mutex::new(None),
            }
        }

        fn latest_run_id(&self) -> Option<RunId> {
            *self
                .latest_run_id
                .lock()
                .expect("latest run id mutex poisoned")
        }
    }

    impl EventStore for CapturingEventStore {
        fn append(&self, run_id: RunId, parent: Option<EventId>, kind: RunEventKind) -> RunEvent {
            if matches!(kind, RunEventKind::RunStarted { .. }) {
                *self
                    .latest_run_id
                    .lock()
                    .expect("latest run id mutex poisoned") = Some(run_id);
            }
            self.inner.append(run_id, parent, kind)
        }

        fn events(&self, run_id: RunId) -> Vec<RunEvent> {
            self.inner.events(run_id)
        }
    }

    struct CancellingTool {
        store: Arc<CapturingEventStore>,
    }

    #[async_trait::async_trait]
    impl Tool for CancellingTool {
        async fn execute(&self, input: Value) -> Result<Value, ToolError> {
            let Some(run_id) = self.store.latest_run_id() else {
                return Err(ToolError::Execution("run id was not captured".into()));
            };
            let parent = latest_event_id(&self.store.events(run_id));
            self.store.append(
                run_id,
                parent,
                RunEventKind::RunCancelled {
                    reason: "test stop".into(),
                },
            );
            Ok(input)
        }
    }

    fn registry_with_subagent() -> Arc<ToolRegistry> {
        let mut reg = ToolRegistry::new();
        reg.register(echo_descriptor(), Arc::new(FakeTool::echo()));
        reg.register(SubagentTool::descriptor(), Arc::new(SubagentTool));
        Arc::new(reg)
    }

    fn external_agent_descriptor(id: &str) -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from(id),
            name: "Remote Reviewer".into(),
            description: "Calls a remote reviewer agent.".into(),
            categories: vec![
                "agent".into(),
                "subagent".into(),
                "external-agent".into(),
                "a2a".into(),
            ],
            input_schema: json!({
                "type": "object",
                "required": ["prompt"],
                "properties": {
                    "prompt": { "type": "string" }
                }
            }),
            output_interpretation_guidance: Some("Preserve the remote agent response.".into()),
            permissions: ToolPermissions {
                network: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("adapter:test".into()),
        }
    }

    fn registry_with_external_agent(id: &str, runner: Arc<dyn Tool>) -> Arc<ToolRegistry> {
        let mut reg = ToolRegistry::new();
        reg.register(external_agent_descriptor(id), runner);
        Arc::new(reg)
    }

    fn probe_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("probe"),
            name: "Probe".into(),
            description: "Tracks concurrent executions.".into(),
            categories: vec!["test".into()],
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            }),
            output_interpretation_guidance: None,
            permissions: ToolPermissions::default(),
            requires_approval: false,
            provenance: None,
        }
    }

    fn slow_echo_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("slow_echo"),
            name: "Slow Echo".into(),
            description: "Echoes after a short delay.".into(),
            categories: vec!["test".into()],
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            }),
            output_interpretation_guidance: None,
            permissions: ToolPermissions::default(),
            requires_approval: false,
            provenance: None,
        }
    }

    struct SlowEchoTool;

    #[async_trait]
    impl Tool for SlowEchoTool {
        async fn execute(&self, input: Value) -> Result<Value, ToolError> {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            Ok(input)
        }
    }

    struct ProbeTool {
        active: Arc<AtomicUsize>,
        observed_parallel: Arc<AtomicBool>,
    }

    #[async_trait]
    impl Tool for ProbeTool {
        async fn execute(&self, input: Value) -> Result<Value, ToolError> {
            if self.active.fetch_add(1, Ordering::SeqCst) > 0 {
                self.observed_parallel.store(true, Ordering::SeqCst);
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(input)
        }
    }

    fn kinds(events: &[RunEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match &e.kind {
                RunEventKind::RunStarted { .. } => "RunStarted",
                RunEventKind::ContextBuilt { .. } => "ContextBuilt",
                RunEventKind::LlmRequestStarted { .. } => "LlmRequestStarted",
                RunEventKind::LlmStreamToken { .. } => "LlmStreamToken",
                RunEventKind::LlmRequestCompleted { .. } => "LlmRequestCompleted",
                RunEventKind::PromptRefinementStarted { .. } => "PromptRefinementStarted",
                RunEventKind::PromptRefinementCompleted { .. } => "PromptRefinementCompleted",
                RunEventKind::ToolCallProposed { .. } => "ToolCallProposed",
                RunEventKind::ToolCallStarted { .. } => "ToolCallStarted",
                RunEventKind::ToolCallCompleted { .. } => "ToolCallCompleted",
                RunEventKind::ToolOutputInterpreted { .. } => "ToolOutputInterpreted",
                RunEventKind::ToolCallFailed { .. } => "ToolCallFailed",
                RunEventKind::ApprovalRequested { .. } => "ApprovalRequested",
                RunEventKind::ApprovalResolved { .. } => "ApprovalResolved",
                RunEventKind::ApprovalControllerAssessed { .. } => "ApprovalControllerAssessed",
                RunEventKind::GuidanceInjected { .. } => "GuidanceInjected",
                RunEventKind::QualityScored { .. } => "QualityScored",
                RunEventKind::MemoryLoaded { .. } => "MemoryLoaded",
                RunEventKind::MemoryRead { .. } => "MemoryRead",
                RunEventKind::MemoryWritten { .. } => "MemoryWritten",
                RunEventKind::IngestionReferenced { .. } => "IngestionReferenced",
                RunEventKind::IngestionStarted { .. } => "IngestionStarted",
                RunEventKind::IngestionCompleted { .. } => "IngestionCompleted",
                RunEventKind::HookFired { .. } => "HookFired",
                RunEventKind::HookFailed { .. } => "HookFailed",
                RunEventKind::PolicyDenied { .. } => "PolicyDenied",
                RunEventKind::ChildRunStarted { .. } => "ChildRunStarted",
                RunEventKind::ChildRunCompleted { .. } => "ChildRunCompleted",
                RunEventKind::BatchRunStarted { .. } => "BatchRunStarted",
                RunEventKind::BatchItemStatus { .. } => "BatchItemStatus",
                RunEventKind::BatchRunCompleted { .. } => "BatchRunCompleted",
                RunEventKind::RunPaused { .. } => "RunPaused",
                RunEventKind::RunCancelled { .. } => "RunCancelled",
                RunEventKind::RunCompleted { .. } => "RunCompleted",
                RunEventKind::RunFailed { .. } => "RunFailed",
            })
            .collect()
    }

    #[tokio::test]
    async fn no_tool_call_path_emits_minimal_sequence() {
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let r = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "hello".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(r.final_output, "[fake] hello");
        assert_eq!(
            kinds(&h.events(r.run_id)),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );
    }

    #[tokio::test]
    async fn lifecycle_hooks_emit_observe_only_trace_events() {
        let h = Harness::new(
            Arc::new(FakeProvider::sequence(vec![
                FakeStep::CallTool {
                    id: "c1".into(),
                    tool: "echo".into(),
                    input: json!({"text": "ping"}),
                },
                FakeStep::Reply("done".into()),
            ])),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        )
        .with_hooks(vec![RunLifecycleHook::new(
            "audit",
            vec![
                HookTrigger::RunStarted,
                HookTrigger::ContextBuilt,
                HookTrigger::ToolProposed,
                HookTrigger::ToolCompleted,
                HookTrigger::RunCompleted,
                HookTrigger::RunFailed,
            ],
        )]);

        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "use a tool".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(result.final_output, "done");
        let hooks = h
            .events(result.run_id)
            .into_iter()
            .filter_map(|event| match event.kind {
                RunEventKind::HookFired {
                    hook_id,
                    trigger,
                    payload_digest,
                } => Some((hook_id, trigger, payload_digest)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            hooks
                .iter()
                .map(|(_, trigger, _)| trigger.as_str())
                .collect::<Vec<_>>(),
            vec![
                "run_started",
                "context_built",
                "tool_proposed",
                "tool_completed",
                "context_built",
                "run_completed",
            ]
        );
        assert!(hooks.iter().all(|(hook_id, _, digest)| {
            hook_id == "audit"
                && digest.len() == 64
                && digest.chars().all(|ch| ch.is_ascii_hexdigit())
        }));
    }

    #[tokio::test]
    async fn lifecycle_hook_command_handlers_reject_shell_interpreters() {
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        )
        .with_hooks(vec![
            RunLifecycleHook::new("shell-hook", vec![HookTrigger::RunStarted]).with_handler(
                RunHookHandler::command("sh", vec!["-c".into(), "true".into()], Some(50)),
            ),
        ]);

        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "hello".into(),
                },
            )
            .await
            .unwrap();
        let events = h.events(result.run_id);
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::HookFired { ref hook_id, .. } if hook_id == "shell-hook"
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::PolicyDenied { ref reason }
                if reason.contains("shell-hook") && reason.contains("not allowed")
        )));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn lifecycle_hook_command_handlers_retry_and_trace_failures() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let state = std::env::temp_dir().join(format!(
            "agent-hook-retry-state-{}-{nanos}",
            std::process::id()
        ));
        let script = write_executable_hook_script(
            "retry",
            &format!(
                r#"#!/bin/sh
cat >/dev/null
if [ ! -f "{state}" ]; then
  echo first > "{state}"
  exit 1
fi
exit 0
"#,
                state = state.display()
            ),
        );
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        )
        .with_hooks(vec![
            RunLifecycleHook::new("retry-hook", vec![HookTrigger::RunStarted]).with_handler(
                RunHookHandler::command(script.display().to_string(), Vec::new(), Some(2_000))
                    .with_retry_attempts(Some(1)),
            ),
        ]);

        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "hello".into(),
                },
            )
            .await
            .unwrap();
        let events = h.events(result.run_id);
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::HookFailed {
                ref hook_id,
                attempt: 1,
                will_retry: true,
                ..
            } if hook_id == "retry-hook"
        )));
        assert!(!events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::PolicyDenied { ref reason } if reason.contains("retry-hook")
        )));
        let _ = std::fs::remove_file(script);
        let _ = std::fs::remove_file(state);
    }

    #[test]
    fn hook_context_fragments_reject_secret_like_content() {
        let err = parse_hook_context_fragments(
            "audit",
            r#"{"context_fragments":[{"content":"api_key=sk-test-secret"}]}"#,
        )
        .unwrap_err();
        assert!(err.contains("secret guardrail"));
    }

    #[test]
    fn hook_tool_output_rejects_secret_like_content() {
        let err = parse_hook_tool_output(
            "audit",
            r#"{"output":{"api_key":"sk-test-secret","safe":"visible"}}"#,
        )
        .unwrap_err();
        assert!(err.contains("secret guardrail"));
    }

    #[test]
    fn hook_tool_call_rejects_secret_like_mutation() {
        let err = parse_hook_tool_call(
            "audit",
            r#"{"input":{"api_key":"sk-test-secret","safe":"visible"}}"#,
        )
        .unwrap_err();
        assert!(err.contains("secret guardrail"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn before_tool_call_hook_can_mutate_manual_tool_input() {
        let script = write_executable_hook_script(
            "tool-input",
            r#"#!/bin/sh
cat >/dev/null
cat <<'JSON'
{"input":{"text":"mutated input from hook"}}
JSON
"#,
        );
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        )
        .with_hooks(vec![
            RunLifecycleHook::new("input-hook", vec![HookTrigger::BeforeToolCall]).with_handler(
                RunHookHandler::command(script.display().to_string(), Vec::new(), Some(2_000)),
            ),
        ]);

        let result = h
            .call_tool(
                &agent_with_tools(vec![], 5),
                ToolId::from("echo"),
                json!({"text": "original input"}),
            )
            .await
            .unwrap();
        let _ = std::fs::remove_file(&script);

        assert_eq!(result.output, json!({"text": "mutated input from hook"}));
        let events = h.events(result.run_id);
        assert_eq!(
            kinds(&events),
            vec![
                "RunStarted",
                "ToolCallProposed",
                "HookFired",
                "ToolCallStarted",
                "ToolCallCompleted",
                "RunCompleted",
            ]
        );
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::HookFired { ref hook_id, ref trigger, .. }
                if hook_id == "input-hook" && trigger == "before_tool_call"
        )));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn before_tool_call_hook_can_deny_llm_tool_call() {
        let script = write_executable_hook_script(
            "tool-deny",
            r#"#!/bin/sh
cat >/dev/null
cat <<'JSON'
{"deny":true,"reason":"blocked by hook"}
JSON
"#,
        );
        let store = Arc::new(CapturingEventStore::new());
        let h = Harness::new(
            Arc::new(FakeProvider::sequence(vec![FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"text": "original input"}),
            }])),
            store.clone(),
            registry_with_echo(),
        )
        .with_hooks(vec![
            RunLifecycleHook::new("deny-hook", vec![HookTrigger::BeforeToolCall]).with_handler(
                RunHookHandler::command(script.display().to_string(), Vec::new(), Some(2_000)),
            ),
        ]);

        let err = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap_err();
        let _ = std::fs::remove_file(&script);

        assert!(err.to_string().contains("blocked by hook"));
        let run_id = store.latest_run_id().expect("run id");
        let events = h.events(run_id);
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::HookFired { ref hook_id, ref trigger, .. }
                if hook_id == "deny-hook" && trigger == "before_tool_call"
        )));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::ToolCallFailed { ref error, .. } if error.contains("blocked by hook")
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event.kind, RunEventKind::ToolCallStarted { .. }))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tool_output_hook_can_mutate_visible_tool_output() {
        let script = write_executable_hook_script(
            "tool-output",
            r#"#!/bin/sh
cat >/dev/null
cat <<'JSON'
{"output":"mutated output from hook"}
JSON
"#,
        );
        let h = Harness::new(
            Arc::new(FakeProvider::sequence(vec![
                FakeStep::CallTool {
                    id: "c1".into(),
                    tool: "echo".into(),
                    input: json!({"text": "original output"}),
                },
                FakeStep::Reply("should not be reached".into()),
            ])),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        )
        .with_hooks(vec![
            RunLifecycleHook::new("output-hook", vec![HookTrigger::ToolOutputReady]).with_handler(
                RunHookHandler::command(script.display().to_string(), Vec::new(), Some(2_000)),
            ),
        ]);
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.output_mode = ToolOutputMode::Raw;

        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let _ = std::fs::remove_file(&script);

        assert_eq!(result.final_output, "mutated output from hook");
        let events = h.events(result.run_id);
        assert_eq!(
            kinds(&events),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "ToolCallProposed",
                "ToolCallStarted",
                "HookFired",
                "ToolCallCompleted",
                "RunCompleted",
            ]
        );
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::HookFired { ref hook_id, ref trigger, .. }
                if hook_id == "output-hook" && trigger == "tool_output_ready"
        )));
        let completed_output = events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ToolCallCompleted { output, .. } => Some(output),
                _ => None,
            })
            .expect("tool completion output");
        assert_eq!(completed_output, &json!("mutated output from hook"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tool_output_hook_rejects_secret_like_mutation_and_keeps_original() {
        let script = write_executable_hook_script(
            "tool-output-secret",
            r#"#!/bin/sh
cat >/dev/null
cat <<'JSON'
{"output":{"api_key":"sk-test-secret","safe":"visible"}}
JSON
"#,
        );
        let h = Harness::new(
            Arc::new(FakeProvider::sequence(vec![
                FakeStep::CallTool {
                    id: "c1".into(),
                    tool: "echo".into(),
                    input: json!({"text": "original output"}),
                },
                FakeStep::Reply("should not be reached".into()),
            ])),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        )
        .with_hooks(vec![
            RunLifecycleHook::new("output-hook", vec![HookTrigger::ToolOutputReady]).with_handler(
                RunHookHandler::command(script.display().to_string(), Vec::new(), Some(2_000)),
            ),
        ]);
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.output_mode = ToolOutputMode::Raw;

        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let _ = std::fs::remove_file(&script);

        assert_eq!(result.final_output, r#"{"text":"original output"}"#);
        let events = h.events(result.run_id);
        assert!(
            events.iter().any(|event| matches!(
                event.kind,
                RunEventKind::PolicyDenied { ref reason }
                    if reason.contains("output-hook")
                        && reason.contains("tool output mutation failed")
                        && reason.contains("secret guardrail")
            )),
            "events: {events:#?}"
        );
        let completed_output = events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ToolCallCompleted { output, .. } => Some(output),
                _ => None,
            })
            .expect("tool completion output");
        assert_eq!(completed_output, &json!({"text": "original output"}));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn before_context_hook_can_add_validated_context_fragment() {
        use std::os::unix::fs::PermissionsExt;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let script = std::env::temp_dir().join(format!(
            "agent-context-hook-test-{}-{nanos}.sh",
            std::process::id()
        ));
        std::fs::write(
            &script,
            r#"#!/bin/sh
cat >/dev/null
cat <<'JSON'
{"context_fragments":[{"id":"hook-note","content":"context from hook","provenance":"test-hook"}]}
JSON
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();

        let h = Harness::new(
            Arc::new(FakeProvider::canned("done")),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        )
        .with_hooks(vec![
            RunLifecycleHook::new("context-hook", vec![HookTrigger::BeforeContextBuilt])
                .with_handler(RunHookHandler::command(
                    script.display().to_string(),
                    Vec::new(),
                    Some(2_000),
                )),
        ]);

        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "hello".into(),
                },
            )
            .await
            .unwrap();
        let _ = std::fs::remove_file(&script);
        let events = h.events(result.run_id);
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::HookFired { ref hook_id, ref trigger, .. }
                if hook_id == "context-hook" && trigger == "before_context_built"
        )));
        let snapshot = events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ContextBuilt { snapshot } => {
                    serde_json::from_value::<ContextSnapshot>(snapshot.clone()).ok()
                }
                _ => None,
            })
            .expect("context snapshot");
        assert!(snapshot.loaded_memory.iter().any(|fragment| {
            fragment.id == "hook-note"
                && fragment.content == "context from hook"
                && fragment.provenance == "test-hook"
        }));
        assert!(snapshot.system_prompt.contains("context from hook"));
        assert!(snapshot.provenance.iter().any(|record| {
            record.fragment == "hook_context"
                && record.source == "run.lifecycle.before_context_built"
        }));
    }

    #[tokio::test]
    async fn streaming_tokens_are_traced_before_completion() {
        let h = Harness::new(
            Arc::new(FakeProvider::sequence(vec![FakeStep::StreamReply(vec![
                "hel".into(),
                "lo".into(),
            ])])),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap();
        let events = h.events(result.run_id);
        let llm_started = events
            .iter()
            .find(|event| matches!(event.kind, RunEventKind::LlmRequestStarted { .. }))
            .expect("LLM request should start");
        let deltas = events
            .iter()
            .filter_map(|event| match &event.kind {
                RunEventKind::LlmStreamToken { delta } => {
                    Some((event.parent_event, delta.as_str()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(result.final_output, "hello");
        assert_eq!(
            deltas,
            vec![(Some(llm_started.id), "hel"), (Some(llm_started.id), "lo")]
        );
        assert_eq!(
            kinds(&events),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmStreamToken",
                "LlmStreamToken",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );
    }

    #[tokio::test]
    async fn llm_request_started_records_request_digest() {
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let r = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "hello".into(),
                },
            )
            .await
            .unwrap();
        let digest = h
            .events(r.run_id)
            .into_iter()
            .find_map(|event| match event.kind {
                RunEventKind::LlmRequestStarted { request_digest, .. } => request_digest,
                _ => None,
            })
            .expect("run should emit LlmRequestStarted with a digest");

        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn llm_request_completed_records_configured_cost() {
        let mut agent = agent_with_tools(vec![], 5);
        agent.cost_policy = CostPolicy {
            input_cost_per_million: Some(1.0),
            output_cost_per_million: Some(2.0),
        };
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );

        let result = h
            .run(
                &agent,
                UserInput {
                    text: "hello".into(),
                },
            )
            .await
            .unwrap();
        let (tokens_in, tokens_out, cost_usd) = h
            .events(result.run_id)
            .into_iter()
            .find_map(|event| match event.kind {
                RunEventKind::LlmRequestCompleted {
                    tokens_in,
                    tokens_out,
                    cost_usd,
                    ..
                } => Some((
                    tokens_in,
                    tokens_out,
                    cost_usd.expect("cost should be traced"),
                )),
                _ => None,
            })
            .expect("run should emit LlmRequestCompleted");
        let expected =
            f64::from(tokens_in) / 1_000_000.0 + (2.0 * f64::from(tokens_out) / 1_000_000.0);

        assert!((cost_usd - expected).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn preview_context_matches_first_context_built_event() {
        let agent = agent_with_tools(vec![], 5);
        let input = UserInput {
            text: "hello".into(),
        };
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );

        let preview = serde_json::to_value(h.preview_context(&agent, input.clone())).unwrap();
        let r = h.run(&agent, input).await.unwrap();
        let event_snapshot = h
            .events(r.run_id)
            .into_iter()
            .find_map(|e| match e.kind {
                RunEventKind::ContextBuilt { snapshot } => Some(snapshot),
                _ => None,
            })
            .expect("run should emit ContextBuilt");

        assert_eq!(preview, event_snapshot);
    }

    #[test]
    fn preview_context_estimates_input_tokens_from_visible_context() {
        let mut agent = agent_with_tools(vec![ToolId::from("echo")], 5);
        agent.memory_fragments = vec![MemoryFragment {
            id: "mem-1".into(),
            content: "memory facts for preview accounting".into(),
            provenance: "test".into(),
        }];
        agent.ingestion_artifacts = vec![IngestedArtifactView {
            id: "ing-1".into(),
            source: "/tmp/source.txt".into(),
            sections: 1,
            content: "ingested artifact text for preview accounting".into(),
            findings: Vec::new(),
            provenance: "test".into(),
        }];
        agent.skill_views = vec![SkillView {
            id: "review".into(),
            name: "Review".into(),
            description: Some("Review skill".into()),
            categories: vec!["review".into()],
            body: Some("# Review\nUse the full review checklist.".into()),
            estimated_tokens: 12,
            visibility: VisibilityLevel::FullSchema,
            provenance: Some("test".into()),
        }];
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );

        let snapshot = h.preview_context(
            &agent,
            UserInput {
                text: "please preview this".into(),
            },
        );

        assert!(
            snapshot
                .system_prompt
                .contains("Use the full review checklist.")
        );
        assert!(snapshot.estimated_input_tokens >= 12);
        assert!(
            snapshot.estimated_input_tokens
                > estimate_text_tokens(&snapshot.system_prompt)
                    + estimate_message_tokens(&snapshot.conversation[0])
        );
    }

    #[test]
    fn preview_context_includes_persisted_conversation_history() {
        let mut agent = agent_with_tools(vec![], 5);
        agent.conversation_history = vec![
            Message::user("earlier request"),
            Message::Assistant {
                content: Some("earlier answer".into()),
                tool_calls: Vec::new(),
            },
        ];
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );

        let snapshot = h.preview_context(
            &agent,
            UserInput {
                text: "new request".into(),
            },
        );

        assert_eq!(snapshot.conversation.len(), 3);
        assert!(matches!(
            &snapshot.conversation[0],
            Message::User { content } if content == "earlier request"
        ));
        assert!(snapshot.provenance.iter().any(|record| {
            record.fragment == "conversation_history"
                && record.source == "profile.main.conversations"
        }));
    }

    #[test]
    fn preview_context_auto_compacts_history_when_policy_threshold_is_exceeded() {
        let mut agent = agent_with_tools(vec![], 5);
        agent.context_policy.compaction = ContextCompactionPolicy {
            max_tokens_before_compaction: Some(24),
            max_output_tokens: Some(96),
            guidance: Some("keep decisions and unresolved facts".into()),
        };
        agent.conversation_history = vec![
            Message::user("earlier request about alpha beta gamma delta epsilon"),
            Message::Assistant {
                content: Some("earlier answer with retained decision one".into()),
                tool_calls: Vec::new(),
            },
            Message::user("follow up about zeta eta theta iota kappa"),
            Message::Assistant {
                content: Some("follow up answer with retained decision two".into()),
                tool_calls: Vec::new(),
            },
        ];
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );

        let snapshot = h.preview_context(
            &agent,
            UserInput {
                text: "latest request stays explicit".into(),
            },
        );

        assert_eq!(snapshot.conversation.len(), 1);
        assert!(matches!(
            &snapshot.conversation[0],
            Message::User { content } if content == "latest request stays explicit"
        ));
        let compacted = snapshot.compacted.as_deref().expect("auto compaction");
        assert!(compacted.contains("<auto-compaction"));
        assert!(compacted.contains("Guidance: keep decisions and unresolved facts"));
        assert!(compacted.contains("earlier request"));
        assert!(snapshot.system_prompt.contains("<compacted-context>"));
        let review = snapshot
            .compaction_review
            .as_ref()
            .expect("compaction review");
        assert_eq!(review.mode, CompactionReviewMode::Auto);
        assert_eq!(review.before_messages.len(), 5);
        assert_eq!(review.visible_messages.len(), 1);
        assert!(
            review
                .before_messages
                .iter()
                .any(|line| line.contains("earlier request"))
        );
        assert!(
            review
                .visible_messages
                .iter()
                .any(|line| line.contains("latest request stays explicit"))
        );
        assert!(
            review
                .compacted_context
                .contains("Guidance: keep decisions and unresolved facts")
        );
        assert!(review.before_tokens > 0);
        assert!(review.after_tokens > 0);
        assert!(snapshot.provenance.iter().any(|record| {
            record.fragment == "compacted_context"
                && record.source == "agent.context_policy.auto_compaction"
        }));
    }

    #[test]
    fn category_allowlists_filter_visible_tools_and_skills() {
        let mut registry = ToolRegistry::new();
        registry.register(echo_descriptor(), Arc::new(FakeTool::echo()));
        registry.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));

        let mut agent = agent_with_tools(vec![], 1);
        agent.tool_policy.allowed_categories = vec!["sensitive".into()];
        agent.allowed_skill_categories = vec!["review".into()];
        agent.skill_views = vec![
            SkillView {
                id: "review".into(),
                name: "Review".into(),
                description: Some("Review skill".into()),
                categories: vec!["review".into()],
                body: Some("Use the review checklist.".into()),
                estimated_tokens: 6,
                visibility: VisibilityLevel::FullSchema,
                provenance: Some("test".into()),
            },
            SkillView {
                id: "writing".into(),
                name: "Writing".into(),
                description: Some("Writing skill".into()),
                categories: vec!["writing".into()],
                body: Some("Write clearly.".into()),
                estimated_tokens: 4,
                visibility: VisibilityLevel::FullSchema,
                provenance: Some("test".into()),
            },
        ];
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(registry),
        );

        let snapshot = h.preview_context(&agent, UserInput { text: "go".into() });

        assert_eq!(
            snapshot
                .visible_tools
                .iter()
                .map(|tool| tool.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sensitive"]
        );
        assert_eq!(
            snapshot
                .visible_skills
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>(),
            vec!["review"]
        );
        assert!(snapshot.system_prompt.contains("Use the review checklist."));
        assert!(!snapshot.system_prompt.contains("Write clearly."));
    }

    #[test]
    fn per_skill_visibility_overrides_global_skill_visibility() {
        let mut agent = agent_with_tools(vec![], 5);
        agent.skill_visibility = VisibilityLevel::NameOnly;
        agent
            .skill_visibility_overrides
            .insert("review".into(), VisibilityLevel::FullSchema);
        agent.skill_views = vec![
            SkillView {
                id: "review".into(),
                name: "Review".into(),
                description: Some("Review skill".into()),
                categories: vec!["review".into()],
                body: Some("Use the review checklist.".into()),
                estimated_tokens: 6,
                visibility: VisibilityLevel::FullSchema,
                provenance: Some("test".into()),
            },
            SkillView {
                id: "writing".into(),
                name: "Writing".into(),
                description: Some("Writing skill".into()),
                categories: vec!["writing".into()],
                body: Some("Write clearly.".into()),
                estimated_tokens: 4,
                visibility: VisibilityLevel::FullSchema,
                provenance: Some("test".into()),
            },
        ];
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );

        let snapshot = h.preview_context(&agent, UserInput { text: "go".into() });
        let review = snapshot
            .visible_skills
            .iter()
            .find(|skill| skill.id == "review")
            .expect("review skill should be visible");
        let writing = snapshot
            .visible_skills
            .iter()
            .find(|skill| skill.id == "writing")
            .expect("writing skill should be visible");

        assert_eq!(review.visibility, VisibilityLevel::FullSchema);
        assert_eq!(review.body.as_deref(), Some("Use the review checklist."));
        assert_eq!(writing.visibility, VisibilityLevel::NameOnly);
        assert!(writing.description.is_none());
        assert!(writing.body.is_none());
        assert!(snapshot.system_prompt.contains("Use the review checklist."));
        assert!(!snapshot.system_prompt.contains("Write clearly."));
    }

    #[tokio::test]
    async fn category_allowlist_denies_unlisted_tool_execution() {
        let mut registry = ToolRegistry::new();
        registry.register(echo_descriptor(), Arc::new(FakeTool::echo()));
        registry.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "echo".into(),
            input: json!({"text": "should not run"}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(registry),
        );
        let mut agent = agent_with_tools(vec![], 1);
        agent.tool_policy.allowed_categories = vec!["sensitive".into()];

        let err = h
            .run(
                &agent,
                UserInput {
                    text: "call hidden tool".into(),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            HarnessError::PolicyDenied(ref reason)
                if reason.contains("not allowed by agent tool policy")
        ));
    }

    #[tokio::test]
    async fn secret_tool_payloads_are_redacted_from_trace_and_result() {
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_secret_tool(),
        );
        let result = h
            .call_tool(
                &agent_with_tools(vec![], 1),
                ToolId::from("secret_echo"),
                json!({"password": "pw-test-secret", "safe": "visible"}),
            )
            .await
            .unwrap();

        assert_eq!(result.output["api_key"], "[REDACTED]");
        assert_eq!(result.output["nested"]["token"], "[REDACTED]");
        assert_eq!(result.output["nested"]["safe"], "visible");

        let events = h.events(result.run_id);
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::RunStarted { input, .. }
                if input.contains("[REDACTED]") && !input.contains("pw-test-secret")
        )));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolCallProposed { input, .. }
                if input["password"] == "[REDACTED]" && input["safe"] == "visible"
        )));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolCallCompleted { output, .. }
                if output["api_key"] == "[REDACTED]"
                    && output["nested"]["token"] == "[REDACTED]"
                    && output["safe"] == "visible"
        )));
        assert!(
            !serde_json::to_string(&events)
                .unwrap()
                .contains("sk-test-secret")
        );
    }

    #[test]
    fn authorization_headers_are_redacted_as_secret_payloads() {
        let payload = json!({
            "headers": {
                "Authorization": "Bearer sk-test-secret",
                "X-Trace": "visible"
            }
        });

        assert!(contains_secret_marker(&payload));
        let redacted = redact_secret_markers(&payload, false);

        assert_eq!(
            redacted["headers"]["Authorization"],
            serde_json::Value::String("[REDACTED]".into())
        );
        assert_eq!(redacted["headers"]["X-Trace"], "visible");
    }

    #[tokio::test]
    async fn secret_tool_output_is_redacted_before_followup_context() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "secret_echo".into(),
                input: json!({}),
            },
            FakeStep::Reply("done".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_secret_tool(),
        );
        let result = h
            .run(
                &agent_with_tools(vec![], 2),
                UserInput {
                    text: "use secret tool".into(),
                },
            )
            .await
            .unwrap();

        let events = h.events(result.run_id);
        let snapshots = events
            .iter()
            .filter_map(|event| match &event.kind {
                RunEventKind::ContextBuilt { snapshot } => Some(snapshot),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(snapshots.len(), 2);
        let followup_context = serde_json::to_string(snapshots[1]).unwrap();
        assert!(followup_context.contains("[REDACTED]"));
        assert!(!followup_context.contains("sk-test-secret"));
        assert!(!followup_context.contains("nested-token"));
    }

    #[tokio::test]
    async fn prompt_refinement_is_traced_and_feeds_the_main_context() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::Reply("refined task".into()),
            FakeStep::Reply("final answer".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.prompt_refinement = Some(PromptRefinement {
            instructions: "Make it crisp.".into(),
            model: Some(ModelRef::from("refiner-model")),
        });

        let result = h
            .run(
                &agent,
                UserInput {
                    text: "original task".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(result.final_output, "final answer");
        assert_eq!(
            kinds(&h.events(result.run_id)),
            vec![
                "RunStarted",
                "PromptRefinementStarted",
                "PromptRefinementCompleted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );

        let events = h.events(result.run_id);
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::RunStarted { input, .. } if input == "original task"
        )));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::PromptRefinementStarted {
                model,
                instructions,
                ..
            } if model == "refiner-model" && instructions == "Make it crisp."
        )));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::PromptRefinementCompleted { refined_input, .. }
                if refined_input == "refined task"
        )));

        let snapshot = events
            .into_iter()
            .find_map(|event| match event.kind {
                RunEventKind::ContextBuilt { snapshot } => {
                    serde_json::from_value::<ContextSnapshot>(snapshot).ok()
                }
                _ => None,
            })
            .expect("refined run should build context");
        assert!(matches!(
            snapshot.conversation.first(),
            Some(Message::User { content }) if content == "refined task"
        ));
        assert!(
            !serde_json::to_string(&snapshot)
                .unwrap()
                .contains("original task")
        );
    }

    #[tokio::test]
    async fn mid_run_guidance_is_added_to_the_next_context() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "slow_echo".into(),
                input: json!({"text": "before guidance"}),
            },
            FakeStep::Reply("done".into()),
        ]);
        let mut registry = ToolRegistry::new();
        registry.register(slow_echo_descriptor(), Arc::new(SlowEchoTool));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let store = Arc::new(PublishingEventStore::new(InMemoryEventStore::new(), tx));
        let h = Harness::new(Arc::new(provider), store.clone(), Arc::new(registry));
        let agent = agent_with_tools(vec![], 5);

        let handle =
            tokio::spawn(async move { h.run(&agent, UserInput { text: "go".into() }).await });

        let mut run_id = None;
        loop {
            let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
                .await
                .expect("run should emit events before timeout")
                .expect("event stream should stay open until guidance is injected");
            match event.kind {
                RunEventKind::RunStarted { .. } => run_id = Some(event.run_id),
                RunEventKind::ToolCallStarted { .. } => {
                    let run_id = run_id.expect("run should start before tools");
                    store.append(
                        run_id,
                        None,
                        RunEventKind::GuidanceInjected {
                            content: "prefer a concise final answer".into(),
                        },
                    );
                    break;
                }
                _ => {}
            }
        }

        let result = handle.await.unwrap().unwrap();
        let snapshots: Vec<ContextSnapshot> = store
            .events(result.run_id)
            .into_iter()
            .filter_map(|event| match event.kind {
                RunEventKind::ContextBuilt { snapshot } => serde_json::from_value(snapshot).ok(),
                _ => None,
            })
            .collect();

        assert_eq!(snapshots.len(), 2);
        assert!(snapshots[1].conversation.iter().any(|message| matches!(
            message,
            Message::System { content }
                if content.contains("Mid-run user guidance")
                    && content.contains("prefer a concise final answer")
        )));
        assert!(snapshots[1].provenance.iter().any(|record| {
            record.fragment == "mid_run_guidance" && record.source == "run.trace.GuidanceInjected"
        }));
    }

    #[tokio::test]
    async fn run_stops_at_checkpoint_after_cancel_event() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "cancel_me".into(),
                input: json!({"text": "before stop"}),
            },
            FakeStep::Reply("should not run".into()),
        ]);
        let store = Arc::new(CapturingEventStore::new());
        let mut registry = ToolRegistry::new();
        registry.register(
            ToolDescriptor {
                id: ToolId::from("cancel_me"),
                name: "Cancel Me".into(),
                description: "Cancels its own run.".into(),
                categories: vec!["test".into()],
                input_schema: json!({"type": "object"}),
                output_interpretation_guidance: None,
                permissions: ToolPermissions::default(),
                requires_approval: false,
                provenance: None,
            },
            Arc::new(CancellingTool {
                store: store.clone(),
            }),
        );
        let h = Harness::new(Arc::new(provider), store.clone(), Arc::new(registry));

        let err = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            HarnessError::Cancelled(ref reason) if reason == "test stop"
        ));
        let run_id = store.latest_run_id().expect("run id should be captured");
        let events = store.events(run_id);
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::RunCancelled { reason } if reason == "test stop"
        )));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event.kind, RunEventKind::RunCompleted { .. }))
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.kind, RunEventKind::LlmRequestStarted { .. }))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn context_warns_model_when_tool_budget_is_low_or_exhausted() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"text": "ping"}),
            },
            FakeStep::Reply("done".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let agent = agent_with_tools(vec![], 1);

        let preview = h.preview_context(&agent, UserInput { text: "go".into() });
        assert!(
            preview
                .system_prompt
                .contains("tool-call budget warning: one tool call remains")
        );

        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let snapshots: Vec<ContextSnapshot> = h
            .events(result.run_id)
            .into_iter()
            .filter_map(|event| match event.kind {
                RunEventKind::ContextBuilt { snapshot } => serde_json::from_value(snapshot).ok(),
                _ => None,
            })
            .collect();

        assert_eq!(snapshots.len(), 2);
        assert!(
            snapshots[0]
                .system_prompt
                .contains("tool-call budget warning: one tool call remains")
        );
        assert!(
            snapshots[1]
                .system_prompt
                .contains("tool-call budget exhausted: do not call tools")
        );
    }

    #[tokio::test]
    async fn context_surfaces_tool_output_guidance_for_interpreted_mode() {
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let agent = agent_with_tools(vec![], 5);

        let snapshot = h.preview_context(&agent, UserInput { text: "go".into() });
        let echo_tool = snapshot
            .visible_tools
            .iter()
            .find(|tool| tool.id == "echo")
            .expect("echo tool should be visible");
        assert_eq!(echo_tool.output_mode, ToolOutputMode::Interpreted);
        assert_eq!(
            echo_tool.output_interpretation_guidance.as_deref(),
            Some("Return echoed text verbatim.")
        );
        assert!(snapshot.system_prompt.contains("<tool-output-guidance>"));
        assert!(
            snapshot
                .system_prompt
                .contains("<tool id=\"echo\">\nReturn echoed text verbatim.\n</tool>")
        );

        let request = h.llm_request_from_snapshot(&agent, &snapshot);
        let echo_schema = request
            .tools
            .iter()
            .find(|tool| tool.name == "echo")
            .expect("echo tool schema should be sent to model");
        assert!(
            echo_schema
                .description
                .contains("Output interpretation guidance: Return echoed text verbatim.")
        );

        let mut raw_agent = agent;
        raw_agent.tool_policy.output_mode = ToolOutputMode::Raw;
        let raw_snapshot = h.preview_context(&raw_agent, UserInput { text: "go".into() });
        assert!(
            !raw_snapshot
                .system_prompt
                .contains("<tool-output-guidance>")
        );
        assert!(
            raw_snapshot
                .system_prompt
                .contains("tool output mode: raw; after a tool call")
        );
        assert!(
            raw_snapshot
                .visible_tools
                .iter()
                .any(|tool| tool.id == "echo"
                    && tool.output_mode == ToolOutputMode::Raw
                    && tool.output_interpretation_guidance.is_none())
        );

        let mut guided_agent = agent_with_tools(vec![], 5);
        guided_agent.tool_policy.per_tool_output_guidance.insert(
            ToolId::from("echo"),
            "Summarize echoed text compactly.".into(),
        );
        let guided_snapshot = h.preview_context(&guided_agent, UserInput { text: "go".into() });
        assert!(
            guided_snapshot
                .system_prompt
                .contains("<tool id=\"echo\">\nSummarize echoed text compactly.\n</tool>")
        );
        let guided_request = h.llm_request_from_snapshot(&guided_agent, &guided_snapshot);
        let guided_schema = guided_request
            .tools
            .iter()
            .find(|tool| tool.name == "echo")
            .expect("echo tool schema should be sent to model");
        assert!(
            guided_schema
                .description
                .contains("Output interpretation guidance: Summarize echoed text compactly.")
        );
    }

    #[test]
    fn per_tool_visibility_overrides_global_tool_visibility() {
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.visibility = VisibilityLevel::NameOnly;
        agent
            .tool_policy
            .per_tool_visibility
            .insert(ToolId::from("echo"), VisibilityLevel::FullSchema);

        let snapshot = h.preview_context(&agent, UserInput { text: "go".into() });
        let echo_tool = snapshot
            .visible_tools
            .iter()
            .find(|tool| tool.id == "echo")
            .expect("echo tool should be visible");
        assert_eq!(echo_tool.visibility, VisibilityLevel::FullSchema);
        assert!(echo_tool.description.is_some());
        assert!(echo_tool.input_schema.is_some());

        let request = h.llm_request_from_snapshot(&agent, &snapshot);
        assert!(request.tools.iter().any(|tool| tool.name == "echo"));
    }

    #[tokio::test]
    async fn loaded_memory_and_ingestion_are_traced_after_context_build() {
        let mut agent = agent_with_tools(vec![], 5);
        agent.memory_backend = "test-memory-v1".into();
        agent.memory_fragments = vec![MemoryFragment {
            id: "mem-1".into(),
            content: "remember this".into(),
            provenance: "test".into(),
        }];
        agent.ingestion_artifacts = vec![IngestedArtifactView {
            id: "ing-1".into(),
            source: "/tmp/source.txt".into(),
            sections: 1,
            content: "source text".into(),
            findings: Vec::new(),
            provenance: "test".into(),
        }];
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let snapshot = h.preview_context(&agent, UserInput { text: "go".into() });
        assert!(snapshot.system_prompt.contains("<memory-context>"));
        assert!(snapshot.system_prompt.contains("remember this"));
        assert!(snapshot.system_prompt.contains("<ingestion-context>"));
        assert!(snapshot.system_prompt.contains("source text"));

        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let events = h.events(result.run_id);
        assert_eq!(
            kinds(&events),
            vec![
                "RunStarted",
                "ContextBuilt",
                "MemoryRead",
                "IngestionReferenced",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::MemoryRead {
                backend,
                fragment_ids,
            } if backend == "test-memory-v1" && fragment_ids == &vec!["mem-1".to_string()]
        )));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::IngestionReferenced {
                artifact_id,
                source,
            } if artifact_id == "ing-1" && source == "/tmp/source.txt"
        )));
    }

    #[test]
    fn context_builder_withholds_secret_like_stored_context() {
        let mut agent = agent_with_tools(vec![], 5);
        agent.memory_fragments = vec![
            MemoryFragment {
                id: "safe-mem".into(),
                content: "remember this".into(),
                provenance: "test".into(),
            },
            MemoryFragment {
                id: "secret-mem".into(),
                content: "api_key=sk-test-secret".into(),
                provenance: "test".into(),
            },
        ];
        agent.ingestion_artifacts = vec![IngestedArtifactView {
            id: "secret-ing".into(),
            source: "/tmp/source.txt".into(),
            sections: 1,
            content: "password: hidden".into(),
            findings: Vec::new(),
            provenance: "test".into(),
        }];
        agent.compacted_context = Some("token=hidden".into());
        let h = Harness::new(
            Arc::new(FakeProvider::echo()),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );

        let snapshot = h.preview_context(&agent, UserInput { text: "go".into() });

        assert!(snapshot.system_prompt.contains("remember this"));
        assert!(!snapshot.system_prompt.contains("sk-test-secret"));
        assert!(!snapshot.system_prompt.contains("password: hidden"));
        assert!(!snapshot.system_prompt.contains("token=hidden"));
        assert_eq!(
            snapshot
                .loaded_memory
                .iter()
                .map(|fragment| fragment.id.as_str())
                .collect::<Vec<_>>(),
            vec!["safe-mem"]
        );
        assert!(snapshot.loaded_artifacts.is_empty());
        assert_eq!(snapshot.compacted, None);
        assert!(snapshot.provenance.iter().any(|record| {
            record.fragment == "withheld_memory" && record.source == "secret-pattern guardrail"
        }));
        assert!(snapshot.provenance.iter().any(|record| {
            record.fragment == "withheld_artifacts" && record.source == "secret-pattern guardrail"
        }));
        assert!(snapshot.provenance.iter().any(|record| {
            record.fragment == "withheld_compacted_context"
                && record.source == "secret-pattern guardrail"
        }));
    }

    #[tokio::test]
    async fn tool_call_then_reply_emits_full_sequence() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"text": "ping"}),
            },
            FakeStep::Reply("done".into()),
        ]);

        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let r = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap();

        assert_eq!(r.final_output, "done");
        assert_eq!(
            kinds(&h.events(r.run_id)),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "ToolCallProposed",
                "ToolCallStarted",
                "ToolCallCompleted",
                "ToolOutputInterpreted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );
        assert!(h.events(r.run_id).iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolOutputInterpreted {
                call_id,
                model,
                summary,
            } if call_id == "c1" && model == "fake-model" && summary == r#"{"text":"ping"}"#
        )));
    }

    #[tokio::test]
    async fn tool_output_interpretation_model_controls_followup_llm_call() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"text": "ping"}),
            },
            FakeStep::Reply("interpreted".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.output_interpretation_model = Some(ModelRef::from("interp-model"));

        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let events = h.events(result.run_id);

        assert_eq!(result.final_output, "interpreted");
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolOutputInterpreted { model, .. } if model == "interp-model"
        )));
        let request_models = events
            .iter()
            .filter_map(|event| match &event.kind {
                RunEventKind::LlmRequestStarted { model, .. } => Some(model.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(request_models, vec!["fake-model", "interp-model"]);
    }

    #[tokio::test]
    async fn side_effect_free_tool_calls_can_run_in_parallel() {
        let active = Arc::new(AtomicUsize::new(0));
        let observed_parallel = Arc::new(AtomicBool::new(false));
        let mut registry = ToolRegistry::new();
        registry.register(
            probe_descriptor(),
            Arc::new(ProbeTool {
                active,
                observed_parallel: observed_parallel.clone(),
            }),
        );
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTools(vec![
                LlmToolCall {
                    id: "c1".into(),
                    tool_name: "probe".into(),
                    input: json!({"text": "one"}),
                },
                LlmToolCall {
                    id: "c2".into(),
                    tool_name: "probe".into(),
                    input: json!({"text": "two"}),
                },
            ]),
            FakeStep::Reply("done".into()),
        ]);

        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(registry),
        );
        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap();

        assert_eq!(result.final_output, "done");
        assert!(observed_parallel.load(Ordering::SeqCst));
        assert_eq!(
            kinds(&h.events(result.run_id)),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "ToolCallProposed",
                "ToolCallStarted",
                "ToolCallProposed",
                "ToolCallStarted",
                "ToolCallCompleted",
                "ToolOutputInterpreted",
                "ToolCallCompleted",
                "ToolOutputInterpreted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );
    }

    #[tokio::test]
    async fn raw_tool_output_mode_returns_without_interpretation_pass() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"text": "raw please"}),
            },
            FakeStep::Reply("should not be reached".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.output_mode = ToolOutputMode::Raw;
        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();

        assert_eq!(result.final_output, r#"{"text":"raw please"}"#);
        assert_eq!(
            kinds(&h.events(result.run_id)),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "ToolCallProposed",
                "ToolCallStarted",
                "ToolCallCompleted",
                "RunCompleted",
            ]
        );
    }

    #[tokio::test]
    async fn required_tool_policy_rejects_missing_tool_call() {
        let provider = FakeProvider::sequence(vec![FakeStep::Reply("no tool".into())]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.required_tool = Some(ToolId::from("echo"));

        let err = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap_err();

        assert!(
            matches!(err, HarnessError::PolicyDenied(reason) if reason.contains("required tool echo was not called"))
        );
    }

    #[tokio::test]
    async fn required_tool_policy_clears_after_required_call() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"text": "forced"}),
            },
            FakeStep::Reply("done".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.required_tool = Some(ToolId::from("echo"));

        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let snapshots = h
            .events(result.run_id)
            .iter()
            .filter_map(|event| match &event.kind {
                RunEventKind::ContextBuilt { snapshot } => Some(
                    serde_json::from_value::<ContextSnapshot>(snapshot.clone())
                        .expect("snapshot should deserialize"),
                ),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(result.final_output, "done");
        assert!(
            snapshots[0]
                .system_prompt
                .contains("required tool call: call `echo` exactly once")
        );
        assert!(
            !snapshots[1]
                .system_prompt
                .contains("required tool call: call `echo` exactly once")
        );
    }

    #[tokio::test]
    async fn per_tool_raw_output_override_returns_without_interpretation_pass() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({"text": "tool raw please"}),
            },
            FakeStep::Reply("should not be reached".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent
            .tool_policy
            .per_tool_output_modes
            .insert(ToolId::from("echo"), ToolOutputMode::Raw);

        let snapshot = h.preview_context(&agent, UserInput { text: "go".into() });
        assert!(
            snapshot
                .system_prompt
                .contains("raw tool output mode applies to: echo")
        );
        assert!(snapshot.visible_tools.iter().any(|tool| tool.id == "echo"
            && tool.output_mode == ToolOutputMode::Raw
            && tool.output_interpretation_guidance.is_none()));

        let result = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();

        assert_eq!(result.final_output, r#"{"text":"tool raw please"}"#);
        assert_eq!(
            kinds(&h.events(result.run_id)),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "ToolCallProposed",
                "ToolCallStarted",
                "ToolCallCompleted",
                "RunCompleted",
            ]
        );
    }

    #[tokio::test]
    async fn subagent_tool_emits_linked_child_run_events() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "subagent".into(),
                input: json!({"prompt": "child task", "agent_id": "worker"}),
            },
            FakeStep::Reply("child output".into()),
            FakeStep::Reply("parent output".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_subagent(),
        );
        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "delegate".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(result.final_output, "parent output");
        let parent_events = h.events(result.run_id);
        let child_run_id = parent_events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ChildRunStarted { child_run_id, .. } => Some(*child_run_id),
                _ => None,
            })
            .expect("parent trace should link child run");
        assert!(parent_events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::ChildRunCompleted {
                child_run_id: id,
                ref status,
            } if id == child_run_id && status == "succeeded"
        )));
        let child_run_id_text = child_run_id.0.to_string();
        assert!(parent_events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolCallCompleted { output, .. }
                if output["child_run_id"].as_str() == Some(child_run_id_text.as_str())
                    && output["agent_id"].as_str() == Some("worker")
                    && output["used_saved_agent_config"].as_bool() == Some(false)
                    && output["final_output"].as_str() == Some("child output")
        )));
        assert_eq!(
            kinds(&h.events(child_run_id)),
            vec![
                "RunStarted",
                "ContextBuilt",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );
    }

    #[tokio::test]
    async fn external_agent_tool_emits_child_run_link_without_rewriting_output() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "remote-1".into(),
                tool: "a2a-review".into(),
                input: json!({"prompt": "review this"}),
            },
            FakeStep::Reply("parent output".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_external_agent(
                "a2a-review",
                Arc::new(FakeTool::constant(json!({
                    "remote_run_id": "remote-42",
                    "status": "completed",
                    "message": "reviewed"
                }))),
            ),
        );

        let result = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "delegate remotely".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(result.final_output, "parent output");
        let parent_events = h.events(result.run_id);
        let child_run_id = parent_events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ChildRunStarted {
                    child_run_id,
                    agent_id,
                } if agent_id == "external-agent:a2a-review" => Some(*child_run_id),
                _ => None,
            })
            .expect("external-agent tool call should link a child run");
        assert!(parent_events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::ChildRunCompleted {
                child_run_id: id,
                ref status,
            } if id == child_run_id && status == "succeeded"
        )));
        assert!(parent_events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolCallCompleted { output, .. }
                if output["remote_run_id"].as_str() == Some("remote-42")
                    && output["status"].as_str() == Some("completed")
                    && output.get("child_run_id").is_none()
        )));
        assert!(h.events(child_run_id).is_empty());

        let tree = build_trace_tree(result.run_id, |run_id| Ok::<_, ()>(h.events(run_id))).unwrap();
        assert_eq!(tree.children.len(), 1);
        let child = &tree.children[0];
        assert_eq!(child.run_id, child_run_id);
        assert_eq!(child.agent_id.as_deref(), Some("external-agent:a2a-review"));
        assert_eq!(child.status, "succeeded");
        assert_eq!(child.link_status.as_deref(), Some("succeeded"));
        assert!(!child.trace_available);
    }

    #[tokio::test]
    async fn external_agent_tool_marks_child_run_link_failed_when_call_fails() {
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "remote-1".into(),
            tool: "a2a-review".into(),
            input: json!({"prompt": "review this"}),
        }]);
        let store = Arc::new(CapturingEventStore::new());
        let h = Harness::new(
            Arc::new(provider),
            store.clone(),
            registry_with_external_agent(
                "a2a-review",
                Arc::new(FakeTool::failing("remote agent unavailable")),
            ),
        );

        let err = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "delegate remotely".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("remote agent unavailable"));

        let run_id = store.latest_run_id().expect("run id should be captured");
        let parent_events = h.events(run_id);
        let child_run_id = parent_events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ChildRunStarted {
                    child_run_id,
                    agent_id,
                } if agent_id == "external-agent:a2a-review" => Some(*child_run_id),
                _ => None,
            })
            .expect("external-agent tool call should link a child run");
        assert!(parent_events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::ChildRunCompleted {
                child_run_id: id,
                ref status,
            } if id == child_run_id && status == "failed"
        )));
    }

    #[tokio::test]
    async fn subagent_tool_uses_saved_child_agent_config_when_available() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "subagent".into(),
                input: json!({"prompt": "child task", "agent_id": "worker"}),
            },
            FakeStep::Reply("child output".into()),
            FakeStep::Reply("parent output".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_subagent(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        let mut worker = agent_with_tools(vec![], 3);
        worker.id = "worker".into();
        worker.name = "Worker".into();
        worker.system_prompt = "Use the worker prompt.".into();
        agent.subagent_configs = vec![worker];

        let result = h
            .run(
                &agent,
                UserInput {
                    text: "delegate".into(),
                },
            )
            .await
            .unwrap();

        let parent_events = h.events(result.run_id);
        let child_run_id = parent_events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ChildRunStarted { child_run_id, .. } => Some(*child_run_id),
                _ => None,
            })
            .expect("parent trace should link child run");
        assert!(parent_events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolCallCompleted { output, .. }
                if output["used_saved_agent_config"].as_bool() == Some(true)
                    && output["agent_id"].as_str() == Some("worker")
        )));

        let child_context = h
            .events(child_run_id)
            .into_iter()
            .find_map(|event| match event.kind {
                RunEventKind::ContextBuilt { snapshot } => Some(snapshot),
                _ => None,
            })
            .expect("child run should build context");
        assert!(
            child_context["system_prompt"]
                .as_str()
                .is_some_and(|prompt| prompt.starts_with("Use the worker prompt."))
        );
    }

    #[tokio::test]
    async fn subagents_can_spawn_nested_subagents_within_depth_policy() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "root-child".into(),
                tool: "subagent".into(),
                input: json!({"prompt": "child task", "agent_id": "worker"}),
            },
            FakeStep::CallTool {
                id: "child-grandchild".into(),
                tool: "subagent".into(),
                input: json!({"prompt": "grandchild task", "agent_id": "leaf"}),
            },
            FakeStep::Reply("grandchild output".into()),
            FakeStep::Reply("child output".into()),
            FakeStep::Reply("parent output".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_subagent(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.execution_policy.max_subagent_depth = 2;

        let result = h
            .run(
                &agent,
                UserInput {
                    text: "delegate twice".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(result.final_output, "parent output");
        let parent_events = h.events(result.run_id);
        let child_run_id = parent_events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ChildRunStarted { child_run_id, .. } => Some(*child_run_id),
                _ => None,
            })
            .expect("parent trace should link child run");
        let child_events = h.events(child_run_id);
        let grandchild_run_id = child_events
            .iter()
            .find_map(|event| match &event.kind {
                RunEventKind::ChildRunStarted { child_run_id, .. } => Some(*child_run_id),
                _ => None,
            })
            .expect("child trace should link grandchild run");
        assert!(child_events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::ChildRunCompleted {
                child_run_id: id,
                ref status,
            } if id == grandchild_run_id && status == "succeeded"
        )));
        assert!(h.events(grandchild_run_id).iter().any(|event| matches!(
            &event.kind,
            RunEventKind::RunCompleted { final_output, .. } if final_output == "grandchild output"
        )));
    }

    #[tokio::test]
    async fn subagent_depth_policy_denies_nested_child_runs_by_default() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "root-child".into(),
                tool: "subagent".into(),
                input: json!({"prompt": "child task", "agent_id": "worker"}),
            },
            FakeStep::CallTool {
                id: "child-grandchild".into(),
                tool: "subagent".into(),
                input: json!({"prompt": "grandchild task", "agent_id": "leaf"}),
            },
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_subagent(),
        );

        let err = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "delegate too far".into(),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            HarnessError::PolicyDenied(ref reason)
                if reason.contains("subagent depth limit reached")
        ));
    }

    #[tokio::test]
    async fn subagent_recursion_policy_denies_agent_cycles_by_default() {
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "self-child".into(),
            tool: "subagent".into(),
            input: json!({"prompt": "recursive task", "agent_id": "fake-agent"}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_subagent(),
        );

        let err = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput {
                    text: "delegate to self".into(),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            HarnessError::PolicyDenied(ref reason)
                if reason.contains("subagent recursion denied")
        ));
    }

    #[tokio::test]
    async fn subagent_recursion_policy_allows_bounded_self_call() {
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "self-child".into(),
                tool: "subagent".into(),
                input: json!({"prompt": "recursive task", "agent_id": "fake-agent"}),
            },
            FakeStep::Reply("child output".into()),
            FakeStep::Reply("parent output".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_subagent(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.execution_policy.max_recursion_depth = 1;

        let result = h
            .run(
                &agent,
                UserInput {
                    text: "delegate to self once".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(result.final_output, "parent output");
        assert!(h.events(result.run_id).iter().any(|event| matches!(
            &event.kind,
            RunEventKind::ToolCallCompleted { output, .. }
                if output["agent_id"].as_str() == Some("fake-agent")
                    && output["final_output"].as_str() == Some("child output")
        )));
    }

    #[tokio::test]
    async fn budget_exhaustion_fails_the_run() {
        // Script three tool calls but only allow two.
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "echo".into(),
                input: json!({}),
            },
            FakeStep::CallTool {
                id: "c2".into(),
                tool: "echo".into(),
                input: json!({}),
            },
            FakeStep::CallTool {
                id: "c3".into(),
                tool: "echo".into(),
                input: json!({}),
            },
            FakeStep::Reply("never reached".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let err = h
            .run(
                &agent_with_tools(vec![], 2),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, HarnessError::BudgetExhausted(_)));
    }

    #[tokio::test]
    async fn allowlist_denies_unlisted_tool() {
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "shell".into(),
            input: json!({}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_echo(),
        );
        let err = h
            .run(
                &agent_with_tools(vec![ToolId::from("echo")], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, HarnessError::PolicyDenied(_)));
    }

    #[tokio::test]
    async fn tool_execution_failure_propagates_and_is_recorded() {
        let mut reg = ToolRegistry::new();
        reg.register(echo_descriptor(), Arc::new(FakeTool::failing("boom")));
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "echo".into(),
            input: json!({}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(reg),
        );
        let err = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            HarnessError::Tool(agent_tools::ToolError::Execution(_))
        ));
    }

    #[tokio::test]
    async fn approval_required_tool_emits_approval_events() {
        let mut reg = ToolRegistry::new();
        reg.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));
        let provider = FakeProvider::sequence(vec![
            FakeStep::CallTool {
                id: "c1".into(),
                tool: "sensitive".into(),
                input: json!({}),
            },
            FakeStep::Reply("done".into()),
        ]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(reg),
        );
        let r = h
            .run(
                &agent_with_tools(vec![], 5),
                UserInput { text: "go".into() },
            )
            .await
            .unwrap();

        let events = h.events(r.run_id);
        assert!(matches!(
            events.iter().find(|event| matches!(event.kind, RunEventKind::ToolCallProposed { .. })).map(|event| &event.kind),
            Some(RunEventKind::ToolCallProposed {
                model,
                permissions: Some(permissions),
                ..
            }) if model.as_deref() == Some("fake-model")
                && permissions["shell"] == true
                && permissions["approval_required"] == true
                && permissions["sandbox"]["overall_level"] == "advisory"
                && permissions["sandbox"]["warnings"].as_array().is_some_and(|warnings| !warnings.is_empty())
        ));
        assert!(matches!(
            events.iter().find(|event| matches!(event.kind, RunEventKind::ApprovalRequested { .. })).map(|event| &event.kind),
            Some(RunEventKind::ApprovalRequested { action, reason, .. })
                if action == "tool:sensitive" && reason.contains("shell")
        ));
        assert!(events.iter().any(|event| matches!(
            event.kind,
            RunEventKind::ApprovalResolved { approved: true, .. }
        )));
    }

    #[tokio::test]
    async fn scoped_approval_controller_is_advertised_and_verified() {
        let mut reg = ToolRegistry::new();
        reg.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "sensitive".into(),
            input: json!({}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(reg),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.approval_controller =
            ApprovalControllerPolicy::new("controller-agent", Vec::new(), vec!["sensitive".into()]);

        let r = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let events = h.events(r.run_id);

        assert!(matches!(
            events.iter().find(|event| matches!(event.kind, RunEventKind::ApprovalRequested { .. })).map(|event| &event.kind),
            Some(RunEventKind::ApprovalRequested {
                approval_id,
                controller_agent: Some(controller),
                controller_scope,
                ..
            }) if approval_id == "approval-c1"
                && controller == "controller-agent"
                && controller_scope == &vec!["category:sensitive".to_string()]
        ));
        assert_eq!(
            verify_approval_controller_delegate(&events, "approval-c1", Some("controller-agent"),),
            Ok(Some("controller-agent".into()))
        );
        let assessment =
            assess_approval_controller_delegate(&events, "approval-c1", Some("controller-agent"))
                .unwrap()
                .unwrap();
        assert_eq!(assessment.status, "scope_verified");
        assert_eq!(assessment.controller_agent, "controller-agent");
        assert_eq!(assessment.scope, vec!["category:sensitive".to_string()]);
        assert!(matches!(
            verify_approval_controller_delegate(&events, "approval-c1", Some("other")),
            Err(ApprovalControllerError::WrongController { .. })
        ));
    }

    #[tokio::test]
    async fn delegated_approval_controller_runs_model_assessment() {
        let mut reg = ToolRegistry::new();
        reg.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "sensitive".into(),
            input: json!({"value": "inspect"}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(reg),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.approval_controller =
            ApprovalControllerPolicy::new("controller-agent", Vec::new(), vec!["sensitive".into()]);
        let controller = AgentConfig {
            id: "controller-agent".into(),
            name: "Controller".into(),
            system_prompt: "Assess approvals conservatively.".into(),
            ..agent_with_tools(vec![], 1)
        };

        let r = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let events = h.events(r.run_id);
        let assessment = assess_approval_controller_with_model(
            &FakeProvider::canned("APPROVE: scope and tool input match the policy."),
            &controller,
            &events,
            "approval-c1",
            None,
        )
        .await
        .unwrap();

        assert_eq!(assessment.status, "model_assessed");
        assert_eq!(assessment.controller_agent, "controller-agent");
        assert_eq!(assessment.recommendation.as_deref(), Some("approve"));
        assert_eq!(assessment.model.as_deref(), Some("fake-model"));
        assert_eq!(assessment.scope, vec!["category:sensitive".to_string()]);
        assert!(
            assessment
                .reason
                .contains("controller model recommended approve")
        );
        assert!(assessment.tokens_in > 0);
    }

    #[tokio::test]
    async fn approval_controller_requires_declared_tool_scope() {
        let mut reg = ToolRegistry::new();
        reg.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "sensitive".into(),
            input: json!({}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(reg),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.approval_controller =
            ApprovalControllerPolicy::new("controller-agent", Vec::new(), Vec::new());

        let r = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap();
        let events = h.events(r.run_id);

        assert!(matches!(
            events.iter().find(|event| matches!(event.kind, RunEventKind::ApprovalRequested { .. })).map(|event| &event.kind),
            Some(RunEventKind::ApprovalRequested {
                controller_agent: None,
                controller_scope,
                ..
            }) if controller_scope.is_empty()
        ));
        assert!(matches!(
            verify_approval_controller_delegate(&events, "approval-c1", Some("controller-agent"),),
            Err(ApprovalControllerError::NotDelegated { .. })
        ));
    }

    #[tokio::test]
    async fn explicit_approval_mode_pauses_before_tool_execution() {
        let mut reg = ToolRegistry::new();
        reg.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "sensitive".into(),
            input: json!({}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(reg),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.approval_mode = ApprovalMode::RequireExplicit;
        let err = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            HarnessError::ApprovalRequired {
                ref approval_id,
                ..
            } if approval_id == "approval-c1"
        ));
        let run_id = match err {
            HarnessError::ApprovalRequired { run_id, .. } => run_id,
            other => panic!("unexpected error: {other:?}"),
        };
        let events = h.events(run_id);
        assert!(
            events
                .iter()
                .any(|event| matches!(event.kind, RunEventKind::RunPaused { .. }))
        );
        assert!(
            !events
                .iter()
                .any(|event| matches!(event.kind, RunEventKind::ToolCallStarted { .. }))
        );
    }

    #[tokio::test]
    async fn sensitive_permissions_pause_even_without_approval_flag() {
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "secret_echo".into(),
            input: json!({}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            registry_with_secret_tool(),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.approval_mode = ApprovalMode::RequireExplicit;
        let err = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap_err();

        let run_id = match err {
            HarnessError::ApprovalRequired {
                run_id, ref action, ..
            } if action == "tool:secret_echo" => run_id,
            other => panic!("unexpected error: {other:?}"),
        };
        let events = h.events(run_id);
        assert!(matches!(
            events.iter().find(|event| matches!(event.kind, RunEventKind::ApprovalRequested { .. })).map(|event| &event.kind),
            Some(RunEventKind::ApprovalRequested { reason, .. })
                if reason.contains("secrets")
        ));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event.kind, RunEventKind::ToolCallStarted { .. }))
        );
    }

    #[tokio::test]
    async fn approval_required_tools_pause_by_default() {
        let mut reg = ToolRegistry::new();
        reg.register(sensitive_descriptor(), Arc::new(FakeTool::echo()));
        let provider = FakeProvider::sequence(vec![FakeStep::CallTool {
            id: "c1".into(),
            tool: "sensitive".into(),
            input: json!({}),
        }]);
        let h = Harness::new(
            Arc::new(provider),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(reg),
        );
        let mut agent = agent_with_tools(vec![], 5);
        agent.tool_policy.approval_mode = ToolPolicy::default().approval_mode;
        let err = h
            .run(&agent, UserInput { text: "go".into() })
            .await
            .unwrap_err();

        assert!(matches!(err, HarnessError::ApprovalRequired { .. }));
    }

    #[tokio::test]
    async fn provider_error_is_recorded_then_returned() {
        struct AlwaysFails;
        #[async_trait]
        impl LlmProvider for AlwaysFails {
            async fn complete(&self, _req: LlmRequest) -> Result<LlmResponse, agent_llm::LlmError> {
                Err(agent_llm::LlmError::Provider("nope".into()))
            }
        }
        let h = Harness::new(
            Arc::new(AlwaysFails),
            Arc::new(InMemoryEventStore::new()),
            Arc::new(ToolRegistry::new()),
        );
        let err = h
            .run(&agent_with_tools(vec![], 5), UserInput { text: "x".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, HarnessError::Llm(_)));
    }
}
