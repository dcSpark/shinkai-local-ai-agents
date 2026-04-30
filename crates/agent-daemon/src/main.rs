use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use agent_adapters::AdapterRegistry;
use agent_batch::{BatchItemState, BatchPlan};
use agent_bundles::{export_bundle, import_bundle};
use agent_config::ConfigResolver;
use agent_core::{
    AgentConfig, ApprovalMode, CostPolicy, Harness, HarnessApi, IngestedArtifactView,
    ToolOutputMode, ToolPolicy, UserInput, VisibilityLevel,
};
use agent_ingest::{IngestionArtifact, IngestionStore};
use agent_llm::{FakeProvider, FakeStep, LlmProvider, ModelRef, RigProvider, RigProviderConfig};
use agent_memory::{MemoryAuthor, MemoryRecord, MemoryStore, MemoryTarget};
use agent_skills::SkillRegistry;
use agent_storage::StoragePaths;
use agent_tools::{FakeTool, ShellTool, ShellToolConfig, SubagentTool, ToolId, ToolRegistry};
use agent_tracing::{EventStore, RunEventKind, RunId, SqliteEventStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::AbortHandle;
use tokio::time::{Duration, timeout};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addr = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("AGENT_DAEMON_ADDR").ok())
        .unwrap_or_else(|| "127.0.0.1:7878".into());
    run_server(&addr).await
}

async fn run_server(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let state = Arc::new(DaemonState::default());
    println!("agent-daemon listening on http://{addr}");
    loop {
        let (mut socket, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let response = match read_request(&mut socket).await {
                Ok(request) => handle_request(request, state).await,
                Err(err) => http_json(400, serde_json::json!({"error": err.to_string()})),
            };
            let _ = socket.write_all(response.as_bytes()).await;
        });
    }
}

#[derive(Default)]
struct DaemonState {
    active_runs: tokio::sync::Mutex<HashMap<String, AbortHandle>>,
}

struct HttpRequest {
    method: String,
    path: String,
    body: String,
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> anyhow::Result<HttpRequest> {
    let mut buf = vec![0; 64 * 1024];
    let n = socket.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let (head, body) = request.split_once("\r\n\r\n").unwrap_or((&request, ""));
    let request_line = head.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    Ok(HttpRequest {
        method,
        path,
        body: body.to_string(),
    })
}

async fn handle_request(request: HttpRequest, state: Arc<DaemonState>) -> String {
    match route(request, state).await {
        Ok((status, body)) => http_json(status, body),
        Err(err) => http_json(500, serde_json::json!({"error": err.to_string()})),
    }
}

async fn route(
    request: HttpRequest,
    state: Arc<DaemonState>,
) -> anyhow::Result<(u16, serde_json::Value)> {
    match (request.method.as_str(), request.path.as_str()) {
        ("OPTIONS", _) => Ok((200, serde_json::json!({"status": "ok"}))),
        ("GET", "/health") => Ok((200, serde_json::json!({"status": "ok"}))),
        ("GET", "/version") => Ok((
            200,
            serde_json::json!({
                "name": "agent-daemon",
                "trace_schema_version": agent_tracing::SCHEMA_VERSION
            }),
        )),
        ("POST", "/run") => daemon_run(&request.body).await.map(|value| (200, value)),
        ("POST", "/run/start") => daemon_run_start(&request.body, state)
            .await
            .map(|value| (200, value)),
        ("POST", "/preview-context") => {
            daemon_preview_context(&request.body).map(|value| (200, value))
        }
        ("POST", "/guide") => daemon_guide(&request.body).map(|value| (200, value)),
        ("POST", "/cancel") => daemon_cancel(&request.body, state)
            .await
            .map(|value| (200, value)),
        ("POST", "/score") => daemon_score(&request.body).map(|value| (200, value)),
        ("POST", "/batch") => daemon_batch(&request.body).await.map(|value| (200, value)),
        ("POST", "/batch/resume") => daemon_batch_resume(&request.body)
            .await
            .map(|value| (200, value)),
        ("GET", "/memory") => daemon_memory_list().map(|value| (200, value)),
        ("POST", "/memory") => daemon_memory_create(&request.body).map(|value| (200, value)),
        ("POST", "/memory/generate") => {
            daemon_memory_generate(&request.body).map(|value| (200, value))
        }
        ("POST", "/memory/rollback") => {
            daemon_memory_rollback(&request.body).map(|value| (200, value))
        }
        ("GET", "/skills") => daemon_skill_list().map(|value| (200, value)),
        ("POST", "/skills/import") => daemon_skill_import(&request.body).map(|value| (200, value)),
        ("GET", "/ingest") => daemon_ingest_list().map(|value| (200, value)),
        ("POST", "/ingest") => daemon_ingest_add(&request.body).map(|value| (200, value)),
        ("GET", "/adapters") => daemon_adapter_list().map(|value| (200, value)),
        ("POST", "/adapters/import") => {
            daemon_adapter_import(&request.body).map(|value| (200, value))
        }
        ("POST", "/bundles/export") => {
            daemon_bundle_export(&request.body).map(|value| (200, value))
        }
        ("POST", "/bundles/import") => {
            daemon_bundle_import(&request.body).map(|value| (200, value))
        }
        _ if request.method == "GET" && request.path.starts_with("/trace/") => {
            let id = request.path.trim_start_matches("/trace/");
            trace_show(id).map(|value| (200, value))
        }
        _ if request.method == "GET" && request.path.starts_with("/run/status/") => {
            let id = request.path.trim_start_matches("/run/status/");
            daemon_run_status(id, state).await.map(|value| (200, value))
        }
        _ if request.method == "GET" && request.path.starts_with("/approvals/") => {
            let id = request.path.trim_start_matches("/approvals/");
            approvals_for_run(id).map(|value| (200, value))
        }
        _ if request.method == "POST" && request.path.starts_with("/approvals/") => {
            daemon_approval_route(&request.path, &request.body)
                .await
                .map(|value| (200, value))
        }
        _ if request.method == "POST" && request.path.starts_with("/memory/") => {
            daemon_memory_route(&request.path, &request.body).map(|value| (200, value))
        }
        _ if request.method == "POST" && request.path.starts_with("/skills/") => {
            daemon_skill_route(&request.path).map(|value| (200, value))
        }
        _ if request.method == "GET" && request.path.starts_with("/ingest/") => {
            let id = request.path.trim_start_matches("/ingest/");
            daemon_ingest_show(id).map(|value| (200, value))
        }
        _ if request.method == "POST"
            && request.path.starts_with("/ingest/")
            && request.path.ends_with("/rm") =>
        {
            let id = request
                .path
                .trim_start_matches("/ingest/")
                .trim_end_matches("/rm");
            daemon_ingest_rm(id).map(|value| (200, value))
        }
        _ if request.method == "POST" && request.path.starts_with("/adapters/") => {
            daemon_adapter_route(&request.path).map(|value| (200, value))
        }
        _ if request.method == "GET" && request.path.starts_with("/adapters/") => {
            let id = request.path.trim_start_matches("/adapters/");
            daemon_adapter_show(id).map(|value| (200, value))
        }
        _ if request.method == "POST" && request.path.starts_with("/tool/") => {
            let name = request.path.trim_start_matches("/tool/");
            daemon_tool(name, &request.body)
                .await
                .map(|value| (200, value))
        }
        _ => Ok((
            404,
            serde_json::json!({
                "error": "not_found",
                "available": [
                    "GET /health",
                    "GET /version",
                    "GET /trace/<run_id>",
                    "POST /run",
                    "POST /run/start",
                    "GET /run/status/<run_id>",
                    "POST /preview-context",
                    "POST /guide",
                    "POST /cancel",
                    "POST /score",
                    "POST /batch",
                    "POST /batch/resume",
                    "POST /tool/<name>",
                    "GET /approvals/<run_id>",
                    "POST /approvals/<run_id>/<approval_id>/decide",
                    "POST /approvals/<run_id>/<approval_id>/execute",
                    "GET|POST /memory",
                    "GET /skills",
                    "POST /skills/import",
                    "POST /skills/<id>/allow",
                    "GET|POST /ingest",
                    "GET /ingest/<id>",
                    "GET /adapters",
                    "POST /adapters/import",
                    "GET /adapters/<id>",
                    "POST /adapters/<id>/allow",
                    "POST /bundles/export",
                    "POST /bundles/import"
                ]
            }),
        )),
    }
}

async fn daemon_run(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: DaemonRunInput = serde_json::from_str(body)?;
    let harness = Harness::new(
        provider_for_run(input.demo.as_deref(), &input.input, &input.options)?,
        Arc::new(open_event_store()?),
        build_registry(input.options.enable_shell, input.options.enable_subagent),
    );
    let agent = build_agent(&input.options);
    let result = harness.run(&agent, UserInput { text: input.input }).await?;
    Ok(serde_json::json!({
        "run_id": result.run_id.0,
        "final_output": result.final_output
    }))
}

async fn daemon_run_start(
    body: &str,
    state: Arc<DaemonState>,
) -> anyhow::Result<serde_json::Value> {
    let input: DaemonRunInput = serde_json::from_str(body)?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<String>();

    let provider = provider_for_run(input.demo.as_deref(), &input.input, &input.options)?;
    let store = Arc::new(agent_tracing::PublishingEventStore::new(
        open_event_store()?,
        tx,
    ));
    let harness = Harness::new(
        provider,
        store,
        build_registry(input.options.enable_shell, input.options.enable_subagent),
    );
    let agent = build_agent(&input.options);
    let input_text = input.input;

    let run_task = tokio::spawn(async move {
        let result = harness.run(&agent, UserInput { text: input_text }).await;
        drop(harness);
        result
    });
    let abort_handle = run_task.abort_handle();

    let state_for_events = state.clone();
    tokio::spawn(async move {
        let mut started_tx = Some(started_tx);
        let mut seen_run_id = None::<String>;
        while let Some(evt) = rx.recv().await {
            let run_id = evt.run_id.0.to_string();
            match &evt.kind {
                RunEventKind::RunStarted { .. } => {
                    seen_run_id = Some(run_id.clone());
                    state_for_events
                        .active_runs
                        .lock()
                        .await
                        .insert(run_id.clone(), abort_handle.clone());
                    if let Some(tx) = started_tx.take() {
                        let _ = tx.send(run_id);
                    }
                }
                RunEventKind::RunPaused { .. }
                | RunEventKind::RunCancelled { .. }
                | RunEventKind::RunCompleted { .. }
                | RunEventKind::RunFailed { .. } => {
                    state_for_events.active_runs.lock().await.remove(&run_id);
                }
                _ => {}
            }
        }
        if let Some(run_id) = seen_run_id {
            state_for_events.active_runs.lock().await.remove(&run_id);
        }
    });

    tokio::spawn(async move {
        let _ = run_task.await;
    });

    let run_id = timeout(Duration::from_secs(2), started_rx)
        .await
        .map_err(|_| anyhow::anyhow!("timed out waiting for RunStarted"))??;
    let status = daemon_run_status(&run_id, state).await?;
    Ok(serde_json::json!({
        "run_id": run_id,
        "status": status["status"],
        "active": status["active"]
    }))
}

async fn daemon_run_status(id: &str, state: Arc<DaemonState>) -> anyhow::Result<serde_json::Value> {
    let run_id = RunId(uuid::Uuid::parse_str(id)?);
    let events = open_event_store()?.try_events(run_id)?;
    let active = state.active_runs.lock().await.contains_key(id);

    let mut status = if active { "running" } else { "unknown" };
    let mut final_output = None::<String>;
    let mut reason = None::<String>;
    let mut total_duration_ms = None::<u64>;
    let event_cost_usd = events
        .iter()
        .filter_map(|event| match &event.kind {
            RunEventKind::LlmRequestCompleted {
                cost_usd: Some(cost),
                ..
            } => Some(*cost),
            RunEventKind::ToolCallCompleted {
                cost_usd: Some(cost),
                ..
            } => Some(*cost),
            _ => None,
        })
        .sum::<f64>();
    let mut total_cost_usd = if event_cost_usd > 0.0 {
        Some(event_cost_usd)
    } else {
        None
    };

    for event in events.iter().rev() {
        match &event.kind {
            RunEventKind::RunCompleted {
                final_output: output,
                total_cost_usd: completed_cost,
                total_duration_ms: duration,
            } => {
                status = "completed";
                final_output = Some(output.clone());
                total_cost_usd = *completed_cost;
                total_duration_ms = Some(*duration);
                break;
            }
            RunEventKind::RunFailed { reason: failure } => {
                status = "failed";
                reason = Some(failure.clone());
                break;
            }
            RunEventKind::RunCancelled {
                reason: cancellation,
            } => {
                status = "cancelled";
                reason = Some(cancellation.clone());
                break;
            }
            RunEventKind::RunPaused { reason: pause } => {
                status = "paused";
                reason = Some(pause.clone());
                break;
            }
            _ => {}
        }
    }

    Ok(serde_json::json!({
        "run_id": run_id.0,
        "status": status,
        "active": active,
        "event_count": events.len(),
        "final_output": final_output,
        "reason": reason,
        "total_cost_usd": total_cost_usd,
        "total_duration_ms": total_duration_ms
    }))
}

fn daemon_preview_context(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: DaemonPreviewInput = serde_json::from_str(body)?;
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store()?),
        build_registry(input.options.enable_shell, input.options.enable_subagent),
    );
    Ok(serde_json::to_value(harness.preview_context(
        &build_agent(&input.options),
        UserInput { text: input.input },
    ))?)
}

fn daemon_guide(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: GuideInput = serde_json::from_str(body)?;
    let run_id = RunId(uuid::Uuid::parse_str(&input.run_id)?);
    open_event_store()?.append(
        run_id,
        None,
        RunEventKind::GuidanceInjected {
            content: input.text,
        },
    );
    Ok(serde_json::json!({ "run_id": run_id.0, "recorded": "guidance" }))
}

async fn daemon_cancel(body: &str, state: Arc<DaemonState>) -> anyhow::Result<serde_json::Value> {
    let input: CancelInput = serde_json::from_str(body)?;
    let run_id = RunId(uuid::Uuid::parse_str(&input.run_id)?);
    let store = open_event_store()?;
    let active_handle = state.active_runs.lock().await.remove(&input.run_id);
    let aborted = active_handle.is_some();
    if !aborted
        && store.try_events(run_id)?.iter().any(|event| {
            matches!(
                &event.kind,
                RunEventKind::RunCompleted { .. }
                    | RunEventKind::RunFailed { .. }
                    | RunEventKind::RunCancelled { .. }
                    | RunEventKind::RunPaused { .. }
            )
        })
    {
        return Ok(serde_json::json!({
            "run_id": run_id.0,
            "recorded": "not_active",
            "aborted": false
        }));
    }
    store.append(
        run_id,
        None,
        RunEventKind::RunCancelled {
            reason: input.reason,
        },
    );
    if let Some(handle) = active_handle {
        handle.abort();
    }
    Ok(serde_json::json!({
        "run_id": run_id.0,
        "recorded": "cancelled",
        "aborted": aborted
    }))
}

fn daemon_score(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: ScoreInput = serde_json::from_str(body)?;
    let run_id = RunId(uuid::Uuid::parse_str(&input.run_id)?);
    open_event_store()?.append(
        run_id,
        None,
        RunEventKind::QualityScored {
            target: input.target,
            score: input.score,
        },
    );
    Ok(serde_json::json!({ "run_id": run_id.0, "recorded": "score" }))
}

async fn daemon_batch(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: DaemonBatchInput = serde_json::from_str(body)?;
    let batch_run_id = RunId::new();
    let batch_id = format!("batch-{}", batch_run_id.0);
    let mut plan = BatchPlan::new(batch_id.clone(), input.items);
    plan.save_to_env()?;
    execute_daemon_batch_plan(plan, batch_run_id, batch_id, input.demo, input.options).await
}

async fn daemon_batch_resume(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: DaemonBatchResumeInput = serde_json::from_str(body)?;
    let batch_run_id = RunId::new();
    let plan = BatchPlan::load_from_env(&input.batch_id)?;
    execute_daemon_batch_plan(
        plan,
        batch_run_id,
        input.batch_id,
        input.demo,
        input.options,
    )
    .await
}

async fn execute_daemon_batch_plan(
    mut plan: BatchPlan,
    batch_run_id: RunId,
    batch_id: String,
    demo: Option<String>,
    options: DaemonRuntimeOptions,
) -> anyhow::Result<serde_json::Value> {
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
        plan.save_to_env()?;
        store.append(
            batch_run_id,
            None,
            RunEventKind::BatchItemStatus {
                batch_id: batch_id.clone(),
                item_key: item_key.clone(),
                status: "running".into(),
            },
        );
        let provider = provider_for_run(demo.as_deref(), &item.input, &options)?;
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
                plan.save_to_env()?;
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
                plan.save_to_env()?;
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

async fn daemon_tool(name: &str, body: &str) -> anyhow::Result<serde_json::Value> {
    let mut input: serde_json::Value = if body.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(body)?
    };
    let require_approval = input
        .as_object_mut()
        .and_then(|map| map.remove("__require_approval"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let enable_shell = name == "shell";
    let enable_subagent = name == "subagent";
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store()?),
        build_registry(enable_shell, enable_subagent),
    );
    let mut agent = build_agent(&DaemonRuntimeOptions {
        enable_shell,
        enable_subagent,
        ..DaemonRuntimeOptions::default()
    });
    if require_approval {
        agent.tool_policy.approval_mode = ApprovalMode::RequireExplicit;
    }
    let result = harness
        .call_tool(&agent, ToolId::from(name.to_string()), input)
        .await?;
    Ok(serde_json::json!({
        "run_id": result.run_id.0,
        "duration_ms": result.duration_ms,
        "output": result.output
    }))
}

fn trace_show(id: &str) -> anyhow::Result<serde_json::Value> {
    let run_id = RunId(uuid::Uuid::parse_str(id)?);
    Ok(serde_json::to_value(
        open_event_store()?.try_events(run_id)?,
    )?)
}

fn approvals_for_run(id: &str) -> anyhow::Result<serde_json::Value> {
    let run_id = RunId(uuid::Uuid::parse_str(id)?);
    let mut approvals = Vec::<serde_json::Value>::new();
    for event in open_event_store()?.try_events(run_id)? {
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
                    existing["status"] = serde_json::Value::String(
                        if approved { "approved" } else { "rejected" }.into(),
                    );
                    existing["approved"] = serde_json::Value::Bool(approved);
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
    Ok(serde_json::Value::Array(approvals))
}

async fn daemon_approval_route(path: &str, body: &str) -> anyhow::Result<serde_json::Value> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() != 4 || parts[0] != "approvals" {
        anyhow::bail!("invalid approval route");
    }
    let run_id = RunId(uuid::Uuid::parse_str(parts[1])?);
    let approval_id = parts[2].to_string();
    match parts[3] {
        "decide" => {
            let input: ApprovalDecisionInput = serde_json::from_str(body)?;
            open_event_store()?.append(
                run_id,
                None,
                RunEventKind::ApprovalResolved {
                    approval_id: approval_id.clone(),
                    approved: input.approved,
                },
            );
            Ok(serde_json::json!({
                "run_id": run_id.0,
                "approval_id": approval_id,
                "approved": input.approved
            }))
        }
        "execute" => execute_approved_tool(run_id, &approval_id).await,
        _ => anyhow::bail!("unknown approval action"),
    }
}

async fn execute_approved_tool(
    run_id: RunId,
    approval_id: &str,
) -> anyhow::Result<serde_json::Value> {
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;
    let approved = events.iter().rev().find_map(|event| match &event.kind {
        RunEventKind::ApprovalResolved {
            approval_id: id,
            approved,
        } if id == approval_id => Some(*approved),
        _ => None,
    });
    if approved != Some(true) {
        anyhow::bail!(
            "approval {approval_id} is not approved for run {}",
            run_id.0
        );
    }
    let proposed_id = events
        .iter()
        .find_map(|event| match &event.kind {
            RunEventKind::ApprovalRequested {
                approval_id: id, ..
            } if id == approval_id => event.parent_event,
            _ => None,
        })
        .ok_or_else(|| {
            anyhow::anyhow!("approval {approval_id} is not linked to a tool proposal")
        })?;
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
        .ok_or_else(|| anyhow::anyhow!("tool proposal for approval {approval_id} was not found"))?;
    if events.iter().any(|event| {
        matches!(
            &event.kind,
            RunEventKind::ToolCallCompleted {
                call_id: id,
                ..
            } if id == &call_id
        )
    }) {
        anyhow::bail!("tool call {call_id} has already completed");
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
    let output = registry.execute(&ToolId::from(tool_id), input).await?;
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
            final_output: serde_json::to_string(&output)?,
            total_cost_usd: None,
            total_duration_ms: duration_ms,
        },
    );
    Ok(serde_json::json!({
        "run_id": run_id.0,
        "duration_ms": duration_ms,
        "output": output
    }))
}

fn daemon_memory_list() -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(MemoryStore::from_env().list()?)?)
}

fn daemon_memory_create(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: MemoryCreateInput = serde_json::from_str(body)?;
    let target = if input.user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let record =
        MemoryStore::from_env().create(target, &input.content, MemoryAuthor::Human, None)?;
    record_memory_written(&record, "created")?;
    Ok(serde_json::to_value(record)?)
}

fn daemon_memory_generate(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: MemoryGenerateInput = serde_json::from_str(body)?;
    let target = if input.user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let records = MemoryStore::from_env().generate_from_text(target, &input.text, input.range)?;
    for record in &records {
        record_memory_written(record, "generated")?;
    }
    Ok(serde_json::to_value(records)?)
}

fn daemon_memory_rollback(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: MemoryRollbackInput = serde_json::from_str(body)?;
    let target = if input.user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    MemoryStore::from_env().rollback(target)?;
    record_memory_operation(
        if input.user { "user.md" } else { "memory.md" },
        "rolled_back",
    )?;
    Ok(serde_json::json!({ "rolled_back": true, "user": input.user }))
}

fn daemon_memory_route(path: &str, body: &str) -> anyhow::Result<serde_json::Value> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() != 3 || parts[0] != "memory" {
        anyhow::bail!("invalid memory route");
    }
    match parts[2] {
        "edit" => {
            let input: MemoryEditInput = serde_json::from_str(body)?;
            let record = MemoryStore::from_env().edit(parts[1], &input.content)?;
            record_memory_written(&record, "edited")?;
            Ok(serde_json::to_value(record)?)
        }
        "delete" => {
            MemoryStore::from_env().delete(parts[1])?;
            record_memory_operation(parts[1], "deleted")?;
            Ok(serde_json::json!({ "id": parts[1], "deleted": true }))
        }
        _ => anyhow::bail!("unknown memory action"),
    }
}

fn record_memory_written(record: &MemoryRecord, operation: &str) -> anyhow::Result<()> {
    record_memory_operation(&record.id, operation)
}

fn record_memory_operation(id: &str, operation: &str) -> anyhow::Result<()> {
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

fn daemon_skill_list() -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(SkillRegistry::from_env().list()?)?)
}

fn daemon_skill_import(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: PathInput = serde_json::from_str(body)?;
    Ok(serde_json::to_value(
        SkillRegistry::from_env().import_openclaw(input.path)?,
    )?)
}

fn daemon_skill_route(path: &str) -> anyhow::Result<serde_json::Value> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() != 3 || parts[0] != "skills" {
        anyhow::bail!("invalid skill route");
    }
    match parts[2] {
        "allow" => Ok(serde_json::to_value(
            SkillRegistry::from_env().allow(parts[1])?,
        )?),
        "quarantine" => Ok(serde_json::to_value(
            SkillRegistry::from_env().quarantine(parts[1])?,
        )?),
        _ => anyhow::bail!("unknown skill action"),
    }
}

fn daemon_ingest_list() -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(IngestionStore::from_env().list()?)?)
}

fn daemon_ingest_add(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: PathInput = serde_json::from_str(body)?;
    let trace_run_id = RunId::new();
    let store = open_event_store()?;
    let started = store.append(
        trace_run_id,
        None,
        RunEventKind::IngestionStarted {
            source: input.path.clone(),
            backend: input.backend.clone(),
        },
    );
    let artifact = IngestionStore::from_env().ingest_with_backend(input.path, &input.backend)?;
    store.append(
        trace_run_id,
        Some(started.id),
        ingestion_completed_event(&artifact),
    );
    Ok(serde_json::json!({
        "trace_run_id": trace_run_id.0,
        "artifact": artifact
    }))
}

fn daemon_ingest_show(id: &str) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(IngestionStore::from_env().show(id)?)?)
}

fn daemon_ingest_rm(id: &str) -> anyhow::Result<serde_json::Value> {
    IngestionStore::from_env().remove(id)?;
    Ok(serde_json::json!({ "id": id, "removed": true }))
}

fn ingestion_completed_event(artifact: &IngestionArtifact) -> RunEventKind {
    RunEventKind::IngestionCompleted {
        artifact_id: artifact.id.clone(),
        content_hash: artifact.content_hash.clone(),
        sections: artifact.sections.len() as u32,
    }
}

fn daemon_adapter_list() -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(AdapterRegistry::from_env().list()?)?)
}

fn daemon_adapter_import(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: PathInput = serde_json::from_str(body)?;
    Ok(serde_json::to_value(
        AdapterRegistry::from_env().import(input.path)?,
    )?)
}

fn daemon_adapter_show(id: &str) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(AdapterRegistry::from_env().show(id)?)?)
}

fn daemon_adapter_route(path: &str) -> anyhow::Result<serde_json::Value> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() != 3 || parts[0] != "adapters" {
        anyhow::bail!("invalid adapter route");
    }
    match parts[2] {
        "allow" => Ok(serde_json::to_value(
            AdapterRegistry::from_env().allow(parts[1])?,
        )?),
        "quarantine" => Ok(serde_json::to_value(
            AdapterRegistry::from_env().quarantine(parts[1])?,
        )?),
        _ => anyhow::bail!("unknown adapter action"),
    }
}

fn daemon_bundle_export(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: PathInput = serde_json::from_str(body)?;
    Ok(serde_json::to_value(export_bundle(input.path)?)?)
}

fn daemon_bundle_import(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: PathInput = serde_json::from_str(body)?;
    Ok(serde_json::to_value(import_bundle(input.path)?)?)
}

#[derive(serde::Deserialize)]
struct DaemonRunInput {
    input: String,
    demo: Option<String>,
    #[serde(flatten)]
    options: DaemonRuntimeOptions,
}

#[derive(serde::Deserialize)]
struct DaemonPreviewInput {
    input: String,
    #[serde(flatten)]
    options: DaemonRuntimeOptions,
}

#[derive(serde::Deserialize)]
struct DaemonBatchInput {
    items: Vec<String>,
    demo: Option<String>,
    #[serde(flatten)]
    options: DaemonRuntimeOptions,
}

#[derive(serde::Deserialize)]
struct DaemonBatchResumeInput {
    batch_id: String,
    demo: Option<String>,
    #[serde(flatten)]
    options: DaemonRuntimeOptions,
}

#[derive(Clone, Default, serde::Deserialize)]
struct DaemonRuntimeOptions {
    provider: Option<String>,
    model: Option<String>,
    api_base_url: Option<String>,
    api_key_env: Option<String>,
    api_key: Option<String>,
    max_output_tokens: Option<u64>,
    temperature: Option<f64>,
    max_tool_calls: Option<u32>,
    tool_visibility: Option<VisibilityLevel>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    #[serde(default)]
    enable_shell: bool,
    #[serde(default)]
    enable_subagent: bool,
    #[serde(default)]
    load_memory: bool,
    #[serde(default)]
    load_skills: bool,
    #[serde(default)]
    include_ingest: Vec<String>,
    #[serde(default)]
    require_approval: bool,
    #[serde(default)]
    raw_tool_output: bool,
}

#[derive(serde::Deserialize)]
struct GuideInput {
    run_id: String,
    text: String,
}

#[derive(serde::Deserialize)]
struct CancelInput {
    run_id: String,
    reason: String,
}

#[derive(serde::Deserialize)]
struct ScoreInput {
    run_id: String,
    target: String,
    score: f32,
}

#[derive(serde::Deserialize)]
struct ApprovalDecisionInput {
    approved: bool,
}

#[derive(serde::Deserialize)]
struct MemoryCreateInput {
    content: String,
    #[serde(default)]
    user: bool,
}

#[derive(serde::Deserialize)]
struct MemoryGenerateInput {
    text: String,
    #[serde(default)]
    user: bool,
    range: Option<String>,
}

#[derive(serde::Deserialize)]
struct MemoryEditInput {
    content: String,
}

#[derive(serde::Deserialize)]
struct MemoryRollbackInput {
    #[serde(default)]
    user: bool,
}

#[derive(serde::Deserialize)]
struct PathInput {
    path: String,
    #[serde(default = "default_ingest_backend")]
    backend: String,
}

fn default_ingest_backend() -> String {
    "local-v0".into()
}

fn provider_for_run(
    demo: Option<&str>,
    input: &str,
    options: &DaemonRuntimeOptions,
) -> anyhow::Result<Arc<dyn LlmProvider>> {
    match options.provider.as_deref() {
        Some("rig") => {
            let model = rig_model_id(options);
            let model_runtime = ConfigResolver::from_env()
                .resolve_model_runtime(&model)
                .ok()
                .flatten();
            let config = RigProviderConfig {
                api_base_url: options.api_base_url.clone(),
                api_key_env: options
                    .api_key_env
                    .clone()
                    .unwrap_or_else(|| "OPENAI_API_KEY".into()),
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
            Ok(Arc::new(RigProvider::from_config_with_api_key_override(
                config,
                options.api_key.clone(),
            )?))
        }
        _ => Ok(match demo {
            Some("tool") => Arc::new(FakeProvider::sequence(vec![
                FakeStep::CallTool {
                    id: "call-1".into(),
                    tool: "echo".into(),
                    input: serde_json::json!({"text": input}),
                },
                FakeStep::Reply("[fake] tool completed".into()),
            ])),
            _ => Arc::new(FakeProvider::echo()),
        }),
    }
}

fn rig_model_id(options: &DaemonRuntimeOptions) -> String {
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

fn build_agent(options: &DaemonRuntimeOptions) -> AgentConfig {
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
    } else if options.provider.as_deref() == Some("rig") && agent.model.0 == "fake-model" {
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

fn open_event_store() -> anyhow::Result<SqliteEventStore> {
    let paths = StoragePaths::from_env();
    paths.ensure_base_dirs()?;
    Ok(SqliteEventStore::open(paths.state_db())?)
}

fn http_json(status: u16, body: serde_json::Value) -> String {
    let status_text = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    let body = body.to_string();
    format!(
        "HTTP/1.1 {status_text}\r\ncontent-type: application/json\r\naccess-control-allow-origin: *\r\naccess-control-allow-methods: GET, POST, OPTIONS\r\naccess-control-allow-headers: content-type\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}
