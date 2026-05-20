//! `agent-storage` — filesystem layout helpers.
//!
//! Stage 2 keeps this deliberately small: one main profile, a config root,
//! cache directory, and `state.sqlite` path. The shape matches
//! `specs/architecture.md` §14 so later config/storage work has a stable home.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid storage quota in {var}: {value}")]
    InvalidQuota { var: String, value: String },
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_remaining_bytes: Option<i64>,
    #[serde(default)]
    pub quota_exceeded: bool,
    pub buckets: Vec<StorageBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageRetentionCandidate {
    pub bucket: String,
    pub path: PathBuf,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_unix_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageRetentionPlan {
    pub root: PathBuf,
    pub retention_days: u64,
    pub cutoff_unix_seconds: u64,
    pub total_bytes: u64,
    pub total_files: u64,
    pub candidates: Vec<StorageRetentionCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageRetentionResult {
    pub dry_run: bool,
    pub plan: StorageRetentionPlan,
    pub deleted_files: u64,
    pub deleted_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StoragePaths {
    root: PathBuf,
    profile_id: String,
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
        Self::new_with_profile(root, "main")
    }

    pub fn new_with_profile(root: impl Into<PathBuf>, profile_id: impl Into<String>) -> Self {
        let profile_id = clean_profile_id(profile_id.into()).unwrap_or_else(|| "main".into());
        Self {
            root: root.into(),
            profile_id,
        }
    }

    pub fn from_env() -> Self {
        let profile_id = std::env::var("AGENT_HARNESS_PROFILE")
            .ok()
            .and_then(clean_profile_id)
            .unwrap_or_else(|| "main".into());
        Self::new_with_profile(Self::default_root(), profile_id)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn active_profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn global_config(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    pub fn profile_dir(&self, profile_id: &str) -> PathBuf {
        self.profiles_dir().join(profile_id)
    }

    pub fn profile_config(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("profile.toml")
    }

    pub fn main_profile_dir(&self) -> PathBuf {
        self.profile_dir("main")
    }

    pub fn main_profile_config(&self) -> PathBuf {
        self.profile_config("main")
    }

    pub fn active_profile_dir(&self) -> PathBuf {
        self.profile_dir(&self.profile_id)
    }

    pub fn active_profile_config(&self) -> PathBuf {
        self.profile_config(&self.profile_id)
    }

    pub fn profile_agents_dir(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("agents")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.profile_agents_dir(&self.profile_id)
    }

    pub fn default_agent_dir(&self) -> PathBuf {
        self.agents_dir().join("fake-agent")
    }

    pub fn default_agent_config(&self) -> PathBuf {
        self.default_agent_dir().join("agent.toml")
    }

    pub fn profile_agent_config(&self, profile_id: &str, agent_id: &str) -> PathBuf {
        self.profile_agents_dir(profile_id)
            .join(agent_id)
            .join("agent.toml")
    }

    pub fn agent_config(&self, agent_id: &str) -> PathBuf {
        self.profile_agent_config(&self.profile_id, agent_id)
    }

    pub fn profile_agent_prompts_dir(&self, profile_id: &str, agent_id: &str) -> PathBuf {
        self.profile_agents_dir(profile_id)
            .join(agent_id)
            .join("prompts")
    }

    pub fn agent_prompts_dir(&self, agent_id: &str) -> PathBuf {
        self.profile_agent_prompts_dir(&self.profile_id, agent_id)
    }

    pub fn profile_skills_dir(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("skills")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.profile_skills_dir(&self.profile_id)
    }

    pub fn profile_adapters_dir(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("adapters")
    }

    pub fn adapters_dir(&self) -> PathBuf {
        self.profile_adapters_dir(&self.profile_id)
    }

    pub fn profile_capability_drafts_dir(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("capability-drafts")
    }

    pub fn capability_drafts_dir(&self) -> PathBuf {
        self.profile_capability_drafts_dir(&self.profile_id)
    }

    pub fn profile_models_dir(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("models")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.profile_models_dir(&self.profile_id)
    }

    pub fn model_config(&self, model: &str) -> PathBuf {
        self.models_dir().join(format!("{model}.toml"))
    }

    pub fn profile_prompts_dir(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("prompts")
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.profile_prompts_dir(&self.profile_id)
    }

    pub fn profile_conversations_dir(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("conversations")
    }

    pub fn conversations_dir(&self) -> PathBuf {
        self.profile_conversations_dir(&self.profile_id)
    }

    pub fn profile_grants_file(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("grants.json")
    }

    pub fn grants_file(&self) -> PathBuf {
        self.profile_grants_file(&self.profile_id)
    }

    pub fn profile_secrets_file(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id).join("secrets.json")
    }

    pub fn secrets_file(&self) -> PathBuf {
        self.profile_secrets_file(&self.profile_id)
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

    pub fn compactions_dir(&self) -> PathBuf {
        self.cache_dir().join("compactions")
    }

    pub fn bridge_deliveries_dir(&self) -> PathBuf {
        self.cache_dir().join("bridge-deliveries")
    }

    pub fn artifacts_dir(&self) -> PathBuf {
        self.cache_dir().join("artifacts")
    }

    pub fn state_db(&self) -> PathBuf {
        self.root.join("state.sqlite")
    }

    pub fn ensure_profile_dirs(&self, profile_id: &str) -> Result<(), StorageError> {
        std::fs::create_dir_all(self.profile_agents_dir(profile_id))?;
        std::fs::create_dir_all(self.profile_skills_dir(profile_id))?;
        std::fs::create_dir_all(self.profile_adapters_dir(profile_id))?;
        std::fs::create_dir_all(self.profile_capability_drafts_dir(profile_id))?;
        std::fs::create_dir_all(self.profile_models_dir(profile_id))?;
        std::fs::create_dir_all(self.profile_prompts_dir(profile_id))?;
        std::fs::create_dir_all(self.profile_conversations_dir(profile_id))?;
        Ok(())
    }

    pub fn ensure_base_dirs(&self) -> Result<(), StorageError> {
        self.ensure_profile_dirs(&self.profile_id)?;
        std::fs::create_dir_all(self.default_agent_dir())?;
        std::fs::create_dir_all(self.memory_backup_dir())?;
        std::fs::create_dir_all(self.ingestion_cache_dir())?;
        std::fs::create_dir_all(self.batches_dir())?;
        std::fs::create_dir_all(self.compactions_dir())?;
        std::fs::create_dir_all(self.bridge_deliveries_dir())?;
        std::fs::create_dir_all(self.artifacts_dir())?;
        Ok(())
    }

    pub fn storage_report(&self) -> Result<StorageReport, StorageError> {
        self.storage_report_with_quota(storage_quota_bytes_from_env()?)
    }

    pub fn storage_report_with_quota(
        &self,
        quota_bytes: Option<u64>,
    ) -> Result<StorageReport, StorageError> {
        let buckets = vec![
            self.storage_bucket("config", self.global_config())?,
            self.storage_bucket("profiles", self.profiles_dir())?,
            self.storage_bucket("cache", self.cache_dir())?,
            self.storage_bucket("state", self.state_db())?,
        ];
        let total = stats_path(&self.root)?;
        let quota_remaining_bytes =
            quota_bytes.map(|quota| quota_remaining_bytes(quota, total.bytes));

        Ok(StorageReport {
            root: self.root.clone(),
            total_bytes: total.bytes,
            total_files: total.files,
            total_directories: total.directories,
            largest_file: total.largest_file,
            largest_file_bytes: total.largest_file_bytes,
            quota_bytes,
            quota_remaining_bytes,
            quota_exceeded: quota_bytes.is_some_and(|quota| total.bytes > quota),
            buckets,
        })
    }

    pub fn cache_retention_plan(
        &self,
        retention_days: u64,
    ) -> Result<StorageRetentionPlan, StorageError> {
        let cutoff = retention_cutoff(retention_days);
        let mut candidates = Vec::new();
        collect_retention_candidates(
            &self.cache_dir(),
            &self.cache_dir(),
            cutoff,
            &mut candidates,
        )?;
        candidates.sort_by(|a, b| {
            a.modified_unix_seconds
                .cmp(&b.modified_unix_seconds)
                .then_with(|| a.path.cmp(&b.path))
        });
        let total_bytes = candidates.iter().map(|candidate| candidate.bytes).sum();
        let total_files = candidates.len().min(u64::MAX as usize) as u64;
        Ok(StorageRetentionPlan {
            root: self.root.clone(),
            retention_days,
            cutoff_unix_seconds: unix_seconds(cutoff),
            total_bytes,
            total_files,
            candidates,
        })
    }

    pub fn prune_cache_retention(
        &self,
        retention_days: u64,
        dry_run: bool,
    ) -> Result<StorageRetentionResult, StorageError> {
        let plan = self.cache_retention_plan(retention_days)?;
        let mut deleted_files = 0;
        let mut deleted_bytes = 0;
        let mut errors = Vec::new();
        if !dry_run {
            for candidate in &plan.candidates {
                match std::fs::remove_file(&candidate.path) {
                    Ok(()) => {
                        deleted_files += 1;
                        deleted_bytes += candidate.bytes;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => errors.push(format!("{}: {err}", candidate.path.display())),
                }
            }
        }
        Ok(StorageRetentionResult {
            dry_run,
            plan,
            deleted_files,
            deleted_bytes,
            errors,
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

fn clean_profile_id(id: String) -> Option<String> {
    let id = id.trim().to_string();
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    valid.then_some(id)
}

fn storage_quota_bytes_from_env() -> Result<Option<u64>, StorageError> {
    const VAR: &str = "AGENT_STORAGE_QUOTA_BYTES";
    let Some(value) = std::env::var_os(VAR) else {
        return Ok(None);
    };
    let value = value.to_string_lossy().trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<u64>()
        .map(Some)
        .map_err(|_| StorageError::InvalidQuota {
            var: VAR.into(),
            value,
        })
}

fn quota_remaining_bytes(quota: u64, used: u64) -> i64 {
    if quota >= used {
        let remaining = quota - used;
        remaining.min(i64::MAX as u64) as i64
    } else {
        let over = used - quota;
        -((over.min(i64::MAX as u64)) as i64)
    }
}

fn retention_cutoff(retention_days: u64) -> SystemTime {
    let seconds = retention_days.saturating_mul(24 * 60 * 60);
    SystemTime::now()
        .checked_sub(Duration::from_secs(seconds))
        .unwrap_or(UNIX_EPOCH)
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn collect_retention_candidates(
    root: &Path,
    path: &Path,
    cutoff: SystemTime,
    candidates: &mut Vec<StorageRetentionCandidate>,
) -> Result<(), StorageError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    if metadata.is_file() {
        let modified = metadata.modified().ok();
        if let Some(modified) = modified
            && modified <= cutoff
        {
            candidates.push(StorageRetentionCandidate {
                bucket: retention_bucket(root, path),
                path: path.to_path_buf(),
                bytes: metadata.len(),
                modified_unix_seconds: Some(unix_seconds(modified)),
            });
        }
        return Ok(());
    }

    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            collect_retention_candidates(root, &entry?.path(), cutoff, candidates)?;
        }
    }

    Ok(())
}

fn retention_bucket(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|component| component.as_os_str().to_str())
        .map(ToOwned::to_owned)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "cache".into())
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
        assert_eq!(paths.active_profile_id(), "main");
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
            paths.profile_config("research"),
            PathBuf::from("/tmp/harness/profiles/research/profile.toml")
        );
        assert_eq!(
            paths.profile_conversations_dir("research"),
            PathBuf::from("/tmp/harness/profiles/research/conversations")
        );
        assert_eq!(
            paths.capability_drafts_dir(),
            PathBuf::from("/tmp/harness/profiles/main/capability-drafts")
        );
        assert_eq!(
            paths.profile_grants_file("research"),
            PathBuf::from("/tmp/harness/profiles/research/grants.json")
        );
        assert_eq!(
            paths.secrets_file(),
            PathBuf::from("/tmp/harness/profiles/main/secrets.json")
        );
        assert_eq!(
            paths.model_config("fake-model"),
            PathBuf::from("/tmp/harness/profiles/main/models/fake-model.toml")
        );
        assert_eq!(
            paths.conversations_dir(),
            PathBuf::from("/tmp/harness/profiles/main/conversations")
        );
        assert_eq!(
            paths.compactions_dir(),
            PathBuf::from("/tmp/harness/cache/compactions")
        );
        assert_eq!(
            paths.bridge_deliveries_dir(),
            PathBuf::from("/tmp/harness/cache/bridge-deliveries")
        );
        assert_eq!(
            paths.artifacts_dir(),
            PathBuf::from("/tmp/harness/cache/artifacts")
        );
        assert_eq!(paths.state_db(), PathBuf::from("/tmp/harness/state.sqlite"));
    }

    #[test]
    fn active_profile_retargets_profile_scoped_paths() {
        let paths = StoragePaths::new_with_profile("/tmp/harness", "research");
        assert_eq!(paths.active_profile_id(), "research");
        assert_eq!(
            paths.default_agent_config(),
            PathBuf::from("/tmp/harness/profiles/research/agents/fake-agent/agent.toml")
        );
        assert_eq!(
            paths.conversations_dir(),
            PathBuf::from("/tmp/harness/profiles/research/conversations")
        );

        let fallback = StoragePaths::new_with_profile("/tmp/harness", "../bad");
        assert_eq!(fallback.active_profile_id(), "main");
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
        assert_eq!(report.total_directories, 18);
        assert_eq!(report.largest_file, Some(paths.memory_file()));
        assert_eq!(report.largest_file_bytes, 7);
        assert_eq!(report.quota_bytes, None);
        assert_eq!(report.quota_remaining_bytes, None);
        assert!(!report.quota_exceeded);
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

        let over_quota = paths.storage_report_with_quota(Some(20)).unwrap();
        assert_eq!(over_quota.quota_bytes, Some(20));
        assert_eq!(over_quota.quota_remaining_bytes, Some(-2));
        assert!(over_quota.quota_exceeded);

        let under_quota = paths.storage_report_with_quota(Some(25)).unwrap();
        assert_eq!(under_quota.quota_bytes, Some(25));
        assert_eq!(under_quota.quota_remaining_bytes, Some(3));
        assert!(!under_quota.quota_exceeded);

        std::fs::remove_dir_all(report.root).unwrap();
    }

    #[test]
    fn cache_retention_can_plan_and_apply_cache_file_pruning() {
        let root = std::env::temp_dir().join(format!(
            "agent-storage-retention-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = StoragePaths::new(&root);
        paths.ensure_base_dirs().unwrap();
        let ingest_file = paths.ingestion_cache_dir().join("old-ingest.txt");
        let artifact_file = paths.artifacts_dir().join("old-artifact.txt");
        std::fs::write(paths.global_config(), b"keep").unwrap();
        std::fs::write(&ingest_file, b"ingest").unwrap();
        std::fs::write(&artifact_file, b"artifact").unwrap();

        let plan = paths.cache_retention_plan(0).unwrap();

        assert_eq!(plan.total_files, 2);
        assert_eq!(plan.total_bytes, 14);
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.bucket == "ingestion" && candidate.bytes == 6)
        );
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.bucket == "artifacts" && candidate.bytes == 8)
        );

        let dry_run = paths.prune_cache_retention(0, true).unwrap();
        assert!(dry_run.dry_run);
        assert_eq!(dry_run.deleted_files, 0);
        assert!(ingest_file.exists());
        assert!(artifact_file.exists());

        let applied = paths.prune_cache_retention(0, false).unwrap();
        assert!(!applied.dry_run);
        assert_eq!(applied.deleted_files, 2);
        assert_eq!(applied.deleted_bytes, 14);
        assert!(applied.errors.is_empty());
        assert!(!ingest_file.exists());
        assert!(!artifact_file.exists());
        assert!(paths.global_config().exists());

        std::fs::remove_dir_all(root).unwrap();
    }
}
