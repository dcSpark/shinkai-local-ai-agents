//! Agent Harness CLI entry point.
//!
//! Default mode is the ratatui TUI. `--print` (or piping stdout to a non-TTY)
//! switches to headless mode for scripting / CI. See `specs/architecture.md`
//! §20.1 for the surface contract.

mod headless;
mod setup;
mod tui;

use std::io::IsTerminal;

use agent_core::VisibilityLevel;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "agent")]
#[command(about = "Agent Harness CLI.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run an agent.
    ///
    /// By default launches the TUI. Use `--print` (or pipe stdout to a
    /// non-TTY) for headless execution suitable for scripting / CI.
    Run {
        /// Initial input. Pre-fills the TUI input box; sent immediately in
        /// headless mode. If omitted in headless mode, stdin is consumed.
        #[arg(short, long)]
        input: Option<String>,

        /// Headless mode (no TUI). Prints transcript to stderr and final
        /// output to stdout.
        #[arg(long)]
        print: bool,

        /// With `--print`: emit one JSON event per line on stdout instead of
        /// a human-readable transcript. Implies `--print`.
        #[arg(long)]
        json: bool,

        /// Demo mode for v0: `echo` (no tool call) or `tool` (single
        /// echo-tool call before the reply).
        #[arg(long, value_enum, default_value_t = Demo::Tool)]
        demo: Demo,

        /// LLM provider to use.
        #[arg(long, value_enum, default_value_t = Provider::Fake)]
        provider: Provider,

        /// Model id. Defaults to fake-model for fake provider and gpt-4o-mini
        /// for rig provider.
        #[arg(long)]
        model: Option<String>,

        /// OpenAI-compatible API base URL for rig provider.
        #[arg(long)]
        api_base_url: Option<String>,

        /// Environment variable containing the API key for rig provider.
        #[arg(long, default_value = "OPENAI_API_KEY")]
        api_key_env: String,

        /// Optional max output tokens for rig provider.
        #[arg(long)]
        max_output_tokens: Option<u64>,

        /// Optional temperature for rig provider.
        #[arg(long)]
        temperature: Option<f64>,

        /// Estimated input-token price in USD per 1M tokens.
        #[arg(long)]
        input_cost_per_million: Option<f64>,

        /// Estimated output-token price in USD per 1M tokens.
        #[arg(long)]
        output_cost_per_million: Option<f64>,

        /// Override the agent's max tool-call budget for this run.
        #[arg(long)]
        max_tool_calls: Option<u32>,

        /// Override how much tool detail is shown to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Explicitly register the shell tool for this run.
        #[arg(long)]
        enable_shell: bool,

        /// Explicitly register the subagent tool for this run.
        #[arg(long)]
        enable_subagent: bool,

        /// Load file-backed memory into context for this run.
        #[arg(long)]
        load_memory: bool,

        /// Load allowed file-backed skills into context for this run.
        #[arg(long)]
        load_skills: bool,

        /// Explicit ingestion artifact id to include in context. Repeatable.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,

        /// Rewrite the user prompt in a traced preprocessing LLM call before the main run.
        #[arg(long)]
        refine_prompt: bool,

        /// Instructions for the prompt refinement preprocessing call.
        #[arg(long)]
        refinement_instructions: Option<String>,

        /// Optional model id for prompt refinement. Defaults to the agent model.
        #[arg(long)]
        refinement_model: Option<String>,

        /// Pause before approval-required tools instead of auto-approving.
        #[arg(long)]
        require_approval: bool,

        /// Return the first tool output directly without an LLM interpretation pass.
        #[arg(long)]
        raw_tool_output: bool,
    },
    /// Show the exact context snapshot that would be sent for an input.
    PreviewContext {
        /// Prompt to preview. If omitted, stdin is consumed.
        #[arg(short, long)]
        input: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,

        /// Include the shell tool in the preview.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in the preview.
        #[arg(long)]
        enable_subagent: bool,

        /// Override the max tool-call budget in the preview.
        #[arg(long)]
        max_tool_calls: Option<u32>,

        /// Override how much tool detail is shown in the preview.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Include loaded memory in the preview.
        #[arg(long)]
        load_memory: bool,

        /// Preview the raw tool-output runtime mode.
        #[arg(long)]
        raw_tool_output: bool,

        /// Include allowed skills in the preview.
        #[arg(long)]
        load_skills: bool,

        /// Explicit ingestion artifact id to include in the preview. Repeatable.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,
    },
    /// Explain the effective v0 agent configuration and provenance.
    ExplainConfig {
        /// Emit JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },
    /// Explain which tools are visible to the default agent.
    ExplainTools {
        /// Emit JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,

        /// Include the shell tool in the explanation.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in the explanation.
        #[arg(long)]
        enable_subagent: bool,

        /// Override how much tool detail is shown in the explanation.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,
    },
    /// Tool operations.
    Tool {
        #[command(subcommand)]
        command: ToolCommand,
    },
    /// Trace operations.
    Trace {
        #[command(subcommand)]
        command: TraceCommand,
    },
    /// Approval operations.
    Approval {
        #[command(subcommand)]
        command: ApprovalCommand,
    },
    /// Attach guidance to a persisted run trace.
    Guide {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Guidance text to inject/record.
        text: String,
    },
    /// Mark a run as cancelled in the durable trace.
    Cancel {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Cancellation reason.
        #[arg(long, default_value = "user requested stop")]
        reason: String,
    },
    /// Score a run output or step.
    Score {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Score, conventionally 0-10.
        score: f32,

        /// Optional score target label.
        #[arg(long, default_value = "last_answer")]
        target: String,
    },
    /// Deterministic batch operations.
    Batch {
        #[command(subcommand)]
        command: BatchCommand,
    },
    /// Memory operations.
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// Skill operations.
    Skill {
        #[command(subcommand)]
        command: SkillCommand,
    },
    /// Saved prompt library operations.
    Prompt {
        #[command(subcommand)]
        command: PromptCommand,
    },
    /// Model registry operations.
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Document ingestion operations.
    Ingest {
        #[command(subcommand)]
        command: IngestCommand,
    },
    /// Inspect third-party capabilities before quarantine/allow decisions.
    Adapter {
        #[command(subcommand)]
        command: AdapterCommand,
    },
    /// Export the current profile/config/cache bundle as a tarball.
    Export {
        /// Destination tar path.
        path: String,
    },
    /// Import a profile/config/cache bundle tarball.
    Import {
        /// Source tar path.
        path: String,
    },
    /// Talk to an agent-daemon over HTTP.
    Remote {
        /// Daemon base URL.
        #[arg(long, default_value = "http://127.0.0.1:7878")]
        url: String,

        #[command(subcommand)]
        command: RemoteCommand,
    },
}

#[derive(Subcommand)]
enum ToolCommand {
    /// Call a tool directly with manual JSON input, bypassing the LLM.
    Call {
        /// Tool id, for example `echo`.
        name: String,

        /// JSON input. If omitted, stdin is consumed; empty stdin means `{}`.
        #[arg(short, long)]
        input: Option<String>,

        /// Emit JSON instead of a human-readable result.
        #[arg(long)]
        json: bool,

        /// Pause before approval-required tools instead of auto-approving.
        #[arg(long)]
        require_approval: bool,
    },
}

#[derive(Subcommand)]
enum TraceCommand {
    /// Show events for a persisted run id.
    Show {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Emit one JSON RunEvent per line.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ApprovalCommand {
    /// List approvals for a run.
    List {
        /// Run UUID printed by `agent run` or an approval-required error.
        run_id: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Record an approval decision in the run trace.
    Decide {
        /// Run UUID printed by `agent run` or an approval-required error.
        run_id: String,

        /// Approval id, for example `approval-manual-1`.
        approval_id: String,

        /// Approve the request. If omitted, the request is rejected.
        #[arg(long)]
        approve: bool,
    },
    /// Execute an approved tool call from a paused run trace.
    Execute {
        /// Run UUID printed by the approval-required error.
        run_id: String,

        /// Approval id, for example `approval-manual-1`.
        approval_id: String,

        /// Emit JSON instead of a human-readable result.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum MemoryCommand {
    /// Create a memory record.
    Create {
        content: String,

        /// Store in user.md instead of memory.md.
        #[arg(long)]
        user: bool,
    },
    /// Generate memory records manually from text.
    Generate {
        text: String,

        /// Store in user.md instead of memory.md.
        #[arg(long)]
        user: bool,

        /// Optional source range label.
        #[arg(long)]
        range: Option<String>,
    },
    /// List memory records.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Edit a memory record.
    Edit { id: String, content: String },
    /// Delete a memory record.
    Delete { id: String },
    /// Roll back memory.md or user.md from its one-step backup.
    Rollback {
        /// Roll back user.md instead of memory.md.
        #[arg(long)]
        user: bool,
    },
}

#[derive(Subcommand)]
enum SkillCommand {
    /// Import an OpenClaw/AgentSkills SKILL.md file or folder; starts quarantined.
    ImportOpenclaw { path: String },
    /// List installed skills.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Inspect an installed skill.
    Inspect { id: String },
    /// Allow a quarantined skill into context.
    Allow { id: String },
    /// Quarantine a skill.
    Quarantine { id: String },
}

#[derive(Subcommand)]
enum PromptCommand {
    /// Save or replace a prompt in the main profile library.
    Save { name: String, text: String },
    /// List saved prompts.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show a saved prompt.
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// Delete a saved prompt.
    Delete { name: String },
}

#[derive(Subcommand)]
enum ModelCommand {
    /// List configured model metadata.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show one configured model.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Save or replace model metadata.
    Save {
        id: String,
        #[arg(long)]
        max_context_tokens: Option<u64>,
        #[arg(long)]
        max_output_tokens: Option<u64>,
        #[arg(long)]
        default_temperature: Option<f64>,
        #[arg(long)]
        tool_support: Option<bool>,
        #[arg(long)]
        privacy_level: Option<String>,
        #[arg(long)]
        cost_tier: Option<String>,
        #[arg(long)]
        input_cost_per_million: Option<f64>,
        #[arg(long)]
        output_cost_per_million: Option<f64>,
    },
    /// Delete configured model metadata.
    Delete { id: String },
}

#[derive(Subcommand)]
enum IngestCommand {
    /// Ingest a local file explicitly.
    Add {
        path: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,
    },
    /// Re-run ingestion for an artifact's source, optionally with another backend.
    Rerun {
        id: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,
    },
    /// List ingestion artifacts.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show an ingestion artifact.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Remove an ingestion artifact.
    Rm { id: String },
}

#[derive(Subcommand)]
enum AdapterCommand {
    /// Import a local adapter source into quarantine.
    Import { path: String },
    /// List persisted adapter manifests.
    List {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Inspect a local adapter source and print its normalized manifest.
    Inspect {
        path: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Show a persisted adapter manifest.
    Show {
        id: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Allow a quarantined adapter manifest locally.
    Allow { id: String },
    /// Re-quarantine an adapter manifest locally.
    Quarantine { id: String },
}

#[derive(Subcommand)]
enum BatchCommand {
    /// Run one deterministic agent step per item.
    Run {
        /// Batch item input. Repeat for multiple items.
        #[arg(long = "item", required = true)]
        items: Vec<String>,

        /// Demo mode for each child run.
        #[arg(long, value_enum, default_value_t = Demo::Echo)]
        demo: Demo,

        /// Emit JSON summary.
        #[arg(long)]
        json: bool,
    },
    /// Resume a persisted batch, skipping succeeded item keys.
    Resume {
        batch_id: String,

        /// Demo mode for each retried child run.
        #[arg(long, value_enum, default_value_t = Demo::Echo)]
        demo: Demo,

        /// Emit JSON summary.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum RemoteCommand {
    /// Check daemon health/version.
    Health,
    /// Run through daemon fake-provider endpoint.
    Run {
        #[arg(short, long)]
        input: String,

        #[arg(long, default_value = "echo")]
        demo: String,

        /// LLM provider to use on the daemon.
        #[arg(long, value_enum, default_value_t = Provider::Fake)]
        provider: Provider,

        /// Model id for real-provider daemon runs.
        #[arg(long)]
        model: Option<String>,

        /// OpenAI-compatible API base URL for daemon rig provider.
        #[arg(long)]
        api_base_url: Option<String>,

        /// Environment variable containing the API key in the daemon process.
        #[arg(long, default_value = "OPENAI_API_KEY")]
        api_key_env: String,

        /// Estimated input-token price in USD per 1M tokens.
        #[arg(long)]
        input_cost_per_million: Option<f64>,

        /// Estimated output-token price in USD per 1M tokens.
        #[arg(long)]
        output_cost_per_million: Option<f64>,

        /// Override the daemon agent's max tool-call budget.
        #[arg(long)]
        max_tool_calls: Option<u32>,

        /// Override how much tool detail the daemon shows to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Include the shell tool in daemon context.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in daemon context.
        #[arg(long)]
        enable_subagent: bool,

        /// Load file-backed memory in daemon context.
        #[arg(long)]
        load_memory: bool,

        /// Load allowed skills in daemon context.
        #[arg(long)]
        load_skills: bool,

        /// Explicit ingestion artifact id to include in daemon context.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,

        /// Pause before approval-required daemon tools.
        #[arg(long)]
        require_approval: bool,

        /// Return the first daemon tool output without an interpretation pass.
        #[arg(long)]
        raw_tool_output: bool,
    },
    /// Start a daemon run asynchronously and return its run id immediately.
    RunStart {
        #[arg(short, long)]
        input: String,

        #[arg(long, default_value = "echo")]
        demo: String,

        /// LLM provider to use on the daemon.
        #[arg(long, value_enum, default_value_t = Provider::Fake)]
        provider: Provider,

        /// Model id for real-provider daemon runs.
        #[arg(long)]
        model: Option<String>,

        /// OpenAI-compatible API base URL for daemon rig provider.
        #[arg(long)]
        api_base_url: Option<String>,

        /// Environment variable containing the API key in the daemon process.
        #[arg(long, default_value = "OPENAI_API_KEY")]
        api_key_env: String,

        /// Estimated input-token price in USD per 1M tokens.
        #[arg(long)]
        input_cost_per_million: Option<f64>,

        /// Estimated output-token price in USD per 1M tokens.
        #[arg(long)]
        output_cost_per_million: Option<f64>,

        /// Override the daemon agent's max tool-call budget.
        #[arg(long)]
        max_tool_calls: Option<u32>,

        /// Override how much tool detail the daemon shows to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Include the shell tool in daemon context.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in daemon context.
        #[arg(long)]
        enable_subagent: bool,

        /// Load file-backed memory in daemon context.
        #[arg(long)]
        load_memory: bool,

        /// Load allowed skills in daemon context.
        #[arg(long)]
        load_skills: bool,

        /// Explicit ingestion artifact id to include in daemon context.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,

        /// Pause before approval-required daemon tools.
        #[arg(long)]
        require_approval: bool,

        /// Return the first daemon tool output without an interpretation pass.
        #[arg(long)]
        raw_tool_output: bool,
    },
    /// Show async daemon run status.
    RunStatus { run_id: String },
    /// Show the exact context snapshot the daemon would build.
    PreviewContext {
        #[arg(short, long)]
        input: String,

        /// Include the shell tool in the preview.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in the preview.
        #[arg(long)]
        enable_subagent: bool,

        /// Override the max tool-call budget in the preview.
        #[arg(long)]
        max_tool_calls: Option<u32>,

        /// Override how much tool detail is shown in the preview.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Include loaded memory in the preview.
        #[arg(long)]
        load_memory: bool,

        /// Preview the raw tool-output runtime mode.
        #[arg(long)]
        raw_tool_output: bool,

        /// Include allowed skills in the preview.
        #[arg(long)]
        load_skills: bool,

        /// Explicit ingestion artifact id to include in the preview.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,
    },
    /// Record remote guidance against a run.
    Guide { run_id: String, text: String },
    /// Mark a remote run as cancelled in the daemon trace store.
    Cancel {
        run_id: String,

        #[arg(long, default_value = "user requested stop")]
        reason: String,
    },
    /// Score a remote run output or step.
    Score {
        run_id: String,
        score: f32,

        #[arg(long, default_value = "last_answer")]
        target: String,
    },
    /// Remote deterministic batch operations.
    Batch {
        #[command(subcommand)]
        command: RemoteBatchCommand,
    },
    /// Call a daemon tool endpoint.
    Tool {
        name: String,

        #[arg(short, long)]
        input: Option<String>,

        /// Ask daemon to pause before approval-required tools.
        #[arg(long)]
        require_approval: bool,
    },
    /// Show daemon trace events.
    Trace { run_id: String },
    /// Remote approval operations.
    Approval {
        #[command(subcommand)]
        command: RemoteApprovalCommand,
    },
    /// Remote memory operations.
    Memory {
        #[command(subcommand)]
        command: RemoteMemoryCommand,
    },
    /// Remote skill operations.
    Skill {
        #[command(subcommand)]
        command: RemoteSkillCommand,
    },
    /// Remote model registry operations.
    Model {
        #[command(subcommand)]
        command: RemoteModelCommand,
    },
    /// Remote ingestion operations.
    Ingest {
        #[command(subcommand)]
        command: RemoteIngestCommand,
    },
    /// Remote adapter operations.
    Adapter {
        #[command(subcommand)]
        command: RemoteAdapterCommand,
    },
    /// Export a bundle from the daemon host.
    Export { path: String },
    /// Import a bundle on the daemon host.
    Import { path: String },
}

#[derive(Subcommand)]
enum RemoteApprovalCommand {
    List {
        run_id: String,
    },
    Decide {
        run_id: String,
        approval_id: String,
        #[arg(long)]
        approve: bool,
    },
    Execute {
        run_id: String,
        approval_id: String,
    },
}

#[derive(Subcommand)]
enum RemoteBatchCommand {
    Run {
        #[arg(long = "item", required = true)]
        items: Vec<String>,

        #[arg(long, default_value = "echo")]
        demo: String,
    },
    Resume {
        batch_id: String,

        #[arg(long, default_value = "echo")]
        demo: String,
    },
}

#[derive(Subcommand)]
enum RemoteMemoryCommand {
    List,
    Create {
        content: String,
        #[arg(long)]
        user: bool,
    },
    Generate {
        text: String,
        #[arg(long)]
        user: bool,
        #[arg(long)]
        range: Option<String>,
    },
    Edit {
        id: String,
        content: String,
    },
    Delete {
        id: String,
    },
    Rollback {
        #[arg(long)]
        user: bool,
    },
}

#[derive(Subcommand)]
enum RemoteSkillCommand {
    List,
    ImportOpenclaw { path: String },
    Allow { id: String },
    Quarantine { id: String },
}

#[derive(Subcommand)]
enum RemoteModelCommand {
    List,
    Show {
        id: String,
    },
    Save {
        id: String,
        #[arg(long)]
        max_context_tokens: Option<u64>,
        #[arg(long)]
        max_output_tokens: Option<u64>,
        #[arg(long)]
        default_temperature: Option<f64>,
        #[arg(long)]
        tool_support: Option<bool>,
        #[arg(long)]
        privacy_level: Option<String>,
        #[arg(long)]
        cost_tier: Option<String>,
        #[arg(long)]
        input_cost_per_million: Option<f64>,
        #[arg(long)]
        output_cost_per_million: Option<f64>,
    },
    Delete {
        id: String,
    },
}

#[derive(Subcommand)]
enum RemoteIngestCommand {
    List,
    Add {
        path: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,
    },
    Rerun {
        id: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,
    },
    Show {
        id: String,
    },
    Rm {
        id: String,
    },
}

#[derive(Subcommand)]
enum RemoteAdapterCommand {
    List,
    Import { path: String },
    Show { id: String },
    Allow { id: String },
    Quarantine { id: String },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Demo {
    /// LLM replies with text immediately; no tool call in the trace.
    Echo,
    /// LLM scripts a single `echo` tool call, then replies.
    Tool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Provider {
    /// Deterministic offline fake provider.
    Fake,
    /// rig-core OpenAI-compatible provider.
    Rig,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ToolVisibility {
    FullSchema,
    NameAndDescription,
    NameOnly,
}

impl From<ToolVisibility> for VisibilityLevel {
    fn from(value: ToolVisibility) -> Self {
        match value {
            ToolVisibility::FullSchema => VisibilityLevel::FullSchema,
            ToolVisibility::NameAndDescription => VisibilityLevel::NameAndDescription,
            ToolVisibility::NameOnly => VisibilityLevel::NameOnly,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = <Cli as Parser>::parse();
    match cli.command {
        Command::Run {
            input,
            print,
            json,
            demo,
            provider,
            model,
            api_base_url,
            api_key_env,
            max_output_tokens,
            temperature,
            input_cost_per_million,
            output_cost_per_million,
            max_tool_calls,
            tool_visibility,
            enable_shell,
            enable_subagent,
            load_memory,
            load_skills,
            include_ingest,
            allow_unsafe_ingest,
            refine_prompt,
            refinement_instructions,
            refinement_model,
            require_approval,
            raw_tool_output,
        } => {
            let options = setup::RuntimeOptions {
                provider,
                model,
                api_base_url,
                api_key_env,
                max_output_tokens,
                temperature,
                input_cost_per_million,
                output_cost_per_million,
                max_tool_calls,
                tool_visibility: tool_visibility.map(VisibilityLevel::from),
                enable_shell,
                enable_subagent,
                load_memory,
                load_skills,
                include_ingest,
                allow_unsafe_ingest,
                enable_prompt_refinement: refine_prompt,
                prompt_refinement_instructions: refinement_instructions,
                prompt_refinement_model: refinement_model,
                require_approval,
                raw_tool_output,
            };
            let force_headless = print || json || !std::io::stdout().is_terminal();
            if force_headless {
                headless::run(input, json, demo, options).await
            } else {
                tui::run(input, demo, options).await
            }
        }
        Command::PreviewContext {
            input,
            json,
            enable_shell,
            enable_subagent,
            max_tool_calls,
            tool_visibility,
            load_memory,
            raw_tool_output,
            load_skills,
            include_ingest,
            allow_unsafe_ingest,
        } => {
            let options = setup::RuntimeOptions {
                enable_shell,
                enable_subagent,
                max_tool_calls,
                tool_visibility: tool_visibility.map(VisibilityLevel::from),
                load_memory,
                raw_tool_output,
                load_skills,
                include_ingest,
                allow_unsafe_ingest,
                ..setup::RuntimeOptions::default()
            };
            headless::preview_context(input, json, options).await
        }
        Command::ExplainConfig { json } => headless::explain_config(json).await,
        Command::ExplainTools {
            json,
            enable_shell,
            enable_subagent,
            tool_visibility,
        } => {
            headless::explain_tools(
                json,
                enable_shell,
                enable_subagent,
                tool_visibility.map(VisibilityLevel::from),
            )
            .await
        }
        Command::Tool {
            command:
                ToolCommand::Call {
                    name,
                    input,
                    json,
                    require_approval,
                },
        } => headless::call_tool(name, input, json, require_approval).await,
        Command::Trace {
            command: TraceCommand::Show { run_id, json },
        } => headless::trace_show(run_id, json).await,
        Command::Approval { command } => match command {
            ApprovalCommand::List { run_id, json } => headless::approval_list(run_id, json).await,
            ApprovalCommand::Decide {
                run_id,
                approval_id,
                approve,
            } => headless::approval_decide(run_id, approval_id, approve).await,
            ApprovalCommand::Execute {
                run_id,
                approval_id,
                json,
            } => headless::approval_execute(run_id, approval_id, json).await,
        },
        Command::Guide { run_id, text } => headless::guide(run_id, text).await,
        Command::Cancel { run_id, reason } => headless::cancel(run_id, reason).await,
        Command::Score {
            run_id,
            score,
            target,
        } => headless::score(run_id, target, score).await,
        Command::Batch { command } => match command {
            BatchCommand::Run { items, demo, json } => headless::batch_run(items, demo, json).await,
            BatchCommand::Resume {
                batch_id,
                demo,
                json,
            } => headless::batch_resume(batch_id, demo, json).await,
        },
        Command::Memory { command } => match command {
            MemoryCommand::Create { content, user } => headless::memory_create(content, user).await,
            MemoryCommand::Generate { text, user, range } => {
                headless::memory_generate(text, user, range).await
            }
            MemoryCommand::List { json } => headless::memory_list(json).await,
            MemoryCommand::Edit { id, content } => headless::memory_edit(id, content).await,
            MemoryCommand::Delete { id } => headless::memory_delete(id).await,
            MemoryCommand::Rollback { user } => headless::memory_rollback(user).await,
        },
        Command::Skill { command } => match command {
            SkillCommand::ImportOpenclaw { path } => headless::skill_import_openclaw(path).await,
            SkillCommand::List { json } => headless::skill_list(json).await,
            SkillCommand::Inspect { id } => headless::skill_inspect(id).await,
            SkillCommand::Allow { id } => headless::skill_allow(id).await,
            SkillCommand::Quarantine { id } => headless::skill_quarantine(id).await,
        },
        Command::Prompt { command } => match command {
            PromptCommand::Save { name, text } => headless::prompt_save(name, text).await,
            PromptCommand::List { json } => headless::prompt_list(json).await,
            PromptCommand::Show { name, json } => headless::prompt_show(name, json).await,
            PromptCommand::Delete { name } => headless::prompt_delete(name).await,
        },
        Command::Model { command } => match command {
            ModelCommand::List { json } => headless::model_list(json).await,
            ModelCommand::Show { id, json } => headless::model_show(id, json).await,
            ModelCommand::Save {
                id,
                max_context_tokens,
                max_output_tokens,
                default_temperature,
                tool_support,
                privacy_level,
                cost_tier,
                input_cost_per_million,
                output_cost_per_million,
            } => {
                headless::model_save(
                    id,
                    max_context_tokens,
                    max_output_tokens,
                    default_temperature,
                    tool_support,
                    privacy_level,
                    cost_tier,
                    input_cost_per_million,
                    output_cost_per_million,
                )
                .await
            }
            ModelCommand::Delete { id } => headless::model_delete(id).await,
        },
        Command::Ingest { command } => match command {
            IngestCommand::Add { path, backend } => headless::ingest_add(path, backend).await,
            IngestCommand::Rerun { id, backend } => headless::ingest_rerun(id, backend).await,
            IngestCommand::List { json } => headless::ingest_list(json).await,
            IngestCommand::Show { id, json } => headless::ingest_show(id, json).await,
            IngestCommand::Rm { id } => headless::ingest_rm(id).await,
        },
        Command::Adapter { command } => match command {
            AdapterCommand::Import { path } => headless::adapter_import(path).await,
            AdapterCommand::List { json } => headless::adapter_list(json).await,
            AdapterCommand::Inspect { path, json } => headless::adapter_inspect(path, json).await,
            AdapterCommand::Show { id, json } => headless::adapter_show(id, json).await,
            AdapterCommand::Allow { id } => headless::adapter_allow(id).await,
            AdapterCommand::Quarantine { id } => headless::adapter_quarantine(id).await,
        },
        Command::Export { path } => headless::bundle_export(path).await,
        Command::Import { path } => headless::bundle_import(path).await,
        Command::Remote { url, command } => match command {
            RemoteCommand::Health => headless::remote_health(url).await,
            RemoteCommand::Run {
                input,
                demo,
                provider,
                model,
                api_base_url,
                api_key_env,
                input_cost_per_million,
                output_cost_per_million,
                max_tool_calls,
                tool_visibility,
                enable_shell,
                enable_subagent,
                load_memory,
                load_skills,
                include_ingest,
                allow_unsafe_ingest,
                require_approval,
                raw_tool_output,
            } => {
                let options = setup::RuntimeOptions {
                    provider,
                    model,
                    api_base_url,
                    api_key_env,
                    max_output_tokens: None,
                    temperature: None,
                    input_cost_per_million,
                    output_cost_per_million,
                    max_tool_calls,
                    tool_visibility: tool_visibility.map(VisibilityLevel::from),
                    enable_shell,
                    enable_subagent,
                    load_memory,
                    load_skills,
                    include_ingest,
                    allow_unsafe_ingest,
                    enable_prompt_refinement: false,
                    prompt_refinement_instructions: None,
                    prompt_refinement_model: None,
                    require_approval,
                    raw_tool_output,
                };
                headless::remote_run(url, input, demo, options).await
            }
            RemoteCommand::RunStart {
                input,
                demo,
                provider,
                model,
                api_base_url,
                api_key_env,
                input_cost_per_million,
                output_cost_per_million,
                max_tool_calls,
                tool_visibility,
                enable_shell,
                enable_subagent,
                load_memory,
                load_skills,
                include_ingest,
                allow_unsafe_ingest,
                require_approval,
                raw_tool_output,
            } => {
                let options = setup::RuntimeOptions {
                    provider,
                    model,
                    api_base_url,
                    api_key_env,
                    max_output_tokens: None,
                    temperature: None,
                    input_cost_per_million,
                    output_cost_per_million,
                    max_tool_calls,
                    tool_visibility: tool_visibility.map(VisibilityLevel::from),
                    enable_shell,
                    enable_subagent,
                    load_memory,
                    load_skills,
                    include_ingest,
                    allow_unsafe_ingest,
                    enable_prompt_refinement: false,
                    prompt_refinement_instructions: None,
                    prompt_refinement_model: None,
                    require_approval,
                    raw_tool_output,
                };
                headless::remote_run_start(url, input, demo, options).await
            }
            RemoteCommand::RunStatus { run_id } => headless::remote_run_status(url, run_id).await,
            RemoteCommand::PreviewContext {
                input,
                enable_shell,
                enable_subagent,
                max_tool_calls,
                tool_visibility,
                load_memory,
                raw_tool_output,
                load_skills,
                include_ingest,
                allow_unsafe_ingest,
            } => {
                let options = setup::RuntimeOptions {
                    enable_shell,
                    enable_subagent,
                    max_tool_calls,
                    tool_visibility: tool_visibility.map(VisibilityLevel::from),
                    load_memory,
                    raw_tool_output,
                    load_skills,
                    include_ingest,
                    allow_unsafe_ingest,
                    ..setup::RuntimeOptions::default()
                };
                headless::remote_preview_context(url, input, options).await
            }
            RemoteCommand::Guide { run_id, text } => {
                headless::remote_guide(url, run_id, text).await
            }
            RemoteCommand::Cancel { run_id, reason } => {
                headless::remote_cancel(url, run_id, reason).await
            }
            RemoteCommand::Score {
                run_id,
                score,
                target,
            } => headless::remote_score(url, run_id, target, score).await,
            RemoteCommand::Batch { command } => match command {
                RemoteBatchCommand::Run { items, demo } => {
                    headless::remote_batch_run(url, items, demo).await
                }
                RemoteBatchCommand::Resume { batch_id, demo } => {
                    headless::remote_batch_resume(url, batch_id, demo).await
                }
            },
            RemoteCommand::Tool {
                name,
                input,
                require_approval,
            } => headless::remote_tool(url, name, input, require_approval).await,
            RemoteCommand::Trace { run_id } => headless::remote_trace(url, run_id).await,
            RemoteCommand::Approval { command } => match command {
                RemoteApprovalCommand::List { run_id } => {
                    headless::remote_approval_list(url, run_id).await
                }
                RemoteApprovalCommand::Decide {
                    run_id,
                    approval_id,
                    approve,
                } => headless::remote_approval_decide(url, run_id, approval_id, approve).await,
                RemoteApprovalCommand::Execute {
                    run_id,
                    approval_id,
                } => headless::remote_approval_execute(url, run_id, approval_id).await,
            },
            RemoteCommand::Memory { command } => match command {
                RemoteMemoryCommand::List => headless::remote_memory_list(url).await,
                RemoteMemoryCommand::Create { content, user } => {
                    headless::remote_memory_create(url, content, user).await
                }
                RemoteMemoryCommand::Generate { text, user, range } => {
                    headless::remote_memory_generate(url, text, user, range).await
                }
                RemoteMemoryCommand::Edit { id, content } => {
                    headless::remote_memory_edit(url, id, content).await
                }
                RemoteMemoryCommand::Delete { id } => headless::remote_memory_delete(url, id).await,
                RemoteMemoryCommand::Rollback { user } => {
                    headless::remote_memory_rollback(url, user).await
                }
            },
            RemoteCommand::Skill { command } => match command {
                RemoteSkillCommand::List => headless::remote_skill_list(url).await,
                RemoteSkillCommand::ImportOpenclaw { path } => {
                    headless::remote_skill_import(url, path).await
                }
                RemoteSkillCommand::Allow { id } => {
                    headless::remote_skill_action(url, id, true).await
                }
                RemoteSkillCommand::Quarantine { id } => {
                    headless::remote_skill_action(url, id, false).await
                }
            },
            RemoteCommand::Model { command } => match command {
                RemoteModelCommand::List => headless::remote_model_list(url).await,
                RemoteModelCommand::Show { id } => headless::remote_model_show(url, id).await,
                RemoteModelCommand::Save {
                    id,
                    max_context_tokens,
                    max_output_tokens,
                    default_temperature,
                    tool_support,
                    privacy_level,
                    cost_tier,
                    input_cost_per_million,
                    output_cost_per_million,
                } => {
                    headless::remote_model_save(
                        url,
                        id,
                        max_context_tokens,
                        max_output_tokens,
                        default_temperature,
                        tool_support,
                        privacy_level,
                        cost_tier,
                        input_cost_per_million,
                        output_cost_per_million,
                    )
                    .await
                }
                RemoteModelCommand::Delete { id } => headless::remote_model_delete(url, id).await,
            },
            RemoteCommand::Ingest { command } => match command {
                RemoteIngestCommand::List => headless::remote_ingest_list(url).await,
                RemoteIngestCommand::Add { path, backend } => {
                    headless::remote_ingest_add(url, path, backend).await
                }
                RemoteIngestCommand::Rerun { id, backend } => {
                    headless::remote_ingest_rerun(url, id, backend).await
                }
                RemoteIngestCommand::Show { id } => headless::remote_ingest_show(url, id).await,
                RemoteIngestCommand::Rm { id } => headless::remote_ingest_rm(url, id).await,
            },
            RemoteCommand::Adapter { command } => match command {
                RemoteAdapterCommand::List => headless::remote_adapter_list(url).await,
                RemoteAdapterCommand::Import { path } => {
                    headless::remote_adapter_import(url, path).await
                }
                RemoteAdapterCommand::Show { id } => headless::remote_adapter_show(url, id).await,
                RemoteAdapterCommand::Allow { id } => {
                    headless::remote_adapter_action(url, id, true).await
                }
                RemoteAdapterCommand::Quarantine { id } => {
                    headless::remote_adapter_action(url, id, false).await
                }
            },
            RemoteCommand::Export { path } => headless::remote_bundle_export(url, path).await,
            RemoteCommand::Import { path } => headless::remote_bundle_import(url, path).await,
        },
    }
}
