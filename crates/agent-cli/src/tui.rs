//! ratatui-based TUI surface. v0 ships:
//! - transcript pane (User / Assistant / Event / Error lines, color-coded)
//! - status bar (run state, tokens, tool-call budget)
//! - input box (Enter to send, Esc / Ctrl+C to quit, Backspace, character entry)
//! - live event streaming from the harness via `PublishingEventStore`
//!
//! Most slash-command grammar (`/tool`, `/tool!`, `/score`, `/guide`, …) and
//! the tool-tray pane from `specs/architecture.md` §20.1 land in later slices.

use std::io::{self, Stdout};
use std::sync::Arc;

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

use agent_core::{AgentConfig, Harness, HarnessApi, ToolPolicy, UserInput};
use agent_prompts::{PromptStore, is_valid_prompt_name};
use agent_storage::StoragePaths;
use agent_tools::{ToolId, ToolRegistry};
use agent_tracing::{
    EventStore, PublishingEventStore, RunEvent, RunEventKind, RunId, SqliteEventStore,
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
    last_run_id: Option<RunId>,
    quit: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum AppState {
    #[default]
    Idle,
    Running,
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
    let registry = setup::build_registry(options.enable_shell, options.enable_subagent);
    let agent = setup::build_agent(&options);
    let calls_max = ToolPolicy::default().max_calls;

    let mut app = App {
        calls_max,
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

    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) | (_, KeyCode::Esc) => {
            app.quit = true;
        }
        (_, KeyCode::Enter) => {
            if app.state == AppState::Running {
                return;
            }
            let prompt = std::mem::take(&mut app.input);
            if prompt.trim().is_empty() {
                return;
            }
            if handle_slash_command(app, &prompt, registry, agent, publish_tx) {
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
    let harness = Harness::new(provider, Arc::new(store), registry.clone());
    let agent_clone = agent.clone();

    tokio::spawn(async move {
        // The harness emits RunFailed before returning Err, so the TUI sees
        // the failure via the event channel; the result here is best-effort.
        let _ = harness.run(&agent_clone, UserInput { text: prompt }).await;
    });
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
    registry: &Arc<ToolRegistry>,
    agent: &AgentConfig,
    publish_tx: &UnboundedSender<RunEvent>,
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
        start_manual_tool_call(app, rest, registry, agent, publish_tx);
        return true;
    }
    if let Some(input) = trimmed.strip_prefix("/preview").map(str::trim) {
        let harness = Harness::new(
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
    if let Some(score) = trimmed.strip_prefix("/score").map(str::trim) {
        let Some(run_id) = app.last_run_id else {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: "No run to score yet.".into(),
            });
            return true;
        };
        let score = score.parse::<f32>().unwrap_or(10.0);
        match open_event_store() {
            Ok(store) => {
                store.append(
                    run_id,
                    None,
                    RunEventKind::QualityScored {
                        target: "last_answer".into(),
                        score,
                    },
                );
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!("Score recorded for {run_id}: {score}/10"),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Score failed: {err}"),
            }),
        }
        return true;
    }
    if let Some(text) = trimmed.strip_prefix("/guide").map(str::trim) {
        let Some(run_id) = app.last_run_id else {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: "No run to guide yet.".into(),
            });
            return true;
        };
        match open_event_store() {
            Ok(store) => {
                store.append(
                    run_id,
                    None,
                    RunEventKind::GuidanceInjected {
                        content: text.to_string(),
                    },
                );
                app.transcript.push(TranscriptLine {
                    kind: LineKind::Event,
                    text: format!("Guidance recorded for {run_id}"),
                });
            }
            Err(err) => app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Guide failed: {err}"),
            }),
        }
        return true;
    }
    false
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
    let harness = Harness::new(
        Arc::new(agent_llm::FakeProvider::echo()),
        Arc::new(store),
        registry.clone(),
    );
    let agent = agent.clone();
    tokio::spawn(async move {
        let _ = harness.call_tool(&agent, ToolId::from(name), input).await;
    });
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

fn open_event_store() -> anyhow::Result<SqliteEventStore> {
    let paths = StoragePaths::from_env();
    paths.ensure_base_dirs()?;
    Ok(SqliteEventStore::open(paths.state_db())?)
}

fn handle_run_event(app: &mut App, evt: &RunEvent) {
    match &evt.kind {
        RunEventKind::RunStarted { .. } => {
            app.last_run_id = Some(evt.run_id);
        }
        RunEventKind::ContextBuilt { snapshot } => {
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
        RunEventKind::LlmRequestStarted { model } => {
            push_event(app, format!("LLM call started ({model})"));
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
        RunEventKind::MemoryWritten { id, operation } => {
            push_event(app, format!("Memory {operation}: {id}"));
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
            ..
        } => {
            push_event(
                app,
                format!("Ingestion completed: {artifact_id} ({sections} sections)"),
            );
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
            app.state = AppState::Idle;
        }
        RunEventKind::RunCancelled { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run cancelled: {reason}"),
            });
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
            app.state = AppState::Idle;
        }
        RunEventKind::RunFailed { reason } => {
            app.transcript.push(TranscriptLine {
                kind: LineKind::Error,
                text: format!("Run failed: {reason}"),
            });
            app.state = AppState::Idle;
        }
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

// ---------- rendering ----------

fn render(f: &mut ratatui::Frame, app: &App) {
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Agent Harness "),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_status(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let state = match app.state {
        AppState::Idle => "idle",
        AppState::Running => "running…",
    };
    let text = format!(
        " tokens {}↑/{}↓ · cost ${:.6} · calls {}/{} · {}",
        app.tokens_in, app.tokens_out, app.cost_usd, app.calls_used, app.calls_max, state
    );
    let p = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(p, area);
}

fn render_input(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let title = if app.state == AppState::Running {
        " a run is in progress… "
    } else {
        " Enter to send · Esc to quit "
    };
    let style = if app.state == AppState::Running {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    let p = Paragraph::new(format!("> {}", app.input))
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(p, area);

    if app.state == AppState::Idle {
        // Inside the box: 1 char in for the left border + 2 for "> " + input length.
        let cursor_x = area.x + 3 + app.input.chars().count() as u16;
        let cursor_y = area.y + 1;
        f.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}
