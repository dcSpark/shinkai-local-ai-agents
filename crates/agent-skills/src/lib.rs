//! `agent-skills` — file-backed SkillDoc registry.
//!
//! v0 imports text-only OpenClaw/AgentSkills `SKILL.md` style files as
//! quarantined markdown. Allowing a skill is explicit and local.

use std::path::{Path, PathBuf};

use agent_core::{SkillView, VisibilityLevel};
use agent_storage::{StorageError, StoragePaths};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("skill not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDoc {
    pub id: String,
    pub name: String,
    pub description: String,
    pub body: String,
    pub source_path: Option<PathBuf>,
    pub quarantined: bool,
}

pub struct SkillRegistry {
    paths: StoragePaths,
}

impl SkillRegistry {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn import_openclaw(&self, path: impl AsRef<Path>) -> Result<SkillDoc, SkillError> {
        self.paths.ensure_base_dirs()?;
        let path = path.as_ref();
        let skill_path = if path.is_dir() {
            path.join("SKILL.md")
        } else {
            path.to_path_buf()
        };
        let body = std::fs::read_to_string(&skill_path)?;
        let name = infer_name(&body, &skill_path);
        let id = slugify(&name);
        let description = infer_description(&body);
        let doc = SkillDoc {
            id,
            name,
            description,
            body,
            source_path: Some(skill_path),
            quarantined: true,
        };
        self.write(&doc)?;
        Ok(doc)
    }

    pub fn list(&self) -> Result<Vec<SkillDoc>, SkillError> {
        self.paths.ensure_base_dirs()?;
        let mut docs = Vec::new();
        for entry in std::fs::read_dir(self.paths.skills_dir())? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                docs.push(read_doc(entry.path())?);
            }
        }
        docs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(docs)
    }

    pub fn inspect(&self, id: &str) -> Result<SkillDoc, SkillError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(SkillError::NotFound(id.into()));
        }
        read_doc(path)
    }

    pub fn allow(&self, id: &str) -> Result<SkillDoc, SkillError> {
        let mut doc = self.inspect(id)?;
        doc.quarantined = false;
        self.write(&doc)?;
        Ok(doc)
    }

    pub fn quarantine(&self, id: &str) -> Result<SkillDoc, SkillError> {
        let mut doc = self.inspect(id)?;
        doc.quarantined = true;
        self.write(&doc)?;
        Ok(doc)
    }

    pub fn visible_skill_views(&self) -> Result<Vec<SkillView>, SkillError> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|s| !s.quarantined)
            .map(|s| SkillView {
                id: s.id,
                name: s.name,
                description: Some(s.description),
                visibility: VisibilityLevel::FullSchema,
            })
            .collect())
    }

    fn write(&self, doc: &SkillDoc) -> Result<(), SkillError> {
        self.paths.ensure_base_dirs()?;
        std::fs::write(self.path_for(&doc.id), serde_json::to_string_pretty(doc)?)?;
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.paths.skills_dir().join(format!("{id}.json"))
    }
}

fn read_doc(path: impl AsRef<Path>) -> Result<SkillDoc, SkillError> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn infer_name(body: &str, path: &Path) -> String {
    body.lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "skill".into())
}

fn infer_description(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("Imported text skill.")
        .chars()
        .take(240)
        .collect()
}

fn slugify(s: &str) -> String {
    let slug: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    slug.split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_starts_quarantined_then_allow_makes_visible() {
        let dir = std::env::temp_dir().join(format!("skills-test-{}", std::process::id()));
        let source = dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "# Review Notes\nUse careful review.",
        )
        .unwrap();
        let registry = SkillRegistry::new(StoragePaths::new(dir.join("home")));
        let doc = registry.import_openclaw(&source).unwrap();
        assert!(doc.quarantined);
        assert!(registry.visible_skill_views().unwrap().is_empty());
        registry.allow(&doc.id).unwrap();
        assert_eq!(registry.visible_skill_views().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
