//! `agent-capabilities` — safe draft path for agent-created capabilities.
//!
//! Agent-created tools, skills, and subagents start as quarantined drafts.
//! Review/allow decisions are explicit and scoped by higher-level config.

use std::path::{Path, PathBuf};

use agent_storage::{StorageError, StoragePaths};
use agent_tools::{Tool, ToolDescriptor, ToolError, ToolId, ToolPermissions};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("capability draft not found: {0}")]
    NotFound(String),
    #[error("invalid capability draft id: {0}")]
    InvalidId(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid kind: {0}")]
    InvalidKind(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Tool,
    Skill,
    #[serde(alias = "subagent")]
    Agent,
}

impl CapabilityKind {
    pub fn parse(value: &str) -> Result<Self, CapabilityError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tool" => Ok(Self::Tool),
            "skill" => Ok(Self::Skill),
            "agent" | "subagent" => Ok(Self::Agent),
            other => Err(CapabilityError::InvalidKind(other.into())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDraftStatus {
    Quarantined,
    Allowed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDraft {
    pub id: String,
    pub kind: CapabilityKind,
    pub name: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: CapabilityDraftStatus,
    pub provenance: String,
}

#[derive(Debug, Clone)]
pub struct CapabilityDraftInput {
    pub id: Option<String>,
    pub kind: CapabilityKind,
    pub name: String,
    pub body: String,
    pub guidance: Option<String>,
    pub created_by: String,
    pub provenance: String,
}

pub struct CapabilityDraftStore {
    dir: PathBuf,
    quota_paths: Option<StoragePaths>,
}

impl CapabilityDraftStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            quota_paths: None,
        }
    }

    pub fn from_env() -> Self {
        let paths = StoragePaths::from_env();
        Self::from_paths(paths)
    }

    pub fn from_paths(paths: StoragePaths) -> Self {
        Self {
            dir: paths.capability_drafts_dir(),
            quota_paths: Some(paths),
        }
    }

    pub fn propose(&self, input: CapabilityDraftInput) -> Result<CapabilityDraft, CapabilityError> {
        std::fs::create_dir_all(&self.dir)?;
        let now = Utc::now();
        let id = input.id.map(validate_id).transpose()?.unwrap_or_else(|| {
            format!(
                "draft-{}-{}",
                slugify(&input.name),
                uuid::Uuid::new_v4().simple()
            )
        });
        let draft = CapabilityDraft {
            id,
            kind: input.kind,
            name: non_empty(input.name, "name")?,
            body: non_empty(input.body, "body")?,
            guidance: input.guidance.and_then(|value| {
                let value = value.trim().to_string();
                (!value.is_empty()).then_some(value)
            }),
            created_by: non_empty(input.created_by, "created_by")?,
            created_at: now,
            updated_at: now,
            status: CapabilityDraftStatus::Quarantined,
            provenance: non_empty(input.provenance, "provenance")?,
        };
        self.write(&draft)?;
        Ok(draft)
    }

    pub fn list(&self) -> Result<Vec<CapabilityDraft>, CapabilityError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut drafts = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|value| value.to_str()) == Some("json") {
                drafts.push(read_draft(entry.path())?);
            }
        }
        drafts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(drafts)
    }

    pub fn show(&self, id: &str) -> Result<CapabilityDraft, CapabilityError> {
        read_draft(self.path_for(id)?)
    }

    pub fn set_status(
        &self,
        id: &str,
        status: CapabilityDraftStatus,
    ) -> Result<CapabilityDraft, CapabilityError> {
        let mut draft = self.show(id)?;
        draft.status = status;
        draft.updated_at = Utc::now();
        self.write(&draft)?;
        Ok(draft)
    }

    pub fn delete(&self, id: &str) -> Result<bool, CapabilityError> {
        let path = self.path_for(id)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(err.into()),
        }
    }

    pub fn export(
        &self,
        id: &str,
        path: impl AsRef<Path>,
    ) -> Result<CapabilityDraft, CapabilityError> {
        let draft = self.show(id)?;
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&draft)?)?;
        Ok(draft)
    }

    pub fn import(&self, path: impl AsRef<Path>) -> Result<CapabilityDraft, CapabilityError> {
        let mut draft = read_draft(path)?;
        draft.id = validate_id(draft.id)?;
        draft.name = non_empty(draft.name, "name")?;
        draft.body = non_empty(draft.body, "body")?;
        draft.guidance = draft.guidance.and_then(|value| {
            let value = value.trim().to_string();
            (!value.is_empty()).then_some(value)
        });
        draft.created_by = non_empty(draft.created_by, "created_by")?;
        draft.provenance = non_empty(draft.provenance, "provenance")?;
        draft.status = CapabilityDraftStatus::Quarantined;
        draft.updated_at = Utc::now();
        self.write(&draft)?;
        Ok(draft)
    }

    fn write(&self, draft: &CapabilityDraft) -> Result<(), CapabilityError> {
        self.write_with_quota(draft, None)
    }

    fn write_with_quota(
        &self,
        draft: &CapabilityDraft,
        quota_bytes: Option<u64>,
    ) -> Result<(), CapabilityError> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path_for(&draft.id)?;
        let body = serde_json::to_string_pretty(draft)?;
        if let Some(paths) = self.quota_paths.as_ref() {
            if let Some(quota_bytes) = quota_bytes {
                paths.write_quota_checked_with_quota(&path, body.as_bytes(), Some(quota_bytes))?;
            } else {
                paths.write_quota_checked(&path, body.as_bytes())?;
            }
        } else {
            std::fs::write(path, body)?;
        }
        Ok(())
    }

    fn path_for(&self, id: &str) -> Result<PathBuf, CapabilityError> {
        let id = validate_id(id.to_string())?;
        Ok(self.dir.join(format!("{id}.json")))
    }
}

pub struct CapabilityDraftTool {
    store: CapabilityDraftStore,
}

impl CapabilityDraftTool {
    pub fn new(store: CapabilityDraftStore) -> Self {
        Self { store }
    }

    pub fn from_env() -> Self {
        Self::new(CapabilityDraftStore::from_env())
    }

    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            id: ToolId::from("capability_draft"),
            name: "Capability Draft".into(),
            description:
                "Creates a quarantined tool, skill, agent, or subagent draft for human review."
                    .into(),
            categories: vec!["agent".into(), "capability".into(), "draft".into()],
            input_schema: json!({
                "type": "object",
                "required": ["kind", "name", "body"],
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["tool", "skill", "agent", "subagent"],
                        "description": "Capability type to draft. Subagent drafts are reviewed and saved as agent configs."
                    },
                    "name": {
                        "type": "string",
                        "description": "Human-readable draft name."
                    },
                    "body": {
                        "type": "string",
                        "description": "Proposed implementation, prompt, config, or spec."
                    },
                    "guidance": {
                        "type": "string",
                        "description": "Optional review or usage guidance."
                    }
                },
                "additionalProperties": false
            }),
            output_interpretation_guidance: Some(
                "Report the draft id and quarantine status; do not claim the capability is enabled."
                    .into(),
            ),
            permissions: ToolPermissions {
                file_write: true,
                ..ToolPermissions::default()
            },
            requires_approval: true,
            provenance: Some("native:agent-capabilities".into()),
        }
    }

    pub fn descriptor_with_guidance(guidance: Option<&str>) -> ToolDescriptor {
        let mut descriptor = Self::descriptor();
        if let Some(guidance) = guidance
            .map(str::trim)
            .filter(|guidance| !guidance.is_empty())
        {
            descriptor.description = format!("{} Guidance: {guidance}", descriptor.description);
            let base_guidance = descriptor
                .output_interpretation_guidance
                .take()
                .unwrap_or_default();
            descriptor.output_interpretation_guidance = Some(format!(
                "{base_guidance} Follow this capability-drafting guidance: {guidance}"
            ));
        }
        descriptor
    }
}

#[async_trait]
impl Tool for CapabilityDraftTool {
    async fn execute(&self, input: Value) -> Result<Value, ToolError> {
        let draft = self
            .store
            .propose(CapabilityDraftInput {
                id: None,
                kind: CapabilityKind::parse(required_string(&input, "kind")?)
                    .map_err(|err| ToolError::InvalidInput(err.to_string()))?,
                name: required_string(&input, "name")?.to_string(),
                body: required_string(&input, "body")?.to_string(),
                guidance: input
                    .get("guidance")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                created_by: "agent".into(),
                provenance: "tool:capability_draft".into(),
            })
            .map_err(|err| ToolError::Execution(err.to_string()))?;
        serde_json::to_value(draft)
            .map_err(|err| ToolError::Execution(format!("failed to serialize draft: {err}")))
    }
}

fn read_draft(path: impl AsRef<Path>) -> Result<CapabilityDraft, CapabilityError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(CapabilityError::NotFound("unknown".into()))
        }
        Err(err) => Err(err.into()),
    }
}

fn required_string<'a>(value: &'a Value, field: &'static str) -> Result<&'a str, ToolError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::InvalidInput(format!("missing string field `{field}`")))
}

fn non_empty(value: String, field: &'static str) -> Result<String, CapabilityError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        Err(CapabilityError::MissingField(field))
    } else {
        Ok(value)
    }
}

fn validate_id(id: String) -> Result<String, CapabilityError> {
    let id = id.trim().to_string();
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        Err(CapabilityError::InvalidId(id))
    } else {
        Ok(id)
    }
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "capability".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(label: &str) -> CapabilityDraftStore {
        let dir = std::env::temp_dir().join(format!(
            "agent-capabilities-{label}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        CapabilityDraftStore::new(dir)
    }

    #[test]
    fn proposed_capabilities_start_quarantined_and_can_be_reviewed() {
        let store = temp_store("lifecycle");
        let draft = store
            .propose(CapabilityDraftInput {
                id: Some("review-skill".into()),
                kind: CapabilityKind::Skill,
                name: "Review Skill".into(),
                body: "Use this checklist.".into(),
                guidance: Some("Review before enabling.".into()),
                created_by: "agent".into(),
                provenance: "test".into(),
            })
            .unwrap();

        assert_eq!(draft.status, CapabilityDraftStatus::Quarantined);
        assert_eq!(store.list().unwrap().len(), 1);
        let allowed = store
            .set_status("review-skill", CapabilityDraftStatus::Allowed)
            .unwrap();
        assert_eq!(allowed.status, CapabilityDraftStatus::Allowed);
        assert!(store.delete("review-skill").unwrap());
    }

    #[test]
    fn exported_capability_drafts_import_back_as_quarantined() {
        let source = temp_store("export");
        let draft = source
            .propose(CapabilityDraftInput {
                id: Some("review-tool".into()),
                kind: CapabilityKind::Tool,
                name: "Review Tool".into(),
                body: r#"{"mcpServers":{}}"#.into(),
                guidance: Some("Review before enabling.".into()),
                created_by: "agent".into(),
                provenance: "test".into(),
            })
            .unwrap();
        source
            .set_status(&draft.id, CapabilityDraftStatus::Allowed)
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "agent-capabilities-export-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));

        let exported = source.export(&draft.id, &path).unwrap();
        assert_eq!(exported.status, CapabilityDraftStatus::Allowed);

        let imported = temp_store("import").import(&path).unwrap();
        assert_eq!(imported.id, "review-tool");
        assert_eq!(imported.kind, CapabilityKind::Tool);
        assert_eq!(imported.status, CapabilityDraftStatus::Quarantined);
        assert_eq!(
            imported.guidance.as_deref(),
            Some("Review before enabling.")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn subagent_kind_alias_imports_as_agent() {
        let path = std::env::temp_dir().join(format!(
            "agent-capabilities-subagent-alias-{}-{}.json",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::write(
            &path,
            r#"{
              "id": "review-subagent",
              "kind": "subagent",
              "name": "Review Subagent",
              "body": "Check the answer for gaps.",
              "created_by": "agent",
              "created_at": "2026-05-20T00:00:00Z",
              "updated_at": "2026-05-20T00:00:00Z",
              "status": "allowed",
              "provenance": "portable:test"
            }"#,
        )
        .unwrap();

        let imported = temp_store("subagent-import").import(&path).unwrap();

        assert_eq!(
            CapabilityKind::parse("subagent").unwrap(),
            CapabilityKind::Agent
        );
        assert_eq!(imported.kind, CapabilityKind::Agent);
        assert_eq!(imported.status, CapabilityDraftStatus::Quarantined);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn draft_tool_schema_advertises_subagent_alias() {
        let descriptor = CapabilityDraftTool::descriptor();
        let kinds = descriptor.input_schema["properties"]["kind"]["enum"]
            .as_array()
            .expect("kind enum should be an array");

        assert!(kinds.contains(&json!("subagent")));
    }

    #[test]
    fn write_rejects_draft_over_storage_quota() {
        let dir = std::env::temp_dir().join(format!(
            "agent-capabilities-quota-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let store = CapabilityDraftStore::from_paths(StoragePaths::new(&dir));
        let now = Utc::now();
        let draft = CapabilityDraft {
            id: "too-large".into(),
            kind: CapabilityKind::Tool,
            name: "Too Large".into(),
            body: "this draft body exceeds the test quota".into(),
            guidance: Some("review carefully".into()),
            created_by: "agent".into(),
            created_at: now,
            updated_at: now,
            status: CapabilityDraftStatus::Quarantined,
            provenance: "test".into(),
        };

        let err = store
            .write_with_quota(&draft, Some(16))
            .expect_err("capability draft write should fail before exceeding quota");

        assert!(matches!(
            err,
            CapabilityError::Storage(StorageError::QuotaExceeded { .. })
        ));
        assert!(store.show("too-large").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn draft_tool_creates_quarantined_draft() {
        let store = temp_store("tool");
        let tool = CapabilityDraftTool::new(store);

        let output = tool
            .execute(json!({
                "kind": "agent",
                "name": "Researcher",
                "body": "system_prompt = 'Research carefully.'"
            }))
            .await
            .unwrap();

        assert_eq!(output["kind"], "agent");
        assert_eq!(output["status"], "quarantined");
        assert!(
            output["id"]
                .as_str()
                .unwrap()
                .starts_with("draft-researcher-")
        );
    }
}
