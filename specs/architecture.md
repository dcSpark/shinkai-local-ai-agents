# Agent Harness CLI — Architecture Spec

Last updated: 2026-04-24
Companion docs: [`general_requirements.md`](general_requirements.md) (product / what), [`gils_feedback.md`](gils_feedback.md) (source spec).

This document is the runtime contract. Product surface lives in `general_requirements.md`; this doc describes the types, lifecycles, invariants, crates, and build sequence the runtime must implement.

---

## 1. Core Rule

```
feature
  │
  ▼
can it be represented as a trace event?
  │
  ├─ yes ─► core runtime candidate
  │
  └─ no  ─► adapter, UI layer, or later feature
```

Every core event carries: input, output, policy decisions applied, model used, tool used, cost, duration, status, provenance. If a feature cannot produce that record, it does not belong in core.

---

## 2. Glossary

| Term | Meaning |
| --- | --- |
| `agent-core` | The Rust runtime library. Single source of truth for the runtime API. |
| `agent-cli` | TUI binary built on ratatui/crossterm. Implements gils_feedback's accessibility section (slash commands, mid-run guidance, scoring, context preview pane). One of two day-one UI clients. |
| `agent-tauri` | Tauri v2 webapp. Separate UI codebase from the TUI. The other day-one UI client. |
| Run | One execution instance of an agent against a user input. Owns a state machine and emits a `RunEvent` stream. |
| ChildRun | A run launched from inside a run (subagent or batch item). Linked to its parent in the trace. |
| BatchRun | An iteration container that owns a list of items and a ChildRun per item. The runtime, not an LLM, owns the iteration. |
| Tool | Executable capability with declared permissions and a schema. Produces raw output. |
| Skill | Procedural instructional context. Enters context, does not execute. |
| Hook | Lifecycle observer registered by a plugin. Receives `RunEvent`s but cannot mutate run state outside declared extension points. |
| ContextSnapshot | The exact LLM input for one call. The same builder produces the preview and the request. |
| RunEvent | One append-only fact in the run trace. |
| Profile | Top-level scope for agents, tools, skills, memories, conversations. Slot exists from v1; full multi-profile features are deferred. |
| Quarantine | Installed but not enabled. Cannot run, cannot enter context. |

---

## 3. Three-Surface Architecture

```
                            agent-core (Rust)
                                  ▲
                                  │ HarnessApi trait
              ┌───────────────────┼───────────────────┐
              │                   │                   │
        agent-cli            agent-tauri        (later) integrations
       (ratatui TUI)       (Tauri v2 webapp)     (messaging / mobile)
              │                   │                   │
       Win / Mac / Linux    Win / Mac / Linux
       (crossterm)          (WebView2 / WKWebView /
                             WebKitGTK)
```

**Day-one invariants:**

- `agent-core` is a Rust library exposing a `HarnessApi` trait surface. Every UI client calls this trait, never internal modules.
- `agent-cli` and `agent-tauri` are **independent UI codebases** consuming the same trait. They are not one UI compiled twice.
- `agent-cli` is a real **TUI**, not a stdin/stdout one-shot. A non-interactive `--print` mode exists for scripting/CI.
- Both UIs target Windows / macOS / Linux from v1. Cross-platform packaging (code signing, WebView2 install, notarization) is in scope from phase 1.
- In v1 both UIs link `agent-core` directly. The trait is designed so a future daemon transport can replace the in-process call site without API churn.

---

## 4. Native ABI — Core Types

These are the load-bearing types. Field details may evolve; the **shape** is the contract that lets the eight crates progress in parallel. Illustrative Rust sketches:

### 4.1 ToolDescriptor

```rust
pub struct ToolDescriptor {
    pub id: ToolId,
    pub name: String,
    pub version: SemVer,
    pub description: String,

    pub input_schema:  JsonSchema,
    pub output_schema: Option<JsonSchema>,

    pub runtime: ToolRuntime,
    pub permissions: PermissionManifest,
    pub visibility: VisibilityPolicy,
    pub execution: ExecutionPolicy,
    pub interpretation: InterpretationPolicy,
    pub provenance: Provenance,
}

pub enum ToolRuntime {
    Native(NativeEntry),
    Process { entrypoint: String, working_dir: Option<PathBuf> },
    Python  { entrypoint: String, env: PythonEnvSpec },
    Node    { entrypoint: String },
    Wasm    { module: PathBuf },
    Mcp     { server: McpServerRef, tool_name: String },
    Http    { endpoint: Url, auth: Option<AuthSpec> },
    Subagent(AgentRef),                 // see §13
    Hermes  { plugin_id: String, tool_name: String },     // adapter
    OpenClaw{ skill_id: String },                          // adapter
}

pub struct VisibilityPolicy {
    pub default: VisibilityLevel, // Hidden | NameOnly | NameDescription | FullSchema
    pub examples_visible: bool,
}

pub struct ExecutionPolicy {
    pub timeout_ms: Option<u64>,
    pub retry: RetryPolicy,
    pub idempotent: bool,
    pub side_effects: SideEffectClass, // None | Local | External | Financial | SecuritySensitive
    pub max_calls_default: Option<u32>,
}

pub struct InterpretationPolicy {
    pub default: InterpretationMode,   // Raw | LlmInterpreted
    pub guidance: Option<String>,      // output_interpretation_guidance from gils §5.4
    pub preferred_model: Option<ModelRef>,
}
```

### 4.2 SkillDoc

```rust
pub struct SkillDoc {
    pub id: SkillId,
    pub name: String,
    pub version: SemVer,
    pub description: String,
    pub body: String,                  // human-readable instructional content
    pub format: SkillFormat,           // Native | AgentSkills | OpenClaw | Hermes
    pub dependencies: SkillDependencies,
    pub visibility: SkillVisibility,
    pub scope: SkillScope,
    pub trust: TrustLevel,             // Trusted | Reviewed | Unreviewed | Quarantined
    pub token_cost: Option<TokenCost>,
    pub provenance: Provenance,
}

pub struct SkillVisibility {
    pub default: VisibilityLevel,
    pub load_strategy: LoadStrategy,   // Always | OnDemand | Manual
}
```

### 4.3 PermissionManifest

```rust
pub struct PermissionManifest {
    pub filesystem: FsPermission,      // None | Read(scope) | Write(scope) | Scoped(...)
    pub network:    NetPermission,     // None | Allowlist(Vec<HostPattern>) | Full
    pub shell:      ShellPermission,   // None | Restricted(allowlist) | Full
    pub secrets:    Vec<SecretRef>,
    pub requires_human_approval: bool,
}
```

### 4.4 PluginManifest

```rust
pub struct PluginManifest {
    pub id: PluginId,
    pub name: String,
    pub version: SemVer,
    pub source: PluginSource,          // Native | Hermes | OpenClaw | Mcp | ClawHub
    pub provides: PluginProvides,
    pub requested_permissions: PermissionManifest,
    pub secrets: Vec<SecretRequirement>,
    pub provenance: Provenance,
}

pub struct PluginProvides {
    pub tools: Vec<ToolDescriptor>,
    pub skills: Vec<SkillDoc>,
    pub hooks: Vec<RunLifecycleHook>,
    pub commands: Vec<CommandAlias>,
    pub assets: Vec<PackageAsset>,
}
```

### 4.5 AgentConfig

```rust
pub struct AgentConfig {
    pub id: AgentId,
    pub name: String,
    pub system_prompt: String,
    pub model_policy: ModelPolicy,         // per-stage models (router, interpretation, per-tool override)
    pub tool_policy: ToolPolicy,           // visible tools + budgets + interpretation overrides
    pub skill_policy: SkillPolicy,
    pub memory_policy: MemoryPolicy,       // generation + loading separately, both off by default
    pub context_policy: ContextPolicy,     // compaction thresholds, ingestion gates
    pub execution_policy: ExecutionPolicy,
    pub approval_policy: ApprovalPolicy,
    pub prompt_refinement: Option<PromptRefinement>,
    pub guardrail: Option<GuardrailConfig>,
    pub storage_path: PathBuf,             // per-agent directory (cf. Shinkai v1 storage_path pattern)
    pub edited: bool,
}

pub struct ModelPolicy {
    pub default: ModelRef,
    pub router: Option<ModelRef>,
    pub interpretation_default: Option<ModelRef>,
    pub interpretation_per_tool: HashMap<ToolId, ModelRef>,
    pub guardrail: Option<ModelRef>,
    pub refinement: Option<ModelRef>,
    pub memory_generator: Option<ModelRef>,
}
```

### 4.6 ContextSnapshot

```rust
pub struct ContextSnapshot {
    pub system_prompt: String,
    pub conversation: Vec<Message>,
    pub compacted: Option<CompactedContext>,
    pub loaded_memory: Vec<MemoryFragment>,    // each carries Provenance
    pub visible_tools: Vec<ToolView>,          // at the chosen disclosure level
    pub visible_skills: Vec<SkillView>,
    pub limits: RuntimeLimits,                 // remaining tool-call budget, max tokens, etc.
    pub provenance: HashMap<FragmentId, ProvenanceLayer>, // for explain-config
}
```

### 4.7 RunEvent

```rust
pub struct RunEvent {
    pub id: EventId,
    pub run_id: RunId,
    pub parent_event: Option<EventId>,
    pub schema_version: u32,           // append-only event schema versioning
    pub at: Timestamp,
    pub kind: RunEventKind,
}

pub enum RunEventKind {
    RunStarted { agent: AgentId, input: UserInput },
    ContextBuilt(ContextSnapshot),
    LlmRequestStarted { model: ModelRef, request_digest: Hash },
    LlmStreamToken    { delta: String },
    LlmRequestCompleted { tokens_in: u32, tokens_out: u32, cost: Cost, duration_ms: u64 },
    ToolCallProposed  { tool: ToolId, input: Value },
    ToolApprovalRequested(ApprovalRequest),
    ToolApprovalDecided(ApprovalDecision),
    ToolCallStarted   { call_id: ToolCallId },
    ToolCallCompleted { call_id: ToolCallId, output: Value, side_effects: SideEffectRecord },
    ToolOutputInterpreted { call_id: ToolCallId, model: ModelRef, summary: String },
    SubagentStarted   { child_run: RunId, parent_call: ToolCallId },
    BatchItemStatus   { batch: BatchId, item_key: ItemKey, status: BatchItemStatus },
    MemoryRead        { backend: MemoryBackendId, fragment_ids: Vec<FragmentId> },
    MemoryWritten     { backend: MemoryBackendId, fragment_id: FragmentId, source_range: Option<MessageRange> },
    IngestionStarted  { source: SourceRef, backend: IngestionBackendId },
    IngestionCompleted{ artifact_id: ArtifactId, content_hash: Hash, sections: u32 },
    HookFired         { hook: HookId, payload_digest: Hash },
    ApprovalGate(ApprovalDecision),
    PolicyDenied(PolicyDenial),
    StopRequested(StopMode),
    StopCompleted { retained: RetainedContext },
    GuidanceInjected { content: String, between: ToolCallId },
    QualityScored { target: ScoreTarget, score: f32 },
    RunFailed(RunFailure),              // see §7
    RunCompleted { final_output: ArtifactRef, total_cost: Cost, total_duration_ms: u64 },
}
```

`schema_version` is non-optional. Trace consumers MUST refuse events from a higher schema version than they understand. Migration tools live in `agent-tracing`.

### 4.8 RunLifecycleHook

```rust
pub struct RunLifecycleHook {
    pub id: HookId,
    pub plugin: PluginId,
    pub triggers: Vec<HookTrigger>,    // OnRunStart, OnContextBuilt, OnToolProposed, OnRunComplete, ...
    pub handler: HookHandler,           // declarative, sandboxed
    pub permissions: PermissionManifest,
}
```

Hooks **observe** the event stream. They cannot mutate run state except through declared extension points (e.g. a `BeforeContextBuilt` hook can return additional fragments to merge, but the harness validates and decides).

---

## 5. Crate Map

```
crates/
  agent-core         domain model + orchestration; defines HarnessApi trait
  agent-api-client   shared client wrapper around HarnessApi (used by agent-cli + agent-tauri)
  agent-cli          ratatui TUI binary
  agent-tauri        Tauri v2 webapp shell + frontend bridge
  agent-daemon       (later) HTTP/WebSocket daemon exposing HarnessApi
  agent-llm          model providers (rig-core under the hood)
  agent-tools        executable tool runtimes (process, wasm later)
  agent-skills       SkillDoc registry + visibility resolution
  agent-memory       memory backends (builtin + plugin trait)        ← new
  agent-ingest       document ingestion backends + ingestion engine  ← new
  agent-adapters/
    agent-adapters-mcp        MCP via rmcp
    agent-adapters-openclaw   AgentSkills SKILL.md folders
    agent-adapters-clawhub    public registry source provider
    agent-adapters-hermes     plugin importer + external-agent adapter
  agent-storage      SQLite + filesystem layout for configs and runs
  agent-tracing      RunEvent stream, queries, schema migrations
  agent-sandbox      process boundary + permission enforcement
  agent-config       layered config resolution + explain-config provenance
  agent-secrets      OS credential store integration
```

---

## 6. Runtime Stack

```
            agent-cli (TUI)        agent-tauri (webapp)
                  │                       │
                  └────────┬──────────────┘
                           │ HarnessApi (trait)
                           ▼
                       agent-core
       ┌───────────────────┼────────────────────┐
       │           │       │       │            │
   context       run    policy   trace        memory
   builder      engine  engine   engine       engine
       │           │       │       │            │
       └───────────┴───────┴───────┴────────────┘
                           │
       ┌───────┬───────────┼───────────┬─────────┬───────────┐
       │       │           │           │         │           │
     tools   skills      llm       ingest    storage      sandbox
                                              + tracing  + secrets
                           │
                           ▼
                       adapters
        (MCP, OpenClaw, ClawHub, Hermes plugins, Hermes A2A)
```

**v1 (default):** TUI and Tauri webapp link `agent-core` directly. The HarnessApi trait is the only call site.
**Later:** `agent-daemon` exposes the same trait over HTTP/WS. UIs swap their transport without API change.

---

## 7. Run Lifecycle

### 7.1 State machine

```
pending
   │
   ▼
building_context
   │
   ▼
waiting_for_llm  ◄────────────────┐
   │                              │
   ├─► waiting_for_tool ──────────┤
   ├─► waiting_for_approval ──────┤
   ├─► waiting_for_subagent ──────┤
   ├─► waiting_for_ingestion ─────┤
   ├─► compacting (reserved) ─────┤
   └─► branching  (reserved) ─────┘
   │
   ▼
completed

any active state ─► stopping ─► stopped
                 ─► failed
```

`compacting` and `branching` are reserved states even though their full implementation is deferred per general §9 — the data model keeps the slot so later phases don't require a state-machine migration.

### 7.2 Streaming and cancellation contract

- Every async task in the run engine takes a `tokio::sync::CancellationToken`. Tasks check the token at every cooperative checkpoint: between LLM stream tokens, between tool calls in a loop, between batch items, between subagent steps.
- LLM streaming surfaces partial tokens as `LlmStreamToken` events. The UI consumes these incrementally. Cancellation between tokens stops streaming within ≤1 chunk.
- Tool calls in progress are cancelled via the runtime they belong to: process tools receive `SIGTERM` then `SIGKILL` after a configured grace period; HTTP/MCP tools are dropped (request future cancelled); Wasm tools (later) are halted via wasmtime fuel/epoch interruption.
- `StopRequested` is enqueued, not synchronous. The run transitions to `stopping`, completes any in-flight cooperative checkpoint cleanup, then emits `StopCompleted` with `RetainedContext` per the agent's stop policy (Discard or Summarise per gils §5.14).
- Mid-run guidance (`GuidanceInjected`) lands at the next safe checkpoint between tool calls — never mid-LLM-call, never mid-tool-execution.

### 7.3 Failure taxonomy

```rust
pub enum RunFailure {
    PolicyDenied(PolicyDenial),            // tool blocked by policy
    SchemaInvalid(ValidationFailure),      // tool input/output schema mismatch
    BudgetExhausted(BudgetKind),           // tokens / calls / wallclock / cost
    LlmError(LlmError),                    // provider 4xx/5xx/timeout/parse
    ToolError(ToolError),                  // non-zero exit / panic / timeout
    SandboxViolation(SandboxBreach),       // tool exceeded declared permissions
    ApprovalTimeout { request: ApprovalRequest, waited_ms: u64 },
    ApprovalRejected(ApprovalRequest),
    SubagentFailure { child_run: RunId, cause: Box<RunFailure> },
    BatchItemFailure { item_key: ItemKey, cause: Box<RunFailure> },
    UserStop(StopMode),
    Internal(InternalError),               // bug; always logged
}
```

Each variant has documented retry/resume semantics:

| Class | Retry | Resume from saved step | User-visible default |
| --- | --- | --- | --- |
| `PolicyDenied` | No | No | Surface to user |
| `SchemaInvalid` | No (LLM regenerates) | N/A | Loop allows ≤N retries before giving up |
| `BudgetExhausted` | No | No (user must raise budget) | Surface + offer resume with new budget |
| `LlmError` | Yes (per `RetryPolicy`) | N/A | Transparent retry, then surface |
| `ToolError` | Per tool's `RetryPolicy` | N/A | Surface to LLM (interpretation) or user (raw) |
| `SandboxViolation` | No | No | Surface + quarantine offending plugin |
| `ApprovalTimeout` | No | Yes (re-prompt) | Surface |
| `ApprovalRejected` | No | No | Surface |
| `SubagentFailure` | Per parent's policy | Yes | Bubble to parent |
| `BatchItemFailure` | Per `BatchPolicy` | Yes (per-item) | Mark item, continue batch |
| `UserStop` | No | Yes | Per stop mode |
| `Internal` | No | No | Crash report, surface |

---

## 8. Context Builder

```
config + conversation + compacted + memory + tools + skills + limits
                            │
                            ▼
                     ContextBuilder
                     (single code path)
                            │
                            ▼
                     ContextSnapshot
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
         preview                    LLM request
   (TUI / Tauri pane)            (executed by run engine)
```

Invariants:

- **The same `ContextBuilder::build()` produces the snapshot for both preview and execution.** It is impossible to diverge.
- Every fragment in the snapshot carries `Provenance` (which layer / which file / which conversation message). The TUI and Tauri preview panes render this.
- Memory enters via `MemoryFragment`s collected by `agent-memory::prefetch_all()` (informed by Hermes' prefetch pattern — see §13).
- Document-derived content enters via ingestion artifact references resolved by `agent-ingest`.
- The builder honours the visibility policy per tool/skill: if a tool is `NameOnly`, only `ToolView::name_only(...)` makes it into the snapshot.

---

## 9. Policy Engine

Layered precedence (matches general §8):

```
global  →  profile  →  agent  →  conversation  →  run / manual override
                                                           │
                                                           ▼
                                                  effective policy
```

For every effective value the engine records `ProvenanceLayer` so `agent explain-config` can show:

```
temperature = 0.7         (agent: research-asst.toml)
max_tool_calls = 10       (conversation override)
tool 'shell' visible      (global default; overridable)
memory loading on         (run-level toggle this turn)
```

In v1 the **profile** layer is a pass-through no-op (single main profile per gils §5.16). The slot exists in the precedence chain so multi-profile features can land later without restructuring the policy code.

Policy domains: models, tools, skills, visibility, interpretation, memory generation, memory loading, ingestion, context compaction, execution limits, approval, sandboxing, deletion.

---

## 10. Tool Pipeline & Hooks

```
ToolCallProposed
       │
       ▼
   policy check (allowed? visible? budget remaining?)
       │
       ▼
   schema validate (input)
       │
       ▼
   approval gate (if required)
       │
       ▼
   sandbox execute
       │
       ▼
   schema validate (output)
       │
       ▼
   store raw output ──► return raw (per InterpretationPolicy)
                  └──► LLM-interpret (per InterpretationPolicy)
```

Hooks fire at every numbered step. Plugin hooks observe `RunEvent`s; harness-internal hooks can extend specific steps (e.g. an `ApprovalProvider` hook implements the human-approval transport for the TUI vs Tauri vs daemon).

Failures at any gate emit a `RunEvent` and transition into the appropriate `RunFailure` variant per §7.3.

---

## 11. Memory Subsystem

Informed by review of Hermes' memory implementation (`/external/hermes/agent/memory_manager.py`, `/tools/memory_tool.py`).

### 11.1 Backend trait

```rust
pub trait MemoryBackend: Send + Sync {
    fn id(&self) -> MemoryBackendId;
    fn is_available(&self) -> bool;

    // Loading: called by ContextBuilder before each LLM call.
    fn prefetch(&self, ctx: &PrefetchContext) -> Result<Vec<MemoryFragment>>;

    // Generation: called after a turn IF generation is enabled for this agent/conversation.
    fn sync_turn(&self, evt: &TurnSummary) -> Result<Vec<MemoryWriteRecord>>;

    // Manual operations:
    fn create(&self, content: &str, meta: MemoryMeta) -> Result<MemoryFragment>;
    fn read(&self, id: FragmentId) -> Result<MemoryFragment>;
    fn update(&self, id: FragmentId, new_content: &str) -> Result<MemoryFragment>;
    fn delete(&self, id: FragmentId) -> Result<()>;

    fn tool_schemas(&self) -> Vec<ToolDescriptor>;        // optional agent-callable tools
    fn handle_tool_call(&self, call: ToolCall) -> Result<Value>;
}
```

### 11.2 v0 (MVP) behaviour

- **Built-in backend only**, file-based, human-readable. Dual files per agent storage_path:
  - `memory.md` — agent's working knowledge (environment facts, project conventions, learned constraints).
  - `user.md` — who the user is (preferences, communication style, etc.) — adopted from Hermes' MEMORY/USER split.
- Entries delimited by a sentinel (e.g. `---`) for stable diffs.
- **Generation v0 is manual.** User selects a conversation range and triggers generation via TUI keystroke or `agent memory generate --range ...`. No async, no cron, no auto-extraction (those are deferred per general §9).
- **Loading v0 is opt-in** at agent and run level. When enabled, memory enters context as a fenced block (`<memory-context>...</memory-context>` adopted from Hermes) with a system note distinguishing recall from user input.
- Every read emits `MemoryRead`, every write emits `MemoryWritten` with source range and generating model recorded.
- **Injection scanning on writes.** Any string about to land in a memory file is scanned for prompt-injection / exfiltration patterns (adopted from Hermes' `tools/memory_tool.py:65-103`).
- **Versioning v0**: each write produces a `.bak` of the prior file (1-step rollback per gils §5.10.4). Deeper history deferred.

### 11.3 What we deliberately diverge from Hermes

- Hermes makes the memory tool always callable when the backend is enabled. We make **generation off by default, separately from loading**, both controllable per agent / per conversation / per run.
- Hermes does not record provenance; we record source conversation range, generating model, and human-vs-AI authorship in `MemoryWritten`.
- Hermes has no rollback. We ship 1-step rollback in v0.
- Hermes' frozen-snapshot pattern (system prompt baked at session start) is good for cache hit rate but conflicts with our run-level loading toggle. v0 resolves this by binding the snapshot at **run** boundary, not session — every new `Run` rebuilds context (already an invariant per §8) and picks up memory toggles for that run.

### 11.4 Plugin trait (deferred to phase 4+)

The trait above is the future plugin contract. Adapters for Mem0, Letta, LangMem, or Hermes' Honcho/Hindsight/Holographic backends slot in as additional `MemoryBackend` implementations.

---

## 12. Document Ingestion Subsystem

### 12.1 Backend trait

```rust
pub trait IngestionBackend: Send + Sync {
    fn id(&self) -> IngestionBackendId;
    fn supports(&self, source: &SourceRef) -> bool;

    fn ingest(&self, source: &SourceRef, opts: IngestOpts) -> Result<IngestionArtifact>;
}

pub struct IngestionArtifact {
    pub id: ArtifactId,
    pub source: SourceRef,
    pub backend: IngestionBackendId,
    pub content_hash: Hash,
    pub sections: Vec<Section>,         // ordered; each carries an offset/range
    pub extracted_text: Option<String>,
    pub tables: Vec<TableExtract>,
    pub assets: Vec<AssetRef>,           // images, figures
    pub created_at: Timestamp,
}
```

### 12.2 v0 (MVP) behaviour

- **Default local backend** for plain text, markdown, code, and PDF text extraction (no vision, no layout-aware).
- Ingestion is **explicit**: invoked via `agent ingest <path>` or a tool the user/agent calls. The runtime never silently pulls files into context (default posture per general §3).
- Artifacts are inspectable before they enter context: `agent ingest show <artifact_id>` and a TUI/Tauri pane render extracted text/sections.
- Inclusion in context is **gated by policy** — an artifact can only enter `ContextSnapshot` via an explicit reference resolved by the ingestion engine, not by being in the workspace.
- `IngestionStarted` and `IngestionCompleted` events emit on the same trace as the run that requested them.
- Artifacts can be deleted (`agent ingest rm`) and re-ingested with a different backend (`agent ingest <path> --backend <id>`).

### 12.3 Deferred

Vision-based OCR, complex layout/table/graph extraction, per-modality routing, remote ingestion, an ingestion-backend marketplace. The trait is designed so each lands as a new backend without runtime changes.

---

## 13. Subagents and Batch

### 13.1 Subagents

```
root run
   ├─ child run: subagent A
   ├─ child run: subagent B
   │      ├─ child run: subagent B.1
   │      └─ child run: subagent B.2
   └─ child run: subagent C
```

A subagent is a `ToolDescriptor` with `runtime: Subagent(AgentRef)`. Calling it spawns a `ChildRun` linked to the parent in the trace. ChildRun events stream into the parent's view (gils §5.6 observability requirement).

**Cycle detection.** Every ChildRun carries the chain `[parent_agent_id, grandparent_agent_id, ...]`. Spawning a ChildRun whose `agent_id` already appears in the chain raises `RunFailure::PolicyDenied(Recursion)`. Configurable: agents can opt into bounded recursion via `ExecutionPolicy::max_recursion_depth`, distinct from `max_subagent_depth`.

### 13.2 Batch

```rust
pub struct BatchPlan {
    pub items: Vec<BatchItem>,
    pub policy: BatchPolicy,
}

pub struct BatchItem {
    pub key: ItemKey,        // primary key for retry/resume identity (see below)
    pub input: Value,
}
```

**Item primary key.** `ItemKey` defaults to `Hash(canonical(input))` for deterministic dedup, but callers can override with a caller-supplied id (e.g. file path, S3 key). Retry/resume operates on `ItemKey`, not on iteration index. A batch can be paused, persisted, and resumed; items already marked `Succeeded` are skipped.

The runtime owns the loop (gils §5.21). LLM-driven plans are explicitly rejected for batch mode.

---

## 14. Configuration Model

This section explicitly inverts Shinkai v1's anti-patterns (binary blob exports, opaque API-only state, single LLM per agent, always-on defaults, no explain-config).

### 14.1 On-disk layout

```
~/.config/agent-harness/
  config.toml                    # global defaults
  profiles/
    main/                        # one main profile (multi-profile deferred)
      profile.toml
      agents/
        research-asst/
          agent.toml
          memory.md              # v0 memory file
          user.md
          .bak/                  # 1-step rollback snapshots
        ...
      models/
        gpt-5.toml
        local-qwen3.toml
        ...
      tools/
        my-tool.toml
        ...
      skills/
        readme-style.md          # SkillDoc native format
        ...
      conversations/
        2026-04-24-foo/          # conversation state
      prompts/                   # saved prompt library (gils §5.25)
  cache/
    ingestion/                   # extracted artifacts
  state.sqlite                   # RunEvent stream + indexes
```

Configs are **TOML**, human-readable, diff-friendly. Skills are markdown with YAML frontmatter (compatible with AgentSkills format per general §7.3). Memory is markdown.

### 14.2 Layered resolution

`agent-config` walks: `config.toml` → `profile.toml` → `agent.toml` → conversation overrides → run overrides → manual flags. Every effective value is tagged with the `ProvenanceLayer` it came from; `agent explain-config <agent>` prints the resolved table with provenance.

### 14.3 Export / import

Bundles are **TOML/markdown/JSON inside a tarball**, never opaque blobs. Manifest:

```toml
# manifest.toml
schema_version = 1
exported_at = "2026-04-24T..."
source_profile = "main"
contents = ["agents/research-asst", "models/gpt-5", "tools/my-tool", ...]
```

`schema_version` is mandatory. Imports refuse versions newer than they understand and offer migration for older versions.

---

## 15. Storage and Trace Durability

### 15.1 Split

| Storage | What | Where |
| --- | --- | --- |
| User config | agents, tools, skills, models, prompts, profiles | `~/.config/agent-harness/` (TOML/markdown) |
| Runtime state | runs, events, conversations, messages, snapshots, approvals, costs, batch items | `state.sqlite` |
| Artifacts | generated docs, ingested chunks, large outputs, export bundles | `~/.config/agent-harness/cache/...` and `~/.config/agent-harness/profiles/main/conversations/.../artifacts/` |

### 15.2 RunEvent durability

- `RunEvent` rows are append-only. The schema is `(id INTEGER PK, run_id TEXT, parent_event INTEGER NULL, schema_version INT, at TIMESTAMP, kind TEXT, payload BLOB)`.
- Writes are batched per run within a transaction; `fsync` happens on:
  1. transition to `completed`, `failed`, `stopped`
  2. before any approval-blocking wait
  3. before any side-effecting tool call (so a crash leaves the proposal recorded)
- On startup, runs in `pending`, `building_context`, `waiting_*`, `compacting`, `branching`, or `stopping` are recovered to `failed(Internal::Crash)` unless a tool has explicit resume support.

### 15.3 Event schema versioning

`RunEvent::schema_version` is the version that wrote the event. `agent-tracing` ships a forward-only migration table. Readers refuse events from a version they don't know.

---

## 16. Adapter Layer

The pipeline (general §7.4):

```
external package
   ▼ detect
   ▼ inspect           → PackageInspection
   ▼ normalize         → NormalizedPackage (ToolDescriptor, SkillDoc, Hooks, Secrets)
   ▼ validate          → schema, references
   ▼ static scan       → SecurityFinding[]
   ▼ quarantine        → off by default
   ▼ user/agent/profile approval
   ▼ native registry
```

Per-source mapping (mirrored from general §7.3 — this is the implementation contract for the adapter crates):

| Source | Crate | Maps to |
| --- | --- | --- |
| OpenClaw / AgentSkills `SKILL.md` | `agent-adapters-openclaw` | `SkillDoc { format: AgentSkills, trust: Unreviewed }` |
| ClawHub registry | `agent-adapters-clawhub` | source provider; emits `PluginManifest` with `source: ClawHub` |
| Hermes plugin (`plugin.yaml`) | `agent-adapters-hermes` | `provides_tools` → `ToolDescriptor`; hooks → `RunLifecycleHook`; bundled skills → `SkillDoc`; env → `SecretRequirement` |
| Hermes toolset | `agent-adapters-hermes` | category/pack |
| Hermes external agent | `agent-adapters-hermes` | `ToolDescriptor { runtime: Subagent(External(Hermes)) }` |
| MCP server | `agent-adapters-mcp` | `ToolDescriptor { runtime: Mcp(...) }` |
| OpenAI-compatible function | (in `agent-tools`) | `ToolDescriptor` lowest-common-denominator schema |

Adapters never bypass policy, sandbox, approval, or trace events. They produce `NormalizedPackage`s that the harness chooses whether to enable.

---

## 17. Security Boundary and Sandbox

### 17.1 Layers

```
UI layer (TUI / Tauri webapp)
       │   no secrets reach here by default (redacted in trace views too)
       ▼
agent-core / agent-daemon
       │   policy checked
       ▼
agent-sandbox
       │   scoped permissions enforced
       ▼
tool execution (process / wasm / mcp / http)
```

### 17.2 v1 enforcement floor

The sandbox is implemented via process boundary in v1 (wasmtime later). The minimum enforced contract:

| Permission | v1 enforcement |
| --- | --- |
| `FsPermission::None` | tool runs in an empty temp dir; no env-passed paths |
| `FsPermission::Read(scope)` | bind-mount / chroot-equivalent on Linux/mac (using sandbox-exec on macOS, bubblewrap or rootless namespaces on Linux); explicit allowlist on Windows via Job Object + restricted token |
| `FsPermission::Write(scope)` | same as Read but writable region scoped |
| `NetPermission::None` | deny by default — Linux netns, macOS `sandbox-exec` `(deny network*)`, Windows WFP rule on the child PID |
| `NetPermission::Allowlist` | egress allowlist via the same per-OS mechanism |
| `ShellPermission::None` | no `sh`/`cmd.exe` available in the tool's PATH |
| `ShellPermission::Restricted(allowlist)` | shim shell wrapper that only forwards allowlisted commands |
| `secrets` | injected via env vars per `SecretRef`; redacted from trace, never echoed in logs |

The cross-platform reality is that Windows enforcement is the weakest of the three. v1 ships with explicit warnings on Windows when a permission cannot be enforced as strictly as on macOS/Linux, and the trace records the enforcement level per call.

### 17.3 Posture

```
install != trust
trust   != global availability
allowed != execution without policy
```

---

## 18. Secrets Lifecycle

```
declare           PluginManifest.requested_permissions.secrets[]
       │
       ▼
provide           agent-secrets (OS keychain: macOS Keychain, Windows Credential Manager, libsecret on Linux)
       │
       ▼
resolve           per tool call, agent-secrets returns a `SecretHandle` (not the value)
       │
       ▼
inject            sandbox dereferences SecretHandle into env vars at exec time
       │
       ▼
redact            trace events store `SecretHandle::id`, never the value; UI views redact
       │
       ▼
rotate            `agent secrets rotate <id>` updates the keychain entry; live runs continue with the old value
```

Secrets never enter the LLM prompt. The context builder rejects fragments that match secret patterns from the secret store.

---

## 19. Cost Accounting

Aggregation tree:

```
RunCompleted.total_cost
   = sum( LlmRequestCompleted.cost in this run )
   + sum( ChildRun.RunCompleted.total_cost where parent = this run )
   + sum( ToolCallCompleted.cost where tool declares cost in ExecutionPolicy )
```

For batch:

```
BatchCompleted.total_cost = sum( ChildRun.RunCompleted.total_cost over items )
```

Per-LLM cost rates are user-configurable per model (general §5.22). The trace stores raw token counts, so re-pricing after a rate update is a query, not a re-run.

---

## 20. TUI and Tauri Surfaces

### 20.1 TUI (`agent-cli`)

- Built on `ratatui` + `crossterm`.
- Multi-pane layout (configurable):
  - Transcript pane (LLM stream + tool calls)
  - Context-preview pane (renders `ContextSnapshot` with provenance, on demand per gils §5.12)
  - Status bar (cost, tokens, time, remaining tool-call budget, current state)
  - Tool/skill tray (visible tools at chosen disclosure level)
- Slash-command layer for accessibility shortcuts (gils §5.25):
  - `/tool <name> [<args>]` — direct call, LLM-filled inputs
  - `/tool!<name> {field=value...}` — direct call, **manual** inputs (gils explicitly contrasts this with Shinkai v1)
  - `/agent <id>` — switch agent
  - `/run <saved-prompt>` — saved prompt library
  - `/score <n>` — quality score on last answer
  - `/guide <text>` — mid-run guidance injection
- Stop key (Ctrl+C debounced; `Esc` for cooperative stop).
- Headless `--print` mode emits structured JSON for scripting/CI.
- Cross-terminal: targets Windows Terminal, iTerm2, gnome-terminal, kitty, alacritty. cmd.exe and old PowerShell-without-VT get a degraded mode warning.

### 20.2 Tauri webapp (`agent-tauri`)

- Tauri v2.
- WebView per platform: WebView2 (Win), WKWebView (mac), WebKitGTK (Linux).
- Frontend framework: TBD (React leading; decision tracked separately).
- Bridges into `agent-core` via `agent-api-client`. v1 is in-process; future daemon transport is drop-in.
- Permissions / capabilities (Tauri v2 model) are deny-by-default. Frontend gets only the `HarnessApi` commands; no direct FS/shell.

### 20.3 Cross-platform packaging

| Concern | Plan |
| --- | --- |
| WebView2 (Windows) | Default to system WebView2; offer offline installer build for enterprise |
| Code signing (Windows) | EV cert in v1 budget; SmartScreen warnings unacceptable for a security-positioned tool |
| Notarization (macOS) | Full notarization + hardened runtime; Gatekeeper-clean |
| Linux packaging | `.deb`, `.rpm`, AppImage; Flatpak considered |
| Auto-update | Tauri updater plugin with signed manifests; CLI uses the same signing |
| Sidecar | The runtime can ship as a sidecar to the Tauri app for users who only want the GUI |

---

## 21. Build Sequence

This sequence aligns with general §9 (memory v0 + ingestion v0 in MVP) and general §7.6 (adapter rollout order).

| Phase | Core | TUI | Tauri | Adapters | Other |
| --- | --- | --- | --- | --- | --- |
| **1. API + scaffolds** | `HarnessApi` trait, fake provider, fake tool, `agent-storage` skeleton, `agent-tracing` event store, `agent-config` layered resolution | ratatui scaffold connecting to `HarnessApi`, transcript + status pane, headless `--print` mode | Tauri v2 scaffold connecting to `HarnessApi`, single-page transcript view | — | Cross-platform CI (Win/Mac/Linux) builds + signed-package stubs |
| **2. Real LLM + tools** | real LLM via `agent-llm` (rig-core), process tool runtime in `agent-tools`, raw + interpreted output paths | tool tray, slash-command layer (`/tool`, `/tool!`, `/agent`), context-preview pane | tool list, raw/interpreted output toggle, context preview | — | `explain-config` |
| **3. Memory v0 + Ingestion v0** | `agent-memory` builtin backend (memory.md / user.md), prefetch/sync split, `agent-ingest` with default local backend, ingestion-before-context gate | memory pane, ingestion review pane, `/score`, `/guide` | memory + ingestion screens | — | Backup/export bundles (TOML/MD inside tarball, schema_version=1) |
| **4. Subagents + batch + approvals + stop/resume** | ChildRun, BatchRun with item-key resume, approval gates, cooperative cancellation | approval prompts, batch progress view, stop/resume controls | approval modals, batch dashboard | — | OpenClaw/AgentSkills text-skill importer |
| **5. MCP + ClawHub** | — | quarantine UI for new skills | quarantine UI | `agent-adapters-mcp` (rmcp), `agent-adapters-clawhub` source provider | Static scan rules |
| **6. Hermes plugins + external-agent** | hooks (`RunLifecycleHook`) | hook visibility | hook visibility | `agent-adapters-hermes` plugin importer + external-agent adapter | Sandbox v1.1 (better Windows enforcement) |
| **7. Daemon + remote** | `agent-daemon` extracts the `HarnessApi` trait over HTTP/WS; UIs swap transports | TUI talks to remote daemon | Tauri talks to remote daemon | — | A2A adapter (when ecosystem stabilises) |
| **8+. Deferred per general §9** | profiles (multi-), payments, voice, mobile/messaging, advanced memory (async/cron, embeddings, cross-profile), advanced ingestion (vision, layout, tables), branching, quality-scoring rollups, agent-created tools/skills | | | | |

The MVP (phase 1 + 2 + 3) ships an inspectable, bounded, traced agent runtime with manual memory and explicit ingestion, on three platforms, on two UIs.

---

## 22. Test Shape

```
fake LLM provider + fake tool + fake memory backend + fake ingestion backend
                            │
                            ▼
       deterministic runtime tests against agent-core via HarnessApi
                            │
       ┌────────────────────┼────────────────────┐
       │                    │                    │
   policy precedence    tool budget         schema versioning
   context snapshots    raw vs interp.      streaming/cancel
   event ordering       stop behaviour      failure taxonomy
   approval gates       adapter quarantine  sandbox enforcement
   memory prefetch      ingestion gating    cost roll-up
```

**Both UIs test against the same fake `HarnessApi` implementation.** TUI snapshot tests use `insta` against rendered terminal frames. Tauri tests run the frontend against a fake `HarnessApi` exposed via the Tauri bridge.

The runtime must be testable end-to-end without paid LLM calls or real shell execution. CI runs the full matrix on Windows / macOS / Linux from phase 1.
