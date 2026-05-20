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
    let manifest = BundleManifest {
        schema_version: BUNDLE_SCHEMA_VERSION,
        exported_at: Utc::now(),
        profile: paths.active_profile_id().into(),
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
        let out = paths.root().join(&entry_path);
        if entry_type.is_dir() {
            std::fs::create_dir_all(out)?;
        } else if entry_type.is_file() {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(out)?;
        } else {
            return Err(BundleError::UnsafePath(format!(
                "unsupported bundle entry type at {}",
                entry_path.display()
            )));
        }
    }

    manifest.ok_or_else(|| BundleError::UnsafePath("missing manifest.toml".into()))
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
        builder.append_dir_all(archive_path, source)?;
    }
    Ok(())
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
}
