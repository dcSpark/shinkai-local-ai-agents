//! Shared scaffolding used by both the headless and TUI surfaces — registry
//! population, default agent config, and the v0 fake-LLM provider builders.
//!
//! Real model providers, configurable agents, and a config-file-driven setup
//! land later.

use std::sync::Arc;

use agent_config::ConfigResolver;
use agent_core::{
    AgentConfig, ApprovalMode, CostPolicy, IngestedArtifactView, PromptRefinement, ToolOutputMode,
    ToolPolicy, VisibilityLevel,
};
use agent_ingest::IngestionStore;
use agent_llm::{FakeProvider, FakeStep, LlmProvider, ModelRef, RigProvider, RigProviderConfig};
use agent_memory::MemoryStore;
use agent_skills::SkillRegistry;
use agent_tools::{FakeTool, ShellTool, ShellToolConfig, SubagentTool, ToolRegistry};

use crate::{Demo, Provider};

#[derive(Clone)]
pub struct RuntimeOptions {
    pub provider: Provider,
    pub model: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key_env: String,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub max_tool_calls: Option<u32>,
    pub tool_visibility: Option<VisibilityLevel>,
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
    pub enable_shell: bool,
    pub enable_subagent: bool,
    pub load_memory: bool,
    pub load_skills: bool,
    pub include_ingest: Vec<String>,
    pub allow_unsafe_ingest: bool,
    pub enable_prompt_refinement: bool,
    pub prompt_refinement_instructions: Option<String>,
    pub prompt_refinement_model: Option<String>,
    pub require_approval: bool,
    pub raw_tool_output: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            provider: Provider::Fake,
            model: None,
            api_base_url: None,
            api_key_env: "OPENAI_API_KEY".into(),
            max_output_tokens: None,
            temperature: None,
            max_tool_calls: None,
            tool_visibility: None,
            input_cost_per_million: None,
            output_cost_per_million: None,
            enable_shell: false,
            enable_subagent: false,
            load_memory: false,
            load_skills: false,
            include_ingest: Vec::new(),
            allow_unsafe_ingest: false,
            enable_prompt_refinement: false,
            prompt_refinement_instructions: None,
            prompt_refinement_model: None,
            require_approval: false,
            raw_tool_output: false,
        }
    }
}

pub fn build_registry(enable_shell: bool, enable_subagent: bool) -> Arc<ToolRegistry> {
    let mut reg = ToolRegistry::new();
    reg.register(FakeTool::echo_descriptor(), Arc::new(FakeTool::echo()));
    if enable_shell {
        reg.register(
            ShellTool::descriptor(),
            Arc::new(ShellTool::new(ShellToolConfig::default())),
        );
    }
    if enable_subagent {
        reg.register(SubagentTool::descriptor(), Arc::new(SubagentTool));
    }
    Arc::new(reg)
}

pub fn build_agent(options: &RuntimeOptions) -> AgentConfig {
    let mut agent = ConfigResolver::from_env()
        .resolve_default_agent()
        .map(|resolved| resolved.agent)
        .unwrap_or_else(|_| AgentConfig {
            id: "fake-agent".into(),
            name: "Fake Agent".into(),
            system_prompt: "You echo what the user says.".into(),
            model: ModelRef::from("fake-model"),
            prompt_refinement: None,
            tool_policy: ToolPolicy::default(),
            cost_policy: CostPolicy::default(),
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            skill_views: Vec::new(),
        });

    if let Some(model) = options.model.clone() {
        agent.model = ModelRef::from(model);
    } else if matches!(options.provider, Provider::Rig) && agent.model.0 == "fake-model" {
        agent.model = ModelRef::from("gpt-4o-mini");
    }
    if let Some(max_tool_calls) = options.max_tool_calls {
        agent.tool_policy.max_calls = max_tool_calls;
    }
    if let Some(visibility) = options.tool_visibility {
        agent.tool_policy.visibility = visibility;
    }
    if options.require_approval {
        agent.tool_policy.approval_mode = ApprovalMode::RequireExplicit;
    }
    if options.raw_tool_output {
        agent.tool_policy.output_mode = ToolOutputMode::Raw;
    }
    if options.input_cost_per_million.is_some() {
        agent.cost_policy.input_cost_per_million = options.input_cost_per_million;
    }
    if options.output_cost_per_million.is_some() {
        agent.cost_policy.output_cost_per_million = options.output_cost_per_million;
    }
    if options.enable_prompt_refinement {
        agent.prompt_refinement = Some(PromptRefinement {
            instructions: options
                .prompt_refinement_instructions
                .clone()
                .unwrap_or_default(),
            model: options.prompt_refinement_model.clone().map(ModelRef::from),
        });
    }

    if options.load_memory
        && let Ok(memory) = MemoryStore::from_env().load_fragments()
    {
        agent.memory_fragments = memory;
    }
    if options.load_skills
        && let Ok(skills) = SkillRegistry::from_env().visible_skill_views()
    {
        agent.skill_views = skills;
    }
    if !options.include_ingest.is_empty() {
        let store = IngestionStore::from_env();
        agent.ingestion_artifacts = options
            .include_ingest
            .iter()
            .filter_map(|id| store.show(id).ok())
            .map(|artifact| {
                let high_risk = artifact.has_high_risk_findings();
                let mut findings: Vec<String> = artifact
                    .findings
                    .into_iter()
                    .map(|finding| format!("{:?}: {}", finding.severity, finding.message))
                    .collect();
                let content = if high_risk && !options.allow_unsafe_ingest {
                    findings.push(
                        "Policy: content withheld; rerun with explicit unsafe-ingest override to include"
                            .into(),
                    );
                    String::new()
                } else {
                    artifact.extracted_text.unwrap_or_default()
                };
                IngestedArtifactView {
                    id: artifact.id,
                    source: artifact.source.display().to_string(),
                    sections: artifact.sections.len(),
                    content,
                    findings,
                    provenance: if high_risk && !options.allow_unsafe_ingest {
                        "agent.ingest blocked by prompt-injection guardrail".into()
                    } else {
                        "agent.ingest explicit reference".into()
                    },
                }
            })
            .collect();
    }

    agent
}

/// Build a fake LLM provider that drives the v0 demo for a single user input.
pub fn build_provider(
    demo: Demo,
    input: &str,
    options: &RuntimeOptions,
) -> Result<Arc<dyn LlmProvider>, agent_llm::LlmError> {
    match options.provider {
        Provider::Fake => Ok(match demo {
            Demo::Echo if options.enable_prompt_refinement => {
                Arc::new(FakeProvider::sequence(vec![
                    FakeStep::Reply(format!("refined: {input}")),
                    FakeStep::Reply(format!("[fake] refined: {input}")),
                ]))
            }
            Demo::Echo => Arc::new(FakeProvider::echo()),
            Demo::Tool => {
                let mut steps = Vec::new();
                if options.enable_prompt_refinement {
                    steps.push(FakeStep::Reply(format!("refined: {input}")));
                }
                steps.extend([
                    FakeStep::CallTool {
                        id: "call-1".into(),
                        tool: "echo".into(),
                        input: serde_json::json!({"text": input}),
                    },
                    FakeStep::Reply(format!("[fake] tool said: {input}")),
                ]);
                Arc::new(FakeProvider::sequence(steps))
            }
        }),
        Provider::Rig => {
            let model = rig_model_id(options);
            let model_runtime = ConfigResolver::from_env()
                .resolve_model_runtime(&model)
                .ok()
                .flatten();
            let config = RigProviderConfig {
                api_base_url: options.api_base_url.clone(),
                api_key_env: options.api_key_env.clone(),
                model: ModelRef::from(model),
                max_output_tokens: options.max_output_tokens.or_else(|| {
                    model_runtime
                        .as_ref()
                        .and_then(|model| model.max_output_tokens)
                }),
                temperature: options.temperature.or_else(|| {
                    model_runtime
                        .as_ref()
                        .and_then(|model| model.default_temperature)
                }),
            };
            Ok(Arc::new(RigProvider::from_config(config)?))
        }
    }
}

fn rig_model_id(options: &RuntimeOptions) -> String {
    if let Some(model) = options.model.clone() {
        return model;
    }
    let configured = ConfigResolver::from_env()
        .resolve_default_agent()
        .map(|resolved| resolved.agent.model.0)
        .unwrap_or_else(|_| "fake-model".into());
    if configured == "fake-model" {
        "gpt-4o-mini".into()
    } else {
        configured
    }
}
