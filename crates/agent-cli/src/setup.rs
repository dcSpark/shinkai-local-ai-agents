//! Shared scaffolding used by both the headless and TUI surfaces — registry
//! population, default agent config, and the v0 fake-LLM provider builders.
//!
//! Real model providers, configurable agents, and a config-file-driven setup
//! land later.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use agent_adapters::AdapterRegistry;
use agent_capabilities::CapabilityDraftTool;
use agent_compaction::CompactionStore;
use agent_config::{
    ConfigResolver, IngestionGuardrailMode, ProfileGrantKind, configured_model_providers,
    system_prompt_with_prompt_refinement_awareness,
};
use agent_conversations::{ConversationPolicy, ConversationRole, ConversationStore};
use agent_core::{
    AgentConfig, ApprovalMode, ConfigValueExplanation, CostPolicy, ExecutionPolicy, Harness,
    HookTrigger, IngestedArtifactView, PromptRefinement, RunHookHandler, RunLifecycleHook,
    SkillView, ToolOutputMode, ToolPolicy, VisibilityLevel, VoiceConfig,
};
use agent_ingest::IngestionStore;
use agent_llm::{
    AnthropicProvider, FakeProvider, FakeStep, GeminiProvider, LlmProvider, Message, ModelRef,
    NativeProviderConfig, RigProvider,
};
use agent_memory::load_fragments_with_profile_grants;
use agent_skills::SkillRegistry;
use agent_storage::StoragePaths;
use agent_tools::{
    ArtifactTool, FakeTool, ShellTool, ShellToolConfig, SubagentTool, ToolRegistry,
    VoiceRuntimeConfig, register_allowed_adapter_tools_for_category_with_provenance,
    register_allowed_adapter_tools_for_resource_with_provenance,
    register_allowed_adapter_tools_with_provenance, register_code_execution_tools,
    register_payment_tools_from_env, register_voice_tools,
};

use crate::{Demo, Provider};

#[derive(Clone)]
pub struct RuntimeOptions {
    pub provider: Provider,
    pub provider_id: Option<String>,
    pub agent_id: Option<String>,
    pub model: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key_env: String,
    pub max_output_tokens: Option<u64>,
    pub temperature: Option<f64>,
    pub max_tool_calls: Option<u32>,
    pub max_tokens_before_compaction: Option<u32>,
    pub max_compaction_output_tokens: Option<u32>,
    pub compaction_guidance: Option<String>,
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
    pub memory_topics: Vec<String>,
    pub load_skills: bool,
    pub include_ingest: Vec<String>,
    pub allow_unsafe_ingest: bool,
    pub include_compact: Option<String>,
    pub conversation_id: Option<String>,
    pub enable_prompt_refinement: bool,
    pub prompt_refinement_instructions: Option<String>,
    pub prompt_refinement_model: Option<String>,
    pub prompt_refinement_agent_awareness: bool,
    pub require_approval: bool,
    pub auto_approve: bool,
    pub raw_tool_output: bool,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            provider: Provider::Fake,
            provider_id: None,
            agent_id: None,
            model: None,
            api_base_url: None,
            api_key_env: "OPENAI_API_KEY".into(),
            max_output_tokens: None,
            temperature: None,
            max_tool_calls: None,
            max_tokens_before_compaction: None,
            max_compaction_output_tokens: None,
            compaction_guidance: None,
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
            memory_topics: Vec::new(),
            load_skills: false,
            include_ingest: Vec::new(),
            allow_unsafe_ingest: false,
            include_compact: None,
            conversation_id: None,
            enable_prompt_refinement: false,
            prompt_refinement_instructions: None,
            prompt_refinement_model: None,
            prompt_refinement_agent_awareness: false,
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
    conversation_id: Option<&str>,
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
            Arc::new(ShellTool::new(shell_config.clone())),
        );
        register_code_execution_tools(&mut reg, &shell_config);
    }
    if enable_subagent {
        reg.register(
            SubagentTool::descriptor_with_agent_options(selectable_subagent_ids(agent_id)),
            Arc::new(SubagentTool),
        );
    }
    let (policy_enabled, guidance) = capability_draft_policy_for(agent_id, conversation_id);
    if enable_capability_drafts || policy_enabled {
        reg.register(
            CapabilityDraftTool::descriptor_with_guidance(guidance.as_deref()),
            Arc::new(CapabilityDraftTool::from_env()),
        );
    }
    register_voice_tools(&mut reg, voice_runtime_config_for_agent(agent_id));
    register_payment_tools_from_env(&mut reg);
    register_profile_scoped_adapter_tools(&mut reg);
    Arc::new(reg)
}

fn selectable_subagent_ids(agent_id: Option<&str>) -> Vec<String> {
    ConfigResolver::from_env()
        .list_subagent_agent_ids(agent_id.unwrap_or("fake-agent"))
        .unwrap_or_default()
}

fn capability_draft_policy_for(
    agent_id: Option<&str>,
    conversation_id: Option<&str>,
) -> (bool, Option<String>) {
    let mut enabled = false;
    let mut guidance = None;
    if let Ok(resolved) = ConfigResolver::from_env().resolve_agent(agent_id.unwrap_or("fake-agent"))
    {
        enabled = resolved.agent.tool_policy.capability_drafts_enabled;
        guidance = resolved.agent.tool_policy.capability_draft_guidance;
    }
    if let Some(policy) = conversation_policy_for(conversation_id) {
        if let Some(value) = policy.capability_drafts_enabled {
            enabled = value;
        }
        if let Some(value) = policy
            .capability_draft_guidance
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            guidance = Some(value.to_string());
        }
    }
    (enabled, guidance)
}

fn conversation_policy_for(conversation_id: Option<&str>) -> Option<ConversationPolicy> {
    conversation_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .and_then(|id| ConversationStore::from_env().expanded(id).ok())
        .map(|expanded| expanded.conversation.policy)
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

fn register_profile_scoped_adapter_tools(registry: &mut ToolRegistry) -> usize {
    register_profile_scoped_adapter_tools_from_paths(registry, StoragePaths::from_env())
}

fn register_profile_scoped_adapter_tools_from_paths(
    registry: &mut ToolRegistry,
    active_paths: StoragePaths,
) -> usize {
    let active_profile = active_paths.active_profile_id().to_string();
    let active_provenance = format!("profile={active_profile}");
    let mut registered = AdapterRegistry::new(active_paths.clone())
        .list()
        .map(|packages| {
            register_allowed_adapter_tools_with_provenance(
                registry,
                packages,
                Some(&active_provenance),
            )
        })
        .unwrap_or_default();
    registered += register_granted_adapter_tools_from_paths(registry, active_paths);
    registered
}

fn register_granted_adapter_tools_from_paths(
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
            ProfileGrantKind::Tool => register_allowed_adapter_tools_for_resource_with_provenance(
                registry,
                packages,
                &grant.resource,
                Some(&provenance),
            ),
            ProfileGrantKind::Category => {
                register_allowed_adapter_tools_for_category_with_provenance(
                    registry,
                    packages,
                    &grant.resource,
                    Some(&provenance),
                )
            }
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
            memory_backend: agent_core::DEFAULT_MEMORY_BACKEND_ID.into(),
            memory_model: None,
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            allowed_skill_categories: Vec::new(),
            skill_visibility: VisibilityLevel::FullSchema,
            skill_visibility_overrides: HashMap::new(),
            skill_views: Vec::new(),
            subagent_configs: Vec::new(),
        });
    let expanded_conversation = options
        .conversation_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .and_then(|id| ConversationStore::from_env().expanded(id).ok());
    let conversation_policy = expanded_conversation
        .as_ref()
        .map(|expanded| expanded.conversation.policy.clone());
    if let Some(policy) = conversation_policy.as_ref() {
        apply_conversation_policy(&mut agent, policy);
    }

    if let Some(model) = options.model.clone() {
        agent.model = ModelRef::from(model);
    } else {
        let provider = selected_provider_id(options);
        if provider != "fake" && agent.model.0 == "fake-model" {
            agent.model = ModelRef::from(default_model_for_provider(&provider));
        }
    }
    if let Some(max_tool_calls) = options.max_tool_calls {
        agent.tool_policy.max_calls = max_tool_calls;
    }
    if let Some(max_tokens_before_compaction) = options.max_tokens_before_compaction {
        agent.context_policy.compaction.max_tokens_before_compaction =
            Some(max_tokens_before_compaction);
    }
    if let Some(max_compaction_output_tokens) = options.max_compaction_output_tokens {
        agent.context_policy.compaction.max_output_tokens = Some(max_compaction_output_tokens);
    }
    if let Some(guidance) = options
        .compaction_guidance
        .as_deref()
        .map(str::trim)
        .filter(|guidance| !guidance.is_empty())
    {
        agent.context_policy.compaction.guidance = Some(guidance.to_string());
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
    if let Some(expanded) = expanded_conversation {
        agent.conversation_history = expanded
            .messages
            .into_iter()
            .map(conversation_message_to_llm)
            .collect();
    }
    if options.enable_prompt_refinement {
        let instructions = options
            .prompt_refinement_instructions
            .clone()
            .unwrap_or_default();
        agent.prompt_refinement = Some(PromptRefinement {
            instructions: instructions.clone(),
            model: options.prompt_refinement_model.clone().map(ModelRef::from),
        });
    }
    if options.prompt_refinement_agent_awareness
        && let Some(refinement) = agent.prompt_refinement.as_ref()
    {
        agent.system_prompt = system_prompt_with_prompt_refinement_awareness(
            &agent.system_prompt,
            &refinement.instructions,
        );
    }

    let load_memory = conversation_policy
        .as_ref()
        .map(|policy| policy.effective_load_memory(config_load_memory, options.load_memory))
        .unwrap_or(config_load_memory || options.load_memory);
    if load_memory
        && let Ok(memory) = load_fragments_with_profile_grants(
            StoragePaths::from_env(),
            &agent.memory_backend,
            &options.memory_topics,
        )
    {
        agent.memory_fragments = memory;
    }
    if let Some(visibility) = options.skill_visibility {
        agent.skill_visibility = visibility;
    }
    if (options.load_skills || config_load_skills)
        && let Ok(skills) = load_skill_views_with_profile_grants()
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
    if options.enable_subagent {
        agent.subagent_configs = ConfigResolver::from_env()
            .resolve_subagent_configs(&agent.id)
            .unwrap_or_default();
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
    let provider = selected_provider_id(options);
    match provider.as_str() {
        "fake" => Ok(fake_provider_for_demo(
            demo,
            input,
            prompt_refinement_enabled,
        )),
        "anthropic" => {
            let model = model_id_for_provider(options, &provider);
            let config = native_provider_config(
                model,
                options,
                NativeProviderConfig::anthropic,
                "ANTHROPIC_API_KEY",
            );
            Ok(Arc::new(AnthropicProvider::from_config(config)?))
        }
        "gemini" => {
            let model = model_id_for_provider(options, &provider);
            let config = native_provider_config(
                model,
                options,
                NativeProviderConfig::gemini,
                "GEMINI_API_KEY",
            );
            Ok(Arc::new(GeminiProvider::from_config(config)?))
        }
        _ => openai_compatible_provider_for_run(&provider, options),
    }
}

fn fake_provider_for_demo(
    demo: Demo,
    input: &str,
    prompt_refinement_enabled: bool,
) -> Arc<dyn LlmProvider> {
    match demo {
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
    }
}

fn openai_compatible_provider_for_run(
    provider: &str,
    options: &RuntimeOptions,
) -> Result<Arc<dyn LlmProvider>, agent_llm::LlmError> {
    let model = model_id_for_provider(options, provider);
    let mut model_runtime = ConfigResolver::from_env()
        .resolve_model_runtime(&model)
        .ok()
        .flatten()
        .unwrap_or_default();
    if model_runtime.provider.is_none() {
        model_runtime.provider = Some(provider.to_string());
    }
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
    if options.api_key_env != "OPENAI_API_KEY" {
        config.api_key_env = options.api_key_env.clone();
    }
    Ok(Arc::new(RigProvider::from_config(config)?))
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

fn model_id_for_provider(options: &RuntimeOptions, provider: &str) -> String {
    if let Some(model) = options.model.clone() {
        return model;
    }
    let configured = ConfigResolver::from_env()
        .resolve_agent(options.agent_id.as_deref().unwrap_or("fake-agent"))
        .map(|resolved| resolved.agent.model.0)
        .unwrap_or_else(|_| "fake-model".into());
    if configured == "fake-model" {
        default_model_for_provider(provider)
    } else {
        configured
    }
}

pub fn selected_provider_id(options: &RuntimeOptions) -> String {
    options
        .provider_id
        .as_deref()
        .map(normalized_provider_id)
        .unwrap_or_else(|| normalized_provider_id(provider_name(options.provider)))
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Fake => "fake",
        Provider::Rig => "rig",
        Provider::Ollama => "ollama",
        Provider::LlamaCpp => "llama_cpp",
        Provider::Anthropic => "anthropic",
        Provider::Gemini => "gemini",
    }
}

fn default_model_for_provider(provider: &str) -> String {
    let provider = normalized_provider_id(provider);
    match provider.as_str() {
        "fake" => "fake-model".into(),
        "rig" => "gpt-4o-mini".into(),
        "ollama" => "llama3.1".into(),
        "llama_cpp" => "local-model".into(),
        "anthropic" => "claude-sonnet-4-5".into(),
        "gemini" => "gemini-2.5-flash".into(),
        other => configured_model_providers()
            .ok()
            .and_then(|providers| {
                providers
                    .into_iter()
                    .find(|descriptor| descriptor.id == other)
            })
            .map(|descriptor| descriptor.default_model)
            .unwrap_or_else(|| "fake-model".into()),
    }
}

fn normalized_provider_id(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "" => "fake".into(),
        "openai" | "openai-compatible" | "openai_compatible" => "rig".into(),
        "llama-cpp" | "llamacpp" => "llama_cpp".into(),
        other => other.into(),
    }
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

fn apply_conversation_policy(agent: &mut AgentConfig, policy: &ConversationPolicy) {
    if let Some(categories) = &policy.allowed_tool_categories {
        agent.tool_policy.allowed_categories = categories.clone();
    }
    if let Some(categories) = &policy.allowed_skill_categories {
        agent.allowed_skill_categories = categories.clone();
    }
    if let Some(enabled) = policy.capability_drafts_enabled {
        agent.tool_policy.capability_drafts_enabled = enabled;
    }
    if let Some(guidance) = policy
        .capability_draft_guidance
        .as_deref()
        .map(str::trim)
        .filter(|guidance| !guidance.is_empty())
    {
        agent.tool_policy.capability_draft_guidance = Some(guidance.to_string());
    }
    if let Some(max_tokens_before_compaction) = policy.max_tokens_before_compaction {
        agent.context_policy.compaction.max_tokens_before_compaction =
            Some(max_tokens_before_compaction);
    }
    if let Some(max_compaction_output_tokens) = policy.max_compaction_output_tokens {
        agent.context_policy.compaction.max_output_tokens = Some(max_compaction_output_tokens);
    }
    if let Some(guidance) = policy
        .compaction_guidance
        .as_deref()
        .map(str::trim)
        .filter(|guidance| !guidance.is_empty())
    {
        agent.context_policy.compaction.guidance = Some(guidance.to_string());
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

    fn restore_env(name: &str, value: Option<std::ffi::OsString>) {
        unsafe {
            if let Some(value) = value {
                std::env::set_var(name, value);
            } else {
                std::env::remove_var(name);
            }
        }
    }

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
            register_profile_scoped_adapter_tools_from_paths(&mut hidden, research_paths.clone()),
            0
        );

        registry.allow(&package.id).unwrap();
        let mut visible = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_adapter_tools_from_paths(&mut visible, research_paths),
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
    fn granted_a2a_external_agents_load_from_source_profile_after_allow() {
        let dir = std::env::temp_dir().join(format!("a2a-grant-setup-test-{}", std::process::id()));
        let source = dir.join("agent-card.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &source,
            r#"{
              "name": "Remote Reviewer",
              "description": "Reviews documents.",
              "url": "https://agents.example.test/a2a",
              "preferredTransport": "JSONRPC",
              "defaultInputModes": ["text/plain"],
              "defaultOutputModes": ["text/plain"],
              "skills": [
                { "id": "review", "name": "Review", "description": "Review text." }
              ]
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
            .grant_profile_access("main", "research", ProfileGrantKind::Tool, "review")
            .unwrap();

        let mut hidden = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_adapter_tools_from_paths(&mut hidden, research_paths.clone()),
            0
        );

        registry.allow(&package.id).unwrap();
        let mut visible = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_adapter_tools_from_paths(&mut visible, research_paths),
            1
        );
        let review = visible.descriptor(&ToolId::from("a2a-review")).unwrap();
        assert!(review.permissions.network);
        assert!(review.provenance.as_deref().is_some_and(|provenance| {
            provenance.contains("shared_from_profile=main; grant=grant-main-research-tool-review")
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn category_grants_load_external_agents_as_subagents_from_source_profile() {
        let dir = std::env::temp_dir().join(format!(
            "a2a-subagent-category-grant-setup-test-{}",
            std::process::id()
        ));
        let source = dir.join("agent-card.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &source,
            r#"{
              "name": "Remote Reviewer",
              "description": "Reviews documents.",
              "url": "https://agents.example.test/a2a",
              "preferredTransport": "JSONRPC",
              "defaultInputModes": ["text/plain"],
              "defaultOutputModes": ["text/plain"],
              "skills": [
                { "id": "review", "name": "Review", "description": "Review text." }
              ]
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
            .grant_profile_access("main", "research", ProfileGrantKind::Category, "subagent")
            .unwrap();

        let mut hidden = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_adapter_tools_from_paths(&mut hidden, research_paths.clone()),
            0
        );

        registry.allow(&package.id).unwrap();
        let mut visible = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_adapter_tools_from_paths(&mut visible, research_paths),
            1
        );
        let review = visible.descriptor(&ToolId::from("a2a-review")).unwrap();
        assert!(
            review
                .categories
                .iter()
                .any(|category| category == "subagent")
        );
        assert!(review.provenance.as_deref().is_some_and(|provenance| {
            provenance
                .contains("shared_from_profile=main; grant=grant-main-research-category-subagent")
        }));
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
            register_profile_scoped_adapter_tools_from_paths(&mut hidden, research_paths.clone()),
            0
        );

        registry.allow(&package.id).unwrap();
        let mut visible = ToolRegistry::new();
        assert_eq!(
            register_profile_scoped_adapter_tools_from_paths(&mut visible, research_paths),
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
            register_profile_scoped_adapter_tools_from_paths(&mut visible, paths),
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
    fn runtime_prompt_refinement_awareness_updates_system_prompt() {
        let agent = build_agent(&RuntimeOptions {
            enable_prompt_refinement: true,
            prompt_refinement_instructions: Some("Clarify the request.".into()),
            prompt_refinement_agent_awareness: true,
            ..RuntimeOptions::default()
        });

        assert!(agent.system_prompt.contains("<prompt-refinement-guidance>"));
        assert!(agent.system_prompt.contains("Clarify the request."));
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
    fn runtime_compaction_overrides_apply_to_built_agent() {
        let agent = build_agent(&RuntimeOptions {
            max_tokens_before_compaction: Some(256),
            max_compaction_output_tokens: Some(96),
            compaction_guidance: Some("keep decisions".into()),
            ..RuntimeOptions::default()
        });

        assert_eq!(
            agent.context_policy.compaction.max_tokens_before_compaction,
            Some(256)
        );
        assert_eq!(agent.context_policy.compaction.max_output_tokens, Some(96));
        assert_eq!(
            agent.context_policy.compaction.guidance.as_deref(),
            Some("keep decisions")
        );
    }

    #[test]
    fn capability_draft_registry_uses_agent_policy_and_guidance() {
        let _guard = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "capability-draft-policy-setup-test-{}",
            std::process::id()
        ));
        let previous_home = std::env::var_os("AGENT_HARNESS_HOME");
        unsafe {
            std::env::set_var("AGENT_HARNESS_HOME", &dir);
        }
        ConfigResolver::from_env()
            .save_agent_config(&AgentConfigFile {
                id: "critic".into(),
                name: "Critic".into(),
                system_prompt: "Review carefully.".into(),
                capability_drafts_enabled: Some(true),
                capability_draft_guidance: Some("Prefer narrow reusable pieces.".into()),
                ..AgentConfigFile::default()
            })
            .unwrap();

        let registry = build_registry(false, false, false, Some("critic"), None);
        let descriptor = registry
            .descriptor(&ToolId::from("capability_draft"))
            .expect("agent policy should register capability draft tool");

        assert!(
            descriptor
                .description
                .contains("Prefer narrow reusable pieces.")
        );
        restore_env("AGENT_HARNESS_HOME", previous_home);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn conversation_policy_can_disable_capability_draft_registry() {
        let _guard = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "capability-draft-conversation-policy-test-{}",
            std::process::id()
        ));
        let previous_home = std::env::var_os("AGENT_HARNESS_HOME");
        unsafe {
            std::env::set_var("AGENT_HARNESS_HOME", &dir);
        }
        ConfigResolver::from_env()
            .save_agent_config(&AgentConfigFile {
                id: "critic".into(),
                name: "Critic".into(),
                system_prompt: "Review carefully.".into(),
                capability_drafts_enabled: Some(true),
                ..AgentConfigFile::default()
            })
            .unwrap();
        let store = ConversationStore::from_env();
        let conversation = store
            .create(Some("Review".into()), Some("critic".into()))
            .unwrap();
        store
            .set_policy(
                &conversation.id,
                ConversationPolicy {
                    capability_drafts_enabled: Some(false),
                    ..ConversationPolicy::default()
                },
            )
            .unwrap();

        let registry = build_registry(false, false, false, Some("critic"), Some(&conversation.id));
        assert!(!registry.contains(&ToolId::from("capability_draft")));

        let forced = build_registry(false, false, true, Some("critic"), Some(&conversation.id));
        assert!(forced.contains(&ToolId::from("capability_draft")));
        restore_env("AGENT_HARNESS_HOME", previous_home);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn conversation_policy_applies_to_built_agent_layer() {
        let mut agent = build_agent(&RuntimeOptions::default());
        apply_conversation_policy(
            &mut agent,
            &ConversationPolicy {
                load_memory: Some(false),
                generate_memory: None,
                allowed_tool_categories: Some(vec!["shell".into()]),
                allowed_skill_categories: Some(vec!["review".into()]),
                capability_drafts_enabled: Some(true),
                capability_draft_guidance: Some("Draft only reusable capability specs.".into()),
                max_tokens_before_compaction: Some(768),
                max_compaction_output_tokens: Some(144),
                compaction_guidance: Some("keep branch decisions".into()),
            },
        );

        assert_eq!(
            agent.context_policy.compaction.max_tokens_before_compaction,
            Some(768)
        );
        assert_eq!(agent.tool_policy.allowed_categories, vec!["shell"]);
        assert_eq!(agent.allowed_skill_categories, vec!["review"]);
        assert!(agent.tool_policy.capability_drafts_enabled);
        assert_eq!(
            agent.tool_policy.capability_draft_guidance.as_deref(),
            Some("Draft only reusable capability specs.")
        );
        assert_eq!(agent.context_policy.compaction.max_output_tokens, Some(144));
        assert_eq!(
            agent.context_policy.compaction.guidance.as_deref(),
            Some("keep branch decisions")
        );
    }

    #[test]
    fn build_agent_loads_profile_granted_memory_fragments() {
        let _guard = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir =
            std::env::temp_dir().join(format!("memory-grant-setup-test-{}", std::process::id()));
        let previous_home = std::env::var_os("AGENT_HARNESS_HOME");
        let previous_profile = std::env::var_os("AGENT_HARNESS_PROFILE");
        unsafe {
            std::env::set_var("AGENT_HARNESS_HOME", &dir);
            std::env::set_var("AGENT_HARNESS_PROFILE", "research");
        }

        let resolver = ConfigResolver::new(StoragePaths::new_with_profile(&dir, "research"));
        let _ = resolver.create_profile("research", Some("Research".into()));
        resolver
            .grant_profile_access("main", "research", ProfileGrantKind::Memory, "critic")
            .unwrap();
        agent_memory::MemoryStore::new(StoragePaths::new(&dir))
            .create_for_conversation_with_topics_for_agent(
                agent_memory::MemoryTarget::Agent,
                "Shared profile memory.",
                agent_memory::MemoryAuthor::Human,
                None,
                None,
                vec!["team".into()],
                Some("critic".into()),
            )
            .unwrap();
        agent_memory::MemoryStore::new(StoragePaths::new_with_profile(&dir, "research"))
            .create_with_topics(
                agent_memory::MemoryTarget::Agent,
                "Research local memory.",
                agent_memory::MemoryAuthor::Human,
                None,
                vec!["team".into()],
            )
            .unwrap();

        let agent = build_agent(&RuntimeOptions {
            load_memory: true,
            memory_topics: vec!["team".into()],
            ..RuntimeOptions::default()
        });

        assert_eq!(agent.memory_fragments.len(), 2);
        assert!(agent.memory_fragments.iter().any(|fragment| {
            fragment.content == "Research local memory."
                && fragment.provenance.contains("profile=research")
        }));
        assert!(agent.memory_fragments.iter().any(|fragment| {
            fragment.content == "Shared profile memory."
                && fragment.provenance.contains("shared_from_profile=main")
                && fragment
                    .provenance
                    .contains("source_backend=local-markdown-v0")
        }));

        restore_env("AGENT_HARNESS_HOME", previous_home);
        restore_env("AGENT_HARNESS_PROFILE", previous_profile);
        let _ = std::fs::remove_dir_all(dir);
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

        agent_core::apply_skill_visibility(&mut skill, VisibilityLevel::NameAndDescription);
        assert_eq!(skill.visibility, VisibilityLevel::NameAndDescription);
        assert!(skill.body.is_none());
        assert_eq!(skill.description.as_deref(), Some("Review description"));
        assert_eq!(skill.estimated_tokens, 0);

        agent_core::apply_skill_visibility(&mut skill, VisibilityLevel::NameOnly);
        assert_eq!(skill.visibility, VisibilityLevel::NameOnly);
        assert!(skill.body.is_none());
        assert!(skill.description.is_none());
        assert_eq!(skill.estimated_tokens, 0);
    }
}
