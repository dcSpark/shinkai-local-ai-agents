//! `agent-config` — v0 layered config resolver.
//!
//! Stage 2 starts with a single main-profile agent config file while preserving
//! provenance strings for `explain-config`. Later layers can slot into the same
//! returned shape without changing callers.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use agent_core::{
    AgentConfig, ApprovalControllerPolicy, ConfigValueExplanation, ContextCompactionPolicy,
    ContextPolicy, CostPolicy, ExecutionPolicy, PromptRefinement, ToolOutputMode, ToolPolicy,
    VisibilityLevel, VoiceConfig,
};
use agent_llm::{ModelRef, NativeProviderConfig, RigProviderConfig};
use agent_storage::StoragePaths;
use agent_tools::ToolId;
use serde::{Deserialize, Serialize};

const MODEL_METADATA_CATALOG_FILE: &str = "metadata-catalog.json";
const MODEL_METADATA_CATALOG_ENV: &str = "AGENT_MODEL_METADATA_CATALOG";
const MODEL_PROVIDER_CATALOG_FILE: &str = "provider-catalog.json";
const MODEL_PROVIDER_CATALOG_ENV: &str = "AGENT_MODEL_PROVIDER_CATALOG";
const BUNDLED_MODEL_METADATA_CATALOG_JSON: &str = include_str!("../model_metadata_catalog.json");

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("storage error: {0}")]
    Storage(#[from] agent_storage::StorageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid config input: {0}")]
    InvalidInput(String),
    #[error("profile already exists: {0}")]
    ProfileAlreadyExists(String),
    #[error("profile not found: {0}")]
    ProfileNotFound(String),
    #[error("profile grant not found: {0}")]
    ProfileGrantNotFound(String),
}

#[derive(Debug, Clone)]
pub struct ResolvedAgentConfig {
    pub agent: AgentConfig,
    pub values: Vec<ConfigValueExplanation>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PolicyLayerToml {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    voice: Option<VoiceConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_tool_calls: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_subagent_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_recursion_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_tool_categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approval_controller_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approval_controller_allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    approval_controller_allowed_tool_categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowed_skill_categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disabled_lifecycle_hooks: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_output_mode: Option<ToolOutputMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_output_interpretation_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_visibility: Option<VisibilityLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    load_memory: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    load_skills: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_tokens_before_compaction: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_compaction_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compaction_guidance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ingestion_guardrail: Option<IngestionGuardrailMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ingestion_guardrail_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input_cost_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    output_cost_per_million: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct GlobalToml {
    #[serde(flatten)]
    policy: PolicyLayerToml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileToml {
    id: String,
    name: String,
    #[serde(flatten)]
    policy: PolicyLayerToml,
}

impl Default for ProfileToml {
    fn default() -> Self {
        Self::for_id("main", "Main")
    }
}

impl ProfileToml {
    fn for_id(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            policy: PolicyLayerToml::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentToml {
    id: String,
    name: String,
    system_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt_refinement: Option<AgentPromptRefinementConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    prompt_refinements: Vec<AgentPromptRefinementConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_overrides: Vec<AgentToolOutputOverrideConfig>,
    #[serde(flatten)]
    policy: PolicyLayerToml,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPromptRefinementConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub agent_awareness: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentToolOutputOverrideConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_mode: Option<ToolOutputMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_interpretation_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_interpretation_guidance: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_enabled: Option<bool>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfigFile {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_refinement: Option<AgentPromptRefinementConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompt_refinements: Vec<AgentPromptRefinementConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_overrides: Vec<AgentToolOutputOverrideConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceConfigFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_subagent_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_recursion_depth: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tool_categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_controller_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_controller_allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_controller_allowed_tool_categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_skill_categories: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_lifecycle_hooks: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output_mode: Option<ToolOutputMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output_interpretation_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_visibility: Option<VisibilityLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_memory: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_skills: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens_before_compaction: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_compaction_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compaction_guidance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingestion_guardrail: Option<IngestionGuardrailMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingestion_guardrail_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cost_per_million: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cost_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IngestionGuardrailMode {
    Block,
    Warn,
    Allow,
}

impl IngestionGuardrailMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Warn => "warn",
            Self::Allow => "allow",
        }
    }

    pub fn from_config_str(value: &str) -> Option<Self> {
        match value {
            "block" => Some(Self::Block),
            "warn" => Some(Self::Warn),
            "allow" => Some(Self::Allow),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedValue<T> {
    value: T,
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_missing_api_key: Option<bool>,
    #[serde(default)]
    pub max_context_tokens: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub default_temperature: Option<f64>,
    #[serde(default)]
    pub available_modalities: Vec<String>,
    #[serde(default)]
    pub reasoning_mode: Option<String>,
    #[serde(default)]
    pub tool_support: Option<bool>,
    #[serde(default)]
    pub privacy_level: Option<String>,
    #[serde(default)]
    pub cost_tier: Option<String>,
    #[serde(default)]
    pub input_cost_per_million: Option<f64>,
    #[serde(default)]
    pub output_cost_per_million: Option<f64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelRuntimeConfig {
    pub provider: Option<String>,
    pub api_base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub allow_missing_api_key: Option<bool>,
    pub max_output_tokens: Option<u64>,
    pub default_temperature: Option<f64>,
    pub provider_options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProviderDescriptor {
    pub id: String,
    pub name: String,
    pub default_model: String,
    pub api_key_env: Option<String>,
    pub api_base_url: Option<String>,
    pub supports_api_base_url: bool,
    pub local: bool,
    pub native: bool,
    pub available_modalities: Vec<String>,
    pub tool_support: Option<bool>,
    pub reasoning_modes: Vec<String>,
    pub settings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub option_schema: Vec<ModelProviderOptionDescriptor>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProviderCatalog {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub providers: Vec<ModelProviderDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProviderOptionDescriptor {
    pub key: String,
    pub target: ModelProviderOptionTarget,
    pub label: String,
    pub kind: ModelProviderOptionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderOptionTarget {
    Runtime,
    ProviderOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderOptionKind {
    Number,
    Integer,
    String,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCapabilityProbe {
    pub model_id: String,
    pub saved_model: bool,
    pub provider: String,
    pub declared_modalities: Vec<String>,
    pub provider_modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub declared_limits: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub declared_pricing: BTreeMap<String, String>,
    pub tool_support: Option<bool>,
    pub live_probe: ModelLiveCapabilityProbe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelModalitySupport {
    pub model_id: String,
    pub provider: String,
    pub modality: String,
    pub supported: bool,
    pub available_modalities: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelLiveCapabilityProbe {
    pub attempted: bool,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_found: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reported_modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reported_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_tool_support: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reported_limits: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reported_pricing: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMetadataCatalog {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub models: Vec<ModelMetadataCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelMetadataCatalogEntry {
    pub provider: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modalities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_support: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub limits: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pricing: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
struct LoadedModelMetadataCatalog {
    source: String,
    catalog: ModelMetadataCatalog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LifecycleHookPolicyLayers {
    pub agent_id: String,
    pub profile: String,
    pub effective_source: String,
    pub effective_disabled_lifecycle_hooks: Vec<String>,
    pub global_disabled_lifecycle_hooks: Vec<String>,
    pub profile_disabled_lifecycle_hooks: Vec<String>,
    pub agent_disabled_lifecycle_hooks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileGrantKind {
    Agent,
    Memory,
    Tool,
    Skill,
    Category,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileGrant {
    pub id: String,
    pub from_profile: String,
    pub to_profile: String,
    pub kind: ProfileGrantKind,
    pub resource: String,
    pub created_at: String,
}

impl ModelConfig {
    pub fn for_id(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            provider: None,
            api_base_url: None,
            api_key_env: None,
            allow_missing_api_key: None,
            max_context_tokens: None,
            max_output_tokens: None,
            default_temperature: None,
            available_modalities: Vec::new(),
            reasoning_mode: None,
            tool_support: None,
            privacy_level: None,
            cost_tier: None,
            input_cost_per_million: None,
            output_cost_per_million: None,
            metadata: BTreeMap::new(),
        }
    }
}

impl ModelRuntimeConfig {
    pub fn rig_provider_config(
        &self,
        model: ModelRef,
        max_output_tokens: Option<u64>,
        temperature: Option<f64>,
    ) -> Result<RigProviderConfig, ConfigError> {
        let provider = self
            .provider
            .as_deref()
            .unwrap_or("rig")
            .trim()
            .to_ascii_lowercase();
        let mut config = match provider.as_str() {
            "rig" | "openai" | "openai_compatible" | "openai-compatible" => RigProviderConfig {
                api_base_url: None,
                api_key_env: "OPENAI_API_KEY".into(),
                allow_missing_api_key: false,
                model,
                max_output_tokens: None,
                temperature: None,
                additional_params: None,
            },
            "ollama" => RigProviderConfig::ollama(model),
            "llama_cpp" | "llama-cpp" | "llamacpp" => RigProviderConfig::llama_cpp(model),
            other => {
                return Err(ConfigError::InvalidInput(format!(
                    "unsupported OpenAI-compatible model provider: {other}"
                )));
            }
        };
        if let Some(api_base_url) = self
            .api_base_url
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            config.api_base_url = Some(api_base_url.to_string());
        }
        if let Some(api_key_env) = self
            .api_key_env
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            config.api_key_env = api_key_env.to_string();
        }
        if let Some(allow_missing_api_key) = self.allow_missing_api_key {
            config.allow_missing_api_key = allow_missing_api_key;
        }
        config.max_output_tokens = max_output_tokens.or(self.max_output_tokens);
        config.temperature = temperature.or(self.default_temperature);
        config.additional_params = self.provider_options.clone();
        Ok(config)
    }

    pub fn native_provider_config(
        &self,
        model: ModelRef,
        constructor: fn(ModelRef) -> NativeProviderConfig,
        max_output_tokens: Option<u64>,
        temperature: Option<f64>,
    ) -> NativeProviderConfig {
        let mut config = constructor(model);
        if let Some(api_key_env) = self
            .api_key_env
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            config.api_key_env = api_key_env.to_string();
        }
        if let Some(allow_missing_api_key) = self.allow_missing_api_key {
            config.allow_missing_api_key = allow_missing_api_key;
        }
        config.max_output_tokens = max_output_tokens.or(self.max_output_tokens);
        config.temperature = temperature.or(self.default_temperature);
        config.additional_params = self.provider_options.clone();
        config
    }
}

pub fn supported_model_providers() -> Vec<ModelProviderDescriptor> {
    builtin_model_providers()
}

pub fn configured_model_providers() -> Result<Vec<ModelProviderDescriptor>, ConfigError> {
    configured_model_providers_from_paths(&StoragePaths::from_env())
}

fn builtin_model_providers() -> Vec<ModelProviderDescriptor> {
    vec![
        ModelProviderDescriptor {
            id: "fake".into(),
            name: "Fake".into(),
            default_model: "fake-model".into(),
            api_key_env: None,
            api_base_url: None,
            supports_api_base_url: false,
            local: true,
            native: true,
            available_modalities: vec!["text".into()],
            tool_support: Some(false),
            reasoning_modes: vec!["scripted".into()],
            settings: vec!["max_output_tokens".into(), "temperature".into()],
            option_schema: provider_option_schema(false, false, Vec::new()),
            notes: Some("Deterministic offline provider for tests and demos.".into()),
        },
        ModelProviderDescriptor {
            id: "rig".into(),
            name: "OpenAI-compatible".into(),
            default_model: "gpt-4o-mini".into(),
            api_key_env: Some("OPENAI_API_KEY".into()),
            api_base_url: None,
            supports_api_base_url: true,
            local: false,
            native: false,
            available_modalities: vec!["text".into(), "image".into()],
            tool_support: Some(true),
            reasoning_modes: vec!["model-default".into()],
            settings: vec![
                "api_base_url".into(),
                "api_key_env".into(),
                "max_output_tokens".into(),
                "provider_options".into(),
                "temperature".into(),
            ],
            option_schema: provider_option_schema(true, true, openai_compatible_provider_options()),
            notes: Some("Use this for OpenAI-compatible cloud or custom /v1 endpoints.".into()),
        },
        ModelProviderDescriptor {
            id: "ollama".into(),
            name: "Ollama".into(),
            default_model: "llama3.1".into(),
            api_key_env: Some("OLLAMA_API_KEY".into()),
            api_base_url: Some("http://127.0.0.1:11434/v1".into()),
            supports_api_base_url: true,
            local: true,
            native: false,
            available_modalities: vec!["text".into()],
            tool_support: None,
            reasoning_modes: vec!["model-default".into()],
            settings: vec![
                "api_base_url".into(),
                "api_key_env".into(),
                "max_output_tokens".into(),
                "provider_options".into(),
                "temperature".into(),
            ],
            option_schema: provider_option_schema(true, true, local_provider_options()),
            notes: Some(
                "Local OpenAI-compatible endpoint; tool support depends on the model.".into(),
            ),
        },
        ModelProviderDescriptor {
            id: "llama_cpp".into(),
            name: "llama.cpp server".into(),
            default_model: "local-model".into(),
            api_key_env: Some("LLAMA_CPP_API_KEY".into()),
            api_base_url: Some("http://127.0.0.1:8080/v1".into()),
            supports_api_base_url: true,
            local: true,
            native: false,
            available_modalities: vec!["text".into()],
            tool_support: None,
            reasoning_modes: vec!["model-default".into()],
            settings: vec![
                "api_base_url".into(),
                "api_key_env".into(),
                "max_output_tokens".into(),
                "provider_options".into(),
                "temperature".into(),
            ],
            option_schema: provider_option_schema(true, true, local_provider_options()),
            notes: Some(
                "Local OpenAI-compatible endpoint; capabilities depend on the served model.".into(),
            ),
        },
        ModelProviderDescriptor {
            id: "anthropic".into(),
            name: "Anthropic".into(),
            default_model: "claude-sonnet-4-5".into(),
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            api_base_url: None,
            supports_api_base_url: false,
            local: false,
            native: true,
            available_modalities: vec!["text".into(), "image".into()],
            tool_support: Some(true),
            reasoning_modes: vec!["model-default".into()],
            settings: vec![
                "api_key_env".into(),
                "max_output_tokens".into(),
                "provider_options".into(),
                "temperature".into(),
            ],
            option_schema: provider_option_schema(false, true, anthropic_provider_options()),
            notes: Some("Native Anthropic Messages API wrapper.".into()),
        },
        ModelProviderDescriptor {
            id: "gemini".into(),
            name: "Google Gemini".into(),
            default_model: "gemini-2.5-flash".into(),
            api_key_env: Some("GEMINI_API_KEY".into()),
            api_base_url: None,
            supports_api_base_url: false,
            local: false,
            native: true,
            available_modalities: vec!["text".into(), "image".into()],
            tool_support: Some(true),
            reasoning_modes: vec!["model-default".into()],
            settings: vec![
                "api_key_env".into(),
                "max_output_tokens".into(),
                "provider_options".into(),
                "temperature".into(),
            ],
            option_schema: provider_option_schema(false, true, gemini_provider_options()),
            notes: Some("Native Gemini GenerateContent wrapper.".into()),
        },
    ]
}

fn configured_model_providers_from_paths(
    paths: &StoragePaths,
) -> Result<Vec<ModelProviderDescriptor>, ConfigError> {
    let mut providers = builtin_model_providers();
    if let Some(catalog) = load_model_provider_catalog(paths)? {
        merge_model_provider_catalog(&mut providers, catalog)?;
    }
    Ok(providers)
}

fn load_model_provider_catalog(
    paths: &StoragePaths,
) -> Result<Option<ModelProviderCatalog>, ConfigError> {
    if let Some(path) = std::env::var_os(MODEL_PROVIDER_CATALOG_ENV)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return read_model_provider_catalog(&path).map(Some);
    }

    let profile_path = paths.models_dir().join(MODEL_PROVIDER_CATALOG_FILE);
    if profile_path.exists() {
        return read_model_provider_catalog(&profile_path).map(Some);
    }

    Ok(None)
}

fn read_model_provider_catalog(path: &Path) -> Result<ModelProviderCatalog, ConfigError> {
    let catalog: ModelProviderCatalog = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    validate_model_provider_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_model_provider_catalog(catalog: &ModelProviderCatalog) -> Result<(), ConfigError> {
    if catalog.schema_version != 1 {
        return Err(ConfigError::InvalidInput(format!(
            "unsupported model provider catalog schema_version {}; expected 1",
            catalog.schema_version
        )));
    }
    let mut ids = BTreeSet::new();
    for provider in &catalog.providers {
        let Some(id) = normalized_provider(Some(&provider.id)) else {
            return Err(ConfigError::InvalidInput(
                "model provider catalog entries require provider id".into(),
            ));
        };
        if !ids.insert(id.clone()) {
            return Err(ConfigError::InvalidInput(format!(
                "duplicate model provider catalog entry: {id}"
            )));
        }
        if provider.name.trim().is_empty() {
            return Err(ConfigError::InvalidInput(format!(
                "model provider catalog entry {id} requires name"
            )));
        }
        if provider.default_model.trim().is_empty() {
            return Err(ConfigError::InvalidInput(format!(
                "model provider catalog entry {id} requires default_model"
            )));
        }
    }
    Ok(())
}

fn merge_model_provider_catalog(
    providers: &mut Vec<ModelProviderDescriptor>,
    catalog: ModelProviderCatalog,
) -> Result<(), ConfigError> {
    for mut provider in catalog.providers {
        provider.id = normalized_provider(Some(&provider.id)).ok_or_else(|| {
            ConfigError::InvalidInput("model provider catalog entries require provider id".into())
        })?;
        if let Some(existing) = providers
            .iter_mut()
            .find(|existing| existing.id == provider.id)
        {
            *existing = provider;
        } else {
            providers.push(provider);
        }
    }
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(())
}

fn provider_option_schema(
    include_api_base_url: bool,
    include_api_key_env: bool,
    provider_options: Vec<ModelProviderOptionDescriptor>,
) -> Vec<ModelProviderOptionDescriptor> {
    let mut schema = Vec::new();
    if include_api_base_url {
        schema.push(runtime_option(
            "api_base_url",
            "API base URL",
            ModelProviderOptionKind::String,
            None,
            None,
            Vec::new(),
            Some("OpenAI-compatible /v1 endpoint override.".into()),
        ));
    }
    if include_api_key_env {
        schema.push(runtime_option(
            "api_key_env",
            "API key environment variable",
            ModelProviderOptionKind::String,
            None,
            None,
            Vec::new(),
            None,
        ));
    }
    schema.push(runtime_option(
        "max_output_tokens",
        "Max output tokens",
        ModelProviderOptionKind::Integer,
        Some(1.0),
        None,
        Vec::new(),
        None,
    ));
    schema.push(runtime_option(
        "temperature",
        "Temperature",
        ModelProviderOptionKind::Number,
        Some(0.0),
        Some(2.0),
        Vec::new(),
        None,
    ));
    schema.extend(provider_options);
    schema
}

fn openai_compatible_provider_options() -> Vec<ModelProviderOptionDescriptor> {
    let mut options = common_sampling_provider_options(true);
    options.push(provider_option(
        "reasoning_effort",
        "Reasoning effort",
        ModelProviderOptionKind::String,
        None,
        None,
        vec!["low".into(), "medium".into(), "high".into()],
        Some("Only save this for OpenAI-compatible reasoning models.".into()),
    ));
    options.push(provider_option(
        "frequency_penalty",
        "Frequency penalty",
        ModelProviderOptionKind::Number,
        Some(-2.0),
        Some(2.0),
        Vec::new(),
        None,
    ));
    options.push(provider_option(
        "presence_penalty",
        "Presence penalty",
        ModelProviderOptionKind::Number,
        Some(-2.0),
        Some(2.0),
        Vec::new(),
        None,
    ));
    options
}

fn local_provider_options() -> Vec<ModelProviderOptionDescriptor> {
    common_sampling_provider_options(true)
}

fn anthropic_provider_options() -> Vec<ModelProviderOptionDescriptor> {
    common_sampling_provider_options(false)
}

fn gemini_provider_options() -> Vec<ModelProviderOptionDescriptor> {
    common_sampling_provider_options(true)
}

fn common_sampling_provider_options(include_top_k: bool) -> Vec<ModelProviderOptionDescriptor> {
    let mut options = vec![provider_option(
        "top_p",
        "Top p",
        ModelProviderOptionKind::Number,
        Some(0.0),
        Some(1.0),
        Vec::new(),
        None,
    )];
    if include_top_k {
        options.push(provider_option(
            "top_k",
            "Top k",
            ModelProviderOptionKind::Integer,
            Some(1.0),
            None,
            Vec::new(),
            None,
        ));
    }
    options
}

fn runtime_option(
    key: &str,
    label: &str,
    kind: ModelProviderOptionKind,
    min: Option<f64>,
    max: Option<f64>,
    allowed_values: Vec<String>,
    notes: Option<String>,
) -> ModelProviderOptionDescriptor {
    model_provider_option(
        key,
        ModelProviderOptionTarget::Runtime,
        label,
        kind,
        min,
        max,
        allowed_values,
        notes,
    )
}

fn provider_option(
    key: &str,
    label: &str,
    kind: ModelProviderOptionKind,
    min: Option<f64>,
    max: Option<f64>,
    allowed_values: Vec<String>,
    notes: Option<String>,
) -> ModelProviderOptionDescriptor {
    model_provider_option(
        key,
        ModelProviderOptionTarget::ProviderOptions,
        label,
        kind,
        min,
        max,
        allowed_values,
        notes,
    )
}

#[allow(clippy::too_many_arguments)]
fn model_provider_option(
    key: &str,
    target: ModelProviderOptionTarget,
    label: &str,
    kind: ModelProviderOptionKind,
    min: Option<f64>,
    max: Option<f64>,
    allowed_values: Vec<String>,
    notes: Option<String>,
) -> ModelProviderOptionDescriptor {
    ModelProviderOptionDescriptor {
        key: key.into(),
        target,
        label: label.into(),
        kind,
        min,
        max,
        allowed_values,
        notes,
    }
}

impl Default for AgentToml {
    fn default() -> Self {
        Self {
            id: "fake-agent".into(),
            name: "Fake Agent".into(),
            system_prompt: "You echo what the user says.".into(),
            prompt_refinement: None,
            prompt_refinements: Vec::new(),
            tool_overrides: Vec::new(),
            policy: PolicyLayerToml {
                model: Some("fake-model".into()),
                voice: None,
                max_tool_calls: Some(5),
                max_subagent_depth: None,
                max_recursion_depth: None,
                allowed_tools: Some(Vec::new()),
                allowed_tool_categories: None,
                approval_controller_agent: None,
                approval_controller_allowed_tools: None,
                approval_controller_allowed_tool_categories: None,
                allowed_skill_categories: None,
                disabled_lifecycle_hooks: None,
                tool_output_mode: Some(default_tool_output_mode()),
                tool_output_interpretation_model: None,
                tool_visibility: Some(default_tool_visibility()),
                load_memory: None,
                load_skills: None,
                max_tokens_before_compaction: None,
                max_compaction_output_tokens: None,
                compaction_guidance: None,
                ingestion_guardrail: None,
                ingestion_guardrail_model: None,
                input_cost_per_million: None,
                output_cost_per_million: None,
            },
        }
    }
}

impl From<AgentToml> for AgentConfigFile {
    fn from(value: AgentToml) -> Self {
        Self {
            id: value.id,
            name: value.name,
            system_prompt: value.system_prompt,
            prompt_refinement: value.prompt_refinement,
            prompt_refinements: value.prompt_refinements,
            tool_overrides: value.tool_overrides,
            voice: value.policy.voice,
            model: value.policy.model,
            max_tool_calls: value.policy.max_tool_calls,
            max_subagent_depth: value.policy.max_subagent_depth,
            max_recursion_depth: value.policy.max_recursion_depth,
            allowed_tools: value.policy.allowed_tools,
            allowed_tool_categories: value.policy.allowed_tool_categories,
            approval_controller_agent: value.policy.approval_controller_agent,
            approval_controller_allowed_tools: value.policy.approval_controller_allowed_tools,
            approval_controller_allowed_tool_categories: value
                .policy
                .approval_controller_allowed_tool_categories,
            allowed_skill_categories: value.policy.allowed_skill_categories,
            disabled_lifecycle_hooks: value.policy.disabled_lifecycle_hooks,
            tool_output_mode: value.policy.tool_output_mode,
            tool_output_interpretation_model: value.policy.tool_output_interpretation_model,
            tool_visibility: value.policy.tool_visibility,
            load_memory: value.policy.load_memory,
            load_skills: value.policy.load_skills,
            max_tokens_before_compaction: value.policy.max_tokens_before_compaction,
            max_compaction_output_tokens: value.policy.max_compaction_output_tokens,
            compaction_guidance: value.policy.compaction_guidance,
            ingestion_guardrail: value.policy.ingestion_guardrail,
            ingestion_guardrail_model: value.policy.ingestion_guardrail_model,
            input_cost_per_million: value.policy.input_cost_per_million,
            output_cost_per_million: value.policy.output_cost_per_million,
        }
    }
}

impl From<AgentConfigFile> for AgentToml {
    fn from(value: AgentConfigFile) -> Self {
        Self {
            id: value.id,
            name: value.name,
            system_prompt: value.system_prompt,
            prompt_refinement: value.prompt_refinement,
            prompt_refinements: value.prompt_refinements,
            tool_overrides: value.tool_overrides,
            policy: PolicyLayerToml {
                model: value.model,
                voice: value.voice,
                max_tool_calls: value.max_tool_calls,
                max_subagent_depth: value.max_subagent_depth,
                max_recursion_depth: value.max_recursion_depth,
                allowed_tools: value.allowed_tools,
                allowed_tool_categories: value.allowed_tool_categories,
                approval_controller_agent: value.approval_controller_agent,
                approval_controller_allowed_tools: value.approval_controller_allowed_tools,
                approval_controller_allowed_tool_categories: value
                    .approval_controller_allowed_tool_categories,
                allowed_skill_categories: value.allowed_skill_categories,
                disabled_lifecycle_hooks: value.disabled_lifecycle_hooks,
                tool_output_mode: value.tool_output_mode,
                tool_output_interpretation_model: value.tool_output_interpretation_model,
                tool_visibility: value.tool_visibility,
                load_memory: value.load_memory,
                load_skills: value.load_skills,
                max_tokens_before_compaction: value.max_tokens_before_compaction,
                max_compaction_output_tokens: value.max_compaction_output_tokens,
                compaction_guidance: value.compaction_guidance,
                ingestion_guardrail: value.ingestion_guardrail,
                ingestion_guardrail_model: value.ingestion_guardrail_model,
                input_cost_per_million: value.input_cost_per_million,
                output_cost_per_million: value.output_cost_per_million,
            },
        }
    }
}

impl Default for AgentConfigFile {
    fn default() -> Self {
        AgentToml::default().into()
    }
}

fn default_tool_output_mode() -> ToolOutputMode {
    ToolOutputMode::Interpreted
}

fn default_tool_visibility() -> VisibilityLevel {
    VisibilityLevel::FullSchema
}

pub struct ConfigResolver {
    paths: StoragePaths,
}

impl ConfigResolver {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn ensure_default_files(&self) -> Result<(), ConfigError> {
        self.paths.ensure_base_dirs()?;
        let global_path = self.paths.global_config();
        if !global_path.exists() {
            let text = toml::to_string_pretty(&GlobalToml::default())?;
            write_storage_text(&self.paths, global_path, text)?;
        }
        let main_profile_path = self.paths.main_profile_config();
        if !main_profile_path.exists() {
            self.paths.ensure_profile_dirs("main")?;
            let text = toml::to_string_pretty(&ProfileToml::default())?;
            write_storage_text(&self.paths, main_profile_path, text)?;
        }
        let profile_path = self.paths.active_profile_config();
        if !profile_path.exists() {
            let active_profile_id = self.paths.active_profile_id();
            let display_name = if active_profile_id == "main" {
                "Main".to_string()
            } else {
                active_profile_id.to_string()
            };
            let text =
                toml::to_string_pretty(&ProfileToml::for_id(active_profile_id, display_name))?;
            write_storage_text(&self.paths, profile_path, text)?;
        }
        let agent_path = self.paths.default_agent_config();
        if !agent_path.exists() {
            let text = toml::to_string_pretty(&AgentToml::default())?;
            write_storage_text(&self.paths, agent_path, text)?;
        }
        let model_path = self.paths.model_config("fake-model");
        if !model_path.exists() {
            let text = toml::to_string_pretty(&ModelConfig::for_id("fake-model"))?;
            write_storage_text(&self.paths, model_path, text)?;
        }
        Ok(())
    }

    pub fn resolve_default_agent(&self) -> Result<ResolvedAgentConfig, ConfigError> {
        self.resolve_agent("fake-agent")
    }

    pub fn resolve_agent(&self, id: &str) -> Result<ResolvedAgentConfig, ConfigError> {
        self.ensure_default_files()?;
        validate_agent_id(id)?;
        match self.resolve_agent_in_paths(&self.paths, id) {
            Ok(resolved) => return Ok(resolved),
            Err(ConfigError::InvalidInput(_)) => {}
            Err(err) => return Err(err),
        }
        let active_profile = self.paths.active_profile_id().to_string();
        if let Some(grant) = self.list_profile_grants()?.into_iter().find(|grant| {
            grant.kind == ProfileGrantKind::Agent
                && grant.to_profile == active_profile
                && (grant.resource == "*" || grant.resource == id)
        }) {
            let source_paths = StoragePaths::new_with_profile(
                self.paths.root().to_path_buf(),
                &grant.from_profile,
            );
            let source_resolver = ConfigResolver::new(source_paths);
            source_resolver.ensure_default_files()?;
            let mut resolved =
                source_resolver.resolve_agent_in_paths(&source_resolver.paths, id)?;
            resolved.values.push(config_value(
                "agent.shared_from_profile",
                grant.from_profile,
                &format!("profile-grant:{}", grant.id),
            ));
            return Ok(resolved);
        }
        Err(ConfigError::InvalidInput(format!("agent not found: {id}")))
    }

    fn resolve_agent_in_paths(
        &self,
        paths: &StoragePaths,
        id: &str,
    ) -> Result<ResolvedAgentConfig, ConfigError> {
        let global_path = paths.global_config();
        let global: GlobalToml = toml::from_str(&std::fs::read_to_string(&global_path)?)?;
        validate_voice_config_file(global.policy.voice.as_ref())?;
        validate_lifecycle_hook_ids(global.policy.disabled_lifecycle_hooks.as_deref())?;
        let profile_path = paths.active_profile_config();
        let profile: ProfileToml = toml::from_str(&std::fs::read_to_string(&profile_path)?)?;
        validate_voice_config_file(profile.policy.voice.as_ref())?;
        validate_lifecycle_hook_ids(profile.policy.disabled_lifecycle_hooks.as_deref())?;
        let path = paths.agent_config(id);
        if !path.exists() {
            return Err(ConfigError::InvalidInput(format!("agent not found: {id}")));
        }
        let text = std::fs::read_to_string(&path)?;
        let parsed: AgentToml = toml::from_str(&text)?;
        validate_voice_config_file(parsed.policy.voice.as_ref())?;
        validate_lifecycle_hook_ids(parsed.policy.disabled_lifecycle_hooks.as_deref())?;
        let global_source = format!("global:{}", global_path.display());
        let profile_source = format!("profile:{}", profile_path.display());
        let agent_source = format!("agent:{}", path.display());
        let model_id = resolve_layered(
            "fake-model".to_string(),
            "default:fake-model".into(),
            vec![
                (global.policy.model.clone(), global_source.clone()),
                (profile.policy.model.clone(), profile_source.clone()),
                (parsed.policy.model.clone(), agent_source.clone()),
            ],
        );
        let model_path = paths.model_config(&model_id.value);
        let model = if model_path.exists() {
            Some(toml::from_str(&std::fs::read_to_string(&model_path)?)?)
        } else {
            None
        };
        Ok(resolve_agent(
            parsed,
            path,
            global,
            global_path,
            profile,
            profile_path,
            model_id,
            model,
            model_path,
        ))
    }

    pub fn list_agent_configs(&self) -> Result<Vec<AgentSummary>, ConfigError> {
        self.ensure_default_files()?;
        let mut agents = Vec::new();
        for entry in std::fs::read_dir(self.paths.agents_dir())? {
            let entry = entry?;
            if entry.path().is_dir() {
                let path = entry.path().join("agent.toml");
                if path.exists() {
                    agents.push(agent_summary(read_agent_config(&path)?, path));
                }
            }
        }
        agents.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(agents)
    }

    pub fn resolve_subagent_configs(
        &self,
        parent_agent_id: &str,
    ) -> Result<Vec<AgentConfig>, ConfigError> {
        let mut agents = Vec::new();
        for agent_id in self.list_subagent_agent_ids(parent_agent_id)? {
            let mut agent = self.resolve_agent(&agent_id)?.agent;
            agent.conversation_history.clear();
            agent.compacted_context = None;
            agent.memory_fragments.clear();
            agent.ingestion_artifacts.clear();
            agent.skill_views.clear();
            agent.subagent_configs.clear();
            agents.push(agent);
        }
        Ok(agents)
    }

    pub fn list_subagent_agent_ids(
        &self,
        parent_agent_id: &str,
    ) -> Result<Vec<String>, ConfigError> {
        validate_agent_id(parent_agent_id)?;
        Ok(self
            .list_agent_configs()?
            .into_iter()
            .filter(|summary| summary.id != parent_agent_id)
            .map(|summary| summary.id)
            .collect())
    }

    pub fn show_agent_config(&self, id: &str) -> Result<Option<AgentConfigFile>, ConfigError> {
        self.ensure_default_files()?;
        validate_agent_id(id)?;
        let path = self.paths.agent_config(id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_agent_config(&path)?.into()))
    }

    pub fn save_agent_config(
        &self,
        agent: &AgentConfigFile,
    ) -> Result<AgentConfigFile, ConfigError> {
        self.ensure_default_files()?;
        validate_agent_config(agent)?;
        let path = self.paths.agent_config(&agent.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_storage_text(&self.paths, &path, toml::to_string_pretty(agent)?)?;
        Ok(agent.clone())
    }

    pub fn disabled_lifecycle_hooks_for_agent(&self, id: &str) -> Result<Vec<String>, ConfigError> {
        Ok(self
            .lifecycle_hook_policy_layers_for_agent(id)?
            .effective_disabled_lifecycle_hooks)
    }

    pub fn lifecycle_hook_policy_layers_for_agent(
        &self,
        id: &str,
    ) -> Result<LifecycleHookPolicyLayers, ConfigError> {
        self.ensure_default_files()?;
        validate_agent_id(id)?;
        let global_path = self.paths.global_config();
        let profile_path = self.paths.active_profile_config();
        let agent_path = self.paths.agent_config(id);
        let global: GlobalToml = toml::from_str(&std::fs::read_to_string(&global_path)?)?;
        let profile: ProfileToml = read_profile_config(&profile_path)?;
        let agent = if agent_path.exists() {
            Some(read_agent_config(&agent_path)?)
        } else {
            None
        };
        let global_raw = global.policy.disabled_lifecycle_hooks;
        let profile_raw = profile.policy.disabled_lifecycle_hooks;
        let agent_raw = agent.and_then(|agent| agent.policy.disabled_lifecycle_hooks);
        let global_hooks = normalize_lifecycle_hook_ids(global_raw.clone().unwrap_or_default())?;
        let profile_hooks = normalize_lifecycle_hook_ids(profile_raw.clone().unwrap_or_default())?;
        let agent_hooks = normalize_lifecycle_hook_ids(agent_raw.clone().unwrap_or_default())?;
        let (effective_source, effective_hooks) = if let Some(hooks) = agent_raw {
            ("agent", normalize_lifecycle_hook_ids(hooks)?)
        } else if let Some(hooks) = profile_raw {
            ("profile", normalize_lifecycle_hook_ids(hooks)?)
        } else if let Some(hooks) = global_raw {
            ("global", normalize_lifecycle_hook_ids(hooks)?)
        } else {
            ("default", Vec::new())
        };
        Ok(LifecycleHookPolicyLayers {
            agent_id: id.into(),
            profile: self.paths.active_profile_id().to_string(),
            effective_source: effective_source.into(),
            effective_disabled_lifecycle_hooks: effective_hooks,
            global_disabled_lifecycle_hooks: global_hooks,
            profile_disabled_lifecycle_hooks: profile_hooks,
            agent_disabled_lifecycle_hooks: agent_hooks,
        })
    }

    pub fn global_disabled_lifecycle_hooks(&self) -> Result<Vec<String>, ConfigError> {
        self.ensure_default_files()?;
        let global_path = self.paths.global_config();
        let global: GlobalToml = toml::from_str(&std::fs::read_to_string(&global_path)?)?;
        normalize_lifecycle_hook_ids(global.policy.disabled_lifecycle_hooks.unwrap_or_default())
    }

    pub fn profile_disabled_lifecycle_hooks(&self) -> Result<Vec<String>, ConfigError> {
        self.ensure_default_files()?;
        let profile_path = self.paths.active_profile_config();
        let profile: ProfileToml = read_profile_config(&profile_path)?;
        normalize_lifecycle_hook_ids(profile.policy.disabled_lifecycle_hooks.unwrap_or_default())
    }

    pub fn agent_disabled_lifecycle_hooks(&self, id: &str) -> Result<Vec<String>, ConfigError> {
        self.ensure_default_files()?;
        validate_agent_id(id)?;
        let agent_path = self.paths.agent_config(id);
        if !agent_path.exists() {
            return Ok(Vec::new());
        }
        let agent = read_agent_config(&agent_path)?;
        normalize_lifecycle_hook_ids(agent.policy.disabled_lifecycle_hooks.unwrap_or_default())
    }

    pub fn set_profile_lifecycle_hook_disabled(
        &self,
        hook_id: &str,
        disabled: bool,
    ) -> Result<Vec<String>, ConfigError> {
        self.ensure_default_files()?;
        let hook_id = validate_lifecycle_hook_id(hook_id)?;
        let profile_path = self.paths.active_profile_config();
        let mut profile: ProfileToml = read_profile_config(&profile_path)?;
        let mut hooks = normalize_lifecycle_hook_ids(
            profile
                .policy
                .disabled_lifecycle_hooks
                .clone()
                .unwrap_or_default(),
        )?
        .into_iter()
        .collect::<BTreeSet<_>>();
        if disabled {
            hooks.insert(hook_id);
        } else {
            hooks.remove(&hook_id);
        }
        let hooks = hooks.into_iter().collect::<Vec<_>>();
        profile.policy.disabled_lifecycle_hooks = (!hooks.is_empty()).then_some(hooks.clone());
        write_storage_text(&self.paths, profile_path, toml::to_string_pretty(&profile)?)?;
        Ok(hooks)
    }

    pub fn set_agent_lifecycle_hook_disabled(
        &self,
        agent_id: &str,
        hook_id: &str,
        disabled: bool,
    ) -> Result<Vec<String>, ConfigError> {
        self.ensure_default_files()?;
        validate_agent_id(agent_id)?;
        let hook_id = validate_lifecycle_hook_id(hook_id)?;
        let agent_path = self.paths.agent_config(agent_id);
        if !agent_path.exists() {
            return Err(ConfigError::InvalidInput(format!(
                "agent config {agent_id:?} not found"
            )));
        }
        let mut agent = read_agent_config(&agent_path)?;
        let mut hooks = normalize_lifecycle_hook_ids(
            agent
                .policy
                .disabled_lifecycle_hooks
                .clone()
                .unwrap_or_default(),
        )?
        .into_iter()
        .collect::<BTreeSet<_>>();
        if disabled {
            hooks.insert(hook_id);
        } else {
            hooks.remove(&hook_id);
        }
        let hooks = hooks.into_iter().collect::<Vec<_>>();
        agent.policy.disabled_lifecycle_hooks = (!hooks.is_empty()).then_some(hooks.clone());
        write_storage_text(&self.paths, agent_path, toml::to_string_pretty(&agent)?)?;
        Ok(hooks)
    }

    pub fn delete_agent_config(&self, id: &str) -> Result<bool, ConfigError> {
        self.ensure_default_files()?;
        validate_agent_id(id)?;
        if id == "fake-agent" {
            return Err(ConfigError::InvalidInput(
                "default agent cannot be deleted".into(),
            ));
        }
        let dir = self.paths.agents_dir().join(id);
        if !dir.exists() {
            return Ok(false);
        }
        std::fs::remove_dir_all(dir)?;
        Ok(true)
    }

    pub fn export_agent_config(
        &self,
        id: &str,
        path: impl AsRef<Path>,
    ) -> Result<AgentConfigFile, ConfigError> {
        let Some(agent) = self.show_agent_config(id)? else {
            return Err(ConfigError::InvalidInput(format!("agent not found: {id}")));
        };
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(&agent)?)?;
        Ok(agent)
    }

    pub fn import_agent_config(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<AgentConfigFile, ConfigError> {
        let agent: AgentConfigFile = toml::from_str(&std::fs::read_to_string(path)?)?;
        self.save_agent_config(&agent)
    }

    pub fn promote_agent_created_config(
        &self,
        draft_id: &str,
        name: &str,
        body: &str,
    ) -> Result<AgentConfigFile, ConfigError> {
        let draft_id = non_empty(draft_id, "draft_id")?;
        let name = non_empty(name, "name")?;
        let body = non_empty(body, "body")?;
        let id = agent_id_for_draft(&draft_id, &name);
        let mut agent = parse_agent_draft_body(&body).unwrap_or_else(|| AgentConfigFile {
            id: id.clone(),
            name: name.clone(),
            system_prompt: body.clone(),
            ..AgentConfigFile::default()
        });
        agent.id = id;
        if agent.name.trim().is_empty() {
            agent.name = name;
        }
        self.save_agent_config(&agent)
    }

    pub fn delete_agent_created_config(
        &self,
        draft_id: &str,
        name: &str,
    ) -> Result<bool, ConfigError> {
        let draft_id = non_empty(draft_id, "draft_id")?;
        let name = non_empty(name, "name")?;
        self.delete_agent_config(&agent_id_for_draft(&draft_id, &name))
    }

    pub fn create_profile(
        &self,
        id: &str,
        name: Option<String>,
    ) -> Result<ProfileSummary, ConfigError> {
        self.ensure_default_files()?;
        validate_profile_id(id)?;
        let path = self.paths.profile_config(id);
        if path.exists() {
            return Err(ConfigError::ProfileAlreadyExists(id.into()));
        }
        self.paths.ensure_profile_dirs(id)?;
        let profile = ProfileToml {
            id: id.into(),
            name: clean_optional(name).unwrap_or_else(|| id.into()),
            policy: PolicyLayerToml::default(),
        };
        write_storage_text(&self.paths, &path, toml::to_string_pretty(&profile)?)?;
        Ok(profile_summary(profile, path))
    }

    pub fn list_profiles(&self) -> Result<Vec<ProfileSummary>, ConfigError> {
        self.ensure_default_files()?;
        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(self.paths.profiles_dir())? {
            let entry = entry?;
            if entry.path().is_dir() {
                let path = entry.path().join("profile.toml");
                if path.exists() {
                    profiles.push(profile_summary(read_profile_config(&path)?, path));
                }
            }
        }
        profiles.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(profiles)
    }

    pub fn show_profile(&self, id: &str) -> Result<ProfileSummary, ConfigError> {
        self.ensure_default_files()?;
        validate_profile_id(id)?;
        let path = self.paths.profile_config(id);
        if !path.exists() {
            return Err(ConfigError::ProfileNotFound(id.into()));
        }
        Ok(profile_summary(read_profile_config(&path)?, path))
    }

    pub fn delete_profile(&self, id: &str) -> Result<bool, ConfigError> {
        self.ensure_default_files()?;
        validate_profile_id(id)?;
        if id == "main" {
            return Err(ConfigError::InvalidInput(
                "main profile cannot be deleted".into(),
            ));
        }
        let dir = self.paths.profile_dir(id);
        if !dir.exists() {
            return Ok(false);
        }
        std::fs::remove_dir_all(dir)?;
        Ok(true)
    }

    pub fn grant_profile_access(
        &self,
        from_profile: &str,
        to_profile: &str,
        kind: ProfileGrantKind,
        resource: &str,
    ) -> Result<ProfileGrant, ConfigError> {
        self.ensure_default_files()?;
        self.show_profile(from_profile)?;
        self.show_profile(to_profile)?;
        if from_profile == to_profile {
            return Err(ConfigError::InvalidInput(
                "profile grants must target a different profile".into(),
            ));
        }
        let resource = validate_resource_id(resource)?;
        let mut grants = self.list_profile_grants_from(from_profile)?;
        let grant = ProfileGrant {
            id: grant_id(from_profile, to_profile, kind, &resource),
            from_profile: from_profile.into(),
            to_profile: to_profile.into(),
            kind,
            resource,
            created_at: timestamp_string(),
        };
        if let Some(existing) = grants.iter_mut().find(|entry| entry.id == grant.id) {
            *existing = grant.clone();
        } else {
            grants.push(grant.clone());
        }
        write_profile_grants(&self.paths, from_profile, &grants)?;
        Ok(grant)
    }

    pub fn list_profile_grants(&self) -> Result<Vec<ProfileGrant>, ConfigError> {
        self.ensure_default_files()?;
        let mut grants = Vec::new();
        for profile in self.list_profiles()? {
            grants.extend(self.list_profile_grants_from(&profile.id)?);
        }
        grants.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(grants)
    }

    pub fn list_profile_grants_from(
        &self,
        from_profile: &str,
    ) -> Result<Vec<ProfileGrant>, ConfigError> {
        self.ensure_default_files()?;
        validate_profile_id(from_profile)?;
        let path = self.paths.profile_grants_file(from_profile);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let mut grants: Vec<ProfileGrant> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        grants.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(grants)
    }

    pub fn revoke_profile_grant(&self, id: &str) -> Result<ProfileGrant, ConfigError> {
        self.ensure_default_files()?;
        validate_grant_id(id)?;
        for profile in self.list_profiles()? {
            let mut grants = self.list_profile_grants_from(&profile.id)?;
            if let Some(idx) = grants.iter().position(|entry| entry.id == id) {
                let removed = grants.remove(idx);
                write_profile_grants(&self.paths, &profile.id, &grants)?;
                return Ok(removed);
            }
        }
        Err(ConfigError::ProfileGrantNotFound(id.into()))
    }

    pub fn list_models(&self) -> Result<Vec<ModelConfig>, ConfigError> {
        self.ensure_default_files()?;
        let mut models = Vec::new();
        for entry in std::fs::read_dir(self.paths.models_dir())? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("toml") {
                models.push(read_model_config(&entry.path())?);
            }
        }
        models.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(models)
    }

    pub fn show_model(&self, id: &str) -> Result<Option<ModelConfig>, ConfigError> {
        self.ensure_default_files()?;
        validate_model_id(id)?;
        let path = self.paths.model_config(id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_model_config(&path)?))
    }

    pub fn save_model(&self, model: &ModelConfig) -> Result<ModelConfig, ConfigError> {
        self.paths.ensure_base_dirs()?;
        validate_model_id(&model.id)?;
        validate_model_config_with_providers(model, &self.model_provider_descriptors()?)?;
        let path = self.paths.model_config(&model.id);
        write_storage_text(&self.paths, path, toml::to_string_pretty(model)?)?;
        Ok(model.clone())
    }

    pub fn delete_model(&self, id: &str) -> Result<bool, ConfigError> {
        self.ensure_default_files()?;
        validate_model_id(id)?;
        let path = self.paths.model_config(id);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(path)?;
        Ok(true)
    }

    pub fn export_model_config(
        &self,
        id: &str,
        path: impl AsRef<Path>,
    ) -> Result<ModelConfig, ConfigError> {
        let Some(model) = self.show_model(id)? else {
            return Err(ConfigError::InvalidInput(format!("model not found: {id}")));
        };
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(&model)?)?;
        Ok(model)
    }

    pub fn import_model_config(&self, path: impl AsRef<Path>) -> Result<ModelConfig, ConfigError> {
        let model: ModelConfig = toml::from_str(&std::fs::read_to_string(path)?)?;
        self.save_model(&model)
    }

    pub fn resolve_model_runtime(
        &self,
        model_id: &str,
    ) -> Result<Option<ModelRuntimeConfig>, ConfigError> {
        self.ensure_default_files()?;
        let model_path = self.paths.model_config(model_id);
        if !model_path.exists() {
            return Ok(None);
        }
        let model: ModelConfig = toml::from_str(&std::fs::read_to_string(model_path)?)?;
        let provider_options = provider_options_from_metadata(&model.metadata)?;
        validate_provider_options(model.provider.as_deref(), provider_options.as_ref())?;
        Ok(Some(ModelRuntimeConfig {
            provider: model.provider,
            api_base_url: model.api_base_url,
            api_key_env: model.api_key_env,
            allow_missing_api_key: model.allow_missing_api_key,
            max_output_tokens: model.max_output_tokens,
            default_temperature: model.default_temperature,
            provider_options,
        }))
    }

    pub fn model_supports_modality(
        &self,
        model_id: &str,
        modality: &str,
    ) -> Result<bool, ConfigError> {
        Ok(self.model_modality_support(model_id, modality)?.supported)
    }

    pub fn model_modality_support(
        &self,
        model_id: &str,
        modality: &str,
    ) -> Result<ModelModalitySupport, ConfigError> {
        self.ensure_default_files()?;
        validate_model_id(model_id)?;
        let modality = modality.trim().to_ascii_lowercase();
        let model = self.show_model(model_id)?;
        let provider =
            normalized_provider(model.as_ref().and_then(|model| model.provider.as_deref()))
                .unwrap_or_else(|| "rig".into());
        if let Some(model) = model.as_ref()
            && !model.available_modalities.is_empty()
        {
            return Ok(model_modality_support_from_modalities(
                model_id,
                &provider,
                &modality,
                model.available_modalities.clone(),
                format!("saved_model:{model_id}"),
            ));
        }

        let metadata_catalog = self.load_model_metadata_catalog()?;
        if let Some((source, metadata)) =
            curated_model_metadata(&provider, model_id, metadata_catalog.as_ref())
            && !metadata.modalities.is_empty()
        {
            return Ok(model_modality_support_from_modalities(
                model_id,
                &provider,
                &modality,
                metadata.modalities,
                format!("model_metadata_catalog:{source}"),
            ));
        }

        Ok(provider_modality_support(
            model_id,
            &provider,
            &modality,
            &self.model_provider_descriptors()?,
        ))
    }

    pub fn model_supports_any_modality(
        &self,
        model_id: &str,
        modalities: &[String],
    ) -> Result<ModelModalitySupport, ConfigError> {
        let mut first_report = None;
        for modality in modalities {
            let report = self.model_modality_support(model_id, modality)?;
            if report.supported {
                return Ok(report);
            }
            if first_report.is_none() {
                first_report = Some(report);
            }
        }
        first_report.ok_or_else(|| {
            ConfigError::InvalidInput("at least one modality must be provided".into())
        })
    }

    pub fn probe_model_capabilities(
        &self,
        model_id: &str,
    ) -> Result<ModelCapabilityProbe, ConfigError> {
        self.ensure_default_files()?;
        validate_model_id(model_id)?;
        let model = self.show_model(model_id)?;
        let provider =
            normalized_provider(model.as_ref().and_then(|model| model.provider.as_deref()))
                .unwrap_or_else(|| "rig".into());
        let provider_descriptors = self.model_provider_descriptors()?;
        let provider_descriptor = provider_descriptors
            .iter()
            .find(|descriptor| descriptor.id == provider);
        let provider_modalities = provider_descriptor
            .as_ref()
            .map(|descriptor| descriptor.available_modalities.clone())
            .unwrap_or_else(|| vec!["text".into(), "image".into()]);
        let declared_modalities = model
            .as_ref()
            .map(|model| model.available_modalities.clone())
            .filter(|modalities| !modalities.is_empty())
            .unwrap_or_else(|| provider_modalities.clone());
        let declared_limits = model
            .as_ref()
            .map(declared_model_limits)
            .unwrap_or_default();
        let declared_pricing = model
            .as_ref()
            .map(declared_model_pricing)
            .unwrap_or_default();
        let api_base_url = model
            .as_ref()
            .and_then(|model| model.api_base_url.clone())
            .or_else(|| {
                provider_descriptor
                    .as_ref()
                    .and_then(|descriptor| descriptor.api_base_url.clone())
            });
        let api_key_env = model
            .as_ref()
            .and_then(|model| model.api_key_env.clone())
            .or_else(|| {
                provider_descriptor
                    .as_ref()
                    .and_then(|descriptor| descriptor.api_key_env.clone())
            });
        let live_probe = if provider_descriptor
            .as_ref()
            .is_some_and(|descriptor| descriptor.native)
        {
            probe_native_provider_capabilities(
                &provider,
                provider_descriptor,
                model_id,
                api_key_env.as_deref(),
                model.is_some(),
            )
        } else {
            probe_openai_compatible_models(
                model_id,
                &provider,
                api_base_url.as_deref(),
                api_key_env.as_deref(),
            )
        };
        let metadata_catalog = self.load_model_metadata_catalog()?;
        let live_probe = apply_curated_metadata_fallback(
            live_probe,
            &provider,
            model_id,
            metadata_catalog.as_ref(),
        );

        Ok(ModelCapabilityProbe {
            model_id: model_id.into(),
            saved_model: model.is_some(),
            provider,
            declared_modalities,
            provider_modalities,
            declared_limits,
            declared_pricing,
            tool_support: model
                .as_ref()
                .and_then(|model| model.tool_support)
                .or_else(|| {
                    provider_descriptor
                        .as_ref()
                        .and_then(|descriptor| descriptor.tool_support)
                }),
            live_probe,
        })
    }

    pub fn model_provider_descriptors(&self) -> Result<Vec<ModelProviderDescriptor>, ConfigError> {
        configured_model_providers_from_paths(&self.paths)
    }

    fn load_model_metadata_catalog(
        &self,
    ) -> Result<Option<LoadedModelMetadataCatalog>, ConfigError> {
        if let Some(path) = std::env::var_os(MODEL_METADATA_CATALOG_ENV)
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
        {
            return read_model_metadata_catalog(&path, format!("env:{MODEL_METADATA_CATALOG_ENV}"))
                .map(Some);
        }

        let profile_path = self.paths.models_dir().join(MODEL_METADATA_CATALOG_FILE);
        if profile_path.exists() {
            return read_model_metadata_catalog(
                &profile_path,
                format!("profile:{}", profile_path.display()),
            )
            .map(Some);
        }

        Ok(None)
    }
}

fn provider_options_from_metadata(
    metadata: &BTreeMap<String, serde_json::Value>,
) -> Result<Option<serde_json::Value>, ConfigError> {
    match metadata.get("provider_options") {
        Some(value @ serde_json::Value::Object(_)) => Ok(Some(value.clone())),
        Some(_) => Err(ConfigError::InvalidInput(
            "model metadata.provider_options must be a JSON object".into(),
        )),
        None => Ok(None),
    }
}

pub fn validate_model_config(model: &ModelConfig) -> Result<(), ConfigError> {
    validate_model_config_with_providers(model, &configured_model_providers()?)
}

fn validate_model_config_with_providers(
    model: &ModelConfig,
    providers: &[ModelProviderDescriptor],
) -> Result<(), ConfigError> {
    if let Some(api_base_url) = model.api_base_url.as_deref()
        && !api_base_url.trim().is_empty()
        && !provider_supports_api_base_url(model.provider.as_deref(), providers)
    {
        return Err(ConfigError::InvalidInput(format!(
            "provider {} does not support api_base_url",
            model.provider.as_deref().unwrap_or("rig")
        )));
    }
    let provider_options = provider_options_from_metadata(&model.metadata)?;
    validate_provider_options(model.provider.as_deref(), provider_options.as_ref())?;
    Ok(())
}

fn validate_provider_options(
    provider: Option<&str>,
    provider_options: Option<&serde_json::Value>,
) -> Result<(), ConfigError> {
    let Some(serde_json::Value::Object(options)) = provider_options else {
        return Ok(());
    };
    if let Some(value) = options.get("top_p") {
        let Some(top_p) = value.as_f64() else {
            return Err(ConfigError::InvalidInput(
                "provider_options.top_p must be a number".into(),
            ));
        };
        if !(0.0..=1.0).contains(&top_p) {
            return Err(ConfigError::InvalidInput(
                "provider_options.top_p must be between 0 and 1".into(),
            ));
        }
    }
    if let Some(value) = options.get("top_k") {
        let Some(top_k) = value.as_u64() else {
            return Err(ConfigError::InvalidInput(
                "provider_options.top_k must be a positive integer".into(),
            ));
        };
        if top_k == 0 {
            return Err(ConfigError::InvalidInput(
                "provider_options.top_k must be greater than 0".into(),
            ));
        }
    }
    if let Some(value) = options.get("reasoning_effort") {
        let Some(reasoning_effort) = value.as_str() else {
            return Err(ConfigError::InvalidInput(
                "provider_options.reasoning_effort must be a string".into(),
            ));
        };
        if reasoning_effort.trim().is_empty() {
            return Err(ConfigError::InvalidInput(
                "provider_options.reasoning_effort cannot be empty".into(),
            ));
        }
        if !provider_accepts_reasoning_effort(provider) {
            return Err(ConfigError::InvalidInput(format!(
                "provider {} does not support provider_options.reasoning_effort",
                provider.unwrap_or("rig")
            )));
        }
    }
    for key in ["frequency_penalty", "presence_penalty"] {
        if let Some(value) = options.get(key) {
            let Some(penalty) = value.as_f64() else {
                return Err(ConfigError::InvalidInput(format!(
                    "provider_options.{key} must be a number"
                )));
            };
            if !(-2.0..=2.0).contains(&penalty) {
                return Err(ConfigError::InvalidInput(format!(
                    "provider_options.{key} must be between -2 and 2"
                )));
            }
        }
    }
    Ok(())
}

fn provider_accepts_reasoning_effort(provider: Option<&str>) -> bool {
    !matches!(
        normalized_provider(provider).as_deref(),
        Some("anthropic" | "gemini" | "ollama" | "llama_cpp" | "fake")
    )
}

fn provider_supports_api_base_url(
    provider: Option<&str>,
    providers: &[ModelProviderDescriptor],
) -> bool {
    let normalized = normalized_provider(provider).unwrap_or_else(|| "rig".into());
    providers
        .iter()
        .find(|descriptor| descriptor.id == normalized)
        .map(|descriptor| descriptor.supports_api_base_url)
        .unwrap_or(true)
}

fn model_modality_support_from_modalities(
    model_id: &str,
    provider: &str,
    modality: &str,
    available_modalities: Vec<String>,
    source: String,
) -> ModelModalitySupport {
    let supported = available_modalities
        .iter()
        .any(|item| item.eq_ignore_ascii_case(modality));
    ModelModalitySupport {
        model_id: model_id.into(),
        provider: provider.into(),
        modality: modality.into(),
        supported,
        available_modalities,
        source,
    }
}

fn provider_modality_support(
    model_id: &str,
    provider: &str,
    modality: &str,
    providers: &[ModelProviderDescriptor],
) -> ModelModalitySupport {
    providers
        .iter()
        .find(|descriptor| descriptor.id == provider)
        .map(|descriptor| {
            model_modality_support_from_modalities(
                model_id,
                provider,
                modality,
                descriptor.available_modalities.clone(),
                format!("provider_descriptor:{provider}"),
            )
        })
        .unwrap_or_else(|| ModelModalitySupport {
            model_id: model_id.into(),
            provider: provider.into(),
            modality: modality.into(),
            supported: true,
            available_modalities: vec![modality.into()],
            source: format!("provider_descriptor:{provider}:unknown"),
        })
}

fn declared_model_limits(model: &ModelConfig) -> BTreeMap<String, u64> {
    let mut limits = BTreeMap::new();
    if let Some(value) = model.max_context_tokens {
        limits.insert("context_tokens".into(), value);
    }
    if let Some(value) = model.max_output_tokens {
        limits.insert("output_tokens".into(), value);
    }
    limits
}

fn declared_model_pricing(model: &ModelConfig) -> BTreeMap<String, String> {
    let mut pricing = BTreeMap::new();
    if let Some(value) = model.input_cost_per_million {
        pricing.insert("input_per_million".into(), value.to_string());
    }
    if let Some(value) = model.output_cost_per_million {
        pricing.insert("output_per_million".into(), value.to_string());
    }
    pricing
}

fn normalized_provider(provider: Option<&str>) -> Option<String> {
    let provider = provider?.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return None;
    }
    let provider = match provider.as_str() {
        "openai" | "openai-compatible" | "openai_compatible" => "rig",
        other => other,
    };
    Some(provider.to_string())
}

fn probe_native_provider_capabilities(
    provider: &str,
    provider_descriptor: Option<&ModelProviderDescriptor>,
    model_id: &str,
    api_key_env: Option<&str>,
    saved_model: bool,
) -> ModelLiveCapabilityProbe {
    match provider {
        "anthropic" => {
            return probe_anthropic_model_catalog(model_id, api_key_env, provider_descriptor);
        }
        "gemini" => {
            return probe_gemini_model_catalog(model_id, api_key_env, provider_descriptor);
        }
        _ => {}
    }
    let provider_name = provider_descriptor
        .map(|descriptor| descriptor.name.as_str())
        .unwrap_or(provider);
    ModelLiveCapabilityProbe {
        attempted: false,
        status: "declared".into(),
        source: Some(format!("provider_descriptor:{provider}")),
        fallback_source: None,
        model_found: saved_model.then_some(true),
        reported_modalities: Vec::new(),
        reported_capabilities: Vec::new(),
        reported_tool_support: None,
        reported_limits: BTreeMap::new(),
        reported_pricing: BTreeMap::new(),
        message: Some(format!(
            "{provider_name} does not expose a standard model-list endpoint here; capabilities are inferred from saved model metadata and provider descriptors"
        )),
    }
}

#[derive(Debug)]
struct NativeModelCatalog {
    model_ids: Vec<String>,
    capabilities: Vec<String>,
    model_metadata: BTreeMap<String, LiveModelMetadata>,
}

#[derive(Debug, Clone, Default)]
struct LiveModelMetadata {
    modalities: Vec<String>,
    capabilities: Vec<String>,
    tool_support: Option<bool>,
    limits: BTreeMap<String, u64>,
    pricing: BTreeMap<String, String>,
}

fn read_model_metadata_catalog(
    path: &Path,
    source: String,
) -> Result<LoadedModelMetadataCatalog, ConfigError> {
    let catalog: ModelMetadataCatalog = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    validate_model_metadata_catalog(&catalog)?;
    Ok(LoadedModelMetadataCatalog { source, catalog })
}

fn validate_model_metadata_catalog(catalog: &ModelMetadataCatalog) -> Result<(), ConfigError> {
    if catalog.schema_version != 1 {
        return Err(ConfigError::InvalidInput(format!(
            "unsupported model metadata catalog schema_version {}; expected 1",
            catalog.schema_version
        )));
    }
    for model in &catalog.models {
        if model.provider.trim().is_empty() {
            return Err(ConfigError::InvalidInput(
                "model metadata catalog entries require provider".into(),
            ));
        }
        if model.model_id.trim().is_empty() {
            return Err(ConfigError::InvalidInput(
                "model metadata catalog entries require model_id".into(),
            ));
        }
    }
    Ok(())
}

fn bundled_model_metadata_catalog() -> Option<LoadedModelMetadataCatalog> {
    let catalog: ModelMetadataCatalog =
        serde_json::from_str(BUNDLED_MODEL_METADATA_CATALOG_JSON).ok()?;
    validate_model_metadata_catalog(&catalog).ok()?;
    Some(LoadedModelMetadataCatalog {
        source: "bundled:agent-config/model_metadata_catalog.json".into(),
        catalog,
    })
}

fn catalog_model_metadata(
    loaded: &LoadedModelMetadataCatalog,
    provider: &str,
    model_id: &str,
) -> Option<(String, LiveModelMetadata)> {
    let model_id = model_id.trim();
    loaded
        .catalog
        .models
        .iter()
        .find(|entry| {
            normalized_provider(Some(&entry.provider)).as_deref() == Some(provider)
                && entry.model_id.trim() == model_id
        })
        .map(|entry| {
            let source = entry
                .source
                .clone()
                .or_else(|| loaded.catalog.source.clone())
                .unwrap_or_else(|| loaded.source.clone());
            (
                source,
                LiveModelMetadata {
                    modalities: entry.modalities.clone(),
                    capabilities: entry.capabilities.clone(),
                    tool_support: entry.tool_support,
                    limits: entry.limits.clone(),
                    pricing: entry.pricing.clone(),
                },
            )
        })
}

fn curated_model_metadata(
    provider: &str,
    model_id: &str,
    configured_catalog: Option<&LoadedModelMetadataCatalog>,
) -> Option<(String, LiveModelMetadata)> {
    configured_catalog
        .and_then(|catalog| catalog_model_metadata(catalog, provider, model_id))
        .or_else(|| {
            bundled_model_metadata_catalog()
                .as_ref()
                .and_then(|catalog| catalog_model_metadata(catalog, provider, model_id))
        })
}

fn append_probe_message(message: Option<String>, note: &str) -> Option<String> {
    Some(match message {
        Some(existing) if existing.contains(note) => existing,
        Some(existing) => format!("{existing}; {note}"),
        None => note.to_string(),
    })
}

fn apply_curated_metadata_fallback(
    mut probe: ModelLiveCapabilityProbe,
    provider: &str,
    model_id: &str,
    configured_catalog: Option<&LoadedModelMetadataCatalog>,
) -> ModelLiveCapabilityProbe {
    if probe.model_found == Some(false) {
        return probe;
    }
    let Some((source, fallback)) = curated_model_metadata(provider, model_id, configured_catalog)
    else {
        return probe;
    };
    let mut applied = false;
    if probe.reported_modalities.is_empty() && !fallback.modalities.is_empty() {
        probe.reported_modalities = fallback.modalities.clone();
        applied = true;
    }
    if probe.reported_capabilities.is_empty() && !fallback.capabilities.is_empty() {
        probe.reported_capabilities = fallback.capabilities.clone();
        applied = true;
    }
    if probe.reported_tool_support.is_none() && fallback.tool_support.is_some() {
        probe.reported_tool_support = fallback.tool_support;
        applied = true;
    }
    for (key, value) in fallback.limits {
        if let std::collections::btree_map::Entry::Vacant(entry) = probe.reported_limits.entry(key)
        {
            entry.insert(value);
            applied = true;
        }
    }
    for (key, value) in fallback.pricing {
        if let std::collections::btree_map::Entry::Vacant(entry) = probe.reported_pricing.entry(key)
        {
            entry.insert(value);
            applied = true;
        }
    }
    if applied {
        probe.fallback_source = Some(source);
        probe.message = append_probe_message(
            probe.message,
            "missing catalog metadata was filled from curated offline metadata",
        );
    }
    probe
}

fn probe_anthropic_model_catalog(
    model_id: &str,
    api_key_env: Option<&str>,
    provider_descriptor: Option<&ModelProviderDescriptor>,
) -> ModelLiveCapabilityProbe {
    let source = "https://api.anthropic.com/v1/models";
    let Some(api_key) = native_catalog_api_key(api_key_env) else {
        return native_catalog_not_configured("Anthropic", source, api_key_env);
    };
    match fetch_anthropic_model_catalog(source, &api_key) {
        Ok(catalog) => native_catalog_reachable(model_id, source, catalog, provider_descriptor),
        Err(message) => native_catalog_unreachable(source, message),
    }
}

fn probe_gemini_model_catalog(
    model_id: &str,
    api_key_env: Option<&str>,
    provider_descriptor: Option<&ModelProviderDescriptor>,
) -> ModelLiveCapabilityProbe {
    let source = "https://generativelanguage.googleapis.com/v1beta/models?pageSize=1000";
    let Some(api_key) = native_catalog_api_key(api_key_env) else {
        return native_catalog_not_configured("Gemini", source, api_key_env);
    };
    match fetch_gemini_model_catalog(source, &api_key) {
        Ok(catalog) => native_catalog_reachable(model_id, source, catalog, provider_descriptor),
        Err(message) => native_catalog_unreachable(source, message),
    }
}

fn native_catalog_api_key(api_key_env: Option<&str>) -> Option<String> {
    api_key_env
        .map(str::trim)
        .filter(|env| !env.is_empty())
        .and_then(|env| std::env::var(env).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn native_catalog_not_configured(
    provider_name: &str,
    source: &str,
    api_key_env: Option<&str>,
) -> ModelLiveCapabilityProbe {
    let env_name = api_key_env
        .map(str::trim)
        .filter(|env| !env.is_empty())
        .unwrap_or("provider API key env var");
    ModelLiveCapabilityProbe {
        attempted: false,
        status: "not_configured".into(),
        source: Some(source.into()),
        fallback_source: None,
        model_found: None,
        reported_modalities: Vec::new(),
        reported_capabilities: Vec::new(),
        reported_tool_support: None,
        reported_limits: BTreeMap::new(),
        reported_pricing: BTreeMap::new(),
        message: Some(format!(
            "{provider_name} model catalog probing needs {env_name} to be set"
        )),
    }
}

fn native_catalog_reachable(
    model_id: &str,
    source: &str,
    catalog: NativeModelCatalog,
    provider_descriptor: Option<&ModelProviderDescriptor>,
) -> ModelLiveCapabilityProbe {
    let model_found = catalog.model_ids.iter().any(|id| id == model_id);
    let metadata = catalog.model_metadata.get(model_id);
    ModelLiveCapabilityProbe {
        attempted: true,
        status: "reachable".into(),
        source: Some(source.into()),
        fallback_source: None,
        model_found: Some(model_found),
        reported_modalities: if model_found {
            metadata
                .map(|metadata| metadata.modalities.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        },
        reported_capabilities: if model_found {
            metadata
                .map(|metadata| metadata.capabilities.clone())
                .filter(|capabilities| !capabilities.is_empty())
                .unwrap_or_else(|| catalog.capabilities.clone())
        } else {
            Vec::new()
        },
        reported_tool_support: if model_found {
            metadata
                .and_then(|metadata| metadata.tool_support)
                .or_else(|| provider_descriptor.and_then(|descriptor| descriptor.tool_support))
        } else {
            None
        },
        reported_limits: if model_found {
            metadata
                .map(|metadata| metadata.limits.clone())
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        },
        reported_pricing: if model_found {
            metadata
                .map(|metadata| metadata.pricing.clone())
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        },
        message: Some(if model_found {
            "model id was listed by the provider catalog".into()
        } else {
            "provider catalog was reachable but did not list this model id".into()
        }),
    }
}

fn native_catalog_unreachable(source: &str, message: String) -> ModelLiveCapabilityProbe {
    ModelLiveCapabilityProbe {
        attempted: true,
        status: "unreachable".into(),
        source: Some(source.into()),
        fallback_source: None,
        model_found: None,
        reported_modalities: Vec::new(),
        reported_capabilities: Vec::new(),
        reported_tool_support: None,
        reported_limits: BTreeMap::new(),
        reported_pricing: BTreeMap::new(),
        message: Some(message),
    }
}

fn native_catalog_agent() -> ureq::Agent {
    use std::time::Duration;

    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(800)))
        .https_only(true)
        .build()
        .into()
}

fn fetch_anthropic_model_catalog(
    source: &str,
    api_key: &str,
) -> Result<NativeModelCatalog, String> {
    let mut response = native_catalog_agent()
        .get(source)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .call()
        .map_err(|err| err.to_string())?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|err| err.to_string())?;
    parse_anthropic_model_catalog(&body)
}

fn fetch_gemini_model_catalog(source: &str, api_key: &str) -> Result<NativeModelCatalog, String> {
    let mut response = native_catalog_agent()
        .get(source)
        .header("x-goog-api-key", api_key)
        .call()
        .map_err(|err| err.to_string())?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|err| err.to_string())?;
    parse_gemini_model_catalog(&body)
}

fn push_unique(values: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !value.trim().is_empty() && !values.iter().any(|item| item == &value) {
        values.push(value);
    }
}

fn string_array_field(item: &serde_json::Value, keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for key in keys {
        for value in item
            .get(*key)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            push_unique(&mut values, value.to_string());
        }
    }
    values
}

fn bool_field(item: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(serde_json::Value::as_bool))
}

fn catalog_field<'a>(item: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    item.get(key).or_else(|| {
        item.get("metadata")
            .and_then(serde_json::Value::as_object)
            .and_then(|metadata| metadata.get(key))
    })
}

fn catalog_object_field<'a>(
    item: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    catalog_field(item, key).and_then(serde_json::Value::as_object)
}

fn u64_catalog_value(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_i64()
                .filter(|value| *value >= 0)
                .map(|value| value as u64)
        })
        .or_else(|| {
            value
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(|value| value as u64)
        })
        .or_else(|| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|value| {
                    value.parse::<u64>().ok().or_else(|| {
                        value
                            .parse::<f64>()
                            .ok()
                            .filter(|value| value.is_finite() && *value >= 0.0)
                            .map(|value| value as u64)
                    })
                })
        })
}

fn string_catalog_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| value.as_f64().map(|value| value.to_string()))
        .or_else(|| value.as_u64().map(|value| value.to_string()))
        .or_else(|| value.as_i64().map(|value| value.to_string()))
}

fn insert_first_u64_catalog_field(
    target: &mut BTreeMap<String, u64>,
    output_key: &str,
    item: &serde_json::Value,
    input_keys: &[&str],
) {
    if target.contains_key(output_key) {
        return;
    }
    if let Some(value) = input_keys
        .iter()
        .find_map(|key| catalog_field(item, key).and_then(u64_catalog_value))
    {
        target.insert(output_key.to_string(), value);
    }
}

fn insert_first_string_catalog_field(
    target: &mut BTreeMap<String, String>,
    output_key: &str,
    item: &serde_json::Value,
    input_keys: &[&str],
) {
    if target.contains_key(output_key) {
        return;
    }
    if let Some(value) = input_keys
        .iter()
        .find_map(|key| catalog_field(item, key).and_then(string_catalog_value))
    {
        target.insert(output_key.to_string(), value);
    }
}

fn live_model_limits(item: &serde_json::Value) -> BTreeMap<String, u64> {
    let mut limits = BTreeMap::new();
    insert_first_u64_catalog_field(
        &mut limits,
        "context_tokens",
        item,
        &[
            "context_length",
            "context_window",
            "context_window_tokens",
            "max_context_length",
            "max_context_tokens",
        ],
    );
    insert_first_u64_catalog_field(
        &mut limits,
        "input_tokens",
        item,
        &[
            "input_token_limit",
            "inputTokenLimit",
            "max_input_tokens",
            "max_prompt_tokens",
        ],
    );
    insert_first_u64_catalog_field(
        &mut limits,
        "output_tokens",
        item,
        &[
            "output_token_limit",
            "outputTokenLimit",
            "max_completion_tokens",
            "max_output_tokens",
            "max_tokens",
        ],
    );
    limits
}

fn live_model_pricing(item: &serde_json::Value) -> BTreeMap<String, String> {
    let mut pricing = BTreeMap::new();
    insert_first_string_catalog_field(
        &mut pricing,
        "input_per_million",
        item,
        &[
            "input_cost_per_million",
            "input_price_per_million",
            "prompt_cost_per_million",
            "prompt_price_per_million",
        ],
    );
    insert_first_string_catalog_field(
        &mut pricing,
        "output_per_million",
        item,
        &[
            "output_cost_per_million",
            "output_price_per_million",
            "completion_cost_per_million",
            "completion_price_per_million",
        ],
    );
    if let Some(raw_pricing) = catalog_object_field(item, "pricing") {
        for key in [
            "prompt",
            "input",
            "completion",
            "output",
            "request",
            "image",
        ] {
            if let Some(value) = raw_pricing.get(key).and_then(string_catalog_value) {
                pricing.insert(format!("pricing.{key}"), value);
            }
        }
    }
    pricing
}

fn tool_support_from_capabilities(capabilities: &[String]) -> Option<bool> {
    capabilities
        .iter()
        .any(|capability| {
            let value = capability.to_ascii_lowercase();
            value.contains("tool") || value.contains("function")
        })
        .then_some(true)
}

fn openai_model_metadata(item: &serde_json::Value) -> Option<(String, LiveModelMetadata)> {
    let id = item
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())?
        .to_string();
    let mut capabilities = string_array_field(
        item,
        &[
            "capabilities",
            "supported_capabilities",
            "features",
            "supported_features",
        ],
    );
    if let Some(serde_json::Value::Object(metadata)) = item.get("metadata") {
        for key in ["capabilities", "supported_capabilities", "features"] {
            if let Some(value) = metadata.get(key) {
                for capability in value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    push_unique(&mut capabilities, capability.to_string());
                }
            }
        }
    }
    let mut modalities = string_array_field(
        item,
        &[
            "modalities",
            "input_modalities",
            "output_modalities",
            "supported_modalities",
        ],
    );
    if let Some(serde_json::Value::Object(metadata)) = item.get("metadata") {
        for key in [
            "modalities",
            "input_modalities",
            "output_modalities",
            "supported_modalities",
        ] {
            if let Some(value) = metadata.get(key) {
                for modality in value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    push_unique(&mut modalities, modality.to_string());
                }
            }
        }
    }
    let tool_support = bool_field(
        item,
        &[
            "tool_support",
            "supports_tools",
            "supports_tool_calls",
            "supports_function_calling",
            "function_calling",
        ],
    )
    .or_else(|| tool_support_from_capabilities(&capabilities));
    Some((
        id,
        LiveModelMetadata {
            modalities,
            capabilities,
            tool_support,
            limits: live_model_limits(item),
            pricing: live_model_pricing(item),
        },
    ))
}

fn parse_anthropic_model_catalog(body: &str) -> Result<NativeModelCatalog, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|err| format!("invalid Anthropic model JSON: {err}"))?;
    let mut model_ids = Vec::new();
    let mut model_metadata = BTreeMap::new();
    for item in value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = item
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        let capabilities = string_array_field(
            item,
            &[
                "capabilities",
                "supported_capabilities",
                "features",
                "supported_features",
            ],
        );
        let metadata = LiveModelMetadata {
            modalities: string_array_field(
                item,
                &[
                    "modalities",
                    "input_modalities",
                    "output_modalities",
                    "supported_modalities",
                ],
            ),
            tool_support: bool_field(
                item,
                &[
                    "tool_support",
                    "supports_tools",
                    "supports_tool_calls",
                    "supports_function_calling",
                ],
            )
            .or_else(|| tool_support_from_capabilities(&capabilities)),
            capabilities,
            limits: live_model_limits(item),
            pricing: live_model_pricing(item),
        };
        model_ids.push(id.clone());
        model_metadata.insert(id, metadata);
    }
    Ok(NativeModelCatalog {
        model_ids,
        capabilities: Vec::new(),
        model_metadata,
    })
}

fn parse_gemini_model_catalog(body: &str) -> Result<NativeModelCatalog, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|err| format!("invalid Gemini model JSON: {err}"))?;
    let mut model_ids = Vec::new();
    let mut capabilities = Vec::new();
    let mut model_metadata = BTreeMap::new();
    for item in value
        .get("models")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = item
            .get("baseModelId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .or_else(|| {
                item.get("name")
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .and_then(|name| name.strip_prefix("models/").or(Some(name)))
                    .map(str::to_string)
                    .filter(|id| !id.is_empty())
            })
        {
            model_ids.push(id);
            let id = model_ids.last().cloned().unwrap_or_default();
            let mut item_capabilities = Vec::new();
            for method in item
                .get("supportedGenerationMethods")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|method| !method.is_empty())
            {
                push_unique(&mut item_capabilities, method.to_string());
            }
            let mut modalities = string_array_field(
                item,
                &[
                    "modalities",
                    "input_modalities",
                    "output_modalities",
                    "supported_modalities",
                ],
            );
            if item_capabilities
                .iter()
                .any(|method| method.eq_ignore_ascii_case("embedContent"))
            {
                push_unique(&mut modalities, "embedding");
            }
            if item_capabilities
                .iter()
                .any(|method| method.eq_ignore_ascii_case("generateContent"))
            {
                push_unique(&mut modalities, "text");
            }
            let tool_support = bool_field(
                item,
                &[
                    "tool_support",
                    "supports_tools",
                    "supports_tool_calls",
                    "supports_function_calling",
                ],
            )
            .or_else(|| tool_support_from_capabilities(&item_capabilities));
            model_metadata.insert(
                id,
                LiveModelMetadata {
                    modalities,
                    capabilities: item_capabilities,
                    tool_support,
                    limits: live_model_limits(item),
                    pricing: live_model_pricing(item),
                },
            );
        }
        for method in item
            .get("supportedGenerationMethods")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|method| !method.is_empty())
        {
            if !capabilities.iter().any(|item| item == method) {
                capabilities.push(method.to_string());
            }
        }
    }
    Ok(NativeModelCatalog {
        model_ids,
        capabilities,
        model_metadata,
    })
}

fn probe_openai_compatible_models(
    model_id: &str,
    provider: &str,
    api_base_url: Option<&str>,
    api_key_env: Option<&str>,
) -> ModelLiveCapabilityProbe {
    if matches!(provider, "fake" | "anthropic" | "gemini") {
        return ModelLiveCapabilityProbe {
            attempted: false,
            status: "unsupported".into(),
            source: None,
            fallback_source: None,
            model_found: None,
            reported_modalities: Vec::new(),
            reported_capabilities: Vec::new(),
            reported_tool_support: None,
            reported_limits: BTreeMap::new(),
            reported_pricing: BTreeMap::new(),
            message: Some(
                "provider does not expose a standard OpenAI-compatible model list".into(),
            ),
        };
    }
    let Some(api_base_url) = api_base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return ModelLiveCapabilityProbe {
            attempted: false,
            status: "not_configured".into(),
            source: None,
            fallback_source: None,
            model_found: None,
            reported_modalities: Vec::new(),
            reported_capabilities: Vec::new(),
            reported_tool_support: None,
            reported_limits: BTreeMap::new(),
            reported_pricing: BTreeMap::new(),
            message: Some("no HTTP API base URL is configured for live probing".into()),
        };
    };
    let parsed = match parse_http_probe_url(api_base_url) {
        Ok(parsed) => parsed,
        Err(message) => {
            return ModelLiveCapabilityProbe {
                attempted: false,
                status: "unsupported".into(),
                source: Some(api_base_url.into()),
                fallback_source: None,
                model_found: None,
                reported_modalities: Vec::new(),
                reported_capabilities: Vec::new(),
                reported_tool_support: None,
                reported_limits: BTreeMap::new(),
                reported_pricing: BTreeMap::new(),
                message: Some(message),
            };
        }
    };
    match fetch_openai_model_catalog(&parsed, api_key_env) {
        Ok(catalog) => {
            let model_found = catalog.model_ids.iter().any(|id| id == model_id);
            let metadata = catalog.model_metadata.get(model_id);
            ModelLiveCapabilityProbe {
                attempted: true,
                status: "reachable".into(),
                source: Some(parsed.source),
                fallback_source: None,
                model_found: Some(model_found),
                reported_modalities: if model_found {
                    metadata
                        .map(|metadata| metadata.modalities.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                },
                reported_capabilities: if model_found {
                    metadata
                        .map(|metadata| metadata.capabilities.clone())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                },
                reported_tool_support: if model_found {
                    metadata.and_then(|metadata| metadata.tool_support)
                } else {
                    None
                },
                reported_limits: if model_found {
                    metadata
                        .map(|metadata| metadata.limits.clone())
                        .unwrap_or_default()
                } else {
                    BTreeMap::new()
                },
                reported_pricing: if model_found {
                    metadata
                        .map(|metadata| metadata.pricing.clone())
                        .unwrap_or_default()
                } else {
                    BTreeMap::new()
                },
                message: Some(if model_found {
                    "model id was listed by the provider".into()
                } else {
                    "provider was reachable but did not list this model id".into()
                }),
            }
        }
        Err(message) => ModelLiveCapabilityProbe {
            attempted: true,
            status: "unreachable".into(),
            source: Some(parsed.source),
            fallback_source: None,
            model_found: None,
            reported_modalities: Vec::new(),
            reported_capabilities: Vec::new(),
            reported_tool_support: None,
            reported_limits: BTreeMap::new(),
            reported_pricing: BTreeMap::new(),
            message: Some(message),
        },
    }
}

#[derive(Debug)]
struct HttpProbeUrl {
    host: String,
    port: u16,
    path: String,
    source: String,
}

fn parse_http_probe_url(base_url: &str) -> Result<HttpProbeUrl, String> {
    if base_url.starts_with("https://") {
        return Err("HTTPS probing is not available in the local lightweight probe".into());
    }
    let Some(rest) = base_url.strip_prefix("http://") else {
        return Err("live probe only supports http:// OpenAI-compatible endpoints".into());
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    if authority.trim().is_empty() {
        return Err("HTTP API base URL is missing a host".into());
    }
    let (host, port) = if let Some((host, port)) = authority.rsplit_once(':') {
        let port = port
            .parse::<u16>()
            .map_err(|_| "HTTP API base URL has an invalid port".to_string())?;
        (host.to_string(), port)
    } else {
        (authority.to_string(), 80)
    };
    if host.trim().is_empty() {
        return Err("HTTP API base URL is missing a host".into());
    }
    let base_path = format!("/{}", path.trim_matches('/'));
    let path = if base_path == "/" {
        "/models".into()
    } else {
        format!("{}/models", base_path.trim_end_matches('/'))
    };
    Ok(HttpProbeUrl {
        source: format!("http://{}:{}{}", host, port, path),
        host,
        port,
        path,
    })
}

fn fetch_openai_model_catalog(
    url: &HttpProbeUrl,
    api_key_env: Option<&str>,
) -> Result<NativeModelCatalog, String> {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::Duration;

    let mut addrs = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|err| err.to_string())?;
    let addr = addrs
        .next()
        .ok_or_else(|| "HTTP API base URL did not resolve".to_string())?;
    let timeout = Duration::from_millis(800);
    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|err| err.to_string())?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| err.to_string())?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|err| err.to_string())?;
    let mut request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n",
        url.path, url.host
    );
    if let Some(api_key) = api_key_env
        .and_then(|env| std::env::var(env).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        request.push_str(&format!("Authorization: Bearer {api_key}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|err| err.to_string())?;
    let mut response = Vec::new();
    stream
        .take(512 * 1024)
        .read_to_end(&mut response)
        .map_err(|err| err.to_string())?;
    let response = String::from_utf8_lossy(&response);
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP response did not include a body".to_string())?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| "HTTP response did not include a valid status".to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("provider returned HTTP status {status}"));
    }
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|err| format!("invalid model list JSON: {err}"))?;
    let mut model_ids = Vec::new();
    let mut model_metadata = BTreeMap::new();
    for item in value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some((id, metadata)) = openai_model_metadata(item) else {
            continue;
        };
        model_ids.push(id.clone());
        model_metadata.insert(id, metadata);
    }
    Ok(NativeModelCatalog {
        model_ids,
        capabilities: Vec::new(),
        model_metadata,
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_agent(
    parsed: AgentToml,
    path: PathBuf,
    global: GlobalToml,
    global_path: PathBuf,
    profile: ProfileToml,
    profile_path: PathBuf,
    model: ResolvedValue<String>,
    model_config: Option<ModelConfig>,
    model_path: PathBuf,
) -> ResolvedAgentConfig {
    let global_source = format!("global:{}", global_path.display());
    let profile_source = format!("profile:{}", profile_path.display());
    let source = format!("agent:{}", path.display());
    let model_source = format!("model:{}", model_path.display());
    let prompt_refinements = prompt_refinement_rules(&parsed);
    let max_tool_calls = resolve_layered(
        ToolPolicy::default().max_calls,
        "default:agent-core ToolPolicy::max_calls".into(),
        vec![
            (global.policy.max_tool_calls, global_source.clone()),
            (profile.policy.max_tool_calls, profile_source.clone()),
            (parsed.policy.max_tool_calls, source.clone()),
        ],
    );
    let max_subagent_depth = resolve_layered(
        ExecutionPolicy::default().max_subagent_depth,
        "default:agent-core ExecutionPolicy::max_subagent_depth".into(),
        vec![
            (global.policy.max_subagent_depth, global_source.clone()),
            (profile.policy.max_subagent_depth, profile_source.clone()),
            (parsed.policy.max_subagent_depth, source.clone()),
        ],
    );
    let max_recursion_depth = resolve_layered(
        ExecutionPolicy::default().max_recursion_depth,
        "default:agent-core ExecutionPolicy::max_recursion_depth".into(),
        vec![
            (global.policy.max_recursion_depth, global_source.clone()),
            (profile.policy.max_recursion_depth, profile_source.clone()),
            (parsed.policy.max_recursion_depth, source.clone()),
        ],
    );
    let allowed_tool_ids = resolve_layered(
        Vec::<String>::new(),
        "default:all registered tools".into(),
        vec![
            (global.policy.allowed_tools, global_source.clone()),
            (profile.policy.allowed_tools, profile_source.clone()),
            (parsed.policy.allowed_tools, source.clone()),
        ],
    );
    let allowed_tools: Vec<ToolId> = allowed_tool_ids
        .value
        .iter()
        .map(|id| ToolId::from(id.clone()))
        .collect();
    let allowed_tool_categories = resolve_layered(
        Vec::<String>::new(),
        "default:all tool categories".into(),
        vec![
            (global.policy.allowed_tool_categories, global_source.clone()),
            (
                profile.policy.allowed_tool_categories,
                profile_source.clone(),
            ),
            (parsed.policy.allowed_tool_categories, source.clone()),
        ],
    );
    let approval_controller_agent = resolve_layered(
        None::<String>,
        "default:no delegated approval controller".into(),
        vec![
            (
                global.policy.approval_controller_agent.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.approval_controller_agent.map(Some),
                profile_source.clone(),
            ),
            (
                parsed.policy.approval_controller_agent.map(Some),
                source.clone(),
            ),
        ],
    );
    let approval_controller_allowed_tools = resolve_layered(
        Vec::<String>::new(),
        "default:no delegated approval tool scope".into(),
        vec![
            (
                global.policy.approval_controller_allowed_tools,
                global_source.clone(),
            ),
            (
                profile.policy.approval_controller_allowed_tools,
                profile_source.clone(),
            ),
            (
                parsed.policy.approval_controller_allowed_tools,
                source.clone(),
            ),
        ],
    );
    let approval_controller_allowed_tool_categories = resolve_layered(
        Vec::<String>::new(),
        "default:no delegated approval category scope".into(),
        vec![
            (
                global.policy.approval_controller_allowed_tool_categories,
                global_source.clone(),
            ),
            (
                profile.policy.approval_controller_allowed_tool_categories,
                profile_source.clone(),
            ),
            (
                parsed.policy.approval_controller_allowed_tool_categories,
                source.clone(),
            ),
        ],
    );
    let approval_controller = approval_controller_agent
        .value
        .clone()
        .and_then(|agent_id| {
            ApprovalControllerPolicy::new(
                agent_id,
                approval_controller_allowed_tools
                    .value
                    .iter()
                    .cloned()
                    .map(ToolId::from)
                    .collect(),
                approval_controller_allowed_tool_categories.value.clone(),
            )
        });
    let allowed_skill_categories = resolve_layered(
        Vec::<String>::new(),
        "default:all skill categories".into(),
        vec![
            (
                global.policy.allowed_skill_categories,
                global_source.clone(),
            ),
            (
                profile.policy.allowed_skill_categories,
                profile_source.clone(),
            ),
            (parsed.policy.allowed_skill_categories, source.clone()),
        ],
    );
    let disabled_lifecycle_hooks = resolve_layered(
        Vec::<String>::new(),
        "default:all lifecycle hooks enabled".into(),
        vec![
            (
                global.policy.disabled_lifecycle_hooks,
                global_source.clone(),
            ),
            (
                profile.policy.disabled_lifecycle_hooks,
                profile_source.clone(),
            ),
            (parsed.policy.disabled_lifecycle_hooks, source.clone()),
        ],
    );
    let tool_output_mode = resolve_layered(
        default_tool_output_mode(),
        "default:agent-core ToolPolicy::output_mode".into(),
        vec![
            (global.policy.tool_output_mode, global_source.clone()),
            (profile.policy.tool_output_mode, profile_source.clone()),
            (parsed.policy.tool_output_mode, source.clone()),
        ],
    );
    let tool_output_interpretation_model = resolve_layered(
        None::<String>,
        "default:agent model".into(),
        vec![
            (
                global.policy.tool_output_interpretation_model.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.tool_output_interpretation_model.map(Some),
                profile_source.clone(),
            ),
            (
                parsed.policy.tool_output_interpretation_model.map(Some),
                source.clone(),
            ),
        ],
    );
    let tool_visibility = resolve_layered(
        default_tool_visibility(),
        "default:agent-core ToolPolicy::visibility".into(),
        vec![
            (global.policy.tool_visibility, global_source.clone()),
            (profile.policy.tool_visibility, profile_source.clone()),
            (parsed.policy.tool_visibility, source.clone()),
        ],
    );
    let load_memory = resolve_layered(
        false,
        "default:memory loading off".into(),
        vec![
            (global.policy.load_memory, global_source.clone()),
            (profile.policy.load_memory, profile_source.clone()),
            (parsed.policy.load_memory, source.clone()),
        ],
    );
    let load_skills = resolve_layered(
        false,
        "default:skill loading off".into(),
        vec![
            (global.policy.load_skills, global_source.clone()),
            (profile.policy.load_skills, profile_source.clone()),
            (parsed.policy.load_skills, source.clone()),
        ],
    );
    let max_tokens_before_compaction = resolve_layered(
        None::<u32>,
        "default:automatic context compaction off".into(),
        vec![
            (
                global.policy.max_tokens_before_compaction.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.max_tokens_before_compaction.map(Some),
                profile_source.clone(),
            ),
            (
                parsed.policy.max_tokens_before_compaction.map(Some),
                source.clone(),
            ),
        ],
    );
    let max_compaction_output_tokens = resolve_layered(
        None::<u32>,
        "default:auto compaction output uses runtime default".into(),
        vec![
            (
                global.policy.max_compaction_output_tokens.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.max_compaction_output_tokens.map(Some),
                profile_source.clone(),
            ),
            (
                parsed.policy.max_compaction_output_tokens.map(Some),
                source.clone(),
            ),
        ],
    );
    let compaction_guidance = resolve_layered(
        None::<String>,
        "default:no auto compaction guidance".into(),
        vec![
            (
                global.policy.compaction_guidance.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.compaction_guidance.map(Some),
                profile_source.clone(),
            ),
            (parsed.policy.compaction_guidance.map(Some), source.clone()),
        ],
    );
    let ingestion_guardrail = resolve_layered(
        IngestionGuardrailMode::Block,
        "default:ingestion guardrail blocks high-risk content".into(),
        vec![
            (global.policy.ingestion_guardrail, global_source.clone()),
            (profile.policy.ingestion_guardrail, profile_source.clone()),
            (parsed.policy.ingestion_guardrail, source.clone()),
        ],
    );
    let ingestion_guardrail_model = resolve_layered(
        None::<String>,
        "default:ingestion model guardrail disabled".into(),
        vec![
            (
                global.policy.ingestion_guardrail_model.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.ingestion_guardrail_model.map(Some),
                profile_source.clone(),
            ),
            (
                parsed.policy.ingestion_guardrail_model.map(Some),
                source.clone(),
            ),
        ],
    );
    let input_cost_per_million = resolve_layered(
        model_config
            .as_ref()
            .and_then(|model| model.input_cost_per_million),
        model_source.clone(),
        vec![
            (
                global.policy.input_cost_per_million.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.input_cost_per_million.map(Some),
                profile_source.clone(),
            ),
            (
                parsed.policy.input_cost_per_million.map(Some),
                source.clone(),
            ),
        ],
    );
    let output_cost_per_million = resolve_layered(
        model_config
            .as_ref()
            .and_then(|model| model.output_cost_per_million),
        model_source.clone(),
        vec![
            (
                global.policy.output_cost_per_million.map(Some),
                global_source.clone(),
            ),
            (
                profile.policy.output_cost_per_million.map(Some),
                profile_source.clone(),
            ),
            (
                parsed.policy.output_cost_per_million.map(Some),
                source.clone(),
            ),
        ],
    );
    let voice_input_enabled = resolve_layered(
        VoiceConfig::default().input_enabled,
        "default:voice.input_enabled".into(),
        vec![
            (
                global
                    .policy
                    .voice
                    .as_ref()
                    .and_then(|voice| voice.input_enabled),
                global_source.clone(),
            ),
            (
                profile
                    .policy
                    .voice
                    .as_ref()
                    .and_then(|voice| voice.input_enabled),
                profile_source.clone(),
            ),
            (
                parsed
                    .policy
                    .voice
                    .as_ref()
                    .and_then(|voice| voice.input_enabled),
                source.clone(),
            ),
        ],
    );
    let voice_output_enabled = resolve_layered(
        VoiceConfig::default().output_enabled,
        "default:voice.output_enabled".into(),
        vec![
            (
                global
                    .policy
                    .voice
                    .as_ref()
                    .and_then(|voice| voice.output_enabled),
                global_source.clone(),
            ),
            (
                profile
                    .policy
                    .voice
                    .as_ref()
                    .and_then(|voice| voice.output_enabled),
                profile_source.clone(),
            ),
            (
                parsed
                    .policy
                    .voice
                    .as_ref()
                    .and_then(|voice| voice.output_enabled),
                source.clone(),
            ),
        ],
    );
    let voice_input_backend = resolve_layered(
        VoiceConfig::default().input_backend,
        "default:voice.input_backend".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.input_backend),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.input_backend),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.input_backend),
                source.clone(),
            ),
        ],
    );
    let voice_input_provider = resolve_layered(
        VoiceConfig::default().input_provider,
        "default:voice.input_provider".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.input_provider),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.input_provider),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.input_provider),
                source.clone(),
            ),
        ],
    );
    let voice_input_model = resolve_layered(
        VoiceConfig::default().input_model,
        "default:voice.input_model".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.input_model),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.input_model),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.input_model),
                source.clone(),
            ),
        ],
    );
    let voice_output_backend = resolve_layered(
        VoiceConfig::default().output_backend,
        "default:voice.output_backend".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.output_backend),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.output_backend),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.output_backend),
                source.clone(),
            ),
        ],
    );
    let voice_tts_provider = resolve_layered(
        VoiceConfig::default().tts_provider,
        "default:voice.tts_provider".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.tts_provider),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.tts_provider),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.tts_provider),
                source.clone(),
            ),
        ],
    );
    let voice_tts_model = resolve_layered(
        VoiceConfig::default().tts_model,
        "default:voice.tts_model".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.tts_model),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.tts_model),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.tts_model),
                source.clone(),
            ),
        ],
    );
    let voice_name = resolve_layered(
        VoiceConfig::default().voice,
        "default:voice.voice".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.voice),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.voice),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.voice),
                source.clone(),
            ),
        ],
    );
    let voice_tone = resolve_layered(
        VoiceConfig::default().tone,
        "default:voice.tone".into(),
        vec![
            (
                voice_string_layer(global.policy.voice.as_ref(), |voice| &voice.tone),
                global_source.clone(),
            ),
            (
                voice_string_layer(profile.policy.voice.as_ref(), |voice| &voice.tone),
                profile_source.clone(),
            ),
            (
                voice_string_layer(parsed.policy.voice.as_ref(), |voice| &voice.tone),
                source.clone(),
            ),
        ],
    );
    let prompt_refinement = (!prompt_refinements.is_empty()).then(|| PromptRefinement {
        instructions: prompt_refinement_instructions(&prompt_refinements),
        model: prompt_refinements
            .iter()
            .find_map(|refinement| refinement.model.clone())
            .map(ModelRef::from),
    });
    let system_prompt =
        system_prompt_with_refinement_awareness(&parsed.system_prompt, &prompt_refinements);
    let per_tool_output_modes = per_tool_output_mode_overrides(&parsed.tool_overrides);
    let per_tool_output_interpretation_models =
        per_tool_output_interpretation_model_overrides(&parsed.tool_overrides);
    let per_tool_output_guidance = per_tool_output_guidance_overrides(&parsed.tool_overrides);

    let agent = AgentConfig {
        id: parsed.id.clone(),
        name: parsed.name.clone(),
        system_prompt,
        model: ModelRef::from(model.value.clone()),
        prompt_refinement,
        voice: VoiceConfig {
            input_enabled: voice_input_enabled.value,
            output_enabled: voice_output_enabled.value,
            input_backend: voice_input_backend.value.clone(),
            input_provider: voice_input_provider.value.clone(),
            input_model: voice_input_model.value.clone(),
            output_backend: voice_output_backend.value.clone(),
            tts_provider: voice_tts_provider.value.clone(),
            tts_model: voice_tts_model.value.clone(),
            voice: voice_name.value.clone(),
            tone: voice_tone.value.clone(),
        },
        tool_policy: ToolPolicy {
            max_calls: max_tool_calls.value,
            allowed_tools,
            allowed_categories: allowed_tool_categories.value.clone(),
            required_tool: None,
            visibility: tool_visibility.value,
            approval_mode: ToolPolicy::default().approval_mode,
            approval_controller,
            output_mode: tool_output_mode.value,
            output_interpretation_model: tool_output_interpretation_model
                .value
                .clone()
                .map(ModelRef::from),
            per_tool_output_modes: per_tool_output_modes
                .iter()
                .map(|(tool_id, mode)| (ToolId::from(tool_id.clone()), *mode))
                .collect::<HashMap<_, _>>(),
            per_tool_output_interpretation_models: per_tool_output_interpretation_models
                .iter()
                .map(|(tool_id, model)| {
                    (ToolId::from(tool_id.clone()), ModelRef::from(model.clone()))
                })
                .collect::<HashMap<_, _>>(),
            per_tool_output_guidance: per_tool_output_guidance
                .iter()
                .map(|(tool_id, guidance)| (ToolId::from(tool_id.clone()), guidance.clone()))
                .collect::<HashMap<_, _>>(),
        },
        context_policy: ContextPolicy {
            compaction: ContextCompactionPolicy {
                max_tokens_before_compaction: max_tokens_before_compaction.value,
                max_output_tokens: max_compaction_output_tokens.value,
                guidance: compaction_guidance.value.clone(),
            },
        },
        execution_policy: ExecutionPolicy {
            max_subagent_depth: max_subagent_depth.value,
            max_recursion_depth: max_recursion_depth.value,
        },
        cost_policy: CostPolicy {
            input_cost_per_million: input_cost_per_million.value,
            output_cost_per_million: output_cost_per_million.value,
        },
        conversation_history: Vec::new(),
        compacted_context: None,
        memory_fragments: Vec::new(),
        ingestion_artifacts: Vec::new(),
        allowed_skill_categories: allowed_skill_categories.value.clone(),
        skill_views: Vec::new(),
        subagent_configs: Vec::new(),
    };

    let mut values = vec![
        config_value("profile.id", profile.id, &profile_source),
        config_value("profile.name", profile.name, &profile_source),
        config_value("agent.id", parsed.id, &source),
        config_value("agent.name", parsed.name, &source),
        config_value(
            "agent.prompt_refinement.enabled",
            !prompt_refinements.is_empty(),
            &source,
        ),
        config_value(
            "agent.prompt_refinement.count",
            prompt_refinements.len(),
            &source,
        ),
        config_value(
            "agent.prompt_refinement.instructions",
            (!prompt_refinements.is_empty())
                .then(|| prompt_refinement_instructions(&prompt_refinements)),
            &source,
        ),
        config_value(
            "agent.prompt_refinement.model",
            prompt_refinements
                .iter()
                .find_map(|refinement| refinement.model.clone()),
            &source,
        ),
        config_value(
            "agent.prompt_refinement.agent_awareness",
            prompt_refinements
                .iter()
                .any(|refinement| refinement.agent_awareness),
            &source,
        ),
        config_value(
            "agent.voice.input_enabled",
            voice_input_enabled.value,
            &voice_input_enabled.source,
        ),
        config_value(
            "agent.voice.output_enabled",
            voice_output_enabled.value,
            &voice_output_enabled.source,
        ),
        config_value(
            "agent.voice.input_backend",
            voice_input_backend.value.clone(),
            &voice_input_backend.source,
        ),
        config_value(
            "agent.voice.input_provider",
            voice_input_provider.value.clone(),
            &voice_input_provider.source,
        ),
        config_value(
            "agent.voice.input_model",
            voice_input_model.value.clone(),
            &voice_input_model.source,
        ),
        config_value(
            "agent.voice.output_backend",
            voice_output_backend.value.clone(),
            &voice_output_backend.source,
        ),
        config_value(
            "agent.voice.tts_provider",
            voice_tts_provider.value.clone(),
            &voice_tts_provider.source,
        ),
        config_value(
            "agent.voice.tts_model",
            voice_tts_model.value.clone(),
            &voice_tts_model.source,
        ),
        config_value(
            "agent.voice.voice",
            voice_name.value.clone(),
            &voice_name.source,
        ),
        config_value(
            "agent.voice.tone",
            voice_tone.value.clone(),
            &voice_tone.source,
        ),
        config_value("agent.model.default", model.value.clone(), &model.source),
        config_value(
            "agent.tool_policy.max_calls",
            max_tool_calls.value,
            &max_tool_calls.source,
        ),
        config_value(
            "agent.execution_policy.max_subagent_depth",
            max_subagent_depth.value,
            &max_subagent_depth.source,
        ),
        config_value(
            "agent.execution_policy.max_recursion_depth",
            max_recursion_depth.value,
            &max_recursion_depth.source,
        ),
        config_value(
            "agent.tool_policy.allowed_tools",
            allowed_tool_ids.value,
            &allowed_tool_ids.source,
        ),
        config_value(
            "agent.tool_policy.allowed_categories",
            allowed_tool_categories.value,
            &allowed_tool_categories.source,
        ),
        config_value(
            "agent.tool_policy.approval_controller.agent",
            approval_controller_agent.value,
            &approval_controller_agent.source,
        ),
        config_value(
            "agent.tool_policy.approval_controller.allowed_tools",
            approval_controller_allowed_tools.value,
            &approval_controller_allowed_tools.source,
        ),
        config_value(
            "agent.tool_policy.approval_controller.allowed_categories",
            approval_controller_allowed_tool_categories.value,
            &approval_controller_allowed_tool_categories.source,
        ),
        config_value(
            "agent.skill_policy.allowed_categories",
            allowed_skill_categories.value,
            &allowed_skill_categories.source,
        ),
        config_value(
            "agent.hook_policy.disabled_lifecycle_hooks",
            disabled_lifecycle_hooks.value,
            &disabled_lifecycle_hooks.source,
        ),
        config_value(
            "agent.tool_policy.output_mode",
            tool_output_mode.value,
            &tool_output_mode.source,
        ),
        config_value(
            "agent.tool_policy.output_interpretation_model",
            tool_output_interpretation_model.value,
            &tool_output_interpretation_model.source,
        ),
        config_value(
            "agent.tool_policy.per_tool_output_modes",
            per_tool_output_modes,
            &source,
        ),
        config_value(
            "agent.tool_policy.per_tool_output_interpretation_models",
            per_tool_output_interpretation_models,
            &source,
        ),
        config_value(
            "agent.tool_policy.per_tool_output_guidance",
            per_tool_output_guidance,
            &source,
        ),
        config_value(
            "agent.tool_policy.visibility",
            tool_visibility.value,
            &tool_visibility.source,
        ),
        config_value(
            "agent.memory_policy.load",
            load_memory.value,
            &load_memory.source,
        ),
        config_value(
            "agent.skill_policy.load",
            load_skills.value,
            &load_skills.source,
        ),
        config_value(
            "agent.context_policy.max_tokens_before_compaction",
            max_tokens_before_compaction.value,
            &max_tokens_before_compaction.source,
        ),
        config_value(
            "agent.context_policy.max_compaction_output_tokens",
            max_compaction_output_tokens.value,
            &max_compaction_output_tokens.source,
        ),
        config_value(
            "agent.context_policy.compaction_guidance",
            compaction_guidance.value,
            &compaction_guidance.source,
        ),
        config_value(
            "agent.ingestion_policy.guardrail_mode",
            ingestion_guardrail.value,
            &ingestion_guardrail.source,
        ),
        config_value(
            "agent.ingestion_policy.guardrail_model",
            ingestion_guardrail_model.value,
            &ingestion_guardrail_model.source,
        ),
        config_value(
            "agent.cost_policy.input_cost_per_million",
            input_cost_per_million.value,
            &input_cost_per_million.source,
        ),
        config_value(
            "agent.cost_policy.output_cost_per_million",
            output_cost_per_million.value,
            &output_cost_per_million.source,
        ),
    ];
    if let Some(model) = model_config {
        values.extend([
            config_value("model.id", model.id, &model_source),
            config_value(
                "model.max_context_tokens",
                model.max_context_tokens,
                &model_source,
            ),
            config_value(
                "model.max_output_tokens",
                model.max_output_tokens,
                &model_source,
            ),
            config_value(
                "model.default_temperature",
                model.default_temperature,
                &model_source,
            ),
            config_value("model.tool_support", model.tool_support, &model_source),
            config_value(
                "model.available_modalities",
                model.available_modalities,
                &model_source,
            ),
            config_value("model.reasoning_mode", model.reasoning_mode, &model_source),
            config_value("model.privacy_level", model.privacy_level, &model_source),
            config_value("model.cost_tier", model.cost_tier, &model_source),
            config_value("model.metadata", model.metadata, &model_source),
        ]);
    }

    ResolvedAgentConfig { agent, values }
}

fn resolve_layered<T: Clone>(
    default: T,
    default_source: String,
    layers: Vec<(Option<T>, String)>,
) -> ResolvedValue<T> {
    let mut resolved = ResolvedValue {
        value: default,
        source: default_source,
    };
    for (value, source) in layers {
        if let Some(value) = value {
            resolved = ResolvedValue { value, source };
        }
    }
    resolved
}

fn voice_string_layer<F>(voice: Option<&VoiceConfigFile>, accessor: F) -> Option<Option<String>>
where
    F: Fn(&VoiceConfigFile) -> &Option<String>,
{
    voice
        .and_then(|voice| clean_optional(accessor(voice).clone()))
        .map(Some)
}

fn prompt_refinement_rules(parsed: &AgentToml) -> Vec<AgentPromptRefinementConfig> {
    parsed
        .prompt_refinement
        .iter()
        .chain(parsed.prompt_refinements.iter())
        .cloned()
        .collect()
}

fn prompt_refinement_instructions(refinements: &[AgentPromptRefinementConfig]) -> String {
    if refinements.len() == 1 && refinements[0].id.is_none() && refinements[0].when.is_none() {
        return refinements[0].instructions.clone();
    }
    let rules = refinements
        .iter()
        .enumerate()
        .map(|(idx, refinement)| prompt_refinement_rule_line(idx, refinement))
        .collect::<Vec<_>>()
        .join("\n");
    format!("Apply the relevant prompt refinement rule(s) for the user's topic or task:\n{rules}")
}

fn prompt_refinement_rule_line(idx: usize, refinement: &AgentPromptRefinementConfig) -> String {
    let id = refinement
        .id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .unwrap_or_else(|| format!("rule-{}", idx + 1));
    let when = refinement
        .when
        .as_deref()
        .map(str::trim)
        .filter(|when| !when.is_empty())
        .map(|when| format!(" when {when}"))
        .unwrap_or_default();
    format!("- {id}{when}: {}", refinement.instructions.trim())
}

fn system_prompt_with_refinement_awareness(
    system_prompt: &str,
    refinements: &[AgentPromptRefinementConfig],
) -> String {
    if refinements.is_empty()
        || !refinements
            .iter()
            .any(|refinement| refinement.agent_awareness)
    {
        return system_prompt.to_string();
    }
    format!(
        "{}\n\n<prompt-refinement-guidance>\nUser prompts are preprocessed before you see them. Refinement instructions:\n{}\n</prompt-refinement-guidance>",
        system_prompt.trim_end(),
        prompt_refinement_instructions(refinements).trim()
    )
}

fn per_tool_output_mode_overrides(
    overrides: &[AgentToolOutputOverrideConfig],
) -> BTreeMap<String, ToolOutputMode> {
    overrides
        .iter()
        .filter_map(|override_config| {
            override_config
                .output_mode
                .map(|mode| (override_config.id.trim().to_string(), mode))
        })
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

fn per_tool_output_interpretation_model_overrides(
    overrides: &[AgentToolOutputOverrideConfig],
) -> BTreeMap<String, String> {
    overrides
        .iter()
        .filter_map(|override_config| {
            override_config
                .output_interpretation_model
                .as_deref()
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .map(|model| (override_config.id.trim().to_string(), model.to_string()))
        })
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

fn per_tool_output_guidance_overrides(
    overrides: &[AgentToolOutputOverrideConfig],
) -> BTreeMap<String, String> {
    overrides
        .iter()
        .filter_map(|override_config| {
            override_config
                .output_interpretation_guidance
                .as_deref()
                .map(str::trim)
                .filter(|guidance| !guidance.is_empty())
                .map(|guidance| (override_config.id.trim().to_string(), guidance.to_string()))
        })
        .filter(|(id, _)| !id.is_empty())
        .collect()
}

fn read_model_config(path: &Path) -> Result<ModelConfig, ConfigError> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}

fn read_agent_config(path: &Path) -> Result<AgentToml, ConfigError> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}

fn read_profile_config(path: &Path) -> Result<ProfileToml, ConfigError> {
    Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
}

fn agent_summary(agent: AgentToml, path: PathBuf) -> AgentSummary {
    AgentSummary {
        id: agent.id,
        name: agent.name,
        path,
    }
}

fn profile_summary(profile: ProfileToml, path: PathBuf) -> ProfileSummary {
    ProfileSummary {
        id: profile.id,
        name: profile.name,
        path,
    }
}

fn write_profile_grants(
    paths: &StoragePaths,
    from_profile: &str,
    grants: &[ProfileGrant],
) -> Result<(), ConfigError> {
    paths.ensure_profile_dirs(from_profile)?;
    write_storage_text(
        paths,
        paths.profile_grants_file(from_profile),
        serde_json::to_string_pretty(grants)?,
    )?;
    Ok(())
}

fn write_storage_text(
    paths: &StoragePaths,
    path: impl AsRef<Path>,
    text: String,
) -> Result<(), ConfigError> {
    paths.ensure_quota_for_path_write(
        path.as_ref(),
        u64::try_from(text.len()).unwrap_or(u64::MAX),
    )?;
    std::fs::write(path, text)?;
    Ok(())
}

fn grant_id(
    from_profile: &str,
    to_profile: &str,
    kind: ProfileGrantKind,
    resource: &str,
) -> String {
    format!(
        "grant-{}-{}-{}-{}",
        sanitize_id_fragment(from_profile),
        sanitize_id_fragment(to_profile),
        grant_kind_label(kind),
        sanitize_id_fragment(resource)
    )
}

fn grant_kind_label(kind: ProfileGrantKind) -> &'static str {
    match kind {
        ProfileGrantKind::Agent => "agent",
        ProfileGrantKind::Memory => "memory",
        ProfileGrantKind::Tool => "tool",
        ProfileGrantKind::Skill => "skill",
        ProfileGrantKind::Category => "category",
    }
}

fn sanitize_id_fragment(value: &str) -> String {
    let out = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if out.is_empty() { "all".into() } else { out }
}

fn timestamp_string() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    nanos.to_string()
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_model_id(id: &str) -> Result<(), ConfigError> {
    let invalid = id.trim().is_empty()
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || id.contains(std::path::MAIN_SEPARATOR);
    if invalid {
        return Err(ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid model id: {id}"),
        )));
    }
    Ok(())
}

fn validate_profile_id(id: &str) -> Result<(), ConfigError> {
    let valid = !id.trim().is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidInput(format!(
            "invalid profile id: {id}"
        )))
    }
}

fn validate_agent_id(id: &str) -> Result<(), ConfigError> {
    let valid = !id.trim().is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidInput(format!("invalid agent id: {id}")))
    }
}

fn parse_agent_draft_body(body: &str) -> Option<AgentConfigFile> {
    serde_json::from_str::<AgentConfigFile>(body)
        .ok()
        .or_else(|| toml::from_str::<AgentConfigFile>(body).ok())
}

fn agent_id_for_draft(draft_id: &str, name: &str) -> String {
    let suffix = [slugify(draft_id), slugify(name), "agent".into()]
        .into_iter()
        .find(|candidate| !candidate.is_empty())
        .unwrap_or_else(|| "agent".into());
    format!("capability-{suffix}")
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn non_empty(value: &str, field: &'static str) -> Result<String, ConfigError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(ConfigError::InvalidInput(format!("missing {field}")))
    } else {
        Ok(value)
    }
}

fn validate_agent_config(agent: &AgentConfigFile) -> Result<(), ConfigError> {
    validate_agent_id(&agent.id)?;
    if agent.name.trim().is_empty() {
        return Err(ConfigError::InvalidInput(
            "agent name cannot be empty".into(),
        ));
    }
    if agent.system_prompt.trim().is_empty() {
        return Err(ConfigError::InvalidInput(
            "agent system prompt cannot be empty".into(),
        ));
    }
    if let Some(model) = &agent.model {
        validate_model_id(model)?;
    }
    if let Some(model) = &agent.tool_output_interpretation_model {
        validate_model_id(model)?;
    }
    if agent.max_tokens_before_compaction == Some(0) {
        return Err(ConfigError::InvalidInput(
            "max_tokens_before_compaction must be greater than zero".into(),
        ));
    }
    if agent.max_compaction_output_tokens == Some(0) {
        return Err(ConfigError::InvalidInput(
            "max_compaction_output_tokens must be greater than zero".into(),
        ));
    }
    if let Some(guidance) = &agent.compaction_guidance
        && guidance.trim().is_empty()
    {
        return Err(ConfigError::InvalidInput(
            "compaction_guidance cannot be empty".into(),
        ));
    }
    validate_voice_config_file(agent.voice.as_ref())?;
    for refinement in agent
        .prompt_refinement
        .iter()
        .chain(agent.prompt_refinements.iter())
    {
        if let Some(id) = refinement.id.as_deref()
            && id.trim().is_empty()
        {
            return Err(ConfigError::InvalidInput(
                "prompt refinement id cannot be empty".into(),
            ));
        }
        if let Some(when) = refinement.when.as_deref()
            && when.trim().is_empty()
        {
            return Err(ConfigError::InvalidInput(
                "prompt refinement topic/task matcher cannot be empty".into(),
            ));
        }
        if refinement.instructions.trim().is_empty() {
            return Err(ConfigError::InvalidInput(
                "prompt refinement instructions cannot be empty".into(),
            ));
        }
        if let Some(model) = &refinement.model {
            validate_model_id(model)?;
        }
    }
    if let Some(allowed_tools) = &agent.allowed_tools {
        for tool in allowed_tools {
            validate_resource_id(tool)?;
        }
    }
    if let Some(categories) = &agent.allowed_tool_categories {
        for category in categories {
            validate_resource_id(category)?;
        }
    }
    if let Some(controller) = &agent.approval_controller_agent {
        validate_agent_id(controller)?;
    }
    if let Some(tools) = &agent.approval_controller_allowed_tools {
        for tool in tools {
            validate_resource_id(tool)?;
        }
    }
    if let Some(categories) = &agent.approval_controller_allowed_tool_categories {
        for category in categories {
            validate_resource_id(category)?;
        }
    }
    if let Some(categories) = &agent.allowed_skill_categories {
        for category in categories {
            validate_resource_id(category)?;
        }
    }
    validate_lifecycle_hook_ids(agent.disabled_lifecycle_hooks.as_deref())?;
    for override_config in &agent.tool_overrides {
        validate_resource_id(&override_config.id)?;
        if override_config.output_mode.is_none()
            && override_config
                .output_interpretation_model
                .as_deref()
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .is_none()
            && override_config
                .output_interpretation_guidance
                .as_deref()
                .map(str::trim)
                .filter(|guidance| !guidance.is_empty())
                .is_none()
        {
            return Err(ConfigError::InvalidInput(
                "tool override must set output_mode, output_interpretation_model, or output_interpretation_guidance".into(),
            ));
        }
        if let Some(model) = &override_config.output_interpretation_model {
            validate_model_id(model)?;
        }
        if let Some(guidance) = &override_config.output_interpretation_guidance
            && guidance.trim().is_empty()
        {
            return Err(ConfigError::InvalidInput(
                "tool override output interpretation guidance cannot be empty".into(),
            ));
        }
    }
    Ok(())
}

fn validate_voice_config_file(voice: Option<&VoiceConfigFile>) -> Result<(), ConfigError> {
    let Some(voice) = voice else {
        return Ok(());
    };
    for (field, value) in [
        ("voice.input_backend", voice.input_backend.as_deref()),
        ("voice.input_provider", voice.input_provider.as_deref()),
        ("voice.input_model", voice.input_model.as_deref()),
        ("voice.output_backend", voice.output_backend.as_deref()),
        ("voice.tts_provider", voice.tts_provider.as_deref()),
        ("voice.tts_model", voice.tts_model.as_deref()),
        ("voice.voice", voice.voice.as_deref()),
        ("voice.tone", voice.tone.as_deref()),
    ] {
        if let Some(value) = value
            && value.trim().is_empty()
        {
            return Err(ConfigError::InvalidInput(format!(
                "{field} cannot be empty"
            )));
        }
    }
    for (field, value) in [
        ("voice.input_backend", voice.input_backend.as_deref()),
        ("voice.output_backend", voice.output_backend.as_deref()),
    ] {
        if let Some(value) = value {
            let normalized = value.trim().to_ascii_lowercase();
            if !matches!(normalized.as_str(), "local" | "cloud") {
                return Err(ConfigError::InvalidInput(format!(
                    "{field} must be local or cloud"
                )));
            }
        }
    }
    Ok(())
}

fn validate_grant_id(id: &str) -> Result<(), ConfigError> {
    let valid = !id.trim().is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidInput(format!("invalid grant id: {id}")))
    }
}

fn validate_resource_id(resource: &str) -> Result<String, ConfigError> {
    let resource = resource.trim();
    let valid = !resource.is_empty() && resource.chars().all(|ch| !ch.is_control());
    if valid {
        Ok(resource.into())
    } else {
        Err(ConfigError::InvalidInput(format!(
            "invalid grant resource: {resource}"
        )))
    }
}

fn validate_lifecycle_hook_id(hook_id: &str) -> Result<String, ConfigError> {
    validate_resource_id(hook_id).map_err(|_| {
        ConfigError::InvalidInput(format!("invalid lifecycle hook id: {}", hook_id.trim()))
    })
}

fn validate_lifecycle_hook_ids(hooks: Option<&[String]>) -> Result<(), ConfigError> {
    if let Some(hooks) = hooks {
        for hook in hooks {
            validate_lifecycle_hook_id(hook)?;
        }
    }
    Ok(())
}

fn normalize_lifecycle_hook_ids(hooks: Vec<String>) -> Result<Vec<String>, ConfigError> {
    hooks
        .into_iter()
        .map(|hook| validate_lifecycle_hook_id(&hook))
        .collect::<Result<BTreeSet<_>, _>>()
        .map(|hooks| hooks.into_iter().collect())
}

fn config_value(
    key: impl Into<String>,
    value: impl Serialize,
    source: &str,
) -> ConfigValueExplanation {
    ConfigValueExplanation {
        key: key.into(),
        value: toml_value_to_json(value),
        source: source.into(),
    }
}

fn toml_value_to_json(value: impl Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_agent_config_resolves() {
        let dir = std::env::temp_dir().join(format!("agent-config-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        let resolved = resolver.resolve_default_agent().unwrap();
        assert_eq!(resolved.agent.id, "fake-agent");
        assert_eq!(resolved.agent.model.0, "fake-model");
        assert!(resolver.paths.default_agent_config().exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn named_agent_config_resolves_from_active_profile() {
        let dir = std::env::temp_dir().join(format!("agent-named-test-{}", uuid_like()));
        let paths = StoragePaths::new(&dir);
        let resolver = ConfigResolver::new(paths.clone());
        resolver.ensure_default_files().unwrap();
        let agent_path = paths.agent_config("critic");
        std::fs::create_dir_all(agent_path.parent().unwrap()).unwrap();
        std::fs::write(
            &agent_path,
            r#"id = "critic"
name = "Critic"
system_prompt = "Review carefully."
max_tool_calls = 1
"#,
        )
        .unwrap();

        let resolved = resolver.resolve_agent("critic").unwrap();
        assert_eq!(resolved.agent.id, "critic");
        assert_eq!(resolved.agent.name, "Critic");
        assert_eq!(resolved.agent.tool_policy.max_calls, 1);
        assert!(resolved.values.iter().any(|value| {
            value.key == "agent.id" && value.source == format!("agent:{}", agent_path.display())
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_profile_grant_resolves_from_source_profile() {
        let dir = std::env::temp_dir().join(format!("agent-grant-test-{}", uuid_like()));
        let main_paths = StoragePaths::new(&dir);
        let research_paths = StoragePaths::new_with_profile(&dir, "research");
        let resolver = ConfigResolver::new(main_paths.clone());
        resolver.create_profile("research", None).unwrap();
        let agent_path = main_paths.agent_config("critic");
        std::fs::create_dir_all(agent_path.parent().unwrap()).unwrap();
        std::fs::write(
            &agent_path,
            r#"id = "critic"
name = "Critic"
system_prompt = "Review carefully."
"#,
        )
        .unwrap();
        resolver
            .grant_profile_access("main", "research", ProfileGrantKind::Agent, "critic")
            .unwrap();

        let resolved = ConfigResolver::new(research_paths)
            .resolve_agent("critic")
            .unwrap();
        assert_eq!(resolved.agent.id, "critic");
        assert!(resolved.values.iter().any(|value| {
            value.key == "agent.shared_from_profile" && value.source.contains("profile-grant:")
        }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_registry_round_trips_and_exports() {
        let dir = std::env::temp_dir().join(format!("agent-registry-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        let agent = AgentConfigFile {
            id: "critic".into(),
            name: "Critic".into(),
            system_prompt: "Review carefully.".into(),
            prompt_refinement: Some(AgentPromptRefinementConfig {
                id: None,
                when: None,
                instructions: "Clarify the request.".into(),
                model: Some("fake-model".into()),
                agent_awareness: true,
            }),
            prompt_refinements: vec![AgentPromptRefinementConfig {
                id: Some("code-review".into()),
                when: Some("the task asks for code review".into()),
                instructions: "Focus the prompt on regression risks and missing tests.".into(),
                model: None,
                agent_awareness: true,
            }],
            tool_overrides: vec![AgentToolOutputOverrideConfig {
                id: "echo".into(),
                output_mode: Some(ToolOutputMode::Raw),
                output_interpretation_model: Some("echo-interpreter".into()),
                output_interpretation_guidance: Some("Return exact echo JSON.".into()),
            }],
            voice: None,
            model: Some("fake-model".into()),
            max_tool_calls: Some(1),
            max_subagent_depth: Some(2),
            max_recursion_depth: Some(1),
            allowed_tools: Some(vec!["echo".into()]),
            allowed_tool_categories: Some(vec!["demo".into()]),
            approval_controller_agent: Some("safety-controller".into()),
            approval_controller_allowed_tools: Some(vec!["shell".into()]),
            approval_controller_allowed_tool_categories: Some(vec!["sensitive".into()]),
            allowed_skill_categories: Some(vec!["review".into()]),
            disabled_lifecycle_hooks: Some(vec!["adapter:demo:audit".into()]),
            tool_output_mode: Some(ToolOutputMode::Raw),
            tool_output_interpretation_model: Some("general-interpreter".into()),
            tool_visibility: Some(VisibilityLevel::NameOnly),
            load_memory: Some(true),
            load_skills: Some(true),
            max_tokens_before_compaction: Some(256),
            max_compaction_output_tokens: Some(96),
            compaction_guidance: Some("Keep review decisions.".into()),
            ingestion_guardrail: Some(IngestionGuardrailMode::Warn),
            ingestion_guardrail_model: Some("guardrail-model".into()),
            input_cost_per_million: Some(0.1),
            output_cost_per_million: Some(0.2),
        };
        resolver.save_agent_config(&agent).unwrap();
        assert_eq!(
            resolver.show_agent_config("critic").unwrap(),
            Some(agent.clone())
        );
        let resolved = resolver.resolve_agent("critic").unwrap();
        assert!(resolved.agent.prompt_refinement.is_some());
        assert_eq!(
            resolved.agent.tool_policy.allowed_categories,
            vec!["demo".to_string()]
        );
        let approval_controller = resolved
            .agent
            .tool_policy
            .approval_controller
            .as_ref()
            .expect("approval controller should resolve");
        assert_eq!(approval_controller.agent_id, "safety-controller");
        assert_eq!(
            approval_controller.allowed_tools,
            vec![ToolId::from("shell")]
        );
        assert_eq!(
            approval_controller.allowed_categories,
            vec!["sensitive".to_string()]
        );
        assert_eq!(
            resolved.agent.allowed_skill_categories,
            vec!["review".to_string()]
        );
        assert!(resolved.values.iter().any(|value| {
            value.key == "agent.hook_policy.disabled_lifecycle_hooks"
                && value.value == serde_json::json!(["adapter:demo:audit"])
        }));
        assert_eq!(resolved.agent.execution_policy.max_subagent_depth, 2);
        assert_eq!(resolved.agent.execution_policy.max_recursion_depth, 1);
        assert_eq!(
            resolved
                .agent
                .context_policy
                .compaction
                .max_tokens_before_compaction,
            Some(256)
        );
        assert_eq!(
            resolved.agent.context_policy.compaction.max_output_tokens,
            Some(96)
        );
        assert_eq!(
            resolved.agent.context_policy.compaction.guidance.as_deref(),
            Some("Keep review decisions.")
        );
        assert!(
            resolved
                .agent
                .system_prompt
                .contains("<prompt-refinement-guidance>")
        );
        assert!(
            resolved
                .agent
                .prompt_refinement
                .as_ref()
                .is_some_and(|refinement| refinement
                    .instructions
                    .contains("code-review when the task asks for code review"))
        );
        assert!(
            resolved
                .values
                .iter()
                .any(|value| value.key == "agent.prompt_refinement.count" && value.value == 2)
        );
        assert!(
            resolved
                .values
                .iter()
                .any(|value| value.key == "agent.memory_policy.load" && value.value == true)
        );
        assert!(
            resolved
                .values
                .iter()
                .any(|value| value.key == "agent.skill_policy.load" && value.value == true)
        );
        assert!(resolved.values.iter().any(|value| {
            value.key == "agent.ingestion_policy.guardrail_mode" && value.value == "warn"
        }));
        assert!(resolved.values.iter().any(|value| {
            value.key == "agent.ingestion_policy.guardrail_model"
                && value.value == "guardrail-model"
        }));
        assert_eq!(
            resolved
                .agent
                .tool_policy
                .per_tool_output_modes
                .get(&ToolId::from("echo")),
            Some(&ToolOutputMode::Raw)
        );
        assert_eq!(
            resolved
                .agent
                .tool_policy
                .per_tool_output_guidance
                .get(&ToolId::from("echo"))
                .map(String::as_str),
            Some("Return exact echo JSON.")
        );
        assert_eq!(
            resolved
                .agent
                .tool_policy
                .output_interpretation_model
                .as_ref()
                .map(|model| model.0.as_str()),
            Some("general-interpreter")
        );
        assert_eq!(
            resolved
                .agent
                .tool_policy
                .per_tool_output_interpretation_models
                .get(&ToolId::from("echo"))
                .map(|model| model.0.as_str()),
            Some("echo-interpreter")
        );
        assert!(
            resolver
                .list_agent_configs()
                .unwrap()
                .iter()
                .any(|entry| entry.id == "critic" && entry.name == "Critic")
        );
        let subagents = resolver.resolve_subagent_configs("fake-agent").unwrap();
        assert!(subagents.iter().any(|entry| entry.id == "critic"));
        assert!(!subagents.iter().any(|entry| entry.id == "fake-agent"));
        assert!(
            subagents
                .iter()
                .all(|entry| entry.subagent_configs.is_empty())
        );

        let export_path = dir.join("critic.toml");
        resolver
            .export_agent_config("critic", &export_path)
            .unwrap();
        assert!(resolver.delete_agent_config("critic").unwrap());
        assert!(resolver.show_agent_config("critic").unwrap().is_none());
        assert_eq!(
            resolver.import_agent_config(&export_path).unwrap(),
            agent.clone()
        );
        assert_eq!(resolver.resolve_agent("critic").unwrap().agent.id, "critic");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_created_draft_promotes_to_agent_config_and_rolls_back() {
        let dir = std::env::temp_dir().join(format!("agent-created-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));

        let agent = resolver
            .promote_agent_created_config(
                "draft.research.agent",
                "Research Agent",
                "Research carefully and cite sources.",
            )
            .unwrap();

        assert_eq!(agent.id, "capability-draft-research-agent");
        assert_eq!(agent.name, "Research Agent");
        assert_eq!(agent.system_prompt, "Research carefully and cite sources.");
        assert!(resolver.show_agent_config(&agent.id).unwrap().is_some());
        assert!(
            resolver
                .delete_agent_created_config("draft.research.agent", "Research Agent")
                .unwrap()
        );
        assert!(resolver.show_agent_config(&agent.id).unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_registry_round_trips() {
        let dir = std::env::temp_dir().join(format!("agent-model-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        let model = ModelConfig {
            id: "gpt-test".into(),
            provider: Some("openai".into()),
            api_base_url: Some("https://api.openai.com/v1".into()),
            api_key_env: Some("OPENAI_API_KEY".into()),
            allow_missing_api_key: Some(false),
            max_context_tokens: Some(128_000),
            max_output_tokens: Some(4096),
            default_temperature: Some(0.2),
            available_modalities: vec!["text".into(), "image".into()],
            reasoning_mode: Some("medium".into()),
            tool_support: Some(true),
            privacy_level: Some("cloud".into()),
            cost_tier: Some("cheap".into()),
            input_cost_per_million: Some(0.15),
            output_cost_per_million: Some(0.6),
            metadata: BTreeMap::from([("vendor".into(), serde_json::json!("openai"))]),
        };
        resolver.save_model(&model).unwrap();
        assert_eq!(
            resolver.show_model("gpt-test").unwrap(),
            Some(model.clone())
        );
        assert!(
            resolver
                .list_models()
                .unwrap()
                .iter()
                .any(|entry| entry.id == "gpt-test")
        );
        let export_path = dir.join("gpt-test.toml");
        resolver
            .export_model_config("gpt-test", &export_path)
            .unwrap();
        assert!(resolver.delete_model("gpt-test").unwrap());
        assert!(resolver.show_model("gpt-test").unwrap().is_none());
        assert_eq!(resolver.import_model_config(&export_path).unwrap(), model);
        assert!(resolver.show_model("gpt-test").unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_runtime_builds_openai_compatible_provider_config() {
        let runtime = ModelRuntimeConfig {
            provider: Some("ollama".into()),
            api_base_url: Some("http://localhost:9999/v1".into()),
            api_key_env: Some("LOCAL_KEY".into()),
            allow_missing_api_key: Some(false),
            max_output_tokens: Some(1024),
            default_temperature: Some(0.4),
            provider_options: Some(serde_json::json!({ "top_p": 0.8 })),
        };
        let config = runtime
            .rig_provider_config(ModelRef::from("local-model"), Some(256), Some(0.0))
            .unwrap();
        assert_eq!(
            config.api_base_url.as_deref(),
            Some("http://localhost:9999/v1")
        );
        assert_eq!(config.api_key_env, "LOCAL_KEY");
        assert!(!config.allow_missing_api_key);
        assert_eq!(config.model.0, "local-model");
        assert_eq!(config.max_output_tokens, Some(256));
        assert_eq!(config.temperature, Some(0.0));
        assert_eq!(
            config.additional_params,
            Some(serde_json::json!({ "top_p": 0.8 }))
        );
    }

    #[test]
    fn model_runtime_builds_native_provider_config() {
        let runtime = ModelRuntimeConfig {
            provider: Some("anthropic".into()),
            api_base_url: Some("ignored-for-native".into()),
            api_key_env: Some("ANTHROPIC_LOCAL_KEY".into()),
            allow_missing_api_key: Some(true),
            max_output_tokens: Some(1024),
            default_temperature: Some(0.4),
            provider_options: Some(serde_json::json!({ "top_k": 20 })),
        };
        let config = runtime.native_provider_config(
            ModelRef::from("claude-sonnet-4-5"),
            NativeProviderConfig::anthropic,
            Some(256),
            Some(0.0),
        );

        assert_eq!(config.api_key_env, "ANTHROPIC_LOCAL_KEY");
        assert!(config.allow_missing_api_key);
        assert_eq!(config.model.0, "claude-sonnet-4-5");
        assert_eq!(config.max_output_tokens, Some(256));
        assert_eq!(config.temperature, Some(0.0));
        assert_eq!(
            config.additional_params,
            Some(serde_json::json!({ "top_k": 20 }))
        );
    }

    #[test]
    fn model_runtime_uses_provider_options_metadata() {
        let dir = std::env::temp_dir().join(format!("agent-provider-options-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        let mut model = ModelConfig::for_id("provider-options-model");
        model.metadata.insert(
            "provider_options".into(),
            serde_json::json!({ "reasoning_effort": "medium" }),
        );
        resolver.save_model(&model).unwrap();

        let runtime = resolver
            .resolve_model_runtime("provider-options-model")
            .unwrap()
            .unwrap();
        assert_eq!(
            runtime.provider_options,
            Some(serde_json::json!({ "reasoning_effort": "medium" }))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_config_validates_provider_specific_options() {
        let dir =
            std::env::temp_dir().join(format!("agent-provider-validation-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));

        let mut anthropic = ModelConfig::for_id("claude-invalid");
        anthropic.provider = Some("anthropic".into());
        anthropic.metadata.insert(
            "provider_options".into(),
            serde_json::json!({ "reasoning_effort": "medium" }),
        );
        assert!(resolver.save_model(&anthropic).is_err());

        let mut invalid_top_p = ModelConfig::for_id("top-p-invalid");
        invalid_top_p.metadata.insert(
            "provider_options".into(),
            serde_json::json!({ "top_p": 1.5 }),
        );
        assert!(resolver.save_model(&invalid_top_p).is_err());

        let mut native_base = ModelConfig::for_id("native-base-invalid");
        native_base.provider = Some("gemini".into());
        native_base.api_base_url = Some("http://localhost:9999/v1".into());
        assert!(resolver.save_model(&native_base).is_err());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_supports_modality_uses_model_then_provider_capabilities() {
        let dir = std::env::temp_dir().join(format!("agent-modality-test-{}", uuid_like()));
        let paths = StoragePaths::new(&dir);
        std::fs::create_dir_all(paths.models_dir()).unwrap();
        std::fs::write(
            paths.models_dir().join(MODEL_METADATA_CATALOG_FILE),
            r#"{
              "schema_version": 1,
              "source": "profile-modality-test",
              "models": [
                {
                  "provider": "openai",
                  "model_id": "catalog-text-only",
                  "modalities": ["text"],
                  "source": "profile-modality-test/catalog-text-only"
                }
              ]
            }"#,
        )
        .unwrap();
        let resolver = ConfigResolver::new(paths);

        assert!(
            resolver
                .model_supports_modality("unsaved-openai", "image")
                .unwrap()
        );

        let mut fake = ModelConfig::for_id("fake-text-only");
        fake.provider = Some("fake".into());
        resolver.save_model(&fake).unwrap();
        assert!(
            !resolver
                .model_supports_modality("fake-text-only", "image")
                .unwrap()
        );

        let mut catalog_text_only = ModelConfig::for_id("catalog-text-only");
        catalog_text_only.provider = Some("rig".into());
        resolver.save_model(&catalog_text_only).unwrap();
        let support = resolver
            .model_modality_support("catalog-text-only", "image")
            .unwrap();
        assert!(!support.supported);
        assert_eq!(support.provider, "rig");
        assert_eq!(
            support.source,
            "model_metadata_catalog:profile-modality-test/catalog-text-only"
        );
        assert_eq!(support.available_modalities, vec!["text"]);
        assert!(
            !resolver
                .model_supports_modality("catalog-text-only", "image")
                .unwrap()
        );

        let mut document_only = ModelConfig::for_id("document-only");
        document_only.available_modalities = vec!["text".into(), "document".into()];
        resolver.save_model(&document_only).unwrap();
        let pdf_modalities = vec!["pdf".into(), "document".into(), "image".into()];
        let support = resolver
            .model_supports_any_modality("document-only", &pdf_modalities)
            .unwrap();
        assert!(support.supported);
        assert_eq!(support.modality, "document");

        let mut local_vision = ModelConfig::for_id("llava-local");
        local_vision.provider = Some("ollama".into());
        local_vision.available_modalities = vec!["text".into(), "image".into()];
        resolver.save_model(&local_vision).unwrap();
        assert!(
            resolver
                .model_supports_modality("llava-local", "image")
                .unwrap()
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_capability_probe_reports_declared_and_live_model_presence() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let body = r#"{"object":"list","data":[{"id":"live-model","modalities":["text","image"],"capabilities":["function_calling"],"supports_tools":true,"context_length":128000,"max_output_tokens":4096,"input_cost_per_million":0.15,"pricing":{"prompt":"0.00000015","completion":"0.0000006"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let dir = std::env::temp_dir().join(format!("agent-live-probe-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        let mut model = ModelConfig::for_id("live-model");
        model.provider = Some("ollama".into());
        model.api_base_url = Some(format!("http://{addr}/v1"));
        resolver.save_model(&model).unwrap();

        let probe = resolver.probe_model_capabilities("live-model").unwrap();
        server.join().unwrap();

        assert!(probe.saved_model);
        assert_eq!(probe.provider, "ollama");
        assert_eq!(probe.live_probe.status, "reachable");
        assert_eq!(probe.live_probe.model_found, Some(true));
        assert!(
            probe
                .live_probe
                .reported_modalities
                .iter()
                .any(|item| item == "image")
        );
        assert!(
            probe
                .live_probe
                .reported_capabilities
                .iter()
                .any(|item| item == "function_calling")
        );
        assert_eq!(probe.live_probe.reported_tool_support, Some(true));
        assert_eq!(
            probe.live_probe.reported_limits.get("context_tokens"),
            Some(&128_000)
        );
        assert_eq!(
            probe.live_probe.reported_limits.get("output_tokens"),
            Some(&4096)
        );
        assert_eq!(
            probe
                .live_probe
                .reported_pricing
                .get("input_per_million")
                .map(String::as_str),
            Some("0.15")
        );
        assert_eq!(
            probe
                .live_probe
                .reported_pricing
                .get("pricing.completion")
                .map(String::as_str),
            Some("0.0000006")
        );
        assert!(probe.declared_modalities.iter().any(|item| item == "text"));

        let mut anthropic = ModelConfig::for_id("claude-probe");
        anthropic.provider = Some("anthropic".into());
        anthropic.api_key_env = Some("SHINKAI_TEST_MISSING_ANTHROPIC_KEY".into());
        resolver.save_model(&anthropic).unwrap();
        let native = resolver.probe_model_capabilities("claude-probe").unwrap();
        assert_eq!(native.live_probe.status, "not_configured");
        assert_eq!(
            native.live_probe.source.as_deref(),
            Some("https://api.anthropic.com/v1/models")
        );
        assert_eq!(native.live_probe.model_found, None);
        assert!(!native.live_probe.attempted);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_capability_probe_fills_missing_catalog_metadata_from_curated_fallbacks() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let body = r#"{"object":"list","data":[{"id":"gpt-4o-mini"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let dir = std::env::temp_dir().join(format!("agent-curated-probe-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        let mut model = ModelConfig::for_id("gpt-4o-mini");
        model.provider = Some("rig".into());
        model.api_base_url = Some(format!("http://{addr}/v1"));
        model.max_context_tokens = Some(32_000);
        model.max_output_tokens = Some(2_048);
        model.input_cost_per_million = Some(0.11);
        model.output_cost_per_million = Some(0.22);
        resolver.save_model(&model).unwrap();

        let probe = resolver.probe_model_capabilities("gpt-4o-mini").unwrap();
        server.join().unwrap();

        assert_eq!(probe.live_probe.status, "reachable");
        assert_eq!(probe.live_probe.model_found, Some(true));
        assert_eq!(probe.declared_limits.get("context_tokens"), Some(&32_000));
        assert_eq!(probe.declared_limits.get("output_tokens"), Some(&2_048));
        assert_eq!(
            probe
                .declared_pricing
                .get("input_per_million")
                .map(String::as_str),
            Some("0.11")
        );
        assert!(
            probe
                .live_probe
                .reported_modalities
                .iter()
                .any(|modality| modality == "image")
        );
        assert!(
            probe
                .live_probe
                .reported_capabilities
                .iter()
                .any(|capability| capability == "function_calling")
        );
        assert_eq!(probe.live_probe.reported_tool_support, Some(true));
        assert_eq!(
            probe.live_probe.reported_limits.get("context_tokens"),
            Some(&128_000)
        );
        assert_eq!(
            probe.live_probe.reported_limits.get("output_tokens"),
            Some(&16_384)
        );
        assert_eq!(
            probe
                .live_probe
                .reported_pricing
                .get("input_per_million")
                .map(String::as_str),
            Some("0.15")
        );
        assert_eq!(
            probe
                .live_probe
                .reported_pricing
                .get("output_per_million")
                .map(String::as_str),
            Some("0.60")
        );
        assert_eq!(
            probe.live_probe.fallback_source.as_deref(),
            Some("https://platform.openai.com/docs/models/gpt-4o-mini")
        );
        assert!(
            probe
                .live_probe
                .message
                .as_deref()
                .is_some_and(|message| message.contains("curated offline metadata"))
        );

        let mut gemini = ModelConfig::for_id("gemini-2.5-flash");
        gemini.provider = Some("gemini".into());
        gemini.api_key_env = Some("SHINKAI_TEST_MISSING_GEMINI_KEY_FOR_CURATED_FALLBACK".into());
        resolver.save_model(&gemini).unwrap();
        let gemini_probe = resolver
            .probe_model_capabilities("gemini-2.5-flash")
            .unwrap();
        assert_eq!(gemini_probe.live_probe.status, "not_configured");
        assert_eq!(
            gemini_probe.live_probe.fallback_source.as_deref(),
            Some("https://ai.google.dev/gemini-api/docs/models/gemini")
        );
        assert!(
            gemini_probe
                .live_probe
                .reported_modalities
                .iter()
                .any(|modality| modality == "audio")
        );
        assert_eq!(
            gemini_probe.live_probe.reported_limits.get("input_tokens"),
            Some(&1_048_576)
        );
        assert_eq!(
            gemini_probe.live_probe.reported_limits.get("output_tokens"),
            Some(&65_536)
        );
        assert_eq!(
            gemini_probe
                .live_probe
                .reported_pricing
                .get("output_per_million")
                .map(String::as_str),
            Some("2.50")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_capability_probe_uses_profile_metadata_catalog_for_non_default_model() {
        let dir = std::env::temp_dir().join(format!("agent-catalog-probe-test-{}", uuid_like()));
        let paths = StoragePaths::new(&dir);
        std::fs::create_dir_all(paths.models_dir()).unwrap();
        std::fs::write(
            paths.models_dir().join(MODEL_METADATA_CATALOG_FILE),
            r#"{
              "schema_version": 1,
              "source": "profile-test-catalog",
              "models": [
                {
                  "provider": "openai",
                  "model_id": "private-non-default",
                  "modalities": ["text", "image"],
                  "capabilities": ["function_calling", "structured_outputs"],
                  "tool_support": true,
                  "limits": {
                    "context_tokens": 123456,
                    "output_tokens": 7890
                  },
                  "pricing": {
                    "input_per_million": "1.23"
                  },
                  "source": "profile-test-catalog/private-non-default"
                }
              ]
            }"#,
        )
        .unwrap();

        let resolver = ConfigResolver::new(paths);
        let mut model = ModelConfig::for_id("private-non-default");
        model.provider = Some("rig".into());
        resolver.save_model(&model).unwrap();

        let probe = resolver
            .probe_model_capabilities("private-non-default")
            .unwrap();

        assert_eq!(probe.live_probe.status, "not_configured");
        assert_eq!(
            probe.live_probe.fallback_source.as_deref(),
            Some("profile-test-catalog/private-non-default")
        );
        assert!(
            probe
                .live_probe
                .reported_modalities
                .iter()
                .any(|modality| modality == "image")
        );
        assert!(
            probe
                .live_probe
                .reported_capabilities
                .iter()
                .any(|capability| capability == "structured_outputs")
        );
        assert_eq!(probe.live_probe.reported_tool_support, Some(true));
        assert_eq!(
            probe.live_probe.reported_limits.get("context_tokens"),
            Some(&123_456)
        );
        assert_eq!(
            probe
                .live_probe
                .reported_pricing
                .get("input_per_million")
                .map(String::as_str),
            Some("1.23")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn native_provider_catalog_parsers_extract_model_ids_and_capabilities() {
        let anthropic = parse_anthropic_model_catalog(
            r#"{"data":[{"id":"claude-sonnet-4-5","display_name":"Claude Sonnet","input_modalities":["text","image"],"capabilities":["tool_use"],"supports_tools":true,"input_token_limit":200000,"output_token_limit":8192}]}"#,
        )
        .unwrap();
        assert_eq!(anthropic.model_ids, vec!["claude-sonnet-4-5"]);
        assert!(anthropic.capabilities.is_empty());
        let anthropic_metadata = anthropic.model_metadata.get("claude-sonnet-4-5").unwrap();
        assert!(
            anthropic_metadata
                .modalities
                .iter()
                .any(|modality| modality == "image")
        );
        assert!(
            anthropic_metadata
                .capabilities
                .iter()
                .any(|capability| capability == "tool_use")
        );
        assert_eq!(anthropic_metadata.tool_support, Some(true));
        assert_eq!(
            anthropic_metadata.limits.get("input_tokens"),
            Some(&200_000)
        );
        assert_eq!(anthropic_metadata.limits.get("output_tokens"), Some(&8192));

        let gemini = parse_gemini_model_catalog(
            r#"{
                "models": [
                    {
                        "name": "models/gemini-2.5-flash",
                        "baseModelId": "gemini-2.5-flash",
                        "supportedGenerationMethods": ["generateContent", "countTokens"],
                        "inputTokenLimit": 1048576,
                        "outputTokenLimit": 65536
                    },
                    {
                        "name": "models/gemini-embedding-001",
                        "supportedGenerationMethods": ["embedContent"]
                    }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(
            gemini.model_ids,
            vec!["gemini-2.5-flash", "gemini-embedding-001"]
        );
        assert!(
            gemini
                .capabilities
                .iter()
                .any(|capability| capability == "generateContent")
        );
        assert!(
            gemini
                .capabilities
                .iter()
                .any(|capability| capability == "embedContent")
        );
        let flash_metadata = gemini.model_metadata.get("gemini-2.5-flash").unwrap();
        assert!(
            flash_metadata
                .capabilities
                .iter()
                .any(|capability| capability == "generateContent")
        );
        assert!(
            flash_metadata
                .modalities
                .iter()
                .any(|modality| modality == "text")
        );
        assert_eq!(flash_metadata.limits.get("input_tokens"), Some(&1_048_576));
        assert_eq!(flash_metadata.limits.get("output_tokens"), Some(&65_536));
        let embedding_metadata = gemini.model_metadata.get("gemini-embedding-001").unwrap();
        assert!(
            embedding_metadata
                .modalities
                .iter()
                .any(|modality| modality == "embedding")
        );
    }

    #[test]
    fn supported_model_providers_describe_native_and_local_capabilities() {
        let providers = supported_model_providers();
        let rig = providers
            .iter()
            .find(|provider| provider.id == "rig")
            .expect("rig provider descriptor");
        assert!(rig.available_modalities.iter().any(|item| item == "image"));
        assert!(rig.option_schema.iter().any(|option| {
            option.target == ModelProviderOptionTarget::ProviderOptions
                && option.key == "reasoning_effort"
                && option.allowed_values.iter().any(|value| value == "high")
        }));
        assert!(rig.option_schema.iter().any(|option| {
            option.target == ModelProviderOptionTarget::ProviderOptions
                && option.key == "top_p"
                && option.min == Some(0.0)
                && option.max == Some(1.0)
        }));

        let anthropic = providers
            .iter()
            .find(|provider| provider.id == "anthropic")
            .expect("anthropic provider descriptor");
        assert!(anthropic.native);
        assert_eq!(anthropic.api_key_env.as_deref(), Some("ANTHROPIC_API_KEY"));
        assert!(
            anthropic
                .available_modalities
                .iter()
                .any(|item| item == "image")
        );
        assert_eq!(anthropic.tool_support, Some(true));
        assert!(
            !anthropic
                .option_schema
                .iter()
                .any(|option| option.key == "reasoning_effort")
        );

        let ollama = providers
            .iter()
            .find(|provider| provider.id == "ollama")
            .expect("ollama provider descriptor");
        assert!(ollama.local);
        assert!(ollama.supports_api_base_url);
        assert_eq!(ollama.tool_support, None);
        assert!(ollama.option_schema.iter().any(|option| {
            option.target == ModelProviderOptionTarget::Runtime && option.key == "api_base_url"
        }));
    }

    #[test]
    fn profile_provider_catalog_extends_provider_descriptors_and_validation() {
        let dir = std::env::temp_dir().join(format!("agent-provider-catalog-test-{}", uuid_like()));
        let paths = StoragePaths::new(&dir);
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            paths.models_dir().join(MODEL_PROVIDER_CATALOG_FILE),
            r#"{
              "schema_version": 1,
              "source": "profile-provider-catalog-test",
              "providers": [
                {
                  "id": "custom-openai",
                  "name": "Custom OpenAI Compatible",
                  "default_model": "custom-default",
                  "api_key_env": "CUSTOM_API_KEY",
                  "api_base_url": "http://127.0.0.1:9999/v1",
                  "supports_api_base_url": true,
                  "local": false,
                  "native": false,
                  "available_modalities": ["text", "audio"],
                  "tool_support": true,
                  "reasoning_modes": ["model-default"],
                  "settings": ["api_base_url", "api_key_env", "provider_options"],
                  "option_schema": [
                    {
                      "key": "custom_flag",
                      "target": "provider_options",
                      "label": "Custom flag",
                      "kind": "boolean"
                    }
                  ],
                  "notes": "Loaded from profile provider catalog."
                }
              ]
            }"#,
        )
        .unwrap();

        let resolver = ConfigResolver::new(paths);
        let providers = resolver.model_provider_descriptors().unwrap();
        let custom = providers
            .iter()
            .find(|provider| provider.id == "custom-openai")
            .expect("custom provider descriptor");
        assert_eq!(custom.default_model, "custom-default");
        assert!(
            custom
                .available_modalities
                .iter()
                .any(|item| item == "audio")
        );
        assert!(custom.option_schema.iter().any(|option| {
            option.target == ModelProviderOptionTarget::ProviderOptions
                && option.key == "custom_flag"
        }));

        let mut model = ModelConfig::for_id("custom-model");
        model.provider = Some("custom-openai".into());
        model.api_base_url = Some("http://127.0.0.1:9999/v1".into());
        resolver.save_model(&model).unwrap();
        let support = resolver
            .model_modality_support("custom-model", "audio")
            .unwrap();
        assert!(support.supported);
        assert_eq!(support.source, "provider_descriptor:custom-openai");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn profile_registry_creates_lists_shows_and_deletes_non_main_profiles() {
        let dir = std::env::temp_dir().join(format!("agent-profile-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));

        let profile = resolver
            .create_profile("research", Some("Research Team".into()))
            .unwrap();
        assert_eq!(profile.id, "research");
        assert_eq!(profile.name, "Research Team");
        assert!(resolver.paths.profile_agents_dir("research").exists());
        assert!(resolver.paths.profile_config("research").exists());
        assert!(
            resolver
                .list_profiles()
                .unwrap()
                .iter()
                .any(|entry| entry.id == "main")
        );
        assert_eq!(resolver.show_profile("research").unwrap(), profile);
        assert!(matches!(
            resolver.delete_profile("main").unwrap_err(),
            ConfigError::InvalidInput(_)
        ));
        assert!(resolver.delete_profile("research").unwrap());
        assert!(matches!(
            resolver.show_profile("research").unwrap_err(),
            ConfigError::ProfileNotFound(_)
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn active_profile_resolves_profile_scoped_defaults() {
        let dir = std::env::temp_dir().join(format!("agent-active-profile-test-{}", uuid_like()));
        let paths = StoragePaths::new_with_profile(&dir, "research");
        let resolver = ConfigResolver::new(paths.clone());

        let resolved = resolver.resolve_default_agent().unwrap();

        assert_eq!(resolved.agent.id, "fake-agent");
        assert!(paths.main_profile_config().exists());
        assert!(paths.active_profile_config().exists());
        assert_eq!(
            paths.default_agent_config(),
            dir.join("profiles/research/agents/fake-agent/agent.toml")
        );
        let profile_id = resolved
            .values
            .iter()
            .find(|value| value.key == "profile.id")
            .unwrap();
        assert_eq!(profile_id.value, serde_json::json!("research"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn profile_grants_are_explicit_listable_and_revocable() {
        let dir = std::env::temp_dir().join(format!("agent-grant-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        resolver
            .create_profile("research", Some("Research".into()))
            .unwrap();

        let grant = resolver
            .grant_profile_access("main", "research", ProfileGrantKind::Memory, "fake-agent")
            .unwrap();
        assert_eq!(grant.from_profile, "main");
        assert_eq!(grant.to_profile, "research");
        assert_eq!(grant.kind, ProfileGrantKind::Memory);
        assert_eq!(grant.resource, "fake-agent");
        assert_eq!(
            resolver.list_profile_grants_from("main").unwrap(),
            vec![grant.clone()]
        );
        assert_eq!(resolver.list_profile_grants().unwrap(), vec![grant.clone()]);

        let revoked = resolver.revoke_profile_grant(&grant.id).unwrap();
        assert_eq!(revoked, grant);
        assert!(resolver.list_profile_grants().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn policy_layers_resolve_with_provenance() {
        let dir = std::env::temp_dir().join(format!("agent-config-layer-test-{}", uuid_like()));
        let paths = StoragePaths::new(&dir);
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            paths.global_config(),
            r#"
model = "global-model"
max_tool_calls = 2
tool_visibility = "name_only"
input_cost_per_million = 0.1
max_tokens_before_compaction = 128
compaction_guidance = "keep decisions"
"#,
        )
        .unwrap();
        std::fs::write(
            paths.main_profile_config(),
            r#"
id = "main"
name = "Main"
model = "profile-model"
tool_output_mode = "raw"
max_compaction_output_tokens = 64
"#,
        )
        .unwrap();
        std::fs::write(
            paths.default_agent_config(),
            r#"
id = "fake-agent"
name = "Fake Agent"
system_prompt = "You echo what the user says."
max_tool_calls = 4
allowed_tools = ["echo"]
"#,
        )
        .unwrap();

        let resolved = ConfigResolver::new(paths.clone())
            .resolve_default_agent()
            .unwrap();

        assert_eq!(resolved.agent.model.0, "profile-model");
        assert_eq!(resolved.agent.tool_policy.max_calls, 4);
        assert_eq!(resolved.agent.tool_policy.allowed_tools[0].0, "echo");
        assert_eq!(resolved.agent.tool_policy.output_mode, ToolOutputMode::Raw);
        assert_eq!(
            resolved.agent.tool_policy.visibility,
            VisibilityLevel::NameOnly
        );
        assert_eq!(resolved.agent.cost_policy.input_cost_per_million, Some(0.1));
        assert_eq!(
            resolved
                .agent
                .context_policy
                .compaction
                .max_tokens_before_compaction,
            Some(128)
        );
        assert_eq!(
            resolved.agent.context_policy.compaction.max_output_tokens,
            Some(64)
        );
        assert_eq!(
            resolved.agent.context_policy.compaction.guidance.as_deref(),
            Some("keep decisions")
        );

        assert_source_contains(&resolved.values, "agent.model.default", "profile:");
        assert_source_contains(&resolved.values, "agent.tool_policy.max_calls", "agent:");
        assert_source_contains(
            &resolved.values,
            "agent.tool_policy.output_mode",
            "profile:",
        );
        assert_source_contains(&resolved.values, "agent.tool_policy.visibility", "global:");
        assert_source_contains(
            &resolved.values,
            "agent.cost_policy.input_cost_per_million",
            "global:",
        );
        assert_source_contains(
            &resolved.values,
            "agent.context_policy.max_tokens_before_compaction",
            "global:",
        );
        assert_source_contains(
            &resolved.values,
            "agent.context_policy.max_compaction_output_tokens",
            "profile:",
        );
        assert_source_contains(
            &resolved.values,
            "agent.context_policy.compaction_guidance",
            "global:",
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn disabled_lifecycle_hooks_persist_and_resolve_by_layer() {
        let dir = std::env::temp_dir().join(format!("agent-hook-policy-test-{}", uuid_like()));
        let paths = StoragePaths::new(&dir);
        let resolver = ConfigResolver::new(paths.clone());
        resolver.ensure_default_files().unwrap();
        let agent = AgentConfigFile {
            id: "critic".into(),
            name: "Critic".into(),
            system_prompt: "Review carefully.".into(),
            disabled_lifecycle_hooks: Some(vec!["agent-hook".into()]),
            ..AgentConfigFile::default()
        };
        resolver.save_agent_config(&agent).unwrap();

        assert_eq!(
            resolver
                .disabled_lifecycle_hooks_for_agent("critic")
                .unwrap(),
            vec!["agent-hook".to_string()]
        );
        assert_eq!(
            resolver.agent_disabled_lifecycle_hooks("critic").unwrap(),
            vec!["agent-hook".to_string()]
        );
        let layers = resolver
            .lifecycle_hook_policy_layers_for_agent("critic")
            .unwrap();
        assert_eq!(layers.effective_source, "agent");
        assert_eq!(
            layers.effective_disabled_lifecycle_hooks,
            vec!["agent-hook".to_string()]
        );

        let profile_hooks = resolver
            .set_profile_lifecycle_hook_disabled("profile-hook", true)
            .unwrap();
        assert_eq!(profile_hooks, vec!["profile-hook".to_string()]);
        assert_eq!(
            resolver.profile_disabled_lifecycle_hooks().unwrap(),
            vec!["profile-hook".to_string()]
        );
        assert_eq!(
            resolver
                .disabled_lifecycle_hooks_for_agent("fake-agent")
                .unwrap(),
            vec!["profile-hook".to_string()]
        );
        let fake_layers = resolver
            .lifecycle_hook_policy_layers_for_agent("fake-agent")
            .unwrap();
        assert_eq!(fake_layers.effective_source, "profile");
        assert_eq!(
            fake_layers.profile_disabled_lifecycle_hooks,
            vec!["profile-hook".to_string()]
        );
        assert_eq!(
            resolver
                .disabled_lifecycle_hooks_for_agent("critic")
                .unwrap(),
            vec!["agent-hook".to_string()]
        );
        let agent_hooks = resolver
            .set_agent_lifecycle_hook_disabled("critic", "second-agent-hook", true)
            .unwrap();
        assert_eq!(
            agent_hooks,
            vec!["agent-hook".to_string(), "second-agent-hook".to_string()]
        );
        assert_eq!(
            resolver
                .disabled_lifecycle_hooks_for_agent("critic")
                .unwrap(),
            vec!["agent-hook".to_string(), "second-agent-hook".to_string()]
        );
        let agent_hooks = resolver
            .set_agent_lifecycle_hook_disabled("critic", "agent-hook", false)
            .unwrap();
        assert_eq!(agent_hooks, vec!["second-agent-hook".to_string()]);

        let profile_hooks = resolver
            .set_profile_lifecycle_hook_disabled("profile-hook", false)
            .unwrap();
        assert!(profile_hooks.is_empty());
        assert!(
            !std::fs::read_to_string(paths.active_profile_config())
                .unwrap()
                .contains("disabled_lifecycle_hooks")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn voice_config_layers_resolve_with_provenance() {
        let dir = std::env::temp_dir().join(format!("agent-voice-layer-test-{}", uuid_like()));
        let paths = StoragePaths::new(&dir);
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            paths.global_config(),
            r#"
[voice]
output_enabled = true
output_backend = "cloud"
tts_provider = "openai"
tts_model = "tts-1"
voice = "alloy"
tone = "calm"
"#,
        )
        .unwrap();
        std::fs::write(
            paths.main_profile_config(),
            r#"
id = "main"
name = "Main"

[voice]
input_enabled = true
input_backend = "local"
input_provider = "whisper.cpp"
input_model = "ggml-base"
tone = "warm"
"#,
        )
        .unwrap();
        std::fs::write(
            paths.default_agent_config(),
            r#"
id = "fake-agent"
name = "Fake Agent"
system_prompt = "You echo what the user says."

[voice]
voice = "nova"
"#,
        )
        .unwrap();

        let resolved = ConfigResolver::new(paths.clone())
            .resolve_default_agent()
            .unwrap();

        assert!(resolved.agent.voice.input_enabled);
        assert!(resolved.agent.voice.output_enabled);
        assert_eq!(resolved.agent.voice.input_backend.as_deref(), Some("local"));
        assert_eq!(
            resolved.agent.voice.input_provider.as_deref(),
            Some("whisper.cpp")
        );
        assert_eq!(
            resolved.agent.voice.input_model.as_deref(),
            Some("ggml-base")
        );
        assert_eq!(
            resolved.agent.voice.output_backend.as_deref(),
            Some("cloud")
        );
        assert_eq!(resolved.agent.voice.tts_provider.as_deref(), Some("openai"));
        assert_eq!(resolved.agent.voice.tts_model.as_deref(), Some("tts-1"));
        assert_eq!(resolved.agent.voice.voice.as_deref(), Some("nova"));
        assert_eq!(resolved.agent.voice.tone.as_deref(), Some("warm"));

        assert_source_contains(&resolved.values, "agent.voice.input_enabled", "profile:");
        assert_source_contains(&resolved.values, "agent.voice.output_enabled", "global:");
        assert_source_contains(&resolved.values, "agent.voice.output_backend", "global:");
        assert_source_contains(&resolved.values, "agent.voice.voice", "agent:");
        assert_source_contains(&resolved.values, "agent.voice.tone", "profile:");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn voice_config_rejects_invalid_backend() {
        let dir = std::env::temp_dir().join(format!("agent-voice-invalid-test-{}", uuid_like()));
        let resolver = ConfigResolver::new(StoragePaths::new(&dir));
        let agent = AgentConfigFile {
            voice: Some(VoiceConfigFile {
                output_backend: Some("remote".into()),
                ..VoiceConfigFile::default()
            }),
            ..AgentConfigFile::default()
        };

        let err = resolver.save_agent_config(&agent).unwrap_err();

        assert!(
            err.to_string()
                .contains("voice.output_backend must be local or cloud")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn assert_source_contains(values: &[ConfigValueExplanation], key: &str, expected: &str) {
        let value = values
            .iter()
            .find(|value| value.key == key)
            .unwrap_or_else(|| panic!("missing config value {key}"));
        assert!(
            value.source.contains(expected),
            "expected {key} source {:?} to contain {expected:?}",
            value.source
        );
    }

    fn uuid_like() -> String {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        format!(
            "{}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
}
