//! `agent-storage` — filesystem layout helpers.
//!
//! Stage 2 keeps this deliberately small: one main profile, a config root,
//! cache directory, and `state.sqlite` path. The shape matches
//! `specs/architecture.md` §14 so later config/storage work has a stable home.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct StoragePaths {
    root: PathBuf,
}

impl StoragePaths {
    pub fn default_root() -> PathBuf {
        if let Some(path) = std::env::var_os("AGENT_HARNESS_HOME") {
            return PathBuf::from(path);
        }
        if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(path).join("agent-harness");
        }
        if let Some(path) = std::env::var_os("HOME") {
            return PathBuf::from(path).join(".config").join("agent-harness");
        }
        PathBuf::from(".agent-harness")
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_env() -> Self {
        Self::new(Self::default_root())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn global_config(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn main_profile_dir(&self) -> PathBuf {
        self.profiles_dir().join("main")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.main_profile_dir().join("agents")
    }

    pub fn default_agent_dir(&self) -> PathBuf {
        self.agents_dir().join("fake-agent")
    }

    pub fn default_agent_config(&self) -> PathBuf {
        self.default_agent_dir().join("agent.toml")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.main_profile_dir().join("skills")
    }

    pub fn adapters_dir(&self) -> PathBuf {
        self.main_profile_dir().join("adapters")
    }

    pub fn memory_file(&self) -> PathBuf {
        self.default_agent_dir().join("memory.md")
    }

    pub fn user_memory_file(&self) -> PathBuf {
        self.default_agent_dir().join("user.md")
    }

    pub fn memory_backup_dir(&self) -> PathBuf {
        self.default_agent_dir().join(".bak")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn ingestion_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("ingestion")
    }

    pub fn state_db(&self) -> PathBuf {
        self.root.join("state.sqlite")
    }

    pub fn ensure_base_dirs(&self) -> Result<(), StorageError> {
        std::fs::create_dir_all(self.default_agent_dir())?;
        std::fs::create_dir_all(self.skills_dir())?;
        std::fs::create_dir_all(self.adapters_dir())?;
        std::fs::create_dir_all(self.memory_backup_dir())?;
        std::fs::create_dir_all(self.ingestion_cache_dir())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_match_spec_layout() {
        let paths = StoragePaths::new("/tmp/harness");
        assert_eq!(
            paths.global_config(),
            PathBuf::from("/tmp/harness/config.toml")
        );
        assert_eq!(
            paths.default_agent_config(),
            PathBuf::from("/tmp/harness/profiles/main/agents/fake-agent/agent.toml")
        );
        assert_eq!(paths.state_db(), PathBuf::from("/tmp/harness/state.sqlite"));
    }
}
