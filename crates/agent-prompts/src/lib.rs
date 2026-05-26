//! `agent-prompts` — small saved prompt libraries for profile and agent scopes.
//!
//! Prompts are plain Markdown files under `profiles/<profile>/prompts/` for
//! profile-global prompts and `profiles/<profile>/agents/<agent>/prompts/` for
//! agent-specific command libraries.

use std::path::{Path, PathBuf};

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
    #[error("saved prompt {0:?} not found")]
    NotFound(String),
    #[error("invalid portable prompt document: {0}")]
    InvalidPortableDoc(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptDoc {
    pub name: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePromptDoc {
    pub schema_version: u32,
    pub prompt: PromptDoc,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PromptImportDoc {
    Portable(PortablePromptDoc),
    Legacy(PromptDoc),
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
        self.save_scoped(None, name, body)
    }

    pub fn save_for_agent(
        &self,
        agent_id: &str,
        name: &str,
        body: &str,
    ) -> Result<PromptDoc, PromptError> {
        self.save_scoped(Some(agent_id), name, body)
    }

    pub fn save_scoped(
        &self,
        agent_id: Option<&str>,
        name: &str,
        body: &str,
    ) -> Result<PromptDoc, PromptError> {
        self.save_scoped_with_quota(agent_id, name, body, None)
    }

    fn save_scoped_with_quota(
        &self,
        agent_id: Option<&str>,
        name: &str,
        body: &str,
        quota_bytes: Option<u64>,
    ) -> Result<PromptDoc, PromptError> {
        self.paths.ensure_base_dirs()?;
        let name = normalize_name(name)?;
        let agent_id = normalize_agent_id_opt(agent_id)?;
        let path = self.prompt_path(agent_id.as_deref(), &name);
        if let Some(quota_bytes) = quota_bytes {
            self.paths
                .write_quota_checked_with_quota(path, body.as_bytes(), Some(quota_bytes))?;
        } else {
            self.paths.write_quota_checked(path, body.as_bytes())?;
        }
        Ok(PromptDoc {
            name,
            body: body.to_string(),
            agent_id,
        })
    }

    pub fn get(&self, name: &str) -> Result<Option<PromptDoc>, PromptError> {
        self.get_scoped(None, name)
    }

    pub fn get_for_agent(
        &self,
        agent_id: &str,
        name: &str,
    ) -> Result<Option<PromptDoc>, PromptError> {
        self.get_scoped(Some(agent_id), name)
    }

    pub fn get_scoped(
        &self,
        agent_id: Option<&str>,
        name: &str,
    ) -> Result<Option<PromptDoc>, PromptError> {
        self.paths.ensure_base_dirs()?;
        let name = normalize_name(name)?;
        let agent_id = normalize_agent_id_opt(agent_id)?;
        let path = self.prompt_path(agent_id.as_deref(), &name);
        if !path.exists() {
            return Ok(None);
        }
        let body = std::fs::read_to_string(path)?;
        Ok(Some(PromptDoc {
            name,
            body,
            agent_id,
        }))
    }

    pub fn resolve_for_agent(
        &self,
        agent_id: Option<&str>,
        name: &str,
    ) -> Result<Option<PromptDoc>, PromptError> {
        if let Some(agent_id) = agent_id
            && let Some(prompt) = self.get_for_agent(agent_id, name)?
        {
            return Ok(Some(prompt));
        }
        self.get(name)
    }

    pub fn list(&self) -> Result<Vec<PromptDoc>, PromptError> {
        self.list_scoped(None)
    }

    pub fn list_for_agent(&self, agent_id: &str) -> Result<Vec<PromptDoc>, PromptError> {
        self.list_scoped(Some(agent_id))
    }

    pub fn list_scoped(&self, agent_id: Option<&str>) -> Result<Vec<PromptDoc>, PromptError> {
        self.paths.ensure_base_dirs()?;
        let agent_id = normalize_agent_id_opt(agent_id)?;
        let dir = self.prompt_dir(agent_id.as_deref());
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut prompts = Vec::new();
        for entry in std::fs::read_dir(dir)? {
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
            prompts.push(PromptDoc {
                name,
                body,
                agent_id: agent_id.clone(),
            });
        }
        prompts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(prompts)
    }

    pub fn delete(&self, name: &str) -> Result<bool, PromptError> {
        self.delete_scoped(None, name)
    }

    pub fn delete_for_agent(&self, agent_id: &str, name: &str) -> Result<bool, PromptError> {
        self.delete_scoped(Some(agent_id), name)
    }

    pub fn delete_scoped(&self, agent_id: Option<&str>, name: &str) -> Result<bool, PromptError> {
        self.paths.ensure_base_dirs()?;
        let name = normalize_name(name)?;
        let agent_id = normalize_agent_id_opt(agent_id)?;
        let path = self.prompt_path(agent_id.as_deref(), &name);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(path)?;
        Ok(true)
    }

    pub fn export_scoped(
        &self,
        agent_id: Option<&str>,
        name: &str,
        path: impl AsRef<Path>,
    ) -> Result<PromptDoc, PromptError> {
        let prompt = self
            .get_scoped(agent_id, name)?
            .ok_or_else(|| PromptError::NotFound(name.to_string()))?;
        let portable = PortablePromptDoc {
            schema_version: 1,
            prompt: prompt.clone(),
        };
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(&portable)?)?;
        Ok(prompt)
    }

    pub fn import_file(
        &self,
        path: impl AsRef<Path>,
        agent_id_override: Option<&str>,
    ) -> Result<PromptDoc, PromptError> {
        let input: PromptImportDoc = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let prompt = match input {
            PromptImportDoc::Portable(doc) => {
                if doc.schema_version != 1 {
                    return Err(PromptError::InvalidPortableDoc(format!(
                        "unsupported schema_version {}",
                        doc.schema_version
                    )));
                }
                doc.prompt
            }
            PromptImportDoc::Legacy(prompt) => prompt,
        };
        let target_agent = agent_id_override.or(prompt.agent_id.as_deref());
        self.save_scoped(target_agent, &prompt.name, &prompt.body)
    }

    fn prompt_dir(&self, agent_id: Option<&str>) -> PathBuf {
        if let Some(agent_id) = agent_id {
            self.paths.agent_prompts_dir(agent_id)
        } else {
            self.paths.prompts_dir()
        }
    }

    fn prompt_path(&self, agent_id: Option<&str>, name: &str) -> PathBuf {
        self.prompt_dir(agent_id).join(format!("{name}.md"))
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

fn normalize_agent_id_opt(agent_id: Option<&str>) -> Result<Option<String>, PromptError> {
    agent_id.map(normalize_agent_id).transpose()
}

fn normalize_agent_id(agent_id: &str) -> Result<String, PromptError> {
    let trimmed = agent_id.trim();
    let valid = !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    if valid {
        Ok(trimmed.to_string())
    } else {
        Err(PromptError::InvalidName(agent_id.to_string()))
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

    #[test]
    fn per_agent_prompts_override_global_when_resolving() {
        let dir = std::env::temp_dir().join(format!("agent-prompts-scoped-test-{}", uuid_like()));
        let store = PromptStore::new(StoragePaths::new(&dir));

        store.save("daily", "global daily").unwrap();
        store
            .save_for_agent("critic", "daily", "critic daily")
            .unwrap();
        store
            .save_for_agent("critic", "review", "critic review")
            .unwrap();

        assert_eq!(
            store.resolve_for_agent(Some("critic"), "daily").unwrap(),
            Some(PromptDoc {
                name: "daily".into(),
                body: "critic daily".into(),
                agent_id: Some("critic".into()),
            })
        );
        assert_eq!(
            store.resolve_for_agent(Some("critic"), "missing").unwrap(),
            None
        );
        assert_eq!(
            store.resolve_for_agent(Some("worker"), "daily").unwrap(),
            Some(PromptDoc {
                name: "daily".into(),
                body: "global daily".into(),
                agent_id: None,
            })
        );
        assert_eq!(store.list_for_agent("critic").unwrap().len(), 2);
        assert!(store.delete_for_agent("critic", "daily").unwrap());
        assert_eq!(
            store.resolve_for_agent(Some("critic"), "daily").unwrap(),
            store.get("daily").unwrap()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_ids_cannot_escape_prompt_dir() {
        let dir = std::env::temp_dir().join(format!("agent-prompts-agent-id-test-{}", uuid_like()));
        let store = PromptStore::new(StoragePaths::new(&dir));

        assert!(store.save_for_agent("../critic", "daily", "nope").is_err());
        assert!(
            store
                .save_for_agent("critic/main", "daily", "nope")
                .is_err()
        );
        assert!(store.save_for_agent("critic-main_1", "daily", "ok").is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prompt_save_rejects_body_over_storage_quota() {
        let dir = std::env::temp_dir().join(format!("agent-prompts-quota-test-{}", uuid_like()));
        let store = PromptStore::new(StoragePaths::new(&dir));

        let err = store
            .save_scoped_with_quota(None, "too-large", "this body exceeds the quota", Some(8))
            .expect_err("prompt save should fail before writing over quota");

        assert!(matches!(
            err,
            PromptError::Storage(StorageError::QuotaExceeded { .. })
        ));
        assert!(store.get("too-large").unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prompt_export_import_preserves_scope_by_default() {
        let dir = std::env::temp_dir().join(format!("agent-prompts-portable-test-{}", uuid_like()));
        let store = PromptStore::new(StoragePaths::new(&dir));
        store
            .save_for_agent("critic", "daily", "review this patch")
            .unwrap();

        let export_path = dir.join("daily.prompt.json");
        let exported = store
            .export_scoped(Some("critic"), "daily", &export_path)
            .unwrap();
        assert_eq!(exported.agent_id.as_deref(), Some("critic"));
        assert!(store.delete_for_agent("critic", "daily").unwrap());

        let imported = store.import_file(&export_path, None).unwrap();
        assert_eq!(imported.agent_id.as_deref(), Some("critic"));
        assert_eq!(
            store
                .get_for_agent("critic", "daily")
                .unwrap()
                .unwrap()
                .body,
            "review this patch"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prompt_import_can_override_scope() {
        let dir = std::env::temp_dir().join(format!(
            "agent-prompts-portable-override-test-{}",
            uuid_like()
        ));
        let store = PromptStore::new(StoragePaths::new(&dir));
        store
            .save_for_agent("critic", "daily", "review this patch")
            .unwrap();

        let export_path = dir.join("daily.prompt.json");
        store
            .export_scoped(Some("critic"), "daily", &export_path)
            .unwrap();
        let imported = store.import_file(&export_path, Some("builder")).unwrap();

        assert_eq!(imported.agent_id.as_deref(), Some("builder"));
        assert_eq!(
            store
                .get_for_agent("builder", "daily")
                .unwrap()
                .unwrap()
                .body,
            "review this patch"
        );
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
