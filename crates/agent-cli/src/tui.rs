//! ratatui-based TUI surface. v0 ships:
//! - transcript pane (User / Assistant / Event / Error lines, color-coded)
//! - status bar (run state, tokens, tool-call budget)
//! - input box (Enter to send, Esc / Ctrl+C to stop active runs or quit when idle)
//! - live event streaming from the harness via `PublishingEventStore`
//!
//! Some advanced slash-command grammar and the tool-tray pane from
//! `specs/architecture.md` §20.1 land in later slices.

use std::io::{self, Stdout};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{Event as CtEvent, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::AbortHandle;
use tokio::time::MissedTickBehavior;

use agent_adapters::{AdapterRegistry, ClawHubProvider, NormalizedPackage, inspect_source};
use agent_batch::{BatchItemState, BatchPlan, prepare_batch_inputs};
use agent_bundles::{export_bundle, import_bundle};
use agent_capabilities::{
    CapabilityDraft, CapabilityDraftDoctorReport, CapabilityDraftInput, CapabilityDraftStatus,
    CapabilityDraftStore, CapabilityKind,
};
use agent_compaction::{CompactionRecord, CompactionStore};
use agent_config::{
    AgentConfigFile, AgentSummary, ConfigResolver, ModelConfig, ProfileGrantKind, ProfileSummary,
    configured_model_providers,
};
use agent_conversations::{
    ConversationMessage, ConversationPolicy, ConversationStore, ConversationTreeNode,
    ConversationUsageReport, build_conversation_usage_report, render_message_range,
};
use agent_core::{
    AgentConfig, ContextSnapshot, Harness, HarnessApi, StopRetentionMode, UserInput,
    VisibilityLevel,
};
use agent_ingest::{
    IngestionArtifact, IngestionFindingReviewDecision, IngestionStore,
    supported_backends as supported_ingestion_backends,
};
use agent_memory::{
    MemoryAuthor, MemoryRecord, MemoryStore, MemoryTarget,
    create_record_for_active_backend_with_topics_for_agent, delete_record_for_active_backend,
    delete_records_by_source_conversation_ids_for_active_backend,
    delete_records_by_source_conversation_message_range_for_active_backend,
    edit_record_for_active_backend, export_target_for_active_backend,
    generate_records_for_active_backend_with_topics_for_agent_and_guidance,
    import_file_for_active_backend_for_agent,
    list_record_ids_by_source_conversation_message_range_for_active_backend,
    list_records_for_active_backend, probe_backend as probe_memory_backend,
    rollback_active_backend, supported_backends as supported_memory_backends,
};
use agent_prompts::{PromptDoc, PromptStore, is_valid_prompt_name};
use agent_secrets::{
    SecretId, SecretValue, default_secret_store, supported_backends as supported_secret_backends,
};
use agent_skills::{SkillDoc, SkillRegistry};
use agent_storage::{StoragePaths, StorageReport};
use agent_tools::{
    ArtifactGenerateInput, GeneratedArtifact, ToolId, ToolRegistry,
    delete_generated_artifact_from_env, delete_generated_artifacts_by_ids_from_env,
    export_generated_artifact_from_env, generate_artifact_from_env,
    generated_artifact_data_url_from_env, generated_artifact_ids_in_value,
    list_generated_artifacts_from_env, open_generated_artifact_from_env,
    show_generated_artifact_from_env,
};
use agent_tracing::{
    EventId, EventStore, PublishingEventStore, RunEvent, RunEventKind, RunId, SqliteEventStore,
    TraceComparison, TraceRunRecord, TraceSummary, TraceTreeNode, build_resume_plan,
    build_trace_comparison, build_trace_tree, hook_remediation_plan, is_terminal_run_event,
    latest_event_id, quality_score_records, summarize_trace, validate_guidance_content,
    validate_quality_score,
};

use crate::{Demo, setup};

/// RAII guard ensuring the terminal is restored even on panic.
struct TerminalGuard;

impl TerminalGuard {
    fn install() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

#[derive(Default)]
struct App {
    transcript: Vec<TranscriptLine>,
    input: String,
    state: AppState,
    tokens_in: u32,
    tokens_out: u32,
    cost_usd: f64,
    calls_used: u32,
    calls_max: u32,
    calls_remaining: u32,
    elapsed_ms: u64,
    run_started_at: Option<Instant>,
    active_run_handle: Option<AbortHandle>,
    last_run_id: Option<RunId>,
    active_batch_run_id: Option<RunId>,
    loaded_trace_run_id: Option<RunId>,
    pending_replay_comparison: Option<ReplayComparisonRequest>,
    pending_auto_compaction_run: Option<RunId>,
    selected_conversation_id: Option<String>,
    conversation_tree_index: Vec<String>,
    conversation_browser: Option<ConversationBrowser>,
    pending_conversation_action: Option<PendingConversationAction>,
    quit: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ConversationBrowser {
    rows: Vec<ConversationBrowserRow>,
    selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConversationBrowserRow {
    id: String,
    title: String,
    agent_id: String,
    depth: usize,
    branch_point: Option<usize>,
    topic_preview: Option<String>,
    own_message_count: usize,
    expanded_message_count: usize,
    branch_reason: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AppState {
    #[default]
    Idle,
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingConversationAction {
    Delete {
        id: String,
        recursive: bool,
        delete_ids: Vec<String>,
    },
    DeleteMany {
        ids: Vec<String>,
        recursive: bool,
        delete_ids: Vec<String>,
    },
    DeleteAgent {
        agent: String,
        recursive: bool,
        delete_ids: Vec<String>,
    },
    DeleteRange {
        id: String,
        from: usize,
        to: usize,
        options: crate::headless::ConversationRangeDeleteOptions,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConversationMemoryArgs {
    id: String,
    from: usize,
    to: usize,
    user: bool,
    topics: Vec<String>,
    guidance: Option<String>,
}

#[derive(Clone)]
struct TranscriptLine {
    kind: LineKind,
    text: String,
}

#[derive(Clone, Copy)]
enum LineKind {
    User,
    Assistant,
    Event,
    Error,
}

pub async fn run(
    initial: Option<String>,
    demo: Demo,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let _guard = TerminalGuard::install()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = main_loop(&mut terminal, initial, demo, options).await;

    terminal.show_cursor().ok();
    result
}

async fn main_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    initial: Option<String>,
    demo: Demo,
    options: setup::RuntimeOptions,
) -> anyhow::Result<()> {
    let mut options = options;
    let mut registry = setup::build_registry(
        options.enable_shell,
        options.enable_subagent,
        options.enable_capability_drafts,
        options.agent_id.as_deref(),
        options.conversation_id.as_deref(),
    );
    let mut agent = setup::build_agent(&options);
    let calls_max = agent.tool_policy.max_calls;

    let mut app = App {
        calls_max,
        calls_remaining: calls_max,
        selected_conversation_id: options.conversation_id.clone(),
        ..App::default()
    };
    if let Some(t) = initial {
        app.input = t;
    }
    push_system_line(
        &mut app,
        "Welcome. Type a message and press Enter. Esc to quit.",
    );

    let (publish_tx, mut events_rx) = tokio::sync::mpsc::unbounded_channel::<RunEvent>();
    let (line_tx, mut lines_rx) = tokio::sync::mpsc::unbounded_channel::<TranscriptLine>();
    let mut term_events = EventStream::new();
    let mut status_tick = tokio::time::interval(Duration::from_millis(250));
    status_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        terminal.draw(|f| render(f, &app))?;
        if app.quit {
            return Ok(());
        }

        tokio::select! {
            term = term_events.next() => match term {
                Some(Ok(evt)) => handle_terminal_event(
                    &mut app, evt, demo, &mut registry, &mut agent, &publish_tx,
                    &line_tx, &mut options,
                ),
                Some(Err(_)) | None => app.quit = true,
            },
            evt = events_rx.recv() => {
                if let Some(e) = evt {
                    handle_run_event(&mut app, &e);
                }
            },
            line = lines_rx.recv() => {
                if let Some(line) = line {
                    app.transcript.push(line);
                }
            },
            _ = status_tick.tick() => {
                update_elapsed_time(&mut app);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_terminal_event(
    app: &mut App,
    evt: CtEvent,
    demo: Demo,
    registry: &mut Arc<ToolRegistry>,
    agent: &mut AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
    line_tx: &UnboundedSender<TranscriptLine>,
    options: &mut setup::RuntimeOptions,
) {
    let key = match evt {
        CtEvent::Key(k) if k.kind == KeyEventKind::Press => k,
        _ => return,
    };

    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
        if app.state == AppState::Running {
            stop_active_run(
                app,
                "user requested stop",
                agent.execution_policy.stop_retention_mode,
            );
        } else {
            app.quit = true;
        }
        return;
    }
    if app.conversation_browser.is_some() && handle_conversation_browser_key(app, key.code) {
        return;
    }

    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => {
            if app.state == AppState::Running {
                stop_active_run(
                    app,
                    "user requested stop",
                    agent.execution_policy.stop_retention_mode,
                );
            } else {
                app.quit = true;
            }
        }
        (_, KeyCode::Enter) => {
            let trimmed = app.input.trim();
            if trimmed.is_empty() {
                return;
            }
            if app.state == AppState::Running {
                if is_mid_run_control_command(trimmed) {
                    let prompt = std::mem::take(&mut app.input);
                    handle_slash_command(
                        app, &prompt, demo, registry, agent, publish_tx, line_tx, options,
                    );
                } else {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text:
                            "Run in progress. Use /guide <text>, /guide <last|run-id> <text>, /stop status, or /stop [default|--summarise|--discard] [reason]."
                                .into(),
                    });
                }
                return;
            }
            let prompt = std::mem::take(&mut app.input);
            if handle_slash_command(
                app, &prompt, demo, registry, agent, publish_tx, line_tx, options,
            ) {
                return;
            }
            spawn_run(app, prompt, demo, registry, agent, publish_tx, options);
        }
        (_, KeyCode::Backspace) => {
            app.input.pop();
        }
        (_, KeyCode::Char(c)) => {
            app.input.push(c);
        }
        _ => {}
    }
}

fn spawn_run(
    app: &mut App,
    prompt: String,
    demo: Demo,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let original_prompt = prompt;
    let prompt =
        match resolve_saved_prompt_or_literal(&original_prompt, options.agent_id.as_deref()) {
            Ok(prompt) => prompt,
            Err(err) => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Prompt lookup failed: {err}"),
                });
                return;
            }
        };
    app.transcript.push(TranscriptLine {
        kind: LineKind::User,
        text: if original_prompt == prompt {
            prompt.clone()
        } else {
            format!("{original_prompt}\n\n{prompt}")
        },
    });
    app.state = AppState::Running;
    app.tokens_in = 0;
    app.tokens_out = 0;
    app.cost_usd = 0.0;
    app.calls_used = 0;
    app.calls_max = agent.tool_policy.max_calls;
    app.calls_remaining = agent.tool_policy.max_calls;
    app.elapsed_ms = 0;
    app.run_started_at = Some(Instant::now());

    let provider = match setup::build_provider(demo, &prompt, options) {
        Ok(provider) => provider,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Provider setup failed: {err}"),
            });
            app.state = AppState::Idle;
            return;
        }
    };
    let store = match open_event_store() {
        Ok(store) => PublishingEventStore::new(store, publish_tx.clone()),
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace store setup failed: {err}"),
            });
            app.state = AppState::Idle;
            return;
        }
    };
    let harness = setup::build_harness_for_agent(
        provider,
        Arc::new(store),
        registry.clone(),
        options.agent_id.as_deref(),
    );
    let agent_clone = agent.clone();

    let run_task = tokio::spawn(async move {
        // The harness emits RunFailed before returning Err, so the TUI sees
        // the failure via the event channel; the result here is best-effort.
        let _ = harness.run(&agent_clone, UserInput { text: prompt }).await;
    });
    app.active_run_handle = Some(run_task.abort_handle());
}

fn resolve_saved_prompt_or_literal(text: &str, agent_id: Option<&str>) -> anyhow::Result<String> {
    let trimmed = text.trim();
    let Some(name) = trimmed.strip_prefix("/run ").map(str::trim) else {
        return Ok(text.to_string());
    };
    if !is_valid_prompt_name(name) {
        return Ok(name.to_string());
    }
    Ok(PromptStore::from_env()
        .resolve_for_agent(agent_id, name)?
        .map(|prompt| prompt.body)
        .unwrap_or_else(|| name.to_string()))
}

struct ResumeSlashArgs {
    run_id: RunId,
    from_event: Option<u64>,
}

struct ReplaySlashArgs {
    run_id: RunId,
    no_hooks: bool,
    compare_source: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReplayComparisonRequest {
    source_run_id: RunId,
    replay_run_id: Option<RunId>,
}

fn show_resume_plan(app: &mut App, rest: &str) {
    let args = match parse_resume_slash_args(rest, app.last_run_id) {
        Ok(args) => args,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume plan failed: {err}"),
            });
            return;
        }
    };
    let store = match open_event_store() {
        Ok(store) => store,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume trace lookup failed: {err}"),
            });
            return;
        }
    };
    let source_events = match store.try_events(args.run_id) {
        Ok(events) => events,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume trace lookup failed: {err}"),
            });
            return;
        }
    };
    let plan = match build_resume_plan(args.run_id, &source_events, args.from_event.map(EventId)) {
        Ok(plan) => plan,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume plan failed: {err}"),
            });
            return;
        }
    };

    app.transcript.push(TranscriptLine {
        kind: LineKind::User,
        text: format!(
            "/resume plan {} --from-event {}",
            args.run_id.0, plan.selected_event_id.0
        ),
    });
    app.transcript.push(TranscriptLine {
        kind: LineKind::Event,
        text: format!(
            "Resume plan for {} from event {} as agent {} (omitted {} earlier events)",
            args.run_id.0, plan.selected_event_id.0, plan.agent_id, plan.omitted_events
        ),
    });
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: plan.prompt,
    });
}

fn start_resume_run(
    app: &mut App,
    rest: &str,
    demo: Demo,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let args = match parse_resume_slash_args(rest, app.last_run_id) {
        Ok(args) => args,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume failed: {err}"),
            });
            return;
        }
    };
    let store = match open_event_store() {
        Ok(store) => store,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume trace lookup failed: {err}"),
            });
            return;
        }
    };
    let source_events = match store.try_events(args.run_id) {
        Ok(events) => events,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume trace lookup failed: {err}"),
            });
            return;
        }
    };
    let plan = match build_resume_plan(args.run_id, &source_events, args.from_event.map(EventId)) {
        Ok(plan) => plan,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume plan failed: {err}"),
            });
            return;
        }
    };
    let retained_compaction = match stopped_run_compaction_for_tui(args.run_id) {
        Ok(record) => record,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume compact lookup failed: {err}"),
            });
            return;
        }
    };
    let mut resume_options = options.clone();
    resume_options.agent_id = Some(plan.agent_id.clone());
    resume_options.include_compact = retained_compaction.clone();
    let resume_agent = setup::build_agent(&resume_options);
    let provider = match setup::build_provider(demo, &plan.prompt, &resume_options) {
        Ok(provider) => provider,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume provider setup failed: {err}"),
            });
            return;
        }
    };
    let publishing_store = PublishingEventStore::new(store, publish_tx.clone());
    let registry = setup::build_registry(
        resume_options.enable_shell,
        resume_options.enable_subagent,
        resume_options.enable_capability_drafts,
        resume_options.agent_id.as_deref(),
        resume_options.conversation_id.as_deref(),
    );
    let harness = setup::build_harness_for_agent(
        provider,
        Arc::new(publishing_store),
        registry,
        resume_options.agent_id.as_deref(),
    );
    let prompt = plan.prompt.clone();
    app.transcript.push(TranscriptLine {
        kind: LineKind::User,
        text: format!(
            "/resume {} --from-event {}",
            args.run_id.0, plan.selected_event_id.0
        ),
    });
    app.transcript.push(TranscriptLine {
        kind: LineKind::Event,
        text: format!(
            "Resuming {} from event {} as agent {}{}",
            args.run_id.0,
            plan.selected_event_id.0,
            plan.agent_id,
            retained_compaction
                .as_deref()
                .map(|id| format!(" with compact {id}"))
                .unwrap_or_default()
        ),
    });
    app.state = AppState::Running;
    app.tokens_in = 0;
    app.tokens_out = 0;
    app.cost_usd = 0.0;
    app.calls_used = 0;
    app.calls_max = resume_agent.tool_policy.max_calls;
    app.calls_remaining = resume_agent.tool_policy.max_calls;
    app.elapsed_ms = 0;
    app.run_started_at = Some(Instant::now());
    let run_task = tokio::spawn(async move {
        let _ = harness.run(&resume_agent, UserInput { text: prompt }).await;
    });
    app.active_run_handle = Some(run_task.abort_handle());
}

fn start_replay_run(
    app: &mut App,
    rest: &str,
    demo: Demo,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let args = match parse_replay_slash_args(rest, trace_default_run_id(app)) {
        Ok(args) => args,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Replay failed: {err}"),
            });
            return;
        }
    };
    let store = match open_event_store() {
        Ok(store) => store,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Replay trace lookup failed: {err}"),
            });
            return;
        }
    };
    let source_events = match store.try_events(args.run_id) {
        Ok(events) => events,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Replay trace lookup failed: {err}"),
            });
            return;
        }
    };
    let (agent_id, prompt) = match trace_replay_source(args.run_id, &source_events) {
        Ok(source) => source,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Replay plan failed: {err}"),
            });
            return;
        }
    };
    let mut replay_options = options.clone();
    replay_options.agent_id = Some(agent_id.clone());
    let replay_agent = setup::build_agent(&replay_options);
    let provider = match setup::build_provider(demo, &prompt, &replay_options) {
        Ok(provider) => provider,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Replay provider setup failed: {err}"),
            });
            return;
        }
    };
    let publishing_store = PublishingEventStore::new(store, publish_tx.clone());
    let registry = setup::build_registry(
        replay_options.enable_shell,
        replay_options.enable_subagent,
        replay_options.enable_capability_drafts,
        replay_options.agent_id.as_deref(),
        replay_options.conversation_id.as_deref(),
    );
    let harness = if args.no_hooks {
        Harness::new(provider, Arc::new(publishing_store), registry)
    } else {
        setup::build_harness_for_agent(
            provider,
            Arc::new(publishing_store),
            registry,
            replay_options.agent_id.as_deref(),
        )
    };
    app.transcript.push(TranscriptLine {
        kind: LineKind::User,
        text: format!("/replay {}{}", args.run_id.0, replay_flags_text(&args)),
    });
    app.transcript.push(TranscriptLine {
        kind: LineKind::Event,
        text: format!(
            "Replaying {} as agent {}{}{}",
            args.run_id.0,
            agent_id,
            if args.no_hooks {
                " with lifecycle hooks skipped"
            } else {
                ""
            },
            if args.compare_source {
                "; will compare replay to source"
            } else {
                ""
            },
        ),
    });
    app.pending_replay_comparison = args.compare_source.then_some(ReplayComparisonRequest {
        source_run_id: args.run_id,
        replay_run_id: None,
    });
    app.state = AppState::Running;
    app.tokens_in = 0;
    app.tokens_out = 0;
    app.cost_usd = 0.0;
    app.calls_used = 0;
    app.calls_max = replay_agent.tool_policy.max_calls;
    app.calls_remaining = replay_agent.tool_policy.max_calls;
    app.elapsed_ms = 0;
    app.run_started_at = Some(Instant::now());
    let run_task = tokio::spawn(async move {
        let _ = harness.run(&replay_agent, UserInput { text: prompt }).await;
    });
    app.active_run_handle = Some(run_task.abort_handle());
}

fn parse_resume_slash_args(
    rest: &str,
    last_run_id: Option<RunId>,
) -> anyhow::Result<ResumeSlashArgs> {
    let mut run_id = None;
    let mut from_event = None;
    let mut parts = rest.split_whitespace();
    while let Some(part) = parts.next() {
        if let Some(value) = part.strip_prefix("--from-event=") {
            from_event = Some(parse_event_id(value)?);
            continue;
        }
        match part {
            "--from-event" => {
                let Some(value) = parts.next() else {
                    anyhow::bail!("--from-event needs an event id");
                };
                from_event = Some(parse_event_id(value)?);
            }
            "last" if run_id.is_none() => {
                run_id = last_run_id;
            }
            value if run_id.is_none() => {
                run_id = Some(RunId(uuid::Uuid::parse_str(value)?));
            }
            other => anyhow::bail!("unexpected resume argument {other:?}"),
        }
    }
    let run_id = run_id
        .or(last_run_id)
        .ok_or_else(|| anyhow::anyhow!("resume needs a run id or a previous run"))?;
    Ok(ResumeSlashArgs { run_id, from_event })
}

fn parse_replay_slash_args(
    rest: &str,
    last_run_id: Option<RunId>,
) -> anyhow::Result<ReplaySlashArgs> {
    let mut run_id = None;
    let mut no_hooks = false;
    let mut compare_source = false;
    for part in rest.split_whitespace() {
        match part {
            "--no-hooks" | "--skip-hooks" | "no-hooks" | "skip-hooks" => no_hooks = true,
            "--compare-source" | "compare-source" => compare_source = true,
            "last" if run_id.is_none() => run_id = last_run_id,
            value if run_id.is_none() => run_id = Some(RunId(uuid::Uuid::parse_str(value)?)),
            other => anyhow::bail!("unexpected replay argument {other:?}"),
        }
    }
    let run_id = run_id
        .or(last_run_id)
        .ok_or_else(|| anyhow::anyhow!("replay needs a run id or a previous run"))?;
    Ok(ReplaySlashArgs {
        run_id,
        no_hooks,
        compare_source,
    })
}

fn replay_flags_text(args: &ReplaySlashArgs) -> String {
    let mut flags = Vec::new();
    if args.no_hooks {
        flags.push("--no-hooks");
    }
    if args.compare_source {
        flags.push("--compare-source");
    }
    if flags.is_empty() {
        String::new()
    } else {
        format!(" {}", flags.join(" "))
    }
}

fn trace_default_run_id(app: &App) -> Option<RunId> {
    app.loaded_trace_run_id.or(app.last_run_id)
}

fn trace_replay_source(run_id: RunId, events: &[RunEvent]) -> anyhow::Result<(String, String)> {
    events
        .iter()
        .find_map(|event| match &event.kind {
            RunEventKind::RunStarted { agent_id, input } => Some((agent_id.clone(), input.clone())),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no RunStarted event"))
}

fn parse_event_id(value: &str) -> anyhow::Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|err| anyhow::anyhow!("invalid event id {value:?}: {err}"))?;
    if parsed == 0 {
        anyhow::bail!("event id must be a positive integer");
    }
    Ok(parsed)
}

fn stopped_run_compaction_for_tui(run_id: RunId) -> anyhow::Result<Option<String>> {
    let source = format!("stopped-run:{}", run_id.0);
    Ok(CompactionStore::from_env()
        .list()?
        .into_iter()
        .filter(|record| record.source == source)
        .max_by_key(|record| record.created_at)
        .map(|record| record.id))
}

fn show_active_agent(app: &mut App, agent: &AgentConfig) {
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: format!(
            "Agent: {} ({})\nmodel: {}\ntool calls: {}\ntool visibility: {:?}\nraw output: {}",
            agent.name,
            agent.id,
            agent.model.0,
            agent.tool_policy.max_calls,
            agent.tool_policy.visibility,
            matches!(
                agent.tool_policy.output_mode,
                agent_core::ToolOutputMode::Raw
            )
        ),
    });
}

fn switch_active_agent(
    app: &mut App,
    rest: &str,
    registry: &mut Arc<ToolRegistry>,
    agent: &mut AgentConfig,
    options: &mut setup::RuntimeOptions,
) {
    let id = match agent_switch_arg(rest) {
        Ok(id) => id,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            });
            return;
        }
    };
    if let Err(err) = ConfigResolver::from_env().resolve_agent(id) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Agent switch failed: {err}"),
        });
        return;
    }

    let mut next_options = options.clone();
    next_options.agent_id = Some(id.to_string());
    let next_agent = setup::build_agent(&next_options);
    let next_registry = setup::build_registry(
        next_options.enable_shell,
        next_options.enable_subagent,
        next_options.enable_capability_drafts,
        next_options.agent_id.as_deref(),
        next_options.conversation_id.as_deref(),
    );
    *options = next_options;
    *registry = next_registry;
    *agent = next_agent;
    app.calls_used = 0;
    app.calls_max = agent.tool_policy.max_calls;
    app.calls_remaining = agent.tool_policy.max_calls;
    push_event(
        app,
        format!("Switched active agent to {} ({})", agent.name, agent.id),
    );
    show_active_agent(app, agent);
}

fn agent_switch_arg(rest: &str) -> anyhow::Result<&str> {
    let mut parts = rest.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("/agent needs an agent id"))?;
    if parts.next().is_some() {
        anyhow::bail!("/agent accepts exactly one agent id");
    }
    Ok(id)
}

fn help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/help" | "/?")
}

fn slash_help_rest(rest: &str) -> bool {
    let rest = rest.trim();
    rest.is_empty() || matches!(rest, "help" | "--help")
}

fn global_slash_help_text() -> &'static str {
    "Slash commands:\n\
     - /tool <name> <request> - force one LLM-filled tool call\n\
     - /tool!<name> <json> - call a tool directly with manual JSON input\n\
     - /python <code>, /typescript <code>, /ts <code> - call native code tools directly\n\
     - /voice status, /voice transcribe <path>, /voice speak <text> - inspect voice config or call native voice tools directly\n\
     - /x402 request|required|settle ... - call native x402 payment tools directly\n\
     - /bridges status - inspect messaging bridge readiness without printing secrets\n\
     - /bridge-deliveries [list], /bridge-deliveries delete <id> --confirm - inspect or remove local bridge delivery dead letters\n\
     - /shell status|on|off - inspect or toggle shell tool access\n\
     - /subagent status|on|off - inspect or toggle subagent tool access\n\
     - /budget <n> - set the max tool-call budget for future TUI runs\n\
     - /visibility full|descriptions|names|config - set tool visibility for future TUI runs\n\
     - /cost input|output|both|clear|status - set token cost overrides for future TUI runs\n\
     - /memory on|off|status|preview, /skills on|off|status|preview - toggle or preview runtime memory/skill context loading\n\
     - /ingest preview <id> [prompt] - inspect context with an explicit ingestion artifact\n\
     - /preview <prompt> - inspect context before running\n\
     - /guide <text>, /guide <last|run-id> <text> - steer a run at the next checkpoint\n\
     - /stop [default|--summarise|--discard] [reason], /stop status - stop or inspect the active run\n\
     - /resume [last|run-id] [--from-event N] - resume a saved run\n\
     - /batch <line-delimited prompts>, /batch files <paths>, /batch folder <path>, /resume-batch <batch-id> - run or resume deterministic batches\n\
     - /score [target] <0-10> - score the latest or selected answer\n\
     - /usage [current|last|trace|run|conversation] - inspect current or persisted usage totals\n\
     - /agent [id] - show or switch the active saved agent\n\
     - /models (/model), /agents, /profiles (/profile), /memory, /ingest, /artifacts (/artifact), /approval (/approvals), /trace - inspect command families\n\
     - /conversation (/conversations), /capabilities (/capability), /skills (/skill), /prompts (/prompt), /adapters (/adapter), /hooks, /compact (/compactions), /storage, /bundles (/bundle), /secrets (/secret) - manage runtime assets"
}

fn run_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/run help" | "/run --help")
}

fn prompt_slash_help_text() -> String {
    [
        "/run <name>",
        "/prompts list [--agent <agent>]",
        "/prompts show <name> [--agent <agent>]",
        "/prompts save <name> [--agent <agent>] <text>",
        "/prompts use <name> [--agent <agent>]",
        "/prompts preview <name> [--agent <agent>]",
        "/prompts export <name> <path> [--agent <agent>]",
        "/prompts import <path> [--agent <agent>]",
        "/prompts delete <name> [--agent <agent>] --confirm",
        "/prompt is accepted as an alias for /prompts.",
    ]
    .join("\n")
}

fn agent_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/agent help" | "/agent --help")
}

fn agent_switch_slash_help_text() -> &'static str {
    "/agent\n/agent <saved-agent-id>\n/agents use <id>\n/agents help"
}

fn code_help_slash_command(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "/python"
            | "/python help"
            | "/python --help"
            | "/typescript"
            | "/typescript help"
            | "/typescript --help"
            | "/ts"
            | "/ts help"
            | "/ts --help"
    )
}

fn code_slash_help_text() -> &'static str {
    "/python <code>\n/typescript <code>\n/ts <code>"
}

fn shell_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/shell" {
        Some("")
    } else {
        trimmed.strip_prefix("/shell ").map(str::trim)
    }
}

fn shell_slash_help_text() -> &'static str {
    "/shell status\n/shell on\n/shell off"
}

fn subagent_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/subagent" {
        Some("")
    } else {
        trimmed.strip_prefix("/subagent ").map(str::trim)
    }
}

fn subagent_slash_help_text() -> &'static str {
    "/subagent status\n/subagent on\n/subagent off"
}

fn budget_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/budget" {
        Some("")
    } else {
        trimmed.strip_prefix("/budget ").map(str::trim)
    }
}

fn budget_slash_help_text() -> &'static str {
    "/budget <n>\n/budget 0\n/budget help"
}

fn visibility_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/visibility" {
        Some("")
    } else {
        trimmed.strip_prefix("/visibility ").map(str::trim)
    }
}

fn visibility_slash_help_text() -> &'static str {
    "/visibility full\n/visibility descriptions\n/visibility names\n/visibility config"
}

fn cost_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/cost help" | "/cost --help")
}

fn cost_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/cost" {
        Some("")
    } else {
        trimmed.strip_prefix("/cost ").map(str::trim)
    }
}

fn cost_slash_help_text() -> &'static str {
    "/cost input <usd-per-million>\n/cost output <usd-per-million>\n/cost both <input> <output>\n/cost clear\n/cost status"
}

fn voice_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/voice help" | "/voice --help")
}

fn tool_help_slash_command(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "/tool help" | "/tool --help" | "/tool!help" | "/tool!--help"
    )
}

fn tool_slash_help_text() -> &'static str {
    "/tool <name> <request>\n/tool!<name> <json>"
}

fn preview_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/preview help" | "/preview --help")
}

fn preview_slash_help_text() -> &'static str {
    "/preview\n/preview <prompt>"
}

fn push_context_preview(
    app: &mut App,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    input: &str,
) {
    let harness = setup::build_harness(
        Arc::new(agent_llm::FakeProvider::echo()),
        Arc::new(agent_tracing::InMemoryEventStore::new()),
        registry.clone(),
    );
    let snapshot = harness.preview_context(
        agent,
        UserInput {
            text: if input.is_empty() { "preview" } else { input }.to_string(),
        },
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Event,
        text: format!(
            "Preview: {} tools, {} skills, {} memory, {} artifacts",
            snapshot.visible_tools.len(),
            snapshot.visible_skills.len(),
            snapshot.loaded_memory.len(),
            snapshot.loaded_artifacts.len()
        ),
    });
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: serde_json::to_string_pretty(&snapshot)
            .unwrap_or_else(|_| "<unserializable context>".into()),
    });
}

fn resume_help_slash_command(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "/resume help"
            | "/resume --help"
            | "/resume plan help"
            | "/resume plan --help"
            | "/resume-plan help"
            | "/resume-plan --help"
    )
}

fn resume_slash_help_text() -> &'static str {
    "/resume [last|run-id] [--from-event N]\n/resume plan [last|run-id] [--from-event N]\n/resume-plan [last|run-id] [--from-event N]"
}

fn trace_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/trace help" | "/trace --help")
}

fn trace_slash_help_text() -> &'static str {
    "/trace [last|run-id]\n/trace list [limit|--limit N]\n/trace summary [last|run-id]\n/trace tree [last|run-id]\n/trace hooks [last|run-id]\n/trace scores [last|run-id]\n/trace prompt [last|run-id]\n/trace clear\n/compare [last|primary-run-id] [last|compare-run-id]\n/replay [last|run-id]"
}

fn compare_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/compare help" | "/compare --help")
}

fn compare_slash_help_text() -> &'static str {
    "/compare <compare-run-id>\n/compare [last|primary-run-id] [last|compare-run-id]"
}

fn replay_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/replay help" | "/replay --help")
}

fn replay_slash_help_text() -> &'static str {
    "/replay [last|run-id] [--no-hooks] [--compare-source]"
}

fn guide_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/guide help" | "/guide --help")
}

fn guide_slash_help_text() -> &'static str {
    "/guide <text>\n/guide <last|run-id> <text>"
}

fn score_help_slash_command(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "/score help" | "/score --help" | "/scores help" | "/scores --help"
    )
}

fn score_slash_help_text() -> &'static str {
    "/score <0-10> [target]\n/score <target> <0-10>\n/score <run-id> <0-10> [target]\n/scores [last|run-id]"
}

fn usage_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/usage help" | "/usage --help")
}

fn usage_slash_help_text() -> &'static str {
    "/usage\n/usage current\n/usage last\n/usage trace [last|run-id]\n/usage run [last|run-id]\n/usage conversation [id] [from:to|last N]"
}

fn stop_help_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/stop help" | "/stop --help")
}

fn stop_slash_help_text() -> &'static str {
    "/stop [default|--summarise|--discard] [reason]\n/stop status"
}

fn batch_help_slash_command(trimmed: &str) -> bool {
    matches!(
        trimmed,
        "/batch help" | "/batch --help" | "/resume-batch help" | "/resume-batch --help"
    )
}

fn batch_slash_help_text() -> &'static str {
    "/batch <line-delimited prompts>\n/batch files <line-delimited paths>\n/batch folder <path>\n/resume-batch <batch-id>"
}

#[allow(clippy::too_many_arguments)]
fn handle_slash_command(
    app: &mut App,
    prompt: &str,
    demo: Demo,
    registry: &mut Arc<ToolRegistry>,
    agent: &mut AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
    line_tx: &UnboundedSender<TranscriptLine>,
    options: &mut setup::RuntimeOptions,
) -> bool {
    let trimmed = prompt.trim();
    if help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: global_slash_help_text().into(),
        });
        return true;
    }
    if run_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: prompt_slash_help_text(),
        });
        return true;
    }
    if agent_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: agent_switch_slash_help_text().into(),
        });
        return true;
    }
    if tool_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: tool_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = agent_slash_rest(trimmed) {
        if rest.is_empty() {
            show_active_agent(app, agent);
        } else {
            switch_active_agent(app, rest, registry, agent, options);
        }
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("/tool!").map(str::trim) {
        start_manual_tool_call(app, rest, registry, agent, publish_tx);
        return true;
    }
    if code_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: code_slash_help_text().into(),
        });
        return true;
    }
    if let Some((tool_name, code)) = code_slash_command(trimmed) {
        start_code_tool_call(app, tool_name, code, registry, agent, publish_tx);
        return true;
    }
    if let Some(rest) = shell_slash_rest(trimmed) {
        handle_shell_slash(app, rest, registry, options);
        return true;
    }
    if let Some(rest) = subagent_slash_rest(trimmed) {
        handle_subagent_slash(app, rest, registry, options);
        return true;
    }
    if let Some(rest) = budget_slash_rest(trimmed) {
        handle_budget_slash(app, rest, agent, options);
        return true;
    }
    if let Some(rest) = visibility_slash_rest(trimmed) {
        handle_visibility_slash(app, rest, agent, options);
        return true;
    }
    if cost_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: cost_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = cost_slash_rest(trimmed) {
        handle_cost_slash(app, rest, agent, options);
        return true;
    }
    if voice_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: voice_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = voice_slash_rest(trimmed) {
        handle_voice_slash(app, rest, registry, agent, publish_tx);
        return true;
    }
    if let Some(rest) = crate::x402_slash::slash_rest(trimmed) {
        handle_x402_slash(app, rest, registry, agent, publish_tx);
        return true;
    }
    if let Some(rest) = bridges_slash_rest(trimmed) {
        handle_bridges_slash(app, rest);
        return true;
    }
    if let Some(rest) = bridge_deliveries_slash_rest(trimmed) {
        handle_bridge_deliveries_slash(app, rest);
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("/tool ").map(str::trim) {
        start_forced_tool_call(app, rest, demo, registry, agent, publish_tx, options);
        return true;
    }
    if let Some(rest) = conversation_slash_rest(trimmed) {
        handle_conversation_slash(app, rest, Some(&agent.id));
        return true;
    }
    if let Some(rest) = memory_slash_rest(trimmed) {
        handle_memory_slash(app, rest, registry, agent, line_tx, options);
        return true;
    }
    if let Some(rest) = capabilities_slash_rest(trimmed) {
        handle_capabilities_slash(app, rest);
        return true;
    }
    if let Some(rest) = artifacts_slash_rest(trimmed) {
        handle_artifacts_slash(app, rest);
        return true;
    }
    if let Some(rest) = ingest_slash_rest(trimmed) {
        handle_ingest_slash(app, rest, registry, agent, line_tx, options);
        return true;
    }
    if let Some(rest) = approval_slash_rest(trimmed) {
        handle_approval_slash(app, rest, line_tx);
        return true;
    }
    if batch_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: batch_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = batch_slash_rest(trimmed) {
        start_batch_run(app, rest, demo, publish_tx, options);
        return true;
    }
    if let Some(rest) = resume_batch_slash_rest(trimmed) {
        start_batch_resume(app, rest, demo, publish_tx, options);
        return true;
    }
    if let Some(rest) = models_slash_rest(trimmed) {
        handle_models_slash(app, rest);
        return true;
    }
    if let Some(rest) = agents_slash_rest(trimmed) {
        handle_agents_slash(app, rest, registry, agent, options);
        return true;
    }
    if let Some(rest) = profiles_slash_rest(trimmed) {
        handle_profiles_slash(app, rest);
        return true;
    }
    if let Some(rest) = secrets_slash_rest(trimmed) {
        handle_secrets_slash(app, rest);
        return true;
    }
    if let Some(rest) = prompts_slash_rest(trimmed) {
        handle_prompts_slash(app, rest, registry, agent);
        return true;
    }
    if let Some(rest) = skills_slash_rest(trimmed) {
        handle_skills_slash(app, rest, registry, agent, options);
        return true;
    }
    if let Some(rest) = storage_slash_rest(trimmed) {
        handle_storage_slash(app, rest);
        return true;
    }
    if let Some(rest) = bundles_slash_rest(trimmed) {
        handle_bundles_slash(app, rest);
        return true;
    }
    if let Some(rest) = adapters_slash_rest(trimmed) {
        handle_adapters_slash(app, rest);
        return true;
    }
    if trace_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: trace_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = trace_slash_rest(trimmed) {
        handle_trace_slash(app, rest);
        return true;
    }
    if compare_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: compare_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = compare_slash_rest(trimmed) {
        handle_compare_slash(app, rest);
        return true;
    }
    if let Some(rest) = hooks_slash_rest(trimmed) {
        handle_hooks_slash(app, rest, agent);
        return true;
    }
    if let Some(rest) = compact_slash_rest(trimmed) {
        handle_compact_slash(app, rest);
        return true;
    }
    if usage_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: usage_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = usage_slash_rest(trimmed) {
        handle_usage_slash(app, rest);
        return true;
    }
    if preview_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: preview_slash_help_text().into(),
        });
        return true;
    }
    if let Some(input) = preview_slash_rest(trimmed) {
        push_context_preview(app, registry, agent, input);
        return true;
    }
    if score_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: score_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = scores_slash_rest(trimmed) {
        let run_id = match parse_scores_run_id(rest, trace_default_run_id(app)) {
            Ok(run_id) => run_id,
            Err(err) => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Score review failed: {err}"),
                });
                return true;
            }
        };
        match quality_score_report_for_run(run_id) {
            Ok(report) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: report,
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Score review failed: {err}"),
            }),
        }
        return true;
    }
    if let Some(rest) = score_slash_rest(trimmed) {
        let score = match parse_score_slash_rest(rest) {
            Ok(score) => score,
            Err(err) => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Score failed: {err}"),
                });
                return true;
            }
        };
        let run_id = match score.run_id.or(app.last_run_id) {
            Some(run_id) => run_id,
            None => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "No run to score yet.".into(),
                });
                return true;
            }
        };
        match open_event_store() {
            Ok(store) => {
                match append_score_event(&store, run_id, score.target.clone(), score.score) {
                    Ok(()) => {
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Event,
                            text: format!(
                                "Score recorded for {run_id}: {}/10 on {}",
                                score.score, score.target
                            ),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Score failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Score failed: {err}"),
            }),
        }
        return true;
    }
    if resume_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: resume_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = resume_plan_slash_rest(trimmed) {
        show_resume_plan(app, rest);
        return true;
    }
    if let Some(rest) = resume_slash_rest(trimmed) {
        start_resume_run(app, rest, demo, publish_tx, options);
        return true;
    }
    if replay_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: replay_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = replay_slash_rest(trimmed) {
        start_replay_run(app, rest, demo, publish_tx, options);
        return true;
    }
    if guide_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: guide_slash_help_text().into(),
        });
        return true;
    }
    if let Some(rest) = guide_slash_rest(trimmed) {
        let request = match parse_guide_slash_args(rest, app.last_run_id) {
            Ok(request) => request,
            Err(err) => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Guide failed: {err}"),
                });
                return true;
            }
        };
        let Some(run_id) = request.run_id else {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: "No run to guide yet.".into(),
            });
            return true;
        };
        match open_event_store() {
            Ok(store) => match append_guidance_event(&store, run_id, &request.text) {
                Ok(()) => {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Event,
                        text: format!("Guidance recorded for {run_id}"),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Guide failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Guide failed: {err}"),
            }),
        }
        return true;
    }
    if stop_help_slash_command(trimmed) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: stop_slash_help_text().into(),
        });
        return true;
    }
    if stop_status_slash_command(trimmed) {
        push_event(
            app,
            format!(
                "Stop mode is configured default {}.",
                agent.execution_policy.stop_retention_mode.as_str()
            ),
        );
        return true;
    }
    if let Some(reason) = stop_slash_rest(trimmed) {
        if app.state != AppState::Running {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: "No active run to stop.".into(),
            });
            return true;
        }
        let request = parse_stop_request(reason);
        stop_active_run(
            app,
            &request.reason,
            request
                .mode
                .unwrap_or(agent.execution_policy.stop_retention_mode),
        );
        return true;
    }
    false
}

fn handle_conversation_slash(app: &mut App, rest: &str, active_agent_id: Option<&str>) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/conversation recover <id>",
                "/conversation tree",
                "/conversation browse",
                "/conversation select <id>",
                "/conversation policy [<id>] [--load-memory true|false|clear] [--generate-memory true|false|clear]",
                "/conversation delete-plan [<id>] [--recursive]",
                "/conversation delete [<id>] [--recursive]",
                "/conversation delete-many-plan <id> <id>... [--recursive]",
                "/conversation delete-many <id> <id>... [--recursive]",
                "/conversation delete-agent-plan [agent] [--recursive]",
                "/conversation delete-agent [agent] [--recursive]",
                "/conversation range [<id>] <from> <to>",
                "/conversation range [<id>] <from>:<to>",
                "/conversation usage [<id>] [<from>:<to>|last <n>]",
                "/conversation memory [<id>] <from>:<to> [--user] [--topic <topic>]",
                "/conversation range-delete [<id>] <from>:<to> [--compact-first] [--memory-first]",
                "/conversation confirm",
                "/conversation cancel",
                "/conversations is accepted as an alias for /conversation.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "recover" => {
            if args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "Conversation recovery needs an id.".into(),
                });
                return;
            }
            match crate::headless::conversation_recovery_plan_value(args) {
                Ok(plan) => {
                    let compactions = plan["linked_compactions"]
                        .as_array()
                        .map(Vec::len)
                        .unwrap_or_default();
                    let memories = plan["linked_memories"]
                        .as_array()
                        .map(Vec::len)
                        .unwrap_or_default();
                    let include_compact = plan["suggested_run"]["include_compact"]
                        .as_str()
                        .unwrap_or("none");
                    let load_memory = plan["suggested_run"]["load_memory"].as_bool() == Some(true);
                    push_event(
                        app,
                        format!(
                            "Recovery plan: {compactions} compactions, {memories} memories, include compact {include_compact}, load memory {load_memory}"
                        ),
                    );
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: format!(
                            "{}\n\n{}",
                            format_conversation_recovery_guidance(&plan),
                            serde_json::to_string_pretty(&plan)
                                .unwrap_or_else(|_| "<unserializable recovery plan>".into())
                        ),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Conversation recovery failed: {err}"),
                }),
            }
        }
        "tree" => match ConversationStore::from_env().tree() {
            Ok(tree) => {
                push_event(app, format!("Conversation tree: {} roots", tree.len()));
                let (formatted, index) =
                    format_conversation_tree_picker(&tree, app.selected_conversation_id.as_deref());
                app.conversation_tree_index = index;
                open_conversation_browser_from_tree(app, &tree);
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: formatted,
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation tree failed: {err}"),
            }),
        },
        "browse" => match open_conversation_browser(app) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation browser failed: {err}"),
            }),
        },
        "select" => match select_conversation(app, args) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation select failed: {err}"),
            }),
        },
        "policy" => match show_or_update_conversation_policy(app, args) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation policy failed: {err}"),
            }),
        },
        "delete-plan" | "delete-preview" => match parse_conversation_delete_plan_args_with_selected(
            args,
            app.selected_conversation_id.as_deref(),
        )
        .and_then(|(id, recursive)| conversation_delete_plan_review_value(&id, recursive))
        {
            Ok(plan) => push_conversation_delete_plan(app, &plan),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation delete plan failed: {err}"),
            }),
        },
        "delete" => match prepare_conversation_delete(app, args) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation delete failed: {err}"),
            }),
        },
        "delete-many-plan" | "delete-bulk-plan" | "bulk-delete-plan" => {
            match parse_conversation_delete_many_args(args)
                .and_then(|(ids, recursive)| conversation_delete_many_plan_review_value(&ids, recursive))
            {
                Ok(plan) => push_conversation_delete_plan(app, &plan),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Conversation bulk delete plan failed: {err}"),
                }),
            }
        }
        "delete-many" | "delete-bulk" | "bulk-delete" => {
            match prepare_conversation_delete_many(app, args) {
                Ok(()) => {}
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Conversation bulk delete failed: {err}"),
                }),
            }
        }
        "delete-agent-plan" | "agent-delete-plan" => {
            match parse_conversation_delete_agent_args_with_active(args, active_agent_id)
                .and_then(|(agent, recursive)| {
                    conversation_delete_agent_plan_review_value(&agent, recursive)
                }) {
                Ok(plan) => push_conversation_delete_plan(app, &plan),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Conversation agent delete plan failed: {err}"),
                }),
            }
        }
        "delete-agent" | "agent-delete" => {
            match prepare_conversation_delete_agent(app, args, active_agent_id) {
                Ok(()) => {}
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Conversation agent delete failed: {err}"),
                }),
            }
        }
        "range" | "range-preview" => match parse_conversation_range_args_with_selected(
            args,
            app.selected_conversation_id.as_deref(),
        )
        .and_then(|(id, from, to)| conversation_range_review_value(&id, from, to))
        {
            Ok(review) => push_conversation_range_review(app, &review),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation range review failed: {err}"),
            }),
        },
        "range-delete" | "delete-range" => match prepare_conversation_range_delete(app, args) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation range delete failed: {err}"),
            }),
        },
        "usage" => match show_conversation_usage(app, args) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation usage failed: {err}"),
            }),
        },
        "memory" | "memory-generate" => match generate_conversation_memory_from_tui(app, args) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation memory generation failed: {err}"),
            }),
        },
        "confirm" => match confirm_conversation_action(app) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Conversation confirm failed: {err}"),
            }),
        },
        "cancel" => {
            app.pending_conversation_action = None;
            push_event(app, "Conversation action cancelled.".to_string());
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Conversation command needs recover, tree, browse, select, policy, delete-plan, delete, delete-many-plan, delete-many, delete-agent-plan, delete-agent, range, usage, memory, range-delete, confirm, cancel, or help.".into(),
        }),
    }
}

fn conversation_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/conversation" || trimmed == "/conversations" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/conversation ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/conversations ").map(str::trim)
    }
}

fn select_conversation(app: &mut App, id: &str) -> anyhow::Result<()> {
    let input = id.trim();
    if input.is_empty() {
        anyhow::bail!("select command needs a conversation id");
    }
    let id = resolve_conversation_selection(app, input)?;
    select_conversation_id(app, &id)
}

fn select_conversation_id(app: &mut App, id: &str) -> anyhow::Result<()> {
    let expanded = ConversationStore::from_env().expanded(id)?;
    app.selected_conversation_id = Some(id.to_string());
    push_event(
        app,
        format!(
            "Selected conversation {id}: {} expanded message(s)",
            expanded.messages.len()
        ),
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: format!(
            "selected {}\ntitle: {}\nagent: {}\nexpanded messages: {}",
            id,
            expanded.conversation.title,
            expanded.conversation.agent_id,
            expanded.messages.len()
        ),
    });
    Ok(())
}

fn show_or_update_conversation_policy(app: &mut App, args: &str) -> anyhow::Result<()> {
    let (id, options) = parse_conversation_policy_args_with_selected(
        args,
        app.selected_conversation_id.as_deref(),
    )?;
    let id = resolve_conversation_selection(app, &id)?;
    let store = ConversationStore::from_env();
    let mut doc = store.show(&id)?;
    if options.changes_policy() {
        let policy = if options.clear {
            ConversationPolicy::default()
        } else {
            doc.policy.clone()
        };
        doc = store.set_policy(
            &id,
            crate::headless::apply_conversation_policy_options(policy, &options),
        )?;
    }
    let summary = crate::headless::conversation_policy_summary(&doc.policy);
    push_event(app, format!("Conversation policy for {id}: {summary}"));
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: format!("conversation: {}\npolicy: {}", doc.id, summary),
    });
    Ok(())
}

fn open_conversation_browser(app: &mut App) -> anyhow::Result<()> {
    let tree = ConversationStore::from_env().tree()?;
    push_event(app, format!("Conversation browser: {} roots", tree.len()));
    open_conversation_browser_from_tree(app, &tree);
    Ok(())
}

fn open_conversation_browser_from_tree(app: &mut App, tree: &[ConversationTreeNode]) {
    let browser = conversation_browser_from_tree(tree, app.selected_conversation_id.as_deref());
    app.conversation_tree_index = browser.rows.iter().map(|row| row.id.clone()).collect();
    if browser.rows.is_empty() {
        app.conversation_browser = None;
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: "no conversations".into(),
        });
        return;
    }
    let count = browser.rows.len();
    app.conversation_browser = Some(browser);
    push_event(
        app,
        format!("Conversation browser opened with {count} item(s)."),
    );
}

fn handle_conversation_browser_key(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.conversation_browser = None;
            push_event(app, "Conversation browser closed.".to_string());
        }
        KeyCode::Up | KeyCode::Char('k') => move_conversation_browser_selection(app, -1),
        KeyCode::Down | KeyCode::Char('j') => move_conversation_browser_selection(app, 1),
        KeyCode::Home => set_conversation_browser_selection(app, 0),
        KeyCode::End => {
            let last = app
                .conversation_browser
                .as_ref()
                .map(|browser| browser.rows.len().saturating_sub(1))
                .unwrap_or_default();
            set_conversation_browser_selection(app, last);
        }
        KeyCode::Enter => {
            if let Some(id) = current_browser_conversation_id(app) {
                match select_conversation_id(app, &id) {
                    Ok(()) => app.conversation_browser = None,
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Conversation select failed: {err}"),
                    }),
                }
            }
        }
        KeyCode::Char('r') => {
            if let Some(id) = current_browser_conversation_id(app) {
                match crate::headless::conversation_recovery_plan_value(&id) {
                    Ok(plan) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: format!(
                            "{}\n\n{}",
                            format_conversation_recovery_guidance(&plan),
                            serde_json::to_string_pretty(&plan)
                                .unwrap_or_else(|_| "<unserializable recovery plan>".into())
                        ),
                    }),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Conversation recovery failed: {err}"),
                    }),
                }
            }
        }
        KeyCode::Char('d') => {
            if let Some(id) = current_browser_conversation_id(app) {
                match conversation_delete_plan_review_value(&id, false) {
                    Ok(plan) => push_conversation_delete_plan(app, &plan),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Conversation delete plan failed: {err}"),
                    }),
                }
            }
        }
        _ => {}
    }
    true
}

fn current_browser_conversation_id(app: &App) -> Option<String> {
    app.conversation_browser
        .as_ref()
        .and_then(|browser| browser.rows.get(browser.selected).map(|row| row.id.clone()))
}

fn move_conversation_browser_selection(app: &mut App, delta: isize) {
    if let Some(browser) = &mut app.conversation_browser {
        let last = browser.rows.len().saturating_sub(1);
        let selected = browser.selected.saturating_add_signed(delta).min(last);
        browser.selected = selected;
    }
}

fn set_conversation_browser_selection(app: &mut App, selected: usize) {
    if let Some(browser) = &mut app.conversation_browser {
        browser.selected = selected.min(browser.rows.len().saturating_sub(1));
    }
}

fn resolve_conversation_selection(app: &App, input: &str) -> anyhow::Result<String> {
    if let Ok(index) = input.parse::<usize>()
        && index > 0
    {
        return app
            .conversation_tree_index
            .get(index - 1)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "conversation tree index {index} is not available; run /conversation tree first"
                )
            });
    }
    Ok(input.to_string())
}

fn parse_conversation_policy_args_with_selected(
    rest: &str,
    selected: Option<&str>,
) -> anyhow::Result<(String, crate::headless::ConversationPolicyOptions)> {
    let mut id = None::<String>;
    let mut options = crate::headless::ConversationPolicyOptions::default();
    let mut parts = rest.split_whitespace();
    while let Some(part) = parts.next() {
        match part {
            "--clear" => options.clear = true,
            "--load-memory" => {
                apply_bool_policy_value(
                    next_policy_value(&mut parts, "--load-memory")?,
                    &mut options.load_memory,
                    &mut options.clear_load_memory,
                )?;
            }
            "--clear-load-memory" => options.clear_load_memory = true,
            "--generate-memory" => {
                apply_bool_policy_value(
                    next_policy_value(&mut parts, "--generate-memory")?,
                    &mut options.generate_memory,
                    &mut options.clear_generate_memory,
                )?;
            }
            "--clear-generate-memory" => options.clear_generate_memory = true,
            "--allow-tool-category" => options
                .allowed_tool_categories
                .push(next_policy_value(&mut parts, "--allow-tool-category")?.to_string()),
            "--clear-tool-categories" | "--clear-tool-category" => {
                options.clear_allowed_tool_categories = true
            }
            "--allow-skill-category" => options
                .allowed_skill_categories
                .push(next_policy_value(&mut parts, "--allow-skill-category")?.to_string()),
            "--clear-skill-categories" | "--clear-skill-category" => {
                options.clear_allowed_skill_categories = true
            }
            "--max-tokens-before-compaction" => apply_u32_policy_value(
                next_policy_value(&mut parts, "--max-tokens-before-compaction")?,
                &mut options.max_tokens_before_compaction,
                &mut options.clear_max_tokens_before_compaction,
            )?,
            "--clear-max-tokens-before-compaction" => {
                options.clear_max_tokens_before_compaction = true
            }
            "--max-compaction-output-tokens" => apply_u32_policy_value(
                next_policy_value(&mut parts, "--max-compaction-output-tokens")?,
                &mut options.max_compaction_output_tokens,
                &mut options.clear_max_compaction_output_tokens,
            )?,
            "--clear-max-compaction-output-tokens" => {
                options.clear_max_compaction_output_tokens = true
            }
            "--compaction-guidance" => {
                let value = next_policy_value(&mut parts, "--compaction-guidance")?;
                if value == "clear" {
                    options.clear_compaction_guidance = true;
                } else {
                    options.compaction_guidance = Some(value.to_string());
                }
            }
            "--clear-compaction-guidance" => options.clear_compaction_guidance = true,
            flag if flag.starts_with("--load-memory=") => apply_bool_policy_value(
                flag.trim_start_matches("--load-memory="),
                &mut options.load_memory,
                &mut options.clear_load_memory,
            )?,
            flag if flag.starts_with("--generate-memory=") => apply_bool_policy_value(
                flag.trim_start_matches("--generate-memory="),
                &mut options.generate_memory,
                &mut options.clear_generate_memory,
            )?,
            flag if flag.starts_with("--allow-tool-category=") => {
                options.allowed_tool_categories.push(
                    flag.trim_start_matches("--allow-tool-category=")
                        .to_string(),
                )
            }
            flag if flag.starts_with("--allow-skill-category=") => {
                options.allowed_skill_categories.push(
                    flag.trim_start_matches("--allow-skill-category=")
                        .to_string(),
                )
            }
            flag if flag.starts_with("--max-tokens-before-compaction=") => apply_u32_policy_value(
                flag.trim_start_matches("--max-tokens-before-compaction="),
                &mut options.max_tokens_before_compaction,
                &mut options.clear_max_tokens_before_compaction,
            )?,
            flag if flag.starts_with("--max-compaction-output-tokens=") => apply_u32_policy_value(
                flag.trim_start_matches("--max-compaction-output-tokens="),
                &mut options.max_compaction_output_tokens,
                &mut options.clear_max_compaction_output_tokens,
            )?,
            flag if flag.starts_with("--compaction-guidance=") => {
                let value = flag.trim_start_matches("--compaction-guidance=");
                if value == "clear" {
                    options.clear_compaction_guidance = true;
                } else {
                    options.compaction_guidance = Some(value.to_string());
                }
            }
            other if other.starts_with("--") => {
                anyhow::bail!("unexpected policy argument: {other}")
            }
            candidate if id.is_none() => id = Some(candidate.to_string()),
            other => anyhow::bail!("unexpected policy argument: {other}"),
        }
    }
    let id = id
        .or_else(|| selected.map(str::to_string))
        .ok_or_else(|| anyhow::anyhow!("policy command needs a conversation id"))?;
    Ok((id, options))
}

fn next_policy_value<'a>(
    parts: &mut std::str::SplitWhitespace<'a>,
    flag: &str,
) -> anyhow::Result<&'a str> {
    parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

fn apply_bool_policy_value(
    value: &str,
    target: &mut Option<bool>,
    clear: &mut bool,
) -> anyhow::Result<()> {
    if value == "clear" {
        *clear = true;
        return Ok(());
    }
    *target = Some(value.parse::<bool>()?);
    Ok(())
}

fn apply_u32_policy_value(
    value: &str,
    target: &mut Option<u32>,
    clear: &mut bool,
) -> anyhow::Result<()> {
    if value == "clear" {
        *clear = true;
        return Ok(());
    }
    *target = Some(value.parse::<u32>()?);
    Ok(())
}

#[allow(dead_code)]
fn parse_conversation_range_args(rest: &str) -> anyhow::Result<(String, usize, usize)> {
    parse_conversation_range_args_with_selected(rest, None)
}

fn parse_conversation_range_args_with_selected(
    rest: &str,
    selected: Option<&str>,
) -> anyhow::Result<(String, usize, usize)> {
    if let Ok(parsed) = parse_conversation_range_args_explicit(rest) {
        return Ok(parsed);
    }
    let Some(id) = selected else {
        return parse_conversation_range_args_explicit(rest);
    };
    let mut parts = rest.split_whitespace();
    let first = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("range command needs a from/to range"))?;
    let (from, to) = if let Some((from, to)) = first.split_once(':') {
        if parts.next().is_some() {
            anyhow::bail!("range command accepts either from to or from:to");
        }
        (from.parse::<usize>()?, to.parse::<usize>()?)
    } else {
        let to = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("range command needs an end index"))?;
        if parts.next().is_some() {
            anyhow::bail!("range command received too many arguments");
        }
        (first.parse::<usize>()?, to.parse::<usize>()?)
    };
    if from > to {
        anyhow::bail!("range start {from} is after range end {to}");
    }
    Ok((id.to_string(), from, to))
}

fn parse_conversation_range_delete_args_with_selected(
    rest: &str,
    selected: Option<&str>,
) -> anyhow::Result<(
    String,
    usize,
    usize,
    crate::headless::ConversationRangeDeleteOptions,
)> {
    let mut positional = Vec::new();
    let mut options = crate::headless::ConversationRangeDeleteOptions::default();
    let mut parts = rest.split_whitespace();
    while let Some(part) = parts.next() {
        match part {
            "--compact-first" => options.compact_first = true,
            "--compact-guidance" => {
                options.compact_guidance = Some(
                    parts
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--compact-guidance needs text"))?
                        .to_string(),
                );
            }
            other if other.starts_with("--compact-guidance=") => {
                options.compact_guidance =
                    Some(other.trim_start_matches("--compact-guidance=").to_string());
            }
            "--compact-max-output-tokens" => {
                options.compact_max_output_tokens = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--compact-max-output-tokens needs a value"))?
                    .parse::<u32>()?;
                if options.compact_max_output_tokens == 0 {
                    anyhow::bail!("--compact-max-output-tokens must be greater than zero");
                }
            }
            other if other.starts_with("--compact-max-output-tokens=") => {
                options.compact_max_output_tokens = other
                    .trim_start_matches("--compact-max-output-tokens=")
                    .parse::<u32>()?;
                if options.compact_max_output_tokens == 0 {
                    anyhow::bail!("--compact-max-output-tokens must be greater than zero");
                }
            }
            "--memory-first" => options.memory_first = true,
            "--memory-guidance" => {
                options.memory_guidance = Some(
                    parts
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--memory-guidance needs text"))?
                        .to_string(),
                );
            }
            other if other.starts_with("--memory-guidance=") => {
                options.memory_guidance =
                    Some(other.trim_start_matches("--memory-guidance=").to_string());
            }
            "--memory-user" => options.memory_user = true,
            other if other.starts_with("--") => {
                anyhow::bail!("unexpected range-delete argument: {other}");
            }
            other => positional.push(other),
        }
    }
    let (id, from, to) =
        parse_conversation_range_args_with_selected(&positional.join(" "), selected)?;
    Ok((id, from, to, options))
}

fn parse_conversation_range_args_explicit(rest: &str) -> anyhow::Result<(String, usize, usize)> {
    let mut parts = rest.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("range command needs a conversation id"))?;
    let first = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("range command needs a from/to range"))?;
    let (from, to) = if let Some((from, to)) = first.split_once(':') {
        if parts.next().is_some() {
            anyhow::bail!("range command accepts either from to or from:to");
        }
        (from.parse::<usize>()?, to.parse::<usize>()?)
    } else {
        let to = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("range command needs an end index"))?;
        if parts.next().is_some() {
            anyhow::bail!("range command received too many arguments");
        }
        (first.parse::<usize>()?, to.parse::<usize>()?)
    };
    if from > to {
        anyhow::bail!("range start {from} is after range end {to}");
    }
    Ok((id.to_string(), from, to))
}

fn parse_conversation_usage_args_with_selected(
    rest: &str,
    selected: Option<&str>,
) -> anyhow::Result<(String, Option<usize>, Option<usize>, Option<usize>)> {
    let mut parts = rest.split_whitespace().collect::<Vec<_>>();
    let mut id = selected.map(str::to_string);
    if parts
        .first()
        .is_some_and(|value| *value != "last" && !value.contains(':'))
    {
        id = Some(parts.remove(0).to_string());
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("usage command needs a conversation id"))?;
    match parts.as_slice() {
        [] => Ok((id, None, None, None)),
        ["last", count] => {
            let last = count.parse::<usize>()?;
            if last == 0 {
                anyhow::bail!("usage last count must be greater than zero");
            }
            Ok((id, None, None, Some(last)))
        }
        [range] if range.contains(':') => {
            let (from, to) = range
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("usage range must be from:to"))?;
            let from = from.parse::<usize>()?;
            let to = to.parse::<usize>()?;
            if from > to {
                anyhow::bail!("usage range start {from} is after range end {to}");
            }
            Ok((id, Some(from), Some(to), None))
        }
        _ => anyhow::bail!("usage accepts [id], [id] <from>:<to>, or [id] last <n>"),
    }
}

fn parse_conversation_memory_args_with_selected(
    rest: &str,
    selected: Option<&str>,
) -> anyhow::Result<ConversationMemoryArgs> {
    let mut positional = Vec::new();
    let mut topics = Vec::new();
    let mut guidance = None;
    let mut user = false;
    let mut parts = rest.split_whitespace();
    while let Some(part) = parts.next() {
        match part {
            "--user" => user = true,
            "--guidance" => {
                guidance = Some(
                    parts
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--guidance needs a value"))?
                        .to_string(),
                );
            }
            other if other.starts_with("--guidance=") => {
                guidance = Some(other.trim_start_matches("--guidance=").to_string());
            }
            "--topic" => {
                let topic = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--topic needs a value"))?;
                topics.push(topic.to_string());
            }
            other if other.starts_with("--topic=") => {
                topics.push(other.trim_start_matches("--topic=").to_string());
            }
            other if other.starts_with("--") => {
                anyhow::bail!("unexpected memory argument: {other}")
            }
            other => positional.push(other),
        }
    }
    let range_args = positional.join(" ");
    let (id, from, to) = parse_conversation_range_args_with_selected(&range_args, selected)?;
    Ok(ConversationMemoryArgs {
        id,
        from,
        to,
        user,
        topics,
        guidance,
    })
}

#[allow(dead_code)]
fn parse_conversation_delete_plan_args(rest: &str) -> anyhow::Result<(String, bool)> {
    parse_conversation_delete_plan_args_with_selected(rest, None)
}

fn parse_conversation_delete_plan_args_with_selected(
    rest: &str,
    selected: Option<&str>,
) -> anyhow::Result<(String, bool)> {
    let parts = rest.split_whitespace();
    let mut id = None::<String>;
    let mut recursive = false;
    for part in parts {
        match part {
            "--recursive" | "-r" => recursive = true,
            candidate if id.is_none() => id = Some(candidate.to_string()),
            other => anyhow::bail!("unexpected delete-plan argument: {other}"),
        }
    }
    let id = id
        .or_else(|| selected.map(str::to_string))
        .ok_or_else(|| anyhow::anyhow!("delete-plan command needs a conversation id"))?;
    Ok((id, recursive))
}

fn parse_conversation_delete_many_args(rest: &str) -> anyhow::Result<(Vec<String>, bool)> {
    let mut ids = Vec::new();
    let mut recursive = false;
    for part in rest.split_whitespace() {
        match part {
            "--recursive" | "-r" => recursive = true,
            other if other.starts_with("--") => {
                anyhow::bail!("unexpected delete-many argument: {other}");
            }
            id => ids.push(id.to_string()),
        }
    }
    if ids.is_empty() {
        anyhow::bail!("delete-many command needs at least one conversation id");
    }
    Ok((ids, recursive))
}

fn parse_conversation_delete_agent_args_with_active(
    rest: &str,
    active_agent_id: Option<&str>,
) -> anyhow::Result<(String, bool)> {
    let mut agent = None::<String>;
    let mut recursive = false;
    for part in rest.split_whitespace() {
        match part {
            "--recursive" | "-r" => recursive = true,
            other if other.starts_with("--") => {
                anyhow::bail!("unexpected delete-agent argument: {other}");
            }
            candidate if agent.is_none() => agent = Some(candidate.to_string()),
            other => anyhow::bail!("unexpected delete-agent argument: {other}"),
        }
    }
    let agent = agent
        .or_else(|| active_agent_id.map(str::to_string))
        .ok_or_else(|| anyhow::anyhow!("delete-agent command needs an agent id"))?;
    Ok((agent, recursive))
}

fn push_conversation_delete_plan(app: &mut App, plan: &serde_json::Value) {
    let delete_count = plan["delete_count"].as_u64().unwrap_or_default();
    let recursive = plan["recursive"].as_bool() == Some(true);
    let compactions = plan["linked_compactions"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let memories = plan["linked_memories"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let artifacts = plan["linked_generated_artifacts"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    push_event(
        app,
        format!(
            "Conversation delete plan: {delete_count} conversations, recursive={recursive}, side effects: {compactions} compaction(s), {memories} memory record(s), {artifacts} generated artifact(s)"
        ),
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: serde_json::to_string_pretty(plan)
            .unwrap_or_else(|_| "<unserializable delete plan>".into()),
    });
}

fn push_conversation_range_review(app: &mut App, review: &serde_json::Value) {
    let deletable = review["deletable_by_delete_range"].as_bool() == Some(true);
    let compactions = review["linked_compactions"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let memories = review["linked_memories"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let artifacts = review["linked_generated_artifacts"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    push_event(
        app,
        format!(
            "Conversation range review: deletable={deletable}, side effects: {compactions} compaction(s), {memories} memory record(s), {artifacts} generated artifact(s)"
        ),
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: serde_json::to_string_pretty(review)
            .unwrap_or_else(|_| "<unserializable range review>".into()),
    });
}

fn prepare_conversation_delete(app: &mut App, args: &str) -> anyhow::Result<()> {
    let (id, recursive) = parse_conversation_delete_plan_args_with_selected(
        args,
        app.selected_conversation_id.as_deref(),
    )?;
    let plan = conversation_delete_plan_review_value(&id, recursive)?;
    let delete_ids = plan["delete_ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    push_conversation_delete_plan(app, &plan);
    app.pending_conversation_action = Some(PendingConversationAction::Delete {
        id,
        recursive,
        delete_ids,
    });
    push_event(
        app,
        "Pending conversation delete. Use /conversation confirm or /conversation cancel."
            .to_string(),
    );
    Ok(())
}

fn prepare_conversation_delete_many(app: &mut App, args: &str) -> anyhow::Result<()> {
    let (ids, recursive) = parse_conversation_delete_many_args(args)?;
    let plan = conversation_delete_many_plan_review_value(&ids, recursive)?;
    let delete_ids = plan["delete_ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    push_conversation_delete_plan(app, &plan);
    app.pending_conversation_action = Some(PendingConversationAction::DeleteMany {
        ids,
        recursive,
        delete_ids,
    });
    push_event(
        app,
        "Pending bulk conversation delete. Use /conversation confirm or /conversation cancel."
            .to_string(),
    );
    Ok(())
}

fn prepare_conversation_delete_agent(
    app: &mut App,
    args: &str,
    active_agent_id: Option<&str>,
) -> anyhow::Result<()> {
    let (agent, recursive) =
        parse_conversation_delete_agent_args_with_active(args, active_agent_id)?;
    let plan = conversation_delete_agent_plan_review_value(&agent, recursive)?;
    let delete_ids = plan["delete_ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    push_conversation_delete_plan(app, &plan);
    app.pending_conversation_action = Some(PendingConversationAction::DeleteAgent {
        agent,
        recursive,
        delete_ids,
    });
    push_event(
        app,
        "Pending agent conversation delete. Use /conversation confirm or /conversation cancel."
            .to_string(),
    );
    Ok(())
}

fn prepare_conversation_range_delete(app: &mut App, args: &str) -> anyhow::Result<()> {
    let (id, from, to, options) = parse_conversation_range_delete_args_with_selected(
        args,
        app.selected_conversation_id.as_deref(),
    )?;
    let review = conversation_range_review_value(&id, from, to)?;
    push_conversation_range_review(app, &review);
    if review["deletable_by_delete_range"].as_bool() != Some(true) {
        let warnings = review["warnings"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .filter(|warnings| !warnings.is_empty())
            .unwrap_or_else(|| "range is not deletable".into());
        anyhow::bail!("{warnings}");
    }
    app.pending_conversation_action = Some(PendingConversationAction::DeleteRange {
        id,
        from,
        to,
        options,
    });
    push_event(
        app,
        "Pending conversation range delete. Use /conversation confirm or /conversation cancel."
            .to_string(),
    );
    Ok(())
}

fn show_conversation_usage(app: &mut App, args: &str) -> anyhow::Result<()> {
    let (id, from, to, last) =
        parse_conversation_usage_args_with_selected(args, app.selected_conversation_id.as_deref())?;
    let selection = ConversationStore::from_env().run_ids_for_segment(&id, from, to, last)?;
    let trace_store = open_event_store()?;
    let report = build_conversation_usage_report(selection, |run_id| {
        let Ok(uuid) = uuid::Uuid::parse_str(run_id) else {
            return Ok::<_, anyhow::Error>(None);
        };
        let run_id = RunId(uuid);
        let events = trace_store.try_events(run_id)?;
        if events.is_empty() {
            return Ok::<_, anyhow::Error>(None);
        }
        Ok(Some(summarize_trace(&events, run_id)))
    })?;
    push_event(
        app,
        format!(
            "Conversation usage: {} message(s), {}/{} trace(s)",
            report.message_count,
            report.trace_count,
            report.run_ids.len()
        ),
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: format_conversation_usage_report(&report),
    });
    Ok(())
}

fn generate_conversation_memory_from_tui(app: &mut App, args: &str) -> anyhow::Result<()> {
    let args = parse_conversation_memory_args_with_selected(
        args,
        app.selected_conversation_id.as_deref(),
    )?;
    let target = if args.user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    };
    let expanded = ConversationStore::from_env().expanded(&args.id)?;
    let rendered = render_message_range(&expanded.messages, Some(args.from), Some(args.to))?;
    let records = generate_records_for_active_backend_with_topics_for_agent_and_guidance(
        target,
        &rendered.text,
        Some(rendered.source_range.clone()),
        Some(args.id.clone()),
        args.topics.clone(),
        None,
        args.guidance.clone(),
    )?;
    for record in &records {
        crate::headless::record_memory_written(record, "generated")?;
    }
    push_event(
        app,
        format!(
            "Generated {} memory record(s) from {} {}.",
            records.len(),
            args.id,
            rendered.source_range
        ),
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: serde_json::to_string_pretty(&records)
            .unwrap_or_else(|_| "<unserializable memory records>".into()),
    });
    Ok(())
}

#[derive(Debug, Default)]
struct PreservedRangeArtifacts {
    compaction_ids: Vec<String>,
    memory_ids: Vec<String>,
}

fn preserve_conversation_range_artifacts_from_tui(
    store: &ConversationStore,
    id: &str,
    from: usize,
    to: usize,
    options: &crate::headless::ConversationRangeDeleteOptions,
) -> anyhow::Result<PreservedRangeArtifacts> {
    let mut preserved = PreservedRangeArtifacts::default();
    if !options.compact_first && !options.memory_first {
        return Ok(preserved);
    }
    let rendered = store.render_deletable_message_range(id, from, to)?;
    let source = format!("pre-delete-range:{id}:{}", rendered.source_range);
    if options.compact_first {
        let record = CompactionStore::from_env().create_from_text_for_conversation(
            &rendered.text,
            options.compact_guidance.clone(),
            Some(options.compact_max_output_tokens),
            Some(source),
            Some(id.to_string()),
        )?;
        preserved.compaction_ids.push(record.id);
    }
    if options.memory_first {
        let target = if options.memory_user {
            MemoryTarget::User
        } else {
            MemoryTarget::Agent
        };
        let records = MemoryStore::from_env()
            .generate_from_conversation_text_with_topics_for_agent_and_guidance(
                target,
                &rendered.text,
                Some(rendered.source_range),
                Some(id.to_string()),
                Vec::new(),
                None,
                options.memory_guidance.clone(),
            )?;
        for record in &records {
            crate::headless::record_memory_written(record, "generated")?;
        }
        preserved
            .memory_ids
            .extend(records.into_iter().map(|record| record.id));
    }
    Ok(preserved)
}

fn cleanup_conversation_range_side_data_from_tui(
    id: &str,
    from: usize,
    to: usize,
    preserved: &PreservedRangeArtifacts,
    run_ids: &[String],
) -> anyhow::Result<(usize, usize, usize)> {
    let compactions = CompactionStore::from_env()
        .remove_by_conversation_message_range(id, from, to, &preserved.compaction_ids)?
        .len();
    let memories = delete_records_by_source_conversation_message_range_for_active_backend(
        id,
        from,
        to,
        &preserved.memory_ids,
    )?
    .len();
    let artifacts = cleanup_generated_artifacts_for_run_ids_from_tui(run_ids)?;
    Ok((compactions, memories, artifacts))
}

fn cleanup_deleted_conversation_side_data_from_tui(
    deleted: &[String],
    run_ids: &[String],
) -> anyhow::Result<(usize, usize, usize)> {
    let compactions = CompactionStore::from_env()
        .remove_by_conversation_ids(deleted)?
        .len();
    let memories = delete_records_by_source_conversation_ids_for_active_backend(deleted)?.len();
    let artifacts = cleanup_generated_artifacts_for_run_ids_from_tui(run_ids)?;
    Ok((compactions, memories, artifacts))
}

fn conversation_run_ids_for_docs_from_tui(
    store: &ConversationStore,
    ids: &[String],
) -> anyhow::Result<Vec<String>> {
    let mut run_ids = Vec::new();
    for id in ids {
        for message in store.show(id)?.messages {
            if let Some(run_id) = message.run_id
                && !run_ids.iter().any(|existing| existing == &run_id)
            {
                run_ids.push(run_id);
            }
        }
    }
    Ok(run_ids)
}

fn conversation_run_ids_for_deletable_range_from_tui(
    store: &ConversationStore,
    id: &str,
    from: usize,
    to: usize,
) -> anyhow::Result<Vec<String>> {
    store.render_deletable_message_range(id, from, to)?;
    let expanded = store.expanded(id)?;
    let mut run_ids = Vec::new();
    for message in &expanded.messages[from..=to] {
        if let Some(run_id) = &message.run_id
            && !run_ids.iter().any(|existing| existing == run_id)
        {
            run_ids.push(run_id.clone());
        }
    }
    Ok(run_ids)
}

fn cleanup_generated_artifacts_for_run_ids_from_tui(run_ids: &[String]) -> anyhow::Result<usize> {
    let artifact_ids = generated_artifact_ids_for_run_ids_from_tui(run_ids)?;
    Ok(delete_generated_artifacts_by_ids_from_env(&artifact_ids)?.len())
}

fn generated_artifact_ids_for_run_ids_from_tui(run_ids: &[String]) -> anyhow::Result<Vec<String>> {
    let store = open_event_store()?;
    let mut artifact_ids = Vec::new();
    for run_id in run_ids {
        let Ok(uuid) = uuid::Uuid::parse_str(run_id) else {
            continue;
        };
        let Ok(events) = store.try_events(RunId(uuid)) else {
            continue;
        };
        for event in events {
            if let RunEventKind::ToolCallCompleted { output, .. } = event.kind {
                for artifact_id in generated_artifact_ids_in_value(&output) {
                    if !artifact_ids.iter().any(|existing| existing == &artifact_id) {
                        artifact_ids.push(artifact_id);
                    }
                }
            }
        }
    }
    Ok(artifact_ids)
}

fn existing_generated_artifact_ids_for_run_ids_from_tui(
    run_ids: &[String],
) -> anyhow::Result<Vec<String>> {
    let mut artifact_ids = Vec::new();
    for artifact_id in generated_artifact_ids_for_run_ids_from_tui(run_ids)? {
        if show_generated_artifact_from_env(&artifact_id).is_ok() {
            artifact_ids.push(artifact_id);
        }
    }
    Ok(artifact_ids)
}

fn planned_conversation_side_effects_from_tui(
    store: &ConversationStore,
    delete_ids: &[String],
) -> anyhow::Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut compactions = CompactionStore::from_env()
        .list()?
        .into_iter()
        .filter(|record| {
            record
                .conversation_id
                .as_ref()
                .is_some_and(|id| delete_ids.iter().any(|deleted| deleted == id))
        })
        .map(|record| record.id)
        .collect::<Vec<_>>();
    compactions.sort();

    let mut memories = list_records_for_active_backend()?
        .into_iter()
        .filter(|record| {
            record
                .source_conversation_id
                .as_ref()
                .is_some_and(|id| delete_ids.iter().any(|deleted| deleted == id))
        })
        .map(|record| record.id)
        .collect::<Vec<_>>();
    memories.sort();

    let run_ids = conversation_run_ids_for_docs_from_tui(store, delete_ids)?;
    let mut artifacts = existing_generated_artifact_ids_for_run_ids_from_tui(&run_ids)?;
    artifacts.sort();

    Ok((compactions, memories, artifacts))
}

fn planned_conversation_range_side_effects_from_tui(
    store: &ConversationStore,
    id: &str,
    from: usize,
    to: usize,
) -> anyhow::Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let compactions =
        CompactionStore::from_env().ids_by_conversation_message_range(id, from, to, &[])?;
    let memories =
        list_record_ids_by_source_conversation_message_range_for_active_backend(id, from, to, &[])?;
    let run_ids = conversation_run_ids_for_deletable_range_from_tui(store, id, from, to)?;
    let mut artifacts = existing_generated_artifact_ids_for_run_ids_from_tui(&run_ids)?;
    artifacts.sort();
    Ok((compactions, memories, artifacts))
}

fn confirm_conversation_action(app: &mut App) -> anyhow::Result<()> {
    let Some(action) = app.pending_conversation_action.take() else {
        anyhow::bail!("no pending conversation action");
    };
    match action {
        PendingConversationAction::Delete {
            id,
            recursive,
            delete_ids,
        } => {
            let store = ConversationStore::from_env();
            let planned = store.deletion_plan(std::slice::from_ref(&id), recursive)?;
            let run_ids = conversation_run_ids_for_docs_from_tui(&store, &planned)?;
            let deleted = store.delete_many(std::slice::from_ref(&id), recursive)?;
            if app
                .selected_conversation_id
                .as_ref()
                .is_some_and(|selected| deleted.contains(selected))
            {
                app.selected_conversation_id = None;
            }
            let (deleted_compactions, deleted_memories, deleted_artifacts) =
                cleanup_deleted_conversation_side_data_from_tui(&deleted, &run_ids)?;
            push_event(
                app,
                format!(
                    "Deleted {} conversation branch(es): {}",
                    deleted.len(),
                    deleted.join(", ")
                ),
            );
            if deleted != delete_ids {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Delete plan changed before confirmation; planned {}, deleted {}.",
                        delete_ids.len(),
                        deleted.len()
                    ),
                });
            }
            if deleted_compactions > 0 || deleted_memories > 0 || deleted_artifacts > 0 {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Deleted {deleted_compactions} linked compaction artifact(s), {deleted_memories} linked memory record(s), and {deleted_artifacts} linked generated artifact(s)."
                    ),
                });
            }
        }
        PendingConversationAction::DeleteMany {
            ids,
            recursive,
            delete_ids,
        } => {
            let store = ConversationStore::from_env();
            let planned = store.deletion_plan(&ids, recursive)?;
            let run_ids = conversation_run_ids_for_docs_from_tui(&store, &planned)?;
            let deleted = store.delete_many(&ids, recursive)?;
            if app
                .selected_conversation_id
                .as_ref()
                .is_some_and(|selected| deleted.contains(selected))
            {
                app.selected_conversation_id = None;
            }
            let (deleted_compactions, deleted_memories, deleted_artifacts) =
                cleanup_deleted_conversation_side_data_from_tui(&deleted, &run_ids)?;
            push_event(
                app,
                format!(
                    "Deleted {} conversation branch(es): {}",
                    deleted.len(),
                    deleted.join(", ")
                ),
            );
            if deleted != delete_ids {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Delete plan changed before confirmation; planned {}, deleted {}.",
                        delete_ids.len(),
                        deleted.len()
                    ),
                });
            }
            if deleted_compactions > 0 || deleted_memories > 0 || deleted_artifacts > 0 {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Deleted {deleted_compactions} linked compaction artifact(s), {deleted_memories} linked memory record(s), and {deleted_artifacts} linked generated artifact(s)."
                    ),
                });
            }
        }
        PendingConversationAction::DeleteAgent {
            agent,
            recursive,
            delete_ids,
        } => {
            let store = ConversationStore::from_env();
            let planned = store.deletion_plan_by_agent(&agent, recursive)?;
            let run_ids = conversation_run_ids_for_docs_from_tui(&store, &planned)?;
            let deleted = store.delete_by_agent(&agent, recursive)?;
            if app
                .selected_conversation_id
                .as_ref()
                .is_some_and(|selected| deleted.contains(selected))
            {
                app.selected_conversation_id = None;
            }
            let (deleted_compactions, deleted_memories, deleted_artifacts) =
                cleanup_deleted_conversation_side_data_from_tui(&deleted, &run_ids)?;
            push_event(
                app,
                format!(
                    "Deleted {} conversation branch(es) for agent {agent}: {}",
                    deleted.len(),
                    deleted.join(", ")
                ),
            );
            if deleted != delete_ids {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Delete plan changed before confirmation; planned {}, deleted {}.",
                        delete_ids.len(),
                        deleted.len()
                    ),
                });
            }
            if deleted_compactions > 0 || deleted_memories > 0 || deleted_artifacts > 0 {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Deleted {deleted_compactions} linked compaction artifact(s), {deleted_memories} linked memory record(s), and {deleted_artifacts} linked generated artifact(s)."
                    ),
                });
            }
        }
        PendingConversationAction::DeleteRange {
            id,
            from,
            to,
            options,
        } => {
            let store = ConversationStore::from_env();
            let before = store.expanded(&id)?.messages.len();
            let run_ids = conversation_run_ids_for_deletable_range_from_tui(&store, &id, from, to)?;
            let preserved =
                preserve_conversation_range_artifacts_from_tui(&store, &id, from, to, &options)?;
            let doc = store.delete_message_range(&id, from, to)?;
            let (deleted_compactions, deleted_memories, deleted_artifacts) =
                cleanup_conversation_range_side_data_from_tui(&id, from, to, &preserved, &run_ids)?;
            let after = store.expanded(&id)?.messages.len();
            push_event(
                app,
                format!(
                    "Deleted conversation range {from}:{to} from {id}; expanded messages {before}->{after}, own messages {}",
                    doc.messages.len()
                ),
            );
            if !preserved.compaction_ids.is_empty() || !preserved.memory_ids.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Preserved {} compaction artifact(s) and {} memory record(s).",
                        preserved.compaction_ids.len(),
                        preserved.memory_ids.len()
                    ),
                });
            }
            if deleted_compactions > 0 || deleted_memories > 0 || deleted_artifacts > 0 {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!(
                        "Deleted {deleted_compactions} linked compaction artifact(s), {deleted_memories} linked memory record(s), and {deleted_artifacts} linked generated artifact(s)."
                    ),
                });
            }
        }
    }
    Ok(())
}

fn conversation_delete_plan_review_value(
    id: &str,
    recursive: bool,
) -> anyhow::Result<serde_json::Value> {
    let store = ConversationStore::from_env();
    let delete_ids = store.deletion_plan(&[id.to_string()], recursive)?;
    let (linked_compactions, linked_memories, linked_generated_artifacts) =
        planned_conversation_side_effects_from_tui(&store, &delete_ids)?;
    Ok(serde_json::json!({
        "conversation_id": id,
        "recursive": recursive,
        "delete_count": delete_ids.len(),
        "delete_ids": delete_ids,
        "linked_compactions": linked_compactions,
        "linked_memories": linked_memories,
        "linked_generated_artifacts": linked_generated_artifacts,
        "confirm_command": "/conversation confirm",
        "cancel_command": "/conversation cancel",
    }))
}

fn conversation_delete_many_plan_review_value(
    ids: &[String],
    recursive: bool,
) -> anyhow::Result<serde_json::Value> {
    let store = ConversationStore::from_env();
    let delete_ids = store.deletion_plan(ids, recursive)?;
    let (linked_compactions, linked_memories, linked_generated_artifacts) =
        planned_conversation_side_effects_from_tui(&store, &delete_ids)?;
    Ok(serde_json::json!({
        "requested": "multiple",
        "requested_ids": ids,
        "recursive": recursive,
        "delete_count": delete_ids.len(),
        "delete_ids": delete_ids,
        "linked_compactions": linked_compactions,
        "linked_memories": linked_memories,
        "linked_generated_artifacts": linked_generated_artifacts,
        "confirm_command": "/conversation confirm",
        "cancel_command": "/conversation cancel",
    }))
}

fn conversation_delete_agent_plan_review_value(
    agent: &str,
    recursive: bool,
) -> anyhow::Result<serde_json::Value> {
    let store = ConversationStore::from_env();
    let delete_ids = store.deletion_plan_by_agent(agent, recursive)?;
    let (linked_compactions, linked_memories, linked_generated_artifacts) =
        planned_conversation_side_effects_from_tui(&store, &delete_ids)?;
    Ok(serde_json::json!({
        "requested": agent,
        "agent_id": agent,
        "recursive": recursive,
        "delete_count": delete_ids.len(),
        "delete_ids": delete_ids,
        "linked_compactions": linked_compactions,
        "linked_memories": linked_memories,
        "linked_generated_artifacts": linked_generated_artifacts,
        "confirm_command": "/conversation confirm",
        "cancel_command": "/conversation cancel",
    }))
}

fn format_conversation_recovery_guidance(plan: &serde_json::Value) -> String {
    let id = plan["conversation_id"].as_str().unwrap_or("unknown");
    let title = plan["title"].as_str().unwrap_or("Untitled conversation");
    let own_messages = plan["own_message_count"].as_u64().unwrap_or_default();
    let expanded_messages = plan["expanded_message_count"]
        .as_u64()
        .unwrap_or(own_messages);
    let compactions = plan["linked_compactions"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let memories = plan["linked_memories"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let artifacts = plan["linked_generated_artifacts"]
        .as_array()
        .map(Vec::len)
        .unwrap_or_default();
    let include_compact = plan["suggested_run"]["include_compact"]
        .as_str()
        .unwrap_or("none");
    let load_memory = plan["suggested_run"]["load_memory"].as_bool() == Some(true);
    [
        format!("Recovery guidance for {title} ({id})"),
        format!("messages: own {own_messages}, expanded {expanded_messages}"),
        format!(
            "linked recovery data: {compactions} compaction(s), {memories} memory item(s), {artifacts} generated artifact(s)"
        ),
        format!("suggested context: include compact {include_compact}, load memory {load_memory}"),
        format!("next TUI step: /conversation select {id}"),
    ]
    .join("\n")
}

#[allow(dead_code)]
fn format_conversation_tree(nodes: &[ConversationTreeNode]) -> String {
    format_conversation_tree_picker(nodes, None).0
}

fn format_conversation_tree_picker(
    nodes: &[ConversationTreeNode],
    selected: Option<&str>,
) -> (String, Vec<String>) {
    if nodes.is_empty() {
        return ("no conversations".into(), Vec::new());
    }
    let mut lines = Vec::new();
    let mut index = Vec::new();
    for node in nodes {
        push_conversation_tree_node(&mut lines, &mut index, node, 0, selected);
    }
    (lines.join("\n"), index)
}

fn conversation_browser_from_tree(
    nodes: &[ConversationTreeNode],
    selected: Option<&str>,
) -> ConversationBrowser {
    let mut rows = Vec::new();
    for node in nodes {
        push_conversation_browser_row(&mut rows, node, 0);
    }
    let selected = selected
        .and_then(|id| rows.iter().position(|row| row.id == id))
        .unwrap_or_default();
    ConversationBrowser { rows, selected }
}

fn push_conversation_browser_row(
    rows: &mut Vec<ConversationBrowserRow>,
    node: &ConversationTreeNode,
    depth: usize,
) {
    rows.push(ConversationBrowserRow {
        id: node.id.clone(),
        title: node.title.clone(),
        agent_id: node.agent_id.clone(),
        depth,
        branch_point: node.branch_point,
        topic_preview: node.topic_preview.clone(),
        own_message_count: node.own_message_count,
        expanded_message_count: node.expanded_message_count,
        branch_reason: node.branch_reason.clone(),
    });
    for child in &node.children {
        push_conversation_browser_row(rows, child, depth + 1);
    }
}

fn push_conversation_tree_node(
    lines: &mut Vec<String>,
    index: &mut Vec<String>,
    node: &ConversationTreeNode,
    depth: usize,
    selected: Option<&str>,
) {
    index.push(node.id.clone());
    let ordinal = index.len();
    let indent = "  ".repeat(depth);
    let marker = if selected == Some(node.id.as_str()) {
        "*"
    } else {
        " "
    };
    let reason = node
        .branch_reason
        .as_deref()
        .map(|reason| format!(" reason={}", compact_preview(reason, 80)))
        .unwrap_or_default();
    let topic = node
        .topic_preview
        .as_deref()
        .map(|topic| format!(" topic={}", compact_preview(topic, 80)))
        .unwrap_or_default();
    let branch_point = node
        .branch_point
        .map(|point| format!(" branch_point={point}"))
        .unwrap_or_default();
    lines.push(format!(
        "{indent}{marker}[{ordinal}] {} ({}) agent={} own={} expanded={}{}{}{}",
        node.title,
        node.id,
        node.agent_id,
        node.own_message_count,
        node.expanded_message_count,
        branch_point,
        topic,
        reason
    ));
    for child in &node.children {
        push_conversation_tree_node(lines, index, child, depth + 1, selected);
    }
}

fn conversation_range_review_value(
    id: &str,
    from: usize,
    to: usize,
) -> anyhow::Result<serde_json::Value> {
    let store = ConversationStore::from_env();
    let docs = store.list()?;
    let has_child_branches = docs.iter().any(|doc| {
        doc.parent
            .as_ref()
            .is_some_and(|parent| parent.conversation_id == id)
    });
    let expanded = store.expanded(id)?;
    if expanded.messages.is_empty() {
        anyhow::bail!("conversation {id} has no expanded messages");
    }
    if from > to {
        anyhow::bail!("range start {from} must be less than or equal to range end {to}");
    }
    if to >= expanded.messages.len() {
        anyhow::bail!(
            "range end {to} exceeds last expanded message index {}",
            expanded.messages.len().saturating_sub(1)
        );
    }
    let own_start = expanded
        .messages
        .len()
        .saturating_sub(expanded.conversation.messages.len());
    let includes_inherited = from < own_start;
    let mut warnings = Vec::new();
    if has_child_branches {
        warnings.push("delete-range is only allowed on leaf branches".to_string());
    }
    if includes_inherited {
        warnings.push(
            "range includes inherited parent messages; delete that range on the parent branch"
                .to_string(),
        );
    }
    let deletable = !has_child_branches && !includes_inherited;
    let (linked_compactions, linked_memories, linked_generated_artifacts) = if deletable {
        planned_conversation_range_side_effects_from_tui(&store, id, from, to)?
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    let selected = expanded.messages[from..=to]
        .iter()
        .enumerate()
        .map(|(offset, message)| conversation_message_review_value(from + offset, message))
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "conversation_id": id,
        "title": expanded.conversation.title,
        "from": from,
        "to": to,
        "expanded_message_count": expanded.messages.len(),
        "own_message_start": own_start,
        "has_child_branches": has_child_branches,
        "includes_inherited_messages": includes_inherited,
        "deletable_by_delete_range": deletable,
        "warnings": warnings,
        "linked_compactions": linked_compactions,
        "linked_memories": linked_memories,
        "linked_generated_artifacts": linked_generated_artifacts,
        "messages": selected,
    }))
}

fn conversation_message_review_value(
    index: usize,
    message: &ConversationMessage,
) -> serde_json::Value {
    serde_json::json!({
        "index": index,
        "role": message.role,
        "created_at": message.created_at,
        "content_preview": compact_preview(&message.content, 240),
    })
}

fn compact_preview(text: &str, max_chars: usize) -> String {
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

fn hooks_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/hooks" {
        Some("")
    } else {
        trimmed.strip_prefix("/hooks ").map(str::trim)
    }
}

fn memory_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/memory" {
        Some("")
    } else {
        trimmed.strip_prefix("/memory ").map(str::trim)
    }
}

fn capabilities_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/capabilities" || trimmed == "/capability" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/capabilities ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/capability ").map(str::trim)
    }
}

fn artifacts_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/artifacts" || trimmed == "/artifact" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/artifacts ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/artifact ").map(str::trim)
    }
}

fn ingest_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/ingest" {
        Some("")
    } else {
        trimmed.strip_prefix("/ingest ").map(str::trim)
    }
}

fn approval_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/approval" || trimmed == "/approvals" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/approval ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/approvals ").map(str::trim)
    }
}

fn models_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/models" || trimmed == "/model" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/models ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/model ").map(str::trim)
    }
}

fn agents_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/agents" {
        Some("")
    } else {
        trimmed.strip_prefix("/agents ").map(str::trim)
    }
}

fn profiles_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/profiles" || trimmed == "/profile" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/profiles ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/profile ").map(str::trim)
    }
}

fn secrets_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/secrets" || trimmed == "/secret" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/secrets ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/secret ").map(str::trim)
    }
}

fn storage_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/storage" {
        Some("")
    } else {
        trimmed.strip_prefix("/storage ").map(str::trim)
    }
}

fn bundles_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/bundles" || trimmed == "/bundle" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/bundles ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/bundle ").map(str::trim)
    }
}

fn skills_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/skills" || trimmed == "/skill" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/skills ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/skill ").map(str::trim)
    }
}

fn prompts_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/prompts" || trimmed == "/prompt" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/prompts ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/prompt ").map(str::trim)
    }
}

fn adapters_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/adapters" || trimmed == "/adapter" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/adapters ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/adapter ").map(str::trim)
    }
}

fn handle_memory_slash(
    app: &mut App,
    rest: &str,
    registry: &Arc<ToolRegistry>,
    agent: &mut AgentConfig,
    line_tx: &UnboundedSender<TranscriptLine>,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/memory on|off|status|preview",
                "/memory preview [prompt]",
                "/memory create [--user] [--agent <agent>] [--conversation <id>] [--topic <topic>] <content>",
                "/memory generate [--user] [--agent <agent>] [--conversation <id>] [--range <range>] [--topic <topic>] <text> [--guidance <text>]",
                "/memory list",
                "/memory access [--topic <topic>] [--agent <agent>]",
                "/memory show <id>",
                "/memory edit <id> <content>",
                "/memory delete <id> --confirm",
                "/memory rollback [--user] --confirm",
                "/memory backends",
                "/memory probe [backend] [--topic <topic>]",
                "/memory classify <id> [--model <model>] [--agent <agent>] [--no-apply]",
                "/memory export <path> [--user] [--agent <agent>]",
                "/memory import <path> [--user] [--agent <agent>]",
            ]
            .join("\n"),
        });
        return;
    }
    let rest_lower = rest.to_lowercase();
    match rest_lower.as_str() {
        "on" | "enable" | "enabled" => {
            options.load_memory = true;
            refresh_agent_runtime_policy(app, agent, options);
            push_event(
                app,
                "Runtime memory loading enabled for future TUI runs.".into(),
            );
            return;
        }
        "off" | "disable" | "disabled" => {
            options.load_memory = false;
            refresh_agent_runtime_policy(app, agent, options);
            push_event(
                app,
                "Runtime memory loading disabled for future TUI runs.".into(),
            );
            return;
        }
        "status" => {
            push_event(
                app,
                format!(
                    "Runtime memory loading is {}.",
                    if options.load_memory {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
            );
            return;
        }
        _ => {}
    }
    if rest_lower == "preview" || rest_lower.starts_with("preview ") {
        let prompt = rest
            .get("preview".len()..)
            .map(str::trim)
            .unwrap_or_default();
        options.load_memory = true;
        refresh_agent_runtime_policy(app, agent, options);
        push_context_preview(app, registry, agent, prompt);
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "create" => match parse_memory_write_args(args, "create", false) {
            Ok(args) => {
                let target = memory_target(args.user);
                let owning_agent = args.agent.or_else(|| Some(agent.id.clone()));
                match create_record_for_active_backend_with_topics_for_agent(
                    target,
                    &args.text,
                    MemoryAuthor::Human,
                    None,
                    args.conversation,
                    args.topics,
                    owning_agent,
                ) {
                    Ok(record) => {
                        if let Err(err) = crate::headless::record_memory_written(&record, "created")
                        {
                            app.transcript.push(TranscriptLine {
                                kind: LineKind::Error,
                                text: format!("Memory create trace write failed: {err}"),
                            });
                            return;
                        }
                        push_event(app, format!("Created memory {}.", record.id));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&record)
                                .unwrap_or_else(|_| "<unserializable memory record>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Memory create failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "generate" => match parse_memory_write_args(args, "generate", true) {
            Ok(args) => {
                let target = memory_target(args.user);
                let owning_agent = args.agent.or_else(|| Some(agent.id.clone()));
                match generate_records_for_active_backend_with_topics_for_agent_and_guidance(
                    target,
                    &args.text,
                    args.range,
                    args.conversation,
                    args.topics,
                    owning_agent,
                    args.guidance,
                ) {
                    Ok(records) => {
                        for record in &records {
                            if let Err(err) =
                                crate::headless::record_memory_written(record, "generated")
                            {
                                app.transcript.push(TranscriptLine {
                                    kind: LineKind::Error,
                                    text: format!("Memory generate trace write failed: {err}"),
                                });
                                return;
                            }
                        }
                        push_event(app, format!("Generated {} memory record(s).", records.len()));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&records)
                                .unwrap_or_else(|_| "<unserializable memory records>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Memory generate failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "list" => match list_records_for_active_backend() {
            Ok(records) => {
                push_event(app, format!("Loaded {} memory record(s).", records.len()));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &records
                            .iter()
                            .map(memory_record_summary)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable memory list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Memory list failed: {err}"),
            }),
        },
        "access" => match memory_access_args(args) {
            Ok((topics, agents)) => match crate::headless::memory_access_result(topics, agents) {
                Ok(report) => {
                    let count = report["records"].as_array().map(Vec::len).unwrap_or(0);
                    push_event(app, format!("Loaded {count} accessible memory record(s)."));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&report)
                            .unwrap_or_else(|_| "<unserializable memory access report>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Memory access failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "show" => match first_memory_arg(args, "show") {
            Ok(id) => match MemoryStore::from_env().get(id) {
                Ok(record) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&record)
                        .unwrap_or_else(|_| "<unserializable memory record>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Memory show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "edit" => match memory_edit_args(args) {
            Ok((id, content)) => match edit_record_for_active_backend(id, content) {
                Ok(record) => {
                    if let Err(err) = crate::headless::record_memory_written(&record, "edited") {
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Memory edit trace write failed: {err}"),
                        });
                        return;
                    }
                    push_event(app, format!("Edited memory {}.", record.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&record)
                            .unwrap_or_else(|_| "<unserializable memory record>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Memory edit failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" => match memory_confirm_id_args(args, "delete") {
            Ok(id) => match delete_record_for_active_backend(id) {
                Ok(()) => {
                    if let Err(err) =
                        crate::headless::record_memory_operation(id, "deleted", None, None)
                    {
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Memory delete trace write failed: {err}"),
                        });
                        return;
                    }
                    push_event(app, format!("Deleted memory {id}."));
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Memory delete failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "rollback" => match memory_rollback_args(args) {
            Ok(user) => {
                let target = memory_target(user);
                match rollback_active_backend(target) {
                    Ok(()) => {
                        let name = if user { "user.md" } else { "memory.md" };
                        if let Err(err) =
                            crate::headless::record_memory_operation(name, "rolled_back", None, None)
                        {
                            app.transcript.push(TranscriptLine {
                                kind: LineKind::Error,
                                text: format!("Memory rollback trace write failed: {err}"),
                            });
                            return;
                        }
                        push_event(app, format!("Rolled back {name}."));
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Memory rollback failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "classify" => match parse_memory_classify_args(args) {
            Ok(args) => {
                let MemoryClassifyArgs {
                    id,
                    model,
                    agent: command_agent,
                    apply,
                } = args;
                let agent_id = command_agent.or_else(|| Some(agent.id.clone()));
                push_event(app, format!("Classifying memory {id}."));
                let tx = line_tx.clone();
                tokio::spawn(async move {
                    let line =
                        match crate::headless::memory_classify_result(&id, model, agent_id, apply)
                            .await
                        {
                            Ok(value) => TranscriptLine {
                                kind: LineKind::Assistant,
                                text: serde_json::to_string_pretty(&value).unwrap_or_else(|_| {
                                    "<unserializable memory classification>".into()
                                }),
                            },
                            Err(err) => TranscriptLine {
                                kind: LineKind::Error,
                                text: format!("Memory classify failed: {err}"),
                            },
                        };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "backends" => {
            let backends = supported_memory_backends();
            push_event(app, format!("Loaded {} memory backend(s).", backends.len()));
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&backends)
                    .unwrap_or_else(|_| "<unserializable memory backends>".into()),
            });
        }
        "probe" => match memory_probe_args(args) {
            Ok((backend, topics)) => match probe_memory_backend(
                StoragePaths::from_env(),
                backend.as_deref().unwrap_or(agent_core::DEFAULT_MEMORY_BACKEND_ID),
                &topics,
            ) {
                Ok(report) => {
                    push_event(
                        app,
                        format!(
                            "Probed memory backend {}: ok={} records={}",
                            report.backend, report.ok, report.records
                        ),
                    );
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&report)
                            .unwrap_or_else(|_| "<unserializable memory backend probe>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Memory backend probe failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match memory_path_args(args, "export") {
            Ok((path, user, agent)) => {
                let target = if user {
                    MemoryTarget::User
                } else {
                    MemoryTarget::Agent
                };
                match export_target_for_active_backend(target, path, agent) {
                    Ok(records) => push_event(
                        app,
                        format!("Exported {} memory record(s) to {path}", records.len()),
                    ),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Memory export failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match memory_path_args(args, "import") {
            Ok((path, user, agent)) => {
                let target = if user {
                    MemoryTarget::User
                } else {
                    MemoryTarget::Agent
                };
                match import_file_for_active_backend_for_agent(path, Some(target), agent) {
                    Ok(records) => {
                        for record in &records {
                            if let Err(err) =
                                crate::headless::record_memory_written(record, "imported")
                            {
                                app.transcript.push(TranscriptLine {
                                    kind: LineKind::Error,
                                    text: format!("Memory import trace write failed: {err}"),
                                });
                                return;
                            }
                        }
                        push_event(app, format!("Imported {} memory record(s).", records.len()));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(
                                &records
                                    .iter()
                                    .map(memory_record_summary)
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap_or_else(|_| "<unserializable imported memory>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Memory import failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Memory command needs on, off, status, preview, create, generate, list, access, show, edit, delete, rollback, classify, backends, probe, export, import, or help.".into(),
        }),
    }
}

struct MemoryWriteArgs {
    text: String,
    user: bool,
    range: Option<String>,
    conversation: Option<String>,
    agent: Option<String>,
    topics: Vec<String>,
    guidance: Option<String>,
}

struct MemoryClassifyArgs {
    id: String,
    model: Option<String>,
    agent: Option<String>,
    apply: bool,
}

fn first_memory_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("memory {command} needs an argument"))
}

fn memory_access_args(args: &str) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let mut parts = args.split_whitespace();
    let mut topics = Vec::new();
    let mut agents = Vec::new();
    while let Some(part) = parts.next() {
        match part {
            "--topic" => topics.push(next_memory_option_value(&mut parts, "--topic")?.to_string()),
            value if value.starts_with("--topic=") => {
                topics.push(value.trim_start_matches("--topic=").to_string());
            }
            "--agent" => agents.push(next_memory_option_value(&mut parts, "--agent")?.to_string()),
            value if value.starts_with("--agent=") => {
                agents.push(value.trim_start_matches("--agent=").to_string());
            }
            other => anyhow::bail!("unexpected memory access argument: {other}"),
        }
    }
    Ok((topics, agents))
}

fn memory_probe_args(args: &str) -> anyhow::Result<(Option<String>, Vec<String>)> {
    let mut parts = args.split_whitespace();
    let mut backend = None;
    let mut topics = Vec::new();
    while let Some(part) = parts.next() {
        match part {
            "--topic" => topics.push(next_memory_option_value(&mut parts, "--topic")?.to_string()),
            value if value.starts_with("--topic=") => {
                topics.push(value.trim_start_matches("--topic=").to_string());
            }
            value if value.starts_with("--") => anyhow::bail!("unknown option {value}"),
            value if backend.is_none() => backend = Some(value.to_string()),
            _ => anyhow::bail!("usage: /memory probe [backend] [--topic <topic>]"),
        }
    }
    Ok((backend, topics))
}

fn parse_memory_write_args(
    args: &str,
    command: &str,
    allow_range: bool,
) -> anyhow::Result<MemoryWriteArgs> {
    let mut parts = args.split_whitespace().peekable();
    let mut user = false;
    let mut range = None;
    let mut conversation = None;
    let mut agent = None;
    let mut topics = Vec::new();
    let mut guidance = None;
    let mut text_parts = Vec::new();
    while let Some(part) = parts.next() {
        match part {
            "--user" if text_parts.is_empty() => user = true,
            "--range" if allow_range && text_parts.is_empty() => {
                range = Some(next_memory_option_value(&mut parts, "--range")?.to_string());
            }
            "--conversation" if text_parts.is_empty() => {
                conversation =
                    Some(next_memory_option_value(&mut parts, "--conversation")?.to_string());
            }
            "--agent" if text_parts.is_empty() => {
                agent = Some(next_memory_option_value(&mut parts, "--agent")?.to_string());
            }
            "--topic" if text_parts.is_empty() => {
                topics.push(next_memory_option_value(&mut parts, "--topic")?.to_string());
            }
            "--guidance" if allow_range && text_parts.is_empty() => {
                guidance = Some(next_memory_option_value(&mut parts, "--guidance")?.to_string());
            }
            value if value.starts_with("--range=") && allow_range && text_parts.is_empty() => {
                range = Some(value.trim_start_matches("--range=").to_string());
            }
            value if value.starts_with("--conversation=") && text_parts.is_empty() => {
                conversation = Some(value.trim_start_matches("--conversation=").to_string());
            }
            value if value.starts_with("--agent=") && text_parts.is_empty() => {
                agent = Some(value.trim_start_matches("--agent=").to_string());
            }
            value if value.starts_with("--topic=") && text_parts.is_empty() => {
                topics.push(value.trim_start_matches("--topic=").to_string());
            }
            value if value.starts_with("--guidance=") && allow_range && text_parts.is_empty() => {
                guidance = Some(value.trim_start_matches("--guidance=").to_string());
            }
            value if value.starts_with("--") && text_parts.is_empty() => {
                anyhow::bail!("unexpected memory {command} argument: {value}");
            }
            value => {
                text_parts.push(value);
                text_parts.extend(parts);
                break;
            }
        }
    }
    let text = text_parts.join(" ");
    if text.trim().is_empty() {
        anyhow::bail!("memory {command} needs text");
    }
    let text = if allow_range {
        let (text, trailing_guidance) = memory_generation_text_and_guidance(&text, command)?;
        if guidance.is_some() && trailing_guidance.is_some() {
            anyhow::bail!("memory {command} guidance specified twice");
        }
        guidance = guidance.or(trailing_guidance);
        text
    } else {
        text
    };
    Ok(MemoryWriteArgs {
        text,
        user,
        range,
        conversation,
        agent,
        topics,
        guidance,
    })
}

fn memory_generation_text_and_guidance(
    text: &str,
    command: &str,
) -> anyhow::Result<(String, Option<String>)> {
    let text = text.trim();
    if text == "--guidance" || text.starts_with("--guidance ") || text.starts_with("--guidance=") {
        anyhow::bail!("memory {command} needs text before --guidance");
    }
    if text.ends_with(" --guidance") {
        anyhow::bail!("memory {command} --guidance needs text");
    }
    let spaced = text.rsplit_once(" --guidance ");
    let inline = text.rsplit_once(" --guidance=");
    let parsed = match (spaced, inline) {
        (Some((spaced_text, spaced_guidance)), Some((inline_text, inline_guidance))) => {
            if spaced_text.len() >= inline_text.len() {
                Some((spaced_text, spaced_guidance))
            } else {
                Some((inline_text, inline_guidance))
            }
        }
        (Some(parsed), None) | (None, Some(parsed)) => Some(parsed),
        (None, None) => None,
    };
    let Some((text, guidance)) = parsed else {
        return Ok((text.to_string(), None));
    };
    let text = text.trim();
    let guidance = guidance.trim();
    if text.is_empty() {
        anyhow::bail!("memory {command} needs text before --guidance");
    }
    if guidance.is_empty() {
        anyhow::bail!("memory {command} --guidance needs text");
    }
    Ok((text.to_string(), Some(guidance.to_string())))
}

fn memory_edit_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let (id, content) = args
        .trim()
        .split_once(char::is_whitespace)
        .ok_or_else(|| anyhow::anyhow!("memory edit needs an id and content"))?;
    let content = content.trim();
    if content.is_empty() {
        anyhow::bail!("memory edit needs content");
    }
    Ok((id, content))
}

fn memory_confirm_id_args<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("memory {command} accepts exactly one id and --confirm");
        }
    }
    if !confirmed {
        anyhow::bail!("memory {command} requires --confirm");
    }
    id.ok_or_else(|| anyhow::anyhow!("memory {command} needs an id"))
}

fn memory_rollback_args(args: &str) -> anyhow::Result<bool> {
    let mut user = false;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        match part {
            "--user" => user = true,
            "--confirm" => confirmed = true,
            other => anyhow::bail!("unexpected memory rollback argument: {other}"),
        }
    }
    if !confirmed {
        anyhow::bail!("memory rollback requires --confirm");
    }
    Ok(user)
}

fn memory_target(user: bool) -> MemoryTarget {
    if user {
        MemoryTarget::User
    } else {
        MemoryTarget::Agent
    }
}

fn parse_memory_classify_args(args: &str) -> anyhow::Result<MemoryClassifyArgs> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("memory classify needs an id"))?
        .to_string();
    let mut model = None;
    let mut agent = None;
    let mut apply = true;
    while let Some(part) = parts.next() {
        match part {
            "--model" => {
                model = Some(next_memory_option_value(&mut parts, "--model")?.to_string());
            }
            "--agent" => {
                agent = Some(next_memory_option_value(&mut parts, "--agent")?.to_string());
            }
            "--no-apply" => apply = false,
            value if value.starts_with("--model=") => {
                model = Some(value.trim_start_matches("--model=").to_string());
            }
            value if value.starts_with("--agent=") => {
                let value = value.trim_start_matches("--agent=");
                if value.is_empty() {
                    anyhow::bail!("memory classify --agent needs an id");
                }
                agent = Some(value.to_string());
            }
            other => anyhow::bail!("unexpected memory classify argument: {other}"),
        }
    }
    Ok(MemoryClassifyArgs {
        id,
        model,
        agent,
        apply,
    })
}

fn next_memory_option_value<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    flag: &str,
) -> anyhow::Result<&'a str> {
    parts
        .next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

fn memory_path_args<'a>(
    args: &'a str,
    command: &str,
) -> anyhow::Result<(&'a str, bool, Option<String>)> {
    let mut path = None;
    let mut user = false;
    let mut agent = None;
    let mut parts = args.split_whitespace();
    while let Some(part) = parts.next() {
        match part {
            "--user" => user = true,
            "--agent" => {
                agent = Some(
                    parts
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("memory {command} --agent needs an id"))?
                        .to_string(),
                );
            }
            value if value.starts_with("--agent=") => {
                let value = value.trim_start_matches("--agent=");
                if value.is_empty() {
                    anyhow::bail!("memory {command} --agent needs an id");
                }
                agent = Some(value.to_string());
            }
            _ if path.is_none() => path = Some(part),
            _ => anyhow::bail!(
                "memory {command} accepts exactly one path plus optional --user and --agent"
            ),
        }
    }
    let path = path.ok_or_else(|| anyhow::anyhow!("memory {command} needs a path"))?;
    Ok((path, user, agent))
}

fn memory_record_summary(record: &MemoryRecord) -> serde_json::Value {
    serde_json::json!({
        "id": record.id,
        "target": record.target,
        "author": record.author,
        "owning_profile": record.owning_profile,
        "owning_agent": record.owning_agent,
        "source_range": record.source_range,
        "source_conversation_id": record.source_conversation_id,
        "topics": record.topics,
        "classification": record.classification,
        "updated_at": record.updated_at,
        "content_preview": compact_preview(&record.content, 240),
    })
}

fn handle_capabilities_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/capabilities list",
                "/capabilities doctor",
                "/capabilities propose <tool|skill|agent|subagent> <name> <body> [--guidance <text>]",
                "/capabilities show <id>",
                "/capabilities export <id> <path>",
                "/capabilities import <path>",
                "/capabilities allow <id> --confirm",
                "/capabilities reject <id> --confirm",
                "/capabilities delete <id> --confirm",
                "/capability is accepted as an alias for /capabilities.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match CapabilityDraftStore::from_env().list() {
            Ok(drafts) => {
                push_event(app, format!("Loaded {} capability draft(s).", drafts.len()));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &drafts
                            .iter()
                            .map(capability_draft_summary)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable capability draft list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Capability list failed: {err}"),
            }),
        },
        "doctor" => match CapabilityDraftStore::from_env().doctor_report() {
            Ok(report) => {
                push_event(
                    app,
                    format!(
                        "Capability doctor {:?}: {} draft(s), {} review needed.",
                        report.status, report.draft_count, report.review_needed_count
                    ),
                );
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&capability_doctor_summary(&report))
                        .unwrap_or_else(|_| "<unserializable capability doctor report>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Capability doctor failed: {err}"),
            }),
        },
        "propose" => match propose_capability_draft(args) {
            Ok(draft) => {
                push_event(app, format!("Proposed capability draft {}", draft.id));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&draft)
                        .unwrap_or_else(|_| "<unserializable capability draft>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Capability propose failed: {err}"),
            }),
        },
        "show" => match first_capability_arg(args, "show") {
            Ok(id) => match CapabilityDraftStore::from_env().show(id) {
                Ok(draft) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&draft)
                        .unwrap_or_else(|_| "<unserializable capability draft>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Capability show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match capability_export_args(args) {
            Ok((id, path)) => match CapabilityDraftStore::from_env().export(id, path) {
                Ok(draft) => push_event(
                    app,
                    format!("Exported capability draft {} to {path}", draft.id),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Capability export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match capability_path_arg(args, "import") {
            Ok(path) => match CapabilityDraftStore::from_env().import(path) {
                Ok(draft) => {
                    push_event(
                        app,
                        format!("Imported quarantined capability draft {}", draft.id),
                    );
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&draft)
                            .unwrap_or_else(|_| "<unserializable capability draft>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Capability import failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "allow" => {
            handle_capability_review_slash(app, args, CapabilityDraftStatus::Allowed, "allow")
        }
        "reject" => {
            handle_capability_review_slash(app, args, CapabilityDraftStatus::Rejected, "reject")
        }
        "delete" | "rm" => handle_capability_delete_slash(app, args),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Capabilities command needs list, doctor, propose, show, export, import, allow, reject, delete, or help."
                .into(),
        }),
    }
}

fn propose_capability_draft(args: &str) -> anyhow::Result<CapabilityDraft> {
    let (kind, name, body, guidance) = capability_propose_args(args)?;
    let draft = CapabilityDraftStore::from_env().propose(CapabilityDraftInput {
        id: None,
        kind: CapabilityKind::parse(kind)?,
        name: name.into(),
        body: body.into(),
        guidance: guidance.map(str::to_string),
        created_by: "user".into(),
        provenance: "tui:/capabilities propose".into(),
    })?;
    Ok(draft)
}

fn handle_capability_review_slash(
    app: &mut App,
    args: &str,
    status: CapabilityDraftStatus,
    action: &str,
) {
    match capability_review_args(args, action) {
        Ok((id, true)) => match crate::headless::capability_review_outcome(id, status) {
            Ok(outcome) => {
                let event = outcome.human_line.unwrap_or_else(|| {
                    format!("Capability draft {} marked {:?}", outcome.draft.id, status)
                });
                push_event(app, event);
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&outcome.value)
                        .unwrap_or_else(|_| "<unserializable capability review>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Capability {action} failed: {err}"),
            }),
        },
        Ok((id, false)) => app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: serde_json::to_string_pretty(&serde_json::json!({
                "pending_action": format!("{action}_capability_draft"),
                "draft_id": id,
                "confirm_command": format!("/capabilities {action} {id} --confirm"),
            }))
            .unwrap_or_else(|_| "<unserializable capability confirmation>".into()),
        }),
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: err.to_string(),
        }),
    }
}

fn handle_capability_delete_slash(app: &mut App, args: &str) {
    match capability_review_args(args, "delete") {
        Ok((id, true)) => match CapabilityDraftStore::from_env().delete(id) {
            Ok(true) => push_event(app, format!("Deleted capability draft {id}")),
            Ok(false) => push_event(app, format!("Capability draft {id} was not stored")),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Capability delete failed: {err}"),
            }),
        },
        Ok((id, false)) => app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: serde_json::to_string_pretty(&serde_json::json!({
                "pending_action": "delete_capability_draft",
                "draft_id": id,
                "confirm_command": format!("/capabilities delete {id} --confirm"),
            }))
            .unwrap_or_else(|_| "<unserializable capability confirmation>".into()),
        }),
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: err.to_string(),
        }),
    }
}

fn first_capability_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("capabilities {command} needs an argument"))
}

fn capability_review_args<'a>(args: &'a str, command: &str) -> anyhow::Result<(&'a str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!(
                "capabilities {command} accepts exactly a draft id and optional --confirm"
            );
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("capabilities {command} needs a draft id"))?;
    Ok((id, confirmed))
}

fn capability_path_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("capabilities {command} needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("capabilities {command} accepts exactly one path");
    }
    Ok(path)
}

fn capability_export_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("capabilities export needs a draft id"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("capabilities export needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("capabilities export accepts exactly a draft id and path");
    }
    Ok((id, path))
}

fn capability_propose_args(args: &str) -> anyhow::Result<(&str, &str, &str, Option<&str>)> {
    let (kind, rest) = args
        .trim()
        .split_once(char::is_whitespace)
        .ok_or_else(|| anyhow::anyhow!("capabilities propose needs a kind, name, and body"))?;
    let (name, body) = rest
        .trim()
        .split_once(char::is_whitespace)
        .ok_or_else(|| anyhow::anyhow!("capabilities propose needs a name and body"))?;
    let body = body.trim();
    if body.is_empty() {
        anyhow::bail!("capabilities propose needs a body");
    }
    let (body, guidance) = capability_propose_body_and_guidance(body)?;
    Ok((kind, name, body, guidance))
}

fn capability_propose_body_and_guidance(body: &str) -> anyhow::Result<(&str, Option<&str>)> {
    let body = body.trim();
    if body == "--guidance" || body.starts_with("--guidance ") || body.starts_with("--guidance=") {
        anyhow::bail!("capabilities propose needs a body before --guidance");
    }
    if body.ends_with(" --guidance") {
        anyhow::bail!("capabilities propose --guidance needs text");
    }
    let spaced = body.rsplit_once(" --guidance ");
    let inline = body.rsplit_once(" --guidance=");
    let parsed = match (spaced, inline) {
        (Some((spaced_body, spaced_guidance)), Some((inline_body, inline_guidance))) => {
            if spaced_body.len() >= inline_body.len() {
                Some((spaced_body, spaced_guidance))
            } else {
                Some((inline_body, inline_guidance))
            }
        }
        (Some(parsed), None) | (None, Some(parsed)) => Some(parsed),
        (None, None) => None,
    };
    let Some((body, guidance)) = parsed else {
        return Ok((body, None));
    };
    let body = body.trim();
    let guidance = guidance.trim();
    if body.is_empty() {
        anyhow::bail!("capabilities propose needs a body before --guidance");
    }
    if guidance.is_empty() {
        anyhow::bail!("capabilities propose --guidance needs text");
    }
    Ok((body, Some(guidance)))
}

fn capability_draft_summary(draft: &CapabilityDraft) -> serde_json::Value {
    serde_json::json!({
        "id": draft.id,
        "kind": draft.kind,
        "name": draft.name,
        "status": draft.status,
        "created_by": draft.created_by,
        "provenance": draft.provenance,
        "updated_at": draft.updated_at,
        "body_preview": compact_preview(&draft.body, 240),
        "guidance_preview": draft.guidance.as_deref().map(|guidance| compact_preview(guidance, 240)),
    })
}

fn capability_doctor_summary(report: &CapabilityDraftDoctorReport) -> serde_json::Value {
    serde_json::json!({
        "status": report.status,
        "draft_count": report.draft_count,
        "quarantined_count": report.quarantined_count,
        "allowed_count": report.allowed_count,
        "rejected_count": report.rejected_count,
        "adapter_pack_candidate_count": report.adapter_pack_candidate_count,
        "skill_candidate_count": report.skill_candidate_count,
        "agent_candidate_count": report.agent_candidate_count,
        "review_needed_count": report.review_needed_count,
        "warnings": &report.warnings,
        "drafts": &report.drafts,
    })
}

fn handle_ingest_slash(
    app: &mut App,
    rest: &str,
    registry: &Arc<ToolRegistry>,
    agent: &mut AgentConfig,
    line_tx: &UnboundedSender<TranscriptLine>,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/ingest list",
                "/ingest backends",
                "/ingest status",
                "/ingest use <id>",
                "/ingest include <id>",
                "/ingest exclude <id>",
                "/ingest show <id>",
                "/ingest add <path> [--backend <backend>] [--vision-model <model>] [--guardrail-model <model>]",
                "/ingest rerun <id> [--backend <backend>] [--vision-model <model>] [--guardrail-model <model>]",
                "/ingest probe-source <path> [--vision-model <model>]",
                "/ingest probe-vision <path> --model <model>",
                "/ingest preview <id> [prompt]",
                "/ingest review <id> <finding-index> <acknowledge|approve|reject> [note]",
                "/ingest delete|remove|rm <id> --confirm",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match IngestionStore::from_env().list() {
            Ok(artifacts) => {
                push_event(
                    app,
                    format!("Loaded {} ingestion artifact(s).", artifacts.len()),
                );
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &artifacts
                            .iter()
                            .map(ingestion_artifact_summary)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable ingestion list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Ingest list failed: {err}"),
            }),
        },
        "backends" => {
            if !args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "ingest backends accepts no arguments".into(),
                });
                return;
            }
            let backends = supported_ingestion_backends();
            push_event(
                app,
                format!("Loaded {} ingestion backend descriptor(s).", backends.len()),
            );
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&backends)
                    .unwrap_or_else(|_| "<unserializable ingestion backends>".into()),
            });
        }
        "status" => {
            if !args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "ingest status accepts no arguments".into(),
                });
                return;
            }
            push_event(
                app,
                format!(
                    "Runtime ingestion includes {} artifact(s).",
                    options.include_ingest.len()
                ),
            );
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&serde_json::json!({
                    "include_ingest": &options.include_ingest,
                }))
                .unwrap_or_else(|_| "<unserializable ingestion status>".into()),
            });
        }
        "use" | "include" => match single_ingest_id_arg(args, command) {
            Ok(id) => match IngestionStore::from_env().show(id) {
                Ok(artifact) => {
                    if !options.include_ingest.iter().any(|existing| existing == id) {
                        options.include_ingest.push(id.to_string());
                    }
                    refresh_agent_runtime_policy(app, agent, options);
                    if artifact.has_unapproved_high_risk_findings() {
                        push_event(
                            app,
                            format!(
                                "Ingestion artifact {id} selected; guardrail policy may withhold high-risk content."
                            ),
                        );
                    } else {
                        push_event(
                            app,
                            format!("Ingestion artifact will be included in future TUI context: {id}"),
                        );
                    }
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Ingest include failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "exclude" => match single_ingest_id_arg(args, "exclude") {
            Ok(id) => {
                let before = options.include_ingest.len();
                options.include_ingest.retain(|existing| existing != id);
                refresh_agent_runtime_policy(app, agent, options);
                if options.include_ingest.len() == before {
                    push_event(app, format!("Ingestion artifact was not included: {id}"));
                } else {
                    push_event(
                        app,
                        format!("Ingestion artifact excluded from future TUI context: {id}"),
                    );
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "show" => match first_ingest_arg(args, "show") {
            Ok(id) => match IngestionStore::from_env().show(id) {
                Ok(artifact) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&artifact)
                        .unwrap_or_else(|_| "<unserializable ingestion artifact>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Ingest show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "preview" => match ingest_preview_args(args) {
            Ok((id, prompt)) => match IngestionStore::from_env().show(id) {
                Ok(_) => {
                    let mut preview_options = options.clone();
                    if !preview_options
                        .include_ingest
                        .iter()
                        .any(|existing| existing == id)
                    {
                        preview_options.include_ingest.push(id.to_string());
                    }
                    let preview_agent = setup::build_agent(&preview_options);
                    push_event(app, format!("Previewing context with ingestion artifact {id}."));
                    push_context_preview(app, registry, &preview_agent, prompt);
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Ingest preview failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "review" => match ingest_review_args(args) {
            Ok((id, finding, decision, note)) => {
                match IngestionStore::from_env().review_finding(id, finding, decision, note) {
                    Ok(artifact) => {
                        push_event(
                            app,
                            format!("Reviewed ingestion finding {finding} on {}", artifact.id),
                        );
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&artifact)
                                .unwrap_or_else(|_| "<unserializable ingestion artifact>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Ingest review failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "add" => match ingest_run_args(args, "add") {
            Ok((path, backend, vision_model, guardrail_model)) => {
                push_event(app, format!("Ingesting {path} with {backend}."));
                let tx = line_tx.clone();
                let path = path.to_string();
                tokio::spawn(async move {
                    let line = match crate::headless::ingest_add_result(
                        path,
                        backend,
                        vision_model,
                        guardrail_model,
                    )
                    .await
                    {
                        Ok(result) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| "<unserializable ingestion result>".into()),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Ingest add failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "rerun" => match ingest_run_args(args, "rerun") {
            Ok((id, backend, vision_model, guardrail_model)) => {
                push_event(app, format!("Re-running ingestion {id} with {backend}."));
                let tx = line_tx.clone();
                let id = id.to_string();
                tokio::spawn(async move {
                    let line = match crate::headless::ingest_rerun_result(
                        id,
                        backend,
                        vision_model,
                        guardrail_model,
                    )
                    .await
                    {
                        Ok(result) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| "<unserializable ingestion result>".into()),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Ingest rerun failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "probe-source" => match ingest_probe_source_args(args) {
            Ok((path, vision_model)) => {
                push_event(
                    app,
                    format!("Probing ingestion source compatibility for {path}."),
                );
                let tx = line_tx.clone();
                let path = path.to_string();
                let vision_model = vision_model.map(str::to_string);
                tokio::spawn(async move {
                    let line = match crate::headless::ingest_probe_source_result(path, vision_model)
                    {
                        Ok(report) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&report).unwrap_or_else(|_| {
                                "<unserializable ingestion source probe>".into()
                            }),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Ingest source probe failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "probe-vision" | "probe" => match ingest_probe_vision_args(args) {
            Ok((path, model)) => {
                push_event(app, format!("Probing vision ingestion for {path}."));
                let tx = line_tx.clone();
                let path = path.to_string();
                let model = model.to_string();
                tokio::spawn(async move {
                    let line = match crate::headless::ingest_probe_vision_result(path, model).await
                    {
                        Ok(probe) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&probe).unwrap_or_else(|_| {
                                "<unserializable ingestion vision probe>".into()
                            }),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Ingest vision probe failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" | "remove" | "rm" => match ingest_delete_args(args) {
            Ok((id, true)) => match IngestionStore::from_env().remove(id) {
                Ok(()) => {
                    let before = options.include_ingest.len();
                    options.include_ingest.retain(|existing| existing != id);
                    if options.include_ingest.len() != before {
                        refresh_agent_runtime_policy(app, agent, options);
                    }
                    push_event(app, format!("Removed ingestion artifact {id}"));
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Ingest delete failed: {err}"),
                }),
            },
            Ok((id, false)) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&serde_json::json!({
                    "pending_action": "delete_ingestion_artifact",
                    "artifact_id": id,
                    "confirm_command": format!("/ingest delete {id} --confirm"),
                }))
                .unwrap_or_else(|_| "<unserializable ingestion confirmation>".into()),
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Ingest command needs list, backends, status, use, include, exclude, show, preview, add, rerun, probe-source, probe-vision, review, delete, or help.".into(),
        }),
    }
}

fn first_ingest_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest {command} needs an argument"))
}

fn single_ingest_id_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: /ingest {command} <id>"))?;
    if let Some(extra) = parts.next() {
        anyhow::bail!("usage: /ingest {command} <id>, unexpected {extra:?}");
    }
    Ok(id)
}

fn ingest_preview_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let args = args.trim();
    let (id, prompt) = args
        .split_once(char::is_whitespace)
        .map(|(id, prompt)| (id.trim(), prompt.trim()))
        .unwrap_or((args, ""));
    if id.is_empty() {
        anyhow::bail!("usage: /ingest preview <id> [prompt]");
    }
    let prompt = if prompt.is_empty() { "preview" } else { prompt };
    Ok((id, prompt))
}

fn ingest_delete_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("ingest delete accepts exactly an artifact id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("ingest delete needs an artifact id"))?;
    Ok((id, confirmed))
}

fn ingest_run_args<'a>(
    args: &'a str,
    command: &str,
) -> anyhow::Result<(&'a str, String, Option<String>, Option<String>)> {
    let mut parts = args.split_whitespace();
    let target = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest {command} needs an argument"))?;
    let mut backend = "local-v0".to_string();
    let mut vision_model = None;
    let mut guardrail_model = None;
    while let Some(part) = parts.next() {
        match part {
            "--backend" => {
                backend = ingest_option_value(&mut parts, command, "--backend")?.to_string();
            }
            "--vision-model" => {
                vision_model =
                    Some(ingest_option_value(&mut parts, command, "--vision-model")?.to_string());
            }
            "--guardrail-model" => {
                guardrail_model = Some(
                    ingest_option_value(&mut parts, command, "--guardrail-model")?.to_string(),
                );
            }
            _ => {
                if let Some(value) = part.strip_prefix("--backend=") {
                    backend = non_empty_ingest_option(value, command, "--backend")?.to_string();
                } else if let Some(value) = part.strip_prefix("--vision-model=") {
                    vision_model = Some(
                        non_empty_ingest_option(value, command, "--vision-model")?.to_string(),
                    );
                } else if let Some(value) = part.strip_prefix("--guardrail-model=") {
                    guardrail_model = Some(
                        non_empty_ingest_option(value, command, "--guardrail-model")?.to_string(),
                    );
                } else {
                    anyhow::bail!("unknown ingest {command} option: {part}");
                }
            }
        }
    }
    Ok((target, backend, vision_model, guardrail_model))
}

fn ingest_option_value<'a>(
    parts: &mut std::str::SplitWhitespace<'a>,
    command: &str,
    option: &str,
) -> anyhow::Result<&'a str> {
    let value = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest {command} {option} needs a value"))?;
    non_empty_ingest_option(value, command, option)
}

fn non_empty_ingest_option<'a>(
    value: &'a str,
    command: &str,
    option: &str,
) -> anyhow::Result<&'a str> {
    if value.is_empty() {
        anyhow::bail!("ingest {command} {option} needs a value");
    }
    Ok(value)
}

fn ingest_probe_vision_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest probe-vision needs a path"))?;
    let mut model = None;
    while let Some(part) = parts.next() {
        if part == "--model" {
            if model.is_some() {
                anyhow::bail!("ingest probe-vision accepts one --model value");
            }
            model = Some(
                parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("ingest probe-vision --model needs a value"))?,
            );
        } else if let Some(value) = part.strip_prefix("--model=") {
            if value.is_empty() {
                anyhow::bail!("ingest probe-vision --model needs a value");
            }
            if model.is_some() {
                anyhow::bail!("ingest probe-vision accepts one --model value");
            }
            model = Some(value);
        } else {
            anyhow::bail!("unknown ingest probe-vision option: {part}");
        }
    }
    let model = model.ok_or_else(|| anyhow::anyhow!("ingest probe-vision needs --model"))?;
    Ok((path, model))
}

fn ingest_probe_source_args(args: &str) -> anyhow::Result<(&str, Option<&str>)> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest probe-source needs a path"))?;
    let mut vision_model = None;
    while let Some(part) = parts.next() {
        if part == "--vision-model" || part == "--model" {
            if vision_model.is_some() {
                anyhow::bail!("ingest probe-source accepts one --vision-model value");
            }
            vision_model = Some(parts.next().ok_or_else(|| {
                anyhow::anyhow!("ingest probe-source --vision-model needs a value")
            })?);
        } else if let Some(value) = part.strip_prefix("--vision-model=") {
            if value.is_empty() {
                anyhow::bail!("ingest probe-source --vision-model needs a value");
            }
            if vision_model.is_some() {
                anyhow::bail!("ingest probe-source accepts one --vision-model value");
            }
            vision_model = Some(value);
        } else if let Some(value) = part.strip_prefix("--model=") {
            if value.is_empty() {
                anyhow::bail!("ingest probe-source --model needs a value");
            }
            if vision_model.is_some() {
                anyhow::bail!("ingest probe-source accepts one --vision-model value");
            }
            vision_model = Some(value);
        } else {
            anyhow::bail!("unknown ingest probe-source option: {part}");
        }
    }
    Ok((path, vision_model))
}

fn ingest_review_args(
    args: &str,
) -> anyhow::Result<(&str, u32, IngestionFindingReviewDecision, Option<String>)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest review needs an artifact id"))?;
    let finding = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest review needs a finding index"))?
        .parse::<u32>()?;
    let decision_raw = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("ingest review needs a decision"))?;
    let decision = IngestionFindingReviewDecision::parse(decision_raw).ok_or_else(|| {
        anyhow::anyhow!("decision must be acknowledge, approve/allow, or reject/block")
    })?;
    let note = parts.collect::<Vec<_>>().join(" ");
    let note = (!note.trim().is_empty()).then_some(note);
    Ok((id, finding, decision, note))
}

fn ingestion_artifact_summary(artifact: &IngestionArtifact) -> serde_json::Value {
    serde_json::json!({
        "id": artifact.id,
        "source": artifact.source,
        "backend": artifact.backend,
        "content_hash": artifact.content_hash,
        "sections": artifact.sections.len(),
        "findings": artifact.findings.len(),
        "unapproved_high_risk_findings": artifact.unapproved_high_risk_finding_count(),
        "created_at": artifact.created_at,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ApprovalActionArgs {
    run_id: String,
    approval_id: String,
    unlock_env: Option<String>,
    signature_env: Option<String>,
    controller_agent: Option<String>,
}

fn handle_approval_slash(app: &mut App, rest: &str, line_tx: &UnboundedSender<TranscriptLine>) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/approval list [run-id|last]",
                "/approval assess [run-id|last] <approval-id> [--controller-agent <agent>]",
                "/approval approve [run-id|last] <approval-id> [--unlock-env <env>] [--signature-env <env>] [--controller-agent <agent>]",
                "/approval reject [run-id|last] <approval-id>",
                "/approval execute [run-id|last] <approval-id> [--unlock-env <env>] [--signature-env <env>]",
                "/approvals is accepted as an alias for /approval.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match approval_list_args(args, app.last_run_id) {
            Ok(run_id) => match crate::headless::approval_list_result(&run_id) {
                Ok(approvals) => {
                    push_event(app, format!("Loaded {} approval(s).", approvals.len()));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&approvals)
                            .unwrap_or_else(|_| "<unserializable approval list>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Approval list failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "assess" => match approval_action_args(args, app.last_run_id, "assess", true, false) {
            Ok(parsed) => {
                push_event(app, format!("Assessing approval {}.", parsed.approval_id));
                let tx = line_tx.clone();
                tokio::spawn(async move {
                    let line = match crate::headless::approval_assess_result(
                        parsed.run_id,
                        parsed.approval_id,
                        parsed.controller_agent,
                    )
                    .await
                    {
                        Ok(result) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| "<unserializable approval assessment>".into()),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Approval assess failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "approve" => match approval_action_args(args, app.last_run_id, "approve", true, true) {
            Ok(parsed) => {
                push_event(app, format!("Approving {}.", parsed.approval_id));
                let tx = line_tx.clone();
                tokio::spawn(async move {
                    let line = match crate::headless::approval_decide_result(
                        parsed.run_id,
                        parsed.approval_id,
                        true,
                        parsed.unlock_env,
                        parsed.signature_env,
                        parsed.controller_agent,
                    )
                    .await
                    {
                        Ok(result) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| "<unserializable approval decision>".into()),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Approval approve failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "reject" => match approval_action_args(args, app.last_run_id, "reject", false, false) {
            Ok(parsed) => {
                push_event(app, format!("Rejecting {}.", parsed.approval_id));
                let tx = line_tx.clone();
                tokio::spawn(async move {
                    let line = match crate::headless::approval_decide_result(
                        parsed.run_id,
                        parsed.approval_id,
                        false,
                        None,
                        None,
                        None,
                    )
                    .await
                    {
                        Ok(result) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| "<unserializable approval decision>".into()),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Approval reject failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "execute" => match approval_action_args(args, app.last_run_id, "execute", false, true) {
            Ok(parsed) => {
                push_event(app, format!("Executing {}.", parsed.approval_id));
                let tx = line_tx.clone();
                tokio::spawn(async move {
                    let line = match crate::headless::approval_execute_result(
                        parsed.run_id,
                        parsed.approval_id,
                        parsed.unlock_env,
                        parsed.signature_env,
                    )
                    .await
                    {
                        Ok(result) => TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| "<unserializable approval execution>".into()),
                        },
                        Err(err) => TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Approval execute failed: {err}"),
                        },
                    };
                    let _ = tx.send(line);
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Approval command needs list, assess, approve, reject, execute, or help.".into(),
        }),
    }
}

fn approval_list_args(args: &str, last_run_id: Option<RunId>) -> anyhow::Result<String> {
    let mut parts = args.split_whitespace();
    let run_id = match parts.next() {
        Some(raw) => resolve_approval_run_id(raw, last_run_id),
        None => last_run_id
            .map(|run_id| run_id.0.to_string())
            .ok_or_else(|| anyhow::anyhow!("approval list needs a run id or a previous run")),
    }?;
    if parts.next().is_some() {
        anyhow::bail!("approval list accepts at most one run id");
    }
    Ok(run_id)
}

fn approval_action_args(
    args: &str,
    last_run_id: Option<RunId>,
    command: &str,
    allow_controller: bool,
    allow_secrets: bool,
) -> anyhow::Result<ApprovalActionArgs> {
    let mut positionals = Vec::new();
    let mut unlock_env = None;
    let mut signature_env = None;
    let mut controller_agent = None;
    let mut parts = args.split_whitespace();
    while let Some(part) = parts.next() {
        if let Some(option) = part.strip_prefix("--") {
            let (name, inline_value) = option
                .split_once('=')
                .map(|(name, value)| (name, Some(value)))
                .unwrap_or((option, None));
            let value = match inline_value {
                Some("") => anyhow::bail!("approval {command} --{name} needs a value"),
                Some(value) => value,
                None => parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("approval {command} --{name} needs a value"))?,
            };
            match name {
                "unlock-env" if allow_secrets => unlock_env = Some(value.to_string()),
                "signature-env" if allow_secrets => signature_env = Some(value.to_string()),
                "controller-agent" if allow_controller => {
                    controller_agent = Some(value.to_string())
                }
                _ => anyhow::bail!("unknown approval {command} option: --{name}"),
            }
        } else {
            positionals.push(part);
        }
    }
    let (run_id, approval_id) = match positionals.as_slice() {
        [approval_id] => (
            last_run_id
                .map(|run_id| run_id.0.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!("approval {command} needs a run id or a previous run")
                })?,
            (*approval_id).to_string(),
        ),
        [run_id, approval_id] => (
            resolve_approval_run_id(run_id, last_run_id)?,
            (*approval_id).to_string(),
        ),
        [] => anyhow::bail!("approval {command} needs an approval id"),
        _ => anyhow::bail!("approval {command} accepts a run id and approval id"),
    };
    Ok(ApprovalActionArgs {
        run_id,
        approval_id,
        unlock_env,
        signature_env,
        controller_agent,
    })
}

fn resolve_approval_run_id(raw: &str, last_run_id: Option<RunId>) -> anyhow::Result<String> {
    if raw == "last" {
        last_run_id
            .map(|run_id| run_id.0.to_string())
            .ok_or_else(|| anyhow::anyhow!("approval command needs a previous run"))
    } else {
        Ok(raw.to_string())
    }
}

fn handle_artifacts_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/artifacts list",
                "/artifacts generate <format> <content>",
                "/artifacts show <id>",
                "/artifacts preview <id>",
                "/artifacts open <id>",
                "/artifacts export <id> <path>",
                "/artifacts download <id> [path]",
                "/artifacts delete <id> --confirm",
                "/artifact is accepted as an alias for /artifacts.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match list_generated_artifacts_from_env() {
            Ok(artifacts) => {
                push_event(
                    app,
                    format!("Loaded {} generated artifact(s).", artifacts.len()),
                );
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &artifacts
                            .iter()
                            .map(generated_artifact_summary)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable artifact list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Artifact list failed: {err}"),
            }),
        },
        "generate" => match artifact_generate_args(args) {
            Ok((format, content)) => match generate_artifact_from_env(ArtifactGenerateInput {
                format: format.into(),
                title: None,
                content: Some(content.into()),
                rows: None,
                filename: None,
            }) {
                Ok(artifact) => push_event(
                    app,
                    format!(
                        "Generated artifact {} {} bytes={} path={}",
                        artifact.id,
                        artifact.format,
                        artifact.bytes,
                        artifact.path.display()
                    ),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Artifact generate failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "show" => match first_artifact_arg(args, "show") {
            Ok(id) => match show_generated_artifact_from_env(id) {
                Ok(artifact) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&artifact)
                        .unwrap_or_else(|_| "<unserializable artifact>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Artifact show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "preview" => match first_artifact_arg(args, "preview") {
            Ok(id) => match generated_artifact_data_url_from_env(id) {
                Ok(preview) => {
                    push_event(
                        app,
                        format!(
                            "Loaded artifact preview {} ({})",
                            preview.artifact.id, preview.media_type
                        ),
                    );
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&preview)
                            .unwrap_or_else(|_| "<unserializable artifact preview>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Artifact preview failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "open" => match first_artifact_arg(args, "open") {
            Ok(id) => match open_generated_artifact_from_env(id) {
                Ok(artifact) => push_event(
                    app,
                    format!(
                        "Opened artifact {} at {}",
                        artifact.id,
                        artifact.path.display()
                    ),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Artifact open failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match artifact_export_args(args) {
            Ok((id, path)) => match export_generated_artifact_from_env(id, path) {
                Ok(exported) => push_event(
                    app,
                    format!(
                        "Exported artifact {} to {} ({} bytes)",
                        exported.artifact.id,
                        exported.output_path.display(),
                        exported.bytes
                    ),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Artifact export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "download" => match artifact_download_args(args) {
            Ok((id, path)) => match export_generated_artifact_from_env(id, path) {
                Ok(exported) => push_event(
                    app,
                    format!(
                        "Downloaded artifact {} to {} ({} bytes)",
                        exported.artifact.id,
                        exported.output_path.display(),
                        exported.bytes
                    ),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Artifact download failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" => match artifact_delete_args(args) {
            Ok((id, true)) => match delete_generated_artifact_from_env(id) {
                Ok(artifact) => push_event(
                    app,
                    format!(
                        "Deleted artifact {} at {}",
                        artifact.id,
                        artifact.path.display()
                    ),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Artifact delete failed: {err}"),
                }),
            },
            Ok((id, false)) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&serde_json::json!({
                    "pending_action": "delete_artifact",
                    "artifact_id": id,
                    "confirm_command": format!("/artifacts delete {id} --confirm"),
                }))
                .unwrap_or_else(|_| "<unserializable artifact confirmation>".into()),
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Artifacts command needs list, generate, show, preview, open, export, download, delete, or help."
                .into(),
        }),
    }
}

fn artifact_generate_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.trim().splitn(2, char::is_whitespace);
    let format = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("artifacts generate needs a format"))?;
    let content = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("artifacts generate needs content"))?;
    Ok((format, content))
}

fn first_artifact_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("artifacts {command} needs an argument"))
}

fn artifact_export_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("artifacts export needs an artifact id"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("artifacts export needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("artifacts export accepts exactly an artifact id and path");
    }
    Ok((id, path))
}

fn artifact_download_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("artifacts download needs an artifact id"))?;
    let path = parts.next().unwrap_or(".");
    if parts.next().is_some() {
        anyhow::bail!("artifacts download accepts an artifact id and optional path");
    }
    Ok((id, path))
}

fn artifact_delete_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("artifacts delete accepts exactly an artifact id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("artifacts delete needs an artifact id"))?;
    Ok((id, confirmed))
}

fn generated_artifact_summary(artifact: &GeneratedArtifact) -> serde_json::Value {
    serde_json::json!({
        "id": artifact.id,
        "format": artifact.format,
        "path": artifact.path,
        "bytes": artifact.bytes,
        "modified_ms": artifact.modified_ms,
    })
}

fn handle_models_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/models list",
                "/models show <id>",
                "/models probe <id>",
                "/models save <id> [json]",
                "/models export <id> <path>",
                "/models import <path> --confirm",
                "/models delete <id> --confirm",
                "/models providers",
                "/models doctor",
                "/models provider-catalog show",
                "/models provider-catalog export <path>",
                "/models provider-catalog import <path> --confirm",
                "/models metadata-catalog show",
                "/models metadata-catalog export <path>",
                "/models metadata-catalog import <path> --confirm",
                "/model is accepted as an alias for /models.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match ConfigResolver::from_env().list_models() {
            Ok(models) => {
                push_event(app, format!("Loaded {} model config(s).", models.len()));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &models.iter().map(model_config_summary).collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable model list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Model list failed: {err}"),
            }),
        },
        "show" => match first_model_arg(args, "show") {
            Ok(id) => match ConfigResolver::from_env().show_model(id) {
                Ok(Some(model)) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&model)
                        .unwrap_or_else(|_| "<unserializable model config>".into()),
                }),
                Ok(None) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Model {id} not found."),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Model show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "probe" => match first_model_arg(args, "probe") {
            Ok(id) => match ConfigResolver::from_env().probe_model_capabilities(id) {
                Ok(probe) => {
                    push_event(app, format!("Probed model {}.", probe.model_id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&probe)
                            .unwrap_or_else(|_| "<unserializable model probe>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Model probe failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "save" => match model_save_args(args) {
            Ok(model) => match ConfigResolver::from_env().save_model(&model) {
                Ok(saved) => {
                    push_event(app, format!("Saved model {}.", saved.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&saved)
                            .unwrap_or_else(|_| "<unserializable model config>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Model save failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match model_export_args(args) {
            Ok((id, path)) => match ConfigResolver::from_env().export_model_config(id, path) {
                Ok(model) => push_event(app, format!("Exported model {} to {path}", model.id)),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Model export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match model_import_args(args) {
            Ok((path, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "import_model",
                            "path": path,
                            "confirm_command": format!("/models import {path} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable model confirmation>".into()),
                    });
                    return;
                }
                match ConfigResolver::from_env().import_model_config(path) {
                    Ok(model) => {
                        push_event(app, format!("Imported model {}", model.id));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&model)
                                .unwrap_or_else(|_| "<unserializable model config>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Model import failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" | "rm" => match model_delete_args(args) {
            Ok((id, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "delete_model",
                            "model_id": id,
                            "confirm_command": format!("/models delete {id} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable model confirmation>".into()),
                    });
                    return;
                }
                match ConfigResolver::from_env().delete_model(id) {
                    Ok(true) => push_event(app, format!("Deleted model {id}.")),
                    Ok(false) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Model {id} not found."),
                    }),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Model delete failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "providers" => match configured_model_providers() {
            Ok(providers) => {
                push_event(
                    app,
                    format!("Loaded {} model provider(s).", providers.len()),
                );
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &providers
                            .iter()
                            .map(model_provider_summary)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable model providers>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Model providers failed: {err}"),
            }),
        },
        "doctor" => match ConfigResolver::from_env().model_doctor_report() {
            Ok(report) => {
                push_event(app, format!("Model doctor status {:?}.", report.status));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&report)
                        .unwrap_or_else(|_| "<unserializable model doctor report>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Model doctor failed: {err}"),
            }),
        },
        "provider-catalog" => handle_model_provider_catalog_slash(app, args),
        "metadata-catalog" => handle_model_metadata_catalog_slash(app, args),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Models command needs list, show, probe, save, export, import, delete, providers, doctor, provider-catalog, metadata-catalog, or help.".into(),
        }),
    }
}

fn first_model_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("models {command} needs an argument"))
}

fn model_save_args(args: &str) -> anyhow::Result<ModelConfig> {
    let trimmed = args.trim();
    let (id, input) = trimmed
        .split_once(char::is_whitespace)
        .map(|(id, input)| (id.trim(), input.trim()))
        .unwrap_or((trimmed, ""));
    if id.is_empty() {
        anyhow::bail!("models save needs a model id");
    }
    let mut value = if input.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str::<serde_json::Value>(input)
            .map_err(|err| anyhow::anyhow!("models save JSON is invalid: {err}"))?
    };
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("models save JSON must be an object"))?;
    object.insert("id".into(), serde_json::Value::String(id.into()));
    serde_json::from_value(value)
        .map_err(|err| anyhow::anyhow!("models save JSON does not match ModelConfig: {err}"))
}

fn model_export_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("models export needs a model id"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("models export needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("models export accepts exactly a model id and path");
    }
    Ok((id, path))
}

fn model_import_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut path = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if path.is_none() {
            path = Some(part);
        } else {
            anyhow::bail!("models import accepts exactly one path and optional --confirm");
        }
    }
    let path = path.ok_or_else(|| anyhow::anyhow!("models import needs a path"))?;
    Ok((path, confirmed))
}

fn model_delete_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("models delete accepts exactly a model id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("models delete needs a model id"))?;
    Ok((id, confirmed))
}

fn handle_model_provider_catalog_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/models provider-catalog show",
                "/models provider-catalog export <path>",
                "/models provider-catalog import <path> --confirm",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "show" => match ConfigResolver::from_env().show_model_provider_catalog() {
            Ok(catalog) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&catalog)
                    .unwrap_or_else(|_| "<unserializable model provider catalog>".into()),
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Model provider catalog show failed: {err}"),
            }),
        },
        "export" => match model_provider_catalog_path_arg(args, "export") {
            Ok(path) => match ConfigResolver::from_env().export_model_provider_catalog(path) {
                Ok(catalog) => push_event(
                    app,
                    format!(
                        "Exported model provider catalog with {} provider(s) to {path}",
                        catalog.providers.len()
                    ),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Model provider catalog export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match model_provider_catalog_import_args(args) {
            Ok((path, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "import_model_provider_catalog",
                            "path": path,
                            "confirm_command": format!(
                                "/models provider-catalog import {path} --confirm"
                            ),
                        }))
                        .unwrap_or_else(|_| {
                            "<unserializable model provider catalog confirmation>".into()
                        }),
                    });
                    return;
                }
                match ConfigResolver::from_env().import_model_provider_catalog(path) {
                    Ok(catalog) => {
                        push_event(
                            app,
                            format!(
                                "Imported model provider catalog with {} provider(s).",
                                catalog.providers.len()
                            ),
                        );
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| {
                                "<unserializable model provider catalog>".into()
                            }),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Model provider catalog import failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Models provider-catalog command needs show, export, import, or help.".into(),
        }),
    }
}

fn model_provider_catalog_path_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("models provider-catalog {command} needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("models provider-catalog {command} accepts exactly one path");
    }
    Ok(path)
}

fn model_provider_catalog_import_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut path = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if path.is_none() {
            path = Some(part);
        } else {
            anyhow::bail!(
                "models provider-catalog import accepts exactly one path and optional --confirm"
            );
        }
    }
    let path =
        path.ok_or_else(|| anyhow::anyhow!("models provider-catalog import needs a path"))?;
    Ok((path, confirmed))
}

fn handle_model_metadata_catalog_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/models metadata-catalog show",
                "/models metadata-catalog export <path>",
                "/models metadata-catalog import <path> --confirm",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "show" => match ConfigResolver::from_env().show_model_metadata_catalog() {
            Ok(catalog) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&catalog)
                    .unwrap_or_else(|_| "<unserializable model metadata catalog>".into()),
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Model metadata catalog show failed: {err}"),
            }),
        },
        "export" => match model_metadata_catalog_path_arg(args, "export") {
            Ok(path) => match ConfigResolver::from_env().export_model_metadata_catalog(path) {
                Ok(catalog) => push_event(
                    app,
                    format!(
                        "Exported model metadata catalog with {} model(s) to {path}",
                        catalog.models.len()
                    ),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Model metadata catalog export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match model_metadata_catalog_import_args(args) {
            Ok((path, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "import_model_metadata_catalog",
                            "path": path,
                            "confirm_command": format!(
                                "/models metadata-catalog import {path} --confirm"
                            ),
                        }))
                        .unwrap_or_else(|_| {
                            "<unserializable model metadata catalog confirmation>".into()
                        }),
                    });
                    return;
                }
                match ConfigResolver::from_env().import_model_metadata_catalog(path) {
                    Ok(catalog) => {
                        push_event(
                            app,
                            format!(
                                "Imported model metadata catalog with {} model(s).",
                                catalog.models.len()
                            ),
                        );
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&catalog).unwrap_or_else(|_| {
                                "<unserializable model metadata catalog>".into()
                            }),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Model metadata catalog import failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Models metadata-catalog command needs show, export, import, or help.".into(),
        }),
    }
}

fn model_metadata_catalog_path_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("models metadata-catalog {command} needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("models metadata-catalog {command} accepts exactly one path");
    }
    Ok(path)
}

fn model_metadata_catalog_import_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut path = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if path.is_none() {
            path = Some(part);
        } else {
            anyhow::bail!(
                "models metadata-catalog import accepts exactly one path and optional --confirm"
            );
        }
    }
    let path =
        path.ok_or_else(|| anyhow::anyhow!("models metadata-catalog import needs a path"))?;
    Ok((path, confirmed))
}

fn model_config_summary(model: &agent_config::ModelConfig) -> serde_json::Value {
    serde_json::json!({
        "id": model.id,
        "provider": model.provider,
        "max_context_tokens": model.max_context_tokens,
        "max_output_tokens": model.max_output_tokens,
        "default_temperature": model.default_temperature,
        "available_modalities": model.available_modalities,
        "tool_support": model.tool_support,
    })
}

fn model_provider_summary(provider: &agent_config::ModelProviderDescriptor) -> serde_json::Value {
    serde_json::json!({
        "id": provider.id,
        "name": provider.name,
        "default_model": provider.default_model,
        "available_modalities": provider.available_modalities,
        "tool_support": provider.tool_support,
        "local": provider.local,
        "native": provider.native,
        "option_schema_count": provider.option_schema.len(),
    })
}

fn handle_agents_slash(
    app: &mut App,
    rest: &str,
    registry: &mut Arc<ToolRegistry>,
    agent: &mut AgentConfig,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/agents list",
                "/agents show <id>",
                "/agents save [--disable-lifecycle-hook <hook-id>] [--refinement-rules-json <json-array>] <id> <system prompt>",
                "/agents use <id>",
                "/agents export <id> <path>",
                "/agents import <path> --confirm",
                "/agents delete <id> --confirm",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match ConfigResolver::from_env().list_agent_configs() {
            Ok(agents) => {
                push_event(app, format!("Loaded {} agent config(s).", agents.len()));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &agents.iter().map(agent_summary).collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable agent list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Agent list failed: {err}"),
            }),
        },
        "show" => match first_agent_arg(args, "show") {
            Ok(id) => match ConfigResolver::from_env().show_agent_config(id) {
                Ok(Some(agent)) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&agent)
                        .unwrap_or_else(|_| "<unserializable agent config>".into()),
                }),
                Ok(None) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Agent {id} not found."),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Agent show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "save" => match agent_save_args(args) {
            Ok(parsed) => {
                let agent_id = parsed.id;
                let agent = AgentConfigFile {
                    id: agent_id.clone(),
                    name: agent_id,
                    system_prompt: parsed.system_prompt,
                    prompt_refinements: parsed.prompt_refinements,
                    disabled_lifecycle_hooks: (!parsed.disabled_lifecycle_hooks.is_empty())
                        .then_some(parsed.disabled_lifecycle_hooks),
                    ..AgentConfigFile::default()
                };
                match ConfigResolver::from_env().save_agent_config(&agent) {
                    Ok(saved) => {
                        push_event(app, format!("Saved agent {}.", saved.id));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&saved)
                                .unwrap_or_else(|_| "<unserializable agent config>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Agent save failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "use" => switch_active_agent(app, args, registry, agent, options),
        "export" => match agent_export_args(args) {
            Ok((id, path)) => match ConfigResolver::from_env().export_agent_config(id, path) {
                Ok(agent) => push_event(app, format!("Exported agent {} to {path}", agent.id)),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Agent export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match agent_import_args(args) {
            Ok((path, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "import_agent",
                            "path": path,
                            "confirm_command": format!("/agents import {path} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable agent confirmation>".into()),
                    });
                    return;
                }
                match ConfigResolver::from_env().import_agent_config(path) {
                    Ok(agent) => {
                        push_event(app, format!("Imported agent {}", agent.id));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&agent)
                                .unwrap_or_else(|_| "<unserializable agent config>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Agent import failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" | "rm" => match agent_delete_args(args) {
            Ok((id, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "delete_agent",
                            "agent_id": id,
                            "confirm_command": format!("/agents delete {id} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable agent confirmation>".into()),
                    });
                    return;
                }
                match ConfigResolver::from_env().delete_agent_config(id) {
                    Ok(true) => push_event(app, format!("Deleted agent {id}.")),
                    Ok(false) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Agent {id} not found."),
                    }),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Agent delete failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Agents command needs list, show, save, use, export, import, delete, or help."
                .into(),
        }),
    }
}

fn first_agent_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("agents {command} needs an argument"))
}

fn agent_save_args(args: &str) -> anyhow::Result<crate::headless::AgentSaveSlashArgs> {
    crate::headless::parse_agent_save_slash_args(args)
}

fn agent_export_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("agents export needs an agent id"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("agents export needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("agents export accepts exactly an agent id and path");
    }
    Ok((id, path))
}

fn agent_import_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut path = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if path.is_none() {
            path = Some(part);
        } else {
            anyhow::bail!("agents import accepts exactly one path and optional --confirm");
        }
    }
    let path = path.ok_or_else(|| anyhow::anyhow!("agents import needs a path"))?;
    Ok((path, confirmed))
}

fn agent_delete_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("agents delete accepts exactly an agent id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("agents delete needs an agent id"))?;
    Ok((id, confirmed))
}

fn agent_summary(agent: &AgentSummary) -> serde_json::Value {
    serde_json::json!({
        "id": agent.id,
        "name": agent.name,
        "path": agent.path.display().to_string(),
        "profile": agent.profile.as_deref(),
        "shared_from_profile": agent.shared_from_profile.as_deref(),
        "grant_id": agent.grant_id.as_deref(),
    })
}

fn handle_profiles_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/profiles current",
                "/profiles list",
                "/profiles show <id>",
                "/profiles create <id> [--name <name>]",
                "/profiles delete <id> --confirm",
                "/profiles grant [--from <profile>] --to <profile> --kind <agent|memory|tool|skill|category> <resource>",
                "  memory resources: agent:<id>, memory:<id>, raw id, or *",
                "/profiles grants [--from <profile>]",
                "/profiles revoke-grant <id> --confirm",
                "/profile is accepted as an alias for /profiles.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "current" => {
            if !args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "profiles current accepts no arguments".into(),
                });
                return;
            }
            let profile_id = StoragePaths::from_env().active_profile_id().to_string();
            match ConfigResolver::from_env().show_profile(&profile_id) {
                Ok(profile) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&profile_summary(&profile))
                        .unwrap_or_else(|_| "<unserializable active profile>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Profile current failed: {err}"),
                }),
            }
        }
        "list" => match ConfigResolver::from_env().list_profiles() {
            Ok(profiles) => {
                push_event(app, format!("Loaded {} profile(s).", profiles.len()));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &profiles.iter().map(profile_summary).collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable profile list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Profile list failed: {err}"),
            }),
        },
        "show" => match first_profile_arg(args, "show") {
            Ok(id) => match ConfigResolver::from_env().show_profile(id) {
                Ok(profile) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&profile_summary(&profile))
                        .unwrap_or_else(|_| "<unserializable profile>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Profile show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "create" => match profile_create_args(args) {
            Ok((id, name)) => match ConfigResolver::from_env().create_profile(id, name) {
                Ok(profile) => {
                    push_event(app, format!("Created profile {}.", profile.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&profile_summary(&profile))
                            .unwrap_or_else(|_| "<unserializable profile>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Profile create failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" | "rm" => match profile_confirm_id_args(args, "delete") {
            Ok((id, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "delete_profile",
                            "profile_id": id,
                            "confirm_command": format!("/profiles delete {id} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable profile confirmation>".into()),
                    });
                    return;
                }
                match ConfigResolver::from_env().delete_profile(id) {
                    Ok(true) => push_event(app, format!("Deleted profile {id}.")),
                    Ok(false) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Profile {id} not found."),
                    }),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Profile delete failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "grant" => match profile_grant_args(args) {
            Ok(parsed) => {
                let from = parsed
                    .from
                    .unwrap_or_else(|| StoragePaths::from_env().active_profile_id().to_string());
                match ConfigResolver::from_env().grant_profile_access(
                    &from,
                    &parsed.to,
                    parsed.kind,
                    &parsed.resource,
                ) {
                    Ok(grant) => {
                        push_event(app, format!("Saved profile grant {}.", grant.id));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&grant)
                                .unwrap_or_else(|_| "<unserializable profile grant>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Profile grant failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "grants" => match profile_grants_args(args) {
            Ok(from) => {
                let result = if let Some(from) = from {
                    ConfigResolver::from_env().list_profile_grants_from(&from)
                } else {
                    ConfigResolver::from_env().list_profile_grants()
                };
                match result {
                    Ok(grants) => {
                        push_event(app, format!("Loaded {} profile grant(s).", grants.len()));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&grants)
                                .unwrap_or_else(|_| "<unserializable profile grants>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Profile grants failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "revoke-grant" | "revoke" => match profile_confirm_id_args(args, "revoke-grant") {
            Ok((id, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "revoke_profile_grant",
                            "grant_id": id,
                            "confirm_command": format!("/profiles revoke-grant {id} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable grant confirmation>".into()),
                    });
                    return;
                }
                match ConfigResolver::from_env().revoke_profile_grant(id) {
                    Ok(grant) => push_event(app, format!("Revoked profile grant {}.", grant.id)),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Profile grant revoke failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Profiles command needs current, list, show, create, delete, grant, grants, revoke-grant, or help.".into(),
        }),
    }
}

struct ProfileGrantArgs {
    from: Option<String>,
    to: String,
    kind: ProfileGrantKind,
    resource: String,
}

fn first_profile_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("profiles {command} needs an argument"))
}

fn profile_create_args(args: &str) -> anyhow::Result<(&str, Option<String>)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("profiles create needs a profile id"))?;
    let mut name = None;
    while let Some(part) = parts.next() {
        match part {
            "--name" => {
                let rest = parts.collect::<Vec<_>>().join(" ");
                if rest.trim().is_empty() {
                    anyhow::bail!("--name needs a value");
                }
                name = Some(rest);
                break;
            }
            value if value.starts_with("--name=") => {
                name = Some(value.trim_start_matches("--name=").to_string());
            }
            other => anyhow::bail!("profiles create received unexpected argument: {other}"),
        }
    }
    Ok((id, name))
}

fn profile_confirm_id_args<'a>(args: &'a str, command: &str) -> anyhow::Result<(&'a str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("profiles {command} accepts exactly one id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("profiles {command} needs an id"))?;
    Ok((id, confirmed))
}

fn profile_grants_args(args: &str) -> anyhow::Result<Option<String>> {
    let mut parts = args.split_whitespace();
    let mut from = None;
    while let Some(part) = parts.next() {
        match part {
            "--from" => from = Some(next_profile_option_value(&mut parts, "--from")?.to_string()),
            value if value.starts_with("--from=") => {
                from = Some(value.trim_start_matches("--from=").to_string());
            }
            other => anyhow::bail!("profiles grants received unexpected argument: {other}"),
        }
    }
    Ok(from)
}

fn profile_grant_args(args: &str) -> anyhow::Result<ProfileGrantArgs> {
    let mut parts = args.split_whitespace();
    let mut from = None;
    let mut to = None;
    let mut kind = None;
    let mut resource = None;
    while let Some(part) = parts.next() {
        match part {
            "--from" => from = Some(next_profile_option_value(&mut parts, "--from")?.to_string()),
            "--to" => to = Some(next_profile_option_value(&mut parts, "--to")?.to_string()),
            "--kind" => {
                kind = Some(parse_profile_grant_kind(next_profile_option_value(
                    &mut parts, "--kind",
                )?)?);
            }
            value if value.starts_with("--from=") => {
                from = Some(value.trim_start_matches("--from=").to_string());
            }
            value if value.starts_with("--to=") => {
                to = Some(value.trim_start_matches("--to=").to_string());
            }
            value if value.starts_with("--kind=") => {
                kind = Some(parse_profile_grant_kind(
                    value.trim_start_matches("--kind="),
                )?);
            }
            value if value.starts_with("--") => {
                anyhow::bail!("profiles grant received unexpected argument: {value}");
            }
            value if resource.is_none() => resource = Some(value.to_string()),
            other => anyhow::bail!("profiles grant accepts exactly one resource, got {other}"),
        }
    }
    Ok(ProfileGrantArgs {
        from,
        to: to.ok_or_else(|| anyhow::anyhow!("profiles grant needs --to <profile>"))?,
        kind: kind.ok_or_else(|| anyhow::anyhow!("profiles grant needs --kind <kind>"))?,
        resource: resource.ok_or_else(|| anyhow::anyhow!("profiles grant needs a resource"))?,
    })
}

fn next_profile_option_value<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    flag: &str,
) -> anyhow::Result<&'a str> {
    parts
        .next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

fn parse_profile_grant_kind(value: &str) -> anyhow::Result<ProfileGrantKind> {
    match value {
        "agent" => Ok(ProfileGrantKind::Agent),
        "memory" => Ok(ProfileGrantKind::Memory),
        "tool" => Ok(ProfileGrantKind::Tool),
        "skill" => Ok(ProfileGrantKind::Skill),
        "category" => Ok(ProfileGrantKind::Category),
        other => anyhow::bail!(
            "unsupported profile grant kind {other}; expected agent, memory, tool, skill, or category"
        ),
    }
}

fn profile_summary(profile: &ProfileSummary) -> serde_json::Value {
    serde_json::json!({
        "id": profile.id,
        "name": profile.name,
        "path": profile.path.display().to_string(),
    })
}

fn handle_secrets_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/secrets backends",
                "/secrets list",
                "/secrets show <id>",
                "/secrets set <id> [--label <label>] <value>",
                "/secrets rotate <id> <value>",
                "/secrets delete <id> --confirm",
                "/secret is accepted as an alias for /secrets.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "backends" => {
            if !args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "secrets backends accepts no arguments".into(),
                });
                return;
            }
            let backends = supported_secret_backends();
            push_event(
                app,
                format!("Loaded {} secret backend descriptor(s).", backends.len()),
            );
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&backends)
                    .unwrap_or_else(|_| "<unserializable secret backends>".into()),
            });
        }
        "list" => {
            if !args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "secrets list accepts no arguments".into(),
                });
                return;
            }
            match default_secret_store().list() {
                Ok(records) => {
                    push_event(app, format!("Loaded {} secret record(s).", records.len()));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&records)
                            .unwrap_or_else(|_| "<unserializable secret list>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Secrets list failed: {err}"),
                }),
            }
        }
        "show" => match first_secret_arg(args, "show") {
            Ok(id) => match SecretId::new(id) {
                Ok(id) => match default_secret_store().show(&id) {
                    Ok(record) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&record)
                            .unwrap_or_else(|_| "<unserializable secret record>".into()),
                    }),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Secret show failed: {err}"),
                    }),
                },
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: err.to_string(),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "set" => match secret_set_args(args) {
            Ok((id, label, value)) => match SecretId::new(id) {
                Ok(id) => {
                    let store = default_secret_store();
                    match store.set(id.clone(), SecretValue::new(value), label) {
                        Ok(handle) => match store.show(&id) {
                            Ok(record) => {
                                push_event(app, format!("Stored secret {}.", handle.id.0));
                                app.transcript.push(TranscriptLine {
                                    kind: LineKind::Assistant,
                                    text: serde_json::to_string_pretty(&serde_json::json!({
                                        "handle": handle,
                                        "record": record,
                                    }))
                                    .unwrap_or_else(|_| "<unserializable secret record>".into()),
                                });
                            }
                            Err(err) => app.transcript.push(TranscriptLine {
                                kind: LineKind::Error,
                                text: format!("Secret metadata read failed: {err}"),
                            }),
                        },
                        Err(err) => app.transcript.push(TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Secret set failed: {err}"),
                        }),
                    }
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: err.to_string(),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "rotate" => match secret_rotate_args(args) {
            Ok((id, value)) => match SecretId::new(id) {
                Ok(id) => {
                    let store = default_secret_store();
                    match store.rotate(&id, SecretValue::new(value)) {
                        Ok(handle) => match store.show(&id) {
                            Ok(record) => {
                                push_event(
                                    app,
                                    format!(
                                        "Rotated secret {} to version {}.",
                                        handle.id.0, handle.version
                                    ),
                                );
                                app.transcript.push(TranscriptLine {
                                    kind: LineKind::Assistant,
                                    text: serde_json::to_string_pretty(&serde_json::json!({
                                        "handle": handle,
                                        "record": record,
                                    }))
                                    .unwrap_or_else(|_| "<unserializable secret record>".into()),
                                });
                            }
                            Err(err) => app.transcript.push(TranscriptLine {
                                kind: LineKind::Error,
                                text: format!("Secret metadata read failed: {err}"),
                            }),
                        },
                        Err(err) => app.transcript.push(TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Secret rotate failed: {err}"),
                        }),
                    }
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: err.to_string(),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" | "rm" => match secret_confirm_id_args(args, "delete") {
            Ok((id, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "delete_secret",
                            "secret_id": id,
                            "confirm_command": format!("/secrets delete {id} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable secret confirmation>".into()),
                    });
                    return;
                }
                match SecretId::new(id) {
                    Ok(id) => match default_secret_store().delete(&id) {
                        Ok(true) => push_event(app, format!("Deleted secret {}.", id.0)),
                        Ok(false) => app.transcript.push(TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Secret {} was not stored.", id.0),
                        }),
                        Err(err) => app.transcript.push(TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Secret delete failed: {err}"),
                        }),
                    },
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: err.to_string(),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Secrets command needs backends, list, show, set, rotate, delete, or help."
                .into(),
        }),
    }
}

fn first_secret_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("secrets {command} needs an argument"))
}

fn secret_set_args(args: &str) -> anyhow::Result<(&str, Option<String>, String)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("secrets set needs a secret id"))?;
    let mut label = None;
    let mut value_parts = Vec::new();
    while let Some(part) = parts.next() {
        match part {
            "--label" if value_parts.is_empty() => {
                label = Some(next_secret_option_value(&mut parts, "--label")?.to_string());
            }
            value if value.starts_with("--label=") && value_parts.is_empty() => {
                label = Some(value.trim_start_matches("--label=").to_string());
            }
            value if value.starts_with("--") && value_parts.is_empty() => {
                anyhow::bail!("secrets set received unexpected argument: {value}");
            }
            value => {
                value_parts.push(value);
                value_parts.extend(parts);
                break;
            }
        }
    }
    let value = value_parts.join(" ");
    if value.trim().is_empty() {
        anyhow::bail!("secrets set needs a value");
    }
    Ok((id, label, value))
}

fn secret_rotate_args(args: &str) -> anyhow::Result<(&str, String)> {
    let (id, value) = args
        .trim()
        .split_once(char::is_whitespace)
        .ok_or_else(|| anyhow::anyhow!("secrets rotate needs an id and value"))?;
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("secrets rotate needs a value");
    }
    Ok((id, value.to_string()))
}

fn secret_confirm_id_args<'a>(args: &'a str, command: &str) -> anyhow::Result<(&'a str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("secrets {command} accepts exactly one id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("secrets {command} needs an id"))?;
    Ok((id, confirmed))
}

fn next_secret_option_value<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    flag: &str,
) -> anyhow::Result<&'a str> {
    parts
        .next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

fn handle_prompts_slash(
    app: &mut App,
    rest: &str,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: prompt_slash_help_text(),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match prompt_agent_arg(args, "list") {
            Ok(agent) => match PromptStore::from_env().list_scoped(agent.as_deref()) {
                Ok(prompts) => {
                    push_event(app, format!("Loaded {} prompt(s).", prompts.len()));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(
                            &prompts.iter().map(prompt_summary).collect::<Vec<_>>(),
                        )
                        .unwrap_or_else(|_| "<unserializable prompt list>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Prompt list failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "show" => match prompt_named_args(args, "show") {
            Ok((name, agent)) => match PromptStore::from_env().get_scoped(agent.as_deref(), name) {
                Ok(Some(prompt)) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&prompt)
                        .unwrap_or_else(|_| "<unserializable prompt>".into()),
                }),
                Ok(None) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Prompt {name:?} not found."),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Prompt show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "save" => match prompt_save_args(args) {
            Ok((name, agent, text)) => {
                match PromptStore::from_env().save_scoped(agent.as_deref(), name, &text) {
                    Ok(prompt) => {
                        push_event(app, format!("Saved prompt {}.", prompt.name));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&prompt_summary(&prompt))
                                .unwrap_or_else(|_| "<unserializable prompt>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Prompt save failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "use" => match prompt_named_args(args, "use") {
            Ok((name, prompt_agent)) => {
                match load_prompt_for_shortcut(name, prompt_agent.as_deref(), Some(&agent.id)) {
                    Ok(prompt) => {
                        let name = prompt.name.clone();
                        app.input = prompt.body;
                        push_event(app, format!("Loaded prompt {name} into input."));
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Use prompt failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "preview" => match prompt_named_args(args, "preview") {
            Ok((name, prompt_agent)) => {
                match load_prompt_for_shortcut(name, prompt_agent.as_deref(), Some(&agent.id)) {
                    Ok(prompt) => {
                        push_context_preview(app, registry, agent, &prompt.body);
                        push_event(app, format!("Previewed saved prompt context: {}", prompt.name));
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Preview prompt failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match prompt_export_args(args) {
            Ok((name, path, agent)) => {
                match PromptStore::from_env().export_scoped(agent.as_deref(), name, path) {
                    Ok(prompt) => {
                        push_event(app, format!("Exported prompt {} to {path}.", prompt.name));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&prompt_summary(&prompt))
                                .unwrap_or_else(|_| "<unserializable prompt>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Prompt export failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match prompt_import_args(args) {
            Ok((path, agent)) => {
                match PromptStore::from_env().import_file(path, agent.as_deref()) {
                    Ok(prompt) => {
                        push_event(app, format!("Imported prompt {}.", prompt.name));
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&prompt_summary(&prompt))
                                .unwrap_or_else(|_| "<unserializable prompt>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Prompt import failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" => match prompt_delete_args(args) {
            Ok((name, agent, confirmed)) => {
                if !confirmed {
                    let agent_flag = agent
                        .as_ref()
                        .map(|agent| format!(" --agent {agent}"))
                        .unwrap_or_default();
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "delete_prompt",
                            "name": name,
                            "agent": agent,
                            "confirm_command": format!("/prompts delete {name}{agent_flag} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable prompt confirmation>".into()),
                    });
                    return;
                }
                match PromptStore::from_env().delete_scoped(agent.as_deref(), name) {
                    Ok(true) => push_event(app, format!("Deleted prompt {name}.")),
                    Ok(false) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Prompt {name:?} not found."),
                    }),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Prompt delete failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text:
                "Prompts command needs list, show, save, use, preview, export, import, delete, or help."
                    .into(),
        }),
    }
}

fn load_prompt_for_shortcut(
    name: &str,
    agent: Option<&str>,
    active_agent: Option<&str>,
) -> anyhow::Result<PromptDoc> {
    let scope = agent.or(active_agent);
    PromptStore::from_env()
        .resolve_for_agent(scope, name)?
        .ok_or_else(|| anyhow::anyhow!("Prompt {name:?} not found."))
}

fn prompt_agent_arg(args: &str, command: &str) -> anyhow::Result<Option<String>> {
    let mut parts = args.split_whitespace();
    let mut agent = None;
    while let Some(part) = parts.next() {
        match part {
            "--agent" => agent = Some(next_prompt_option_value(&mut parts, "--agent")?.to_string()),
            value if value.starts_with("--agent=") => {
                agent = Some(value.trim_start_matches("--agent=").to_string());
            }
            other => anyhow::bail!("prompts {command} received unexpected argument: {other}"),
        }
    }
    Ok(agent)
}

fn prompt_named_args<'a>(
    args: &'a str,
    command: &str,
) -> anyhow::Result<(&'a str, Option<String>)> {
    let mut parts = args.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("prompts {command} needs a prompt name"))?;
    let mut agent = None;
    while let Some(part) = parts.next() {
        match part {
            "--agent" => agent = Some(next_prompt_option_value(&mut parts, "--agent")?.to_string()),
            value if value.starts_with("--agent=") => {
                agent = Some(value.trim_start_matches("--agent=").to_string());
            }
            other => anyhow::bail!("prompts {command} received unexpected argument: {other}"),
        }
    }
    Ok((name, agent))
}

fn prompt_save_args(args: &str) -> anyhow::Result<(&str, Option<String>, String)> {
    let mut parts = args.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("prompts save needs a prompt name"))?;
    let mut agent = None;
    let mut text_parts = Vec::new();
    while let Some(part) = parts.next() {
        match part {
            "--agent" if text_parts.is_empty() => {
                agent = Some(next_prompt_option_value(&mut parts, "--agent")?.to_string());
            }
            value if value.starts_with("--agent=") && text_parts.is_empty() => {
                agent = Some(value.trim_start_matches("--agent=").to_string());
            }
            value if value.starts_with("--") && text_parts.is_empty() => {
                anyhow::bail!("prompts save received unexpected argument: {value}");
            }
            value => {
                text_parts.push(value);
                text_parts.extend(parts);
                break;
            }
        }
    }
    let text = text_parts.join(" ");
    let text = text.trim();
    if text.is_empty() {
        anyhow::bail!("prompts save needs prompt text");
    }
    Ok((name, agent, text.to_string()))
}

fn prompt_delete_args(args: &str) -> anyhow::Result<(&str, Option<String>, bool)> {
    let (name, agent, confirmed) = prompt_named_confirm_args(args, "delete")?;
    Ok((name, agent, confirmed))
}

fn prompt_export_args<'a>(args: &'a str) -> anyhow::Result<(&'a str, &'a str, Option<String>)> {
    let mut parts = args.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("prompts export needs a prompt name"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("prompts export needs a path"))?;
    let mut agent = None;
    while let Some(part) = parts.next() {
        match part {
            "--agent" => agent = Some(next_prompt_option_value(&mut parts, "--agent")?.to_string()),
            value if value.starts_with("--agent=") => {
                agent = Some(value.trim_start_matches("--agent=").to_string());
            }
            other => anyhow::bail!("prompts export received unexpected argument: {other}"),
        }
    }
    Ok((name, path, agent))
}

fn prompt_import_args(args: &str) -> anyhow::Result<(&str, Option<String>)> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("prompts import needs a path"))?;
    let mut agent = None;
    while let Some(part) = parts.next() {
        match part {
            "--agent" => agent = Some(next_prompt_option_value(&mut parts, "--agent")?.to_string()),
            value if value.starts_with("--agent=") => {
                agent = Some(value.trim_start_matches("--agent=").to_string());
            }
            other => anyhow::bail!("prompts import received unexpected argument: {other}"),
        }
    }
    Ok((path, agent))
}

fn prompt_named_confirm_args<'a>(
    args: &'a str,
    command: &str,
) -> anyhow::Result<(&'a str, Option<String>, bool)> {
    let mut parts = args.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("prompts {command} needs a prompt name"))?;
    let mut agent = None;
    let mut confirmed = false;
    while let Some(part) = parts.next() {
        match part {
            "--confirm" => confirmed = true,
            "--agent" => agent = Some(next_prompt_option_value(&mut parts, "--agent")?.to_string()),
            value if value.starts_with("--agent=") => {
                agent = Some(value.trim_start_matches("--agent=").to_string());
            }
            other => anyhow::bail!("prompts {command} received unexpected argument: {other}"),
        }
    }
    Ok((name, agent, confirmed))
}

fn next_prompt_option_value<'a>(
    parts: &mut impl Iterator<Item = &'a str>,
    flag: &str,
) -> anyhow::Result<&'a str> {
    parts
        .next()
        .filter(|value| !value.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
}

fn prompt_summary(prompt: &PromptDoc) -> serde_json::Value {
    serde_json::json!({
        "name": prompt.name,
        "agent_id": prompt.agent_id,
        "body_preview": compact_preview(&prompt.body, 160),
    })
}

fn handle_skills_slash(
    app: &mut App,
    rest: &str,
    registry: &Arc<ToolRegistry>,
    agent: &mut AgentConfig,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/skills on|off|status|preview",
                "/skills preview [prompt]",
                "/skills list",
                "/skills show <id>",
                "/skills inspect <id>",
                "/skills import-openclaw <path>",
                "/skills import-doc <path>",
                "/skills export <id> <path>",
                "/skills allow <id> --confirm",
                "/skills quarantine <id> --confirm",
                "/skill is accepted as an alias for /skills.",
            ]
            .join("\n"),
        });
        return;
    }
    let rest_lower = rest.to_lowercase();
    match rest_lower.as_str() {
        "on" | "enable" | "enabled" => {
            options.load_skills = true;
            refresh_agent_runtime_policy(app, agent, options);
            push_event(
                app,
                "Runtime skill loading enabled for future TUI runs.".into(),
            );
            return;
        }
        "off" | "disable" | "disabled" => {
            options.load_skills = false;
            refresh_agent_runtime_policy(app, agent, options);
            push_event(
                app,
                "Runtime skill loading disabled for future TUI runs.".into(),
            );
            return;
        }
        "status" => {
            push_event(
                app,
                format!(
                    "Runtime skill loading is {}.",
                    if options.load_skills {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
            );
            return;
        }
        _ => {}
    }
    if rest_lower == "preview" || rest_lower.starts_with("preview ") {
        let prompt = rest
            .get("preview".len()..)
            .map(str::trim)
            .unwrap_or_default();
        options.load_skills = true;
        refresh_agent_runtime_policy(app, agent, options);
        push_context_preview(app, registry, agent, prompt);
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match SkillRegistry::from_env().list() {
            Ok(docs) => {
                push_event(app, format!("Loaded {} skill(s).", docs.len()));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &docs.iter().map(skill_summary).collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable skill list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Skill list failed: {err}"),
            }),
        },
        "show" | "inspect" => match first_skill_arg(args, "show") {
            Ok(id) => match SkillRegistry::from_env().inspect(id) {
                Ok(doc) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&doc)
                        .unwrap_or_else(|_| "<unserializable skill>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Skill show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import-openclaw" | "install" => match skill_path_arg(args, "import-openclaw") {
            Ok(path) => match SkillRegistry::from_env().import_openclaw(path) {
                Ok(doc) => {
                    push_event(app, format!("Imported quarantined skill {}", doc.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&skill_summary(&doc))
                            .unwrap_or_else(|_| "<unserializable skill>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Skill import failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import-doc" | "import" => match skill_path_arg(args, "import-doc") {
            Ok(path) => match SkillRegistry::from_env().import_doc(path) {
                Ok(doc) => {
                    push_event(app, format!("Imported quarantined skill {}", doc.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&skill_summary(&doc))
                            .unwrap_or_else(|_| "<unserializable skill>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Skill doc import failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match skill_export_args(args) {
            Ok((id, path)) => match SkillRegistry::from_env().export(id, path) {
                Ok(doc) => push_event(app, format!("Exported skill {} to {path}", doc.id)),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Skill export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "allow" | "quarantine" => handle_skill_review_slash(app, command, args),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Skills command needs on, off, status, preview, list, show, inspect, import-openclaw, import-doc, export, allow, quarantine, or help.".into(),
        }),
    }
}

fn handle_skill_review_slash(app: &mut App, action: &str, args: &str) {
    let (id, confirmed) = match skill_review_args(args, action) {
        Ok(parsed) => parsed,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            });
            return;
        }
    };
    if !confirmed {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: serde_json::to_string_pretty(&serde_json::json!({
                "pending_action": format!("{action}_skill"),
                "skill_id": id,
                "confirm_command": format!("/skills {action} {id} --confirm"),
            }))
            .unwrap_or_else(|_| "<unserializable skill confirmation>".into()),
        });
        return;
    }
    let result = if action == "allow" {
        SkillRegistry::from_env().allow(id)
    } else {
        SkillRegistry::from_env().quarantine(id)
    };
    match result {
        Ok(doc) => {
            let verb = if action == "allow" {
                "Allowed"
            } else {
                "Quarantined"
            };
            push_event(app, format!("{verb} skill {}", doc.id));
        }
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Skill {action} failed: {err}"),
        }),
    }
}

fn first_skill_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("skills {command} needs an argument"))
}

fn skill_path_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("skills {command} needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("skills {command} accepts exactly one path");
    }
    Ok(path)
}

fn skill_export_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("skills export needs a skill id"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("skills export needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("skills export accepts exactly a skill id and path");
    }
    Ok((id, path))
}

fn skill_review_args<'a>(args: &'a str, action: &str) -> anyhow::Result<(&'a str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("skills {action} accepts exactly a skill id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("skills {action} needs a skill id"))?;
    Ok((id, confirmed))
}

fn skill_summary(doc: &SkillDoc) -> serde_json::Value {
    let high_risk_findings = doc
        .findings
        .iter()
        .filter(|finding| finding.severity == agent_adapters::FindingSeverity::High)
        .count();
    serde_json::json!({
        "id": doc.id,
        "name": doc.name,
        "description": doc.description,
        "categories": doc.categories,
        "quarantined": doc.quarantined,
        "estimated_tokens": doc.estimated_tokens,
        "findings": doc.findings.len(),
        "high_risk_findings": high_risk_findings,
        "digest": doc.digest,
        "provenance": doc.provenance,
    })
}

fn handle_storage_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: ["/storage report", "/storage prune-cache <days> [--apply]"].join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "report" => match StoragePaths::from_env().storage_report() {
            Ok(report) => {
                push_event(app, storage_report_event_text(&report));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&report)
                        .unwrap_or_else(|_| "<unserializable storage report>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Storage report failed: {err}"),
            }),
        },
        "prune-cache" => match storage_prune_cache_args(args) {
            Ok((days, apply)) => {
                match StoragePaths::from_env().prune_cache_retention(days, !apply) {
                    Ok(result) => {
                        let mode = if apply { "applied" } else { "planned" };
                        push_event(
                            app,
                            format!(
                                "Cache retention {mode}: {} file(s), {} bytes.",
                                result.plan.total_files, result.plan.total_bytes
                            ),
                        );
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&result)
                                .unwrap_or_else(|_| "<unserializable storage prune result>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Storage prune failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Storage command needs report, prune-cache, or help.".into(),
        }),
    }
}

fn storage_report_event_text(report: &StorageReport) -> String {
    let quota = report.quota_bytes.map(|quota| {
        let remaining = report.quota_remaining_bytes.unwrap_or_default();
        let status = if report.quota_exceeded {
            "over quota"
        } else {
            "within quota"
        };
        format!(" Quota: {quota} bytes, remaining {remaining} bytes ({status}).")
    });
    format!(
        "Storage: {} bytes across {} file(s).{}",
        report.total_bytes,
        report.total_files,
        quota.unwrap_or_default()
    )
}

fn storage_prune_cache_args(args: &str) -> anyhow::Result<(u64, bool)> {
    let mut days = None;
    let mut apply = false;
    for part in args.split_whitespace() {
        if part == "--apply" {
            apply = true;
        } else if days.is_none() {
            days = Some(part.parse::<u64>()?);
        } else {
            anyhow::bail!("storage prune-cache accepts exactly days and optional --apply");
        }
    }
    let days = days.ok_or_else(|| anyhow::anyhow!("storage prune-cache needs days"))?;
    Ok((days, apply))
}

fn handle_bundles_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/bundles backup <path>",
                "/bundles export <path>",
                "/bundles import <path> --confirm",
                "/bundle is accepted as an alias for /bundles.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "export" | "backup" => match bundle_path_arg(args, command) {
            Ok(path) => match export_bundle(path) {
                Ok(manifest) => {
                    push_event(
                        app,
                        format!("Exported bundle for profile {} to {path}", manifest.profile),
                    );
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&manifest)
                            .unwrap_or_else(|_| "<unserializable bundle manifest>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Bundle export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match bundle_import_args(args) {
            Ok((path, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "import_bundle",
                            "path": path,
                            "confirm_command": format!("/bundles import {path} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable bundle confirmation>".into()),
                    });
                    return;
                }
                match import_bundle(path) {
                    Ok(manifest) => {
                        push_event(
                            app,
                            format!("Imported bundle for profile {}.", manifest.profile),
                        );
                        app.transcript.push(TranscriptLine {
                            kind: LineKind::Assistant,
                            text: serde_json::to_string_pretty(&manifest)
                                .unwrap_or_else(|_| "<unserializable bundle manifest>".into()),
                        });
                    }
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Bundle import failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Bundles command needs backup, export, import, or help.".into(),
        }),
    }
}

fn bundle_path_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("bundles {command} needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("bundles {command} accepts exactly one path");
    }
    Ok(path)
}

fn bundle_import_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut path = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if path.is_none() {
            path = Some(part);
        } else {
            anyhow::bail!("bundles import accepts exactly one path and optional --confirm");
        }
    }
    let path = path.ok_or_else(|| anyhow::anyhow!("bundles import needs a path"))?;
    Ok((path, confirmed))
}

fn handle_adapters_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/adapters list",
                "/adapters doctor",
                "/adapters show <id>",
                "/adapters inspect <path>",
                "/adapters install-skill <id>",
                "/adapters import <path>",
                "/adapters import-manifest <path>",
                "/adapters export <id> <path>",
                "/adapters quarantine <id>",
                "/adapters allow <id> --confirm",
                "/adapters clawhub search <catalog> [query]",
                "/adapters clawhub inspect <catalog> <id>",
                "/adapters clawhub pin <catalog> <id>",
                "/adapters clawhub install <catalog> <id>",
                "/adapter is accepted as an alias for /adapters.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "list" => match AdapterRegistry::from_env().list() {
            Ok(packages) => {
                push_event(app, format!("Loaded {} adapter manifest(s).", packages.len()));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &packages
                            .iter()
                            .map(adapter_package_summary)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable adapter list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Adapter list failed: {err}"),
            }),
        },
        "doctor" => match AdapterRegistry::from_env().doctor_report() {
            Ok(report) => {
                push_event(app, format!("Adapter doctor status {:?}.", report.status));
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&report)
                        .unwrap_or_else(|_| "<unserializable adapter doctor report>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Adapter doctor failed: {err}"),
            }),
        },
        "show" => match first_adapter_arg(args, "show") {
            Ok(id) => match AdapterRegistry::from_env().show(id) {
                Ok(package) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&package)
                        .unwrap_or_else(|_| "<unserializable adapter manifest>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Adapter show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "inspect" => match first_adapter_arg(args, "inspect") {
            Ok(path) => match inspect_source(path) {
                Ok(package) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&adapter_package_summary(&package))
                        .unwrap_or_else(|_| "<unserializable adapter manifest>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Adapter inspect failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "install-skill" => match first_adapter_arg(args, "install-skill") {
            Ok(id) => match AdapterRegistry::from_env().show(id) {
                Ok(package) => {
                    match SkillRegistry::from_env().import_openclaw_adapter_package(&package) {
                        Ok(doc) => {
                            push_event(
                                app,
                                format!("Installed adapter {id} as quarantined skill {}", doc.id),
                            );
                            app.transcript.push(TranscriptLine {
                                kind: LineKind::Assistant,
                                text: serde_json::to_string_pretty(&skill_summary(&doc))
                                    .unwrap_or_else(|_| "<unserializable skill>".into()),
                            });
                        }
                        Err(err) => app.transcript.push(TranscriptLine {
                            kind: LineKind::Error,
                            text: format!("Adapter skill install failed: {err}"),
                        }),
                    }
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Adapter show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match first_adapter_arg(args, "import") {
            Ok(path) => match AdapterRegistry::from_env().import(path) {
                Ok(package) => {
                    push_event(app, format!("Imported quarantined adapter {}", package.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&adapter_package_summary(&package))
                            .unwrap_or_else(|_| "<unserializable adapter manifest>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Adapter import failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import-manifest" => match first_adapter_arg(args, "import-manifest") {
            Ok(path) => match AdapterRegistry::from_env().import_manifest(path) {
                Ok(package) => {
                    push_event(app, format!("Imported quarantined adapter {}", package.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&adapter_package_summary(&package))
                            .unwrap_or_else(|_| "<unserializable adapter manifest>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Adapter manifest import failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match adapter_export_args(args) {
            Ok((id, path)) => match AdapterRegistry::from_env().export_manifest(id, path) {
                Ok(package) => push_event(
                    app,
                    format!("Exported adapter manifest {} to {path}", package.id),
                ),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Adapter export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "quarantine" => match first_adapter_arg(args, "quarantine") {
            Ok(id) => match AdapterRegistry::from_env().quarantine(id) {
                Ok(package) => push_event(app, format!("Quarantined adapter {}", package.id)),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Adapter quarantine failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "allow" => handle_adapter_allow_slash(app, args),
        "clawhub" => handle_adapter_clawhub_slash(app, args),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Adapters command needs list, doctor, show, inspect, install-skill, import, import-manifest, export, quarantine, allow, clawhub, or help.".into(),
        }),
    }
}

fn first_adapter_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("adapters {command} needs an argument"))
}

fn adapter_export_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("adapters export needs an adapter id"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("adapters export needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("adapters export accepts exactly an adapter id and path");
    }
    Ok((id, path))
}

fn handle_adapter_clawhub_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/adapters clawhub search <catalog> [query]",
                "/adapters clawhub inspect <catalog> <id>",
                "/adapters clawhub pin <catalog> <id>",
                "/adapters clawhub install <catalog> <id>",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "search" => match clawhub_search_args(args) {
            Ok((catalog, query)) => match ClawHubProvider::from_catalog(catalog) {
                Ok(provider) => {
                    let entries = provider.search(query);
                    push_event(app, format!("Loaded {} ClawHub entries.", entries.len()));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&entries)
                            .unwrap_or_else(|_| "<unserializable ClawHub entries>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("ClawHub search failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "inspect" => match clawhub_entry_args(args, "inspect") {
            Ok((catalog, id)) => match ClawHubProvider::from_catalog(catalog)
                .and_then(|provider| provider.inspect(id))
            {
                Ok(inspection) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&inspection)
                        .unwrap_or_else(|_| "<unserializable ClawHub inspection>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("ClawHub inspect failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "pin" => match clawhub_entry_args(args, "pin") {
            Ok((catalog, id)) => {
                match ClawHubProvider::from_catalog(catalog).and_then(|provider| provider.pin(id)) {
                    Ok(pin) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&pin)
                            .unwrap_or_else(|_| "<unserializable ClawHub pin>".into()),
                    }),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("ClawHub pin failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "install" => match clawhub_entry_args(args, "install") {
            Ok((catalog, id)) => match ClawHubProvider::from_catalog(catalog)
                .and_then(|provider| provider.install(id, &AdapterRegistry::from_env()))
            {
                Ok(package) => {
                    push_event(
                        app,
                        format!("Installed ClawHub adapter {} into quarantine.", package.id),
                    );
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&adapter_package_summary(&package))
                            .unwrap_or_else(|_| "<unserializable adapter manifest>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("ClawHub install failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Adapters clawhub command needs search, inspect, pin, install, or help.".into(),
        }),
    }
}

fn clawhub_search_args(args: &str) -> anyhow::Result<(&str, Option<&str>)> {
    let trimmed = args.trim();
    let (catalog, query) = trimmed
        .split_once(char::is_whitespace)
        .map(|(catalog, query)| (catalog.trim(), query.trim()))
        .unwrap_or((trimmed, ""));
    if catalog.is_empty() {
        anyhow::bail!("adapters clawhub search needs a catalog path");
    }
    Ok((catalog, (!query.is_empty()).then_some(query)))
}

fn clawhub_entry_args<'a>(args: &'a str, command: &str) -> anyhow::Result<(&'a str, &'a str)> {
    let mut parts = args.split_whitespace();
    let catalog = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("adapters clawhub {command} needs a catalog path"))?;
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("adapters clawhub {command} needs an entry id"))?;
    if parts.next().is_some() {
        anyhow::bail!("adapters clawhub {command} accepts exactly a catalog path and entry id");
    }
    Ok((catalog, id))
}

fn handle_adapter_allow_slash(app: &mut App, args: &str) {
    let mut parts = args.split_whitespace();
    let Some(id) = parts.next() else {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "adapters allow needs an adapter id".into(),
        });
        return;
    };
    let confirmed = parts.any(|part| part == "--confirm");
    if !confirmed {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: serde_json::to_string_pretty(&serde_json::json!({
                "pending_action": "allow_adapter",
                "adapter_id": id,
                "confirm_command": format!("/adapters allow {id} --confirm"),
            }))
            .unwrap_or_else(|_| "<unserializable adapter confirmation>".into()),
        });
        return;
    }
    match AdapterRegistry::from_env().allow(id) {
        Ok(package) => push_event(app, format!("Allowed adapter {}", package.id)),
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Adapter allow failed: {err}"),
        }),
    }
}

fn adapter_package_summary(package: &NormalizedPackage) -> serde_json::Value {
    serde_json::json!({
        "id": package.id,
        "adapter": package.adapter,
        "quarantined": package.quarantined,
        "digest": package.digest,
        "source": package.source,
        "capabilities": package.capabilities.len(),
        "runtime_capabilities": package.capabilities.iter().filter(|capability| capability.runtime.is_some()).count(),
        "findings": package.findings.len(),
        "secret_requirements": package.secret_requirements.len(),
    })
}

fn handle_hooks_slash(app: &mut App, rest: &str, agent: &AgentConfig) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/hooks review [last|run-id]",
                "/hooks list",
                "/hooks policy",
                "/hooks available",
                "/hooks disable <hook-id> [--agent] --confirm",
                "/hooks enable <hook-id> [--agent] --confirm",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "review" => handle_hooks_review(app, args),
        "list" | "policy" => {
            if let Err(err) = hook_view_args(command, args) {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Hook policy list failed: {err}"),
                });
                return;
            }
            match ConfigResolver::from_env().lifecycle_hook_policy_layers_for_agent(&agent.id) {
                Ok(policy) => {
                    push_event(
                        app,
                        format!(
                            "Lifecycle hook policy: {} disabled from {}",
                            policy.effective_disabled_lifecycle_hooks.len(),
                            policy.effective_source
                        ),
                    );
                    let effective_hooks = policy.effective_disabled_lifecycle_hooks.clone();
                    app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&serde_json::json!({
                        "agent_id": agent.id,
                        "profile": policy.profile,
                        "effective_source": policy.effective_source,
                        "disabled_lifecycle_hooks": effective_hooks,
                        "effective_disabled_lifecycle_hooks": policy.effective_disabled_lifecycle_hooks,
                        "global_disabled_lifecycle_hooks": policy.global_disabled_lifecycle_hooks,
                        "profile_disabled_lifecycle_hooks": policy.profile_disabled_lifecycle_hooks,
                        "agent_disabled_lifecycle_hooks": policy.agent_disabled_lifecycle_hooks,
                    }))
                    .unwrap_or_else(|_| "<unserializable hook policy>".into()),
                });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Hook policy list failed: {err}"),
                }),
            }
        }
        "available" => match hook_view_args(command, args) {
            Ok(()) => handle_hooks_available(app, &agent.id),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Hook catalog failed: {err}"),
            }),
        },
        "disable" | "enable" => handle_hook_policy_change(app, command, args, &agent.id),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Hooks command needs review, list, policy, available, disable, enable, or help."
                .into(),
        }),
    }
}

fn hook_view_args(command: &str, args: &str) -> anyhow::Result<()> {
    if args.trim().is_empty() {
        Ok(())
    } else {
        anyhow::bail!("usage: /hooks {command}")
    }
}

fn handle_hooks_available(app: &mut App, agent_id: &str) {
    let result = (|| -> anyhow::Result<serde_json::Value> {
        let policy = ConfigResolver::from_env().lifecycle_hook_policy_layers_for_agent(agent_id)?;
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
        Ok(serde_json::json!({
            "agent_id": policy.agent_id,
            "effective_source": policy.effective_source,
            "hooks": records,
        }))
    })();
    match result {
        Ok(value) => {
            let count = value["hooks"].as_array().map(Vec::len).unwrap_or_default();
            push_event(app, format!("Available lifecycle hooks: {count}"));
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&value)
                    .unwrap_or_else(|_| "<unserializable hook catalog>".into()),
            });
        }
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Hook catalog failed: {err}"),
        }),
    }
}

fn handle_hooks_review(app: &mut App, args: &str) {
    let run_id = match hooks_review_run_id(args, app.last_run_id) {
        Ok(run_id) => run_id,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Hook review failed: {err}"),
            });
            return;
        }
    };

    match open_event_store().and_then(|store| store.try_events(run_id).map_err(Into::into)) {
        Ok(events) => {
            let plan = hook_remediation_plan(&events);
            push_event(app, format!("Hook review: {} issue(s)", plan.len()));
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&plan)
                    .unwrap_or_else(|_| "<unserializable hook review>".into()),
            });
        }
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Hook review failed: {err}"),
        }),
    }
}

fn hooks_review_run_id(args: &str, last_run_id: Option<RunId>) -> anyhow::Result<RunId> {
    let selector = args.trim();
    if selector.is_empty() || selector == "last" {
        return last_run_id.ok_or_else(|| anyhow::anyhow!("needs a run id or a previous run"));
    }
    uuid::Uuid::parse_str(selector)
        .map(RunId)
        .map_err(Into::into)
}

fn handle_hook_policy_change(app: &mut App, command: &str, args: &str, agent_id: &str) {
    let (hook_id, confirmed, agent_scope) = match parse_hook_policy_change_args(args) {
        Ok(parsed) => parsed,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Hook policy change failed: {err}"),
            });
            return;
        }
    };
    let disabling = command == "disable";
    let scope = if agent_scope {
        "agent"
    } else {
        "active_profile"
    };
    if !confirmed {
        let confirm_command = if agent_scope {
            format!("/hooks {command} {hook_id} --agent --confirm")
        } else {
            format!("/hooks {command} {hook_id} --confirm")
        };
        push_event(
            app,
            format!("Hook policy confirmation required for {hook_id}"),
        );
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: serde_json::to_string_pretty(&serde_json::json!({
                "hook_id": hook_id,
                "action": command,
                "scope": scope,
                "agent_id": agent_scope.then_some(agent_id),
                "effect": if disabling {
                    if agent_scope {
                        "future runs for this agent will use this agent-level hook policy"
                    } else {
                        "future runs in this active profile will skip this hook unless agent config overrides the list"
                    }
                } else {
                    if agent_scope {
                        "future runs for this agent can load this hook again unless another layer disables it"
                    } else {
                        "future runs in this active profile can load this hook again unless another layer disables it"
                    }
                },
                "confirm_command": confirm_command,
            }))
            .unwrap_or_else(|_| "<unserializable hook policy plan>".into()),
        });
        return;
    }
    let resolver = ConfigResolver::from_env();
    let result = if agent_scope {
        resolver.set_agent_lifecycle_hook_disabled(agent_id, &hook_id, disabling)
    } else {
        resolver.set_profile_lifecycle_hook_disabled(&hook_id, disabling)
    };
    match result {
        Ok(hooks) => {
            push_event(
                app,
                format!(
                    "Lifecycle hook policy updated ({scope}): {} disabled",
                    hooks.len()
                ),
            );
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&serde_json::json!({
                    "scope": scope,
                    "agent_id": agent_scope.then_some(agent_id),
                    "disabled_lifecycle_hooks": hooks,
                }))
                .unwrap_or_else(|_| "<unserializable hook policy>".into()),
            });
        }
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Hook policy change failed: {err}"),
        }),
    }
}

fn parse_hook_policy_change_args(rest: &str) -> anyhow::Result<(String, bool, bool)> {
    let mut parts = rest.split_whitespace();
    let hook_id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("hook policy change needs a hook id"))?
        .to_string();
    let mut confirmed = false;
    let mut agent_scope = false;
    for part in parts {
        match part {
            "--confirm" => confirmed = true,
            "--agent" => agent_scope = true,
            other => anyhow::bail!("unknown hook policy option: {other}"),
        }
    }
    Ok((hook_id, confirmed, agent_scope))
}

fn compact_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/compact" || trimmed == "/compactions" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/compact ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/compactions ").map(str::trim)
    }
}

fn handle_compact_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/compact keep [run-id|last]",
                "/compact keep-run [run-id|last]",
                "/compact list",
                "/compact show <id>",
                "/compact export <id> <path>",
                "/compact import <path>",
                "/compact delete <id> --confirm",
                "/compact status",
                "/compact dismiss",
                "/compactions is accepted as an alias for /compact record commands.",
            ]
            .join("\n"),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "keep" | "keep-run" => handle_compact_keep(app, args),
        "list" => match CompactionStore::from_env().list() {
            Ok(records) => {
                push_event(
                    app,
                    format!("Loaded {} compaction record(s).", records.len()),
                );
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(
                        &records
                            .iter()
                            .map(compaction_record_summary)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap_or_else(|_| "<unserializable compaction list>".into()),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Compact list failed: {err}"),
            }),
        },
        "show" => match first_compact_arg(args, "show") {
            Ok(id) => match CompactionStore::from_env().show(id) {
                Ok(record) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&record)
                        .unwrap_or_else(|_| "<unserializable compaction record>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Compact show failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "export" => match compact_export_args(args) {
            Ok((id, path)) => match CompactionStore::from_env().export_record(id, path) {
                Ok(record) => {
                    push_event(app, format!("Exported compaction {} to {path}", record.id))
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Compact export failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "import" => match compact_path_arg(args, "import") {
            Ok(path) => match CompactionStore::from_env().import_record(path) {
                Ok(record) => {
                    push_event(app, format!("Imported compaction {}", record.id));
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&record)
                            .unwrap_or_else(|_| "<unserializable compaction record>".into()),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Compact import failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "delete" | "rm" => match compact_delete_args(args) {
            Ok((id, confirmed)) => {
                if !confirmed {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Assistant,
                        text: serde_json::to_string_pretty(&serde_json::json!({
                            "pending_action": "delete_compaction",
                            "compaction_id": id,
                            "confirm_command": format!("/compact delete {id} --confirm"),
                        }))
                        .unwrap_or_else(|_| "<unserializable compaction confirmation>".into()),
                    });
                    return;
                }
                match CompactionStore::from_env().remove(id) {
                    Ok(()) => push_event(app, format!("Deleted compaction {id}.")),
                    Err(err) => app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Compact delete failed: {err}"),
                    }),
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            }),
        },
        "status" => {
            let text = app
                .pending_auto_compaction_run
                .map(|run_id| format!("Auto compaction ready from run {run_id}."))
                .unwrap_or_else(|| "No auto compaction is pending.".into());
            push_event(app, text);
        }
        "dismiss" | "cancel" => {
            app.pending_auto_compaction_run = None;
            push_event(app, "Auto compaction keep prompt dismissed.".into());
        }
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text:
                "Compact command needs keep, keep-run, list, show, export, import, delete, status, dismiss, or help.".into(),
        }),
    }
}

fn first_compact_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    args.split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("compact {command} needs an argument"))
}

fn compact_path_arg<'a>(args: &'a str, command: &str) -> anyhow::Result<&'a str> {
    let mut parts = args.split_whitespace();
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("compact {command} needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("compact {command} accepts exactly one path");
    }
    Ok(path)
}

fn compact_export_args(args: &str) -> anyhow::Result<(&str, &str)> {
    let mut parts = args.split_whitespace();
    let id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("compact export needs a compaction id"))?;
    let path = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("compact export needs a path"))?;
    if parts.next().is_some() {
        anyhow::bail!("compact export accepts exactly a compaction id and path");
    }
    Ok((id, path))
}

fn compact_delete_args(args: &str) -> anyhow::Result<(&str, bool)> {
    let mut id = None;
    let mut confirmed = false;
    for part in args.split_whitespace() {
        if part == "--confirm" {
            confirmed = true;
        } else if id.is_none() {
            id = Some(part);
        } else {
            anyhow::bail!("compact delete accepts exactly a compaction id and optional --confirm");
        }
    }
    let id = id.ok_or_else(|| anyhow::anyhow!("compact delete needs a compaction id"))?;
    Ok((id, confirmed))
}

fn compaction_record_summary(record: &CompactionRecord) -> serde_json::Value {
    serde_json::json!({
        "id": record.id,
        "source": record.source,
        "conversation_id": record.conversation_id,
        "max_output_tokens": record.max_output_tokens,
        "created_at": record.created_at,
        "content_preview": compact_preview(&record.content, 240),
    })
}

fn handle_compact_keep(app: &mut App, args: &str) {
    let run_id = match resolve_compact_keep_run_id(app, args) {
        Ok(run_id) => run_id,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Compact keep failed: {err}"),
            });
            return;
        }
    };
    match crate::headless::keep_auto_compaction_for_run(
        run_id,
        app.selected_conversation_id.clone(),
        None,
    ) {
        Ok(Some(record)) => {
            if app.pending_auto_compaction_run == Some(run_id) {
                app.pending_auto_compaction_run = None;
            }
            push_event(app, format!("Kept auto compaction {}", record.id));
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&record)
                    .unwrap_or_else(|_| "<unserializable compaction record>".into()),
            });
        }
        Ok(None) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("No auto-compacted context found for run {run_id}."),
        }),
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Compact keep failed: {err}"),
        }),
    }
}

fn resolve_compact_keep_run_id(app: &App, args: &str) -> anyhow::Result<RunId> {
    let input = args.trim();
    if input.is_empty() {
        return app
            .pending_auto_compaction_run
            .or(app.last_run_id)
            .ok_or_else(|| anyhow::anyhow!("keep needs a run id or a pending auto compaction"));
    }
    if input == "last" {
        return app
            .last_run_id
            .ok_or_else(|| anyhow::anyhow!("keep last needs a previous run"));
    }
    if input.split_whitespace().nth(1).is_some() {
        anyhow::bail!("compact keep accepts at most one run id");
    }
    Ok(RunId(uuid::Uuid::parse_str(input)?))
}

fn preview_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/preview" {
        Some("")
    } else {
        trimmed.strip_prefix("/preview ").map(str::trim)
    }
}

fn score_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/score" {
        Some("")
    } else {
        trimmed.strip_prefix("/score ").map(str::trim)
    }
}

fn scores_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/scores" {
        Some("")
    } else {
        trimmed.strip_prefix("/scores ").map(str::trim)
    }
}

fn guide_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/guide" {
        Some("")
    } else {
        trimmed.strip_prefix("/guide ").map(str::trim)
    }
}

#[derive(Debug, PartialEq)]
struct GuideSlashRequest {
    run_id: Option<RunId>,
    text: String,
}

fn parse_guide_slash_args(
    rest: &str,
    last_run_id: Option<RunId>,
) -> anyhow::Result<GuideSlashRequest> {
    let rest = rest.trim();
    if rest.is_empty() {
        anyhow::bail!("guide needs guidance text");
    }
    let (first, text_after_selector) = rest
        .split_once(char::is_whitespace)
        .map(|(first, text)| (first, Some(text.trim())))
        .unwrap_or((rest, None));
    if first == "last" {
        let text = text_after_selector
            .filter(|text| !text.is_empty())
            .ok_or_else(|| anyhow::anyhow!("guide needs guidance text"))?;
        return Ok(GuideSlashRequest {
            run_id: last_run_id,
            text: text.to_string(),
        });
    }
    if let Ok(uuid) = uuid::Uuid::parse_str(first) {
        let text = text_after_selector
            .filter(|text| !text.is_empty())
            .ok_or_else(|| anyhow::anyhow!("guide needs guidance text"))?;
        return Ok(GuideSlashRequest {
            run_id: Some(RunId(uuid)),
            text: text.to_string(),
        });
    }
    Ok(GuideSlashRequest {
        run_id: last_run_id,
        text: rest.to_string(),
    })
}

fn stop_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/stop" {
        Some("")
    } else {
        trimmed.strip_prefix("/stop ").map(str::trim)
    }
}

fn stop_status_slash_command(trimmed: &str) -> bool {
    matches!(trimmed, "/stop status")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StopRequest {
    reason: String,
    mode: Option<StopRetentionMode>,
}

fn parse_stop_request(rest: &str) -> StopRequest {
    let rest = rest.trim();
    let (mode, reason) = if rest == "--summarise"
        || rest == "--summarize"
        || rest == "summarise"
        || rest == "summarize"
    {
        (Some(StopRetentionMode::Summarise), "")
    } else if let Some(reason) = rest.strip_prefix("--summarise ") {
        (Some(StopRetentionMode::Summarise), reason)
    } else if let Some(reason) = rest.strip_prefix("--summarize ") {
        (Some(StopRetentionMode::Summarise), reason)
    } else if let Some(reason) = rest.strip_prefix("summarise ") {
        (Some(StopRetentionMode::Summarise), reason)
    } else if let Some(reason) = rest.strip_prefix("summarize ") {
        (Some(StopRetentionMode::Summarise), reason)
    } else if let Some(reason) = rest.strip_prefix("--discard ") {
        (Some(StopRetentionMode::Discard), reason)
    } else if rest == "--discard" || rest == "discard" {
        (Some(StopRetentionMode::Discard), "")
    } else if let Some(reason) = rest.strip_prefix("discard ") {
        (Some(StopRetentionMode::Discard), reason)
    } else if rest == "--default" || rest == "default" {
        (None, "")
    } else if let Some(reason) = rest.strip_prefix("--default ") {
        (None, reason)
    } else if let Some(reason) = rest.strip_prefix("default ") {
        (None, reason)
    } else {
        (None, rest)
    };
    let reason = if reason.trim().is_empty() {
        "user requested stop"
    } else {
        reason.trim()
    };
    StopRequest {
        reason: reason.to_string(),
        mode,
    }
}

fn is_mid_run_control_command(trimmed: &str) -> bool {
    guide_slash_rest(trimmed).is_some()
        || stop_slash_rest(trimmed).is_some()
        || approval_slash_rest(trimmed).is_some()
}

fn agent_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/agent" {
        Some("")
    } else {
        trimmed.strip_prefix("/agent ").map(str::trim)
    }
}

fn resume_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/resume" {
        Some("")
    } else {
        trimmed.strip_prefix("/resume ").map(str::trim)
    }
}

fn resume_plan_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/resume plan" || trimmed == "/resume-plan" {
        Some("")
    } else if let Some(rest) = trimmed.strip_prefix("/resume plan ") {
        Some(rest.trim())
    } else {
        trimmed.strip_prefix("/resume-plan ").map(str::trim)
    }
}

fn replay_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/replay" {
        Some("")
    } else {
        trimmed.strip_prefix("/replay ").map(str::trim)
    }
}

fn trace_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/trace" {
        Some("")
    } else {
        trimmed.strip_prefix("/trace ").map(str::trim)
    }
}

fn compare_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/compare" {
        Some("")
    } else {
        trimmed.strip_prefix("/compare ").map(str::trim)
    }
}

fn usage_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/usage" {
        Some("")
    } else {
        trimmed.strip_prefix("/usage ").map(str::trim)
    }
}

fn code_slash_command(trimmed: &str) -> Option<(&'static str, &str)> {
    if trimmed == "/python" {
        Some(("code_python", ""))
    } else if let Some(rest) = trimmed.strip_prefix("/python ") {
        Some(("code_python", rest.trim()))
    } else if trimmed == "/typescript" || trimmed == "/ts" {
        Some(("code_typescript", ""))
    } else if let Some(rest) = trimmed.strip_prefix("/typescript ") {
        Some(("code_typescript", rest.trim()))
    } else {
        trimmed
            .strip_prefix("/ts ")
            .map(|rest| ("code_typescript", rest.trim()))
    }
}

fn voice_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/voice" {
        Some("")
    } else {
        trimmed.strip_prefix("/voice ").map(str::trim)
    }
}

fn bridges_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/bridges" {
        Some("")
    } else {
        trimmed.strip_prefix("/bridges ").map(str::trim)
    }
}

fn bridges_slash_help_text() -> &'static str {
    "/bridges status"
}

fn bridge_deliveries_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/bridge-deliveries" {
        Some("")
    } else {
        trimmed.strip_prefix("/bridge-deliveries ").map(str::trim)
    }
}

fn bridge_deliveries_slash_help_text() -> &'static str {
    "/bridge-deliveries [list]\n/bridge-deliveries delete <id> --confirm"
}

fn voice_slash_help_text() -> &'static str {
    "Voice commands:\n\
     - /voice status - show resolved active-agent voice config\n\
     - /voice transcribe <path> - transcribe an existing audio file\n\
     - /voice speak <text> - synthesize speech from text\n\
     Voice capture/stop is available from the app UI; the TUI can operate on saved audio paths."
}

fn voice_slash_status_text(agent: &AgentConfig) -> String {
    let voice = &agent.voice;
    let mut lines = vec![
        "Voice status:".to_string(),
        format!("agent: {} ({})", agent.name, agent.id),
        format!("input: {}", enabled_label(voice.input_enabled)),
    ];
    push_voice_status_field(&mut lines, "input backend", voice.input_backend.as_deref());
    push_voice_status_field(
        &mut lines,
        "input provider",
        voice.input_provider.as_deref(),
    );
    push_voice_status_field(&mut lines, "input model", voice.input_model.as_deref());
    lines.push(format!("output: {}", enabled_label(voice.output_enabled)));
    push_voice_status_field(
        &mut lines,
        "output backend",
        voice.output_backend.as_deref(),
    );
    push_voice_status_field(&mut lines, "tts provider", voice.tts_provider.as_deref());
    push_voice_status_field(&mut lines, "tts model", voice.tts_model.as_deref());
    push_voice_status_field(&mut lines, "voice", voice.voice.as_deref());
    push_voice_status_field(&mut lines, "tone", voice.tone.as_deref());
    lines.push("capture: app UI only; use /voice transcribe <path> for saved audio".into());
    lines.push("shortcuts: /voice transcribe <path>, /voice speak <text>".into());
    lines.join("\n")
}

fn enabled_label(enabled: bool) -> &'static str {
    if enabled { "enabled" } else { "disabled" }
}

fn push_voice_status_field(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        lines.push(format!("{label}: {value}"));
    }
}

fn start_batch_run(
    app: &mut App,
    rest: &str,
    demo: Demo,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let source = match parse_batch_slash_run(rest) {
        Ok(source) => source,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Batch failed: {err}"),
            });
            return;
        }
    };
    let batch_run_id = RunId::new();
    let batch_id = format!("batch-{}", batch_run_id.0);
    let (items, item_keys) =
        match prepare_batch_inputs(source.items, None, source.files, source.folders) {
            Ok(inputs) => inputs,
            Err(err) => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Batch failed: {err}"),
                });
                return;
            }
        };
    let mut plan = match BatchPlan::new_with_optional_item_keys(batch_id.clone(), items, item_keys)
    {
        Ok(plan) => plan,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Batch failed: {err}"),
            });
            return;
        }
    };
    if let Err(err) = plan.save_to_env() {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Batch failed: {err}"),
        });
        return;
    }
    start_batch_plan(app, plan, batch_run_id, batch_id, demo, publish_tx, options);
}

fn start_batch_resume(
    app: &mut App,
    rest: &str,
    demo: Demo,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let batch_id = match parse_resume_batch_slash_rest(rest) {
        Ok(batch_id) => batch_id,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume batch failed: {err}"),
            });
            return;
        }
    };
    let plan = match BatchPlan::load_from_env(&batch_id) {
        Ok(plan) => plan,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Resume batch failed: {err}"),
            });
            return;
        }
    };
    start_batch_plan(app, plan, RunId::new(), batch_id, demo, publish_tx, options);
}

fn start_batch_plan(
    app: &mut App,
    plan: BatchPlan,
    batch_run_id: RunId,
    batch_id: String,
    demo: Demo,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let store: Arc<dyn EventStore> = match open_event_store() {
        Ok(store) => Arc::new(PublishingEventStore::new(store, publish_tx.clone())),
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Batch trace setup failed: {err}"),
            });
            return;
        }
    };
    let agent = setup::build_agent(options);
    app.transcript.push(TranscriptLine {
        kind: LineKind::User,
        text: format!("/batch {batch_id}"),
    });
    app.state = AppState::Running;
    app.tokens_in = 0;
    app.tokens_out = 0;
    app.cost_usd = 0.0;
    app.calls_used = 0;
    app.calls_max = agent.tool_policy.max_calls;
    app.calls_remaining = agent.tool_policy.max_calls;
    app.elapsed_ms = 0;
    app.run_started_at = Some(Instant::now());
    app.last_run_id = Some(batch_run_id);
    app.active_batch_run_id = Some(batch_run_id);

    let options = options.clone();
    let store_for_failure = Arc::clone(&store);
    let batch_id_for_failure = batch_id.clone();
    let run_task = tokio::spawn(async move {
        if let Err(err) =
            execute_tui_batch_plan(plan, batch_run_id, batch_id, demo, options, store).await
        {
            store_for_failure.append(
                batch_run_id,
                None,
                RunEventKind::RunFailed {
                    reason: format!("Batch {batch_id_for_failure} failed: {err}"),
                },
            );
        }
    });
    app.active_run_handle = Some(run_task.abort_handle());
}

async fn execute_tui_batch_plan(
    mut plan: BatchPlan,
    batch_run_id: RunId,
    batch_id: String,
    demo: Demo,
    options: setup::RuntimeOptions,
    store: Arc<dyn EventStore>,
) -> anyhow::Result<()> {
    store.append(
        batch_run_id,
        None,
        RunEventKind::BatchRunStarted {
            batch_id: batch_id.clone(),
            items: plan.items.len() as u32,
        },
    );

    for item in plan.items.clone() {
        let item_key = item.key.clone();
        if item.status == BatchItemState::Succeeded {
            store.append(
                batch_run_id,
                None,
                RunEventKind::BatchItemStatus {
                    batch_id: batch_id.clone(),
                    item_key,
                    status: "skipped: already succeeded".into(),
                },
            );
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

        let agent = setup::build_agent(&options);
        let result = match setup::build_provider(demo, &item.input, &options) {
            Ok(provider) => {
                let registry = setup::build_registry(
                    options.enable_shell,
                    options.enable_subagent,
                    options.enable_capability_drafts,
                    options.agent_id.as_deref(),
                    options.conversation_id.as_deref(),
                );
                let harness = setup::build_harness_for_agent(
                    provider,
                    Arc::clone(&store),
                    registry,
                    options.agent_id.as_deref(),
                );
                harness
                    .run(
                        &agent,
                        UserInput {
                            text: item.input.clone(),
                        },
                    )
                    .await
                    .map_err(|err| anyhow::anyhow!(err))
            }
            Err(err) => Err(anyhow::anyhow!(err)),
        };

        match result {
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
                        item_key,
                        status: "succeeded".into(),
                    },
                );
            }
            Err(err) => {
                plan.mark_failed(&item_key, err.to_string());
                plan.save_to_env()?;
                store.append(
                    batch_run_id,
                    None,
                    RunEventKind::BatchItemStatus {
                        batch_id: batch_id.clone(),
                        item_key,
                        status: format!("failed: {err}"),
                    },
                );
            }
        }
    }

    store.append(
        batch_run_id,
        None,
        RunEventKind::BatchRunCompleted {
            batch_id,
            succeeded: plan.succeeded_count(),
            failed: plan.failed_count(),
        },
    );
    Ok(())
}

fn handle_voice_slash(
    app: &mut App,
    rest: &str,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
) {
    let rest = rest.trim();
    if rest.is_empty() || rest == "status" {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: voice_slash_status_text(agent),
        });
        return;
    }
    if matches!(rest, "help" | "--help") {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: voice_slash_help_text().into(),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "transcribe" => {
            if args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "Voice transcribe needs an audio path.".into(),
                });
                return;
            }
            start_manual_tool_call_with_input(
                app,
                "voice_transcribe".into(),
                serde_json::json!({ "audio_path": args }),
                registry,
                agent,
                publish_tx,
            );
        }
        "speak" => {
            if args.is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "Voice speak needs text.".into(),
                });
                return;
            }
            start_manual_tool_call_with_input(
                app,
                "voice_speak".into(),
                serde_json::json!({ "text": args }),
                registry,
                agent,
                publish_tx,
            );
        }
        "capture" | "stop" => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Voice capture controls are available in the app UI; use /voice transcribe <path> for saved audio."
                .into(),
        }),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Voice command needs status, transcribe, speak, or help.".into(),
        }),
    }
}

fn handle_bridges_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if slash_help_rest(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: bridges_slash_help_text().into(),
        });
        return;
    }
    match rest {
        "status" => app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: serde_json::to_string_pretty(&crate::headless::bridge_status_report_from_env())
                .unwrap_or_else(|_| "<unserializable bridge status>".into()),
        }),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Bridges command needs status or help.".into(),
        }),
    }
}

fn handle_bridge_deliveries_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if matches!(rest, "help" | "--help") {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: bridge_deliveries_slash_help_text().into(),
        });
        return;
    }
    if rest.is_empty() || rest == "list" {
        match crate::headless::bridge_delivery_report_from_env() {
            Ok(report) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: serde_json::to_string_pretty(&report)
                    .unwrap_or_else(|_| "<unserializable bridge deliveries>".into()),
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Failed to list bridge deliveries: {err}"),
            }),
        }
        return;
    }
    let mut parts = rest.split_whitespace();
    match parts.next() {
        Some("delete" | "rm") => {
            let Some(id) = parts.next() else {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "Usage: /bridge-deliveries delete <id> --confirm".into(),
                });
                return;
            };
            let mut confirm = false;
            for part in parts {
                if part == "--confirm" {
                    confirm = true;
                    continue;
                }
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!(
                        "Usage: /bridge-deliveries delete <id> --confirm, unexpected {part:?}"
                    ),
                });
                return;
            }
            match crate::headless::bridge_delivery_delete_from_env(
                id,
                confirm,
                &format!("/bridge-deliveries delete {id} --confirm"),
            ) {
                Ok(result) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Assistant,
                    text: serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_| "<unserializable bridge delivery delete>".into()),
                }),
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Failed to delete bridge delivery: {err}"),
                }),
            }
        }
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Bridge deliveries command needs list, delete, or help.".into(),
        }),
    }
}

fn handle_x402_slash(
    app: &mut App,
    rest: &str,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
) {
    let rest = rest.trim();
    if crate::x402_slash::is_help(rest) {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: crate::x402_slash::help_text().into(),
        });
        return;
    }
    let (tool_name, input) = match crate::x402_slash::parse_tool_call(rest) {
        Ok(parsed) => parsed,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: err.to_string(),
            });
            return;
        }
    };
    start_manual_tool_call_with_input(app, tool_name.into(), input, registry, agent, publish_tx);
}

fn start_manual_tool_call(
    app: &mut App,
    rest: &str,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
) {
    let (name, input) = match parse_tool_slash_rest(rest) {
        Ok(parsed) => parsed,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Tool command failed: {err}"),
            });
            return;
        }
    };
    start_manual_tool_call_with_input(app, name, input, registry, agent, publish_tx);
}

fn handle_shell_slash(
    app: &mut App,
    rest: &str,
    registry: &mut Arc<ToolRegistry>,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim().to_lowercase();
    match rest.as_str() {
        "" | "status" => {
            push_event(
                app,
                format!(
                    "Shell tool access is {}.",
                    if options.enable_shell {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
            );
        }
        "help" | "--help" => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: shell_slash_help_text().into(),
            });
        }
        "on" | "enable" | "enabled" => {
            options.enable_shell = true;
            *registry = setup::build_registry(
                options.enable_shell,
                options.enable_subagent,
                options.enable_capability_drafts,
                options.agent_id.as_deref(),
                options.conversation_id.as_deref(),
            );
            push_event(app, "Shell tool access enabled.".into());
        }
        "off" | "disable" | "disabled" => {
            options.enable_shell = false;
            *registry = setup::build_registry(
                options.enable_shell,
                options.enable_subagent,
                options.enable_capability_drafts,
                options.agent_id.as_deref(),
                options.conversation_id.as_deref(),
            );
            push_event(app, "Shell tool access disabled.".into());
        }
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Shell command needs status, on, off, or help.".into(),
        }),
    }
}

fn handle_subagent_slash(
    app: &mut App,
    rest: &str,
    registry: &mut Arc<ToolRegistry>,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim().to_lowercase();
    match rest.as_str() {
        "" | "status" => {
            push_event(
                app,
                format!(
                    "Subagent tool access is {}.",
                    if options.enable_subagent {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
            );
        }
        "help" | "--help" => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: subagent_slash_help_text().into(),
            });
        }
        "on" | "enable" | "enabled" => {
            options.enable_subagent = true;
            *registry = setup::build_registry(
                options.enable_shell,
                options.enable_subagent,
                options.enable_capability_drafts,
                options.agent_id.as_deref(),
                options.conversation_id.as_deref(),
            );
            push_event(app, "Subagent tool access enabled.".into());
        }
        "off" | "disable" | "disabled" => {
            options.enable_subagent = false;
            *registry = setup::build_registry(
                options.enable_shell,
                options.enable_subagent,
                options.enable_capability_drafts,
                options.agent_id.as_deref(),
                options.conversation_id.as_deref(),
            );
            push_event(app, "Subagent tool access disabled.".into());
        }
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Subagent command needs status, on, off, or help.".into(),
        }),
    }
}

fn handle_budget_slash(
    app: &mut App,
    rest: &str,
    agent: &mut AgentConfig,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim();
    if rest == "help" || rest == "--help" {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: budget_slash_help_text().into(),
        });
        return;
    }
    match parse_budget_slash_rest(rest) {
        Ok(max_tool_calls) => {
            options.max_tool_calls = Some(max_tool_calls);
            refresh_agent_runtime_policy(app, agent, options);
            push_event(app, format!("Tool-call budget set to {max_tool_calls}."));
        }
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: err.to_string(),
        }),
    }
}

fn parse_budget_slash_rest(rest: &str) -> anyhow::Result<u32> {
    let mut parts = rest.split_whitespace();
    let value = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Budget command needs a non-negative tool-call limit."))?;
    if let Some(extra) = parts.next() {
        anyhow::bail!("usage: /budget <n>, unexpected {extra:?}");
    }
    let parsed = value
        .parse::<u32>()
        .map_err(|_| anyhow::anyhow!("Budget command needs a non-negative integer."))?;
    Ok(parsed)
}

fn handle_visibility_slash(
    app: &mut App,
    rest: &str,
    agent: &mut AgentConfig,
    options: &mut setup::RuntimeOptions,
) {
    let rest = rest.trim().to_lowercase();
    if rest == "help" || rest == "--help" {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: visibility_slash_help_text().into(),
        });
        return;
    }
    match parse_visibility_slash_rest(&rest) {
        Ok(visibility) => {
            options.tool_visibility = visibility;
            refresh_agent_runtime_policy(app, agent, options);
            push_event(
                app,
                visibility
                    .map(|value| format!("Tool visibility set to {}.", visibility_label(value)))
                    .unwrap_or_else(|| "Tool visibility set to config default.".into()),
            );
        }
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: err.to_string(),
        }),
    }
}

fn parse_visibility_slash_rest(rest: &str) -> anyhow::Result<Option<VisibilityLevel>> {
    let mut parts = rest.split_whitespace();
    let value = parts.next().ok_or_else(|| {
        anyhow::anyhow!("Visibility command needs full, descriptions, names, or config.")
    })?;
    if let Some(extra) = parts.next() {
        anyhow::bail!("usage: /visibility full|descriptions|names|config, unexpected {extra:?}");
    }
    match value {
        "full" | "full-schema" | "full_schema" => Ok(Some(VisibilityLevel::FullSchema)),
        "descriptions" | "name-and-description" | "name_and_description" => {
            Ok(Some(VisibilityLevel::NameAndDescription))
        }
        "names" | "name-only" | "name_only" => Ok(Some(VisibilityLevel::NameOnly)),
        "config" => Ok(None),
        _ => anyhow::bail!("Visibility command needs full, descriptions, names, or config."),
    }
}

fn visibility_label(value: VisibilityLevel) -> &'static str {
    match value {
        VisibilityLevel::FullSchema => "full schema",
        VisibilityLevel::NameAndDescription => "name and description",
        VisibilityLevel::NameOnly => "name only",
    }
}

fn handle_cost_slash(
    app: &mut App,
    rest: &str,
    agent: &mut AgentConfig,
    options: &mut setup::RuntimeOptions,
) {
    let mut parts = rest.split_whitespace();
    let command = parts.next().unwrap_or_default();
    match command {
        "" | "status" => {
            if let Err(err) = ensure_no_extra_cost_args(parts, "usage: /cost status") {
                push_cost_error(app, err);
                return;
            }
            push_event(app, cost_status_text(agent, options));
        }
        "help" | "--help" => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: cost_slash_help_text().into(),
            });
        }
        "input" => match single_cost_value(parts, "usage: /cost input <usd-per-million>") {
            Ok(value) => {
                options.input_cost_per_million = Some(value);
                refresh_agent_runtime_policy(app, agent, options);
                push_event(
                    app,
                    format!("Input token cost override set to {value} $/M."),
                );
            }
            Err(err) => push_cost_error(app, err),
        },
        "output" => match single_cost_value(parts, "usage: /cost output <usd-per-million>") {
            Ok(value) => {
                options.output_cost_per_million = Some(value);
                refresh_agent_runtime_policy(app, agent, options);
                push_event(
                    app,
                    format!("Output token cost override set to {value} $/M."),
                );
            }
            Err(err) => push_cost_error(app, err),
        },
        "both" => match two_cost_values(parts, "usage: /cost both <input> <output>") {
            Ok((input, output)) => {
                options.input_cost_per_million = Some(input);
                options.output_cost_per_million = Some(output);
                refresh_agent_runtime_policy(app, agent, options);
                push_event(
                    app,
                    format!(
                        "Token cost overrides set to input {input} $/M and output {output} $/M."
                    ),
                );
            }
            Err(err) => push_cost_error(app, err),
        },
        "clear" => {
            if let Err(err) = ensure_no_extra_cost_args(parts, "usage: /cost clear") {
                push_cost_error(app, err);
                return;
            }
            options.input_cost_per_million = None;
            options.output_cost_per_million = None;
            refresh_agent_runtime_policy(app, agent, options);
            push_event(
                app,
                "Token cost overrides cleared; configured model costs will be used.".into(),
            );
        }
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Cost command needs input, output, both, clear, status, or help.".into(),
        }),
    }
}

fn refresh_agent_runtime_policy(
    app: &mut App,
    agent: &mut AgentConfig,
    options: &setup::RuntimeOptions,
) {
    *agent = setup::build_agent(options);
    app.calls_used = 0;
    app.calls_max = agent.tool_policy.max_calls;
    app.calls_remaining = agent.tool_policy.max_calls;
}

fn push_cost_error(app: &mut App, err: anyhow::Error) {
    app.transcript.push(TranscriptLine {
        kind: LineKind::Error,
        text: err.to_string(),
    });
}

fn ensure_no_extra_cost_args<'a>(
    mut parts: impl Iterator<Item = &'a str>,
    usage: &str,
) -> anyhow::Result<()> {
    if let Some(extra) = parts.next() {
        anyhow::bail!("{usage}, unexpected {extra:?}");
    }
    Ok(())
}

fn single_cost_value<'a>(
    mut parts: impl Iterator<Item = &'a str>,
    usage: &str,
) -> anyhow::Result<f64> {
    let value = parts.next().ok_or_else(|| anyhow::anyhow!("{usage}"))?;
    let parsed = parse_cost_rate(value)?;
    ensure_no_extra_cost_args(parts, usage)?;
    Ok(parsed)
}

fn two_cost_values<'a>(
    mut parts: impl Iterator<Item = &'a str>,
    usage: &str,
) -> anyhow::Result<(f64, f64)> {
    let input = parts.next().ok_or_else(|| anyhow::anyhow!("{usage}"))?;
    let output = parts.next().ok_or_else(|| anyhow::anyhow!("{usage}"))?;
    let input = parse_cost_rate(input)?;
    let output = parse_cost_rate(output)?;
    ensure_no_extra_cost_args(parts, usage)?;
    Ok((input, output))
}

fn parse_cost_rate(value: &str) -> anyhow::Result<f64> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| anyhow::anyhow!("cost rate must be a non-negative number"))?;
    if !parsed.is_finite() || parsed < 0.0 {
        anyhow::bail!("cost rate must be a non-negative number");
    }
    Ok(parsed)
}

fn cost_status_text(agent: &AgentConfig, options: &setup::RuntimeOptions) -> String {
    format!(
        "Token cost overrides: input {}, output {}; effective input {}, output {}.",
        cost_override_label(options.input_cost_per_million),
        cost_override_label(options.output_cost_per_million),
        cost_effective_label(agent.cost_policy.input_cost_per_million),
        cost_effective_label(agent.cost_policy.output_cost_per_million),
    )
}

fn cost_override_label(value: Option<f64>) -> String {
    value
        .map(|rate| format!("{rate} $/M"))
        .unwrap_or_else(|| "config".into())
}

fn cost_effective_label(value: Option<f64>) -> String {
    value
        .map(|rate| format!("{rate} $/M"))
        .unwrap_or_else(|| "n/a".into())
}

fn start_code_tool_call(
    app: &mut App,
    tool_name: &str,
    code: &str,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
) {
    if code.trim().is_empty() {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Code shortcut needs code text.".into(),
        });
        return;
    }
    start_manual_tool_call_with_input(
        app,
        tool_name.to_string(),
        serde_json::json!({ "code": code }),
        registry,
        agent,
        publish_tx,
    );
}

fn start_manual_tool_call_with_input(
    app: &mut App,
    name: String,
    input: serde_json::Value,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
) {
    app.transcript.push(TranscriptLine {
        kind: LineKind::User,
        text: format!("/tool! {name} {input}"),
    });
    app.state = AppState::Running;
    app.tokens_in = 0;
    app.tokens_out = 0;
    app.cost_usd = 0.0;
    app.calls_used = 0;
    app.calls_max = agent.tool_policy.max_calls;
    app.calls_remaining = agent.tool_policy.max_calls;
    app.elapsed_ms = 0;
    app.run_started_at = Some(Instant::now());

    let store = match open_event_store() {
        Ok(store) => PublishingEventStore::new(store, publish_tx.clone()),
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace store setup failed: {err}"),
            });
            app.state = AppState::Idle;
            return;
        }
    };
    let harness = setup::build_harness_for_agent(
        Arc::new(agent_llm::FakeProvider::echo()),
        Arc::new(store),
        registry.clone(),
        Some(&agent.id),
    );
    let agent = agent.clone();
    let run_task = tokio::spawn(async move {
        let _ = harness.call_tool(&agent, ToolId::from(name), input).await;
    });
    app.active_run_handle = Some(run_task.abort_handle());
}

fn start_forced_tool_call(
    app: &mut App,
    rest: &str,
    demo: Demo,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let (name, prompt) = match parse_forced_tool_slash_rest(rest) {
        Ok(parsed) => parsed,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Tool command failed: {err}"),
            });
            return;
        }
    };
    let tool_id = ToolId::from(name.clone());
    if !agent.tool_policy.allowed_tools.is_empty()
        && !agent.tool_policy.allowed_tools.contains(&tool_id)
    {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Tool command failed: tool {name:?} is not allowed by this agent"),
        });
        return;
    }
    let mut forced_agent = agent.clone();
    forced_agent.tool_policy.allowed_tools = vec![tool_id.clone()];
    forced_agent.tool_policy.required_tool = Some(tool_id);
    let forced_prompt = forced_tool_prompt(&name, &prompt);
    spawn_run(
        app,
        forced_prompt,
        demo,
        registry,
        &forced_agent,
        publish_tx,
        options,
    );
}

fn parse_tool_slash_rest(rest: &str) -> anyhow::Result<(String, serde_json::Value)> {
    let (name, input) = rest
        .trim()
        .split_once(char::is_whitespace)
        .map(|(name, input)| (name.to_string(), input.trim().to_string()))
        .unwrap_or_else(|| (rest.trim().to_string(), "{}".into()));
    if name.is_empty() {
        anyhow::bail!("missing tool name");
    }
    let input = serde_json::from_str(&input)?;
    Ok((name, input))
}

fn batch_slash_rest(text: &str) -> Option<&str> {
    if text == "/batch" {
        return Some("");
    }
    text.strip_prefix("/batch ").map(str::trim)
}

fn resume_batch_slash_rest(text: &str) -> Option<&str> {
    if text == "/resume-batch" {
        return Some("");
    }
    text.strip_prefix("/resume-batch ").map(str::trim)
}

struct BatchSlashRun {
    items: Vec<String>,
    files: Vec<String>,
    folders: Vec<String>,
}

fn parse_batch_slash_run(rest: &str) -> anyhow::Result<BatchSlashRun> {
    if rest == "files" {
        anyhow::bail!("batch files shortcut needs one or more file paths after /batch files");
    }
    if let Some(files) = rest.strip_prefix("files ").map(str::trim) {
        return Ok(BatchSlashRun {
            items: Vec::new(),
            files: parse_batch_slash_lines(files, "file paths", "/batch files")?,
            folders: Vec::new(),
        });
    }
    if rest == "folder" || rest == "folders" {
        anyhow::bail!("batch folder shortcut needs one or more folder paths after /batch folder");
    }
    if let Some(folders) = rest
        .strip_prefix("folder ")
        .or_else(|| rest.strip_prefix("folders "))
        .map(str::trim)
    {
        return Ok(BatchSlashRun {
            items: Vec::new(),
            files: Vec::new(),
            folders: parse_batch_slash_lines(folders, "folder paths", "/batch folder")?,
        });
    }
    Ok(BatchSlashRun {
        items: parse_batch_slash_items(rest)?,
        files: Vec::new(),
        folders: Vec::new(),
    })
}

fn parse_batch_slash_items(rest: &str) -> anyhow::Result<Vec<String>> {
    let items = parse_batch_slash_lines(rest, "prompts", "/batch")?;
    if items.is_empty() {
        anyhow::bail!("batch shortcut needs one or more prompts after /batch");
    }
    Ok(items)
}

fn parse_batch_slash_lines(rest: &str, label: &str, command: &str) -> anyhow::Result<Vec<String>> {
    let lines = rest
        .lines()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        anyhow::bail!("batch shortcut needs one or more {label} after {command}");
    }
    Ok(lines)
}

fn parse_resume_batch_slash_rest(rest: &str) -> anyhow::Result<String> {
    let mut parts = rest.split_whitespace();
    let batch_id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("resume batch shortcut needs a batch id"))?;
    if parts.next().is_some() {
        anyhow::bail!("resume batch shortcut accepts exactly one batch id");
    }
    Ok(batch_id.to_string())
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

fn forced_tool_prompt(name: &str, prompt: &str) -> String {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        format!("Call the `{name}` tool with appropriate inputs, then answer from its result.")
    } else {
        format!("Call the `{name}` tool for this request, then answer from its result.\n\n{prompt}")
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ScoreSlashRequest {
    run_id: Option<RunId>,
    target: String,
    score: f32,
}

fn parse_score_slash_rest(rest: &str) -> anyhow::Result<ScoreSlashRequest> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(ScoreSlashRequest {
            run_id: None,
            target: "last_answer".into(),
            score: 10.0,
        });
    }
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    if let Ok(run_id) = uuid::Uuid::parse_str(parts[0]) {
        let score = parts
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("usage: /score <run-id> <0-10> [target]"))?
            .parse::<f32>()?;
        validate_quality_score(score)?;
        let target = parts.get(2..).unwrap_or_default().join(" ");
        return Ok(ScoreSlashRequest {
            run_id: Some(RunId(run_id)),
            target: if target.trim().is_empty() {
                "last_answer".into()
            } else {
                target.trim().to_string()
            },
            score,
        });
    }
    let first_score = parts[0].parse::<f32>().ok();
    let (target, score) = if let Some(score) = first_score {
        (parts[1..].join(" "), score)
    } else {
        let Some((score_part, target_parts)) = parts.split_last() else {
            anyhow::bail!("score command needs a target and/or score");
        };
        (target_parts.join(" "), score_part.parse::<f32>()?)
    };
    validate_quality_score(score)?;
    Ok(ScoreSlashRequest {
        run_id: None,
        target: if target.trim().is_empty() {
            "last_answer".into()
        } else {
            target.trim().to_string()
        },
        score,
    })
}

fn append_score_event(
    store: &dyn EventStore,
    run_id: RunId,
    target: String,
    score: f32,
) -> anyhow::Result<()> {
    validate_quality_score(score)?;
    let parent = latest_event_id(&store.events(run_id))
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no trace events"))?;
    store.append(
        run_id,
        Some(parent),
        RunEventKind::QualityScored { target, score },
    );
    Ok(())
}

fn parse_scores_run_id(rest: &str, last_run_id: Option<RunId>) -> anyhow::Result<RunId> {
    let mut parts = rest.split_whitespace();
    let run_part = parts.next();
    if let Some(extra) = parts.next() {
        anyhow::bail!("scores accepts at most one run id, unexpected {extra:?}");
    }
    parse_run_id_arg(run_part, last_run_id, "scores")
}

fn quality_score_report_for_run(run_id: RunId) -> anyhow::Result<String> {
    open_event_store().and_then(|store| {
        let events = store.try_events(run_id)?;
        Ok(quality_score_report(
            run_id,
            &quality_score_records(&events),
        ))
    })
}

fn quality_score_report(run_id: RunId, records: &[agent_tracing::QualityScoreRecord]) -> String {
    if records.is_empty() {
        return format!("No quality scores recorded for {run_id}.");
    }
    let average = records.iter().map(|record| record.score).sum::<f32>() / records.len() as f32;
    let mut lines = vec![format!(
        "Quality scores for {run_id}: {} score(s), avg {average:.1}/10",
        records.len()
    )];
    lines.extend(records.iter().map(|record| {
        format!(
            "#{} {}: {:.1}/10 ({})",
            record.event_id.0,
            record.target,
            record.score,
            record.at.to_rfc3339()
        )
    }));
    lines.join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraceSlashMode {
    List,
    Clear,
    Overview,
    Summary,
    Tree,
    Hooks,
    Scores,
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TraceSlashArgs {
    mode: TraceSlashMode,
    run_id: Option<RunId>,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompareSlashArgs {
    primary_run_id: RunId,
    compare_run_id: RunId,
}

fn handle_trace_slash(app: &mut App, rest: &str) {
    let args = match parse_trace_slash_args(rest, trace_default_run_id(app)) {
        Ok(args) => args,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace failed: {err}"),
            });
            return;
        }
    };
    if args.mode == TraceSlashMode::Clear {
        app.loaded_trace_run_id = None;
        app.transcript.push(TranscriptLine {
            kind: LineKind::Event,
            text: "Cleared loaded trace selection.".into(),
        });
        return;
    }
    let store = match open_event_store() {
        Ok(store) => store,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace failed: {err}"),
            });
            return;
        }
    };
    if args.mode == TraceSlashMode::List {
        match store.try_run_records(args.limit) {
            Ok(records) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: trace_run_records_report(&records),
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace list failed: {err}"),
            }),
        }
        return;
    }
    let run_id = match args.run_id {
        Some(run_id) => run_id,
        None => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: "Trace failed: run id required.".into(),
            });
            return;
        }
    };
    let events = match store.try_events(run_id) {
        Ok(events) => events,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace failed: {err}"),
            });
            return;
        }
    };
    if events.is_empty() {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: format!("No events found for run {}.", run_id.0),
        });
        return;
    }
    app.loaded_trace_run_id = Some(run_id);
    if args.mode == TraceSlashMode::Prompt {
        match trace_replay_source(run_id, &events) {
            Ok((agent_id, prompt)) => app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: format!("Trace prompt for {run_id}\nagent: {agent_id}\n{prompt}"),
            }),
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace prompt failed: {err}"),
            }),
        }
        return;
    }
    if args.mode == TraceSlashMode::Hooks {
        let plan = hook_remediation_plan(&events);
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: serde_json::to_string_pretty(&plan)
                .unwrap_or_else(|_| "<unserializable hook review>".into()),
        });
        return;
    }
    if args.mode == TraceSlashMode::Scores {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: quality_score_report(run_id, &quality_score_records(&events)),
        });
        return;
    }
    let summary = summarize_trace(&events, run_id);
    let tree = match build_trace_tree(run_id, |id| store.try_events(id)) {
        Ok(tree) => tree,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Trace tree failed: {err}"),
            });
            return;
        }
    };
    let text = match args.mode {
        TraceSlashMode::Overview => {
            format!(
                "{}\n\n{}",
                trace_summary_report(&summary),
                trace_tree_report(&tree)
            )
        }
        TraceSlashMode::Summary => trace_summary_report(&summary),
        TraceSlashMode::Tree => trace_tree_report(&tree),
        TraceSlashMode::List => unreachable!("list mode returns before trace rendering"),
        TraceSlashMode::Clear => unreachable!("clear mode returns before trace lookup"),
        TraceSlashMode::Hooks => unreachable!("hooks mode returns before trace rendering"),
        TraceSlashMode::Scores => unreachable!("scores mode returns before trace rendering"),
        TraceSlashMode::Prompt => unreachable!("prompt mode returns before trace rendering"),
    };
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text,
    });
}

fn handle_compare_slash(app: &mut App, rest: &str) {
    let args = match parse_compare_slash_args(rest, trace_default_run_id(app)) {
        Ok(args) => args,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Compare failed: {err}"),
            });
            return;
        }
    };
    let store = match open_event_store() {
        Ok(store) => store,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Compare failed: {err}"),
            });
            return;
        }
    };
    match build_trace_comparison(args.primary_run_id, args.compare_run_id, |id| {
        store.try_events(id)
    }) {
        Ok(comparison) => app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: trace_compare_report(&comparison),
        }),
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Compare failed: {err}"),
        }),
    }
}

fn handle_usage_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if matches!(rest, "help" | "--help") {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: usage_slash_help_text().into(),
        });
        return;
    }
    let (command, args) = rest
        .split_once(char::is_whitespace)
        .map(|(command, args)| (command, args.trim()))
        .unwrap_or((rest, ""));
    match command {
        "" | "current" => {
            if !args.trim().is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "Usage current accepts no arguments.".into(),
                });
                return;
            }
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: current_usage_report(app),
            });
        }
        "last" => {
            if !args.trim().is_empty() {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "Usage last accepts no arguments.".into(),
                });
                return;
            }
            match show_trace_usage(app, "last", "usage last") {
                Ok(()) => {}
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Usage last failed: {err}"),
                }),
            }
        }
        "trace" | "run" => {
            let label = if command == "run" {
                "usage run"
            } else {
                "usage trace"
            };
            match show_trace_usage(app, args, label) {
                Ok(()) => {}
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("{} failed: {err}", usage_error_label(label)),
                }),
            }
        }
        "conversation" | "conv" => match show_conversation_usage(app, args) {
            Ok(()) => {}
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Usage conversation failed: {err}"),
            }),
        },
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Usage shortcut needs current, last, trace, run, conversation, or help.".into(),
        }),
    }
}

fn show_trace_usage(app: &mut App, args: &str, label: &str) -> anyhow::Result<()> {
    let run_id =
        parse_usage_trace_run_id(args, usage_trace_default_run_id(app, args, label), label)?;
    let events = open_event_store()?.try_events(run_id)?;
    if events.is_empty() {
        anyhow::bail!("run {run_id} has no trace events");
    }
    let summary = summarize_trace(&events, run_id);
    push_event(
        app,
        format!(
            "{}: tokens {}/{}, events {}, tools {}",
            usage_error_label(label),
            summary.tokens_in,
            summary.tokens_out,
            summary.events,
            summary.tool_calls
        ),
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: trace_summary_report(&summary),
    });
    Ok(())
}

fn usage_trace_default_run_id(app: &App, args: &str, label: &str) -> Option<RunId> {
    if args.trim().is_empty() && label == "usage trace" {
        trace_default_run_id(app)
    } else {
        app.last_run_id
    }
}

fn parse_usage_trace_run_id(
    rest: &str,
    last_run_id: Option<RunId>,
    label: &str,
) -> anyhow::Result<RunId> {
    let mut parts = rest.split_whitespace();
    let run_part = parts.next();
    if let Some(extra) = parts.next() {
        anyhow::bail!("{label} accepts at most one run id, unexpected {extra:?}");
    }
    parse_run_id_arg(run_part, last_run_id, label)
}

fn usage_error_label(label: &str) -> &'static str {
    match label {
        "usage run" => "Usage run",
        "usage last" => "Usage last",
        _ => "Usage trace",
    }
}

fn current_usage_report(app: &App) -> String {
    [
        "Current usage".to_string(),
        format!("tokens {}/{}", app.tokens_in, app.tokens_out),
        format!("cost ${:.6}", app.cost_usd),
        format!("time {}", format_duration(app.elapsed_ms)),
        format!(
            "tool calls {}/{} ({} remaining)",
            app.calls_used, app.calls_max, app.calls_remaining
        ),
        format!(
            "state {}",
            match app.state {
                AppState::Idle => "idle",
                AppState::Running => "running",
            }
        ),
    ]
    .join("\n")
}

fn parse_trace_slash_args(
    rest: &str,
    last_run_id: Option<RunId>,
) -> anyhow::Result<TraceSlashArgs> {
    let mut parts = rest.split_whitespace();
    let first = parts.next();
    if matches!(first, Some("list" | "runs")) {
        return Ok(TraceSlashArgs {
            mode: TraceSlashMode::List,
            run_id: None,
            limit: parse_trace_list_slash_limit(parts)?,
        });
    }
    if matches!(first, Some("clear")) {
        if let Some(extra) = parts.next() {
            anyhow::bail!("usage: /trace clear, unexpected {extra:?}");
        }
        return Ok(TraceSlashArgs {
            mode: TraceSlashMode::Clear,
            run_id: None,
            limit: 20,
        });
    }
    let (mode, run_part) = match first {
        None => (TraceSlashMode::Overview, None),
        Some("summary") => (TraceSlashMode::Summary, parts.next()),
        Some("tree") => (TraceSlashMode::Tree, parts.next()),
        Some("hooks") => (TraceSlashMode::Hooks, parts.next()),
        Some("scores") => (TraceSlashMode::Scores, parts.next()),
        Some("prompt") => (TraceSlashMode::Prompt, parts.next()),
        Some(value) => (TraceSlashMode::Overview, Some(value)),
    };
    if let Some(extra) = parts.next() {
        anyhow::bail!("unexpected trace argument {extra:?}");
    }
    Ok(TraceSlashArgs {
        mode,
        run_id: (mode != TraceSlashMode::List)
            .then(|| parse_run_id_arg(run_part, last_run_id, "trace"))
            .transpose()?,
        limit: 20,
    })
}

fn parse_trace_list_slash_limit<'a>(
    mut parts: impl Iterator<Item = &'a str>,
) -> anyhow::Result<usize> {
    let mut limit = None;
    while let Some(part) = parts.next() {
        match part {
            "--limit" => {
                let value = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--limit needs a count"))?;
                if limit.is_some() {
                    anyhow::bail!("trace list limit specified twice");
                }
                limit = Some(parse_positive_usize(value, "--limit")?);
            }
            _ if part.starts_with("--limit=") => {
                let value = part
                    .split_once('=')
                    .map(|(_, value)| value)
                    .unwrap_or_default();
                if value.trim().is_empty() {
                    anyhow::bail!("--limit needs a count");
                }
                if limit.is_some() {
                    anyhow::bail!("trace list limit specified twice");
                }
                limit = Some(parse_positive_usize(value, "--limit")?);
            }
            _ if !part.starts_with("--") => {
                if limit.is_some() {
                    anyhow::bail!("trace list limit specified twice");
                }
                limit = Some(parse_positive_usize(part, "trace list limit")?);
            }
            _ => anyhow::bail!("unknown trace list option: {part}"),
        }
    }
    Ok(limit.unwrap_or(20))
}

fn parse_compare_slash_args(
    rest: &str,
    last_run_id: Option<RunId>,
) -> anyhow::Result<CompareSlashArgs> {
    let parts = rest.split_whitespace().collect::<Vec<_>>();
    let args = match parts.as_slice() {
        [compare] => CompareSlashArgs {
            primary_run_id: parse_run_id_arg(None, last_run_id, "compare primary")?,
            compare_run_id: parse_run_id_arg(Some(compare), last_run_id, "compare target")?,
        },
        [primary, compare] => CompareSlashArgs {
            primary_run_id: parse_run_id_arg(Some(primary), last_run_id, "compare primary")?,
            compare_run_id: parse_run_id_arg(Some(compare), last_run_id, "compare target")?,
        },
        [] => anyhow::bail!("compare needs a run id, or primary and compare run ids"),
        _ => anyhow::bail!("compare accepts at most two run ids"),
    };
    if args.primary_run_id == args.compare_run_id {
        anyhow::bail!("compare needs two different run ids");
    }
    Ok(args)
}

fn parse_run_id_arg(
    value: Option<&str>,
    last_run_id: Option<RunId>,
    label: &str,
) -> anyhow::Result<RunId> {
    match value {
        None | Some("last") => last_run_id.ok_or_else(|| anyhow::anyhow!("{label} needs a run id")),
        Some(value) => Ok(RunId(uuid::Uuid::parse_str(value)?)),
    }
}

fn parse_positive_usize(value: &str, label: &str) -> anyhow::Result<usize> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("{label} needs a positive integer"))?;
    if parsed == 0 {
        anyhow::bail!("{label} needs a positive integer");
    }
    Ok(parsed)
}

fn trace_summary_report(summary: &TraceSummary) -> String {
    let cost = summary
        .cost_usd
        .map(|value| format!("${value:.6}"))
        .unwrap_or_else(|| "n/a".into());
    let duration = summary
        .duration_ms
        .map(|value| format!("{value}ms"))
        .unwrap_or_else(|| "n/a".into());
    let score = summary
        .quality_score_average
        .map(|value| format!("{value:.1}/10"))
        .unwrap_or_else(|| "n/a".into());
    [
        format!("Trace {}", summary.run_id.0),
        format!(
            "events {}, contexts {}, llm {}, tools {}",
            summary.events, summary.context_snapshots, summary.llm_calls, summary.tool_calls
        ),
        format!(
            "tokens {}/{}, cost {}, duration {}",
            summary.tokens_in, summary.tokens_out, cost, duration
        ),
        format!(
            "approvals {}, guidance {}, quality {}, hooks {}/{} failed",
            summary.approvals,
            summary.guidance_injections,
            score,
            summary.hooks,
            summary.hook_failures
        ),
        format!(
            "memory fragments {}, artifact refs {}",
            summary.memory_fragments, summary.artifact_refs
        ),
    ]
    .join("\n")
}

fn trace_run_records_report(records: &[TraceRunRecord]) -> String {
    if records.is_empty() {
        return "No trace runs found.".into();
    }
    let mut lines = Vec::new();
    lines.push(format!("Recent trace runs ({})", records.len()));
    for record in records {
        lines.push(format!(
            "- {} status={} events={} children={} agent={} updated={}",
            record.run_id.0,
            record.status,
            record.event_count,
            record.child_run_count,
            record.agent_id.as_deref().unwrap_or("unknown"),
            record.updated_at
        ));
        if let Some(input) = record
            .input_preview
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("  input: {input}"));
        }
        if let Some(output) = record
            .final_output_preview
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            lines.push(format!("  output: {output}"));
        }
    }
    lines.join("\n")
}

fn format_conversation_usage_report(report: &ConversationUsageReport) -> String {
    let range = match (report.from, report.to) {
        (Some(from), Some(to)) => format!("{from}:{to}"),
        _ => "empty".into(),
    };
    let cost = report
        .totals
        .cost_usd
        .map(|value| format!("${value:.6}"))
        .unwrap_or_else(|| "n/a".into());
    let duration = report
        .totals
        .duration_ms
        .map(|value| format!("{value}ms"))
        .unwrap_or_else(|| "n/a".into());
    [
        format!("Conversation usage {}", report.conversation_id),
        format!("range {range}, messages {}", report.message_count),
        format!("runs {}/{}", report.trace_count, report.run_ids.len()),
        format!(
            "tokens {}/{}, cost {}, duration {}",
            report.totals.tokens_in, report.totals.tokens_out, cost, duration
        ),
        format!(
            "events {}, llm {}, tools {}, hooks {}/{} failed",
            report.totals.events,
            report.totals.llm_calls,
            report.totals.tool_calls,
            report.totals.hooks,
            report.totals.hook_failures
        ),
        format!(
            "incomplete: {} unlinked message(s), {} missing trace(s)",
            report.missing_run_id_messages.len(),
            report.missing_traces.len()
        ),
    ]
    .join("\n")
}

fn trace_tree_report(tree: &TraceTreeNode) -> String {
    let mut lines = vec![format!("Trace tree {}", tree.run_id.0)];
    push_trace_tree_report_node(&mut lines, tree, 0);
    lines.join("\n")
}

fn push_trace_tree_report_node(lines: &mut Vec<String>, node: &TraceTreeNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let agent = node.agent_id.as_deref().unwrap_or("unknown");
    let trace = if node.trace_available {
        format!("events={}", node.event_count)
    } else {
        "trace=missing".into()
    };
    let link = node
        .link_status
        .as_deref()
        .map(|status| format!(" link_status={status}"))
        .unwrap_or_default();
    lines.push(format!(
        "{indent}- {} agent={} status={} {}{}",
        node.run_id.0, agent, node.status, trace, link
    ));
    for child in &node.children {
        push_trace_tree_report_node(lines, child, depth + 1);
    }
}

fn trace_compare_report(comparison: &TraceComparison) -> String {
    let mut lines = vec![
        format!(
            "Trace compare {} -> {}",
            comparison.primary_run_id.0, comparison.compare_run_id.0
        ),
        format!(
            "primary tree: {} run(s), {} leaf run(s), depth {}",
            comparison.primary_tree.runs,
            comparison.primary_tree.leaf_runs,
            comparison.primary_tree.max_depth
        ),
        format!(
            "compare tree: {} run(s), {} leaf run(s), depth {}",
            comparison.compare_tree.runs,
            comparison.compare_tree.leaf_runs,
            comparison.compare_tree.max_depth
        ),
        "metric | primary | compare | delta".into(),
    ];
    lines.extend(comparison.rows.iter().map(|row| {
        format!(
            "{} | {} | {} | {}",
            row.label, row.primary, row.compare, row.delta
        )
    }));
    lines.join("\n")
}

fn append_guidance_event(store: &dyn EventStore, run_id: RunId, text: &str) -> anyhow::Result<()> {
    let content = validate_guidance_content(text)?;
    let events = store.events(run_id);
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
        RunEventKind::GuidanceInjected { content },
    );
    Ok(())
}

fn open_event_store() -> anyhow::Result<SqliteEventStore> {
    let paths = StoragePaths::from_env();
    paths.ensure_base_dirs()?;
    Ok(SqliteEventStore::open(paths.state_db())?)
}

fn context_value_has_auto_compaction(snapshot: &serde_json::Value) -> bool {
    serde_json::from_value::<ContextSnapshot>(snapshot.clone())
        .ok()
        .is_some_and(|snapshot| crate::headless::is_auto_compaction_snapshot(&snapshot))
}

fn active_batch_child_event(app: &App, run_id: RunId) -> bool {
    app.active_batch_run_id.is_some_and(|batch| batch != run_id)
}

fn handle_run_event(app: &mut App, evt: &RunEvent) {
    match &evt.kind {
        RunEventKind::RunStarted { .. } => {
            if !active_batch_child_event(app, evt.run_id) {
                app.last_run_id = Some(evt.run_id);
                if let Some(request) = app.pending_replay_comparison.as_mut()
                    && request.replay_run_id.is_none()
                {
                    request.replay_run_id = Some(evt.run_id);
                }
                app.pending_auto_compaction_run = None;
            }
        }
        RunEventKind::ContextBuilt { snapshot } => {
            update_tool_budget_from_snapshot(app, snapshot);
            if context_value_has_auto_compaction(snapshot) {
                app.pending_auto_compaction_run = Some(evt.run_id);
            }
            let visible_tools = snapshot
                .get("visible_tools")
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len);
            let memory_fragments = snapshot
                .get("loaded_memory")
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len);
            push_event(
                app,
                format!(
                    "Context built ({visible_tools} tools, {memory_fragments} memory fragments)"
                ),
            );
        }
        RunEventKind::LlmRequestStarted {
            model,
            request_digest,
        } => {
            let digest = request_digest
                .as_ref()
                .map(|value| format!(" digest {}", short_digest(value)))
                .unwrap_or_default();
            push_event(app, format!("LLM call started ({model}){digest}"));
        }
        RunEventKind::LlmStreamToken { delta } => {
            if !delta.trim().is_empty() {
                push_event(app, format!("LLM stream token {}", delta.escape_debug()));
            }
        }
        RunEventKind::LlmRequestCompleted {
            tokens_in,
            tokens_out,
            cost_usd,
            duration_ms,
        } => {
            app.tokens_in += tokens_in;
            app.tokens_out += tokens_out;
            if let Some(value) = cost_usd {
                app.cost_usd += value;
            }
            let cost = cost_usd
                .map(|value| format!(", ${value:.6}"))
                .unwrap_or_default();
            push_event(
                app,
                format!(
                    "LLM call completed (in: {tokens_in}, out: {tokens_out}{cost}, {duration_ms} ms)"
                ),
            );
        }
        RunEventKind::PromptRefinementStarted { model, .. } => {
            push_event(app, format!("Prompt refinement started ({model})"));
        }
        RunEventKind::PromptRefinementCompleted {
            refined_input,
            tokens_in,
            tokens_out,
            cost_usd,
            duration_ms,
        } => {
            app.tokens_in += tokens_in;
            app.tokens_out += tokens_out;
            if let Some(value) = cost_usd {
                app.cost_usd += value;
            }
            push_event(
                app,
                format!("Prompt refined (in: {tokens_in}, out: {tokens_out}, {duration_ms} ms)"),
            );
            push_event(app, format!("Refined prompt: {refined_input}"));
        }
        RunEventKind::ToolCallProposed {
            tool_id,
            input,
            call_id,
            ..
        } => {
            push_event(
                app,
                format!("Tool proposed: {tool_id}({input}) [{call_id}]"),
            );
        }
        RunEventKind::ToolCallStarted { call_id } => {
            push_event(app, format!("Tool started [{call_id}]"));
        }
        RunEventKind::ToolCallCompleted {
            call_id,
            output,
            cost_usd,
            duration_ms,
        } => {
            app.calls_used += 1;
            app.calls_remaining = app.calls_max.saturating_sub(app.calls_used);
            if let Some(value) = cost_usd {
                app.cost_usd += value;
            }
            let cost = cost_usd
                .map(|value| format!(", ${value:.6}"))
                .unwrap_or_default();
            push_event(
                app,
                format!("Tool completed [{call_id}] -> {output} ({duration_ms} ms{cost})"),
            );
        }
        RunEventKind::ToolOutputInterpreted {
            call_id,
            model,
            summary,
        } => {
            push_event(
                app,
                format!("Tool output queued for interpretation [{call_id}] by {model}: {summary}"),
            );
        }
        RunEventKind::ToolCallFailed { call_id, error } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Tool failed [{call_id}]: {error}"),
            });
        }
        RunEventKind::ApprovalRequested {
            approval_id,
            action,
            ..
        } => {
            push_event(
                app,
                format!("Approval requested [{approval_id}] for {action}"),
            );
        }
        RunEventKind::ApprovalResolved {
            approval_id,
            approved,
            ..
        } => {
            push_event(
                app,
                format!("Approval resolved [{approval_id}] approved={approved}"),
            );
        }
        RunEventKind::ApprovalControllerAssessed {
            approval_id,
            controller_agent,
            recommendation,
            ..
        } => {
            push_event(
                app,
                format!(
                    "Approval controller {controller_agent} assessed [{approval_id}] recommendation={recommendation}"
                ),
            );
        }
        RunEventKind::GuidanceInjected { content } => {
            push_event(app, format!("Guidance injected: {content}"));
        }
        RunEventKind::QualityScored { target, score } => {
            push_event(app, format!("Quality scored {target}: {score}/10"));
        }
        RunEventKind::MemoryLoaded { ids } => {
            push_event(app, format!("Memory loaded: {}", ids.join(", ")));
        }
        RunEventKind::MemoryRead {
            backend,
            fragment_ids,
        } => {
            push_event(
                app,
                format!("Memory read via {backend}: {}", fragment_ids.join(", ")),
            );
        }
        RunEventKind::MemoryWritten {
            id,
            operation,
            source_range,
            generating_model,
        } => {
            let range = source_range
                .as_ref()
                .map(|value| format!(" (range: {value})"))
                .unwrap_or_default();
            let model = generating_model
                .as_ref()
                .map(|value| format!(" via {value}"))
                .unwrap_or_default();
            push_event(app, format!("Memory {operation}: {id}{range}{model}"));
        }
        RunEventKind::IngestionReferenced {
            artifact_id,
            source,
        } => {
            push_event(
                app,
                format!("Ingestion referenced: {artifact_id} ({source})"),
            );
        }
        RunEventKind::IngestionStarted { source, backend } => {
            push_event(app, format!("Ingestion started: {source} via {backend}"));
        }
        RunEventKind::IngestionCompleted {
            artifact_id,
            sections,
            findings,
            high_risk_findings,
            finding_snippets,
            ..
        } => {
            let risk = if *high_risk_findings > 0 {
                format!("; {high_risk_findings} high-risk findings")
            } else if !findings.is_empty() {
                format!("; {} findings", findings.len())
            } else {
                String::new()
            };
            let snippet = finding_snippets
                .first()
                .map(|snippet| format!("; source {snippet:?}"))
                .unwrap_or_default();
            push_event(
                app,
                format!("Ingestion completed: {artifact_id} ({sections} sections{risk}{snippet})"),
            );
        }
        RunEventKind::HookFired {
            hook_id,
            trigger,
            payload_digest,
        } => {
            push_event(
                app,
                format!(
                    "Hook fired: {hook_id} ({trigger}, digest {})",
                    short_digest(payload_digest)
                ),
            );
        }
        RunEventKind::HookFailed {
            hook_id,
            trigger,
            error,
            attempt,
            will_retry,
        } => {
            let retry = if *will_retry { "retrying" } else { "final" };
            app.transcript.push(TranscriptLine {
                kind: if *will_retry {
                    LineKind::Event
                } else {
                    LineKind::Error
                },
                text: format!(
                    "Hook failed: {hook_id} ({trigger}) attempt {attempt} {retry}: {error}"
                ),
            });
        }
        RunEventKind::PolicyDenied { reason } => {
            push_event(app, format!("Policy denied: {reason}"));
        }
        RunEventKind::ChildRunStarted {
            child_run_id,
            agent_id,
        } => {
            push_event(
                app,
                format!("Child run started: {} ({agent_id})", child_run_id.0),
            );
        }
        RunEventKind::ChildRunCompleted {
            child_run_id,
            status,
        } => {
            push_event(
                app,
                format!("Child run completed: {} ({status})", child_run_id.0),
            );
        }
        RunEventKind::BatchRunStarted { batch_id, items } => {
            app.last_run_id = Some(evt.run_id);
            app.active_batch_run_id = Some(evt.run_id);
            push_event(app, format!("Batch started: {batch_id} ({items} items)"));
        }
        RunEventKind::BatchItemStatus {
            batch_id,
            item_key,
            status,
        } => {
            push_event(app, format!("Batch {batch_id} item {item_key}: {status}"));
        }
        RunEventKind::BatchRunCompleted {
            batch_id,
            succeeded,
            failed,
        } => {
            push_event(
                app,
                format!("Batch completed: {batch_id} ({succeeded} ok, {failed} failed)"),
            );
            update_elapsed_time(app);
            app.run_started_at = None;
            app.active_run_handle = None;
            app.active_batch_run_id = None;
            app.state = AppState::Idle;
        }
        RunEventKind::RunPaused { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run paused: {reason}"),
            });
            if !active_batch_child_event(app, evt.run_id) {
                update_elapsed_time(app);
                app.run_started_at = None;
                app.active_run_handle = None;
                app.active_batch_run_id = None;
                app.state = AppState::Idle;
                maybe_append_replay_comparison(app, evt.run_id);
            }
        }
        RunEventKind::RunCancelled { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run cancelled: {reason}"),
            });
            if !active_batch_child_event(app, evt.run_id) {
                update_elapsed_time(app);
                app.run_started_at = None;
                app.active_run_handle = None;
                app.active_batch_run_id = None;
                app.state = AppState::Idle;
                maybe_append_replay_comparison(app, evt.run_id);
            }
        }
        RunEventKind::RunCompleted {
            final_output,
            total_cost_usd,
            total_duration_ms,
        } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Assistant,
                text: final_output.clone(),
            });
            if let Some(value) = total_cost_usd {
                app.cost_usd = *value;
            }
            let cost = total_cost_usd
                .map(|value| format!(", ${value:.6}"))
                .unwrap_or_default();
            push_event(
                app,
                format!("Run completed in {total_duration_ms} ms{cost}"),
            );
            if app.pending_auto_compaction_run == Some(evt.run_id) {
                push_event(
                    app,
                    "Auto compacted context ready. Type /compact keep or /compact keep-run to save it, or /compact dismiss.".into(),
                );
            }
            if !active_batch_child_event(app, evt.run_id) {
                app.elapsed_ms = *total_duration_ms;
                app.run_started_at = None;
                app.active_run_handle = None;
                app.active_batch_run_id = None;
                app.state = AppState::Idle;
                maybe_append_replay_comparison(app, evt.run_id);
            }
        }
        RunEventKind::RunFailed { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run failed: {reason}"),
            });
            if !active_batch_child_event(app, evt.run_id) {
                update_elapsed_time(app);
                app.run_started_at = None;
                app.active_run_handle = None;
                app.active_batch_run_id = None;
                app.state = AppState::Idle;
                maybe_append_replay_comparison(app, evt.run_id);
            }
        }
    }
}

fn maybe_append_replay_comparison(app: &mut App, replay_run_id: RunId) {
    let Some(request) = app.pending_replay_comparison else {
        return;
    };
    if request.replay_run_id != Some(replay_run_id) {
        return;
    }
    app.pending_replay_comparison = None;
    let store = match open_event_store() {
        Ok(store) => store,
        Err(err) => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Replay comparison failed: {err}"),
            });
            return;
        }
    };
    match build_trace_comparison(request.source_run_id, replay_run_id, |id| {
        store.try_events(id)
    }) {
        Ok(comparison) => app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: trace_compare_report(&comparison),
        }),
        Err(err) => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: format!("Replay comparison failed: {err}"),
        }),
    }
}

fn stop_active_run(app: &mut App, reason: &str, mode: StopRetentionMode) {
    if let Some(handle) = app.active_run_handle.take() {
        handle.abort();
    }
    if let Some(run_id) = app.last_run_id {
        match open_event_store() {
            Ok(store) => match record_stop_event(&store, run_id, reason.to_string()) {
                Ok(events) => {
                    if mode.summarises() {
                        match create_tui_stop_compaction(run_id, reason, &events) {
                            Ok(record) => push_event(
                                app,
                                format!("Stopped-run summary retained as {}", record.id),
                            ),
                            Err(err) => app.transcript.push(TranscriptLine {
                                kind: LineKind::Error,
                                text: format!("Stop summary retention failed: {err}"),
                            }),
                        }
                    }
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Stop trace write failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Stop trace write failed: {err}"),
            }),
        }
    }
    update_elapsed_time(app);
    app.run_started_at = None;
    app.active_batch_run_id = None;
    app.state = AppState::Idle;
    app.transcript.push(TranscriptLine {
        kind: LineKind::Error,
        text: format!("Run cancelled: {reason}"),
    });
}

fn record_stop_event(
    store: &dyn EventStore,
    run_id: RunId,
    reason: String,
) -> anyhow::Result<Vec<RunEvent>> {
    let mut events = store.events(run_id);
    let parent = latest_event_id(&events)
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no trace events"))?;
    let stopped = store.append(run_id, Some(parent), RunEventKind::RunCancelled { reason });
    events.push(stopped);
    Ok(events)
}

fn create_tui_stop_compaction(
    run_id: RunId,
    reason: &str,
    events: &[RunEvent],
) -> anyhow::Result<CompactionRecord> {
    Ok(CompactionStore::from_env().keep_compacted_context(
        &stopped_run_summary_text(run_id, reason, events),
        Some("Retained summary of a TUI run stopped by the user.".into()),
        Some(512),
        Some(format!("stopped-run:{}", run_id.0)),
        None,
    )?)
}

fn stopped_run_summary_text(run_id: RunId, reason: &str, events: &[RunEvent]) -> String {
    let llm_calls = events
        .iter()
        .filter(|event| matches!(event.kind, RunEventKind::LlmRequestCompleted { .. }))
        .count();
    let tool_calls = events
        .iter()
        .filter(|event| matches!(event.kind, RunEventKind::ToolCallCompleted { .. }))
        .count();
    let approvals = events
        .iter()
        .filter(|event| matches!(event.kind, RunEventKind::ApprovalRequested { .. }))
        .count();
    let guidance = events
        .iter()
        .filter(|event| matches!(event.kind, RunEventKind::GuidanceInjected { .. }))
        .count();
    let mut lines = vec![
        format!("Stopped TUI run {}", run_id.0),
        format!("Reason: {reason}"),
        format!(
            "Observed before stop: {} events, {llm_calls} LLM calls, {tool_calls} tool calls, {approvals} approvals, {guidance} guidance injections.",
            events.len()
        ),
    ];
    for event in events
        .iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        lines.push(format!(
            "- {} {}",
            event.id.0,
            stopped_run_event_label(event)
        ));
    }
    lines.join("\n")
}

fn stopped_run_event_label(event: &RunEvent) -> String {
    match &event.kind {
        RunEventKind::RunStarted { agent_id, input } => {
            format!(
                "run started agent={agent_id} input={}",
                compact_preview(input, 120)
            )
        }
        RunEventKind::ContextBuilt { snapshot } => {
            let model = snapshot
                .get("model")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let visible_tools = json_array_len(snapshot, "visible_tools");
            let visible_skills = json_array_len(snapshot, "visible_skills");
            let memory = json_array_len(snapshot, "memory");
            format!(
                "context built model={model} tools={visible_tools} skills={visible_skills} memories={memory}"
            )
        }
        RunEventKind::LlmRequestStarted { model, .. } => {
            format!("llm request started model={model}")
        }
        RunEventKind::LlmRequestCompleted {
            tokens_in,
            tokens_out,
            ..
        } => format!("llm request completed tokens_in={tokens_in} tokens_out={tokens_out}"),
        RunEventKind::ToolCallProposed { tool_id, .. } => {
            format!("tool proposed {tool_id}")
        }
        RunEventKind::ToolCallCompleted { call_id, .. } => {
            format!("tool completed {call_id}")
        }
        RunEventKind::ApprovalControllerAssessed {
            approval_id,
            recommendation,
            ..
        } => format!("approval assessed {approval_id} recommendation={recommendation}"),
        RunEventKind::GuidanceInjected { content } => {
            format!("guidance injected {}", compact_preview(content, 120))
        }
        RunEventKind::RunCancelled { reason } => format!("run cancelled: {reason}"),
        RunEventKind::RunFailed { reason } => format!("run failed: {reason}"),
        RunEventKind::RunCompleted { .. } => "run completed".into(),
        _ => "trace event".into(),
    }
}

fn json_array_len(value: &serde_json::Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

fn update_elapsed_time(app: &mut App) {
    if app.state == AppState::Running
        && let Some(started_at) = app.run_started_at
    {
        app.elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    }
}

fn push_event(app: &mut App, text: String) {
    app.transcript.push(TranscriptLine {
        kind: LineKind::Event,
        text,
    });
}

fn push_system_line(app: &mut App, text: &str) {
    app.transcript.push(TranscriptLine {
        kind: LineKind::Event,
        text: text.into(),
    });
}

fn short_digest(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

fn update_tool_budget_from_snapshot(app: &mut App, snapshot: &serde_json::Value) {
    let Some(limits) = snapshot.get("limits") else {
        return;
    };
    if let Some(max) = limits
        .get("max_tool_calls")
        .and_then(|value| value.as_u64())
    {
        app.calls_max = max.min(u32::MAX as u64) as u32;
    }
    if let Some(remaining) = limits
        .get("remaining_tool_calls")
        .and_then(|value| value.as_u64())
    {
        app.calls_remaining = remaining.min(u32::MAX as u64) as u32;
        app.calls_used = app.calls_max.saturating_sub(app.calls_remaining);
    }
}

// ---------- rendering ----------

fn render(f: &mut ratatui::Frame, app: &App) {
    if app.conversation_browser.is_some() {
        let browser_height = (f.area().height / 3).clamp(5, 14);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(browser_height),
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(3),
            ])
            .split(f.area());
        render_conversation_browser(f, chunks[0], app);
        render_transcript(f, chunks[1], app);
        render_status(f, chunks[2], app);
        render_input(f, chunks[3], app);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(1),
                Constraint::Length(3),
            ])
            .split(f.area());

        render_transcript(f, chunks[0], app);
        render_status(f, chunks[1], app);
        render_input(f, chunks[2], app);
    }
}

fn render_conversation_browser(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let Some(browser) = &app.conversation_browser else {
        return;
    };
    let visible_rows = usize::from(area.height.saturating_sub(2)).max(1);
    let mut offset = 0usize;
    if browser.selected >= visible_rows {
        offset = browser.selected + 1 - visible_rows;
    }
    let lines = browser
        .rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible_rows)
        .map(|(index, row)| {
            let selected = index == browser.selected;
            let marker = if selected { ">" } else { " " };
            let indent = "  ".repeat(row.depth);
            let reason = row
                .branch_reason
                .as_deref()
                .map(|reason| format!(" reason={}", compact_preview(reason, 64)))
                .unwrap_or_default();
            let topic = row
                .topic_preview
                .as_deref()
                .map(|topic| format!(" topic={}", compact_preview(topic, 64)))
                .unwrap_or_default();
            let branch_point = row
                .branch_point
                .map(|point| format!(" branch_point={point}"))
                .unwrap_or_default();
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::styled(
                format!(
                    "{marker} {}{title} ({id}) agent={agent} own={own} expanded={expanded}{branch_point}{topic}{reason}",
                    indent,
                    title = row.title.as_str(),
                    id = row.id.as_str(),
                    agent = row.agent_id.as_str(),
                    own = row.own_message_count,
                    expanded = row.expanded_message_count,
                ),
                style,
            )
        })
        .collect::<Vec<_>>();
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(
            " Conversation Browser: Up/Down move | Enter select | r recover | d delete plan | Esc close ",
        ))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_transcript(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = app
        .transcript
        .iter()
        .map(|tl| {
            let (prefix, style) = match tl.kind {
                LineKind::User => (
                    "user: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                LineKind::Assistant => (
                    "asst: ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                LineKind::Event => (
                    "  • ",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
                ),
                LineKind::Error => (
                    "  ✗ ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            };
            Line::from(vec![
                Span::styled(prefix, style),
                Span::raw(tl.text.clone()),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Shinkai "))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_status(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let state = match app.state {
        AppState::Idle => "idle",
        AppState::Running => "running…",
    };
    let text = format!(
        " tokens {}↑/{}↓ · cost ${:.6} · time {} · calls {}/{} · left {} · {}",
        app.tokens_in,
        app.tokens_out,
        app.cost_usd,
        format_duration(app.elapsed_ms),
        app.calls_used,
        app.calls_max,
        app.calls_remaining,
        state
    );
    let p = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

fn format_duration(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1_000.0)
    } else {
        let minutes = ms / 60_000;
        let seconds = (ms % 60_000) / 1_000;
        format!("{minutes}m {seconds}s")
    }
}

fn render_input(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let browser_active = app.conversation_browser.is_some();
    let title = if browser_active {
        " Conversation browser active "
    } else if app.state == AppState::Running {
        " /guide <text> to steer current run "
    } else {
        " Enter to send · Esc to quit "
    };
    let style = if app.state == AppState::Running || browser_active {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    let p = Paragraph::new(format!("> {}", app.input))
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);

    if app.state == AppState::Idle && !browser_active {
        // Inside the box: 1 char in for the left border + 2 for "> " + input length.
        let cursor_x = area.x + 3 + app.input.chars().count() as u16;
        let cursor_y = area.y + 1;
        f.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct HarnessHomeGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        dir: std::path::PathBuf,
    }

    impl HarnessHomeGuard {
        fn new() -> Self {
            let lock = crate::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = std::env::var_os("AGENT_HARNESS_HOME");
            let dir = std::env::temp_dir()
                .join(format!("tui-prompt-shortcut-test-{}", uuid::Uuid::new_v4()));
            unsafe {
                std::env::set_var("AGENT_HARNESS_HOME", &dir);
            }
            Self {
                _lock: lock,
                previous,
                dir,
            }
        }
    }

    impl Drop for HarnessHomeGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(value) = self.previous.take() {
                    std::env::set_var("AGENT_HARNESS_HOME", value);
                } else {
                    std::env::remove_var("AGENT_HARNESS_HOME");
                }
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn context_snapshot_updates_status_budget() {
        let mut app = App {
            calls_max: 5,
            calls_remaining: 5,
            ..App::default()
        };

        update_tool_budget_from_snapshot(
            &mut app,
            &json!({
                "limits": {
                    "max_tool_calls": 3,
                    "remaining_tool_calls": 1
                }
            }),
        );

        assert_eq!(app.calls_max, 3);
        assert_eq!(app.calls_remaining, 1);
        assert_eq!(app.calls_used, 2);
    }

    #[test]
    fn score_and_guidance_events_are_validated_and_parented() {
        let store = agent_tracing::InMemoryEventStore::new();
        let run_id = RunId::new();
        let started = store.append(
            run_id,
            None,
            RunEventKind::RunStarted {
                agent_id: "agent".into(),
                input: "task".into(),
            },
        );

        append_score_event(&store, run_id, "last_answer".into(), 8.0).unwrap();
        append_guidance_event(&store, run_id, "  steer this way  ").unwrap();

        let events = store.events(run_id);
        assert!(matches!(
            events[1].kind,
            RunEventKind::QualityScored { score, .. } if (score - 8.0).abs() < f32::EPSILON
        ));
        assert_eq!(events[1].parent_event, Some(started.id));
        assert!(matches!(
            &events[2].kind,
            RunEventKind::GuidanceInjected { content } if content == "steer this way"
        ));
        assert_eq!(events[2].parent_event, Some(events[1].id));
    }

    #[test]
    fn quality_score_report_lists_bookmarkable_events() {
        let store = agent_tracing::InMemoryEventStore::new();
        let run_id = RunId::new();
        store.append(
            run_id,
            None,
            RunEventKind::RunStarted {
                agent_id: "agent".into(),
                input: "task".into(),
            },
        );
        let scored = store.append(
            run_id,
            None,
            RunEventKind::QualityScored {
                target: "last_answer".into(),
                score: 7.5,
            },
        );
        let records = quality_score_records(&store.events(run_id));
        let report = quality_score_report(run_id, &records);

        assert!(report.contains("1 score(s), avg 7.5/10"));
        assert!(report.contains(&format!("#{} last_answer: 7.5/10", scored.id.0)));
    }

    #[test]
    fn guidance_rejects_terminal_runs() {
        let store = agent_tracing::InMemoryEventStore::new();
        let run_id = RunId::new();
        store.append(
            run_id,
            None,
            RunEventKind::RunCompleted {
                final_output: "done".into(),
                total_cost_usd: None,
                total_duration_ms: 1,
            },
        );

        assert!(append_guidance_event(&store, run_id, "too late").is_err());
        assert_eq!(store.events(run_id).len(), 1);
    }

    #[test]
    fn score_slash_rejects_out_of_range_values() {
        assert_eq!(
            parse_score_slash_rest("10").unwrap(),
            ScoreSlashRequest {
                run_id: None,
                target: "last_answer".into(),
                score: 10.0
            }
        );
        assert_eq!(
            parse_score_slash_rest("").unwrap(),
            ScoreSlashRequest {
                run_id: None,
                target: "last_answer".into(),
                score: 10.0
            }
        );
        assert_eq!(
            parse_score_slash_rest("conversation 9").unwrap(),
            ScoreSlashRequest {
                run_id: None,
                target: "conversation".into(),
                score: 9.0
            }
        );
        assert_eq!(
            parse_score_slash_rest("8 range:messages:1..3").unwrap(),
            ScoreSlashRequest {
                run_id: None,
                target: "range:messages:1..3".into(),
                score: 8.0
            }
        );
        let run_id = RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap());
        assert_eq!(
            parse_score_slash_rest(&format!("{} 8.5 regression check", run_id.0)).unwrap(),
            ScoreSlashRequest {
                run_id: Some(run_id),
                target: "regression check".into(),
                score: 8.5
            }
        );
        assert!(parse_score_slash_rest("11").is_err());
        assert!(parse_score_slash_rest("conversation").is_err());
        assert!(parse_score_slash_rest(&run_id.0.to_string()).is_err());
        assert!(parse_score_slash_rest(&format!("{} 11", run_id.0)).is_err());
    }

    #[test]
    fn slash_helpers_match_exact_command_names() {
        assert!(help_slash_command("/help"));
        assert!(help_slash_command("/?"));
        assert!(!help_slash_command("/helper"));
        assert!(slash_help_rest(""));
        assert!(slash_help_rest("help"));
        assert!(slash_help_rest("--help"));
        assert!(!slash_help_rest("helper"));
        assert!(global_slash_help_text().contains("/usage [current|last|trace|run|conversation]"));
        assert!(global_slash_help_text().contains("/subagent status|on|off"));
        assert!(global_slash_help_text().contains("/budget <n>"));
        assert!(global_slash_help_text().contains("/visibility full|descriptions|names|config"));
        assert!(global_slash_help_text().contains("/cost input|output|both|clear|status"));
        assert!(global_slash_help_text().contains("/memory on|off|status|preview"));
        assert!(global_slash_help_text().contains("/skills on|off|status|preview"));
        assert!(global_slash_help_text().contains("/ingest preview <id>"));
        assert!(global_slash_help_text().contains("/batch <line-delimited prompts>"));
        assert!(global_slash_help_text().contains("/bridges status"));
        assert!(global_slash_help_text().contains("/bridge-deliveries [list]"));
        assert_eq!(memory_slash_rest("/memory --help"), Some("--help"));
        assert_eq!(agents_slash_rest("/agents --help"), Some("--help"));
        assert_eq!(compact_slash_rest("/compactions --help"), Some("--help"));
        assert!(run_help_slash_command("/run help"));
        assert!(run_help_slash_command("/run --help"));
        assert!(!run_help_slash_command("/runner help"));
        let prompt_help = prompt_slash_help_text();
        assert!(prompt_help.contains("/run <name>"));
        assert!(prompt_help.contains("/prompts use <name>"));
        assert!(prompt_help.contains("/prompts preview <name>"));
        assert!(prompt_help.contains("/prompt is accepted"));
        assert!(agent_help_slash_command("/agent help"));
        assert!(agent_help_slash_command("/agent --help"));
        assert!(!agent_help_slash_command("/agents help"));
        assert!(!agent_help_slash_command("/agent helpful"));
        assert!(agent_switch_slash_help_text().contains("/agent <saved-agent-id>"));
        assert!(code_help_slash_command("/python"));
        assert!(code_help_slash_command("/python help"));
        assert!(code_help_slash_command("/python --help"));
        assert!(code_help_slash_command("/typescript"));
        assert!(code_help_slash_command("/typescript help"));
        assert!(code_help_slash_command("/typescript --help"));
        assert!(code_help_slash_command("/ts"));
        assert!(code_help_slash_command("/ts help"));
        assert!(code_help_slash_command("/ts --help"));
        assert!(!code_help_slash_command("/python helpful"));
        assert!(!code_help_slash_command("/typescript print('help')"));
        assert!(code_slash_help_text().contains("/typescript <code>"));
        assert_eq!(shell_slash_rest("/shell"), Some(""));
        assert_eq!(shell_slash_rest("/shell status"), Some("status"));
        assert_eq!(shell_slash_rest("/shell on"), Some("on"));
        assert_eq!(shell_slash_rest("/shells"), None);
        assert!(shell_slash_help_text().contains("/shell status"));
        assert_eq!(subagent_slash_rest("/subagent"), Some(""));
        assert_eq!(subagent_slash_rest("/subagent status"), Some("status"));
        assert_eq!(subagent_slash_rest("/subagents status"), None);
        assert!(subagent_slash_help_text().contains("/subagent status"));
        assert_eq!(budget_slash_rest("/budget"), Some(""));
        assert_eq!(budget_slash_rest("/budget 3"), Some("3"));
        assert_eq!(budget_slash_rest("/budgeting 3"), None);
        assert!(budget_slash_help_text().contains("/budget <n>"));
        assert_eq!(visibility_slash_rest("/visibility"), Some(""));
        assert_eq!(visibility_slash_rest("/visibility names"), Some("names"));
        assert_eq!(visibility_slash_rest("/visibilityx names"), None);
        assert!(visibility_slash_help_text().contains("/visibility names"));
        assert!(cost_help_slash_command("/cost help"));
        assert!(cost_help_slash_command("/cost --help"));
        assert!(!cost_help_slash_command("/cost helper"));
        assert!(!cost_help_slash_command("/costs help"));
        assert_eq!(cost_slash_rest("/cost"), Some(""));
        assert_eq!(cost_slash_rest("/cost status"), Some("status"));
        assert_eq!(cost_slash_rest("/costs status"), None);
        assert!(cost_slash_help_text().contains("/cost both <input> <output>"));
        assert!(voice_help_slash_command("/voice help"));
        assert!(voice_help_slash_command("/voice --help"));
        assert!(!voice_help_slash_command("/voice helper"));
        assert!(!voice_help_slash_command("/voices help"));
        assert!(voice_slash_help_text().contains("/voice transcribe <path>"));
        assert!(tool_help_slash_command("/tool help"));
        assert!(tool_help_slash_command("/tool --help"));
        assert!(tool_help_slash_command("/tool!help"));
        assert!(tool_help_slash_command("/tool!--help"));
        assert!(!tool_help_slash_command("/tools help"));
        assert!(!tool_help_slash_command("/tool helper"));
        assert!(tool_slash_help_text().contains("/tool!<name> <json>"));
        assert!(preview_help_slash_command("/preview help"));
        assert!(preview_help_slash_command("/preview --help"));
        assert!(!preview_help_slash_command("/preview helper"));
        assert!(!preview_help_slash_command("/previewer help"));
        assert!(preview_slash_help_text().contains("/preview <prompt>"));
        assert!(resume_help_slash_command("/resume help"));
        assert!(resume_help_slash_command("/resume --help"));
        assert!(resume_help_slash_command("/resume plan help"));
        assert!(resume_help_slash_command("/resume plan --help"));
        assert!(resume_help_slash_command("/resume-plan help"));
        assert!(resume_help_slash_command("/resume-plan --help"));
        assert!(!resume_help_slash_command("/resumed help"));
        assert!(!resume_help_slash_command("/resume helper"));
        assert!(resume_slash_help_text().contains("/resume-plan [last|run-id]"));
        assert!(trace_help_slash_command("/trace help"));
        assert!(trace_help_slash_command("/trace --help"));
        assert!(trace_slash_help_text().contains("/trace scores [last|run-id]"));
        assert!(trace_slash_help_text().contains("/trace clear"));
        assert!(!trace_help_slash_command("/trace helper"));
        assert!(!trace_help_slash_command("/traces help"));
        assert!(trace_slash_help_text().contains("/trace tree [last|run-id]"));
        assert!(trace_slash_help_text().contains("/trace prompt [last|run-id]"));
        assert!(compare_help_slash_command("/compare help"));
        assert!(compare_help_slash_command("/compare --help"));
        assert!(!compare_help_slash_command("/compare helper"));
        assert!(!compare_help_slash_command("/compared help"));
        assert!(compare_slash_help_text().contains("/compare [last|primary-run-id]"));
        assert!(replay_help_slash_command("/replay help"));
        assert!(replay_help_slash_command("/replay --help"));
        assert!(!replay_help_slash_command("/replay helper"));
        assert!(!replay_help_slash_command("/replayed help"));
        assert!(replay_slash_help_text().contains("[--no-hooks]"));
        assert!(guide_help_slash_command("/guide help"));
        assert!(guide_help_slash_command("/guide --help"));
        assert!(!guide_help_slash_command("/guide helper"));
        assert!(!guide_help_slash_command("/guidance help"));
        assert!(guide_slash_help_text().contains("/guide <text>"));
        assert!(guide_slash_help_text().contains("/guide <last|run-id> <text>"));
        let guide_run_id = RunId(uuid::Uuid::new_v4());
        assert_eq!(
            parse_guide_slash_args("steer this run", Some(guide_run_id)).unwrap(),
            GuideSlashRequest {
                run_id: Some(guide_run_id),
                text: "steer this run".into(),
            }
        );
        assert_eq!(
            parse_guide_slash_args("last steer this run", Some(guide_run_id)).unwrap(),
            GuideSlashRequest {
                run_id: Some(guide_run_id),
                text: "steer this run".into(),
            }
        );
        assert_eq!(
            parse_guide_slash_args(&format!("{} steer this run", guide_run_id.0), None).unwrap(),
            GuideSlashRequest {
                run_id: Some(guide_run_id),
                text: "steer this run".into(),
            }
        );
        assert!(parse_guide_slash_args("", Some(guide_run_id)).is_err());
        assert!(parse_guide_slash_args("last", Some(guide_run_id)).is_err());
        assert!(score_help_slash_command("/score help"));
        assert!(score_help_slash_command("/score --help"));
        assert!(score_help_slash_command("/scores help"));
        assert!(score_help_slash_command("/scores --help"));
        assert!(!score_help_slash_command("/score helper"));
        assert!(!score_help_slash_command("/scoreboard help"));
        assert!(score_slash_help_text().contains("/scores [last|run-id]"));
        assert!(usage_help_slash_command("/usage help"));
        assert!(usage_help_slash_command("/usage --help"));
        assert!(!usage_help_slash_command("/usage helper"));
        assert!(!usage_help_slash_command("/usageful help"));
        assert!(usage_slash_help_text().contains("/usage last"));
        assert!(usage_slash_help_text().contains("/usage trace [last|run-id]"));
        assert!(global_slash_help_text().contains("/stop status"));
        assert!(global_slash_help_text().contains("default|--summarise|--discard"));
        assert!(stop_help_slash_command("/stop help"));
        assert!(stop_help_slash_command("/stop --help"));
        assert!(!stop_help_slash_command("/stop helper"));
        assert!(!stop_help_slash_command("/stopped help"));
        assert!(stop_status_slash_command("/stop status"));
        assert!(!stop_status_slash_command("/stop status now"));
        assert!(stop_slash_help_text().contains("default|--summarise|--discard"));
        assert!(stop_slash_help_text().contains("/stop status"));
        assert!(batch_help_slash_command("/batch help"));
        assert!(batch_help_slash_command("/batch --help"));
        assert!(batch_help_slash_command("/resume-batch help"));
        assert!(batch_help_slash_command("/resume-batch --help"));
        assert!(!batch_help_slash_command("/batch helper"));
        assert!(batch_slash_help_text().contains("/resume-batch <batch-id>"));
        assert!(batch_slash_help_text().contains("/batch files <line-delimited paths>"));
        assert!(batch_slash_help_text().contains("/batch folder <path>"));
        assert_eq!(
            code_slash_command("/python print(1)"),
            Some(("code_python", "print(1)"))
        );
        assert_eq!(
            code_slash_command("/typescript console.log(1)"),
            Some(("code_typescript", "console.log(1)"))
        );
        assert_eq!(
            code_slash_command("/ts console.log(1)"),
            Some(("code_typescript", "console.log(1)"))
        );
        assert_eq!(code_slash_command("/pythonista print(1)"), None);
        assert_eq!(voice_slash_rest("/voice"), Some(""));
        assert_eq!(
            voice_slash_rest("/voice transcribe ./sample.wav"),
            Some("transcribe ./sample.wav")
        );
        assert_eq!(voice_slash_rest("/voices"), None);
        assert_eq!(bridges_slash_rest("/bridges"), Some(""));
        assert_eq!(bridges_slash_rest("/bridges status"), Some("status"));
        assert_eq!(bridges_slash_rest("/bridgesx status"), None);
        assert!(bridges_slash_help_text().contains("/bridges status"));
        assert_eq!(bridge_deliveries_slash_rest("/bridge-deliveries"), Some(""));
        assert_eq!(
            bridge_deliveries_slash_rest("/bridge-deliveries delete delivery-1"),
            Some("delete delivery-1")
        );
        assert_eq!(bridge_deliveries_slash_rest("/bridge-delivery"), None);
        assert!(bridge_deliveries_slash_help_text().contains("delete <id> --confirm"));
        assert_eq!(
            crate::x402_slash::slash_rest("/x402 request https://example.test"),
            Some("request https://example.test")
        );
        assert_eq!(
            crate::x402_slash::slash_rest("/payment x402-request https://example.test"),
            Some("x402-request https://example.test")
        );
        assert!(crate::x402_slash::is_help("--help"));
        assert_eq!(crate::x402_slash::slash_rest("/payments"), None);
        assert_eq!(score_slash_rest("/score 7"), Some("7"));
        assert_eq!(score_slash_rest("/score"), Some(""));
        assert_eq!(score_slash_rest("/scoreboard 7"), None);
        assert_eq!(scores_slash_rest("/scores"), Some(""));
        assert_eq!(scores_slash_rest("/scores last"), Some("last"));
        assert_eq!(scores_slash_rest("/scoresboard"), None);
        assert_eq!(usage_slash_rest("/usage"), Some(""));
        assert_eq!(usage_slash_rest("/usage last"), Some("last"));
        assert_eq!(usage_slash_rest("/usage trace last"), Some("trace last"));
        assert_eq!(usage_slash_rest("/usageful"), None);
        assert_eq!(agent_slash_rest("/agent"), Some(""));
        assert_eq!(agent_slash_rest("/agent research"), Some("research"));
        assert_eq!(agent_slash_rest("/agents"), None);
        assert_eq!(
            stop_slash_rest("/stop changed my mind"),
            Some("changed my mind")
        );
        assert_eq!(stop_slash_rest("/stop"), Some(""));
        assert_eq!(stop_slash_rest("/stopped"), None);
        assert_eq!(resume_slash_rest("/resume"), Some(""));
        assert_eq!(
            resume_slash_rest("/resume last --from-event 7"),
            Some("last --from-event 7")
        );
        assert_eq!(resume_slash_rest("/resumed"), None);
        assert_eq!(batch_slash_rest("/batch"), Some(""));
        assert_eq!(batch_slash_rest("/batch first"), Some("first"));
        assert_eq!(batch_slash_rest("/batcher first"), None);
        assert_eq!(resume_batch_slash_rest("/resume-batch"), Some(""));
        assert_eq!(
            resume_batch_slash_rest("/resume-batch batch-1"),
            Some("batch-1")
        );
        assert_eq!(resume_batch_slash_rest("/resume-batched"), None);
        assert_eq!(resume_plan_slash_rest("/resume plan"), Some(""));
        assert_eq!(resume_plan_slash_rest("/resume-plan"), Some(""));
        assert_eq!(
            resume_plan_slash_rest("/resume plan last --from-event 7"),
            Some("last --from-event 7")
        );
        assert_eq!(
            resume_plan_slash_rest("/resume-plan last --from-event 7"),
            Some("last --from-event 7")
        );
        assert_eq!(replay_slash_rest("/replay"), Some(""));
        assert_eq!(
            replay_slash_rest("/replay last --no-hooks --compare-source"),
            Some("last --no-hooks --compare-source")
        );
        assert_eq!(replay_slash_rest("/replayed"), None);
        assert_eq!(trace_slash_rest("/trace"), Some(""));
        assert_eq!(trace_slash_rest("/trace tree last"), Some("tree last"));
        assert_eq!(trace_slash_rest("/traces"), None);
        assert_eq!(compare_slash_rest("/compare"), Some(""));
        assert_eq!(
            compare_slash_rest("/compare last run-2"),
            Some("last run-2")
        );
        assert_eq!(compare_slash_rest("/compared"), None);
        assert_eq!(
            parse_stop_request("--summarise changed my mind"),
            StopRequest {
                reason: "changed my mind".into(),
                mode: Some(StopRetentionMode::Summarise),
            }
        );
        assert_eq!(
            parse_stop_request("discard no longer needed"),
            StopRequest {
                reason: "no longer needed".into(),
                mode: Some(StopRetentionMode::Discard),
            }
        );
        assert_eq!(
            parse_stop_request("default no longer needed"),
            StopRequest {
                reason: "no longer needed".into(),
                mode: None,
            }
        );
        assert_eq!(
            parse_stop_request(""),
            StopRequest {
                reason: "user requested stop".into(),
                mode: None,
            }
        );
        assert_eq!(preview_slash_rest("/preview hello"), Some("hello"));
        assert_eq!(preview_slash_rest("/preview"), Some(""));
        assert_eq!(preview_slash_rest("/previewer hello"), None);
        assert_eq!(
            conversation_slash_rest("/conversation recover conv-1"),
            Some("recover conv-1")
        );
        assert_eq!(conversation_slash_rest("/conversation"), Some(""));
        assert_eq!(conversation_slash_rest("/conversations"), Some(""));
        assert_eq!(conversation_slash_rest("/conversations tree"), Some("tree"));
        assert_eq!(conversation_slash_rest("/conversationx"), None);
        assert_eq!(memory_slash_rest("/memory list"), Some("list"));
        assert_eq!(memory_slash_rest("/memory preview"), Some("preview"));
        assert_eq!(memory_slash_rest("/memory"), Some(""));
        assert_eq!(memory_slash_rest("/memories"), None);
        assert_eq!(capabilities_slash_rest("/capabilities list"), Some("list"));
        assert_eq!(
            capabilities_slash_rest("/capabilities doctor"),
            Some("doctor")
        );
        assert_eq!(
            capabilities_slash_rest("/capability doctor"),
            Some("doctor")
        );
        assert_eq!(capabilities_slash_rest("/capabilities"), Some(""));
        assert_eq!(capabilities_slash_rest("/capability"), Some(""));
        assert_eq!(capabilities_slash_rest("/capabilityx"), None);
        assert_eq!(artifacts_slash_rest("/artifacts list"), Some("list"));
        assert_eq!(artifacts_slash_rest("/artifacts"), Some(""));
        assert_eq!(artifacts_slash_rest("/artifact"), Some(""));
        assert_eq!(
            artifacts_slash_rest("/artifact show report"),
            Some("show report")
        );
        assert_eq!(
            artifacts_slash_rest("/artifacts preview artifact-1"),
            Some("preview artifact-1")
        );
        assert_eq!(artifacts_slash_rest("/artifactx"), None);
        assert_eq!(ingest_slash_rest("/ingest list"), Some("list"));
        assert_eq!(ingest_slash_rest("/ingest backends"), Some("backends"));
        assert_eq!(
            ingest_slash_rest("/ingest include artifact-1"),
            Some("include artifact-1")
        );
        assert_eq!(ingest_slash_rest("/ingest"), Some(""));
        assert_eq!(ingest_slash_rest("/ingester"), None);
        assert_eq!(
            single_ingest_id_arg("artifact-1", "include").unwrap(),
            "artifact-1"
        );
        assert!(single_ingest_id_arg("", "include").is_err());
        assert!(single_ingest_id_arg("artifact-1 extra", "include").is_err());
        assert_eq!(
            ingest_preview_args("artifact-1 summarize the doc").unwrap(),
            ("artifact-1", "summarize the doc")
        );
        assert_eq!(
            ingest_preview_args("artifact-1").unwrap(),
            ("artifact-1", "preview")
        );
        assert!(ingest_preview_args("").is_err());
        assert_eq!(approval_slash_rest("/approval list"), Some("list"));
        assert_eq!(approval_slash_rest("/approval"), Some(""));
        assert_eq!(approval_slash_rest("/approvals list"), Some("list"));
        assert_eq!(approval_slash_rest("/approvals"), Some(""));
        assert_eq!(approval_slash_rest("/approvalx"), None);
        assert_eq!(
            hooks_slash_rest("/hooks review run-1"),
            Some("review run-1")
        );
        let hook_run_id = RunId(uuid::Uuid::new_v4());
        assert_eq!(
            hooks_review_run_id("", Some(hook_run_id)).unwrap(),
            hook_run_id
        );
        assert_eq!(
            hooks_review_run_id("last", Some(hook_run_id)).unwrap(),
            hook_run_id
        );
        assert_eq!(
            hooks_review_run_id(&hook_run_id.0.to_string(), None).unwrap(),
            hook_run_id
        );
        assert!(hooks_review_run_id("last", None).is_err());
        assert!(hooks_review_run_id("not-a-run", Some(hook_run_id)).is_err());
        assert_eq!(hooks_slash_rest("/hooks"), Some(""));
        assert_eq!(hooks_slash_rest("/hook"), None);
        assert!(hook_view_args("list", "").is_ok());
        assert!(hook_view_args("policy", "").is_ok());
        assert!(hook_view_args("available", "").is_ok());
        assert!(hook_view_args("list", "--agent critic").is_err());
        assert!(hook_view_args("policy", "extra").is_err());
        assert_eq!(models_slash_rest("/models providers"), Some("providers"));
        assert_eq!(models_slash_rest("/models doctor"), Some("doctor"));
        assert_eq!(
            models_slash_rest("/models probe gpt-test"),
            Some("probe gpt-test")
        );
        assert_eq!(
            models_slash_rest("/models delete gpt-test"),
            Some("delete gpt-test")
        );
        assert_eq!(models_slash_rest("/model providers"), Some("providers"));
        assert_eq!(models_slash_rest("/models"), Some(""));
        assert_eq!(models_slash_rest("/model"), Some(""));
        assert_eq!(models_slash_rest("/modelx"), None);
        assert_eq!(agents_slash_rest("/agents list"), Some("list"));
        assert_eq!(agents_slash_rest("/agents"), Some(""));
        assert_eq!(agents_slash_rest("/agentz"), None);
        assert_eq!(profiles_slash_rest("/profiles list"), Some("list"));
        assert_eq!(profiles_slash_rest("/profile current"), Some("current"));
        assert_eq!(profiles_slash_rest("/profiles"), Some(""));
        assert_eq!(profiles_slash_rest("/profile"), Some(""));
        assert_eq!(profiles_slash_rest("/profilex"), None);
        assert_eq!(secrets_slash_rest("/secrets list"), Some("list"));
        assert_eq!(secrets_slash_rest("/secret backends"), Some("backends"));
        assert_eq!(secrets_slash_rest("/secrets"), Some(""));
        assert_eq!(secrets_slash_rest("/secret"), Some(""));
        assert_eq!(secrets_slash_rest("/secretsx"), None);
        assert_eq!(prompts_slash_rest("/prompts list"), Some("list"));
        assert_eq!(
            prompts_slash_rest("/prompts preview daily"),
            Some("preview daily")
        );
        assert_eq!(prompts_slash_rest("/prompt list"), Some("list"));
        assert_eq!(prompts_slash_rest("/prompts"), Some(""));
        assert_eq!(prompts_slash_rest("/prompt"), Some(""));
        assert_eq!(prompts_slash_rest("/promptx"), None);
        assert_eq!(skills_slash_rest("/skills list"), Some("list"));
        assert_eq!(skills_slash_rest("/skills preview"), Some("preview"));
        assert_eq!(skills_slash_rest("/skill list"), Some("list"));
        assert_eq!(skills_slash_rest("/skills"), Some(""));
        assert_eq!(skills_slash_rest("/skill"), Some(""));
        assert_eq!(skills_slash_rest("/skillx"), None);
        assert_eq!(
            storage_slash_rest("/storage prune-cache 30"),
            Some("prune-cache 30")
        );
        assert_eq!(storage_slash_rest("/storage"), Some(""));
        assert_eq!(storage_slash_rest("/storages"), None);
        assert_eq!(
            bundles_slash_rest("/bundles export ./bundle.tar"),
            Some("export ./bundle.tar")
        );
        assert_eq!(
            bundles_slash_rest("/bundle import ./bundle.tar --confirm"),
            Some("import ./bundle.tar --confirm")
        );
        assert_eq!(
            bundles_slash_rest("/bundle backup ./bundle.tar"),
            Some("backup ./bundle.tar")
        );
        assert_eq!(bundles_slash_rest("/bundles"), Some(""));
        assert_eq!(bundles_slash_rest("/bundle"), Some(""));
        assert_eq!(bundles_slash_rest("/bundlex"), None);
        assert_eq!(
            adapters_slash_rest("/adapters show adapter-1"),
            Some("show adapter-1")
        );
        assert_eq!(adapters_slash_rest("/adapters doctor"), Some("doctor"));
        assert_eq!(
            adapters_slash_rest("/adapters install-skill adapter-1"),
            Some("install-skill adapter-1")
        );
        assert_eq!(
            adapters_slash_rest("/adapters inspect ./adapter"),
            Some("inspect ./adapter")
        );
        assert_eq!(
            adapters_slash_rest("/adapters import ./adapter"),
            Some("import ./adapter")
        );
        assert_eq!(
            adapters_slash_rest("/adapters clawhub search ./clawhub.json"),
            Some("clawhub search ./clawhub.json")
        );
        assert_eq!(adapters_slash_rest("/adapter doctor"), Some("doctor"));
        assert_eq!(adapters_slash_rest("/adapters"), Some(""));
        assert_eq!(adapters_slash_rest("/adapter"), Some(""));
        assert_eq!(adapters_slash_rest("/adapterx"), None);
        assert_eq!(compact_slash_rest("/compact keep"), Some("keep"));
        assert_eq!(
            compact_slash_rest("/compact keep-run last"),
            Some("keep-run last")
        );
        assert_eq!(compact_slash_rest("/compactions list"), Some("list"));
        assert_eq!(
            compact_slash_rest("/compactions keep-run"),
            Some("keep-run")
        );
        assert_eq!(compact_slash_rest("/compact"), Some(""));
        assert_eq!(compact_slash_rest("/compactions"), Some(""));
        assert_eq!(compact_slash_rest("/compactness"), None);
        assert_eq!(compact_slash_rest("/compaction"), None);
    }

    #[test]
    fn bridge_delivery_slash_lists_and_deletes_dead_letters() {
        let _home = HarnessHomeGuard::new();
        let deliveries_dir = StoragePaths::from_env().bridge_deliveries_dir();
        std::fs::create_dir_all(&deliveries_dir).unwrap();
        std::fs::write(
            deliveries_dir.join("delivery-1.json"),
            serde_json::to_string_pretty(&json!({
                "id": "delivery-1",
                "target": "webhook.response_url",
                "url": "https://hooks.example.test/secret/token",
                "payload": {"text": "hello"},
                "last_delivery": {"delivered": false},
                "created_ms": 1,
                "updated_ms": 1
            }))
            .unwrap(),
        )
        .unwrap();

        let mut app = App::default();
        handle_bridge_deliveries_slash(&mut app, "");
        let listed = app.transcript.last().unwrap().text.clone();
        assert!(listed.contains("delivery-1"));
        assert!(listed.contains("https://hooks.example.test/<redacted>"));
        assert!(!listed.contains("secret/token"));

        handle_bridge_deliveries_slash(&mut app, "delete delivery-1");
        let preview = app.transcript.last().unwrap().text.clone();
        assert!(preview.contains("/bridge-deliveries delete delivery-1 --confirm"));
        assert!(deliveries_dir.join("delivery-1.json").exists());

        handle_bridge_deliveries_slash(&mut app, "delete delivery-1 --confirm");
        let deleted = app.transcript.last().unwrap().text.clone();
        assert!(deleted.contains("\"deleted\": true"));
        assert!(!deliveries_dir.join("delivery-1.json").exists());
    }

    #[test]
    fn prompt_shortcut_resolution_uses_active_agent_fallback() {
        let _home = HarnessHomeGuard::new();
        let store = PromptStore::from_env();
        store.save("daily", "global prompt").unwrap();
        store
            .save_for_agent("critic", "daily", "critic prompt")
            .unwrap();
        store
            .save_for_agent("builder", "daily", "builder prompt")
            .unwrap();

        let active = load_prompt_for_shortcut("daily", None, Some("critic")).unwrap();
        assert_eq!(active.body, "critic prompt");

        let explicit = load_prompt_for_shortcut("daily", Some("builder"), Some("critic")).unwrap();
        assert_eq!(explicit.body, "builder prompt");

        let global_fallback = load_prompt_for_shortcut("daily", None, Some("missing")).unwrap();
        assert_eq!(global_fallback.body, "global prompt");
    }

    #[test]
    fn compact_keep_resolves_pending_last_and_explicit_runs() {
        let pending_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000111").unwrap());
        let last_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000222").unwrap());
        let explicit_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000333").unwrap());
        let app = App {
            pending_auto_compaction_run: Some(pending_run),
            last_run_id: Some(last_run),
            ..App::default()
        };

        assert_eq!(resolve_compact_keep_run_id(&app, "").unwrap(), pending_run);
        assert_eq!(resolve_compact_keep_run_id(&app, "last").unwrap(), last_run);
        assert_eq!(
            resolve_compact_keep_run_id(&app, &explicit_run.0.to_string()).unwrap(),
            explicit_run
        );
        assert!(resolve_compact_keep_run_id(&app, "last extra").is_err());
        assert!(resolve_compact_keep_run_id(&App::default(), "").is_err());
        assert!(resolve_compact_keep_run_id(&App::default(), "last").is_err());
    }

    #[test]
    fn batch_slash_args_parse_items_and_resume_id() {
        assert_eq!(
            parse_batch_slash_items("first\nsecond\n\n third").unwrap(),
            vec!["first", "second", "third"]
        );
        assert_eq!(
            parse_batch_slash_items("single prompt").unwrap(),
            vec!["single prompt"]
        );
        assert_eq!(
            parse_resume_batch_slash_rest("batch-123").unwrap(),
            "batch-123"
        );
        let file_source = parse_batch_slash_run("files /tmp/a.txt\n/tmp/b.txt").unwrap();
        assert!(file_source.items.is_empty());
        assert_eq!(file_source.files, vec!["/tmp/a.txt", "/tmp/b.txt"]);
        assert!(file_source.folders.is_empty());
        let folder_source = parse_batch_slash_run("folder /tmp/batch folder").unwrap();
        assert!(folder_source.items.is_empty());
        assert!(folder_source.files.is_empty());
        assert_eq!(folder_source.folders, vec!["/tmp/batch folder"]);
        assert!(parse_batch_slash_items("").is_err());
        assert!(parse_batch_slash_run("files").is_err());
        assert!(parse_batch_slash_run("folder").is_err());
        assert!(parse_resume_batch_slash_rest("").is_err());
        assert!(parse_resume_batch_slash_rest("batch-123 extra").is_err());
    }

    #[test]
    fn batch_child_completion_keeps_tui_running_until_batch_completes() {
        let batch_run = RunId::new();
        let child_run = RunId::new();
        let mut app = App {
            state: AppState::Running,
            run_started_at: Some(Instant::now()),
            active_batch_run_id: Some(batch_run),
            last_run_id: Some(batch_run),
            ..App::default()
        };
        let store = agent_tracing::InMemoryEventStore::new();
        let child_completed = store.append(
            child_run,
            None,
            RunEventKind::RunCompleted {
                final_output: "child done".into(),
                total_cost_usd: None,
                total_duration_ms: 42,
            },
        );

        handle_run_event(&mut app, &child_completed);

        assert_eq!(app.state, AppState::Running);
        assert_eq!(app.active_batch_run_id, Some(batch_run));
        assert!(app.run_started_at.is_some());

        let batch_completed = store.append(
            batch_run,
            None,
            RunEventKind::BatchRunCompleted {
                batch_id: "batch-1".into(),
                succeeded: 1,
                failed: 0,
            },
        );
        handle_run_event(&mut app, &batch_completed);

        assert_eq!(app.state, AppState::Idle);
        assert_eq!(app.active_batch_run_id, None);
        assert_eq!(app.run_started_at, None);
    }

    #[tokio::test]
    async fn tui_batch_executor_persists_items_and_emits_batch_events() {
        let _home = HarnessHomeGuard::new();
        let batch_run = RunId::new();
        let store: Arc<dyn EventStore> = Arc::new(agent_tracing::InMemoryEventStore::new());
        let plan = BatchPlan::new(
            "batch-test",
            vec!["first prompt".into(), "second prompt".into()],
        );

        execute_tui_batch_plan(
            plan,
            batch_run,
            "batch-test".into(),
            Demo::Echo,
            setup::RuntimeOptions::default(),
            Arc::clone(&store),
        )
        .await
        .unwrap();

        let events = store.events(batch_run);
        assert!(matches!(
            events.first().map(|event| &event.kind),
            Some(RunEventKind::BatchRunStarted { items: 2, .. })
        ));
        assert!(events.iter().any(|event| matches!(
            &event.kind,
            RunEventKind::BatchItemStatus { status, .. } if status == "succeeded"
        )));
        assert!(matches!(
            events.last().map(|event| &event.kind),
            Some(RunEventKind::BatchRunCompleted {
                succeeded: 2,
                failed: 0,
                ..
            })
        ));
        let saved = BatchPlan::load_from_env("batch-test").unwrap();
        assert_eq!(saved.succeeded_count(), 2);
        assert_eq!(saved.failed_count(), 0);
    }

    #[test]
    fn resume_slash_args_parse_run_last_and_event() {
        let explicit_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap());
        let last_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000456").unwrap());

        let explicit =
            parse_resume_slash_args(&format!("{} --from-event 7", explicit_run.0), None).unwrap();
        assert_eq!(explicit.run_id, explicit_run);
        assert_eq!(explicit.from_event, Some(7));

        let last = parse_resume_slash_args("last --from-event=9", Some(last_run)).unwrap();
        assert_eq!(last.run_id, last_run);
        assert_eq!(last.from_event, Some(9));

        assert!(parse_resume_slash_args("--from-event", Some(last_run)).is_err());
        assert!(parse_resume_slash_args("last --from-event 0", Some(last_run)).is_err());
        assert!(parse_resume_slash_args("", None).is_err());
        assert!(parse_resume_slash_args("last extra", Some(last_run)).is_err());
    }

    #[test]
    fn replay_slash_args_parse_run_last_and_hook_override() {
        let explicit_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap());
        let last_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000456").unwrap());

        let explicit =
            parse_replay_slash_args(&format!("{} --no-hooks", explicit_run.0), None).unwrap();
        assert_eq!(explicit.run_id, explicit_run);
        assert!(explicit.no_hooks);
        assert!(!explicit.compare_source);

        let last =
            parse_replay_slash_args("last skip-hooks --compare-source", Some(last_run)).unwrap();
        assert_eq!(last.run_id, last_run);
        assert!(last.no_hooks);
        assert!(last.compare_source);
        assert_eq!(replay_flags_text(&last), " --no-hooks --compare-source");

        let default_last = parse_replay_slash_args("", Some(last_run)).unwrap();
        assert_eq!(default_last.run_id, last_run);
        assert!(!default_last.no_hooks);
        assert!(!default_last.compare_source);

        assert!(parse_replay_slash_args("", None).is_err());
        assert!(parse_replay_slash_args("last extra", Some(last_run)).is_err());
    }

    #[test]
    fn trace_replay_source_reads_original_prompt_and_agent() {
        let run_id = RunId::new();
        let store = agent_tracing::InMemoryEventStore::new();
        store.append(
            run_id,
            None,
            RunEventKind::RunStarted {
                agent_id: "researcher".into(),
                input: "write a memo".into(),
            },
        );

        let (agent_id, prompt) = trace_replay_source(run_id, &store.events(run_id)).unwrap();

        assert_eq!(agent_id, "researcher");
        assert_eq!(prompt, "write a memo");
        assert!(trace_replay_source(RunId::new(), &[]).is_err());
    }

    #[test]
    fn trace_and_compare_slash_args_accept_last_and_explicit_ids() {
        let primary = RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap());
        let compare = RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000456").unwrap());

        let trace = parse_trace_slash_args("", Some(primary)).unwrap();
        assert_eq!(trace.mode, TraceSlashMode::Overview);
        assert_eq!(trace.run_id, Some(primary));

        let tree = parse_trace_slash_args(&format!("tree {}", compare.0), Some(primary)).unwrap();
        assert_eq!(tree.mode, TraceSlashMode::Tree);
        assert_eq!(tree.run_id, Some(compare));

        let hooks = parse_trace_slash_args("hooks last", Some(primary)).unwrap();
        assert_eq!(hooks.mode, TraceSlashMode::Hooks);
        assert_eq!(hooks.run_id, Some(primary));

        let scores = parse_trace_slash_args("scores last", Some(primary)).unwrap();
        assert_eq!(scores.mode, TraceSlashMode::Scores);
        assert_eq!(scores.run_id, Some(primary));

        let prompt = parse_trace_slash_args("prompt last", Some(primary)).unwrap();
        assert_eq!(prompt.mode, TraceSlashMode::Prompt);
        assert_eq!(prompt.run_id, Some(primary));

        let list = parse_trace_slash_args("list", None).unwrap();
        assert_eq!(list.mode, TraceSlashMode::List);
        assert_eq!(list.run_id, None);
        assert_eq!(list.limit, 20);

        let clear = parse_trace_slash_args("clear", None).unwrap();
        assert_eq!(clear.mode, TraceSlashMode::Clear);
        assert_eq!(clear.run_id, None);

        let limited = parse_trace_slash_args("list 5", None).unwrap();
        assert_eq!(limited.mode, TraceSlashMode::List);
        assert_eq!(limited.limit, 5);

        let flag_limited = parse_trace_slash_args("runs --limit=6", None).unwrap();
        assert_eq!(flag_limited.mode, TraceSlashMode::List);
        assert_eq!(flag_limited.limit, 6);

        let implicit_primary =
            parse_compare_slash_args(&compare.0.to_string(), Some(primary)).unwrap();
        assert_eq!(implicit_primary.primary_run_id, primary);
        assert_eq!(implicit_primary.compare_run_id, compare);

        let explicit =
            parse_compare_slash_args(&format!("{} {}", primary.0, compare.0), None).unwrap();
        assert_eq!(explicit.primary_run_id, primary);
        assert_eq!(explicit.compare_run_id, compare);

        let explicit_primary_to_last =
            parse_compare_slash_args(&format!("{} last", compare.0), Some(primary)).unwrap();
        assert_eq!(explicit_primary_to_last.primary_run_id, compare);
        assert_eq!(explicit_primary_to_last.compare_run_id, primary);

        assert!(parse_trace_slash_args("summary", None).is_err());
        assert!(parse_trace_slash_args("tree last extra", Some(primary)).is_err());
        assert!(parse_trace_slash_args("hooks", None).is_err());
        assert!(parse_trace_slash_args("scores", None).is_err());
        assert!(parse_trace_slash_args("prompt", None).is_err());
        assert!(parse_trace_slash_args("clear extra", None).is_err());
        assert!(parse_trace_slash_args("list --limit 0", None).is_err());
        assert!(parse_trace_slash_args("list 5 6", None).is_err());
        assert!(parse_trace_slash_args("list --json", None).is_err());
        assert!(parse_compare_slash_args("", Some(primary)).is_err());
        assert!(parse_compare_slash_args("last", None).is_err());
        assert!(parse_compare_slash_args("last", Some(primary)).is_err());

        let selected_app = App {
            loaded_trace_run_id: Some(compare),
            last_run_id: Some(primary),
            ..App::default()
        };
        assert_eq!(trace_default_run_id(&selected_app), Some(compare));
        assert_eq!(
            usage_trace_default_run_id(&selected_app, "", "usage trace"),
            Some(compare)
        );
        assert_eq!(
            usage_trace_default_run_id(&selected_app, "last", "usage trace"),
            Some(primary)
        );
        assert_eq!(
            usage_trace_default_run_id(&selected_app, "", "usage run"),
            Some(primary)
        );
    }

    #[test]
    fn usage_trace_args_accept_last_and_explicit_ids() {
        let primary = RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap());
        let explicit =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000456").unwrap());

        assert_eq!(
            parse_usage_trace_run_id("", Some(primary), "usage trace").unwrap(),
            primary
        );
        assert_eq!(
            parse_usage_trace_run_id("last", Some(primary), "usage trace").unwrap(),
            primary
        );
        assert_eq!(
            parse_usage_trace_run_id(&explicit.0.to_string(), Some(primary), "usage trace")
                .unwrap(),
            explicit
        );

        assert!(parse_usage_trace_run_id("", None, "usage trace").is_err());
        assert!(parse_usage_trace_run_id("last", None, "usage trace").is_err());
        assert!(parse_usage_trace_run_id("last extra", Some(primary), "usage trace").is_err());
        let run_err = parse_usage_trace_run_id("last extra", Some(primary), "usage run")
            .unwrap_err()
            .to_string();
        assert!(run_err.contains("usage run accepts at most one run id"));
        assert!(parse_usage_trace_run_id("not-a-run", Some(primary), "usage trace").is_err());
    }

    #[test]
    fn scores_args_accept_last_and_explicit_ids() {
        let primary = RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000123").unwrap());
        let explicit =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000456").unwrap());

        assert_eq!(parse_scores_run_id("", Some(primary)).unwrap(), primary);
        assert_eq!(parse_scores_run_id("last", Some(primary)).unwrap(), primary);
        assert_eq!(
            parse_scores_run_id(&explicit.0.to_string(), Some(primary)).unwrap(),
            explicit
        );

        assert!(parse_scores_run_id("", None).is_err());
        assert!(parse_scores_run_id("last", None).is_err());
        assert!(parse_scores_run_id("last extra", Some(primary)).is_err());
        assert!(parse_scores_run_id("not-a-run", Some(primary)).is_err());
    }

    #[test]
    fn global_help_advertises_manual_tool_calls() {
        let help = global_slash_help_text();
        assert!(help.contains("/tool <name> <request>"));
        assert!(help.contains("/tool!<name> <json>"));
        assert!(help.contains("/python <code>"));
        assert!(help.contains("/voice status, /voice transcribe <path>"));
        assert!(help.contains("/x402 request"));
        assert!(help.contains("manual JSON input"));
        assert!(help.contains("/guide <text>"));
        assert!(help.contains("/subagent status|on|off"));
        assert!(help.contains("/budget <n>"));
        assert!(help.contains("/visibility full|descriptions|names|config"));
        assert!(help.contains("/cost input|output|both|clear|status"));
        assert!(help.contains("/memory on|off|status|preview"));
        assert!(help.contains("/skills on|off|status|preview"));
        assert!(help.contains("/conversation"));
    }

    #[test]
    fn voice_help_advertises_terminal_safe_paths() {
        let help = voice_slash_help_text();
        assert!(help.contains("/voice status"));
        assert!(help.contains("resolved active-agent voice config"));
        assert!(help.contains("/voice transcribe <path>"));
        assert!(help.contains("/voice speak <text>"));
        assert!(help.contains("saved audio paths"));
    }

    #[test]
    fn voice_status_reports_resolved_agent_voice_config() {
        let mut agent = setup::build_agent(&setup::RuntimeOptions::default());
        agent.voice.input_enabled = true;
        agent.voice.output_enabled = true;
        agent.voice.input_backend = Some("local".into());
        agent.voice.input_model = Some("whisper-local".into());
        agent.voice.output_backend = Some("cloud".into());
        agent.voice.tts_provider = Some("openai".into());
        agent.voice.tts_model = Some("tts-1".into());
        agent.voice.voice = Some("alloy".into());
        agent.voice.tone = Some("warm".into());

        let status = voice_slash_status_text(&agent);
        assert!(status.contains("Voice status:"));
        assert!(status.contains("agent: Fake Agent (fake-agent)"));
        assert!(status.contains("input: enabled"));
        assert!(status.contains("input backend: local"));
        assert!(status.contains("input model: whisper-local"));
        assert!(status.contains("output: enabled"));
        assert!(status.contains("output backend: cloud"));
        assert!(status.contains("tts provider: openai"));
        assert!(status.contains("tts model: tts-1"));
        assert!(status.contains("voice: alloy"));
        assert!(status.contains("tone: warm"));
        assert!(status.contains("/voice transcribe <path>"));
    }

    #[test]
    fn approval_args_accept_last_run_and_options() {
        let last_run =
            RunId(uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000456").unwrap());
        let explicit_run = "00000000-0000-0000-0000-000000000123";

        assert_eq!(
            approval_list_args("", Some(last_run)).unwrap(),
            last_run.0.to_string()
        );
        assert_eq!(
            approval_list_args("last", Some(last_run)).unwrap(),
            last_run.0.to_string()
        );
        assert_eq!(
            approval_action_args(
                "approval-1 --unlock-env APPROVAL_SECRET --signature-env=APPROVAL_SIG --controller-agent gatekeeper",
                Some(last_run),
                "approve",
                true,
                true,
            )
            .unwrap(),
            ApprovalActionArgs {
                run_id: last_run.0.to_string(),
                approval_id: "approval-1".into(),
                unlock_env: Some("APPROVAL_SECRET".into()),
                signature_env: Some("APPROVAL_SIG".into()),
                controller_agent: Some("gatekeeper".into()),
            }
        );
        assert_eq!(
            approval_action_args(
                &format!("{explicit_run} approval-2 --controller-agent=gatekeeper"),
                Some(last_run),
                "assess",
                true,
                false,
            )
            .unwrap(),
            ApprovalActionArgs {
                run_id: explicit_run.into(),
                approval_id: "approval-2".into(),
                unlock_env: None,
                signature_env: None,
                controller_agent: Some("gatekeeper".into()),
            }
        );
        assert!(approval_list_args("", None).is_err());
        assert!(approval_action_args("", Some(last_run), "approve", true, true).is_err());
        assert!(
            approval_action_args(
                "approval-1 --unlock-env",
                Some(last_run),
                "approve",
                true,
                true
            )
            .is_err()
        );
        assert!(
            approval_action_args(
                "approval-1 --controller-agent gatekeeper",
                Some(last_run),
                "execute",
                false,
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn agent_switch_arg_requires_exactly_one_id() {
        assert_eq!(agent_switch_arg("research").unwrap(), "research");
        assert!(agent_switch_arg("").is_err());
        assert!(agent_switch_arg("research extra").is_err());
    }

    #[test]
    fn hook_policy_change_args_require_explicit_confirm_flag() {
        assert_eq!(
            parse_hook_policy_change_args("adapter:pkg:audit --confirm").unwrap(),
            ("adapter:pkg:audit".into(), true, false)
        );
        assert_eq!(
            parse_hook_policy_change_args("adapter:pkg:audit --agent --confirm").unwrap(),
            ("adapter:pkg:audit".into(), true, true)
        );
        assert_eq!(
            parse_hook_policy_change_args("adapter:pkg:audit").unwrap(),
            ("adapter:pkg:audit".into(), false, false)
        );
        assert!(parse_hook_policy_change_args("").is_err());
        assert!(parse_hook_policy_change_args("adapter:pkg:audit --force").is_err());
    }

    #[test]
    fn adapter_export_args_require_id_and_path() {
        assert_eq!(
            adapter_export_args("adapter-1 ./adapter.json").unwrap(),
            ("adapter-1", "./adapter.json")
        );
        assert_eq!(
            clawhub_search_args("./clawhub.json qwen tools").unwrap(),
            ("./clawhub.json", Some("qwen tools"))
        );
        assert_eq!(
            clawhub_search_args("./clawhub.json").unwrap(),
            ("./clawhub.json", None)
        );
        assert_eq!(
            clawhub_entry_args("./clawhub.json adapter-1", "install").unwrap(),
            ("./clawhub.json", "adapter-1")
        );
        assert!(adapter_export_args("adapter-1").is_err());
        assert!(adapter_export_args("adapter-1 ./adapter.json extra").is_err());
        assert!(clawhub_search_args("").is_err());
        assert!(clawhub_entry_args("./clawhub.json", "inspect").is_err());
        assert!(clawhub_entry_args("./clawhub.json adapter-1 extra", "pin").is_err());
    }

    #[test]
    fn model_args_require_expected_id_and_path() {
        let model = model_save_args(
            r#"gpt-test {"provider":"openai-compatible","available_modalities":["text"]}"#,
        )
        .unwrap();
        assert_eq!(model.id, "gpt-test");
        assert_eq!(model.provider.as_deref(), Some("openai-compatible"));
        assert_eq!(model.available_modalities, vec!["text"]);
        assert_eq!(model_save_args("gpt-test").unwrap().id, "gpt-test");
        assert_eq!(first_model_arg("gpt-test", "probe").unwrap(), "gpt-test");
        assert!(first_model_arg("", "probe").is_err());
        assert!(model_save_args("").is_err());
        assert!(model_save_args("gpt-test []").is_err());
        assert!(model_save_args("gpt-test {").is_err());
        assert_eq!(
            model_export_args("gpt-test ./model.toml").unwrap(),
            ("gpt-test", "./model.toml")
        );
        assert_eq!(
            model_import_args("./model.toml").unwrap(),
            ("./model.toml", false)
        );
        assert_eq!(
            model_import_args("./model.toml --confirm").unwrap(),
            ("./model.toml", true)
        );
        assert!(model_export_args("gpt-test").is_err());
        assert!(model_export_args("gpt-test ./model.toml extra").is_err());
        assert_eq!(
            model_delete_args("gpt-test --confirm").unwrap(),
            ("gpt-test", true)
        );
        assert_eq!(model_delete_args("gpt-test").unwrap(), ("gpt-test", false));
        assert!(model_delete_args("").is_err());
        assert!(model_delete_args("gpt-test extra").is_err());
        assert!(model_import_args("").is_err());
        assert!(model_import_args("./model.toml extra").is_err());
    }

    #[test]
    fn model_provider_catalog_path_arg_requires_exactly_one_path() {
        assert_eq!(
            model_provider_catalog_path_arg("./catalog.json", "export").unwrap(),
            "./catalog.json"
        );
        assert_eq!(
            model_provider_catalog_import_args("./catalog.json --confirm").unwrap(),
            ("./catalog.json", true)
        );
        assert_eq!(
            model_provider_catalog_import_args("./catalog.json").unwrap(),
            ("./catalog.json", false)
        );
        assert!(model_provider_catalog_path_arg("", "export").is_err());
        assert!(model_provider_catalog_path_arg("./catalog.json extra", "import").is_err());
        assert!(model_provider_catalog_import_args("").is_err());
        assert!(model_provider_catalog_import_args("./catalog.json extra").is_err());
        assert_eq!(
            model_metadata_catalog_path_arg("./metadata.json", "export").unwrap(),
            "./metadata.json"
        );
        assert_eq!(
            model_metadata_catalog_import_args("./metadata.json --confirm").unwrap(),
            ("./metadata.json", true)
        );
        assert_eq!(
            model_metadata_catalog_import_args("./metadata.json").unwrap(),
            ("./metadata.json", false)
        );
        assert!(model_metadata_catalog_path_arg("", "export").is_err());
        assert!(model_metadata_catalog_path_arg("./metadata.json extra", "import").is_err());
        assert!(model_metadata_catalog_import_args("").is_err());
        assert!(model_metadata_catalog_import_args("./metadata.json extra").is_err());
    }

    #[test]
    fn agent_args_require_expected_id_path_and_confirmation() {
        let saved = agent_save_args("research You are a careful researcher.").unwrap();
        assert_eq!(saved.id, "research");
        assert_eq!(saved.system_prompt, "You are a careful researcher.");
        assert!(saved.prompt_refinements.is_empty());
        assert!(saved.disabled_lifecycle_hooks.is_empty());
        let saved = agent_save_args(
            r#"--disable-lifecycle-hook adapter:pkg:audit --refinement-rules-json [{"id":"support","when":"support request","instructions":"Ask first.","agent_awareness":true}] research You are a careful researcher."#,
        )
        .unwrap();
        assert_eq!(saved.id, "research");
        assert_eq!(saved.system_prompt, "You are a careful researcher.");
        assert_eq!(saved.prompt_refinements.len(), 1);
        assert_eq!(saved.prompt_refinements[0].id.as_deref(), Some("support"));
        assert!(saved.prompt_refinements[0].agent_awareness);
        assert_eq!(
            saved.disabled_lifecycle_hooks,
            vec!["adapter:pkg:audit".to_string()]
        );
        assert!(agent_save_args("").is_err());
        assert!(agent_save_args("research").is_err());
        assert_eq!(
            agent_export_args("research ./agent.toml").unwrap(),
            ("research", "./agent.toml")
        );
        assert!(agent_export_args("research").is_err());
        assert!(agent_export_args("research ./agent.toml extra").is_err());
        assert_eq!(
            agent_import_args("./agent.toml").unwrap(),
            ("./agent.toml", false)
        );
        assert_eq!(
            agent_import_args("./agent.toml --confirm").unwrap(),
            ("./agent.toml", true)
        );
        assert!(agent_import_args("").is_err());
        assert!(agent_import_args("./agent.toml extra").is_err());
        assert_eq!(agent_delete_args("research").unwrap(), ("research", false));
        assert_eq!(
            agent_delete_args("research --confirm").unwrap(),
            ("research", true)
        );
        assert!(agent_delete_args("").is_err());
        assert!(agent_delete_args("research critic --confirm").is_err());
    }

    #[test]
    fn profile_args_parse_create_grant_filters_and_confirmations() {
        assert_eq!(profile_create_args("research").unwrap(), ("research", None));
        assert_eq!(
            profile_create_args("research --name Research Team").unwrap(),
            ("research", Some("Research Team".to_string()))
        );
        assert!(profile_create_args("").is_err());
        assert!(profile_create_args("research --title nope").is_err());

        let grant =
            profile_grant_args("--from main --to research --kind memory agent:fake-agent").unwrap();
        assert_eq!(grant.from.as_deref(), Some("main"));
        assert_eq!(grant.to, "research");
        assert_eq!(grant.kind, ProfileGrantKind::Memory);
        assert_eq!(grant.resource, "agent:fake-agent");

        let grant = profile_grant_args("--to=research --kind=category review").unwrap();
        assert_eq!(grant.from, None);
        assert_eq!(grant.to, "research");
        assert_eq!(grant.kind, ProfileGrantKind::Category);
        assert_eq!(grant.resource, "review");
        assert!(profile_grant_args("--to research --kind memory").is_err());
        assert!(profile_grant_args("--to research --kind unknown resource").is_err());
        assert!(profile_grant_args("--to research --kind memory one two").is_err());

        assert_eq!(
            profile_grants_args("--from main").unwrap().as_deref(),
            Some("main")
        );
        assert_eq!(profile_grants_args("").unwrap(), None);
        assert!(profile_grants_args("main").is_err());

        assert_eq!(
            profile_confirm_id_args("research --confirm", "delete").unwrap(),
            ("research", true)
        );
        assert_eq!(
            profile_confirm_id_args("grant-1", "revoke-grant").unwrap(),
            ("grant-1", false)
        );
        assert!(profile_confirm_id_args("", "delete").is_err());
        assert!(profile_confirm_id_args("one two --confirm", "delete").is_err());
    }

    #[test]
    fn secret_args_parse_values_labels_and_confirmation() {
        assert_eq!(
            secret_set_args("api-key --label openai sk-test value").unwrap(),
            (
                "api-key",
                Some("openai".to_string()),
                "sk-test value".to_string()
            )
        );
        assert_eq!(
            secret_set_args("api-key --label=openai sk-test").unwrap(),
            ("api-key", Some("openai".to_string()), "sk-test".to_string())
        );
        assert_eq!(
            secret_set_args("api-key sk-test value").unwrap(),
            ("api-key", None, "sk-test value".to_string())
        );
        assert!(secret_set_args("").is_err());
        assert!(secret_set_args("api-key --label").is_err());
        assert!(secret_set_args("api-key").is_err());

        assert_eq!(
            secret_rotate_args("api-key sk-next value").unwrap(),
            ("api-key", "sk-next value".to_string())
        );
        assert!(secret_rotate_args("api-key").is_err());

        assert_eq!(
            secret_confirm_id_args("api-key --confirm", "delete").unwrap(),
            ("api-key", true)
        );
        assert_eq!(
            secret_confirm_id_args("api-key", "delete").unwrap(),
            ("api-key", false)
        );
        assert!(secret_confirm_id_args("", "delete").is_err());
        assert!(secret_confirm_id_args("api-key other --confirm", "delete").is_err());
    }

    #[test]
    fn prompt_args_accept_agent_scope_text_and_confirmation() {
        assert_eq!(prompt_agent_arg("", "list").unwrap(), None);
        assert_eq!(
            prompt_agent_arg("--agent research", "list")
                .unwrap()
                .as_deref(),
            Some("research")
        );
        assert!(prompt_agent_arg("extra", "list").is_err());

        assert_eq!(
            prompt_named_args("daily --agent=research", "show").unwrap(),
            ("daily", Some("research".to_string()))
        );
        assert!(prompt_named_args("", "show").is_err());

        let (name, agent, text) =
            prompt_save_args("daily --agent research summarize the latest notes").unwrap();
        assert_eq!(name, "daily");
        assert_eq!(agent.as_deref(), Some("research"));
        assert_eq!(text, "summarize the latest notes");
        assert!(prompt_save_args("daily --agent").is_err());
        assert!(prompt_save_args("daily").is_err());

        assert_eq!(
            prompt_delete_args("daily --agent research --confirm").unwrap(),
            ("daily", Some("research".to_string()), true)
        );
        assert_eq!(prompt_delete_args("daily").unwrap(), ("daily", None, false));
        assert!(prompt_delete_args("").is_err());
        assert!(prompt_delete_args("daily extra").is_err());
    }

    #[test]
    fn skill_args_require_expected_id_path_and_confirmation() {
        assert_eq!(
            skill_path_arg("./SKILL.md", "import-openclaw").unwrap(),
            "./SKILL.md"
        );
        assert!(skill_path_arg("", "import-doc").is_err());
        assert!(skill_path_arg("./skill.json extra", "import-doc").is_err());
        assert_eq!(
            skill_export_args("skill-1 ./skill.json").unwrap(),
            ("skill-1", "./skill.json")
        );
        assert!(skill_export_args("skill-1").is_err());
        assert!(skill_export_args("skill-1 ./skill.json extra").is_err());
        assert_eq!(
            skill_review_args("skill-1", "allow").unwrap(),
            ("skill-1", false)
        );
        assert_eq!(
            skill_review_args("skill-1 --confirm", "allow").unwrap(),
            ("skill-1", true)
        );
        assert!(skill_review_args("", "allow").is_err());
        assert!(skill_review_args("skill-1 skill-2 --confirm", "allow").is_err());
    }

    #[test]
    fn storage_prune_cache_args_accept_days_and_apply_flag() {
        assert_eq!(storage_prune_cache_args("30").unwrap(), (30, false));
        assert_eq!(storage_prune_cache_args("14 --apply").unwrap(), (14, true));
        assert_eq!(storage_prune_cache_args("--apply 7").unwrap(), (7, true));
        assert!(storage_prune_cache_args("").is_err());
        assert!(storage_prune_cache_args("abc").is_err());
        assert!(storage_prune_cache_args("7 8").is_err());
    }

    #[test]
    fn storage_report_event_surfaces_quota_status() {
        let mut report = StorageReport {
            root: std::path::PathBuf::from("/tmp/harness"),
            total_bytes: 120,
            total_files: 3,
            total_directories: 1,
            largest_file: None,
            largest_file_bytes: 0,
            quota_bytes: None,
            quota_remaining_bytes: None,
            quota_exceeded: false,
            buckets: Vec::new(),
        };

        assert_eq!(
            storage_report_event_text(&report),
            "Storage: 120 bytes across 3 file(s)."
        );

        report.quota_bytes = Some(100);
        report.quota_remaining_bytes = Some(-20);
        report.quota_exceeded = true;

        assert_eq!(
            storage_report_event_text(&report),
            "Storage: 120 bytes across 3 file(s). Quota: 100 bytes, remaining -20 bytes (over quota)."
        );
    }

    #[test]
    fn shell_slash_toggles_runtime_shell_access() {
        let mut app = App::default();
        let mut options = setup::RuntimeOptions::default();
        let mut registry = setup::build_registry(
            options.enable_shell,
            options.enable_subagent,
            options.enable_capability_drafts,
            options.agent_id.as_deref(),
            options.conversation_id.as_deref(),
        );

        handle_shell_slash(&mut app, "status", &mut registry, &mut options);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Shell tool access is disabled."))
        );

        handle_shell_slash(&mut app, "on", &mut registry, &mut options);
        assert!(options.enable_shell);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Shell tool access enabled."))
        );

        handle_shell_slash(&mut app, "off", &mut registry, &mut options);
        assert!(!options.enable_shell);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Shell tool access disabled."))
        );
    }

    #[test]
    fn subagent_slash_toggles_runtime_subagent_access() {
        let mut app = App::default();
        let mut options = setup::RuntimeOptions::default();
        let mut registry = setup::build_registry(
            options.enable_shell,
            options.enable_subagent,
            options.enable_capability_drafts,
            options.agent_id.as_deref(),
            options.conversation_id.as_deref(),
        );

        handle_subagent_slash(&mut app, "status", &mut registry, &mut options);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Subagent tool access is disabled."))
        );

        handle_subagent_slash(&mut app, "on", &mut registry, &mut options);
        assert!(options.enable_subagent);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Subagent tool access enabled."))
        );

        handle_subagent_slash(&mut app, "off", &mut registry, &mut options);
        assert!(!options.enable_subagent);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Subagent tool access disabled."))
        );
    }

    #[test]
    fn budget_slash_updates_runtime_tool_budget() {
        let _home = HarnessHomeGuard::new();
        let mut app = App::default();
        let mut options = setup::RuntimeOptions::default();
        let mut agent = setup::build_agent(&options);

        handle_budget_slash(&mut app, "3", &mut agent, &mut options);
        assert_eq!(options.max_tool_calls, Some(3));
        assert_eq!(agent.tool_policy.max_calls, 3);
        assert_eq!(app.calls_max, 3);
        assert_eq!(app.calls_remaining, 3);

        handle_budget_slash(&mut app, "0", &mut agent, &mut options);
        assert_eq!(options.max_tool_calls, Some(0));
        assert_eq!(agent.tool_policy.max_calls, 0);
        assert_eq!(app.calls_max, 0);

        handle_budget_slash(&mut app, "-1", &mut agent, &mut options);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("non-negative integer"))
        );
        assert_eq!(options.max_tool_calls, Some(0));
    }

    #[test]
    fn visibility_slash_updates_runtime_tool_visibility() {
        let _home = HarnessHomeGuard::new();
        let mut app = App::default();
        let mut options = setup::RuntimeOptions::default();
        let mut agent = setup::build_agent(&options);

        handle_visibility_slash(&mut app, "names", &mut agent, &mut options);
        assert_eq!(options.tool_visibility, Some(VisibilityLevel::NameOnly));
        assert_eq!(agent.tool_policy.visibility, VisibilityLevel::NameOnly);

        handle_visibility_slash(&mut app, "descriptions", &mut agent, &mut options);
        assert_eq!(
            options.tool_visibility,
            Some(VisibilityLevel::NameAndDescription)
        );
        assert_eq!(
            agent.tool_policy.visibility,
            VisibilityLevel::NameAndDescription
        );

        handle_visibility_slash(&mut app, "full", &mut agent, &mut options);
        assert_eq!(options.tool_visibility, Some(VisibilityLevel::FullSchema));
        assert_eq!(agent.tool_policy.visibility, VisibilityLevel::FullSchema);

        handle_visibility_slash(&mut app, "config", &mut agent, &mut options);
        assert_eq!(options.tool_visibility, None);
        assert_eq!(agent.tool_policy.visibility, VisibilityLevel::FullSchema);

        handle_visibility_slash(&mut app, "mystery", &mut agent, &mut options);
        assert!(app.transcript.iter().any(|line| {
            line.text
                .contains("Visibility command needs full, descriptions, names, or config.")
        }));
        assert_eq!(options.tool_visibility, None);
    }

    #[test]
    fn memory_and_skill_slash_toggle_context_loading() {
        let _home = HarnessHomeGuard::new();
        let mut app = App::default();
        let mut options = setup::RuntimeOptions::default();
        let mut agent = setup::build_agent(&options);
        let registry = setup::build_registry(
            options.enable_shell,
            options.enable_subagent,
            options.enable_capability_drafts,
            options.agent_id.as_deref(),
            options.conversation_id.as_deref(),
        );
        let (line_tx, _line_rx) = tokio::sync::mpsc::unbounded_channel();

        handle_memory_slash(
            &mut app,
            "status",
            &registry,
            &mut agent,
            &line_tx,
            &mut options,
        );
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Runtime memory loading is disabled."))
        );

        handle_memory_slash(
            &mut app,
            "on",
            &registry,
            &mut agent,
            &line_tx,
            &mut options,
        );
        assert!(options.load_memory);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Runtime memory loading enabled"))
        );

        handle_memory_slash(
            &mut app,
            "off",
            &registry,
            &mut agent,
            &line_tx,
            &mut options,
        );
        assert!(!options.load_memory);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Runtime memory loading disabled"))
        );

        handle_skills_slash(&mut app, "status", &registry, &mut agent, &mut options);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Runtime skill loading is disabled."))
        );

        handle_skills_slash(&mut app, "on", &registry, &mut agent, &mut options);
        assert!(options.load_skills);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Runtime skill loading enabled"))
        );

        handle_skills_slash(&mut app, "off", &registry, &mut agent, &mut options);
        assert!(!options.load_skills);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Runtime skill loading disabled"))
        );
    }

    #[test]
    fn memory_and_skill_slash_preview_context_loading() {
        let _home = HarnessHomeGuard::new();
        let mut app = App::default();
        let mut options = setup::RuntimeOptions::default();
        let mut agent = setup::build_agent(&options);
        let registry = setup::build_registry(
            options.enable_shell,
            options.enable_subagent,
            options.enable_capability_drafts,
            options.agent_id.as_deref(),
            options.conversation_id.as_deref(),
        );
        let (line_tx, _line_rx) = tokio::sync::mpsc::unbounded_channel();

        handle_memory_slash(
            &mut app,
            "preview memory test",
            &registry,
            &mut agent,
            &line_tx,
            &mut options,
        );
        assert!(options.load_memory);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Preview:"))
        );
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("memory test"))
        );

        handle_skills_slash(
            &mut app,
            "preview skills test",
            &registry,
            &mut agent,
            &mut options,
        );
        assert!(options.load_skills);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("skills test"))
        );
    }

    #[test]
    fn cost_slash_updates_runtime_cost_overrides() {
        let _home = HarnessHomeGuard::new();
        let mut app = App::default();
        let mut options = setup::RuntimeOptions::default();
        let mut agent = setup::build_agent(&options);

        handle_cost_slash(&mut app, "status", &mut agent, &mut options);
        assert!(
            app.transcript
                .iter()
                .any(|line| line.text.contains("Token cost overrides: input config"))
        );

        handle_cost_slash(&mut app, "input 0.15", &mut agent, &mut options);
        assert_eq!(options.input_cost_per_million, Some(0.15));
        assert_eq!(agent.cost_policy.input_cost_per_million, Some(0.15));

        handle_cost_slash(&mut app, "output 0.6", &mut agent, &mut options);
        assert_eq!(options.output_cost_per_million, Some(0.6));
        assert_eq!(agent.cost_policy.output_cost_per_million, Some(0.6));

        handle_cost_slash(&mut app, "both 0.1 0.2", &mut agent, &mut options);
        assert_eq!(options.input_cost_per_million, Some(0.1));
        assert_eq!(options.output_cost_per_million, Some(0.2));
        assert_eq!(agent.cost_policy.input_cost_per_million, Some(0.1));
        assert_eq!(agent.cost_policy.output_cost_per_million, Some(0.2));

        handle_cost_slash(&mut app, "input -1", &mut agent, &mut options);
        assert!(app.transcript.iter().any(|line| {
            line.text
                .contains("cost rate must be a non-negative number")
        }));
        assert_eq!(options.input_cost_per_million, Some(0.1));

        handle_cost_slash(&mut app, "clear", &mut agent, &mut options);
        assert_eq!(options.input_cost_per_million, None);
        assert_eq!(options.output_cost_per_million, None);
        assert_eq!(agent.cost_policy.input_cost_per_million, None);
        assert_eq!(agent.cost_policy.output_cost_per_million, None);
    }

    #[test]
    fn bundle_args_require_paths_and_import_confirmation() {
        assert_eq!(
            bundle_path_arg("./bundle.tar", "export").unwrap(),
            "./bundle.tar"
        );
        assert_eq!(
            bundle_path_arg("./bundle.tar", "backup").unwrap(),
            "./bundle.tar"
        );
        assert!(bundle_path_arg("", "export").is_err());
        assert!(bundle_path_arg("./bundle.tar extra", "export").is_err());
        assert_eq!(
            bundle_import_args("./bundle.tar").unwrap(),
            ("./bundle.tar", false)
        );
        assert_eq!(
            bundle_import_args("./bundle.tar --confirm").unwrap(),
            ("./bundle.tar", true)
        );
        assert!(bundle_import_args("").is_err());
        assert!(bundle_import_args("./bundle.tar extra --confirm").is_err());
    }

    #[test]
    fn compact_args_require_expected_id_and_path() {
        assert_eq!(
            compact_export_args("compact-1 ./compact.json").unwrap(),
            ("compact-1", "./compact.json")
        );
        assert_eq!(
            compact_path_arg("./compact.json", "import").unwrap(),
            "./compact.json"
        );
        assert!(compact_export_args("compact-1").is_err());
        assert!(compact_export_args("compact-1 ./compact.json extra").is_err());
        assert!(compact_path_arg("", "import").is_err());
        assert!(compact_path_arg("./compact.json extra", "import").is_err());
        assert_eq!(
            compact_delete_args("compact-1 --confirm").unwrap(),
            ("compact-1", true)
        );
        assert_eq!(
            compact_delete_args("compact-1").unwrap(),
            ("compact-1", false)
        );
        assert!(compact_delete_args("").is_err());
        assert!(compact_delete_args("compact-1 compact-2").is_err());
    }

    #[test]
    fn memory_path_args_accept_optional_user_flag() {
        assert_eq!(
            memory_path_args("./memory.md", "export").unwrap(),
            ("./memory.md", false, None)
        );
        assert_eq!(
            memory_path_args("./user.md --user --agent critic", "import").unwrap(),
            ("./user.md", true, Some("critic".into()))
        );
        assert_eq!(
            memory_path_args("--user ./user.md", "import").unwrap(),
            ("./user.md", true, None)
        );
        assert!(memory_path_args("", "export").is_err());
        assert!(memory_path_args("./memory.md extra", "export").is_err());
    }

    #[test]
    fn memory_access_args_parse_topic_and_agent_filters() {
        assert_eq!(
            memory_access_args("--topic finance --topic=ops --agent critic --agent=writer")
                .unwrap(),
            (
                vec!["finance".to_string(), "ops".to_string()],
                vec!["critic".to_string(), "writer".to_string()]
            )
        );
        assert!(memory_access_args("--topic").is_err());
        assert!(memory_access_args("--agent").is_err());
        assert!(memory_access_args("extra").is_err());
    }

    #[test]
    fn memory_probe_args_accept_backend_and_topics() {
        let (backend, topics) =
            memory_probe_args("external-command-v0 --topic team --topic=launch").unwrap();
        assert_eq!(backend.as_deref(), Some("external-command-v0"));
        assert_eq!(topics, vec!["team".to_string(), "launch".to_string()]);

        let (backend, topics) = memory_probe_args("--topic ops").unwrap();
        assert!(backend.is_none());
        assert_eq!(topics, vec!["ops".to_string()]);
        assert!(memory_probe_args("one two").is_err());
        assert!(memory_probe_args("--topic").is_err());
    }

    #[test]
    fn memory_write_args_parse_flags_before_text() {
        let args = parse_memory_write_args(
            "--user --agent research --conversation conv-1 --topic rust remember this fact",
            "create",
            false,
        )
        .unwrap();
        assert!(args.user);
        assert_eq!(args.agent.as_deref(), Some("research"));
        assert_eq!(args.conversation.as_deref(), Some("conv-1"));
        assert_eq!(args.topics, vec!["rust".to_string()]);
        assert_eq!(args.text, "remember this fact");
        assert!(args.range.is_none());

        let args = parse_memory_write_args(
            "--range=messages:1..3 --topic ops generated text",
            "generate",
            true,
        )
        .unwrap();
        assert_eq!(args.range.as_deref(), Some("messages:1..3"));
        assert_eq!(args.topics, vec!["ops".to_string()]);
        assert_eq!(args.text, "generated text");
        assert!(args.guidance.is_none());

        let args = parse_memory_write_args(
            "--topic ops generated text --guidance keep stable preferences",
            "generate",
            true,
        )
        .unwrap();
        assert_eq!(args.text, "generated text");
        assert_eq!(args.guidance.as_deref(), Some("keep stable preferences"));

        assert!(parse_memory_write_args("--range messages:1..3 text", "create", false).is_err());
        assert!(parse_memory_write_args("--topic", "create", false).is_err());
        assert!(parse_memory_write_args("", "create", false).is_err());
    }

    #[test]
    fn memory_edit_delete_and_rollback_args_require_expected_confirmation() {
        assert_eq!(
            memory_edit_args("mem-1 updated content").unwrap(),
            ("mem-1", "updated content")
        );
        assert!(memory_edit_args("mem-1").is_err());

        assert_eq!(
            memory_confirm_id_args("mem-1 --confirm", "delete").unwrap(),
            "mem-1"
        );
        assert_eq!(
            memory_confirm_id_args("--confirm mem-1", "delete").unwrap(),
            "mem-1"
        );
        assert!(memory_confirm_id_args("mem-1", "delete").is_err());
        assert!(memory_confirm_id_args("mem-1 mem-2 --confirm", "delete").is_err());

        assert!(!memory_rollback_args("--confirm").unwrap());
        assert!(memory_rollback_args("--user --confirm").unwrap());
        assert!(memory_rollback_args("--user").is_err());
        assert!(memory_rollback_args("--confirm extra").is_err());
    }

    #[test]
    fn memory_classify_args_accept_model_agent_and_apply_flags() {
        let args = parse_memory_classify_args(
            "mem-1 --model memory-classifier --agent research --no-apply",
        )
        .unwrap();
        assert_eq!(args.id, "mem-1");
        assert_eq!(args.model.as_deref(), Some("memory-classifier"));
        assert_eq!(args.agent.as_deref(), Some("research"));
        assert!(!args.apply);

        let args = parse_memory_classify_args("mem-2 --model=classifier --agent=writer").unwrap();
        assert_eq!(args.id, "mem-2");
        assert_eq!(args.model.as_deref(), Some("classifier"));
        assert_eq!(args.agent.as_deref(), Some("writer"));
        assert!(args.apply);

        assert!(parse_memory_classify_args("").is_err());
        assert!(parse_memory_classify_args("mem-1 --model").is_err());
        assert!(parse_memory_classify_args("mem-1 extra").is_err());
    }

    #[test]
    fn capability_args_require_expected_id_and_path() {
        assert_eq!(
            capability_propose_args("tool draft-name echo hello world").unwrap(),
            ("tool", "draft-name", "echo hello world", None)
        );
        assert_eq!(
            capability_propose_args("skill draft-name echo hello --guidance promote carefully")
                .unwrap(),
            (
                "skill",
                "draft-name",
                "echo hello",
                Some("promote carefully")
            )
        );
        assert_eq!(
            capability_propose_args("agent draft-name echo hello --guidance=promote carefully")
                .unwrap(),
            (
                "agent",
                "draft-name",
                "echo hello",
                Some("promote carefully")
            )
        );
        assert_eq!(
            capability_export_args("draft-1 ./draft.json").unwrap(),
            ("draft-1", "./draft.json")
        );
        assert_eq!(
            capability_review_args("draft-1 --confirm", "allow").unwrap(),
            ("draft-1", true)
        );
        assert_eq!(
            capability_review_args("draft-1", "reject").unwrap(),
            ("draft-1", false)
        );
        assert_eq!(
            capability_review_args("draft-1 --confirm", "delete").unwrap(),
            ("draft-1", true)
        );
        assert_eq!(
            capability_path_arg("./draft.json", "import").unwrap(),
            "./draft.json"
        );
        assert!(capability_propose_args("").is_err());
        assert!(capability_propose_args("tool").is_err());
        assert!(capability_propose_args("tool draft-name").is_err());
        assert!(capability_propose_args("tool draft-name --guidance promote carefully").is_err());
        assert!(capability_propose_args("tool draft-name echo --guidance=").is_err());
        assert!(capability_export_args("draft-1").is_err());
        assert!(capability_export_args("draft-1 ./draft.json extra").is_err());
        assert!(capability_review_args("", "allow").is_err());
        assert!(capability_review_args("draft-1 extra", "reject").is_err());
        assert!(capability_path_arg("", "import").is_err());
        assert!(capability_path_arg("./draft.json extra", "import").is_err());
    }

    #[test]
    fn capability_propose_draft_stores_guidance() {
        let _home = HarnessHomeGuard::new();
        let draft = propose_capability_draft(
            "skill guided-draft Create a reusable reviewer skill --guidance Review before allowing",
        )
        .unwrap();

        assert_eq!(draft.guidance.as_deref(), Some("Review before allowing"));
        let stored = CapabilityDraftStore::from_env().show(&draft.id).unwrap();
        assert_eq!(stored.guidance.as_deref(), Some("Review before allowing"));
        let summary = capability_draft_summary(&stored);
        assert_eq!(
            summary["guidance_preview"].as_str(),
            Some("Review before allowing")
        );
    }

    #[test]
    fn artifact_delete_args_require_id_and_confirm_flag() {
        assert_eq!(
            artifact_generate_args("pdf Hello report").unwrap(),
            ("pdf", "Hello report")
        );
        assert!(artifact_generate_args("").is_err());
        assert!(artifact_generate_args("pdf").is_err());
        assert_eq!(
            artifact_export_args("artifact-1 /tmp/artifact.txt").unwrap(),
            ("artifact-1", "/tmp/artifact.txt")
        );
        assert!(artifact_export_args("").is_err());
        assert!(artifact_export_args("artifact-1").is_err());
        assert!(artifact_export_args("artifact-1 /tmp/a extra").is_err());
        assert_eq!(
            artifact_download_args("artifact-1").unwrap(),
            ("artifact-1", ".")
        );
        assert_eq!(
            artifact_download_args("artifact-1 /tmp/artifact.txt").unwrap(),
            ("artifact-1", "/tmp/artifact.txt")
        );
        assert!(artifact_download_args("").is_err());
        assert!(artifact_download_args("artifact-1 /tmp/a extra").is_err());
        assert_eq!(
            artifact_delete_args("artifact-1 --confirm").unwrap(),
            ("artifact-1", true)
        );
        assert_eq!(
            artifact_delete_args("artifact-1").unwrap(),
            ("artifact-1", false)
        );
        assert!(artifact_delete_args("").is_err());
        assert!(artifact_delete_args("artifact-1 extra").is_err());
    }

    #[test]
    fn ingest_args_parse_review_and_confirmed_delete() {
        assert_eq!(
            ingest_delete_args("ingest-1 --confirm").unwrap(),
            ("ingest-1", true)
        );
        assert_eq!(ingest_delete_args("ingest-1").unwrap(), ("ingest-1", false));
        assert_eq!(
            ingest_probe_vision_args("doc.pdf --model gpt-4.1").unwrap(),
            ("doc.pdf", "gpt-4.1")
        );
        assert_eq!(
            ingest_probe_vision_args("chart.png --model=gemini-2.5-pro").unwrap(),
            ("chart.png", "gemini-2.5-pro")
        );
        assert_eq!(
            ingest_probe_source_args("scan.pdf --vision-model gpt-4o").unwrap(),
            ("scan.pdf", Some("gpt-4o"))
        );
        assert_eq!(
            ingest_probe_source_args("chart.png --model=vision").unwrap(),
            ("chart.png", Some("vision"))
        );
        assert_eq!(
            ingest_probe_source_args("notes.md").unwrap(),
            ("notes.md", None)
        );
        assert_eq!(
            ingest_run_args(
                "doc.md --backend local-lines-v0 --vision-model gpt-4o --guardrail-model gpt-4o-mini",
                "add"
            )
            .unwrap(),
            (
                "doc.md",
                "local-lines-v0".to_string(),
                Some("gpt-4o".to_string()),
                Some("gpt-4o-mini".to_string())
            )
        );
        assert_eq!(
            ingest_run_args("ingest-1 --backend=local-layout-v0", "rerun").unwrap(),
            ("ingest-1", "local-layout-v0".to_string(), None, None)
        );
        let (id, finding, decision, note) =
            ingest_review_args("ingest-1 2 approve reviewed by user").unwrap();
        assert_eq!(id, "ingest-1");
        assert_eq!(finding, 2);
        assert_eq!(decision, IngestionFindingReviewDecision::Approve);
        assert_eq!(note.as_deref(), Some("reviewed by user"));
        assert!(ingest_delete_args("").is_err());
        assert!(ingest_delete_args("ingest-1 extra").is_err());
        assert!(ingest_probe_vision_args("doc.pdf").is_err());
        assert!(ingest_probe_vision_args("doc.pdf --model").is_err());
        assert!(ingest_probe_vision_args("doc.pdf --model gpt-4.1 extra").is_err());
        assert!(ingest_probe_source_args("").is_err());
        assert!(ingest_probe_source_args("doc.pdf --vision-model").is_err());
        assert!(ingest_probe_source_args("doc.pdf extra").is_err());
        assert!(ingest_run_args("", "add").is_err());
        assert!(ingest_run_args("doc.md --backend", "add").is_err());
        assert!(ingest_run_args("doc.md extra", "add").is_err());
        assert!(ingest_review_args("ingest-1 0 maybe").is_err());
    }

    #[test]
    fn conversation_range_args_accept_split_and_colon_forms() {
        assert_eq!(
            parse_conversation_range_args("conv-1 2 4").unwrap(),
            ("conv-1".into(), 2, 4)
        );
        assert_eq!(
            parse_conversation_range_args("conv-1 2:4").unwrap(),
            ("conv-1".into(), 2, 4)
        );
        assert!(parse_conversation_range_args("conv-1 4 2").is_err());
        assert!(parse_conversation_range_args("conv-1 2").is_err());
        assert!(parse_conversation_range_args("conv-1 2:4 5").is_err());
    }

    #[test]
    fn conversation_range_args_use_selected_conversation_when_id_omitted() {
        assert_eq!(
            parse_conversation_range_args_with_selected("2 4", Some("conv-1")).unwrap(),
            ("conv-1".into(), 2, 4)
        );
        assert_eq!(
            parse_conversation_range_args_with_selected("2:4", Some("conv-1")).unwrap(),
            ("conv-1".into(), 2, 4)
        );
        assert!(parse_conversation_range_args_with_selected("2:4", None).is_err());
    }

    #[test]
    fn conversation_range_delete_args_accept_preservation_options() {
        let (id, from, to, options) = parse_conversation_range_delete_args_with_selected(
            "2:4 --compact-first --compact-guidance keep --compact-max-output-tokens=128 --memory-first --memory-guidance stable --memory-user",
            Some("conv-1"),
        )
        .unwrap();
        assert_eq!(id, "conv-1");
        assert_eq!(from, 2);
        assert_eq!(to, 4);
        assert!(options.compact_first);
        assert_eq!(options.compact_guidance.as_deref(), Some("keep"));
        assert_eq!(options.compact_max_output_tokens, 128);
        assert!(options.memory_first);
        assert_eq!(options.memory_guidance.as_deref(), Some("stable"));
        assert!(options.memory_user);
    }

    #[test]
    fn conversation_usage_args_accept_full_range_and_last_forms() {
        assert_eq!(
            parse_conversation_usage_args_with_selected("", Some("conv-1")).unwrap(),
            ("conv-1".into(), None, None, None)
        );
        assert_eq!(
            parse_conversation_usage_args_with_selected("conv-2 2:4", Some("conv-1")).unwrap(),
            ("conv-2".into(), Some(2), Some(4), None)
        );
        assert_eq!(
            parse_conversation_usage_args_with_selected("last 3", Some("conv-1")).unwrap(),
            ("conv-1".into(), None, None, Some(3))
        );
        assert!(parse_conversation_usage_args_with_selected("", None).is_err());
        assert!(parse_conversation_usage_args_with_selected("last 0", Some("conv-1")).is_err());
    }

    #[test]
    fn conversation_memory_args_accept_selected_range_and_topics() {
        assert_eq!(
            parse_conversation_memory_args_with_selected(
                "2:4 --user --topic finance --topic=ops",
                Some("conv-1")
            )
            .unwrap(),
            ConversationMemoryArgs {
                id: "conv-1".into(),
                from: 2,
                to: 4,
                user: true,
                topics: vec!["finance".into(), "ops".into()],
                guidance: None,
            }
        );
        assert_eq!(
            parse_conversation_memory_args_with_selected("conv-2 1 3", Some("conv-1")).unwrap(),
            ConversationMemoryArgs {
                id: "conv-2".into(),
                from: 1,
                to: 3,
                user: false,
                topics: Vec::new(),
                guidance: None,
            }
        );
        assert_eq!(
            parse_conversation_memory_args_with_selected(
                "conv-2 1 3 --guidance keep-preferences",
                Some("conv-1")
            )
            .unwrap()
            .guidance
            .as_deref(),
            Some("keep-preferences")
        );
        assert!(parse_conversation_memory_args_with_selected("--topic", Some("conv-1")).is_err());
    }

    #[test]
    fn conversation_policy_args_use_selected_conversation_and_flags() {
        let (id, options) = parse_conversation_policy_args_with_selected(
            "--load-memory true --generate-memory=false --allow-tool-category shell --allow-skill-category=review --max-tokens-before-compaction 512 --max-compaction-output-tokens=96 --compaction-guidance Keep",
            Some("conv-1"),
        )
        .unwrap();
        assert_eq!(id, "conv-1");
        assert_eq!(options.load_memory, Some(true));
        assert_eq!(options.generate_memory, Some(false));
        assert_eq!(options.allowed_tool_categories, vec!["shell"]);
        assert_eq!(options.allowed_skill_categories, vec!["review"]);
        assert_eq!(options.max_tokens_before_compaction, Some(512));
        assert_eq!(options.max_compaction_output_tokens, Some(96));
        assert_eq!(options.compaction_guidance.as_deref(), Some("Keep"));
    }

    #[test]
    fn conversation_policy_args_accept_clear_values_and_explicit_id() {
        let (id, options) = parse_conversation_policy_args_with_selected(
            "conv-2 --clear --load-memory clear --clear-generate-memory --clear-tool-categories --clear-skill-categories --max-tokens-before-compaction clear --clear-max-compaction-output-tokens --compaction-guidance=clear",
            Some("conv-1"),
        )
        .unwrap();
        assert_eq!(id, "conv-2");
        assert!(options.clear);
        assert!(options.clear_load_memory);
        assert!(options.clear_generate_memory);
        assert!(options.clear_allowed_tool_categories);
        assert!(options.clear_allowed_skill_categories);
        assert!(options.clear_max_tokens_before_compaction);
        assert!(options.clear_max_compaction_output_tokens);
        assert!(options.clear_compaction_guidance);
        assert!(parse_conversation_policy_args_with_selected("", None).is_err());
        assert!(
            parse_conversation_policy_args_with_selected("conv-2 --load-memory maybe", None)
                .is_err()
        );
    }

    #[test]
    fn conversation_delete_plan_args_accept_recursive_flag() {
        assert_eq!(
            parse_conversation_delete_plan_args("conv-1").unwrap(),
            ("conv-1".into(), false)
        );
        assert_eq!(
            parse_conversation_delete_plan_args("conv-1 --recursive").unwrap(),
            ("conv-1".into(), true)
        );
        assert_eq!(
            parse_conversation_delete_plan_args("conv-1 -r").unwrap(),
            ("conv-1".into(), true)
        );
        assert!(parse_conversation_delete_plan_args("").is_err());
        assert!(parse_conversation_delete_plan_args("conv-1 --force").is_err());
    }

    #[test]
    fn conversation_delete_plan_args_use_selected_conversation_when_id_omitted() {
        assert_eq!(
            parse_conversation_delete_plan_args_with_selected("--recursive", Some("conv-1"))
                .unwrap(),
            ("conv-1".into(), true)
        );
        assert_eq!(
            parse_conversation_delete_plan_args_with_selected("", Some("conv-1")).unwrap(),
            ("conv-1".into(), false)
        );
        assert!(parse_conversation_delete_plan_args_with_selected("--recursive", None).is_err());
    }

    #[test]
    fn conversation_delete_many_args_accept_multiple_ids() {
        assert_eq!(
            parse_conversation_delete_many_args("conv-1 conv-2 --recursive").unwrap(),
            (vec!["conv-1".into(), "conv-2".into()], true)
        );
        assert_eq!(
            parse_conversation_delete_many_args("conv-1 -r").unwrap(),
            (vec!["conv-1".into()], true)
        );
        assert!(parse_conversation_delete_many_args("").is_err());
        assert!(parse_conversation_delete_many_args("conv-1 --force").is_err());
    }

    #[test]
    fn conversation_delete_agent_args_use_active_agent_when_omitted() {
        assert_eq!(
            parse_conversation_delete_agent_args_with_active("--recursive", Some("agent-active"))
                .unwrap(),
            ("agent-active".into(), true)
        );
        assert_eq!(
            parse_conversation_delete_agent_args_with_active("agent-other -r", None).unwrap(),
            ("agent-other".into(), true)
        );
        assert!(parse_conversation_delete_agent_args_with_active("", None).is_err());
        assert!(parse_conversation_delete_agent_args_with_active("agent-a agent-b", None).is_err());
        assert!(parse_conversation_delete_agent_args_with_active("agent-a --force", None).is_err());
    }

    #[test]
    fn conversation_confirm_delete_cleans_linked_side_data() {
        let _home = HarnessHomeGuard::new();
        let store = ConversationStore::from_env();
        let conversation = store
            .create(Some("Delete side data".into()), Some("agent-a".into()))
            .unwrap();
        let compaction = CompactionStore::from_env()
            .create_from_text_for_conversation(
                "linked compacted context",
                None,
                None,
                Some("test".into()),
                Some(conversation.id.clone()),
            )
            .unwrap();
        let memory = MemoryStore::from_env()
            .create_for_conversation(
                MemoryTarget::Agent,
                "Remember linked context",
                MemoryAuthor::Model,
                None,
                Some(conversation.id.clone()),
            )
            .unwrap();
        let mut app = App {
            selected_conversation_id: Some(conversation.id.clone()),
            pending_conversation_action: Some(PendingConversationAction::Delete {
                id: conversation.id.clone(),
                recursive: false,
                delete_ids: vec![conversation.id.clone()],
            }),
            ..App::default()
        };

        confirm_conversation_action(&mut app).unwrap();

        assert!(
            ConversationStore::from_env()
                .show(&conversation.id)
                .is_err()
        );
        assert!(CompactionStore::from_env().show(&compaction.id).is_err());
        assert!(MemoryStore::from_env().get(&memory.id).is_err());
        assert_eq!(app.selected_conversation_id, None);
        assert!(app.transcript.iter().any(|line| {
            matches!(line.kind, LineKind::Event)
                && line
                    .text
                    .contains("Deleted 1 linked compaction artifact(s)")
                && line.text.contains("1 linked memory record(s)")
        }));
    }

    #[test]
    fn conversation_confirm_delete_many_cleans_linked_side_data() {
        let _home = HarnessHomeGuard::new();
        let store = ConversationStore::from_env();
        let first = store
            .create(Some("Delete first".into()), Some("agent-a".into()))
            .unwrap();
        let second = store
            .create(Some("Delete second".into()), Some("agent-a".into()))
            .unwrap();
        let keep = store
            .create(Some("Keep".into()), Some("agent-a".into()))
            .unwrap();
        let compaction = CompactionStore::from_env()
            .create_from_text_for_conversation(
                "linked compacted context",
                None,
                None,
                Some("test".into()),
                Some(first.id.clone()),
            )
            .unwrap();
        let memory = MemoryStore::from_env()
            .create_for_conversation(
                MemoryTarget::Agent,
                "Remember linked context",
                MemoryAuthor::Model,
                None,
                Some(second.id.clone()),
            )
            .unwrap();
        let mut app = App {
            selected_conversation_id: Some(first.id.clone()),
            pending_conversation_action: Some(PendingConversationAction::DeleteMany {
                ids: vec![first.id.clone(), second.id.clone()],
                recursive: false,
                delete_ids: vec![first.id.clone(), second.id.clone()],
            }),
            ..App::default()
        };

        confirm_conversation_action(&mut app).unwrap();

        assert!(ConversationStore::from_env().show(&first.id).is_err());
        assert!(ConversationStore::from_env().show(&second.id).is_err());
        assert!(ConversationStore::from_env().show(&keep.id).is_ok());
        assert!(CompactionStore::from_env().show(&compaction.id).is_err());
        assert!(MemoryStore::from_env().get(&memory.id).is_err());
        assert_eq!(app.selected_conversation_id, None);
        assert!(app.transcript.iter().any(|line| {
            matches!(line.kind, LineKind::Event)
                && line
                    .text
                    .contains("Deleted 1 linked compaction artifact(s)")
                && line.text.contains("1 linked memory record(s)")
        }));
    }

    #[test]
    fn conversation_confirm_delete_agent_cleans_linked_side_data() {
        let _home = HarnessHomeGuard::new();
        let store = ConversationStore::from_env();
        let first = store
            .create(Some("Delete first".into()), Some("agent-a".into()))
            .unwrap();
        let second = store
            .create(Some("Delete second".into()), Some("agent-a".into()))
            .unwrap();
        let keep = store
            .create(Some("Keep".into()), Some("agent-b".into()))
            .unwrap();
        let compaction = CompactionStore::from_env()
            .create_from_text_for_conversation(
                "linked compacted context",
                None,
                None,
                Some("test".into()),
                Some(first.id.clone()),
            )
            .unwrap();
        let memory = MemoryStore::from_env()
            .create_for_conversation(
                MemoryTarget::Agent,
                "Remember linked context",
                MemoryAuthor::Model,
                None,
                Some(second.id.clone()),
            )
            .unwrap();
        let mut app = App {
            selected_conversation_id: Some(first.id.clone()),
            pending_conversation_action: Some(PendingConversationAction::DeleteAgent {
                agent: "agent-a".into(),
                recursive: false,
                delete_ids: vec![first.id.clone(), second.id.clone()],
            }),
            ..App::default()
        };

        confirm_conversation_action(&mut app).unwrap();

        assert!(ConversationStore::from_env().show(&first.id).is_err());
        assert!(ConversationStore::from_env().show(&second.id).is_err());
        assert!(ConversationStore::from_env().show(&keep.id).is_ok());
        assert!(CompactionStore::from_env().show(&compaction.id).is_err());
        assert!(MemoryStore::from_env().get(&memory.id).is_err());
        assert_eq!(app.selected_conversation_id, None);
        assert!(app.transcript.iter().any(|line| {
            matches!(line.kind, LineKind::Event)
                && line.text.contains("for agent agent-a")
                && line.text.contains(&first.id)
                && line.text.contains(&second.id)
        }));
        assert!(app.transcript.iter().any(|line| {
            matches!(line.kind, LineKind::Event)
                && line
                    .text
                    .contains("Deleted 1 linked compaction artifact(s)")
                && line.text.contains("1 linked memory record(s)")
        }));
    }

    #[test]
    fn conversation_tree_format_includes_nested_branch_counts() {
        let tree = vec![ConversationTreeNode {
            id: "conv-root".into(),
            title: "Root".into(),
            agent_id: "agent-a".into(),
            parent_id: None,
            branch_point: None,
            branch_reason: None,
            topic_preview: Some("root topic".into()),
            own_message_count: 2,
            expanded_message_count: 2,
            children: vec![ConversationTreeNode {
                id: "conv-child".into(),
                title: "Child".into(),
                agent_id: "agent-a".into(),
                parent_id: Some("conv-root".into()),
                branch_point: Some(2),
                branch_reason: Some("try another path".into()),
                topic_preview: Some("child topic".into()),
                own_message_count: 1,
                expanded_message_count: 3,
                children: Vec::new(),
            }],
        }];

        let formatted = format_conversation_tree(&tree);
        assert!(formatted.contains(" [1] Root (conv-root) agent=agent-a own=2 expanded=2"));
        assert!(
            formatted.contains(
                "   [2] Child (conv-child) agent=agent-a own=1 expanded=3 branch_point=2"
            )
        );
        assert!(formatted.contains("topic=child topic"));
        assert!(formatted.contains("reason=try another path"));
        assert_eq!(format_conversation_tree(&[]), "no conversations");

        let (picker, index) = format_conversation_tree_picker(&tree, Some("conv-child"));
        assert_eq!(
            index,
            vec!["conv-root".to_string(), "conv-child".to_string()]
        );
        assert!(picker.contains("*[2] Child"));
    }

    #[test]
    fn conversation_browser_flattens_tree_and_moves_selection() {
        let tree = vec![ConversationTreeNode {
            id: "conv-root".into(),
            title: "Root".into(),
            agent_id: "agent-a".into(),
            parent_id: None,
            branch_point: None,
            branch_reason: None,
            topic_preview: Some("root topic".into()),
            own_message_count: 2,
            expanded_message_count: 2,
            children: vec![ConversationTreeNode {
                id: "conv-child".into(),
                title: "Child".into(),
                agent_id: "agent-a".into(),
                parent_id: Some("conv-root".into()),
                branch_point: Some(2),
                branch_reason: Some("try another path".into()),
                topic_preview: Some("child topic".into()),
                own_message_count: 1,
                expanded_message_count: 3,
                children: Vec::new(),
            }],
        }];
        let browser = conversation_browser_from_tree(&tree, Some("conv-child"));
        assert_eq!(browser.rows.len(), 2);
        assert_eq!(browser.selected, 1);
        assert_eq!(browser.rows[1].depth, 1);
        assert_eq!(browser.rows[1].id, "conv-child");
        assert_eq!(browser.rows[1].branch_point, Some(2));
        assert_eq!(
            browser.rows[1].topic_preview.as_deref(),
            Some("child topic")
        );

        let mut app = App {
            conversation_browser: Some(browser),
            ..App::default()
        };
        move_conversation_browser_selection(&mut app, -1);
        assert_eq!(app.conversation_browser.as_ref().unwrap().selected, 0);
        move_conversation_browser_selection(&mut app, 4);
        assert_eq!(app.conversation_browser.as_ref().unwrap().selected, 1);
        assert_eq!(
            current_browser_conversation_id(&app).as_deref(),
            Some("conv-child")
        );
    }

    #[test]
    fn only_mid_run_control_slash_commands_are_allowed_while_running() {
        assert!(is_mid_run_control_command("/guide steer this run"));
        assert!(is_mid_run_control_command("/guide"));
        assert!(is_mid_run_control_command("/stop changed my mind"));
        assert!(is_mid_run_control_command("/stop"));
        assert!(is_mid_run_control_command("/approval list last"));
        assert!(!is_mid_run_control_command("/score 8"));
        assert!(!is_mid_run_control_command("/replay last"));
        assert!(!is_mid_run_control_command("/tool!echo {}"));
        assert!(!is_mid_run_control_command("/guidance"));
        assert!(!is_mid_run_control_command("normal prompt"));
    }

    #[test]
    fn formats_status_duration() {
        assert_eq!(format_duration(42), "42ms");
        assert_eq!(format_duration(1_250), "1.2s");
        assert_eq!(format_duration(65_000), "1m 5s");
    }

    #[test]
    fn run_completed_sets_final_elapsed_time() {
        let mut app = App {
            state: AppState::Running,
            run_started_at: Some(Instant::now()),
            ..App::default()
        };
        let run_id = RunId::new();
        let store = agent_tracing::InMemoryEventStore::new();
        let event = store.append(
            run_id,
            None,
            RunEventKind::RunCompleted {
                final_output: "done".into(),
                total_cost_usd: None,
                total_duration_ms: 1234,
            },
        );

        handle_run_event(&mut app, &event);

        assert_eq!(app.elapsed_ms, 1234);
        assert_eq!(app.run_started_at, None);
        assert_eq!(app.state, AppState::Idle);
    }

    #[test]
    fn run_completed_prompts_to_keep_auto_compaction() {
        let mut app = App {
            state: AppState::Running,
            run_started_at: Some(Instant::now()),
            ..App::default()
        };
        let run_id = RunId::new();
        let store = agent_tracing::InMemoryEventStore::new();
        let context = store.append(
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
        handle_run_event(&mut app, &context);

        let completed = store.append(
            run_id,
            Some(context.id),
            RunEventKind::RunCompleted {
                final_output: "done".into(),
                total_cost_usd: None,
                total_duration_ms: 1234,
            },
        );
        handle_run_event(&mut app, &completed);

        assert_eq!(app.pending_auto_compaction_run, Some(run_id));
        assert!(app.transcript.iter().any(|line| {
            line.text.contains("Auto compacted context ready")
                && line.text.contains("/compact keep-run")
        }));
    }

    #[test]
    fn stop_event_records_cancellation() {
        let store = agent_tracing::InMemoryEventStore::new();
        let run_id = RunId::new();
        let started = store.append(
            run_id,
            None,
            RunEventKind::RunStarted {
                agent_id: "agent".into(),
                input: "task".into(),
            },
        );
        let recorded_events =
            record_stop_event(&store, run_id, "user requested stop".into()).unwrap();

        let events = store.events(run_id);
        assert_eq!(events.len(), 2);
        assert_eq!(recorded_events.len(), 2);
        assert!(matches!(
            &events[1].kind,
            RunEventKind::RunCancelled { reason } if reason == "user requested stop"
        ));
        assert_eq!(events[1].parent_event, Some(started.id));
        let summary = stopped_run_summary_text(run_id, "user requested stop", &recorded_events);
        assert!(summary.contains("Stopped TUI run"));
        assert!(summary.contains("Observed before stop: 2 events"));
    }
}
