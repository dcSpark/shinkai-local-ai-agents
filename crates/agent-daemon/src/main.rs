use std::sync::Arc;
use std::time::Instant;

use agent_adapters::AdapterRegistry;
use agent_bundles::{export_bundle, import_bundle};
use agent_config::ConfigResolver;
use agent_core::{
    AgentConfig, ApprovalMode, Harness, HarnessApi, IngestedArtifactView, ToolPolicy, UserInput,
};
use agent_ingest::IngestionStore;
use agent_llm::{FakeProvider, FakeStep, LlmProvider, ModelRef, RigProvider, RigProviderConfig};
use agent_memory::{MemoryAuthor, MemoryStore, MemoryTarget};
use agent_skills::SkillRegistry;
use agent_storage::StoragePaths;
use agent_tools::{FakeTool, ShellTool, ShellToolConfig, ToolId, ToolRegistry};
use agent_tracing::{EventStore, RunEventKind, RunId, SqliteEventStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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
    println!("agent-daemon listening on http://{addr}");
    loop {
        let (mut socket, _) = listener.accept().await?;
        tokio::spawn(async move {
            let response = match read_request(&mut socket).await {
                Ok(request) => handle_request(request).await,
                Err(err) => http_json(400, serde_json::json!({"error": err.to_string()})),
            };
            let _ = socket.write_all(response.as_bytes()).await;
        });
    }
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

async fn handle_request(request: HttpRequest) -> String {
    match route(request).await {
        Ok((status, body)) => http_json(status, body),
        Err(err) => http_json(500, serde_json::json!({"error": err.to_string()})),
    }
}

async fn route(request: HttpRequest) -> anyhow::Result<(u16, serde_json::Value)> {
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
        ("POST", "/preview-context") => {
            daemon_preview_context(&request.body).map(|value| (200, value))
        }
        ("POST", "/guide") => daemon_guide(&request.body).map(|value| (200, value)),
        ("POST", "/cancel") => daemon_cancel(&request.body).map(|value| (200, value)),
        ("POST", "/score") => daemon_score(&request.body).map(|value| (200, value)),
        ("POST", "/batch") => daemon_batch(&request.body).await.map(|value| (200, value)),
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
                    "POST /preview-context",
                    "POST /guide",
                    "POST /cancel",
                    "POST /score",
                    "POST /batch",
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
        build_registry(input.options.enable_shell),
    );
    let agent = build_agent(&input.options);
    let result = harness.run(&agent, UserInput { text: input.input }).await?;
    Ok(serde_json::json!({
        "run_id": result.run_id.0,
        "final_output": result.final_output
    }))
}

fn daemon_preview_context(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: DaemonPreviewInput = serde_json::from_str(body)?;
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store()?),
        build_registry(input.options.enable_shell),
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

fn daemon_cancel(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: CancelInput = serde_json::from_str(body)?;
    let run_id = RunId(uuid::Uuid::parse_str(&input.run_id)?);
    open_event_store()?.append(
        run_id,
        None,
        RunEventKind::RunCancelled {
            reason: input.reason,
        },
    );
    Ok(serde_json::json!({ "run_id": run_id.0, "recorded": "cancelled" }))
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
    let store = Arc::new(open_event_store()?);
    store.append(
        batch_run_id,
        None,
        RunEventKind::BatchRunStarted {
            batch_id: batch_id.clone(),
            items: input.items.len() as u32,
        },
    );

    let mut succeeded = 0;
    let mut failed = 0;
    let mut summaries = Vec::new();
    for (idx, item) in input.items.into_iter().enumerate() {
        let item_key = format!("item-{idx}");
        store.append(
            batch_run_id,
            None,
            RunEventKind::BatchItemStatus {
                batch_id: batch_id.clone(),
                item_key: item_key.clone(),
                status: "running".into(),
            },
        );
        let provider = provider_for_run(input.demo.as_deref(), &item, &input.options)?;
        let harness = Harness::new(
            provider,
            store.clone(),
            build_registry(input.options.enable_shell),
        );
        let agent = build_agent(&input.options);
        match harness.run(&agent, UserInput { text: item }).await {
            Ok(result) => {
                succeeded += 1;
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
                failed += 1;
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
            succeeded,
            failed,
        },
    );

    Ok(serde_json::json!({
        "batch_run_id": batch_run_id.0,
        "batch_id": batch_id,
        "succeeded": succeeded,
        "failed": failed,
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
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store()?),
        build_registry(enable_shell),
    );
    let mut agent = build_agent(&DaemonRuntimeOptions::default());
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
    let registry = build_registry(tool_id == "shell");
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
            duration_ms,
        },
    );
    store.append(
        run_id,
        None,
        RunEventKind::RunCompleted {
            final_output: serde_json::to_string(&output)?,
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
    Ok(serde_json::to_value(MemoryStore::from_env().create(
        target,
        &input.content,
        MemoryAuthor::Human,
        None,
    )?)?)
}

fn daemon_memory_generate(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: MemoryGenerateInput = serde_json::from_str(body)?;
    let target = if input.user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    Ok(serde_json::to_value(
        MemoryStore::from_env().generate_from_text(target, &input.text, input.range)?,
    )?)
}

fn daemon_memory_rollback(body: &str) -> anyhow::Result<serde_json::Value> {
    let input: MemoryRollbackInput = serde_json::from_str(body)?;
    let target = if input.user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    MemoryStore::from_env().rollback(target)?;
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
            Ok(serde_json::to_value(
                MemoryStore::from_env().edit(parts[1], &input.content)?,
            )?)
        }
        "delete" => {
            MemoryStore::from_env().delete(parts[1])?;
            Ok(serde_json::json!({ "id": parts[1], "deleted": true }))
        }
        _ => anyhow::bail!("unknown memory action"),
    }
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
    Ok(serde_json::to_value(
        IngestionStore::from_env().ingest(input.path)?,
    )?)
}

fn daemon_ingest_show(id: &str) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(IngestionStore::from_env().show(id)?)?)
}

fn daemon_ingest_rm(id: &str) -> anyhow::Result<serde_json::Value> {
    IngestionStore::from_env().remove(id)?;
    Ok(serde_json::json!({ "id": id, "removed": true }))
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

#[derive(Clone, Default, serde::Deserialize)]
struct DaemonRuntimeOptions {
    provider: Option<String>,
    model: Option<String>,
    api_base_url: Option<String>,
    api_key_env: Option<String>,
    api_key: Option<String>,
    #[serde(default)]
    enable_shell: bool,
    #[serde(default)]
    load_memory: bool,
    #[serde(default)]
    load_skills: bool,
    #[serde(default)]
    include_ingest: Vec<String>,
    #[serde(default)]
    require_approval: bool,
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
}

fn provider_for_run(
    demo: Option<&str>,
    input: &str,
    options: &DaemonRuntimeOptions,
) -> anyhow::Result<Arc<dyn LlmProvider>> {
    match options.provider.as_deref() {
        Some("rig") => {
            let config = RigProviderConfig {
                api_base_url: options.api_base_url.clone(),
                api_key_env: options
                    .api_key_env
                    .clone()
                    .unwrap_or_else(|| "OPENAI_API_KEY".into()),
                model: ModelRef::from(
                    options
                        .model
                        .clone()
                        .unwrap_or_else(|| "gpt-4o-mini".into()),
                ),
                max_output_tokens: None,
                temperature: None,
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

fn build_registry(enable_shell: bool) -> Arc<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    registry.register(FakeTool::echo_descriptor(), Arc::new(FakeTool::echo()));
    if enable_shell {
        registry.register(
            ShellTool::descriptor(),
            Arc::new(ShellTool::new(ShellToolConfig::default())),
        );
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
            memory_fragments: Vec::new(),
            ingestion_artifacts: Vec::new(),
            skill_views: Vec::new(),
        });

    if let Some(model) = options.model.clone() {
        agent.model = ModelRef::from(model);
    } else if options.provider.as_deref() == Some("rig") && agent.model.0 == "fake-model" {
        agent.model = ModelRef::from("gpt-4o-mini");
    }
    if options.require_approval {
        agent.tool_policy.approval_mode = ApprovalMode::RequireExplicit;
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
