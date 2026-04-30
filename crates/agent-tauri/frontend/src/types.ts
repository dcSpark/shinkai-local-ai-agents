// TypeScript mirrors of the Rust types in `agent-tracing` and `agent-tauri`.
// See `specs/architecture.md` §4.7 (RunEvent).
//
// Kept in lockstep with Rust manually for v0; auto-generation (ts-rs / specta)
// can land in a later slice.

export type Uuid = string;
export type EventId = number;

export type RunEvent = {
  id: EventId;
  run_id: Uuid;
  parent_event: EventId | null;
  schema_version: number;
  at: string; // ISO-8601
  kind: RunEventKind;
};

export type RunEventKind =
  | { type: "RunStarted"; agent_id: string; input: string }
  | { type: "ContextBuilt"; snapshot: ContextSnapshot }
  | { type: "LlmRequestStarted"; model: string }
  | {
      type: "LlmRequestCompleted";
      tokens_in: number;
      tokens_out: number;
      cost_usd: number | null;
      duration_ms: number;
    }
  | {
      type: "PromptRefinementStarted";
      model: string;
      original_input: string;
      instructions: string;
    }
  | {
      type: "PromptRefinementCompleted";
      refined_input: string;
      tokens_in: number;
      tokens_out: number;
      cost_usd: number | null;
      duration_ms: number;
    }
  | {
      type: "ToolCallProposed";
      call_id: string;
      tool_id: string;
      input: unknown;
    }
  | { type: "ToolCallStarted"; call_id: string }
  | {
      type: "ToolCallCompleted";
      call_id: string;
      output: unknown;
      cost_usd: number | null;
      duration_ms: number;
    }
  | { type: "ToolCallFailed"; call_id: string; error: string }
  | {
      type: "ApprovalRequested";
      approval_id: string;
      action: string;
      reason: string;
    }
  | { type: "ApprovalResolved"; approval_id: string; approved: boolean }
  | { type: "GuidanceInjected"; content: string }
  | { type: "QualityScored"; target: string; score: number }
  | { type: "MemoryLoaded"; ids: string[] }
  | { type: "MemoryWritten"; id: string; operation: string }
  | { type: "IngestionReferenced"; artifact_id: string; source: string }
  | { type: "IngestionStarted"; source: string; backend: string }
  | {
      type: "IngestionCompleted";
      artifact_id: string;
      content_hash: string;
      sections: number;
    }
  | { type: "PolicyDenied"; reason: string }
  | { type: "ChildRunStarted"; child_run_id: Uuid; agent_id: string }
  | { type: "ChildRunCompleted"; child_run_id: Uuid; status: string }
  | { type: "BatchRunStarted"; batch_id: string; items: number }
  | {
      type: "BatchItemStatus";
      batch_id: string;
      item_key: string;
      status: string;
    }
  | {
      type: "BatchRunCompleted";
      batch_id: string;
      succeeded: number;
      failed: number;
    }
  | { type: "RunPaused"; reason: string }
  | { type: "RunCancelled"; reason: string }
  | {
      type: "RunCompleted";
      final_output: string;
      total_cost_usd: number | null;
      total_duration_ms: number;
    }
  | { type: "RunFailed"; reason: string };

export type RunSummary = {
  run_id: Uuid;
  final_output: string;
};

export type Demo = "echo" | "tool";
export type Provider = "fake" | "rig";
export type ToolVisibility = "full_schema" | "name_and_description" | "name_only";

export type RunOptions = {
  provider: Provider;
  model: string | null;
  api_base_url: string | null;
  api_key_env: string;
  api_key: string | null;
  max_output_tokens: number | null;
  temperature: number | null;
  input_cost_per_million: number | null;
  output_cost_per_million: number | null;
  max_tool_calls: number | null;
  tool_visibility: ToolVisibility | null;
  enable_shell: boolean;
  enable_subagent: boolean;
  load_memory: boolean;
  load_skills: boolean;
  include_ingest: string[];
  allow_unsafe_ingest: boolean;
  enable_prompt_refinement: boolean;
  prompt_refinement_instructions: string | null;
  prompt_refinement_model: string | null;
  require_approval: boolean;
  raw_tool_output: boolean;
};

export type ContextSnapshot = {
  system_prompt: string;
  conversation: unknown[];
  compacted: string | null;
  loaded_memory: unknown[];
  loaded_artifacts: unknown[];
  visible_tools: ToolView[];
  visible_skills: unknown[];
  limits: {
    max_tool_calls: number;
    remaining_tool_calls: number;
  };
  provenance: unknown[];
};

export type ToolView = {
  id: string;
  name: string;
  description: string | null;
  input_schema: unknown | null;
  visibility: string;
};
