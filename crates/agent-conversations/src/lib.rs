//! `agent-conversations` — branch-compatible conversation storage v0.
//!
//! Branches store only their own divergent messages plus a pointer to the
//! parent conversation and parent message count. This keeps the shared main
//! branch from being duplicated on disk while still allowing an expanded view.

use std::path::PathBuf;

use agent_storage::{StorageError, StoragePaths};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("conversation not found: {0}")]
    NotFound(String),
    #[error("invalid conversation input: {0}")]
    InvalidInput(String),
    #[error("conversation {id} has child branches; retry with recursive delete")]
    HasChildren { id: String },
    #[error("conversation {id} has child branches; delete message ranges on leaf branches only")]
    HasChildrenForRange { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDoc {
    pub id: String,
    pub title: String,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<BranchRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_reason: Option<String>,
    #[serde(default)]
    pub messages: Vec<ConversationMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchRef {
    pub conversation_id: String,
    pub parent_message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationMessage {
    pub role: ConversationRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedConversation {
    pub conversation: ConversationDoc,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTreeNode {
    pub id: String,
    pub title: String,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_reason: Option<String>,
    pub own_message_count: usize,
    pub expanded_message_count: usize,
    #[serde(default)]
    pub children: Vec<ConversationTreeNode>,
}

pub struct ConversationStore {
    paths: StoragePaths,
}

impl ConversationStore {
    pub fn new(paths: StoragePaths) -> Self {
        Self { paths }
    }

    pub fn from_env() -> Self {
        Self::new(StoragePaths::from_env())
    }

    pub fn create(
        &self,
        title: Option<String>,
        agent_id: Option<String>,
    ) -> Result<ConversationDoc, ConversationError> {
        self.paths.ensure_base_dirs()?;
        let now = Utc::now();
        let doc = ConversationDoc {
            id: format!("conv-{}", uuid::Uuid::new_v4()),
            title: clean_optional(title).unwrap_or_else(|| "Untitled conversation".into()),
            agent_id: clean_optional(agent_id).unwrap_or_else(|| "fake-agent".into()),
            parent: None,
            branch_reason: None,
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        self.write(&doc)?;
        Ok(doc)
    }

    pub fn append_message(
        &self,
        id: &str,
        role: ConversationRole,
        content: &str,
    ) -> Result<ConversationDoc, ConversationError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(ConversationError::InvalidInput(
                "message content must not be empty".into(),
            ));
        }
        let mut doc = self.show(id)?;
        let now = Utc::now();
        doc.messages.push(ConversationMessage {
            role,
            content: content.into(),
            created_at: now,
        });
        doc.updated_at = now;
        self.write(&doc)?;
        Ok(doc)
    }

    pub fn branch(
        &self,
        parent_id: &str,
        parent_message_count: usize,
        title: Option<String>,
        reason: Option<String>,
    ) -> Result<ConversationDoc, ConversationError> {
        let parent = self.expanded(parent_id)?;
        if parent_message_count > parent.messages.len() {
            return Err(ConversationError::InvalidInput(format!(
                "branch point {parent_message_count} exceeds parent expanded length {}",
                parent.messages.len()
            )));
        }
        let now = Utc::now();
        let doc = ConversationDoc {
            id: format!("conv-{}", uuid::Uuid::new_v4()),
            title: clean_optional(title)
                .unwrap_or_else(|| format!("Branch of {}", parent.conversation.title)),
            agent_id: parent.conversation.agent_id.clone(),
            parent: Some(BranchRef {
                conversation_id: parent.conversation.id,
                parent_message_count,
            }),
            branch_reason: clean_optional(reason),
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        self.write(&doc)?;
        Ok(doc)
    }

    pub fn list(&self) -> Result<Vec<ConversationDoc>, ConversationError> {
        self.paths.ensure_base_dirs()?;
        let mut docs = Vec::new();
        for entry in std::fs::read_dir(self.paths.conversations_dir())? {
            let entry = entry?;
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                docs.push(read_doc(entry.path())?);
            }
        }
        docs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(docs)
    }

    pub fn show(&self, id: &str) -> Result<ConversationDoc, ConversationError> {
        validate_id(id)?;
        let path = self.path_for(id);
        if !path.exists() {
            return Err(ConversationError::NotFound(id.into()));
        }
        read_doc(path)
    }

    pub fn expanded(&self, id: &str) -> Result<ExpandedConversation, ConversationError> {
        let conversation = self.show(id)?;
        let messages = self.expand_doc(&conversation)?;
        Ok(ExpandedConversation {
            conversation,
            messages,
        })
    }

    pub fn tree(&self) -> Result<Vec<ConversationTreeNode>, ConversationError> {
        let docs = self.list()?;
        let roots = docs
            .iter()
            .filter(|doc| doc.parent.is_none())
            .map(|doc| self.tree_node(doc, &docs))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(roots)
    }

    pub fn delete(&self, id: &str, recursive: bool) -> Result<Vec<String>, ConversationError> {
        self.delete_many(&[id.to_string()], recursive)
    }

    pub fn deletion_plan(
        &self,
        ids: &[String],
        recursive: bool,
    ) -> Result<Vec<String>, ConversationError> {
        self.plan_delete_many(ids, recursive)
    }

    pub fn delete_by_agent(
        &self,
        agent_id: &str,
        recursive: bool,
    ) -> Result<Vec<String>, ConversationError> {
        let agent_id = agent_id.trim();
        if agent_id.is_empty() {
            return Err(ConversationError::InvalidInput(
                "agent id must not be empty".into(),
            ));
        }
        let docs = self.list()?;
        let ids = docs
            .iter()
            .filter(|doc| doc.agent_id == agent_id)
            .map(|doc| doc.id.clone())
            .collect::<Vec<_>>();
        self.delete_many(&ids, recursive)
    }

    pub fn deletion_plan_by_agent(
        &self,
        agent_id: &str,
        recursive: bool,
    ) -> Result<Vec<String>, ConversationError> {
        let agent_id = agent_id.trim();
        if agent_id.is_empty() {
            return Err(ConversationError::InvalidInput(
                "agent id must not be empty".into(),
            ));
        }
        let docs = self.list()?;
        let ids = docs
            .iter()
            .filter(|doc| doc.agent_id == agent_id)
            .map(|doc| doc.id.clone())
            .collect::<Vec<_>>();
        self.plan_delete_many(&ids, recursive)
    }

    pub fn delete_message_range(
        &self,
        id: &str,
        from: usize,
        to: usize,
    ) -> Result<ConversationDoc, ConversationError> {
        validate_id(id)?;
        if from > to {
            return Err(ConversationError::InvalidInput(format!(
                "range start {from} is after range end {to}"
            )));
        }
        let docs = self.list()?;
        if has_children(id, &docs) {
            return Err(ConversationError::HasChildrenForRange { id: id.into() });
        }
        let mut doc = self.show(id)?;
        let expanded_len = self.expand_doc(&doc)?.len();
        if to >= expanded_len {
            return Err(ConversationError::InvalidInput(format!(
                "range end {to} exceeds last expanded message index {}",
                expanded_len.saturating_sub(1)
            )));
        }
        let own_start = expanded_len.saturating_sub(doc.messages.len());
        if from < own_start {
            return Err(ConversationError::InvalidInput(format!(
                "range {from}:{to} includes inherited parent messages; delete that range on the parent branch"
            )));
        }
        let own_from = from - own_start;
        let own_to = to - own_start;
        doc.messages.drain(own_from..=own_to);
        doc.updated_at = Utc::now();
        self.write(&doc)?;
        Ok(doc)
    }

    pub fn delete_many(
        &self,
        ids: &[String],
        recursive: bool,
    ) -> Result<Vec<String>, ConversationError> {
        let deleted = self.plan_delete_many(ids, recursive)?;
        for doc_id in &deleted {
            let path = self.path_for(doc_id);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        Ok(deleted)
    }

    fn plan_delete_many(
        &self,
        ids: &[String],
        recursive: bool,
    ) -> Result<Vec<String>, ConversationError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut requested = Vec::new();
        for id in ids {
            validate_id(id)?;
            if !requested.iter().any(|existing| existing == id) {
                requested.push(id.clone());
            }
        }
        let docs = self.list()?;
        for id in &requested {
            if !docs.iter().any(|doc| doc.id == *id) {
                return Err(ConversationError::NotFound(id.clone()));
            }
        }
        let mut deleted = requested.clone();
        for id in &requested {
            let descendants = descendants_of(id, &docs);
            if !recursive
                && descendants
                    .iter()
                    .any(|descendant| !requested.iter().any(|candidate| candidate == descendant))
            {
                return Err(ConversationError::HasChildren { id: id.clone() });
            }
            if recursive {
                deleted.extend(descendants);
            }
        }
        deleted.sort();
        deleted.dedup();
        Ok(deleted)
    }

    fn expand_doc(
        &self,
        doc: &ConversationDoc,
    ) -> Result<Vec<ConversationMessage>, ConversationError> {
        let mut messages = if let Some(parent) = &doc.parent {
            let parent_messages = self.expanded(&parent.conversation_id)?.messages;
            parent_messages
                .into_iter()
                .take(parent.parent_message_count)
                .collect()
        } else {
            Vec::new()
        };
        messages.extend(doc.messages.clone());
        Ok(messages)
    }

    fn tree_node(
        &self,
        doc: &ConversationDoc,
        docs: &[ConversationDoc],
    ) -> Result<ConversationTreeNode, ConversationError> {
        let children = docs
            .iter()
            .filter(|candidate| {
                candidate
                    .parent
                    .as_ref()
                    .is_some_and(|parent| parent.conversation_id == doc.id)
            })
            .map(|child| self.tree_node(child, docs))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ConversationTreeNode {
            id: doc.id.clone(),
            title: doc.title.clone(),
            agent_id: doc.agent_id.clone(),
            parent_id: doc
                .parent
                .as_ref()
                .map(|parent| parent.conversation_id.clone()),
            branch_reason: doc.branch_reason.clone(),
            own_message_count: doc.messages.len(),
            expanded_message_count: self.expanded(&doc.id)?.messages.len(),
            children,
        })
    }

    fn write(&self, doc: &ConversationDoc) -> Result<(), ConversationError> {
        std::fs::write(self.path_for(&doc.id), serde_json::to_string_pretty(doc)?)?;
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.paths.conversations_dir().join(format!("{id}.json"))
    }
}

fn read_doc(path: PathBuf) -> Result<ConversationDoc, ConversationError> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_id(id: &str) -> Result<(), ConversationError> {
    let valid = !id.trim().is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(ConversationError::InvalidInput(format!(
            "invalid conversation id: {id}"
        )))
    }
}

fn descendants_of(id: &str, docs: &[ConversationDoc]) -> Vec<String> {
    let mut out = Vec::new();
    for child in docs.iter().filter(|doc| {
        doc.parent
            .as_ref()
            .is_some_and(|parent| parent.conversation_id == id)
    }) {
        out.push(child.id.clone());
        out.extend(descendants_of(&child.id, docs));
    }
    out
}

fn has_children(id: &str, docs: &[ConversationDoc]) -> bool {
    docs.iter().any(|doc| {
        doc.parent
            .as_ref()
            .is_some_and(|parent| parent.conversation_id == id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branches_expand_without_duplicating_parent_messages() {
        let dir = std::env::temp_dir().join(format!("conversation-test-{}", uuid_like()));
        let store = ConversationStore::new(StoragePaths::new(&dir));
        let root = store
            .create(Some("Root".into()), Some("fake-agent".into()))
            .unwrap();
        store
            .append_message(&root.id, ConversationRole::User, "one")
            .unwrap();
        store
            .append_message(&root.id, ConversationRole::Assistant, "two")
            .unwrap();

        let branch = store
            .branch(
                &root.id,
                1,
                Some("Alternative".into()),
                Some("try another path".into()),
            )
            .unwrap();
        store
            .append_message(&branch.id, ConversationRole::User, "branch one")
            .unwrap();
        let branch_doc = store.show(&branch.id).unwrap();
        assert_eq!(branch_doc.messages.len(), 1);
        assert_eq!(
            branch_doc.parent.unwrap(),
            BranchRef {
                conversation_id: root.id.clone(),
                parent_message_count: 1
            }
        );

        let expanded = store.expanded(&branch.id).unwrap();
        assert_eq!(expanded.messages.len(), 2);
        assert_eq!(expanded.messages[0].content, "one");
        assert_eq!(expanded.messages[1].content, "branch one");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tree_and_recursive_delete_keep_shared_parent() {
        let dir = std::env::temp_dir().join(format!("conversation-tree-test-{}", uuid_like()));
        let store = ConversationStore::new(StoragePaths::new(&dir));
        let root = store.create(Some("Root".into()), None).unwrap();
        store
            .append_message(&root.id, ConversationRole::User, "hello")
            .unwrap();
        let branch = store
            .branch(&root.id, 1, Some("Branch".into()), None)
            .unwrap();
        let grandchild = store
            .branch(&branch.id, 1, Some("Nested".into()), None)
            .unwrap();

        let tree = store.tree().unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].children[0].id, grandchild.id);

        assert!(matches!(
            store.delete(&branch.id, false).unwrap_err(),
            ConversationError::HasChildren { .. }
        ));
        let deleted = store.delete(&branch.id, true).unwrap();
        assert!(deleted.contains(&branch.id));
        assert!(deleted.contains(&grandchild.id));
        assert!(store.show(&root.id).is_ok());
        assert!(store.show(&branch.id).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_by_agent_removes_matching_branches_only() {
        let dir = std::env::temp_dir().join(format!("conversation-agent-test-{}", uuid_like()));
        let store = ConversationStore::new(StoragePaths::new(&dir));
        let root = store
            .create(Some("Root".into()), Some("agent-a".into()))
            .unwrap();
        store
            .append_message(&root.id, ConversationRole::User, "hello")
            .unwrap();
        let branch = store
            .branch(&root.id, 1, Some("Branch".into()), None)
            .unwrap();
        let other = store
            .create(Some("Other".into()), Some("agent-b".into()))
            .unwrap();

        let deleted = store.delete_by_agent("agent-a", false).unwrap();
        assert!(deleted.contains(&root.id));
        assert!(deleted.contains(&branch.id));
        assert!(store.show(&root.id).is_err());
        assert!(store.show(&branch.id).is_err());
        assert!(store.show(&other.id).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_message_range_is_leaf_only_and_own_messages_only() {
        let dir = std::env::temp_dir().join(format!("conversation-range-test-{}", uuid_like()));
        let store = ConversationStore::new(StoragePaths::new(&dir));
        let root = store
            .create(Some("Root".into()), Some("agent-a".into()))
            .unwrap();
        store
            .append_message(&root.id, ConversationRole::User, "root one")
            .unwrap();
        store
            .append_message(&root.id, ConversationRole::Assistant, "root two")
            .unwrap();
        let branch = store
            .branch(&root.id, 2, Some("Branch".into()), None)
            .unwrap();
        store
            .append_message(&branch.id, ConversationRole::User, "branch one")
            .unwrap();
        store
            .append_message(&branch.id, ConversationRole::Assistant, "branch two")
            .unwrap();

        assert!(matches!(
            store.delete_message_range(&root.id, 0, 0).unwrap_err(),
            ConversationError::HasChildrenForRange { .. }
        ));
        assert!(
            store
                .delete_message_range(&branch.id, 1, 2)
                .unwrap_err()
                .to_string()
                .contains("inherited parent messages")
        );

        let updated = store.delete_message_range(&branch.id, 2, 2).unwrap();
        assert_eq!(updated.messages.len(), 1);
        assert_eq!(updated.messages[0].content, "branch two");
        let expanded = store.expanded(&branch.id).unwrap();
        assert_eq!(expanded.messages.len(), 3);
        assert_eq!(expanded.messages[2].content, "branch two");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_invalid_branch_points_and_empty_messages() {
        let dir = std::env::temp_dir().join(format!("conversation-invalid-test-{}", uuid_like()));
        let store = ConversationStore::new(StoragePaths::new(&dir));
        let root = store.create(None, None).unwrap();

        assert!(
            store
                .append_message(&root.id, ConversationRole::User, " ")
                .is_err()
        );
        assert!(store.branch(&root.id, 99, None, None).is_err());
        assert!(store.show("../nope").is_err());
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
