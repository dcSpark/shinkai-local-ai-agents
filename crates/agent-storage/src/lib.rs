//! `agent-storage` — filesystem layout helpers.
//!
//! Stage 2 keeps this deliberately small: one main profile, a config root,
//! cache directory, and `state.sqlite` path. The shape matches
//! `specs/architecture.md` §14 so later config/storage work has a stable home.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageBucket {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub directories: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub largest_file: Option<PathBuf>,
    #[serde(default)]
    pub largest_file_bytes: u64,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageReport {
    pub root: PathBuf,
    pub total_bytes: u64,
    pub total_files: u64,
    pub total_directories: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub largest_file: Option<PathBuf>,
    #[serde(default)]
    pub largest_file_bytes: u64,
    pub buckets: Vec<StorageBucket>,
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

    pub fn main_profile_config(&self) -> PathBuf {
        self.main_profile_dir().join("profile.toml")
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

    pub fn models_dir(&self) -> PathBuf {
        self.main_profile_dir().join("models")
    }

    pub fn model_config(&self, model: &str) -> PathBuf {
        self.models_dir().join(format!("{model}.toml"))
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.main_profile_dir().join("prompts")
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

    pub fn batches_dir(&self) -> PathBuf {
        self.cache_dir().join("batches")
    }

    pub fn state_db(&self) -> PathBuf {
        self.root.join("state.sqlite")
    }

    pub fn ensure_base_dirs(&self) -> Result<(), StorageError> {
        std::fs::create_dir_all(self.default_agent_dir())?;
        std::fs::create_dir_all(self.skills_dir())?;
        std::fs::create_dir_all(self.adapters_dir())?;
        std::fs::create_dir_all(self.models_dir())?;
        std::fs::create_dir_all(self.prompts_dir())?;
        std::fs::create_dir_all(self.memory_backup_dir())?;
        std::fs::create_dir_all(self.ingestion_cache_dir())?;
        std::fs::create_dir_all(self.batches_dir())?;
        Ok(())
    }

    pub fn storage_report(&self) -> Result<StorageReport, StorageError> {
        let buckets = vec![
            self.storage_bucket("config", self.global_config())?,
            self.storage_bucket("profiles", self.profiles_dir())?,
            self.storage_bucket("cache", self.cache_dir())?,
            self.storage_bucket("state", self.state_db())?,
        ];
        let total = stats_path(&self.root)?;

        Ok(StorageReport {
            root: self.root.clone(),
            total_bytes: total.bytes,
            total_files: total.files,
            total_directories: total.directories,
            largest_file: total.largest_file,
            largest_file_bytes: total.largest_file_bytes,
            buckets,
        })
    }

    fn storage_bucket(
        &self,
        name: impl Into<String>,
        path: PathBuf,
    ) -> Result<StorageBucket, StorageError> {
        let stats = stats_path(&path)?;
        Ok(StorageBucket {
            name: name.into(),
            exists: path.exists(),
            bytes: stats.bytes,
            files: stats.files,
            directories: stats.directories,
            largest_file: stats.largest_file,
            largest_file_bytes: stats.largest_file_bytes,
            path,
        })
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct StorageStats {
    bytes: u64,
    files: u64,
    directories: u64,
    largest_file: Option<PathBuf>,
    largest_file_bytes: u64,
}

fn stats_path(path: &Path) -> Result<StorageStats, StorageError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StorageStats::default());
        }
        Err(err) => return Err(err.into()),
    };

    if metadata.is_file() {
        return Ok(StorageStats {
            bytes: metadata.len(),
            files: 1,
            directories: 0,
            largest_file: Some(path.to_path_buf()),
            largest_file_bytes: metadata.len(),
        });
    }
    if !metadata.is_dir() {
        return Ok(StorageStats::default());
    }

    let mut total = StorageStats {
        bytes: 0,
        files: 0,
        directories: 1,
        largest_file: None,
        largest_file_bytes: 0,
    };
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let child = stats_path(&entry.path())?;
        total.bytes += child.bytes;
        total.files += child.files;
        total.directories += child.directories;
        if child.largest_file_bytes > total.largest_file_bytes {
            total.largest_file = child.largest_file;
            total.largest_file_bytes = child.largest_file_bytes;
        }
    }
    Ok(total)
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
        assert_eq!(
            paths.main_profile_config(),
            PathBuf::from("/tmp/harness/profiles/main/profile.toml")
        );
        assert_eq!(
            paths.model_config("fake-model"),
            PathBuf::from("/tmp/harness/profiles/main/models/fake-model.toml")
        );
        assert_eq!(paths.state_db(), PathBuf::from("/tmp/harness/state.sqlite"));
    }

    #[test]
    fn storage_report_sums_known_buckets() {
        let root = std::env::temp_dir().join(format!(
            "agent-storage-report-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = StoragePaths::new(&root);
        paths.ensure_base_dirs().unwrap();
        std::fs::write(paths.global_config(), b"abc").unwrap();
        std::fs::write(paths.memory_file(), b"memory!").unwrap();
        std::fs::write(paths.ingestion_cache_dir().join("artifact.txt"), b"ingest").unwrap();
        std::fs::write(paths.state_db(), b"sqlite").unwrap();

        let report = paths.storage_report().unwrap();

        assert_eq!(report.root, root);
        assert_eq!(report.total_bytes, 22);
        assert_eq!(report.total_files, 4);
        assert_eq!(report.total_directories, 13);
        assert_eq!(report.largest_file, Some(paths.memory_file()));
        assert_eq!(report.largest_file_bytes, 7);
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "config")
                .unwrap()
                .bytes,
            3
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "config")
                .unwrap()
                .files,
            1
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "profiles")
                .unwrap()
                .bytes,
            7
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "profiles")
                .unwrap()
                .files,
            1
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "profiles")
                .unwrap()
                .largest_file,
            Some(paths.memory_file())
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "profiles")
                .unwrap()
                .largest_file_bytes,
            7
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "cache")
                .unwrap()
                .bytes,
            6
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "cache")
                .unwrap()
                .files,
            1
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "state")
                .unwrap()
                .bytes,
            6
        );
        assert_eq!(
            report
                .buckets
                .iter()
                .find(|bucket| bucket.name == "state")
                .unwrap()
                .files,
            1
        );

        std::fs::remove_dir_all(report.root).unwrap();
    }
}
