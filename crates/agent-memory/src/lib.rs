//! `agent-memory` — constrained file-backed memory v0.
//!
//! Memory generation and loading are deliberately separate. This crate only
//! writes/loads when explicitly called by the CLI/Tauri layer.

use std::path::{Path, PathBuf};

use agent_core::MemoryFragment;
use agent_storage::{StorageError, StoragePaths};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("memory rejected by injection scan: {0}")]
    Injection(String),
    #[error("memory not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub content: String,
    pub target: MemoryTarget,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub author: MemoryAuthor,
    pub source_range: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTarget {
    Agent,
    User,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAuthor {
    Human,
    Model,
}

pub struct MemoryStore {
    paths: StoragePaths,
}

impl MemoryStore {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn create(
        &self,
        target: MemoryTarget,
        content: &str,
        author: MemoryAuthor,
        source_range: Option<String>,
    ) -> Result<MemoryRecord, MemoryError> {
        scan(content)?;
        self.paths.ensure_base_dirs()?;
        let now = Utc::now();
        let record = MemoryRecord {
            id: format!("mem-{}", now.timestamp_nanos_opt().unwrap_or_default()),
            content: content.into(),
            target,
            created_at: now,
            updated_at: now,
            author,
            source_range,
        };
        let mut records = self.list_target(target)?;
        records.push(record.clone());
        self.write_target(target, &records)?;
        Ok(record)
    }

    pub fn generate_from_text(
        &self,
        target: MemoryTarget,
        text: &str,
        source_range: Option<String>,
    ) -> Result<Vec<MemoryRecord>, MemoryError> {
        let candidates = generated_memory_candidates(text);
        let mut records = Vec::new();
        for candidate in candidates {
            records.push(self.create(
                target,
                &candidate,
                MemoryAuthor::Model,
                source_range.clone(),
            )?);
        }
        Ok(records)
    }

    pub fn list(&self) -> Result<Vec<MemoryRecord>, MemoryError> {
        let mut records = self.list_target(MemoryTarget::Agent)?;
        records.extend(self.list_target(MemoryTarget::User)?);
        records.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(records)
    }

    pub fn edit(&self, id: &str, content: &str) -> Result<MemoryRecord, MemoryError> {
        scan(content)?;
        for target in [MemoryTarget::Agent, MemoryTarget::User] {
            let mut records = self.list_target(target)?;
            if let Some(record) = records.iter_mut().find(|r| r.id == id) {
                record.content = content.into();
                record.updated_at = Utc::now();
                let updated = record.clone();
                self.write_target(target, &records)?;
                return Ok(updated);
            }
        }
        Err(MemoryError::NotFound(id.into()))
    }

    pub fn delete(&self, id: &str) -> Result<(), MemoryError> {
        for target in [MemoryTarget::Agent, MemoryTarget::User] {
            let mut records = self.list_target(target)?;
            let before = records.len();
            records.retain(|r| r.id != id);
            if records.len() != before {
                self.write_target(target, &records)?;
                return Ok(());
            }
        }
        Err(MemoryError::NotFound(id.into()))
    }

    pub fn rollback(&self, target: MemoryTarget) -> Result<(), MemoryError> {
        let path = self.path_for(target);
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("memory.md");
        let backup = self.paths.memory_backup_dir().join(format!("{name}.bak"));
        if !backup.exists() {
            return Err(MemoryError::NotFound(backup.display().to_string()));
        }
        self.paths.ensure_base_dirs()?;
        std::fs::copy(backup, path)?;
        Ok(())
    }

    pub fn load_fragments(&self) -> Result<Vec<MemoryFragment>, MemoryError> {
        Ok(self
            .list()?
            .into_iter()
            .map(|r| MemoryFragment {
                id: r.id,
                content: r.content,
                provenance: format!("{:?} memory ({:?})", r.target, r.author),
            })
            .collect())
    }

    fn list_target(&self, target: MemoryTarget) -> Result<Vec<MemoryRecord>, MemoryError> {
        let path = self.path_for(target);
        if !path.exists() {
            return Ok(Vec::new());
        }
        parse_records(&std::fs::read_to_string(path)?)
    }

    fn write_target(
        &self,
        target: MemoryTarget,
        records: &[MemoryRecord],
    ) -> Result<(), MemoryError> {
        self.paths.ensure_base_dirs()?;
        let path = self.path_for(target);
        backup_existing(&path, &self.paths.memory_backup_dir())?;
        std::fs::write(path, render_records(records)?)?;
        Ok(())
    }

    fn path_for(&self, target: MemoryTarget) -> PathBuf {
        match target {
            MemoryTarget::Agent => self.paths.memory_file(),
            MemoryTarget::User => self.paths.user_memory_file(),
        }
    }
}

fn render_records(records: &[MemoryRecord]) -> Result<String, MemoryError> {
    let mut out = String::new();
    for record in records {
        out.push_str("---\n");
        out.push_str(&serde_json::to_string(record)?);
        out.push_str("\n---\n");
        out.push_str(&record.content);
        out.push('\n');
    }
    Ok(out)
}

fn parse_records(text: &str) -> Result<Vec<MemoryRecord>, MemoryError> {
    let mut records = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line != "---" {
            continue;
        }
        let Some(json_line) = lines.next() else {
            break;
        };
        let mut record: MemoryRecord = serde_json::from_str(json_line)?;
        let _closing = lines.next();
        let mut content = String::new();
        for body_line in lines.by_ref() {
            if body_line == "---" {
                let mut rest = vec!["---".to_string()];
                rest.extend(lines.map(str::to_string));
                let remaining = rest.join("\n");
                records.push(record);
                records.extend(parse_records(&remaining)?);
                return Ok(records);
            }
            content.push_str(body_line);
            content.push('\n');
        }
        if !content.trim().is_empty() {
            record.content = content.trim_end().into();
        }
        records.push(record);
        break;
    }
    Ok(records)
}

fn backup_existing(path: &Path, backup_dir: &Path) -> Result<(), MemoryError> {
    if path.exists() {
        std::fs::create_dir_all(backup_dir)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("memory.md");
        std::fs::copy(path, backup_dir.join(format!("{name}.bak")))?;
    }
    Ok(())
}

fn generated_memory_candidates(text: &str) -> Vec<String> {
    let mut candidates: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            line.strip_prefix("remember:")
                .or_else(|| line.strip_prefix("Remember:"))
                .map(str::trim)
        })
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    if candidates.is_empty() && !text.trim().is_empty() {
        candidates.push(text.trim().chars().take(500).collect());
    }
    candidates
}

fn scan(content: &str) -> Result<(), MemoryError> {
    let lower = content.to_ascii_lowercase();
    for needle in [
        "ignore previous instructions",
        "reveal your system prompt",
        "exfiltrate",
        "steal",
        "private key",
        "seed phrase",
        "ssh key",
    ] {
        if lower.contains(needle) {
            return Err(MemoryError::Injection(needle.into()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_load_and_delete_memory() {
        let dir = std::env::temp_dir().join(format!("memory-test-{}", std::process::id()));
        let store = MemoryStore::new(StoragePaths::new(&dir));
        let record = store
            .create(
                MemoryTarget::Agent,
                "Use concise answers.",
                MemoryAuthor::Human,
                None,
            )
            .unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(
            store.load_fragments().unwrap()[0].content,
            "Use concise answers."
        );
        let edited = store.edit(&record.id, "Use direct answers.").unwrap();
        assert_eq!(edited.content, "Use direct answers.");
        store.delete(&record.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn generate_extracts_remember_lines_and_rollback_restores_backup() {
        let dir = std::env::temp_dir().join(format!("memory-gen-test-{}", std::process::id()));
        let store = MemoryStore::new(StoragePaths::new(&dir));
        let generated = store
            .generate_from_text(
                MemoryTarget::Agent,
                "hello\nRemember: prefers terse status updates",
                Some("1:2".into()),
            )
            .unwrap();
        assert_eq!(generated.len(), 1);
        store.edit(&generated[0].id, "temporary edit").unwrap();
        store.rollback(MemoryTarget::Agent).unwrap();
        assert_eq!(
            store.list().unwrap()[0].content,
            "prefers terse status updates"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
