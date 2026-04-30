//! `agent-config` — v0 layered config resolver.
//!
//! Stage 2 starts with a single main-profile agent config file while preserving
//! provenance strings for `explain-config`. Later layers can slot into the same
//! returned shape without changing callers.

use std::path::PathBuf;

use agent_core::{
    AgentConfig, ConfigValueExplanation, CostPolicy, ToolOutputMode, ToolPolicy, VisibilityLevel,
};
use agent_llm::ModelRef;
use agent_storage::StoragePaths;
use agent_tools::ToolId;
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone)]
pub struct ResolvedAgentConfig {
    pub agent: AgentConfig,
    pub values: Vec<ConfigValueExplanation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentToml {
    id: String,
    name: String,
    system_prompt: String,
    model: String,
    max_tool_calls: u32,
    #[serde(default)]
    allowed_tools: Vec<String>,
    #[serde(default = "default_tool_output_mode")]
    tool_output_mode: ToolOutputMode,
    #[serde(default = "default_tool_visibility")]
    tool_visibility: VisibilityLevel,
    #[serde(default)]
    input_cost_per_million: Option<f64>,
    #[serde(default)]
    output_cost_per_million: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfileToml {
    id: String,
    name: String,
}

impl Default for ProfileToml {
    fn default() -> Self {
        Self {
            id: "main".into(),
            name: "Main".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelToml {
    id: String,
    #[serde(default)]
    max_context_tokens: Option<u64>,
    #[serde(default)]
    max_output_tokens: Option<u64>,
    #[serde(default)]
    default_temperature: Option<f64>,
    #[serde(default)]
    tool_support: Option<bool>,
    #[serde(default)]
    privacy_level: Option<String>,
    #[serde(default)]
    cost_tier: Option<String>,
    #[serde(default)]
    input_cost_per_million: Option<f64>,
    #[serde(default)]
    output_cost_per_million: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelRuntimeConfig {
    pub max_output_tokens: Option<u64>,
    pub default_temperature: Option<f64>,
}

impl ModelToml {
    fn for_id(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            max_context_tokens: None,
            max_output_tokens: None,
            default_temperature: None,
            tool_support: None,
            privacy_level: None,
            cost_tier: None,
            input_cost_per_million: None,
            output_cost_per_million: None,
        }
    }
}

impl Default for AgentToml {
    fn default() -> Self {
        Self {
            id: "fake-agent".into(),
            name: "Fake Agent".into(),
            system_prompt: "You echo what the user says.".into(),
            model: "fake-model".into(),
            max_tool_calls: 5,
            allowed_tools: Vec::new(),
            tool_output_mode: default_tool_output_mode(),
            tool_visibility: default_tool_visibility(),
            input_cost_per_million: None,
            output_cost_per_million: None,
        }
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
        let profile_path = self.paths.main_profile_config();
        if !profile_path.exists() {
            let text = toml::to_string_pretty(&ProfileToml::default())?;
            std::fs::write(profile_path, text)?;
        }
        let agent_path = self.paths.default_agent_config();
        if !agent_path.exists() {
            let text = toml::to_string_pretty(&AgentToml::default())?;
            std::fs::write(agent_path, text)?;
        }
        let model_path = self.paths.model_config("fake-model");
        if !model_path.exists() {
            let text = toml::to_string_pretty(&ModelToml::for_id("fake-model"))?;
            std::fs::write(model_path, text)?;
        }
        Ok(())
    }

    pub fn resolve_default_agent(&self) -> Result<ResolvedAgentConfig, ConfigError> {
        self.ensure_default_files()?;
        let path = self.paths.default_agent_config();
        let text = std::fs::read_to_string(&path)?;
        let parsed: AgentToml = toml::from_str(&text)?;
        let model_path = self.paths.model_config(&parsed.model);
        let model = if model_path.exists() {
            Some(toml::from_str(&std::fs::read_to_string(&model_path)?)?)
        } else {
            None
        };
        Ok(resolve_agent(parsed, path, model, model_path))
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
        let model: ModelToml = toml::from_str(&std::fs::read_to_string(model_path)?)?;
        Ok(Some(ModelRuntimeConfig {
            max_output_tokens: model.max_output_tokens,
            default_temperature: model.default_temperature,
        }))
    }
}

fn resolve_agent(
    parsed: AgentToml,
    path: PathBuf,
    model_config: Option<ModelToml>,
    model_path: PathBuf,
) -> ResolvedAgentConfig {
    let source = format!("agent:{}", path.display());
    let model_source = format!("model:{}", model_path.display());
    let allowed_tools: Vec<ToolId> = parsed
        .allowed_tools
        .iter()
        .map(|id| ToolId::from(id.clone()))
        .collect();
    let input_cost_per_million = parsed.input_cost_per_million.or_else(|| {
        model_config
            .as_ref()
            .and_then(|model| model.input_cost_per_million)
    });
    let output_cost_per_million = parsed.output_cost_per_million.or_else(|| {
        model_config
            .as_ref()
            .and_then(|model| model.output_cost_per_million)
    });
    let agent = AgentConfig {
        id: parsed.id.clone(),
        name: parsed.name.clone(),
        system_prompt: parsed.system_prompt.clone(),
        model: ModelRef::from(parsed.model.clone()),
        tool_policy: ToolPolicy {
            max_calls: parsed.max_tool_calls,
            allowed_tools,
            visibility: parsed.tool_visibility,
            approval_mode: ToolPolicy::default().approval_mode,
            output_mode: parsed.tool_output_mode,
        },
        cost_policy: CostPolicy {
            input_cost_per_million,
            output_cost_per_million,
        },
        memory_fragments: Vec::new(),
        ingestion_artifacts: Vec::new(),
        skill_views: Vec::new(),
    };

    let mut values = vec![
        config_value("agent.id", parsed.id, &source),
        config_value("agent.name", parsed.name, &source),
        config_value("agent.model.default", parsed.model.clone(), &source),
        config_value(
            "agent.tool_policy.max_calls",
            parsed.max_tool_calls,
            &source,
        ),
        config_value(
            "agent.tool_policy.allowed_tools",
            parsed.allowed_tools,
            &source,
        ),
        config_value(
            "agent.tool_policy.output_mode",
            parsed.tool_output_mode,
            &source,
        ),
        config_value(
            "agent.tool_policy.visibility",
            parsed.tool_visibility,
            &source,
        ),
        config_value(
            "agent.cost_policy.input_cost_per_million",
            input_cost_per_million,
            if parsed.input_cost_per_million.is_some() {
                &source
            } else {
                &model_source
            },
        ),
        config_value(
            "agent.cost_policy.output_cost_per_million",
            output_cost_per_million,
            if parsed.output_cost_per_million.is_some() {
                &source
            } else {
                &model_source
            },
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
            config_value("model.privacy_level", model.privacy_level, &model_source),
            config_value("model.cost_tier", model.cost_tier, &model_source),
        ]);
    }

    ResolvedAgentConfig { agent, values }
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

    fn uuid_like() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
}
