//! `agent-core` — domain model + orchestration. Defines the `HarnessApi` trait
//! that every UI client (TUI, Tauri webapp, future daemon) calls.
//!
//! v0 surface: a tools-enabled run loop plus inspectable context snapshots.
//! The same context builder powers `preview_context` and each LLM request. The
//! LLM may propose tool calls; the runtime enforces an allowlist and a
//! max-calls budget, executes via the `ToolRegistry`, feeds results back to the
//! LLM, and iterates until the LLM stops calling tools or the budget runs out.
//! Memory, ingestion, approvals, batch, and subagent tracing now land in early
//! slices; hooks, streaming tokens, and sandboxing continue to follow
//! `specs/architecture.md` §21.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use agent_llm::{LlmProvider, LlmRequest, Message, ModelRef, ToolSchema};
use agent_tools::{ToolId, ToolRegistry};
use agent_tracing::{EventId, EventStore, RunEvent, RunEventKind, RunId};

/// Tool-related policy slice. v0 cut of `specs/architecture.md` §4.5
/// `ToolPolicy`. Visibility levels and per-tool overrides land later.
#[derive(Debug, Clone)]
pub struct ToolPolicy {
    /// Hard cap on the number of tool calls in a single run.
    pub max_calls: u32,
    /// Allowlist by tool id. Empty means "every registered tool is allowed."
    pub allowed_tools: Vec<ToolId>,
    /// How much detail the model/user context sees for registered tools.
    pub visibility: VisibilityLevel,
    /// How approval-required tools are handled.
    pub approval_mode: ApprovalMode,
    /// Whether tool outputs are returned raw or fed back to the LLM.
    pub output_mode: ToolOutputMode,
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self {
            max_calls: 5,
            allowed_tools: Vec::new(),
            visibility: VisibilityLevel::FullSchema,
            approval_mode: ApprovalMode::AutoApprove,
            output_mode: ToolOutputMode::Interpreted,
        }
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

/// User/config-provided model pricing for trace cost estimates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostPolicy {
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
}

/// Minimal `AgentConfig` — v0 cut of `specs/architecture.md` §4.5.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub model: ModelRef,
    pub prompt_refinement: Option<PromptRefinement>,
    pub tool_policy: ToolPolicy,
    pub cost_policy: CostPolicy,
    pub memory_fragments: Vec<MemoryFragment>,
    pub ingestion_artifacts: Vec<IngestedArtifactView>,
    pub skill_views: Vec<SkillView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptRefinement {
    pub instructions: String,
    pub model: Option<ModelRef>,
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
    pub loaded_memory: Vec<MemoryFragment>,
    pub loaded_artifacts: Vec<IngestedArtifactView>,
    pub visible_tools: Vec<ToolView>,
    pub visible_skills: Vec<SkillView>,
    pub limits: RuntimeLimits,
    pub provenance: Vec<ProvenanceRecord>,
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
    pub input_schema: Option<Value>,
    pub visibility: VisibilityLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillView {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub visibility: VisibilityLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityLevel {
    FullSchema,
    NameAndDescription,
    NameOnly,
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
        }
    }

    fn initial_conversation(&self, input: &UserInput) -> Vec<Message> {
        vec![Message::user(&input.text)]
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
        ContextBuilder {
            agent,
            tools: &self.tools,
            conversation,
            calls_used,
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
                    description: t.description.clone().unwrap_or_default(),
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
                self.events.append(
                    run_id,
                    Some(started.id),
                    RunEventKind::RunFailed {
                        reason: err.to_string(),
                    },
                );
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
        if !agent.tool_policy.allowed_tools.is_empty()
            && !agent.tool_policy.allowed_tools.contains(tool_id)
        {
            return Err(HarnessError::PolicyDenied(format!(
                "tool {:?} not in agent allowlist",
                tool_id
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
        if !descriptor.requires_approval {
            return Ok(());
        }
        let approval_id = format!("approval-{call_id}");
        let action = format!("tool:{}", tool_id.0);
        let reason = permission_reason(descriptor);
        let requested = self.events.append(
            run_id,
            Some(parent),
            RunEventKind::ApprovalRequested {
                approval_id: approval_id.clone(),
                action: action.clone(),
                reason,
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
        run_id: RunId,
        parent: EventId,
        snapshot: &ContextSnapshot,
    ) {
        if !snapshot.loaded_memory.is_empty() {
            self.events.append(
                run_id,
                Some(parent),
                RunEventKind::MemoryLoaded {
                    ids: snapshot
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

    async fn execute_tool_or_subagent(
        &self,
        agent: &AgentConfig,
        parent_run_id: RunId,
        parent_event: EventId,
        tool_id: &ToolId,
        input: Value,
    ) -> Result<ToolExecutionResult, HarnessError> {
        if tool_id.0 == "subagent" {
            return self
                .execute_subagent(agent, parent_run_id, parent_event, tool_id, input)
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

    async fn execute_subagent(
        &self,
        agent: &AgentConfig,
        parent_run_id: RunId,
        parent_event: EventId,
        tool_id: &ToolId,
        input: Value,
    ) -> Result<ToolExecutionResult, HarnessError> {
        if !self.tools.contains(tool_id) {
            return Err(agent_tools::ToolError::NotFound(tool_id.clone()).into());
        }

        let (prompt, requested_agent_id) = parse_subagent_input(&input)?;
        let child_run_id = RunId::new();
        let child_agent_id = requested_agent_id.unwrap_or_else(|| format!("{}:subagent", agent.id));
        let child_link = self.events.append(
            parent_run_id,
            Some(parent_event),
            RunEventKind::ChildRunStarted {
                child_run_id,
                agent_id: child_agent_id.clone(),
            },
        );

        let mut child_agent = agent.clone();
        child_agent.id = child_agent_id.clone();
        child_agent.name = format!("{} Subagent", agent.name);
        child_agent.tool_policy.max_calls = 0;
        child_agent.tool_policy.allowed_tools = vec![ToolId::from("__no_tools__")];

        let started_at = Instant::now();
        let child_started = self.events.append(
            child_run_id,
            Some(child_link.id),
            RunEventKind::RunStarted {
                agent_id: child_agent_id.clone(),
                input: prompt.clone(),
            },
        );
        let conversation = vec![Message::user(&prompt)];
        let snapshot = self.build_context_snapshot(&child_agent, conversation, 0);
        let context_built = self.events.append(
            child_run_id,
            Some(child_started.id),
            RunEventKind::ContextBuilt {
                snapshot: serde_json::to_value(&snapshot).unwrap_or(Value::Null),
            },
        );
        self.record_context_references(child_run_id, context_built.id, &snapshot);
        let req = self.llm_request_from_snapshot(&child_agent, &snapshot);
        let llm_started = self.events.append(
            child_run_id,
            Some(context_built.id),
            RunEventKind::LlmRequestStarted {
                model: child_agent.model.0.clone(),
            },
        );

        let llm_t0 = Instant::now();
        let response = match self.provider.complete(req).await {
            Ok(response) => response,
            Err(err) => {
                let reason = err.to_string();
                self.events.append(
                    child_run_id,
                    Some(llm_started.id),
                    RunEventKind::RunFailed {
                        reason: reason.clone(),
                    },
                );
                self.events.append(
                    parent_run_id,
                    Some(child_link.id),
                    RunEventKind::ChildRunCompleted {
                        child_run_id,
                        status: "failed".into(),
                    },
                );
                return Err(err.into());
            }
        };
        let llm_duration = llm_t0.elapsed().as_millis() as u64;
        let child_llm_cost_usd =
            self.llm_cost_usd(&child_agent, response.tokens_in, response.tokens_out);
        self.events.append(
            child_run_id,
            Some(llm_started.id),
            RunEventKind::LlmRequestCompleted {
                tokens_in: response.tokens_in,
                tokens_out: response.tokens_out,
                cost_usd: child_llm_cost_usd,
                duration_ms: llm_duration,
            },
        );

        if !response.tool_calls.is_empty() {
            let reason = "subagent tool calls are disabled in this v0 runtime slice".to_string();
            self.record_policy_denied(child_run_id, child_started.id, &reason);
            self.events.append(
                child_run_id,
                Some(child_started.id),
                RunEventKind::RunFailed {
                    reason: reason.clone(),
                },
            );
            self.events.append(
                parent_run_id,
                Some(child_link.id),
                RunEventKind::ChildRunCompleted {
                    child_run_id,
                    status: "failed".into(),
                },
            );
            return Err(HarnessError::PolicyDenied(reason));
        }

        let final_output = response.content.unwrap_or_default();
        self.events.append(
            child_run_id,
            Some(child_started.id),
            RunEventKind::RunCompleted {
                final_output: final_output.clone(),
                total_cost_usd: child_llm_cost_usd,
                total_duration_ms: started_at.elapsed().as_millis() as u64,
            },
        );
        self.events.append(
            parent_run_id,
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
            "final_output": final_output,
            "total_cost_usd": child_llm_cost_usd
            }),
            cost_usd: child_llm_cost_usd,
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
    if reasons.is_empty() {
        "tool marked approval-required".into()
    } else {
        format!("sensitive permissions: {}", reasons.join(","))
    }
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
}

impl ContextBuilder<'_> {
    fn build(self) -> ContextSnapshot {
        let max_tool_calls = self.agent.tool_policy.max_calls;
        let remaining_tool_calls = max_tool_calls.saturating_sub(self.calls_used);
        let mut visible_tools: Vec<ToolView> = self
            .tools
            .descriptors()
            .filter(|d| {
                self.agent.tool_policy.allowed_tools.is_empty()
                    || self.agent.tool_policy.allowed_tools.contains(&d.id)
            })
            .map(|d| ToolView {
                id: d.id.0.clone(),
                name: d.name.clone(),
                description: match self.agent.tool_policy.visibility {
                    VisibilityLevel::FullSchema | VisibilityLevel::NameAndDescription => {
                        Some(d.description.clone())
                    }
                    VisibilityLevel::NameOnly => None,
                },
                input_schema: match self.agent.tool_policy.visibility {
                    VisibilityLevel::FullSchema => Some(d.input_schema.clone()),
                    VisibilityLevel::NameAndDescription | VisibilityLevel::NameOnly => None,
                },
                visibility: self.agent.tool_policy.visibility,
            })
            .collect();
        visible_tools.sort_by(|a, b| a.id.cmp(&b.id));

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
        if self.agent.tool_policy.output_mode == ToolOutputMode::Raw {
            system_prompt.push_str(
                "\n- tool output mode: raw; after a tool call, the runtime returns the tool output without an interpretation pass",
            );
        }
        if !self.agent.memory_fragments.is_empty() {
            system_prompt.push_str("\n\n<memory-context>");
            for fragment in &self.agent.memory_fragments {
                system_prompt.push_str(&format!(
                    "\n<memory id=\"{}\" provenance=\"{}\">\n{}\n</memory>",
                    fragment.id, fragment.provenance, fragment.content
                ));
            }
            system_prompt.push_str("\n</memory-context>");
        }
        if !self.agent.ingestion_artifacts.is_empty() {
            system_prompt.push_str("\n\n<ingestion-context>");
            for artifact in &self.agent.ingestion_artifacts {
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
        if !self.agent.skill_views.is_empty() {
            system_prompt.push_str("\n\n<skill-context>");
            for skill in &self.agent.skill_views {
                let description = skill.description.as_deref().unwrap_or("");
                system_prompt.push_str(&format!(
                    "\n<skill id=\"{}\" visibility=\"{:?}\">\n{}\n</skill>",
                    skill.id, skill.visibility, description
                ));
            }
            system_prompt.push_str("\n</skill-context>");
        }

        ContextSnapshot {
            system_prompt,
            conversation: self.conversation,
            compacted: None,
            loaded_memory: self.agent.memory_fragments.clone(),
            loaded_artifacts: self.agent.ingestion_artifacts.clone(),
            visible_tools,
            visible_skills: self.agent.skill_views.clone(),
            limits: RuntimeLimits {
                max_tool_calls,
                remaining_tool_calls,
            },
            provenance: vec![
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
            ],
        }
    }
}

#[async_trait]
impl HarnessApi for Harness {
    async fn run(&self, agent: &AgentConfig, input: UserInput) -> Result<RunResult, HarnessError> {
        let run_id = RunId::new();
        let started_at = Instant::now();

        let run_started = self.events.append(
            run_id,
            None,
            RunEventKind::RunStarted {
                agent_id: agent.id.clone(),
                input: input.text.clone(),
            },
        );

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
        let mut conversation = self.initial_conversation(&UserInput {
            text: refined_input,
        });

        loop {
            let snapshot = self.build_context_snapshot(agent, conversation.clone(), calls_used);
            let context_built = self.events.append(
                run_id,
                Some(run_started.id),
                RunEventKind::ContextBuilt {
                    snapshot: serde_json::to_value(&snapshot).unwrap_or(Value::Null),
                },
            );
            self.record_context_references(run_id, context_built.id, &snapshot);
            let req = self.llm_request_from_snapshot(agent, &snapshot);

            let llm_started = self.events.append(
                run_id,
                Some(context_built.id),
                RunEventKind::LlmRequestStarted {
                    model: agent.model.0.clone(),
                },
            );

            let llm_t0 = Instant::now();
            let response = match self.provider.complete(req).await {
                Ok(r) => r,
                Err(e) => {
                    self.events.append(
                        run_id,
                        Some(llm_started.id),
                        RunEventKind::RunFailed {
                            reason: e.to_string(),
                        },
                    );
                    return Err(e.into());
                }
            };
            let llm_duration = llm_t0.elapsed().as_millis() as u64;
            let llm_cost_usd = self.llm_cost_usd(agent, response.tokens_in, response.tokens_out);
            if let Some(cost) = llm_cost_usd {
                total_cost_usd += cost;
                has_cost_usd = true;
            }

            self.events.append(
                run_id,
                Some(llm_started.id),
                RunEventKind::LlmRequestCompleted {
                    tokens_in: response.tokens_in,
                    tokens_out: response.tokens_out,
                    cost_usd: llm_cost_usd,
                    duration_ms: llm_duration,
                },
            );

            // No tool calls -> final answer.
            if response.tool_calls.is_empty() {
                let final_output = response.content.unwrap_or_default();
                conversation.push(Message::Assistant {
                    content: Some(final_output.clone()),
                    tool_calls: vec![],
                });

                let total_duration_ms = started_at.elapsed().as_millis() as u64;
                self.events.append(
                    run_id,
                    Some(run_started.id),
                    RunEventKind::RunCompleted {
                        final_output: final_output.clone(),
                        total_cost_usd: has_cost_usd.then_some(total_cost_usd),
                        total_duration_ms,
                    },
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

            for tc in response.tool_calls {
                let tool_id = ToolId::from(tc.tool_name.clone());

                let proposed = self.events.append(
                    run_id,
                    Some(run_started.id),
                    RunEventKind::ToolCallProposed {
                        call_id: tc.id.clone(),
                        tool_id: tc.tool_name.clone(),
                        input: tc.input.clone(),
                    },
                );

                // Allowlist check.
                if !agent.tool_policy.allowed_tools.is_empty()
                    && !agent.tool_policy.allowed_tools.contains(&tool_id)
                {
                    let reason = format!("tool {:?} not in agent allowlist", tool_id);
                    self.record_policy_denied(run_id, proposed.id, &reason);
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
                        RunEventKind::RunFailed {
                            reason: reason.clone(),
                        },
                    );
                    return Err(HarnessError::PolicyDenied(reason));
                }

                // Budget check (per-call, before execution).
                if calls_used >= agent.tool_policy.max_calls {
                    let reason = format!(
                        "tool-call budget exhausted (used={}, limit={})",
                        calls_used, agent.tool_policy.max_calls,
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
                    self.events.append(
                        run_id,
                        Some(run_started.id),
                        RunEventKind::RunFailed {
                            reason: reason.clone(),
                        },
                    );
                    return Err(HarnessError::BudgetExhausted(reason));
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

                let tool_t0 = Instant::now();
                let execution = match self
                    .execute_tool_or_subagent(
                        agent,
                        run_id,
                        tool_started.id,
                        &tool_id,
                        tc.input.clone(),
                    )
                    .await
                {
                    Ok(v) => v,
                    Err(e) => {
                        let err_msg = e.to_string();
                        self.events.append(
                            run_id,
                            Some(proposed.id),
                            RunEventKind::ToolCallFailed {
                                call_id: tc.id.clone(),
                                error: err_msg.clone(),
                            },
                        );
                        self.events.append(
                            run_id,
                            Some(run_started.id),
                            RunEventKind::RunFailed {
                                reason: err_msg.clone(),
                            },
                        );
                        return Err(e);
                    }
                };
                if let Some(cost) = execution.cost_usd {
                    total_cost_usd += cost;
                    has_cost_usd = true;
                }
                let output = execution.output;
                let tool_duration = tool_t0.elapsed().as_millis() as u64;

                self.events.append(
                    run_id,
                    Some(proposed.id),
                    RunEventKind::ToolCallCompleted {
                        call_id: tc.id.clone(),
                        output: output.clone(),
                        cost_usd: execution.cost_usd,
                        duration_ms: tool_duration,
                    },
                );

                let result_str = match &output {
                    serde_json::Value::String(s) => s.clone(),
                    other => {
                        serde_json::to_string(other).unwrap_or_else(|_| "<unserializable>".into())
                    }
                };
                conversation.push(Message::ToolResult {
                    tool_call_id: tc.id,
                    content: result_str,
                });

                calls_used += 1;
                if agent.tool_policy.output_mode == ToolOutputMode::Raw {
                    let final_output = Self::stringify_tool_output(&output);
                    self.events.append(
                        run_id,
                        Some(run_started.id),
                        RunEventKind::RunCompleted {
                            final_output: final_output.clone(),
                            total_cost_usd: has_cost_usd.then_some(total_cost_usd),
                            total_duration_ms: started_at.elapsed().as_millis() as u64,
                        },
                    );
                    return Ok(RunResult {
                        run_id,
                        final_output,
                    });
                }
            }
        }
    }

    async fn call_tool(
        &self,
        agent: &AgentConfig,
        tool_id: ToolId,
        input: Value,
    ) -> Result<ToolCallResult, HarnessError> {
        let run_id = RunId::new();
        let started_at = Instant::now();
        let call_id = "manual-1".to_string();

        let run_started = self.events.append(
            run_id,
            None,
            RunEventKind::RunStarted {
                agent_id: agent.id.clone(),
                input: format!("/tool! {} {}", tool_id.0, input),
            },
        );

        let proposed = self.events.append(
            run_id,
            Some(run_started.id),
            RunEventKind::ToolCallProposed {
                call_id: call_id.clone(),
                tool_id: tool_id.0.clone(),
                input: input.clone(),
            },
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
            self.events.append(
                run_id,
                Some(run_started.id),
                RunEventKind::RunFailed {
                    reason: reason.clone(),
                },
            );
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
            self.events.append(
                run_id,
                Some(run_started.id),
                RunEventKind::RunFailed {
                    reason: reason.clone(),
                },
            );
            return Err(e);
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
            .execute_tool_or_subagent(agent, run_id, tool_started.id, &tool_id, input)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                let err_msg = e.to_string();
                self.events.append(
                    run_id,
                    Some(proposed.id),
                    RunEventKind::ToolCallFailed {
                        call_id,
                        error: err_msg.clone(),
                    },
                );
                self.events.append(
                    run_id,
                    Some(run_started.id),
                    RunEventKind::RunFailed {
                        reason: err_msg.clone(),
                    },
                );
                return Err(e);
            }
        };
        let output = execution.output;
        let duration_ms = tool_t0.elapsed().as_millis() as u64;

        self.events.append(
            run_id,
            Some(proposed.id),
            RunEventKind::ToolCallCompleted {
                call_id,
                output: output.clone(),
                cost_usd: execution.cost_usd,
                duration_ms,
            },
        );
        self.events.append(
            run_id,
            Some(run_started.id),
            RunEventKind::RunCompleted {
                final_output: Self::stringify_tool_output(&output),
                total_cost_usd: execution.cost_usd,
                total_duration_ms: started_at.elapsed().as_millis() as u64,
            },
        );

        Ok(ToolCallResult {
            run_id,
            output,
            duration_ms,
        })
    }

    fn preview_context(&self, agent: &AgentConfig, input: UserInput) -> ContextSnapshot {
        self.build_context_snapshot(agent, self.initial_conversation(&input), 0)
    }

    fn explain_config(&self, agent: &AgentConfig) -> ConfigExplanation {
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
                    key: "agent.tool_policy.allowed_tools".into(),
                    value: serde_json::to_value(&agent.tool_policy.allowed_tools)
                        .unwrap_or(Value::Null),
                    source: "agent/default".into(),
                },
                ConfigValueExplanation {
                    key: "agent.tool_policy.output_mode".into(),
                    value: serde_json::to_value(agent.tool_policy.output_mode)
                        .unwrap_or(Value::Null),
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
    use agent_llm::{FakeProvider, FakeStep, LlmResponse};
    use agent_tools::{FakeTool, SubagentTool, ToolDescriptor, ToolPermissions};
    use agent_tracing::InMemoryEventStore;
    use serde_json::json;

    fn agent_with_tools(allowed: Vec<ToolId>, max_calls: u32) -> AgentConfig {
        AgentConfig {
            id: "fake-agent".into(),
            name: "Fake".into(),
            system_prompt: "be brief".into(),
            model: ModelRef::from("fake-model"),
            prompt_refinement: None,
            tool_policy: ToolPolicy {
                max_calls,
                allowed_tools: allowed,
                visibility: VisibilityLevel::FullSchema,
                approval_mode: ApprovalMode::AutoApprove,
                output_mode: ToolOutputMode::Interpreted,
            },
            cost_policy: CostPolicy::default(),
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            skill_views: Vec::new(),
        }
    }

    fn echo_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("echo"),
            name: "Echo".into(),
            description: "Returns its input unchanged.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            }),
            permissions: ToolPermissions::default(),
            requires_approval: false,
        }
    }

    fn sensitive_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("sensitive"),
            name: "Sensitive".into(),
            description: "Requires approval.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                }
            }),
            permissions: ToolPermissions {
                shell: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
        }
    }

    fn registry_with_echo() -> Arc<ToolRegistry> {
        let mut reg = ToolRegistry::new();
        reg.register(echo_descriptor(), Arc::new(FakeTool::echo()));
        Arc::new(reg)
    }

    fn registry_with_subagent() -> Arc<ToolRegistry> {
        let mut reg = ToolRegistry::new();
        reg.register(echo_descriptor(), Arc::new(FakeTool::echo()));
        reg.register(SubagentTool::descriptor(), Arc::new(SubagentTool));
        Arc::new(reg)
    }

    fn kinds(events: &[RunEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|e| match &e.kind {
                RunEventKind::RunStarted { .. } => "RunStarted",
                RunEventKind::ContextBuilt { .. } => "ContextBuilt",
                RunEventKind::LlmRequestStarted { .. } => "LlmRequestStarted",
                RunEventKind::LlmRequestCompleted { .. } => "LlmRequestCompleted",
                RunEventKind::PromptRefinementStarted { .. } => "PromptRefinementStarted",
                RunEventKind::PromptRefinementCompleted { .. } => "PromptRefinementCompleted",
                RunEventKind::ToolCallProposed { .. } => "ToolCallProposed",
                RunEventKind::ToolCallStarted { .. } => "ToolCallStarted",
                RunEventKind::ToolCallCompleted { .. } => "ToolCallCompleted",
                RunEventKind::ToolCallFailed { .. } => "ToolCallFailed",
                RunEventKind::ApprovalRequested { .. } => "ApprovalRequested",
                RunEventKind::ApprovalResolved { .. } => "ApprovalResolved",
                RunEventKind::GuidanceInjected { .. } => "GuidanceInjected",
                RunEventKind::QualityScored { .. } => "QualityScored",
                RunEventKind::MemoryLoaded { .. } => "MemoryLoaded",
                RunEventKind::MemoryWritten { .. } => "MemoryWritten",
                RunEventKind::IngestionReferenced { .. } => "IngestionReferenced",
                RunEventKind::IngestionStarted { .. } => "IngestionStarted",
                RunEventKind::IngestionCompleted { .. } => "IngestionCompleted",
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
    async fn loaded_memory_and_ingestion_are_traced_after_context_build() {
        let mut agent = agent_with_tools(vec![], 5);
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
                "MemoryLoaded",
                "IngestionReferenced",
                "LlmRequestStarted",
                "LlmRequestCompleted",
                "RunCompleted",
            ]
        );
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::MemoryLoaded { ids } if ids == &vec!["mem-1".to_string()]
        )));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::IngestionReferenced {
                artifact_id,
                source,
            } if artifact_id == "ing-1" && source == "/tmp/source.txt"
        )));
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
