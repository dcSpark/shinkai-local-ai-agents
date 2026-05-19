//! Shared scaffolding used by both the headless and TUI surfaces — registry
//! population, default agent config, and the v0 fake-LLM provider builders.
//!
//! Real model providers, configurable agents, and a config-file-driven setup
//! land later.

use std::{collections::HashSet, sync::Arc};

use agent_adapters::AdapterRegistry;
use agent_capabilities::CapabilityDraftTool;
use agent_compaction::CompactionStore;
use agent_config::{ConfigResolver, IngestionGuardrailMode, ProfileGrantKind};
use agent_conversations::{ConversationRole, ConversationStore};
use agent_core::{
    AgentConfig, ApprovalMode, ConfigValueExplanation, CostPolicy, ExecutionPolicy, Harness,
    HookTrigger, IngestedArtifactView, MemoryFragment, PromptRefinement, RunHookHandler,
    RunLifecycleHook, SkillView, ToolOutputMode, ToolPolicy, VisibilityLevel, VoiceConfig,
};
use agent_ingest::IngestionStore;
use agent_llm::{
    AnthropicProvider, FakeProvider, FakeStep, GeminiProvider, LlmProvider, Message, ModelRef,
    NativeProviderConfig, RigProvider, RigProviderConfig,
};
use agent_memory::{MemoryRecord, MemoryStore};
use agent_skills::SkillRegistry;
use agent_storage::StoragePaths;
use agent_tools::{
    ArtifactTool, FakeTool, ShellTool, ShellToolConfig, SubagentTool, ToolRegistry,
    VoiceRuntimeConfig, register_allowed_mcp_tools_for_category_with_provenance,
    register_allowed_mcp_tools_for_resource_with_provenance,
    register_allowed_mcp_tools_with_provenance, register_voice_tools,
};

use crate::{Demo, Provider};

#[derive(Clone)]
pub struct RuntimeOptions {
    pub provider: Provider,
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key_env: String,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub max_tool_calls: Option<u32>,
    pub allowed_tool_categories: Vec<String>,
    pub allowed_skill_categories: Vec<String>,
    pub tool_visibility: Option<VisibilityLevel>,
    pub skill_visibility: Option<VisibilityLevel>,
    pub input_cost_per_million: Option<f64>,
    pub output_cost_per_million: Option<f64>,
    pub enable_shell: bool,
    pub enable_subagent: bool,
    pub enable_capability_drafts: bool,
    pub load_memory: bool,
    pub load_skills: bool,
    pub include_ingest: Vec<String>,
    pub allow_unsafe_ingest: bool,
    pub include_compact: Option<String>,
    pub conversation_id: Option<String>,
    pub enable_prompt_refinement: bool,
    pub prompt_refinement_instructions: Option<String>,
    pub prompt_refinement_model: Option<String>,
    pub require_approval: bool,
    pub auto_approve: bool,
    pub raw_tool_output: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            provider: Provider::Fake,
            agent_id: None,
            model: None,
            api_base_url: None,
            api_key_env: "OPENAI_API_KEY".into(),
            max_output_tokens: None,
            temperature: None,
            max_tool_calls: None,
            allowed_tool_categories: Vec::new(),
            allowed_skill_categories: Vec::new(),
            tool_visibility: None,
            skill_visibility: None,
            input_cost_per_million: None,
            output_cost_per_million: None,
            enable_shell: false,
            enable_subagent: false,
            enable_capability_drafts: false,
            load_memory: false,
            load_skills: false,
            include_ingest: Vec::new(),
            allow_unsafe_ingest: false,
            include_compact: None,
            conversation_id: None,
            enable_prompt_refinement: false,
            prompt_refinement_instructions: None,
            prompt_refinement_model: None,
            require_approval: false,
            auto_approve: false,
            raw_tool_output: false,
        }
    }
}

pub fn build_registry(
    enable_shell: bool,
    enable_subagent: bool,
    enable_capability_drafts: bool,
    agent_id: Option<&str>,
) -> Arc<ToolRegistry> {
    let mut reg = ToolRegistry::new();
    reg.register(FakeTool::echo_descriptor(), Arc::new(FakeTool::echo()));
    reg.register(
        ArtifactTool::descriptor(),
        Arc::new(ArtifactTool::from_env()),
    );
    if enable_shell {
        let shell_config = ShellToolConfig::from_env();
        reg.register(
            ShellTool::descriptor_for_config(&shell_config),
            Arc::new(ShellTool::new(shell_config)),
        );
    }
    if enable_subagent {
        reg.register(SubagentTool::descriptor(), Arc::new(SubagentTool));
    }
    if enable_capability_drafts {
        reg.register(
            CapabilityDraftTool::descriptor(),
            Arc::new(CapabilityDraftTool::from_env()),
        );
    }
    register_voice_tools(&mut reg, voice_runtime_config_for_agent(agent_id));
    register_profile_scoped_mcp_tools(&mut reg);
    Arc::new(reg)
}

fn voice_runtime_config_for_agent(agent_id: Option<&str>) -> VoiceRuntimeConfig {
    ConfigResolver::from_env()
        .resolve_agent(agent_id.unwrap_or("fake-agent"))
        .map(|resolved| {
            let voice = resolved.agent.voice;
            VoiceRuntimeConfig {
                input_enabled: voice.input_enabled,
                output_enabled: voice.output_enabled,
                input_backend: voice.input_backend,
                input_provider: voice.input_provider,
                input_model: voice.input_model,
                output_backend: voice.output_backend,
                tts_provider: voice.tts_provider,
                tts_model: voice.tts_model,
                voice: voice.voice,
                tone: voice.tone,
            }
        })
        .unwrap_or_default()
}

pub fn build_harness(
    provider: Arc<dyn LlmProvider>,
    events: Arc<dyn agent_tracing::EventStore>,
    registry: Arc<ToolRegistry>,
) -> Harness {
    build_harness_for_agent(provider, events, registry, None)
}

pub fn build_harness_for_agent(
    provider: Arc<dyn LlmProvider>,
    events: Arc<dyn agent_tracing::EventStore>,
    registry: Arc<ToolRegistry>,
    agent_id: Option<&str>,
) -> Harness {
    Harness::new(provider, events, registry).with_hooks(build_lifecycle_hooks_for_agent(agent_id))
}

pub fn build_lifecycle_hooks_for_agent(agent_id: Option<&str>) -> Vec<RunLifecycleHook> {
    build_lifecycle_hooks_from_paths(StoragePaths::from_env(), agent_id)
}

fn build_lifecycle_hooks_from_paths(
    paths: StoragePaths,
    agent_id: Option<&str>,
) -> Vec<RunLifecycleHook> {
    let disabled = ConfigResolver::new(paths.clone())
        .disabled_lifecycle_hooks_for_agent(agent_id.unwrap_or("fake-agent"))
        .unwrap_or_default()
        .into_iter()
        .collect::<HashSet<_>>();
    AdapterRegistry::new(paths)
        .lifecycle_hooks()
        .unwrap_or_default()
        .into_iter()
        .filter(|hook| !disabled.contains(hook.id.trim()))
        .filter_map(|hook| {
            let triggers = hook
                .triggers
                .iter()
                .filter_map(|trigger| HookTrigger::from_config_str(trigger))
                .collect::<Vec<_>>();
            (!triggers.is_empty()).then(|| {
                let mut lifecycle_hook = RunLifecycleHook::new(hook.id, triggers);
                if let Some(handler) = hook.handler {
                    lifecycle_hook = lifecycle_hook.with_handler(
                        RunHookHandler::command(handler.command, handler.args, handler.timeout_ms)
                            .with_retry_attempts(handler.retry_attempts),
                    );
                }
                lifecycle_hook
            })
        })
        .collect()
}

fn register_profile_scoped_mcp_tools(registry: &mut ToolRegistry) -> usize {
    register_profile_scoped_mcp_tools_from_paths(registry, StoragePaths::from_env())
}

fn register_profile_scoped_mcp_tools_from_paths(
    registry: &mut ToolRegistry,
    active_paths: StoragePaths,
) -> usize {
    let active_profile = active_paths.active_profile_id().to_string();
    let active_provenance = format!("profile={active_profile}");
    let mut registered = AdapterRegistry::new(active_paths.clone())
        .list()
        .map(|packages| {
            register_allowed_mcp_tools_with_provenance(registry, packages, Some(&active_provenance))
        })
        .unwrap_or_default();
    registered += register_granted_mcp_tools_from_paths(registry, active_paths);
    registered
}

fn register_granted_mcp_tools_from_paths(
    registry: &mut ToolRegistry,
    active_paths: StoragePaths,
) -> usize {
    let active_profile = active_paths.active_profile_id().to_string();
    let Ok(grants) = ConfigResolver::new(active_paths.clone()).list_profile_grants() else {
        return 0;
    };
    let mut registered = 0;
    for grant in grants.into_iter().filter(|grant| {
        matches!(
            grant.kind,
            ProfileGrantKind::Tool | ProfileGrantKind::Category
        ) && grant.to_profile == active_profile
    }) {
        let source_paths =
            StoragePaths::new_with_profile(active_paths.root().to_path_buf(), &grant.from_profile);
        let Ok(packages) = AdapterRegistry::new(source_paths).list() else {
            continue;
        };
        let provenance = format!(
            "profile={}; shared_from_profile={}; grant={}",
            grant.from_profile, grant.from_profile, grant.id
        );
        registered += match grant.kind {
            ProfileGrantKind::Tool => register_allowed_mcp_tools_for_resource_with_provenance(
                registry,
                packages,
                &grant.resource,
                Some(&provenance),
            ),
            ProfileGrantKind::Category => register_allowed_mcp_tools_for_category_with_provenance(
                registry,
                packages,
                &grant.resource,
                Some(&provenance),
            ),
            _ => 0,
        };
    }
    registered
}

pub fn build_agent(options: &RuntimeOptions) -> AgentConfig {
    let resolved = ConfigResolver::from_env()
        .resolve_agent(options.agent_id.as_deref().unwrap_or("fake-agent"));
    let config_load_memory = resolved
        .as_ref()
        .ok()
        .is_some_and(|resolved| config_bool(&resolved.values, "agent.memory_policy.load"));
    let config_load_skills = resolved
        .as_ref()
        .ok()
        .is_some_and(|resolved| config_bool(&resolved.values, "agent.skill_policy.load"));
    let ingestion_guardrail = if options.allow_unsafe_ingest {
        IngestionGuardrailMode::Allow
    } else {
        resolved
            .as_ref()
            .ok()
            .map(|resolved| config_ingestion_guardrail(&resolved.values))
            .unwrap_or(IngestionGuardrailMode::Block)
    };
    let mut agent = resolved
        .map(|resolved| resolved.agent)
        .unwrap_or_else(|_| AgentConfig {
            id: "fake-agent".into(),
            name: "Fake Agent".into(),
            system_prompt: "You echo what the user says.".into(),
            model: ModelRef::from("fake-model"),
            prompt_refinement: None,
            voice: VoiceConfig::default(),
            tool_policy: ToolPolicy::default(),
            context_policy: agent_core::ContextPolicy::default(),
            execution_policy: ExecutionPolicy::default(),
            cost_policy: CostPolicy::default(),
            conversation_history: Vec::new(),
            compacted_context: None,
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            allowed_skill_categories: Vec::new(),
            skill_views: Vec::new(),
        });

    if let Some(model) = options.model.clone() {
        agent.model = ModelRef::from(model);
    } else if !matches!(options.provider, Provider::Fake) && agent.model.0 == "fake-model" {
        agent.model = ModelRef::from(default_model_for_provider(options.provider));
    }
    if let Some(max_tool_calls) = options.max_tool_calls {
        agent.tool_policy.max_calls = max_tool_calls;
    }
    if !options.allowed_tool_categories.is_empty() {
        agent.tool_policy.allowed_categories = options.allowed_tool_categories.clone();
    }
    if !options.allowed_skill_categories.is_empty() {
        agent.allowed_skill_categories = options.allowed_skill_categories.clone();
    }
    if let Some(visibility) = options.tool_visibility {
        agent.tool_policy.visibility = visibility;
    }
    if options.auto_approve {
        agent.tool_policy.approval_mode = ApprovalMode::AutoApprove;
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
    if let Some(id) = options.include_compact.as_deref()
        && let Ok(record) = CompactionStore::from_env().show(id)
    {
        agent.compacted_context = Some(record.content);
    }
    if let Some(id) = options.conversation_id.as_deref()
        && let Ok(expanded) = ConversationStore::from_env().expanded(id)
    {
        agent.conversation_history = expanded
            .messages
            .into_iter()
            .map(conversation_message_to_llm)
            .collect();
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

    if (options.load_memory || config_load_memory)
        && let Ok(memory) = load_memory_fragments_with_profile_grants()
    {
        agent.memory_fragments = memory;
    }
    if (options.load_skills || config_load_skills)
        && let Ok(mut skills) = load_skill_views_with_profile_grants()
    {
        if let Some(visibility) = options.skill_visibility {
            for skill in &mut skills {
                apply_skill_visibility(skill, visibility);
            }
        }
        agent.skill_views = skills;
    }
    if !options.include_ingest.is_empty() {
        let store = IngestionStore::from_env();
        agent.ingestion_artifacts = options
            .include_ingest
            .iter()
            .filter_map(|id| store.show(id).ok())
            .map(|artifact| {
                let high_risk = artifact.has_unapproved_high_risk_findings();
                let mut findings = artifact.finding_summaries();
                let blocked =
                    high_risk && matches!(ingestion_guardrail, IngestionGuardrailMode::Block);
                let warned =
                    high_risk && matches!(ingestion_guardrail, IngestionGuardrailMode::Warn);
                let content = if blocked {
                    findings.push(
                        "Policy: content withheld; rerun with explicit unsafe-ingest override to include"
                            .into(),
                    );
                    String::new()
                } else {
                    if warned {
                        findings.push(
                            "Policy: unapproved high-risk content included with ingestion guardrail warning"
                                .into(),
                        );
                    }
                    artifact.extracted_text.unwrap_or_default()
                };
                IngestedArtifactView {
                    id: artifact.id,
                    source: artifact.source.display().to_string(),
                    sections: artifact.sections.len(),
                    content,
                    findings,
                    provenance: if blocked {
                        "agent.ingest blocked by prompt-injection guardrail".into()
                    } else if warned {
                        "agent.ingest warning from prompt-injection guardrail".into()
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
    let prompt_refinement_enabled = effective_prompt_refinement_enabled(options);
    match options.provider {
        Provider::Fake => Ok(match demo {
            Demo::Echo if prompt_refinement_enabled => Arc::new(FakeProvider::sequence(vec![
                FakeStep::Reply(format!("refined: {input}")),
                FakeStep::Reply(format!("[fake] refined: {input}")),
            ])),
            Demo::Echo => Arc::new(FakeProvider::echo()),
            Demo::Tool => {
                let mut steps = Vec::new();
                if prompt_refinement_enabled {
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
            let model = model_id_for_provider(options, Provider::Rig);
            let model_runtime = ConfigResolver::from_env()
                .resolve_model_runtime(&model)
                .ok()
                .flatten()
                .unwrap_or_default();
            let mut config = model_runtime
                .rig_provider_config(
                    ModelRef::from(model),
                    options.max_output_tokens,
                    options.temperature,
                )
                .map_err(|e| agent_llm::LlmError::Config(e.to_string()))?;
            if let Some(api_base_url) = options.api_base_url.clone() {
                config.api_base_url = Some(api_base_url);
            }
            if options.api_key_env != "OPENAI_API_KEY" || model_runtime.api_key_env.is_none() {
                config.api_key_env = options.api_key_env.clone();
            }
            Ok(Arc::new(RigProvider::from_config(config)?))
        }
        Provider::Ollama => {
            let model = model_id_for_provider(options, Provider::Ollama);
            let mut config = RigProviderConfig::ollama(ModelRef::from(model));
            if let Some(base_url) = options.api_base_url.clone() {
                config.api_base_url = Some(base_url);
            }
            config.max_output_tokens = options.max_output_tokens;
            config.temperature = options.temperature;
            Ok(Arc::new(RigProvider::from_config(config)?))
        }
        Provider::LlamaCpp => {
            let model = model_id_for_provider(options, Provider::LlamaCpp);
            let mut config = RigProviderConfig::llama_cpp(ModelRef::from(model));
            if let Some(base_url) = options.api_base_url.clone() {
                config.api_base_url = Some(base_url);
            }
            config.max_output_tokens = options.max_output_tokens;
            config.temperature = options.temperature;
            Ok(Arc::new(RigProvider::from_config(config)?))
        }
        Provider::Anthropic => {
            let model = model_id_for_provider(options, Provider::Anthropic);
            let config = native_provider_config(
                model,
                options,
                NativeProviderConfig::anthropic,
                "ANTHROPIC_API_KEY",
            );
            Ok(Arc::new(AnthropicProvider::from_config(config)?))
        }
        Provider::Gemini => {
            let model = model_id_for_provider(options, Provider::Gemini);
            let config = native_provider_config(
                model,
                options,
                NativeProviderConfig::gemini,
                "GEMINI_API_KEY",
            );
            Ok(Arc::new(GeminiProvider::from_config(config)?))
        }
    }
}

fn native_provider_config(
    model: String,
    options: &RuntimeOptions,
    constructor: fn(ModelRef) -> NativeProviderConfig,
    default_api_key_env: &str,
) -> NativeProviderConfig {
    let model_runtime = ConfigResolver::from_env()
        .resolve_model_runtime(&model)
        .ok()
        .flatten()
        .unwrap_or_default();
    let runtime_has_api_key_env = model_runtime.api_key_env.is_some();
    let mut config = model_runtime.native_provider_config(
        ModelRef::from(model),
        constructor,
        options.max_output_tokens,
        options.temperature,
    );
    if !runtime_has_api_key_env && options.api_key_env != "OPENAI_API_KEY" {
        config.api_key_env = options.api_key_env.clone();
    } else if !runtime_has_api_key_env {
        config.api_key_env = default_api_key_env.into();
    }
    config
}

fn effective_prompt_refinement_enabled(options: &RuntimeOptions) -> bool {
    effective_prompt_refinement_enabled_from_paths(options, StoragePaths::from_env())
}

fn effective_prompt_refinement_enabled_from_paths(
    options: &RuntimeOptions,
    paths: StoragePaths,
) -> bool {
    if options.enable_prompt_refinement {
        return true;
    }
    ConfigResolver::new(paths)
        .resolve_agent(options.agent_id.as_deref().unwrap_or("fake-agent"))
        .map(|resolved| resolved.agent.prompt_refinement.is_some())
        .unwrap_or(false)
}

fn config_bool(values: &[ConfigValueExplanation], key: &str) -> bool {
    values
        .iter()
        .find(|value| value.key == key)
        .and_then(|value| value.value.as_bool())
        .unwrap_or(false)
}

fn config_ingestion_guardrail(values: &[ConfigValueExplanation]) -> IngestionGuardrailMode {
    values
        .iter()
        .find(|value| value.key == "agent.ingestion_policy.guardrail_mode")
        .and_then(|value| value.value.as_str())
        .and_then(IngestionGuardrailMode::from_config_str)
        .unwrap_or(IngestionGuardrailMode::Block)
}

fn model_id_for_provider(options: &RuntimeOptions, provider: Provider) -> String {
    if let Some(model) = options.model.clone() {
        return model;
    }
    let configured = ConfigResolver::from_env()
        .resolve_agent(options.agent_id.as_deref().unwrap_or("fake-agent"))
        .map(|resolved| resolved.agent.model.0)
        .unwrap_or_else(|_| "fake-model".into());
    if configured == "fake-model" {
        default_model_for_provider(provider).into()
    } else {
        configured
    }
}

fn default_model_for_provider(provider: Provider) -> &'static str {
    match provider {
        Provider::Fake => "fake-model",
        Provider::Rig => "gpt-4o-mini",
        Provider::Ollama => "llama3.1",
        Provider::LlamaCpp => "local-model",
        Provider::Anthropic => "claude-sonnet-4-5",
        Provider::Gemini => "gemini-2.5-flash",
    }
}

fn load_memory_fragments_with_profile_grants() -> anyhow::Result<Vec<MemoryFragment>> {
    let active_paths = StoragePaths::from_env();
    let active_profile = active_paths.active_profile_id().to_string();
    let mut fragments = MemoryStore::new(active_paths.clone()).load_fragments()?;
    let resolver = ConfigResolver::from_env();
    for grant in resolver.list_profile_grants()?.into_iter().filter(|grant| {
        grant.kind == ProfileGrantKind::Memory && grant.to_profile == active_profile
    }) {
        let source_paths =
            StoragePaths::new_with_profile(active_paths.root().to_path_buf(), &grant.from_profile);
        let records = MemoryStore::new(source_paths).list()?;
        for record in records
            .into_iter()
            .filter(|record| memory_record_matches_grant(record, &grant.resource))
        {
            fragments.push(MemoryStore::fragment_from_record(
                record,
                Some(format!(
                    "shared_from_profile={}; grant={}",
                    grant.from_profile, grant.id
                )),
            ));
        }
    }
    Ok(fragments)
}

fn memory_record_matches_grant(record: &MemoryRecord, resource: &str) -> bool {
    resource == "*" || record.id == resource || record.owning_agent.as_deref() == Some(resource)
}

fn load_skill_views_with_profile_grants() -> anyhow::Result<Vec<SkillView>> {
    let active_paths = StoragePaths::from_env();
    load_skill_views_with_profile_grants_from(active_paths)
}

fn load_skill_views_with_profile_grants_from(
    active_paths: StoragePaths,
) -> anyhow::Result<Vec<SkillView>> {
    let active_profile = active_paths.active_profile_id().to_string();
    let mut skills = SkillRegistry::new(active_paths.clone()).visible_skill_views()?;
    let mut loaded_ids = skills
        .iter()
        .map(|skill| skill.id.clone())
        .collect::<HashSet<_>>();
    let resolver = ConfigResolver::new(active_paths.clone());
    for grant in resolver.list_profile_grants()?.into_iter().filter(|grant| {
        matches!(
            grant.kind,
            ProfileGrantKind::Skill | ProfileGrantKind::Category
        ) && grant.to_profile == active_profile
    }) {
        let source_paths =
            StoragePaths::new_with_profile(active_paths.root().to_path_buf(), &grant.from_profile);
        for mut skill in SkillRegistry::new(source_paths)
            .visible_skill_views()?
            .into_iter()
            .filter(|skill| skill_view_matches_grant(skill, &grant))
        {
            if loaded_ids.contains(&skill.id) {
                continue;
            }
            let shared_provenance = format!(
                "shared_from_profile={}; grant={}",
                grant.from_profile, grant.id
            );
            skill.provenance = Some(match skill.provenance {
                Some(base) => format!("{base}; {shared_provenance}"),
                None => shared_provenance,
            });
            loaded_ids.insert(skill.id.clone());
            skills.push(skill);
        }
    }
    Ok(skills)
}

fn skill_view_matches_grant(skill: &SkillView, grant: &agent_config::ProfileGrant) -> bool {
    match grant.kind {
        ProfileGrantKind::Skill => {
            grant.resource == "*" || skill.id == grant.resource || skill.name == grant.resource
        }
        ProfileGrantKind::Category => {
            grant.resource == "*"
                || skill
                    .categories
                    .iter()
                    .any(|category| category == &grant.resource)
        }
        _ => false,
    }
}

fn apply_skill_visibility(skill: &mut SkillView, visibility: VisibilityLevel) {
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

fn conversation_message_to_llm(message: agent_conversations::ConversationMessage) -> Message {
    match message.role {
        ConversationRole::System => Message::system(message.content),
        ConversationRole::User => Message::user(message.content),
        ConversationRole::Assistant => Message::Assistant {
            content: Some(message.content),
            tool_calls: Vec::new(),
        },
        ConversationRole::Tool => {
            Message::system(format!("Persisted tool result: {}", message.content))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_config::{AgentConfigFile, AgentPromptRefinementConfig};
    use agent_tools::ToolId;

    #[test]
    fn granted_skills_load_from_source_profile_after_allow() {
        let dir =
            std::env::temp_dir().join(format!("skill-grant-setup-test-{}", std::process::id()));
        let source = dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "# Shared Review\nUse careful shared review.",
        )
        .unwrap();
        let root = dir.join("home");
        let main_paths = StoragePaths::new(root.clone());
        let research_paths = StoragePaths::new_with_profile(root, "research");
        let resolver = ConfigResolver::new(main_paths.clone());
        resolver.create_profile("research", None).unwrap();
        let registry = SkillRegistry::new(main_paths);
        let doc = registry.import_openclaw(&source).unwrap();
        resolver
            .grant_profile_access("main", "research", ProfileGrantKind::Skill, &doc.id)
            .unwrap();

        let hidden = load_skill_views_with_profile_grants_from(research_paths.clone()).unwrap();
        assert!(hidden.is_empty());

        registry.allow(&doc.id).unwrap();
        let visible = load_skill_views_with_profile_grants_from(research_paths).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, doc.id);
        assert_eq!(
            visible[0].provenance.as_deref(),
            Some(
                "profile=main; shared_from_profile=main; grant=grant-main-research-skill-shared-review"
            )
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn category_grants_load_matching_skills_from_source_profile() {
        let dir = std::env::temp_dir().join(format!(
            "skill-category-grant-setup-test-{}",
            std::process::id()
        ));
        let source = dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "# Shared Review\ncategories: review, quality\nUse careful shared review.",
        )
        .unwrap();
        let root = dir.join("home");
        let main_paths = StoragePaths::new(root.clone());
        let research_paths = StoragePaths::new_with_profile(root, "research");
        let resolver = ConfigResolver::new(main_paths.clone());
        resolver.create_profile("research", None).unwrap();
        let registry = SkillRegistry::new(main_paths);
        let doc = registry.import_openclaw(&source).unwrap();
        resolver
            .grant_profile_access("main", "research", ProfileGrantKind::Category, "review")
            .unwrap();

        registry.allow(&doc.id).unwrap();
        let visible = load_skill_views_with_profile_grants_from(research_paths).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, doc.id);
        assert_eq!(visible[0].categories, vec!["review", "quality"]);
        assert!(visible[0].provenance.as_deref().is_some_and(|provenance| {
            provenance
                .contains("shared_from_profile=main; grant=grant-main-research-category-review")
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn granted_mcp_tools_load_from_source_profile_after_allow() {
        let dir =
            std::env::temp_dir().join(format!("tool-grant-setup-test-{}", std::process::id()));
        let source = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).unwrap();
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
        let root = dir.join("home");
        let main_paths = StoragePaths::new(root.clone());
        let research_paths = StoragePaths::new_with_profile(root, "research");
        let resolver = ConfigResolver::new(main_paths.clone());
        resolver.create_profile("research", None).unwrap();
        let registry = AdapterRegistry::new(main_paths);
        let package = registry.import(&source).unwrap();
        resolver
            .grant_profile_access("main", "research", ProfileGrantKind::Tool, "mcp-search")
            .unwrap();

        let mut hidden = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_mcp_tools_from_paths(&mut hidden, research_paths.clone()),
            0
        );

        registry.allow(&package.id).unwrap();
        let mut visible = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_mcp_tools_from_paths(&mut visible, research_paths),
            1
        );
        let search = visible.descriptor(&ToolId::from("mcp-search")).unwrap();
        assert!(search.provenance.as_deref().is_some_and(|provenance| {
            provenance
                .contains("shared_from_profile=main; grant=grant-main-research-tool-mcp-search")
        }));
        assert!(
            visible
                .descriptor(&ToolId::from("mcp-filesystem"))
                .is_none()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn category_grants_load_mcp_tools_from_source_profile() {
        let dir = std::env::temp_dir().join(format!(
            "tool-category-grant-setup-test-{}",
            std::process::id()
        ));
        let source = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).unwrap();
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
        let root = dir.join("home");
        let main_paths = StoragePaths::new(root.clone());
        let research_paths = StoragePaths::new_with_profile(root, "research");
        let resolver = ConfigResolver::new(main_paths.clone());
        resolver.create_profile("research", None).unwrap();
        let registry = AdapterRegistry::new(main_paths);
        let package = registry.import(&source).unwrap();
        resolver
            .grant_profile_access("main", "research", ProfileGrantKind::Category, "mcp")
            .unwrap();

        let mut hidden = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_mcp_tools_from_paths(&mut hidden, research_paths.clone()),
            0
        );

        registry.allow(&package.id).unwrap();
        let mut visible = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_mcp_tools_from_paths(&mut visible, research_paths),
            2
        );
        let search = visible.descriptor(&ToolId::from("mcp-search")).unwrap();
        assert!(search.provenance.as_deref().is_some_and(|provenance| {
            provenance.contains("shared_from_profile=main; grant=grant-main-research-category-mcp")
        }));
        assert!(
            visible
                .descriptor(&ToolId::from("mcp-filesystem"))
                .is_some()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn active_mcp_tools_include_profile_provenance() {
        let dir = std::env::temp_dir().join(format!(
            "active-tool-provenance-test-{}",
            std::process::id()
        ));
        let source = dir.join("mcp.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &source,
            r#"{
              "mcpServers": {
                "search": { "url": "https://example.invalid/mcp" }
              }
            }"#,
        )
        .unwrap();
        let paths = StoragePaths::new_with_profile(dir.join("home"), "research");
        let registry = AdapterRegistry::new(paths.clone());
        let package = registry.import(&source).unwrap();
        registry.allow(&package.id).unwrap();

        let mut visible = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_mcp_tools_from_paths(&mut visible, paths),
            1
        );
        let search = visible.descriptor(&ToolId::from("mcp-search")).unwrap();
        assert!(
            search
                .provenance
                .as_deref()
                .is_some_and(|provenance| provenance.contains("profile=research"))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prompt_refinement_enabled_uses_agent_config() {
        let dir = std::env::temp_dir().join(format!(
            "prompt-refinement-setup-test-{}",
            std::process::id()
        ));
        let paths = StoragePaths::new(dir.join("home"));
        let resolver = ConfigResolver::new(paths.clone());
        resolver
            .save_agent_config(&AgentConfigFile {
                id: "critic".into(),
                name: "Critic".into(),
                system_prompt: "Review carefully.".into(),
                prompt_refinement: Some(AgentPromptRefinementConfig {
                    id: None,
                    when: None,
                    instructions: "Clarify first.".into(),
                    model: None,
                    agent_awareness: false,
                }),
                model: Some("fake-model".into()),
                ..AgentConfigFile::default()
            })
            .unwrap();

        let mut options = RuntimeOptions {
            agent_id: Some("critic".into()),
            ..RuntimeOptions::default()
        };
        assert!(effective_prompt_refinement_enabled_from_paths(
            &options,
            paths.clone()
        ));

        options.agent_id = Some("missing".into());
        assert!(!effective_prompt_refinement_enabled_from_paths(
            &options,
            paths.clone()
        ));

        options.enable_prompt_refinement = true;
        assert!(effective_prompt_refinement_enabled_from_paths(
            &options, paths
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lifecycle_hook_loading_honors_persisted_disabled_policy() {
        let dir =
            std::env::temp_dir().join(format!("hook-policy-setup-test-{}", std::process::id()));
        let source = dir.join("plugin.yaml");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &source,
            r#"
name: hook-demo
hooks:
  - name: audit_tool_completion
    command: /usr/bin/true
"#,
        )
        .unwrap();
        let paths = StoragePaths::new(dir.join("home"));
        let registry = AdapterRegistry::new(paths.clone());
        let package = registry.import(&source).unwrap();
        let allowed = registry.allow(&package.id).unwrap();
        let hook_id = format!("adapter:{}:audit-tool-completion", allowed.id);

        assert_eq!(
            build_lifecycle_hooks_from_paths(paths.clone(), None).len(),
            1
        );

        ConfigResolver::new(paths.clone())
            .set_profile_lifecycle_hook_disabled(&hook_id, true)
            .unwrap();

        assert!(build_lifecycle_hooks_from_paths(paths, None).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn runtime_category_overrides_apply_to_built_agent() {
        let agent = build_agent(&RuntimeOptions {
            allowed_tool_categories: vec!["mcp".into()],
            allowed_skill_categories: vec!["review".into()],
            ..RuntimeOptions::default()
        });

        assert_eq!(agent.tool_policy.allowed_categories, vec!["mcp"]);
        assert_eq!(agent.allowed_skill_categories, vec!["review"]);
    }

    #[test]
    fn skill_visibility_override_hides_body_or_description() {
        let mut skill = SkillView {
            id: "review".into(),
            name: "Review".into(),
            description: Some("Review description".into()),
            categories: vec!["review".into()],
            body: Some("# Review\nUse the full checklist.".into()),
            estimated_tokens: 12,
            visibility: VisibilityLevel::FullSchema,
            provenance: Some("test".into()),
        };

        apply_skill_visibility(&mut skill, VisibilityLevel::NameAndDescription);
        assert_eq!(skill.visibility, VisibilityLevel::NameAndDescription);
        assert!(skill.body.is_none());
        assert_eq!(skill.description.as_deref(), Some("Review description"));
        assert_eq!(skill.estimated_tokens, 0);

        apply_skill_visibility(&mut skill, VisibilityLevel::NameOnly);
        assert_eq!(skill.visibility, VisibilityLevel::NameOnly);
        assert!(skill.body.is_none());
        assert!(skill.description.is_none());
        assert_eq!(skill.estimated_tokens, 0);
    }
}
