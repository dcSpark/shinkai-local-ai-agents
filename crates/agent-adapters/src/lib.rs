//! `agent-adapters` — normalization and quarantine metadata.
//!
//! Adapter execution is intentionally out of scope here. This crate gives the
//! runtime a single inspectable shape for imports before any capability is
//! allowed into an agent.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use agent_storage::{StorageError, StoragePaths};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("adapter package not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    OpenClawAgentSkills,
    Mcp,
    ClawHub,
    HermesPlugin,
    HermesExternalAgent,
    A2a,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedPackage {
    pub id: String,
    pub source: PathBuf,
    pub adapter: AdapterKind,
    pub digest: String,
    pub quarantined: bool,
    pub capabilities: Vec<NormalizedCapability>,
    pub permissions: PermissionManifest,
    pub findings: Vec<StaticScanFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedCapability {
    pub id: String,
    pub kind: CapabilityKind,
    pub name: String,
    pub description: String,
    pub quarantined: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Tool,
    Skill,
    ExternalAgent,
    SourceProvider,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionManifest {
    pub shell: bool,
    pub file_read: bool,
    pub file_write: bool,
    pub network: bool,
    pub secrets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticScanFinding {
    pub severity: FindingSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Info,
    Warning,
    High,
}

pub fn inspect_source(source: impl AsRef<Path>) -> Result<NormalizedPackage, AdapterError> {
    let source = source.as_ref();
    let bytes = read_for_digest(source)?;
    let text = String::from_utf8_lossy(&bytes);
    let adapter = detect_adapter(source, &text);
    let permissions = scan_permissions(&text);
    let mut findings = Vec::new();
    if permissions.shell || permissions.file_write || permissions.network || permissions.secrets {
        findings.push(StaticScanFinding {
            severity: FindingSeverity::Warning,
            message: "import requests sensitive capabilities and must stay quarantined".into(),
        });
    }
    if text
        .to_ascii_lowercase()
        .contains("ignore previous instructions")
    {
        findings.push(StaticScanFinding {
            severity: FindingSeverity::High,
            message: "possible prompt injection phrase detected".into(),
        });
    }

    Ok(NormalizedPackage {
        id: format!("adapter-{}", digest(&bytes)),
        source: source.to_path_buf(),
        adapter,
        digest: digest(&bytes),
        quarantined: true,
        capabilities: capabilities_for(adapter, source, &text),
        permissions,
        findings,
    })
}

pub struct AdapterRegistry {
    paths: StoragePaths,
}

impl AdapterRegistry {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn import(&self, source: impl AsRef<Path>) -> Result<NormalizedPackage, AdapterError> {
        self.paths.ensure_base_dirs()?;
        let package = inspect_source(source)?;
        self.write(&package)?;
        Ok(package)
    }

    pub fn list(&self) -> Result<Vec<NormalizedPackage>, AdapterError> {
        self.paths.ensure_base_dirs()?;
        let mut packages = Vec::new();
        for entry in std::fs::read_dir(self.paths.adapters_dir())? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                packages.push(read_package(entry.path())?);
            }
        }
        packages.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(packages)
    }

    pub fn show(&self, id: &str) -> Result<NormalizedPackage, AdapterError> {
        let path = self.path_for(id);
        if !path.exists() {
            return Err(AdapterError::NotFound(id.into()));
        }
        read_package(path)
    }

    pub fn allow(&self, id: &str) -> Result<NormalizedPackage, AdapterError> {
        let mut package = self.show(id)?;
        package.quarantined = false;
        for capability in &mut package.capabilities {
            capability.quarantined = false;
        }
        self.write(&package)?;
        Ok(package)
    }

    pub fn quarantine(&self, id: &str) -> Result<NormalizedPackage, AdapterError> {
        let mut package = self.show(id)?;
        package.quarantined = true;
        for capability in &mut package.capabilities {
            capability.quarantined = true;
        }
        self.write(&package)?;
        Ok(package)
    }

    fn write(&self, package: &NormalizedPackage) -> Result<(), AdapterError> {
        self.paths.ensure_base_dirs()?;
        std::fs::write(
            self.path_for(&package.id),
            serde_json::to_string_pretty(package)?,
        )?;
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.paths.adapters_dir().join(format!("{id}.json"))
    }
}

fn read_package(path: impl AsRef<Path>) -> Result<NormalizedPackage, AdapterError> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn detect_adapter(source: &Path, text: &str) -> AdapterKind {
    let file_name = source.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if source.is_dir() && source.join("SKILL.md").exists() || file_name == "SKILL.md" {
        AdapterKind::OpenClawAgentSkills
    } else if file_name == "plugin.yaml" || file_name == "plugin.yml" {
        AdapterKind::HermesPlugin
    } else if file_name == "mcp.json" || text.contains("\"mcpServers\"") {
        AdapterKind::Mcp
    } else if text.contains("clawhub") {
        AdapterKind::ClawHub
    } else if text.contains("a2a") {
        AdapterKind::A2a
    } else {
        AdapterKind::Unknown
    }
}

fn capabilities_for(adapter: AdapterKind, source: &Path, text: &str) -> Vec<NormalizedCapability> {
    let name = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported-capability")
        .to_string();
    let kind = match adapter {
        AdapterKind::OpenClawAgentSkills => CapabilityKind::Skill,
        AdapterKind::HermesExternalAgent | AdapterKind::A2a => CapabilityKind::ExternalAgent,
        AdapterKind::ClawHub => CapabilityKind::SourceProvider,
        AdapterKind::Mcp | AdapterKind::HermesPlugin | AdapterKind::Unknown => CapabilityKind::Tool,
    };
    let description = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("Imported capability.")
        .chars()
        .take(240)
        .collect();
    vec![NormalizedCapability {
        id: slugify(&name),
        kind,
        name,
        description,
        quarantined: true,
    }]
}

fn scan_permissions(text: &str) -> PermissionManifest {
    let lower = text.to_ascii_lowercase();
    PermissionManifest {
        shell: lower.contains("shell") || lower.contains("terminal"),
        file_read: lower.contains("file_read") || lower.contains("read file"),
        file_write: lower.contains("file_write") || lower.contains("write file"),
        network: lower.contains("http") || lower.contains("network"),
        secrets: lower.contains("secret") || lower.contains("api_key"),
    }
}

fn read_for_digest(source: &Path) -> Result<Vec<u8>, AdapterError> {
    if source.is_dir() {
        let skill = source.join("SKILL.md");
        if skill.exists() {
            return Ok(std::fs::read(skill)?);
        }
        return Ok(source.display().to_string().into_bytes());
    }
    Ok(std::fs::read(source)?)
}

fn digest(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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
    fn openclaw_skill_is_detected_and_quarantined() {
        let dir = std::env::temp_dir().join(format!("adapter-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "# Demo\nUse shell with caution.").unwrap();
        let package = inspect_source(&dir).unwrap();
        assert!(package.id.starts_with("adapter-"));
        assert_eq!(package.adapter, AdapterKind::OpenClawAgentSkills);
        assert!(package.quarantined);
        assert!(package.capabilities[0].quarantined);
        assert!(package.permissions.shell);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn registry_import_allow_and_quarantine_round_trip() {
        let dir = std::env::temp_dir().join(format!("adapter-reg-test-{}", std::process::id()));
        let source = dir.join("skill");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "# Demo\nUse inspection.").unwrap();
        let registry = AdapterRegistry::new(StoragePaths::new(dir.join("home")));
        let package = registry.import(&source).unwrap();
        assert!(package.quarantined);
        assert_eq!(registry.list().unwrap().len(), 1);
        assert!(!registry.allow(&package.id).unwrap().quarantined);
        assert!(registry.quarantine(&package.id).unwrap().quarantined);
        let _ = std::fs::remove_dir_all(dir);
    }
}
