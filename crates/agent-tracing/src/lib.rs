//! `agent-tracing` — RunEvent stream and event store.
//!
//! See `specs/architecture.md` §4.7 (RunEvent), §15 (storage and durability).
//! v0 ships an in-memory store and a `PublishingEventStore` wrapper that
//! forwards every appended event over a `tokio` channel for live UIs.
//! SQLite lands in a later slice.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

/// Schema version embedded in every emitted event. Bump on breaking changes.
/// See `specs/architecture.md` §4.7 / §15.3.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub uuid::Uuid);

impl RunId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    pub id: EventId,
    pub run_id: RunId,
    pub parent_event: Option<EventId>,
    pub schema_version: u32,
    pub at: chrono::DateTime<chrono::Utc>,
    pub kind: RunEventKind,
}

/// Subset of `RunEventKind` from `specs/architecture.md` §4.7. v0 carries
/// the variants needed by the tools-enabled run loop. Memory, ingestion,
/// hooks, batch, subagents, streaming tokens, and approvals land later.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RunEventKind {
    RunStarted {
        agent_id: String,
        input: String,
    },
    ContextBuilt {
        snapshot: serde_json::Value,
    },
    LlmRequestStarted {
        model: String,
    },
    LlmRequestCompleted {
        tokens_in: u32,
        tokens_out: u32,
        #[serde(default)]
        cost_usd: Option<f64>,
        duration_ms: u64,
    },
    ToolCallProposed {
        call_id: String,
        tool_id: String,
        input: serde_json::Value,
    },
    ToolCallStarted {
        call_id: String,
    },
    ToolCallCompleted {
        call_id: String,
        output: serde_json::Value,
        #[serde(default)]
        cost_usd: Option<f64>,
        duration_ms: u64,
    },
    ToolCallFailed {
        call_id: String,
        error: String,
    },
    ApprovalRequested {
        approval_id: String,
        action: String,
        reason: String,
    },
    ApprovalResolved {
        approval_id: String,
        approved: bool,
    },
    GuidanceInjected {
        content: String,
    },
    QualityScored {
        target: String,
        score: f32,
    },
    MemoryLoaded {
        ids: Vec<String>,
    },
    MemoryWritten {
        id: String,
        operation: String,
    },
    IngestionReferenced {
        artifact_id: String,
        source: String,
    },
    IngestionStarted {
        source: String,
        backend: String,
    },
    IngestionCompleted {
        artifact_id: String,
        content_hash: String,
        sections: u32,
    },
    PolicyDenied {
        reason: String,
    },
    ChildRunStarted {
        child_run_id: RunId,
        agent_id: String,
    },
    ChildRunCompleted {
        child_run_id: RunId,
        status: String,
    },
    BatchRunStarted {
        batch_id: String,
        items: u32,
    },
    BatchItemStatus {
        batch_id: String,
        item_key: String,
        status: String,
    },
    BatchRunCompleted {
        batch_id: String,
        succeeded: u32,
        failed: u32,
    },
    RunPaused {
        reason: String,
    },
    RunCancelled {
        reason: String,
    },
    RunCompleted {
        final_output: String,
        #[serde(default)]
        total_cost_usd: Option<f64>,
        total_duration_ms: u64,
    },
    RunFailed {
        reason: String,
    },
}

/// Append-only event store contract. `append` returns the full `RunEvent` so
/// wrapping stores (see `PublishingEventStore`) can forward it without
/// duplicating construction.
pub trait EventStore: Send + Sync {
    fn append(&self, run_id: RunId, parent: Option<EventId>, kind: RunEventKind) -> RunEvent;
    fn events(&self, run_id: RunId) -> Vec<RunEvent>;
}

#[derive(Debug, thiserror::Error)]
pub enum TraceStoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("time parse error: {0}")]
    Time(#[from] chrono::ParseError),
    #[error("uuid parse error: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("unsupported future schema version {found}; this reader supports {supported}")]
    FutureSchema { found: u32, supported: u32 },
    #[error("integer conversion failed: {0}")]
    Int(#[from] std::num::TryFromIntError),
}

/// Process-local in-memory event store. Cheap to instantiate, useful for tests
/// and the `--print` CLI mode. Replaced by a SQLite-backed implementation in a
/// later slice (see `specs/architecture.md` §15).
pub struct InMemoryEventStore {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    events: Vec<RunEvent>,
    next_id: u64,
}

impl InMemoryEventStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                events: Vec::new(),
                next_id: 1,
            })),
        }
    }
}

impl Default for InMemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EventStore for InMemoryEventStore {
    fn append(&self, run_id: RunId, parent: Option<EventId>, kind: RunEventKind) -> RunEvent {
        let mut g = self.inner.lock().expect("event store mutex poisoned");
        let id = EventId(g.next_id);
        g.next_id += 1;
        let event = RunEvent {
            id,
            run_id,
            parent_event: parent,
            schema_version: SCHEMA_VERSION,
            at: chrono::Utc::now(),
            kind,
        };
        g.events.push(event.clone());
        event
    }

    fn events(&self, run_id: RunId) -> Vec<RunEvent> {
        let g = self.inner.lock().expect("event store mutex poisoned");
        g.events
            .iter()
            .filter(|e| e.run_id == run_id)
            .cloned()
            .collect()
    }
}

/// SQLite-backed append-only event store. This is the Stage 2 durable trace
/// implementation; callers can still use [`InMemoryEventStore`] for tests.
pub struct SqliteEventStore {
    conn: Mutex<Connection>,
}

impl SqliteEventStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, TraceStoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_in_memory() -> Result<Self, TraceStoreError> {
        let conn = Connection::open_in_memory()?;
        init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn try_events(&self, run_id: RunId) -> Result<Vec<RunEvent>, TraceStoreError> {
        let conn = self.conn.lock().expect("sqlite event store mutex poisoned");
        read_events(&conn, run_id)
    }
}

impl EventStore for SqliteEventStore {
    fn append(&self, run_id: RunId, parent: Option<EventId>, kind: RunEventKind) -> RunEvent {
        let mut conn = self.conn.lock().expect("sqlite event store mutex poisoned");
        let event = RunEvent {
            id: EventId(0),
            run_id,
            parent_event: parent,
            schema_version: SCHEMA_VERSION,
            at: chrono::Utc::now(),
            kind,
        };
        let kind_json =
            serde_json::to_string(&event.kind).expect("RunEventKind serialization should not fail");
        let parent_i64 = event.parent_event.map(|id| id.0 as i64);
        let tx = conn
            .transaction()
            .expect("sqlite transaction should start for event append");
        tx.execute(
            "INSERT INTO run_events (run_id, parent_event, schema_version, at, kind_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run_id.0.to_string(),
                parent_i64,
                i64::from(event.schema_version),
                event.at.to_rfc3339(),
                kind_json
            ],
        )
        .expect("sqlite event insert should succeed");
        let id = EventId(tx.last_insert_rowid() as u64);
        tx.commit().expect("sqlite event commit should succeed");

        RunEvent { id, ..event }
    }

    fn events(&self, run_id: RunId) -> Vec<RunEvent> {
        self.try_events(run_id)
            .expect("sqlite event read should succeed")
    }
}

fn init_schema(conn: &Connection) -> Result<(), TraceStoreError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS run_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT NOT NULL,
            parent_event INTEGER NULL,
            schema_version INTEGER NOT NULL,
            at TEXT NOT NULL,
            kind_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_run_events_run_id_id
            ON run_events(run_id, id);
        "#,
    )?;
    Ok(())
}

fn read_events(conn: &Connection, run_id: RunId) -> Result<Vec<RunEvent>, TraceStoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, parent_event, schema_version, at, kind_json
         FROM run_events
         WHERE run_id = ?1
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![run_id.0.to_string()], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;

    let mut events = Vec::new();
    for row in rows {
        let (id, run_id, parent_event, schema_version, at, kind_json) = row?;
        let schema_version = u32::try_from(schema_version)?;
        if schema_version > SCHEMA_VERSION {
            return Err(TraceStoreError::FutureSchema {
                found: schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        events.push(RunEvent {
            id: EventId(u64::try_from(id)?),
            run_id: RunId(uuid::Uuid::parse_str(&run_id)?),
            parent_event: parent_event.map(u64::try_from).transpose()?.map(EventId),
            schema_version,
            at: chrono::DateTime::parse_from_rfc3339(&at)?.with_timezone(&chrono::Utc),
            kind: serde_json::from_str(&kind_json)?,
        });
    }
    Ok(events)
}

/// Wraps another `EventStore` and forwards every appended event over a `tokio`
/// channel. Used by the TUI / Tauri UIs to render events live as they arrive
/// without polling the underlying store.
pub struct PublishingEventStore<S: EventStore> {
    inner: S,
    sender: UnboundedSender<RunEvent>,
}

impl<S: EventStore> PublishingEventStore<S> {
    pub fn new(inner: S, sender: UnboundedSender<RunEvent>) -> Self {
        Self { inner, sender }
    }
}

impl<S: EventStore> EventStore for PublishingEventStore<S> {
    fn append(&self, run_id: RunId, parent: Option<EventId>, kind: RunEventKind) -> RunEvent {
        let event = self.inner.append(run_id, parent, kind);
        // Best-effort: if the receiver has been dropped we just drop the event.
        let _ = self.sender.send(event.clone());
        event
    }

    fn events(&self, run_id: RunId) -> Vec<RunEvent> {
        self.inner.events(run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_and_read_in_order() {
        let store = InMemoryEventStore::new();
        let run = RunId::new();

        let a = store.append(
            run,
            None,
            RunEventKind::RunStarted {
                agent_id: "x".into(),
                input: "hi".into(),
            },
        );
        let b = store.append(
            run,
            Some(a.id),
            RunEventKind::RunCompleted {
                final_output: "ok".into(),
                total_cost_usd: None,
                total_duration_ms: 1,
            },
        );

        let events = store.events(run);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, a.id);
        assert_eq!(events[1].id, b.id);
        assert_eq!(events[1].parent_event, Some(a.id));
        assert!(events.iter().all(|e| e.schema_version == SCHEMA_VERSION));
    }

    #[test]
    fn runs_are_isolated() {
        let store = InMemoryEventStore::new();
        let r1 = RunId::new();
        let r2 = RunId::new();

        store.append(
            r1,
            None,
            RunEventKind::RunStarted {
                agent_id: "a".into(),
                input: "x".into(),
            },
        );
        store.append(
            r2,
            None,
            RunEventKind::RunStarted {
                agent_id: "b".into(),
                input: "y".into(),
            },
        );

        assert_eq!(store.events(r1).len(), 1);
        assert_eq!(store.events(r2).len(), 1);
    }

    #[test]
    fn event_id_monotonic_per_store() {
        let store = InMemoryEventStore::new();
        let r = RunId::new();
        let a = store.append(
            r,
            None,
            RunEventKind::RunStarted {
                agent_id: "a".into(),
                input: "1".into(),
            },
        );
        let b = store.append(
            r,
            None,
            RunEventKind::RunStarted {
                agent_id: "a".into(),
                input: "2".into(),
            },
        );
        assert!(b.id.0 > a.id.0);
    }

    #[test]
    fn tool_call_events_round_trip_through_serde() {
        let store = InMemoryEventStore::new();
        let r = RunId::new();
        store.append(
            r,
            None,
            RunEventKind::ToolCallProposed {
                call_id: "c1".into(),
                tool_id: "echo".into(),
                input: serde_json::json!({"x": 1}),
            },
        );
        let evts = store.events(r);
        let json = serde_json::to_string(&evts[0]).unwrap();
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        match back.kind {
            RunEventKind::ToolCallProposed {
                call_id,
                tool_id,
                input,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(tool_id, "echo");
                assert_eq!(input, serde_json::json!({"x": 1}));
            }
            other => panic!("unexpected kind: {other:?}"),
        }
    }

    #[tokio::test]
    async fn publishing_event_store_forwards_appended_events() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let store = PublishingEventStore::new(InMemoryEventStore::new(), tx);
        let run = RunId::new();

        store.append(
            run,
            None,
            RunEventKind::RunStarted {
                agent_id: "a".into(),
                input: "x".into(),
            },
        );
        store.append(
            run,
            None,
            RunEventKind::RunCompleted {
                final_output: "ok".into(),
                total_cost_usd: None,
                total_duration_ms: 0,
            },
        );

        let first = rx.recv().await.unwrap();
        let second = rx.recv().await.unwrap();
        matches!(first.kind, RunEventKind::RunStarted { .. });
        matches!(second.kind, RunEventKind::RunCompleted { .. });
    }

    #[test]
    fn publishing_store_tolerates_dropped_receiver() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        drop(rx);
        let store = PublishingEventStore::new(InMemoryEventStore::new(), tx);
        let r = RunId::new();
        // Should not panic or error even though the receiver is gone.
        let _ = store.append(
            r,
            None,
            RunEventKind::RunStarted {
                agent_id: "a".into(),
                input: "x".into(),
            },
        );
    }

    #[test]
    fn sqlite_store_persists_and_reads_events() {
        let store = SqliteEventStore::open_in_memory().unwrap();
        let run = RunId::new();
        let started = store.append(
            run,
            None,
            RunEventKind::RunStarted {
                agent_id: "a".into(),
                input: "hello".into(),
            },
        );
        store.append(
            run,
            Some(started.id),
            RunEventKind::RunCompleted {
                final_output: "ok".into(),
                total_cost_usd: Some(0.0001),
                total_duration_ms: 7,
            },
        );

        let events = store.try_events(run).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id.0, 1);
        assert_eq!(events[1].parent_event, Some(started.id));
        assert!(matches!(
            events[1].kind,
            RunEventKind::RunCompleted {
                ref final_output,
                ..
            } if final_output == "ok"
        ));
    }
}
