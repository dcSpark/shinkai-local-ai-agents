//! `agent-secrets` — opaque secret handles for tool execution.
//!
//! This crate intentionally exposes handles and metadata separately from
//! secret values. The file-backed store is a local development backend so the
//! harness has concrete lifecycle semantics while OS keychain integrations
//! land behind the same API.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use agent_storage::StoragePaths;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum SecretStoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("secret not found: {0}")]
    NotFound(String),
    #[error("invalid secret id: {0}")]
    InvalidId(String),
    #[error("secret handle version {version} not found for {id}")]
    VersionNotFound { id: String, version: u64 },
    #[error("secret backend unsupported: {0}")]
    UnsupportedBackend(String),
    #[error("secret backend command failed: {0}")]
    BackendCommand(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SecretId(pub String);

impl SecretId {
    pub fn new(value: impl Into<String>) -> Result<Self, SecretStoreError> {
        let value = value.into();
        if is_valid_secret_id(&value) {
            Ok(Self(value))
        } else {
            Err(SecretStoreError::InvalidId(value))
        }
    }
}

impl AsRef<str> for SecretId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretHandle {
    pub id: SecretId,
    pub version: u64,
}

impl std::fmt::Display for SecretHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "secret://{}@{}", self.id.0, self.version)
    }
}

impl FromStr for SecretHandle {
    type Err = SecretStoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value
            .trim()
            .strip_prefix("secret://")
            .ok_or_else(|| SecretStoreError::InvalidId(value.into()))?;
        let (id, version) = value
            .rsplit_once('@')
            .ok_or_else(|| SecretStoreError::InvalidId(value.into()))?;
        let version = version
            .parse::<u64>()
            .map_err(|_| SecretStoreError::InvalidId(value.into()))?;
        Ok(Self {
            id: SecretId::new(id)?,
            version,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRecord {
    pub id: SecretId,
    pub label: Option<String>,
    pub current_version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub backend: String,
    pub value_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretBackendDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub supported: bool,
    pub active: bool,
}

pub trait SecretStore: Send + Sync {
    fn set(
        &self,
        id: SecretId,
        value: SecretValue,
        label: Option<String>,
    ) -> Result<SecretHandle, SecretStoreError>;
    fn rotate(&self, id: &SecretId, value: SecretValue) -> Result<SecretHandle, SecretStoreError>;
    fn resolve(&self, handle: &SecretHandle) -> Result<SecretValue, SecretStoreError>;
    fn list(&self) -> Result<Vec<SecretRecord>, SecretStoreError>;
    fn show(&self, id: &SecretId) -> Result<SecretRecord, SecretStoreError>;
    fn delete(&self, id: &SecretId) -> Result<bool, SecretStoreError>;
}

pub fn default_secret_store() -> Arc<dyn SecretStore> {
    match std::env::var("AGENT_SECRET_BACKEND")
        .unwrap_or_else(|_| "file_dev".into())
        .trim()
    {
        "os_keychain" | "keychain" => Arc::new(KeychainSecretStore::from_env()),
        _ => Arc::new(FileSecretStore::from_env()),
    }
}

pub fn supported_backends() -> Vec<SecretBackendDescriptor> {
    let active = std::env::var("AGENT_SECRET_BACKEND").unwrap_or_else(|_| "file_dev".into());
    vec![
        SecretBackendDescriptor {
            id: "file_dev".into(),
            name: "File development store".into(),
            description: "Local JSON metadata and values under the active profile.".into(),
            supported: true,
            active: active != "os_keychain" && active != "keychain",
        },
        SecretBackendDescriptor {
            id: "os_keychain".into(),
            name: "OS keychain".into(),
            description:
                "Stores secret values in the platform keychain and metadata in the active profile."
                    .into(),
            supported: cfg!(target_os = "macos"),
            active: active == "os_keychain" || active == "keychain",
        },
    ]
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

pub struct FileSecretStore {
    path: PathBuf,
}

pub struct KeychainSecretStore {
    metadata_path: PathBuf,
    service: String,
}

impl KeychainSecretStore {
    pub fn new(metadata_path: impl Into<PathBuf>, service: impl Into<String>) -> Self {
        Self {
            metadata_path: metadata_path.into(),
            service: service.into(),
        }
    }

    pub fn from_env() -> Self {
        let metadata_path = StoragePaths::from_env()
            .secrets_file()
            .with_file_name("secrets-keychain.json");
        let service = std::env::var("AGENT_SECRET_KEYCHAIN_SERVICE")
            .unwrap_or_else(|_| "shinkai-agent-harness".into());
        Self::new(metadata_path, service)
    }

    pub fn metadata_path(&self) -> &PathBuf {
        &self.metadata_path
    }

    fn read_document(&self) -> Result<SecretDocument, SecretStoreError> {
        match std::fs::read_to_string(&self.metadata_path) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(SecretDocument::default()),
            Err(err) => Err(err.into()),
        }
    }

    fn write_document(&self, document: &SecretDocument) -> Result<(), SecretStoreError> {
        if let Some(parent) = self.metadata_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.metadata_path, serde_json::to_string_pretty(document)?)?;
        Ok(())
    }

    fn account(&self, id: &SecretId, version: u64) -> String {
        format!("{}:{}@{}", self.service, id.0, version)
    }
}

impl FileSecretStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env().secrets_file())
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    fn read_document(&self) -> Result<SecretDocument, SecretStoreError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(SecretDocument::default()),
            Err(err) => Err(err.into()),
        }
    }

    fn write_document(&self, document: &SecretDocument) -> Result<(), SecretStoreError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(document)?;
        std::fs::write(&self.path, text)?;
        Ok(())
    }
}

impl SecretStore for KeychainSecretStore {
    fn set(
        &self,
        id: SecretId,
        value: SecretValue,
        label: Option<String>,
    ) -> Result<SecretHandle, SecretStoreError> {
        let mut document = self.read_document()?;
        let now = Utc::now();
        let entry = match document.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.label = label.or_else(|| entry.label.clone());
                entry.updated_at = now;
                entry
            }
            None => {
                document.entries.push(StoredSecret {
                    id: id.clone(),
                    label,
                    created_at: now,
                    updated_at: now,
                    versions: Vec::new(),
                });
                document.entries.last_mut().expect("entry was just pushed")
            }
        };
        let version = entry.current_version().saturating_add(1).max(1);
        let account = self.account(&id, version);
        keychain_set(&self.service, &account, value.expose())?;
        entry.versions.push(SecretVersion {
            version,
            value: account,
            created_at: now,
            value_fingerprint: fingerprint(value.expose()),
        });
        self.write_document(&document)?;
        Ok(SecretHandle { id, version })
    }

    fn rotate(&self, id: &SecretId, value: SecretValue) -> Result<SecretHandle, SecretStoreError> {
        let mut document = self.read_document()?;
        let entry = document
            .entries
            .iter_mut()
            .find(|entry| &entry.id == id)
            .ok_or_else(|| SecretStoreError::NotFound(id.0.clone()))?;
        let now = Utc::now();
        entry.updated_at = now;
        let version = entry.current_version().saturating_add(1).max(1);
        let account = self.account(id, version);
        keychain_set(&self.service, &account, value.expose())?;
        entry.versions.push(SecretVersion {
            version,
            value: account,
            created_at: now,
            value_fingerprint: fingerprint(value.expose()),
        });
        self.write_document(&document)?;
        Ok(SecretHandle {
            id: id.clone(),
            version,
        })
    }

    fn resolve(&self, handle: &SecretHandle) -> Result<SecretValue, SecretStoreError> {
        let document = self.read_document()?;
        let entry = document
            .entries
            .iter()
            .find(|entry| entry.id == handle.id)
            .ok_or_else(|| SecretStoreError::NotFound(handle.id.0.clone()))?;
        let version = entry
            .versions
            .iter()
            .find(|version| version.version == handle.version)
            .ok_or_else(|| SecretStoreError::VersionNotFound {
                id: handle.id.0.clone(),
                version: handle.version,
            })?;
        keychain_get(&self.service, &version.value).map(SecretValue::new)
    }

    fn list(&self) -> Result<Vec<SecretRecord>, SecretStoreError> {
        let mut records: Vec<SecretRecord> = self
            .read_document()?
            .entries
            .iter()
            .filter_map(|entry| secret_record_with_backend(entry, "os_keychain"))
            .collect();
        records.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        Ok(records)
    }

    fn show(&self, id: &SecretId) -> Result<SecretRecord, SecretStoreError> {
        self.read_document()?
            .entries
            .iter()
            .find(|entry| &entry.id == id)
            .and_then(|entry| secret_record_with_backend(entry, "os_keychain"))
            .ok_or_else(|| SecretStoreError::NotFound(id.0.clone()))
    }

    fn delete(&self, id: &SecretId) -> Result<bool, SecretStoreError> {
        let mut document = self.read_document()?;
        let mut deleted_accounts = Vec::new();
        let before = document.entries.len();
        document.entries.retain(|entry| {
            if &entry.id == id {
                deleted_accounts.extend(entry.versions.iter().map(|version| version.value.clone()));
                false
            } else {
                true
            }
        });
        let deleted = document.entries.len() != before;
        if deleted {
            for account in deleted_accounts {
                let _ = keychain_delete(&self.service, &account);
            }
            self.write_document(&document)?;
        }
        Ok(deleted)
    }
}

impl SecretStore for FileSecretStore {
    fn set(
        &self,
        id: SecretId,
        value: SecretValue,
        label: Option<String>,
    ) -> Result<SecretHandle, SecretStoreError> {
        let mut document = self.read_document()?;
        let now = Utc::now();
        let entry = match document.entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                entry.label = label.or_else(|| entry.label.clone());
                entry.updated_at = now;
                entry
            }
            None => {
                document.entries.push(StoredSecret {
                    id: id.clone(),
                    label,
                    created_at: now,
                    updated_at: now,
                    versions: Vec::new(),
                });
                document.entries.last_mut().expect("entry was just pushed")
            }
        };
        let version = entry.current_version().saturating_add(1).max(1);
        entry.versions.push(SecretVersion {
            version,
            value: value.expose().to_string(),
            created_at: now,
            value_fingerprint: fingerprint(value.expose()),
        });
        self.write_document(&document)?;
        Ok(SecretHandle { id, version })
    }

    fn rotate(&self, id: &SecretId, value: SecretValue) -> Result<SecretHandle, SecretStoreError> {
        let mut document = self.read_document()?;
        let entry = document
            .entries
            .iter_mut()
            .find(|entry| &entry.id == id)
            .ok_or_else(|| SecretStoreError::NotFound(id.0.clone()))?;
        let now = Utc::now();
        entry.updated_at = now;
        let version = entry.current_version().saturating_add(1).max(1);
        entry.versions.push(SecretVersion {
            version,
            value: value.expose().to_string(),
            created_at: now,
            value_fingerprint: fingerprint(value.expose()),
        });
        self.write_document(&document)?;
        Ok(SecretHandle {
            id: id.clone(),
            version,
        })
    }

    fn resolve(&self, handle: &SecretHandle) -> Result<SecretValue, SecretStoreError> {
        let document = self.read_document()?;
        let entry = document
            .entries
            .iter()
            .find(|entry| entry.id == handle.id)
            .ok_or_else(|| SecretStoreError::NotFound(handle.id.0.clone()))?;
        let version = entry
            .versions
            .iter()
            .find(|version| version.version == handle.version)
            .ok_or_else(|| SecretStoreError::VersionNotFound {
                id: handle.id.0.clone(),
                version: handle.version,
            })?;
        Ok(SecretValue::new(version.value.clone()))
    }

    fn list(&self) -> Result<Vec<SecretRecord>, SecretStoreError> {
        let mut records: Vec<SecretRecord> = self
            .read_document()?
            .entries
            .iter()
            .filter_map(secret_record)
            .collect();
        records.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        Ok(records)
    }

    fn show(&self, id: &SecretId) -> Result<SecretRecord, SecretStoreError> {
        self.read_document()?
            .entries
            .iter()
            .find(|entry| &entry.id == id)
            .and_then(secret_record)
            .ok_or_else(|| SecretStoreError::NotFound(id.0.clone()))
    }

    fn delete(&self, id: &SecretId) -> Result<bool, SecretStoreError> {
        let mut document = self.read_document()?;
        let before = document.entries.len();
        document.entries.retain(|entry| &entry.id != id);
        let deleted = document.entries.len() != before;
        if deleted {
            self.write_document(&document)?;
        }
        Ok(deleted)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SecretDocument {
    #[serde(default)]
    entries: Vec<StoredSecret>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSecret {
    id: SecretId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    versions: Vec<SecretVersion>,
}

impl StoredSecret {
    fn current_version(&self) -> u64 {
        self.versions
            .iter()
            .map(|version| version.version)
            .max()
            .unwrap_or(0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SecretVersion {
    version: u64,
    value: String,
    created_at: DateTime<Utc>,
    value_fingerprint: String,
}

fn secret_record(entry: &StoredSecret) -> Option<SecretRecord> {
    secret_record_with_backend(entry, "file_dev")
}

fn secret_record_with_backend(entry: &StoredSecret, backend: &str) -> Option<SecretRecord> {
    let version = entry
        .versions
        .iter()
        .max_by_key(|version| version.version)?;
    Some(SecretRecord {
        id: entry.id.clone(),
        label: entry.label.clone(),
        current_version: version.version,
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        backend: backend.into(),
        value_fingerprint: version.value_fingerprint.clone(),
    })
}

#[cfg(target_os = "macos")]
fn keychain_set(service: &str, account: &str, value: &str) -> Result<(), SecretStoreError> {
    let status = Command::new("security")
        .args([
            "add-generic-password",
            "-a",
            account,
            "-s",
            service,
            "-w",
            value,
            "-U",
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(SecretStoreError::BackendCommand(format!(
            "security add-generic-password exited with {status}"
        )))
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain_set(_service: &str, _account: &str, _value: &str) -> Result<(), SecretStoreError> {
    Err(SecretStoreError::UnsupportedBackend(
        "os_keychain is only implemented for macOS in this build".into(),
    ))
}

#[cfg(target_os = "macos")]
fn keychain_get(service: &str, account: &str) -> Result<String, SecretStoreError> {
    let output = Command::new("security")
        .args(["find-generic-password", "-a", account, "-s", service, "-w"])
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\r', '\n'])
            .to_string())
    } else {
        Err(SecretStoreError::NotFound(account.into()))
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain_get(_service: &str, account: &str) -> Result<String, SecretStoreError> {
    Err(SecretStoreError::NotFound(account.into()))
}

#[cfg(target_os = "macos")]
fn keychain_delete(service: &str, account: &str) -> Result<(), SecretStoreError> {
    let status = Command::new("security")
        .args(["delete-generic-password", "-a", account, "-s", service])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(SecretStoreError::NotFound(account.into()))
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain_delete(_service: &str, account: &str) -> Result<(), SecretStoreError> {
    Err(SecretStoreError::NotFound(account.into()))
}

fn fingerprint(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hex.chars().take(16).collect()
}

fn is_valid_secret_id(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(label: &str) -> FileSecretStore {
        let path = std::env::temp_dir().join(format!(
            "agent-secrets-{label}-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        FileSecretStore::new(path)
    }

    #[test]
    fn file_store_sets_lists_resolves_and_deletes_secret_handles() {
        let store = temp_store("lifecycle");
        let id = SecretId::new("openai.api_key").unwrap();

        let first = store
            .set(
                id.clone(),
                SecretValue::new("sk-first"),
                Some("OpenAI".into()),
            )
            .unwrap();
        let second = store.rotate(&id, SecretValue::new("sk-second")).unwrap();

        assert_eq!(first.version, 1);
        assert_eq!(second.version, 2);
        assert_eq!(store.resolve(&first).unwrap().expose(), "sk-first");
        assert_eq!(store.resolve(&second).unwrap().expose(), "sk-second");
        assert_eq!(second.to_string(), "secret://openai.api_key@2");
        assert_eq!(second.to_string().parse::<SecretHandle>().unwrap(), second);

        let records = store.list().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, id);
        assert_eq!(records[0].label.as_deref(), Some("OpenAI"));
        assert_eq!(records[0].current_version, 2);
        assert_eq!(records[0].backend, "file_dev");
        assert_ne!(records[0].value_fingerprint, "sk-second");

        assert!(
            store
                .delete(&SecretId::new("openai.api_key").unwrap())
                .unwrap()
        );
        assert!(store.list().unwrap().is_empty());
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn invalid_ids_are_rejected() {
        assert!(matches!(
            SecretId::new("../secret"),
            Err(SecretStoreError::InvalidId(_))
        ));
        assert!(matches!(
            SecretId::new(""),
            Err(SecretStoreError::InvalidId(_))
        ));
    }

    #[test]
    fn supported_secret_backends_are_described() {
        let backends = supported_backends();
        let ids = backends
            .iter()
            .map(|backend| backend.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["file_dev", "os_keychain"]);
        assert!(backends[0].supported);
    }

    #[test]
    fn keychain_store_uses_separate_metadata_file() {
        let path = std::env::temp_dir().join(format!(
            "agent-secrets-keychain-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = KeychainSecretStore::new(&path, "test-service");

        assert_eq!(store.metadata_path(), &path);
    }
}
