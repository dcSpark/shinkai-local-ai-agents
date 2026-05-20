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

use agent_adapters::AdapterRegistry;
use agent_config::ConfigResolver;
use agent_conversations::{
    ConversationMessage, ConversationPolicy, ConversationStore, ConversationTreeNode,
    render_message_range,
};
use agent_core::{AgentConfig, ContextSnapshot, HarnessApi, UserInput};
use agent_memory::{MemoryStore, MemoryTarget};
use agent_prompts::{PromptStore, is_valid_prompt_name};
use agent_storage::StoragePaths;
use agent_tools::{ToolId, ToolRegistry};
use agent_tracing::{
    EventStore, PublishingEventStore, RunEvent, RunEventKind, RunId, SqliteEventStore,
    hook_remediation_plan, is_terminal_run_event, latest_event_id, validate_guidance_content,
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
    DeleteRange {
        id: String,
        from: usize,
        to: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConversationMemoryArgs {
    id: String,
    from: usize,
    to: usize,
    user: bool,
    topics: Vec<String>,
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
    let registry = setup::build_registry(
        options.enable_shell,
        options.enable_subagent,
        options.enable_capability_drafts,
        options.agent_id.as_deref(),
    );
    let agent = setup::build_agent(&options);
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
                    &mut app, evt, demo, &registry, &agent, &publish_tx,
                    &options,
                ),
                Some(Err(_)) | None => app.quit = true,
            },
            evt = events_rx.recv() => {
                if let Some(e) = evt {
                    handle_run_event(&mut app, &e);
                }
            },
            _ = status_tick.tick() => {
                update_elapsed_time(&mut app);
            }
        }
    }
}

fn handle_terminal_event(
    app: &mut App,
    evt: CtEvent,
    demo: Demo,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) {
    let key = match evt {
        CtEvent::Key(k) if k.kind == KeyEventKind::Press => k,
        _ => return,
    };

    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
        if app.state == AppState::Running {
            stop_active_run(app, "user requested stop");
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
                stop_active_run(app, "user requested stop");
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
                if is_mid_run_guidance_command(trimmed) {
                    let prompt = std::mem::take(&mut app.input);
                    handle_slash_command(app, &prompt, demo, registry, agent, publish_tx, options);
                } else {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: "Run in progress. Use /guide <text> for mid-run guidance.".into(),
                    });
                }
                return;
            }
            let prompt = std::mem::take(&mut app.input);
            if handle_slash_command(app, &prompt, demo, registry, agent, publish_tx, options) {
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
    let prompt = match resolve_saved_prompt_or_literal(&original_prompt) {
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

fn resolve_saved_prompt_or_literal(text: &str) -> anyhow::Result<String> {
    let trimmed = text.trim();
    let Some(name) = trimmed.strip_prefix("/run ").map(str::trim) else {
        return Ok(text.to_string());
    };
    if !is_valid_prompt_name(name) {
        return Ok(name.to_string());
    }
    Ok(PromptStore::from_env()
        .get(name)?
        .map(|prompt| prompt.body)
        .unwrap_or_else(|| name.to_string()))
}

fn handle_slash_command(
    app: &mut App,
    prompt: &str,
    demo: Demo,
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
    options: &setup::RuntimeOptions,
) -> bool {
    let trimmed = prompt.trim();
    if trimmed == "/agent" {
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
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("/tool!").map(str::trim) {
        start_manual_tool_call(app, rest, registry, agent, publish_tx);
        return true;
    }
    if let Some(rest) = trimmed.strip_prefix("/tool ").map(str::trim) {
        start_forced_tool_call(app, rest, demo, registry, agent, publish_tx, options);
        return true;
    }
    if let Some(rest) = conversation_slash_rest(trimmed) {
        handle_conversation_slash(app, rest);
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
    if let Some(input) = preview_slash_rest(trimmed) {
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
        return true;
    }
    if let Some(score) = score_slash_rest(trimmed) {
        let Some(run_id) = app.last_run_id else {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: "No run to score yet.".into(),
            });
            return true;
        };
        let score = match parse_score_slash_rest(score) {
            Ok(score) => score,
            Err(err) => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Score failed: {err}"),
                });
                return true;
            }
        };
        match open_event_store() {
            Ok(store) => match append_score_event(&store, run_id, "last_answer".into(), score) {
                Ok(()) => {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Event,
                        text: format!("Score recorded for {run_id}: {score}/10"),
                    });
                }
                Err(err) => app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Score failed: {err}"),
                }),
            },
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Score failed: {err}"),
            }),
        }
        return true;
    }
    if let Some(text) = guide_slash_rest(trimmed) {
        let Some(run_id) = app.last_run_id else {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: "No run to guide yet.".into(),
            });
            return true;
        };
        match open_event_store() {
            Ok(store) => match append_guidance_event(&store, run_id, text) {
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
    false
}

fn handle_conversation_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if rest.is_empty() || rest == "help" {
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
                "/conversation range [<id>] <from> <to>",
                "/conversation range [<id>] <from>:<to>",
                "/conversation memory [<id>] <from>:<to> [--user] [--topic <topic>]",
                "/conversation range-delete [<id>] <from>:<to>",
                "/conversation confirm",
                "/conversation cancel",
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
            text: "Conversation command needs recover, tree, browse, select, policy, delete-plan, delete, range, memory, range-delete, confirm, cancel, or help.".into(),
        }),
    }
}

fn conversation_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/conversation" {
        Some("")
    } else {
        trimmed.strip_prefix("/conversation ").map(str::trim)
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

fn parse_conversation_memory_args_with_selected(
    rest: &str,
    selected: Option<&str>,
) -> anyhow::Result<ConversationMemoryArgs> {
    let mut positional = Vec::new();
    let mut topics = Vec::new();
    let mut user = false;
    let mut parts = rest.split_whitespace();
    while let Some(part) = parts.next() {
        match part {
            "--user" => user = true,
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

fn push_conversation_delete_plan(app: &mut App, plan: &serde_json::Value) {
    let delete_count = plan["delete_count"].as_u64().unwrap_or_default();
    let recursive = plan["recursive"].as_bool() == Some(true);
    push_event(
        app,
        format!("Conversation delete plan: {delete_count} conversations, recursive={recursive}"),
    );
    app.transcript.push(TranscriptLine {
        kind: LineKind::Assistant,
        text: serde_json::to_string_pretty(plan)
            .unwrap_or_else(|_| "<unserializable delete plan>".into()),
    });
}

fn push_conversation_range_review(app: &mut App, review: &serde_json::Value) {
    let deletable = review["deletable_by_delete_range"].as_bool() == Some(true);
    push_event(
        app,
        format!("Conversation range review: deletable={deletable}"),
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

fn prepare_conversation_range_delete(app: &mut App, args: &str) -> anyhow::Result<()> {
    let (id, from, to) =
        parse_conversation_range_args_with_selected(args, app.selected_conversation_id.as_deref())?;
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
    app.pending_conversation_action = Some(PendingConversationAction::DeleteRange { id, from, to });
    push_event(
        app,
        "Pending conversation range delete. Use /conversation confirm or /conversation cancel."
            .to_string(),
    );
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
    let records = MemoryStore::from_env().generate_from_conversation_text_with_topics(
        target,
        &rendered.text,
        Some(rendered.source_range.clone()),
        Some(args.id.clone()),
        args.topics.clone(),
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
            let deleted = ConversationStore::from_env().delete_many(&[id], recursive)?;
            if app
                .selected_conversation_id
                .as_ref()
                .is_some_and(|selected| deleted.contains(selected))
            {
                app.selected_conversation_id = None;
            }
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
        }
        PendingConversationAction::DeleteRange { id, from, to } => {
            let before = ConversationStore::from_env().expanded(&id)?.messages.len();
            let doc = ConversationStore::from_env().delete_message_range(&id, from, to)?;
            let after = ConversationStore::from_env().expanded(&id)?.messages.len();
            push_event(
                app,
                format!(
                    "Deleted conversation range {from}:{to} from {id}; expanded messages {before}->{after}, own messages {}",
                    doc.messages.len()
                ),
            );
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
    Ok(serde_json::json!({
        "conversation_id": id,
        "recursive": recursive,
        "delete_count": delete_ids.len(),
        "delete_ids": delete_ids,
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
    let include_compact = plan["suggested_run"]["include_compact"]
        .as_str()
        .unwrap_or("none");
    let load_memory = plan["suggested_run"]["load_memory"].as_bool() == Some(true);
    [
        format!("Recovery guidance for {title} ({id})"),
        format!("messages: own {own_messages}, expanded {expanded_messages}"),
        format!("linked recovery data: {compactions} compaction(s), {memories} memory item(s)"),
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
    lines.push(format!(
        "{indent}{marker}[{ordinal}] {} ({}) agent={} own={} expanded={}{}",
        node.title,
        node.id,
        node.agent_id,
        node.own_message_count,
        node.expanded_message_count,
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
        "deletable_by_delete_range": !has_child_branches && !includes_inherited,
        "warnings": warnings,
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

fn handle_hooks_slash(app: &mut App, rest: &str, agent: &AgentConfig) {
    let rest = rest.trim();
    if rest.is_empty() || rest == "help" {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/hooks review [run-id]",
                "/hooks list",
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
        "list" => {
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
        "available" => handle_hooks_available(app, &agent.id),
        "disable" | "enable" => handle_hook_policy_change(app, command, args, &agent.id),
        _ => app.transcript.push(TranscriptLine {
            kind: LineKind::Error,
            text: "Hooks command needs review, list, available, disable, enable, or help.".into(),
        }),
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
    let run_id = if args.trim().is_empty() {
        match app.last_run_id {
            Some(run_id) => run_id,
            None => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: "Hook review needs a run id or a previous run.".into(),
                });
                return;
            }
        }
    } else {
        match uuid::Uuid::parse_str(args.trim()) {
            Ok(id) => RunId(id),
            Err(err) => {
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Error,
                    text: format!("Hook review failed: {err}"),
                });
                return;
            }
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
    if trimmed == "/compact" {
        Some("")
    } else {
        trimmed.strip_prefix("/compact ").map(str::trim)
    }
}

fn handle_compact_slash(app: &mut App, rest: &str) {
    let rest = rest.trim();
    if rest.is_empty() || rest == "help" {
        app.transcript.push(TranscriptLine {
            kind: LineKind::Assistant,
            text: [
                "/compact keep [run-id]",
                "/compact status",
                "/compact dismiss",
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
        "keep" => handle_compact_keep(app, args),
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
            text: "Compact command needs keep, status, dismiss, or help.".into(),
        }),
    }
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

fn guide_slash_rest(trimmed: &str) -> Option<&str> {
    if trimmed == "/guide" {
        Some("")
    } else {
        trimmed.strip_prefix("/guide ").map(str::trim)
    }
}

fn is_mid_run_guidance_command(trimmed: &str) -> bool {
    guide_slash_rest(trimmed).is_some()
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

fn parse_score_slash_rest(rest: &str) -> anyhow::Result<f32> {
    let score = if rest.trim().is_empty() {
        10.0
    } else {
        rest.trim().parse::<f32>()?
    };
    validate_quality_score(score)?;
    Ok(score)
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

fn handle_run_event(app: &mut App, evt: &RunEvent) {
    match &evt.kind {
        RunEventKind::RunStarted { .. } => {
            app.last_run_id = Some(evt.run_id);
            app.pending_auto_compaction_run = None;
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
        }
        RunEventKind::RunPaused { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run paused: {reason}"),
            });
            update_elapsed_time(app);
            app.run_started_at = None;
            app.active_run_handle = None;
            app.state = AppState::Idle;
        }
        RunEventKind::RunCancelled { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run cancelled: {reason}"),
            });
            update_elapsed_time(app);
            app.run_started_at = None;
            app.active_run_handle = None;
            app.state = AppState::Idle;
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
                    "Auto compacted context ready. Type /compact keep to save it or /compact dismiss.".into(),
                );
            }
            app.elapsed_ms = *total_duration_ms;
            app.run_started_at = None;
            app.active_run_handle = None;
            app.state = AppState::Idle;
        }
        RunEventKind::RunFailed { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run failed: {reason}"),
            });
            update_elapsed_time(app);
            app.run_started_at = None;
            app.active_run_handle = None;
            app.state = AppState::Idle;
        }
    }
}

fn stop_active_run(app: &mut App, reason: &str) {
    if let Some(handle) = app.active_run_handle.take() {
        handle.abort();
    }
    if let Some(run_id) = app.last_run_id {
        match open_event_store() {
            Ok(store) => {
                if let Err(err) = record_stop_event(&store, run_id, reason.to_string()) {
                    app.transcript.push(TranscriptLine {
                        kind: LineKind::Error,
                        text: format!("Stop trace write failed: {err}"),
                    });
                }
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Stop trace write failed: {err}"),
            }),
        }
    }
    update_elapsed_time(app);
    app.run_started_at = None;
    app.state = AppState::Idle;
    app.transcript.push(TranscriptLine {
        kind: LineKind::Error,
        text: format!("Run cancelled: {reason}"),
    });
}

fn record_stop_event(store: &dyn EventStore, run_id: RunId, reason: String) -> anyhow::Result<()> {
    let parent = latest_event_id(&store.events(run_id))
        .ok_or_else(|| anyhow::anyhow!("run {run_id} has no trace events"))?;
    store.append(run_id, Some(parent), RunEventKind::RunCancelled { reason });
    Ok(())
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
            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::styled(
                format!(
                    "{marker} {}{title} ({id}) agent={agent} own={own} expanded={expanded}{reason}",
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
        assert!(parse_score_slash_rest("10").is_ok());
        assert!(parse_score_slash_rest("").is_ok());
        assert!(parse_score_slash_rest("11").is_err());
    }

    #[test]
    fn slash_helpers_match_exact_command_names() {
        assert_eq!(score_slash_rest("/score 7"), Some("7"));
        assert_eq!(score_slash_rest("/score"), Some(""));
        assert_eq!(score_slash_rest("/scoreboard 7"), None);
        assert_eq!(preview_slash_rest("/preview hello"), Some("hello"));
        assert_eq!(preview_slash_rest("/preview"), Some(""));
        assert_eq!(preview_slash_rest("/previewer hello"), None);
        assert_eq!(
            conversation_slash_rest("/conversation recover conv-1"),
            Some("recover conv-1")
        );
        assert_eq!(conversation_slash_rest("/conversation"), Some(""));
        assert_eq!(conversation_slash_rest("/conversations"), None);
        assert_eq!(
            hooks_slash_rest("/hooks review run-1"),
            Some("review run-1")
        );
        assert_eq!(hooks_slash_rest("/hooks"), Some(""));
        assert_eq!(hooks_slash_rest("/hook"), None);
        assert_eq!(compact_slash_rest("/compact keep"), Some("keep"));
        assert_eq!(compact_slash_rest("/compact"), Some(""));
        assert_eq!(compact_slash_rest("/compactness"), None);
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
            }
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
    fn conversation_tree_format_includes_nested_branch_counts() {
        let tree = vec![ConversationTreeNode {
            id: "conv-root".into(),
            title: "Root".into(),
            agent_id: "agent-a".into(),
            parent_id: None,
            branch_reason: None,
            own_message_count: 2,
            expanded_message_count: 2,
            children: vec![ConversationTreeNode {
                id: "conv-child".into(),
                title: "Child".into(),
                agent_id: "agent-a".into(),
                parent_id: Some("conv-root".into()),
                branch_reason: Some("try another path".into()),
                own_message_count: 1,
                expanded_message_count: 3,
                children: Vec::new(),
            }],
        }];

        let formatted = format_conversation_tree(&tree);
        assert!(formatted.contains(" [1] Root (conv-root) agent=agent-a own=2 expanded=2"));
        assert!(formatted.contains("   [2] Child (conv-child) agent=agent-a own=1 expanded=3"));
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
            branch_reason: None,
            own_message_count: 2,
            expanded_message_count: 2,
            children: vec![ConversationTreeNode {
                id: "conv-child".into(),
                title: "Child".into(),
                agent_id: "agent-a".into(),
                parent_id: Some("conv-root".into()),
                branch_reason: Some("try another path".into()),
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
    fn only_guide_slash_is_allowed_while_running() {
        assert!(is_mid_run_guidance_command("/guide steer this run"));
        assert!(is_mid_run_guidance_command("/guide"));
        assert!(!is_mid_run_guidance_command("/score 8"));
        assert!(!is_mid_run_guidance_command("/tool!echo {}"));
        assert!(!is_mid_run_guidance_command("/guidance"));
        assert!(!is_mid_run_guidance_command("normal prompt"));
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
                && line.text.contains("/compact keep")
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
        record_stop_event(&store, run_id, "user requested stop".into()).unwrap();

        let events = store.events(run_id);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[1].kind,
            RunEventKind::RunCancelled { reason } if reason == "user requested stop"
        ));
        assert_eq!(events[1].parent_event, Some(started.id));
    }
}
