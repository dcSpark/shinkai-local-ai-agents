//! `agent-prompts` — a small saved prompt library for the main profile.
//!
//! Prompts are plain Markdown files in `profiles/main/prompts/`, matching the
//! human-readable layout in `specs/architecture.md` §14.1.

use std::path::PathBuf;

use agent_storage::{StorageError, StoragePaths};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid prompt name {0:?}; use letters, numbers, dots, dashes, or underscores")]
    InvalidName(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptDoc {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct PromptStore {
    paths: StoragePaths,
}

impl PromptStore {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn save(&self, name: &str, body: &str) -> Result<PromptDoc, PromptError> {
        self.paths.ensure_base_dirs()?;
        let name = normalize_name(name)?;
        std::fs::write(self.prompt_path(&name), body)?;
        Ok(PromptDoc {
            name,
            body: body.to_string(),
        })
    }

    pub fn get(&self, name: &str) -> Result<Option<PromptDoc>, PromptError> {
        self.paths.ensure_base_dirs()?;
        let name = normalize_name(name)?;
        let path = self.prompt_path(&name);
        if !path.exists() {
            return Ok(None);
        }
        let body = std::fs::read_to_string(path)?;
        Ok(Some(PromptDoc { name, body }))
    }

    pub fn list(&self) -> Result<Vec<PromptDoc>, PromptError> {
        self.paths.ensure_base_dirs()?;
        let mut prompts = Vec::new();
        for entry in std::fs::read_dir(self.paths.prompts_dir())? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let name = normalize_name(stem)?;
            let body = std::fs::read_to_string(path)?;
            prompts.push(PromptDoc { name, body });
        }
        prompts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(prompts)
    }

    pub fn delete(&self, name: &str) -> Result<bool, PromptError> {
        self.paths.ensure_base_dirs()?;
        let name = normalize_name(name)?;
        let path = self.prompt_path(&name);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(path)?;
        Ok(true)
    }

    fn prompt_path(&self, name: &str) -> PathBuf {
        self.paths.prompts_dir().join(format!("{name}.md"))
    }
}

fn normalize_name(name: &str) -> Result<String, PromptError> {
    let trimmed = name.trim().strip_suffix(".md").unwrap_or(name.trim());
    let valid = !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'));
    if valid {
        Ok(trimmed.to_string())
    } else {
        Err(PromptError::InvalidName(name.to_string()))
    }
}

pub fn is_valid_prompt_name(name: &str) -> bool {
    normalize_name(name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_round_trip_and_sort() {
        let dir = std::env::temp_dir().join(format!("agent-prompts-test-{}", uuid_like()));
        let store = PromptStore::new(StoragePaths::new(&dir));

        store.save("zeta", "last").unwrap();
        store.save("alpha", "first").unwrap();

        assert_eq!(store.get("alpha").unwrap().unwrap().body, "first");
        assert_eq!(
            store
                .list()
                .unwrap()
                .into_iter()
                .map(|prompt| prompt.name)
                .collect::<Vec<_>>(),
            vec!["alpha".to_string(), "zeta".to_string()]
        );
        assert!(store.delete("alpha").unwrap());
        assert!(store.get("alpha").unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prompt_names_cannot_escape_prompt_dir() {
        assert!(normalize_name("../secret").is_err());
        assert!(normalize_name("nested/name").is_err());
        assert!(normalize_name("ok-name_1.2").is_ok());
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
