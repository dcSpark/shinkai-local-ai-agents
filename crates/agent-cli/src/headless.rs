//! Headless `--print` mode. Reads input (CLI flag or stdin), runs once, and
//! emits either a human-readable transcript on stderr (with the final answer
//! on stdout) or one JSON `RunEvent` per line on stdout (`--json`).

use std::io::{self, Read};
use std::sync::Arc;
use std::time::Instant;

use agent_adapters::{AdapterRegistry, NormalizedPackage, inspect_source};
use agent_api_client::DaemonHttpClient;
use agent_bundles::{export_bundle, import_bundle};
use agent_config::ConfigResolver;
use agent_core::{Harness, HarnessApi, UserInput};
use agent_ingest::IngestionStore;
use agent_llm::FakeProvider;
use agent_memory::{MemoryAuthor, MemoryStore, MemoryTarget};
use agent_skills::SkillRegistry;
use agent_storage::StoragePaths;
use agent_tools::ToolId;
use agent_tracing::{EventStore, RunEvent, RunEventKind, RunId, SqliteEventStore};

use crate::{Demo, Provider, setup};

pub async fn run(
    input: Option<String>,
    json: bool,
    demo: Demo,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let mut text = read_text(input)?;

    match parse_slash_command(&text)? {
        Some(SlashCommand::Agent) => return explain_config(json).await,
        Some(SlashCommand::Tool { name, input }) => {
            return call_tool(name, Some(input), json, options.require_approval).await;
        }
        Some(SlashCommand::Run(prompt)) => text = prompt,
        None => {}
    }

    let provider = setup::build_provider(demo, &text, &options)?;
    let events = Arc::new(open_event_store()?);
    let registry = setup::build_registry(options.enable_shell);
    let harness = Harness::new(provider, events, registry);
    let agent = setup::build_agent(&options);

    let result = harness.run(&agent, UserInput { text }).await?;

    if json {
        for evt in harness.events(result.run_id) {
            println!("{}", serde_json::to_string(&evt)?);
        }
    } else {
        println!("{}", result.final_output);
        eprintln!();
        eprintln!("--- trace ({}) ---", result.run_id.0);
        for evt in harness.events(result.run_id) {
            eprintln!("  [{}] {:?}", evt.id.0, evt.kind);
        }
    }

    Ok(())
}

pub async fn preview_context(
    input: Option<String>,
    json: bool,
    enable_shell: bool,
    load_memory: bool,
    load_skills: bool,
    include_ingest: Vec<String>,
) -> anyhow::Result<()> {
    let text = read_text(input)?;
    let harness = inspection_harness(enable_shell);
    let options = setup::RuntimeOptions {
        enable_shell,
        load_memory,
        load_skills,
        include_ingest,
        ..setup::RuntimeOptions::default()
    };
    let agent = setup::build_agent(&options);
    let snapshot = harness.preview_context(&agent, UserInput { text });

    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        println!("agent: {} ({})", agent.name, agent.id);
        println!("model: {}", agent.model.0);
        println!(
            "tool calls remaining: {}/{}",
            snapshot.limits.remaining_tool_calls, snapshot.limits.max_tool_calls
        );
        println!();
        println!("system prompt:");
        println!("{}", snapshot.system_prompt);
        println!();
        println!("conversation messages: {}", snapshot.conversation.len());
        for message in &snapshot.conversation {
            println!("  {:?}", message);
        }
        println!();
        println!("visible tools: {}", snapshot.visible_tools.len());
        for tool in &snapshot.visible_tools {
            println!(
                "  {} — {}",
                tool.id,
                tool.description.as_deref().unwrap_or("")
            );
        }
        println!("visible skills: {}", snapshot.visible_skills.len());
        println!("loaded memory fragments: {}", snapshot.loaded_memory.len());
        println!(
            "loaded ingestion artifacts: {}",
            snapshot.loaded_artifacts.len()
        );
    }

    Ok(())
}

pub async fn explain_config(json: bool) -> anyhow::Result<()> {
    let resolved = ConfigResolver::from_env().resolve_default_agent()?;
    let explanation = agent_core::ConfigExplanation {
        agent_id: resolved.agent.id.clone(),
        values: resolved.values,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&explanation)?);
    } else {
        println!("effective config for {}", explanation.agent_id);
        for value in explanation.values {
            println!("{:<36} {:<18} {}", value.key, value.source, value.value);
        }
    }

    Ok(())
}

pub async fn explain_tools(json: bool, enable_shell: bool) -> anyhow::Result<()> {
    let harness = inspection_harness(enable_shell);
    let options = setup::RuntimeOptions {
        enable_shell,
        ..setup::RuntimeOptions::default()
    };
    let agent = setup::build_agent(&options);
    let tools = harness.explain_tools(&agent);

    if json {
        println!("{}", serde_json::to_string_pretty(&tools)?);
    } else {
        println!("visible tools for {} ({})", agent.name, agent.id);
        for tool in tools {
            println!(
                "{:<16} {:<12} {}",
                tool.id,
                format!("{:?}", tool.visibility),
                tool.description.unwrap_or_default()
            );
        }
    }

    Ok(())
}

pub async fn call_tool(
    name: String,
    input: Option<String>,
    json: bool,
    require_approval: bool,
) -> anyhow::Result<()> {
    let input = read_optional_json(input)?;
    let enable_shell = name == "shell";
    let events = Arc::new(open_event_store()?);
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        events,
        setup::build_registry(enable_shell),
    );
    let options = setup::RuntimeOptions {
        enable_shell,
        require_approval,
        ..setup::RuntimeOptions::default()
    };
    let agent = setup::build_agent(&options);
    let result = harness.call_tool(&agent, ToolId::from(name), input).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result.output)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&result.output)?);
        eprintln!();
        eprintln!(
            "--- direct tool trace ({}, {} ms) ---",
            result.run_id.0, result.duration_ms
        );
        for evt in harness.events(result.run_id) {
            eprintln!("  [{}] {:?}", evt.id.0, evt.kind);
        }
    }

    Ok(())
}

fn inspection_harness(enable_shell: bool) -> Harness {
    Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store().expect("sqlite event store should open")),
        setup::build_registry(enable_shell),
    )
}

pub async fn trace_show(run_id: String, json: bool) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;

    if json {
        for evt in events {
            println!("{}", serde_json::to_string(&evt)?);
        }
    } else if events.is_empty() {
        println!("No events found for run {}", run_id.0);
    } else {
        println!("trace {}", run_id.0);
        for evt in events {
            println!("{}", format_event(&evt));
        }
    }

    Ok(())
}

pub async fn approval_list(run_id: String, json: bool) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let approvals = approvals_for_run(run_id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&approvals)?);
    } else if approvals.is_empty() {
        println!("No approvals found for run {}", run_id.0);
    } else {
        for approval in approvals {
            println!(
                "{} {} action={} reason={}",
                approval["approval_id"], approval["status"], approval["action"], approval["reason"]
            );
        }
    }
    Ok(())
}

pub async fn approval_decide(
    run_id: String,
    approval_id: String,
    approved: bool,
) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    store.append(
        run_id,
        None,
        RunEventKind::ApprovalResolved {
            approval_id: approval_id.clone(),
            approved,
        },
    );
    println!(
        "{} approval {} for {}",
        if approved { "approved" } else { "rejected" },
        approval_id,
        run_id.0
    );
    Ok(())
}

pub async fn approval_execute(
    run_id: String,
    approval_id: String,
    json: bool,
) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;
    let approved = events.iter().rev().find_map(|event| match &event.kind {
        RunEventKind::ApprovalResolved {
            approval_id: id,
            approved,
        } if id == &approval_id => Some(*approved),
        _ => None,
    });
    if approved != Some(true) {
        anyhow::bail!(
            "approval {approval_id} is not approved for run {}",
            run_id.0
        );
    }
    let requested_parent = events.iter().find_map(|event| match &event.kind {
        RunEventKind::ApprovalRequested {
            approval_id: id, ..
        } if id == &approval_id => event.parent_event,
        _ => None,
    });
    let Some(proposed_id) = requested_parent else {
        anyhow::bail!("approval {approval_id} is not linked to a tool proposal");
    };
    let proposed = events.iter().find_map(|event| {
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
    });
    let Some((call_id, tool_id, input)) = proposed else {
        anyhow::bail!("tool proposal for approval {approval_id} was not found");
    };
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

    let registry = setup::build_registry(tool_id == "shell");
    store.append(
        run_id,
        Some(proposed_id),
        RunEventKind::ToolCallStarted {
            call_id: call_id.clone(),
        },
    );
    let started = Instant::now();
    let output = match registry
        .execute(&ToolId::from(tool_id.clone()), input)
        .await
    {
        Ok(output) => output,
        Err(err) => {
            let reason = err.to_string();
            store.append(
                run_id,
                Some(proposed_id),
                RunEventKind::ToolCallFailed {
                    call_id,
                    error: reason.clone(),
                },
            );
            anyhow::bail!(reason);
        }
    };
    let duration_ms = started.elapsed().as_millis() as u64;
    store.append(
        run_id,
        Some(proposed_id),
        RunEventKind::ToolCallCompleted {
            call_id: call_id.clone(),
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

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&output)?);
        eprintln!("executed approved tool call {call_id} in {duration_ms} ms");
    }
    Ok(())
}

pub async fn guide(run_id: String, text: String) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    store.append(
        run_id,
        None,
        RunEventKind::GuidanceInjected { content: text },
    );
    println!("recorded guidance for {}", run_id.0);
    Ok(())
}

pub async fn cancel(run_id: String, reason: String) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    store.append(run_id, None, RunEventKind::RunCancelled { reason });
    println!("recorded cancellation for {}", run_id.0);
    Ok(())
}

pub async fn score(run_id: String, target: String, score: f32) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    store.append(run_id, None, RunEventKind::QualityScored { target, score });
    println!("recorded score for {}", run_id.0);
    Ok(())
}

pub async fn batch_run(items: Vec<String>, demo: Demo, json: bool) -> anyhow::Result<()> {
    let batch_run_id = RunId::new();
    let batch_id = format!("batch-{}", batch_run_id.0);
    let store = Arc::new(open_event_store()?);
    store.append(
        batch_run_id,
        None,
        RunEventKind::BatchRunStarted {
            batch_id: batch_id.clone(),
            items: items.len() as u32,
        },
    );

    let mut succeeded = 0;
    let mut failed = 0;
    let mut summaries = Vec::new();
    for (idx, item) in items.into_iter().enumerate() {
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
        let options = setup::RuntimeOptions::default();
        let provider = setup::build_provider(demo, &item, &options)?;
        let harness = Harness::new(
            provider,
            store.clone(),
            setup::build_registry(options.enable_shell),
        );
        let agent = setup::build_agent(&options);
        match harness.run(&agent, UserInput { text: item.clone() }).await {
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

    let summary = serde_json::json!({
        "batch_run_id": batch_run_id.0,
        "batch_id": batch_id,
        "succeeded": succeeded,
        "failed": failed,
        "items": summaries
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "batch {} succeeded={} failed={}",
            summary["batch_id"], succeeded, failed
        );
        for item in summary["items"].as_array().into_iter().flatten() {
            println!("{}", serde_json::to_string(item)?);
        }
    }
    Ok(())
}

pub async fn memory_create(content: String, user: bool) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let record = MemoryStore::from_env().create(target, &content, MemoryAuthor::Human, None)?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    Ok(())
}

pub async fn memory_generate(
    text: String,
    user: bool,
    range: Option<String>,
) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let records = MemoryStore::from_env().generate_from_text(target, &text, range)?;
    println!("{}", serde_json::to_string_pretty(&records)?);
    Ok(())
}

pub async fn memory_list(json: bool) -> anyhow::Result<()> {
    let records = MemoryStore::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else {
        for record in records {
            println!(
                "{} {:?} {:?} {}",
                record.id, record.target, record.author, record.content
            );
        }
    }
    Ok(())
}

pub async fn memory_edit(id: String, content: String) -> anyhow::Result<()> {
    let record = MemoryStore::from_env().edit(&id, &content)?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    Ok(())
}

pub async fn memory_delete(id: String) -> anyhow::Result<()> {
    MemoryStore::from_env().delete(&id)?;
    println!("deleted memory {id}");
    Ok(())
}

pub async fn memory_rollback(user: bool) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    MemoryStore::from_env().rollback(target)?;
    println!("rolled back {}", if user { "user.md" } else { "memory.md" });
    Ok(())
}

pub async fn skill_import_openclaw(path: String) -> anyhow::Result<()> {
    let doc = SkillRegistry::from_env().import_openclaw(path)?;
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

pub async fn skill_list(json: bool) -> anyhow::Result<()> {
    let docs = SkillRegistry::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&docs)?);
    } else {
        for doc in docs {
            let state = if doc.quarantined {
                "quarantined"
            } else {
                "allowed"
            };
            println!("{} {} {}", doc.id, state, doc.name);
        }
    }
    Ok(())
}

pub async fn skill_inspect(id: String) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&SkillRegistry::from_env().inspect(&id)?)?
    );
    Ok(())
}

pub async fn skill_allow(id: String) -> anyhow::Result<()> {
    let doc = SkillRegistry::from_env().allow(&id)?;
    println!("allowed skill {}", doc.id);
    Ok(())
}

pub async fn skill_quarantine(id: String) -> anyhow::Result<()> {
    let doc = SkillRegistry::from_env().quarantine(&id)?;
    println!("quarantined skill {}", doc.id);
    Ok(())
}

pub async fn ingest_add(path: String) -> anyhow::Result<()> {
    let artifact = IngestionStore::from_env().ingest(path)?;
    println!("{}", serde_json::to_string_pretty(&artifact)?);
    Ok(())
}

pub async fn ingest_list(json: bool) -> anyhow::Result<()> {
    let artifacts = IngestionStore::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&artifacts)?);
    } else {
        for artifact in artifacts {
            println!(
                "{} {} sections={} source={}",
                artifact.id,
                artifact.backend,
                artifact.sections.len(),
                artifact.source.display()
            );
        }
    }
    Ok(())
}

pub async fn ingest_show(id: String, json: bool) -> anyhow::Result<()> {
    let artifact = IngestionStore::from_env().show(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&artifact)?);
    } else {
        println!("{} {}", artifact.id, artifact.source.display());
        println!("backend: {}", artifact.backend);
        println!("hash: {}", artifact.content_hash);
        println!("sections: {}", artifact.sections.len());
        println!();
        println!("{}", artifact.extracted_text.unwrap_or_default());
    }
    Ok(())
}

pub async fn ingest_rm(id: String) -> anyhow::Result<()> {
    IngestionStore::from_env().remove(&id)?;
    println!("removed ingestion artifact {id}");
    Ok(())
}

pub async fn adapter_inspect(path: String, json: bool) -> anyhow::Result<()> {
    let package = inspect_source(path)?;
    print_adapter_package(package, json)?;
    Ok(())
}

pub async fn adapter_import(path: String) -> anyhow::Result<()> {
    let package = AdapterRegistry::from_env().import(path)?;
    println!("{}", serde_json::to_string_pretty(&package)?);
    Ok(())
}

pub async fn adapter_list(json: bool) -> anyhow::Result<()> {
    let packages = AdapterRegistry::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&packages)?);
    } else {
        for package in packages {
            println!(
                "{} {:?} digest={} quarantined={}",
                package.id, package.adapter, package.digest, package.quarantined
            );
        }
    }
    Ok(())
}

pub async fn adapter_show(id: String, json: bool) -> anyhow::Result<()> {
    let package = AdapterRegistry::from_env().show(&id)?;
    print_adapter_package(package, json)?;
    Ok(())
}

pub async fn adapter_allow(id: String) -> anyhow::Result<()> {
    let package = AdapterRegistry::from_env().allow(&id)?;
    println!("allowed adapter {}", package.id);
    Ok(())
}

pub async fn adapter_quarantine(id: String) -> anyhow::Result<()> {
    let package = AdapterRegistry::from_env().quarantine(&id)?;
    println!("quarantined adapter {}", package.id);
    Ok(())
}

fn print_adapter_package(package: NormalizedPackage, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&package)?);
    } else {
        println!(
            "{} {:?} digest={} quarantined={}",
            package.id, package.adapter, package.digest, package.quarantined
        );
        println!(
            "permissions shell={} file_read={} file_write={} network={} secrets={}",
            package.permissions.shell,
            package.permissions.file_read,
            package.permissions.file_write,
            package.permissions.network,
            package.permissions.secrets
        );
        for capability in package.capabilities {
            println!(
                "capability {:?} {} quarantined={}",
                capability.kind, capability.id, capability.quarantined
            );
        }
        for finding in package.findings {
            println!("finding {:?}: {}", finding.severity, finding.message);
        }
    }
    Ok(())
}

pub async fn bundle_export(path: String) -> anyhow::Result<()> {
    let manifest = export_bundle(path)?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

pub async fn bundle_import(path: String) -> anyhow::Result<()> {
    let manifest = import_bundle(path)?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

pub async fn remote_health(url: String) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    println!(
        "{}",
        serde_json::to_string_pretty(&client.get_json("/health")?)?
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&client.get_json("/version")?)?
    );
    Ok(())
}

pub async fn remote_run(
    url: String,
    input: String,
    demo: String,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.post_json(
        "/run",
        serde_json::json!({
            "input": input,
            "demo": demo,
            "provider": provider_name(options.provider),
            "model": options.model,
            "api_base_url": options.api_base_url,
            "api_key_env": options.api_key_env,
            "enable_shell": options.enable_shell,
            "load_memory": options.load_memory,
            "load_skills": options.load_skills,
            "include_ingest": options.include_ingest,
            "require_approval": options.require_approval
        }),
    )?)
}

pub async fn remote_preview_context(
    url: String,
    input: String,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/preview-context",
        serde_json::json!({
            "input": input,
            "enable_shell": options.enable_shell,
            "load_memory": options.load_memory,
            "load_skills": options.load_skills,
            "include_ingest": options.include_ingest
        }),
    )?)
}

pub async fn remote_guide(url: String, run_id: String, text: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/guide",
        serde_json::json!({ "run_id": run_id, "text": text }),
    )?)
}

pub async fn remote_cancel(url: String, run_id: String, reason: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/cancel",
        serde_json::json!({ "run_id": run_id, "reason": reason }),
    )?)
}

pub async fn remote_score(
    url: String,
    run_id: String,
    target: String,
    score: f32,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/score",
        serde_json::json!({ "run_id": run_id, "target": target, "score": score }),
    )?)
}

pub async fn remote_batch_run(url: String, items: Vec<String>, demo: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/batch",
        serde_json::json!({ "items": items, "demo": demo }),
    )?)
}

pub async fn remote_tool(
    url: String,
    name: String,
    input: Option<String>,
    require_approval: bool,
) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    let mut input = read_optional_json(input)?;
    if require_approval {
        match input.as_object_mut() {
            Some(map) => {
                map.insert("__require_approval".into(), serde_json::Value::Bool(true));
            }
            None => {
                input = serde_json::json!({
                    "__require_approval": true,
                    "value": input
                });
            }
        }
    }
    print_remote(client.post_json(&format!("/tool/{name}"), input)?)
}

pub async fn remote_trace(url: String, run_id: String) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.get_json(&format!("/trace/{run_id}"))?)
}

pub async fn remote_approval_list(url: String, run_id: String) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.get_json(&format!("/approvals/{run_id}"))?)
}

pub async fn remote_approval_decide(
    url: String,
    run_id: String,
    approval_id: String,
    approved: bool,
) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.post_json(
        &format!("/approvals/{run_id}/{approval_id}/decide"),
        serde_json::json!({ "approved": approved }),
    )?)
}

pub async fn remote_approval_execute(
    url: String,
    run_id: String,
    approval_id: String,
) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.post_json(
        &format!("/approvals/{run_id}/{approval_id}/execute"),
        serde_json::json!({}),
    )?)
}

pub async fn remote_memory_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/memory")?)
}

pub async fn remote_memory_create(url: String, content: String, user: bool) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory",
        serde_json::json!({ "content": content, "user": user }),
    )?)
}

pub async fn remote_memory_generate(
    url: String,
    text: String,
    user: bool,
    range: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory/generate",
        serde_json::json!({ "text": text, "user": user, "range": range }),
    )?)
}

pub async fn remote_memory_edit(url: String, id: String, content: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/memory/{id}/edit"),
        serde_json::json!({ "content": content }),
    )?)
}

pub async fn remote_memory_delete(url: String, id: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json(&format!("/memory/{id}/delete"), serde_json::json!({}))?,
    )
}

pub async fn remote_memory_rollback(url: String, user: bool) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/memory/rollback", serde_json::json!({ "user": user }))?,
    )
}

pub async fn remote_skill_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/skills")?)
}

pub async fn remote_skill_import(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/skills/import", serde_json::json!({ "path": path }))?,
    )
}

pub async fn remote_skill_action(url: String, id: String, allow: bool) -> anyhow::Result<()> {
    let action = if allow { "allow" } else { "quarantine" };
    print_remote(
        DaemonHttpClient::new(url)
            .post_json(&format!("/skills/{id}/{action}"), serde_json::json!({}))?,
    )
}

pub async fn remote_ingest_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/ingest")?)
}

pub async fn remote_ingest_add(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url).post_json("/ingest", serde_json::json!({ "path": path }))?,
    )
}

pub async fn remote_ingest_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/ingest/{id}"))?)
}

pub async fn remote_ingest_rm(url: String, id: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url).post_json(&format!("/ingest/{id}/rm"), serde_json::json!({}))?,
    )
}

pub async fn remote_adapter_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/adapters")?)
}

pub async fn remote_adapter_import(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/adapters/import", serde_json::json!({ "path": path }))?,
    )
}

pub async fn remote_adapter_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/adapters/{id}"))?)
}

pub async fn remote_adapter_action(url: String, id: String, allow: bool) -> anyhow::Result<()> {
    let action = if allow { "allow" } else { "quarantine" };
    print_remote(
        DaemonHttpClient::new(url)
            .post_json(&format!("/adapters/{id}/{action}"), serde_json::json!({}))?,
    )
}

pub async fn remote_bundle_export(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/bundles/export", serde_json::json!({ "path": path }))?,
    )
}

pub async fn remote_bundle_import(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/bundles/import", serde_json::json!({ "path": path }))?,
    )
}

fn print_remote(value: serde_json::Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Fake => "fake",
        Provider::Rig => "rig",
    }
}

fn open_event_store() -> anyhow::Result<SqliteEventStore> {
    let paths = StoragePaths::from_env();
    paths.ensure_base_dirs()?;
    Ok(SqliteEventStore::open(paths.state_db())?)
}

fn approvals_for_run(run_id: RunId) -> anyhow::Result<Vec<serde_json::Value>> {
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
    Ok(approvals)
}

fn format_event(evt: &RunEvent) -> String {
    let parent = evt
        .parent_event
        .map(|id| id.0.to_string())
        .unwrap_or_else(|| "-".into());
    format!(
        "[{}] parent={} {} {}",
        evt.id.0,
        parent,
        evt.at.to_rfc3339(),
        event_label(&evt.kind)
    )
}

fn event_label(kind: &RunEventKind) -> String {
    match kind {
        RunEventKind::RunStarted { agent_id, input } => {
            format!("RunStarted agent={agent_id} input={input:?}")
        }
        RunEventKind::ContextBuilt { snapshot } => {
            let tools = snapshot
                .get("visible_tools")
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len);
            format!("ContextBuilt visible_tools={tools}")
        }
        RunEventKind::LlmRequestStarted { model } => {
            format!("LlmRequestStarted model={model}")
        }
        RunEventKind::LlmRequestCompleted {
            tokens_in,
            tokens_out,
            duration_ms,
        } => format!(
            "LlmRequestCompleted tokens_in={tokens_in} tokens_out={tokens_out} duration_ms={duration_ms}"
        ),
        RunEventKind::ToolCallProposed {
            call_id,
            tool_id,
            input,
        } => format!("ToolCallProposed call={call_id} tool={tool_id} input={input}"),
        RunEventKind::ToolCallStarted { call_id } => {
            format!("ToolCallStarted call={call_id}")
        }
        RunEventKind::ToolCallCompleted {
            call_id,
            output,
            duration_ms,
        } => format!("ToolCallCompleted call={call_id} duration_ms={duration_ms} output={output}"),
        RunEventKind::ToolCallFailed { call_id, error } => {
            format!("ToolCallFailed call={call_id} error={error}")
        }
        RunEventKind::ApprovalRequested {
            approval_id,
            action,
            reason,
        } => format!("ApprovalRequested id={approval_id} action={action} reason={reason}"),
        RunEventKind::ApprovalResolved {
            approval_id,
            approved,
        } => format!("ApprovalResolved id={approval_id} approved={approved}"),
        RunEventKind::GuidanceInjected { content } => {
            format!("GuidanceInjected content={content:?}")
        }
        RunEventKind::QualityScored { target, score } => {
            format!("QualityScored target={target} score={score}")
        }
        RunEventKind::MemoryLoaded { ids } => {
            format!("MemoryLoaded ids={}", ids.join(","))
        }
        RunEventKind::MemoryWritten { id, operation } => {
            format!("MemoryWritten id={id} operation={operation}")
        }
        RunEventKind::IngestionReferenced {
            artifact_id,
            source,
        } => format!("IngestionReferenced artifact={artifact_id} source={source}"),
        RunEventKind::ChildRunStarted {
            child_run_id,
            agent_id,
        } => format!("ChildRunStarted child={} agent={agent_id}", child_run_id.0),
        RunEventKind::ChildRunCompleted {
            child_run_id,
            status,
        } => format!("ChildRunCompleted child={} status={status}", child_run_id.0),
        RunEventKind::BatchRunStarted { batch_id, items } => {
            format!("BatchRunStarted batch={batch_id} items={items}")
        }
        RunEventKind::BatchItemStatus {
            batch_id,
            item_key,
            status,
        } => format!("BatchItemStatus batch={batch_id} item={item_key} status={status}"),
        RunEventKind::BatchRunCompleted {
            batch_id,
            succeeded,
            failed,
        } => format!("BatchRunCompleted batch={batch_id} succeeded={succeeded} failed={failed}"),
        RunEventKind::RunPaused { reason } => format!("RunPaused reason={reason}"),
        RunEventKind::RunCancelled { reason } => format!("RunCancelled reason={reason}"),
        RunEventKind::RunCompleted {
            final_output,
            total_duration_ms,
        } => format!("RunCompleted duration_ms={total_duration_ms} output={final_output:?}"),
        RunEventKind::RunFailed { reason } => format!("RunFailed reason={reason}"),
    }
}

fn read_text(input: Option<String>) -> anyhow::Result<String> {
    match input {
        Some(t) => Ok(t),
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf.trim().to_string())
        }
    }
}

fn read_optional_json(input: Option<String>) -> anyhow::Result<serde_json::Value> {
    let text = read_text(input)?;
    if text.trim().is_empty() {
        Ok(serde_json::json!({}))
    } else {
        Ok(serde_json::from_str(&text)?)
    }
}

enum SlashCommand {
    Agent,
    Tool { name: String, input: String },
    Run(String),
}

fn parse_slash_command(text: &str) -> anyhow::Result<Option<SlashCommand>> {
    let trimmed = text.trim();
    if trimmed == "/agent" {
        return Ok(Some(SlashCommand::Agent));
    }
    if let Some(rest) = trimmed.strip_prefix("/run ") {
        return Ok(Some(SlashCommand::Run(rest.trim().to_string())));
    }
    if let Some(rest) = trimmed
        .strip_prefix("/tool! ")
        .or_else(|| trimmed.strip_prefix("/tool "))
    {
        let (name, input) = rest
            .trim()
            .split_once(' ')
            .map(|(name, input)| (name.to_string(), input.trim().to_string()))
            .unwrap_or_else(|| (rest.trim().to_string(), "{}".into()));
        if name.is_empty() {
            anyhow::bail!("missing tool name");
        }
        let _: serde_json::Value = serde_json::from_str(&input)?;
        return Ok(Some(SlashCommand::Tool { name, input }));
    }
    Ok(None)
}
