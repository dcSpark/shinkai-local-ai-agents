//! `agent-tracing` — RunEvent stream and event store.
//!
//! See `specs/architecture.md` §4.7 (RunEvent), §15 (storage and durability).
//! v0 ships an in-memory store and a `PublishingEventStore` wrapper that
//! forwards every appended event over a `tokio` channel for live UIs.
//! SQLite-backed durability is available for the CLI, daemon, and Tauri app.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;

/// Schema version embedded in every emitted event. Bump on breaking changes.
/// See `specs/architecture.md` §4.7 / §15.3.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(pub u64);

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumePlan {
    pub source_run_id: RunId,
    pub agent_id: String,
    pub original_input: String,
    pub selected_event_id: EventId,
    pub omitted_events: usize,
    pub prompt: String,
}

/// Subset of `RunEventKind` from `specs/architecture.md` §4.7. The enum is
/// append-only; older serialized traces continue to deserialize as variants
/// gain optional fields or new variants are added.
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_digest: Option<String>,
    },
    LlmStreamToken {
        delta: String,
    },
    LlmRequestCompleted {
        tokens_in: u32,
        tokens_out: u32,
        #[serde(default)]
        cost_usd: Option<f64>,
        duration_ms: u64,
    },
    PromptRefinementStarted {
        model: String,
        original_input: String,
        instructions: String,
    },
    PromptRefinementCompleted {
        refined_input: String,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permissions: Option<serde_json::Value>,
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
    ToolOutputInterpreted {
        call_id: String,
        model: String,
        summary: String,
    },
    ToolCallFailed {
        call_id: String,
        error: String,
    },
    ApprovalRequested {
        approval_id: String,
        action: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        controller_agent: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        controller_scope: Vec<String>,
    },
    ApprovalResolved {
        approval_id: String,
        approved: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delegated_controller: Option<String>,
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
    MemoryRead {
        backend: String,
        fragment_ids: Vec<String>,
    },
    MemoryWritten {
        id: String,
        operation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_range: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generating_model: Option<String>,
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
        #[serde(default)]
        findings: Vec<String>,
        #[serde(default)]
        high_risk_findings: u32,
        #[serde(default)]
        finding_snippets: Vec<String>,
    },
    HookFired {
        hook_id: String,
        trigger: String,
        payload_digest: String,
    },
    HookFailed {
        hook_id: String,
        trigger: String,
        error: String,
        attempt: u32,
        will_retry: bool,
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

#[derive(Debug, thiserror::Error)]
pub enum TraceValidationError {
    #[error("quality score must be a finite number from 0 to 10, got {score}")]
    QualityScoreOutOfRange { score: f32 },
    #[error("guidance content must not be empty")]
    EmptyGuidance,
}

#[derive(Debug, thiserror::Error)]
pub enum ResumePlanError {
    #[error("run {0} has no trace events")]
    EmptyTrace(RunId),
    #[error("run {0} has no RunStarted event")]
    MissingRunStarted(RunId),
    #[error("event {event_id} was not found in run {run_id}")]
    EventNotFound { run_id: RunId, event_id: EventId },
}

pub fn validate_quality_score(score: f32) -> Result<(), TraceValidationError> {
    if score.is_finite() && (0.0..=10.0).contains(&score) {
        Ok(())
    } else {
        Err(TraceValidationError::QualityScoreOutOfRange { score })
    }
}

pub fn validate_guidance_content(content: &str) -> Result<String, TraceValidationError> {
    let content = content.trim();
    if content.is_empty() {
        Err(TraceValidationError::EmptyGuidance)
    } else {
        Ok(content.to_string())
    }
}

pub fn latest_event_id(events: &[RunEvent]) -> Option<EventId> {
    events.last().map(|event| event.id)
}

pub fn build_resume_plan(
    run_id: RunId,
    events: &[RunEvent],
    from_event: Option<EventId>,
) -> Result<ResumePlan, ResumePlanError> {
    if events.is_empty() {
        return Err(ResumePlanError::EmptyTrace(run_id));
    }
    let (agent_id, original_input) = events
        .iter()
        .find_map(|event| match &event.kind {
            RunEventKind::RunStarted { agent_id, input } => Some((agent_id.clone(), input.clone())),
            _ => None,
        })
        .ok_or(ResumePlanError::MissingRunStarted(run_id))?;
    let selected_index = if let Some(event_id) = from_event {
        events
            .iter()
            .position(|event| event.id == event_id)
            .ok_or(ResumePlanError::EventNotFound { run_id, event_id })?
    } else {
        events
            .iter()
            .rposition(|event| !is_terminal_run_event(&event.kind))
            .unwrap_or(events.len() - 1)
    };
    let selected_event_id = events[selected_index].id;
    let selected_events = &events[..=selected_index];
    let omitted_events = selected_events.len().saturating_sub(80);
    let excerpt_events = &selected_events[omitted_events..];
    let mut excerpt = String::new();
    if omitted_events > 0 {
        excerpt.push_str(&format!(
            "... omitted {omitted_events} earlier trace event(s) ...\n"
        ));
    }
    for event in excerpt_events {
        excerpt.push_str(&resume_event_line(event));
        excerpt.push('\n');
    }
    let prompt = format!(
        "Resume stopped run {run_id} from saved event {}.\n\nOriginal user input:\n{}\n\nSaved trace through event {}:\n{}\nContinue from the selected step. Avoid repeating completed successful tool calls unless they are needed to finish the task.",
        selected_event_id.0,
        original_input.trim(),
        selected_event_id.0,
        excerpt.trim_end()
    );

    Ok(ResumePlan {
        source_run_id: run_id,
        agent_id,
        original_input,
        selected_event_id,
        omitted_events,
        prompt,
    })
}

fn resume_event_line(event: &RunEvent) -> String {
    let parent = event
        .parent_event
        .map(|id| id.0.to_string())
        .unwrap_or_else(|| "-".into());
    format!(
        "[{}] parent={} {}",
        event.id.0,
        parent,
        resume_event_label(&event.kind)
    )
}

fn resume_event_label(kind: &RunEventKind) -> String {
    match kind {
        RunEventKind::RunStarted { agent_id, input } => {
            format!("RunStarted agent={agent_id} input={input:?}")
        }
        RunEventKind::ContextBuilt { snapshot } => {
            let tools = snapshot
                .get("visible_tools")
                .and_then(|value| value.as_array())
                .map_or(0, Vec::len);
            let skills = snapshot
                .get("visible_skills")
                .and_then(|value| value.as_array())
                .map_or(0, Vec::len);
            format!("ContextBuilt visible_tools={tools} visible_skills={skills}")
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
            ..
        } => format!("PromptRefinementStarted model={model} input={original_input:?}"),
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
            ..
        } => format!("ToolCallProposed call={call_id} tool={tool_id} input={input}"),
        RunEventKind::ToolCallStarted { call_id } => format!("ToolCallStarted call={call_id}"),
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
        RunEventKind::MemoryLoaded { ids } => format!("MemoryLoaded ids={}", ids.join(",")),
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
            let finding_count = if findings.is_empty() {
                String::new()
            } else {
                format!(" findings={}", findings.len())
            };
            let snippet_count = if finding_snippets.is_empty() {
                String::new()
            } else {
                format!(" snippets={}", finding_snippets.len())
            };
            format!(
                "IngestionCompleted artifact={artifact_id} hash={content_hash} sections={sections}{risk}{finding_count}{snippet_count}"
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

pub fn is_terminal_run_event(kind: &RunEventKind) -> bool {
    matches!(
        kind,
        RunEventKind::RunPaused { .. }
            | RunEventKind::RunCancelled { .. }
            | RunEventKind::RunCompleted { .. }
            | RunEventKind::RunFailed { .. }
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceSummary {
    pub run_id: RunId,
    pub events: usize,
    pub context_snapshots: u32,
    pub llm_calls: u32,
    pub tool_calls: u32,
    pub approvals: u32,
    pub guidance_injections: u32,
    pub quality_scores: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score_average: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score_min: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_score_max: Option<f32>,
    pub memory_fragments: u32,
    pub artifact_refs: u32,
    #[serde(default)]
    pub hooks: u32,
    #[serde(default)]
    pub hook_failures: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookRemediation {
    pub event_id: EventId,
    pub hook_id: String,
    pub trigger: String,
    pub error: String,
    pub attempt: u32,
    pub will_retry: bool,
    pub final_failure: bool,
    #[serde(default)]
    pub policy_denials: Vec<String>,
    #[serde(default)]
    pub suggested_actions: Vec<String>,
}

impl TraceSummary {
    pub fn empty(run_id: RunId) -> Self {
        Self {
            run_id,
            events: 0,
            context_snapshots: 0,
            llm_calls: 0,
            tool_calls: 0,
            approvals: 0,
            guidance_injections: 0,
            quality_scores: 0,
            quality_score_average: None,
            quality_score_min: None,
            quality_score_max: None,
            memory_fragments: 0,
            artifact_refs: 0,
            hooks: 0,
            hook_failures: 0,
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: None,
            duration_ms: None,
        }
    }
}

pub fn hook_remediation_plan(events: &[RunEvent]) -> Vec<HookRemediation> {
    let mut denials_by_parent = BTreeMap::<EventId, Vec<String>>::new();
    for event in events {
        if let RunEventKind::PolicyDenied { reason } = &event.kind
            && let Some(parent) = event.parent_event
        {
            denials_by_parent
                .entry(parent)
                .or_default()
                .push(reason.clone());
        }
    }

    events
        .iter()
        .filter_map(|event| {
            let RunEventKind::HookFailed {
                hook_id,
                trigger,
                error,
                attempt,
                will_retry,
            } = &event.kind
            else {
                return None;
            };
            let final_failure = !will_retry;
            let mut suggested_actions = Vec::new();
            if *will_retry {
                suggested_actions.push(
                    "Wait for the configured hook retry; review the final attempt if it fails."
                        .into(),
                );
            } else {
                suggested_actions.push(format!(
                    "Fix or disable hook `{hook_id}` in the adapter configuration, then replay the run."
                ));
                suggested_actions.push(
                    "Use a one-run lifecycle-hook override only after accepting the skipped enforcement."
                        .into(),
                );
            }
            Some(HookRemediation {
                event_id: event.id,
                hook_id: hook_id.clone(),
                trigger: trigger.clone(),
                error: error.clone(),
                attempt: *attempt,
                will_retry: *will_retry,
                final_failure,
                policy_denials: event
                    .parent_event
                    .and_then(|parent| denials_by_parent.get(&parent).cloned())
                    .unwrap_or_default(),
                suggested_actions,
            })
        })
        .collect()
}

pub fn summarize_trace(events: &[RunEvent], fallback_run_id: RunId) -> TraceSummary {
    let run_id = events
        .first()
        .map(|event| event.run_id)
        .unwrap_or(fallback_run_id);
    let mut summary = TraceSummary::empty(run_id);
    summary.events = events.len();
    let mut event_cost_usd = 0.0;
    let mut has_event_cost = false;
    let mut completed_cost_usd = None;
    let mut quality_score_total = 0.0f32;

    for event in events {
        match &event.kind {
            RunEventKind::ContextBuilt { .. } => summary.context_snapshots += 1,
            RunEventKind::LlmStreamToken { .. } => {}
            RunEventKind::LlmRequestCompleted {
                tokens_in,
                tokens_out,
                cost_usd,
                ..
            } => {
                summary.llm_calls += 1;
                summary.tokens_in = summary.tokens_in.saturating_add(*tokens_in);
                summary.tokens_out = summary.tokens_out.saturating_add(*tokens_out);
                if let Some(cost) = cost_usd {
                    event_cost_usd += cost;
                    has_event_cost = true;
                }
            }
            RunEventKind::PromptRefinementCompleted {
                tokens_in,
                tokens_out,
                cost_usd,
                ..
            } => {
                summary.tokens_in = summary.tokens_in.saturating_add(*tokens_in);
                summary.tokens_out = summary.tokens_out.saturating_add(*tokens_out);
                if let Some(cost) = cost_usd {
                    event_cost_usd += cost;
                    has_event_cost = true;
                }
            }
            RunEventKind::ToolCallCompleted { cost_usd, .. } => {
                summary.tool_calls += 1;
                if let Some(cost) = cost_usd {
                    event_cost_usd += cost;
                    has_event_cost = true;
                }
            }
            RunEventKind::ApprovalRequested { .. } => summary.approvals += 1,
            RunEventKind::GuidanceInjected { .. } => summary.guidance_injections += 1,
            RunEventKind::QualityScored { score, .. } => {
                summary.quality_scores += 1;
                quality_score_total += *score;
                summary.quality_score_min = Some(
                    summary
                        .quality_score_min
                        .map(|current| current.min(*score))
                        .unwrap_or(*score),
                );
                summary.quality_score_max = Some(
                    summary
                        .quality_score_max
                        .map(|current| current.max(*score))
                        .unwrap_or(*score),
                );
            }
            RunEventKind::MemoryLoaded { ids } => {
                summary.memory_fragments =
                    summary.memory_fragments.saturating_add(ids.len() as u32);
            }
            RunEventKind::MemoryRead { fragment_ids, .. } => {
                summary.memory_fragments = summary
                    .memory_fragments
                    .saturating_add(fragment_ids.len() as u32);
            }
            RunEventKind::IngestionReferenced { .. } => summary.artifact_refs += 1,
            RunEventKind::HookFired { .. } => summary.hooks += 1,
            RunEventKind::HookFailed { .. } => summary.hook_failures += 1,
            RunEventKind::RunCompleted {
                total_cost_usd,
                total_duration_ms,
                ..
            } => {
                completed_cost_usd = *total_cost_usd;
                summary.duration_ms = Some(*total_duration_ms);
            }
            _ => {}
        }
    }

    if summary.quality_scores > 0 {
        summary.quality_score_average = Some(quality_score_total / summary.quality_scores as f32);
    }
    summary.cost_usd = completed_cost_usd.or_else(|| has_event_cost.then_some(event_cost_usd));
    summary
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
/// and the `--print` CLI mode. Persistent clients use
/// [`SqliteEventStore`] (see `specs/architecture.md` §15).
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
    fn resume_plan_uses_original_input_and_selected_step() {
        let store = InMemoryEventStore::new();
        let run = RunId::new();
        let started = store.append(
            run,
            None,
            RunEventKind::RunStarted {
                agent_id: "researcher".into(),
                input: "finish report".into(),
            },
        );
        let proposed = store.append(
            run,
            Some(started.id),
            RunEventKind::ToolCallProposed {
                call_id: "tool-1".into(),
                tool_id: "echo".into(),
                input: serde_json::json!({"text": "draft"}),
                model: Some("fake".into()),
                permissions: None,
            },
        );
        store.append(
            run,
            Some(proposed.id),
            RunEventKind::RunCancelled {
                reason: "user requested stop".into(),
            },
        );

        let plan = build_resume_plan(run, &store.events(run), Some(proposed.id)).unwrap();

        assert_eq!(plan.agent_id, "researcher");
        assert_eq!(plan.original_input, "finish report");
        assert_eq!(plan.selected_event_id, proposed.id);
        assert!(plan.prompt.contains("finish report"));
        assert!(plan.prompt.contains("ToolCallProposed"));
        assert!(!plan.prompt.contains("RunCancelled"));
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
                model: Some("fake-model".into()),
                permissions: Some(serde_json::json!({"shell": false})),
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
                model,
                permissions,
            } => {
                assert_eq!(call_id, "c1");
                assert_eq!(tool_id, "echo");
                assert_eq!(input, serde_json::json!({"x": 1}));
                assert_eq!(model.as_deref(), Some("fake-model"));
                assert_eq!(permissions, Some(serde_json::json!({"shell": false})));
            }
            other => panic!("unexpected kind: {other:?}"),
        }
    }

    #[test]
    fn tool_call_proposed_reads_older_trace_shape() {
        let kind: RunEventKind = serde_json::from_str(
            r#"{"type":"ToolCallProposed","call_id":"c1","tool_id":"echo","input":{"x":1}}"#,
        )
        .unwrap();
        match kind {
            RunEventKind::ToolCallProposed {
                model, permissions, ..
            } => {
                assert_eq!(model, None);
                assert_eq!(permissions, None);
            }
            other => panic!("unexpected kind: {other:?}"),
        }
    }

    #[test]
    fn llm_request_started_reads_older_trace_shape() {
        let kind: RunEventKind =
            serde_json::from_str(r#"{"type":"LlmRequestStarted","model":"fake-model"}"#).unwrap();
        match kind {
            RunEventKind::LlmRequestStarted {
                model,
                request_digest,
            } => {
                assert_eq!(model, "fake-model");
                assert_eq!(request_digest, None);
            }
            other => panic!("unexpected kind: {other:?}"),
        }
    }

    #[test]
    fn memory_written_event_round_trips_provenance() {
        let store = InMemoryEventStore::new();
        let r = RunId::new();
        store.append(
            r,
            None,
            RunEventKind::MemoryWritten {
                id: "mem-1".into(),
                operation: "generated".into(),
                source_range: Some("turns 2-4".into()),
                generating_model: Some("manual-memory-generator-v0".into()),
            },
        );
        let evts = store.events(r);
        let json = serde_json::to_string(&evts[0]).unwrap();
        let back: RunEvent = serde_json::from_str(&json).unwrap();
        match back.kind {
            RunEventKind::MemoryWritten {
                id,
                operation,
                source_range,
                generating_model,
            } => {
                assert_eq!(id, "mem-1");
                assert_eq!(operation, "generated");
                assert_eq!(source_range.as_deref(), Some("turns 2-4"));
                assert_eq!(
                    generating_model.as_deref(),
                    Some("manual-memory-generator-v0")
                );
            }
            other => panic!("unexpected kind: {other:?}"),
        }
    }

    #[test]
    fn memory_written_event_reads_older_trace_shape() {
        let kind: RunEventKind =
            serde_json::from_str(r#"{"type":"MemoryWritten","id":"mem-1","operation":"created"}"#)
                .unwrap();
        match kind {
            RunEventKind::MemoryWritten {
                source_range,
                generating_model,
                ..
            } => {
                assert_eq!(source_range, None);
                assert_eq!(generating_model, None);
            }
            other => panic!("unexpected kind: {other:?}"),
        }
    }

    #[test]
    fn ingestion_completed_event_reads_older_trace_shape() {
        let kind: RunEventKind = serde_json::from_str(
            r#"{"type":"IngestionCompleted","artifact_id":"ingest-1","content_hash":"abc","sections":2}"#,
        )
        .unwrap();
        match kind {
            RunEventKind::IngestionCompleted {
                findings,
                high_risk_findings,
                finding_snippets,
                ..
            } => {
                assert!(findings.is_empty());
                assert_eq!(high_risk_findings, 0);
                assert!(finding_snippets.is_empty());
            }
            other => panic!("unexpected kind: {other:?}"),
        }
    }

    #[test]
    fn quality_scores_are_limited_to_zero_through_ten() {
        assert!(validate_quality_score(0.0).is_ok());
        assert!(validate_quality_score(7.5).is_ok());
        assert!(validate_quality_score(10.0).is_ok());

        assert!(validate_quality_score(-0.1).is_err());
        assert!(validate_quality_score(10.1).is_err());
        assert!(validate_quality_score(f32::NAN).is_err());
        assert!(validate_quality_score(f32::INFINITY).is_err());
    }

    #[test]
    fn guidance_content_must_be_non_empty() {
        assert_eq!(
            validate_guidance_content("  course correct  ").unwrap(),
            "course correct"
        );
        assert!(validate_guidance_content("").is_err());
        assert!(validate_guidance_content("   \n\t  ").is_err());
    }

    #[test]
    fn latest_event_id_returns_tail_event() {
        let store = InMemoryEventStore::new();
        let run = RunId::new();
        assert_eq!(latest_event_id(&store.events(run)), None);
        let first = store.append(
            run,
            None,
            RunEventKind::RunStarted {
                agent_id: "a".into(),
                input: "x".into(),
            },
        );
        let second = store.append(
            run,
            Some(first.id),
            RunEventKind::GuidanceInjected {
                content: "course correct".into(),
            },
        );

        assert_eq!(latest_event_id(&store.events(run)), Some(second.id));
    }

    #[test]
    fn terminal_run_events_are_identified() {
        assert!(is_terminal_run_event(&RunEventKind::RunPaused {
            reason: "approval".into()
        }));
        assert!(is_terminal_run_event(&RunEventKind::RunCancelled {
            reason: "stop".into()
        }));
        assert!(is_terminal_run_event(&RunEventKind::RunCompleted {
            final_output: "done".into(),
            total_cost_usd: None,
            total_duration_ms: 1,
        }));
        assert!(is_terminal_run_event(&RunEventKind::RunFailed {
            reason: "error".into()
        }));
        assert!(!is_terminal_run_event(&RunEventKind::RunStarted {
            agent_id: "agent".into(),
            input: "task".into(),
        }));
    }

    #[test]
    fn summarizes_trace_observability_counters() {
        let run = RunId::new();
        let store = InMemoryEventStore::new();
        store.append(
            run,
            None,
            RunEventKind::ContextBuilt {
                snapshot: serde_json::json!({}),
            },
        );
        store.append(
            run,
            None,
            RunEventKind::LlmRequestCompleted {
                tokens_in: 100,
                tokens_out: 25,
                cost_usd: Some(0.001),
                duration_ms: 40,
            },
        );
        store.append(
            run,
            None,
            RunEventKind::ToolCallCompleted {
                call_id: "call-1".into(),
                output: serde_json::json!({"ok": true}),
                cost_usd: Some(0.002),
                duration_ms: 5,
            },
        );
        store.append(
            run,
            None,
            RunEventKind::MemoryRead {
                backend: "local-v0".into(),
                fragment_ids: vec!["mem-1".into(), "mem-2".into()],
            },
        );
        store.append(
            run,
            None,
            RunEventKind::IngestionReferenced {
                artifact_id: "ing-1".into(),
                source: "/tmp/doc.txt".into(),
            },
        );
        store.append(
            run,
            None,
            RunEventKind::HookFired {
                hook_id: "audit".into(),
                trigger: "run_started".into(),
                payload_digest: "abc123".into(),
            },
        );
        store.append(
            run,
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
            run,
            None,
            RunEventKind::QualityScored {
                target: "last_answer".into(),
                score: 8.0,
            },
        );
        store.append(
            run,
            None,
            RunEventKind::RunCompleted {
                final_output: "done".into(),
                total_cost_usd: Some(0.01),
                total_duration_ms: 99,
            },
        );

        let summary = summarize_trace(&store.events(run), run);

        assert_eq!(summary.run_id, run);
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
        assert_eq!(summary.quality_score_average, Some(8.0));
        assert_eq!(summary.quality_score_min, Some(8.0));
        assert_eq!(summary.quality_score_max, Some(8.0));
    }

    #[test]
    fn hook_remediation_plan_links_failures_denials_and_actions() {
        let run = RunId::new();
        let store = InMemoryEventStore::new();
        let fired = store.append(
            run,
            None,
            RunEventKind::HookFired {
                hook_id: "guard".into(),
                trigger: "before_tool_call".into(),
                payload_digest: "abc123".into(),
            },
        );
        let failed = store.append(
            run,
            Some(fired.id),
            RunEventKind::HookFailed {
                hook_id: "guard".into(),
                trigger: "before_tool_call".into(),
                error: "blocked".into(),
                attempt: 2,
                will_retry: false,
            },
        );
        store.append(
            run,
            Some(fired.id),
            RunEventKind::PolicyDenied {
                reason: "hook guard tool call mutation failed: blocked".into(),
            },
        );

        let plan = hook_remediation_plan(&store.events(run));

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].event_id, failed.id);
        assert_eq!(plan[0].hook_id, "guard");
        assert_eq!(plan[0].trigger, "before_tool_call");
        assert_eq!(plan[0].attempt, 2);
        assert!(plan[0].final_failure);
        assert_eq!(plan[0].policy_denials.len(), 1);
        assert!(
            plan[0]
                .suggested_actions
                .iter()
                .any(|action| action.contains("lifecycle-hook override"))
        );
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
