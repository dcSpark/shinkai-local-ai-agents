//! Headless `--print` mode. Reads input (CLI flag or stdin), runs once, and
//! emits either a human-readable transcript on stderr (with the final answer
//! on stdout) or one JSON `RunEvent` per line on stdout (`--json`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::sync::Arc;
use std::time::Instant;

use agent_adapters::{AdapterRegistry, ClawHubProvider, NormalizedPackage, inspect_source};
use agent_api_client::DaemonHttpClient;
use agent_batch::{BatchItemState, BatchPlan};
use agent_bundles::{export_bundle, import_bundle};
use agent_capabilities::{
    CapabilityDraft, CapabilityDraftInput, CapabilityDraftStatus, CapabilityDraftStore,
    CapabilityKind,
};
use agent_compaction::{CompactionRecord, CompactionStore};
use agent_config::{
    AgentConfigFile, AgentPromptRefinementConfig, AgentToolOutputOverrideConfig, ConfigResolver,
    IngestionGuardrailMode, ModelConfig, ModelProviderOptionTarget, ModelRuntimeConfig,
    ProfileGrant, ProfileGrantKind, supported_model_providers,
};
use agent_conversations::{
    ConversationPolicy, ConversationRole, ConversationStore, ConversationTreeNode,
    render_message_range,
};
use agent_core::{
    ContextSnapshot, Harness, HarnessApi, ToolOutputMode, UserInput, VisibilityLevel,
    verify_approval_controller_delegate, verify_configured_approval_signature,
    verify_configured_approval_unlock,
};
use agent_ingest::{
    IngestionArtifact, IngestionFindingReviewDecision, IngestionModelCall, IngestionStore,
    model_vision_source_requirement, supported_backends as supported_ingestion_backends,
};
use agent_llm::{
    AnthropicProvider, FakeProvider, GeminiProvider, LlmProvider, ModelRef, NativeProviderConfig,
    RigProvider,
};
use agent_memory::{
    MemoryAuthor, MemoryRecord, MemoryStore, MemoryTarget,
    supported_backends as supported_memory_backends,
};
use agent_prompts::{PromptStore, is_valid_prompt_name};
use agent_secrets::{
    SecretId, SecretValue, default_secret_store, supported_backends as supported_secret_backends,
};
use agent_skills::{SkillDoc, SkillRegistry};
use agent_storage::StoragePaths;
use agent_tools::{
    ToolId, is_shell_runtime_tool_id, list_generated_artifacts_from_env,
    open_generated_artifact_from_env, show_generated_artifact_from_env,
};
use agent_tracing::{
    EventId, EventStore, RunEvent, RunEventKind, RunId, SqliteEventStore, TraceSummary,
    build_resume_plan, hook_remediation_plan, is_terminal_run_event, latest_event_id,
    summarize_trace, validate_guidance_content, validate_quality_score,
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
        Some(SlashCommand::Agent) => return explain_config(options.agent_id.clone(), json).await,
        Some(SlashCommand::ToolManual { name, input }) => {
            return call_tool(
                name,
                Some(input),
                json,
                options.require_approval,
                options.auto_approve,
            )
            .await;
        }
        Some(SlashCommand::ToolForced { name, prompt }) => {
            return force_tool(name, prompt, json, demo, options).await;
        }
        Some(SlashCommand::Run(prompt)) => {
            text = resolve_saved_prompt_or_literal(&prompt, options.agent_id.as_deref())?
        }
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
    let registry = setup::build_registry(
        options.enable_shell,
        options.enable_subagent,
        options.enable_capability_drafts,
        options.agent_id.as_deref(),
    );
    let harness =
        setup::build_harness_for_agent(provider, events, registry, options.agent_id.as_deref());
    let agent = setup::build_agent(&options);

    let result = harness.run(&agent, UserInput { text }).await?;
    if let Some(conversation_id) = options.conversation_id.as_deref() {
        persist_conversation_turn(
            conversation_id,
            &result.final_output,
            &harness,
            result.run_id,
        )?;
    }
    let run_events = harness.events(result.run_id);

    if json {
        for evt in &run_events {
            println!("{}", serde_json::to_string(&evt)?);
        }
    } else {
        println!("{}", result.final_output);
        eprintln!();
        eprintln!("--- trace ({}) ---", result.run_id.0);
        for evt in &run_events {
            eprintln!("  [{}] {:?}", evt.id.0, evt.kind);
        }
        print_auto_compaction_keep_hint(
            result.run_id,
            &run_events,
            options.conversation_id.as_deref(),
        );
    }

    Ok(())
}

pub async fn preview_context(
    input: Option<String>,
    json: bool,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let text = read_text(input)?;
    let text = if text.trim().starts_with("/run ") {
        resolve_saved_prompt_or_literal(&text, options.agent_id.as_deref())?
    } else {
        text
    };
    let harness = inspection_harness(
        options.enable_shell,
        options.enable_subagent,
        options.enable_capability_drafts,
        options.agent_id.as_deref(),
    );
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

pub async fn explain_config(agent_id: Option<String>, json: bool) -> anyhow::Result<()> {
    let resolved =
        ConfigResolver::from_env().resolve_agent(agent_id.as_deref().unwrap_or("fake-agent"))?;
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

pub async fn secrets_set(
    id: String,
    value: Option<String>,
    label: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    let store = default_secret_store();
    let id = SecretId::new(id)?;
    let value = read_text(value)?;
    let handle = store.set(id.clone(), SecretValue::new(value), label)?;
    let record = store.show(&id)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "handle": handle,
                "record": record
            }))?
        );
    } else {
        println!(
            "stored secret {} version {} ({})",
            handle.id.0, handle.version, record.backend
        );
    }
    Ok(())
}

pub async fn secrets_rotate(id: String, value: Option<String>, json: bool) -> anyhow::Result<()> {
    let store = default_secret_store();
    let id = SecretId::new(id)?;
    let value = read_text(value)?;
    let handle = store.rotate(&id, SecretValue::new(value))?;
    let record = store.show(&id)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "handle": handle,
                "record": record
            }))?
        );
    } else {
        println!(
            "rotated secret {} to version {} ({})",
            handle.id.0, handle.version, record.backend
        );
    }
    Ok(())
}

pub async fn secrets_list(json: bool) -> anyhow::Result<()> {
    let records = default_secret_store().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else if records.is_empty() {
        println!("no secrets stored");
    } else {
        for record in records {
            let label = record
                .label
                .as_deref()
                .map(|value| format!(" label={value:?}"))
                .unwrap_or_default();
            println!(
                "{} version={} backend={} fingerprint={}{}",
                record.id.0,
                record.current_version,
                record.backend,
                record.value_fingerprint,
                label
            );
        }
    }
    Ok(())
}

pub async fn secrets_show(id: String, json: bool) -> anyhow::Result<()> {
    let id = SecretId::new(id)?;
    let record = default_secret_store().show(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&record)?);
    } else {
        let label = record
            .label
            .as_deref()
            .map(|value| format!(" label={value:?}"))
            .unwrap_or_default();
        println!(
            "{} version={} backend={} fingerprint={}{}",
            record.id.0, record.current_version, record.backend, record.value_fingerprint, label
        );
    }
    Ok(())
}

pub async fn secrets_delete(id: String) -> anyhow::Result<()> {
    let id = SecretId::new(id)?;
    let deleted = default_secret_store().delete(&id)?;
    if deleted {
        println!("deleted secret {}", id.0);
    } else {
        println!("secret {} was not stored", id.0);
    }
    Ok(())
}

pub async fn secrets_backends(json: bool) -> anyhow::Result<()> {
    let backends = supported_secret_backends();
    if json {
        println!("{}", serde_json::to_string_pretty(&backends)?);
    } else {
        for backend in backends {
            println!(
                "{} supported={} active={} name={:?}",
                backend.id, backend.supported, backend.active, backend.name
            );
        }
    }
    Ok(())
}

pub async fn capability_propose(
    kind: String,
    name: String,
    body: Option<String>,
    guidance: Option<String>,
    created_by: String,
    json: bool,
) -> anyhow::Result<()> {
    let body = read_text(body)?;
    let draft = CapabilityDraftStore::from_env().propose(CapabilityDraftInput {
        id: None,
        kind: CapabilityKind::parse(&kind)?,
        name,
        body,
        guidance,
        created_by,
        provenance: "cli:capability propose".into(),
    })?;
    print_capability_draft(&draft, json)?;
    Ok(())
}

pub async fn capability_list(json: bool) -> anyhow::Result<()> {
    let drafts = CapabilityDraftStore::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&drafts)?);
    } else if drafts.is_empty() {
        println!("no capability drafts");
    } else {
        for draft in drafts {
            print_capability_draft_line(&draft);
        }
    }
    Ok(())
}

pub async fn capability_show(id: String, json: bool) -> anyhow::Result<()> {
    let draft = CapabilityDraftStore::from_env().show(&id)?;
    print_capability_draft(&draft, json)
}

pub async fn capability_review(
    id: String,
    status: CapabilityDraftStatus,
    json: bool,
) -> anyhow::Result<()> {
    let store = CapabilityDraftStore::from_env();
    if status == CapabilityDraftStatus::Allowed {
        let draft = store.show(&id)?;
        if draft.kind == CapabilityKind::Skill {
            let skill = promote_capability_skill(&draft)?;
            let draft = store.set_status(&id, status)?;
            return print_capability_review_result(
                &draft,
                Some(CapabilityReviewArtifact::PromotedSkill(&skill)),
                json,
            );
        }
        if draft.kind == CapabilityKind::Agent {
            let agent = promote_capability_agent(&draft)?;
            let draft = store.set_status(&id, status)?;
            return print_capability_review_result(
                &draft,
                Some(CapabilityReviewArtifact::PromotedAgent(&agent)),
                json,
            );
        }
        if draft.kind == CapabilityKind::Tool {
            let tool = promote_capability_tool(&draft)?;
            let draft = store.set_status(&id, status)?;
            return print_capability_review_result(
                &draft,
                Some(CapabilityReviewArtifact::PromotedTool(&tool)),
                json,
            );
        }
    } else if status == CapabilityDraftStatus::Rejected {
        let draft = store.show(&id)?;
        if draft.kind == CapabilityKind::Skill {
            let skill = quarantine_capability_skill(&draft)?;
            let draft = store.set_status(&id, status)?;
            return print_capability_review_result(
                &draft,
                skill
                    .as_ref()
                    .map(CapabilityReviewArtifact::QuarantinedSkill),
                json,
            );
        }
        if draft.kind == CapabilityKind::Agent {
            delete_capability_agent(&draft)?;
            let draft = store.set_status(&id, status)?;
            return print_capability_review_result(&draft, None, json);
        }
        if draft.kind == CapabilityKind::Tool {
            let tool = quarantine_capability_tool(&draft)?;
            let draft = store.set_status(&id, status)?;
            return print_capability_review_result(
                &draft,
                tool.as_ref().map(CapabilityReviewArtifact::QuarantinedTool),
                json,
            );
        }
    }

    let draft = store.set_status(&id, status)?;
    print_capability_review_result(&draft, None, json)
}

pub async fn capability_delete(id: String) -> anyhow::Result<()> {
    let deleted = CapabilityDraftStore::from_env().delete(&id)?;
    if deleted {
        println!("deleted capability draft {id}");
    } else {
        println!("capability draft {id} was not stored");
    }
    Ok(())
}

fn print_capability_draft(draft: &CapabilityDraft, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(draft)?);
    } else {
        print_capability_draft_line(draft);
    }
    Ok(())
}

fn print_capability_review_result(
    draft: &CapabilityDraft,
    artifact: Option<CapabilityReviewArtifact<'_>>,
    json: bool,
) -> anyhow::Result<()> {
    if json {
        if let Some(artifact) = artifact {
            let (key, artifact_value) = artifact.json_entry()?;
            let mut result = serde_json::Map::new();
            result.insert("draft".into(), serde_json::to_value(draft)?);
            result.insert(key.into(), artifact_value);
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!("{}", serde_json::to_string_pretty(draft)?);
        }
    } else {
        print_capability_draft_line(draft);
        if let Some(artifact) = artifact {
            println!("{}", artifact.human_line());
        }
    }
    Ok(())
}

enum CapabilityReviewArtifact<'a> {
    PromotedSkill(&'a SkillDoc),
    QuarantinedSkill(&'a SkillDoc),
    PromotedAgent(&'a AgentConfigFile),
    PromotedTool(&'a NormalizedPackage),
    QuarantinedTool(&'a NormalizedPackage),
}

impl CapabilityReviewArtifact<'_> {
    fn json_entry(&self) -> anyhow::Result<(&'static str, serde_json::Value)> {
        match self {
            Self::PromotedSkill(skill) => Ok(("promoted_skill", serde_json::to_value(skill)?)),
            Self::QuarantinedSkill(skill) => {
                Ok(("quarantined_skill", serde_json::to_value(skill)?))
            }
            Self::PromotedAgent(agent) => Ok(("promoted_agent", serde_json::to_value(agent)?)),
            Self::PromotedTool(package) => Ok(("promoted_tool", serde_json::to_value(package)?)),
            Self::QuarantinedTool(package) => {
                Ok(("quarantined_tool", serde_json::to_value(package)?))
            }
        }
    }

    fn human_line(&self) -> String {
        match self {
            Self::PromotedSkill(skill) => format!("promoted skill {}", skill.id),
            Self::QuarantinedSkill(skill) => format!("quarantined promoted skill {}", skill.id),
            Self::PromotedAgent(agent) => format!("promoted agent {}", agent.id),
            Self::PromotedTool(package) => format!("promoted tool package {}", package.id),
            Self::QuarantinedTool(package) => {
                format!("quarantined promoted tool package {}", package.id)
            }
        }
    }
}

fn promote_capability_skill(draft: &CapabilityDraft) -> anyhow::Result<SkillDoc> {
    SkillRegistry::from_env()
        .promote_agent_created_skill(
            &draft.id,
            &draft.name,
            &draft.body,
            &draft.created_by,
            &draft.provenance,
        )
        .map_err(Into::into)
}

fn quarantine_capability_skill(draft: &CapabilityDraft) -> anyhow::Result<Option<SkillDoc>> {
    SkillRegistry::from_env()
        .quarantine_agent_created_skill(&draft.id, &draft.name)
        .map_err(Into::into)
}

fn promote_capability_agent(draft: &CapabilityDraft) -> anyhow::Result<AgentConfigFile> {
    ConfigResolver::from_env()
        .promote_agent_created_config(&draft.id, &draft.name, &draft.body)
        .map_err(Into::into)
}

fn delete_capability_agent(draft: &CapabilityDraft) -> anyhow::Result<bool> {
    ConfigResolver::from_env()
        .delete_agent_created_config(&draft.id, &draft.name)
        .map_err(Into::into)
}

fn promote_capability_tool(draft: &CapabilityDraft) -> anyhow::Result<NormalizedPackage> {
    AdapterRegistry::from_env()
        .promote_agent_created_tool(
            &draft.id,
            &draft.name,
            &draft.body,
            &draft.created_by,
            &draft.provenance,
        )
        .map_err(Into::into)
}

fn quarantine_capability_tool(
    draft: &CapabilityDraft,
) -> anyhow::Result<Option<NormalizedPackage>> {
    AdapterRegistry::from_env()
        .quarantine_agent_created_tool(&draft.id)
        .map_err(Into::into)
}

fn print_capability_draft_line(draft: &CapabilityDraft) {
    println!(
        "{} {:?} status={:?} created_by={} name={:?}",
        draft.id, draft.kind, draft.status, draft.created_by, draft.name
    );
}

pub async fn explain_tools(
    json: bool,
    agent_id: Option<String>,
    enable_shell: bool,
    enable_subagent: bool,
    enable_capability_drafts: bool,
    tool_visibility: Option<VisibilityLevel>,
) -> anyhow::Result<()> {
    let options = setup::RuntimeOptions {
        agent_id,
        enable_shell,
        enable_subagent,
        enable_capability_drafts,
        tool_visibility,
        ..setup::RuntimeOptions::default()
    };
    let harness = inspection_harness(
        enable_shell,
        enable_subagent,
        enable_capability_drafts,
        options.agent_id.as_deref(),
    );
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
    auto_approve: bool,
) -> anyhow::Result<()> {
    let input = read_optional_json(input)?;
    let enable_shell = is_shell_runtime_tool_id(&name);
    let enable_subagent = name == "subagent";
    let events = Arc::new(open_event_store()?);
    let harness = setup::build_harness(
        Arc::new(FakeProvider::echo()),
        events,
        setup::build_registry(enable_shell, enable_subagent, false, None),
    );
    let options = setup::RuntimeOptions {
        enable_shell,
        enable_subagent,
        require_approval,
        auto_approve,
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

pub async fn force_tool(
    name: String,
    prompt: String,
    json: bool,
    demo: Demo,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let tool_id = ToolId::from(name.clone());
    let text = forced_tool_prompt(&name, &prompt);
    let provider = setup::build_provider(demo, &text, &options)?;
    let events = Arc::new(open_event_store()?);
    let registry = setup::build_registry(
        options.enable_shell,
        options.enable_subagent,
        options.enable_capability_drafts,
        options.agent_id.as_deref(),
    );
    let harness =
        setup::build_harness_for_agent(provider, events, registry, options.agent_id.as_deref());
    let mut agent = setup::build_agent(&options);
    if !agent.tool_policy.allowed_tools.is_empty()
        && !agent.tool_policy.allowed_tools.contains(&tool_id)
    {
        anyhow::bail!("tool {name:?} is not allowed by this agent");
    }
    agent.tool_policy.allowed_tools = vec![tool_id.clone()];
    agent.tool_policy.required_tool = Some(tool_id);

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

fn forced_tool_prompt(name: &str, prompt: &str) -> String {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        format!("Call the `{name}` tool with appropriate inputs, then answer from its result.")
    } else {
        format!("Call the `{name}` tool for this request, then answer from its result.\n\n{prompt}")
    }
}

fn inspection_harness(
    enable_shell: bool,
    enable_subagent: bool,
    enable_capability_drafts: bool,
    agent_id: Option<&str>,
) -> Harness {
    setup::build_harness_for_agent(
        Arc::new(FakeProvider::echo()),
        Arc::new(open_event_store().expect("sqlite event store should open")),
        setup::build_registry(
            enable_shell,
            enable_subagent,
            enable_capability_drafts,
            agent_id,
        ),
        agent_id,
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

pub async fn trace_hooks(run_id: String, json: bool) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;
    let plan = hook_remediation_plan(&events);
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else if plan.is_empty() {
        println!("No hook failures found for run {}", run_id.0);
    } else {
        println!("hook remediation for {}", run_id.0);
        for item in plan {
            let state = if item.final_failure {
                "final"
            } else {
                "retrying"
            };
            println!(
                "[{}] {} trigger={} attempt={} {}: {}",
                item.event_id, item.hook_id, item.trigger, item.attempt, state, item.error
            );
            for denial in item.policy_denials {
                println!("  denied: {denial}");
            }
            for action in item.suggested_actions {
                println!("  action: {action}");
            }
        }
    }
    Ok(())
}

pub async fn hooks_list(agent: Option<String>, json: bool) -> anyhow::Result<()> {
    let agent_id = agent.unwrap_or_else(|| "fake-agent".into());
    let resolver = ConfigResolver::from_env();
    let policy = resolver.lifecycle_hook_policy_layers_for_agent(&agent_id)?;
    if json {
        let effective_hooks = policy.effective_disabled_lifecycle_hooks.clone();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "agent_id": policy.agent_id,
                "profile": policy.profile,
                "effective_source": policy.effective_source,
                "disabled_lifecycle_hooks": effective_hooks,
                "effective_disabled_lifecycle_hooks": policy.effective_disabled_lifecycle_hooks,
                "global_disabled_lifecycle_hooks": policy.global_disabled_lifecycle_hooks,
                "profile_disabled_lifecycle_hooks": policy.profile_disabled_lifecycle_hooks,
                "agent_disabled_lifecycle_hooks": policy.agent_disabled_lifecycle_hooks,
            }))?
        );
    } else if policy.effective_disabled_lifecycle_hooks.is_empty() {
        println!("No lifecycle hooks are disabled for agent {agent_id}.");
    } else {
        println!("disabled lifecycle hooks for {agent_id}:");
        for hook in &policy.effective_disabled_lifecycle_hooks {
            println!("- {hook}");
        }
        println!("effective source: {}", policy.effective_source);
        if !policy.agent_disabled_lifecycle_hooks.is_empty() {
            println!(
                "agent scope: {}",
                policy.agent_disabled_lifecycle_hooks.len()
            );
        }
        if !policy.profile_disabled_lifecycle_hooks.is_empty() {
            println!(
                "profile scope: {}",
                policy.profile_disabled_lifecycle_hooks.len()
            );
        }
        if !policy.global_disabled_lifecycle_hooks.is_empty() {
            println!(
                "global scope: {}",
                policy.global_disabled_lifecycle_hooks.len()
            );
        }
    }
    Ok(())
}

pub async fn hooks_available(agent: Option<String>, json: bool) -> anyhow::Result<()> {
    let agent_id = agent.unwrap_or_else(|| "fake-agent".into());
    let policy = ConfigResolver::from_env().lifecycle_hook_policy_layers_for_agent(&agent_id)?;
    let hooks = AdapterRegistry::from_env().lifecycle_hooks()?;
    let records = hooks
        .into_iter()
        .map(|hook| {
            let disabled = policy
                .effective_disabled_lifecycle_hooks
                .iter()
                .any(|id| id == &hook.id);
            serde_json::json!({
                "id": hook.id,
                "triggers": hook.triggers,
                "provenance": hook.provenance,
                "handler": hook.handler,
                "disabled": disabled,
                "disabled_source": disabled.then_some(policy.effective_source.clone()),
            })
        })
        .collect::<Vec<_>>();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "agent_id": policy.agent_id,
                "effective_source": policy.effective_source,
                "hooks": records,
            }))?
        );
    } else if records.is_empty() {
        println!("No allowed lifecycle hooks are installed.");
    } else {
        println!("available lifecycle hooks for {agent_id}:");
        for record in records {
            let id = record["id"].as_str().unwrap_or("<unknown>");
            let triggers = record["triggers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(",");
            let state = if record["disabled"].as_bool().unwrap_or(false) {
                format!(
                    "disabled:{}",
                    record["disabled_source"].as_str().unwrap_or("policy")
                )
            } else {
                "enabled".into()
            };
            println!("- {id} triggers={triggers} {state}");
        }
    }
    Ok(())
}

pub async fn hooks_set_disabled(
    hook_id: String,
    disabled: bool,
    agent: Option<String>,
    confirm: bool,
    json: bool,
) -> anyhow::Result<()> {
    let action = if disabled { "disable" } else { "enable" };
    let scope = if agent.is_some() {
        "agent"
    } else {
        "active_profile"
    };
    if !confirm {
        let profile = StoragePaths::from_env().active_profile_id().to_string();
        let confirm_command = if let Some(agent_id) = agent.as_deref() {
            format!("agent hooks {action} {hook_id} --agent {agent_id} --confirm")
        } else {
            format!("agent hooks {action} {hook_id} --confirm")
        };
        let plan = serde_json::json!({
            "hook_id": hook_id,
            "action": action,
            "scope": scope,
            "profile": profile,
            "agent_id": agent,
            "confirm_command": confirm_command,
        });
        if json {
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            println!("Lifecycle hook policy change needs confirmation.");
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        return Ok(());
    }

    let resolver = ConfigResolver::from_env();
    let hooks = if let Some(agent_id) = agent.as_deref() {
        resolver.set_agent_lifecycle_hook_disabled(agent_id, &hook_id, disabled)?
    } else {
        resolver.set_profile_lifecycle_hook_disabled(&hook_id, disabled)?
    };
    let effective_agent = agent.clone().unwrap_or_else(|| "fake-agent".into());
    let policy = resolver.lifecycle_hook_policy_layers_for_agent(&effective_agent)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "scope": scope,
                "agent_id": agent,
                "disabled_lifecycle_hooks": hooks,
                "effective_agent_id": policy.agent_id,
                "effective_source": policy.effective_source,
                "effective_disabled_lifecycle_hooks": policy.effective_disabled_lifecycle_hooks,
                "global_disabled_lifecycle_hooks": policy.global_disabled_lifecycle_hooks,
                "profile_disabled_lifecycle_hooks": policy.profile_disabled_lifecycle_hooks,
                "agent_disabled_lifecycle_hooks": policy.agent_disabled_lifecycle_hooks,
            }))?
        );
    } else {
        println!(
            "Lifecycle hook policy updated ({scope}): {} disabled hook(s).",
            hooks.len()
        );
        for hook in hooks {
            println!("- {hook}");
        }
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
    if let Some(avg) = summary.quality_score_average {
        let min = summary.quality_score_min.unwrap_or(avg);
        let max = summary.quality_score_max.unwrap_or(avg);
        println!("score rollup: avg {avg:.1}/10 min {min:.1} max {max:.1}");
    }
    println!("memory fragments: {}", summary.memory_fragments);
    println!("artifact refs: {}", summary.artifact_refs);
    println!("hooks: {}", summary.hooks);
    println!("hook failures: {}", summary.hook_failures);
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
    unlock_env: Option<String>,
    signature_env: Option<String>,
    controller_agent: Option<String>,
) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;
    let delegated_controller = if approved {
        verify_approval_controller_delegate(&events, &approval_id, controller_agent.as_deref())?
    } else {
        None
    };
    if approved {
        let unlock = approval_unlock_from_env(unlock_env)?;
        verify_configured_approval_unlock(unlock.as_deref())?;
        let signature = approval_signature_from_env(signature_env)?;
        verify_configured_approval_signature(
            &run_id.0.to_string(),
            &approval_id,
            signature.as_deref(),
        )?;
    }
    store.append(
        run_id,
        None,
        RunEventKind::ApprovalResolved {
            approval_id: approval_id.clone(),
            approved,
            delegated_controller,
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
    unlock_env: Option<String>,
    signature_env: Option<String>,
) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let unlock = approval_unlock_from_env(unlock_env)?;
    verify_configured_approval_unlock(unlock.as_deref())?;
    let signature = approval_signature_from_env(signature_env)?;
    verify_configured_approval_signature(
        &run_id.0.to_string(),
        &approval_id,
        signature.as_deref(),
    )?;
    let store = open_event_store()?;
    let events = store.try_events(run_id)?;
    let approved = events.iter().rev().find_map(|event| match &event.kind {
        RunEventKind::ApprovalResolved {
            approval_id: id,
            approved,
            ..
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
                ..
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

    let registry = setup::build_registry(
        is_shell_runtime_tool_id(&tool_id),
        tool_id == "subagent",
        tool_id == "capability_draft",
        run_agent_id(&events).as_deref(),
    );
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

fn approval_unlock_from_env(unlock_env: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(name) = unlock_env
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };
    std::env::var(name)
        .map(Some)
        .map_err(|_| anyhow::anyhow!("approval unlock env var {name:?} is not set"))
}

fn approval_signature_from_env(signature_env: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(name) = signature_env
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };
    std::env::var(name)
        .map(Some)
        .map_err(|_| anyhow::anyhow!("approval signature env var {name:?} is not set"))
}

fn run_agent_id(events: &[RunEvent]) -> Option<String> {
    events.iter().find_map(|event| match &event.kind {
        RunEventKind::RunStarted { agent_id, .. } => Some(agent_id.clone()),
        _ => None,
    })
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

pub async fn resume(
    run_id: String,
    from_event: Option<u64>,
    demo: Demo,
    json: bool,
) -> anyhow::Result<()> {
    let source_run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let store = open_event_store()?;
    let events = store.try_events(source_run_id)?;
    let plan = build_resume_plan(source_run_id, &events, from_event.map(EventId))?;
    let retained_compaction = stop_compaction_for_run(source_run_id)?;
    let options = setup::RuntimeOptions {
        agent_id: Some(plan.agent_id.clone()),
        include_compact: retained_compaction.clone(),
        ..setup::RuntimeOptions::default()
    };
    let provider = setup::build_provider(demo, &plan.prompt, &options)?;
    let events = Arc::new(open_event_store()?);
    let registry = setup::build_registry(
        options.enable_shell,
        options.enable_subagent,
        options.enable_capability_drafts,
        options.agent_id.as_deref(),
    );
    let harness =
        setup::build_harness_for_agent(provider, events, registry, options.agent_id.as_deref());
    let agent = setup::build_agent(&options);
    let result = harness.run(&agent, UserInput { text: plan.prompt }).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "source_run_id": source_run_id.0,
                "resumed_run_id": result.run_id.0,
                "from_event": plan.selected_event_id.0,
                "retained_compaction": retained_compaction,
                "final_output": result.final_output,
            }))?
        );
    } else {
        println!("{}", result.final_output);
        eprintln!();
        eprintln!(
            "--- resumed {} from event {} as {} ---",
            source_run_id.0, plan.selected_event_id.0, result.run_id.0
        );
        if let Some(compaction) = retained_compaction {
            eprintln!("retained compacted context: {compaction}");
        }
    }

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

pub async fn compact_create(
    input: Option<String>,
    guidance: Option<String>,
    source: Option<String>,
    conversation: Option<String>,
    max_output_tokens: u32,
    json: bool,
) -> anyhow::Result<()> {
    let text = read_text(input)?;
    let record = CompactionStore::from_env().create_from_text_for_conversation(
        &text,
        guidance,
        Some(max_output_tokens),
        source,
        conversation,
    )?;

    print_compaction_record(&record, json)?;
    Ok(())
}

pub async fn compact_conversation(
    id: String,
    from: Option<usize>,
    to: Option<usize>,
    guidance: Option<String>,
    max_output_tokens: u32,
    json: bool,
) -> anyhow::Result<()> {
    let expanded = ConversationStore::from_env().expanded(&id)?;
    if expanded.messages.is_empty() {
        anyhow::bail!("conversation {id} has no messages to compact");
    }
    let start = from.unwrap_or(0);
    let end = to.unwrap_or(expanded.messages.len() - 1);
    if start > end {
        anyhow::bail!("range start {start} is after range end {end}");
    }
    if end >= expanded.messages.len() {
        anyhow::bail!(
            "range end {end} exceeds last expanded message index {}",
            expanded.messages.len() - 1
        );
    }
    let text = expanded.messages[start..=end]
        .iter()
        .enumerate()
        .map(|(offset, message)| {
            format!(
                "{}: {:?}: {}",
                start + offset,
                message.role,
                message.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let record = CompactionStore::from_env().create_from_text_for_conversation(
        &text,
        guidance,
        Some(max_output_tokens),
        Some(format!("conversation:{id}:{start}:{end}")),
        Some(id),
    )?;

    print_compaction_record(&record, json)?;
    Ok(())
}

pub async fn compact_keep(
    input: Option<String>,
    guidance: Option<String>,
    source: Option<String>,
    conversation: Option<String>,
    max_output_tokens: Option<u32>,
    json: bool,
) -> anyhow::Result<()> {
    let content = read_text(input)?;
    let record = CompactionStore::from_env().keep_compacted_context(
        &content,
        guidance,
        max_output_tokens,
        source,
        conversation,
    )?;

    print_compaction_record(&record, json)?;
    Ok(())
}

pub async fn compact_keep_run(
    run_id: String,
    conversation: Option<String>,
    guidance: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let record = keep_auto_compaction_for_run(run_id, conversation, guidance)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "run_id": run_id.0,
                "record": record,
            }))?
        );
    } else if let Some(record) = record {
        print_compaction_record(&record, false)?;
    } else {
        println!("No auto-compacted context found for run {}", run_id.0);
    }
    Ok(())
}

pub async fn compact_list(json: bool) -> anyhow::Result<()> {
    let records = CompactionStore::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else if records.is_empty() {
        println!("No compacted-context artifacts found.");
    } else {
        for record in records {
            println!(
                "{} source={} max_output_tokens={} created_at={}",
                record.id, record.source, record.max_output_tokens, record.created_at
            );
        }
    }
    Ok(())
}

pub async fn compact_show(id: String, json: bool) -> anyhow::Result<()> {
    let record = CompactionStore::from_env().show(&id)?;
    print_compaction_record(&record, json)?;
    Ok(())
}

pub async fn compact_export(id: String, path: String, json: bool) -> anyhow::Result<()> {
    let record = CompactionStore::from_env().export_record(&id, &path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": path,
                "record": record
            }))?
        );
    } else {
        println!(
            "exported compacted-context artifact {} to {}",
            record.id, path
        );
    }
    Ok(())
}

pub async fn compact_import(path: String, json: bool) -> anyhow::Result<()> {
    let record = CompactionStore::from_env().import_record(&path)?;
    print_compaction_record(&record, json)?;
    Ok(())
}

fn print_compaction_record(record: &CompactionRecord, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(&record)?);
    } else {
        println!("id: {}", record.id);
        println!("source: {}", record.source);
        if let Some(conversation_id) = &record.conversation_id {
            println!("conversation: {conversation_id}");
        }
        println!("created_at: {}", record.created_at);
        println!("original_input_hash: {}", record.original_input_hash);
        if let Some(guidance) = &record.guidance {
            println!("guidance: {guidance}");
        }
        println!();
        println!("{}", record.content);
    }
    Ok(())
}

pub async fn compact_rm(id: String) -> anyhow::Result<()> {
    CompactionStore::from_env().remove(&id)?;
    println!("removed compacted-context artifact {id}");
    Ok(())
}

pub async fn conversation_create(
    title: Option<String>,
    agent: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    let doc = ConversationStore::from_env().create(title, agent)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        println!("{}", doc.id);
        println!("title: {}", doc.title);
        println!("agent: {}", doc.agent_id);
    }
    Ok(())
}

pub async fn conversation_list(json: bool) -> anyhow::Result<()> {
    let docs = ConversationStore::from_env().list()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&docs)?);
    } else if docs.is_empty() {
        println!("No conversations found.");
    } else {
        for doc in docs {
            let parent = doc
                .parent
                .as_ref()
                .map(|parent| {
                    format!(
                        " parent={}@{}",
                        parent.conversation_id, parent.parent_message_count
                    )
                })
                .unwrap_or_default();
            println!(
                "{} title={:?} agent={} own_messages={}{}",
                doc.id,
                doc.title,
                doc.agent_id,
                doc.messages.len(),
                parent
            );
        }
    }
    Ok(())
}

pub async fn conversation_add_message(
    id: String,
    role: ConversationRole,
    content: String,
) -> anyhow::Result<()> {
    let doc = ConversationStore::from_env().append_message(&id, role, &content)?;
    println!(
        "conversation {} now has {} own messages",
        doc.id,
        doc.messages.len()
    );
    Ok(())
}

pub async fn conversation_branch(
    id: String,
    at: usize,
    title: Option<String>,
    reason: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    let doc = ConversationStore::from_env().branch(&id, at, title, reason)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        println!("{}", doc.id);
        println!(
            "parent: {}@{}",
            id,
            doc.parent
                .as_ref()
                .map(|parent| parent.parent_message_count)
                .unwrap_or(at)
        );
        if let Some(reason) = doc.branch_reason {
            println!("reason: {reason}");
        }
    }
    Ok(())
}

pub async fn conversation_show(id: String, json: bool) -> anyhow::Result<()> {
    let expanded = ConversationStore::from_env().expanded(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&expanded)?);
    } else {
        println!("id: {}", expanded.conversation.id);
        println!("title: {}", expanded.conversation.title);
        println!("agent: {}", expanded.conversation.agent_id);
        if let Some(parent) = &expanded.conversation.parent {
            println!(
                "parent: {}@{}",
                parent.conversation_id, parent.parent_message_count
            );
        }
        if let Some(reason) = &expanded.conversation.branch_reason {
            println!("reason: {reason}");
        }
        println!("expanded messages: {}", expanded.messages.len());
        for (idx, message) in expanded.messages.iter().enumerate() {
            println!("{idx}: {:?}: {}", message.role, message.content);
        }
    }
    Ok(())
}

#[derive(Default)]
pub struct ConversationPolicyOptions {
    pub load_memory: Option<bool>,
    pub clear_load_memory: bool,
    pub generate_memory: Option<bool>,
    pub clear_generate_memory: bool,
    pub allowed_tool_categories: Vec<String>,
    pub clear_allowed_tool_categories: bool,
    pub allowed_skill_categories: Vec<String>,
    pub clear_allowed_skill_categories: bool,
    pub max_tokens_before_compaction: Option<u32>,
    pub clear_max_tokens_before_compaction: bool,
    pub max_compaction_output_tokens: Option<u32>,
    pub clear_max_compaction_output_tokens: bool,
    pub compaction_guidance: Option<String>,
    pub clear_compaction_guidance: bool,
    pub clear: bool,
    pub json: bool,
}

impl ConversationPolicyOptions {
    fn changes_policy(&self) -> bool {
        self.clear
            || self.load_memory.is_some()
            || self.clear_load_memory
            || self.generate_memory.is_some()
            || self.clear_generate_memory
            || !self.allowed_tool_categories.is_empty()
            || self.clear_allowed_tool_categories
            || !self.allowed_skill_categories.is_empty()
            || self.clear_allowed_skill_categories
            || self.max_tokens_before_compaction.is_some()
            || self.clear_max_tokens_before_compaction
            || self.max_compaction_output_tokens.is_some()
            || self.clear_max_compaction_output_tokens
            || self.compaction_guidance.is_some()
            || self.clear_compaction_guidance
    }
}

pub async fn conversation_policy(
    id: String,
    options: ConversationPolicyOptions,
) -> anyhow::Result<()> {
    let store = ConversationStore::from_env();
    let mut doc = store.show(&id)?;
    if options.changes_policy() {
        let mut policy = if options.clear {
            ConversationPolicy::default()
        } else {
            doc.policy.clone()
        };
        if options.clear_load_memory {
            policy.load_memory = None;
        }
        if let Some(load_memory) = options.load_memory {
            policy.load_memory = Some(load_memory);
        }
        if options.clear_generate_memory {
            policy.generate_memory = None;
        }
        if let Some(generate_memory) = options.generate_memory {
            policy.generate_memory = Some(generate_memory);
        }
        if options.clear_allowed_tool_categories {
            policy.allowed_tool_categories = None;
        }
        if !options.allowed_tool_categories.is_empty() {
            policy.allowed_tool_categories = Some(options.allowed_tool_categories);
        }
        if options.clear_allowed_skill_categories {
            policy.allowed_skill_categories = None;
        }
        if !options.allowed_skill_categories.is_empty() {
            policy.allowed_skill_categories = Some(options.allowed_skill_categories);
        }
        if options.clear_max_tokens_before_compaction {
            policy.max_tokens_before_compaction = None;
        }
        if let Some(max_tokens_before_compaction) = options.max_tokens_before_compaction {
            policy.max_tokens_before_compaction = Some(max_tokens_before_compaction);
        }
        if options.clear_max_compaction_output_tokens {
            policy.max_compaction_output_tokens = None;
        }
        if let Some(max_compaction_output_tokens) = options.max_compaction_output_tokens {
            policy.max_compaction_output_tokens = Some(max_compaction_output_tokens);
        }
        if options.clear_compaction_guidance {
            policy.compaction_guidance = None;
        }
        if let Some(compaction_guidance) = options.compaction_guidance {
            policy.compaction_guidance = Some(compaction_guidance);
        }
        doc = store.set_policy(&id, policy)?;
    }

    if options.json {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        println!("conversation: {}", doc.id);
        println!("policy: {}", conversation_policy_summary(&doc.policy));
    }
    Ok(())
}

pub async fn conversation_tree(json: bool) -> anyhow::Result<()> {
    let tree = ConversationStore::from_env().tree()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&tree)?);
    } else if tree.is_empty() {
        println!("No conversations found.");
    } else {
        for node in &tree {
            print_conversation_tree_node(node, 0);
        }
    }
    Ok(())
}

pub async fn conversation_recover(id: String, json: bool) -> anyhow::Result<()> {
    let plan = conversation_recovery_plan_value(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!(
            "conversation {}",
            plan["conversation_id"].as_str().unwrap_or(&id)
        );
        println!(
            "expanded messages: {}",
            plan["expanded_message_count"].as_u64().unwrap_or_default()
        );
        println!(
            "linked compactions: {}",
            plan["linked_compactions"]
                .as_array()
                .map(Vec::len)
                .unwrap_or_default()
        );
        println!(
            "linked memories: {}",
            plan["linked_memories"]
                .as_array()
                .map(Vec::len)
                .unwrap_or_default()
        );
        if let Some(compaction) = plan["suggested_run"]["include_compact"].as_str() {
            println!("suggested include compact: {compaction}");
        }
        if plan["suggested_run"]["load_memory"].as_bool() == Some(true) {
            println!("suggested: run with --load-memory");
        }
    }
    Ok(())
}

pub(crate) fn conversation_recovery_plan_value(id: &str) -> anyhow::Result<serde_json::Value> {
    let conversation_store = ConversationStore::from_env();
    let expanded = conversation_store.expanded(id)?;
    let mut compactions = CompactionStore::from_env()
        .list()?
        .into_iter()
        .filter(|record| record.conversation_id.as_deref() == Some(id))
        .collect::<Vec<_>>();
    compactions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let mut memories = MemoryStore::from_env()
        .list()?
        .into_iter()
        .filter(|record| record.source_conversation_id.as_deref() == Some(id))
        .collect::<Vec<_>>();
    memories.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let latest_compaction = compactions.first();

    Ok(serde_json::json!({
        "conversation_id": id,
        "title": expanded.conversation.title,
        "agent_id": expanded.conversation.agent_id,
        "own_message_count": expanded.conversation.messages.len(),
        "expanded_message_count": expanded.messages.len(),
        "linked_compactions": compactions.iter().map(compaction_recovery_summary).collect::<Vec<_>>(),
        "linked_memories": memories.iter().map(memory_recovery_summary).collect::<Vec<_>>(),
        "suggested_run": {
            "conversation_id": id,
            "include_compact": latest_compaction.map(|record| record.id.clone()),
            "load_memory": !memories.is_empty(),
            "compacted_context": latest_compaction.map(|record| record.content.clone()),
        }
    }))
}

fn compaction_recovery_summary(record: &CompactionRecord) -> serde_json::Value {
    serde_json::json!({
        "id": record.id,
        "source": record.source,
        "guidance": record.guidance,
        "max_output_tokens": record.max_output_tokens,
        "original_input_excerpt": record.original_input_excerpt,
        "content_preview": preview_for_recovery(&record.content, 240),
        "created_at": record.created_at,
    })
}

fn memory_recovery_summary(record: &MemoryRecord) -> serde_json::Value {
    serde_json::json!({
        "id": record.id,
        "target": record.target,
        "author": record.author,
        "source_range": record.source_range,
        "generating_model": record.generating_model,
        "content_preview": preview_for_recovery(&record.content, 240),
        "updated_at": record.updated_at,
    })
}

fn preview_for_recovery(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    compact
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>()
        + "..."
}

#[derive(Debug, Clone)]
pub struct ConversationDeleteOptions {
    pub recursive: bool,
    pub compact_first: bool,
    pub compact_guidance: Option<String>,
    pub compact_max_output_tokens: u32,
    pub memory_first: bool,
    pub memory_user: bool,
}

pub async fn conversation_delete(
    id: String,
    options: ConversationDeleteOptions,
) -> anyhow::Result<()> {
    let store = ConversationStore::from_env();
    let planned = store.deletion_plan(&[id], options.recursive)?;
    let preserved = preserve_conversation_artifacts(&store, &planned, &options)?;
    let deleted = store.delete_many(&planned, false)?;
    print_conversation_deletion(deleted, preserved)?;
    Ok(())
}

pub async fn conversation_delete_range(id: String, from: usize, to: usize) -> anyhow::Result<()> {
    let before = ConversationStore::from_env().expanded(&id)?.messages.len();
    let doc = ConversationStore::from_env().delete_message_range(&id, from, to)?;
    let after = ConversationStore::from_env().expanded(&id)?.messages.len();
    println!(
        "deleted {} message(s) from {id}",
        before.saturating_sub(after)
    );
    println!("own messages remaining: {}", doc.messages.len());
    Ok(())
}

pub async fn conversation_delete_agent(
    agent: String,
    options: ConversationDeleteOptions,
) -> anyhow::Result<()> {
    let store = ConversationStore::from_env();
    let planned = store.deletion_plan_by_agent(&agent, options.recursive)?;
    let preserved = preserve_conversation_artifacts(&store, &planned, &options)?;
    let deleted = store.delete_many(&planned, false)?;
    print_conversation_deletion(deleted, preserved)?;
    Ok(())
}

#[derive(Debug, Default)]
struct PreservedConversationArtifacts {
    compaction_ids: Vec<String>,
    memory_ids: Vec<String>,
}

fn print_conversation_deletion(
    deleted: Vec<String>,
    preserved: PreservedConversationArtifacts,
) -> anyhow::Result<()> {
    let cleanup = cleanup_conversation_side_data(&deleted, &preserved)?;
    println!("deleted {} conversation branch(es)", deleted.len());
    for id in &deleted {
        println!("{id}");
    }
    if !preserved.compaction_ids.is_empty() {
        println!(
            "preserved {} pre-delete compaction artifact(s)",
            preserved.compaction_ids.len()
        );
        for id in &preserved.compaction_ids {
            println!("{id}");
        }
    }
    if !preserved.memory_ids.is_empty() {
        println!(
            "preserved {} pre-delete memory record(s)",
            preserved.memory_ids.len()
        );
        for id in &preserved.memory_ids {
            println!("{id}");
        }
    }
    println!(
        "deleted {} linked compaction artifact(s)",
        cleanup.compactions
    );
    println!("deleted {} linked memory record(s)", cleanup.memories);
    Ok(())
}

#[derive(Debug, Default)]
struct ConversationDeletionCleanup {
    compactions: usize,
    memories: usize,
}

fn cleanup_conversation_side_data(
    deleted: &[String],
    preserved: &PreservedConversationArtifacts,
) -> anyhow::Result<ConversationDeletionCleanup> {
    let deleted = deleted.iter().collect::<BTreeSet<_>>();
    let preserved_compactions = preserved.compaction_ids.iter().collect::<BTreeSet<_>>();
    let preserved_memories = preserved.memory_ids.iter().collect::<BTreeSet<_>>();

    let compaction_store = CompactionStore::from_env();
    let mut compactions = 0usize;
    for record in compaction_store.list()? {
        let linked_to_deleted = record
            .conversation_id
            .as_ref()
            .is_some_and(|id| deleted.contains(id));
        if linked_to_deleted && !preserved_compactions.contains(&record.id) {
            compaction_store.remove(&record.id)?;
            compactions += 1;
        }
    }

    let memory_store = MemoryStore::from_env();
    let mut memories = 0usize;
    for record in memory_store.list()? {
        let linked_to_deleted = record
            .source_conversation_id
            .as_ref()
            .is_some_and(|id| deleted.contains(id));
        if linked_to_deleted && !preserved_memories.contains(&record.id) {
            memory_store.delete(&record.id)?;
            memories += 1;
        }
    }

    Ok(ConversationDeletionCleanup {
        compactions,
        memories,
    })
}

fn preserve_conversation_artifacts(
    store: &ConversationStore,
    planned: &[String],
    options: &ConversationDeleteOptions,
) -> anyhow::Result<PreservedConversationArtifacts> {
    let mut preserved = PreservedConversationArtifacts::default();
    if !options.compact_first && !options.memory_first {
        return Ok(preserved);
    }
    let compaction_store = CompactionStore::from_env();
    let memory_store = MemoryStore::from_env();
    let memory_target = if options.memory_user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };

    for id in planned {
        let expanded = store.expanded(id)?;
        if expanded.messages.is_empty() {
            continue;
        }
        let text = render_conversation_messages(&expanded.messages);
        if options.compact_first {
            let record = compaction_store.create_from_text_for_conversation(
                &text,
                options.compact_guidance.clone(),
                Some(options.compact_max_output_tokens),
                Some(format!("pre-delete-conversation:{id}")),
                Some(id.clone()),
            )?;
            preserved.compaction_ids.push(record.id);
        }
        if options.memory_first {
            let records = memory_store.generate_from_conversation_text(
                memory_target,
                &text,
                Some(format!("pre-delete-conversation:{id}")),
                Some(id.clone()),
            )?;
            for record in &records {
                record_memory_written(record, "generated")?;
            }
            preserved
                .memory_ids
                .extend(records.into_iter().map(|record| record.id));
        }
    }

    Ok(preserved)
}

fn render_conversation_messages(messages: &[agent_conversations::ConversationMessage]) -> String {
    messages
        .iter()
        .enumerate()
        .map(|(idx, message)| format!("{idx}: {:?}: {}", message.role, message.content))
        .collect::<Vec<_>>()
        .join("\n")
}

fn conversation_policy_summary(policy: &ConversationPolicy) -> String {
    let mut parts = Vec::new();
    if let Some(load_memory) = policy.load_memory {
        parts.push(format!("load_memory={load_memory}"));
    }
    if let Some(generate_memory) = policy.generate_memory {
        parts.push(format!("generate_memory={generate_memory}"));
    }
    if let Some(categories) = &policy.allowed_tool_categories {
        parts.push(format!("allowed_tool_categories={categories:?}"));
    }
    if let Some(categories) = &policy.allowed_skill_categories {
        parts.push(format!("allowed_skill_categories={categories:?}"));
    }
    if let Some(max_tokens_before_compaction) = policy.max_tokens_before_compaction {
        parts.push(format!(
            "max_tokens_before_compaction={max_tokens_before_compaction}"
        ));
    }
    if let Some(max_compaction_output_tokens) = policy.max_compaction_output_tokens {
        parts.push(format!(
            "max_compaction_output_tokens={max_compaction_output_tokens}"
        ));
    }
    if let Some(guidance) = &policy.compaction_guidance {
        parts.push(format!("compaction_guidance={guidance:?}"));
    }
    if parts.is_empty() {
        "default".into()
    } else {
        parts.join(", ")
    }
}

fn persist_conversation_turn(
    conversation_id: &str,
    final_output: &str,
    harness: &Harness,
    run_id: RunId,
) -> anyhow::Result<()> {
    let input = harness
        .events(run_id)
        .into_iter()
        .find_map(|event| match event.kind {
            RunEventKind::RunStarted { input, .. } => Some(input),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no RunStarted event"))?;
    let store = ConversationStore::from_env();
    store.append_message(conversation_id, ConversationRole::User, &input)?;
    store.append_message(conversation_id, ConversationRole::Assistant, final_output)?;
    Ok(())
}

fn print_conversation_tree_node(node: &ConversationTreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let reason = node
        .branch_reason
        .as_ref()
        .map(|reason| format!(" reason={reason:?}"))
        .unwrap_or_default();
    println!(
        "{}{} title={:?} expanded_messages={}{}",
        indent, node.id, node.title, node.expanded_message_count, reason
    );
    for child in &node.children {
        print_conversation_tree_node(child, depth + 1);
    }
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
        let harness = setup::build_harness(
            provider,
            store.clone(),
            setup::build_registry(
                options.enable_shell,
                options.enable_subagent,
                options.enable_capability_drafts,
                options.agent_id.as_deref(),
            ),
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

pub async fn memory_create(
    content: String,
    user: bool,
    conversation: Option<String>,
    topics: Vec<String>,
) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let record = MemoryStore::from_env().create_for_conversation_with_topics(
        target,
        &content,
        MemoryAuthor::Human,
        None,
        conversation,
        topics,
    )?;
    record_memory_written(&record, "created")?;
    println!("{}", serde_json::to_string_pretty(&record)?);
    Ok(())
}

pub async fn memory_generate(
    text: String,
    user: bool,
    range: Option<String>,
    conversation: Option<String>,
    topics: Vec<String>,
) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let records = MemoryStore::from_env().generate_from_conversation_text_with_topics(
        target,
        &text,
        range,
        conversation,
        topics,
    )?;
    for record in &records {
        record_memory_written(record, "generated")?;
    }
    println!("{}", serde_json::to_string_pretty(&records)?);
    Ok(())
}

pub async fn memory_generate_conversation(
    id: String,
    from: Option<usize>,
    to: Option<usize>,
    user: bool,
    topics: Vec<String>,
) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let expanded = ConversationStore::from_env().expanded(&id)?;
    let rendered = render_message_range(&expanded.messages, from, to)?;
    let records = MemoryStore::from_env().generate_from_conversation_text_with_topics(
        target,
        &rendered.text,
        Some(rendered.source_range),
        Some(id),
        topics,
    )?;
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

pub async fn memory_backends(json: bool) -> anyhow::Result<()> {
    let backends = supported_memory_backends();
    if json {
        println!("{}", serde_json::to_string_pretty(&backends)?);
    } else {
        for backend in backends {
            println!(
                "{} name={:?} generation={} rollback={} storage={}",
                backend.id,
                backend.name,
                backend.supports_generation,
                backend.supports_rollback,
                backend.storage
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

pub async fn memory_export(path: String, user: bool, json: bool) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let records = MemoryStore::from_env().export_target(target, &path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": path,
                "target": target,
                "records": records
            }))?
        );
    } else {
        println!("exported {} memory record(s) to {}", records.len(), path);
    }
    Ok(())
}

pub async fn memory_import(path: String, user: bool, json: bool) -> anyhow::Result<()> {
    let target = if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let records = MemoryStore::from_env().import_file(&path, Some(target))?;
    for record in &records {
        record_memory_written(record, "imported")?;
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else {
        println!("imported {} memory record(s)", records.len());
    }
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

pub async fn skill_export(id: String, path: String, json: bool) -> anyhow::Result<()> {
    let doc = SkillRegistry::from_env().export(&id, &path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": path,
                "skill": doc
            }))?
        );
    } else {
        println!("exported skill {} to {}", doc.id, path);
    }
    Ok(())
}

pub async fn skill_import_doc(path: String, json: bool) -> anyhow::Result<()> {
    let doc = SkillRegistry::from_env().import_doc(&path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    } else {
        println!("imported quarantined skill {}", doc.id);
    }
    Ok(())
}

pub async fn prompt_save(name: String, text: String, agent: Option<String>) -> anyhow::Result<()> {
    let prompt = PromptStore::from_env().save_scoped(agent.as_deref(), &name, &text)?;
    println!("{}", serde_json::to_string_pretty(&prompt)?);
    Ok(())
}

pub async fn prompt_list(json: bool, agent: Option<String>) -> anyhow::Result<()> {
    let prompts = PromptStore::from_env().list_scoped(agent.as_deref())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&prompts)?);
    } else {
        for prompt in prompts {
            let first_line = prompt.body.lines().next().unwrap_or_default();
            if let Some(agent_id) = &prompt.agent_id {
                println!("{} agent={} {}", prompt.name, agent_id, first_line);
            } else {
                println!("{} {}", prompt.name, first_line);
            }
        }
    }
    Ok(())
}

pub async fn prompt_show(name: String, json: bool, agent: Option<String>) -> anyhow::Result<()> {
    let Some(prompt) = PromptStore::from_env().get_scoped(agent.as_deref(), &name)? else {
        anyhow::bail!("saved prompt {name:?} not found");
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&prompt)?);
    } else {
        println!("{}", prompt.body);
    }
    Ok(())
}

pub async fn prompt_delete(name: String, agent: Option<String>) -> anyhow::Result<()> {
    if PromptStore::from_env().delete_scoped(agent.as_deref(), &name)? {
        if let Some(agent) = agent {
            println!("deleted prompt {name} for agent {agent}");
        } else {
            println!("deleted prompt {name}");
        }
    } else {
        println!("prompt {name} not found");
    }
    Ok(())
}

pub async fn agent_list(json: bool) -> anyhow::Result<()> {
    let agents = ConfigResolver::from_env().list_agent_configs()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&agents)?);
    } else {
        for agent in agents {
            println!(
                "{} name={:?} path={}",
                agent.id,
                agent.name,
                agent.path.display()
            );
        }
    }
    Ok(())
}

pub async fn agent_show(id: String, json: bool) -> anyhow::Result<()> {
    let Some(agent) = ConfigResolver::from_env().show_agent_config(&id)? else {
        anyhow::bail!("agent {id:?} not found");
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&agent)?);
    } else {
        println!("{}", toml::to_string_pretty(&agent)?);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn agent_save(
    id: String,
    name: Option<String>,
    system_prompt: String,
    model: Option<String>,
    max_tool_calls: Option<u32>,
    max_tokens_before_compaction: Option<u32>,
    max_compaction_output_tokens: Option<u32>,
    compaction_guidance: Option<String>,
    max_subagent_depth: Option<u32>,
    max_recursion_depth: Option<u32>,
    allowed_tools: Vec<String>,
    allowed_tool_categories: Vec<String>,
    approval_controller_agent: Option<String>,
    approval_controller_allowed_tools: Vec<String>,
    approval_controller_allowed_tool_categories: Vec<String>,
    allowed_skill_categories: Vec<String>,
    tool_output_mode: Option<ToolOutputMode>,
    tool_output_interpretation_model: Option<String>,
    tool_output_overrides: Vec<String>,
    tool_interpretation_model_overrides: Vec<String>,
    tool_guidance_overrides: Vec<String>,
    tool_visibility: Option<VisibilityLevel>,
    load_memory: bool,
    load_skills: bool,
    ingestion_guardrail: Option<IngestionGuardrailMode>,
    ingestion_guardrail_model: Option<String>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    refinement_instructions: Option<String>,
    refinement_model: Option<String>,
    refinement_aware: bool,
) -> anyhow::Result<()> {
    let agent = agent_config_from_parts(
        id,
        name,
        system_prompt,
        model,
        max_tool_calls,
        max_tokens_before_compaction,
        max_compaction_output_tokens,
        compaction_guidance,
        max_subagent_depth,
        max_recursion_depth,
        allowed_tools,
        allowed_tool_categories,
        approval_controller_agent,
        approval_controller_allowed_tools,
        approval_controller_allowed_tool_categories,
        allowed_skill_categories,
        tool_output_mode,
        tool_output_interpretation_model,
        tool_output_overrides,
        tool_interpretation_model_overrides,
        tool_guidance_overrides,
        tool_visibility,
        load_memory,
        load_skills,
        ingestion_guardrail,
        ingestion_guardrail_model,
        input_cost_per_million,
        output_cost_per_million,
        refinement_instructions,
        refinement_model,
        refinement_aware,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&ConfigResolver::from_env().save_agent_config(&agent)?)?
    );
    Ok(())
}

pub async fn agent_delete(id: String) -> anyhow::Result<()> {
    if ConfigResolver::from_env().delete_agent_config(&id)? {
        println!("deleted agent {id}");
    } else {
        println!("agent {id} not found");
    }
    Ok(())
}

pub async fn agent_export(id: String, path: String, json: bool) -> anyhow::Result<()> {
    let agent = ConfigResolver::from_env().export_agent_config(&id, &path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": path,
                "agent": agent
            }))?
        );
    } else {
        println!("exported agent {} to {}", agent.id, path);
    }
    Ok(())
}

pub async fn agent_import(path: String, json: bool) -> anyhow::Result<()> {
    let agent = ConfigResolver::from_env().import_agent_config(&path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&agent)?);
    } else {
        println!("imported agent {}", agent.id);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn agent_config_from_parts(
    id: String,
    name: Option<String>,
    system_prompt: String,
    model: Option<String>,
    max_tool_calls: Option<u32>,
    max_tokens_before_compaction: Option<u32>,
    max_compaction_output_tokens: Option<u32>,
    compaction_guidance: Option<String>,
    max_subagent_depth: Option<u32>,
    max_recursion_depth: Option<u32>,
    allowed_tools: Vec<String>,
    allowed_tool_categories: Vec<String>,
    approval_controller_agent: Option<String>,
    approval_controller_allowed_tools: Vec<String>,
    approval_controller_allowed_tool_categories: Vec<String>,
    allowed_skill_categories: Vec<String>,
    tool_output_mode: Option<ToolOutputMode>,
    tool_output_interpretation_model: Option<String>,
    tool_output_overrides: Vec<String>,
    tool_interpretation_model_overrides: Vec<String>,
    tool_guidance_overrides: Vec<String>,
    tool_visibility: Option<VisibilityLevel>,
    load_memory: bool,
    load_skills: bool,
    ingestion_guardrail: Option<IngestionGuardrailMode>,
    ingestion_guardrail_model: Option<String>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    refinement_instructions: Option<String>,
    refinement_model: Option<String>,
    refinement_aware: bool,
) -> anyhow::Result<AgentConfigFile> {
    let prompt_refinement =
        prompt_refinement_config(refinement_instructions, refinement_model, refinement_aware)?;
    let tool_overrides = parse_tool_output_overrides(
        tool_output_overrides,
        tool_interpretation_model_overrides,
        tool_guidance_overrides,
    )?;
    let approval_controller_allowed_tool_categories =
        (!approval_controller_allowed_tool_categories.is_empty())
            .then_some(approval_controller_allowed_tool_categories);
    Ok(AgentConfigFile {
        id: id.clone(),
        name: name.unwrap_or(id),
        system_prompt,
        prompt_refinement,
        prompt_refinements: Vec::new(),
        tool_overrides,
        voice: None,
        model,
        max_tool_calls,
        max_subagent_depth,
        max_recursion_depth,
        allowed_tools: (!allowed_tools.is_empty()).then_some(allowed_tools),
        allowed_tool_categories: (!allowed_tool_categories.is_empty())
            .then_some(allowed_tool_categories),
        approval_controller_agent: clean_optional_string(approval_controller_agent),
        approval_controller_allowed_tools: (!approval_controller_allowed_tools.is_empty())
            .then_some(approval_controller_allowed_tools),
        approval_controller_allowed_tool_categories,
        allowed_skill_categories: (!allowed_skill_categories.is_empty())
            .then_some(allowed_skill_categories),
        disabled_lifecycle_hooks: None,
        tool_output_mode,
        tool_output_interpretation_model,
        tool_visibility,
        load_memory: load_memory.then_some(true),
        load_skills: load_skills.then_some(true),
        max_tokens_before_compaction,
        max_compaction_output_tokens,
        compaction_guidance: clean_optional_string(compaction_guidance),
        ingestion_guardrail,
        ingestion_guardrail_model,
        input_cost_per_million,
        output_cost_per_million,
    })
}

fn prompt_refinement_config(
    refinement_instructions: Option<String>,
    refinement_model: Option<String>,
    refinement_aware: bool,
) -> anyhow::Result<Option<AgentPromptRefinementConfig>> {
    let Some(instructions) = refinement_instructions else {
        if refinement_model.is_some() || refinement_aware {
            anyhow::bail!(
                "--refinement-instructions is required when setting prompt refinement options"
            );
        }
        return Ok(None);
    };
    if instructions.trim().is_empty() {
        anyhow::bail!("--refinement-instructions cannot be empty");
    }
    Ok(Some(AgentPromptRefinementConfig {
        id: None,
        when: None,
        instructions,
        model: refinement_model,
        agent_awareness: refinement_aware,
    }))
}

fn parse_tool_output_overrides(
    mode_specs: Vec<String>,
    model_specs: Vec<String>,
    guidance_specs: Vec<String>,
) -> anyhow::Result<Vec<AgentToolOutputOverrideConfig>> {
    let mut overrides = BTreeMap::<String, AgentToolOutputOverrideConfig>::new();
    for spec in mode_specs {
        let (tool_id, mode) = parse_tool_output_mode_override(&spec)?;
        overrides
            .entry(tool_id.clone())
            .or_insert_with(|| AgentToolOutputOverrideConfig {
                id: tool_id,
                output_mode: None,
                output_interpretation_model: None,
                output_interpretation_guidance: None,
            })
            .output_mode = Some(mode);
    }
    for spec in model_specs {
        let (tool_id, model) = parse_tool_interpretation_model_override(&spec)?;
        overrides
            .entry(tool_id.clone())
            .or_insert_with(|| AgentToolOutputOverrideConfig {
                id: tool_id,
                output_mode: None,
                output_interpretation_model: None,
                output_interpretation_guidance: None,
            })
            .output_interpretation_model = Some(model);
    }
    for spec in guidance_specs {
        let (tool_id, guidance) = parse_tool_guidance_override(&spec)?;
        overrides
            .entry(tool_id.clone())
            .or_insert_with(|| AgentToolOutputOverrideConfig {
                id: tool_id,
                output_mode: None,
                output_interpretation_model: None,
                output_interpretation_guidance: None,
            })
            .output_interpretation_guidance = Some(guidance);
    }
    Ok(overrides.into_values().collect())
}

fn parse_tool_output_mode_override(spec: &str) -> anyhow::Result<(String, ToolOutputMode)> {
    let (tool_id, value) = parse_tool_override_pair(spec, "--tool-output-override")?;
    let mode = match value {
        "interpreted" => ToolOutputMode::Interpreted,
        "raw" => ToolOutputMode::Raw,
        _ => anyhow::bail!(
            "--tool-output-override expects TOOL=raw or TOOL=interpreted, got {spec:?}"
        ),
    };
    Ok((tool_id, mode))
}

fn parse_tool_interpretation_model_override(spec: &str) -> anyhow::Result<(String, String)> {
    let (tool_id, model) = parse_tool_override_pair(spec, "--tool-interpretation-model")?;
    if model.trim().is_empty() {
        anyhow::bail!("--tool-interpretation-model model cannot be empty");
    }
    Ok((tool_id, model.to_string()))
}

fn parse_tool_guidance_override(spec: &str) -> anyhow::Result<(String, String)> {
    let (tool_id, guidance) = parse_tool_override_pair(spec, "--tool-guidance-override")?;
    if guidance.trim().is_empty() {
        anyhow::bail!("--tool-guidance-override guidance cannot be empty");
    }
    Ok((tool_id, guidance.to_string()))
}

fn parse_tool_override_pair<'a>(spec: &'a str, flag: &str) -> anyhow::Result<(String, &'a str)> {
    let Some((tool_id, value)) = spec.split_once('=') else {
        anyhow::bail!("{flag} expects TOOL=VALUE, got {spec:?}");
    };
    let tool_id = tool_id.trim();
    if tool_id.is_empty() || tool_id.chars().any(char::is_control) {
        anyhow::bail!("{flag} tool id cannot be empty or contain control characters");
    }
    Ok((tool_id.to_string(), value.trim()))
}

pub async fn profile_current(json: bool) -> anyhow::Result<()> {
    let paths = StoragePaths::from_env();
    let profile_id = paths.active_profile_id().to_string();
    let profile = ConfigResolver::from_env().show_profile(&profile_id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        println!("id: {}", profile.id);
        println!("name: {}", profile.name);
        println!("path: {}", profile.path.display());
    }
    Ok(())
}

pub async fn profile_create(id: String, name: Option<String>, json: bool) -> anyhow::Result<()> {
    let profile = ConfigResolver::from_env().create_profile(&id, name)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        println!("{}", profile.id);
        println!("name: {}", profile.name);
        println!("path: {}", profile.path.display());
    }
    Ok(())
}

pub async fn profile_list(json: bool) -> anyhow::Result<()> {
    let profiles = ConfigResolver::from_env().list_profiles()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&profiles)?);
    } else {
        for profile in profiles {
            println!(
                "{} name={:?} path={}",
                profile.id,
                profile.name,
                profile.path.display()
            );
        }
    }
    Ok(())
}

pub async fn profile_show(id: String, json: bool) -> anyhow::Result<()> {
    let profile = ConfigResolver::from_env().show_profile(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
    } else {
        println!("id: {}", profile.id);
        println!("name: {}", profile.name);
        println!("path: {}", profile.path.display());
    }
    Ok(())
}

pub async fn profile_delete(id: String) -> anyhow::Result<()> {
    if ConfigResolver::from_env().delete_profile(&id)? {
        println!("deleted profile {id}");
    } else {
        println!("profile {id} not found");
    }
    Ok(())
}

pub async fn profile_grant(
    from: Option<String>,
    to: String,
    kind: ProfileGrantKind,
    resource: String,
    json: bool,
) -> anyhow::Result<()> {
    let from = from.unwrap_or_else(|| StoragePaths::from_env().active_profile_id().to_string());
    let grant = ConfigResolver::from_env().grant_profile_access(&from, &to, kind, &resource)?;
    print_profile_grant(&grant, json)?;
    Ok(())
}

pub async fn profile_grants(from: Option<String>, json: bool) -> anyhow::Result<()> {
    let grants = if let Some(from) = from {
        ConfigResolver::from_env().list_profile_grants_from(&from)?
    } else {
        ConfigResolver::from_env().list_profile_grants()?
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&grants)?);
    } else if grants.is_empty() {
        println!("No profile grants found.");
    } else {
        for grant in grants {
            print_profile_grant(&grant, false)?;
        }
    }
    Ok(())
}

pub async fn profile_revoke_grant(id: String, json: bool) -> anyhow::Result<()> {
    let grant = ConfigResolver::from_env().revoke_profile_grant(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&grant)?);
    } else {
        println!("revoked profile grant {}", grant.id);
    }
    Ok(())
}

fn print_profile_grant(grant: &ProfileGrant, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(grant)?);
    } else {
        println!(
            "{} {:?} {}:{} -> {} created_at={}",
            grant.id,
            grant.kind,
            grant.from_profile,
            grant.resource,
            grant.to_profile,
            grant.created_at
        );
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

pub async fn model_providers(json: bool) -> anyhow::Result<()> {
    let providers = supported_model_providers();
    if json {
        println!("{}", serde_json::to_string_pretty(&providers)?);
    } else {
        for provider in providers {
            let modalities = if provider.available_modalities.is_empty() {
                "none".into()
            } else {
                provider.available_modalities.join(",")
            };
            let tool_support = provider
                .tool_support
                .map(|supported| supported.to_string())
                .unwrap_or_else(|| "model-dependent".into());
            let api_key = provider.api_key_env.as_deref().unwrap_or("none");
            let options = if provider.option_schema.is_empty() {
                "none".into()
            } else {
                provider
                    .option_schema
                    .iter()
                    .map(|option| match option.target {
                        ModelProviderOptionTarget::Runtime => option.key.clone(),
                        ModelProviderOptionTarget::ProviderOptions => {
                            format!("provider_options.{}", option.key)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            };
            println!(
                "{} default={} api_key_env={} modalities={} tools={} local={} native={} options={}",
                provider.id,
                provider.default_model,
                api_key,
                modalities,
                tool_support,
                provider.local,
                provider.native,
                options
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

pub async fn model_probe(id: String, json: bool) -> anyhow::Result<()> {
    let probe = ConfigResolver::from_env().probe_model_capabilities(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&probe)?);
    } else {
        println!(
            "{} provider={} saved={} modalities={} provider_modalities={} tools={} live={} found={}",
            probe.model_id,
            probe.provider,
            probe.saved_model,
            probe.declared_modalities.join(","),
            probe.provider_modalities.join(","),
            probe
                .tool_support
                .map(|supported| supported.to_string())
                .unwrap_or_else(|| "model-dependent".into()),
            probe.live_probe.status,
            probe
                .live_probe
                .model_found
                .map(|found| found.to_string())
                .unwrap_or_else(|| "unknown".into())
        );
        if let Some(message) = probe.live_probe.message {
            println!("note={message}");
        }
        if !probe.declared_limits.is_empty() {
            println!(
                "declared_limits={}",
                probe
                    .declared_limits
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        if !probe.declared_pricing.is_empty() {
            println!(
                "declared_pricing={}",
                probe
                    .declared_pricing
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        if !probe.live_probe.reported_modalities.is_empty() {
            println!(
                "live_modalities={}",
                probe.live_probe.reported_modalities.join(",")
            );
        }
        if !probe.live_probe.reported_capabilities.is_empty() {
            println!(
                "live_capabilities={}",
                probe.live_probe.reported_capabilities.join(",")
            );
        }
        if let Some(tool_support) = probe.live_probe.reported_tool_support {
            println!("live_tools={tool_support}");
        }
        if let Some(source) = probe.live_probe.fallback_source {
            println!("fallback_source={source}");
        }
        if !probe.live_probe.reported_limits.is_empty() {
            println!(
                "live_limits={}",
                probe
                    .live_probe
                    .reported_limits
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        if !probe.live_probe.reported_pricing.is_empty() {
            println!(
                "live_pricing={}",
                probe
                    .live_probe
                    .reported_pricing
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn model_save(
    id: String,
    provider: Option<String>,
    api_base_url: Option<String>,
    api_key_env: Option<String>,
    allow_missing_api_key: bool,
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
    top_p: Option<f64>,
    top_k: Option<u64>,
    reasoning_effort: Option<String>,
    metadata_json: Option<String>,
) -> anyhow::Result<()> {
    let model = model_config_from_parts(
        id,
        provider,
        api_base_url,
        api_key_env,
        allow_missing_api_key,
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
        top_p,
        top_k,
        reasoning_effort,
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

pub async fn model_export(id: String, path: String, json: bool) -> anyhow::Result<()> {
    let model = ConfigResolver::from_env().export_model_config(&id, &path)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": path,
                "model": model
            }))?
        );
    } else {
        println!("exported model {} to {}", model.id, path);
    }
    Ok(())
}

pub async fn model_import(path: String, json: bool) -> anyhow::Result<()> {
    let model = ConfigResolver::from_env().import_model_config(&path)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&model)?);
    } else {
        println!("imported model {}", model.id);
    }
    Ok(())
}

pub async fn ingest_add(
    path: String,
    backend: String,
    vision_model: Option<String>,
    guardrail_model: Option<String>,
) -> anyhow::Result<()> {
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
    let artifact =
        ingest_with_optional_models(path, &backend, vision_model, guardrail_model).await?;
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

pub async fn ingest_backends(json: bool) -> anyhow::Result<()> {
    let backends = supported_ingestion_backends();
    if json {
        println!("{}", serde_json::to_string_pretty(&backends)?);
    } else {
        for backend in backends {
            let modalities = if backend.modalities.is_empty() {
                "none".into()
            } else {
                backend.modalities.join(",")
            };
            println!(
                "{} name={:?} modalities={} description={:?}",
                backend.id, backend.name, modalities, backend.description
            );
        }
    }
    Ok(())
}

pub async fn ingest_rerun(
    id: String,
    backend: String,
    vision_model: Option<String>,
    guardrail_model: Option<String>,
) -> anyhow::Result<()> {
    let source = IngestionStore::from_env().show(&id)?.source;
    ingest_add(
        source.display().to_string(),
        backend,
        vision_model,
        guardrail_model,
    )
    .await
}

async fn ingest_with_optional_models(
    path: String,
    backend: &str,
    vision_model: Option<String>,
    guardrail_model: Option<String>,
) -> anyhow::Result<IngestionArtifact> {
    let store = IngestionStore::from_env();
    let vision_model = clean_optional_string(vision_model);
    let guardrail_model =
        clean_optional_string(guardrail_model).or_else(configured_ingestion_guardrail_model);
    let vision_provider = match vision_model.as_deref() {
        Some(model) => {
            ensure_model_supports_vision(model, &path)?;
            Some(ingestion_provider_for_model(model, Some(2048), Some(0.0))?)
        }
        None => None,
    };
    let guardrail_provider = match guardrail_model.as_deref() {
        Some(model) => Some(ingestion_provider_for_model(model, Some(256), Some(0.0))?),
        None => None,
    };

    Ok(store
        .ingest_with_backend_and_models(
            path,
            backend,
            vision_model
                .map(ModelRef::from)
                .zip(vision_provider.as_ref())
                .map(|(model, provider)| IngestionModelCall {
                    provider: provider.as_ref(),
                    model,
                }),
            guardrail_model
                .map(ModelRef::from)
                .zip(guardrail_provider.as_ref())
                .map(|(model, provider)| IngestionModelCall {
                    provider: provider.as_ref(),
                    model,
                }),
        )
        .await?)
}

fn ensure_model_supports_vision(model: &str, source: &str) -> anyhow::Result<()> {
    let Some(requirement) = model_vision_source_requirement(source) else {
        return Ok(());
    };
    let support = ConfigResolver::from_env()
        .model_supports_any_modality(model, &requirement.required_modalities)?;
    if support.supported {
        return Ok(());
    }
    let modalities = if support.available_modalities.is_empty() {
        "none".into()
    } else {
        support.available_modalities.join(",")
    };
    let required_modalities = requirement.required_modalities.join(",");
    anyhow::bail!(
        "vision model {model:?} does not advertise a supported input modality for {} vision source (attachment={}, requires one of {}, provider={}, source={}, modalities={}); save the model with the required available_modalities or choose a compatible provider",
        requirement.source_kind,
        requirement.attachment_kind,
        required_modalities,
        support.provider,
        support.source,
        modalities
    )
}

#[cfg(test)]
fn guardrail_provider_for_model(
    model: &str,
    model_runtime: &ModelRuntimeConfig,
) -> anyhow::Result<Arc<dyn LlmProvider>> {
    ingestion_provider_for_runtime(model, model_runtime, Some(256), Some(0.0))
}

fn ingestion_provider_for_model(
    model: &str,
    max_output_tokens: Option<u64>,
    temperature: Option<f64>,
) -> anyhow::Result<Arc<dyn LlmProvider>> {
    let model_runtime = ConfigResolver::from_env()
        .resolve_model_runtime(model)?
        .unwrap_or_default();
    ingestion_provider_for_runtime(model, &model_runtime, max_output_tokens, temperature)
}

fn ingestion_provider_for_runtime(
    model: &str,
    model_runtime: &ModelRuntimeConfig,
    max_output_tokens: Option<u64>,
    temperature: Option<f64>,
) -> anyhow::Result<Arc<dyn LlmProvider>> {
    let provider = model_runtime
        .provider
        .as_deref()
        .unwrap_or("rig")
        .trim()
        .to_ascii_lowercase();
    match provider.as_str() {
        "anthropic" => Ok(Arc::new(AnthropicProvider::from_config(
            model_runtime.native_provider_config(
                ModelRef::from(model),
                NativeProviderConfig::anthropic,
                max_output_tokens,
                temperature,
            ),
        )?)),
        "gemini" => Ok(Arc::new(GeminiProvider::from_config(
            model_runtime.native_provider_config(
                ModelRef::from(model),
                NativeProviderConfig::gemini,
                max_output_tokens,
                temperature,
            ),
        )?)),
        _ => Ok(Arc::new(RigProvider::from_config(
            model_runtime.rig_provider_config(
                ModelRef::from(model),
                max_output_tokens,
                temperature,
            )?,
        )?)),
    }
}

fn configured_ingestion_guardrail_model() -> Option<String> {
    ConfigResolver::from_env()
        .resolve_agent("fake-agent")
        .ok()
        .and_then(|resolved| {
            resolved
                .values
                .into_iter()
                .find(|value| value.key == "agent.ingestion_policy.guardrail_model")
        })
        .and_then(|value| value.value.as_str().map(str::to_string))
        .filter(|value| !value.trim().is_empty())
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
        if !artifact.findings.is_empty() {
            println!("findings:");
            for (index, summary) in artifact.finding_summaries().iter().enumerate() {
                println!("- #{index} {summary}");
            }
        }
        println!();
        println!("{}", artifact.extracted_text.unwrap_or_default());
    }
    Ok(())
}

pub async fn ingest_review(
    id: String,
    finding: u32,
    decision: String,
    note: Option<String>,
) -> anyhow::Result<()> {
    let decision = parse_ingestion_review_decision(&decision)?;
    let artifact = IngestionStore::from_env().review_finding(&id, finding, decision, note)?;
    println!("{}", serde_json::to_string_pretty(&artifact)?);
    Ok(())
}

fn parse_ingestion_review_decision(
    decision: &str,
) -> anyhow::Result<IngestionFindingReviewDecision> {
    IngestionFindingReviewDecision::parse(decision).ok_or_else(|| {
        anyhow::anyhow!("decision must be acknowledge, approve/allow, or reject/block")
    })
}

pub async fn ingest_rm(id: String) -> anyhow::Result<()> {
    IngestionStore::from_env().remove(&id)?;
    println!("removed ingestion artifact {id}");
    Ok(())
}

pub async fn artifact_list(json: bool) -> anyhow::Result<()> {
    let artifacts = list_generated_artifacts_from_env()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&artifacts)?);
    } else {
        for artifact in artifacts {
            println!(
                "{} {} bytes={} path={}",
                artifact.id,
                artifact.format,
                artifact.bytes,
                artifact.path.display()
            );
        }
    }
    Ok(())
}

pub async fn artifact_show(id: String, json: bool) -> anyhow::Result<()> {
    let artifact = show_generated_artifact_from_env(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&artifact)?);
    } else {
        println!("{} {}", artifact.id, artifact.path.display());
        println!("format: {}", artifact.format);
        println!("bytes: {}", artifact.bytes);
        if let Some(modified_ms) = artifact.modified_ms {
            println!("modified_ms: {modified_ms}");
        }
    }
    Ok(())
}

pub async fn artifact_open(id: String, json: bool) -> anyhow::Result<()> {
    let artifact = open_generated_artifact_from_env(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&artifact)?);
    } else {
        println!(
            "opened artifact {} at {}",
            artifact.id,
            artifact.path.display()
        );
    }
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

pub async fn clawhub_search(
    catalog: String,
    query: Option<String>,
    json: bool,
) -> anyhow::Result<()> {
    let entries = ClawHubProvider::from_catalog(catalog)?.search(query.as_deref());
    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        for entry in entries {
            println!(
                "{} {} source={} digest={}",
                entry.id,
                entry.name,
                entry.source.display(),
                entry.digest.as_deref().unwrap_or("unpinned")
            );
        }
    }
    Ok(())
}

pub async fn clawhub_inspect(catalog: String, id: String, json: bool) -> anyhow::Result<()> {
    let inspection = ClawHubProvider::from_catalog(catalog)?.inspect(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&inspection)?);
    } else {
        println!(
            "{} {} source={}",
            inspection.entry.id,
            inspection.entry.name,
            inspection.entry.source.display()
        );
        println!(
            "catalog_digest_match={}",
            inspection
                .digest_matches
                .map(|matches| matches.to_string())
                .unwrap_or_else(|| "unpinned".into())
        );
        print_adapter_package(inspection.package, false)?;
    }
    Ok(())
}

pub async fn clawhub_pin(catalog: String, id: String, json: bool) -> anyhow::Result<()> {
    let pin = ClawHubProvider::from_catalog(catalog)?.pin(&id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&pin)?);
    } else {
        println!("{} {} {}", pin.id, pin.digest, pin.source.display());
    }
    Ok(())
}

pub async fn clawhub_install(catalog: String, id: String) -> anyhow::Result<()> {
    let package =
        ClawHubProvider::from_catalog(catalog)?.install(&id, &AdapterRegistry::from_env())?;
    println!("{}", serde_json::to_string_pretty(&package)?);
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
            "permissions shell={} file_read={} file_write={} network={} secrets={} wallet={} payment={} browser_profile={}",
            package.permissions.shell,
            package.permissions.file_read,
            package.permissions.file_write,
            package.permissions.network,
            package.permissions.secrets,
            package.permissions.wallet,
            package.permissions.payment,
            package.permissions.browser_profile
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
    let url_for_hint = url.clone();
    let client = DaemonHttpClient::new(url);
    let conversation_id = options.conversation_id.clone();
    let response = client.post_json(
        "/run",
        serde_json::json!({
            "input": input,
            "demo": demo,
            "provider": provider_name(options.provider),
            "agent_id": options.agent_id.clone(),
            "model": options.model,
            "api_base_url": options.api_base_url,
            "api_key_env": options.api_key_env,
            "input_cost_per_million": options.input_cost_per_million,
            "output_cost_per_million": options.output_cost_per_million,
            "max_tool_calls": options.max_tool_calls,
            "max_tokens_before_compaction": options.max_tokens_before_compaction,
            "max_compaction_output_tokens": options.max_compaction_output_tokens,
            "compaction_guidance": options.compaction_guidance,
            "tool_visibility": options.tool_visibility,
            "skill_visibility": options.skill_visibility,
            "enable_shell": options.enable_shell,
            "enable_subagent": options.enable_subagent,
            "raw_tool_output": options.raw_tool_output,
            "load_memory": options.load_memory,
            "memory_topics": options.memory_topics,
            "load_skills": options.load_skills,
            "compacted_context": included_compacted_context(&options)?,
            "conversation_id": options.conversation_id,
            "include_ingest": options.include_ingest,
            "allow_unsafe_ingest": options.allow_unsafe_ingest,
            "enable_prompt_refinement": options.enable_prompt_refinement,
            "prompt_refinement_instructions": options.prompt_refinement_instructions,
            "prompt_refinement_model": options.prompt_refinement_model,
            "require_approval": options.require_approval,
            "auto_approve": options.auto_approve
        }),
    )?;
    if let Some(run_id) = response.get("run_id").and_then(|value| value.as_str()) {
        print_remote_auto_compaction_keep_hint(
            &client,
            &url_for_hint,
            run_id,
            conversation_id.as_deref(),
        );
    }
    print_remote(response)
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
            "agent_id": options.agent_id.clone(),
            "model": options.model,
            "api_base_url": options.api_base_url,
            "api_key_env": options.api_key_env,
            "input_cost_per_million": options.input_cost_per_million,
            "output_cost_per_million": options.output_cost_per_million,
            "max_tool_calls": options.max_tool_calls,
            "max_tokens_before_compaction": options.max_tokens_before_compaction,
            "max_compaction_output_tokens": options.max_compaction_output_tokens,
            "compaction_guidance": options.compaction_guidance,
            "tool_visibility": options.tool_visibility,
            "skill_visibility": options.skill_visibility,
            "enable_shell": options.enable_shell,
            "enable_subagent": options.enable_subagent,
            "raw_tool_output": options.raw_tool_output,
            "load_memory": options.load_memory,
            "memory_topics": options.memory_topics,
            "load_skills": options.load_skills,
            "compacted_context": included_compacted_context(&options)?,
            "conversation_id": options.conversation_id,
            "include_ingest": options.include_ingest,
            "allow_unsafe_ingest": options.allow_unsafe_ingest,
            "enable_prompt_refinement": options.enable_prompt_refinement,
            "prompt_refinement_instructions": options.prompt_refinement_instructions,
            "prompt_refinement_model": options.prompt_refinement_model,
            "require_approval": options.require_approval,
            "auto_approve": options.auto_approve
        }),
    )?)
}

pub async fn remote_run_status(url: String, run_id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/run/status/{run_id}"))?)
}

pub async fn remote_run_events(
    url: String,
    run_id: String,
    after: Option<u64>,
) -> anyhow::Result<()> {
    let path = match after {
        Some(after) => format!("/run/events/{run_id}?after={after}"),
        None => format!("/run/events/{run_id}"),
    };
    print_remote(DaemonHttpClient::new(url).get_json(&path)?)
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
            "agent_id": options.agent_id.clone(),
            "enable_shell": options.enable_shell,
            "enable_subagent": options.enable_subagent,
            "max_tool_calls": options.max_tool_calls,
            "max_tokens_before_compaction": options.max_tokens_before_compaction,
            "max_compaction_output_tokens": options.max_compaction_output_tokens,
            "compaction_guidance": options.compaction_guidance,
            "tool_visibility": options.tool_visibility,
            "skill_visibility": options.skill_visibility,
            "raw_tool_output": options.raw_tool_output,
            "load_memory": options.load_memory,
            "memory_topics": options.memory_topics,
            "load_skills": options.load_skills,
            "compacted_context": included_compacted_context(&options)?,
            "conversation_id": options.conversation_id,
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

pub async fn remote_resume(
    url: String,
    run_id: String,
    from_event: Option<u64>,
    demo: Demo,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/resume",
        serde_json::json!({
            "run_id": run_id,
            "from_event": from_event,
            "demo": demo_name(demo)
        }),
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
    auto_approve: bool,
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
    if auto_approve {
        match input.as_object_mut() {
            Some(map) => {
                map.insert("__auto_approve".into(), serde_json::Value::Bool(true));
            }
            None => {
                input = serde_json::json!({
                    "__auto_approve": true,
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

pub async fn remote_trace_hooks(url: String, run_id: String) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    print_remote(client.get_json(&format!("/trace/{run_id}/hooks"))?)
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
    unlock_env: Option<String>,
    signature_env: Option<String>,
    controller_agent: Option<String>,
) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    let (unlock, signature) = if approved {
        (
            approval_unlock_from_env(unlock_env)?,
            approval_signature_from_env(signature_env)?,
        )
    } else {
        (None, None)
    };
    let mut body = serde_json::json!({ "approved": approved });
    if let Some(unlock) = unlock {
        body["unlock"] = serde_json::Value::String(unlock);
    }
    if let Some(signature) = signature {
        body["signature"] = serde_json::Value::String(signature);
    }
    if let Some(controller_agent) = controller_agent
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        body["controller_agent"] = serde_json::Value::String(controller_agent);
    }
    print_remote(client.post_json(&format!("/approvals/{run_id}/{approval_id}/decide"), body)?)
}

pub async fn remote_approval_execute(
    url: String,
    run_id: String,
    approval_id: String,
    unlock_env: Option<String>,
    signature_env: Option<String>,
) -> anyhow::Result<()> {
    let client = DaemonHttpClient::new(url);
    let unlock = approval_unlock_from_env(unlock_env)?;
    let signature = approval_signature_from_env(signature_env)?;
    let mut body = serde_json::json!({});
    if let Some(unlock) = unlock {
        body["unlock"] = serde_json::Value::String(unlock);
    }
    if let Some(signature) = signature {
        body["signature"] = serde_json::Value::String(signature);
    }
    print_remote(client.post_json(&format!("/approvals/{run_id}/{approval_id}/execute"), body)?)
}

pub async fn remote_storage_report(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/storage")?)
}

pub async fn remote_memory_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/memory")?)
}

pub async fn remote_memory_backends(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/memory/backends")?)
}

pub async fn remote_memory_create(
    url: String,
    content: String,
    user: bool,
    topics: Vec<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory",
        serde_json::json!({ "content": content, "user": user, "topics": topics }),
    )?)
}

pub async fn remote_memory_generate(
    url: String,
    text: String,
    user: bool,
    range: Option<String>,
    topics: Vec<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory/generate",
        serde_json::json!({ "text": text, "user": user, "range": range, "topics": topics }),
    )?)
}

pub async fn remote_memory_generate_conversation(
    url: String,
    id: String,
    from: Option<usize>,
    to: Option<usize>,
    user: bool,
    topics: Vec<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory/generate-conversation",
        serde_json::json!({ "id": id, "from": from, "to": to, "user": user, "topics": topics }),
    )?)
}

pub async fn remote_memory_generate_pending(
    url: String,
    user: bool,
    limit: Option<usize>,
    topics: Vec<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory/generate-pending",
        serde_json::json!({ "user": user, "limit": limit, "topics": topics }),
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

pub async fn remote_memory_export(url: String, path: String, user: bool) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory/export",
        serde_json::json!({ "path": path, "user": user }),
    )?)
}

pub async fn remote_memory_import(url: String, path: String, user: bool) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/memory/import",
        serde_json::json!({ "path": path, "user": user }),
    )?)
}

pub async fn remote_compact_keep_run(
    url: String,
    run_id: String,
    conversation: Option<String>,
    guidance: Option<String>,
) -> anyhow::Result<()> {
    let run_id = RunId(uuid::Uuid::parse_str(&run_id)?);
    let client = DaemonHttpClient::new(url);
    let events: Vec<RunEvent> =
        serde_json::from_value(client.get_json(&format!("/trace/{}", run_id.0))?)?;
    let Some(snapshot) = latest_auto_compaction_snapshot(&events) else {
        return print_remote(serde_json::json!({
            "run_id": run_id.0,
            "record": null
        }));
    };
    let Some(content) = snapshot.compacted else {
        return print_remote(serde_json::json!({
            "run_id": run_id.0,
            "record": null
        }));
    };
    let record = client.post_json(
        "/compactions/keep",
        serde_json::json!({
            "content": content,
            "guidance": guidance,
            "source": format!("auto-run:{}", run_id.0),
            "conversation_id": conversation,
            "max_output_tokens": null
        }),
    )?;
    print_remote(serde_json::json!({
        "run_id": run_id.0,
        "record": record
    }))
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

pub async fn remote_skill_export(url: String, id: String, path: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/skills/{id}/export"),
        serde_json::json!({ "path": path }),
    )?)
}

pub async fn remote_skill_import_doc(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/skills/import-doc", serde_json::json!({ "path": path }))?,
    )
}

pub async fn remote_capability_propose(
    url: String,
    kind: String,
    name: String,
    body: Option<String>,
    guidance: Option<String>,
    created_by: String,
) -> anyhow::Result<()> {
    let body = read_text(body)?;
    print_remote(DaemonHttpClient::new(url).post_json(
        "/capabilities/propose",
        serde_json::json!({
            "kind": kind,
            "name": name,
            "body": body,
            "guidance": guidance,
            "created_by": created_by
        }),
    )?)
}

pub async fn remote_capability_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/capabilities")?)
}

pub async fn remote_capability_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/capabilities/{id}"))?)
}

pub async fn remote_capability_review(url: String, id: String, allow: bool) -> anyhow::Result<()> {
    let action = if allow { "allow" } else { "reject" };
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/capabilities/{id}/{action}"),
        serde_json::json!({}),
    )?)
}

pub async fn remote_capability_delete(url: String, id: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json(&format!("/capabilities/{id}/delete"), serde_json::json!({}))?,
    )
}

pub async fn remote_prompt_save(
    url: String,
    name: String,
    text: String,
    agent: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/prompts",
        serde_json::json!({ "name": name, "body": text, "agent_id": agent }),
    )?)
}

pub async fn remote_prompt_list(url: String, agent: Option<String>) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/prompts/list", serde_json::json!({ "agent_id": agent }))?,
    )
}

pub async fn remote_prompt_show(
    url: String,
    name: String,
    agent: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/prompts/show",
        serde_json::json!({ "name": name, "agent_id": agent }),
    )?)
}

pub async fn remote_prompt_delete(
    url: String,
    name: String,
    agent: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/prompts/delete",
        serde_json::json!({ "name": name, "agent_id": agent }),
    )?)
}

pub async fn remote_agent_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/agents")?)
}

pub async fn remote_agent_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/agents/{id}"))?)
}

#[allow(clippy::too_many_arguments)]
pub async fn remote_agent_save(
    url: String,
    id: String,
    name: Option<String>,
    system_prompt: String,
    model: Option<String>,
    max_tool_calls: Option<u32>,
    max_tokens_before_compaction: Option<u32>,
    max_compaction_output_tokens: Option<u32>,
    compaction_guidance: Option<String>,
    max_subagent_depth: Option<u32>,
    max_recursion_depth: Option<u32>,
    allowed_tools: Vec<String>,
    allowed_tool_categories: Vec<String>,
    approval_controller_agent: Option<String>,
    approval_controller_allowed_tools: Vec<String>,
    approval_controller_allowed_tool_categories: Vec<String>,
    allowed_skill_categories: Vec<String>,
    tool_output_mode: Option<ToolOutputMode>,
    tool_output_interpretation_model: Option<String>,
    tool_output_overrides: Vec<String>,
    tool_interpretation_model_overrides: Vec<String>,
    tool_guidance_overrides: Vec<String>,
    tool_visibility: Option<VisibilityLevel>,
    load_memory: bool,
    load_skills: bool,
    ingestion_guardrail: Option<IngestionGuardrailMode>,
    ingestion_guardrail_model: Option<String>,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
    refinement_instructions: Option<String>,
    refinement_model: Option<String>,
    refinement_aware: bool,
) -> anyhow::Result<()> {
    let agent = agent_config_from_parts(
        id,
        name,
        system_prompt,
        model,
        max_tool_calls,
        max_tokens_before_compaction,
        max_compaction_output_tokens,
        compaction_guidance,
        max_subagent_depth,
        max_recursion_depth,
        allowed_tools,
        allowed_tool_categories,
        approval_controller_agent,
        approval_controller_allowed_tools,
        approval_controller_allowed_tool_categories,
        allowed_skill_categories,
        tool_output_mode,
        tool_output_interpretation_model,
        tool_output_overrides,
        tool_interpretation_model_overrides,
        tool_guidance_overrides,
        tool_visibility,
        load_memory,
        load_skills,
        ingestion_guardrail,
        ingestion_guardrail_model,
        input_cost_per_million,
        output_cost_per_million,
        refinement_instructions,
        refinement_model,
        refinement_aware,
    )?;
    print_remote(DaemonHttpClient::new(url).post_json("/agents", serde_json::to_value(agent)?)?)
}

pub async fn remote_agent_delete(url: String, id: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json(&format!("/agents/{id}/delete"), serde_json::json!({}))?,
    )
}

pub async fn remote_agent_export(url: String, id: String, path: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/agents/{id}/export"),
        serde_json::json!({ "path": path }),
    )?)
}

pub async fn remote_agent_import(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/agents/import", serde_json::json!({ "path": path }))?,
    )
}

pub async fn remote_model_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/models")?)
}

pub async fn remote_model_providers(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/model-providers")?)
}

pub async fn remote_model_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/models/{id}"))?)
}

pub async fn remote_model_probe(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/models/{id}/probe"))?)
}

#[allow(clippy::too_many_arguments)]
pub async fn remote_model_save(
    url: String,
    id: String,
    provider: Option<String>,
    api_base_url: Option<String>,
    api_key_env: Option<String>,
    allow_missing_api_key: bool,
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
    top_p: Option<f64>,
    top_k: Option<u64>,
    reasoning_effort: Option<String>,
    metadata_json: Option<String>,
) -> anyhow::Result<()> {
    let model = model_config_from_parts(
        id,
        provider,
        api_base_url,
        api_key_env,
        allow_missing_api_key,
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
        top_p,
        top_k,
        reasoning_effort,
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

pub async fn remote_model_export(url: String, id: String, path: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/models/{id}/export"),
        serde_json::json!({ "path": path }),
    )?)
}

pub async fn remote_model_import(url: String, path: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json("/models/import", serde_json::json!({ "path": path }))?,
    )
}

pub async fn remote_ingest_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/ingest")?)
}

#[allow(clippy::too_many_arguments)]
fn model_config_from_parts(
    id: String,
    provider: Option<String>,
    api_base_url: Option<String>,
    api_key_env: Option<String>,
    allow_missing_api_key: bool,
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
    top_p: Option<f64>,
    top_k: Option<u64>,
    reasoning_effort: Option<String>,
    metadata_json: Option<String>,
) -> anyhow::Result<ModelConfig> {
    let mut metadata = match metadata_json {
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
    merge_provider_option_flags(&mut metadata, top_p, top_k, reasoning_effort)?;
    let available_modalities = available_modalities
        .into_iter()
        .map(|modality| modality.trim().to_string())
        .filter(|modality| !modality.is_empty())
        .collect();
    Ok(ModelConfig {
        id,
        provider: clean_optional_string(provider),
        api_base_url: clean_optional_string(api_base_url),
        api_key_env: clean_optional_string(api_key_env),
        allow_missing_api_key: allow_missing_api_key.then_some(true),
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

fn merge_provider_option_flags(
    metadata: &mut BTreeMap<String, serde_json::Value>,
    top_p: Option<f64>,
    top_k: Option<u64>,
    reasoning_effort: Option<String>,
) -> anyhow::Result<()> {
    let mut provider_options = match metadata.remove("provider_options") {
        Some(serde_json::Value::Object(object)) => object,
        Some(_) => anyhow::bail!("metadata.provider_options must be a JSON object"),
        None => serde_json::Map::new(),
    };
    if let Some(top_p) = top_p {
        if !(0.0..=1.0).contains(&top_p) {
            anyhow::bail!("--top-p must be between 0.0 and 1.0");
        }
        provider_options.insert("top_p".into(), serde_json::json!(top_p));
    }
    if let Some(top_k) = top_k {
        if top_k == 0 {
            anyhow::bail!("--top-k must be greater than zero");
        }
        provider_options.insert("top_k".into(), serde_json::json!(top_k));
    }
    if let Some(reasoning_effort) = clean_optional_string(reasoning_effort) {
        provider_options.insert(
            "reasoning_effort".into(),
            serde_json::json!(reasoning_effort),
        );
    }
    if !provider_options.is_empty() {
        metadata.insert(
            "provider_options".into(),
            serde_json::Value::Object(provider_options),
        );
    }
    Ok(())
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub async fn remote_ingest_add(
    url: String,
    path: String,
    backend: String,
    vision_model: Option<String>,
    guardrail_model: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/ingest",
        serde_json::json!({
            "path": path,
            "backend": backend,
            "vision_model": vision_model,
            "guardrail_model": guardrail_model
        }),
    )?)
}

pub async fn remote_ingest_backends(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/ingest/backends")?)
}

pub async fn remote_ingest_rerun(
    url: String,
    id: String,
    backend: String,
    vision_model: Option<String>,
    guardrail_model: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/ingest/{id}/rerun"),
        serde_json::json!({
            "backend": backend,
            "vision_model": vision_model,
            "guardrail_model": guardrail_model
        }),
    )?)
}

pub async fn remote_ingest_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/ingest/{id}"))?)
}

pub async fn remote_ingest_review(
    url: String,
    id: String,
    finding: u32,
    decision: String,
    note: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        &format!("/ingest/{id}/review"),
        serde_json::json!({
            "finding": finding,
            "decision": decision,
            "note": note
        }),
    )?)
}

pub async fn remote_ingest_rm(url: String, id: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url).post_json(&format!("/ingest/{id}/rm"), serde_json::json!({}))?,
    )
}

pub async fn remote_artifact_list(url: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json("/artifacts")?)
}

pub async fn remote_artifact_show(url: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).get_json(&format!("/artifacts/{id}"))?)
}

pub async fn remote_artifact_open(url: String, id: String) -> anyhow::Result<()> {
    print_remote(
        DaemonHttpClient::new(url)
            .post_json(&format!("/artifacts/{id}/open"), serde_json::json!({}))?,
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

pub async fn remote_clawhub_search(
    url: String,
    catalog: String,
    query: Option<String>,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/adapters/clawhub/search",
        serde_json::json!({ "catalog": catalog, "query": query }),
    )?)
}

pub async fn remote_clawhub_inspect(
    url: String,
    catalog: String,
    id: String,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/adapters/clawhub/inspect",
        serde_json::json!({ "catalog": catalog, "id": id }),
    )?)
}

pub async fn remote_clawhub_pin(url: String, catalog: String, id: String) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/adapters/clawhub/pin",
        serde_json::json!({ "catalog": catalog, "id": id }),
    )?)
}

pub async fn remote_clawhub_install(
    url: String,
    catalog: String,
    id: String,
) -> anyhow::Result<()> {
    print_remote(DaemonHttpClient::new(url).post_json(
        "/adapters/clawhub/install",
        serde_json::json!({ "catalog": catalog, "id": id }),
    )?)
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
        Provider::Ollama => "ollama",
        Provider::LlamaCpp => "llama_cpp",
        Provider::Anthropic => "anthropic",
        Provider::Gemini => "gemini",
    }
}

fn demo_name(demo: Demo) -> &'static str {
    match demo {
        Demo::Echo => "echo",
        Demo::Tool => "tool",
    }
}

fn included_compacted_context(options: &setup::RuntimeOptions) -> anyhow::Result<Option<String>> {
    let Some(id) = options.include_compact.as_deref() else {
        return Ok(None);
    };
    Ok(Some(CompactionStore::from_env().show(id)?.content))
}

pub(crate) fn is_auto_compaction_snapshot(snapshot: &ContextSnapshot) -> bool {
    snapshot
        .compacted
        .as_deref()
        .is_some_and(|text| !text.trim().is_empty())
        && snapshot.provenance.iter().any(|record| {
            record.fragment == "compacted_context"
                && record.source == "agent.context_policy.auto_compaction"
        })
}

pub(crate) fn keep_auto_compaction_for_run(
    run_id: RunId,
    conversation_id: Option<String>,
    guidance: Option<String>,
) -> anyhow::Result<Option<CompactionRecord>> {
    let events = open_event_store()?.try_events(run_id)?;
    keep_auto_compaction_from_events(run_id, &events, conversation_id, guidance)
}

fn keep_auto_compaction_from_events(
    run_id: RunId,
    events: &[RunEvent],
    conversation_id: Option<String>,
    guidance: Option<String>,
) -> anyhow::Result<Option<CompactionRecord>> {
    let Some(snapshot) = latest_auto_compaction_snapshot(events) else {
        return Ok(None);
    };
    let Some(content) = snapshot.compacted.as_deref() else {
        return Ok(None);
    };
    Ok(Some(CompactionStore::from_env().keep_compacted_context(
        content,
        guidance,
        None,
        Some(format!("auto-run:{}", run_id.0)),
        conversation_id,
    )?))
}

fn latest_auto_compaction_snapshot(events: &[RunEvent]) -> Option<ContextSnapshot> {
    events.iter().filter_map(context_built_snapshot).next_back()
}

fn context_built_snapshot(event: &RunEvent) -> Option<ContextSnapshot> {
    let RunEventKind::ContextBuilt { snapshot } = &event.kind else {
        return None;
    };
    let snapshot = serde_json::from_value::<ContextSnapshot>(snapshot.clone()).ok()?;
    is_auto_compaction_snapshot(&snapshot).then_some(snapshot)
}

fn print_auto_compaction_keep_hint(
    run_id: RunId,
    events: &[RunEvent],
    conversation_id: Option<&str>,
) {
    if latest_auto_compaction_snapshot(events).is_none() {
        return;
    }
    let conversation = conversation_id
        .map(|id| format!(" --conversation {id}"))
        .unwrap_or_default();
    eprintln!(
        "auto compacted context ready: keep it with `agent compact keep-run {}{conversation}`",
        run_id.0
    );
}

fn print_remote_auto_compaction_keep_hint(
    client: &DaemonHttpClient,
    url: &str,
    run_id: &str,
    conversation_id: Option<&str>,
) {
    let Ok(value) = client.get_json(&format!("/trace/{run_id}")) else {
        return;
    };
    let Ok(events) = serde_json::from_value::<Vec<RunEvent>>(value) else {
        return;
    };
    if latest_auto_compaction_snapshot(&events).is_none() {
        return;
    }
    let conversation = conversation_id
        .map(|id| format!(" --conversation {id}"))
        .unwrap_or_default();
    eprintln!(
        "auto compacted context ready: keep it with `agent remote --url {url} compact keep-run {run_id}{conversation}`"
    );
}

fn stop_compaction_for_run(run_id: RunId) -> anyhow::Result<Option<String>> {
    let source = format!("stopped-run:{}", run_id.0);
    Ok(CompactionStore::from_env()
        .list()?
        .into_iter()
        .filter(|record| record.source == source)
        .max_by_key(|record| record.created_at)
        .map(|record| record.id))
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
                controller_agent,
                controller_scope,
            } => approvals.push(serde_json::json!({
                "approval_id": approval_id,
                "action": action,
                "reason": reason,
                "controller_agent": controller_agent,
                "controller_scope": controller_scope,
                "status": "pending"
            })),
            RunEventKind::ApprovalResolved {
                approval_id,
                approved,
                delegated_controller,
            } => {
                if let Some(existing) = approvals
                    .iter_mut()
                    .find(|value| value["approval_id"] == approval_id)
                {
                    existing["status"] = serde_json::Value::String(
                        if approved { "approved" } else { "rejected" }.into(),
                    );
                    existing["approved"] = serde_json::Value::Bool(approved);
                    existing["delegated_controller"] = delegated_controller
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null);
                } else {
                    approvals.push(serde_json::json!({
                        "approval_id": approval_id,
                        "action": null,
                        "reason": null,
                        "status": if approved { "approved" } else { "rejected" },
                        "approved": approved,
                        "delegated_controller": delegated_controller
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
        RunEventKind::LlmStreamToken { delta } => {
            format!("LlmStreamToken delta={delta:?}")
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
            model,
            permissions,
        } => {
            let model = model
                .as_deref()
                .map(|value| format!(" model={value}"))
                .unwrap_or_default();
            let permissions = permissions
                .as_ref()
                .map(|value| format!(" permissions={value}"))
                .unwrap_or_default();
            format!(
                "ToolCallProposed call={call_id} tool={tool_id}{model} input={input}{permissions}"
            )
        }
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
            controller_agent,
            ..
        } => {
            let controller = controller_agent
                .as_ref()
                .map(|agent| format!(" controller={agent}"))
                .unwrap_or_default();
            format!(
                "ApprovalRequested id={approval_id} action={action} reason={reason}{controller}"
            )
        }
        RunEventKind::ApprovalResolved {
            approval_id,
            approved,
            delegated_controller,
        } => {
            let controller = delegated_controller
                .as_ref()
                .map(|agent| format!(" delegated_controller={agent}"))
                .unwrap_or_default();
            format!("ApprovalResolved id={approval_id} approved={approved}{controller}")
        }
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
            findings,
            high_risk_findings,
            finding_snippets,
        } => {
            let risk = if *high_risk_findings > 0 {
                format!(" high_risk_findings={high_risk_findings}")
            } else {
                String::new()
            };
            let finding_summary = if findings.is_empty() {
                String::new()
            } else {
                format!(" findings={:?}", findings)
            };
            let snippets = if finding_snippets.is_empty() {
                String::new()
            } else {
                format!(" snippets={:?}", finding_snippets)
            };
            format!(
                "IngestionCompleted artifact={artifact_id} hash={content_hash} sections={sections}{risk}{finding_summary}{snippets}"
            )
        }
        RunEventKind::HookFired {
            hook_id,
            trigger,
            payload_digest,
        } => {
            format!("HookFired hook={hook_id} trigger={trigger} payload_digest={payload_digest}")
        }
        RunEventKind::HookFailed {
            hook_id,
            trigger,
            error,
            attempt,
            will_retry,
        } => {
            format!(
                "HookFailed hook={hook_id} trigger={trigger} attempt={attempt} will_retry={will_retry} error={error}"
            )
        }
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

fn resolve_saved_prompt_or_literal(text: &str, agent_id: Option<&str>) -> anyhow::Result<String> {
    let trimmed = text.trim();
    let (name, slash_invocation) = trimmed
        .strip_prefix("/run ")
        .map(str::trim)
        .map(|name| (name, true))
        .unwrap_or((text, false));
    if !is_valid_prompt_name(name) {
        return Ok(if slash_invocation {
            name.to_string()
        } else {
            text.to_string()
        });
    }
    Ok(PromptStore::from_env()
        .resolve_for_agent(agent_id, name)?
        .map(|prompt| prompt.body)
        .unwrap_or_else(|| {
            if slash_invocation {
                name.to_string()
            } else {
                text.to_string()
            }
        }))
}

fn ingestion_completed_event(artifact: &IngestionArtifact) -> RunEventKind {
    RunEventKind::IngestionCompleted {
        artifact_id: artifact.id.clone(),
        content_hash: artifact.content_hash.clone(),
        sections: artifact.sections.len() as u32,
        findings: artifact.finding_summaries(),
        high_risk_findings: artifact.high_risk_finding_count() as u32,
        finding_snippets: artifact.finding_source_snippets(),
    }
}

pub(crate) fn record_memory_written(record: &MemoryRecord, operation: &str) -> anyhow::Result<()> {
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
    ToolManual {
        name: String,
        input: String,
    },
    ToolForced {
        name: String,
        prompt: String,
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
        return Ok(Some(SlashCommand::ToolManual { name, input }));
    }
    if let Some(rest) = trimmed.strip_prefix("/tool ").map(str::trim) {
        let (name, prompt) = parse_forced_tool_slash_rest(rest)?;
        return Ok(Some(SlashCommand::ToolForced { name, prompt }));
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

fn parse_forced_tool_slash_rest(rest: &str) -> anyhow::Result<(String, String)> {
    let (name, prompt) = rest
        .trim()
        .split_once(char::is_whitespace)
        .map(|(name, prompt)| (name.trim().to_string(), prompt.trim().to_string()))
        .unwrap_or_else(|| (rest.trim().to_string(), String::new()));
    if name.is_empty() {
        anyhow::bail!("missing tool name");
    }
    Ok((name, prompt))
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
    fn guardrail_provider_accepts_native_runtime_configs() {
        let anthropic = ModelRuntimeConfig {
            provider: Some("anthropic".into()),
            allow_missing_api_key: Some(true),
            ..ModelRuntimeConfig::default()
        };
        assert!(guardrail_provider_for_model("claude-sonnet-4-5", &anthropic).is_ok());

        let gemini = ModelRuntimeConfig {
            provider: Some("gemini".into()),
            allow_missing_api_key: Some(true),
            ..ModelRuntimeConfig::default()
        };
        assert!(guardrail_provider_for_model("gemini-2.5-flash", &gemini).is_ok());
    }

    #[test]
    fn agent_config_from_parts_preserves_compaction_policy() {
        let agent = agent_config_from_parts(
            "critic".into(),
            None,
            "Review carefully.".into(),
            Some("fake-model".into()),
            Some(1),
            Some(128),
            Some(48),
            Some(" keep decisions ".into()),
            None,
            None,
            Vec::new(),
            Vec::new(),
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            false,
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
        )
        .unwrap();

        assert_eq!(agent.max_tokens_before_compaction, Some(128));
        assert_eq!(agent.max_compaction_output_tokens, Some(48));
        assert_eq!(agent.compaction_guidance.as_deref(), Some("keep decisions"));
    }

    #[test]
    fn latest_auto_compaction_snapshot_requires_auto_provenance() {
        let store = agent_tracing::InMemoryEventStore::new();
        let run_id = RunId::new();
        store.append(
            run_id,
            None,
            RunEventKind::ContextBuilt {
                snapshot: serde_json::json!({
                    "system_prompt": "system",
                    "conversation": [],
                    "compacted": "<auto-compaction>summary</auto-compaction>",
                    "loaded_memory": [],
                    "loaded_artifacts": [],
                    "visible_tools": [],
                    "visible_skills": [],
                    "limits": {
                        "max_tool_calls": 5,
                        "remaining_tool_calls": 5
                    },
                    "estimated_input_tokens": 12,
                    "provenance": [{
                        "fragment": "compacted_context",
                        "source": "run.manual_compaction"
                    }]
                }),
            },
        );
        assert!(latest_auto_compaction_snapshot(&store.events(run_id)).is_none());

        store.append(
            run_id,
            None,
            RunEventKind::ContextBuilt {
                snapshot: serde_json::json!({
                    "system_prompt": "system",
                    "conversation": [],
                    "compacted": "<auto-compaction>summary</auto-compaction>",
                    "loaded_memory": [],
                    "loaded_artifacts": [],
                    "visible_tools": [],
                    "visible_skills": [],
                    "limits": {
                        "max_tool_calls": 5,
                        "remaining_tool_calls": 5
                    },
                    "estimated_input_tokens": 12,
                    "provenance": [{
                        "fragment": "compacted_context",
                        "source": "agent.context_policy.auto_compaction"
                    }]
                }),
            },
        );

        let snapshot =
            latest_auto_compaction_snapshot(&store.events(run_id)).expect("auto compaction");
        assert_eq!(
            snapshot.compacted.as_deref(),
            Some("<auto-compaction>summary</auto-compaction>")
        );
    }

    #[test]
    fn model_config_merges_typed_provider_options() {
        let model = model_config_from_parts(
            "typed-provider-options".into(),
            Some("anthropic".into()),
            None,
            None,
            false,
            None,
            None,
            None,
            Vec::new(),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(0.7),
            Some(40),
            Some("high".into()),
            Some(r#"{"provider_options":{"existing":true},"owner":"test"}"#.into()),
        )
        .unwrap();

        assert_eq!(
            model.metadata.get("owner"),
            Some(&serde_json::json!("test"))
        );
        assert_eq!(
            model.metadata.get("provider_options"),
            Some(&serde_json::json!({
                "existing": true,
                "top_p": 0.7,
                "top_k": 40,
                "reasoning_effort": "high"
            }))
        );
    }

    #[test]
    fn model_config_rejects_invalid_typed_provider_options() {
        let err = model_config_from_parts(
            "bad-provider-options".into(),
            None,
            None,
            None,
            false,
            None,
            None,
            None,
            Vec::new(),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(1.5),
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("--top-p"));
    }

    #[test]
    fn parses_direct_tool_without_space_after_bang() {
        let parsed = parse_slash_command(r#"/tool!echo {"text":"hi"}"#).unwrap();
        match parsed {
            Some(SlashCommand::ToolManual { name, input }) => {
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
            Some(SlashCommand::ToolManual { name, input }) => {
                assert_eq!(name, "echo");
                assert_eq!(input, "{}");
            }
            _ => panic!("expected tool command"),
        }
    }

    #[test]
    fn parses_forced_tool_with_llm_filled_prompt() {
        let parsed = parse_slash_command("/tool echo summarize hello").unwrap();
        match parsed {
            Some(SlashCommand::ToolForced { name, prompt }) => {
                assert_eq!(name, "echo");
                assert_eq!(prompt, "summarize hello");
            }
            _ => panic!("expected forced tool command"),
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
            RunEventKind::HookFired {
                hook_id: "audit".into(),
                trigger: "run_started".into(),
                payload_digest: "abc123".into(),
            },
        );
        store.append(
            run_id,
            None,
            RunEventKind::HookFailed {
                hook_id: "audit".into(),
                trigger: "run_started".into(),
                error: "temporary failure".into(),
                attempt: 1,
                will_retry: false,
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
        assert_eq!(summary.events, 9);
        assert_eq!(summary.context_snapshots, 1);
        assert_eq!(summary.llm_calls, 1);
        assert_eq!(summary.tool_calls, 1);
        assert_eq!(summary.tokens_in, 100);
        assert_eq!(summary.tokens_out, 25);
        assert_eq!(summary.cost_usd, Some(0.01));
        assert_eq!(summary.duration_ms, Some(99));
        assert_eq!(summary.memory_fragments, 2);
        assert_eq!(summary.artifact_refs, 1);
        assert_eq!(summary.hooks, 1);
        assert_eq!(summary.hook_failures, 1);
        assert_eq!(summary.quality_scores, 1);
    }
}
