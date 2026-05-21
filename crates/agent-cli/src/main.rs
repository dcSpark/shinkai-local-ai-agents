//! Shinkai CLI entry point.
//!
//! Default mode is the ratatui TUI. `--print` (or piping stdout to a non-TTY)
//! switches to headless mode for scripting / CI. See `specs/architecture.md`
//! §20.1 for the surface contract.

#![allow(clippy::items_after_test_module)]

mod headless;
mod setup;
mod tui;

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use std::io::IsTerminal;

use agent_config::{IngestionGuardrailMode, ProfileGrantKind};
use agent_core::{StopRetentionMode, ToolOutputMode, VisibilityLevel};
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

        /// Restrict loaded memory to one topic. Repeat for multiple topics.
        #[arg(long = "memory-topic")]
        memory_topics: Vec<String>,

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

        /// Restrict loaded memory to one topic. Repeat for multiple topics.
        #[arg(long = "memory-topic")]
        memory_topics: Vec<String>,

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
        /// Plan or apply cache-file pruning for files older than this many days.
        #[arg(long = "prune-cache-days")]
        prune_cache_days: Option<u64>,
        /// Apply the cache prune plan. Without this flag, pruning is a dry run.
        #[arg(long)]
        apply: bool,
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
    /// Show a nested child-run tree for a persisted run id.
    Tree {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Emit a JSON tree object.
        #[arg(long)]
        json: bool,
    },
    /// Compare two persisted run traces side by side.
    Compare {
        /// Baseline run UUID printed by `agent run`.
        run_id: String,

        /// Run UUID to compare against the baseline.
        compare_run_id: String,

        /// Emit a JSON comparison object.
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
    /// List quality score records for a persisted run id.
    Scores {
        /// Run UUID printed by `agent run`.
        run_id: String,

        /// Emit JSON score records with event ids.
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
    /// Ask the configured controller agent model for a recommendation.
    Assess {
        /// Run UUID printed by `agent run` or an approval-required error.
        run_id: String,

        /// Approval id, for example `approval-manual-1`.
        approval_id: String,

        /// Override the delegated controller agent id from the approval trace.
        #[arg(long = "controller-agent")]
        controller_agent: Option<String>,

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

        /// Read the approval unlock secret from this environment variable.
        #[arg(long = "unlock-env", value_name = "ENV")]
        unlock_env: Option<String>,

        /// Read the approval HMAC signature from this environment variable.
        #[arg(long = "signature-env", value_name = "ENV")]
        signature_env: Option<String>,

        /// Claim this configured controller agent as the delegated approver.
        #[arg(long = "controller-agent")]
        controller_agent: Option<String>,
    },
    /// Approve an approval request without executing it.
    Approve {
        /// Run UUID printed by `agent run` or an approval-required error.
        run_id: String,

        /// Approval id, for example `approval-manual-1`.
        approval_id: String,

        /// Read the approval unlock secret from this environment variable.
        #[arg(long = "unlock-env", value_name = "ENV")]
        unlock_env: Option<String>,

        /// Read the approval HMAC signature from this environment variable.
        #[arg(long = "signature-env", value_name = "ENV")]
        signature_env: Option<String>,

        /// Claim this configured controller agent as the delegated approver.
        #[arg(long = "controller-agent")]
        controller_agent: Option<String>,
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

        /// Read the approval unlock secret from this environment variable.
        #[arg(long = "unlock-env", value_name = "ENV")]
        unlock_env: Option<String>,

        /// Read the approval HMAC signature from this environment variable.
        #[arg(long = "signature-env", value_name = "ENV")]
        signature_env: Option<String>,
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

        /// Agent id that should own this memory record.
        #[arg(long)]
        agent: Option<String>,

        /// Topic tag for this memory. Repeat for multiple topics.
        #[arg(long = "topic")]
        topics: Vec<String>,
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

        /// Agent id that should own generated memory records.
        #[arg(long)]
        agent: Option<String>,

        /// Topic tag for generated memory. Repeat for multiple topics.
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    /// Generate memory records from an expanded conversation message range.
    GenerateConversation {
        id: String,

        /// First expanded message index to include. Defaults to 0.
        #[arg(long)]
        from: Option<usize>,

        /// Last expanded message index to include. Defaults to the final message.
        #[arg(long)]
        to: Option<usize>,

        /// Store in user.md instead of memory.md.
        #[arg(long)]
        user: bool,

        /// Agent id that should own generated memory records. Defaults to the conversation agent.
        #[arg(long)]
        agent: Option<String>,

        /// Topic tag for generated memory. Repeat for multiple topics.
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    /// List memory records.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show local and profile-granted memory visible to the active profile.
    Access {
        /// Topic tag to filter by. Repeat for multiple topics.
        #[arg(long = "topic")]
        topics: Vec<String>,

        #[arg(long)]
        json: bool,
    },
    /// List supported memory backends.
    Backends {
        #[arg(long)]
        json: bool,
    },
    /// Classify an existing memory record with a saved model.
    Classify {
        id: String,

        /// Saved model id. Falls back to --agent memory policy, then AGENT_MEMORY_CLASSIFICATION_MODEL.
        #[arg(long)]
        model: Option<String>,

        /// Agent id whose memory model policy is used when --model is omitted.
        #[arg(long)]
        agent: Option<String>,

        /// Return model classification without updating the memory record.
        #[arg(long = "no-apply")]
        no_apply: bool,
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
        /// Agent id that should own imported memory records.
        #[arg(long)]
        agent: Option<String>,
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
        /// Default context retention when this agent's run is stopped without an explicit mode.
        #[arg(long = "stop-retention-mode", value_enum)]
        stop_retention_mode: Option<StopRetentionModeArg>,
        /// Restrict this agent to one tool id. Repeat for multiple tools.
        #[arg(long = "allow-tool")]
        allowed_tools: Vec<String>,
        /// Restrict this agent to one tool category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,
        /// Delegate scoped approvals to this controller agent id.
        #[arg(long = "approval-controller-agent")]
        approval_controller_agent: Option<String>,
        /// Allow the approval controller to approve one tool id. Repeat for multiple tools.
        #[arg(long = "approval-controller-tool")]
        approval_controller_allowed_tools: Vec<String>,
        /// Allow the approval controller to approve one tool category. Repeat for multiple categories.
        #[arg(long = "approval-controller-tool-category")]
        approval_controller_allowed_tool_categories: Vec<String>,
        /// Allow this agent to create quarantined tool, skill, agent, or subagent drafts.
        #[arg(long)]
        capability_drafts_enabled: Option<bool>,
        /// Guidance shown when this agent can create capability drafts.
        #[arg(long)]
        capability_draft_guidance: Option<String>,
        /// Restrict loaded skills to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,
        /// Override one skill's visibility, as SKILL=full-schema, SKILL=name-and-description, or SKILL=name-only. Repeat for multiple skills.
        #[arg(long = "skill-visibility-override")]
        skill_visibility_overrides: Vec<String>,
        /// How much skill detail is shown to the model.
        #[arg(long, value_enum)]
        skill_visibility: Option<ToolVisibility>,
        /// Whether tool outputs are interpreted by the LLM or returned raw.
        #[arg(long, value_enum)]
        tool_output_mode: Option<ToolOutputModeArg>,
        /// Agent-level model id used for tool-call selection.
        #[arg(long = "tool-routing-model")]
        tool_routing_model: Option<String>,
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
        /// Override one tool's visibility, as TOOL=full-schema, TOOL=name-and-description, or TOOL=name-only. Repeat for multiple tools.
        #[arg(long = "tool-visibility-override")]
        tool_visibility_overrides: Vec<String>,
        /// How much tool detail is shown to the model.
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,
        /// Load this agent's memory into context by default.
        #[arg(long = "load-memory")]
        load_memory: bool,
        /// Memory backend id for this agent. Defaults to the built-in local markdown backend.
        #[arg(long = "memory-backend")]
        memory_backend: Option<String>,
        /// Saved model id used by memory operations such as classification.
        #[arg(long = "memory-model")]
        memory_model: Option<String>,
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
    /// Check saved model configs against provider descriptors and metadata catalogs.
    Doctor {
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
    /// Show, export, or import the active profile model provider catalog JSON.
    ProviderCatalog {
        #[command(subcommand)]
        command: ModelProviderCatalogCommand,
    },
    /// Show, export, or import the active profile model metadata catalog JSON.
    MetadataCatalog {
        #[command(subcommand)]
        command: ModelMetadataCatalogCommand,
    },
}

#[derive(Subcommand)]
enum ModelProviderCatalogCommand {
    /// Show the configured provider catalog, if one exists.
    Show {
        #[arg(long)]
        json: bool,
    },
    /// Export the configured provider catalog as portable JSON.
    Export {
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Import portable JSON into the active profile provider catalog.
    Import {
        path: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ModelMetadataCatalogCommand {
    /// Show the configured metadata catalog, if one exists.
    Show {
        #[arg(long)]
        json: bool,
    },
    /// Export the configured metadata catalog as portable JSON.
    Export {
        path: String,
        #[arg(long)]
        json: bool,
    },
    /// Import portable JSON into the active profile metadata catalog.
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
    /// Live-probe whether a saved model accepts a vision/document source attachment.
    ProbeVision {
        path: String,

        /// Saved model id to probe.
        #[arg(long)]
        model: String,

        /// Emit JSON instead of a human-readable summary.
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
    /// Delete a generated artifact from the local artifact cache.
    Delete {
        id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AdapterCommand {
    /// Import a local adapter source into quarantine.
    Import { path: String },
    /// Import a portable normalized adapter manifest into quarantine.
    ImportManifest {
        path: String,

        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
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
    /// Report adapter review and runtime operability status.
    Doctor {
        /// Emit JSON instead of a human-readable summary.
        #[arg(long)]
        json: bool,
    },
    /// Install an OpenClaw adapter package into the skill registry; starts quarantined.
    InstallSkill {
        id: String,

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
    /// Export a persisted adapter manifest as portable JSON.
    Export {
        id: String,
        path: String,

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
        /// Capability kind: tool, skill, agent, or subagent.
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
    /// Export a capability draft as portable JSON.
    Export {
        id: String,
        path: String,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
    /// Import a capability draft from portable JSON. Imported drafts stay quarantined.
    Import {
        path: String,
        /// Emit JSON metadata.
        #[arg(long)]
        json: bool,
    },
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
    /// Show or update conversation-level context policy overrides.
    Policy {
        id: String,

        /// Conversation default for loading file-backed memory.
        #[arg(long)]
        load_memory: Option<bool>,

        /// Clear the conversation memory-loading override.
        #[arg(long)]
        clear_load_memory: bool,

        /// Conversation default for async/manual pending memory generation.
        #[arg(long)]
        generate_memory: Option<bool>,

        /// Clear the conversation memory-generation override.
        #[arg(long)]
        clear_generate_memory: bool,

        /// Restrict tools for this conversation to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,

        /// Clear the conversation tool-category override.
        #[arg(long)]
        clear_allowed_tool_categories: bool,

        /// Restrict loaded skills for this conversation to one category/pack. Repeat for multiple categories.
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,

        /// Clear the conversation skill-category override.
        #[arg(long)]
        clear_allowed_skill_categories: bool,

        /// Allow or block agent-created capability drafts for this conversation.
        #[arg(long)]
        capability_drafts_enabled: Option<bool>,

        /// Clear the conversation capability-drafting enablement override.
        #[arg(long)]
        clear_capability_drafts_enabled: bool,

        /// Conversation guidance for agent-created capability drafts.
        #[arg(long)]
        capability_draft_guidance: Option<String>,

        /// Clear the conversation capability-drafting guidance override.
        #[arg(long)]
        clear_capability_draft_guidance: bool,

        /// Conversation default auto-compaction threshold.
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,

        /// Clear the conversation auto-compaction threshold override.
        #[arg(long)]
        clear_max_tokens_before_compaction: bool,

        /// Conversation default auto-compaction output budget.
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,

        /// Clear the conversation auto-compaction output budget override.
        #[arg(long)]
        clear_max_compaction_output_tokens: bool,

        /// Conversation default guidance for automatic context compaction.
        #[arg(long)]
        compaction_guidance: Option<String>,

        /// Clear the conversation auto-compaction guidance override.
        #[arg(long)]
        clear_compaction_guidance: bool,

        /// Clear all conversation policy overrides.
        #[arg(long)]
        clear: bool,

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

#[allow(clippy::large_enum_variant)]
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

        /// Restrict loaded daemon memory to one topic. Repeat for multiple topics.
        #[arg(long = "memory-topic")]
        memory_topics: Vec<String>,

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

        /// Restrict loaded daemon memory to one topic. Repeat for multiple topics.
        #[arg(long = "memory-topic")]
        memory_topics: Vec<String>,

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
    /// Poll a daemon async run until it reaches a terminal status.
    RunWait {
        run_id: String,

        /// Poll interval in milliseconds.
        #[arg(long, default_value_t = 1000)]
        poll_ms: u64,

        /// Maximum wait in milliseconds. Defaults to no timeout.
        #[arg(long)]
        timeout_ms: Option<u64>,

        /// Include newly observed run events in the final JSON report.
        #[arg(long)]
        events: bool,
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

        /// Restrict loaded memory to one topic. Repeat for multiple topics.
        #[arg(long = "memory-topic")]
        memory_topics: Vec<String>,

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
    /// Start a resumed remote run asynchronously and return its run id immediately.
    ResumeStart {
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
    Storage {
        /// Plan or apply daemon cache-file pruning for files older than this many days.
        #[arg(long = "prune-cache-days")]
        prune_cache_days: Option<u64>,
        /// Apply the daemon cache prune plan. Without this flag, pruning is a dry run.
        #[arg(long)]
        apply: bool,
    },
    /// Remote messaging bridge delivery dead-letter operations.
    BridgeDeliveries {
        #[command(subcommand)]
        command: RemoteBridgeDeliveryCommand,
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
    /// Show daemon child-run trace tree.
    TraceTree {
        run_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Compare two daemon traces side by side.
    TraceCompare {
        run_id: String,
        compare_run_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Show daemon hook remediation plan.
    TraceHooks { run_id: String },
    /// Show daemon quality score records.
    TraceScores { run_id: String },
    /// Remote approval operations.
    Approval {
        #[command(subcommand)]
        command: RemoteApprovalCommand,
    },
    /// Remote conversation branch operations.
    Conversation {
        #[command(subcommand)]
        command: RemoteConversationCommand,
    },
    /// Remote memory operations.
    Memory {
        #[command(subcommand)]
        command: RemoteMemoryCommand,
    },
    /// Remote compacted-context artifact operations.
    Compact {
        #[command(subcommand)]
        command: RemoteCompactCommand,
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
    Assess {
        run_id: String,
        approval_id: String,
        #[arg(long = "controller-agent")]
        controller_agent: Option<String>,
    },
    Decide {
        run_id: String,
        approval_id: String,
        #[arg(long)]
        approve: bool,
        #[arg(long = "unlock-env", value_name = "ENV")]
        unlock_env: Option<String>,
        #[arg(long = "signature-env", value_name = "ENV")]
        signature_env: Option<String>,
        #[arg(long = "controller-agent")]
        controller_agent: Option<String>,
    },
    Approve {
        run_id: String,
        approval_id: String,
        #[arg(long = "unlock-env", value_name = "ENV")]
        unlock_env: Option<String>,
        #[arg(long = "signature-env", value_name = "ENV")]
        signature_env: Option<String>,
        #[arg(long = "controller-agent")]
        controller_agent: Option<String>,
    },
    Reject {
        run_id: String,
        approval_id: String,
    },
    Execute {
        run_id: String,
        approval_id: String,
        #[arg(long = "unlock-env", value_name = "ENV")]
        unlock_env: Option<String>,
        #[arg(long = "signature-env", value_name = "ENV")]
        signature_env: Option<String>,
    },
}

#[derive(Subcommand)]
enum RemoteBridgeDeliveryCommand {
    /// List failed outbound bridge deliveries.
    List,
    /// Retry one failed outbound bridge delivery by id.
    Retry { id: String },
    /// Retry all failed outbound bridge deliveries up to the daemon batch limit.
    RetryAll,
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
enum RemoteConversationCommand {
    List,
    Tree,
    Show {
        id: String,
    },
    Recover {
        id: String,
    },
    Policy {
        id: String,
        #[arg(long)]
        load_memory: Option<bool>,
        #[arg(long)]
        clear_load_memory: bool,
        #[arg(long)]
        generate_memory: Option<bool>,
        #[arg(long)]
        clear_generate_memory: bool,
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,
        #[arg(long)]
        clear_allowed_tool_categories: bool,
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,
        #[arg(long)]
        clear_allowed_skill_categories: bool,
        #[arg(long)]
        capability_drafts_enabled: Option<bool>,
        #[arg(long)]
        clear_capability_drafts_enabled: bool,
        #[arg(long)]
        capability_draft_guidance: Option<String>,
        #[arg(long)]
        clear_capability_draft_guidance: bool,
        #[arg(long)]
        max_tokens_before_compaction: Option<u32>,
        #[arg(long)]
        clear_max_tokens_before_compaction: bool,
        #[arg(long)]
        max_compaction_output_tokens: Option<u32>,
        #[arg(long)]
        clear_max_compaction_output_tokens: bool,
        #[arg(long)]
        compaction_guidance: Option<String>,
        #[arg(long)]
        clear_compaction_guidance: bool,
        #[arg(long)]
        clear: bool,
    },
    DeletePlan {
        id: String,
        #[arg(long)]
        recursive: bool,
    },
    Delete {
        id: String,
        #[arg(long)]
        recursive: bool,
    },
    DeleteAgentPlan {
        agent: String,
        #[arg(long)]
        recursive: bool,
    },
    DeleteAgent {
        agent: String,
        #[arg(long)]
        recursive: bool,
    },
    DeleteRange {
        id: String,
        #[arg(long)]
        from: usize,
        #[arg(long)]
        to: usize,
    },
}

#[derive(Subcommand)]
enum RemoteMemoryCommand {
    List,
    Access {
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    Backends,
    Create {
        content: String,
        #[arg(long)]
        user: bool,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    Generate {
        text: String,
        #[arg(long)]
        user: bool,
        #[arg(long)]
        range: Option<String>,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    GenerateConversation {
        id: String,
        #[arg(long)]
        from: Option<usize>,
        #[arg(long)]
        to: Option<usize>,
        #[arg(long)]
        user: bool,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    GeneratePending {
        #[arg(long)]
        user: bool,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long = "topic")]
        topics: Vec<String>,
    },
    Classify {
        id: String,
        /// Saved model id. Falls back to --agent memory policy, then AGENT_MEMORY_CLASSIFICATION_MODEL.
        #[arg(long)]
        model: Option<String>,
        /// Agent id whose memory model policy is used when --model is omitted.
        #[arg(long)]
        agent: Option<String>,
        #[arg(long = "no-apply")]
        no_apply: bool,
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
        #[arg(long)]
        agent: Option<String>,
    },
}

#[derive(Subcommand)]
enum RemoteCompactCommand {
    /// Keep the auto-compacted context emitted by a completed daemon run.
    KeepRun {
        /// Run id that emitted an auto-compacted ContextBuilt event.
        run_id: String,

        /// Conversation this compaction should be linked to.
        #[arg(long)]
        conversation: Option<String>,

        /// Guidance associated with the compacted context.
        #[arg(long)]
        guidance: Option<String>,
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
    /// Export a daemon capability draft as portable JSON on the daemon host.
    Export { id: String, path: String },
    /// Import a daemon capability draft from portable JSON on the daemon host.
    Import { path: String },
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
        #[arg(long = "stop-retention-mode", value_enum)]
        stop_retention_mode: Option<StopRetentionModeArg>,
        #[arg(long = "allow-tool")]
        allowed_tools: Vec<String>,
        #[arg(long = "allow-tool-category")]
        allowed_tool_categories: Vec<String>,
        #[arg(long = "approval-controller-agent")]
        approval_controller_agent: Option<String>,
        #[arg(long = "approval-controller-tool")]
        approval_controller_allowed_tools: Vec<String>,
        #[arg(long = "approval-controller-tool-category")]
        approval_controller_allowed_tool_categories: Vec<String>,
        #[arg(long)]
        capability_drafts_enabled: Option<bool>,
        #[arg(long)]
        capability_draft_guidance: Option<String>,
        #[arg(long = "allow-skill-category")]
        allowed_skill_categories: Vec<String>,
        #[arg(long = "skill-visibility-override")]
        skill_visibility_overrides: Vec<String>,
        #[arg(long, value_enum)]
        skill_visibility: Option<ToolVisibility>,
        #[arg(long, value_enum)]
        tool_output_mode: Option<ToolOutputModeArg>,
        #[arg(long = "tool-routing-model")]
        tool_routing_model: Option<String>,
        #[arg(long = "tool-output-interpretation-model")]
        tool_output_interpretation_model: Option<String>,
        #[arg(long = "tool-output-override")]
        tool_output_overrides: Vec<String>,
        #[arg(long = "tool-interpretation-model")]
        tool_interpretation_model_overrides: Vec<String>,
        #[arg(long = "tool-guidance-override")]
        tool_guidance_overrides: Vec<String>,
        #[arg(long = "tool-visibility-override")]
        tool_visibility_overrides: Vec<String>,
        #[arg(long, value_enum)]
        tool_visibility: Option<ToolVisibility>,
        #[arg(long = "load-memory")]
        load_memory: bool,
        #[arg(long = "memory-backend")]
        memory_backend: Option<String>,
        #[arg(long = "memory-model")]
        memory_model: Option<String>,
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
    Doctor,
    ProviderCatalog {
        #[command(subcommand)]
        command: RemoteModelProviderCatalogCommand,
    },
    MetadataCatalog {
        #[command(subcommand)]
        command: RemoteModelMetadataCatalogCommand,
    },
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
enum RemoteModelProviderCatalogCommand {
    Show,
    Export { path: String },
    Import { path: String },
}

#[derive(Subcommand)]
enum RemoteModelMetadataCatalogCommand {
    Show,
    Export { path: String },
    Import { path: String },
}

#[derive(Subcommand)]
enum RemoteIngestCommand {
    Backends,
    List,
    ProbeVision {
        path: String,

        /// Saved model id to probe on the daemon host.
        #[arg(long)]
        model: String,
    },
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
    Delete { id: String },
}

#[derive(Subcommand)]
enum RemoteAdapterCommand {
    List,
    Doctor,
    InstallSkill {
        id: String,
    },
    Import {
        path: String,
    },
    ImportManifest {
        path: String,
    },
    Clawhub {
        #[command(subcommand)]
        command: ClawHubCommand,
    },
    Show {
        id: String,
    },
    Export {
        id: String,
        path: String,
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
pub enum StopRetentionModeArg {
    Discard,
    Summarise,
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

impl From<StopRetentionModeArg> for StopRetentionMode {
    fn from(value: StopRetentionModeArg) -> Self {
        match value {
            StopRetentionModeArg::Discard => StopRetentionMode::Discard,
            StopRetentionModeArg::Summarise => StopRetentionMode::Summarise,
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

    fn parse_cli<const N: usize>(args: [&'static str; N]) -> clap::error::Result<Cli> {
        std::thread::Builder::new()
            .name("cli-parse".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || Cli::try_parse_from(args))
            .expect("spawn parser thread")
            .join()
            .expect("parser thread")
    }

    fn into_command(cli: Cli) -> Command {
        cli.command
    }

    fn into_remote_command(cli: Cli) -> RemoteCommand {
        let Command::Remote { command, .. } = into_command(cli) else {
            panic!("expected remote command");
        };
        command
    }

    #[test]
    fn storage_prune_commands_parse() {
        let cli = parse_cli([
            "agent",
            "storage",
            "--prune-cache-days",
            "30",
            "--apply",
            "--json",
        ])
        .unwrap();
        let Command::Storage {
            json,
            prune_cache_days,
            apply,
        } = into_command(cli)
        else {
            panic!("expected storage command");
        };
        assert!(json);
        assert_eq!(prune_cache_days, Some(30));
        assert!(apply);

        let cli = parse_cli([
            "agent",
            "remote",
            "storage",
            "--prune-cache-days",
            "14",
            "--apply",
        ])
        .unwrap();
        let RemoteCommand::Storage {
            prune_cache_days,
            apply,
        } = into_remote_command(cli)
        else {
            panic!("expected remote storage command");
        };
        assert_eq!(prune_cache_days, Some(14));
        assert!(apply);
    }

    #[test]
    fn remote_bridge_delivery_commands_parse() {
        let cli = parse_cli(["agent", "remote", "bridge-deliveries", "list"]).unwrap();
        let RemoteCommand::BridgeDeliveries {
            command: RemoteBridgeDeliveryCommand::List,
        } = into_remote_command(cli)
        else {
            panic!("expected remote bridge delivery list command");
        };

        let cli = parse_cli([
            "agent",
            "remote",
            "bridge-deliveries",
            "retry",
            "bridge-delivery-1",
        ])
        .unwrap();
        let RemoteCommand::BridgeDeliveries {
            command: RemoteBridgeDeliveryCommand::Retry { id },
        } = into_remote_command(cli)
        else {
            panic!("expected remote bridge delivery retry command");
        };
        assert_eq!(id, "bridge-delivery-1");

        let cli = parse_cli(["agent", "remote", "bridge-deliveries", "retry-all"]).unwrap();
        let RemoteCommand::BridgeDeliveries {
            command: RemoteBridgeDeliveryCommand::RetryAll,
        } = into_remote_command(cli)
        else {
            panic!("expected remote bridge delivery retry-all command");
        };
    }

    #[test]
    fn skill_install_alias_matches_first_commands_spec() {
        let cli = parse_cli(["agent", "skill", "install", "./SKILL.md"]).unwrap();
        let Command::Skill {
            command: SkillCommand::Install { source },
        } = into_command(cli)
        else {
            panic!("expected skill install command");
        };
        assert_eq!(source, "./SKILL.md");
    }

    #[test]
    fn skill_allow_accepts_agent_scoped_spec_shape() {
        let cli = parse_cli(["agent", "skill", "allow", "research-agent", "skill-readme"]).unwrap();
        let Command::Skill {
            command: SkillCommand::Allow { agent_or_id, skill },
        } = into_command(cli)
        else {
            panic!("expected skill allow command");
        };
        assert_eq!(agent_or_id, "research-agent");
        assert_eq!(skill.as_deref(), Some("skill-readme"));
    }

    #[test]
    fn trace_hooks_command_parses() {
        let cli = parse_cli([
            "agent",
            "trace",
            "hooks",
            "00000000-0000-0000-0000-000000000000",
            "--json",
        ])
        .unwrap();
        let Command::Trace {
            command: TraceCommand::Hooks { run_id, json },
        } = into_command(cli)
        else {
            panic!("expected trace hooks command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert!(json);
    }

    #[test]
    fn trace_scores_command_parses() {
        let cli = parse_cli([
            "agent",
            "trace",
            "scores",
            "00000000-0000-0000-0000-000000000000",
            "--json",
        ])
        .unwrap();
        let Command::Trace {
            command: TraceCommand::Scores { run_id, json },
        } = into_command(cli)
        else {
            panic!("expected trace scores command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert!(json);

        let cli = parse_cli([
            "agent",
            "remote",
            "trace-scores",
            "00000000-0000-0000-0000-000000000000",
        ])
        .unwrap();
        let RemoteCommand::TraceScores { run_id } = into_remote_command(cli) else {
            panic!("expected remote trace scores command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn trace_tree_commands_parse() {
        let cli = parse_cli([
            "agent",
            "trace",
            "tree",
            "00000000-0000-0000-0000-000000000000",
            "--json",
        ])
        .unwrap();
        let Command::Trace {
            command: TraceCommand::Tree { run_id, json },
        } = into_command(cli)
        else {
            panic!("expected trace tree command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert!(json);

        let cli = parse_cli([
            "agent",
            "remote",
            "trace-tree",
            "00000000-0000-0000-0000-000000000000",
            "--json",
        ])
        .unwrap();
        let RemoteCommand::TraceTree { run_id, json } = into_remote_command(cli) else {
            panic!("expected remote trace-tree command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert!(json);
    }

    #[test]
    fn trace_compare_commands_parse() {
        let primary = "00000000-0000-0000-0000-000000000001";
        let compare = "00000000-0000-0000-0000-000000000002";
        let cli = parse_cli(["agent", "trace", "compare", primary, compare, "--json"]).unwrap();
        let Command::Trace {
            command:
                TraceCommand::Compare {
                    run_id,
                    compare_run_id,
                    json,
                },
        } = into_command(cli)
        else {
            panic!("expected trace compare command");
        };
        assert_eq!(run_id, primary);
        assert_eq!(compare_run_id, compare);
        assert!(json);

        let cli = parse_cli([
            "agent",
            "remote",
            "trace-compare",
            primary,
            compare,
            "--json",
        ])
        .unwrap();
        let RemoteCommand::TraceCompare {
            run_id,
            compare_run_id,
            json,
        } = into_remote_command(cli)
        else {
            panic!("expected remote trace-compare command");
        };
        assert_eq!(run_id, primary);
        assert_eq!(compare_run_id, compare);
        assert!(json);
    }

    #[test]
    fn hooks_disable_command_requires_confirm_flag() {
        let cli = parse_cli([
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
        } = into_command(cli)
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
        let cli = parse_cli([
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
        } = into_command(cli)
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
            parse_cli(["agent", "hooks", "available", "--agent", "critic", "--json"]).unwrap();
        let Command::Hooks {
            command: HookCommand::Available { agent, json },
        } = into_command(cli)
        else {
            panic!("expected hook available command");
        };
        assert_eq!(agent.as_deref(), Some("critic"));
        assert!(json);
    }

    #[test]
    fn approval_assess_command_accepts_controller_agent() {
        let cli = parse_cli([
            "agent",
            "approval",
            "assess",
            "00000000-0000-0000-0000-000000000001",
            "approval-c1",
            "--controller-agent",
            "safety-controller",
            "--json",
        ])
        .unwrap();
        let Command::Approval {
            command:
                ApprovalCommand::Assess {
                    run_id,
                    approval_id,
                    controller_agent,
                    json,
                },
        } = into_command(cli)
        else {
            panic!("expected approval assess command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(approval_id, "approval-c1");
        assert_eq!(controller_agent.as_deref(), Some("safety-controller"));
        assert!(json);
    }

    #[test]
    fn remote_approval_assess_command_accepts_controller_agent() {
        let cli = parse_cli([
            "agent",
            "remote",
            "--url",
            "http://localhost:8080",
            "approval",
            "assess",
            "00000000-0000-0000-0000-000000000001",
            "approval-c1",
            "--controller-agent",
            "safety-controller",
        ])
        .unwrap();
        let Command::Remote {
            command: RemoteCommand::Approval { command },
            ..
        } = into_command(cli)
        else {
            panic!("expected remote approval command");
        };
        let RemoteApprovalCommand::Assess {
            run_id,
            approval_id,
            controller_agent,
        } = command
        else {
            panic!("expected remote approval assess command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(approval_id, "approval-c1");
        assert_eq!(controller_agent.as_deref(), Some("safety-controller"));
    }

    #[test]
    fn agent_save_accepts_portable_agent_config_shape() {
        let cli = parse_cli([
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
            "--stop-retention-mode",
            "summarise",
            "--allow-tool",
            "echo",
            "--approval-controller-agent",
            "safety-controller",
            "--approval-controller-tool",
            "shell",
            "--approval-controller-tool-category",
            "sensitive",
            "--capability-drafts-enabled",
            "true",
            "--capability-draft-guidance",
            "Draft narrow reusable capabilities.",
            "--allow-skill-category",
            "review",
            "--skill-visibility-override",
            "review=name-only",
            "--skill-visibility",
            "name-and-description",
            "--tool-output-mode",
            "raw",
            "--tool-routing-model",
            "router-model",
            "--tool-output-interpretation-model",
            "general-interpreter",
            "--tool-output-override",
            "echo=raw",
            "--tool-interpretation-model",
            "echo=echo-interpreter",
            "--tool-guidance-override",
            "echo=Return exact echo JSON.",
            "--tool-visibility-override",
            "echo=full-schema",
            "--tool-visibility",
            "name-only",
            "--load-memory",
            "--memory-backend",
            "local-markdown-v0",
            "--memory-model",
            "memory-classifier",
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
                    stop_retention_mode,
                    allowed_tools,
                    allowed_skill_categories,
                    skill_visibility_overrides,
                    skill_visibility,
                    approval_controller_agent,
                    approval_controller_allowed_tools,
                    approval_controller_allowed_tool_categories,
                    capability_drafts_enabled,
                    capability_draft_guidance,
                    tool_output_mode,
                    tool_routing_model,
                    tool_output_interpretation_model,
                    tool_output_overrides,
                    tool_interpretation_model_overrides,
                    tool_guidance_overrides,
                    tool_visibility_overrides,
                    tool_visibility,
                    load_memory,
                    memory_backend,
                    memory_model,
                    load_skills,
                    ingestion_guardrail,
                    ingestion_guardrail_model,
                    refinement_instructions,
                    refinement_model,
                    refinement_aware,
                    ..
                },
        } = into_command(cli)
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
        assert!(matches!(
            stop_retention_mode,
            Some(StopRetentionModeArg::Summarise)
        ));
        assert_eq!(allowed_tools, vec!["echo"]);
        assert_eq!(allowed_skill_categories, vec!["review"]);
        assert_eq!(skill_visibility_overrides, vec!["review=name-only"]);
        assert!(matches!(
            skill_visibility,
            Some(ToolVisibility::NameAndDescription)
        ));
        assert_eq!(
            approval_controller_agent.as_deref(),
            Some("safety-controller")
        );
        assert_eq!(approval_controller_allowed_tools, vec!["shell"]);
        assert_eq!(
            approval_controller_allowed_tool_categories,
            vec!["sensitive"]
        );
        assert_eq!(capability_drafts_enabled, Some(true));
        assert_eq!(
            capability_draft_guidance.as_deref(),
            Some("Draft narrow reusable capabilities.")
        );
        assert!(matches!(tool_output_mode, Some(ToolOutputModeArg::Raw)));
        assert_eq!(tool_routing_model.as_deref(), Some("router-model"));
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
        assert_eq!(tool_visibility_overrides, vec!["echo=full-schema"]);
        assert!(matches!(tool_visibility, Some(ToolVisibility::NameOnly)));
        assert!(load_memory);
        assert_eq!(memory_backend.as_deref(), Some("local-markdown-v0"));
        assert_eq!(memory_model.as_deref(), Some("memory-classifier"));
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
        let cli = parse_cli([
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
            "--stop-retention-mode",
            "discard",
            "--allow-tool",
            "echo",
            "--approval-controller-agent",
            "safety-controller",
            "--approval-controller-tool",
            "shell",
            "--capability-drafts-enabled",
            "true",
            "--capability-draft-guidance",
            "Draft narrow reusable capabilities.",
            "--allow-skill-category",
            "review",
            "--skill-visibility-override",
            "review=full-schema",
            "--skill-visibility",
            "name-only",
            "--tool-routing-model",
            "router-model",
            "--tool-output-override",
            "echo=interpreted",
            "--tool-interpretation-model",
            "echo=echo-interpreter",
            "--tool-visibility-override",
            "echo=name-only",
            "--load-memory",
            "--memory-backend",
            "local-markdown-v0",
            "--memory-model",
            "memory-classifier",
            "--ingest-guardrail",
            "allow",
            "--ingest-guardrail-model",
            "guardrail-model",
            "--refinement-instructions",
            "Clarify first.",
            "--refinement-aware",
        ])
        .unwrap();
        let RemoteCommand::Agent {
            command:
                RemoteAgentCommand::Save {
                    id,
                    system_prompt,
                    max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    compaction_guidance,
                    max_subagent_depth,
                    max_recursion_depth,
                    stop_retention_mode,
                    allowed_tools,
                    allowed_skill_categories,
                    skill_visibility_overrides,
                    skill_visibility,
                    approval_controller_agent,
                    approval_controller_allowed_tools,
                    capability_drafts_enabled,
                    capability_draft_guidance,
                    tool_routing_model,
                    tool_output_overrides,
                    tool_interpretation_model_overrides,
                    tool_visibility_overrides,
                    load_memory,
                    memory_backend,
                    memory_model,
                    ingestion_guardrail,
                    ingestion_guardrail_model,
                    refinement_instructions,
                    refinement_aware,
                    ..
                },
        } = into_remote_command(cli)
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
        assert!(matches!(
            stop_retention_mode,
            Some(StopRetentionModeArg::Discard)
        ));
        assert_eq!(allowed_tools, vec!["echo"]);
        assert_eq!(allowed_skill_categories, vec!["review"]);
        assert_eq!(skill_visibility_overrides, vec!["review=full-schema"]);
        assert!(matches!(skill_visibility, Some(ToolVisibility::NameOnly)));
        assert_eq!(
            approval_controller_agent.as_deref(),
            Some("safety-controller")
        );
        assert_eq!(approval_controller_allowed_tools, vec!["shell"]);
        assert_eq!(capability_drafts_enabled, Some(true));
        assert_eq!(
            capability_draft_guidance.as_deref(),
            Some("Draft narrow reusable capabilities.")
        );
        assert_eq!(tool_routing_model.as_deref(), Some("router-model"));
        assert_eq!(tool_output_overrides, vec!["echo=interpreted"]);
        assert_eq!(
            tool_interpretation_model_overrides,
            vec!["echo=echo-interpreter"]
        );
        assert_eq!(tool_visibility_overrides, vec!["echo=name-only"]);
        assert!(load_memory);
        assert_eq!(memory_backend.as_deref(), Some("local-markdown-v0"));
        assert_eq!(memory_model.as_deref(), Some("memory-classifier"));
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
        let cli = parse_cli([
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
        let RemoteCommand::Run {
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
        } = into_remote_command(cli)
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
        let cli = parse_cli([
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
        } = into_command(cli)
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
        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected compact keep-run command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(conversation.as_deref(), Some("conv-1"));
        assert_eq!(guidance.as_deref(), Some("Keep decisions."));
        assert!(json);
    }

    #[test]
    fn remote_compact_keep_run_command_parses() {
        let cli = parse_cli([
            "agent",
            "remote",
            "--url",
            "http://127.0.0.1:7878",
            "compact",
            "keep-run",
            "00000000-0000-0000-0000-000000000001",
            "--conversation",
            "conv-1",
            "--guidance",
            "Keep decisions.",
        ])
        .unwrap();
        let RemoteCommand::Compact {
            command:
                RemoteCompactCommand::KeepRun {
                    run_id,
                    conversation,
                    guidance,
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote compact keep-run command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(conversation.as_deref(), Some("conv-1"));
        assert_eq!(guidance.as_deref(), Some("Keep decisions."));
    }

    #[test]
    fn compact_export_import_commands_parse() {
        let export_cli = parse_cli([
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
        } = into_command(export_cli)
        else {
            panic!("expected compact export command");
        };
        assert_eq!(id, "compact-1");
        assert_eq!(path, "/tmp/compact-1.json");
        assert!(json);

        let import_cli = parse_cli([
            "agent",
            "compact",
            "import",
            "/tmp/compact-1.json",
            "--json",
        ])
        .unwrap();
        let Command::Compact {
            command: CompactCommand::Import { path, json },
        } = into_command(import_cli)
        else {
            panic!("expected compact import command");
        };
        assert_eq!(path, "/tmp/compact-1.json");
        assert!(json);
    }

    #[test]
    fn local_provider_aliases_parse() {
        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::Ollama));
        assert_eq!(model.as_deref(), Some("llama3.1"));

        let cli = parse_cli([
            "agent",
            "run",
            "--provider",
            "llama-cpp",
            "--input",
            "hello",
        ])
        .unwrap();
        let Command::Run { provider, .. } = into_command(cli) else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::LlamaCpp));

        let cli = parse_cli([
            "agent",
            "run",
            "--provider",
            "anthropic",
            "--input",
            "hello",
        ])
        .unwrap();
        let Command::Run { provider, .. } = into_command(cli) else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::Anthropic));

        let cli = parse_cli(["agent", "run", "--provider", "gemini", "--input", "hello"]).unwrap();
        let Command::Run { provider, .. } = into_command(cli) else {
            panic!("expected run command");
        };
        assert!(matches!(provider, Provider::Gemini));
    }

    #[test]
    fn remote_run_events_accepts_after_cursor() {
        let cli = parse_cli([
            "agent",
            "remote",
            "run-events",
            "00000000-0000-0000-0000-000000000000",
            "--after",
            "4",
        ])
        .unwrap();
        let RemoteCommand::RunEvents { run_id, after } = into_remote_command(cli) else {
            panic!("expected remote run-events command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert_eq!(after, Some(4));
    }

    #[test]
    fn remote_run_wait_command_parses() {
        let cli = parse_cli([
            "agent",
            "remote",
            "run-wait",
            "00000000-0000-0000-0000-000000000000",
            "--poll-ms",
            "250",
            "--timeout-ms",
            "1000",
            "--events",
        ])
        .unwrap();
        let RemoteCommand::RunWait {
            run_id,
            poll_ms,
            timeout_ms,
            events,
        } = into_remote_command(cli)
        else {
            panic!("expected remote run-wait command");
        };
        assert_eq!(run_id, "00000000-0000-0000-0000-000000000000");
        assert_eq!(poll_ms, 250);
        assert_eq!(timeout_ms, Some(1000));
        assert!(events);
    }

    #[test]
    fn conversation_recover_command_parses() {
        let cli = parse_cli([
            "agent",
            "conversation",
            "recover",
            "conversation-1",
            "--json",
        ])
        .unwrap();
        let Command::Conversation {
            command: ConversationCommand::Recover { id, json },
        } = into_command(cli)
        else {
            panic!("expected conversation recover command");
        };
        assert_eq!(id, "conversation-1");
        assert!(json);
    }

    #[test]
    fn conversation_policy_command_parses() {
        let cli = parse_cli([
            "agent",
            "conversation",
            "policy",
            "conversation-1",
            "--load-memory",
            "false",
            "--generate-memory",
            "false",
            "--allow-tool-category",
            "shell",
            "--allow-tool-category",
            "mcp",
            "--allow-skill-category",
            "review",
            "--max-tokens-before-compaction",
            "512",
            "--max-compaction-output-tokens",
            "128",
            "--compaction-guidance",
            "keep decisions",
            "--json",
        ])
        .unwrap();
        let Command::Conversation {
            command:
                ConversationCommand::Policy {
                    id,
                    load_memory,
                    generate_memory,
                    allowed_tool_categories,
                    allowed_skill_categories,
                    max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    compaction_guidance,
                    json,
                    ..
                },
        } = into_command(cli)
        else {
            panic!("expected conversation policy command");
        };
        assert_eq!(id, "conversation-1");
        assert_eq!(load_memory, Some(false));
        assert_eq!(generate_memory, Some(false));
        assert_eq!(allowed_tool_categories, vec!["shell", "mcp"]);
        assert_eq!(allowed_skill_categories, vec!["review"]);
        assert_eq!(max_tokens_before_compaction, Some(512));
        assert_eq!(max_compaction_output_tokens, Some(128));
        assert_eq!(compaction_guidance.as_deref(), Some("keep decisions"));
        assert!(json);

        let cli = parse_cli([
            "agent",
            "remote",
            "conversation",
            "policy",
            "conversation-1",
            "--generate-memory",
            "true",
            "--allow-tool-category",
            "shell",
            "--clear-compaction-guidance",
        ])
        .unwrap();
        let RemoteCommand::Conversation {
            command:
                RemoteConversationCommand::Policy {
                    id,
                    generate_memory,
                    allowed_tool_categories,
                    clear_compaction_guidance,
                    ..
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote conversation policy command");
        };
        assert_eq!(id, "conversation-1");
        assert_eq!(generate_memory, Some(true));
        assert_eq!(allowed_tool_categories, vec!["shell"]);
        assert!(clear_compaction_guidance);

        let cli = parse_cli([
            "agent",
            "remote",
            "conversation",
            "delete-range",
            "conversation-1",
            "--from",
            "2",
            "--to",
            "4",
        ])
        .unwrap();
        let RemoteCommand::Conversation {
            command: RemoteConversationCommand::DeleteRange { id, from, to },
        } = into_remote_command(cli)
        else {
            panic!("expected remote conversation delete-range command");
        };
        assert_eq!(id, "conversation-1");
        assert_eq!(from, 2);
        assert_eq!(to, 4);
    }

    #[test]
    fn model_export_import_commands_parse() {
        let cli = parse_cli([
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
        } = into_command(cli)
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

        let cli = parse_cli([
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
        let RemoteCommand::Model {
            command:
                RemoteModelCommand::Save {
                    id,
                    provider,
                    allow_missing_api_key,
                    top_p,
                    ..
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote model save command");
        };
        assert_eq!(id, "remote-guard");
        assert_eq!(provider.as_deref(), Some("llama_cpp"));
        assert!(allow_missing_api_key);
        assert_eq!(top_p, Some(0.8));

        let cli = parse_cli(["agent", "model", "providers", "--json"]).unwrap();
        let Command::Model {
            command: ModelCommand::Providers { json },
        } = into_command(cli)
        else {
            panic!("expected model providers command");
        };
        assert!(json);

        let cli = parse_cli(["agent", "model", "doctor", "--json"]).unwrap();
        let Command::Model {
            command: ModelCommand::Doctor { json },
        } = into_command(cli)
        else {
            panic!("expected model doctor command");
        };
        assert!(json);

        let cli = parse_cli(["agent", "remote", "model", "providers"]).unwrap();
        let RemoteCommand::Model {
            command: RemoteModelCommand::Providers,
        } = into_remote_command(cli)
        else {
            panic!("expected remote model providers command");
        };

        let cli = parse_cli(["agent", "remote", "model", "doctor"]).unwrap();
        let RemoteCommand::Model {
            command: RemoteModelCommand::Doctor,
        } = into_remote_command(cli)
        else {
            panic!("expected remote model doctor command");
        };

        let cli = parse_cli(["agent", "model", "probe", "gpt-test", "--json"]).unwrap();
        let Command::Model {
            command: ModelCommand::Probe { id, json },
        } = into_command(cli)
        else {
            panic!("expected model probe command");
        };
        assert_eq!(id, "gpt-test");
        assert!(json);

        let cli = parse_cli(["agent", "remote", "model", "probe", "remote-guard"]).unwrap();
        let RemoteCommand::Model {
            command: RemoteModelCommand::Probe { id },
        } = into_remote_command(cli)
        else {
            panic!("expected remote model probe command");
        };
        assert_eq!(id, "remote-guard");

        let cli = parse_cli(["agent", "model", "export", "gpt-test", "./gpt-test.toml"]).unwrap();
        let Command::Model {
            command: ModelCommand::Export { id, path, json },
        } = into_command(cli)
        else {
            panic!("expected model export command");
        };
        assert_eq!(id, "gpt-test");
        assert_eq!(path, "./gpt-test.toml");
        assert!(!json);

        let cli = parse_cli([
            "agent",
            "model",
            "provider-catalog",
            "export",
            "./providers.json",
        ])
        .unwrap();
        let Command::Model {
            command:
                ModelCommand::ProviderCatalog {
                    command: ModelProviderCatalogCommand::Export { path, json },
                },
        } = into_command(cli)
        else {
            panic!("expected model provider catalog export command");
        };
        assert_eq!(path, "./providers.json");
        assert!(!json);

        let cli = parse_cli([
            "agent",
            "remote",
            "model",
            "provider-catalog",
            "import",
            "./providers.json",
        ])
        .unwrap();
        let RemoteCommand::Model {
            command:
                RemoteModelCommand::ProviderCatalog {
                    command: RemoteModelProviderCatalogCommand::Import { path },
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote model provider catalog import command");
        };
        assert_eq!(path, "./providers.json");

        let cli = parse_cli([
            "agent",
            "model",
            "metadata-catalog",
            "export",
            "./metadata.json",
            "--json",
        ])
        .unwrap();
        let Command::Model {
            command:
                ModelCommand::MetadataCatalog {
                    command: ModelMetadataCatalogCommand::Export { path, json },
                },
        } = into_command(cli)
        else {
            panic!("expected model metadata catalog export command");
        };
        assert_eq!(path, "./metadata.json");
        assert!(json);

        let cli = parse_cli([
            "agent",
            "remote",
            "model",
            "metadata-catalog",
            "import",
            "./metadata.json",
        ])
        .unwrap();
        let RemoteCommand::Model {
            command:
                RemoteModelCommand::MetadataCatalog {
                    command: RemoteModelMetadataCatalogCommand::Import { path },
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote model metadata catalog import command");
        };
        assert_eq!(path, "./metadata.json");

        let cli = parse_cli(["agent", "remote", "model", "import", "./gpt-test.toml"]).unwrap();
        let RemoteCommand::Model {
            command: RemoteModelCommand::Import { path },
        } = into_remote_command(cli)
        else {
            panic!("expected remote model import command");
        };
        assert_eq!(path, "./gpt-test.toml");
    }

    #[test]
    fn secret_commands_parse() {
        let cli = parse_cli(["agent", "secrets", "backends", "--json"]).unwrap();
        let Command::Secrets {
            command: SecretsCommand::Backends { json },
        } = into_command(cli)
        else {
            panic!("expected secrets backends command");
        };
        assert!(json);

        let cli = parse_cli([
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
        } = into_command(cli)
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
        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected capability propose command");
        };
        assert_eq!(kind, "skill");
        assert_eq!(name, "Review Skill");
        assert_eq!(body.as_deref(), Some("Use the checklist."));
        assert_eq!(created_by, "agent");
        assert!(json);

        let cli = parse_cli([
            "agent",
            "remote",
            "--url",
            "http://127.0.0.1:7878",
            "capability",
            "allow",
            "draft-review",
        ])
        .unwrap();
        let RemoteCommand::Capability {
            command: RemoteCapabilityCommand::Allow { id },
        } = into_remote_command(cli)
        else {
            panic!("expected remote capability allow command");
        };
        assert_eq!(id, "draft-review");

        let cli = parse_cli([
            "agent",
            "capability",
            "export",
            "draft-review",
            "./draft-review.json",
            "--json",
        ])
        .unwrap();
        let Command::Capability {
            command: CapabilityCommand::Export { id, path, json },
        } = into_command(cli)
        else {
            panic!("expected capability export command");
        };
        assert_eq!(id, "draft-review");
        assert_eq!(path, "./draft-review.json");
        assert!(json);

        let cli = parse_cli([
            "agent",
            "remote",
            "--url",
            "http://127.0.0.1:7878",
            "capability",
            "import",
            "./draft-review.json",
        ])
        .unwrap();
        let RemoteCommand::Capability {
            command: RemoteCapabilityCommand::Import { path },
        } = into_remote_command(cli)
        else {
            panic!("expected remote capability import command");
        };
        assert_eq!(path, "./draft-review.json");
    }

    #[test]
    fn resume_commands_parse() {
        let run_id = "00000000-0000-0000-0000-000000000000";
        let cli = parse_cli(["agent", "resume", run_id, "--from-event", "7", "--json"]).unwrap();
        let Command::Resume {
            run_id: parsed_id,
            from_event,
            json,
            ..
        } = into_command(cli)
        else {
            panic!("expected resume command");
        };
        assert_eq!(parsed_id, run_id);
        assert_eq!(from_event, Some(7));
        assert!(json);

        let cli = parse_cli([
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
        let RemoteCommand::Resume {
            run_id: parsed_id,
            from_event,
            ..
        } = into_remote_command(cli)
        else {
            panic!("expected remote resume command");
        };
        assert_eq!(parsed_id, run_id);
        assert_eq!(from_event, Some(3));

        let cli = parse_cli([
            "agent",
            "remote",
            "resume-start",
            run_id,
            "--from-event",
            "4",
        ])
        .unwrap();
        let RemoteCommand::ResumeStart {
            run_id: parsed_id,
            from_event,
            ..
        } = into_remote_command(cli)
        else {
            panic!("expected remote resume-start command");
        };
        assert_eq!(parsed_id, run_id);
        assert_eq!(from_event, Some(4));
    }

    #[test]
    fn skill_export_import_commands_parse() {
        let cli = parse_cli(["agent", "skill", "export", "review", "./review.skill.json"]).unwrap();
        let Command::Skill {
            command: SkillCommand::Export { id, path, json },
        } = into_command(cli)
        else {
            panic!("expected skill export command");
        };
        assert_eq!(id, "review");
        assert_eq!(path, "./review.skill.json");
        assert!(!json);

        let cli = parse_cli(["agent", "remote", "skill", "import", "./review.skill.json"]).unwrap();
        let RemoteCommand::Skill {
            command: RemoteSkillCommand::Import { path },
        } = into_remote_command(cli)
        else {
            panic!("expected remote skill import command");
        };
        assert_eq!(path, "./review.skill.json");
    }

    #[test]
    fn prompt_commands_accept_agent_scope() {
        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected prompt save command");
        };
        assert_eq!(name, "daily");
        assert_eq!(text, "Review carefully.");
        assert_eq!(agent.as_deref(), Some("critic"));

        let cli = parse_cli(["agent", "remote", "prompt", "list", "--agent", "critic"]).unwrap();
        let RemoteCommand::Prompt {
            command: RemotePromptCommand::List { agent },
        } = into_remote_command(cli)
        else {
            panic!("expected remote prompt list command");
        };
        assert_eq!(agent.as_deref(), Some("critic"));
    }

    #[test]
    fn memory_export_import_commands_parse() {
        let cli = parse_cli(["agent", "memory", "backends", "--json"]).unwrap();
        let Command::Memory {
            command: MemoryCommand::Backends { json },
        } = into_command(cli)
        else {
            panic!("expected memory backends command");
        };
        assert!(json);

        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected memory export command");
        };
        assert_eq!(path, "./memory.md");
        assert!(user);
        assert!(json);

        let cli = parse_cli([
            "agent",
            "memory",
            "generate-conversation",
            "conv-1",
            "--from",
            "2",
            "--to",
            "4",
            "--agent",
            "critic",
            "--topic",
            "finance",
        ])
        .unwrap();
        let Command::Memory {
            command:
                MemoryCommand::GenerateConversation {
                    id,
                    from,
                    to,
                    agent,
                    topics,
                    ..
                },
        } = into_command(cli)
        else {
            panic!("expected memory generate-conversation command");
        };
        assert_eq!(id, "conv-1");
        assert_eq!(from, Some(2));
        assert_eq!(to, Some(4));
        assert_eq!(agent.as_deref(), Some("critic"));
        assert_eq!(topics, vec!["finance"]);

        let cli = parse_cli([
            "agent", "memory", "access", "--topic", "finance", "--topic", "ops", "--json",
        ])
        .unwrap();
        let Command::Memory {
            command: MemoryCommand::Access { topics, json },
        } = into_command(cli)
        else {
            panic!("expected memory access command");
        };
        assert_eq!(topics, vec!["finance", "ops"]);
        assert!(json);

        let cli = parse_cli([
            "agent",
            "memory",
            "classify",
            "mem-local",
            "--model",
            "classifier",
            "--agent",
            "critic",
            "--no-apply",
        ])
        .unwrap();
        let Command::Memory {
            command:
                MemoryCommand::Classify {
                    id,
                    model,
                    agent,
                    no_apply,
                },
        } = into_command(cli)
        else {
            panic!("expected memory classify command");
        };
        assert_eq!(id, "mem-local");
        assert_eq!(model.as_deref(), Some("classifier"));
        assert_eq!(agent.as_deref(), Some("critic"));
        assert!(no_apply);

        let cli = parse_cli([
            "agent",
            "remote",
            "memory",
            "import",
            "./memory.md",
            "--user",
            "--agent",
            "critic",
        ])
        .unwrap();
        let RemoteCommand::Memory {
            command:
                RemoteMemoryCommand::Import {
                    path, user, agent, ..
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote memory import command");
        };
        assert_eq!(path, "./memory.md");
        assert!(user);
        assert_eq!(agent.as_deref(), Some("critic"));

        let cli = parse_cli(["agent", "remote", "memory", "backends"]).unwrap();
        let RemoteCommand::Memory {
            command: RemoteMemoryCommand::Backends,
        } = into_remote_command(cli)
        else {
            panic!("expected remote memory backends command");
        };

        let cli = parse_cli([
            "agent", "remote", "memory", "access", "--topic", "finance", "--topic", "ops",
        ])
        .unwrap();
        let RemoteCommand::Memory {
            command: RemoteMemoryCommand::Access { topics },
        } = into_remote_command(cli)
        else {
            panic!("expected remote memory access command");
        };
        assert_eq!(topics, vec!["finance", "ops"]);

        let cli = parse_cli([
            "agent",
            "remote",
            "memory",
            "generate-pending",
            "--user",
            "--limit",
            "3",
            "--topic",
            "finance",
            "--topic",
            "ops",
        ])
        .unwrap();
        let RemoteCommand::Memory {
            command:
                RemoteMemoryCommand::GeneratePending {
                    user,
                    limit,
                    topics,
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote memory generate-pending command");
        };
        assert!(user);
        assert_eq!(limit, Some(3));
        assert_eq!(topics, vec!["finance", "ops"]);

        let cli = parse_cli([
            "agent",
            "remote",
            "memory",
            "classify",
            "mem-1",
            "--model",
            "classifier",
            "--agent",
            "critic",
            "--no-apply",
        ])
        .unwrap();
        let RemoteCommand::Memory {
            command:
                RemoteMemoryCommand::Classify {
                    id,
                    model,
                    agent,
                    no_apply,
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote memory classify command");
        };
        assert_eq!(id, "mem-1");
        assert_eq!(model.as_deref(), Some("classifier"));
        assert_eq!(agent.as_deref(), Some("critic"));
        assert!(no_apply);

        let cli = parse_cli([
            "agent",
            "remote",
            "memory",
            "generate-conversation",
            "conv-remote",
            "--from",
            "1",
            "--to",
            "1",
            "--user",
        ])
        .unwrap();
        let RemoteCommand::Memory {
            command:
                RemoteMemoryCommand::GenerateConversation {
                    id, from, to, user, ..
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote memory generate-conversation command");
        };
        assert_eq!(id, "conv-remote");
        assert_eq!(from, Some(1));
        assert_eq!(to, Some(1));
        assert!(user);
    }

    #[test]
    fn ingest_backends_commands_parse() {
        let cli = parse_cli(["agent", "ingest", "backends", "--json"]).unwrap();
        let Command::Ingest {
            command: IngestCommand::Backends { json },
        } = into_command(cli)
        else {
            panic!("expected ingest backends command");
        };
        assert!(json);

        let cli = parse_cli([
            "agent",
            "ingest",
            "probe-vision",
            "scan.pdf",
            "--model",
            "gpt-4o",
            "--json",
        ])
        .unwrap();
        let Command::Ingest {
            command: IngestCommand::ProbeVision { path, model, json },
        } = into_command(cli)
        else {
            panic!("expected ingest probe-vision command");
        };
        assert_eq!(path, "scan.pdf");
        assert_eq!(model, "gpt-4o");
        assert!(json);

        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected ingest add command");
        };
        assert_eq!(path, "doc.md");
        assert_eq!(backend, "local-lines-v0");
        assert_eq!(vision_model.as_deref(), Some("gpt-4o"));
        assert_eq!(guardrail_model.as_deref(), Some("gpt-4o-mini"));

        let cli = parse_cli([
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
        let RemoteCommand::Ingest {
            command:
                RemoteIngestCommand::Add {
                    path,
                    vision_model,
                    guardrail_model,
                    ..
                },
        } = into_remote_command(cli)
        else {
            panic!("expected remote ingest add command");
        };
        assert_eq!(path, "doc.md");
        assert_eq!(vision_model.as_deref(), Some("gpt-4o"));
        assert_eq!(guardrail_model.as_deref(), Some("gpt-4o-mini"));

        let cli = parse_cli([
            "agent",
            "remote",
            "ingest",
            "probe-vision",
            "scan.pdf",
            "--model",
            "gpt-4o",
        ])
        .unwrap();
        let RemoteCommand::Ingest {
            command: RemoteIngestCommand::ProbeVision { path, model },
        } = into_remote_command(cli)
        else {
            panic!("expected remote ingest probe-vision command");
        };
        assert_eq!(path, "scan.pdf");
        assert_eq!(model, "gpt-4o");

        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected ingest review command");
        };
        assert_eq!(id, "ingest-local-v0-abc");
        assert_eq!(finding, 0);
        assert_eq!(decision, "approve");
        assert_eq!(note.as_deref(), Some("looks intentional"));

        let cli = parse_cli(["agent", "remote", "ingest", "backends"]).unwrap();
        let RemoteCommand::Ingest {
            command: RemoteIngestCommand::Backends,
        } = into_remote_command(cli)
        else {
            panic!("expected remote ingest backends command");
        };
    }

    #[test]
    fn artifact_commands_parse() {
        let cli = parse_cli(["agent", "artifact", "open", "report.pdf", "--json"]).unwrap();
        let Command::Artifact {
            command: ArtifactCommand::Open { id, json },
        } = into_command(cli)
        else {
            panic!("expected artifact open command");
        };
        assert_eq!(id, "report.pdf");
        assert!(json);

        let cli = parse_cli(["agent", "remote", "artifact", "show", "report.pdf"]).unwrap();
        let RemoteCommand::Artifact {
            command: RemoteArtifactCommand::Show { id },
        } = into_remote_command(cli)
        else {
            panic!("expected remote artifact show command");
        };
        assert_eq!(id, "report.pdf");
    }

    #[test]
    fn adapter_clawhub_install_matches_source_provider_spec() {
        let cli = parse_cli([
            "agent",
            "adapter",
            "export",
            "adapter-demo",
            "./adapter.json",
            "--json",
        ])
        .unwrap();
        let Command::Adapter {
            command: AdapterCommand::Export { id, path, json },
        } = into_command(cli)
        else {
            panic!("expected adapter export command");
        };
        assert_eq!(id, "adapter-demo");
        assert_eq!(path, "./adapter.json");
        assert!(json);

        let cli = parse_cli(["agent", "adapter", "doctor", "--json"]).unwrap();
        let Command::Adapter {
            command: AdapterCommand::Doctor { json },
        } = into_command(cli)
        else {
            panic!("expected adapter doctor command");
        };
        assert!(json);

        let cli = parse_cli(["agent", "adapter", "install-skill", "adapter-demo"]).unwrap();
        let Command::Adapter {
            command: AdapterCommand::InstallSkill { id, json },
        } = into_command(cli)
        else {
            panic!("expected adapter install-skill command");
        };
        assert_eq!(id, "adapter-demo");
        assert!(!json);

        let cli = parse_cli(["agent", "remote", "adapter", "doctor"]).unwrap();
        let RemoteCommand::Adapter {
            command: RemoteAdapterCommand::Doctor,
        } = into_remote_command(cli)
        else {
            panic!("expected remote adapter doctor command");
        };

        let cli = parse_cli([
            "agent",
            "remote",
            "adapter",
            "install-skill",
            "adapter-demo",
        ])
        .unwrap();
        let RemoteCommand::Adapter {
            command: RemoteAdapterCommand::InstallSkill { id },
        } = into_remote_command(cli)
        else {
            panic!("expected remote adapter install-skill command");
        };
        assert_eq!(id, "adapter-demo");

        let cli = parse_cli([
            "agent",
            "remote",
            "adapter",
            "import-manifest",
            "./adapter.json",
        ])
        .unwrap();
        let RemoteCommand::Adapter {
            command: RemoteAdapterCommand::ImportManifest { path },
        } = into_remote_command(cli)
        else {
            panic!("expected remote adapter import-manifest command");
        };
        assert_eq!(path, "./adapter.json");

        let cli = parse_cli([
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
        } = into_command(cli)
        else {
            panic!("expected adapter clawhub install command");
        };
        assert_eq!(catalog, "catalog.json");
        assert_eq!(id, "demo");
    }

    #[test]
    fn remote_adapter_clawhub_search_uses_same_command_shape() {
        let cli = parse_cli([
            "agent",
            "remote",
            "adapter",
            "clawhub",
            "search",
            "catalog.json",
            "docs",
        ])
        .unwrap();
        let RemoteCommand::Adapter {
            command:
                RemoteAdapterCommand::Clawhub {
                    command: ClawHubCommand::Search { catalog, query, .. },
                },
        } = into_remote_command(cli)
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
            memory_topics,
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
                memory_topics,
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
            memory_topics,
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
                memory_topics,
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
            command: TraceCommand::Tree { run_id, json },
        } => headless::trace_tree(run_id, json).await,
        Command::Trace {
            command:
                TraceCommand::Compare {
                    run_id,
                    compare_run_id,
                    json,
                },
        } => headless::trace_compare(run_id, compare_run_id, json).await,
        Command::Trace {
            command: TraceCommand::Hooks { run_id, json },
        } => headless::trace_hooks(run_id, json).await,
        Command::Trace {
            command: TraceCommand::Scores { run_id, json },
        } => headless::trace_scores(run_id, json).await,
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
            ApprovalCommand::Assess {
                run_id,
                approval_id,
                controller_agent,
                json,
            } => headless::approval_assess(run_id, approval_id, controller_agent, json).await,
            ApprovalCommand::Decide {
                run_id,
                approval_id,
                approve,
                unlock_env,
                signature_env,
                controller_agent,
            } => {
                headless::approval_decide(
                    run_id,
                    approval_id,
                    approve,
                    unlock_env,
                    signature_env,
                    controller_agent,
                )
                .await
            }
            ApprovalCommand::Approve {
                run_id,
                approval_id,
                unlock_env,
                signature_env,
                controller_agent,
            } => {
                headless::approval_decide(
                    run_id,
                    approval_id,
                    true,
                    unlock_env,
                    signature_env,
                    controller_agent,
                )
                .await
            }
            ApprovalCommand::Reject {
                run_id,
                approval_id,
            } => headless::approval_decide(run_id, approval_id, false, None, None, None).await,
            ApprovalCommand::Execute {
                run_id,
                approval_id,
                json,
                unlock_env,
                signature_env,
            } => {
                headless::approval_execute(run_id, approval_id, json, unlock_env, signature_env)
                    .await
            }
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
        Command::Storage {
            json,
            prune_cache_days,
            apply,
        } => headless::storage_report(json, prune_cache_days, apply).await,
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
            CapabilityCommand::Export { id, path, json } => {
                headless::capability_export(id, path, json).await
            }
            CapabilityCommand::Import { path, json } => {
                headless::capability_import(path, json).await
            }
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
            ConversationCommand::Policy {
                id,
                load_memory,
                clear_load_memory,
                generate_memory,
                clear_generate_memory,
                allowed_tool_categories,
                clear_allowed_tool_categories,
                allowed_skill_categories,
                clear_allowed_skill_categories,
                capability_drafts_enabled,
                clear_capability_drafts_enabled,
                capability_draft_guidance,
                clear_capability_draft_guidance,
                max_tokens_before_compaction,
                clear_max_tokens_before_compaction,
                max_compaction_output_tokens,
                clear_max_compaction_output_tokens,
                compaction_guidance,
                clear_compaction_guidance,
                clear,
                json,
            } => {
                let options = headless::ConversationPolicyOptions {
                    load_memory,
                    clear_load_memory,
                    generate_memory,
                    clear_generate_memory,
                    allowed_tool_categories,
                    clear_allowed_tool_categories,
                    allowed_skill_categories,
                    clear_allowed_skill_categories,
                    capability_drafts_enabled,
                    clear_capability_drafts_enabled,
                    capability_draft_guidance,
                    clear_capability_draft_guidance,
                    max_tokens_before_compaction,
                    clear_max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    clear_max_compaction_output_tokens,
                    compaction_guidance,
                    clear_compaction_guidance,
                    clear,
                    json,
                };
                headless::conversation_policy(id, options).await
            }
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
                agent,
                topics,
            } => headless::memory_create(content, user, conversation, agent, topics).await,
            MemoryCommand::Generate {
                text,
                user,
                range,
                conversation,
                agent,
                topics,
            } => headless::memory_generate(text, user, range, conversation, agent, topics).await,
            MemoryCommand::GenerateConversation {
                id,
                from,
                to,
                user,
                agent,
                topics,
            } => headless::memory_generate_conversation(id, from, to, user, agent, topics).await,
            MemoryCommand::List { json } => headless::memory_list(json).await,
            MemoryCommand::Access { topics, json } => headless::memory_access(topics, json).await,
            MemoryCommand::Backends { json } => headless::memory_backends(json).await,
            MemoryCommand::Classify {
                id,
                model,
                agent,
                no_apply,
            } => headless::memory_classify(id, model, agent, !no_apply).await,
            MemoryCommand::Edit { id, content } => headless::memory_edit(id, content).await,
            MemoryCommand::Delete { id } => headless::memory_delete(id).await,
            MemoryCommand::Rollback { user } => headless::memory_rollback(user).await,
            MemoryCommand::Export { path, user, json } => {
                headless::memory_export(path, user, json).await
            }
            MemoryCommand::Import {
                path,
                user,
                agent,
                json,
            } => headless::memory_import(path, user, agent, json).await,
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
                stop_retention_mode,
                allowed_tools,
                allowed_tool_categories,
                approval_controller_agent,
                approval_controller_allowed_tools,
                approval_controller_allowed_tool_categories,
                capability_drafts_enabled,
                capability_draft_guidance,
                allowed_skill_categories,
                skill_visibility_overrides,
                skill_visibility,
                tool_output_mode,
                tool_routing_model,
                tool_output_interpretation_model,
                tool_output_overrides,
                tool_interpretation_model_overrides,
                tool_guidance_overrides,
                tool_visibility_overrides,
                tool_visibility,
                load_memory,
                memory_backend,
                memory_model,
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
                    stop_retention_mode.map(StopRetentionMode::from),
                    allowed_tools,
                    allowed_tool_categories,
                    approval_controller_agent,
                    approval_controller_allowed_tools,
                    approval_controller_allowed_tool_categories,
                    capability_drafts_enabled,
                    capability_draft_guidance,
                    allowed_skill_categories,
                    skill_visibility_overrides,
                    skill_visibility.map(VisibilityLevel::from),
                    tool_output_mode.map(ToolOutputMode::from),
                    tool_routing_model,
                    tool_output_interpretation_model,
                    tool_output_overrides,
                    tool_interpretation_model_overrides,
                    tool_guidance_overrides,
                    tool_visibility_overrides,
                    tool_visibility.map(VisibilityLevel::from),
                    load_memory,
                    memory_backend,
                    memory_model,
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
            ModelCommand::Doctor { json } => headless::model_doctor(json).await,
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
            ModelCommand::ProviderCatalog { command } => match command {
                ModelProviderCatalogCommand::Show { json } => {
                    headless::model_provider_catalog_show(json).await
                }
                ModelProviderCatalogCommand::Export { path, json } => {
                    headless::model_provider_catalog_export(path, json).await
                }
                ModelProviderCatalogCommand::Import { path, json } => {
                    headless::model_provider_catalog_import(path, json).await
                }
            },
            ModelCommand::MetadataCatalog { command } => match command {
                ModelMetadataCatalogCommand::Show { json } => {
                    headless::model_metadata_catalog_show(json).await
                }
                ModelMetadataCatalogCommand::Export { path, json } => {
                    headless::model_metadata_catalog_export(path, json).await
                }
                ModelMetadataCatalogCommand::Import { path, json } => {
                    headless::model_metadata_catalog_import(path, json).await
                }
            },
        },
        Command::Ingest { command } => match command {
            IngestCommand::Backends { json } => headless::ingest_backends(json).await,
            IngestCommand::ProbeVision { path, model, json } => {
                headless::ingest_probe_vision(path, model, json).await
            }
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
            ArtifactCommand::Delete { id, json } => headless::artifact_delete(id, json).await,
        },
        Command::Adapter { command } => match command {
            AdapterCommand::Import { path } => headless::adapter_import(path).await,
            AdapterCommand::ImportManifest { path, json } => {
                headless::adapter_import_manifest(path, json).await
            }
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
            AdapterCommand::Doctor { json } => headless::adapter_doctor(json).await,
            AdapterCommand::InstallSkill { id, json } => {
                headless::adapter_install_skill(id, json).await
            }
            AdapterCommand::Inspect { path, json } => headless::adapter_inspect(path, json).await,
            AdapterCommand::Show { id, json } => headless::adapter_show(id, json).await,
            AdapterCommand::Export { id, path, json } => {
                headless::adapter_export(id, path, json).await
            }
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
                memory_topics,
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
                    memory_topics,
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
                memory_topics,
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
                    memory_topics,
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
            RemoteCommand::RunWait {
                run_id,
                poll_ms,
                timeout_ms,
                events,
            } => headless::remote_run_wait(url, run_id, poll_ms, timeout_ms, events).await,
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
                memory_topics,
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
                    memory_topics,
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
            RemoteCommand::ResumeStart {
                run_id,
                from_event,
                demo,
            } => headless::remote_resume_start(url, run_id, from_event, demo).await,
            RemoteCommand::Score {
                run_id,
                score,
                target,
            } => headless::remote_score(url, run_id, target, score).await,
            RemoteCommand::Storage {
                prune_cache_days,
                apply,
            } => headless::remote_storage_report(url, prune_cache_days, apply).await,
            RemoteCommand::BridgeDeliveries { command } => match command {
                RemoteBridgeDeliveryCommand::List => {
                    headless::remote_bridge_delivery_list(url).await
                }
                RemoteBridgeDeliveryCommand::Retry { id } => {
                    headless::remote_bridge_delivery_retry(url, id).await
                }
                RemoteBridgeDeliveryCommand::RetryAll => {
                    headless::remote_bridge_delivery_retry_all(url).await
                }
            },
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
            RemoteCommand::TraceTree { run_id, json } => {
                headless::remote_trace_tree(url, run_id, json).await
            }
            RemoteCommand::TraceCompare {
                run_id,
                compare_run_id,
                json,
            } => headless::remote_trace_compare(url, run_id, compare_run_id, json).await,
            RemoteCommand::TraceHooks { run_id } => headless::remote_trace_hooks(url, run_id).await,
            RemoteCommand::TraceScores { run_id } => {
                headless::remote_trace_scores(url, run_id).await
            }
            RemoteCommand::Approval { command } => match command {
                RemoteApprovalCommand::List { run_id } => {
                    headless::remote_approval_list(url, run_id).await
                }
                RemoteApprovalCommand::Assess {
                    run_id,
                    approval_id,
                    controller_agent,
                } => {
                    headless::remote_approval_assess(url, run_id, approval_id, controller_agent)
                        .await
                }
                RemoteApprovalCommand::Decide {
                    run_id,
                    approval_id,
                    approve,
                    unlock_env,
                    signature_env,
                    controller_agent,
                } => {
                    headless::remote_approval_decide(
                        url,
                        run_id,
                        approval_id,
                        approve,
                        unlock_env,
                        signature_env,
                        controller_agent,
                    )
                    .await
                }
                RemoteApprovalCommand::Approve {
                    run_id,
                    approval_id,
                    unlock_env,
                    signature_env,
                    controller_agent,
                } => {
                    headless::remote_approval_decide(
                        url,
                        run_id,
                        approval_id,
                        true,
                        unlock_env,
                        signature_env,
                        controller_agent,
                    )
                    .await
                }
                RemoteApprovalCommand::Reject {
                    run_id,
                    approval_id,
                } => {
                    headless::remote_approval_decide(
                        url,
                        run_id,
                        approval_id,
                        false,
                        None,
                        None,
                        None,
                    )
                    .await
                }
                RemoteApprovalCommand::Execute {
                    run_id,
                    approval_id,
                    unlock_env,
                    signature_env,
                } => {
                    headless::remote_approval_execute(
                        url,
                        run_id,
                        approval_id,
                        unlock_env,
                        signature_env,
                    )
                    .await
                }
            },
            RemoteCommand::Conversation { command } => match command {
                RemoteConversationCommand::List => headless::remote_conversation_list(url).await,
                RemoteConversationCommand::Tree => headless::remote_conversation_tree(url).await,
                RemoteConversationCommand::Show { id } => {
                    headless::remote_conversation_show(url, id).await
                }
                RemoteConversationCommand::Recover { id } => {
                    headless::remote_conversation_recover(url, id).await
                }
                RemoteConversationCommand::Policy {
                    id,
                    load_memory,
                    clear_load_memory,
                    generate_memory,
                    clear_generate_memory,
                    allowed_tool_categories,
                    clear_allowed_tool_categories,
                    allowed_skill_categories,
                    clear_allowed_skill_categories,
                    capability_drafts_enabled,
                    clear_capability_drafts_enabled,
                    capability_draft_guidance,
                    clear_capability_draft_guidance,
                    max_tokens_before_compaction,
                    clear_max_tokens_before_compaction,
                    max_compaction_output_tokens,
                    clear_max_compaction_output_tokens,
                    compaction_guidance,
                    clear_compaction_guidance,
                    clear,
                } => {
                    let options = headless::ConversationPolicyOptions {
                        load_memory,
                        clear_load_memory,
                        generate_memory,
                        clear_generate_memory,
                        allowed_tool_categories,
                        clear_allowed_tool_categories,
                        allowed_skill_categories,
                        clear_allowed_skill_categories,
                        capability_drafts_enabled,
                        clear_capability_drafts_enabled,
                        capability_draft_guidance,
                        clear_capability_draft_guidance,
                        max_tokens_before_compaction,
                        clear_max_tokens_before_compaction,
                        max_compaction_output_tokens,
                        clear_max_compaction_output_tokens,
                        compaction_guidance,
                        clear_compaction_guidance,
                        clear,
                        json: true,
                    };
                    headless::remote_conversation_policy(url, id, options).await
                }
                RemoteConversationCommand::DeletePlan { id, recursive } => {
                    headless::remote_conversation_delete_plan(url, id, recursive).await
                }
                RemoteConversationCommand::Delete { id, recursive } => {
                    headless::remote_conversation_delete(url, id, recursive).await
                }
                RemoteConversationCommand::DeleteAgentPlan { agent, recursive } => {
                    headless::remote_conversation_delete_agent_plan(url, agent, recursive).await
                }
                RemoteConversationCommand::DeleteAgent { agent, recursive } => {
                    headless::remote_conversation_delete_agent(url, agent, recursive).await
                }
                RemoteConversationCommand::DeleteRange { id, from, to } => {
                    headless::remote_conversation_delete_range(url, id, from, to).await
                }
            },
            RemoteCommand::Memory { command } => match command {
                RemoteMemoryCommand::List => headless::remote_memory_list(url).await,
                RemoteMemoryCommand::Access { topics } => {
                    headless::remote_memory_access(url, topics).await
                }
                RemoteMemoryCommand::Backends => headless::remote_memory_backends(url).await,
                RemoteMemoryCommand::Create {
                    content,
                    user,
                    agent,
                    topics,
                } => headless::remote_memory_create(url, content, user, agent, topics).await,
                RemoteMemoryCommand::Generate {
                    text,
                    user,
                    range,
                    agent,
                    topics,
                } => headless::remote_memory_generate(url, text, user, range, agent, topics).await,
                RemoteMemoryCommand::GenerateConversation {
                    id,
                    from,
                    to,
                    user,
                    agent,
                    topics,
                } => {
                    headless::remote_memory_generate_conversation(
                        url, id, from, to, user, agent, topics,
                    )
                    .await
                }
                RemoteMemoryCommand::GeneratePending {
                    user,
                    limit,
                    topics,
                } => headless::remote_memory_generate_pending(url, user, limit, topics).await,
                RemoteMemoryCommand::Classify {
                    id,
                    model,
                    agent,
                    no_apply,
                } => headless::remote_memory_classify(url, id, model, agent, !no_apply).await,
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
                RemoteMemoryCommand::Import { path, user, agent } => {
                    headless::remote_memory_import(url, path, user, agent).await
                }
            },
            RemoteCommand::Compact { command } => match command {
                RemoteCompactCommand::KeepRun {
                    run_id,
                    conversation,
                    guidance,
                } => headless::remote_compact_keep_run(url, run_id, conversation, guidance).await,
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
                RemoteCapabilityCommand::Export { id, path } => {
                    headless::remote_capability_export(url, id, path).await
                }
                RemoteCapabilityCommand::Import { path } => {
                    headless::remote_capability_import(url, path).await
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
                    stop_retention_mode,
                    allowed_tools,
                    allowed_tool_categories,
                    approval_controller_agent,
                    approval_controller_allowed_tools,
                    approval_controller_allowed_tool_categories,
                    capability_drafts_enabled,
                    capability_draft_guidance,
                    allowed_skill_categories,
                    skill_visibility_overrides,
                    skill_visibility,
                    tool_output_mode,
                    tool_routing_model,
                    tool_output_interpretation_model,
                    tool_output_overrides,
                    tool_interpretation_model_overrides,
                    tool_guidance_overrides,
                    tool_visibility_overrides,
                    tool_visibility,
                    load_memory,
                    memory_backend,
                    memory_model,
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
                        stop_retention_mode.map(StopRetentionMode::from),
                        allowed_tools,
                        allowed_tool_categories,
                        approval_controller_agent,
                        approval_controller_allowed_tools,
                        approval_controller_allowed_tool_categories,
                        capability_drafts_enabled,
                        capability_draft_guidance,
                        allowed_skill_categories,
                        skill_visibility_overrides,
                        skill_visibility.map(VisibilityLevel::from),
                        tool_output_mode.map(ToolOutputMode::from),
                        tool_routing_model,
                        tool_output_interpretation_model,
                        tool_output_overrides,
                        tool_interpretation_model_overrides,
                        tool_guidance_overrides,
                        tool_visibility_overrides,
                        tool_visibility.map(VisibilityLevel::from),
                        load_memory,
                        memory_backend,
                        memory_model,
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
                RemoteModelCommand::Doctor => headless::remote_model_doctor(url).await,
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
                RemoteModelCommand::ProviderCatalog { command } => match command {
                    RemoteModelProviderCatalogCommand::Show => {
                        headless::remote_model_provider_catalog_show(url).await
                    }
                    RemoteModelProviderCatalogCommand::Export { path } => {
                        headless::remote_model_provider_catalog_export(url, path).await
                    }
                    RemoteModelProviderCatalogCommand::Import { path } => {
                        headless::remote_model_provider_catalog_import(url, path).await
                    }
                },
                RemoteModelCommand::MetadataCatalog { command } => match command {
                    RemoteModelMetadataCatalogCommand::Show => {
                        headless::remote_model_metadata_catalog_show(url).await
                    }
                    RemoteModelMetadataCatalogCommand::Export { path } => {
                        headless::remote_model_metadata_catalog_export(url, path).await
                    }
                    RemoteModelMetadataCatalogCommand::Import { path } => {
                        headless::remote_model_metadata_catalog_import(url, path).await
                    }
                },
            },
            RemoteCommand::Ingest { command } => match command {
                RemoteIngestCommand::Backends => headless::remote_ingest_backends(url).await,
                RemoteIngestCommand::List => headless::remote_ingest_list(url).await,
                RemoteIngestCommand::ProbeVision { path, model } => {
                    headless::remote_ingest_probe_vision(url, path, model).await
                }
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
                RemoteArtifactCommand::Delete { id } => {
                    headless::remote_artifact_delete(url, id).await
                }
            },
            RemoteCommand::Adapter { command } => match command {
                RemoteAdapterCommand::List => headless::remote_adapter_list(url).await,
                RemoteAdapterCommand::Doctor => headless::remote_adapter_doctor(url).await,
                RemoteAdapterCommand::InstallSkill { id } => {
                    headless::remote_adapter_install_skill(url, id).await
                }
                RemoteAdapterCommand::Import { path } => {
                    headless::remote_adapter_import(url, path).await
                }
                RemoteAdapterCommand::ImportManifest { path } => {
                    headless::remote_adapter_import_manifest(url, path).await
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
                RemoteAdapterCommand::Export { id, path } => {
                    headless::remote_adapter_export(url, id, path).await
                }
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
