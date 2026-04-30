//! `agent-ingest` — explicit local ingestion v0.
//!
//! The backend is intentionally conservative: text/markdown/code are decoded
//! directly; PDFs get a lightweight text scan fallback until a richer backend
//! is plugged in.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use agent_storage::{StorageError, StoragePaths};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("artifact not found: {0}")]
    NotFound(String),
    #[error("unsupported ingestion backend: {0}")]
    UnsupportedBackend(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionArtifact {
    pub id: String,
    pub source: PathBuf,
    pub backend: String,
    pub content_hash: String,
    pub sections: Vec<IngestSection>,
    pub extracted_text: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSection {
    pub index: u32,
    pub title: Option<String>,
    pub text: String,
}

pub struct IngestionStore {
    paths: StoragePaths,
}

impl IngestionStore {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn ingest(&self, source: impl AsRef<Path>) -> Result<IngestionArtifact, IngestError> {
        self.ingest_with_backend(source, "local-v0")
    }

    pub fn ingest_with_backend(
        &self,
        source: impl AsRef<Path>,
        backend: &str,
    ) -> Result<IngestionArtifact, IngestError> {
        if backend != "local-v0" {
            return Err(IngestError::UnsupportedBackend(backend.into()));
        }
        self.paths.ensure_base_dirs()?;
        let source = source.as_ref();
        let bytes = std::fs::read(source)?;
        let extracted = extract_text(source, &bytes);
        let content_hash = hash_bytes(&bytes);
        let id = format!("ingest-{backend}-{content_hash}");
        let artifact = IngestionArtifact {
            id,
            source: source.to_path_buf(),
            backend: backend.into(),
            content_hash,
            sections: split_sections(&extracted),
            extracted_text: Some(extracted),
            created_at: Utc::now(),
        };
        self.write(&artifact)?;
        Ok(artifact)
    }

    pub fn list(&self) -> Result<Vec<IngestionArtifact>, IngestError> {
        self.paths.ensure_base_dirs()?;
        let mut artifacts = Vec::new();
        for entry in std::fs::read_dir(self.paths.ingestion_cache_dir())? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                artifacts.push(read_artifact(entry.path())?);
            }
        }
        artifacts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(artifacts)
    }

    pub fn show(&self, id: &str) -> Result<IngestionArtifact, IngestError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(IngestError::NotFound(id.into()));
        }
        read_artifact(path)
    }

    pub fn remove(&self, id: &str) -> Result<(), IngestError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(IngestError::NotFound(id.into()));
        }
        std::fs::remove_file(path)?;
        Ok(())
    }

    fn write(&self, artifact: &IngestionArtifact) -> Result<(), IngestError> {
        std::fs::write(
            self.path_for(&artifact.id),
            serde_json::to_string_pretty(artifact)?,
        )?;
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.paths.ingestion_cache_dir().join(format!("{id}.json"))
    }
}

fn read_artifact(path: impl AsRef<Path>) -> Result<IngestionArtifact, IngestError> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn extract_text(source: &Path, bytes: &[u8]) -> String {
    if source.extension().and_then(|s| s.to_str()) == Some("pdf") {
        return extract_pdfish_text(bytes);
    }
    String::from_utf8_lossy(bytes).to_string()
}

fn extract_pdfish_text(bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    let mut in_text = false;
    for ch in raw.chars() {
        match ch {
            '(' => in_text = true,
            ')' => {
                in_text = false;
                out.push('\n');
            }
            _ if in_text && !ch.is_control() => out.push(ch),
            _ => {}
        }
    }
    if out.trim().is_empty() {
        "[pdf text extraction produced no plain text with local-v0 backend]".into()
    } else {
        out
    }
}

fn split_sections(text: &str) -> Vec<IngestSection> {
    let mut sections = Vec::new();
    let chunks: Vec<&str> = text
        .split("\n\n")
        .filter(|s| !s.trim().is_empty())
        .collect();
    for (idx, chunk) in chunks.iter().enumerate() {
        sections.push(IngestSection {
            index: idx as u32,
            title: chunk
                .lines()
                .next()
                .filter(|line| line.starts_with('#'))
                .map(|s| s.trim_start_matches('#').trim().to_string()),
            text: chunk.trim().into(),
        });
    }
    if sections.is_empty() && !text.trim().is_empty() {
        sections.push(IngestSection {
            index: 0,
            title: None,
            text: text.trim().into(),
        });
    }
    sections
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_show_and_remove_text_artifact() {
        let dir = std::env::temp_dir().join(format!("ingest-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("note.md");
        std::fs::write(&source, "# Title\n\nBody").unwrap();
        let store = IngestionStore::new(StoragePaths::new(dir.join("home")));
        let artifact = store.ingest(&source).unwrap();
        assert_eq!(artifact.sections.len(), 2);
        assert_eq!(store.show(&artifact.id).unwrap().id, artifact.id);
        store.remove(&artifact.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
