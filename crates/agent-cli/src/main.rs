//! Shinkai CLI entry point.
//!
//! Default mode is the ratatui TUI. `--print` (or piping stdout to a non-TTY)
//! switches to headless mode for scripting / CI. See `specs/architecture.md`
//! §20.1 for the surface contract.

mod headless;
mod setup;
mod tui;

use std::io::IsTerminal;

use agent_config::{IngestionGuardrailMode, ProfileGrantKind};
use agent_core::{ToolOutputMode, VisibilityLevel};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "agent")]
#[command(about = "Shinkai CLI.", long_about = None)]
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

        /// Agent id to run. Can refer to an active-profile agent or a granted agent.
        #[arg(long)]
        agent: Option<String>,

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

        /// Trigger automatic context compaction after approximately this many conversation tokens.
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,

        /// Approximate output-token budget for automatic context compaction.
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,

        /// Guidance for automatic context compaction in this run.
        #[arg(long)]
        compaction_guidance: Option<String>,

        /// Restrict this run to one tool category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,

        /// Restrict loaded skills to one category/pack for this run. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,

        /// Override how much tool detail is shown to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Override how much skill detail is shown to the model.
        #[arg(long, value_enum)]
        skill_visibility: Option<ToolVisibility>,

        /// Explicitly register the shell tool for this run.
        #[arg(long)]
        enable_shell: bool,

        /// Explicitly register the subagent tool for this run.
        #[arg(long)]
        enable_subagent: bool,

        /// Explicitly register the quarantined capability-draft creation tool for this run.
        #[arg(long)]
        enable_capability_drafts: bool,

        /// Load file-backed memory into context for this run.
        #[arg(long)]
        load_memory: bool,

        /// Load allowed file-backed skills into context for this run.
        #[arg(long)]
        load_skills: bool,

        /// Load and append to a persisted conversation branch.
        #[arg(long)]
        conversation: Option<String>,

        /// Explicit compacted-context artifact id to include in context.
        #[arg(long = "include-compact")]
        include_compact: Option<String>,

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

        /// Keep approval-required tools gated. This is the default safe posture.
        #[arg(long)]
        require_approval: bool,

        /// Explicitly auto-approve approval-required tools for this run.
        #[arg(long, conflicts_with = "require_approval")]
        auto_approve: bool,

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

        /// Agent id to preview. Can refer to an active-profile agent or a granted agent.
        #[arg(long)]
        agent: Option<String>,

        /// Include the shell tool in the preview.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in the preview.
        #[arg(long)]
        enable_subagent: bool,

        /// Include the quarantined capability-draft creation tool in the preview.
        #[arg(long)]
        enable_capability_drafts: bool,

        /// Override the max tool-call budget in the preview.
        #[arg(long)]
        max_tool_calls: Option<u32>,

        /// Trigger automatic context compaction after approximately this many conversation tokens.
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,

        /// Approximate output-token budget for automatic context compaction.
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,

        /// Guidance for automatic context compaction in this preview.
        #[arg(long)]
        compaction_guidance: Option<String>,

        /// Restrict previewed tools to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,

        /// Restrict previewed skills to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,

        /// Override how much tool detail is shown in the preview.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Override how much skill detail is shown in the preview.
        #[arg(long, value_enum)]
        skill_visibility: Option<ToolVisibility>,

        /// Include loaded memory in the preview.
        #[arg(long)]
        load_memory: bool,

        /// Preview the raw tool-output runtime mode.
        #[arg(long)]
        raw_tool_output: bool,

        /// Include allowed skills in the preview.
        #[arg(long)]
        load_skills: bool,

        /// Load a persisted conversation branch into the preview.
        #[arg(long)]
        conversation: Option<String>,

        /// Explicit compacted-context artifact id to include in the preview.
        #[arg(long = "include-compact")]
        include_compact: Option<String>,

        /// Explicit ingestion artifact id to include in the preview. Repeatable.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,
    },
    /// Explain the effective v0 agent configuration and provenance.
    ExplainConfig {
        /// Agent id to explain. Can refer to an active-profile agent or a granted agent.
        #[arg(long)]
        agent: Option<String>,

        /// Emit JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,
    },
    /// Explain which tools are visible to the default agent.
    ExplainTools {
        /// Emit JSON instead of a human-readable table.
        #[arg(long)]
        json: bool,

        /// Agent id whose tool policy should be used.
        #[arg(long)]
        agent: Option<String>,

        /// Include the shell tool in the explanation.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in the explanation.
        #[arg(long)]
        enable_subagent: bool,

        /// Include the quarantined capability-draft creation tool in the explanation.
        #[arg(long)]
        enable_capability_drafts: bool,

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
    /// Lifecycle hook policy operations.
    Hooks {
        #[command(subcommand)]
        command: HookCommand,
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
    /// Restart a saved run trace from an event boundary.
    Resume {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Event id to resume from. Defaults to the last non-terminal event.
        #[arg(long)]
        from_event: Option<u64>,

        /// Demo provider behavior.
        #[arg(long, value_enum, default_value_t = Demo::Echo)]
        demo: Demo,

        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
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
    /// Show local harness storage footprint.
    Storage {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Secret handle operations.
    Secrets {
        #[command(subcommand)]
        command: SecretsCommand,
    },
    /// Quarantined agent-created capability draft operations.
    Capability {
        #[command(subcommand)]
        command: CapabilityCommand,
    },
    /// Deterministic batch operations.
    Batch {
        #[command(subcommand)]
        command: BatchCommand,
    },
    /// Manual context compaction operations.
    Compact {
        #[command(subcommand)]
        command: CompactCommand,
    },
    /// Branchable conversation graph operations.
    Conversation {
        #[command(subcommand)]
        command: ConversationCommand,
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
    /// Agent configuration operations.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Model registry operations.
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Profile registry operations.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Document ingestion operations.
    Ingest {
        #[command(subcommand)]
        command: IngestCommand,
    },
    /// Generated document artifact operations.
    Artifact {
        #[command(subcommand)]
        command: ArtifactCommand,
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
    /// Talk to a Shinkai daemon over HTTP.
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

        /// Keep approval-required tools gated. This is the default safe posture.
        #[arg(long)]
        require_approval: bool,

        /// Explicitly auto-approve approval-required tools for this call.
        #[arg(long, conflicts_with = "require_approval")]
        auto_approve: bool,
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
    /// Summarize tokens, cost, duration, approvals, memory, artifacts, and scores.
    Summary {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Emit a JSON summary object.
        #[arg(long)]
        json: bool,
    },
    /// Review hook failures and suggested retry/override actions.
    Hooks {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Emit a JSON remediation plan.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum HookCommand {
    /// List persisted disabled lifecycle hooks for an agent.
    List {
        /// Agent id whose layered hook policy should be shown.
        #[arg(long)]
        agent: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// List allowed lifecycle hook declarations and their policy state.
    Available {
        /// Agent id whose layered hook policy should annotate hooks.
        #[arg(long)]
        agent: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Persistently disable one lifecycle hook for the active profile.
    Disable {
        /// Full lifecycle hook id, for example `adapter:<package>:<hook>`.
        hook_id: String,

        /// Persist on this agent config instead of the active profile.
        #[arg(long)]
        agent: Option<String>,

        /// Persist the change. Without this, only a confirmation plan is printed.
        #[arg(long)]
        confirm: bool,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Re-enable one lifecycle hook in the active profile policy.
    Enable {
        /// Full lifecycle hook id, for example `adapter:<package>:<hook>`.
        hook_id: String,

        /// Persist on this agent config instead of the active profile.
        #[arg(long)]
        agent: Option<String>,

        /// Persist the change. Without this, only a confirmation plan is printed.
        #[arg(long)]
        confirm: bool,

        /// Emit JSON instead of a human-readable summary.
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
    /// Approve an approval request without executing it.
    Approve {
        /// Run UUID printed by `agent run` or an approval-required error.
        run_id: String,

        /// Approval id, for example `approval-manual-1`.
        approval_id: String,
    },
    /// Reject an approval request without executing it.
    Reject {
        /// Run UUID printed by `agent run` or an approval-required error.
        run_id: String,

        /// Approval id, for example `approval-manual-1`.
        approval_id: String,
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

        /// Conversation this memory was generated from.
        #[arg(long)]
        conversation: Option<String>,
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

        /// Conversation this memory was generated from.
        #[arg(long)]
        conversation: Option<String>,
    },
    /// List memory records.
    List {
        #[arg(long)]
        json: bool,
    },
    /// List supported memory backends.
    Backends {
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
    /// Export memory.md or user.md as portable markdown.
    Export {
        path: String,
        /// Export user.md instead of memory.md.
        #[arg(long)]
        user: bool,
        #[arg(long)]
        json: bool,
    },
    /// Import portable memory markdown into memory.md or user.md.
    Import {
        path: String,
        /// Import into user.md instead of memory.md.
        #[arg(long)]
        user: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SkillCommand {
    /// Import an OpenClaw/AgentSkills SKILL.md file or folder; starts quarantined.
    ImportOpenclaw { path: String },
    /// Install an OpenClaw/AgentSkills SKILL.md file or folder; starts quarantined.
    Install { source: String },
    /// List installed skills.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Inspect an installed skill.
    Inspect { id: String },
    /// Allow a quarantined skill into context.
    Allow {
        /// Either the skill id, or an agent id when a skill id is also provided.
        agent_or_id: String,
        /// Optional skill id for the spec shape `agent skill allow <agent> <skill>`.
        skill: Option<String>,
    },
    /// Quarantine a skill.
    Quarantine { id: String },
    /// Export one skill registry document as portable JSON.
    Export {
        id: String,
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Import one portable skill registry JSON document. Starts quarantined.
    Import {
        path: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum PromptCommand {
    /// Save or replace a prompt in the profile or per-agent library.
    Save {
        name: String,
        text: String,
        #[arg(long)]
        agent: Option<String>,
    },
    /// List saved prompts.
    List {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Show a saved prompt.
    Show {
        name: String,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Delete a saved prompt.
    Delete {
        name: String,
        #[arg(long)]
        agent: Option<String>,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum AgentCommand {
    /// List configured agents in the active profile.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show one raw agent config.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Save or replace one agent config in the active profile.
    Save {
        id: String,
        /// Human-readable agent name. Defaults to the id.
        #[arg(long)]
        name: Option<String>,
        /// System prompt for the agent.
        #[arg(long)]
        system_prompt: String,
        /// Default model id for this agent.
        #[arg(long)]
        model: Option<String>,
        /// Per-run tool-call budget for this agent.
        #[arg(long)]
        max_tool_calls: Option<u32>,
        /// Trigger automatic context compaction after approximately this many conversation tokens.
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,
        /// Approximate output-token budget for automatic context compaction.
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,
        /// Agent-level guidance for automatic context compaction.
        #[arg(long)]
        compaction_guidance: Option<String>,
        /// Maximum number of nested child-run levels this agent may spawn.
        #[arg(long)]
        max_subagent_depth: Option<u32>,
        /// Number of repeated agent-id occurrences allowed in a subagent ancestry chain.
        #[arg(long)]
        max_recursion_depth: Option<u32>,
        /// Restrict this agent to one tool id. Repeat for multiple tools.
        #[arg(long = "allow-tool")]
        allowed_tools: Vec<String>,
        /// Restrict this agent to one tool category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,
        /// Restrict loaded skills to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,
        /// Whether tool outputs are interpreted by the LLM or returned raw.
        #[arg(long, value_enum)]
        tool_output_mode: Option<ToolOutputModeArg>,
        /// Agent-level model id used for interpreted tool outputs.
        #[arg(long = "tool-output-interpretation-model")]
        tool_output_interpretation_model: Option<String>,
        /// Override one tool's output mode, as TOOL=raw or TOOL=interpreted. Repeat for multiple tools.
        #[arg(long = "tool-output-override")]
        tool_output_overrides: Vec<String>,
        /// Override one tool's output interpretation model, as TOOL=MODEL. Repeat for multiple tools.
        #[arg(long = "tool-interpretation-model")]
        tool_interpretation_model_overrides: Vec<String>,
        /// Override one tool's output interpretation guidance, as TOOL=TEXT. Repeat for multiple tools.
        #[arg(long = "tool-guidance-override")]
        tool_guidance_overrides: Vec<String>,
        /// How much tool detail is shown to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,
        /// Load this agent's memory into context by default.
        #[arg(long = "load-memory")]
        load_memory: bool,
        /// Load allowed skills into context for this agent by default.
        #[arg(long = "load-skills")]
        load_skills: bool,
        /// Prompt-injection guardrail posture for high-risk ingested content.
        #[arg(long = "ingest-guardrail", value_enum)]
        ingestion_guardrail: Option<IngestionGuardrailArg>,
        /// Optional model id used to classify ingested content for prompt-injection risk.
        #[arg(long = "ingest-guardrail-model")]
        ingestion_guardrail_model: Option<String>,
        /// Estimated input-token price in USD per 1M tokens.
        #[arg(long)]
        input_cost_per_million: Option<f64>,
        /// Estimated output-token price in USD per 1M tokens.
        #[arg(long)]
        output_cost_per_million: Option<f64>,
        /// Instructions used to refine user prompts before the main agent run.
        #[arg(long = "refinement-instructions")]
        refinement_instructions: Option<String>,
        /// Optional model id used for prompt refinement.
        #[arg(long = "refinement-model")]
        refinement_model: Option<String>,
        /// Include prompt refinement guidance in the agent's system prompt.
        #[arg(long = "refinement-aware")]
        refinement_aware: bool,
    },
    /// Delete one non-default agent config from the active profile.
    Delete { id: String },
    /// Export one agent config as portable TOML.
    Export {
        id: String,
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Import one portable TOML agent config into the active profile.
    Import {
        path: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// Show the active profile selected by AGENT_HARNESS_PROFILE, defaulting to main.
    Current {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Create a profile directory and profile.toml.
    Create {
        id: String,

        /// Human-readable profile name.
        #[arg(long)]
        name: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// List profiles.
    List {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Show one profile.
    Show {
        id: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Delete a non-main profile directory.
    Delete { id: String },
    /// Grant one profile access to a resource owned by another profile.
    Grant {
        /// Granting profile. Defaults to the active profile.
        #[arg(long)]
        from: Option<String>,

        /// Receiving profile.
        #[arg(long)]
        to: String,

        /// Resource kind being granted.
        #[arg(long, value_enum)]
        kind: ProfileGrantKindArg,

        /// Resource id, category name, or `*`.
        resource: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// List cross-profile grants.
    Grants {
        /// Only list grants from this source profile.
        #[arg(long)]
        from: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Revoke a cross-profile grant by id.
    RevokeGrant {
        id: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ProfileGrantKindArg {
    Agent,
    Memory,
    Tool,
    Skill,
    Category,
}

impl From<ProfileGrantKindArg> for ProfileGrantKind {
    fn from(value: ProfileGrantKindArg) -> Self {
        match value {
            ProfileGrantKindArg::Agent => ProfileGrantKind::Agent,
            ProfileGrantKindArg::Memory => ProfileGrantKind::Memory,
            ProfileGrantKindArg::Tool => ProfileGrantKind::Tool,
            ProfileGrantKindArg::Skill => ProfileGrantKind::Skill,
            ProfileGrantKindArg::Category => ProfileGrantKind::Category,
        }
    }
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum ModelCommand {
    /// List configured model metadata.
    List {
        #[arg(long)]
        json: bool,
    },
    /// List built-in provider defaults and capability hints.
    Providers {
        #[arg(long)]
        json: bool,
    },
    /// Show one configured model.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Probe declared and live model capabilities where the provider supports it.
    Probe {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Save or replace model metadata.
    Save {
        id: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        api_base_url: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long)]
        allow_missing_api_key: bool,
        #[arg(long)]
        max_context_tokens: Option<u64>,
        #[arg(long)]
        max_output_tokens: Option<u64>,
        #[arg(long)]
        default_temperature: Option<f64>,
        #[arg(long = "modality")]
        available_modalities: Vec<String>,
        #[arg(long)]
        reasoning_mode: Option<String>,
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
        /// Provider-specific nucleus sampling value merged into metadata.provider_options.
        #[arg(long)]
        top_p: Option<f64>,
        /// Provider-specific top-k sampling value merged into metadata.provider_options.
        #[arg(long)]
        top_k: Option<u64>,
        /// Provider-specific reasoning effort merged into metadata.provider_options.
        #[arg(long)]
        reasoning_effort: Option<String>,
        /// Arbitrary metadata as a JSON object.
        #[arg(long)]
        metadata_json: Option<String>,
    },
    /// Delete configured model metadata.
    Delete { id: String },
    /// Export one configured model as portable TOML.
    Export {
        id: String,
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Import one portable TOML model config.
    Import {
        path: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum IngestCommand {
    /// List supported ingestion backends.
    Backends {
        #[arg(long)]
        json: bool,
    },
    /// Ingest a local file explicitly.
    Add {
        path: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,

        /// Optional model id for vision/OCR extraction with local-layout-v0.
        #[arg(long)]
        vision_model: Option<String>,

        /// Optional model id for a prompt-injection guardrail classification call.
        #[arg(long)]
        guardrail_model: Option<String>,
    },
    /// Re-run ingestion for an artifact's source, optionally with another backend.
    Rerun {
        id: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,

        /// Optional model id for vision/OCR extraction with local-layout-v0.
        #[arg(long)]
        vision_model: Option<String>,

        /// Optional model id for a prompt-injection guardrail classification call.
        #[arg(long)]
        guardrail_model: Option<String>,
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
    /// Review one finding on an ingestion artifact.
    Review {
        id: String,

        /// Zero-based finding index to review.
        #[arg(long)]
        finding: u32,

        /// Review decision: acknowledge, approve/allow, or reject/block.
        #[arg(long)]
        decision: String,

        /// Optional reviewer note.
        #[arg(long)]
        note: Option<String>,
    },
    /// Remove an ingestion artifact.
    Rm { id: String },
}

#[derive(Subcommand)]
enum ArtifactCommand {
    /// List generated document artifacts.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show a generated artifact by id or filename.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Open a generated artifact in the OS default app.
    Open {
        id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AdapterCommand {
    /// Import a local adapter source into quarantine.
    Import { path: String },
    /// Search, inspect, pin, and install from a local ClawHub catalog.
    Clawhub {
        #[command(subcommand)]
        command: ClawHubCommand,
    },
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
enum ClawHubCommand {
    /// Search a local ClawHub catalog.
    Search {
        catalog: String,
        query: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Inspect one catalog entry and its normalized package.
    Inspect {
        catalog: String,
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Verify and print the digest pin for one catalog entry.
    Pin {
        catalog: String,
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Install one catalog entry into the adapter quarantine registry.
    Install { catalog: String, id: String },
}

#[derive(Subcommand)]
enum SecretsCommand {
    /// List supported secret storage backends.
    Backends {
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Store or update a secret and return an opaque handle.
    Set {
        id: String,
        /// Secret value. If omitted, stdin is consumed.
        #[arg(long)]
        value: Option<String>,
        /// Optional human label shown in metadata.
        #[arg(long)]
        label: Option<String>,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Rotate an existing secret while preserving old version handles.
    Rotate {
        id: String,
        /// New secret value. If omitted, stdin is consumed.
        #[arg(long)]
        value: Option<String>,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// List secret metadata without revealing values.
    List {
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Show one secret's metadata without revealing its value.
    Show {
        id: String,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Delete all versions of one secret.
    Delete { id: String },
}

#[derive(Subcommand)]
enum CapabilityCommand {
    /// Propose a quarantined capability draft. Body is read from stdin when omitted.
    Propose {
        /// Capability kind: tool, skill, or agent.
        #[arg(long)]
        kind: String,
        /// Human-readable draft name.
        #[arg(long)]
        name: String,
        /// Draft body/spec. If omitted, stdin is consumed.
        #[arg(long)]
        body: Option<String>,
        /// Optional creation/review guidance.
        #[arg(long)]
        guidance: Option<String>,
        /// Creator label for provenance.
        #[arg(long, default_value = "user")]
        created_by: String,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// List capability drafts.
    List {
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Show one capability draft.
    Show {
        id: String,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Mark a quarantined draft allowed after review.
    Allow {
        id: String,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Mark a draft rejected after review.
    Reject {
        id: String,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Delete a capability draft.
    Delete { id: String },
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
enum CompactCommand {
    /// Create a portable compacted-context artifact from selected text.
    Create {
        /// Text to compact. If omitted, stdin is consumed.
        #[arg(short, long)]
        input: Option<String>,

        /// Guidance describing what to keep/drop in the compacted context.
        #[arg(long)]
        guidance: Option<String>,

        /// Source label, for example a conversation id or selected range.
        #[arg(long)]
        source: Option<String>,

        /// Conversation this compaction was generated from.
        #[arg(long)]
        conversation: Option<String>,

        /// Approximate max output tokens for the compacted artifact.
        #[arg(long, default_value_t = 512)]
        max_output_tokens: u32,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Create a compacted-context artifact from a conversation message range.
    Conversation {
        id: String,

        /// First expanded message index to include. Defaults to 0.
        #[arg(long)]
        from: Option<usize>,

        /// Last expanded message index to include. Defaults to the final message.
        #[arg(long)]
        to: Option<usize>,

        /// Guidance describing what to keep/drop in the compacted context.
        #[arg(long)]
        guidance: Option<String>,

        /// Approximate max output tokens for the compacted artifact.
        #[arg(long, default_value_t = 512)]
        max_output_tokens: u32,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Persist an already-compacted context artifact without rewriting it.
    Keep {
        /// Compacted context text. If omitted, stdin is consumed.
        #[arg(short, long)]
        input: Option<String>,

        /// Guidance associated with the compacted context.
        #[arg(long)]
        guidance: Option<String>,

        /// Source label, for example auto-preview or run id.
        #[arg(long)]
        source: Option<String>,

        /// Conversation this compaction should be linked to.
        #[arg(long)]
        conversation: Option<String>,

        /// Approximate max output tokens represented by this compacted artifact.
        #[arg(long)]
        max_output_tokens: Option<u32>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Keep the auto-compacted context emitted by a completed run.
    KeepRun {
        /// Run id that emitted an auto-compacted ContextBuilt event.
        run_id: String,

        /// Conversation this compaction should be linked to.
        #[arg(long)]
        conversation: Option<String>,

        /// Guidance associated with the compacted context.
        #[arg(long)]
        guidance: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// List compacted-context artifacts.
    List {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Show one compacted-context artifact.
    Show {
        id: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Export one compacted-context artifact as portable JSON.
    Export {
        id: String,

        /// Destination JSON path.
        #[arg(short, long)]
        path: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Import one portable compacted-context artifact JSON file.
    Import {
        /// Source JSON path.
        path: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Remove a compacted-context artifact.
    Rm { id: String },
}

#[derive(Subcommand)]
enum ConversationCommand {
    /// Create a new root conversation branch.
    Create {
        /// Human-readable title.
        #[arg(long)]
        title: Option<String>,

        /// Owning agent id.
        #[arg(long)]
        agent: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// List conversation branches.
    List {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Append one message to a conversation branch.
    AddMessage {
        id: String,

        #[arg(long, value_enum)]
        role: MessageRole,

        content: String,
    },
    /// Create a child branch after the given expanded message count.
    Branch {
        id: String,

        /// Number of expanded parent messages to retain before divergence.
        #[arg(long)]
        at: usize,

        /// Child branch title.
        #[arg(long)]
        title: Option<String>,

        /// Reason/topic for the divergence.
        #[arg(long)]
        reason: Option<String>,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Show one branch and its expanded inherited messages.
    Show {
        id: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Print the conversation branch tree.
    Tree {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Build a recovery plan from linked compactions and memories.
    Recover {
        id: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Delete a branch. Use --recursive to delete its child branches too.
    Delete {
        id: String,

        #[arg(long)]
        recursive: bool,

        /// Compact the expanded conversation before deleting and keep that artifact.
        #[arg(long)]
        compact_first: bool,

        /// Guidance for --compact-first.
        #[arg(long)]
        compact_guidance: Option<String>,

        /// Approximate max output tokens for --compact-first.
        #[arg(long, default_value_t = 512)]
        compact_max_output_tokens: u32,

        /// Generate memory from the expanded conversation before deleting and keep it.
        #[arg(long)]
        memory_first: bool,

        /// Store --memory-first output in user.md instead of memory.md.
        #[arg(long)]
        memory_user: bool,
    },
    /// Delete an inclusive expanded message-index range from a leaf branch.
    DeleteRange {
        id: String,

        /// First expanded message index to delete.
        #[arg(long)]
        from: usize,

        /// Last expanded message index to delete.
        #[arg(long)]
        to: usize,
    },
    /// Delete all conversation branches owned by one agent id.
    DeleteAgent {
        agent: String,

        #[arg(long)]
        recursive: bool,

        /// Compact each deleted conversation before deleting and keep those artifacts.
        #[arg(long)]
        compact_first: bool,

        /// Guidance for --compact-first.
        #[arg(long)]
        compact_guidance: Option<String>,

        /// Approximate max output tokens for --compact-first.
        #[arg(long, default_value_t = 512)]
        compact_max_output_tokens: u32,

        /// Generate memory from each deleted conversation before deleting and keep it.
        #[arg(long)]
        memory_first: bool,

        /// Store --memory-first output in user.md instead of memory.md.
        #[arg(long)]
        memory_user: bool,
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

        /// Agent id to run. Can refer to a daemon active-profile agent or granted agent.
        #[arg(long)]
        agent: Option<String>,

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

        /// Trigger daemon automatic context compaction after approximately this many conversation tokens.
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,

        /// Approximate output-token budget for daemon automatic context compaction.
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,

        /// Guidance for daemon automatic context compaction.
        #[arg(long)]
        compaction_guidance: Option<String>,

        /// Restrict daemon-visible tools to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,

        /// Restrict daemon-loaded skills to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,

        /// Override how much tool detail the daemon shows to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Override how much skill detail the daemon shows to the model.
        #[arg(long, value_enum)]
        skill_visibility: Option<ToolVisibility>,

        /// Include the shell tool in daemon context.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in daemon context.
        #[arg(long)]
        enable_subagent: bool,

        /// Include the quarantined capability-draft creation tool in daemon context.
        #[arg(long)]
        enable_capability_drafts: bool,

        /// Load file-backed memory in daemon context.
        #[arg(long)]
        load_memory: bool,

        /// Load allowed skills in daemon context.
        #[arg(long)]
        load_skills: bool,

        /// Explicit compacted-context artifact id to include in daemon context.
        #[arg(long = "include-compact")]
        include_compact: Option<String>,

        /// Load a persisted conversation branch into daemon context.
        #[arg(long)]
        conversation: Option<String>,

        /// Explicit ingestion artifact id to include in daemon context.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,

        /// Rewrite the user prompt in a daemon preprocessing LLM call before the main run.
        #[arg(long)]
        refine_prompt: bool,

        /// Instructions for the daemon prompt refinement preprocessing call.
        #[arg(long)]
        refinement_instructions: Option<String>,

        /// Optional model id for daemon prompt refinement. Defaults to the agent model.
        #[arg(long)]
        refinement_model: Option<String>,

        /// Keep approval-required daemon tools gated. This is the default safe posture.
        #[arg(long)]
        require_approval: bool,

        /// Explicitly auto-approve approval-required daemon tools.
        #[arg(long, conflicts_with = "require_approval")]
        auto_approve: bool,

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

        /// Agent id to run. Can refer to a daemon active-profile agent or granted agent.
        #[arg(long)]
        agent: Option<String>,

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

        /// Trigger daemon automatic context compaction after approximately this many conversation tokens.
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,

        /// Approximate output-token budget for daemon automatic context compaction.
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,

        /// Guidance for daemon automatic context compaction.
        #[arg(long)]
        compaction_guidance: Option<String>,

        /// Restrict daemon-visible tools to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,

        /// Restrict daemon-loaded skills to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,

        /// Override how much tool detail the daemon shows to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Override how much skill detail the daemon shows to the model.
        #[arg(long, value_enum)]
        skill_visibility: Option<ToolVisibility>,

        /// Include the shell tool in daemon context.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in daemon context.
        #[arg(long)]
        enable_subagent: bool,

        /// Include the quarantined capability-draft creation tool in daemon context.
        #[arg(long)]
        enable_capability_drafts: bool,

        /// Load file-backed memory in daemon context.
        #[arg(long)]
        load_memory: bool,

        /// Load allowed skills in daemon context.
        #[arg(long)]
        load_skills: bool,

        /// Explicit compacted-context artifact id to include in daemon context.
        #[arg(long = "include-compact")]
        include_compact: Option<String>,

        /// Load a persisted conversation branch into daemon context.
        #[arg(long)]
        conversation: Option<String>,

        /// Explicit ingestion artifact id to include in daemon context.
        #[arg(long = "include-ingest")]
        include_ingest: Vec<String>,

        /// Include high-risk ingestion artifacts that guardrails would otherwise withhold.
        #[arg(long)]
        allow_unsafe_ingest: bool,

        /// Rewrite the user prompt in a daemon preprocessing LLM call before the main run.
        #[arg(long)]
        refine_prompt: bool,

        /// Instructions for the daemon prompt refinement preprocessing call.
        #[arg(long)]
        refinement_instructions: Option<String>,

        /// Optional model id for daemon prompt refinement. Defaults to the agent model.
        #[arg(long)]
        refinement_model: Option<String>,

        /// Keep approval-required daemon tools gated. This is the default safe posture.
        #[arg(long)]
        require_approval: bool,

        /// Explicitly auto-approve approval-required daemon tools.
        #[arg(long, conflicts_with = "require_approval")]
        auto_approve: bool,

        /// Return the first daemon tool output without an interpretation pass.
        #[arg(long)]
        raw_tool_output: bool,
    },
    /// Show async daemon run status.
    RunStatus { run_id: String },
    /// Show daemon run events after an optional event id.
    RunEvents {
        run_id: String,

        #[arg(long)]
        after: Option<u64>,
    },
    /// Show the exact context snapshot the daemon would build.
    PreviewContext {
        #[arg(short, long)]
        input: String,

        /// Agent id to preview. Can refer to a daemon active-profile agent or granted agent.
        #[arg(long)]
        agent: Option<String>,

        /// Include the shell tool in the preview.
        #[arg(long)]
        enable_shell: bool,

        /// Include the subagent tool in the preview.
        #[arg(long)]
        enable_subagent: bool,

        /// Include the quarantined capability-draft creation tool in the preview.
        #[arg(long)]
        enable_capability_drafts: bool,

        /// Override the max tool-call budget in the preview.
        #[arg(long)]
        max_tool_calls: Option<u32>,

        /// Trigger daemon automatic context compaction after approximately this many conversation tokens.
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,

        /// Approximate output-token budget for daemon automatic context compaction.
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,

        /// Guidance for daemon automatic context compaction.
        #[arg(long)]
        compaction_guidance: Option<String>,

        /// Restrict previewed daemon tools to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,

        /// Restrict previewed daemon skills to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,

        /// Override how much tool detail is shown in the preview.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,

        /// Override how much skill detail is shown in the preview.
        #[arg(long, value_enum)]
        skill_visibility: Option<ToolVisibility>,

        /// Include loaded memory in the preview.
        #[arg(long)]
        load_memory: bool,

        /// Preview the raw tool-output runtime mode.
        #[arg(long)]
        raw_tool_output: bool,

        /// Include allowed skills in the preview.
        #[arg(long)]
        load_skills: bool,

        /// Explicit compacted-context artifact id to include in the preview.
        #[arg(long = "include-compact")]
        include_compact: Option<String>,

        /// Load a persisted conversation branch into the preview.
        #[arg(long)]
        conversation: Option<String>,

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
    /// Restart a saved remote run trace from an event boundary.
    Resume {
        run_id: String,

        #[arg(long)]
        from_event: Option<u64>,

        #[arg(long, value_enum, default_value_t = Demo::Echo)]
        demo: Demo,
    },
    /// Score a remote run output or step.
    Score {
        run_id: String,
        score: f32,

        #[arg(long, default_value = "last_answer")]
        target: String,
    },
    /// Show daemon host storage footprint.
    Storage,
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

        /// Keep approval-required daemon tools gated. This is the default safe posture.
        #[arg(long)]
        require_approval: bool,

        /// Explicitly auto-approve approval-required daemon tools.
        #[arg(long, conflicts_with = "require_approval")]
        auto_approve: bool,
    },
    /// Show daemon trace events.
    Trace { run_id: String },
    /// Show daemon trace summary counters.
    TraceSummary { run_id: String },
    /// Show daemon hook remediation plan.
    TraceHooks { run_id: String },
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
    /// Remote agent-created capability draft operations.
    Capability {
        #[command(subcommand)]
        command: RemoteCapabilityCommand,
    },
    /// Remote saved prompt library operations.
    Prompt {
        #[command(subcommand)]
        command: RemotePromptCommand,
    },
    /// Remote agent configuration operations.
    Agent {
        #[command(subcommand)]
        command: RemoteAgentCommand,
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
    /// Remote generated document artifact operations.
    Artifact {
        #[command(subcommand)]
        command: RemoteArtifactCommand,
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
    Approve {
        run_id: String,
        approval_id: String,
    },
    Reject {
        run_id: String,
        approval_id: String,
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
    Backends,
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
    Export {
        path: String,
        #[arg(long)]
        user: bool,
    },
    Import {
        path: String,
        #[arg(long)]
        user: bool,
    },
}

#[derive(Subcommand)]
enum RemoteSkillCommand {
    List,
    ImportOpenclaw {
        path: String,
    },
    Install {
        source: String,
    },
    Allow {
        agent_or_id: String,
        skill: Option<String>,
    },
    Quarantine {
        id: String,
    },
    Export {
        id: String,
        path: String,
    },
    Import {
        path: String,
    },
}

#[derive(Subcommand)]
enum RemoteCapabilityCommand {
    /// Propose a quarantined capability draft on the daemon host.
    Propose {
        #[arg(long)]
        kind: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        guidance: Option<String>,
        #[arg(long, default_value = "user")]
        created_by: String,
    },
    /// List daemon capability drafts.
    List,
    /// Show one daemon capability draft.
    Show { id: String },
    /// Mark a daemon capability draft allowed after review.
    Allow { id: String },
    /// Mark a daemon capability draft rejected after review.
    Reject { id: String },
    /// Delete a daemon capability draft.
    Delete { id: String },
}

#[derive(Subcommand)]
enum RemotePromptCommand {
    Save {
        name: String,
        text: String,
        #[arg(long)]
        agent: Option<String>,
    },
    List {
        #[arg(long)]
        agent: Option<String>,
    },
    Show {
        name: String,
        #[arg(long)]
        agent: Option<String>,
    },
    Delete {
        name: String,
        #[arg(long)]
        agent: Option<String>,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum RemoteAgentCommand {
    List,
    Show {
        id: String,
    },
    Save {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        system_prompt: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        max_tool_calls: Option<u32>,
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,
        #[arg(long)]
        compaction_guidance: Option<String>,
        #[arg(long)]
        max_subagent_depth: Option<u32>,
        #[arg(long)]
        max_recursion_depth: Option<u32>,
        #[arg(long = "allow-tool")]
        allowed_tools: Vec<String>,
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,
        #[arg(long, value_enum)]
        tool_output_mode: Option<ToolOutputModeArg>,
        #[arg(long = "tool-output-interpretation-model")]
        tool_output_interpretation_model: Option<String>,
        #[arg(long = "tool-output-override")]
        tool_output_overrides: Vec<String>,
        #[arg(long = "tool-interpretation-model")]
        tool_interpretation_model_overrides: Vec<String>,
        #[arg(long = "tool-guidance-override")]
        tool_guidance_overrides: Vec<String>,
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,
        #[arg(long = "load-memory")]
        load_memory: bool,
        #[arg(long = "load-skills")]
        load_skills: bool,
        #[arg(long = "ingest-guardrail", value_enum)]
        ingestion_guardrail: Option<IngestionGuardrailArg>,
        #[arg(long = "ingest-guardrail-model")]
        ingestion_guardrail_model: Option<String>,
        #[arg(long)]
        input_cost_per_million: Option<f64>,
        #[arg(long)]
        output_cost_per_million: Option<f64>,
        #[arg(long = "refinement-instructions")]
        refinement_instructions: Option<String>,
        #[arg(long = "refinement-model")]
        refinement_model: Option<String>,
        #[arg(long = "refinement-aware")]
        refinement_aware: bool,
    },
    Delete {
        id: String,
    },
    Export {
        id: String,
        path: String,
    },
    Import {
        path: String,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum RemoteModelCommand {
    List,
    Providers,
    Show {
        id: String,
    },
    Probe {
        id: String,
    },
    Save {
        id: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        api_base_url: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        #[arg(long)]
        allow_missing_api_key: bool,
        #[arg(long)]
        max_context_tokens: Option<u64>,
        #[arg(long)]
        max_output_tokens: Option<u64>,
        #[arg(long)]
        default_temperature: Option<f64>,
        #[arg(long = "modality")]
        available_modalities: Vec<String>,
        #[arg(long)]
        reasoning_mode: Option<String>,
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
        /// Provider-specific nucleus sampling value merged into metadata.provider_options.
        #[arg(long)]
        top_p: Option<f64>,
        /// Provider-specific top-k sampling value merged into metadata.provider_options.
        #[arg(long)]
        top_k: Option<u64>,
        /// Provider-specific reasoning effort merged into metadata.provider_options.
        #[arg(long)]
        reasoning_effort: Option<String>,
        /// Arbitrary metadata as a JSON object.
        #[arg(long)]
        metadata_json: Option<String>,
    },
    Delete {
        id: String,
    },
    Export {
        id: String,
        path: String,
    },
    Import {
        path: String,
    },
}

#[derive(Subcommand)]
enum RemoteIngestCommand {
    Backends,
    List,
    Add {
        path: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,

        /// Optional model id for vision/OCR extraction with local-layout-v0.
        #[arg(long)]
        vision_model: Option<String>,

        /// Optional model id for a prompt-injection guardrail classification call.
        #[arg(long)]
        guardrail_model: Option<String>,
    },
    Rerun {
        id: String,

        /// Ingestion backend id.
        #[arg(long, default_value = "local-v0")]
        backend: String,

        /// Optional model id for vision/OCR extraction with local-layout-v0.
        #[arg(long)]
        vision_model: Option<String>,

        /// Optional model id for a prompt-injection guardrail classification call.
        #[arg(long)]
        guardrail_model: Option<String>,
    },
    Show {
        id: String,
    },
    Review {
        id: String,

        /// Zero-based finding index to review.
        #[arg(long)]
        finding: u32,

        /// Review decision: acknowledge, approve/allow, or reject/block.
        #[arg(long)]
        decision: String,

        /// Optional reviewer note.
        #[arg(long)]
        note: Option<String>,
    },
    Rm {
        id: String,
    },
}

#[derive(Subcommand)]
enum RemoteArtifactCommand {
    List,
    Show { id: String },
    Open { id: String },
}

#[derive(Subcommand)]
enum RemoteAdapterCommand {
    List,
    Import {
        path: String,
    },
    Clawhub {
        #[command(subcommand)]
        command: ClawHubCommand,
    },
    Show {
        id: String,
    },
    Allow {
        id: String,
    },
    Quarantine {
        id: String,
    },
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
    /// Ollama's OpenAI-compatible local endpoint.
    Ollama,
    /// llama.cpp server's OpenAI-compatible local endpoint.
    LlamaCpp,
    /// Native Anthropic Messages API via rig-core.
    Anthropic,
    /// Native Google Gemini GenerateContent API via rig-core.
    Gemini,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl From<MessageRole> for agent_conversations::ConversationRole {
    fn from(value: MessageRole) -> Self {
        match value {
            MessageRole::System => agent_conversations::ConversationRole::System,
            MessageRole::User => agent_conversations::ConversationRole::User,
            MessageRole::Assistant => agent_conversations::ConversationRole::Assistant,
            MessageRole::Tool => agent_conversations::ConversationRole::Tool,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ToolVisibility {
    FullSchema,
    NameAndDescription,
    NameOnly,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ToolOutputModeArg {
    Interpreted,
    Raw,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum IngestionGuardrailArg {
    Block,
    Warn,
    Allow,
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

impl From<ToolOutputModeArg> for ToolOutputMode {
    fn from(value: ToolOutputModeArg) -> Self {
        match value {
            ToolOutputModeArg::Interpreted => ToolOutputMode::Interpreted,
            ToolOutputModeArg::Raw => ToolOutputMode::Raw,
        }
    }
}

impl From<IngestionGuardrailArg> for IngestionGuardrailMode {
    fn from(value: IngestionGuardrailArg) -> Self {
        match value {
            IngestionGuardrailArg::Block => IngestionGuardrailMode::Block,
            IngestionGuardrailArg::Warn => IngestionGuardrailMode::Warn,
            IngestionGuardrailArg::Allow => IngestionGuardrailMode::Allow,
        }
    }
}

#[cfg(test)]
mod cli_parse_tests {
    use super::*;

    #[test]
    fn skill_install_alias_matches_first_commands_spec() {
        let cli = Cli::try_parse_from(["agent", "skill", "install", "./SKILL.md"]).unwrap();
        let Command::Skill {
            command: SkillCommand::Install { source },
        } = cli.command
        else {
            panic!("expected skill install command");
        };
        assert_eq!(source, "./SKILL.md");
    }

    #[test]
    fn skill_allow_accepts_agent_scoped_spec_shape() {
        let cli =
            Cli::try_parse_from(["agent", "skill", "allow", "research-agent", "skill-readme"])
                .unwrap();
        let Command::Skill {
            command: SkillCommand::Allow { agent_or_id, skill },
        } = cli.command
        else {
            panic!("expected skill allow command");
        };
        assert_eq!(agent_or_id, "research-agent");
        assert_eq!(skill.as_deref(), Some("skill-readme"));
    }

    #[test]
    fn trace_hooks_command_parses() {
        let cli = Cli::try_parse_from([
            "agent",
            "trace",
            "hooks",
            "00000000-0000-0000-0000-000000000000",
            "--json",
        ])
        .unwrap();
        let Command::Trace {
            command: TraceCommand::Hooks { run_id, json },
        } = cli.command
        else {
            panic!("expected trace hooks command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert!(json);
    }

    #[test]
    fn hooks_disable_command_requires_confirm_flag() {
        let cli = Cli::try_parse_from([
            "agent",
            "hooks",
            "disable",
            "adapter:pkg:audit",
            "--confirm",
            "--json",
        ])
        .unwrap();
        let Command::Hooks {
            command:
                HookCommand::Disable {
                    hook_id,
                    agent,
                    confirm,
                    json,
                },
        } = cli.command
        else {
            panic!("expected hook disable command");
        };
        assert_eq!(hook_id, "adapter:pkg:audit");
        assert_eq!(agent, None);
        assert!(confirm);
        assert!(json);
    }

    #[test]
    fn hooks_disable_command_accepts_agent_scope() {
        let cli = Cli::try_parse_from([
            "agent",
            "hooks",
            "disable",
            "adapter:pkg:audit",
            "--agent",
            "critic",
            "--confirm",
        ])
        .unwrap();
        let Command::Hooks {
            command:
                HookCommand::Disable {
                    hook_id,
                    agent,
                    confirm,
                    ..
                },
        } = cli.command
        else {
            panic!("expected hook disable command");
        };
        assert_eq!(hook_id, "adapter:pkg:audit");
        assert_eq!(agent.as_deref(), Some("critic"));
        assert!(confirm);
    }

    #[test]
    fn hooks_available_command_accepts_agent_scope() {
        let cli =
            Cli::try_parse_from(["agent", "hooks", "available", "--agent", "critic", "--json"])
                .unwrap();
        let Command::Hooks {
            command: HookCommand::Available { agent, json },
        } = cli.command
        else {
            panic!("expected hook available command");
        };
        assert_eq!(agent.as_deref(), Some("critic"));
        assert!(json);
    }

    #[test]
    fn agent_save_accepts_portable_agent_config_shape() {
        let cli = Cli::try_parse_from([
            "agent",
            "agent",
            "save",
            "critic",
            "--name",
            "Critic",
            "--system-prompt",
            "Review carefully.",
            "--model",
            "fake-model",
            "--max-tool-calls",
            "1",
            "--max-tokens-before-compaction",
            "128",
            "--max-compaction-output-tokens",
            "48",
            "--compaction-guidance",
            "Keep decisions.",
            "--max-subagent-depth",
            "2",
            "--max-recursion-depth",
            "1",
            "--allow-tool",
            "echo",
            "--tool-output-mode",
            "raw",
            "--tool-output-interpretation-model",
            "general-interpreter",
            "--tool-output-override",
            "echo=raw",
            "--tool-interpretation-model",
            "echo=echo-interpreter",
            "--tool-guidance-override",
            "echo=Return exact echo JSON.",
            "--tool-visibility",
            "name-only",
            "--load-memory",
            "--load-skills",
            "--ingest-guardrail",
            "warn",
            "--ingest-guardrail-model",
            "guardrail-model",
            "--refinement-instructions",
            "Clarify first.",
            "--refinement-model",
            "fake-refiner",
            "--refinement-aware",
        ])
        .unwrap();
        let Command::Agent {
            command:
                AgentCommand::Save {
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
                    refinement_instructions,
                    refinement_model,
                    refinement_aware,
                    ..
                },
        } = cli.command
        else {
            panic!("expected agent save command");
        };
        assert_eq!(id, "critic");
        assert_eq!(name.as_deref(), Some("Critic"));
        assert_eq!(system_prompt, "Review carefully.");
        assert_eq!(model.as_deref(), Some("fake-model"));
        assert_eq!(max_tool_calls, Some(1));
        assert_eq!(max_tokens_before_compaction, Some(128));
        assert_eq!(max_compaction_output_tokens, Some(48));
        assert_eq!(compaction_guidance.as_deref(), Some("Keep decisions."));
        assert_eq!(max_subagent_depth, Some(2));
        assert_eq!(max_recursion_depth, Some(1));
        assert_eq!(allowed_tools, vec!["echo"]);
        assert!(matches!(tool_output_mode, Some(ToolOutputModeArg::Raw)));
        assert_eq!(
            tool_output_interpretation_model.as_deref(),
            Some("general-interpreter")
        );
        assert_eq!(tool_output_overrides, vec!["echo=raw"]);
        assert_eq!(
            tool_interpretation_model_overrides,
            vec!["echo=echo-interpreter"]
        );
        assert_eq!(
            tool_guidance_overrides,
            vec!["echo=Return exact echo JSON."]
        );
        assert!(matches!(tool_visibility, Some(ToolVisibility::NameOnly)));
        assert!(load_memory);
        assert!(load_skills);
        assert!(matches!(
            ingestion_guardrail,
            Some(IngestionGuardrailArg::Warn)
        ));
        assert_eq!(
            ingestion_guardrail_model.as_deref(),
            Some("guardrail-model")
        );
        assert_eq!(refinement_instructions.as_deref(), Some("Clarify first."));
        assert_eq!(refinement_model.as_deref(), Some("fake-refiner"));
        assert!(refinement_aware);
    }

    #[test]
    fn remote_agent_save_matches_local_agent_config_shape() {
        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "agent",
            "save",
            "critic",
            "--system-prompt",
            "Review carefully.",
            "--max-tokens-before-compaction",
            "256",
            "--max-compaction-output-tokens",
            "96",
            "--compaction-guidance",
            "Keep facts.",
            "--max-subagent-depth",
            "2",
            "--max-recursion-depth",
            "1",
            "--allow-tool",
            "echo",
            "--tool-output-override",
            "echo=interpreted",
            "--tool-interpretation-model",
            "echo=echo-interpreter",
            "--load-memory",
            "--ingest-guardrail",
            "allow",
            "--ingest-guardrail-model",
            "guardrail-model",
            "--refinement-instructions",
            "Clarify first.",
            "--refinement-aware",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Agent {
                    command:
                        RemoteAgentCommand::Save {
                            id,
                            system_prompt,
                            max_tokens_before_compaction,
                            max_compaction_output_tokens,
                            compaction_guidance,
                            max_subagent_depth,
                            max_recursion_depth,
                            allowed_tools,
                            tool_output_overrides,
                            tool_interpretation_model_overrides,
                            load_memory,
                            ingestion_guardrail,
                            ingestion_guardrail_model,
                            refinement_instructions,
                            refinement_aware,
                            ..
                        },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote agent save command");
        };
        assert_eq!(id, "critic");
        assert_eq!(system_prompt, "Review carefully.");
        assert_eq!(max_tokens_before_compaction, Some(256));
        assert_eq!(max_compaction_output_tokens, Some(96));
        assert_eq!(compaction_guidance.as_deref(), Some("Keep facts."));
        assert_eq!(max_subagent_depth, Some(2));
        assert_eq!(max_recursion_depth, Some(1));
        assert_eq!(allowed_tools, vec!["echo"]);
        assert_eq!(tool_output_overrides, vec!["echo=interpreted"]);
        assert_eq!(
            tool_interpretation_model_overrides,
            vec!["echo=echo-interpreter"]
        );
        assert!(load_memory);
        assert!(matches!(
            ingestion_guardrail,
            Some(IngestionGuardrailArg::Allow)
        ));
        assert_eq!(
            ingestion_guardrail_model.as_deref(),
            Some("guardrail-model")
        );
        assert_eq!(refinement_instructions.as_deref(), Some("Clarify first."));
        assert!(refinement_aware);
    }

    #[test]
    fn remote_run_accepts_prompt_refinement_flags() {
        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "run",
            "--input",
            "hello",
            "--allow-tool-category",
            "mcp",
            "--allow-skill-category",
            "review",
            "--max-tokens-before-compaction",
            "512",
            "--max-compaction-output-tokens",
            "120",
            "--compaction-guidance",
            "Keep open tasks.",
            "--conversation",
            "conv-1",
            "--refine-prompt",
            "--refinement-instructions",
            "Clarify first.",
            "--refinement-model",
            "fake-refiner",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Run {
                    allowed_tool_categories,
                    allowed_skill_categories,
                    max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    compaction_guidance,
                    conversation,
                    refine_prompt,
                    refinement_instructions,
                    refinement_model,
                    ..
                },
            ..
        } = cli.command
        else {
            panic!("expected remote run command");
        };
        assert_eq!(allowed_tool_categories, vec!["mcp"]);
        assert_eq!(allowed_skill_categories, vec!["review"]);
        assert_eq!(max_tokens_before_compaction, Some(512));
        assert_eq!(max_compaction_output_tokens, Some(120));
        assert_eq!(compaction_guidance.as_deref(), Some("Keep open tasks."));
        assert_eq!(conversation.as_deref(), Some("conv-1"));
        assert!(refine_prompt);
        assert_eq!(refinement_instructions.as_deref(), Some("Clarify first."));
        assert_eq!(refinement_model.as_deref(), Some("fake-refiner"));
    }

    #[test]
    fn compact_keep_command_preserves_compacted_artifact_shape() {
        let cli = Cli::try_parse_from([
            "agent",
            "compact",
            "keep",
            "--input",
            "<auto-compaction>summary</auto-compaction>",
            "--guidance",
            "Keep decisions.",
            "--source",
            "auto-preview",
            "--conversation",
            "conv-1",
            "--max-output-tokens",
            "96",
            "--json",
        ])
        .unwrap();
        let Command::Compact {
            command:
                CompactCommand::Keep {
                    input,
                    guidance,
                    source,
                    conversation,
                    max_output_tokens,
                    json,
                },
        } = cli.command
        else {
            panic!("expected compact keep command");
        };
        assert_eq!(
            input.as_deref(),
            Some("<auto-compaction>summary</auto-compaction>")
        );
        assert_eq!(guidance.as_deref(), Some("Keep decisions."));
        assert_eq!(source.as_deref(), Some("auto-preview"));
        assert_eq!(conversation.as_deref(), Some("conv-1"));
        assert_eq!(max_output_tokens, Some(96));
        assert!(json);
    }

    #[test]
    fn compact_keep_run_command_parses() {
        let cli = Cli::try_parse_from([
            "agent",
            "compact",
            "keep-run",
            "00000000-0000-0000-0000-000000000001",
            "--conversation",
            "conv-1",
            "--guidance",
            "Keep decisions.",
            "--json",
        ])
        .unwrap();
        let Command::Compact {
            command:
                CompactCommand::KeepRun {
                    run_id,
                    conversation,
                    guidance,
                    json,
                },
        } = cli.command
        else {
            panic!("expected compact keep-run command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(conversation.as_deref(), Some("conv-1"));
        assert_eq!(guidance.as_deref(), Some("Keep decisions."));
        assert!(json);
    }

    #[test]
    fn compact_export_import_commands_parse() {
        let export_cli = Cli::try_parse_from([
            "agent",
            "compact",
            "export",
            "compact-1",
            "--path",
            "/tmp/compact-1.json",
            "--json",
        ])
        .unwrap();
        let Command::Compact {
            command: CompactCommand::Export { id, path, json },
        } = export_cli.command
        else {
            panic!("expected compact export command");
        };
        assert_eq!(id, "compact-1");
        assert_eq!(path, "/tmp/compact-1.json");
        assert!(json);

        let import_cli = Cli::try_parse_from([
            "agent",
            "compact",
            "import",
            "/tmp/compact-1.json",
            "--json",
        ])
        .unwrap();
        let Command::Compact {
            command: CompactCommand::Import { path, json },
        } = import_cli.command
        else {
            panic!("expected compact import command");
        };
        assert_eq!(path, "/tmp/compact-1.json");
        assert!(json);
    }

    #[test]
    fn local_provider_aliases_parse() {
        let cli = Cli::try_parse_from([
            "agent",
            "run",
            "--provider",
            "ollama",
            "--model",
            "llama3.1",
            "--input",
            "hello",
        ])
        .unwrap();
        let Command::Run {
            provider, model, ..
        } = cli.command
        else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::Ollama));
        assert_eq!(model.as_deref(), Some("llama3.1"));

        let cli = Cli::try_parse_from([
            "agent",
            "run",
            "--provider",
            "llama-cpp",
            "--input",
            "hello",
        ])
        .unwrap();
        let Command::Run { provider, .. } = cli.command else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::LlamaCpp));

        let cli = Cli::try_parse_from([
            "agent",
            "run",
            "--provider",
            "anthropic",
            "--input",
            "hello",
        ])
        .unwrap();
        let Command::Run { provider, .. } = cli.command else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::Anthropic));

        let cli = Cli::try_parse_from(["agent", "run", "--provider", "gemini", "--input", "hello"])
            .unwrap();
        let Command::Run { provider, .. } = cli.command else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::Gemini));
    }

    #[test]
    fn remote_run_events_accepts_after_cursor() {
        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "run-events",
            "00000000-0000-0000-0000-000000000000",
            "--after",
            "4",
        ])
        .unwrap();
        let Command::Remote {
            command: RemoteCommand::RunEvents { run_id, after },
            ..
        } = cli.command
        else {
            panic!("expected remote run-events command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert_eq!(after, Some(4));
    }

    #[test]
    fn conversation_recover_command_parses() {
        let cli = Cli::try_parse_from([
            "agent",
            "conversation",
            "recover",
            "conversation-1",
            "--json",
        ])
        .unwrap();
        let Command::Conversation {
            command: ConversationCommand::Recover { id, json },
        } = cli.command
        else {
            panic!("expected conversation recover command");
        };
        assert_eq!(id, "conversation-1");
        assert!(json);
    }

    #[test]
    fn model_export_import_commands_parse() {
        let cli = Cli::try_parse_from([
            "agent",
            "model",
            "save",
            "local-guard",
            "--provider",
            "ollama",
            "--api-base-url",
            "http://127.0.0.1:11434/v1",
            "--api-key-env",
            "OLLAMA_API_KEY",
            "--allow-missing-api-key",
            "--top-p",
            "0.7",
            "--top-k",
            "40",
            "--reasoning-effort",
            "medium",
        ])
        .unwrap();
        let Command::Model {
            command:
                ModelCommand::Save {
                    id,
                    provider,
                    api_base_url,
                    api_key_env,
                    allow_missing_api_key,
                    top_p,
                    top_k,
                    reasoning_effort,
                    ..
                },
        } = cli.command
        else {
            panic!("expected model save command");
        };
        assert_eq!(id, "local-guard");
        assert_eq!(provider.as_deref(), Some("ollama"));
        assert_eq!(api_base_url.as_deref(), Some("http://127.0.0.1:11434/v1"));
        assert_eq!(api_key_env.as_deref(), Some("OLLAMA_API_KEY"));
        assert!(allow_missing_api_key);
        assert_eq!(top_p, Some(0.7));
        assert_eq!(top_k, Some(40));
        assert_eq!(reasoning_effort.as_deref(), Some("medium"));

        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "model",
            "save",
            "remote-guard",
            "--provider",
            "llama_cpp",
            "--allow-missing-api-key",
            "--top-p",
            "0.8",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Model {
                    command:
                        RemoteModelCommand::Save {
                            id,
                            provider,
                            allow_missing_api_key,
                            top_p,
                            ..
                        },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote model save command");
        };
        assert_eq!(id, "remote-guard");
        assert_eq!(provider.as_deref(), Some("llama_cpp"));
        assert!(allow_missing_api_key);
        assert_eq!(top_p, Some(0.8));

        let cli = Cli::try_parse_from(["agent", "model", "providers", "--json"]).unwrap();
        let Command::Model {
            command: ModelCommand::Providers { json },
        } = cli.command
        else {
            panic!("expected model providers command");
        };
        assert!(json);

        let cli = Cli::try_parse_from(["agent", "remote", "model", "providers"]).unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Model {
                    command: RemoteModelCommand::Providers,
                },
            ..
        } = cli.command
        else {
            panic!("expected remote model providers command");
        };

        let cli = Cli::try_parse_from(["agent", "model", "probe", "gpt-test", "--json"]).unwrap();
        let Command::Model {
            command: ModelCommand::Probe { id, json },
        } = cli.command
        else {
            panic!("expected model probe command");
        };
        assert_eq!(id, "gpt-test");
        assert!(json);

        let cli =
            Cli::try_parse_from(["agent", "remote", "model", "probe", "remote-guard"]).unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Model {
                    command: RemoteModelCommand::Probe { id },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote model probe command");
        };
        assert_eq!(id, "remote-guard");

        let cli = Cli::try_parse_from(["agent", "model", "export", "gpt-test", "./gpt-test.toml"])
            .unwrap();
        let Command::Model {
            command: ModelCommand::Export { id, path, json },
        } = cli.command
        else {
            panic!("expected model export command");
        };
        assert_eq!(id, "gpt-test");
        assert_eq!(path, "./gpt-test.toml");
        assert!(!json);

        let cli =
            Cli::try_parse_from(["agent", "remote", "model", "import", "./gpt-test.toml"]).unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Model {
                    command: RemoteModelCommand::Import { path },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote model import command");
        };
        assert_eq!(path, "./gpt-test.toml");
    }

    #[test]
    fn secret_commands_parse() {
        let cli = Cli::try_parse_from(["agent", "secrets", "backends", "--json"]).unwrap();
        let Command::Secrets {
            command: SecretsCommand::Backends { json },
        } = cli.command
        else {
            panic!("expected secrets backends command");
        };
        assert!(json);

        let cli = Cli::try_parse_from([
            "agent",
            "secrets",
            "set",
            "openai.api_key",
            "--value",
            "sk-test",
            "--label",
            "OpenAI",
            "--json",
        ])
        .unwrap();
        let Command::Secrets {
            command:
                SecretsCommand::Set {
                    id,
                    value,
                    label,
                    json,
                },
        } = cli.command
        else {
            panic!("expected secrets set command");
        };
        assert_eq!(id, "openai.api_key");
        assert_eq!(value.as_deref(), Some("sk-test"));
        assert_eq!(label.as_deref(), Some("OpenAI"));
        assert!(json);
    }

    #[test]
    fn capability_commands_parse() {
        let cli = Cli::try_parse_from([
            "agent",
            "capability",
            "propose",
            "--kind",
            "skill",
            "--name",
            "Review Skill",
            "--body",
            "Use the checklist.",
            "--created-by",
            "agent",
            "--json",
        ])
        .unwrap();
        let Command::Capability {
            command:
                CapabilityCommand::Propose {
                    kind,
                    name,
                    body,
                    created_by,
                    json,
                    ..
                },
        } = cli.command
        else {
            panic!("expected capability propose command");
        };
        assert_eq!(kind, "skill");
        assert_eq!(name, "Review Skill");
        assert_eq!(body.as_deref(), Some("Use the checklist."));
        assert_eq!(created_by, "agent");
        assert!(json);

        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "--url",
            "http://127.0.0.1:7878",
            "capability",
            "allow",
            "draft-review",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Capability {
                    command: RemoteCapabilityCommand::Allow { id },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote capability allow command");
        };
        assert_eq!(id, "draft-review");
    }

    #[test]
    fn resume_commands_parse() {
        let run_id = "00000000-0000-0000-0000-000000000000";
        let cli = Cli::try_parse_from(["agent", "resume", run_id, "--from-event", "7", "--json"])
            .unwrap();
        let Command::Resume {
            run_id: parsed_id,
            from_event,
            json,
            ..
        } = cli.command
        else {
            panic!("expected resume command");
        };
        assert_eq!(parsed_id, run_id);
        assert_eq!(from_event, Some(7));
        assert!(json);

        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "--url",
            "http://127.0.0.1:7878",
            "resume",
            run_id,
            "--from-event",
            "3",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Resume {
                    run_id: parsed_id,
                    from_event,
                    ..
                },
            ..
        } = cli.command
        else {
            panic!("expected remote resume command");
        };
        assert_eq!(parsed_id, run_id);
        assert_eq!(from_event, Some(3));
    }

    #[test]
    fn skill_export_import_commands_parse() {
        let cli =
            Cli::try_parse_from(["agent", "skill", "export", "review", "./review.skill.json"])
                .unwrap();
        let Command::Skill {
            command: SkillCommand::Export { id, path, json },
        } = cli.command
        else {
            panic!("expected skill export command");
        };
        assert_eq!(id, "review");
        assert_eq!(path, "./review.skill.json");
        assert!(!json);

        let cli =
            Cli::try_parse_from(["agent", "remote", "skill", "import", "./review.skill.json"])
                .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Skill {
                    command: RemoteSkillCommand::Import { path },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote skill import command");
        };
        assert_eq!(path, "./review.skill.json");
    }

    #[test]
    fn prompt_commands_accept_agent_scope() {
        let cli = Cli::try_parse_from([
            "agent",
            "prompt",
            "save",
            "daily",
            "Review carefully.",
            "--agent",
            "critic",
        ])
        .unwrap();
        let Command::Prompt {
            command: PromptCommand::Save { name, text, agent },
        } = cli.command
        else {
            panic!("expected prompt save command");
        };
        assert_eq!(name, "daily");
        assert_eq!(text, "Review carefully.");
        assert_eq!(agent.as_deref(), Some("critic"));

        let cli = Cli::try_parse_from(["agent", "remote", "prompt", "list", "--agent", "critic"])
            .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Prompt {
                    command: RemotePromptCommand::List { agent },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote prompt list command");
        };
        assert_eq!(agent.as_deref(), Some("critic"));
    }

    #[test]
    fn memory_export_import_commands_parse() {
        let cli = Cli::try_parse_from(["agent", "memory", "backends", "--json"]).unwrap();
        let Command::Memory {
            command: MemoryCommand::Backends { json },
        } = cli.command
        else {
            panic!("expected memory backends command");
        };
        assert!(json);

        let cli = Cli::try_parse_from([
            "agent",
            "memory",
            "export",
            "./memory.md",
            "--user",
            "--json",
        ])
        .unwrap();
        let Command::Memory {
            command: MemoryCommand::Export { path, user, json },
        } = cli.command
        else {
            panic!("expected memory export command");
        };
        assert_eq!(path, "./memory.md");
        assert!(user);
        assert!(json);

        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "memory",
            "import",
            "./memory.md",
            "--user",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Memory {
                    command: RemoteMemoryCommand::Import { path, user },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote memory import command");
        };
        assert_eq!(path, "./memory.md");
        assert!(user);

        let cli = Cli::try_parse_from(["agent", "remote", "memory", "backends"]).unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Memory {
                    command: RemoteMemoryCommand::Backends,
                },
            ..
        } = cli.command
        else {
            panic!("expected remote memory backends command");
        };
    }

    #[test]
    fn ingest_backends_commands_parse() {
        let cli = Cli::try_parse_from(["agent", "ingest", "backends", "--json"]).unwrap();
        let Command::Ingest {
            command: IngestCommand::Backends { json },
        } = cli.command
        else {
            panic!("expected ingest backends command");
        };
        assert!(json);

        let cli = Cli::try_parse_from([
            "agent",
            "ingest",
            "add",
            "doc.md",
            "--backend",
            "local-lines-v0",
            "--vision-model",
            "gpt-4o",
            "--guardrail-model",
            "gpt-4o-mini",
        ])
        .unwrap();
        let Command::Ingest {
            command:
                IngestCommand::Add {
                    path,
                    backend,
                    vision_model,
                    guardrail_model,
                },
        } = cli.command
        else {
            panic!("expected ingest add command");
        };
        assert_eq!(path, "doc.md");
        assert_eq!(backend, "local-lines-v0");
        assert_eq!(vision_model.as_deref(), Some("gpt-4o"));
        assert_eq!(guardrail_model.as_deref(), Some("gpt-4o-mini"));

        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "ingest",
            "add",
            "doc.md",
            "--vision-model",
            "gpt-4o",
            "--guardrail-model",
            "gpt-4o-mini",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Ingest {
                    command:
                        RemoteIngestCommand::Add {
                            path,
                            vision_model,
                            guardrail_model,
                            ..
                        },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote ingest add command");
        };
        assert_eq!(path, "doc.md");
        assert_eq!(vision_model.as_deref(), Some("gpt-4o"));
        assert_eq!(guardrail_model.as_deref(), Some("gpt-4o-mini"));

        let cli = Cli::try_parse_from([
            "agent",
            "ingest",
            "review",
            "ingest-local-v0-abc",
            "--finding",
            "0",
            "--decision",
            "approve",
            "--note",
            "looks intentional",
        ])
        .unwrap();
        let Command::Ingest {
            command:
                IngestCommand::Review {
                    id,
                    finding,
                    decision,
                    note,
                },
        } = cli.command
        else {
            panic!("expected ingest review command");
        };
        assert_eq!(id, "ingest-local-v0-abc");
        assert_eq!(finding, 0);
        assert_eq!(decision, "approve");
        assert_eq!(note.as_deref(), Some("looks intentional"));

        let cli = Cli::try_parse_from(["agent", "remote", "ingest", "backends"]).unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Ingest {
                    command: RemoteIngestCommand::Backends,
                },
            ..
        } = cli.command
        else {
            panic!("expected remote ingest backends command");
        };
    }

    #[test]
    fn artifact_commands_parse() {
        let cli =
            Cli::try_parse_from(["agent", "artifact", "open", "report.pdf", "--json"]).unwrap();
        let Command::Artifact {
            command: ArtifactCommand::Open { id, json },
        } = cli.command
        else {
            panic!("expected artifact open command");
        };
        assert_eq!(id, "report.pdf");
        assert!(json);

        let cli =
            Cli::try_parse_from(["agent", "remote", "artifact", "show", "report.pdf"]).unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Artifact {
                    command: RemoteArtifactCommand::Show { id },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote artifact show command");
        };
        assert_eq!(id, "report.pdf");
    }

    #[test]
    fn adapter_clawhub_install_matches_source_provider_spec() {
        let cli = Cli::try_parse_from([
            "agent",
            "adapter",
            "clawhub",
            "install",
            "catalog.json",
            "demo",
        ])
        .unwrap();
        let Command::Adapter {
            command:
                AdapterCommand::Clawhub {
                    command: ClawHubCommand::Install { catalog, id },
                },
        } = cli.command
        else {
            panic!("expected adapter clawhub install command");
        };
        assert_eq!(catalog, "catalog.json");
        assert_eq!(id, "demo");
    }

    #[test]
    fn remote_adapter_clawhub_search_uses_same_command_shape() {
        let cli = Cli::try_parse_from([
            "agent",
            "remote",
            "adapter",
            "clawhub",
            "search",
            "catalog.json",
            "docs",
        ])
        .unwrap();
        let Command::Remote {
            command:
                RemoteCommand::Adapter {
                    command:
                        RemoteAdapterCommand::Clawhub {
                            command: ClawHubCommand::Search { catalog, query, .. },
                        },
                },
            ..
        } = cli.command
        else {
            panic!("expected remote adapter clawhub search command");
        };
        assert_eq!(catalog, "catalog.json");
        assert_eq!(query.as_deref(), Some("docs"));
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
            agent,
            provider,
            model,
            api_base_url,
            api_key_env,
            max_output_tokens,
            temperature,
            input_cost_per_million,
            output_cost_per_million,
            max_tool_calls,
            max_tokens_before_compaction,
            max_compaction_output_tokens,
            compaction_guidance,
            allowed_tool_categories,
            allowed_skill_categories,
            tool_visibility,
            skill_visibility,
            enable_shell,
            enable_subagent,
            enable_capability_drafts,
            load_memory,
            load_skills,
            conversation,
            include_compact,
            include_ingest,
            allow_unsafe_ingest,
            refine_prompt,
            refinement_instructions,
            refinement_model,
            require_approval,
            auto_approve,
            raw_tool_output,
        } => {
            let options = setup::RuntimeOptions {
                provider,
                agent_id: agent,
                model,
                api_base_url,
                api_key_env,
                max_output_tokens,
                temperature,
                input_cost_per_million,
                output_cost_per_million,
                max_tool_calls,
                max_tokens_before_compaction,
                max_compaction_output_tokens,
                compaction_guidance,
                allowed_tool_categories,
                allowed_skill_categories,
                tool_visibility: tool_visibility.map(VisibilityLevel::from),
                skill_visibility: skill_visibility.map(VisibilityLevel::from),
                enable_shell,
                enable_subagent,
                enable_capability_drafts,
                load_memory,
                load_skills,
                conversation_id: conversation,
                include_compact,
                include_ingest,
                allow_unsafe_ingest,
                enable_prompt_refinement: refine_prompt,
                prompt_refinement_instructions: refinement_instructions,
                prompt_refinement_model: refinement_model,
                require_approval,
                auto_approve,
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
            agent,
            enable_shell,
            enable_subagent,
            enable_capability_drafts,
            max_tool_calls,
            max_tokens_before_compaction,
            max_compaction_output_tokens,
            compaction_guidance,
            allowed_tool_categories,
            allowed_skill_categories,
            tool_visibility,
            skill_visibility,
            load_memory,
            raw_tool_output,
            load_skills,
            conversation,
            include_compact,
            include_ingest,
            allow_unsafe_ingest,
        } => {
            let options = setup::RuntimeOptions {
                enable_shell,
                enable_subagent,
                enable_capability_drafts,
                agent_id: agent,
                max_tool_calls,
                max_tokens_before_compaction,
                max_compaction_output_tokens,
                compaction_guidance,
                allowed_tool_categories,
                allowed_skill_categories,
                tool_visibility: tool_visibility.map(VisibilityLevel::from),
                skill_visibility: skill_visibility.map(VisibilityLevel::from),
                load_memory,
                raw_tool_output,
                load_skills,
                conversation_id: conversation,
                include_compact,
                include_ingest,
                allow_unsafe_ingest,
                ..setup::RuntimeOptions::default()
            };
            headless::preview_context(input, json, options).await
        }
        Command::ExplainConfig { agent, json } => headless::explain_config(agent, json).await,
        Command::ExplainTools {
            json,
            agent,
            enable_shell,
            enable_subagent,
            enable_capability_drafts,
            tool_visibility,
        } => {
            headless::explain_tools(
                json,
                agent,
                enable_shell,
                enable_subagent,
                enable_capability_drafts,
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
                    auto_approve,
                },
        } => headless::call_tool(name, input, json, require_approval, auto_approve).await,
        Command::Trace {
            command: TraceCommand::Show { run_id, json },
        } => headless::trace_show(run_id, json).await,
        Command::Trace {
            command: TraceCommand::Summary { run_id, json },
        } => headless::trace_summary(run_id, json).await,
        Command::Trace {
            command: TraceCommand::Hooks { run_id, json },
        } => headless::trace_hooks(run_id, json).await,
        Command::Hooks { command } => match command {
            HookCommand::List { agent, json } => headless::hooks_list(agent, json).await,
            HookCommand::Available { agent, json } => headless::hooks_available(agent, json).await,
            HookCommand::Disable {
                hook_id,
                agent,
                confirm,
                json,
            } => headless::hooks_set_disabled(hook_id, true, agent, confirm, json).await,
            HookCommand::Enable {
                hook_id,
                agent,
                confirm,
                json,
            } => headless::hooks_set_disabled(hook_id, false, agent, confirm, json).await,
        },
        Command::Approval { command } => match command {
            ApprovalCommand::List { run_id, json } => headless::approval_list(run_id, json).await,
            ApprovalCommand::Decide {
                run_id,
                approval_id,
                approve,
            } => headless::approval_decide(run_id, approval_id, approve).await,
            ApprovalCommand::Approve {
                run_id,
                approval_id,
            } => headless::approval_decide(run_id, approval_id, true).await,
            ApprovalCommand::Reject {
                run_id,
                approval_id,
            } => headless::approval_decide(run_id, approval_id, false).await,
            ApprovalCommand::Execute {
                run_id,
                approval_id,
                json,
            } => headless::approval_execute(run_id, approval_id, json).await,
        },
        Command::Guide { run_id, text } => headless::guide(run_id, text).await,
        Command::Cancel { run_id, reason } => headless::cancel(run_id, reason).await,
        Command::Resume {
            run_id,
            from_event,
            demo,
            json,
        } => headless::resume(run_id, from_event, demo, json).await,
        Command::Score {
            run_id,
            score,
            target,
        } => headless::score(run_id, target, score).await,
        Command::Storage { json } => headless::storage_report(json).await,
        Command::Secrets { command } => match command {
            SecretsCommand::Backends { json } => headless::secrets_backends(json).await,
            SecretsCommand::Set {
                id,
                value,
                label,
                json,
            } => headless::secrets_set(id, value, label, json).await,
            SecretsCommand::Rotate { id, value, json } => {
                headless::secrets_rotate(id, value, json).await
            }
            SecretsCommand::List { json } => headless::secrets_list(json).await,
            SecretsCommand::Show { id, json } => headless::secrets_show(id, json).await,
            SecretsCommand::Delete { id } => headless::secrets_delete(id).await,
        },
        Command::Capability { command } => match command {
            CapabilityCommand::Propose {
                kind,
                name,
                body,
                guidance,
                created_by,
                json,
            } => headless::capability_propose(kind, name, body, guidance, created_by, json).await,
            CapabilityCommand::List { json } => headless::capability_list(json).await,
            CapabilityCommand::Show { id, json } => headless::capability_show(id, json).await,
            CapabilityCommand::Allow { id, json } => {
                headless::capability_review(
                    id,
                    agent_capabilities::CapabilityDraftStatus::Allowed,
                    json,
                )
                .await
            }
            CapabilityCommand::Reject { id, json } => {
                headless::capability_review(
                    id,
                    agent_capabilities::CapabilityDraftStatus::Rejected,
                    json,
                )
                .await
            }
            CapabilityCommand::Delete { id } => headless::capability_delete(id).await,
        },
        Command::Batch { command } => match command {
            BatchCommand::Run { items, demo, json } => headless::batch_run(items, demo, json).await,
            BatchCommand::Resume {
                batch_id,
                demo,
                json,
            } => headless::batch_resume(batch_id, demo, json).await,
        },
        Command::Compact { command } => match command {
            CompactCommand::Create {
                input,
                guidance,
                source,
                conversation,
                max_output_tokens,
                json,
            } => {
                headless::compact_create(
                    input,
                    guidance,
                    source,
                    conversation,
                    max_output_tokens,
                    json,
                )
                .await
            }
            CompactCommand::Conversation {
                id,
                from,
                to,
                guidance,
                max_output_tokens,
                json,
            } => {
                headless::compact_conversation(id, from, to, guidance, max_output_tokens, json)
                    .await
            }
            CompactCommand::Keep {
                input,
                guidance,
                source,
                conversation,
                max_output_tokens,
                json,
            } => {
                headless::compact_keep(
                    input,
                    guidance,
                    source,
                    conversation,
                    max_output_tokens,
                    json,
                )
                .await
            }
            CompactCommand::KeepRun {
                run_id,
                conversation,
                guidance,
                json,
            } => headless::compact_keep_run(run_id, conversation, guidance, json).await,
            CompactCommand::List { json } => headless::compact_list(json).await,
            CompactCommand::Show { id, json } => headless::compact_show(id, json).await,
            CompactCommand::Export { id, path, json } => {
                headless::compact_export(id, path, json).await
            }
            CompactCommand::Import { path, json } => headless::compact_import(path, json).await,
            CompactCommand::Rm { id } => headless::compact_rm(id).await,
        },
        Command::Conversation { command } => match command {
            ConversationCommand::Create { title, agent, json } => {
                headless::conversation_create(title, agent, json).await
            }
            ConversationCommand::List { json } => headless::conversation_list(json).await,
            ConversationCommand::AddMessage { id, role, content } => {
                headless::conversation_add_message(id, role.into(), content).await
            }
            ConversationCommand::Branch {
                id,
                at,
                title,
                reason,
                json,
            } => headless::conversation_branch(id, at, title, reason, json).await,
            ConversationCommand::Show { id, json } => headless::conversation_show(id, json).await,
            ConversationCommand::Tree { json } => headless::conversation_tree(json).await,
            ConversationCommand::Recover { id, json } => {
                headless::conversation_recover(id, json).await
            }
            ConversationCommand::Delete {
                id,
                recursive,
                compact_first,
                compact_guidance,
                compact_max_output_tokens,
                memory_first,
                memory_user,
            } => {
                let options = headless::ConversationDeleteOptions {
                    recursive,
                    compact_first,
                    compact_guidance,
                    compact_max_output_tokens,
                    memory_first,
                    memory_user,
                };
                headless::conversation_delete(id, options).await
            }
            ConversationCommand::DeleteRange { id, from, to } => {
                headless::conversation_delete_range(id, from, to).await
            }
            ConversationCommand::DeleteAgent {
                agent,
                recursive,
                compact_first,
                compact_guidance,
                compact_max_output_tokens,
                memory_first,
                memory_user,
            } => {
                let options = headless::ConversationDeleteOptions {
                    recursive,
                    compact_first,
                    compact_guidance,
                    compact_max_output_tokens,
                    memory_first,
                    memory_user,
                };
                headless::conversation_delete_agent(agent, options).await
            }
        },
        Command::Memory { command } => match command {
            MemoryCommand::Create {
                content,
                user,
                conversation,
            } => headless::memory_create(content, user, conversation).await,
            MemoryCommand::Generate {
                text,
                user,
                range,
                conversation,
            } => headless::memory_generate(text, user, range, conversation).await,
            MemoryCommand::List { json } => headless::memory_list(json).await,
            MemoryCommand::Backends { json } => headless::memory_backends(json).await,
            MemoryCommand::Edit { id, content } => headless::memory_edit(id, content).await,
            MemoryCommand::Delete { id } => headless::memory_delete(id).await,
            MemoryCommand::Rollback { user } => headless::memory_rollback(user).await,
            MemoryCommand::Export { path, user, json } => {
                headless::memory_export(path, user, json).await
            }
            MemoryCommand::Import { path, user, json } => {
                headless::memory_import(path, user, json).await
            }
        },
        Command::Skill { command } => match command {
            SkillCommand::ImportOpenclaw { path } => headless::skill_import_openclaw(path).await,
            SkillCommand::Install { source } => headless::skill_import_openclaw(source).await,
            SkillCommand::List { json } => headless::skill_list(json).await,
            SkillCommand::Inspect { id } => headless::skill_inspect(id).await,
            SkillCommand::Allow { agent_or_id, skill } => {
                let id = skill.unwrap_or(agent_or_id);
                headless::skill_allow(id).await
            }
            SkillCommand::Quarantine { id } => headless::skill_quarantine(id).await,
            SkillCommand::Export { id, path, json } => headless::skill_export(id, path, json).await,
            SkillCommand::Import { path, json } => headless::skill_import_doc(path, json).await,
        },
        Command::Prompt { command } => match command {
            PromptCommand::Save { name, text, agent } => {
                headless::prompt_save(name, text, agent).await
            }
            PromptCommand::List { json, agent } => headless::prompt_list(json, agent).await,
            PromptCommand::Show { name, json, agent } => {
                headless::prompt_show(name, json, agent).await
            }
            PromptCommand::Delete { name, agent } => headless::prompt_delete(name, agent).await,
        },
        Command::Agent { command } => match command {
            AgentCommand::List { json } => headless::agent_list(json).await,
            AgentCommand::Show { id, json } => headless::agent_show(id, json).await,
            AgentCommand::Save {
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
            } => {
                headless::agent_save(
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
                    allowed_skill_categories,
                    tool_output_mode.map(ToolOutputMode::from),
                    tool_output_interpretation_model,
                    tool_output_overrides,
                    tool_interpretation_model_overrides,
                    tool_guidance_overrides,
                    tool_visibility.map(VisibilityLevel::from),
                    load_memory,
                    load_skills,
                    ingestion_guardrail.map(IngestionGuardrailMode::from),
                    ingestion_guardrail_model,
                    input_cost_per_million,
                    output_cost_per_million,
                    refinement_instructions,
                    refinement_model,
                    refinement_aware,
                )
                .await
            }
            AgentCommand::Delete { id } => headless::agent_delete(id).await,
            AgentCommand::Export { id, path, json } => headless::agent_export(id, path, json).await,
            AgentCommand::Import { path, json } => headless::agent_import(path, json).await,
        },
        Command::Profile { command } => match command {
            ProfileCommand::Current { json } => headless::profile_current(json).await,
            ProfileCommand::Create { id, name, json } => {
                headless::profile_create(id, name, json).await
            }
            ProfileCommand::List { json } => headless::profile_list(json).await,
            ProfileCommand::Show { id, json } => headless::profile_show(id, json).await,
            ProfileCommand::Delete { id } => headless::profile_delete(id).await,
            ProfileCommand::Grant {
                from,
                to,
                kind,
                resource,
                json,
            } => headless::profile_grant(from, to, kind.into(), resource, json).await,
            ProfileCommand::Grants { from, json } => headless::profile_grants(from, json).await,
            ProfileCommand::RevokeGrant { id, json } => {
                headless::profile_revoke_grant(id, json).await
            }
        },
        Command::Model { command } => match command {
            ModelCommand::List { json } => headless::model_list(json).await,
            ModelCommand::Providers { json } => headless::model_providers(json).await,
            ModelCommand::Show { id, json } => headless::model_show(id, json).await,
            ModelCommand::Probe { id, json } => headless::model_probe(id, json).await,
            ModelCommand::Save {
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
            } => {
                headless::model_save(
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
                )
                .await
            }
            ModelCommand::Delete { id } => headless::model_delete(id).await,
            ModelCommand::Export { id, path, json } => headless::model_export(id, path, json).await,
            ModelCommand::Import { path, json } => headless::model_import(path, json).await,
        },
        Command::Ingest { command } => match command {
            IngestCommand::Backends { json } => headless::ingest_backends(json).await,
            IngestCommand::Add {
                path,
                backend,
                vision_model,
                guardrail_model,
            } => headless::ingest_add(path, backend, vision_model, guardrail_model).await,
            IngestCommand::Rerun {
                id,
                backend,
                vision_model,
                guardrail_model,
            } => headless::ingest_rerun(id, backend, vision_model, guardrail_model).await,
            IngestCommand::List { json } => headless::ingest_list(json).await,
            IngestCommand::Show { id, json } => headless::ingest_show(id, json).await,
            IngestCommand::Review {
                id,
                finding,
                decision,
                note,
            } => headless::ingest_review(id, finding, decision, note).await,
            IngestCommand::Rm { id } => headless::ingest_rm(id).await,
        },
        Command::Artifact { command } => match command {
            ArtifactCommand::List { json } => headless::artifact_list(json).await,
            ArtifactCommand::Show { id, json } => headless::artifact_show(id, json).await,
            ArtifactCommand::Open { id, json } => headless::artifact_open(id, json).await,
        },
        Command::Adapter { command } => match command {
            AdapterCommand::Import { path } => headless::adapter_import(path).await,
            AdapterCommand::Clawhub { command } => match command {
                ClawHubCommand::Search {
                    catalog,
                    query,
                    json,
                } => headless::clawhub_search(catalog, query, json).await,
                ClawHubCommand::Inspect { catalog, id, json } => {
                    headless::clawhub_inspect(catalog, id, json).await
                }
                ClawHubCommand::Pin { catalog, id, json } => {
                    headless::clawhub_pin(catalog, id, json).await
                }
                ClawHubCommand::Install { catalog, id } => {
                    headless::clawhub_install(catalog, id).await
                }
            },
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
                agent,
                provider,
                model,
                api_base_url,
                api_key_env,
                input_cost_per_million,
                output_cost_per_million,
                max_tool_calls,
                max_tokens_before_compaction,
                max_compaction_output_tokens,
                compaction_guidance,
                allowed_tool_categories,
                allowed_skill_categories,
                tool_visibility,
                skill_visibility,
                enable_shell,
                enable_subagent,
                enable_capability_drafts,
                load_memory,
                load_skills,
                include_compact,
                conversation,
                include_ingest,
                allow_unsafe_ingest,
                refine_prompt,
                refinement_instructions,
                refinement_model,
                require_approval,
                auto_approve,
                raw_tool_output,
            } => {
                let options = setup::RuntimeOptions {
                    provider,
                    agent_id: agent,
                    model,
                    api_base_url,
                    api_key_env,
                    max_output_tokens: None,
                    temperature: None,
                    input_cost_per_million,
                    output_cost_per_million,
                    max_tool_calls,
                    max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    compaction_guidance,
                    allowed_tool_categories,
                    allowed_skill_categories,
                    tool_visibility: tool_visibility.map(VisibilityLevel::from),
                    skill_visibility: skill_visibility.map(VisibilityLevel::from),
                    enable_shell,
                    enable_subagent,
                    enable_capability_drafts,
                    load_memory,
                    load_skills,
                    include_compact,
                    conversation_id: conversation,
                    include_ingest,
                    allow_unsafe_ingest,
                    enable_prompt_refinement: refine_prompt,
                    prompt_refinement_instructions: refinement_instructions,
                    prompt_refinement_model: refinement_model,
                    require_approval,
                    auto_approve,
                    raw_tool_output,
                };
                headless::remote_run(url, input, demo, options).await
            }
            RemoteCommand::RunStart {
                input,
                demo,
                agent,
                provider,
                model,
                api_base_url,
                api_key_env,
                input_cost_per_million,
                output_cost_per_million,
                max_tool_calls,
                max_tokens_before_compaction,
                max_compaction_output_tokens,
                compaction_guidance,
                allowed_tool_categories,
                allowed_skill_categories,
                tool_visibility,
                skill_visibility,
                enable_shell,
                enable_subagent,
                enable_capability_drafts,
                load_memory,
                load_skills,
                include_compact,
                conversation,
                include_ingest,
                allow_unsafe_ingest,
                refine_prompt,
                refinement_instructions,
                refinement_model,
                require_approval,
                auto_approve,
                raw_tool_output,
            } => {
                let options = setup::RuntimeOptions {
                    provider,
                    agent_id: agent,
                    model,
                    api_base_url,
                    api_key_env,
                    max_output_tokens: None,
                    temperature: None,
                    input_cost_per_million,
                    output_cost_per_million,
                    max_tool_calls,
                    max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    compaction_guidance,
                    allowed_tool_categories,
                    allowed_skill_categories,
                    tool_visibility: tool_visibility.map(VisibilityLevel::from),
                    skill_visibility: skill_visibility.map(VisibilityLevel::from),
                    enable_shell,
                    enable_subagent,
                    enable_capability_drafts,
                    load_memory,
                    load_skills,
                    include_compact,
                    conversation_id: conversation,
                    include_ingest,
                    allow_unsafe_ingest,
                    enable_prompt_refinement: refine_prompt,
                    prompt_refinement_instructions: refinement_instructions,
                    prompt_refinement_model: refinement_model,
                    require_approval,
                    auto_approve,
                    raw_tool_output,
                };
                headless::remote_run_start(url, input, demo, options).await
            }
            RemoteCommand::RunStatus { run_id } => headless::remote_run_status(url, run_id).await,
            RemoteCommand::RunEvents { run_id, after } => {
                headless::remote_run_events(url, run_id, after).await
            }
            RemoteCommand::PreviewContext {
                input,
                agent,
                enable_shell,
                enable_subagent,
                enable_capability_drafts,
                max_tool_calls,
                max_tokens_before_compaction,
                max_compaction_output_tokens,
                compaction_guidance,
                allowed_tool_categories,
                allowed_skill_categories,
                tool_visibility,
                skill_visibility,
                load_memory,
                raw_tool_output,
                load_skills,
                include_compact,
                conversation,
                include_ingest,
                allow_unsafe_ingest,
            } => {
                let options = setup::RuntimeOptions {
                    agent_id: agent,
                    enable_shell,
                    enable_subagent,
                    enable_capability_drafts,
                    max_tool_calls,
                    max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    compaction_guidance,
                    allowed_tool_categories,
                    allowed_skill_categories,
                    tool_visibility: tool_visibility.map(VisibilityLevel::from),
                    skill_visibility: skill_visibility.map(VisibilityLevel::from),
                    load_memory,
                    raw_tool_output,
                    load_skills,
                    include_compact,
                    conversation_id: conversation,
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
            RemoteCommand::Resume {
                run_id,
                from_event,
                demo,
            } => headless::remote_resume(url, run_id, from_event, demo).await,
            RemoteCommand::Score {
                run_id,
                score,
                target,
            } => headless::remote_score(url, run_id, target, score).await,
            RemoteCommand::Storage => headless::remote_storage_report(url).await,
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
                auto_approve,
            } => headless::remote_tool(url, name, input, require_approval, auto_approve).await,
            RemoteCommand::Trace { run_id } => headless::remote_trace(url, run_id).await,
            RemoteCommand::TraceSummary { run_id } => {
                headless::remote_trace_summary(url, run_id).await
            }
            RemoteCommand::TraceHooks { run_id } => headless::remote_trace_hooks(url, run_id).await,
            RemoteCommand::Approval { command } => match command {
                RemoteApprovalCommand::List { run_id } => {
                    headless::remote_approval_list(url, run_id).await
                }
                RemoteApprovalCommand::Decide {
                    run_id,
                    approval_id,
                    approve,
                } => headless::remote_approval_decide(url, run_id, approval_id, approve).await,
                RemoteApprovalCommand::Approve {
                    run_id,
                    approval_id,
                } => headless::remote_approval_decide(url, run_id, approval_id, true).await,
                RemoteApprovalCommand::Reject {
                    run_id,
                    approval_id,
                } => headless::remote_approval_decide(url, run_id, approval_id, false).await,
                RemoteApprovalCommand::Execute {
                    run_id,
                    approval_id,
                } => headless::remote_approval_execute(url, run_id, approval_id).await,
            },
            RemoteCommand::Memory { command } => match command {
                RemoteMemoryCommand::List => headless::remote_memory_list(url).await,
                RemoteMemoryCommand::Backends => headless::remote_memory_backends(url).await,
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
                RemoteMemoryCommand::Export { path, user } => {
                    headless::remote_memory_export(url, path, user).await
                }
                RemoteMemoryCommand::Import { path, user } => {
                    headless::remote_memory_import(url, path, user).await
                }
            },
            RemoteCommand::Skill { command } => match command {
                RemoteSkillCommand::List => headless::remote_skill_list(url).await,
                RemoteSkillCommand::ImportOpenclaw { path } => {
                    headless::remote_skill_import(url, path).await
                }
                RemoteSkillCommand::Install { source } => {
                    headless::remote_skill_import(url, source).await
                }
                RemoteSkillCommand::Allow { agent_or_id, skill } => {
                    let id = skill.unwrap_or(agent_or_id);
                    headless::remote_skill_action(url, id, true).await
                }
                RemoteSkillCommand::Quarantine { id } => {
                    headless::remote_skill_action(url, id, false).await
                }
                RemoteSkillCommand::Export { id, path } => {
                    headless::remote_skill_export(url, id, path).await
                }
                RemoteSkillCommand::Import { path } => {
                    headless::remote_skill_import_doc(url, path).await
                }
            },
            RemoteCommand::Capability { command } => match command {
                RemoteCapabilityCommand::Propose {
                    kind,
                    name,
                    body,
                    guidance,
                    created_by,
                } => {
                    headless::remote_capability_propose(url, kind, name, body, guidance, created_by)
                        .await
                }
                RemoteCapabilityCommand::List => headless::remote_capability_list(url).await,
                RemoteCapabilityCommand::Show { id } => {
                    headless::remote_capability_show(url, id).await
                }
                RemoteCapabilityCommand::Allow { id } => {
                    headless::remote_capability_review(url, id, true).await
                }
                RemoteCapabilityCommand::Reject { id } => {
                    headless::remote_capability_review(url, id, false).await
                }
                RemoteCapabilityCommand::Delete { id } => {
                    headless::remote_capability_delete(url, id).await
                }
            },
            RemoteCommand::Prompt { command } => match command {
                RemotePromptCommand::Save { name, text, agent } => {
                    headless::remote_prompt_save(url, name, text, agent).await
                }
                RemotePromptCommand::List { agent } => {
                    headless::remote_prompt_list(url, agent).await
                }
                RemotePromptCommand::Show { name, agent } => {
                    headless::remote_prompt_show(url, name, agent).await
                }
                RemotePromptCommand::Delete { name, agent } => {
                    headless::remote_prompt_delete(url, name, agent).await
                }
            },
            RemoteCommand::Agent { command } => match command {
                RemoteAgentCommand::List => headless::remote_agent_list(url).await,
                RemoteAgentCommand::Show { id } => headless::remote_agent_show(url, id).await,
                RemoteAgentCommand::Save {
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
                } => {
                    headless::remote_agent_save(
                        url,
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
                        allowed_skill_categories,
                        tool_output_mode.map(ToolOutputMode::from),
                        tool_output_interpretation_model,
                        tool_output_overrides,
                        tool_interpretation_model_overrides,
                        tool_guidance_overrides,
                        tool_visibility.map(VisibilityLevel::from),
                        load_memory,
                        load_skills,
                        ingestion_guardrail.map(IngestionGuardrailMode::from),
                        ingestion_guardrail_model,
                        input_cost_per_million,
                        output_cost_per_million,
                        refinement_instructions,
                        refinement_model,
                        refinement_aware,
                    )
                    .await
                }
                RemoteAgentCommand::Delete { id } => headless::remote_agent_delete(url, id).await,
                RemoteAgentCommand::Export { id, path } => {
                    headless::remote_agent_export(url, id, path).await
                }
                RemoteAgentCommand::Import { path } => {
                    headless::remote_agent_import(url, path).await
                }
            },
            RemoteCommand::Model { command } => match command {
                RemoteModelCommand::List => headless::remote_model_list(url).await,
                RemoteModelCommand::Providers => headless::remote_model_providers(url).await,
                RemoteModelCommand::Show { id } => headless::remote_model_show(url, id).await,
                RemoteModelCommand::Probe { id } => headless::remote_model_probe(url, id).await,
                RemoteModelCommand::Save {
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
                } => {
                    headless::remote_model_save(
                        url,
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
                    )
                    .await
                }
                RemoteModelCommand::Delete { id } => headless::remote_model_delete(url, id).await,
                RemoteModelCommand::Export { id, path } => {
                    headless::remote_model_export(url, id, path).await
                }
                RemoteModelCommand::Import { path } => {
                    headless::remote_model_import(url, path).await
                }
            },
            RemoteCommand::Ingest { command } => match command {
                RemoteIngestCommand::Backends => headless::remote_ingest_backends(url).await,
                RemoteIngestCommand::List => headless::remote_ingest_list(url).await,
                RemoteIngestCommand::Add {
                    path,
                    backend,
                    vision_model,
                    guardrail_model,
                } => {
                    headless::remote_ingest_add(url, path, backend, vision_model, guardrail_model)
                        .await
                }
                RemoteIngestCommand::Rerun {
                    id,
                    backend,
                    vision_model,
                    guardrail_model,
                } => {
                    headless::remote_ingest_rerun(url, id, backend, vision_model, guardrail_model)
                        .await
                }
                RemoteIngestCommand::Show { id } => headless::remote_ingest_show(url, id).await,
                RemoteIngestCommand::Review {
                    id,
                    finding,
                    decision,
                    note,
                } => headless::remote_ingest_review(url, id, finding, decision, note).await,
                RemoteIngestCommand::Rm { id } => headless::remote_ingest_rm(url, id).await,
            },
            RemoteCommand::Artifact { command } => match command {
                RemoteArtifactCommand::List => headless::remote_artifact_list(url).await,
                RemoteArtifactCommand::Show { id } => headless::remote_artifact_show(url, id).await,
                RemoteArtifactCommand::Open { id } => headless::remote_artifact_open(url, id).await,
            },
            RemoteCommand::Adapter { command } => match command {
                RemoteAdapterCommand::List => headless::remote_adapter_list(url).await,
                RemoteAdapterCommand::Import { path } => {
                    headless::remote_adapter_import(url, path).await
                }
                RemoteAdapterCommand::Clawhub { command } => match command {
                    ClawHubCommand::Search {
                        catalog,
                        query,
                        json: _,
                    } => headless::remote_clawhub_search(url, catalog, query).await,
                    ClawHubCommand::Inspect {
                        catalog,
                        id,
                        json: _,
                    } => headless::remote_clawhub_inspect(url, catalog, id).await,
                    ClawHubCommand::Pin {
                        catalog,
                        id,
                        json: _,
                    } => headless::remote_clawhub_pin(url, catalog, id).await,
                    ClawHubCommand::Install { catalog, id } => {
                        headless::remote_clawhub_install(url, catalog, id).await
                    }
                },
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
