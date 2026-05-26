//! `agent-bundles` — schema-versioned import/export tarballs.
//!
//! Bundles intentionally mirror the filesystem layout from `agent-storage`:
//! TOML config, Markdown memory, JSON skills, and JSON ingestion artifacts.

use std::fs::File;
use std::path::{Component, Path, PathBuf};

use agent_storage::StoragePaths;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const BUNDLE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] agent_storage::StorageError),
    #[error("toml serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("toml parse error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("unsupported bundle schema_version {0}")]
    UnsupportedSchema(u32),
    #[error("unsafe bundle path: {0}")]
    UnsafePath(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub schema_version: u32,
    pub exported_at: DateTime<Utc>,
    pub profile: String,
    #[serde(default)]
    pub credential_reminders: Vec<BundleCredentialReminder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleCredentialReminder {
    pub path: String,
    pub reason: String,
}

pub fn export_bundle(destination: impl AsRef<Path>) -> Result<BundleManifest, BundleError> {
    let paths = StoragePaths::from_env();
    export_bundle_from(paths, destination)
}

pub fn import_bundle(source: impl AsRef<Path>) -> Result<BundleManifest, BundleError> {
    let paths = StoragePaths::from_env();
    import_bundle_into(paths, source)
}

pub fn export_bundle_from(
    paths: StoragePaths,
    destination: impl AsRef<Path>,
) -> Result<BundleManifest, BundleError> {
    paths.ensure_base_dirs()?;
    let mut credential_reminders = Vec::new();
    collect_credential_reminders(
        paths.profiles_dir(),
        Path::new("profiles"),
        &mut credential_reminders,
    )?;
    collect_credential_reminders(
        paths.cache_dir(),
        Path::new("cache"),
        &mut credential_reminders,
    )?;
    collect_external_memory_credential_reminders(
        paths.global_config(),
        Path::new("config.toml"),
        &mut credential_reminders,
    )?;
    collect_external_memory_credential_reminders(
        paths.profiles_dir(),
        Path::new("profiles"),
        &mut credential_reminders,
    )?;
    collect_adapter_secret_credential_reminders(
        paths.profiles_dir(),
        Path::new("profiles"),
        &mut credential_reminders,
    )?;
    credential_reminders.sort_by(|left, right| left.path.cmp(&right.path));
    let manifest = BundleManifest {
        schema_version: BUNDLE_SCHEMA_VERSION,
        exported_at: Utc::now(),
        profile: paths.active_profile_id().into(),
        credential_reminders,
    };

    let file = File::create(destination)?;
    let mut builder = tar::Builder::new(file);
    let manifest_text = toml::to_string_pretty(&manifest)?;
    append_bytes(
        &mut builder,
        Path::new("manifest.toml"),
        manifest_text.as_bytes(),
    )?;
    append_path_if_exists(
        &mut builder,
        paths.global_config(),
        Path::new("config.toml"),
    )?;
    append_dir_if_exists(&mut builder, paths.profiles_dir(), Path::new("profiles"))?;
    append_dir_if_exists(&mut builder, paths.cache_dir(), Path::new("cache"))?;
    builder.finish()?;
    Ok(manifest)
}

pub fn import_bundle_into(
    paths: StoragePaths,
    source: impl AsRef<Path>,
) -> Result<BundleManifest, BundleError> {
    paths.ensure_base_dirs()?;
    let file = File::open(source)?;
    let mut archive = tar::Archive::new(file);
    let mut manifest = None;
    let mut credential_reminders = Vec::new();
    let mut imported_adapter_manifests = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_path = entry.path()?.into_owned();
        ensure_safe_relative(&entry_path)?;
        let entry_type = entry.header().entry_type();
        if entry_path == Path::new("manifest.toml") {
            if !entry_type.is_file() {
                return Err(BundleError::UnsafePath(
                    "manifest.toml must be a regular file".into(),
                ));
            }
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text)?;
            let parsed: BundleManifest = toml::from_str(&text)?;
            if parsed.schema_version != BUNDLE_SCHEMA_VERSION {
                return Err(BundleError::UnsupportedSchema(parsed.schema_version));
            }
            manifest = Some(parsed);
            continue;
        }
        if is_credential_bundle_path(&entry_path) {
            credential_reminders.push(credential_reminder(&entry_path));
            continue;
        }
        let out = paths.root().join(&entry_path);
        if entry_type.is_dir() {
            std::fs::create_dir_all(out)?;
        } else if entry_type.is_file() {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            paths.ensure_quota_for_path_write(&out, entry.header().size()?)?;
            entry.unpack(&out)?;
            if looks_like_adapter_manifest_path(&entry_path) {
                imported_adapter_manifests.push((out, entry_path));
            }
        } else {
            return Err(BundleError::UnsafePath(format!(
                "unsupported bundle entry type at {}",
                entry_path.display()
            )));
        }
    }

    let mut manifest =
        manifest.ok_or_else(|| BundleError::UnsafePath("missing manifest.toml".into()))?;
    for (source, archive_path) in imported_adapter_manifests {
        collect_adapter_secret_credential_reminders_from_file(
            &source,
            &archive_path,
            &mut credential_reminders,
        )?;
    }
    merge_credential_reminders(&mut manifest, credential_reminders);
    Ok(manifest)
}

fn append_bytes(
    builder: &mut tar::Builder<File>,
    path: &Path,
    bytes: &[u8],
) -> Result<(), BundleError> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes)?;
    Ok(())
}

fn append_path_if_exists(
    builder: &mut tar::Builder<File>,
    source: PathBuf,
    archive_path: &Path,
) -> Result<(), BundleError> {
    if source.exists() {
        builder.append_path_with_name(source, archive_path)?;
    }
    Ok(())
}

fn append_dir_if_exists(
    builder: &mut tar::Builder<File>,
    source: PathBuf,
    archive_path: &Path,
) -> Result<(), BundleError> {
    if source.exists() {
        append_dir_filtered(builder, &source, archive_path)?;
    }
    Ok(())
}

fn append_dir_filtered(
    builder: &mut tar::Builder<File>,
    source: &Path,
    archive_path: &Path,
) -> Result<(), BundleError> {
    builder.append_dir(archive_path, source)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let entry_source = entry.path();
        let entry_archive_path = archive_path.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            append_dir_filtered(builder, &entry_source, &entry_archive_path)?;
        } else if file_type.is_file() {
            if is_credential_bundle_path(&entry_archive_path) {
                continue;
            }
            builder.append_path_with_name(entry_source, entry_archive_path)?;
        } else {
            return Err(BundleError::UnsafePath(format!(
                "unsupported bundle source entry: {}",
                entry_source.display()
            )));
        }
    }
    Ok(())
}

fn collect_credential_reminders(
    source: PathBuf,
    archive_path: &Path,
    reminders: &mut Vec<BundleCredentialReminder>,
) -> Result<(), BundleError> {
    if !source.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let entry_source = entry.path();
        let entry_archive_path = archive_path.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_credential_reminders(entry_source, &entry_archive_path, reminders)?;
        } else if file_type.is_file() && is_credential_bundle_path(&entry_archive_path) {
            reminders.push(credential_reminder(&entry_archive_path));
        }
    }
    Ok(())
}

fn credential_reminder(path: &Path) -> BundleCredentialReminder {
    BundleCredentialReminder {
        path: archive_path_label(path),
        reason: "secret credential file omitted; recreate matching secrets after import".into(),
    }
}

fn collect_external_memory_credential_reminders(
    source: PathBuf,
    archive_path: &Path,
    reminders: &mut Vec<BundleCredentialReminder>,
) -> Result<(), BundleError> {
    if !source.exists() {
        return Ok(());
    }
    if source.is_dir() {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            collect_external_memory_credential_reminders(
                entry.path(),
                &archive_path.join(entry.file_name()),
                reminders,
            )?;
        }
        return Ok(());
    }
    if !source.is_file() || !looks_like_toml_config(archive_path) {
        return Ok(());
    }
    let text = std::fs::read_to_string(source)?;
    if text.contains("external-command-v0") {
        reminders.extend(external_command_memory_reminders(archive_path));
    }
    if text.contains("external-http-v0") {
        reminders.extend(external_http_memory_reminders(archive_path));
    }
    Ok(())
}

fn looks_like_toml_config(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
}

fn external_command_memory_reminders(path: &Path) -> Vec<BundleCredentialReminder> {
    let location = archive_path_label(path);
    [
        (
            "env:AGENT_MEMORY_EXTERNAL_COMMAND",
            "external-command-v0 memory backend configured in {location}; set the adapter executable after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_ARGS_JSON",
            "external-command-v0 memory backend configured in {location}; recreate optional adapter args after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_TIMEOUT_MS",
            "external-command-v0 memory backend configured in {location}; recreate optional adapter timeout after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_ENABLE_WRITES",
            "external-command-v0 memory backend configured in {location}; recreate optional external write enablement after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_ENABLE_ROLLBACK",
            "external-command-v0 memory backend configured in {location}; recreate optional external rollback enablement after import",
        ),
    ]
    .into_iter()
    .map(|(path, reason)| BundleCredentialReminder {
        path: path.into(),
        reason: reason.replace("{location}", &location),
    })
    .collect()
}

fn external_http_memory_reminders(path: &Path) -> Vec<BundleCredentialReminder> {
    let location = archive_path_label(path);
    [
        (
            "env:AGENT_MEMORY_EXTERNAL_HTTP_URL",
            "external-http-v0 memory backend configured in {location}; set the memory service endpoint after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_HTTP_BEARER_TOKEN",
            "external-http-v0 memory backend configured in {location}; recreate optional bearer token after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_HTTP_TIMEOUT_MS",
            "external-http-v0 memory backend configured in {location}; recreate optional adapter timeout after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_WRITES",
            "external-http-v0 memory backend configured in {location}; recreate optional external write enablement after import",
        ),
        (
            "env:AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_ROLLBACK",
            "external-http-v0 memory backend configured in {location}; recreate optional external rollback enablement after import",
        ),
    ]
    .into_iter()
    .map(|(path, reason)| BundleCredentialReminder {
        path: path.into(),
        reason: reason.replace("{location}", &location),
    })
    .collect()
}

fn collect_adapter_secret_credential_reminders(
    source: PathBuf,
    archive_path: &Path,
    reminders: &mut Vec<BundleCredentialReminder>,
) -> Result<(), BundleError> {
    if !source.exists() {
        return Ok(());
    }
    if source.is_dir() {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            collect_adapter_secret_credential_reminders(
                entry.path(),
                &archive_path.join(entry.file_name()),
                reminders,
            )?;
        }
        return Ok(());
    }
    if !source.is_file() {
        return Ok(());
    }
    collect_adapter_secret_credential_reminders_from_file(&source, archive_path, reminders)
}

fn collect_adapter_secret_credential_reminders_from_file(
    source: &Path,
    archive_path: &Path,
    reminders: &mut Vec<BundleCredentialReminder>,
) -> Result<(), BundleError> {
    if !looks_like_adapter_manifest_path(archive_path) {
        return Ok(());
    }
    let text = std::fs::read_to_string(source)?;
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    reminders.extend(adapter_secret_reminders(archive_path, &value));
    Ok(())
}

fn looks_like_adapter_manifest_path(path: &Path) -> bool {
    let is_json = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    is_json
        && path.components().any(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|part| part == "adapters")
        })
}

fn adapter_secret_reminders(
    path: &Path,
    value: &serde_json::Value,
) -> Vec<BundleCredentialReminder> {
    let Some(requirements) = value
        .get("secret_requirements")
        .and_then(|requirements| requirements.as_array())
    else {
        return Vec::new();
    };
    let location = archive_path_label(path);
    requirements
        .iter()
        .filter_map(|requirement| adapter_secret_reminder(&location, requirement))
        .collect()
}

fn adapter_secret_reminder(
    location: &str,
    requirement: &serde_json::Value,
) -> Option<BundleCredentialReminder> {
    let name = requirement
        .get("name")
        .and_then(|name| name.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())?;
    let source = requirement
        .get("source")
        .and_then(|source| source.as_str())
        .map(str::trim)
        .filter(|source| !source.is_empty())
        .unwrap_or("adapter");
    let required_label = if requirement
        .get("required")
        .and_then(|required| required.as_bool())
        .unwrap_or(true)
    {
        "required"
    } else {
        "optional"
    };

    Some(BundleCredentialReminder {
        path: format!("{location}#secret:{source}:{name}"),
        reason: format!(
            "adapter manifest {location} declares {required_label} connector secret {name} from {source}; recreate the value after import"
        ),
    })
}

fn merge_credential_reminders(
    manifest: &mut BundleManifest,
    reminders: Vec<BundleCredentialReminder>,
) {
    manifest.credential_reminders.extend(reminders);
    manifest
        .credential_reminders
        .sort_by(|left, right| left.path.cmp(&right.path));
    manifest
        .credential_reminders
        .dedup_by(|left, right| left.path == right.path);
}

fn is_credential_bundle_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "secrets.json" | "secrets-keychain.json"))
}

fn archive_path_label(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            Component::CurDir => None,
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                Some(component.as_os_str().to_string_lossy().into_owned())
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn ensure_safe_relative(path: &Path) -> Result<(), BundleError> {
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(BundleError::UnsafePath(path.display().to_string()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_and_import_bundle_round_trips_layout() {
        let base = std::env::temp_dir().join(format!("bundle-test-{}", std::process::id()));
        let src = StoragePaths::new_with_profile(base.join("src"), "research");
        src.ensure_base_dirs().unwrap();
        std::fs::write(src.default_agent_config(), "id = \"fake-agent\"\n").unwrap();
        std::fs::write(src.memory_file(), "---\n{}\n---\nhello\n").unwrap();
        let bundle = base.join("bundle.tar");
        let manifest = export_bundle_from(src, &bundle).unwrap();
        assert_eq!(manifest.schema_version, BUNDLE_SCHEMA_VERSION);
        assert_eq!(manifest.profile, "research");
        assert!(manifest.credential_reminders.is_empty());

        let dst = StoragePaths::new_with_profile(base.join("dst"), "research");
        let imported = import_bundle_into(dst.clone(), &bundle).unwrap();
        assert_eq!(imported.schema_version, BUNDLE_SCHEMA_VERSION);
        assert_eq!(imported.profile, "research");
        assert!(dst.memory_file().exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn import_bundle_rejects_symlink_entries() {
        let base = std::env::temp_dir().join(format!("bundle-symlink-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let bundle = base.join("bundle.tar");
        let manifest = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            exported_at: Utc::now(),
            profile: "main".into(),
            credential_reminders: Vec::new(),
        };

        let file = File::create(&bundle).unwrap();
        let mut builder = tar::Builder::new(file);
        let manifest_text = toml::to_string_pretty(&manifest).unwrap();
        append_bytes(
            &mut builder,
            Path::new("manifest.toml"),
            manifest_text.as_bytes(),
        )
        .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header
            .set_path(Path::new("profiles/main/agents/link"))
            .unwrap();
        header.set_link_name(Path::new("../../outside")).unwrap();
        header.set_size(0);
        header.set_cksum();
        builder.append(&header, std::io::empty()).unwrap();
        builder.finish().unwrap();

        let dst = StoragePaths::new(base.join("dst"));
        let err = import_bundle_into(dst, &bundle).unwrap_err();
        assert!(matches!(err, BundleError::UnsafePath(_)));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn export_bundle_omits_secret_files_and_records_reminders() {
        let base = std::env::temp_dir().join(format!(
            "bundle-secret-redaction-test-{}",
            std::process::id()
        ));
        let src = StoragePaths::new_with_profile(base.join("src"), "main");
        src.ensure_base_dirs().unwrap();
        std::fs::write(
            src.secrets_file(),
            r#"{"entries":[{"id":"api.key","versions":[{"version":1,"value":"sk-secret"}]}]}"#,
        )
        .unwrap();
        let bundle = base.join("bundle.tar");

        let manifest = export_bundle_from(src.clone(), &bundle).unwrap();

        assert_eq!(manifest.credential_reminders.len(), 1);
        assert_eq!(
            manifest.credential_reminders[0].path,
            "profiles/main/secrets.json"
        );

        let file = File::open(&bundle).unwrap();
        let mut archive = tar::Archive::new(file);
        let mut entry_names = Vec::new();
        let mut contents = String::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            entry_names.push(entry.path().unwrap().display().to_string());
            if entry.header().entry_type().is_file() {
                let _ = std::io::Read::read_to_string(&mut entry, &mut contents);
            }
        }

        assert!(
            !entry_names
                .iter()
                .any(|name| name.ends_with("secrets.json"))
        );
        assert!(!contents.contains("sk-secret"));
        assert!(contents.contains("credential_reminders"));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn export_bundle_records_external_memory_env_reminders() {
        let base = std::env::temp_dir().join(format!(
            "bundle-external-memory-reminder-test-{}",
            std::process::id()
        ));
        let src = StoragePaths::new_with_profile(base.join("src"), "main");
        src.ensure_base_dirs().unwrap();
        std::fs::write(
            src.global_config(),
            "[policy]\nmemory_backend = \"external-command-v0\"\n",
        )
        .unwrap();
        std::fs::write(
            src.default_agent_config(),
            "id = \"fake-agent\"\nmemory_backend = \"external-http-v0\"\n",
        )
        .unwrap();
        let bundle = base.join("bundle.tar");

        let manifest = export_bundle_from(src, &bundle).unwrap();
        let reminder_paths = manifest
            .credential_reminders
            .iter()
            .map(|reminder| reminder.path.as_str())
            .collect::<Vec<_>>();

        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_COMMAND"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_ARGS_JSON"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_TIMEOUT_MS"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_ENABLE_WRITES"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_ENABLE_ROLLBACK"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_HTTP_URL"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_HTTP_BEARER_TOKEN"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_HTTP_TIMEOUT_MS"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_WRITES"));
        assert!(reminder_paths.contains(&"env:AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_ROLLBACK"));
        assert!(manifest.credential_reminders.iter().any(|reminder| {
            reminder.path == "env:AGENT_MEMORY_EXTERNAL_HTTP_URL"
                && reminder.reason.contains("fake-agent")
        }));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn export_bundle_records_adapter_secret_reminders() {
        let base = std::env::temp_dir().join(format!(
            "bundle-adapter-secret-reminder-test-{}",
            std::process::id()
        ));
        let src = StoragePaths::new_with_profile(base.join("src"), "main");
        src.ensure_base_dirs().unwrap();
        std::fs::create_dir_all(src.adapters_dir()).unwrap();
        std::fs::write(
            src.adapters_dir().join("adapter-search.json"),
            r#"{
                "secret_requirements": [
                    {
                        "name": "SEARCH_API_KEY",
                        "source": "mcp:env",
                        "required": true
                    },
                    {
                        "name": "Authorization",
                        "source": "mcp:header",
                        "required": false
                    }
                ]
            }"#,
        )
        .unwrap();
        let bundle = base.join("bundle.tar");

        let manifest = export_bundle_from(src, &bundle).unwrap();
        let reminder_paths = manifest
            .credential_reminders
            .iter()
            .map(|reminder| reminder.path.as_str())
            .collect::<Vec<_>>();

        assert!(
            reminder_paths.contains(
                &"profiles/main/adapters/adapter-search.json#secret:mcp:env:SEARCH_API_KEY"
            )
        );
        assert!(reminder_paths.contains(
            &"profiles/main/adapters/adapter-search.json#secret:mcp:header:Authorization"
        ));
        assert!(manifest.credential_reminders.iter().any(|reminder| {
            reminder.path
                == "profiles/main/adapters/adapter-search.json#secret:mcp:env:SEARCH_API_KEY"
                && reminder
                    .reason
                    .contains("required connector secret SEARCH_API_KEY")
                && reminder
                    .reason
                    .contains("profiles/main/adapters/adapter-search.json")
        }));
        assert!(manifest.credential_reminders.iter().any(|reminder| {
            reminder.path
                == "profiles/main/adapters/adapter-search.json#secret:mcp:header:Authorization"
                && reminder
                    .reason
                    .contains("optional connector secret Authorization")
        }));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn import_bundle_skips_secret_files_and_records_reminders() {
        let base =
            std::env::temp_dir().join(format!("bundle-secret-import-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let bundle = base.join("bundle.tar");
        let manifest = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            exported_at: Utc::now(),
            profile: "main".into(),
            credential_reminders: Vec::new(),
        };

        let file = File::create(&bundle).unwrap();
        let mut builder = tar::Builder::new(file);
        let manifest_text = toml::to_string_pretty(&manifest).unwrap();
        append_bytes(
            &mut builder,
            Path::new("manifest.toml"),
            manifest_text.as_bytes(),
        )
        .unwrap();
        append_bytes(
            &mut builder,
            Path::new("profiles/main/secrets.json"),
            br#"{"entries":[{"id":"api.key","versions":[{"version":1,"value":"sk-secret"}]}]}"#,
        )
        .unwrap();
        builder.finish().unwrap();

        let dst = StoragePaths::new_with_profile(base.join("dst"), "main");
        let imported = import_bundle_into(dst.clone(), &bundle).unwrap();

        assert!(!dst.secrets_file().exists());
        assert_eq!(imported.credential_reminders.len(), 1);
        assert_eq!(
            imported.credential_reminders[0].path,
            "profiles/main/secrets.json"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn import_bundle_records_adapter_secret_reminders() {
        let base = std::env::temp_dir().join(format!(
            "bundle-adapter-secret-import-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let bundle = base.join("bundle.tar");
        let manifest = BundleManifest {
            schema_version: BUNDLE_SCHEMA_VERSION,
            exported_at: Utc::now(),
            profile: "main".into(),
            credential_reminders: Vec::new(),
        };

        let file = File::create(&bundle).unwrap();
        let mut builder = tar::Builder::new(file);
        let manifest_text = toml::to_string_pretty(&manifest).unwrap();
        append_bytes(
            &mut builder,
            Path::new("manifest.toml"),
            manifest_text.as_bytes(),
        )
        .unwrap();
        append_bytes(
            &mut builder,
            Path::new("profiles/main/adapters/adapter-search.json"),
            br#"{
                "secret_requirements": [
                    {
                        "name": "SEARCH_API_KEY",
                        "source": "mcp:env",
                        "required": true
                    }
                ]
            }"#,
        )
        .unwrap();
        builder.finish().unwrap();

        let dst = StoragePaths::new_with_profile(base.join("dst"), "main");
        let imported = import_bundle_into(dst.clone(), &bundle).unwrap();

        assert!(dst.adapters_dir().join("adapter-search.json").exists());
        assert!(imported.credential_reminders.iter().any(|reminder| {
            reminder.path
                == "profiles/main/adapters/adapter-search.json#secret:mcp:env:SEARCH_API_KEY"
                && reminder
                    .reason
                    .contains("required connector secret SEARCH_API_KEY")
        }));
        let _ = std::fs::remove_dir_all(base);
    }
}
