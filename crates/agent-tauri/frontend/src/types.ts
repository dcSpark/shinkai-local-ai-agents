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
  | { type: "LlmStreamToken"; delta: string }
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
      model?: string | null;
      permissions?: unknown | null;
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
      findings?: string[];
      high_risk_findings?: number;
      finding_snippets?: string[];
    }
  | {
      type: "HookFired";
      hook_id: string;
      trigger: string;
      payload_digest: string;
    }
  | {
      type: "HookFailed";
      hook_id: string;
      trigger: string;
      error: string;
      attempt: number;
      will_retry: boolean;
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

export type TraceTreeNode = {
  run_id: Uuid;
  agent_id?: string | null;
  status: string;
  event_count: number;
  trace_available: boolean;
  link_event_id?: number | null;
  completion_event_id?: number | null;
  link_status?: string | null;
  children?: TraceTreeNode[];
};

export type Demo = "echo" | "tool";
export type Provider =
  | "fake"
  | "rig"
  | "ollama"
  | "llama_cpp"
  | "anthropic"
  | "gemini";

export type ModelProviderOptionTarget = "runtime" | "provider_options";
export type ModelProviderOptionKind = "number" | "integer" | "string" | "boolean";

export type ModelProviderOptionDescriptor = {
  key: string;
  target: ModelProviderOptionTarget;
  label: string;
  kind: ModelProviderOptionKind;
  min?: number | null;
  max?: number | null;
  allowed_values?: string[];
  notes?: string | null;
};

export type ModelProviderDescriptor = {
  id: Provider | string;
  name: string;
  default_model: string;
  api_key_env?: string | null;
  api_base_url?: string | null;
  supports_api_base_url: boolean;
  local: boolean;
  native: boolean;
  available_modalities: string[];
  tool_support?: boolean | null;
  reasoning_modes: string[];
  settings: string[];
  option_schema: ModelProviderOptionDescriptor[];
  notes?: string | null;
};

export type ModelProviderCatalog = {
  schema_version: number;
  source?: string | null;
  providers: ModelProviderDescriptor[];
};

export type ModelDoctorStatus = "ok" | "warning" | "error";

export type ModelDoctorModelReport = {
  id: string;
  provider: string;
  provider_known: boolean;
  validation_status: ModelDoctorStatus;
  validation_error?: string | null;
  declared_modalities: string[];
  metadata_present: boolean;
  metadata_source?: string | null;
  metadata_modalities: string[];
};

export type ModelDoctorReport = {
  active_profile: string;
  status: ModelDoctorStatus;
  provider_count: number;
  provider_catalog_configured: boolean;
  metadata_catalog_source?: string | null;
  metadata_catalog_models: number;
  bundled_metadata_models: number;
  saved_model_count: number;
  saved_models: ModelDoctorModelReport[];
  warnings: string[];
  errors: string[];
};

export type ModelMetadataCatalogEntry = {
  provider: string;
  model_id: string;
  modalities?: string[];
  capabilities?: string[];
  tool_support?: boolean | null;
  limits?: Record<string, number>;
  pricing?: Record<string, string>;
  source?: string | null;
};

export type ModelMetadataCatalog = {
  schema_version: number;
  source?: string | null;
  updated_at?: string | null;
  models: ModelMetadataCatalogEntry[];
};

export type ToolVisibility = "full_schema" | "name_and_description" | "name_only";
export type ToolOutputMode = "interpreted" | "raw";

export type AgentToolOverrideConfig = {
  id: string;
  output_mode?: ToolOutputMode | null;
  output_interpretation_model?: string | null;
  output_interpretation_guidance?: string | null;
  visibility?: ToolVisibility | null;
};

export type AgentSkillOverrideConfig = {
  id: string;
  visibility: ToolVisibility;
};

export type AgentConfigFile = {
  id: string;
  name: string;
  system_prompt: string;
  model?: string | null;
  max_tool_calls?: number | null;
  prompt_refinement?: unknown | null;
  prompt_refinements?: unknown[];
  tool_overrides?: AgentToolOverrideConfig[];
  skill_overrides?: AgentSkillOverrideConfig[];
  tool_visibility?: ToolVisibility | null;
  skill_visibility?: ToolVisibility | null;
  load_memory?: boolean | null;
  memory_backend?: string | null;
  memory_model?: string | null;
  load_skills?: boolean | null;
  allowed_tools?: string[] | null;
  allowed_tool_categories?: string[] | null;
  allowed_skill_categories?: string[] | null;
};

export type AgentSummary = {
  id: string;
  name: string;
  path: string;
};

export type PromptDoc = {
  name: string;
  body: string;
  agent_id?: string | null;
};

export type ConversationRole = "system" | "user" | "assistant" | "tool";

export type BranchRef = {
  conversation_id: string;
  parent_message_count: number;
};

export type ConversationMessage = {
  role: ConversationRole;
  content: string;
  created_at: string;
};

export type ConversationPolicy = {
  load_memory?: boolean | null;
  generate_memory?: boolean | null;
  allowed_tool_categories?: string[] | null;
  allowed_skill_categories?: string[] | null;
  max_tokens_before_compaction?: number | null;
  max_compaction_output_tokens?: number | null;
  compaction_guidance?: string | null;
};

export type ConversationDoc = {
  id: string;
  title: string;
  agent_id: string;
  parent?: BranchRef | null;
  branch_reason?: string | null;
  policy?: ConversationPolicy | null;
  messages: ConversationMessage[];
  created_at: string;
  updated_at: string;
};

export type ExpandedConversation = {
  conversation: ConversationDoc;
  messages: ConversationMessage[];
};

export type ConversationTreeNode = {
  id: string;
  title: string;
  agent_id: string;
  parent_id?: string | null;
  branch_reason?: string | null;
  own_message_count: number;
  expanded_message_count: number;
  children: ConversationTreeNode[];
};

export type ConversationDeleteResult = {
  requested: string;
  recursive: boolean;
  planned: string[];
  deleted: string[];
  deleted_compactions?: string[];
  deleted_memories?: string[];
};

export type ConversationDeleteRangeResult = {
  id: string;
  from: number;
  to: number;
  deleted_messages: number;
  expanded_message_count: number;
  conversation: ConversationDoc;
};

export type ConversationRecoveryCompaction = {
  id: string;
  source: string;
  guidance?: string | null;
  max_output_tokens: number;
  original_input_excerpt: string;
  content_preview: string;
  created_at: string;
};

export type ConversationRecoveryMemory = {
  id: string;
  target: MemoryTarget;
  author: MemoryAuthor;
  source_range?: string | null;
  generating_model?: string | null;
  content_preview: string;
  updated_at: string;
};

export type ConversationRecoveryPlan = {
  conversation_id: string;
  title: string;
  agent_id: string;
  own_message_count: number;
  expanded_message_count: number;
  linked_compactions: ConversationRecoveryCompaction[];
  linked_memories: ConversationRecoveryMemory[];
  suggested_run: {
    conversation_id: string;
    include_compact?: string | null;
    load_memory: boolean;
    compacted_context?: string | null;
  };
};

export type CompactionRecord = {
  id: string;
  content: string;
  guidance?: string | null;
  conversation_id?: string | null;
  source: string;
  max_output_tokens: number;
  original_input_hash: string;
  original_input_excerpt: string;
  created_at: string;
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
  agent_id: string | null;
  model: string | null;
  api_base_url: string | null;
  api_key_env: string;
  api_key: string | null;
  max_output_tokens: number | null;
  temperature: number | null;
  input_cost_per_million: number | null;
  output_cost_per_million: number | null;
  max_tool_calls: number | null;
  max_tokens_before_compaction: number | null;
  max_compaction_output_tokens: number | null;
  compaction_guidance: string | null;
  allowed_tool_categories: string[];
  allowed_skill_categories: string[];
  tool_visibility: ToolVisibility | null;
  skill_visibility: ToolVisibility | null;
  enable_shell: boolean;
  enable_subagent: boolean;
  enable_capability_drafts: boolean;
  load_memory: boolean;
  memory_topics: string[];
  load_skills: boolean;
  include_ingest: string[];
  allow_unsafe_ingest: boolean;
  enable_prompt_refinement: boolean;
  prompt_refinement_instructions: string | null;
  prompt_refinement_model: string | null;
  require_approval: boolean;
  auto_approve: boolean;
  raw_tool_output: boolean;
  disable_lifecycle_hooks: boolean;
  compacted_context: string | null;
  conversation_id: string | null;
};

export type ContextSnapshot = {
  system_prompt: string;
  conversation: Message[];
  compacted: string | null;
  compaction_review?: CompactionReview | null;
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

export type CompactionReview = {
  mode: "auto" | "manual";
  before_messages: string[];
  compacted_context: string;
  visible_messages: string[];
  before_tokens: number;
  after_tokens: number;
  withheld_before_messages: number;
};

export type MemoryFragment = {
  id: string;
  content: string;
  provenance: string;
};

export type MemoryTarget = "agent" | "user";
export type MemoryAuthor = "human" | "model";

export type MemoryClassification = {
  topics?: string[];
  tasks?: string[];
  source?: string | null;
};

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
  source_conversation_id?: string | null;
  generating_model?: string | null;
  topics?: string[];
  classification?: MemoryClassification;
};

export type MemoryAccessGrant = {
  id: string;
  resource: string;
  from_profile: string;
  to_profile: string;
  matched_records?: number;
};

export type MemoryAccessEntry = {
  access: "local" | "profile_grant";
  source_profile: string;
  source_backend: string;
  grant: MemoryAccessGrant | null;
  record: MemoryRecord;
};

export type MemoryAccessReport = {
  active_profile: string;
  topics: string[];
  local_records: number;
  granted_records: number;
  grants: MemoryAccessGrant[];
  records: MemoryAccessEntry[];
};

export type MemoryClassifyResult = {
  id: string;
  model: string;
  classification: MemoryClassification;
  record?: MemoryRecord | null;
  applied: boolean;
};

export type MemoryBackendDescriptor = {
  id: string;
  name: string;
  description: string;
  storage: string;
  supports_generation: boolean;
  supports_rollback: boolean;
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

export type IngestionBackendDescriptor = {
  id: string;
  name: string;
  description: string;
  modalities: string[];
};

export type IngestionFinding = {
  severity: IngestionFindingSeverity;
  message: string;
};

export type IngestionFindingReviewDecision =
  | "acknowledge"
  | "approve"
  | "reject";

export type IngestionFindingReview = {
  finding_index: number;
  decision: IngestionFindingReviewDecision;
  note?: string | null;
  reviewed_at: string;
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
  finding_reviews: IngestionFindingReview[];
  created_at: string;
};

export type IngestionResult = {
  trace_run_id: string;
  artifact: IngestionArtifact;
};

export type GeneratedArtifact = {
  id: string;
  format: string;
  path: string;
  bytes: number;
  modified_ms?: number | null;
};

export type GeneratedArtifactDataUrl = {
  artifact: GeneratedArtifact;
  media_type: string;
  data_url: string;
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
  wallet: boolean;
  payment: boolean;
  browser_profile: boolean;
};

export type AdapterCapability = {
  id: string;
  kind: string;
  name: string;
  description: string;
  quarantined: boolean;
  runtime?: AdapterCapabilityRuntime | null;
};

export type AdapterCapabilityRuntime = {
  transport: string;
  endpoint?: string | null;
  command?: string | null;
  args?: string[];
  env_keys?: string[];
  input_modes?: string[];
  output_modes?: string[];
  auth_schemes?: string[];
};

export type AdapterSecretRequirement = {
  name: string;
  source: string;
  description?: string | null;
  required?: boolean | null;
};

export type AdapterPackage = {
  id: string;
  source: string;
  adapter: string;
  digest: string;
  quarantined: boolean;
  capabilities: AdapterCapability[];
  permissions: AdapterPermissions;
  secret_requirements?: AdapterSecretRequirement[];
  findings: AdapterFinding[];
  provenance?: string | null;
};

export type CapabilityKind = "tool" | "skill" | "agent";
export type CapabilityDraftStatus = "quarantined" | "allowed" | "rejected";

export type CapabilityDraft = {
  id: string;
  kind: CapabilityKind;
  name: string;
  body: string;
  guidance?: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
  status: CapabilityDraftStatus;
  provenance: string;
};

export type CapabilityReviewResult =
  | CapabilityDraft
  | {
      draft: CapabilityDraft;
      promoted_skill?: SkillDoc;
      quarantined_skill?: SkillDoc;
      promoted_agent?: unknown;
      promoted_tool?: AdapterPackage;
      quarantined_tool?: AdapterPackage;
    };

export type SkillDoc = {
  id: string;
  name: string;
  description: string;
  categories: string[];
  body: string;
  source_path: string | null;
  provenance?: string | null;
  digest: string;
  estimated_tokens: number;
  quarantined: boolean;
};

export type ToolView = {
  id: string;
  name: string;
  description: string | null;
  categories: string[];
  input_schema: unknown | null;
  output_mode: ToolOutputMode;
  output_interpretation_guidance?: string | null;
  visibility: string;
  provenance?: string | null;
};

export type SkillView = {
  id: string;
  name: string;
  description: string | null;
  categories: string[];
  body?: string | null;
  estimated_tokens: number;
  visibility: string;
  provenance?: string | null;
};

export type ProvenanceRecord = {
  fragment: string;
  source: string;
};
