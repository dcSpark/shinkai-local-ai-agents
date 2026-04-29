//! Shared scaffolding used by both the headless and TUI surfaces — registry
//! population, default agent config, and the v0 fake-LLM provider builders.
//!
//! Real model providers, configurable agents, and a config-file-driven setup
//! land later.

use std::sync::Arc;

use agent_config::ConfigResolver;
use agent_core::{AgentConfig, ApprovalMode, IngestedArtifactView, ToolPolicy};
use agent_ingest::IngestionStore;
use agent_llm::{FakeProvider, FakeStep, LlmProvider, ModelRef, RigProvider, RigProviderConfig};
use agent_memory::MemoryStore;
use agent_skills::SkillRegistry;
use agent_tools::{FakeTool, ShellTool, ShellToolConfig, ToolRegistry};

use crate::{Demo, Provider};

#[derive(Clone)]
pub struct RuntimeOptions {
    pub provider: Provider,
    pub model: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key_env: String,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub enable_shell: bool,
    pub load_memory: bool,
    pub load_skills: bool,
    pub include_ingest: Vec<String>,
    pub require_approval: bool,
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
            enable_shell: false,
            load_memory: false,
            load_skills: false,
            include_ingest: Vec::new(),
            require_approval: false,
        }
    }
}

pub fn build_registry(enable_shell: bool) -> Arc<ToolRegistry> {
    let mut reg = ToolRegistry::new();
    reg.register(FakeTool::echo_descriptor(), Arc::new(FakeTool::echo()));
    if enable_shell {
        reg.register(
            ShellTool::descriptor(),
            Arc::new(ShellTool::new(ShellToolConfig::default())),
        );
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
            tool_policy: ToolPolicy::default(),
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            skill_views: Vec::new(),
        });

    if let Some(model) = options.model.clone() {
        agent.model = ModelRef::from(model);
    } else if matches!(options.provider, Provider::Rig) && agent.model.0 == "fake-model" {
        agent.model = ModelRef::from("gpt-4o-mini");
    }
    if options.require_approval {
        agent.tool_policy.approval_mode = ApprovalMode::RequireExplicit;
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
            .map(|artifact| IngestedArtifactView {
                id: artifact.id,
                source: artifact.source.display().to_string(),
                sections: artifact.sections.len(),
                content: artifact.extracted_text.unwrap_or_default(),
                provenance: "agent.ingest explicit reference".into(),
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
            Demo::Echo => Arc::new(FakeProvider::echo()),
            Demo::Tool => Arc::new(FakeProvider::sequence(vec![
                FakeStep::CallTool {
                    id: "call-1".into(),
                    tool: "echo".into(),
                    input: serde_json::json!({"text": input}),
                },
                FakeStep::Reply(format!("[fake] tool said: {input}")),
            ])),
        }),
        Provider::Rig => {
            let config = RigProviderConfig {
                api_base_url: options.api_base_url.clone(),
                api_key_env: options.api_key_env.clone(),
                model: ModelRef::from(
                    options
                        .model
                        .clone()
                        .unwrap_or_else(|| "gpt-4o-mini".into()),
                ),
                max_output_tokens: options.max_output_tokens,
                temperature: options.temperature,
            };
            Ok(Arc::new(RigProvider::from_config(config)?))
        }
    }
}
