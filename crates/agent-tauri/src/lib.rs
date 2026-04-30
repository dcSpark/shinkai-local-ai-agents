//! Tauri v2 backend for the Agent Harness webapp.
//!
//! Wires the same `agent-core::HarnessApi` as the TUI does (see
//! `specs/architecture.md` §3 — three-surface architecture). This crate is one
//! of two day-one UI clients of the runtime; the other is `agent-cli`.
//!
//! v0 surface:
//! - One `tauri::command`: `run_agent(input, demo) -> RunSummary`.
//! - Streams every `RunEvent` to the frontend via `app.emit("run-event", _)`.
//!
//! Slash commands, multiple agents, configurable models, and the rest of
//! `general_requirements.md` §5 land in later slices.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use agent_adapters::{AdapterRegistry, NormalizedPackage, inspect_source};
use agent_batch::{BatchItemState, BatchPlan};
use agent_bundles::{BundleManifest, export_bundle, import_bundle};
use agent_config::ConfigResolver;
use agent_core::{
    AgentConfig, ApprovalMode, ConfigExplanation, ContextSnapshot, CostPolicy, Harness, HarnessApi,
    IngestedArtifactView, RunResult, ToolOutputMode, ToolPolicy, ToolView, UserInput,
    VisibilityLevel,
};
use agent_ingest::{IngestionArtifact, IngestionStore};
use agent_llm::{FakeProvider, FakeStep, LlmProvider, ModelRef, RigProvider, RigProviderConfig};
use agent_memory::{MemoryAuthor, MemoryRecord, MemoryStore, MemoryTarget};
use agent_skills::{SkillDoc, SkillRegistry};
use agent_storage::StoragePaths;
use agent_tools::{FakeTool, ShellTool, ShellToolConfig, SubagentTool, ToolId, ToolRegistry};
use agent_tracing::{
    EventStore, PublishingEventStore, RunEvent, RunEventKind, RunId, SqliteEventStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio::task::AbortHandle;

#[derive(Default)]
struct AppState {
    active_runs: Arc<tokio::sync::Mutex<HashMap<String, AbortHandle>>>,
}

#[derive(Serialize)]
struct RunSummary {
    run_id: String,
    final_output: String,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Demo {
    Echo,
    Tool,
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
enum ProviderKind {
    #[default]
    Fake,
    Rig,
}

#[derive(Deserialize, Clone)]
#[serde(default)]
struct RunOptions {
    provider: ProviderKind,
    model: Option<String>,
    api_base_url: Option<String>,
    api_key_env: String,
    api_key: Option<String>,
    max_output_tokens: Option<u64>,
    temperature: Option<f64>,
    max_tool_calls: Option<u32>,
    tool_visibility: Option<VisibilityLevel>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    enable_shell: bool,
    enable_subagent: bool,
    load_memory: bool,
    load_skills: bool,
    include_ingest: Vec<String>,
    require_approval: bool,
    raw_tool_output: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Fake,
            model: None,
            api_base_url: None,
            api_key_env: "OPENAI_API_KEY".into(),
            api_key: None,
            max_output_tokens: None,
            temperature: None,
            max_tool_calls: None,
            tool_visibility: None,
            input_cost_per_million: None,
            output_cost_per_million: None,
            enable_shell: false,
            enable_subagent: false,
            load_memory: false,
            load_skills: false,
            include_ingest: Vec::new(),
            require_approval: false,
            raw_tool_output: false,
        }
    }
}

fn build_provider(
    demo: Demo,
    input: &str,
    options: &RunOptions,
) -> Result<Arc<dyn LlmProvider>, String> {
    match options.provider {
        ProviderKind::Fake => Ok(match demo {
            Demo::Echo => Arc::new(FakeProvider::echo()),
            Demo::Tool => Arc::new(FakeProvider::sequence(vec![
                FakeStep::CallTool {
                    id: "call-1".into(),
                    tool: "echo".into(),
                    input: serde_json::json!({"text": input}),
                },
                FakeStep::Reply(format!("[fake] tool said: {input}")),
            ])),
        }),
        ProviderKind::Rig => {
            let model = rig_model_id(options);
            let model_runtime = ConfigResolver::from_env()
                .resolve_model_runtime(&model)
                .ok()
                .flatten();
            let config = RigProviderConfig {
                api_base_url: options.api_base_url.clone(),
                api_key_env: options.api_key_env.clone(),
                model: ModelRef::from(model),
                max_output_tokens: options.max_output_tokens.or_else(|| {
                    model_runtime
                        .as_ref()
                        .and_then(|model| model.max_output_tokens)
                }),
                temperature: options.temperature.or_else(|| {
                    model_runtime
                        .as_ref()
                        .and_then(|model| model.default_temperature)
                }),
            };
            RigProvider::from_config_with_api_key_override(config, options.api_key.clone())
                .map(|provider| Arc::new(provider) as Arc<dyn LlmProvider>)
                .map_err(|e| e.to_string())
        }
    }
}

fn rig_model_id(options: &RunOptions) -> String {
    if let Some(model) = options.model.clone() {
        return model;
    }
    let configured = ConfigResolver::from_env()
        .resolve_default_agent()
        .map(|resolved| resolved.agent.model.0)
        .unwrap_or_else(|_| "fake-model".into());
    if configured == "fake-model" {
        "gpt-4o-mini".into()
    } else {
        configured
    }
}

fn build_registry(enable_shell: bool, enable_subagent: bool) -> Arc<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    registry.register(FakeTool::echo_descriptor(), Arc::new(FakeTool::echo()));
    if enable_shell {
        registry.register(
            ShellTool::descriptor(),
            Arc::new(ShellTool::new(ShellToolConfig::default())),
        );
    }
    if enable_subagent {
        registry.register(SubagentTool::descriptor(), Arc::new(SubagentTool));
    }
    Arc::new(registry)
}

fn build_agent(options: &RunOptions) -> AgentConfig {
    let mut agent = ConfigResolver::from_env()
        .resolve_default_agent()
        .map(|resolved| resolved.agent)
        .unwrap_or_else(|_| AgentConfig {
            id: "fake-agent".into(),
            name: "Fake Agent".into(),
            system_prompt: "You echo what the user says.".into(),
            model: ModelRef::from("fake-model"),
            tool_policy: ToolPolicy::default(),
            cost_policy: CostPolicy::default(),
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            skill_views: Vec::new(),
        });

    if let Some(model) = options.model.clone() {
        agent.model = ModelRef::from(model);
    } else if matches!(options.provider, ProviderKind::Rig) && agent.model.0 == "fake-model" {
        agent.model = ModelRef::from("gpt-4o-mini");
    }
    if let Some(max_tool_calls) = options.max_tool_calls {
        agent.tool_policy.max_calls = max_tool_calls;
    }
    if let Some(visibility) = options.tool_visibility {
        agent.tool_policy.visibility = visibility;
    }
    if options.require_approval {
        agent.tool_policy.approval_mode = ApprovalMode::RequireExplicit;
    }
    if options.raw_tool_output {
        agent.tool_policy.output_mode = ToolOutputMode::Raw;
    }
    if options.input_cost_per_million.is_some() {
        agent.cost_policy.input_cost_per_million = options.input_cost_per_million;
    }
    if options.output_cost_per_million.is_some() {
        agent.cost_policy.output_cost_per_million = options.output_cost_per_million;
    }
    if options.load_memory
        && let Ok(memory) = MemoryStore::from_env().load_fragments()
    {
        agent.memory_fragments = memory;
    }
    if options.load_skills
        && let Ok(skills) = SkillRegistry::from_env().visible_skill_views()
    {
        agent.skill_views = skills;
    }
    if !options.include_ingest.is_empty() {
        let store = IngestionStore::from_env();
        agent.ingestion_artifacts = options
            .include_ingest
            .iter()
            .filter_map(|id| store.show(id).ok())
            .map(|artifact| IngestedArtifactView {
                id: artifact.id,
                source: artifact.source.display().to_string(),
                sections: artifact.sections.len(),
                content: artifact.extracted_text.unwrap_or_default(),
                provenance: "agent.ingest explicit reference".into(),
            })
            .collect();
    }

    agent
}

#[tauri::command]
async fn run_agent(
    app: AppHandle,
    state: State<'_, AppState>,
    input: String,
    demo: Demo,
    options: RunOptions,
) -> Result<RunSummary, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<RunEvent>();

    let provider = build_provider(demo, &input, &options)?;
    let store = Arc::new(PublishingEventStore::new(open_event_store()?, tx));
    let harness = Harness::new(
        provider,
        store,
        build_registry(options.enable_shell, options.enable_subagent),
    );
    let agent = build_agent(&options);

    let run_task = tokio::spawn(async move {
        let result = harness.run(&agent, UserInput { text: input }).await;
        drop(harness);
        result
    });
    let abort_handle = run_task.abort_handle();

    // Forward every published RunEvent to the frontend until the channel
    // closes (i.e. until the harness drops its sender). Best-effort: emit
    // failures are ignored so the run still completes.
    let active_runs = state.active_runs.clone();
    let forward = tokio::spawn(async move {
        let mut seen_run_id = None::<String>;
        while let Some(evt) = rx.recv().await {
            let run_id = evt.run_id.0.to_string();
            match &evt.kind {
                RunEventKind::RunStarted { .. } => {
                    seen_run_id = Some(run_id.clone());
                    active_runs
                        .lock()
                        .await
                        .insert(run_id.clone(), abort_handle.clone());
                }
                RunEventKind::RunPaused { .. }
                | RunEventKind::RunCancelled { .. }
                | RunEventKind::RunCompleted { .. }
                | RunEventKind::RunFailed { .. } => {
                    active_runs.lock().await.remove(&run_id);
                }
                _ => {}
            }
            let _ = app.emit("run-event", evt);
        }
        if let Some(run_id) = seen_run_id {
            active_runs.lock().await.remove(&run_id);
        }
    });

    let result: Result<RunResult, String> = match run_task.await {
        Ok(result) => result.map_err(|e| e.to_string()),
        Err(err) if err.is_cancelled() => Err("run cancelled".to_string()),
        Err(err) => Err(err.to_string()),
    };
    let _ = forward.await;

    match result {
        Ok(r) => Ok(RunSummary {
            run_id: r.run_id.0.to_string(),
            final_output: r.final_output,
        }),
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn preview_context(input: String, options: RunOptions) -> Result<ContextSnapshot, String> {
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store()?),
        build_registry(options.enable_shell, options.enable_subagent),
    );
    Ok(harness.preview_context(&build_agent(&options), UserInput { text: input }))
}

#[tauri::command]
async fn explain_config(_options: RunOptions) -> Result<ConfigExplanation, String> {
    let resolved = ConfigResolver::from_env()
        .resolve_default_agent()
        .map_err(|e| e.to_string())?;
    Ok(ConfigExplanation {
        agent_id: resolved.agent.id,
        values: resolved.values,
    })
}

#[tauri::command]
async fn explain_tools(options: RunOptions) -> Result<Vec<ToolView>, String> {
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store()?),
        build_registry(options.enable_shell, options.enable_subagent),
    );
    Ok(harness.explain_tools(&build_agent(&options)))
}

#[tauri::command]
async fn call_tool(name: String, input: Value, options: RunOptions) -> Result<Value, String> {
    let enable_shell = options.enable_shell || name == "shell";
    let enable_subagent = options.enable_subagent || name == "subagent";
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store()?),
        build_registry(enable_shell, enable_subagent),
    );
    let agent = build_agent(&RunOptions {
        enable_shell,
        enable_subagent,
        ..options
    });
    harness
        .call_tool(&agent, ToolId::from(name), input)
        .await
        .map(|result| result.output)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn trace_show(run_id: String) -> Result<Vec<RunEvent>, String> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id).map_err(|e| e.to_string())?);
    open_event_store()?
        .try_events(run_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn approval_list(run_id: String) -> Result<Vec<Value>, String> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id).map_err(|e| e.to_string())?);
    approvals_for_run(run_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn approval_decide(
    run_id: String,
    approval_id: String,
    approved: bool,
) -> Result<(), String> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id).map_err(|e| e.to_string())?);
    open_event_store()?.append(
        run_id,
        None,
        RunEventKind::ApprovalResolved {
            approval_id,
            approved,
        },
    );
    Ok(())
}

#[tauri::command]
async fn approval_execute(approval_id: String, run_id: String) -> Result<Value, String> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id).map_err(|e| e.to_string())?);
    let store = open_event_store()?;
    let events = store.try_events(run_id).map_err(|e| e.to_string())?;
    let approved = events.iter().rev().find_map(|event| match &event.kind {
        RunEventKind::ApprovalResolved {
            approval_id: id,
            approved,
        } if id == &approval_id => Some(*approved),
        _ => None,
    });
    if approved != Some(true) {
        return Err(format!(
            "approval {approval_id} is not approved for run {}",
            run_id.0
        ));
    }
    let proposed_id = events
        .iter()
        .find_map(|event| match &event.kind {
            RunEventKind::ApprovalRequested {
                approval_id: id, ..
            } if id == &approval_id => event.parent_event,
            _ => None,
        })
        .ok_or_else(|| format!("approval {approval_id} is not linked to a tool proposal"))?;
    let (call_id, tool_id, input) = events
        .iter()
        .find_map(|event| {
            if event.id != proposed_id {
                return None;
            }
            match &event.kind {
                RunEventKind::ToolCallProposed {
                    call_id,
                    tool_id,
                    input,
                } => Some((call_id.clone(), tool_id.clone(), input.clone())),
                _ => None,
            }
        })
        .ok_or_else(|| format!("tool proposal for approval {approval_id} was not found"))?;
    if events.iter().any(|event| {
        matches!(
            &event.kind,
            RunEventKind::ToolCallCompleted {
                call_id: id,
                ..
            } if id == &call_id
        )
    }) {
        return Err(format!("tool call {call_id} has already completed"));
    }

    let registry = build_registry(tool_id == "shell", tool_id == "subagent");
    store.append(
        run_id,
        Some(proposed_id),
        RunEventKind::ToolCallStarted {
            call_id: call_id.clone(),
        },
    );
    let started = Instant::now();
    let output = registry
        .execute(&ToolId::from(tool_id), input)
        .await
        .map_err(|e| e.to_string())?;
    let duration_ms = started.elapsed().as_millis() as u64;
    store.append(
        run_id,
        Some(proposed_id),
        RunEventKind::ToolCallCompleted {
            call_id,
            output: output.clone(),
            cost_usd: None,
            duration_ms,
        },
    );
    store.append(
        run_id,
        None,
        RunEventKind::RunCompleted {
            final_output: serde_json::to_string(&output).map_err(|e| e.to_string())?,
            total_cost_usd: None,
            total_duration_ms: duration_ms,
        },
    );
    Ok(output)
}

#[tauri::command]
async fn guide(run_id: String, text: String) -> Result<(), String> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id).map_err(|e| e.to_string())?);
    open_event_store()?.append(
        run_id,
        None,
        RunEventKind::GuidanceInjected { content: text },
    );
    Ok(())
}

#[tauri::command]
async fn cancel(
    app: AppHandle,
    state: State<'_, AppState>,
    run_id: String,
    reason: String,
) -> Result<(), String> {
    let parsed_run_id = RunId(uuid::Uuid::parse_str(&run_id).map_err(|e| e.to_string())?);
    let event = open_event_store()?.append(
        parsed_run_id,
        None,
        RunEventKind::RunCancelled {
            reason: reason.clone(),
        },
    );
    let _ = app.emit("run-event", event);
    if let Some(handle) = state.active_runs.lock().await.remove(&run_id) {
        handle.abort();
    }
    Ok(())
}

#[tauri::command]
async fn score(run_id: String, target: String, score: f32) -> Result<(), String> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id).map_err(|e| e.to_string())?);
    open_event_store()?.append(run_id, None, RunEventKind::QualityScored { target, score });
    Ok(())
}

#[tauri::command]
async fn batch_run(items: Vec<String>, demo: Demo, options: RunOptions) -> Result<Value, String> {
    let batch_run_id = RunId::new();
    let batch_id = format!("batch-{}", batch_run_id.0);
    let mut plan = BatchPlan::new(batch_id.clone(), items);
    plan.save_to_env().map_err(|e| e.to_string())?;
    execute_batch_plan(plan, batch_run_id, batch_id, demo, options).await
}

#[tauri::command]
async fn batch_resume(batch_id: String, demo: Demo, options: RunOptions) -> Result<Value, String> {
    let batch_run_id = RunId::new();
    let plan = BatchPlan::load_from_env(&batch_id).map_err(|e| e.to_string())?;
    execute_batch_plan(plan, batch_run_id, batch_id, demo, options).await
}

async fn execute_batch_plan(
    mut plan: BatchPlan,
    batch_run_id: RunId,
    batch_id: String,
    demo: Demo,
    options: RunOptions,
) -> Result<Value, String> {
    let store = Arc::new(open_event_store()?);
    store.append(
        batch_run_id,
        None,
        RunEventKind::BatchRunStarted {
            batch_id: batch_id.clone(),
            items: plan.items.len() as u32,
        },
    );

    let mut skipped = 0;
    let mut summaries = Vec::new();
    for item in plan.items.clone() {
        let item_key = item.key.clone();
        if item.status == BatchItemState::Succeeded {
            skipped += 1;
            store.append(
                batch_run_id,
                None,
                RunEventKind::BatchItemStatus {
                    batch_id: batch_id.clone(),
                    item_key: item_key.clone(),
                    status: "skipped: already succeeded".into(),
                },
            );
            summaries.push(serde_json::json!({
                "item_key": item_key,
                "status": "skipped",
                "last_run_id": item.last_run_id,
                "final_output": item.final_output
            }));
            continue;
        }

        plan.mark_running(&item_key);
        plan.save_to_env().map_err(|e| e.to_string())?;
        store.append(
            batch_run_id,
            None,
            RunEventKind::BatchItemStatus {
                batch_id: batch_id.clone(),
                item_key: item_key.clone(),
                status: "running".into(),
            },
        );
        let provider = build_provider(demo, &item.input, &options)?;
        let harness = Harness::new(
            provider,
            store.clone(),
            build_registry(options.enable_shell, options.enable_subagent),
        );
        let agent = build_agent(&options);
        match harness
            .run(
                &agent,
                UserInput {
                    text: item.input.clone(),
                },
            )
            .await
        {
            Ok(result) => {
                plan.mark_succeeded(
                    &item_key,
                    result.run_id.0.to_string(),
                    result.final_output.clone(),
                );
                plan.save_to_env().map_err(|e| e.to_string())?;
                store.append(
                    batch_run_id,
                    None,
                    RunEventKind::ChildRunStarted {
                        child_run_id: result.run_id,
                        agent_id: agent.id.clone(),
                    },
                );
                store.append(
                    batch_run_id,
                    None,
                    RunEventKind::ChildRunCompleted {
                        child_run_id: result.run_id,
                        status: "succeeded".into(),
                    },
                );
                store.append(
                    batch_run_id,
                    None,
                    RunEventKind::BatchItemStatus {
                        batch_id: batch_id.clone(),
                        item_key: item_key.clone(),
                        status: "succeeded".into(),
                    },
                );
                summaries.push(serde_json::json!({
                    "item_key": item_key,
                    "run_id": result.run_id.0,
                    "status": "succeeded",
                    "final_output": result.final_output
                }));
            }
            Err(err) => {
                plan.mark_failed(&item_key, err.to_string());
                plan.save_to_env().map_err(|e| e.to_string())?;
                store.append(
                    batch_run_id,
                    None,
                    RunEventKind::BatchItemStatus {
                        batch_id: batch_id.clone(),
                        item_key: item_key.clone(),
                        status: format!("failed: {err}"),
                    },
                );
                summaries.push(serde_json::json!({
                    "item_key": item_key,
                    "status": "failed",
                    "error": err.to_string()
                }));
            }
        }
    }
    store.append(
        batch_run_id,
        None,
        RunEventKind::BatchRunCompleted {
            batch_id: batch_id.clone(),
            succeeded: plan.succeeded_count(),
            failed: plan.failed_count(),
        },
    );
    Ok(serde_json::json!({
        "batch_run_id": batch_run_id.0,
        "batch_id": batch_id,
        "succeeded": plan.succeeded_count(),
        "failed": plan.failed_count(),
        "skipped": skipped,
        "items": summaries
    }))
}

#[tauri::command]
async fn memory_create(content: String, user: bool) -> Result<MemoryRecord, String> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let record = MemoryStore::from_env()
        .create(target, &content, MemoryAuthor::Human, None)
        .map_err(|e| e.to_string())?;
    record_memory_written(&record, "created")?;
    Ok(record)
}

#[tauri::command]
async fn memory_generate(
    text: String,
    user: bool,
    range: Option<String>,
) -> Result<Vec<MemoryRecord>, String> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let records = MemoryStore::from_env()
        .generate_from_text(target, &text, range)
        .map_err(|e| e.to_string())?;
    for record in &records {
        record_memory_written(record, "generated")?;
    }
    Ok(records)
}

#[tauri::command]
async fn memory_list() -> Result<Vec<MemoryRecord>, String> {
    MemoryStore::from_env().list().map_err(|e| e.to_string())
}

#[tauri::command]
async fn memory_edit(id: String, content: String) -> Result<MemoryRecord, String> {
    let record = MemoryStore::from_env()
        .edit(&id, &content)
        .map_err(|e| e.to_string())?;
    record_memory_written(&record, "edited")?;
    Ok(record)
}

#[tauri::command]
async fn memory_delete(id: String) -> Result<(), String> {
    MemoryStore::from_env()
        .delete(&id)
        .map_err(|e| e.to_string())?;
    record_memory_operation(&id, "deleted")
}

#[tauri::command]
async fn memory_rollback(user: bool) -> Result<(), String> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    MemoryStore::from_env()
        .rollback(target)
        .map_err(|e| e.to_string())?;
    record_memory_operation(if user { "user.md" } else { "memory.md" }, "rolled_back")
}

fn record_memory_written(record: &MemoryRecord, operation: &str) -> Result<(), String> {
    record_memory_operation(&record.id, operation)
}

fn record_memory_operation(id: &str, operation: &str) -> Result<(), String> {
    open_event_store()?.append(
        RunId::new(),
        None,
        RunEventKind::MemoryWritten {
            id: id.to_string(),
            operation: operation.to_string(),
        },
    );
    Ok(())
}

#[tauri::command]
async fn skill_import_openclaw(path: String) -> Result<SkillDoc, String> {
    SkillRegistry::from_env()
        .import_openclaw(path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn skill_list() -> Result<Vec<SkillDoc>, String> {
    SkillRegistry::from_env().list().map_err(|e| e.to_string())
}

#[tauri::command]
async fn skill_inspect(id: String) -> Result<SkillDoc, String> {
    SkillRegistry::from_env()
        .inspect(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn skill_allow(id: String) -> Result<SkillDoc, String> {
    SkillRegistry::from_env()
        .allow(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn skill_quarantine(id: String) -> Result<SkillDoc, String> {
    SkillRegistry::from_env()
        .quarantine(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ingest_add(
    app: AppHandle,
    path: String,
    backend: Option<String>,
) -> Result<IngestionArtifact, String> {
    let backend = backend.unwrap_or_else(|| "local-v0".into());
    let trace_run_id = RunId::new();
    let store = open_event_store()?;
    let started = store.append(
        trace_run_id,
        None,
        RunEventKind::IngestionStarted {
            source: path.clone(),
            backend: backend.clone(),
        },
    );
    let _ = app.emit("run-event", &started);
    let artifact = IngestionStore::from_env()
        .ingest_with_backend(path, &backend)
        .map_err(|e| e.to_string())?;
    let completed = store.append(
        trace_run_id,
        Some(started.id),
        ingestion_completed_event(&artifact),
    );
    let _ = app.emit("run-event", &completed);
    Ok(artifact)
}

#[tauri::command]
async fn ingest_list() -> Result<Vec<IngestionArtifact>, String> {
    IngestionStore::from_env().list().map_err(|e| e.to_string())
}

#[tauri::command]
async fn ingest_show(id: String) -> Result<IngestionArtifact, String> {
    IngestionStore::from_env()
        .show(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ingest_rm(id: String) -> Result<(), String> {
    IngestionStore::from_env()
        .remove(&id)
        .map_err(|e| e.to_string())
}

fn ingestion_completed_event(artifact: &IngestionArtifact) -> RunEventKind {
    RunEventKind::IngestionCompleted {
        artifact_id: artifact.id.clone(),
        content_hash: artifact.content_hash.clone(),
        sections: artifact.sections.len() as u32,
    }
}

#[tauri::command]
async fn bundle_export(path: String) -> Result<BundleManifest, String> {
    export_bundle(path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn bundle_import(path: String) -> Result<BundleManifest, String> {
    import_bundle(path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn adapter_inspect(path: String) -> Result<NormalizedPackage, String> {
    inspect_source(path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn adapter_import(path: String) -> Result<NormalizedPackage, String> {
    AdapterRegistry::from_env()
        .import(path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn adapter_list() -> Result<Vec<NormalizedPackage>, String> {
    AdapterRegistry::from_env()
        .list()
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn adapter_show(id: String) -> Result<NormalizedPackage, String> {
    AdapterRegistry::from_env()
        .show(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn adapter_allow(id: String) -> Result<NormalizedPackage, String> {
    AdapterRegistry::from_env()
        .allow(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn adapter_quarantine(id: String) -> Result<NormalizedPackage, String> {
    AdapterRegistry::from_env()
        .quarantine(&id)
        .map_err(|e| e.to_string())
}

fn open_event_store() -> Result<SqliteEventStore, String> {
    let paths = StoragePaths::from_env();
    paths.ensure_base_dirs().map_err(|e| e.to_string())?;
    SqliteEventStore::open(paths.state_db()).map_err(|e| e.to_string())
}

fn approvals_for_run(run_id: RunId) -> Result<Vec<Value>, String> {
    let mut approvals = Vec::<Value>::new();
    for event in open_event_store()?
        .try_events(run_id)
        .map_err(|e| e.to_string())?
    {
        match event.kind {
            RunEventKind::ApprovalRequested {
                approval_id,
                action,
                reason,
            } => approvals.push(serde_json::json!({
                "approval_id": approval_id,
                "action": action,
                "reason": reason,
                "status": "pending"
            })),
            RunEventKind::ApprovalResolved {
                approval_id,
                approved,
            } => {
                if let Some(existing) = approvals
                    .iter_mut()
                    .find(|value| value["approval_id"] == approval_id)
                {
                    existing["status"] =
                        Value::String(if approved { "approved" } else { "rejected" }.into());
                    existing["approved"] = Value::Bool(approved);
                } else {
                    approvals.push(serde_json::json!({
                        "approval_id": approval_id,
                        "action": null,
                        "reason": null,
                        "status": if approved { "approved" } else { "rejected" },
                        "approved": approved
                    }));
                }
            }
            _ => {}
        }
    }
    Ok(approvals)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            run_agent,
            preview_context,
            explain_config,
            explain_tools,
            call_tool,
            trace_show,
            approval_list,
            approval_decide,
            approval_execute,
            guide,
            cancel,
            score,
            batch_run,
            batch_resume,
            memory_create,
            memory_generate,
            memory_list,
            memory_edit,
            memory_delete,
            memory_rollback,
            skill_import_openclaw,
            skill_list,
            skill_inspect,
            skill_allow,
            skill_quarantine,
            ingest_add,
            ingest_list,
            ingest_show,
            ingest_rm,
            bundle_export,
            bundle_import,
            adapter_inspect,
            adapter_import,
            adapter_list,
            adapter_show,
            adapter_allow,
            adapter_quarantine
        ])
        .run(tauri::generate_context!())
        .expect("error while running Agent Harness Tauri app");
}
