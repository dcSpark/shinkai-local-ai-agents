//! Headless `--print` mode. Reads input (CLI flag or stdin), runs once, and
//! emits either a human-readable transcript on stderr (with the final answer
//! on stdout) or one JSON `RunEvent` per line on stdout (`--json`).

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::sync::Arc;
use std::time::Instant;

use agent_adapters::{AdapterRegistry, NormalizedPackage, inspect_source};
use agent_api_client::DaemonHttpClient;
use agent_batch::{BatchItemState, BatchPlan};
use agent_bundles::{export_bundle, import_bundle};
use agent_config::{ConfigResolver, ModelConfig};
use agent_core::{Harness, HarnessApi, UserInput, VisibilityLevel};
use agent_ingest::{IngestionArtifact, IngestionStore};
use agent_llm::FakeProvider;
use agent_memory::{MemoryAuthor, MemoryRecord, MemoryStore, MemoryTarget};
use agent_prompts::{PromptStore, is_valid_prompt_name};
use agent_skills::SkillRegistry;
use agent_storage::StoragePaths;
use agent_tools::ToolId;
use agent_tracing::{
    EventStore, RunEvent, RunEventKind, RunId, SqliteEventStore, TraceSummary,
    is_terminal_run_event, latest_event_id, summarize_trace, validate_guidance_content,
    validate_quality_score,
};

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
        Some(SlashCommand::Run(prompt)) => text = resolve_saved_prompt_or_literal(&prompt)?,
        Some(SlashCommand::Guide { run_id, text }) => return guide(run_id, text).await,
        Some(SlashCommand::Score {
            run_id,
            score: value,
            target,
        }) => return score(run_id, target, value).await,
        None => {}
    }

    let provider = setup::build_provider(demo, &text, &options)?;
    let events = Arc::new(open_event_store()?);
    let registry = setup::build_registry(options.enable_shell, options.enable_subagent);
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
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let text = read_text(input)?;
    let harness = inspection_harness(options.enable_shell, options.enable_subagent);
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
        println!(
            "estimated input tokens: {}",
            snapshot.estimated_input_tokens
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

pub async fn storage_report(json: bool) -> anyhow::Result<()> {
    let report = StoragePaths::from_env().storage_report()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("storage root: {}", report.root.display());
        println!(
            "total: {} bytes, {} files, {} directories",
            report.total_bytes, report.total_files, report.total_directories
        );
        if let Some(path) = &report.largest_file {
            println!(
                "largest: {} ({} bytes)",
                path.display(),
                report.largest_file_bytes
            );
        }
        for bucket in report.buckets {
            let largest = bucket
                .largest_file
                .as_ref()
                .map(|path| {
                    format!(
                        " largest={} ({})",
                        path.display(),
                        bucket.largest_file_bytes
                    )
                })
                .unwrap_or_default();
            println!(
                "{:<10} {:>12} bytes {:>6} files {:>6} dirs {}{}{}",
                bucket.name,
                bucket.bytes,
                bucket.files,
                bucket.directories,
                if bucket.exists { "" } else { "(missing) " },
                bucket.path.display(),
                largest
            );
        }
    }

    Ok(())
}

pub async fn explain_tools(
    json: bool,
    enable_shell: bool,
    enable_subagent: bool,
    tool_visibility: Option<VisibilityLevel>,
) -> anyhow::Result<()> {
    let harness = inspection_harness(enable_shell, enable_subagent);
    let options = setup::RuntimeOptions {
        enable_shell,
        enable_subagent,
        tool_visibility,
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
    let enable_subagent = name == "subagent";
    let events = Arc::new(open_event_store()?);
    let harness = Harness::new(
        Arc::new(FakeProvider::echo()),
        events,
        setup::build_registry(enable_shell, enable_subagent),
    );
    let options = setup::RuntimeOptions {
        enable_shell,
        enable_subagent,
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

fn inspection_harness(enable_shell: bool, enable_subagent: bool) -> Harness {
    Harness::new(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store().expect("sqlite event store should open")),
        setup::build_registry(enable_shell, enable_subagent),
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

pub async fn trace_summary(run_id: String, json: bool) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;

    if events.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&TraceSummary::empty(run_id))?
            );
        } else {
            println!("No events found for run {}", run_id.0);
        }
        return Ok(());
    }

    let summary = summarize_trace(&events, run_id);
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        print_trace_summary(&summary);
    }
    Ok(())
}

fn print_trace_summary(summary: &TraceSummary) {
    println!("trace {}", summary.run_id.0);
    println!("events: {}", summary.events);
    println!("contexts: {}", summary.context_snapshots);
    println!("llm calls: {}", summary.llm_calls);
    println!("tool calls: {}", summary.tool_calls);
    println!("tokens: {}/{}", summary.tokens_in, summary.tokens_out);
    match summary.cost_usd {
        Some(cost) => println!("cost: ${cost:.6}"),
        None => println!("cost: n/a"),
    }
    match summary.duration_ms {
        Some(duration) => println!("duration: {duration}ms"),
        None => println!("duration: n/a"),
    }
    println!("approvals: {}", summary.approvals);
    println!("guidance: {}", summary.guidance_injections);
    println!("scores: {}", summary.quality_scores);
    println!("memory fragments: {}", summary.memory_fragments);
    println!("artifact refs: {}", summary.artifact_refs);
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

    let registry = setup::build_registry(tool_id == "shell", tool_id == "subagent");
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

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&output)?);
        eprintln!("executed approved tool call {call_id} in {duration_ms} ms");
    }
    Ok(())
}

pub async fn guide(run_id: String, text: String) -> anyhow::Result<()> {
    let text = validate_guidance_content(&text)?;
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;
    if events
        .iter()
        .any(|event| is_terminal_run_event(&event.kind))
    {
        anyhow::bail!("run {run_id} is terminal and cannot accept guidance");
    }
    let parent = latest_event_id(&events)
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no trace events"))?;
    store.append(
        run_id,
        Some(parent),
        RunEventKind::GuidanceInjected { content: text },
    );
    println!("recorded guidance for {}", run_id.0);
    Ok(())
}

pub async fn cancel(run_id: String, reason: String) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;
    if events
        .iter()
        .any(|event| is_terminal_run_event(&event.kind))
    {
        println!(
            "run {} is already terminal; cancellation not recorded",
            run_id.0
        );
        return Ok(());
    }
    let parent = latest_event_id(&events)
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no trace events"))?;
    store.append(run_id, Some(parent), RunEventKind::RunCancelled { reason });
    println!("recorded cancellation for {}", run_id.0);
    Ok(())
}

pub async fn score(run_id: String, target: String, score: f32) -> anyhow::Result<()> {
    validate_quality_score(score)?;
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let parent = latest_event_id(&store.try_events(run_id)?)
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no trace events"))?;
    store.append(
        run_id,
        Some(parent),
        RunEventKind::QualityScored { target, score },
    );
    println!("recorded score for {}", run_id.0);
    Ok(())
}

pub async fn batch_run(items: Vec<String>, demo: Demo, json: bool) -> anyhow::Result<()> {
    let batch_run_id = RunId::new();
    let batch_id = format!("batch-{}", batch_run_id.0);
    let mut plan = BatchPlan::new(batch_id.clone(), items);
    plan.save_to_env()?;
    execute_batch_plan(plan, batch_run_id, batch_id, demo, json).await
}

pub async fn batch_resume(batch_id: String, demo: Demo, json: bool) -> anyhow::Result<()> {
    let batch_run_id = RunId::new();
    let plan = BatchPlan::load_from_env(&batch_id)?;
    execute_batch_plan(plan, batch_run_id, batch_id, demo, json).await
}

async fn execute_batch_plan(
    mut plan: BatchPlan,
    batch_run_id: RunId,
    batch_id: String,
    demo: Demo,
    json: bool,
) -> anyhow::Result<()> {
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
        let options = setup::RuntimeOptions::default();
        let provider = setup::build_provider(demo, &item.input, &options)?;
        let harness = Harness::new(
            provider,
            store.clone(),
            setup::build_registry(options.enable_shell, options.enable_subagent),
        );
        let agent = setup::build_agent(&options);
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

    let summary = serde_json::json!({
        "batch_run_id": batch_run_id.0,
        "batch_id": batch_id,
        "succeeded": plan.succeeded_count(),
        "failed": plan.failed_count(),
        "skipped": skipped,
        "items": summaries
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "batch {} succeeded={} failed={} skipped={}",
            summary["batch_id"],
            plan.succeeded_count(),
            plan.failed_count(),
            skipped
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
    record_memory_written(&record, "created")?;
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
    for record in &records {
        record_memory_written(record, "generated")?;
    }
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
    record_memory_written(&record, "edited")?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    Ok(())
}

pub async fn memory_delete(id: String) -> anyhow::Result<()> {
    MemoryStore::from_env().delete(&id)?;
    record_memory_operation(&id, "deleted", None, None)?;
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
    record_memory_operation(
        if user { "user.md" } else { "memory.md" },
        "rolled_back",
        None,
        None,
    )?;
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

pub async fn prompt_save(name: String, text: String) -> anyhow::Result<()> {
    let prompt = PromptStore::from_env().save(&name, &text)?;
    println!("{}", serde_json::to_string_pretty(&prompt)?);
    Ok(())
}

pub async fn prompt_list(json: bool) -> anyhow::Result<()> {
    let prompts = PromptStore::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&prompts)?);
    } else {
        for prompt in prompts {
            let first_line = prompt.body.lines().next().unwrap_or_default();
            println!("{} {}", prompt.name, first_line);
        }
    }
    Ok(())
}

pub async fn prompt_show(name: String, json: bool) -> anyhow::Result<()> {
    let Some(prompt) = PromptStore::from_env().get(&name)? else {
        anyhow::bail!("saved prompt {name:?} not found");
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&prompt)?);
    } else {
        println!("{}", prompt.body);
    }
    Ok(())
}

pub async fn prompt_delete(name: String) -> anyhow::Result<()> {
    if PromptStore::from_env().delete(&name)? {
        println!("deleted prompt {name}");
    } else {
        println!("prompt {name} not found");
    }
    Ok(())
}

pub async fn model_list(json: bool) -> anyhow::Result<()> {
    let models = ConfigResolver::from_env().list_models()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&models)?);
    } else {
        for model in models {
            println!(
                "{} max_output={:?} temperature={:?} tool_support={:?}",
                model.id, model.max_output_tokens, model.default_temperature, model.tool_support
            );
        }
    }
    Ok(())
}

pub async fn model_show(id: String, json: bool) -> anyhow::Result<()> {
    let Some(model) = ConfigResolver::from_env().show_model(&id)? else {
        anyhow::bail!("model {id:?} not found");
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&model)?);
    } else {
        println!("{}", toml::to_string_pretty(&model)?);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn model_save(
    id: String,
    max_context_tokens: Option<u64>,
    max_output_tokens: Option<u64>,
    default_temperature: Option<f64>,
    available_modalities: Vec<String>,
    reasoning_mode: Option<String>,
    tool_support: Option<bool>,
    privacy_level: Option<String>,
    cost_tier: Option<String>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    metadata_json: Option<String>,
) -> anyhow::Result<()> {
    let model = model_config_from_parts(
        id,
        max_context_tokens,
        max_output_tokens,
        default_temperature,
        available_modalities,
        reasoning_mode,
        tool_support,
        privacy_level,
        cost_tier,
        input_cost_per_million,
        output_cost_per_million,
        metadata_json,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&ConfigResolver::from_env().save_model(&model)?)?
    );
    Ok(())
}

pub async fn model_delete(id: String) -> anyhow::Result<()> {
    if ConfigResolver::from_env().delete_model(&id)? {
        println!("deleted model {id}");
    } else {
        println!("model {id} not found");
    }
    Ok(())
}

pub async fn ingest_add(path: String, backend: String) -> anyhow::Result<()> {
    let trace_run_id = RunId::new();
    let store = open_event_store()?;
    let source = path.clone();
    let started = store.append(
        trace_run_id,
        None,
        RunEventKind::IngestionStarted {
            source: source.clone(),
            backend: backend.clone(),
        },
    );
    let artifact = IngestionStore::from_env().ingest_with_backend(path, &backend)?;
    store.append(
        trace_run_id,
        Some(started.id),
        ingestion_completed_event(&artifact),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "trace_run_id": trace_run_id.0,
            "artifact": artifact
        }))?
    );
    Ok(())
}

pub async fn ingest_rerun(id: String, backend: String) -> anyhow::Result<()> {
    let source = IngestionStore::from_env().show(&id)?.source;
    ingest_add(source.display().to_string(), backend).await
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
            "input_cost_per_million": options.input_cost_per_million,
            "output_cost_per_million": options.output_cost_per_million,
            "max_tool_calls": options.max_tool_calls,
            "tool_visibility": options.tool_visibility,
            "enable_shell": options.enable_shell,
            "enable_subagent": options.enable_subagent,
            "raw_tool_output": options.raw_tool_output,
            "load_memory": options.load_memory,
            "load_skills": options.load_skills,
            "include_ingest": options.include_ingest,
            "allow_unsafe_ingest": options.allow_unsafe_ingest,
            "require_approval": options.require_approval
        }),
    )?)
}

pub async fn remote_run_start(
    url: String,
    input: String,
    demo: String,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.post_json(
        "/run/start",
        serde_json::json!({
            "input": input,
            "demo": demo,
            "provider": provider_name(options.provider),
            "model": options.model,
            "api_base_url": options.api_base_url,
            "api_key_env": options.api_key_env,
            "input_cost_per_million": options.input_cost_per_million,
            "output_cost_per_million": options.output_cost_per_million,
            "max_tool_calls": options.max_tool_calls,
            "tool_visibility": options.tool_visibility,
            "enable_shell": options.enable_shell,
            "enable_subagent": options.enable_subagent,
            "raw_tool_output": options.raw_tool_output,
            "load_memory": options.load_memory,
            "load_skills": options.load_skills,
            "include_ingest": options.include_ingest,
            "allow_unsafe_ingest": options.allow_unsafe_ingest,
            "require_approval": options.require_approval
        }),
    )?)
}

pub async fn remote_run_status(url: String, run_id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/run/status/{run_id}"))?)
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
            "enable_subagent": options.enable_subagent,
            "max_tool_calls": options.max_tool_calls,
            "tool_visibility": options.tool_visibility,
            "raw_tool_output": options.raw_tool_output,
            "load_memory": options.load_memory,
            "load_skills": options.load_skills,
            "include_ingest": options.include_ingest,
            "allow_unsafe_ingest": options.allow_unsafe_ingest
        }),
    )?)
}

pub async fn remote_guide(url: String, run_id: String, text: String) -> anyhow::Result<()> {
    let text = validate_guidance_content(&text)?;
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
    validate_quality_score(score)?;
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

pub async fn remote_batch_resume(
    url: String,
    batch_id: String,
    demo: String,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/batch/resume",
        serde_json::json!({ "batch_id": batch_id, "demo": demo }),
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

pub async fn remote_trace_summary(url: String, run_id: String) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.get_json(&format!("/trace/{run_id}/summary"))?)
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

pub async fn remote_storage_report(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/storage")?)
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

pub async fn remote_model_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/models")?)
}

pub async fn remote_model_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/models/{id}"))?)
}

#[allow(clippy::too_many_arguments)]
pub async fn remote_model_save(
    url: String,
    id: String,
    max_context_tokens: Option<u64>,
    max_output_tokens: Option<u64>,
    default_temperature: Option<f64>,
    available_modalities: Vec<String>,
    reasoning_mode: Option<String>,
    tool_support: Option<bool>,
    privacy_level: Option<String>,
    cost_tier: Option<String>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    metadata_json: Option<String>,
) -> anyhow::Result<()> {
    let model = model_config_from_parts(
        id,
        max_context_tokens,
        max_output_tokens,
        default_temperature,
        available_modalities,
        reasoning_mode,
        tool_support,
        privacy_level,
        cost_tier,
        input_cost_per_million,
        output_cost_per_million,
        metadata_json,
    )?;
    print_remote(DaemonHttpClient::new(url).post_json("/models", serde_json::to_value(model)?)?)
}

pub async fn remote_model_delete(url: String, id: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json(&format!("/models/{id}/delete"), serde_json::json!({}))?,
    )
}

pub async fn remote_ingest_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/ingest")?)
}

#[allow(clippy::too_many_arguments)]
fn model_config_from_parts(
    id: String,
    max_context_tokens: Option<u64>,
    max_output_tokens: Option<u64>,
    default_temperature: Option<f64>,
    available_modalities: Vec<String>,
    reasoning_mode: Option<String>,
    tool_support: Option<bool>,
    privacy_level: Option<String>,
    cost_tier: Option<String>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    metadata_json: Option<String>,
) -> anyhow::Result<ModelConfig> {
    let metadata = match metadata_json {
        Some(text) => {
            let value: serde_json::Value = serde_json::from_str(&text)?;
            let Some(object) = value.as_object() else {
                anyhow::bail!("--metadata-json must be a JSON object");
            };
            object
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        }
        None => BTreeMap::new(),
    };
    let available_modalities = available_modalities
        .into_iter()
        .map(|modality| modality.trim().to_string())
        .filter(|modality| !modality.is_empty())
        .collect();
    Ok(ModelConfig {
        id,
        max_context_tokens,
        max_output_tokens,
        default_temperature,
        available_modalities,
        reasoning_mode,
        tool_support,
        privacy_level,
        cost_tier,
        input_cost_per_million,
        output_cost_per_million,
        metadata,
    })
}

pub async fn remote_ingest_add(url: String, path: String, backend: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/ingest",
        serde_json::json!({ "path": path, "backend": backend }),
    )?)
}

pub async fn remote_ingest_rerun(url: String, id: String, backend: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/ingest/{id}/rerun"),
        serde_json::json!({ "backend": backend }),
    )?)
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
        RunEventKind::LlmRequestStarted {
            model,
            request_digest,
        } => {
            let digest = request_digest
                .as_ref()
                .map(|value| format!(" request_digest={value}"))
                .unwrap_or_default();
            format!("LlmRequestStarted model={model}{digest}")
        }
        RunEventKind::LlmRequestCompleted {
            tokens_in,
            tokens_out,
            cost_usd,
            duration_ms,
        } => {
            let cost = cost_usd
                .map(|value| format!(" cost_usd={value:.6}"))
                .unwrap_or_default();
            format!(
                "LlmRequestCompleted tokens_in={tokens_in} tokens_out={tokens_out}{cost} duration_ms={duration_ms}"
            )
        }
        RunEventKind::PromptRefinementStarted {
            model,
            original_input,
            instructions,
        } => format!(
            "PromptRefinementStarted model={model} input={original_input:?} instructions={instructions:?}"
        ),
        RunEventKind::PromptRefinementCompleted {
            refined_input,
            tokens_in,
            tokens_out,
            cost_usd,
            duration_ms,
        } => {
            let cost = cost_usd
                .map(|value| format!(" cost_usd={value:.6}"))
                .unwrap_or_default();
            format!(
                "PromptRefinementCompleted tokens_in={tokens_in} tokens_out={tokens_out}{cost} duration_ms={duration_ms} refined={refined_input:?}"
            )
        }
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
            cost_usd,
            duration_ms,
        } => {
            let cost = cost_usd
                .map(|value| format!(" cost_usd={value:.6}"))
                .unwrap_or_default();
            format!(
                "ToolCallCompleted call={call_id} duration_ms={duration_ms}{cost} output={output}"
            )
        }
        RunEventKind::ToolOutputInterpreted {
            call_id,
            model,
            summary,
        } => {
            format!("ToolOutputInterpreted call={call_id} model={model} summary={summary:?}")
        }
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
        RunEventKind::MemoryRead {
            backend,
            fragment_ids,
        } => {
            format!(
                "MemoryRead backend={backend} fragment_ids={}",
                fragment_ids.join(",")
            )
        }
        RunEventKind::MemoryWritten {
            id,
            operation,
            source_range,
            generating_model,
        } => {
            let range = source_range
                .as_ref()
                .map(|value| format!(" range={value:?}"))
                .unwrap_or_default();
            let model = generating_model
                .as_ref()
                .map(|value| format!(" model={value}"))
                .unwrap_or_default();
            format!("MemoryWritten id={id} operation={operation}{range}{model}")
        }
        RunEventKind::IngestionReferenced {
            artifact_id,
            source,
        } => format!("IngestionReferenced artifact={artifact_id} source={source}"),
        RunEventKind::IngestionStarted { source, backend } => {
            format!("IngestionStarted source={source} backend={backend}")
        }
        RunEventKind::IngestionCompleted {
            artifact_id,
            content_hash,
            sections,
        } => format!(
            "IngestionCompleted artifact={artifact_id} hash={content_hash} sections={sections}"
        ),
        RunEventKind::PolicyDenied { reason } => format!("PolicyDenied reason={reason}"),
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
            total_cost_usd,
            total_duration_ms,
        } => {
            let cost = total_cost_usd
                .map(|value| format!(" cost_usd={value:.6}"))
                .unwrap_or_default();
            format!("RunCompleted duration_ms={total_duration_ms}{cost} output={final_output:?}")
        }
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

fn resolve_saved_prompt_or_literal(text: &str) -> anyhow::Result<String> {
    if !is_valid_prompt_name(text) {
        return Ok(text.to_string());
    }
    Ok(PromptStore::from_env()
        .get(text)?
        .map(|prompt| prompt.body)
        .unwrap_or_else(|| text.to_string()))
}

fn ingestion_completed_event(artifact: &IngestionArtifact) -> RunEventKind {
    RunEventKind::IngestionCompleted {
        artifact_id: artifact.id.clone(),
        content_hash: artifact.content_hash.clone(),
        sections: artifact.sections.len() as u32,
    }
}

fn record_memory_written(record: &MemoryRecord, operation: &str) -> anyhow::Result<()> {
    record_memory_operation(
        &record.id,
        operation,
        record.source_range.clone(),
        record.generating_model.clone(),
    )
}

fn record_memory_operation(
    id: &str,
    operation: &str,
    source_range: Option<String>,
    generating_model: Option<String>,
) -> anyhow::Result<()> {
    let run_id = RunId::new();
    open_event_store()?.append(
        run_id,
        None,
        RunEventKind::MemoryWritten {
            id: id.to_string(),
            operation: operation.to_string(),
            source_range,
            generating_model,
        },
    );
    Ok(())
}

enum SlashCommand {
    Agent,
    Tool {
        name: String,
        input: String,
    },
    Run(String),
    Guide {
        run_id: String,
        text: String,
    },
    Score {
        run_id: String,
        score: f32,
        target: String,
    },
}

fn parse_slash_command(text: &str) -> anyhow::Result<Option<SlashCommand>> {
    let trimmed = text.trim();
    if trimmed == "/agent" {
        return Ok(Some(SlashCommand::Agent));
    }
    if let Some(rest) = trimmed.strip_prefix("/run ") {
        return Ok(Some(SlashCommand::Run(rest.trim().to_string())));
    }
    if let Some(rest) = trimmed.strip_prefix("/guide ").map(str::trim) {
        let (run_id, text) = parse_guide_slash_rest(rest)?;
        return Ok(Some(SlashCommand::Guide { run_id, text }));
    }
    if let Some(rest) = trimmed.strip_prefix("/score ").map(str::trim) {
        let (run_id, score, target) = parse_score_slash_rest(rest)?;
        return Ok(Some(SlashCommand::Score {
            run_id,
            score,
            target,
        }));
    }
    if let Some(rest) = trimmed.strip_prefix("/tool!").map(str::trim) {
        let (name, input) = parse_tool_slash_rest(rest)?;
        return Ok(Some(SlashCommand::Tool { name, input }));
    }
    if let Some(rest) = trimmed.strip_prefix("/tool ").map(str::trim) {
        let (name, input) = parse_tool_slash_rest(rest)?;
        return Ok(Some(SlashCommand::Tool { name, input }));
    }
    Ok(None)
}

fn parse_tool_slash_rest(rest: &str) -> anyhow::Result<(String, String)> {
    let (name, input) = rest
        .trim()
        .split_once(char::is_whitespace)
        .map(|(name, input)| (name.to_string(), input.trim().to_string()))
        .unwrap_or_else(|| (rest.trim().to_string(), "{}".into()));
    if name.is_empty() {
        anyhow::bail!("missing tool name");
    }
    let _: serde_json::Value = serde_json::from_str(&input)?;
    Ok((name, input))
}

fn parse_guide_slash_rest(rest: &str) -> anyhow::Result<(String, String)> {
    let (run_id, text) = rest
        .trim()
        .split_once(char::is_whitespace)
        .map(|(run_id, text)| (run_id.trim().to_string(), text.trim().to_string()))
        .ok_or_else(|| anyhow::anyhow!("usage: /guide <run-id> <text>"))?;
    if run_id.is_empty() || text.is_empty() {
        anyhow::bail!("usage: /guide <run-id> <text>");
    }
    let _ = uuid::Uuid::parse_str(&run_id)?;
    Ok((run_id, text))
}

fn parse_score_slash_rest(rest: &str) -> anyhow::Result<(String, f32, String)> {
    let mut parts = rest.split_whitespace();
    let run_id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: /score <run-id> <0-10> [target]"))?
        .to_string();
    let score = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: /score <run-id> <0-10> [target]"))?
        .parse::<f32>()?;
    validate_quality_score(score)?;
    let target = parts.collect::<Vec<_>>().join(" ");
    let target = if target.trim().is_empty() {
        "last_answer".into()
    } else {
        target
    };
    let _ = uuid::Uuid::parse_str(&run_id)?;
    Ok((run_id, score, target))
}

#[cfg(test)]
mod slash_tests {
    use super::*;

    #[test]
    fn parses_direct_tool_without_space_after_bang() {
        let parsed = parse_slash_command(r#"/tool!echo {"text":"hi"}"#).unwrap();
        match parsed {
            Some(SlashCommand::Tool { name, input }) => {
                assert_eq!(name, "echo");
                assert_eq!(input, r#"{"text":"hi"}"#);
            }
            _ => panic!("expected tool command"),
        }
    }

    #[test]
    fn parses_direct_tool_with_implicit_empty_object() {
        let parsed = parse_slash_command("/tool!echo").unwrap();
        match parsed {
            Some(SlashCommand::Tool { name, input }) => {
                assert_eq!(name, "echo");
                assert_eq!(input, "{}");
            }
            _ => panic!("expected tool command"),
        }
    }

    #[test]
    fn parses_guide_shortcut_with_run_id() {
        let run_id = uuid::Uuid::new_v4().to_string();
        let parsed = parse_slash_command(&format!("/guide {run_id} steer here")).unwrap();
        match parsed {
            Some(SlashCommand::Guide { run_id: got, text }) => {
                assert_eq!(got, run_id);
                assert_eq!(text, "steer here");
            }
            _ => panic!("expected guide command"),
        }
    }

    #[test]
    fn parses_score_shortcut_with_default_target() {
        let run_id = uuid::Uuid::new_v4().to_string();
        let parsed = parse_slash_command(&format!("/score {run_id} 8.5")).unwrap();
        match parsed {
            Some(SlashCommand::Score {
                run_id: got,
                score,
                target,
            }) => {
                assert_eq!(got, run_id);
                assert_eq!(score, 8.5);
                assert_eq!(target, "last_answer");
            }
            _ => panic!("expected score command"),
        }
    }

    #[test]
    fn parses_score_shortcut_with_explicit_target() {
        let run_id = uuid::Uuid::new_v4().to_string();
        let parsed = parse_slash_command(&format!("/score {run_id} 6 loop attempt")).unwrap();
        match parsed {
            Some(SlashCommand::Score {
                run_id: got,
                score,
                target,
            }) => {
                assert_eq!(got, run_id);
                assert_eq!(score, 6.0);
                assert_eq!(target, "loop attempt");
            }
            _ => panic!("expected score command"),
        }
    }

    #[test]
    fn rejects_out_of_range_score_shortcut() {
        let run_id = uuid::Uuid::new_v4().to_string();
        assert!(parse_slash_command(&format!("/score {run_id} 11")).is_err());
    }

    #[test]
    fn summarizes_trace_observability_counters() {
        let run_id = RunId::new();
        let store = agent_tracing::InMemoryEventStore::new();
        store.append(
            run_id,
            None,
            RunEventKind::ContextBuilt {
                snapshot: serde_json::json!({}),
            },
        );
        store.append(
            run_id,
            None,
            RunEventKind::LlmRequestCompleted {
                tokens_in: 100,
                tokens_out: 25,
                cost_usd: Some(0.001),
                duration_ms: 40,
            },
        );
        store.append(
            run_id,
            None,
            RunEventKind::ToolCallCompleted {
                call_id: "call-1".into(),
                output: serde_json::json!({"ok": true}),
                cost_usd: Some(0.002),
                duration_ms: 5,
            },
        );
        store.append(
            run_id,
            None,
            RunEventKind::MemoryRead {
                backend: "local-v0".into(),
                fragment_ids: vec!["mem-1".into(), "mem-2".into()],
            },
        );
        store.append(
            run_id,
            None,
            RunEventKind::IngestionReferenced {
                artifact_id: "ing-1".into(),
                source: "/tmp/doc.txt".into(),
            },
        );
        store.append(
            run_id,
            None,
            RunEventKind::QualityScored {
                target: "last_answer".into(),
                score: 8.0,
            },
        );
        store.append(
            run_id,
            None,
            RunEventKind::RunCompleted {
                final_output: "done".into(),
                total_cost_usd: Some(0.01),
                total_duration_ms: 99,
            },
        );
        let events = store.events(run_id);

        let summary = summarize_trace(&events, run_id);

        assert_eq!(summary.run_id, run_id);
        assert_eq!(summary.events, 7);
        assert_eq!(summary.context_snapshots, 1);
        assert_eq!(summary.llm_calls, 1);
        assert_eq!(summary.tool_calls, 1);
        assert_eq!(summary.tokens_in, 100);
        assert_eq!(summary.tokens_out, 25);
        assert_eq!(summary.cost_usd, Some(0.01));
        assert_eq!(summary.duration_ms, Some(99));
        assert_eq!(summary.memory_fragments, 2);
        assert_eq!(summary.artifact_refs, 1);
        assert_eq!(summary.quality_scores, 1);
    }
}
