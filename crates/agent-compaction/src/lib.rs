//! `agent-compaction` — portable manual context compaction v0.
//!
//! This crate stores user-triggered compaction artifacts as JSON under the
//! harness cache. The runtime can then include one of those artifacts as the
//! compacted context for a run without silently compacting conversations.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use agent_storage::{StorageError, StoragePaths};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 512;

#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("compaction not found: {0}")]
    NotFound(String),
    #[error("invalid compaction input: {0}")]
    InvalidInput(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionRecord {
    pub id: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    pub source: String,
    pub max_output_tokens: u32,
    pub original_input_hash: String,
    pub original_input_excerpt: String,
    pub created_at: DateTime<Utc>,
}

pub struct CompactionStore {
    paths: StoragePaths,
}

impl CompactionStore {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn create_from_text(
        &self,
        text: &str,
        guidance: Option<String>,
        max_output_tokens: Option<u32>,
        source: Option<String>,
    ) -> Result<CompactionRecord, CompactionError> {
        self.create_from_text_for_conversation(text, guidance, max_output_tokens, source, None)
    }

    pub fn create_from_text_for_conversation(
        &self,
        text: &str,
        guidance: Option<String>,
        max_output_tokens: Option<u32>,
        source: Option<String>,
        conversation_id: Option<String>,
    ) -> Result<CompactionRecord, CompactionError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(CompactionError::InvalidInput(
                "selected conversation text must not be empty".into(),
            ));
        }
        let max_output_tokens = max_output_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS);
        if max_output_tokens == 0 {
            return Err(CompactionError::InvalidInput(
                "max_output_tokens must be greater than zero".into(),
            ));
        }
        self.paths.ensure_base_dirs()?;
        let created_at = Utc::now();
        let conversation_id = clean_optional(conversation_id);
        if let Some(id) = conversation_id.as_deref() {
            validate_id(id).map_err(|_| {
                CompactionError::InvalidInput(format!("invalid conversation id: {id}"))
            })?;
        }
        let source = source
            .map(|source| source.trim().to_string())
            .filter(|source| !source.is_empty())
            .unwrap_or_else(|| {
                conversation_id
                    .as_ref()
                    .map(|id| format!("conversation:{id}"))
                    .unwrap_or_else(|| "manual-selection".into())
            });
        let guidance = guidance
            .map(|guidance| guidance.trim().to_string())
            .filter(|guidance| !guidance.is_empty());
        let original_input_hash = hash_text(text);
        let content = compact_text(text, guidance.as_deref(), &source, max_output_tokens);
        let record = CompactionRecord {
            id: format!(
                "compact-{}-{}",
                created_at.timestamp_nanos_opt().unwrap_or_default(),
                &original_input_hash[..8]
            ),
            content,
            guidance,
            conversation_id,
            source,
            max_output_tokens,
            original_input_hash,
            original_input_excerpt: excerpt(text, 500),
            created_at,
        };
        self.write(&record)?;
        Ok(record)
    }

    pub fn keep_compacted_context(
        &self,
        content: &str,
        guidance: Option<String>,
        max_output_tokens: Option<u32>,
        source: Option<String>,
        conversation_id: Option<String>,
    ) -> Result<CompactionRecord, CompactionError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(CompactionError::InvalidInput(
                "compacted context must not be empty".into(),
            ));
        }
        self.paths.ensure_base_dirs()?;
        let created_at = Utc::now();
        let conversation_id = clean_optional(conversation_id);
        if let Some(id) = conversation_id.as_deref() {
            validate_id(id).map_err(|_| {
                CompactionError::InvalidInput(format!("invalid conversation id: {id}"))
            })?;
        }
        let source = source
            .map(|source| source.trim().to_string())
            .filter(|source| !source.is_empty())
            .unwrap_or_else(|| {
                conversation_id
                    .as_ref()
                    .map(|id| format!("kept-auto-compaction:{id}"))
                    .unwrap_or_else(|| "kept-auto-compaction".into())
            });
        let guidance = guidance
            .map(|guidance| guidance.trim().to_string())
            .filter(|guidance| !guidance.is_empty());
        let max_output_tokens = max_output_tokens
            .filter(|value| *value > 0)
            .unwrap_or_else(|| estimate_tokens(content).max(1));
        let original_input_hash = hash_text(content);
        let record = CompactionRecord {
            id: format!(
                "compact-{}-{}",
                created_at.timestamp_nanos_opt().unwrap_or_default(),
                &original_input_hash[..8]
            ),
            content: content.to_string(),
            guidance,
            conversation_id,
            source,
            max_output_tokens,
            original_input_hash,
            original_input_excerpt: excerpt(content, 500),
            created_at,
        };
        self.write(&record)?;
        Ok(record)
    }

    pub fn list(&self) -> Result<Vec<CompactionRecord>, CompactionError> {
        self.paths.ensure_base_dirs()?;
        let mut records = Vec::new();
        for entry in std::fs::read_dir(self.paths.compactions_dir())? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                records.push(read_record(entry.path())?);
            }
        }
        records.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(records)
    }

    pub fn show(&self, id: &str) -> Result<CompactionRecord, CompactionError> {
        validate_id(id)?;
        let path = self.path_for(id);
        if !path.exists() {
            return Err(CompactionError::NotFound(id.into()));
        }
        read_record(path)
    }

    pub fn remove(&self, id: &str) -> Result<(), CompactionError> {
        validate_id(id)?;
        let path = self.path_for(id);
        if !path.exists() {
            return Err(CompactionError::NotFound(id.into()));
        }
        std::fs::remove_file(path)?;
        Ok(())
    }

    pub fn export_record(
        &self,
        id: &str,
        path: impl AsRef<Path>,
    ) -> Result<CompactionRecord, CompactionError> {
        let record = self.show(id)?;
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&record)?)?;
        Ok(record)
    }

    pub fn import_record(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<CompactionRecord, CompactionError> {
        let mut record: CompactionRecord = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        normalize_imported_record(&mut record)?;
        self.paths.ensure_base_dirs()?;
        if self.path_for(&record.id).exists() {
            let existing = self.show(&record.id)?;
            if existing == record {
                return Ok(existing);
            }
            record.id = self.next_import_id(&record);
        }
        self.write(&record)?;
        Ok(record)
    }

    pub fn remove_by_conversation_ids(
        &self,
        conversation_ids: &[String],
    ) -> Result<Vec<String>, CompactionError> {
        if conversation_ids.is_empty() {
            return Ok(Vec::new());
        }
        let records = self.list()?;
        let mut removed = Vec::new();
        for record in records {
            if record
                .conversation_id
                .as_ref()
                .is_some_and(|id| conversation_ids.iter().any(|candidate| candidate == id))
            {
                self.remove(&record.id)?;
                removed.push(record.id);
            }
        }
        Ok(removed)
    }

    pub fn remove_by_conversation_message_range(
        &self,
        conversation_id: &str,
        from: usize,
        to: usize,
        preserved_ids: &[String],
    ) -> Result<Vec<String>, CompactionError> {
        let ids =
            self.ids_by_conversation_message_range(conversation_id, from, to, preserved_ids)?;
        for id in &ids {
            self.remove(id)?;
        }
        Ok(ids)
    }

    pub fn ids_by_conversation_message_range(
        &self,
        conversation_id: &str,
        from: usize,
        to: usize,
        preserved_ids: &[String],
    ) -> Result<Vec<String>, CompactionError> {
        if from > to {
            return Ok(Vec::new());
        }
        let records = self.list()?;
        let mut ids = Vec::new();
        for record in records {
            let linked_to_conversation = record.conversation_id.as_deref() == Some(conversation_id);
            let preserved = preserved_ids.iter().any(|id| id == &record.id);
            if linked_to_conversation
                && !preserved
                && compaction_source_overlaps_message_range(
                    &record.source,
                    conversation_id,
                    from,
                    to,
                )
            {
                ids.push(record.id);
            }
        }
        ids.sort();
        Ok(ids)
    }

    fn write(&self, record: &CompactionRecord) -> Result<(), CompactionError> {
        self.write_with_quota(record, None)
    }

    fn write_with_quota(
        &self,
        record: &CompactionRecord,
        quota_bytes: Option<u64>,
    ) -> Result<(), CompactionError> {
        let path = self.path_for(&record.id);
        let body = serde_json::to_string_pretty(record)?;
        if let Some(quota_bytes) = quota_bytes {
            self.paths
                .write_quota_checked_with_quota(path, body.as_bytes(), Some(quota_bytes))?;
        } else {
            self.paths.write_quota_checked(path, body.as_bytes())?;
        }
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.paths.compactions_dir().join(format!("{id}.json"))
    }

    fn next_import_id(&self, record: &CompactionRecord) -> String {
        let hash = hash_text(&record.content);
        for idx in 0.. {
            let suffix = if idx == 0 {
                String::new()
            } else {
                format!("-{idx}")
            };
            let id = format!(
                "compact-{}-{}{}",
                Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                &hash[..8],
                suffix
            );
            if !self.path_for(&id).exists() {
                return id;
            }
        }
        unreachable!("compaction import id loop is unbounded")
    }
}

fn read_record(path: PathBuf) -> Result<CompactionRecord, CompactionError> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_imported_record(record: &mut CompactionRecord) -> Result<(), CompactionError> {
    record.id = record.id.trim().to_string();
    validate_id(&record.id)?;
    if record.content.trim().is_empty() {
        return Err(CompactionError::InvalidInput(
            "compacted context must not be empty".into(),
        ));
    }
    record.guidance = clean_optional(record.guidance.take());
    record.conversation_id = clean_optional(record.conversation_id.take());
    if let Some(id) = record.conversation_id.as_deref() {
        validate_id(id)
            .map_err(|_| CompactionError::InvalidInput(format!("invalid conversation id: {id}")))?;
    }
    record.source = record.source.trim().to_string();
    if record.source.is_empty() {
        return Err(CompactionError::InvalidInput(
            "compaction source must not be empty".into(),
        ));
    }
    if record.max_output_tokens == 0 {
        return Err(CompactionError::InvalidInput(
            "max_output_tokens must be greater than zero".into(),
        ));
    }
    if record.original_input_hash.trim().is_empty() {
        record.original_input_hash = hash_text(&record.content);
    }
    if record.original_input_excerpt.trim().is_empty() {
        record.original_input_excerpt = excerpt(&record.content, 500);
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), CompactionError> {
    let valid = !id.trim().is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(CompactionError::InvalidInput(format!(
            "invalid compaction id: {id}"
        )))
    }
}

fn compaction_source_overlaps_message_range(
    source: &str,
    conversation_id: &str,
    from: usize,
    to: usize,
) -> bool {
    let bounds = parse_embedded_message_source_range(source)
        .or_else(|| parse_legacy_conversation_source_range(source, conversation_id));
    bounds.is_some_and(|(start, end)| ranges_overlap(start, end, from, to))
}

fn parse_embedded_message_source_range(source: &str) -> Option<(usize, usize)> {
    let (_, rest) = source.split_once("messages:")?;
    let (start, rest) = parse_leading_usize(rest)?;
    let rest = rest.strip_prefix("..")?;
    let (end, _) = parse_leading_usize(rest)?;
    (start < end).then_some((start, end))
}

fn parse_legacy_conversation_source_range(
    source: &str,
    conversation_id: &str,
) -> Option<(usize, usize)> {
    let prefix = format!("conversation:{conversation_id}:");
    let rest = source.strip_prefix(&prefix)?;
    let (start, rest) = parse_leading_usize(rest)?;
    let rest = rest.strip_prefix(':')?;
    let (end_inclusive, _) = parse_leading_usize(rest)?;
    let end = end_inclusive.checked_add(1)?;
    (start < end).then_some((start, end))
}

fn parse_leading_usize(value: &str) -> Option<(usize, &str)> {
    let end = value
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(value.len());
    if end == 0 {
        return None;
    }
    let (digits, rest) = value.split_at(end);
    Some((digits.parse().ok()?, rest))
}

fn ranges_overlap(source_start: usize, source_end: usize, from: usize, to: usize) -> bool {
    let Some(delete_end) = to.checked_add(1) else {
        return false;
    };
    source_start < delete_end && from < source_end
}

fn compact_text(text: &str, guidance: Option<&str>, source: &str, max_tokens: u32) -> String {
    let header = if let Some(guidance) = guidance {
        format!(
            "<manual-compaction source=\"{source}\" max_output_tokens=\"{max_tokens}\">\nGuidance: {guidance}\nSummary:\n"
        )
    } else {
        format!(
            "<manual-compaction source=\"{source}\" max_output_tokens=\"{max_tokens}\">\nSummary:\n"
        )
    };
    let footer = "\n</manual-compaction>";
    let overhead = estimate_tokens(&header).saturating_add(estimate_tokens(footer));
    let body_budget = max_tokens.saturating_sub(overhead).max(1);
    let body = bullet_compact(text, body_budget);
    format!("{header}{body}{footer}")
}

fn bullet_compact(text: &str, max_tokens: u32) -> String {
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let candidate = format!("- {}\n", normalize_whitespace(line));
        if estimate_tokens(&out).saturating_add(estimate_tokens(&candidate)) > max_tokens {
            let remaining = max_tokens.saturating_sub(estimate_tokens(&out));
            if remaining > 0 && out.is_empty() {
                out.push_str("- ");
                out.push_str(&truncate_to_token_estimate(line, remaining));
                out.push('\n');
            }
            break;
        }
        out.push_str(&candidate);
    }
    if out.trim().is_empty() {
        format!("- {}\n", truncate_to_token_estimate(text, max_tokens))
    } else {
        out.trim_end().to_string()
    }
}

fn truncate_to_token_estimate(text: &str, max_tokens: u32) -> String {
    let max_chars = (max_tokens as usize).saturating_mul(4).max(1);
    let mut out = String::new();
    for ch in normalize_whitespace(text).chars().take(max_chars) {
        out.push(ch);
    }
    if normalize_whitespace(text).chars().count() > out.chars().count() {
        out.push_str("...");
    }
    out
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn excerpt(text: &str, max_chars: usize) -> String {
    let normalized = normalize_whitespace(text);
    let mut out = String::new();
    for ch in normalized.chars().take(max_chars) {
        out.push(ch);
    }
    if normalized.chars().count() > out.chars().count() {
        out.push_str("...");
    }
    out
}

fn estimate_tokens(text: &str) -> u32 {
    if text.trim().is_empty() {
        return 0;
    }
    let chars = text.chars().count();
    let char_estimate = chars.div_ceil(4);
    let word_floor = text.split_whitespace().count();
    char_estimate.max(word_floor).min(u32::MAX as usize) as u32
}

fn hash_text(text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_list_show_and_remove_compaction() {
        let dir = std::env::temp_dir().join(format!("compaction-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));

        let record = store
            .create_from_text_for_conversation(
                "User: remember alpha\nAssistant: alpha was stored",
                Some("keep decisions".into()),
                Some(80),
                Some("manual range 1:2".into()),
                Some("conv-1".into()),
            )
            .unwrap();

        assert!(record.content.contains("Guidance: keep decisions"));
        assert_eq!(record.conversation_id.as_deref(), Some("conv-1"));
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.show(&record.id).unwrap().id, record.id);
        store.remove(&record.id).unwrap();
        assert!(store.list().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removes_only_matching_conversation_compactions() {
        let dir = std::env::temp_dir().join(format!("compaction-conv-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));
        let keep = store
            .create_from_text_for_conversation(
                "keep this",
                None,
                None,
                None,
                Some("conv-keep".into()),
            )
            .unwrap();
        let remove = store
            .create_from_text_for_conversation(
                "remove this",
                None,
                None,
                None,
                Some("conv-remove".into()),
            )
            .unwrap();

        let removed = store
            .remove_by_conversation_ids(&["conv-remove".to_string()])
            .unwrap();
        assert_eq!(removed, vec![remove.id]);
        assert!(store.show(&keep.id).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removes_only_overlapping_conversation_range_compactions() {
        let dir = std::env::temp_dir().join(format!("compaction-range-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));
        let remove_messages = store
            .create_from_text_for_conversation(
                "remove messages source",
                None,
                None,
                Some("pre-delete-range:conv-1:messages:1..3".into()),
                Some("conv-1".into()),
            )
            .unwrap();
        let remove_legacy = store
            .create_from_text_for_conversation(
                "remove legacy source",
                None,
                None,
                Some("conversation:conv-1:2:2".into()),
                Some("conv-1".into()),
            )
            .unwrap();
        let preserved = store
            .create_from_text_for_conversation(
                "preserve this one",
                None,
                None,
                Some("messages:1..3".into()),
                Some("conv-1".into()),
            )
            .unwrap();
        let keep_later = store
            .create_from_text_for_conversation(
                "keep later source",
                None,
                None,
                Some("messages:3..4".into()),
                Some("conv-1".into()),
            )
            .unwrap();
        let keep_other = store
            .create_from_text_for_conversation(
                "keep other conversation",
                None,
                None,
                Some("messages:1..3".into()),
                Some("conv-2".into()),
            )
            .unwrap();

        let removed = store
            .remove_by_conversation_message_range(
                "conv-1",
                1,
                2,
                std::slice::from_ref(&preserved.id),
            )
            .unwrap();

        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&remove_messages.id));
        assert!(removed.contains(&remove_legacy.id));
        assert!(store.show(&preserved.id).is_ok());
        assert!(store.show(&keep_later.id).is_ok());
        assert!(store.show(&keep_other.id).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compaction_rejects_empty_input_and_invalid_ids() {
        let dir = std::env::temp_dir().join(format!("compaction-invalid-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));

        assert!(
            store
                .create_from_text(" ", None, Some(100), None)
                .unwrap_err()
                .to_string()
                .contains("must not be empty")
        );
        assert!(
            store
                .show("../nope")
                .unwrap_err()
                .to_string()
                .contains("invalid")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compaction_write_rejects_records_over_storage_quota() {
        let dir = std::env::temp_dir().join(format!("compaction-quota-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));
        let record = CompactionRecord {
            id: "compact-large".into(),
            content: "x".repeat(512),
            guidance: None,
            conversation_id: None,
            source: "test".into(),
            max_output_tokens: 128,
            original_input_hash: hash_text("large"),
            original_input_excerpt: "large".into(),
            created_at: Utc::now(),
        };

        let err = store
            .write_with_quota(&record, Some(32))
            .expect_err("compaction writes should respect storage quota");

        assert!(matches!(
            err,
            CompactionError::Storage(StorageError::QuotaExceeded { .. })
        ));
        assert!(!store.path_for(&record.id).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn compacted_content_respects_rough_token_budget() {
        let text = (0..200)
            .map(|idx| format!("line {idx} with enough words to take space"))
            .collect::<Vec<_>>()
            .join("\n");
        let compacted = compact_text(&text, None, "test", 64);

        assert!(estimate_tokens(&compacted) <= 70);
        assert!(compacted.contains("<manual-compaction"));
        assert!(compacted.contains("</manual-compaction>"));
    }

    #[test]
    fn keeps_existing_compacted_context_without_rewriting() {
        let dir = std::env::temp_dir().join(format!("compaction-keep-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));
        let content = "<auto-compaction>\nSummary:\n- keep exact text\n</auto-compaction>";

        let record = store
            .keep_compacted_context(
                content,
                Some("keep decisions".into()),
                Some(96),
                Some("preview:auto".into()),
                Some("conv-1".into()),
            )
            .unwrap();

        assert_eq!(record.content, content);
        assert_eq!(record.source, "preview:auto");
        assert_eq!(record.conversation_id.as_deref(), Some("conv-1"));
        assert_eq!(record.guidance.as_deref(), Some("keep decisions"));
        assert_eq!(store.show(&record.id).unwrap().content, content);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exports_and_imports_portable_compaction_records() {
        let dir = std::env::temp_dir().join(format!("compaction-portable-test-{}", uuid_like()));
        let import_dir =
            std::env::temp_dir().join(format!("compaction-import-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));
        let import_store = CompactionStore::new(StoragePaths::new(&import_dir));
        let export_path = std::env::temp_dir().join(format!("{}.json", uuid_like()));

        let record = store
            .keep_compacted_context(
                "<manual-compaction>\n- saved\n</manual-compaction>",
                Some("keep facts".into()),
                Some(64),
                Some("manual".into()),
                Some("conv-1".into()),
            )
            .unwrap();

        let exported = store.export_record(&record.id, &export_path).unwrap();
        let imported = import_store.import_record(&export_path).unwrap();

        assert_eq!(exported, record);
        assert_eq!(imported.id, record.id);
        assert_eq!(imported.content, record.content);
        assert_eq!(imported.conversation_id.as_deref(), Some("conv-1"));
        assert_eq!(import_store.import_record(&export_path).unwrap(), imported);
        let _ = std::fs::remove_file(export_path);
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(import_dir);
    }

    #[test]
    fn import_mints_new_id_for_different_collision() {
        let dir = std::env::temp_dir().join(format!("compaction-collision-test-{}", uuid_like()));
        let store = CompactionStore::new(StoragePaths::new(&dir));
        let first = store
            .keep_compacted_context(
                "<manual-compaction>\n- first\n</manual-compaction>",
                None,
                Some(64),
                Some("manual".into()),
                None,
            )
            .unwrap();
        let mut colliding = first.clone();
        colliding.content = "<manual-compaction>\n- second\n</manual-compaction>".into();
        let import_path = std::env::temp_dir().join(format!("{}.json", uuid_like()));
        std::fs::write(
            &import_path,
            serde_json::to_string_pretty(&colliding).unwrap(),
        )
        .unwrap();

        let imported = store.import_record(&import_path).unwrap();

        assert_ne!(imported.id, first.id);
        assert_eq!(imported.content, colliding.content);
        assert_eq!(store.list().unwrap().len(), 2);
        let _ = std::fs::remove_file(import_path);
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
