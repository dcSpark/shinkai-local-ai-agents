//! `agent-config` — v0 layered config resolver.
//!
//! Stage 2 starts with a single main-profile agent config file while preserving
//! provenance strings for `explain-config`. Later layers can slot into the same
//! returned shape without changing callers.

use std::path::PathBuf;

use agent_core::{AgentConfig, ConfigValueExplanation, ToolPolicy};
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
        }
    }
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
        let agent_path = self.paths.default_agent_config();
        if !agent_path.exists() {
            let text = toml::to_string_pretty(&AgentToml::default())?;
            std::fs::write(agent_path, text)?;
        }
        Ok(())
    }

    pub fn resolve_default_agent(&self) -> Result<ResolvedAgentConfig, ConfigError> {
        self.ensure_default_files()?;
        let path = self.paths.default_agent_config();
        let text = std::fs::read_to_string(&path)?;
        let parsed: AgentToml = toml::from_str(&text)?;
        Ok(resolve_agent(parsed, path))
    }
}

fn resolve_agent(parsed: AgentToml, path: PathBuf) -> ResolvedAgentConfig {
    let source = format!("agent:{}", path.display());
    let allowed_tools: Vec<ToolId> = parsed
        .allowed_tools
        .iter()
        .map(|id| ToolId::from(id.clone()))
        .collect();
    let agent = AgentConfig {
        id: parsed.id.clone(),
        name: parsed.name.clone(),
        system_prompt: parsed.system_prompt.clone(),
        model: ModelRef::from(parsed.model.clone()),
        tool_policy: ToolPolicy {
            max_calls: parsed.max_tool_calls,
            allowed_tools,
            visibility: ToolPolicy::default().visibility,
            approval_mode: ToolPolicy::default().approval_mode,
        },
        memory_fragments: Vec::new(),
        ingestion_artifacts: Vec::new(),
        skill_views: Vec::new(),
    };

    ResolvedAgentConfig {
        agent,
        values: vec![
            config_value("agent.id", parsed.id, &source),
            config_value("agent.name", parsed.name, &source),
            config_value("agent.model.default", parsed.model, &source),
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
        ],
    }
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
