//! Durable deterministic batch plans.
//!
//! The runtime owns list iteration. This crate stores the item list and stable
//! per-item keys so a batch can be resumed without asking an LLM to remember
//! what was processed.

use std::collections::HashSet;
use std::{fs, path::Path};

use agent_storage::StoragePaths;
use serde::{Deserialize, Serialize};

pub const BATCH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error(transparent)]
    Storage(#[from] agent_storage::StorageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("unsupported future batch schema version {found}; this reader supports {supported}")]
    FutureSchema { found: u32, supported: u32 },
    #[error("invalid batch items: {0}")]
    InvalidItems(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPlan {
    pub schema_version: u32,
    pub batch_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub items: Vec<BatchItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPlanSummary {
    pub batch_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub items: u32,
    pub pending: u32,
    pub running: u32,
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub key: String,
    pub input: String,
    pub status: BatchItemState,
    pub attempts: u32,
    pub last_run_id: Option<String>,
    pub final_output: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchItemState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

impl BatchPlan {
    pub fn new(batch_id: impl Into<String>, inputs: Vec<String>) -> Self {
        let mut seen = HashSet::new();
        let items = inputs
            .into_iter()
            .filter_map(|input| {
                let key = item_key(&input);
                if !seen.insert(key.clone()) {
                    return None;
                }
                Some(BatchItem {
                    key,
                    input,
                    status: BatchItemState::Pending,
                    attempts: 0,
                    last_run_id: None,
                    final_output: None,
                    error: None,
                })
            })
            .collect();
        Self::from_items(batch_id, items)
    }

    pub fn new_with_item_keys(
        batch_id: impl Into<String>,
        inputs: Vec<String>,
        item_keys: Vec<String>,
    ) -> Result<Self, BatchError> {
        if inputs.len() != item_keys.len() {
            return Err(BatchError::InvalidItems(format!(
                "item_keys count {} must match items count {}",
                item_keys.len(),
                inputs.len()
            )));
        }

        let mut seen = HashSet::new();
        let mut items = Vec::new();
        for (index, (input, key)) in inputs.into_iter().zip(item_keys).enumerate() {
            let key = key.trim().to_string();
            if key.is_empty() {
                return Err(BatchError::InvalidItems(format!(
                    "item key at index {index} cannot be empty"
                )));
            }
            if !seen.insert(key.clone()) {
                return Err(BatchError::InvalidItems(format!(
                    "duplicate item key {key:?}"
                )));
            }
            items.push(BatchItem {
                key,
                input,
                status: BatchItemState::Pending,
                attempts: 0,
                last_run_id: None,
                final_output: None,
                error: None,
            });
        }

        Ok(Self::from_items(batch_id, items))
    }

    pub fn new_with_optional_item_keys(
        batch_id: impl Into<String>,
        inputs: Vec<String>,
        item_keys: Option<Vec<String>>,
    ) -> Result<Self, BatchError> {
        match item_keys {
            Some(item_keys) => Self::new_with_item_keys(batch_id, inputs, item_keys),
            None => Ok(Self::new(batch_id, inputs)),
        }
    }

    fn from_items(batch_id: impl Into<String>, items: Vec<BatchItem>) -> Self {
        let now = chrono::Utc::now();
        Self {
            schema_version: BATCH_SCHEMA_VERSION,
            batch_id: batch_id.into(),
            created_at: now,
            updated_at: now,
            items,
        }
    }

    pub fn load_from_env(batch_id: &str) -> Result<Self, BatchError> {
        Self::load(&StoragePaths::from_env(), batch_id)
    }

    pub fn load(paths: &StoragePaths, batch_id: &str) -> Result<Self, BatchError> {
        let text = fs::read_to_string(batch_path(paths, batch_id))?;
        let plan: Self = serde_json::from_str(&text)?;
        if plan.schema_version > BATCH_SCHEMA_VERSION {
            return Err(BatchError::FutureSchema {
                found: plan.schema_version,
                supported: BATCH_SCHEMA_VERSION,
            });
        }
        Ok(plan)
    }

    pub fn list_from_env() -> Result<Vec<BatchPlanSummary>, BatchError> {
        Self::list(&StoragePaths::from_env())
    }

    pub fn list(paths: &StoragePaths) -> Result<Vec<BatchPlanSummary>, BatchError> {
        let dir = paths.batches_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut summaries = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let text = fs::read_to_string(path)?;
            let plan: Self = serde_json::from_str(&text)?;
            if plan.schema_version > BATCH_SCHEMA_VERSION {
                return Err(BatchError::FutureSchema {
                    found: plan.schema_version,
                    supported: BATCH_SCHEMA_VERSION,
                });
            }
            summaries.push(plan.summary());
        }
        summaries.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.batch_id.cmp(&b.batch_id))
        });
        Ok(summaries)
    }

    pub fn delete_from_env(batch_id: &str) -> Result<(), BatchError> {
        Self::delete(&StoragePaths::from_env(), batch_id)
    }

    pub fn delete(paths: &StoragePaths, batch_id: &str) -> Result<(), BatchError> {
        fs::remove_file(batch_path(paths, batch_id))?;
        Ok(())
    }

    pub fn summary(&self) -> BatchPlanSummary {
        BatchPlanSummary {
            batch_id: self.batch_id.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            items: self.items.len() as u32,
            pending: self
                .items
                .iter()
                .filter(|item| item.status == BatchItemState::Pending)
                .count() as u32,
            running: self
                .items
                .iter()
                .filter(|item| item.status == BatchItemState::Running)
                .count() as u32,
            succeeded: self.succeeded_count(),
            failed: self.failed_count(),
        }
    }

    pub fn save_to_env(&mut self) -> Result<(), BatchError> {
        self.save(&StoragePaths::from_env())
    }

    pub fn save(&mut self, paths: &StoragePaths) -> Result<(), BatchError> {
        self.save_with_quota(paths, None)
    }

    fn save_with_quota(
        &mut self,
        paths: &StoragePaths,
        quota_bytes: Option<u64>,
    ) -> Result<(), BatchError> {
        paths.ensure_base_dirs()?;
        fs::create_dir_all(paths.batches_dir())?;
        self.updated_at = chrono::Utc::now();
        let text = serde_json::to_string_pretty(self)?;
        let path = batch_path(paths, &self.batch_id);
        if let Some(quota_bytes) = quota_bytes {
            paths.write_quota_checked_with_quota(&path, text.as_bytes(), Some(quota_bytes))?;
        } else {
            paths.write_quota_checked(&path, text.as_bytes())?;
        }
        Ok(())
    }

    pub fn mark_running(&mut self, key: &str) {
        if let Some(item) = self.item_mut(key) {
            item.status = BatchItemState::Running;
            item.attempts = item.attempts.saturating_add(1);
            item.error = None;
        }
    }

    pub fn mark_succeeded(&mut self, key: &str, run_id: String, final_output: String) {
        if let Some(item) = self.item_mut(key) {
            item.status = BatchItemState::Succeeded;
            item.last_run_id = Some(run_id);
            item.final_output = Some(final_output);
            item.error = None;
        }
    }

    pub fn mark_failed(&mut self, key: &str, error: String) {
        if let Some(item) = self.item_mut(key) {
            item.status = BatchItemState::Failed;
            item.error = Some(error);
        }
    }

    pub fn succeeded_count(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.status == BatchItemState::Succeeded)
            .count() as u32
    }

    pub fn failed_count(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.status == BatchItemState::Failed)
            .count() as u32
    }

    fn item_mut(&mut self, key: &str) -> Option<&mut BatchItem> {
        self.items.iter_mut().find(|item| item.key == key)
    }
}

pub fn batch_path(paths: &StoragePaths, batch_id: &str) -> std::path::PathBuf {
    paths.batches_dir().join(format!("{batch_id}.json"))
}

pub fn item_key(input: &str) -> String {
    format!("item-{:016x}", fnv1a64(input.as_bytes()))
}

pub fn text_file_batch_items(paths: &[String]) -> Result<(Vec<String>, Vec<String>), BatchError> {
    let mut inputs = Vec::new();
    let mut keys = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let key = path.trim();
        if key.is_empty() {
            return Err(BatchError::InvalidItems(format!(
                "file path at index {index} cannot be empty"
            )));
        }
        let content = fs::read_to_string(key).map_err(|err| {
            BatchError::InvalidItems(format!("failed to read batch file {key:?}: {err}"))
        })?;
        inputs.push(format!("File: {key}\n\n{content}"));
        keys.push(key.to_string());
    }
    Ok((inputs, keys))
}

pub fn prepare_batch_inputs(
    items: Vec<String>,
    item_keys: Option<Vec<String>>,
    files: Vec<String>,
    folders: Vec<String>,
) -> Result<(Vec<String>, Option<Vec<String>>), BatchError> {
    if items.is_empty() && files.is_empty() && folders.is_empty() {
        return Err(BatchError::InvalidItems(
            "batch run needs at least one item, file, or folder".into(),
        ));
    }
    if files.is_empty() && folders.is_empty() {
        return Ok((items, item_keys));
    }

    let mut combined_items = Vec::new();
    let mut combined_keys = Vec::new();
    if let Some(item_keys) = item_keys {
        if item_keys.len() != items.len() {
            return Err(BatchError::InvalidItems(format!(
                "item key count {} must match item count {}",
                item_keys.len(),
                items.len()
            )));
        }
        combined_items.extend(items);
        combined_keys.extend(item_keys);
    } else {
        let mut seen = HashSet::new();
        for item in items {
            let key = item_key(&item);
            if seen.insert(key.clone()) {
                combined_items.push(item);
                combined_keys.push(key);
            }
        }
    }

    let mut all_files = files;
    all_files.extend(text_folder_batch_files(&folders)?);
    let (file_items, file_keys) = text_file_batch_items(&all_files)?;
    combined_items.extend(file_items);
    combined_keys.extend(file_keys);
    Ok((combined_items, Some(combined_keys)))
}

pub fn text_folder_batch_files(folders: &[String]) -> Result<Vec<String>, BatchError> {
    let mut files = Vec::new();
    for (index, folder) in folders.iter().enumerate() {
        let folder = folder.trim();
        if folder.is_empty() {
            return Err(BatchError::InvalidItems(format!(
                "folder path at index {index} cannot be empty"
            )));
        }
        let root = Path::new(folder);
        if !root.is_dir() {
            return Err(BatchError::InvalidItems(format!(
                "batch folder {folder:?} is not a directory"
            )));
        }
        collect_folder_files(root, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_folder_files(path: &Path, files: &mut Vec<String>) -> Result<(), BatchError> {
    let mut entries = fs::read_dir(path)
        .map_err(|err| {
            BatchError::InvalidItems(format!(
                "failed to read batch folder {}: {err}",
                path.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            BatchError::InvalidItems(format!(
                "failed to read batch folder entry under {}: {err}",
                path.display()
            ))
        })?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_folder_files(&path, files)?;
        } else if path.is_file() {
            files.push(path.to_string_lossy().to_string());
        }
    }
    Ok(())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn plan_exists(paths: &StoragePaths, batch_id: &str) -> bool {
    batch_path(paths, batch_id).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_keys_are_stable_and_deduped() {
        assert_eq!(item_key("a"), item_key("a"));
        assert_ne!(item_key("a"), item_key("b"));

        let plan = BatchPlan::new("batch-x", vec!["a".into(), "a".into(), "b".into()]);
        assert_eq!(plan.items.len(), 2);
    }

    #[test]
    fn explicit_item_keys_are_preserved_and_validated() {
        let plan = BatchPlan::new_with_item_keys(
            "batch-x",
            vec!["same".into(), "same".into()],
            vec!["path/a.md".into(), "path/b.md".into()],
        )
        .unwrap();
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.items[0].key, "path/a.md");
        assert_eq!(plan.items[1].key, "path/b.md");

        let err = BatchPlan::new_with_item_keys(
            "batch-x",
            vec!["a".into(), "b".into()],
            vec!["dup".into(), "dup".into()],
        )
        .expect_err("duplicate explicit keys are ambiguous for resume");
        assert!(matches!(err, BatchError::InvalidItems(_)));

        let err = BatchPlan::new_with_item_keys("batch-x", vec!["a".into()], vec![])
            .expect_err("key count must match item count");
        assert!(matches!(err, BatchError::InvalidItems(_)));
    }

    #[test]
    fn text_file_items_use_paths_as_keys_and_include_content() {
        let root = std::env::temp_dir().join(format!(
            "agent-batch-file-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("input.txt");
        std::fs::write(&file, "hello from a file").unwrap();
        let path = file.to_string_lossy().to_string();

        let (inputs, keys) = text_file_batch_items(std::slice::from_ref(&path)).unwrap();
        assert_eq!(keys, vec![path.clone()]);
        assert_eq!(inputs, vec![format!("File: {path}\n\nhello from a file")]);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prepared_inputs_combine_prompt_and_file_sources() {
        let root = std::env::temp_dir().join(format!(
            "agent-batch-prepare-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("input.txt");
        std::fs::write(&file, "file body").unwrap();
        let path = file.to_string_lossy().to_string();

        let (inputs, keys) = prepare_batch_inputs(
            vec!["prompt".into(), "prompt".into()],
            None,
            vec![path.clone()],
            Vec::new(),
        )
        .unwrap();
        assert_eq!(inputs.len(), 2);
        assert_eq!(keys.unwrap(), vec![item_key("prompt"), path.clone()]);
        assert!(inputs[1].contains("file body"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn folder_inputs_expand_regular_files() {
        let root = std::env::temp_dir().join(format!(
            "agent-batch-folder-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("b.txt"), "b body").unwrap();
        std::fs::write(nested.join("a.txt"), "a body").unwrap();
        let folder = root.to_string_lossy().to_string();

        let (inputs, keys) =
            prepare_batch_inputs(Vec::new(), None, Vec::new(), vec![folder]).unwrap();
        let keys = keys.unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().any(|key| key.ends_with("a.txt")));
        assert!(keys.iter().any(|key| key.ends_with("b.txt")));
        assert!(inputs.iter().any(|input| input.contains("a body")));
        assert!(inputs.iter().any(|input| input.contains("b body")));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plan_round_trips_to_storage() {
        let root = std::env::temp_dir().join(format!(
            "agent-batch-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let paths = StoragePaths::new(&root);
        let mut plan = BatchPlan::new("batch-x", vec!["a".into()]);
        plan.mark_running(&item_key("a"));
        plan.mark_succeeded(&item_key("a"), "run-1".into(), "ok".into());
        plan.save(&paths).unwrap();

        let loaded = BatchPlan::load(&paths, "batch-x").unwrap();
        assert_eq!(loaded.items[0].status, BatchItemState::Succeeded);
        assert_eq!(loaded.items[0].last_run_id.as_deref(), Some("run-1"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plan_list_summarizes_and_delete_removes_saved_plan() {
        let root = std::env::temp_dir().join(format!(
            "agent-batch-list-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let paths = StoragePaths::new(&root);

        let mut plan = BatchPlan::new("batch-list", vec!["a".into(), "b".into()]);
        plan.mark_running(&item_key("a"));
        plan.mark_succeeded(&item_key("a"), "run-1".into(), "ok".into());
        plan.mark_failed(&item_key("b"), "bad".into());
        plan.save(&paths).unwrap();

        let summaries = BatchPlan::list(&paths).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].batch_id, "batch-list");
        assert_eq!(summaries[0].items, 2);
        assert_eq!(summaries[0].succeeded, 1);
        assert_eq!(summaries[0].failed, 1);

        BatchPlan::delete(&paths, "batch-list").unwrap();
        assert!(!batch_path(&paths, "batch-list").exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plan_save_rejects_batches_over_storage_quota() {
        let root = std::env::temp_dir().join(format!(
            "agent-batch-quota-test-{}",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let paths = StoragePaths::new(&root);
        let mut plan = BatchPlan::new(
            "batch-quota",
            vec!["this batch input exceeds the tiny test quota".into()],
        );

        let err = plan
            .save_with_quota(&paths, Some(16))
            .expect_err("batch save should fail before exceeding quota");
        assert!(matches!(
            err,
            BatchError::Storage(agent_storage::StorageError::QuotaExceeded { .. })
        ));
        assert!(!batch_path(&paths, "batch-quota").exists());

        let _ = std::fs::remove_dir_all(root);
    }
}
