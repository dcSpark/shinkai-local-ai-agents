//! Durable deterministic batch plans.
//!
//! The runtime owns list iteration. This crate stores the item list and stable
//! per-item keys so a batch can be resumed without asking an LLM to remember
//! what was processed.

use std::collections::HashSet;
use std::fs;

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

    pub fn save_to_env(&mut self) -> Result<(), BatchError> {
        self.save(&StoragePaths::from_env())
    }

    pub fn save(&mut self, paths: &StoragePaths) -> Result<(), BatchError> {
        paths.ensure_base_dirs()?;
        fs::create_dir_all(paths.batches_dir())?;
        self.updated_at = chrono::Utc::now();
        let text = serde_json::to_string_pretty(self)?;
        fs::write(batch_path(paths, &self.batch_id), text)?;
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
}
