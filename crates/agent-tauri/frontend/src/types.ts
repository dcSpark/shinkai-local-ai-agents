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
  | {
      type: "LlmRequestStarted";
      model: string;
      request_digest?: string | null;
    }
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
  | {
      type: "ToolOutputInterpreted";
      call_id: string;
      model: string;
      summary: string;
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
  | { type: "MemoryRead"; backend: string; fragment_ids: string[] }
  | {
      type: "MemoryWritten";
      id: string;
      operation: string;
      source_range?: string | null;
      generating_model?: string | null;
    }
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

export type PromptDoc = {
  name: string;
  body: string;
};

export type BundleManifest = {
  schema_version: number;
  exported_at: string;
  profile: string;
};

export type Message =
  | { role: "system"; content: string }
  | { role: "user"; content: string }
  | { role: "assistant"; content: string | null; tool_calls: unknown[] }
  | { role: "tool_result"; tool_call_id: string; content: string };

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
  conversation: Message[];
  compacted: string | null;
  loaded_memory: MemoryFragment[];
  loaded_artifacts: IngestedArtifactView[];
  visible_tools: ToolView[];
  visible_skills: SkillView[];
  limits: {
    max_tool_calls: number;
    remaining_tool_calls: number;
  };
  estimated_input_tokens: number;
  provenance: ProvenanceRecord[];
};

export type MemoryFragment = {
  id: string;
  content: string;
  provenance: string;
};

export type MemoryTarget = "agent" | "user";
export type MemoryAuthor = "human" | "model";

export type MemoryRecord = {
  id: string;
  content: string;
  target: MemoryTarget;
  owning_profile: string;
  owning_agent: string | null;
  created_at: string;
  updated_at: string;
  author: MemoryAuthor;
  source_range: string | null;
  generating_model?: string | null;
};

export type IngestedArtifactView = {
  id: string;
  source: string;
  sections: number;
  content: string;
  findings: string[];
  provenance: string;
};

export type IngestionFindingSeverity = "info" | "warning" | "high";

export type IngestionFinding = {
  severity: IngestionFindingSeverity;
  message: string;
};

export type IngestSection = {
  index: number;
  title: string | null;
  text: string;
};

export type IngestionArtifact = {
  id: string;
  source: string;
  backend: string;
  content_hash: string;
  sections: IngestSection[];
  extracted_text: string | null;
  findings: IngestionFinding[];
  created_at: string;
};

export type IngestionResult = {
  trace_run_id: string;
  artifact: IngestionArtifact;
};

export type AdapterFindingSeverity = "info" | "warning" | "high";

export type AdapterFinding = {
  severity: AdapterFindingSeverity;
  message: string;
};

export type AdapterPermissions = {
  shell: boolean;
  file_read: boolean;
  file_write: boolean;
  network: boolean;
  secrets: boolean;
};

export type AdapterCapability = {
  id: string;
  kind: string;
  name: string;
  description: string;
  quarantined: boolean;
};

export type AdapterPackage = {
  id: string;
  source: string;
  adapter: string;
  digest: string;
  quarantined: boolean;
  capabilities: AdapterCapability[];
  permissions: AdapterPermissions;
  findings: AdapterFinding[];
};

export type SkillDoc = {
  id: string;
  name: string;
  description: string;
  body: string;
  source_path: string | null;
  digest: string;
  estimated_tokens: number;
  quarantined: boolean;
};

export type ToolView = {
  id: string;
  name: string;
  description: string | null;
  input_schema: unknown | null;
  output_interpretation_guidance?: string | null;
  visibility: string;
};

export type SkillView = {
  id: string;
  name: string;
  description: string | null;
  estimated_tokens: number;
  visibility: string;
};

export type ProvenanceRecord = {
  fragment: string;
  source: string;
};
