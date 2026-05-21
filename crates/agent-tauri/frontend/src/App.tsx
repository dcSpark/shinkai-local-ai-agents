import { useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AdapterCapabilityRuntime,
  AdapterDoctorReport,
  AdapterPackage,
  AgentConfigFile,
  AgentSummary,
  BundleManifest,
  CapabilityDraft,
  CapabilityKind,
  CapabilityReviewResult,
  CompactionRecord,
  ContextSnapshot,
  ConversationDeleteResult,
  ConversationDeleteRangeResult,
  ConversationDoc,
  ConversationMessage,
  ConversationPolicy,
  ConversationRecoveryPlan,
  ConversationTreeNode,
  Demo,
  ExpandedConversation,
  GeneratedArtifact,
  GeneratedArtifactDataUrl,
  IngestionArtifact,
  IngestionBackendDescriptor,
  IngestionFindingReviewDecision,
  IngestionResult,
  MemoryAccessReport,
  MemoryBackendDescriptor,
  MemoryClassifyResult,
  MemoryRecord,
  ModelDoctorReport,
  ModelMetadataCatalog,
  ModelVisionProbe,
  ModelProviderCatalog,
  ModelProviderDescriptor,
  ModelProviderOptionDescriptor,
  PromptDoc,
  ProfileGrant,
  ProfileGrantKind,
  ProfileSummary,
  Provider,
  RunEvent,
  RunOptions,
  RunSummary,
  SecretBackendDescriptor,
  SecretRecord,
  SecretWriteResult,
  SkillDoc,
  StopRetentionMode,
  TraceTreeNode,
  ToolVisibility,
} from "./types";

type LineKind = "user" | "assistant" | "event" | "error";
type ArtifactPreview = GeneratedArtifactDataUrl;
type Transport = "in-process" | "daemon";
type ActiveSection =
  | "chat"
  | "trace"
  | "conversations"
  | "profiles"
  | "memory"
  | "skills"
  | "prompts"
  | "ingest"
  | "artifacts"
  | "adapters"
  | "approvals";
type AgentMode = "answer" | "action" | "workflow" | "custom";
type AgentConfigEntry = AgentConfigFile | AgentSummary;
type AgentDeleteResult = { id?: string; deleted?: boolean };
type IngestModelOptions = {
  backend?: string;
  visionModel?: string | null;
  guardrailModel?: string | null;
};

function agentSharedProfile(doc: AgentConfigEntry | null | undefined) {
  return doc?.shared_from_profile?.trim() || null;
}

interface TranscriptLine {
  kind: LineKind;
  text: string;
}

interface SlashCommandSuggestion {
  command: string;
  label: string;
}

interface ToolParameterView {
  name: string;
  type: string;
  required: boolean;
  description: string | null;
}

interface ContextReviewCard {
  title: string;
  value: string;
  detail: string;
  tone: "neutral" | "ok" | "warning" | "danger";
}

interface ConversationTreeRow {
  node: ConversationTreeNode;
  depth: number;
}

interface ConversationTreeStats {
  total: number;
  roots: number;
  branchPoints: number;
  leaves: number;
  maxDepth: number;
}

interface ConversationRange {
  from: number;
  to: number;
}

type JsonValue =
  | null
  | string
  | number
  | boolean
  | JsonValue[]
  | { [key: string]: JsonValue };

interface RemoteRunStart {
  run_id: string;
  status: string;
  active: boolean;
}

interface RemoteResumeStart extends RemoteRunStart {
  source_run_id: string;
  resumed_run_id?: string | null;
  from_event: number;
  retained_compaction?: string | null;
}

interface RemoteRunStatus {
  run_id: string;
  status: "running" | "completed" | "failed" | "cancelled" | "paused" | "unknown";
  active: boolean;
  event_count: number;
  final_output: string | null;
  reason: string | null;
  total_cost_usd: number | null;
  total_duration_ms: number | null;
}

interface TraceSummary {
  run_id: string;
  events: number;
  context_snapshots: number;
  llm_calls: number;
  tool_calls: number;
  approvals: number;
  guidance_injections: number;
  quality_scores: number;
  quality_score_average?: number | null;
  quality_score_min?: number | null;
  quality_score_max?: number | null;
  memory_fragments: number;
  artifact_refs: number;
  hooks: number;
  hook_failures: number;
  tokens_in: number;
  tokens_out: number;
  cost_usd: number | null;
  duration_ms: number | null;
}

interface QualityScoreRecord {
  event_id: number;
  run_id: string;
  at: string;
  target: string;
  score: number;
}

interface TraceTimelineItem {
  id: number;
  title: string;
  meta: string;
  detail: string;
  tone: "neutral" | "ok" | "warning" | "danger";
}

interface TraceComparisonRow {
  label: string;
  primary: string;
  compare: string;
  delta: string;
}

interface HookRemediationRecord {
  event_id: number;
  hook_id: string;
  trigger: string;
  error: string;
  attempt: number;
  will_retry: boolean;
  final_failure: boolean;
  policy_denials: string[];
  suggested_actions: string[];
}

interface HookPolicyRecord {
  agent_id?: string | null;
  profile: string;
  effective_source?: string;
  disabled_lifecycle_hooks: string[];
  effective_disabled_lifecycle_hooks?: string[];
  global_disabled_lifecycle_hooks?: string[];
  profile_disabled_lifecycle_hooks?: string[];
  agent_disabled_lifecycle_hooks?: string[];
}

interface HookCatalogRecord {
  id: string;
  triggers: string[];
  provenance: string;
  disabled: boolean;
  disabled_source?: string | null;
}

interface HookCatalogResponse {
  agent_id: string;
  effective_source: string;
  hooks: HookCatalogRecord[];
}

interface ResumeResult {
  source_run_id: string;
  resumed_run_id: string;
  from_event: number;
  retained_compaction?: string | null;
  final_output: string;
}

interface CancelResult {
  run_id: string;
  recorded: string;
  aborted: boolean;
  compaction?: CompactionRecord | null;
}

interface ApprovalRecord {
  approval_id: string;
  action: string | null;
  reason: string | null;
  controller_agent?: string | null;
  controller_scope?: string[];
  status: string;
  approved?: boolean | null;
  delegated_controller?: string | null;
  assessment?: ApprovalAssessment | null;
}

interface ApprovalAssessment {
  approval_id: string;
  controller_agent: string;
  status: string;
  scope: string[];
  reason: string;
  model?: string | null;
  recommendation?: string | null;
  model_output?: string | null;
  tokens_in: number;
  tokens_out: number;
  duration_ms: number;
}

interface ApprovalAssessResult {
  run_id: string;
  event_id: number;
  assessment: ApprovalAssessment;
}

interface StorageBucket {
  name: string;
  path: string;
  bytes: number;
  files: number;
  directories: number;
  largest_file?: string | null;
  largest_file_bytes: number;
  exists: boolean;
}

interface StorageReport {
  root: string;
  total_bytes: number;
  total_files: number;
  total_directories: number;
  largest_file?: string | null;
  largest_file_bytes: number;
  quota_bytes?: number | null;
  quota_remaining_bytes?: number | null;
  quota_exceeded: boolean;
  buckets: StorageBucket[];
}

interface StorageRetentionCandidate {
  bucket: string;
  path: string;
  bytes: number;
  modified_unix_seconds?: number | null;
}

interface StorageRetentionPlan {
  root: string;
  retention_days: number;
  cutoff_unix_seconds: number;
  total_bytes: number;
  total_files: number;
  candidates: StorageRetentionCandidate[];
}

interface StorageRetentionResult {
  dry_run: boolean;
  plan: StorageRetentionPlan;
  deleted_files: number;
  deleted_bytes: number;
  errors?: string[];
}

interface BridgeDeliveryRecord {
  id: string;
  target: string;
  url: string;
  payload: JsonValue;
  last_delivery: JsonValue;
  created_ms: number;
  updated_ms: number;
}

interface BridgeDeliveryListResponse {
  deliveries: BridgeDeliveryRecord[];
}

interface BundleStatus {
  operation: "exported" | "imported";
  path: string;
  manifest: BundleManifest;
}

interface CompactionExportResult {
  path: string;
  record: CompactionRecord;
}

interface CompactionTransferStatus {
  operation: "exported" | "imported";
  path: string;
  record: CompactionRecord;
}

interface PostRunCompactionPrompt {
  runId: string;
  snapshot: ContextSnapshot;
}

interface CompactionMetrics {
  mode: "auto" | "manual";
  originalTokens: number | null;
  threshold: number | null;
  maxOutputTokens: number | null;
  compactedTokens: number;
  visibleConversationTokens: number;
  currentTokens: number;
  savedTokens: number | null;
}

interface VoiceCaptureResponse {
  audio_path: string;
  artifact: GeneratedArtifact;
}

const CALLS_MAX = 5;

function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export default function App() {
  const [transcript, setTranscript] = useState<TranscriptLine[]>([
    {
      kind: "event",
      text: "Welcome. Ask the agent to handle one focused task.",
    },
  ]);
  const [input, setInput] = useState("");
  const [slashCommandIndex, setSlashCommandIndex] = useState(0);
  const [slashCommandDismissed, setSlashCommandDismissed] = useState(false);
  const [running, setRunning] = useState(false);
  const [tokensIn, setTokensIn] = useState(0);
  const [tokensOut, setTokensOut] = useState(0);
  const [costUsd, setCostUsd] = useState(0);
  const [calls, setCalls] = useState(0);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [demo, setDemo] = useState<Demo>("tool");
  const [provider, setProvider] = useState<Provider>("fake");
  const tauriRuntime = hasTauriRuntime();
  const [transport, setTransport] = useState<Transport>(() =>
    tauriRuntime ? "in-process" : "daemon",
  );
  const [daemonUrl, setDaemonUrl] = useState("http://127.0.0.1:7878");
  const [agentId, setAgentId] = useState("");
  const [conversationId, setConversationId] = useState("");
  const [opsValue, setOpsValue] = useState("");
  const [opsId, setOpsId] = useState("");
  const [capabilityKind, setCapabilityKind] = useState<CapabilityKind>("skill");
  const [memorySourceRange, setMemorySourceRange] = useState("");
  const [memoryTopics, setMemoryTopics] = useState("");
  const [memoryClassificationModel, setMemoryClassificationModel] = useState("");
  const [opsUserMemory, setOpsUserMemory] = useState(false);
  const [ingestBackend, setIngestBackend] = useState("local-v0");
  const [ingestVisionModel, setIngestVisionModel] = useState("");
  const [ingestGuardrailModel, setIngestGuardrailModel] = useState("");
  const [ingestFindingIndex, setIngestFindingIndex] = useState("0");
  const [ingestReviewDecision, setIngestReviewDecision] =
    useState<IngestionFindingReviewDecision>("approve");
  const [ingestReviewNote, setIngestReviewNote] = useState("");
  const [model, setModel] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [apiKeyEnv, setApiKeyEnv] = useState("OPENAI_API_KEY");
  const [apiKey, setApiKey] = useState("");
  const [modelProviderDescriptors, setModelProviderDescriptors] = useState<
    ModelProviderDescriptor[]
  >([]);
  const [providerTopP, setProviderTopP] = useState("");
  const [providerTopK, setProviderTopK] = useState("");
  const [providerReasoningEffort, setProviderReasoningEffort] = useState("");
  const [providerFrequencyPenalty, setProviderFrequencyPenalty] = useState("");
  const [providerPresencePenalty, setProviderPresencePenalty] = useState("");
  const [modelSupportsImage, setModelSupportsImage] = useState(false);
  const [inputCostPerMillion, setInputCostPerMillion] = useState("");
  const [outputCostPerMillion, setOutputCostPerMillion] = useState("");
  const [maxToolCalls, setMaxToolCalls] = useState("");
  const [maxTokensBeforeCompaction, setMaxTokensBeforeCompaction] =
    useState("");
  const [maxCompactionOutputTokens, setMaxCompactionOutputTokens] =
    useState("");
  const [compactionGuidance, setCompactionGuidance] = useState("");
  const [allowedToolCategories, setAllowedToolCategories] = useState("");
  const [allowedSkillCategories, setAllowedSkillCategories] = useState("");
  const [toolVisibility, setToolVisibility] = useState<ToolVisibility | "">("");
  const [enableShell, setEnableShell] = useState(false);
  const [enableSubagent, setEnableSubagent] = useState(false);
  const [enableCapabilityDrafts, setEnableCapabilityDrafts] = useState(false);
  const [capabilityDraftGuidance, setCapabilityDraftGuidance] = useState("");
  const [loadMemory, setLoadMemory] = useState(false);
  const [generateMemoryPolicy, setGenerateMemoryPolicy] = useState<
    "" | "on" | "off"
  >("");
  const [loadSkills, setLoadSkills] = useState(false);
  const [includeIngestIds, setIncludeIngestIds] = useState<string[]>([]);
  const [allowUnsafeIngest, setAllowUnsafeIngest] = useState(false);
  const [enablePromptRefinement, setEnablePromptRefinement] = useState(false);
  const [promptRefinementInstructions, setPromptRefinementInstructions] =
    useState("");
  const [promptRefinementModel, setPromptRefinementModel] = useState("");
  const [requireApproval, setRequireApproval] = useState(true);
  const [rawToolOutput, setRawToolOutput] = useState(false);
  const [toolRoutingModel, setToolRoutingModel] = useState("");
  const [toolOutputInterpretationModel, setToolOutputInterpretationModel] =
    useState("");
  const [stopRetentionMode, setStopRetentionMode] =
    useState<StopRetentionMode | null>(null);
  const [manualCompactedContext, setManualCompactedContext] = useState("");
  const [lastRunId, setLastRunId] = useState<string | null>(null);
  const [contextPreview, setContextPreview] = useState<ContextSnapshot | null>(
    null,
  );
  const [contextPreviewPrompt, setContextPreviewPrompt] = useState<string | null>(
    null,
  );
  const [contextCopyStatus, setContextCopyStatus] = useState("");
  const [traceEvents, setTraceEvents] = useState<RunEvent[]>([]);
  const [traceSummary, setTraceSummary] = useState<TraceSummary | null>(null);
  const [traceTree, setTraceTree] = useState<TraceTreeNode | null>(null);
  const [collapsedTraceTreeRuns, setCollapsedTraceTreeRuns] = useState<string[]>(
    [],
  );
  const [traceCompareSummary, setTraceCompareSummary] =
    useState<TraceSummary | null>(null);
  const [traceCompareTree, setTraceCompareTree] =
    useState<TraceTreeNode | null>(null);
  const [traceCompareRunId, setTraceCompareRunId] = useState("");
  const [hookPolicy, setHookPolicy] = useState<HookPolicyRecord | null>(null);
  const [hookCatalog, setHookCatalog] = useState<HookCatalogRecord[]>([]);
  const [storageReport, setStorageReport] = useState<StorageReport | null>(null);
  const [storagePruneResult, setStoragePruneResult] =
    useState<StorageRetentionResult | null>(null);
  const [bridgeDeliveries, setBridgeDeliveries] = useState<BridgeDeliveryRecord[]>(
    [],
  );
  const [bridgeDeliveryResult, setBridgeDeliveryResult] =
    useState<JsonValue | null>(null);
  const [bundleStatus, setBundleStatus] = useState<BundleStatus | null>(null);
  const [compactionRecords, setCompactionRecords] = useState<CompactionRecord[]>(
    [],
  );
  const [compactionTransferStatus, setCompactionTransferStatus] =
    useState<CompactionTransferStatus | null>(null);
  const [postRunCompactionPrompt, setPostRunCompactionPrompt] =
    useState<PostRunCompactionPrompt | null>(null);
  const [ingestionBackends, setIngestionBackends] = useState<
    IngestionBackendDescriptor[]
  >([]);
  const [ingestionArtifacts, setIngestionArtifacts] = useState<
    IngestionArtifact[]
  >([]);
  const [generatedArtifacts, setGeneratedArtifacts] = useState<
    GeneratedArtifact[]
  >([]);
  const [artifactPreview, setArtifactPreview] =
    useState<ArtifactPreview | null>(null);
  const [recordingVoice, setRecordingVoice] = useState(false);
  const [voicePreviewUrl, setVoicePreviewUrl] = useState<string | null>(null);
  const [voiceCaptureArtifact, setVoiceCaptureArtifact] =
    useState<GeneratedArtifact | null>(null);
  const [voiceOutputArtifact, setVoiceOutputArtifact] =
    useState<GeneratedArtifact | null>(null);
  const [voiceOutputPreviewUrl, setVoiceOutputPreviewUrl] = useState<string | null>(
    null,
  );
  const [voiceOutputBusy, setVoiceOutputBusy] = useState(false);
  const [memoryRecords, setMemoryRecords] = useState<MemoryRecord[]>([]);
  const [memoryBackends, setMemoryBackends] = useState<MemoryBackendDescriptor[]>(
    [],
  );
  const [promptDocs, setPromptDocs] = useState<PromptDoc[]>([]);
  const [conversationDocs, setConversationDocs] = useState<ConversationDoc[]>([]);
  const [conversationTree, setConversationTree] = useState<ConversationTreeNode[]>(
    [],
  );
  const [expandedConversation, setExpandedConversation] =
    useState<ExpandedConversation | null>(null);
  const [conversationDeletePlan, setConversationDeletePlan] = useState<string[]>(
    [],
  );
  const [skillDocs, setSkillDocs] = useState<SkillDoc[]>([]);
  const [agentConfigs, setAgentConfigs] = useState<AgentConfigEntry[]>([]);
  const [profileSummaries, setProfileSummaries] = useState<ProfileSummary[]>([]);
  const [currentProfile, setCurrentProfile] = useState<ProfileSummary | null>(null);
  const [profileGrants, setProfileGrants] = useState<ProfileGrant[]>([]);
  const [secretBackends, setSecretBackends] = useState<SecretBackendDescriptor[]>(
    [],
  );
  const [secretRecords, setSecretRecords] = useState<SecretRecord[]>([]);
  const [secretLabel, setSecretLabel] = useState("");
  const [secretStatus, setSecretStatus] = useState<JsonValue | null>(null);
  const [capabilityDrafts, setCapabilityDrafts] = useState<CapabilityDraft[]>([]);
  const [adapterPackages, setAdapterPackages] = useState<AdapterPackage[]>([]);
  const [adapterDoctorReport, setAdapterDoctorReport] =
    useState<AdapterDoctorReport | null>(null);
  const [activeSection, setActiveSection] = useState<ActiveSection>("chat");
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);
  const [approvalUnlock, setApprovalUnlock] = useState("");
  const [approvalSignature, setApprovalSignature] = useState("");
  const [approvalControllerAgent, setApprovalControllerAgent] = useState("");

  const transcriptRef = useRef<HTMLElement>(null);
  const terminalEventSeenRef = useRef(false);
  const rootRunIdRef = useRef<string | null>(null);
  const runStartedAtRef = useRef<number | null>(null);
  const latestRunContextRef = useRef<ContextSnapshot | null>(null);
  const remoteSeenEventKeysRef = useRef<Set<string>>(new Set());
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const voiceChunksRef = useRef<Blob[]>([]);
  const runLabel = lastRunId ? lastRunId.slice(0, 8) : "none";
  const effectiveMaxToolCalls =
    parseOptionalNonNegativeInt(maxToolCalls) ?? CALLS_MAX;
  const agentMode = toolBudgetMode();
  const remainingToolCalls = Math.max(0, effectiveMaxToolCalls - calls);
  const budgetPillClass =
    remainingToolCalls === 0
      ? "pill exhausted"
      : remainingToolCalls <= 1
        ? "pill warning"
        : "pill";
  const slashCommandItems = slashCommandDismissed ? [] : slashCommandSuggestions();
  const activeSlashCommand =
    slashCommandItems[
      Math.min(slashCommandIndex, Math.max(0, slashCommandItems.length - 1))
    ];
  const selectedProviderDescriptor = modelProviderDescriptors.find(
    (descriptor) => descriptor.id === provider,
  );
  const supportsApiBaseUrl = providerSupportsRuntimeOption("api_base_url");
  const supportsTopP = providerSupportsProviderOption("top_p");
  const supportsTopK = providerSupportsProviderOption("top_k");
  const supportsReasoningEffort = providerSupportsProviderOption("reasoning_effort");
  const supportsFrequencyPenalty = providerSupportsProviderOption("frequency_penalty");
  const supportsPresencePenalty = providerSupportsProviderOption("presence_penalty");
  const providerOptionKeys = providerOptionSchema()
    .map((option) => option.key)
    .join(", ");
  const activeSlashCommandId =
    activeSlashCommand && slashCommandItems.length
      ? `slash-command-${slashCommandIndex}`
      : undefined;

  // Subscribe to streaming RunEvents from the Rust backend.
  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }
    const promise = listen<RunEvent>("run-event", (msg) => {
      handleRunEvent(msg.payload);
    });
    return () => {
      promise.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    if (transport === "in-process") {
      void refreshModelProviderDescriptors(true);
    }
  }, [transport]);

  useEffect(() => {
    const transcriptEl = transcriptRef.current;
    if (transcriptEl) {
      transcriptEl.scrollTo({
        top: transcriptEl.scrollHeight,
        behavior: "smooth",
      });
    }
  }, [transcript]);

  useEffect(() => {
    setSlashCommandIndex(0);
  }, [input]);

  useEffect(() => {
    setSlashCommandIndex((index) =>
      Math.min(index, Math.max(0, slashCommandItems.length - 1)),
    );
  }, [slashCommandItems.length]);

  useEffect(() => {
    if (!running || runStartedAtRef.current === null) {
      return;
    }
    const timer = window.setInterval(() => {
      if (runStartedAtRef.current !== null) {
        setElapsedMs(Math.max(0, Math.round(performance.now() - runStartedAtRef.current)));
      }
    }, 250);
    return () => window.clearInterval(timer);
  }, [running]);

  useEffect(() => {
    if (!running || !lastRunId) {
      return;
    }
    const onEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }
      event.preventDefault();
      void cancelLastRun();
    };
    window.addEventListener("keydown", onEscape);
    return () => window.removeEventListener("keydown", onEscape);
  }, [lastRunId, running, stopRetentionMode]);

  useEffect(() => {
    return () => {
      if (voicePreviewUrl) {
        URL.revokeObjectURL(voicePreviewUrl);
      }
    };
  }, [voicePreviewUrl]);

  function handleRunEvent(evt: RunEvent) {
    const k = evt.kind;
    switch (k.type) {
      case "RunStarted":
        if (rootRunIdRef.current === null) {
          rootRunIdRef.current = evt.run_id;
          setLastRunId(evt.run_id);
          runStartedAtRef.current = performance.now();
          setElapsedMs(0);
        } else if (evt.run_id !== rootRunIdRef.current) {
          appendEvent(`Run started: ${evt.run_id.slice(0, 8)}`);
        }
        return;
      case "ContextBuilt":
        if (rootRunIdRef.current === null || evt.run_id === rootRunIdRef.current) {
          latestRunContextRef.current = k.snapshot;
          setContextPreview(k.snapshot);
          setContextPreviewPrompt(null);
        }
        const compaction = compactionMetrics(k.snapshot);
        appendEvent(
          `Context built (${k.snapshot.visible_tools.length} tools, ${k.snapshot.loaded_memory.length} memory fragments${compaction ? `, ${compactionCardValue(compaction)}` : ""})`,
        );
        return;
      case "LlmRequestStarted":
        const requestDigest = k.request_digest
          ? ` digest ${k.request_digest.slice(0, 12)}`
          : "";
        appendEvent(
          `LLM call started (${k.model})${requestDigest}`,
        );
        return;
      case "LlmStreamToken":
        return;
      case "LlmRequestCompleted":
        setTokensIn((v) => v + k.tokens_in);
        setTokensOut((v) => v + k.tokens_out);
        const eventCostUsd = k.cost_usd;
        if (eventCostUsd !== null) {
          setCostUsd((v) => v + eventCostUsd);
        }
        const cost = eventCostUsd === null ? "" : `, $${eventCostUsd.toFixed(6)}`;
        appendEvent(
          `LLM call completed (in: ${k.tokens_in}, out: ${k.tokens_out}${cost}, ${k.duration_ms} ms)`,
        );
        return;
      case "PromptRefinementStarted":
        appendEvent(`Prompt refinement started (${k.model})`);
        return;
      case "PromptRefinementCompleted":
        setTokensIn((v) => v + k.tokens_in);
        setTokensOut((v) => v + k.tokens_out);
        const refinementCostUsd = k.cost_usd;
        if (refinementCostUsd !== null) {
          setCostUsd((v) => v + refinementCostUsd);
        }
        appendEvent(
          `Prompt refined (in: ${k.tokens_in}, out: ${k.tokens_out}, ${k.duration_ms} ms)`,
        );
        appendLine("event", `Refined prompt: ${k.refined_input}`);
        return;
      case "ToolCallProposed":
        appendEvent(
          `Tool proposed: ${k.tool_id}(${JSON.stringify(k.input)})${
            k.model ? ` via ${k.model}` : ""
          } [${k.call_id}]`,
        );
        return;
      case "ToolCallStarted":
        appendEvent(`Tool started [${k.call_id}]`);
        return;
      case "ToolCallCompleted":
        setCalls((v) => v + 1);
        const toolCostUsd = k.cost_usd;
        if (toolCostUsd !== null) {
          setCostUsd((v) => v + toolCostUsd);
        }
        const toolCost =
          toolCostUsd === null ? "" : `, $${toolCostUsd.toFixed(6)}`;
        appendEvent(
          `Tool completed [${k.call_id}] -> ${JSON.stringify(k.output)} (${k.duration_ms} ms${toolCost})`,
        );
        return;
      case "ToolOutputInterpreted":
        appendEvent(
          `Tool output queued for interpretation [${k.call_id}] by ${k.model}: ${k.summary}`,
        );
        return;
      case "ToolCallFailed":
        appendLine("error", `Tool failed [${k.call_id}]: ${k.error}`);
        return;
      case "ApprovalRequested":
        setApprovals((items) =>
          upsertApproval(items, {
            approval_id: k.approval_id,
            action: k.action,
            reason: k.reason,
            controller_agent: k.controller_agent ?? null,
            controller_scope: k.controller_scope ?? [],
            status: "pending",
            approved: null,
          }),
        );
        appendEvent(`Approval requested [${k.approval_id}] for ${k.action}`);
        return;
      case "ApprovalResolved":
        setApprovals((items) =>
          items.map((approval) =>
            approval.approval_id === k.approval_id
              ? {
                  ...approval,
                  status: k.approved ? "approved" : "rejected",
                  approved: k.approved,
                  delegated_controller:
                    k.delegated_controller ?? approval.delegated_controller,
                }
              : approval,
          ),
        );
        appendEvent(
          `Approval resolved [${k.approval_id}] approved=${String(k.approved)}`,
        );
        return;
      case "ApprovalControllerAssessed": {
        const assessment: ApprovalAssessment = {
          approval_id: k.approval_id,
          controller_agent: k.controller_agent,
          status: "assessed",
          scope: k.scope,
          reason: k.summary,
          model: k.model,
          recommendation: k.recommendation,
          tokens_in: k.tokens_in,
          tokens_out: k.tokens_out,
          duration_ms: k.duration_ms,
        };
        setApprovals((items) =>
          upsertApprovalAssessment(items, k.approval_id, assessment),
        );
        appendEvent(
          `Approval assessed [${k.approval_id}] by ${k.controller_agent}: ${k.recommendation}`,
        );
        return;
      }
      case "GuidanceInjected":
        appendEvent(`Guidance injected: ${k.content}`);
        return;
      case "QualityScored":
        appendEvent(`Quality scored ${k.target}: ${k.score}/10`);
        return;
      case "MemoryLoaded":
        appendEvent(`Memory loaded: ${k.ids.join(", ")}`);
        return;
      case "MemoryRead":
        appendEvent(
          `Memory read via ${k.backend}: ${k.fragment_ids.join(", ")}`,
        );
        return;
      case "MemoryWritten":
        const memoryRange = k.source_range
          ? ` (range: ${k.source_range})`
          : "";
        const memoryModel = k.generating_model
          ? ` via ${k.generating_model}`
          : "";
        appendEvent(
          `Memory ${k.operation}: ${k.id}${memoryRange}${memoryModel}`,
        );
        return;
      case "IngestionReferenced":
        appendEvent(`Ingestion referenced: ${k.artifact_id} (${k.source})`);
        return;
      case "IngestionStarted":
        appendEvent(`Ingestion started: ${k.source} via ${k.backend}`);
        return;
      case "IngestionCompleted": {
        const risk = k.high_risk_findings
          ? `; ${k.high_risk_findings} high-risk findings`
          : k.findings?.length
            ? `; ${k.findings.length} findings`
            : "";
        const snippet = k.finding_snippets?.[0]
          ? `; source "${k.finding_snippets[0]}"`
          : "";
        appendEvent(
          `Ingestion completed: ${k.artifact_id} (${k.sections} sections${risk}${snippet})`,
        );
        return;
      }
      case "HookFired":
        appendEvent(
          `Hook fired: ${k.hook_id} (${k.trigger}, digest ${k.payload_digest.slice(0, 12)})`,
        );
        return;
      case "HookFailed":
        appendEvent(
          `Hook failed: ${k.hook_id} (${k.trigger}, attempt ${k.attempt}, retry=${k.will_retry}): ${k.error}`,
        );
        return;
      case "PolicyDenied":
        appendEvent(`Policy denied: ${k.reason}`);
        return;
      case "ChildRunStarted":
        appendEvent(`Child run started: ${k.child_run_id} (${k.agent_id})`);
        return;
      case "ChildRunCompleted":
        appendEvent(`Child run completed: ${k.child_run_id} (${k.status})`);
        return;
      case "BatchRunStarted":
        appendEvent(`Batch started: ${k.batch_id} (${k.items} items)`);
        return;
      case "BatchItemStatus":
        appendEvent(`Batch ${k.batch_id} item ${k.item_key}: ${k.status}`);
        return;
      case "BatchRunCompleted":
        appendEvent(
          `Batch completed: ${k.batch_id} (${k.succeeded} ok, ${k.failed} failed)`,
        );
        return;
      case "RunPaused":
        if (evt.run_id !== rootRunIdRef.current) {
          appendEvent(`Run paused: ${evt.run_id.slice(0, 8)} (${k.reason})`);
          return;
        }
        terminalEventSeenRef.current = true;
        appendLine("error", `Run paused: ${k.reason}`);
        setRunning(false);
        runStartedAtRef.current = null;
        return;
      case "RunCancelled":
        if (evt.run_id !== rootRunIdRef.current) {
          appendEvent(`Run cancelled: ${evt.run_id.slice(0, 8)} (${k.reason})`);
          return;
        }
        terminalEventSeenRef.current = true;
        appendLine("error", `Run cancelled: ${k.reason}`);
        setRunning(false);
        runStartedAtRef.current = null;
        return;
      case "RunCompleted":
        if (k.total_cost_usd !== null) {
          setCostUsd(k.total_cost_usd);
        }
        const totalCost =
          k.total_cost_usd === null ? "" : `, $${k.total_cost_usd.toFixed(6)}`;
        if (evt.run_id !== rootRunIdRef.current) {
          appendEvent(
            `Run completed: ${evt.run_id.slice(0, 8)} (${k.total_duration_ms} ms${totalCost})`,
          );
          return;
        }
        terminalEventSeenRef.current = true;
        appendLine("assistant", k.final_output);
        appendEvent(`Run completed in ${k.total_duration_ms} ms${totalCost}`);
        promptForPostRunCompaction(evt.run_id);
        setElapsedMs(k.total_duration_ms);
        setRunning(false);
        runStartedAtRef.current = null;
        return;
      case "RunFailed":
        if (evt.run_id !== rootRunIdRef.current) {
          appendEvent(`Run failed: ${evt.run_id.slice(0, 8)} (${k.reason})`);
          return;
        }
        terminalEventSeenRef.current = true;
        appendLine("error", `Run failed: ${k.reason}`);
        setRunning(false);
        runStartedAtRef.current = null;
        return;
    }
  }

  function appendEvent(text: string) {
    setTranscript((t) => [...t, { kind: "event", text }]);
  }
  function appendLine(kind: LineKind, text: string) {
    setTranscript((t) => [...t, { kind, text }]);
  }

  function parsedMemoryTopics() {
    return memoryTopics
      .split(",")
      .map((topic) => topic.trim().toLowerCase())
      .filter(Boolean)
      .filter((topic, index, topics) => topics.indexOf(topic) === index);
  }

  function parsedCategoryList(value: string) {
    return value
      .split(",")
      .map((category) => category.trim())
      .filter(Boolean)
      .filter(
        (category, index, categories) => categories.indexOf(category) === index,
      );
  }

  function optionalList<T>(items: T[]) {
    return items.length ? items : null;
  }

  function runtimeOptions(): RunOptions {
    return {
      provider,
      agent_id: agentId.trim() || null,
      model: model.trim() || null,
      api_base_url: apiBaseUrl.trim() || null,
      api_key_env: apiKeyEnv.trim() || "OPENAI_API_KEY",
      api_key: apiKey.trim() || null,
      max_output_tokens: null,
      temperature: null,
      input_cost_per_million: parseOptionalNonNegativeFloat(inputCostPerMillion),
      output_cost_per_million: parseOptionalNonNegativeFloat(outputCostPerMillion),
      max_tool_calls: parseOptionalNonNegativeInt(maxToolCalls),
      max_tokens_before_compaction: parseOptionalPositiveInt(
        maxTokensBeforeCompaction,
      ),
      max_compaction_output_tokens: parseOptionalPositiveInt(
        maxCompactionOutputTokens,
      ),
      compaction_guidance: compactionGuidance.trim() || null,
      allowed_tool_categories: parsedCategoryList(allowedToolCategories),
      allowed_skill_categories: parsedCategoryList(allowedSkillCategories),
      tool_visibility: toolVisibility || null,
      skill_visibility: null,
      enable_shell: enableShell,
      enable_subagent: enableSubagent,
      enable_capability_drafts: enableCapabilityDrafts,
      load_memory: loadMemory,
      memory_topics: parsedMemoryTopics(),
      load_skills: loadSkills,
      include_ingest: includeIngestIds,
      allow_unsafe_ingest: allowUnsafeIngest,
      enable_prompt_refinement: enablePromptRefinement,
      prompt_refinement_instructions:
        promptRefinementInstructions.trim() || null,
      prompt_refinement_model: promptRefinementModel.trim() || null,
      require_approval: requireApproval,
      auto_approve: !requireApproval,
      raw_tool_output: rawToolOutput,
      tool_routing_model: toolRoutingModel.trim() || null,
      tool_output_interpretation_model:
        toolOutputInterpretationModel.trim() || null,
      disable_lifecycle_hooks: false,
      compacted_context: manualCompactedContext.trim() || null,
      conversation_id: conversationId.trim() || null,
    };
  }

  function daemonBaseUrl() {
    return (daemonUrl.trim() || "http://127.0.0.1:7878").replace(/\/+$/, "");
  }

  async function daemonJson<T>(
    path: string,
    body?: Record<string, unknown>,
  ): Promise<T> {
    const response = await fetch(`${daemonBaseUrl()}${path}`, {
      method: body ? "POST" : "GET",
      headers: body ? { "content-type": "application/json" } : undefined,
      body: body ? JSON.stringify(body) : undefined,
    });
    const text = await response.text();
    const value = text ? JSON.parse(text) : null;
    if (!response.ok) {
      const message =
        value && typeof value === "object" && "error" in value
          ? String((value as { error: unknown }).error)
          : text || `HTTP ${response.status}`;
      throw new Error(message);
    }
    return value as T;
  }

  function captureRunIdFromError(message: string) {
    const match = message.match(
      /run_id=([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/,
    );
    if (match) {
      setLastRunId(match[1]);
    }
  }

  function captureDirectToolMetadata(value: unknown) {
    const runId = runIdFromValue(value);
    if (runId) {
      setLastRunId(runId);
      rootRunIdRef.current = runId;
      setTraceEvents([]);
      setTraceSummary(null);
      setTraceTree(null);
      setCollapsedTraceTreeRuns([]);
      setTraceCompareSummary(null);
      setTraceCompareTree(null);
      setTraceCompareRunId("");
    }
    const duration = durationMsFromValue(value);
    if (duration !== null) {
      setElapsedMs(duration);
    }
    setCalls((count) => count + 1);
  }

  function runIdFromValue(value: unknown) {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return null;
    }
    const runId = (value as Record<string, unknown>).run_id;
    return typeof runId === "string" &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(runId)
      ? runId
      : null;
  }

  function durationMsFromValue(value: unknown) {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return null;
    }
    const duration = (value as Record<string, unknown>).duration_ms;
    return typeof duration === "number" && Number.isFinite(duration)
      ? Math.max(0, Math.round(duration))
      : null;
  }

  function sleep(ms: number) {
    return new Promise<void>((resolve) => window.setTimeout(resolve, ms));
  }

  async function appendRemoteRunEvents(runId: string) {
    const events = await daemonJson<RunEvent[]>(`/trace/${runId}`);
    for (const evt of events) {
      const key = `${evt.run_id}:${evt.id}`;
      if (remoteSeenEventKeysRef.current.has(key)) {
        continue;
      }
      remoteSeenEventKeysRef.current.add(key);
      handleRunEvent(evt);
    }
  }

  async function pollRemoteRun(runId: string) {
    for (;;) {
      await appendRemoteRunEvents(runId);
      const status = await daemonJson<RemoteRunStatus>(`/run/status/${runId}`);
      if (status.status === "running" || status.status === "unknown") {
        await sleep(500);
        continue;
      }
      await appendRemoteRunEvents(runId);
      if (status.status === "completed") {
        if (status.total_cost_usd !== null) {
          setCostUsd(status.total_cost_usd);
        }
        if (status.total_duration_ms !== null) {
          setElapsedMs(status.total_duration_ms);
        }
        if (!terminalEventSeenRef.current) {
          appendLine("assistant", status.final_output ?? "(no final output returned)");
          appendEvent(
            `Remote run completed: ${runId}${
              status.total_duration_ms === null
                ? ""
                : ` (${status.total_duration_ms} ms)`
            }${status.total_cost_usd === null ? "" : `, $${status.total_cost_usd.toFixed(6)}`}`,
          );
        }
      } else if (status.status === "cancelled") {
        if (!terminalEventSeenRef.current) {
          appendLine(
            "error",
            `Remote run cancelled: ${status.reason ?? "user requested stop"}`,
          );
        }
      } else if (status.status === "paused") {
        if (!terminalEventSeenRef.current) {
          appendLine("error", `Remote run paused: ${status.reason ?? "approval required"}`);
        }
      } else {
        if (!terminalEventSeenRef.current) {
          appendLine("error", `Remote run failed: ${status.reason ?? status.status}`);
        }
      }
      setRunning(false);
      runStartedAtRef.current = null;
      return;
    }
  }

  function requireOpsValue(label: string) {
    const value = opsValue.trim();
    if (!value) {
      appendLine("error", `${label} needs a value.`);
      return null;
    }
    return value;
  }

  function requireOpsId(label: string) {
    const id = opsId.trim();
    if (!id) {
      appendLine("error", `${label} needs an id.`);
      return null;
    }
    return id;
  }

  function retentionDaysFromOps(label: string) {
    const raw = requireOpsValue(label);
    if (!raw) return null;
    const days = Number(raw);
    if (!Number.isInteger(days) || days <= 0) {
      appendLine("error", `${label} needs Value to be a positive whole number of days.`);
      return null;
    }
    return days;
  }

  function qualityScoreFromOps() {
    const raw = opsValue.trim();
    if (!raw) {
      return 10;
    }
    const score = Number(raw);
    if (!Number.isFinite(score) || score < 0 || score > 10) {
      appendLine("error", "Score needs Value to be a number from 0 to 10.");
      return null;
    }
    return score;
  }

  function parseOpsJsonObject(label: string) {
    return parseJsonObject(label, opsValue.trim());
  }

  function parseJsonObject(label: string, value: string) {
    if (!value) {
      return {};
    }
    try {
      const parsed = JSON.parse(value) as unknown;
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        appendLine("error", `${label} needs a JSON object input.`);
        return null;
      }
      return parsed as Record<string, unknown>;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `${label} input is not valid JSON: ${msg}`);
      return null;
    }
  }

  function parseConversationRangeFromOps(label = "Conversation range delete") {
    const value = requireOpsValue(label);
    if (!value) return null;
    const parsed = parseJsonObject(label, value);
    if (!parsed) return null;
    const from = Number(parsed.from);
    const to = Number(parsed.to);
    if (
      !Number.isInteger(from) ||
      !Number.isInteger(to) ||
      from < 0 ||
      to < 0
    ) {
      appendLine(
        "error",
        `${label} needs Value like { "from": 2, "to": 4 }.`,
      );
      return null;
    }
    if (from > to) {
      appendLine("error", "Conversation range start must be before the end.");
      return null;
    }
    return { from, to } satisfies ConversationRange;
  }

  function parseDirectToolShortcut(text: string) {
    const trimmed = text.trim();
    const rest = trimmed.startsWith("/tool!")
      ? trimmed.slice("/tool!".length).trim()
      : null;
    if (rest === null) return null;
    const match = rest.match(/^(\S+)(?:\s+([\s\S]*))?$/);
    if (!match) {
      appendLine("error", "Direct tool shortcut needs a tool name.");
      return null;
    }
    return {
      name: match[1],
      inputText: match[2]?.trim() || "{}",
    };
  }

  function isShellRuntimeToolName(name: string) {
    return name === "shell" || name === "code_python" || name === "code_typescript";
  }

  function parsePreviewShortcut(text: string) {
    const trimmed = text.trim();
    if (trimmed === "/preview") {
      return "";
    }
    return trimmed.startsWith("/preview ")
      ? trimmed.slice("/preview ".length).trim()
      : null;
  }

  function parseAgentShortcut(text: string) {
    const trimmed = text.trim();
    if (trimmed === "/agent") {
      appendLine("error", "Agent shortcut needs an agent id: echo, tool, or a saved agent id.");
      return null;
    }
    if (!trimmed.startsWith("/agent ")) {
      return null;
    }
    const id = trimmed.slice("/agent ".length).trim();
    if (!id) {
      appendLine("error", "Agent shortcut needs an agent id.");
      return null;
    }
    return id;
  }

  function parseExportShortcut(text: string) {
    const trimmed = text.trim();
    if (trimmed === "/export") {
      return defaultBundlePath();
    }
    return trimmed.startsWith("/export ")
      ? trimmed.slice("/export ".length).trim()
      : null;
  }

  function slashCommandCatalog(): SlashCommandSuggestion[] {
    const forcedToolCommands = contextPreview?.visible_tools.map((tool) => ({
      command: `/tool ${tool.id} `,
      label: `Ask ${tool.name} through the agent`,
    })) ?? [
      {
        command: "/tool echo ",
        label: "Ask echo through the agent",
      },
    ];
    const toolCommands = contextPreview?.visible_tools.map((tool) => ({
      command: `/tool!${tool.id} ${compactJson(sampleToolInput(tool.input_schema))}`,
      label: `Call ${tool.name} directly`,
    })) ?? [
      {
        command: '/tool!echo {"text":"hello"}',
        label: "Call echo directly",
      },
    ];
    const commands: SlashCommandSuggestion[] = [
      { command: "/help", label: "Show shortcuts" },
      { command: "/preview", label: "Preview context" },
      { command: "/agent tool", label: "Switch to Tool agent" },
      { command: "/agent echo", label: "Switch to Echo agent" },
      { command: "/agent ", label: "Use saved agent id" },
      { command: "/agents", label: "List saved agents" },
      { command: "/agents show ", label: "Show saved agent" },
      { command: "/agents use ", label: "Use saved agent" },
      { command: "/agents delete ", label: "Delete saved agent" },
      ...forcedToolCommands,
      ...toolCommands,
      { command: "/run ", label: "Run saved prompt" },
      { command: "/prompt ", label: "Load saved prompt" },
      { command: "/prompts", label: "List saved prompts" },
      { command: "/prompts list", label: "List saved prompts" },
      { command: "/prompts show ", label: "Show saved prompt" },
      { command: "/prompts use ", label: "Load saved prompt" },
      { command: "/prompts preview ", label: "Preview saved prompt context" },
      { command: "/prompts delete ", label: "Delete saved prompt" },
      { command: "/models", label: "List model metadata" },
      { command: "/models list", label: "List model metadata" },
      { command: "/models show ", label: "Show model metadata" },
      { command: "/models probe ", label: "Probe model capabilities" },
      { command: "/models delete ", label: "Delete model metadata" },
      { command: "/models providers", label: "List model providers" },
      { command: "/models doctor", label: "Run model doctor" },
      { command: "/models provider-catalog", label: "Show provider catalog" },
      { command: "/models provider-catalog export ", label: "Export provider catalog" },
      { command: "/models provider-catalog import ", label: "Import provider catalog" },
      { command: "/models metadata-catalog", label: "Show metadata catalog" },
      { command: "/models metadata-catalog export ", label: "Export metadata catalog" },
      { command: "/models metadata-catalog import ", label: "Import metadata catalog" },
      { command: "/simple", label: "Use low-overhead answer mode" },
      { command: "/router", label: "Use one-action raw router mode" },
      { command: "/answer", label: "Use zero tool calls" },
      { command: "/action", label: "Use one tool call" },
      { command: "/workflow", label: `Use default ${CALLS_MAX}-call workflow` },
      { command: "/budget ", label: "Set max tool calls" },
      { command: "/visibility full", label: "Show full tool schemas" },
      { command: "/visibility descriptions", label: "Show tool names and descriptions" },
      { command: "/visibility names", label: "Show tool names only" },
      { command: "/visibility config", label: "Use configured tool visibility" },
      { command: "/approval on", label: "Require approval for tool actions" },
      { command: "/approval off", label: "Auto-approve tool actions" },
      { command: "/approval status", label: "Show approval gate status" },
      { command: "/approval list", label: "List approvals for current run" },
      { command: "/approval list last", label: "List approvals for last run" },
      { command: "/approval assess ", label: "Assess an approval" },
      { command: "/approval approve ", label: "Approve and execute an approval" },
      { command: "/approval reject ", label: "Reject an approval" },
      { command: "/approval execute ", label: "Execute an approved action" },
      { command: "/refine on", label: "Enable prompt refinement" },
      { command: "/refine off", label: "Disable prompt refinement" },
      { command: "/refine status", label: "Show prompt refinement status" },
      { command: "/refine model ", label: "Set refiner model" },
      { command: "/refine instructions ", label: "Set refinement instructions" },
      { command: "/shell on", label: "Enable shell tool access" },
      { command: "/shell off", label: "Disable shell tool access" },
      { command: "/shell status", label: "Show shell access status" },
      { command: "/python ", label: "Run Python code" },
      { command: "/typescript ", label: "Run TypeScript code" },
      { command: "/ts ", label: "Run TypeScript code" },
      { command: "/x402 request ", label: "Probe an x402 endpoint" },
      { command: "/x402 required ", label: "Build an x402 payment challenge" },
      { command: "/x402 settle ", label: "Verify and settle an x402 payment" },
      { command: "/payment x402-request ", label: "Probe an x402 endpoint" },
      { command: "/payment x402-required ", label: "Build an x402 payment challenge" },
      { command: "/payment x402-settle ", label: "Verify and settle an x402 payment" },
      { command: "/memory on", label: "Load memory in context" },
      { command: "/memory off", label: "Stop loading memory" },
      { command: "/memory status", label: "Show memory loading status" },
      { command: "/memory list", label: "List memory records" },
      { command: "/memory access", label: "Show visible memory access" },
      { command: "/memory backends", label: "List memory backends" },
      { command: "/memory preview", label: "Preview context with memory" },
      { command: "/memory create ", label: "Create memory record" },
      { command: "/memory generate ", label: "Generate memory from text" },
      { command: "/memory generate-conversation ", label: "Generate memory from conversation range" },
      { command: "/memory classify ", label: "Classify memory record" },
      { command: "/memory edit ", label: "Edit memory record" },
      { command: "/memory delete ", label: "Delete memory record" },
      { command: "/memory rollback --confirm", label: "Rollback memory file" },
      { command: "/skills on", label: "Load skills in context" },
      { command: "/skills off", label: "Stop loading skills" },
      { command: "/skills status", label: "Show skill loading status" },
      { command: "/skills list", label: "List imported skills" },
      { command: "/skills show ", label: "Show imported skill" },
      { command: "/skills import-openclaw ", label: "Import OpenClaw skill" },
      { command: "/skills import-doc ", label: "Import portable skill doc" },
      { command: "/skills export ", label: "Export portable skill doc" },
      { command: "/skills allow ", label: "Allow quarantined skill" },
      { command: "/skills quarantine ", label: "Quarantine skill" },
      { command: "/subagent on", label: "Enable subagent tool" },
      { command: "/subagent off", label: "Disable subagent tool" },
      { command: "/subagent status", label: "Show subagent status" },
      { command: "/cost input ", label: "Set input token cost per million" },
      { command: "/cost output ", label: "Set output token cost per million" },
      { command: "/cost both ", label: "Set input and output token costs" },
      { command: "/cost clear", label: "Use configured model costs" },
      { command: "/cost status", label: "Show token cost overrides" },
      { command: "/usage", label: "Show current usage totals" },
      { command: "/usage trace", label: "Load last trace usage totals" },
      { command: "/usage trace ", label: "Load run trace usage totals by id" },
      { command: "/usage run ", label: "Load run usage totals by id" },
      { command: "/score 10", label: "Score last answer" },
      { command: "/score conversation 10", label: "Score the full conversation" },
      { command: "/score range:important 8", label: "Score a selected range" },
      { command: "/scores", label: "Review quality scores" },
      { command: "/resume", label: "Resume last or selected run" },
      { command: "/resume ", label: "Resume a run by id" },
      { command: "/stop", label: "Stop current run" },
      { command: "/stop default", label: "Use configured stop mode" },
      { command: "/stop discard", label: "Stop without retaining context" },
      { command: "/stop summarise", label: "Stop and retain a summary" },
      { command: "/stop status", label: "Show stop retention mode" },
      { command: "/compact ", label: "Create a guided compaction draft" },
      { command: "/compact status", label: "Show manual compacted context" },
      { command: "/compact clear", label: "Clear manual compacted context" },
      { command: "/compactions", label: "List compacted-context artifacts" },
      { command: "/compactions list", label: "List compacted-context artifacts" },
      { command: "/compactions show ", label: "Show compacted-context artifact" },
      { command: "/compactions use ", label: "Use compacted-context artifact" },
      { command: "/compactions export ", label: "Export compacted-context artifact" },
      { command: "/compactions import ", label: "Import compacted-context artifact" },
      { command: "/compactions delete ", label: "Delete compacted-context artifact" },
      { command: "/conversation", label: "List conversation branches" },
      { command: "/conversation list", label: "List conversation branches" },
      { command: "/conversation tree", label: "Show conversation tree" },
      { command: "/conversation select ", label: "Select conversation branch" },
      { command: "/conversation show ", label: "Show conversation branch" },
      { command: "/conversation recover ", label: "Recover conversation context" },
      { command: "/conversation policy", label: "Show conversation policy" },
      { command: "/conversation policy apply", label: "Apply conversation policy" },
      { command: "/conversation policy save", label: "Save conversation policy" },
      { command: "/conversation policy clear", label: "Clear conversation policy" },
      { command: "/conversation delete-plan ", label: "Preview conversation deletion" },
      { command: "/conversation delete ", label: "Delete conversation branch" },
      { command: "/conversation range-delete ", label: "Delete conversation message range" },
      { command: "/guardrails", label: "Review ingestion guardrails" },
      { command: "/guardrails unsafe on", label: "Allow flagged ingestion content" },
      { command: "/guardrails unsafe off", label: "Block flagged ingestion content" },
      { command: "/guardrails status", label: "Show guardrail status" },
      { command: "/raw", label: "Use raw tool outputs" },
      { command: "/interpret", label: "Interpret tool outputs" },
      { command: "/interpret ", label: "Set interpreter model" },
      { command: "/interpret clear", label: "Use configured interpreter model" },
      { command: "/interpret status", label: "Show interpreter model" },
      { command: "/router-model ", label: "Set tool routing model" },
      { command: "/router-model clear", label: "Use configured routing model" },
      { command: "/router-model status", label: "Show routing model" },
      { command: "/export", label: "Export backup bundle" },
      { command: "/config", label: "Explain effective config" },
      { command: "/tools", label: "Show visible tools" },
      { command: "/storage", label: "Show storage usage" },
      { command: "/storage report", label: "Show storage usage" },
      { command: "/storage prune-cache ", label: "Plan cache pruning" },
      { command: "/storage prune-cache 30 --apply", label: "Apply cache pruning" },
      { command: "/memory", label: "List memory records" },
      { command: "/voice status", label: "Show voice artifacts" },
      { command: "/voice capture", label: "Start voice capture" },
      { command: "/voice stop", label: "Stop voice capture" },
      { command: "/voice transcribe", label: "Transcribe latest voice capture" },
      { command: "/voice transcribe ", label: "Transcribe an audio path" },
      { command: "/voice speak ", label: "Create speech from text" },
      { command: "/voice stage ", label: "Stage speech tool input" },
      { command: "/ingest", label: "List ingestion artifacts" },
      { command: "/ingest list", label: "List ingestion artifacts" },
      { command: "/ingest backends", label: "List ingestion backends" },
      { command: "/ingest add ", label: "Ingest a file path" },
      { command: "/ingest probe-vision ", label: "Probe vision ingestion" },
      { command: "/ingest probe ", label: "Probe vision ingestion" },
      { command: "/ingest show ", label: "Show ingestion artifact" },
      { command: "/ingest rerun ", label: "Rerun ingestion artifact" },
      { command: "/ingest use ", label: "Use ingestion artifact" },
      { command: "/ingest preview ", label: "Preview context with artifact" },
      { command: "/ingest review ", label: "Review ingestion finding" },
      { command: "/ingest delete ", label: "Delete ingestion artifact" },
      { command: "/artifacts", label: "List generated artifacts" },
      { command: "/artifacts list", label: "List generated artifacts" },
      { command: "/artifacts show ", label: "Show generated artifact" },
      { command: "/artifacts open ", label: "Open generated artifact" },
      { command: "/artifacts preview ", label: "Preview generated artifact" },
      { command: "/artifacts delete ", label: "Delete generated artifact" },
      { command: "/skills", label: "List imported skills" },
      { command: "/capabilities", label: "List capability drafts" },
      { command: "/capabilities list", label: "List capability drafts" },
      { command: "/capabilities show ", label: "Show capability draft" },
      { command: "/capabilities export ", label: "Export capability draft" },
      { command: "/capabilities import ", label: "Import capability draft" },
      { command: "/capabilities allow ", label: "Allow capability draft" },
      { command: "/capabilities reject ", label: "Reject capability draft" },
      { command: "/capabilities delete ", label: "Delete capability draft" },
      { command: "/profiles", label: "List profiles" },
      { command: "/profiles current", label: "Show current profile" },
      { command: "/profiles show ", label: "Show a profile" },
      { command: "/profiles create ", label: "Create a profile" },
      { command: "/profiles delete ", label: "Delete a profile" },
      { command: "/profiles grants", label: "List profile grants" },
      { command: "/profiles grant ", label: "Grant profile access" },
      { command: "/profiles revoke ", label: "Revoke profile grant" },
      { command: "/secrets backends", label: "List secret backends" },
      { command: "/secrets list", label: "List secret metadata" },
      { command: "/secrets show ", label: "Show secret metadata" },
      { command: "/secrets delete ", label: "Delete secret metadata" },
      { command: "/bundles backup", label: "Export profile backup bundle" },
      { command: "/bundles export ", label: "Export profile bundle" },
      { command: "/bundles import ", label: "Import profile bundle" },
      { command: "/adapters", label: "List adapter manifests" },
      { command: "/adapters list", label: "List adapter manifests" },
      { command: "/adapters doctor", label: "Check adapter operability" },
      { command: "/adapters show ", label: "Show adapter manifest" },
      { command: "/adapters import ", label: "Import adapter package" },
      { command: "/adapters import-manifest ", label: "Import adapter manifest" },
      { command: "/adapters export ", label: "Export adapter manifest" },
      { command: "/adapters install-skill ", label: "Install adapter as skill" },
      { command: "/adapters allow ", label: "Allow adapter manifest" },
      { command: "/adapters quarantine ", label: "Quarantine adapter manifest" },
      { command: "/bridge-deliveries", label: "List bridge dead letters" },
      { command: "/bridge-deliveries retry ", label: "Retry bridge delivery" },
      { command: "/bridge-deliveries retry-all", label: "Retry all bridge deliveries" },
      { command: "/hooks", label: "List lifecycle hooks" },
      { command: "/hooks available", label: "List lifecycle hooks" },
      { command: "/hooks list", label: "Show lifecycle hook policy" },
      { command: "/hooks policy", label: "Show lifecycle hook policy" },
      { command: "/hooks review", label: "Review hook failures" },
      { command: "/hooks disable ", label: "Disable lifecycle hook" },
      { command: "/hooks enable ", label: "Enable lifecycle hook" },
      { command: "/trace", label: "Load last run trace" },
      { command: "/trace ", label: "Load a run trace by id" },
      { command: "/trace prompt", label: "Load loaded trace prompt" },
      { command: "/trace clear", label: "Clear loaded trace" },
      { command: "/compare ", label: "Compare loaded trace to a run id" },
      { command: "/compare clear", label: "Clear comparison trace" },
      { command: "/replay", label: "Replay loaded trace prompt" },
      { command: "/replay ", label: "Replay a run trace by id" },
      { command: "/replay --no-hooks", label: "Replay loaded trace without hooks" },
      { command: "/approvals", label: "Review current run approvals" },
      { command: "/batch ", label: "Run lines as deterministic batch" },
      { command: "/resume-batch ", label: "Resume deterministic batch" },
    ];
    if (lastRunId) {
      commands.push(
        { command: "/guide ", label: "Guide current run" },
      );
    }
    for (const prompt of promptDocs.slice(0, 5)) {
      commands.push({
        command: `/run ${prompt.name}`,
        label: `Run ${prompt.name}`,
      });
      commands.push({
        command: `/prompt ${prompt.name}`,
        label: `Load ${prompt.name}`,
      });
    }
    return commands;
  }

  function slashCommandSuggestions(): SlashCommandSuggestion[] {
    const trimmed = input.trimStart();
    if (!trimmed.startsWith("/") || trimmed.includes("\n")) {
      return [];
    }
    const query = trimmed.slice(1).toLowerCase();
    return slashCommandCatalog()
      .filter((item) => {
        const haystack = `${item.command} ${item.label}`.toLowerCase();
        return haystack.includes(query);
      })
      .sort((a, b) => slashCommandRank(a, query) - slashCommandRank(b, query))
      .slice(0, 6);
  }

  function slashCommandHelpText() {
    return [
      "Available shortcuts:",
      ...slashCommandCatalog().map((item) => `${item.command} - ${item.label}`),
    ].join("\n");
  }

  function slashCommandRank(item: SlashCommandSuggestion, query: string) {
    const command = item.command.slice(1).toLowerCase();
    if (command.startsWith(query)) {
      return 0;
    }
    if (item.label.toLowerCase().startsWith(query)) {
      return 1;
    }
    return 2;
  }

  function parseCompactionExportShortcut(rest: string) {
    const match = rest.match(/^(\S+)(?:\s+([\s\S]+))?$/);
    if (!match) {
      appendLine(
        "error",
        "Compactions export shortcut needs: /compactions export <id> [path].",
      );
      return null;
    }
    return {
      id: match[1],
      path: match[2]?.trim() || undefined,
    };
  }

  function parseStoragePruneShortcut(rest: string) {
    const parts = rest.split(/\s+/).filter(Boolean);
    let days: number | null = null;
    let apply = false;
    for (const part of parts) {
      if (part === "--apply") {
        apply = true;
        continue;
      }
      const parsed = Number(part);
      if (!Number.isInteger(parsed) || parsed <= 0 || days !== null) {
        appendLine(
          "error",
          "Storage prune-cache shortcut needs: /storage prune-cache <days> [--apply].",
        );
        return null;
      }
      days = parsed;
    }
    if (days === null) {
      appendLine("error", "Storage prune-cache shortcut needs retention days.");
      return null;
    }
    return { days, apply };
  }

  function parseIngestProbeVisionShortcut(rest: string) {
    const parts = rest.split(/\s+/).filter(Boolean);
    const pathParts: string[] = [];
    let model: string | null = null;
    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index];
      if (part === "--model") {
        const value = parts[index + 1];
        if (!value || value.startsWith("--") || model !== null) {
          appendLine(
            "error",
            "Ingest probe shortcut needs: /ingest probe-vision <path> --model <model>.",
          );
          return null;
        }
        model = value;
        index += 1;
        continue;
      }
      if (part.startsWith("--model=")) {
        const value = part.slice("--model=".length).trim();
        if (!value || model !== null) {
          appendLine(
            "error",
            "Ingest probe shortcut needs: /ingest probe-vision <path> --model <model>.",
          );
          return null;
        }
        model = value;
        continue;
      }
      if (part.startsWith("--")) {
        appendLine("error", `Unknown ingest probe option: ${part}`);
        return null;
      }
      pathParts.push(part);
    }
    const path = pathParts.join(" ").trim();
    if (!path || !model) {
      appendLine(
        "error",
        "Ingest probe shortcut needs: /ingest probe-vision <path> --model <model>.",
      );
      return null;
    }
    return { path, model };
  }

  function parseIngestModelShortcut(rest: string, command: string) {
    const parts = rest.split(/\s+/).filter(Boolean);
    const targetParts: string[] = [];
    const options: IngestModelOptions = {};
    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index];
      if (
        part === "--backend" ||
        part === "--vision-model" ||
        part === "--guardrail-model"
      ) {
        const value = parts[index + 1];
        if (!value || value.startsWith("--")) {
          appendLine("error", `Ingest ${command} ${part} needs a value.`);
          return null;
        }
        if (!setIngestModelOption(options, part, value, command)) {
          return null;
        }
        index += 1;
        continue;
      }
      const inline = part.match(/^(--backend|--vision-model|--guardrail-model)=(.+)$/);
      if (inline) {
        const [, flag, value] = inline;
        if (!setIngestModelOption(options, flag, value.trim(), command)) {
          return null;
        }
        continue;
      }
      if (part.startsWith("--")) {
        appendLine("error", `Unknown ingest ${command} option: ${part}`);
        return null;
      }
      targetParts.push(part);
    }
    const target = targetParts.join(" ").trim();
    if (!target) {
      appendLine(
        "error",
        `Ingest ${command} shortcut needs a ${command === "add" ? "file path" : "artifact id"}.`,
      );
      return null;
    }
    return { target, options };
  }

  function setIngestModelOption(
    options: IngestModelOptions,
    flag: string,
    value: string,
    command: string,
  ) {
    if (!value) {
      appendLine("error", `Ingest ${command} ${flag} needs a value.`);
      return false;
    }
    if (flag === "--backend") {
      if (options.backend !== undefined) {
        appendLine("error", `Ingest ${command} accepts one --backend value.`);
        return false;
      }
      options.backend = value;
      return true;
    }
    if (flag === "--vision-model") {
      if (options.visionModel !== undefined) {
        appendLine("error", `Ingest ${command} accepts one --vision-model value.`);
        return false;
      }
      options.visionModel = value;
      return true;
    }
    if (options.guardrailModel !== undefined) {
      appendLine("error", `Ingest ${command} accepts one --guardrail-model value.`);
      return false;
    }
    options.guardrailModel = value;
    return true;
  }

  function parseX402RequestShortcut(rest: string) {
    const parts = rest.split(/\s+/).filter(Boolean);
    const url = parts.shift();
    if (!url) {
      appendLine("error", "x402 request shortcut needs a URL.");
      return null;
    }
    const inputBody: Record<string, unknown> = { url };
    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index];
      if (part === "--auto-pay") {
        inputBody.auto_pay = true;
        continue;
      }
      if (part === "--method" || part === "--max-amount" || part === "--signature-secret") {
        const value = parts[index + 1];
        if (!value || value.startsWith("--")) {
          appendLine("error", `x402 request ${part} needs a value.`);
          return null;
        }
        if (!setX402RequestOption(inputBody, part, value)) {
          return null;
        }
        index += 1;
        continue;
      }
      const inline = part.match(/^(--method|--max-amount|--signature-secret)=(.+)$/);
      if (inline) {
        const [, flag, value] = inline;
        if (!setX402RequestOption(inputBody, flag, value.trim())) {
          return null;
        }
        continue;
      }
      if (part.startsWith("--")) {
        appendLine("error", `Unknown x402 request option: ${part}`);
        return null;
      }
      appendLine("error", "x402 request accepts one URL plus option flags.");
      return null;
    }
    return inputBody;
  }

  function setX402RequestOption(
    inputBody: Record<string, unknown>,
    flag: string,
    value: string,
  ) {
    if (flag === "--method") {
      const method = value.toUpperCase();
      if (method !== "GET" && method !== "POST") {
        appendLine("error", "x402 request --method must be GET or POST.");
        return false;
      }
      inputBody.method = method;
      return true;
    }
    if (flag === "--max-amount") {
      const amount = Number(value);
      if (!Number.isFinite(amount) || amount < 0) {
        appendLine("error", "x402 request --max-amount needs a non-negative number.");
        return false;
      }
      inputBody.max_amount = amount;
      return true;
    }
    inputBody.payment_signature_secret = value;
    return true;
  }

  function parseX402RequiredShortcut(rest: string) {
    const parts = rest.split(/\s+/).filter(Boolean);
    const inputBody: Record<string, unknown> = {};
    const accept: Record<string, unknown> = { scheme: "exact" };
    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index];
      const inline = part.match(
        /^(--scheme|--network|--amount|--max-amount|--pay-to|--asset|--resource|--version|--error|--body)=(.+)$/,
      );
      const flag = inline?.[1] ?? part;
      const inlineValue = inline?.[2]?.trim();
      if (
        flag === "--scheme" ||
        flag === "--network" ||
        flag === "--amount" ||
        flag === "--max-amount" ||
        flag === "--pay-to" ||
        flag === "--asset" ||
        flag === "--resource" ||
        flag === "--version" ||
        flag === "--error" ||
        flag === "--body"
      ) {
        const value = inlineValue ?? parts[index + 1];
        if (!value || value.startsWith("--")) {
          appendLine("error", `x402 required ${flag} needs a value.`);
          return null;
        }
        if (!setX402RequiredOption(inputBody, accept, flag, value)) {
          return null;
        }
        if (inlineValue === undefined) index += 1;
        continue;
      }
      appendLine("error", `Unknown x402 required option: ${part}`);
      return null;
    }
    if (!validateX402Accept(accept, "x402 required")) {
      return null;
    }
    inputBody.accepts = [accept];
    return inputBody;
  }

  function setX402RequiredOption(
    inputBody: Record<string, unknown>,
    accept: Record<string, unknown>,
    flag: string,
    value: string,
  ) {
    if (flag === "--version") {
      const version = Number(value);
      if (!Number.isInteger(version) || version < 1) {
        appendLine("error", "x402 required --version needs a positive integer.");
        return false;
      }
      inputBody.x402_version = version;
      return true;
    }
    if (flag === "--error") {
      inputBody.error = value;
      return true;
    }
    if (flag === "--body") {
      inputBody.body = value;
      return true;
    }
    setX402AcceptOption(accept, flag, value);
    return true;
  }

  function parseX402SettleShortcut(rest: string) {
    const parts = rest.split(/\s+/).filter(Boolean);
    const paymentSignature = parts.shift();
    if (!paymentSignature || paymentSignature.startsWith("--")) {
      appendLine("error", "x402 settle shortcut needs a PAYMENT-SIGNATURE value.");
      return null;
    }
    const inputBody: Record<string, unknown> = {
      payment_signature: paymentSignature,
    };
    const accept: Record<string, unknown> = { scheme: "exact" };
    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index];
      const inline = part.match(
        /^(--facilitator|--facilitator-url|--mode|--scheme|--network|--amount|--max-amount|--pay-to|--asset|--resource|--version)=(.+)$/,
      );
      const flag = inline?.[1] ?? part;
      const inlineValue = inline?.[2]?.trim();
      if (
        flag === "--facilitator" ||
        flag === "--facilitator-url" ||
        flag === "--mode" ||
        flag === "--scheme" ||
        flag === "--network" ||
        flag === "--amount" ||
        flag === "--max-amount" ||
        flag === "--pay-to" ||
        flag === "--asset" ||
        flag === "--resource" ||
        flag === "--version"
      ) {
        const value = inlineValue ?? parts[index + 1];
        if (!value || value.startsWith("--")) {
          appendLine("error", `x402 settle ${flag} needs a value.`);
          return null;
        }
        if (!setX402SettleOption(inputBody, accept, flag, value)) {
          return null;
        }
        if (inlineValue === undefined) index += 1;
        continue;
      }
      appendLine("error", `Unknown x402 settle option: ${part}`);
      return null;
    }
    if (!inputBody.facilitator_url) {
      appendLine("error", "x402 settle needs --facilitator <url>.");
      return null;
    }
    if (!validateX402Accept(accept, "x402 settle")) {
      return null;
    }
    inputBody.payment_requirements = accept;
    return inputBody;
  }

  function setX402SettleOption(
    inputBody: Record<string, unknown>,
    accept: Record<string, unknown>,
    flag: string,
    value: string,
  ) {
    if (flag === "--facilitator" || flag === "--facilitator-url") {
      inputBody.facilitator_url = value;
      return true;
    }
    if (flag === "--mode") {
      const mode = value.replace(/-/g, "_");
      if (mode !== "verify" && mode !== "settle" && mode !== "verify_and_settle") {
        appendLine("error", "x402 settle --mode must be verify, settle, or verify-and-settle.");
        return false;
      }
      inputBody.mode = mode;
      return true;
    }
    if (flag === "--version") {
      const version = Number(value);
      if (!Number.isInteger(version) || version < 1) {
        appendLine("error", "x402 settle --version needs a positive integer.");
        return false;
      }
      inputBody.x402_version = version;
      return true;
    }
    setX402AcceptOption(accept, flag, value);
    return true;
  }

  function setX402AcceptOption(
    accept: Record<string, unknown>,
    flag: string,
    value: string,
  ) {
    if (flag === "--scheme") {
      accept.scheme = value;
    } else if (flag === "--network") {
      accept.network = value;
    } else if (flag === "--amount" || flag === "--max-amount") {
      accept.maxAmountRequired = value;
    } else if (flag === "--pay-to") {
      accept.payTo = value;
    } else if (flag === "--asset") {
      accept.asset = value;
    } else if (flag === "--resource") {
      accept.resource = value;
    }
  }

  function validateX402Accept(accept: Record<string, unknown>, label: string) {
    const missing = [
      ["--resource", accept.resource],
      ["--amount", accept.maxAmountRequired],
      ["--pay-to", accept.payTo],
      ["--asset", accept.asset],
      ["--network", accept.network],
    ]
      .filter(([, value]) => typeof value !== "string" || !value.trim())
      .map(([flag]) => flag);
    if (missing.length > 0) {
      appendLine(
        "error",
        `${label} needs ${missing.join(", ")}.`,
      );
      return false;
    }
    return true;
  }

  function bundleShortcutHelpText() {
    return [
      "/bundles backup",
      "/bundles export <path>",
      "/bundles import <path> --confirm",
    ].join("\n");
  }

  function approvalShortcutHelpText() {
    return [
      "/approval on",
      "/approval off",
      "/approval status",
      "/approval list [run-id|last]",
      "/approval assess [run-id|last] <approval-id>",
      "/approval approve [run-id|last] <approval-id>",
      "/approval reject [run-id|last] <approval-id>",
      "/approval execute [run-id|last] <approval-id>",
    ].join("\n");
  }

  function approvalShortcutRunId(raw?: string) {
    if (!raw || raw === "last") return lastRunId || "";
    return raw;
  }

  function parseApprovalListShortcut(args: string[]) {
    if (args.length > 1) {
      appendLine("error", "Approval list shortcut accepts at most one run id.");
      return null;
    }
    const runId = approvalShortcutRunId(args[0]);
    if (!runId) {
      appendLine("error", "Approval list shortcut needs a run id or previous run.");
      return null;
    }
    return runId;
  }

  function parseApprovalActionShortcut(command: string, args: string[]) {
    if (args.length === 0) {
      appendLine("error", `Approval ${command} shortcut needs an approval id.`);
      return null;
    }
    if (args.length > 2) {
      appendLine(
        "error",
        `Approval ${command} shortcut accepts [run-id|last] <approval-id>.`,
      );
      return null;
    }
    const runId = args.length === 1 ? approvalShortcutRunId() : approvalShortcutRunId(args[0]);
    const approvalId = args.length === 1 ? args[0] : args[1];
    if (!runId) {
      appendLine(
        "error",
        `Approval ${command} shortcut needs a run id or previous run.`,
      );
      return null;
    }
    if (!approvalId) {
      appendLine("error", `Approval ${command} shortcut needs an approval id.`);
      return null;
    }
    return { runId, approvalId };
  }

  function skillShortcutHelpText() {
    return [
      "/skills list",
      "/skills show <id>",
      "/skills import-openclaw <path>",
      "/skills import-doc <path>",
      "/skills export <id> <path>",
      "/skills allow <id> --confirm",
      "/skills quarantine <id> --confirm",
    ].join("\n");
  }

  function adapterShortcutHelpText() {
    return [
      "/adapters list",
      "/adapters doctor",
      "/adapters show <id>",
      "/adapters import <path>",
      "/adapters import-manifest <path>",
      "/adapters export <id> <path>",
      "/adapters install-skill <id>",
      "/adapters allow <id> --confirm",
      "/adapters quarantine <id>",
    ].join("\n");
  }

  function bridgeDeliveryShortcutHelpText() {
    return [
      "/bridge-deliveries list",
      "/bridge-deliveries retry <id>",
      "/bridge-deliveries retry-all",
    ].join("\n");
  }

  function hookShortcutHelpText() {
    return [
      "/hooks available",
      "/hooks list",
      "/hooks review [run-id]",
      "/hooks disable <hook-id> [--agent] --confirm",
      "/hooks enable <hook-id> [--agent] --confirm",
    ].join("\n");
  }

  function conversationShortcutHelpText() {
    return [
      "/conversation list",
      "/conversation tree",
      "/conversation select <id>",
      "/conversation show [id]",
      "/conversation recover [id]",
      "/conversation policy [show] [id]",
      "/conversation policy apply [id]",
      "/conversation policy save [id]",
      "/conversation policy clear [id]",
      "/conversation delete-plan [id] [--recursive]",
      "/conversation delete [id] [--recursive] --confirm",
      "/conversation range-delete [id] <from>:<to> --confirm",
    ].join("\n");
  }

  function selectedConversationShortcutId() {
    return expandedConversation?.conversation.id || opsId.trim();
  }

  function parseConversationPolicyShortcutId(args: string[], label: string) {
    if (args.length > 1) {
      appendLine("error", `${label} shortcut accepts at most one conversation id.`);
      return null;
    }
    const id = args[0] || selectedConversationShortcutId();
    if (!id) {
      appendLine("error", `${label} shortcut needs a conversation id or selected Id.`);
      return null;
    }
    return id;
  }

  function parseConversationBranchShortcut(
    command: string,
    args: string[],
    requireConfirm: boolean,
  ) {
    const recursive = args.includes("--recursive");
    const confirmed = args.includes("--confirm");
    const unknownFlags = args.filter(
      (arg) => arg.startsWith("--") && arg !== "--recursive" && arg !== "--confirm",
    );
    const ids = args.filter((arg) => !arg.startsWith("--"));
    if (unknownFlags.length || ids.length > 1) {
      appendLine(
        "error",
        `Conversation ${command} shortcut accepts [id], optional --recursive, and${requireConfirm ? " required" : " no"} --confirm.`,
      );
      return null;
    }
    if (requireConfirm && !confirmed) {
      appendLine("error", `Conversation ${command} shortcut requires --confirm.`);
      return null;
    }
    if (!requireConfirm && confirmed) {
      appendLine("error", `Conversation ${command} shortcut does not use --confirm.`);
      return null;
    }
    const id = ids[0] || selectedConversationShortcutId();
    if (!id) {
      appendLine(
        "error",
        `Conversation ${command} shortcut needs a conversation id or selected Id.`,
      );
      return null;
    }
    return { id, recursive };
  }

  function parseConversationRangeText(fromText: string, toText: string) {
    const from = Number(fromText);
    const to = Number(toText);
    if (
      !Number.isInteger(from) ||
      !Number.isInteger(to) ||
      from < 0 ||
      to < 0
    ) {
      appendLine("error", "Conversation range needs non-negative whole-number indexes.");
      return null;
    }
    if (from > to) {
      appendLine("error", "Conversation range start must be before the end.");
      return null;
    }
    return { from, to } satisfies ConversationRange;
  }

  function parseConversationRangeShortcut(args: string[]) {
    const confirmed = args.includes("--confirm");
    const unknownFlags = args.filter(
      (arg) => arg.startsWith("--") && arg !== "--confirm",
    );
    const positional = args.filter((arg) => !arg.startsWith("--"));
    if (unknownFlags.length || positional.length < 1 || positional.length > 3) {
      appendLine(
        "error",
        "Conversation range-delete shortcut needs [id] <from>:<to> or [id] <from> <to> plus --confirm.",
      );
      return null;
    }
    if (!confirmed) {
      appendLine("error", "Conversation range-delete shortcut requires --confirm.");
      return null;
    }

    let id = "";
    let fromText = "";
    let toText = "";
    if (positional.length === 1) {
      const [from, to] = positional[0].split(":");
      id = selectedConversationShortcutId();
      fromText = from || "";
      toText = to || "";
    } else if (positional.length === 2 && positional[1].includes(":")) {
      const [from, to] = positional[1].split(":");
      id = positional[0];
      fromText = from || "";
      toText = to || "";
    } else if (positional.length === 2) {
      id = selectedConversationShortcutId();
      [fromText, toText] = positional;
    } else {
      [id, fromText, toText] = positional;
    }

    if (!id) {
      appendLine(
        "error",
        "Conversation range-delete shortcut needs a conversation id or selected Id.",
      );
      return null;
    }
    const range = parseConversationRangeText(fromText, toText);
    if (!range) return null;
    return { id, range };
  }

  function memoryShortcutHelpText() {
    return [
      "/memory list",
      "/memory access",
      "/memory backends",
      "/memory preview",
      "/memory create <content>",
      "/memory generate <text>",
      "/memory generate-conversation [id] <from>:<to>",
      "/memory classify <id>",
      "/memory edit <id> <content>",
      "/memory delete <id> --confirm",
      "/memory rollback --confirm",
    ].join("\n");
  }

  function parseMemoryConversationRangeShortcut(args: string[]) {
    const unknownFlags = args.filter((arg) => arg.startsWith("--"));
    const positional = args.filter((arg) => !arg.startsWith("--"));
    if (unknownFlags.length || positional.length < 1 || positional.length > 3) {
      appendLine(
        "error",
        "Memory generate-conversation shortcut needs [id] <from>:<to> or [id] <from> <to>.",
      );
      return null;
    }

    let id = "";
    let fromText = "";
    let toText = "";
    if (positional.length === 1) {
      const [from, to] = positional[0].split(":");
      id = selectedConversationShortcutId();
      fromText = from || "";
      toText = to || "";
    } else if (positional.length === 2 && positional[1].includes(":")) {
      const [from, to] = positional[1].split(":");
      id = positional[0];
      fromText = from || "";
      toText = to || "";
    } else if (positional.length === 2) {
      id = selectedConversationShortcutId();
      [fromText, toText] = positional;
    } else {
      [id, fromText, toText] = positional;
    }

    if (!id) {
      appendLine(
        "error",
        "Memory generate-conversation shortcut needs a conversation id or selected Id.",
      );
      return null;
    }
    const range = parseConversationRangeText(fromText, toText);
    if (!range) return null;
    return { id, range };
  }

  function parseIngestReviewShortcut(rest: string) {
    const parts = rest.split(/\s+/).filter(Boolean);
    const [id, findingText, decisionText, ...noteParts] = parts;
    if (!id || !findingText || !decisionText) {
      appendLine(
        "error",
        "Ingest review shortcut needs: /ingest review <id> <finding-index> <approve|acknowledge|reject> [note].",
      );
      return null;
    }
    const finding = Number(findingText);
    if (!Number.isInteger(finding) || finding < 0) {
      appendLine("error", "Ingest review finding index must be a zero-based integer.");
      return null;
    }
    const decision = parseIngestReviewDecision(decisionText);
    if (!decision) {
      appendLine("error", "Ingest review decision must be approve, acknowledge, or reject.");
      return null;
    }
    return {
      id,
      finding,
      decision,
      note: noteParts.join(" ").trim() || null,
    };
  }

  function parseIngestReviewDecision(
    value: string,
  ): IngestionFindingReviewDecision | null {
    const normalized = value.trim().toLowerCase();
    if (
      normalized === "approve" ||
      normalized === "acknowledge" ||
      normalized === "reject"
    ) {
      return normalized;
    }
    if (normalized === "allow") return "approve";
    if (normalized === "block") return "reject";
    return null;
  }

  function parseScoreShortcut(text: string) {
    const rest = text.trim().startsWith("/score ")
      ? text.trim().slice("/score ".length).trim()
      : null;
    if (rest === null) return null;
    const parts = rest.split(/\s+/);
    if (!parts.length) {
      appendLine("error", "Score shortcut needs a number from 0 to 10.");
      return null;
    }
    const firstScore = Number(parts[0]);
    const scoreFirst = Number.isFinite(firstScore);
    const rawScore = scoreFirst ? parts[0] : parts[parts.length - 1];
    const score = Number(rawScore);
    if (!Number.isFinite(score) || score < 0 || score > 10) {
      appendLine("error", "Score shortcut needs a number from 0 to 10.");
      return null;
    }
    const target = scoreFirst ? parts.slice(1).join(" ") : parts.slice(0, -1).join(" ");
    return {
      score,
      target: target.trim() || "last_answer",
    };
  }

  function parseResumeShortcut(text: string) {
    const trimmed = text.trim();
    if (trimmed !== "/resume" && !trimmed.startsWith("/resume ")) {
      return null;
    }
    const parts =
      trimmed === "/resume" ? [] : trimmed.slice("/resume ".length).trim().split(/\s+/);
    let runId = "";
    let fromEvent: number | null = null;

    for (let index = 0; index < parts.length; index += 1) {
      const part = parts[index];
      if (!part) continue;
      if (part === "--from-event") {
        const raw = parts[index + 1];
        const parsed = Number(raw);
        if (!raw || !Number.isInteger(parsed) || parsed < 0) {
          appendLine("error", "Resume shortcut --from-event needs a non-negative event id.");
          return null;
        }
        fromEvent = parsed;
        index += 1;
        continue;
      }
      if (part.startsWith("--from-event=")) {
        const raw = part.slice("--from-event=".length);
        const parsed = Number(raw);
        if (!Number.isInteger(parsed) || parsed < 0) {
          appendLine("error", "Resume shortcut --from-event needs a non-negative event id.");
          return null;
        }
        fromEvent = parsed;
        continue;
      }
      if (!runId) {
        runId = part === "last" ? "" : part;
        continue;
      }
      appendLine("error", "Resume shortcut accepts one run id plus optional --from-event.");
      return null;
    }

    const selectedRunId = runId || opsId.trim() || lastRunId || "";
    if (!selectedRunId) {
      appendLine("error", "Resume shortcut needs a completed run or a run id.");
      return null;
    }
    return { runId: selectedRunId, fromEvent };
  }

  function appendJson(label: string, value: unknown) {
    appendEvent(label);
    appendLine("assistant", JSON.stringify(value, null, 2));
  }

  function promptForPostRunCompaction(runId: string) {
    const snapshot = latestRunContextRef.current;
    if (!snapshot || !isAutoCompactionSnapshot(snapshot)) {
      return;
    }
    setContextPreview(snapshot);
    setContextPreviewPrompt(null);
    setPostRunCompactionPrompt({ runId, snapshot });
    const detail = compactionSavingsLabel(snapshot);
    appendEvent(
      `Auto-compacted context ready to keep${detail ? ` (${detail})` : ""}.`,
    );
  }

  function summarizeTrace(events: RunEvent[]): TraceSummary | null {
    if (!events.length) return null;
    let contextSnapshots = 0;
    let llmCalls = 0;
    let toolCalls = 0;
    let approvals = 0;
    let guidanceInjections = 0;
    let qualityScores = 0;
    let qualityScoreTotal = 0;
    let qualityScoreMin: number | null = null;
    let qualityScoreMax: number | null = null;
    let memoryFragments = 0;
    let artifactRefs = 0;
    let hooks = 0;
    let hookFailures = 0;
    let tokensIn = 0;
    let tokensOut = 0;
    let eventCostUsd = 0;
    let hasEventCost = false;
    let completedCostUsd: number | null = null;
    let durationMs: number | null = null;

    for (const event of events) {
      const kind = event.kind;
      switch (kind.type) {
        case "ContextBuilt":
          contextSnapshots += 1;
          break;
        case "LlmRequestCompleted":
          llmCalls += 1;
          tokensIn += kind.tokens_in;
          tokensOut += kind.tokens_out;
          if (kind.cost_usd !== null) {
            eventCostUsd += kind.cost_usd;
            hasEventCost = true;
          }
          break;
        case "PromptRefinementCompleted":
          tokensIn += kind.tokens_in;
          tokensOut += kind.tokens_out;
          if (kind.cost_usd !== null) {
            eventCostUsd += kind.cost_usd;
            hasEventCost = true;
          }
          break;
        case "ToolCallCompleted":
          toolCalls += 1;
          if (kind.cost_usd !== null) {
            eventCostUsd += kind.cost_usd;
            hasEventCost = true;
          }
          break;
        case "ApprovalRequested":
          approvals += 1;
          break;
        case "GuidanceInjected":
          guidanceInjections += 1;
          break;
        case "QualityScored":
          qualityScores += 1;
          qualityScoreTotal += kind.score;
          qualityScoreMin =
            qualityScoreMin === null
              ? kind.score
              : Math.min(qualityScoreMin, kind.score);
          qualityScoreMax =
            qualityScoreMax === null
              ? kind.score
              : Math.max(qualityScoreMax, kind.score);
          break;
        case "MemoryLoaded":
          memoryFragments += kind.ids.length;
          break;
        case "MemoryRead":
          memoryFragments += kind.fragment_ids.length;
          break;
        case "IngestionReferenced":
          artifactRefs += 1;
          break;
        case "HookFired":
          hooks += 1;
          break;
        case "HookFailed":
          hookFailures += 1;
          break;
        case "RunCompleted":
          completedCostUsd = kind.total_cost_usd;
          durationMs = kind.total_duration_ms;
          break;
      }
    }

    return {
      run_id: events[0].run_id,
      events: events.length,
      context_snapshots: contextSnapshots,
      llm_calls: llmCalls,
      tool_calls: toolCalls,
      approvals,
      guidance_injections: guidanceInjections,
      quality_scores: qualityScores,
      quality_score_average: qualityScores
        ? qualityScoreTotal / qualityScores
        : null,
      quality_score_min: qualityScoreMin,
      quality_score_max: qualityScoreMax,
      memory_fragments: memoryFragments,
      artifact_refs: artifactRefs,
      hooks,
      hook_failures: hookFailures,
      tokens_in: tokensIn,
      tokens_out: tokensOut,
      cost_usd: completedCostUsd ?? (hasEventCost ? eventCostUsd : null),
      duration_ms: durationMs,
    };
  }

  function latestContextSnapshot(events: RunEvent[]) {
    for (let i = events.length - 1; i >= 0; i -= 1) {
      const kind = events[i].kind;
      if (kind.type === "ContextBuilt") {
        return kind.snapshot;
      }
    }
    return null;
  }

  function qualityScoresFromEvents(events: RunEvent[]): QualityScoreRecord[] {
    return events.flatMap((event) => {
      const kind = event.kind;
      if (kind.type !== "QualityScored") return [];
      return [
        {
          event_id: event.id,
          run_id: event.run_id,
          at: event.at,
          target: kind.target,
          score: kind.score,
        },
      ];
    });
  }

  function qualityScoreReport(records: QualityScoreRecord[]) {
    if (!records.length) {
      return "No quality scores recorded in the loaded trace.";
    }
    const average =
      records.reduce((total, record) => total + record.score, 0) / records.length;
    const summary = `Quality scores: ${records.length}, avg ${average.toFixed(1)}/10`;
    const lines = records.map(
      (record) =>
        `#${record.event_id} ${record.target}: ${record.score}/10 (${record.at})`,
    );
    return [summary, ...lines].join("\n");
  }

  function traceTimelineItems(events: RunEvent[]): TraceTimelineItem[] {
    return events.map((event) => {
      const at = new Date(event.at).toLocaleTimeString();
      const kind = event.kind;
      switch (kind.type) {
        case "RunStarted":
          return {
            id: event.id,
            title: "Run started",
            meta: `${at} / ${kind.agent_id}`,
            detail: previewText(kind.input, 180),
            tone: "neutral",
          };
        case "ContextBuilt":
          return {
            id: event.id,
            title: "Context built",
            meta: `${at} / ${kind.snapshot.estimated_input_tokens} tokens`,
            detail: `${kind.snapshot.visible_tools.length} tools, ${kind.snapshot.visible_skills.length} skills, ${kind.snapshot.loaded_memory.length} memory fragments, ${kind.snapshot.loaded_artifacts.length} artifacts.`,
            tone: "ok",
          };
        case "LlmRequestStarted":
          return {
            id: event.id,
            title: "LLM call started",
            meta: `${at} / ${kind.model}`,
            detail: kind.request_digest
              ? `Request digest ${kind.request_digest.slice(0, 16)}...`
              : "Request was sent to the provider.",
            tone: "neutral",
          };
        case "LlmStreamToken":
          return {
            id: event.id,
            title: "LLM stream token",
            meta: at,
            detail: previewText(kind.delta, 180),
            tone: "neutral",
          };
        case "LlmRequestCompleted":
          return {
            id: event.id,
            title: "LLM call completed",
            meta: `${at} / ${formatDuration(kind.duration_ms)}`,
            detail: `Tokens ${kind.tokens_in}/${kind.tokens_out}; cost ${formatCost(kind.cost_usd)}.`,
            tone: "ok",
          };
        case "PromptRefinementStarted":
          return {
            id: event.id,
            title: "Prompt refinement started",
            meta: `${at} / ${kind.model}`,
            detail: previewText(kind.original_input, 180),
            tone: "neutral",
          };
        case "PromptRefinementCompleted":
          return {
            id: event.id,
            title: "Prompt refined",
            meta: `${at} / ${formatDuration(kind.duration_ms)}`,
            detail: previewText(kind.refined_input, 180),
            tone: "ok",
          };
        case "ToolCallProposed":
          return {
            id: event.id,
            title: "Tool proposed",
            meta: `${at} / ${kind.tool_id}${kind.model ? ` / ${kind.model}` : ""}`,
            detail: previewText(compactJson(kind.input), 180),
            tone: "warning",
          };
        case "ToolCallStarted":
          return {
            id: event.id,
            title: "Tool started",
            meta: `${at} / ${kind.call_id}`,
            detail: "The tool execution began.",
            tone: "neutral",
          };
        case "ToolCallCompleted":
          return {
            id: event.id,
            title: "Tool completed",
            meta: `${at} / ${formatDuration(kind.duration_ms)}`,
            detail: previewText(compactJson(kind.output), 180),
            tone: "ok",
          };
        case "ToolOutputInterpreted":
          return {
            id: event.id,
            title: "Tool output interpreted",
            meta: `${at} / ${kind.model}`,
            detail: previewText(kind.summary, 180),
            tone: "ok",
          };
        case "ToolCallFailed":
          return {
            id: event.id,
            title: "Tool failed",
            meta: `${at} / ${kind.call_id}`,
            detail: kind.error,
            tone: "danger",
          };
        case "ApprovalRequested":
          return {
            id: event.id,
            title: "Approval requested",
            meta: `${at} / ${kind.action}`,
            detail: kind.reason,
            tone: "warning",
          };
        case "ApprovalResolved":
          return {
            id: event.id,
            title: kind.approved ? "Approval granted" : "Approval rejected",
            meta: `${at} / ${kind.approval_id}`,
            detail: kind.approved
              ? "The gated action was approved."
              : "The gated action was rejected.",
            tone: kind.approved ? "ok" : "danger",
          };
        case "ApprovalControllerAssessed":
          return {
            id: event.id,
            title: "Approval assessed",
            meta: `${at} / ${kind.controller_agent} / ${kind.recommendation}`,
            detail: previewText(kind.summary, 180),
            tone:
              kind.recommendation === "approve"
                ? "ok"
                : kind.recommendation === "reject"
                  ? "danger"
                  : "warning",
          };
        case "GuidanceInjected":
          return {
            id: event.id,
            title: "Guidance injected",
            meta: at,
            detail: previewText(kind.content, 180),
            tone: "warning",
          };
        case "QualityScored":
          return {
            id: event.id,
            title: "Quality scored",
            meta: `${at} / ${kind.score}/10`,
            detail: kind.target,
            tone: "ok",
          };
        case "MemoryLoaded":
          return {
            id: event.id,
            title: "Memory loaded",
            meta: `${at} / ${kind.ids.length} records`,
            detail: kind.ids.join(", ") || "No memory ids.",
            tone: "ok",
          };
        case "MemoryRead":
          return {
            id: event.id,
            title: "Memory read",
            meta: `${at} / ${kind.backend}`,
            detail: kind.fragment_ids.join(", ") || "No fragments returned.",
            tone: "ok",
          };
        case "MemoryWritten":
          return {
            id: event.id,
            title: "Memory written",
            meta: `${at} / ${kind.operation}`,
            detail: [kind.id, kind.source_range, kind.generating_model]
              .filter(Boolean)
              .join(" / "),
            tone: "ok",
          };
        case "IngestionReferenced":
          return {
            id: event.id,
            title: "Ingestion referenced",
            meta: `${at} / ${kind.artifact_id}`,
            detail: kind.source,
            tone: "ok",
          };
        case "IngestionStarted":
          return {
            id: event.id,
            title: "Ingestion started",
            meta: `${at} / ${kind.backend}`,
            detail: kind.source,
            tone: "neutral",
          };
        case "IngestionCompleted":
          const ingestionFindings = kind.findings?.length
            ? `${kind.findings.join("; ")}`
            : "no findings";
          const ingestionSnippet = kind.finding_snippets?.[0]
            ? ` / ${kind.finding_snippets[0]}`
            : "";
          return {
            id: event.id,
            title: "Ingestion completed",
            meta: `${at} / ${kind.sections} sections / ${kind.high_risk_findings ?? 0} high-risk`,
            detail: `${kind.artifact_id}; ${kind.content_hash.slice(0, 16)}...; ${ingestionFindings}${ingestionSnippet}`,
            tone: kind.high_risk_findings ? "warning" : "ok",
          };
        case "HookFired":
          return {
            id: event.id,
            title: "Hook fired",
            meta: `${at} / ${kind.hook_id}`,
            detail: `${kind.trigger}; digest ${kind.payload_digest.slice(0, 16)}...`,
            tone: "neutral",
          };
        case "HookFailed":
          return {
            id: event.id,
            title: "Hook failed",
            meta: `${at} / ${kind.hook_id} / attempt ${kind.attempt}`,
            detail: `${kind.trigger}; retry=${kind.will_retry}; ${kind.error}`,
            tone: kind.will_retry ? "warning" : "danger",
          };
        case "PolicyDenied":
          return {
            id: event.id,
            title: "Policy denied",
            meta: at,
            detail: kind.reason,
            tone: "danger",
          };
        case "ChildRunStarted":
          return {
            id: event.id,
            title: "Child run started",
            meta: `${at} / ${kind.agent_id}`,
            detail: kind.child_run_id,
            tone: "neutral",
          };
        case "ChildRunCompleted":
          return {
            id: event.id,
            title: "Child run completed",
            meta: `${at} / ${kind.status}`,
            detail: kind.child_run_id,
            tone: kind.status === "completed" ? "ok" : "warning",
          };
        case "BatchRunStarted":
          return {
            id: event.id,
            title: "Batch started",
            meta: `${at} / ${kind.items} items`,
            detail: kind.batch_id,
            tone: "neutral",
          };
        case "BatchItemStatus":
          return {
            id: event.id,
            title: "Batch item updated",
            meta: `${at} / ${kind.status}`,
            detail: `${kind.batch_id}: ${kind.item_key}`,
            tone: kind.status === "failed" ? "danger" : "ok",
          };
        case "BatchRunCompleted":
          return {
            id: event.id,
            title: "Batch completed",
            meta: `${at} / ${kind.batch_id}`,
            detail: `${kind.succeeded} succeeded, ${kind.failed} failed.`,
            tone: kind.failed ? "warning" : "ok",
          };
        case "RunPaused":
          return {
            id: event.id,
            title: "Run paused",
            meta: at,
            detail: kind.reason,
            tone: "warning",
          };
        case "RunCancelled":
          return {
            id: event.id,
            title: "Run cancelled",
            meta: at,
            detail: kind.reason,
            tone: "danger",
          };
        case "RunCompleted":
          return {
            id: event.id,
            title: "Run completed",
            meta: `${at} / ${formatDuration(kind.total_duration_ms)}`,
            detail: `${formatCost(kind.total_cost_usd)}; ${previewText(kind.final_output, 180)}`,
            tone: "ok",
          };
        case "RunFailed":
          return {
            id: event.id,
            title: "Run failed",
            meta: at,
            detail: kind.reason,
            tone: "danger",
          };
      }
    });
  }

  async function fetchTraceEvents(runId: string) {
    return transport === "daemon"
      ? await daemonJson<RunEvent[]>(`/trace/${runId}`)
      : await invoke<RunEvent[]>("trace_show", { runId });
  }

  async function fetchTraceTree(runId: string) {
    return transport === "daemon"
      ? await daemonJson<TraceTreeNode>(`/trace/${runId}/tree`)
      : await invoke<TraceTreeNode>("trace_tree", { runId });
  }

  function applyTraceEvents(events: RunEvent[]) {
    const summary = summarizeTrace(events);
    const latestContext = latestContextSnapshot(events);
    setTraceEvents(events);
    setTraceSummary(summary);
    if (latestContext) {
      setContextPreview(latestContext);
      setContextPreviewPrompt(null);
    }
    const runId = events[0]?.run_id ?? "unknown";
    appendEvent(`Loaded trace ${runId} (${events.length} events)`);
    if (summary) {
      appendEvent(
        `Trace summary: ${summary.context_snapshots} contexts, ${summary.llm_calls} LLM calls, ${summary.tool_calls} tools, tokens ${summary.tokens_in}/${summary.tokens_out}`,
      );
    }
    void refreshHookPolicy(false);
    return summary;
  }

  function traceTreeNodeCount(node: TraceTreeNode | null): number {
    if (!node) return 0;
    return 1 + (node.children ?? []).reduce((sum, child) => sum + traceTreeNodeCount(child), 0);
  }

  function traceTreeChildCount(node: TraceTreeNode | null): number {
    if (!node) return 0;
    return (
      (node.children ?? []).length +
      (node.children ?? []).reduce((sum, child) => sum + traceTreeChildCount(child), 0)
    );
  }

  function traceTreeLeafCount(node: TraceTreeNode | null): number {
    if (!node) return 0;
    const children = node.children ?? [];
    if (!children.length) return 1;
    return children.reduce((sum, child) => sum + traceTreeLeafCount(child), 0);
  }

  function traceTreeMaxDepth(node: TraceTreeNode | null): number {
    if (!node) return 0;
    const children = node.children ?? [];
    if (!children.length) return 1;
    return 1 + Math.max(...children.map(traceTreeMaxDepth));
  }

  function formatTraceNumber(value: number | null | undefined, suffix = "") {
    if (value == null) return "n/a";
    return `${value}${suffix}`;
  }

  function formatTraceCost(value: number | null | undefined) {
    if (value == null) return "n/a";
    return `$${value.toFixed(6)}`;
  }

  function formatTraceAverage(value: number | null | undefined) {
    if (value == null) return "n/a";
    return value.toFixed(1);
  }

  function traceNumberDelta(
    compare: number | null | undefined,
    primary: number | null | undefined,
    suffix = "",
    fixed?: number,
  ) {
    if (compare == null || primary == null) return "n/a";
    const delta = compare - primary;
    const formatted =
      fixed == null ? String(delta) : Math.abs(delta).toFixed(fixed);
    const value = fixed == null ? formatted : `${delta < 0 ? "-" : ""}${formatted}`;
    return `${delta > 0 ? "+" : ""}${value}${suffix}`;
  }

  function traceComparisonRows(
    primary: TraceSummary,
    compare: TraceSummary,
    primaryTree: TraceTreeNode | null,
    compareTree: TraceTreeNode | null,
  ): TraceComparisonRow[] {
    return [
      {
        label: "Events",
        primary: formatTraceNumber(primary.events),
        compare: formatTraceNumber(compare.events),
        delta: traceNumberDelta(compare.events, primary.events),
      },
      {
        label: "Run Tree",
        primary: `${traceTreeNodeCount(primaryTree)} runs / depth ${traceTreeMaxDepth(primaryTree)}`,
        compare: `${traceTreeNodeCount(compareTree)} runs / depth ${traceTreeMaxDepth(compareTree)}`,
        delta: `${traceNumberDelta(
          traceTreeNodeCount(compareTree),
          traceTreeNodeCount(primaryTree),
        )} runs / ${traceNumberDelta(
          traceTreeMaxDepth(compareTree),
          traceTreeMaxDepth(primaryTree),
        )} depth`,
      },
      {
        label: "Contexts",
        primary: formatTraceNumber(primary.context_snapshots),
        compare: formatTraceNumber(compare.context_snapshots),
        delta: traceNumberDelta(
          compare.context_snapshots,
          primary.context_snapshots,
        ),
      },
      {
        label: "LLM Calls",
        primary: formatTraceNumber(primary.llm_calls),
        compare: formatTraceNumber(compare.llm_calls),
        delta: traceNumberDelta(compare.llm_calls, primary.llm_calls),
      },
      {
        label: "Tool Calls",
        primary: formatTraceNumber(primary.tool_calls),
        compare: formatTraceNumber(compare.tool_calls),
        delta: traceNumberDelta(compare.tool_calls, primary.tool_calls),
      },
      {
        label: "Input Tokens",
        primary: formatTraceNumber(primary.tokens_in),
        compare: formatTraceNumber(compare.tokens_in),
        delta: traceNumberDelta(compare.tokens_in, primary.tokens_in),
      },
      {
        label: "Output Tokens",
        primary: formatTraceNumber(primary.tokens_out),
        compare: formatTraceNumber(compare.tokens_out),
        delta: traceNumberDelta(compare.tokens_out, primary.tokens_out),
      },
      {
        label: "Cost",
        primary: formatTraceCost(primary.cost_usd),
        compare: formatTraceCost(compare.cost_usd),
        delta: traceNumberDelta(compare.cost_usd, primary.cost_usd, "", 6),
      },
      {
        label: "Duration",
        primary: formatTraceNumber(primary.duration_ms, "ms"),
        compare: formatTraceNumber(compare.duration_ms, "ms"),
        delta: traceNumberDelta(compare.duration_ms, primary.duration_ms, "ms"),
      },
      {
        label: "Approvals",
        primary: formatTraceNumber(primary.approvals),
        compare: formatTraceNumber(compare.approvals),
        delta: traceNumberDelta(compare.approvals, primary.approvals),
      },
      {
        label: "Quality Avg",
        primary: formatTraceAverage(primary.quality_score_average),
        compare: formatTraceAverage(compare.quality_score_average),
        delta: traceNumberDelta(
          compare.quality_score_average,
          primary.quality_score_average,
          "",
          1,
        ),
      },
      {
        label: "Hook Failures",
        primary: formatTraceNumber(primary.hook_failures),
        compare: formatTraceNumber(compare.hook_failures),
        delta: traceNumberDelta(compare.hook_failures, primary.hook_failures),
      },
    ];
  }

  function expandableTraceTreeRunIds(node: TraceTreeNode | null): string[] {
    if (!node) return [];
    const children = node.children ?? [];
    return [
      ...(children.length ? [node.run_id] : []),
      ...children.flatMap(expandableTraceTreeRunIds),
    ];
  }

  function toggleTraceTreeNode(runId: string) {
    setCollapsedTraceTreeRuns((ids) =>
      ids.includes(runId)
        ? ids.filter((id) => id !== runId)
        : [...ids, runId],
    );
  }

  function traceTreeNodeTone(node: TraceTreeNode) {
    if (!node.trace_available) return "warning";
    if (node.status === "completed" || node.status === "succeeded") return "ok";
    if (node.status === "failed" || node.status === "cancelled") return "danger";
    if (node.status === "paused" || node.status === "cycle") return "warning";
    return "neutral";
  }

  function traceTreeNodeKind(node: TraceTreeNode) {
    if (node.agent_id?.startsWith("external-agent:")) return "external agent";
    if (node.link_event_id != null) return "child run";
    return "root run";
  }

  function traceTreeNodeAvailability(node: TraceTreeNode) {
    if (node.trace_available) return `${node.event_count} events`;
    if (node.agent_id?.startsWith("external-agent:")) return "remote trace link";
    return "linked trace only";
  }

  function renderTraceTreeNode(node: TraceTreeNode, depth = 0) {
    const children = node.children ?? [];
    const hasChildren = children.length > 0;
    const collapsed = collapsedTraceTreeRuns.includes(node.run_id);
    const agent = node.agent_id || "unknown";
    const descendantCount = Math.max(traceTreeNodeCount(node) - 1, 0);
    const leafCount = traceTreeLeafCount(node);
    const link =
      node.link_status && node.link_status !== node.status ? ` / link ${node.link_status}` : "";
    const depthStyle = {
      "--tree-depth": String(Math.min(depth, 7)),
    } as CSSProperties;
    return (
      <div
        className={`trace-tree-node ${traceTreeNodeTone(node)} ${
          depth > 0 ? "nested" : "root"
        } ${collapsed ? "collapsed" : ""}`}
        key={`${node.run_id}:${node.link_event_id ?? "root"}`}
        style={depthStyle}
      >
        <div className="trace-tree-node-main">
          <button
            type="button"
            className="trace-tree-toggle"
            title={hasChildren ? (collapsed ? "Expand branch" : "Collapse branch") : "Leaf run"}
            aria-label={hasChildren ? (collapsed ? "Expand branch" : "Collapse branch") : "Leaf run"}
            aria-expanded={hasChildren ? !collapsed : undefined}
            onClick={() => toggleTraceTreeNode(node.run_id)}
            disabled={running || !hasChildren}
          >
            {hasChildren ? (collapsed ? "+" : "-") : ""}
          </button>
          <div className="trace-tree-title">
            <strong>{agent}</strong>
            <code>{node.run_id}</code>
          </div>
          <span className="trace-tree-status">
            {node.status}
            {link}
          </span>
        </div>
        <div className="trace-tree-node-meta">
          <span>{traceTreeNodeKind(node)}</span>
          <span>{traceTreeNodeAvailability(node)}</span>
          {node.link_event_id != null ? <span>link event {node.link_event_id}</span> : null}
          {descendantCount ? <span>{descendantCount} descendant run(s)</span> : null}
          {children.length ? <span>{leafCount} leaf run(s)</span> : null}
        </div>
        <div className="mini-actions trace-tree-actions">
          <button
            type="button"
            title="Move this run id into the Id field."
            onClick={() => setOpsId(node.run_id)}
            disabled={running}
          >
            Set Id
          </button>
          <button
            type="button"
            title="Load this run's trace and subtree."
            onClick={() => void loadTraceTreeNode(node.run_id)}
            disabled={running || !node.trace_available}
          >
            Load
          </button>
        </div>
        {children.length && !collapsed ? (
          <div className="trace-tree-children">
            {children.map((child) => renderTraceTreeNode(child, depth + 1))}
          </div>
        ) : null}
      </div>
    );
  }

  function traceOriginalPrompt(events: RunEvent[]) {
    for (const event of events) {
      if (event.kind.type === "RunStarted") {
        return event.kind.input;
      }
    }
    return "";
  }

  function hookRemediationsFromEvents(events: RunEvent[]): HookRemediationRecord[] {
    const denialsByParent = new Map<number, string[]>();
    for (const event of events) {
      if (event.kind.type === "PolicyDenied" && event.parent_event !== null) {
        const denials = denialsByParent.get(event.parent_event) ?? [];
        denials.push(event.kind.reason);
        denialsByParent.set(event.parent_event, denials);
      }
    }
    return events
      .filter((event) => event.kind.type === "HookFailed")
      .map((event) => {
        const kind = event.kind as Extract<RunEvent["kind"], { type: "HookFailed" }>;
        const finalFailure = !kind.will_retry;
        return {
          event_id: event.id,
          hook_id: kind.hook_id,
          trigger: kind.trigger,
          error: kind.error,
          attempt: kind.attempt,
          will_retry: kind.will_retry,
          final_failure: finalFailure,
          policy_denials:
            event.parent_event === null
              ? []
              : denialsByParent.get(event.parent_event) ?? [],
          suggested_actions: finalFailure
            ? [
                `Fix or disable hook ${kind.hook_id}, then replay the run.`,
                "Replay with hooks skipped once only after accepting the override.",
              ]
            : ["Wait for the configured hook retry and review the final attempt."],
        };
      });
  }

  function loadTracePromptToComposer() {
    const prompt = traceOriginalPrompt(traceEvents);
    if (!prompt) {
      appendLine("error", "Load prompt needs a trace with a RunStarted event.");
      return;
    }
    setInput(prompt);
    setActiveSection("chat");
    appendEvent("Loaded original trace prompt into the composer.");
  }

  function clearLoadedTrace() {
    setTraceEvents([]);
    setTraceSummary(null);
    setTraceTree(null);
    setCollapsedTraceTreeRuns([]);
    clearTraceComparison();
  }

  function clearTraceComparison() {
    setTraceCompareSummary(null);
    setTraceCompareTree(null);
    setTraceCompareRunId("");
  }

  async function replayTracePrompt() {
    await replayTracePromptWithOptions();
  }

  async function replayTracePromptWithoutHooks() {
    await replayTracePromptWithOptions({ skipHooks: true });
  }

  async function replayTracePromptWithOptions(options?: {
    runId?: string;
    skipHooks?: boolean;
  }) {
    let events = traceEvents;
    const runId = options?.runId?.trim();
    if (runId) {
      try {
        const loaded = await loadTraceFor(runId);
        events = loaded.events;
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        appendLine("error", `Trace load failed: ${msg}`);
        return;
      }
    }
    const prompt = traceOriginalPrompt(events);
    if (!prompt) {
      appendLine(
        "error",
        options?.skipHooks
          ? "Hook override needs a trace with a RunStarted event."
          : "Replay needs a trace with a RunStarted event.",
      );
      return;
    }
    const label = runId
      ? `replay trace ${runId} prompt${options?.skipHooks ? " (hooks skipped once)" : ""}`
      : `replay trace prompt${options?.skipHooks ? " (hooks skipped once)" : ""}`;
    await runAgentPrompt(
      prompt,
      label,
      options?.skipHooks ? { disable_lifecycle_hooks: true } : undefined,
    );
  }

  async function refreshHookPolicy(announce = true) {
    try {
      const agent = agentId.trim() || null;
      const policy =
        transport === "daemon"
          ? await daemonJson<HookPolicyRecord>("/hooks/policy", { agent_id: agent })
          : await invoke<HookPolicyRecord>("hook_policy", { agentId: agent });
      setHookPolicy(policy);
      if (announce) {
        appendEvent(
          `Loaded hook policy (${policy.disabled_lifecycle_hooks.length} disabled).`,
        );
      }
      return policy;
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      appendLine("error", `Hook policy load failed: ${msg}`);
      return null;
    }
  }

  async function refreshHookCatalog() {
    try {
      const agent = agentId.trim() || null;
      const catalog =
        transport === "daemon"
          ? await daemonJson<HookCatalogResponse>("/hooks/available", {
              agent_id: agent,
            })
          : await invoke<HookCatalogResponse>("hook_available", { agentId: agent });
      setHookCatalog(catalog.hooks);
      appendEvent(`Loaded lifecycle hooks (${catalog.hooks.length}).`);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      appendLine("error", `Hook catalog load failed: ${msg}`);
    }
  }

  async function reviewHooksFromOps(explicitRunId?: string) {
    const runId =
      explicitRunId?.trim() || traceEvents[0]?.run_id || lastRunId || "";
    if (!runId) {
      appendLine("error", "Hook review shortcut needs a run id or previous run.");
      return;
    }
    try {
      const events = await fetchTraceEvents(runId);
      const plan = hookRemediationsFromEvents(events);
      appendEvent(`Hook review: ${plan.length} issue(s) for ${runId}.`);
      appendJson("Hook review", plan);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      appendLine("error", `Hook review failed: ${msg}`);
    }
  }

  async function setPersistentHookDisabled(
    hookId: string,
    disabled: boolean,
    scope: "profile" | "agent" = "profile",
    confirmed = false,
  ) {
    const action = disabled ? "Disable lifecycle hook" : "Enable lifecycle hook";
    const agent = agentId.trim() || "fake-agent";
    if (!confirmed && !confirmLocalChange(`${action} ${hookId}`)) {
      return;
    }
    try {
      const policy =
        transport === "daemon"
          ? await daemonJson<HookPolicyRecord>("/hooks/policy/set", {
              hook_id: hookId,
              disabled,
              agent_id: agent,
              scope,
            })
          : await invoke<HookPolicyRecord>("set_hook_disabled", {
              hookId,
              disabled,
              agentId: agent,
              scope,
            });
      setHookPolicy(policy);
      setHookCatalog((hooks) =>
        hooks.map((hook) => {
          const isDisabled = policy.disabled_lifecycle_hooks.includes(hook.id);
          return {
            ...hook,
            disabled: isDisabled,
            disabled_source: isDisabled ? (policy.effective_source ?? "policy") : null,
          };
        }),
      );
      appendEvent(
        `${disabled ? "Disabled" : "Enabled"} hook ${hookId} for future runs in ${scope === "agent" ? `agent ${policy.agent_id ?? agent}` : `profile ${policy.profile}`}.`,
      );
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      appendLine("error", `Hook policy update failed: ${msg}`);
    }
  }

  function hookIsPersistentlyDisabled(hookId: string) {
    return hookPolicy?.disabled_lifecycle_hooks.includes(hookId) ?? false;
  }

  function hookIsProfileDisabled(hookId: string) {
    return hookPolicy?.profile_disabled_lifecycle_hooks?.includes(hookId) ?? false;
  }

  function hookIsAgentDisabled(hookId: string) {
    return hookPolicy?.agent_disabled_lifecycle_hooks?.includes(hookId) ?? false;
  }

  function hookIsGlobalDisabled(hookId: string) {
    return hookPolicy?.global_disabled_lifecycle_hooks?.includes(hookId) ?? false;
  }

  function hookPolicyScopeSummary(hookId: string) {
    const scopes = [
      hookIsGlobalDisabled(hookId) ? "global" : null,
      hookIsProfileDisabled(hookId) ? "profile" : null,
      hookIsAgentDisabled(hookId) ? "agent" : null,
    ].filter(Boolean);
    return scopes.length ? ` - scopes ${scopes.join("/")}` : "";
  }

  function hookPolicyConflictNote(hookId: string) {
    if (!hookPolicy || hookPolicy.effective_source !== "agent") return "";
    const agentDisabled = hookIsAgentDisabled(hookId);
    if (!agentDisabled && hookIsProfileDisabled(hookId)) {
      return "Profile disable is shadowed by the agent policy.";
    }
    if (!agentDisabled && hookIsGlobalDisabled(hookId)) {
      return "Global disable is shadowed by the agent policy.";
    }
    return "";
  }

  function confirmLocalChange(action: string) {
    const confirmed = window.confirm(`${action}? This changes local harness data.`);
    if (!confirmed) {
      appendEvent(`${action} cancelled.`);
    }
    return confirmed;
  }

  function promptScopeAgentId() {
    return agentId.trim() || null;
  }

  function promptScopeLabel(agent = promptScopeAgentId()) {
    return agent ? `agent ${agent}` : "profile";
  }

  async function fetchPrompt(name: string, agent = promptScopeAgentId()) {
    if (transport === "daemon") {
      if (agent) {
        return await daemonJson<PromptDoc>("/prompts/show", {
          name,
          agent_id: agent,
        });
      }
      return await daemonJson<PromptDoc>(`/prompts/${encodeURIComponent(name)}`);
    }
    return await invoke<PromptDoc>("prompt_show", { name, agentId: agent });
  }

  async function loadPromptBody(name: string) {
    const agent = promptScopeAgentId();
    let prompt: PromptDoc;
    try {
      prompt = await fetchPrompt(name, agent);
    } catch (err: unknown) {
      if (!agent) {
        throw err;
      }
      prompt = await fetchPrompt(name, null);
    }
    return prompt.body;
  }

  async function loadTraceFor(runId: string) {
    const [events, tree] = await Promise.all([
      fetchTraceEvents(runId),
      fetchTraceTree(runId),
    ]);
    setTraceTree(tree);
    setCollapsedTraceTreeRuns([]);
    appendEvent(
      `Trace tree: ${traceTreeNodeCount(tree)} run(s), ${traceTreeChildCount(tree)} child link(s)`,
    );
    const summary = applyTraceEvents(events);
    return { events, summary };
  }

  async function loadTraceById(runId: string) {
    try {
      await loadTraceFor(runId);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Trace load failed: ${msg}`);
    }
  }

  async function loadTraceComparison(runIdInput?: string) {
    const runId = runIdInput?.trim() || traceCompareRunId.trim() || opsId.trim();
    if (!runId) {
      appendLine("error", "Compare trace needs a run id.");
      return;
    }
    if (traceSummary?.run_id === runId) {
      appendLine("error", "Compare trace needs a different run id.");
      return;
    }
    try {
      const [events, tree] = await Promise.all([
        fetchTraceEvents(runId),
        fetchTraceTree(runId),
      ]);
      const summary = summarizeTrace(events);
      if (!summary) {
        appendLine("error", `Compare trace ${runId} has no events.`);
        return;
      }
      setTraceCompareRunId(runId);
      setTraceCompareSummary(summary);
      setTraceCompareTree(tree);
      appendEvent(
        `Loaded compare trace ${runId} (${events.length} events, ${traceTreeNodeCount(tree)} run(s))`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Compare trace load failed: ${msg}`);
    }
  }

  async function loadTraceComparisonFromOps() {
    await loadTraceComparison();
  }

  async function loadTraceTreeNode(runId: string) {
    setOpsId(runId);
    await loadTraceById(runId);
  }

  async function submit() {
    let prompt = input.trim();
    if (!prompt) return;

    if (prompt.startsWith("/guide ")) {
      const guidance = prompt.slice("/guide ".length).trim();
      if (!activeGuidanceRunId()) {
        appendLine("error", "Guide shortcut needs an active run.");
        return;
      }
      if (!guidance) {
        appendLine("error", "Guide shortcut needs guidance text.");
        return;
      }
      setInput("");
      await guideLastRun(guidance, false);
      return;
    }
    if (prompt === "/guide") {
      appendLine("error", "Guide shortcut needs guidance text.");
      return;
    }

    if (prompt === "/help") {
      setInput("");
      appendLine("user", "/help");
      appendLine("assistant", slashCommandHelpText());
      return;
    }

    if (prompt === "/usage" || prompt.startsWith("/usage ")) {
      const scope = prompt === "/usage" ? "current" : prompt.slice("/usage ".length).trim();
      const usageArgs = scope.split(/\s+/).filter(Boolean);
      const usageCommand = usageArgs[0] ?? "current";
      if (usageCommand === "current" || usageCommand === "status") {
        setInput("");
        appendLine("user", prompt);
        if (usageArgs.length > 1) {
          appendLine("error", "Usage current shortcut does not accept a run id.");
          return;
        }
        appendEvent(currentUsageSummary());
        return;
      }
      if (usageCommand === "trace" || usageCommand === "last" || usageCommand === "run") {
        setInput("");
        appendLine("user", prompt);
        if (usageArgs.length > 2) {
          appendLine("error", "Usage trace shortcut accepts at most one run id.");
          return;
        }
        const requestedRunId = usageArgs[1] === "last" ? "" : usageArgs[1];
        const runId = requestedRunId || lastRunId;
        if (!runId) {
          appendLine("error", "Usage trace shortcut needs a completed or active run.");
          return;
        }
        if (running) {
          appendLine("error", "Usage trace shortcut is available after the run settles.");
          return;
        }
        try {
          if (requestedRunId) {
            setOpsId(requestedRunId);
          }
          const { summary } = await loadTraceFor(runId);
          if (summary) {
            appendEvent(traceUsageSummary(summary));
          }
        } catch (err: unknown) {
          const msg = err instanceof Error ? err.message : String(err);
          appendLine("error", `Usage trace failed: ${msg}`);
        }
        return;
      }
      appendLine("error", "Usage shortcut needs current, trace, or run.");
      return;
    }

    if (prompt === "/stop" || prompt.startsWith("/stop ")) {
      const value = prompt === "/stop" ? "now" : prompt.slice("/stop ".length).trim().toLowerCase();
      if (value === "status") {
        setInput("");
        appendLine("user", prompt);
        appendEvent(`Stop mode is ${stopRetentionLabel(stopRetentionMode)}.`);
        return;
      }
      const requestedMode = parseStopRetentionMode(value);
      if (requestedMode !== null) {
        setInput("");
        const nextMode = requestedMode === "default" ? null : requestedMode;
        setStopRetentionMode(nextMode);
        appendLine("user", prompt);
        appendEvent(`Stop mode set to ${stopRetentionLabel(nextMode)}.`);
        if (running && lastRunId) {
          await cancelLastRun(nextMode);
        }
        return;
      }
      if (value === "now") {
        setInput("");
        appendLine("user", prompt);
        if (!running || !lastRunId) {
          appendLine("error", "Stop shortcut needs an active run.");
          return;
        }
        await cancelLastRun();
        return;
      }
      appendLine("error", "Stop shortcut needs default, discard, summarise, status, or an active run.");
      return;
    }

    if (prompt === "/compact" || prompt.startsWith("/compact ")) {
      const guidance = prompt === "/compact" ? "" : prompt.slice("/compact ".length).trim();
      const command = guidance.toLowerCase();
      if (command === "clear") {
        setInput("");
        setManualCompactedContext("");
        appendLine("user", prompt);
        appendEvent("Manual compacted context cleared.");
        return;
      }
      if (command === "status") {
        setInput("");
        appendLine("user", prompt);
        appendEvent(compactionStatus());
        return;
      }
      const draft = buildCompactionDraft(guidance);
      setInput("");
      setManualCompactedContext(draft);
      setOpsValue(draft);
      appendLine("user", prompt);
      appendEvent(
        `Manual compacted context set (~${estimateLocalTokens(draft)} tokens) and sent to Value.`,
      );
      return;
    }

    if (prompt === "/guardrails" || prompt.startsWith("/guardrails ")) {
      const value =
        prompt === "/guardrails"
          ? "review"
          : prompt.slice("/guardrails ".length).trim().toLowerCase();
      setInput("");
      appendLine("user", prompt);
      if (value === "review" || value === "status") {
        setActiveSection("ingest");
        appendLine("assistant", guardrailReport());
        return;
      }
      if (value === "unsafe on") {
        setAllowUnsafeIngest(true);
        appendEvent("Unsafe ingest override enabled for this run.");
        return;
      }
      if (value === "unsafe off") {
        setAllowUnsafeIngest(false);
        appendEvent("Unsafe ingest override disabled.");
        return;
      }
      appendLine("error", "Guardrails shortcut needs review, status, unsafe on, or unsafe off.");
      return;
    }

    if (running) return;

    if (prompt === "/scores") {
      setInput("");
      appendLine("user", "/scores");
      try {
        let events = traceEvents;
        if (!events.length && lastRunId) {
          events = await fetchTraceEvents(lastRunId);
          applyTraceEvents(events);
        }
        if (!events.length) {
          appendLine("error", "Scores shortcut needs a loaded trace or completed run.");
          return;
        }
        appendLine("assistant", qualityScoreReport(qualityScoresFromEvents(events)));
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        appendLine("error", `Scores review failed: ${msg}`);
      }
      return;
    }

    if (prompt === "/raw") {
      setInput("");
      setRawToolOutput(true);
      appendLine("user", "/raw");
      appendEvent("Output mode set to raw tool results.");
      return;
    }

    if (
      prompt === "/interpret" ||
      prompt === "/interpreted" ||
      prompt.startsWith("/interpret ")
    ) {
      setInput("");
      setRawToolOutput(false);
      appendLine("user", prompt);
      const interpreterModel = prompt.startsWith("/interpret ")
        ? prompt.slice("/interpret ".length).trim()
        : "";
      if (interpreterModel === "clear") {
        setToolOutputInterpretationModel("");
        appendEvent("Interpreter model cleared.");
      } else if (interpreterModel === "status") {
        appendEvent(
          `Interpreter model: ${toolOutputInterpretationModel.trim() || "configured default"}.`,
        );
      } else if (interpreterModel) {
        setToolOutputInterpretationModel(interpreterModel);
        appendEvent(`Interpreter model set to ${interpreterModel}.`);
      } else {
        appendEvent("Output mode set to interpreted tool results.");
      }
      return;
    }

    if (prompt === "/router-model" || prompt.startsWith("/router-model ")) {
      setInput("");
      appendLine("user", prompt);
      const routingModel = prompt.startsWith("/router-model ")
        ? prompt.slice("/router-model ".length).trim()
        : "";
      if (routingModel === "clear") {
        setToolRoutingModel("");
        appendEvent("Routing model cleared.");
      } else if (routingModel === "status" || !routingModel) {
        appendEvent(
          `Routing model: ${toolRoutingModel.trim() || "configured default"}.`,
        );
      } else {
        setToolRoutingModel(routingModel);
        appendEvent(`Routing model set to ${routingModel}.`);
      }
      return;
    }

    if (prompt === "/answer") {
      setInput("");
      setAgentMode("answer");
      appendLine("user", "/answer");
      appendEvent("Agent mode set to answer only (0 tool calls).");
      return;
    }

    if (prompt === "/action") {
      setInput("");
      setAgentMode("action");
      appendLine("user", "/action");
      appendEvent("Agent mode set to one action (1 tool call).");
      return;
    }

    if (prompt === "/workflow") {
      setInput("");
      setAgentMode("workflow");
      appendLine("user", "/workflow");
      appendEvent(`Agent mode set to workflow (${CALLS_MAX} tool calls).`);
      return;
    }

    if (prompt === "/simple" || prompt === "/router") {
      const routerMode = prompt === "/router";
      setInput("");
      setAgentMode(routerMode ? "action" : "answer");
      setRawToolOutput(true);
      setEnableShell(false);
      setEnableSubagent(false);
      setLoadMemory(false);
      setLoadSkills(false);
      setIncludeIngestIds([]);
      setAllowUnsafeIngest(false);
      setEnablePromptRefinement(false);
      appendLine("user", prompt);
      appendEvent(
        routerMode
          ? "Preset applied: one-action raw router, with memory and subagents off."
          : "Preset applied: simple raw answer, with tools, memory, and subagents off.",
      );
      return;
    }

    if (prompt === "/budget") {
      appendLine("error", "Budget shortcut needs a non-negative tool-call limit.");
      return;
    }
    if (prompt.startsWith("/budget ")) {
      const value = prompt.slice("/budget ".length).trim();
      const parsedBudget = parseOptionalNonNegativeInt(value);
      if (parsedBudget === null) {
        appendLine("error", "Budget shortcut needs a non-negative integer.");
        return;
      }
      setInput("");
      setMaxToolCalls(String(parsedBudget));
      appendLine("user", `/budget ${parsedBudget}`);
      appendEvent(`Tool-call budget set to ${parsedBudget}.`);
      return;
    }

    if (prompt === "/visibility") {
      appendLine(
        "error",
        "Visibility shortcut needs full, descriptions, names, or config.",
      );
      return;
    }
    if (prompt.startsWith("/visibility ")) {
      const value = prompt.slice("/visibility ".length).trim().toLowerCase();
      const visibility =
        value === "full" || value === "full_schema"
          ? "full_schema"
          : value === "descriptions" || value === "name_and_description"
            ? "name_and_description"
            : value === "names" || value === "name_only"
              ? "name_only"
              : value === "config"
                ? ""
                : null;
      if (visibility === null) {
        appendLine(
          "error",
          "Visibility shortcut needs full, descriptions, names, or config.",
        );
        return;
      }
      setInput("");
      setToolVisibility(visibility as ToolVisibility | "");
      appendLine("user", `/visibility ${value}`);
      appendEvent(
        visibility
          ? `Tool visibility set to ${visibility.replaceAll("_", " ")}.`
          : "Tool visibility set to config default.",
      );
      return;
    }

    if (prompt === "/approval") {
      setInput("");
      appendLine("user", "/approval");
      appendLine("assistant", approvalShortcutHelpText());
      return;
    }
    if (prompt.startsWith("/approval ")) {
      const rest = prompt.slice("/approval ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      const normalized = command.toLowerCase();
      if (normalized === "help") {
        setInput("");
        appendLine("user", prompt);
        appendLine("assistant", approvalShortcutHelpText());
        return;
      }
      if (normalized === "on" && args.length === 0) {
        setInput("");
        setRequireApproval(true);
        appendLine("user", "/approval on");
        appendEvent("Approval gate enabled for tool actions.");
        return;
      }
      if (normalized === "off" && args.length === 0) {
        setInput("");
        setRequireApproval(false);
        appendLine("user", "/approval off");
        appendEvent(
          "Approval gate disabled; sensitive tool calls will be auto-approved for this run configuration.",
        );
        return;
      }
      if (normalized === "status" && args.length === 0) {
        setInput("");
        appendLine("user", "/approval status");
        appendEvent(`Approval gate is ${requireApproval ? "enabled" : "disabled"}.`);
        return;
      }
      if (normalized === "list") {
        setInput("");
        setActiveSection("approvals");
        appendLine("user", prompt);
        const runId = parseApprovalListShortcut(args);
        if (runId) {
          await reviewApprovals(runId);
        }
        return;
      }
      if (normalized === "assess") {
        setInput("");
        setActiveSection("approvals");
        appendLine("user", prompt);
        const parsed = parseApprovalActionShortcut("assess", args);
        if (parsed) {
          await assessApproval(parsed.approvalId, parsed.runId);
        }
        return;
      }
      if (normalized === "approve" || normalized === "reject") {
        setInput("");
        setActiveSection("approvals");
        appendLine("user", prompt);
        const parsed = parseApprovalActionShortcut(normalized, args);
        if (parsed) {
          await decideApproval(
            parsed.approvalId,
            normalized === "approve",
            parsed.runId,
          );
        }
        return;
      }
      if (normalized === "execute") {
        setInput("");
        setActiveSection("approvals");
        appendLine("user", prompt);
        const parsed = parseApprovalActionShortcut("execute", args);
        if (parsed) {
          await executeApproval(parsed.approvalId, parsed.runId);
        }
        return;
      }
      appendLine(
        "error",
        "Approval shortcut needs on, off, status, list, assess, approve, reject, or execute.",
      );
      return;
    }

    if (prompt === "/refine") {
      appendLine("error", "Refine shortcut needs on, off, status, model, or instructions.");
      return;
    }
    if (prompt === "/refine on") {
      setInput("");
      setEnablePromptRefinement(true);
      appendLine("user", "/refine on");
      appendEvent("Prompt refinement enabled.");
      return;
    }
    if (prompt === "/refine off") {
      setInput("");
      setEnablePromptRefinement(false);
      appendLine("user", "/refine off");
      appendEvent("Prompt refinement disabled.");
      return;
    }
    if (prompt === "/refine status") {
      setInput("");
      appendLine("user", "/refine status");
      appendEvent(
        `Prompt refinement is ${enablePromptRefinement ? "enabled" : "disabled"}.`,
      );
      if (promptRefinementModel.trim()) {
        appendEvent(`Refiner model: ${promptRefinementModel.trim()}`);
      }
      if (promptRefinementInstructions.trim()) {
        appendEvent(`Refinement instructions: ${promptRefinementInstructions.trim()}`);
      }
      return;
    }
    if (prompt.startsWith("/refine model ")) {
      const value = prompt.slice("/refine model ".length).trim();
      if (!value) {
        appendLine("error", "Refine model shortcut needs a model id.");
        return;
      }
      setInput("");
      setEnablePromptRefinement(true);
      setPromptRefinementModel(value);
      appendLine("user", `/refine model ${value}`);
      appendEvent(`Prompt refiner model set to ${value}.`);
      return;
    }
    if (prompt.startsWith("/refine instructions ")) {
      const value = prompt.slice("/refine instructions ".length).trim();
      if (!value) {
        appendLine("error", "Refine instructions shortcut needs instruction text.");
        return;
      }
      setInput("");
      setEnablePromptRefinement(true);
      setPromptRefinementInstructions(value);
      appendLine("user", "/refine instructions");
      appendEvent("Prompt refinement instructions updated.");
      return;
    }

    if (prompt === "/shell") {
      appendLine("error", "Shell shortcut needs on, off, or status.");
      return;
    }
    if (prompt.startsWith("/shell ")) {
      const value = prompt.slice("/shell ".length).trim().toLowerCase();
      if (value === "on") {
        setInput("");
        setEnableShell(true);
        appendLine("user", "/shell on");
        appendEvent("Shell tool access enabled.");
        return;
      }
      if (value === "off") {
        setInput("");
        setEnableShell(false);
        appendLine("user", "/shell off");
        appendEvent("Shell tool access disabled.");
        return;
      }
      if (value === "status") {
        setInput("");
        appendLine("user", "/shell status");
        appendEvent(`Shell tool access is ${enableShell ? "enabled" : "disabled"}.`);
        return;
      }
      appendLine("error", "Shell shortcut needs on, off, or status.");
      return;
    }

    if (prompt === "/python" || prompt === "/typescript" || prompt === "/ts") {
      appendLine("error", "Code shortcut needs code text.");
      return;
    }
    if (
      prompt.startsWith("/python ") ||
      prompt.startsWith("/typescript ") ||
      prompt.startsWith("/ts ")
    ) {
      const python = prompt.startsWith("/python ");
      const prefix = python
        ? "/python "
        : prompt.startsWith("/typescript ")
          ? "/typescript "
          : "/ts ";
      const code = prompt.slice(prefix.length).trim();
      if (!code) {
        appendLine("error", "Code shortcut needs code text.");
        return;
      }
      setInput("");
      await callToolDirect(
        python ? "code_python" : "code_typescript",
        { code },
        prompt,
      );
      return;
    }

    if (
      prompt === "/x402" ||
      prompt === "/payment" ||
      prompt === "/x402 request" ||
      prompt === "/x402 required" ||
      prompt === "/x402 settle" ||
      prompt === "/payment x402-request" ||
      prompt === "/payment x402-required" ||
      prompt === "/payment x402-settle"
    ) {
      appendLine(
        "error",
        "x402 shortcuts: /x402 request <url> [--method GET|POST] [--max-amount n] [--auto-pay] [--signature-secret id]; /x402 required --resource <url> --amount <n> --pay-to <addr> --asset <asset> --network <name>; /x402 settle <payment-signature> --facilitator <url> --resource <url> --amount <n> --pay-to <addr> --asset <asset> --network <name> [--mode verify|settle|verify-and-settle].",
      );
      return;
    }
    if (
      prompt.startsWith("/x402 request ") ||
      prompt.startsWith("/payment x402-request ")
    ) {
      const prefix = prompt.startsWith("/x402 request ")
        ? "/x402 request "
        : "/payment x402-request ";
      const inputBody = parseX402RequestShortcut(
        prompt.slice(prefix.length).trim(),
      );
      if (!inputBody) return;
      setInput("");
      await callToolDirect("payment_x402_request", inputBody, prompt);
      return;
    }
    if (
      prompt.startsWith("/x402 required ") ||
      prompt.startsWith("/payment x402-required ")
    ) {
      const prefix = prompt.startsWith("/x402 required ")
        ? "/x402 required "
        : "/payment x402-required ";
      const inputBody = parseX402RequiredShortcut(
        prompt.slice(prefix.length).trim(),
      );
      if (!inputBody) return;
      setInput("");
      await callToolDirect("payment_x402_required", inputBody, prompt);
      return;
    }
    if (
      prompt.startsWith("/x402 settle ") ||
      prompt.startsWith("/payment x402-settle ")
    ) {
      const prefix = prompt.startsWith("/x402 settle ")
        ? "/x402 settle "
        : "/payment x402-settle ";
      const inputBody = parseX402SettleShortcut(
        prompt.slice(prefix.length).trim(),
      );
      if (!inputBody) return;
      setInput("");
      await callToolDirect("payment_x402_settle", inputBody, prompt);
      return;
    }

    if (prompt.startsWith("/memory ")) {
      const value = prompt.slice("/memory ".length).trim().toLowerCase();
      if (value === "on") {
        setInput("");
        setLoadMemory(true);
        appendLine("user", "/memory on");
        appendEvent("Memory loading enabled for context.");
        return;
      }
      if (value === "off") {
        setInput("");
        setLoadMemory(false);
        appendLine("user", "/memory off");
        appendEvent("Memory loading disabled.");
        return;
      }
      if (value === "status") {
        setInput("");
        appendLine("user", "/memory status");
        appendEvent(`Memory loading is ${loadMemory ? "enabled" : "disabled"}.`);
        return;
      }
    }

    if (prompt.startsWith("/skills ")) {
      const value = prompt.slice("/skills ".length).trim().toLowerCase();
      if (value === "on") {
        setInput("");
        setLoadSkills(true);
        appendLine("user", "/skills on");
        appendEvent("Skill loading enabled for context.");
        return;
      }
      if (value === "off") {
        setInput("");
        setLoadSkills(false);
        appendLine("user", "/skills off");
        appendEvent("Skill loading disabled.");
        return;
      }
      if (value === "status") {
        setInput("");
        appendLine("user", "/skills status");
        appendEvent(`Skill loading is ${loadSkills ? "enabled" : "disabled"}.`);
        return;
      }
    }

    if (prompt === "/subagent") {
      appendLine("error", "Subagent shortcut needs on, off, or status.");
      return;
    }
    if (prompt.startsWith("/subagent ")) {
      const value = prompt.slice("/subagent ".length).trim().toLowerCase();
      if (value === "on") {
        setInput("");
        setEnableSubagent(true);
        appendLine("user", "/subagent on");
        appendEvent("Subagent tool enabled.");
        return;
      }
      if (value === "off") {
        setInput("");
        setEnableSubagent(false);
        appendLine("user", "/subagent off");
        appendEvent("Subagent tool disabled.");
        return;
      }
      if (value === "status") {
        setInput("");
        appendLine("user", "/subagent status");
        appendEvent(`Subagent tool is ${enableSubagent ? "enabled" : "disabled"}.`);
        return;
      }
      appendLine("error", "Subagent shortcut needs on, off, or status.");
      return;
    }

    if (prompt === "/cost") {
      appendLine("error", "Cost shortcut needs input, output, both, clear, or status.");
      return;
    }
    if (prompt.startsWith("/cost ")) {
      const rest = prompt.slice("/cost ".length).trim();
      const [action, ...args] = rest.split(/\s+/);
      if (action === "clear") {
        setInput("");
        setInputCostPerMillion("");
        setOutputCostPerMillion("");
        appendLine("user", "/cost clear");
        appendEvent("Token cost overrides cleared; configured model costs will be used.");
        return;
      }
      if (action === "status") {
        setInput("");
        appendLine("user", "/cost status");
        appendEvent(
          `Input cost override: ${inputCostPerMillion.trim() || "config"} $/M`,
        );
        appendEvent(
          `Output cost override: ${outputCostPerMillion.trim() || "config"} $/M`,
        );
        return;
      }
      if (action === "input" || action === "output") {
        const value = args[0] ?? "";
        const parsedCost = parseOptionalNonNegativeFloat(value);
        if (parsedCost === null) {
          appendLine("error", `Cost ${action} shortcut needs a non-negative number.`);
          return;
        }
        setInput("");
        if (action === "input") {
          setInputCostPerMillion(String(parsedCost));
        } else {
          setOutputCostPerMillion(String(parsedCost));
        }
        appendLine("user", `/cost ${action} ${parsedCost}`);
        appendEvent(
          `${action === "input" ? "Input" : "Output"} token cost set to ${parsedCost} $/M.`,
        );
        return;
      }
      if (action === "both") {
        const [inputCost, outputCost] = args;
        const parsedInputCost = parseOptionalNonNegativeFloat(inputCost ?? "");
        const parsedOutputCost = parseOptionalNonNegativeFloat(outputCost ?? "");
        if (parsedInputCost === null || parsedOutputCost === null) {
          appendLine("error", "Cost both shortcut needs two non-negative numbers.");
          return;
        }
        setInput("");
        setInputCostPerMillion(String(parsedInputCost));
        setOutputCostPerMillion(String(parsedOutputCost));
        appendLine("user", `/cost both ${parsedInputCost} ${parsedOutputCost}`);
        appendEvent(
          `Token costs set to input ${parsedInputCost} $/M and output ${parsedOutputCost} $/M.`,
        );
        return;
      }
      appendLine("error", "Cost shortcut needs input, output, both, clear, or status.");
      return;
    }

    const isAgentShortcut = prompt === "/agent" || prompt.startsWith("/agent ");
    const nextAgent = parseAgentShortcut(prompt);
    if (nextAgent) {
      setInput("");
      appendLine("user", `/agent ${nextAgent}`);
      if (nextAgent === "echo" || nextAgent === "tool") {
        setDemo(nextAgent);
        setAgentId("");
        appendEvent(`Switched demo agent to ${agentDisplayName(nextAgent)}`);
      } else {
        setAgentId(nextAgent);
        appendEvent(`Selected configured agent ${nextAgent}`);
      }
      return;
    }
    if (isAgentShortcut) return;

    if (prompt === "/agents" || prompt.startsWith("/agents ")) {
      setInput("");
      setActiveSection("chat");
      appendLine("user", prompt);
      const rest = prompt === "/agents" ? "" : prompt.slice("/agents ".length).trim();
      if (!rest) {
        await reviewAgents();
      } else if (rest.startsWith("show ")) {
        const id = rest.slice("show ".length).trim();
        if (!id) {
          appendLine("error", "Agents show shortcut needs an agent id.");
        } else {
          await showAgent(id);
        }
      } else if (rest.startsWith("use ")) {
        const id = rest.slice("use ".length).trim();
        if (!id) {
          appendLine("error", "Agents use shortcut needs an agent id.");
        } else {
          setAgentId(id);
          appendEvent(`Selected configured agent ${id}`);
        }
      } else if (rest.startsWith("delete ")) {
        const id = rest.slice("delete ".length).trim();
        if (!id) {
          appendLine("error", "Agents delete shortcut needs an agent id.");
        } else {
          await deleteAgentFromOps(id);
        }
      } else {
        appendLine("error", "Agents shortcut needs show, use, or delete.");
      }
      return;
    }

    const previewPrompt = parsePreviewShortcut(prompt);
    if (previewPrompt !== null) {
      setInput("");
      appendLine("user", previewPrompt ? `/preview ${previewPrompt}` : "/preview");
      await previewCurrentContext(previewPrompt || "preview");
      return;
    }

    if (prompt === "/tool") {
      appendLine("error", "Tool run shortcut needs a tool name: /tool <name> <request>.");
      return;
    }

    const isToolShortcut = prompt.startsWith("/tool!");
    const toolShortcut = parseDirectToolShortcut(prompt);
    if (toolShortcut) {
      const inputBody = parseJsonObject("Tool shortcut", toolShortcut.inputText);
      if (!inputBody) return;
      setInput("");
      await callToolDirect(toolShortcut.name, inputBody, prompt);
      return;
    }
    if (isToolShortcut) return;

    const scoreShortcut = parseScoreShortcut(prompt);
    if (scoreShortcut) {
      setInput("");
      appendLine("user", prompt);
      if (!lastRunId) {
        appendLine("error", "Score shortcut needs a completed or active run.");
        return;
      }
      await scoreLastRun(scoreShortcut.score, scoreShortcut.target);
      return;
    }
    if (prompt === "/score") {
      appendLine("error", "Score shortcut needs a number from 0 to 10.");
      return;
    }

    const resumeShortcut = parseResumeShortcut(prompt);
    if (resumeShortcut) {
      setInput("");
      appendLine("user", prompt);
      await resumeRun(resumeShortcut.runId, resumeShortcut.fromEvent);
      return;
    }

    if (prompt === "/config") {
      setInput("");
      appendLine("user", "/config");
      await explainCurrentConfig();
      return;
    }

    if (prompt === "/tools") {
      setInput("");
      appendLine("user", "/tools");
      await explainCurrentTools();
      return;
    }

    if (prompt === "/voice" || prompt.startsWith("/voice ")) {
      setInput("");
      setActiveSection("artifacts");
      const rest = prompt === "/voice" ? "status" : prompt.slice("/voice ".length).trim();
      if (!rest || rest === "status") {
        appendLine("user", prompt);
        appendVoiceStatus();
      } else if (rest === "capture" || rest === "start" || rest === "record") {
        appendLine("user", prompt);
        await startVoiceCapture();
      } else if (rest === "stop" || rest === "end") {
        appendLine("user", prompt);
        stopVoiceCapture();
      } else if (rest === "transcribe") {
        if (!voiceCaptureArtifact) {
          appendLine("user", prompt);
          appendLine("error", "Voice transcribe needs a captured artifact or audio path.");
        } else {
          await transcribeVoicePath(voiceCaptureArtifact.path, prompt);
        }
      } else if (rest.startsWith("transcribe ")) {
        const path = rest.slice("transcribe ".length).trim();
        if (!path) {
          appendLine("user", prompt);
          appendLine("error", "Voice transcribe needs an audio path.");
        } else {
          await transcribeVoicePath(path, prompt);
        }
      } else if (rest === "speak") {
        const text = voiceSpeakText();
        if (!text) {
          appendLine("user", prompt);
          appendLine("error", "Voice speak needs text or a recent assistant answer.");
        } else {
          await speakVoiceText(text, prompt);
        }
      } else if (rest.startsWith("speak ")) {
        const text = rest.slice("speak ".length).trim();
        if (!text) {
          appendLine("user", prompt);
          appendLine("error", "Voice speak needs text.");
        } else {
          await speakVoiceText(text, prompt);
        }
      } else if (rest === "stage") {
        appendLine("user", prompt);
        stageVoiceSpeak();
      } else if (rest.startsWith("stage ")) {
        appendLine("user", prompt);
        stageVoiceSpeak(rest.slice("stage ".length).trim());
      } else {
        appendLine(
          "error",
          "Voice shortcut needs status, capture, stop, transcribe, speak, or stage.",
        );
      }
      return;
    }

    if (
      prompt === "/compactions" ||
      prompt === "/compactions list" ||
      prompt.startsWith("/compactions ")
    ) {
      setInput("");
      setActiveSection("chat");
      appendLine("user", prompt);
      const rest =
        prompt === "/compactions"
          ? ""
          : prompt.slice("/compactions ".length).trim();
      if (!rest || rest === "list") {
        await listCompactionsFromOps();
      } else if (rest.startsWith("show ")) {
        const id = rest.slice("show ".length).trim();
        if (!id) {
          appendLine("error", "Compactions show shortcut needs a compaction id.");
        } else {
          await showCompactionFromOps(id);
        }
      } else if (rest.startsWith("use ")) {
        const id = rest.slice("use ".length).trim();
        if (!id) {
          appendLine("error", "Compactions use shortcut needs a compaction id.");
        } else {
          await useCompactionFromOps(id);
        }
      } else if (rest.startsWith("export ")) {
        const exportArgs = parseCompactionExportShortcut(
          rest.slice("export ".length).trim(),
        );
        if (exportArgs) {
          await exportCompactionFromOps(exportArgs.id, exportArgs.path);
        }
      } else if (rest.startsWith("import ")) {
        const path = rest.slice("import ".length).trim();
        if (!path) {
          appendLine("error", "Compactions import shortcut needs a file path.");
        } else {
          await importCompactionFromOps(path);
        }
      } else if (rest.startsWith("delete ") || rest.startsWith("rm ")) {
        const prefix = rest.startsWith("delete ") ? "delete " : "rm ";
        const id = rest.slice(prefix.length).trim();
        if (!id) {
          appendLine("error", "Compactions delete shortcut needs a compaction id.");
        } else {
          await deleteCompactionFromOps(id);
        }
      } else {
        appendLine(
          "error",
          "Compactions shortcut needs list, show, use, export, import, or delete.",
        );
      }
      return;
    }

    if (prompt === "/storage" || prompt === "/storage report" || prompt.startsWith("/storage ")) {
      setInput("");
      setActiveSection("adapters");
      appendLine("user", prompt);
      const rest = prompt === "/storage" ? "" : prompt.slice("/storage ".length).trim();
      if (!rest || rest === "report") {
        await storageReportFromOps();
      } else if (rest.startsWith("prune-cache ")) {
        const prune = parseStoragePruneShortcut(
          rest.slice("prune-cache ".length).trim(),
        );
        if (prune) {
          await storagePruneCacheFromOps(prune.apply, prune.days);
        }
      } else {
        appendLine("error", "Storage shortcut needs report or prune-cache.");
      }
      return;
    }

    if (prompt === "/memory" || prompt.startsWith("/memory ")) {
      setInput("");
      setActiveSection("memory");
      appendLine("user", prompt);
      const rest = prompt === "/memory" ? "" : prompt.slice("/memory ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "list") {
        await reviewMemory();
      } else if (command === "help") {
        appendLine("assistant", memoryShortcutHelpText());
      } else if (command === "access") {
        await reviewMemoryAccess();
      } else if (command === "backends") {
        await reviewMemoryBackends();
      } else if (command === "preview") {
        await previewWithMemoryFromOps();
      } else if (command === "create") {
        const content = rest.slice("create".length).trim();
        if (!content) {
          appendLine("error", "Memory create shortcut needs content.");
        } else {
          await createMemoryFromOps(content);
        }
      } else if (command === "generate") {
        const text = rest.slice("generate".length).trim();
        if (!text) {
          appendLine("error", "Memory generate shortcut needs text.");
        } else {
          await generateMemoryFromOps(text);
        }
      } else if (command === "generate-conversation") {
        const parsed = parseMemoryConversationRangeShortcut(args);
        if (parsed) {
          await generateConversationMemoryFromOps(parsed);
        }
      } else if (command === "classify") {
        if (args.length !== 1) {
          appendLine("error", "Memory classify shortcut needs a memory id.");
        } else {
          await classifyMemoryFromOps(args[0]);
        }
      } else if (command === "edit") {
        const match = rest.slice("edit".length).trim().match(/^(\S+)\s+([\s\S]+)$/);
        if (!match) {
          appendLine("error", "Memory edit shortcut needs a memory id and content.");
        } else {
          await editMemoryFromOps(match[1], match[2].trim());
        }
      } else if (command === "delete") {
        const ids = args.filter((arg) => arg !== "--confirm");
        const confirmed = args.includes("--confirm");
        if (ids.length !== 1) {
          appendLine(
            "error",
            "Memory delete shortcut needs a memory id and required --confirm.",
          );
        } else if (!confirmed) {
          appendLine("error", "Memory delete shortcut requires --confirm.");
        } else {
          await deleteMemoryFromOps(ids[0], true);
        }
      } else if (command === "rollback") {
        const extra = args.filter((arg) => arg !== "--confirm");
        const confirmed = args.includes("--confirm");
        if (extra.length) {
          appendLine("error", "Memory rollback shortcut accepts only --confirm.");
        } else if (!confirmed) {
          appendLine("error", "Memory rollback shortcut requires --confirm.");
        } else {
          await rollbackMemoryFromOps(true);
        }
      } else {
        appendLine(
          "error",
          "Memory shortcut needs on, off, status, list, access, backends, preview, create, generate, generate-conversation, classify, edit, delete, rollback, or help.",
        );
      }
      return;
    }

    if (prompt === "/ingest" || prompt === "/ingest list" || prompt.startsWith("/ingest ")) {
      setInput("");
      setActiveSection("ingest");
      appendLine("user", prompt);
      if (prompt === "/ingest" || prompt === "/ingest list") {
        await reviewIngestion();
      } else if (prompt === "/ingest backends") {
        await reviewIngestionBackends();
      } else if (prompt.startsWith("/ingest add ")) {
        const parsed = parseIngestModelShortcut(
          prompt.slice("/ingest add ".length).trim(),
          "add",
        );
        if (parsed) {
          await ingestPathFromOps(parsed.target, parsed.options);
        }
      } else if (
        prompt.startsWith("/ingest probe-vision ") ||
        prompt.startsWith("/ingest probe ")
      ) {
        const prefix = prompt.startsWith("/ingest probe-vision ")
          ? "/ingest probe-vision "
          : "/ingest probe ";
        const probe = parseIngestProbeVisionShortcut(
          prompt.slice(prefix.length).trim(),
        );
        if (probe) {
          await probeIngestVisionFromOps(probe.path, probe.model);
        }
      } else if (prompt.startsWith("/ingest show ")) {
        const id = prompt.slice("/ingest show ".length).trim();
        if (!id) {
          appendLine("error", "Ingest show shortcut needs an artifact id.");
        } else {
          await showIngestFromOps(id);
        }
      } else if (prompt.startsWith("/ingest rerun ")) {
        const parsed = parseIngestModelShortcut(
          prompt.slice("/ingest rerun ".length).trim(),
          "rerun",
        );
        if (parsed) {
          await rerunIngestFromOps(parsed.target, parsed.options);
        }
      } else if (
        prompt.startsWith("/ingest use ") ||
        prompt.startsWith("/ingest include ")
      ) {
        const prefix = prompt.startsWith("/ingest use ")
          ? "/ingest use "
          : "/ingest include ";
        const id = prompt.slice(prefix.length).trim();
        if (!id) {
          appendLine("error", "Ingest use shortcut needs an artifact id.");
        } else {
          includeIngestFromOps(id);
        }
      } else if (prompt.startsWith("/ingest preview ")) {
        const id = prompt.slice("/ingest preview ".length).trim();
        if (!id) {
          appendLine("error", "Ingest preview shortcut needs an artifact id.");
        } else {
          await previewWithIngestFromOps(id);
        }
      } else if (prompt.startsWith("/ingest review ")) {
        const review = parseIngestReviewShortcut(
          prompt.slice("/ingest review ".length).trim(),
        );
        if (review) {
          await reviewIngestFindingFromOps(review);
        }
      } else if (
        prompt.startsWith("/ingest delete ") ||
        prompt.startsWith("/ingest rm ") ||
        prompt.startsWith("/ingest remove ")
      ) {
        const prefix = prompt.startsWith("/ingest delete ")
          ? "/ingest delete "
          : prompt.startsWith("/ingest remove ")
            ? "/ingest remove "
            : "/ingest rm ";
        const id = prompt.slice(prefix.length).trim();
        if (!id) {
          appendLine("error", "Ingest delete shortcut needs an artifact id.");
        } else {
          await removeIngestFromOps(id);
        }
      } else {
        appendLine(
          "error",
          "Ingest shortcut needs list, backends, add, probe-vision, show, rerun, use, preview, review, or delete.",
        );
      }
      return;
    }

    if (prompt === "/artifacts" || prompt === "/artifacts list" || prompt.startsWith("/artifacts ")) {
      setInput("");
      setActiveSection("artifacts");
      appendLine("user", prompt);
      if (prompt === "/artifacts" || prompt === "/artifacts list") {
        await reviewGeneratedArtifacts();
      } else if (prompt.startsWith("/artifacts show ")) {
        const id = prompt.slice("/artifacts show ".length).trim();
        if (!id) {
          appendLine("error", "Artifact show shortcut needs an artifact id.");
        } else {
          await showGeneratedArtifactFromOps(id);
        }
      } else if (prompt.startsWith("/artifacts open ")) {
        const id = prompt.slice("/artifacts open ".length).trim();
        if (!id) {
          appendLine("error", "Artifact open shortcut needs an artifact id.");
        } else {
          await openGeneratedArtifactFromOps(id);
        }
      } else if (prompt.startsWith("/artifacts preview ")) {
        const id = prompt.slice("/artifacts preview ".length).trim();
        if (!id) {
          appendLine("error", "Artifact preview shortcut needs an artifact id.");
        } else {
          await previewGeneratedArtifactById(id);
        }
      } else if (prompt.startsWith("/artifacts delete ")) {
        const id = prompt.slice("/artifacts delete ".length).trim();
        if (!id) {
          appendLine("error", "Artifact delete shortcut needs an artifact id.");
        } else {
          await deleteGeneratedArtifactFromOps(id);
        }
      } else {
        appendLine(
          "error",
          "Artifacts shortcut needs list, show, open, preview, or delete.",
        );
      }
      return;
    }

    if (prompt === "/skills" || prompt.startsWith("/skills ")) {
      setInput("");
      setActiveSection("skills");
      appendLine("user", prompt);
      const rest = prompt === "/skills" ? "" : prompt.slice("/skills ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "list") {
        await reviewSkills();
      } else if (command === "help") {
        appendLine("assistant", skillShortcutHelpText());
      } else if (command === "show" || command === "inspect") {
        if (args.length !== 1) {
          appendLine("error", "Skills show shortcut needs a skill id.");
        } else {
          await showSkillFromOps(args[0]);
        }
      } else if (command === "import-openclaw" || command === "install") {
        if (args.length !== 1) {
          appendLine("error", "Skills import-openclaw shortcut needs a path.");
        } else {
          await importSkillFromOps(args[0]);
        }
      } else if (command === "import-doc" || command === "import") {
        if (args.length !== 1) {
          appendLine("error", "Skills import-doc shortcut needs a path.");
        } else {
          await importSkillDocFromOps(args[0]);
        }
      } else if (command === "export") {
        if (args.length !== 2) {
          appendLine("error", "Skills export shortcut needs a skill id and path.");
        } else {
          await exportSkillFromOps(args[0], args[1]);
        }
      } else if (command === "allow" || command === "quarantine") {
        const ids = args.filter((arg) => arg !== "--confirm");
        const id = ids[0] ?? "";
        const confirmed = args.includes("--confirm");
        if (ids.length !== 1) {
          appendLine(
            "error",
            `Skills ${command} shortcut needs a skill id and optional --confirm.`,
          );
        } else if (!confirmed) {
          appendLine("error", `Skills ${command} shortcut requires --confirm.`);
        } else {
          await setSkillQuarantine(command === "allow", id);
        }
      } else {
        appendLine(
          "error",
          "Skills shortcut needs list, show, import-openclaw, import-doc, export, allow, quarantine, or help.",
        );
      }
      return;
    }

    if (prompt === "/capabilities" || prompt.startsWith("/capabilities ")) {
      setInput("");
      setActiveSection("skills");
      appendLine("user", prompt);
      const rest =
        prompt === "/capabilities"
          ? ""
          : prompt.slice("/capabilities ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "list") {
        await reviewCapabilities();
      } else if (command === "help") {
        appendLine(
          "assistant",
          [
            "/capabilities list",
            "/capabilities show <id>",
            "/capabilities export <id> <path>",
            "/capabilities import <path>",
            "/capabilities allow <id> --confirm",
            "/capabilities reject <id> --confirm",
            "/capabilities delete <id>",
          ].join("\n"),
        );
      } else if (command === "show") {
        if (args.length !== 1) {
          appendLine("error", "Capabilities show shortcut needs a draft id.");
        } else {
          await showCapabilityFromOps(args[0]);
        }
      } else if (command === "export") {
        if (args.length !== 2) {
          appendLine(
            "error",
            "Capabilities export shortcut needs a draft id and path.",
          );
        } else {
          await exportCapabilityFromOps(args[0], args[1]);
        }
      } else if (command === "import") {
        if (args.length !== 1) {
          appendLine("error", "Capabilities import shortcut needs a path.");
        } else {
          await importCapabilityFromOps(args[0]);
        }
      } else if (command === "allow" || command === "reject") {
        const ids = args.filter((arg) => arg !== "--confirm");
        const id = ids[0] ?? "";
        const confirmed = args.includes("--confirm");
        if (ids.length !== 1) {
          appendLine(
            "error",
            `Capabilities ${command} shortcut needs a draft id and optional --confirm.`,
          );
        } else if (!confirmed) {
          appendLine(
            "error",
            `Capabilities ${command} shortcut requires --confirm.`,
          );
        } else {
          await reviewCapabilityDraft(command === "allow", id);
        }
      } else if (command === "delete") {
        if (args.length !== 1) {
          appendLine("error", "Capabilities delete shortcut needs a draft id.");
        } else {
          await deleteCapabilityFromOps(args[0]);
        }
      } else {
        appendLine(
          "error",
          "Capabilities shortcut needs list, show, export, import, allow, reject, delete, or help.",
        );
      }
      return;
    }

    if (prompt === "/profiles" || prompt.startsWith("/profiles ")) {
      setInput("");
      setActiveSection("profiles");
      appendLine("user", prompt);
      const rest = prompt === "/profiles" ? "" : prompt.slice("/profiles ".length).trim();
      if (!rest) {
        await listProfilesFromOps();
      } else if (rest === "current") {
        await showCurrentProfileFromOps();
      } else if (rest === "grants" || rest.startsWith("grants ")) {
        const fromProfile =
          rest === "grants" ? "" : rest.slice("grants ".length).trim();
        await listProfileGrantsFromOps(fromProfile || undefined);
      } else if (rest.startsWith("show ")) {
        const id = rest.slice("show ".length).trim();
        if (!id) {
          appendLine("error", "Profiles show shortcut needs a profile id.");
        } else {
          await showProfileFromOps(id);
        }
      } else if (rest.startsWith("create ")) {
        const args = rest.slice("create ".length).trim().split(/\s+/);
        const id = args.shift()?.trim() ?? "";
        const name = args.join(" ").trim() || null;
        if (!id) {
          appendLine("error", "Profiles create shortcut needs a profile id.");
        } else {
          await createProfile(id, name);
        }
      } else if (rest.startsWith("delete ")) {
        const id = rest.slice("delete ".length).trim();
        if (!id) {
          appendLine("error", "Profiles delete shortcut needs a profile id.");
        } else {
          await deleteProfileFromOps(id);
        }
      } else if (rest.startsWith("revoke ")) {
        const id = rest.slice("revoke ".length).trim();
        if (!id) {
          appendLine("error", "Profiles revoke shortcut needs a grant id.");
        } else {
          await revokeProfileGrantFromOps(id);
        }
      } else if (rest.startsWith("grant ")) {
        const args = rest.slice("grant ".length).trim().split(/\s+/).filter(Boolean);
        let fromProfile: string | null = null;
        const fromIndex = args.indexOf("--from");
        if (fromIndex >= 0) {
          fromProfile = args[fromIndex + 1] ?? null;
          args.splice(fromIndex, fromProfile ? 2 : 1);
        }
        const [toProfile, kind, resource] = args;
        if (!toProfile || !isProfileGrantKind(kind) || !resource || args.length !== 3) {
          appendLine(
            "error",
            "Profiles grant shortcut needs: /profiles grant <to-profile> <agent|memory|tool|skill|category> <resource> [--from <profile>].",
          );
        } else {
          await grantProfile(toProfile, kind, resource, fromProfile);
        }
      } else {
        appendLine(
          "error",
          "Profiles shortcut needs current, show, create, delete, grants, grant, or revoke.",
        );
      }
      return;
    }

    if (
      prompt === "/secrets" ||
      prompt === "/secrets backends" ||
      prompt === "/secrets list" ||
      prompt.startsWith("/secrets show ") ||
      prompt.startsWith("/secrets delete ")
    ) {
      setInput("");
      setActiveSection("profiles");
      appendLine("user", prompt);
      if (prompt === "/secrets backends") {
        await listSecretBackendsFromOps();
      } else if (prompt.startsWith("/secrets show ")) {
        const id = prompt.slice("/secrets show ".length).trim();
        if (!id) {
          appendLine("error", "Secrets show shortcut needs a secret id.");
        } else {
          await showSecretFromOps(id);
        }
      } else if (prompt.startsWith("/secrets delete ")) {
        const id = prompt.slice("/secrets delete ".length).trim();
        if (!id) {
          appendLine("error", "Secrets delete shortcut needs a secret id.");
        } else {
          await deleteSecretFromOps(id);
        }
      } else {
        await listSecretsFromOps();
      }
      return;
    }

    if (prompt === "/bundles" || prompt.startsWith("/bundles ")) {
      setInput("");
      setActiveSection("adapters");
      appendLine("user", prompt);
      const rest = prompt === "/bundles" ? "" : prompt.slice("/bundles ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "help") {
        appendLine("assistant", bundleShortcutHelpText());
      } else if (command === "backup") {
        if (args.length) {
          appendLine("error", "Bundles backup shortcut accepts no arguments.");
        } else {
          await backupBundleNow();
        }
      } else if (command === "export") {
        const path = rest.slice("export".length).trim();
        if (!path) {
          appendLine("error", "Bundles export shortcut needs a bundle path.");
        } else {
          await exportBundleFromOps(path);
        }
      } else if (command === "import") {
        const confirmed = args.includes("--confirm");
        const path = args.filter((arg) => arg !== "--confirm").join(" ").trim();
        if (!path) {
          appendLine("error", "Bundles import shortcut needs a bundle path.");
        } else if (!confirmed) {
          appendLine("error", "Bundles import shortcut requires --confirm.");
        } else {
          await importBundleFromOps(path, true);
        }
      } else {
        appendLine("error", "Bundles shortcut needs backup, export, import, or help.");
      }
      return;
    }

    if (
      prompt === "/prompts" ||
      prompt === "/prompts list" ||
      prompt.startsWith("/prompts show ") ||
      prompt.startsWith("/prompts use ") ||
      prompt.startsWith("/prompts preview ") ||
      prompt.startsWith("/prompts delete ")
    ) {
      setInput("");
      setActiveSection("prompts");
      appendLine("user", prompt);
      if (prompt === "/prompts" || prompt === "/prompts list") {
        await reviewPrompts();
      } else if (prompt.startsWith("/prompts show ")) {
        const name = prompt.slice("/prompts show ".length).trim();
        if (!name) {
          appendLine("error", "Prompts show shortcut needs a prompt name.");
        } else {
          await showPromptFromOps(name);
        }
      } else if (prompt.startsWith("/prompts use ")) {
        const name = prompt.slice("/prompts use ".length).trim();
        if (!name) {
          appendLine("error", "Prompts use shortcut needs a prompt name.");
        } else {
          await usePromptByName(name);
        }
      } else if (prompt.startsWith("/prompts preview ")) {
        const name = prompt.slice("/prompts preview ".length).trim();
        if (!name) {
          appendLine("error", "Prompts preview shortcut needs a prompt name.");
        } else {
          await previewPromptByName(name);
        }
      } else if (prompt.startsWith("/prompts delete ")) {
        const name = prompt.slice("/prompts delete ".length).trim();
        if (!name) {
          appendLine("error", "Prompts delete shortcut needs a prompt name.");
        } else {
          await deletePromptByName(name);
        }
      }
      return;
    }

    if (
      prompt === "/models" ||
      prompt === "/models list" ||
      prompt === "/models providers" ||
      prompt === "/models doctor" ||
      prompt === "/models provider-catalog" ||
      prompt.startsWith("/models provider-catalog ") ||
      prompt === "/models metadata-catalog" ||
      prompt.startsWith("/models metadata-catalog ") ||
      prompt.startsWith("/models show ") ||
      prompt.startsWith("/models probe ") ||
      prompt.startsWith("/models delete ")
    ) {
      setInput("");
      setActiveSection("prompts");
      appendLine("user", prompt);
      if (prompt === "/models" || prompt === "/models list") {
        await listModelsFromOps();
      } else if (prompt === "/models providers") {
        await listModelProvidersFromOps();
      } else if (prompt === "/models doctor") {
        await modelDoctorFromOps();
      } else if (
        prompt === "/models provider-catalog" ||
        prompt.startsWith("/models provider-catalog ")
      ) {
        const rest =
          prompt === "/models provider-catalog"
            ? ""
            : prompt.slice("/models provider-catalog ".length).trim();
        if (!rest || rest === "show") {
          await showModelProviderCatalogFromOps();
        } else if (rest.startsWith("export ")) {
          const path = rest.slice("export ".length).trim();
          if (!path) {
            appendLine("error", "Provider catalog export shortcut needs a path.");
          } else {
            await exportModelProviderCatalogFromOps(path);
          }
        } else if (rest.startsWith("import ")) {
          const args = rest.slice("import ".length).trim().split(/\s+/).filter(Boolean);
          const confirmed = args.includes("--confirm");
          const path = args.filter((arg) => arg !== "--confirm").join(" ").trim();
          if (!path) {
            appendLine("error", "Provider catalog import shortcut needs a path.");
          } else if (!confirmed) {
            appendLine("error", "Provider catalog import shortcut requires --confirm.");
          } else {
            await importModelProviderCatalogFromOps(path, true);
          }
        } else {
          appendLine(
            "error",
            "Provider catalog shortcut needs show, export, or import.",
          );
        }
      } else if (
        prompt === "/models metadata-catalog" ||
        prompt.startsWith("/models metadata-catalog ")
      ) {
        const rest =
          prompt === "/models metadata-catalog"
            ? ""
            : prompt.slice("/models metadata-catalog ".length).trim();
        if (!rest || rest === "show") {
          await showModelMetadataCatalogFromOps();
        } else if (rest.startsWith("export ")) {
          const path = rest.slice("export ".length).trim();
          if (!path) {
            appendLine("error", "Metadata catalog export shortcut needs a path.");
          } else {
            await exportModelMetadataCatalogFromOps(path);
          }
        } else if (rest.startsWith("import ")) {
          const args = rest.slice("import ".length).trim().split(/\s+/).filter(Boolean);
          const confirmed = args.includes("--confirm");
          const path = args.filter((arg) => arg !== "--confirm").join(" ").trim();
          if (!path) {
            appendLine("error", "Metadata catalog import shortcut needs a path.");
          } else if (!confirmed) {
            appendLine("error", "Metadata catalog import shortcut requires --confirm.");
          } else {
            await importModelMetadataCatalogFromOps(path, true);
          }
        } else {
          appendLine(
            "error",
            "Metadata catalog shortcut needs show, export, or import.",
          );
        }
      } else if (prompt.startsWith("/models show ")) {
        const id = prompt.slice("/models show ".length).trim();
        if (!id) {
          appendLine("error", "Models show shortcut needs a model id.");
        } else {
          await showModelFromOps(id);
        }
      } else if (prompt.startsWith("/models probe ")) {
        const id = prompt.slice("/models probe ".length).trim();
        if (!id) {
          appendLine("error", "Models probe shortcut needs a model id.");
        } else {
          await probeModelFromOps(id);
        }
      } else if (prompt.startsWith("/models delete ")) {
        const id = prompt.slice("/models delete ".length).trim();
        if (!id) {
          appendLine("error", "Models delete shortcut needs a model id.");
        } else {
          await deleteModelFromOps(id);
        }
      }
      return;
    }

    if (prompt === "/adapters" || prompt.startsWith("/adapters ")) {
      setInput("");
      setActiveSection("adapters");
      appendLine("user", prompt);
      const rest = prompt === "/adapters" ? "" : prompt.slice("/adapters ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "list") {
        await reviewAdapters();
      } else if (command === "help") {
        appendLine("assistant", adapterShortcutHelpText());
      } else if (command === "doctor") {
        await adapterDoctorFromOps();
      } else if (command === "show") {
        if (args.length !== 1) {
          appendLine("error", "Adapters show shortcut needs an adapter id.");
        } else {
          await showAdapterFromOps(args[0]);
        }
      } else if (command === "import") {
        if (args.length !== 1) {
          appendLine("error", "Adapters import shortcut needs a path.");
        } else {
          await importAdapterFromOps(args[0]);
        }
      } else if (command === "import-manifest") {
        if (args.length !== 1) {
          appendLine("error", "Adapters import-manifest shortcut needs a path.");
        } else {
          await importAdapterManifestFromOps(args[0]);
        }
      } else if (command === "export") {
        if (args.length !== 2) {
          appendLine("error", "Adapters export shortcut needs an adapter id and path.");
        } else {
          await exportAdapterFromOps(args[0], args[1]);
        }
      } else if (command === "install-skill") {
        if (args.length !== 1) {
          appendLine("error", "Adapters install-skill shortcut needs an adapter id.");
        } else {
          await installAdapterSkillFromOps(args[0]);
        }
      } else if (command === "allow") {
        const ids = args.filter((arg) => arg !== "--confirm");
        const confirmed = args.includes("--confirm");
        if (ids.length !== 1) {
          appendLine(
            "error",
            "Adapters allow shortcut needs an adapter id and optional --confirm.",
          );
        } else if (!confirmed) {
          appendLine("error", "Adapters allow shortcut requires --confirm.");
        } else {
          await setAdapterQuarantine(true, ids[0]);
        }
      } else if (command === "quarantine") {
        const ids = args.filter((arg) => arg !== "--confirm");
        if (ids.length !== 1) {
          appendLine(
            "error",
            "Adapters quarantine shortcut needs an adapter id and optional --confirm.",
          );
        } else {
          await setAdapterQuarantine(false, ids[0]);
        }
      } else {
        appendLine(
          "error",
          "Adapters shortcut needs list, doctor, show, import, import-manifest, export, install-skill, allow, quarantine, or help.",
        );
      }
      return;
    }

    if (prompt === "/conversation" || prompt.startsWith("/conversation ")) {
      setInput("");
      setActiveSection("conversations");
      appendLine("user", prompt);
      const rest =
        prompt === "/conversation" ? "" : prompt.slice("/conversation ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "list" || command === "tree") {
        await reviewConversations();
      } else if (command === "help") {
        appendLine("assistant", conversationShortcutHelpText());
      } else if (command === "select") {
        if (args.length !== 1) {
          appendLine("error", "Conversation select shortcut needs a conversation id.");
        } else {
          setOpsId(args[0]);
          setConversationId(args[0]);
          appendEvent(`Conversation selected: ${args[0]}`);
        }
      } else if (command === "show") {
        if (args.length > 1) {
          appendLine("error", "Conversation show shortcut accepts at most one id.");
        } else {
          const id = args[0] || selectedConversationShortcutId();
          if (!id) {
            appendLine("error", "Conversation show shortcut needs a conversation id or selected Id.");
          } else {
            await showConversation(id);
          }
        }
      } else if (command === "recover") {
        if (args.length > 1) {
          appendLine("error", "Conversation recover shortcut accepts at most one id.");
        } else {
          const id = args[0] || selectedConversationShortcutId();
          if (!id) {
            appendLine(
              "error",
              "Conversation recover shortcut needs a conversation id or selected Id.",
            );
          } else {
            await recoverConversation(id);
          }
        }
      } else if (command === "policy") {
        if (args[0] === "help") {
          appendLine("assistant", conversationShortcutHelpText());
          return;
        }
        const knownPolicyCommands = ["show", "apply", "save", "clear"];
        const policyCommand = knownPolicyCommands.includes(args[0] || "")
          ? args[0] || "show"
          : "show";
        const idArgs = policyCommand === args[0] ? args.slice(1) : args;
        const id = parseConversationPolicyShortcutId(
          idArgs,
          `Conversation policy ${policyCommand}`,
        );
        if (id) {
          if (policyCommand === "show") {
            await showConversationPolicy(id);
          } else if (policyCommand === "apply") {
            await applyConversationPolicyFromShortcut(id);
          } else if (policyCommand === "save") {
            await saveConversationPolicy(id);
          } else {
            await clearConversationPolicy(id);
          }
        }
      } else if (command === "delete-plan" || command === "delete-preview") {
        const parsed = parseConversationBranchShortcut("delete-plan", args, false);
        if (parsed) {
          await previewConversationDelete(parsed.id, parsed.recursive);
        }
      } else if (command === "delete") {
        const parsed = parseConversationBranchShortcut("delete", args, true);
        if (parsed) {
          await deleteConversation(parsed.id, parsed.recursive, true);
        }
      } else if (command === "range-delete" || command === "delete-range") {
        const parsed = parseConversationRangeShortcut(args);
        if (parsed) {
          await deleteConversationRange(parsed.id, parsed.range, true);
        }
      } else {
        appendLine(
          "error",
          "Conversation shortcut needs list, tree, select, show, recover, policy, delete-plan, delete, range-delete, or help.",
        );
      }
      return;
    }

    if (prompt === "/bridge-deliveries" || prompt.startsWith("/bridge-deliveries ")) {
      setInput("");
      setActiveSection("adapters");
      appendLine("user", prompt);
      const rest =
        prompt === "/bridge-deliveries"
          ? ""
          : prompt.slice("/bridge-deliveries ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "list") {
        await listBridgeDeliveriesFromOps();
      } else if (command === "help") {
        appendLine("assistant", bridgeDeliveryShortcutHelpText());
      } else if (command === "retry") {
        if (args.length !== 1) {
          appendLine("error", "Bridge delivery retry shortcut needs a delivery id.");
        } else {
          await retryBridgeDeliveryFromOps(args[0]);
        }
      } else if (command === "retry-all") {
        if (args.length) {
          appendLine("error", "Bridge delivery retry-all shortcut accepts no arguments.");
        } else {
          await retryAllBridgeDeliveriesFromOps();
        }
      } else {
        appendLine(
          "error",
          "Bridge delivery shortcut needs list, retry, retry-all, or help.",
        );
      }
      return;
    }

    if (prompt === "/hooks" || prompt.startsWith("/hooks ")) {
      setInput("");
      setActiveSection("trace");
      appendLine("user", prompt);
      const rest = prompt === "/hooks" ? "" : prompt.slice("/hooks ".length).trim();
      const [command = "", ...args] = rest.split(/\s+/).filter(Boolean);
      if (!rest || command === "available") {
        await refreshHookCatalog();
      } else if (command === "help") {
        appendLine("assistant", hookShortcutHelpText());
      } else if (command === "list" || command === "policy") {
        await refreshHookPolicy();
      } else if (command === "review") {
        if (args.length > 1) {
          appendLine("error", "Hooks review shortcut accepts at most one run id.");
        } else {
          await reviewHooksFromOps(args[0]);
        }
      } else if (command === "disable" || command === "enable") {
        const hookIds = args.filter((arg) => arg !== "--confirm" && arg !== "--agent");
        const confirmed = args.includes("--confirm");
        const scope = args.includes("--agent") ? "agent" : "profile";
        if (hookIds.length !== 1) {
          appendLine(
            "error",
            `Hooks ${command} shortcut needs a hook id plus optional --agent and required --confirm.`,
          );
        } else if (!confirmed) {
          appendLine("error", `Hooks ${command} shortcut requires --confirm.`);
        } else {
          await setPersistentHookDisabled(
            hookIds[0],
            command === "disable",
            scope,
            true,
          );
        }
      } else {
        appendLine(
          "error",
          "Hooks shortcut needs available, list, review, disable, enable, policy, or help.",
        );
      }
      return;
    }

    if (prompt === "/trace" || prompt.startsWith("/trace ")) {
      const rest = prompt === "/trace" ? "" : prompt.slice("/trace ".length).trim();
      setInput("");
      setActiveSection("trace");
      appendLine("user", prompt);
      if (rest === "prompt" || rest === "load-prompt") {
        loadTracePromptToComposer();
        return;
      }
      if (rest === "clear") {
        clearLoadedTrace();
        appendEvent("Cleared loaded trace.");
        return;
      }
      if (rest) {
        setOpsId(rest);
        await loadTraceById(rest);
        return;
      }
      if (!lastRunId) {
        appendLine("error", "Trace shortcut needs a completed or active run.");
        return;
      }
      await loadLastTrace();
      return;
    }

    if (prompt === "/compare" || prompt.startsWith("/compare ")) {
      const rest =
        prompt === "/compare" ? "" : prompt.slice("/compare ".length).trim();
      setInput("");
      setActiveSection("trace");
      appendLine("user", prompt);
      if (rest === "clear") {
        clearTraceComparison();
        appendEvent("Cleared comparison trace.");
        return;
      }
      if (!rest) {
        appendLine("error", "Compare shortcut needs a run id.");
        return;
      }
      if (!traceSummary) {
        appendLine("error", "Load a primary trace before comparing.");
        return;
      }
      await loadTraceComparison(rest);
      return;
    }

    if (prompt === "/replay" || prompt.startsWith("/replay ")) {
      const rest = prompt === "/replay" ? "" : prompt.slice("/replay ".length).trim();
      const args = rest.split(/\s+/).filter(Boolean);
      const replayFlags = [
        "--no-hooks",
        "--skip-hooks",
        "no-hooks",
        "skip-hooks",
        "help",
        "--help",
      ];
      const skipHooks =
        args.includes("--no-hooks") ||
        args.includes("--skip-hooks") ||
        args.includes("no-hooks") ||
        args.includes("skip-hooks");
      const help = args.includes("help") || args.includes("--help");
      const runIds = args.filter((arg) => !replayFlags.includes(arg));
      const unknownFlags = runIds.filter((arg) => arg.startsWith("--"));
      setInput("");
      setActiveSection("trace");
      appendLine("user", prompt);
      if (help) {
        appendLine(
          "assistant",
          "Use /replay [run-id] to run a trace prompt again, or add --no-hooks to skip lifecycle hooks once.",
        );
        return;
      }
      if (unknownFlags.length) {
        appendLine("error", `Replay shortcut does not support ${unknownFlags[0]}.`);
        return;
      }
      if (runIds.length > 1) {
        appendLine(
          "error",
          "Replay shortcut accepts at most one run id plus optional --no-hooks or --skip-hooks.",
        );
        return;
      }
      await replayTracePromptWithOptions({ runId: runIds[0], skipHooks });
      return;
    }

    if (prompt === "/approvals") {
      setInput("");
      setActiveSection("approvals");
      appendLine("user", "/approvals");
      if (!lastRunId) {
        appendLine("error", "Approvals shortcut needs a completed or active run.");
        return;
      }
      await reviewApprovals();
      return;
    }

    if (prompt === "/batch") {
      appendLine("error", "Batch shortcut needs one item per line after /batch.");
      return;
    }
    if (prompt.startsWith("/batch ")) {
      const items = prompt
        .slice("/batch ".length)
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean);
      if (!items.length) {
        appendLine("error", "Batch shortcut needs one item per line after /batch.");
        return;
      }
      await runBatchItems(items);
      return;
    }

    if (prompt === "/resume-batch") {
      appendLine("error", "Resume batch shortcut needs a batch id.");
      return;
    }
    if (prompt.startsWith("/resume-batch ")) {
      const batchId = prompt.slice("/resume-batch ".length).trim();
      if (!batchId) {
        appendLine("error", "Resume batch shortcut needs a batch id.");
        return;
      }
      setInput("");
      setOpsId(batchId);
      appendLine("user", `/resume-batch ${batchId}`);
      await resumeBatchById(batchId);
      return;
    }

    const exportPath = parseExportShortcut(prompt);
    if (exportPath !== null) {
      if (!exportPath) {
        appendLine("error", "Export shortcut needs a bundle path.");
        return;
      }
      setInput("");
      setOpsValue(exportPath);
      appendLine("user", prompt === "/export" ? "/export" : `/export ${exportPath}`);
      await exportBundleToPath(exportPath, "Backup exported");
      return;
    }

    const savedPromptName = prompt.startsWith("/run ")
      ? prompt.slice("/run ".length).trim()
      : null;
    if (savedPromptName) {
      try {
        const savedPromptBody = await loadPromptBody(savedPromptName);
        appendEvent(`Loaded saved prompt for run: ${savedPromptName}`);
        prompt = savedPromptBody;
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        appendLine("error", `Saved prompt failed: ${msg}`);
        return;
      }
    }

    const promptShortcutName = prompt.startsWith("/prompt ")
      ? prompt.slice("/prompt ".length).trim()
      : null;
    if (promptShortcutName) {
      setInput("");
      appendLine("user", `/prompt ${promptShortcutName}`);
      await usePromptByName(promptShortcutName);
      return;
    }
    if (prompt === "/prompt") {
      appendLine("error", "Prompt shortcut needs a saved prompt name.");
      return;
    }

    await runAgentPrompt(prompt, savedPromptName ? `/run ${savedPromptName}` : prompt);
  }

  async function runAgentPrompt(
    prompt: string,
    displayText: string,
    optionOverrides: Partial<RunOptions> = {},
  ) {
    if (running) return;
    setInput("");
    setRunning(true);
    setTokensIn(0);
    setTokensOut(0);
    setCostUsd(0);
    setCalls(0);
    setElapsedMs(0);
    setTraceEvents([]);
    setTraceSummary(null);
    setTraceTree(null);
    setCollapsedTraceTreeRuns([]);
    setTraceCompareSummary(null);
    setTraceCompareTree(null);
    setTraceCompareRunId("");
    setApprovals([]);
    setPostRunCompactionPrompt(null);
    terminalEventSeenRef.current = false;
    rootRunIdRef.current = null;
    latestRunContextRef.current = null;
    remoteSeenEventKeysRef.current = new Set();
    runStartedAtRef.current = performance.now();
    appendLine("user", displayText);
    const options = { ...runtimeOptions(), ...optionOverrides };

    try {
      if (transport === "daemon") {
        const started = await daemonJson<RemoteRunStart>("/run/start", {
          input: prompt,
          demo,
          ...options,
        });
        setLastRunId(started.run_id);
        rootRunIdRef.current = started.run_id;
        appendEvent(`Remote run started: ${started.run_id}`);
        await pollRemoteRun(started.run_id);
        return;
      }
      // The harness emits RunCompleted/RunFailed via the event channel;
      // the resolved RunSummary here is informational.
      const summary = await invoke<RunSummary>("run_agent", {
        input: prompt,
        demo,
        options,
      });
      setLastRunId(summary.run_id);
      if (!terminalEventSeenRef.current) {
        appendLine("assistant", summary.final_output);
        appendEvent(`Run completed: ${summary.run_id}`);
      }
      setRunning(false);
      runStartedAtRef.current = null;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      captureRunIdFromError(msg);
      if (!terminalEventSeenRef.current) {
        appendLine("error", `Invoke failed: ${msg}`);
      }
      setRunning(false);
      runStartedAtRef.current = null;
    }
  }

  async function callShell() {
    const command = input.trim();
    if (!command || running) return;
    setInput("");
    appendLine("user", `$ ${command}`);
    try {
      if (transport === "daemon") {
        const inputBody: Record<string, unknown> = { command };
        if (requireApproval) {
          inputBody.__require_approval = true;
        } else {
          inputBody.__auto_approve = true;
        }
        const output = await daemonJson<unknown>("/tool/shell", inputBody);
        captureDirectToolMetadata(output);
        appendJson("Shell output", output);
        return;
      }
      const output = await invoke<unknown>("call_tool", {
        name: "shell",
        input: { command },
        options: { ...runtimeOptions(), enable_shell: true },
      });
      captureDirectToolMetadata(output);
      appendJson("Shell output", output);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      captureRunIdFromError(msg);
      appendLine("error", `Shell failed: ${msg}`);
    }
  }

  async function callToolFromOps() {
    const name = requireOpsId("Tool call");
    if (!name || running) return;
    const inputBody = parseOpsJsonObject("Tool call");
    if (!inputBody) return;
    const validationErrors = validateVisibleToolInput(name, inputBody);
    if (validationErrors.length) {
      appendLine("error", `Tool input invalid: ${validationErrors.join("; ")}`);
      return;
    }
    await callToolDirect(name, inputBody, `tool ${name} ${JSON.stringify(inputBody)}`);
  }

  async function startVoiceCapture() {
    if (running || recordingVoice) return;
    if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      appendLine("error", "Voice capture is not available in this webview.");
      return;
    }
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const recorder = new MediaRecorder(stream);
      voiceChunksRef.current = [];
      mediaRecorderRef.current = recorder;
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          voiceChunksRef.current.push(event.data);
        }
      };
      recorder.onstop = () => {
        setRecordingVoice(false);
        stream.getTracks().forEach((track) => track.stop());
        const mediaType = recorder.mimeType || "audio/webm";
        const blob = new Blob(voiceChunksRef.current, { type: mediaType });
        void persistVoiceCapture(blob);
      };
      recorder.start();
      setRecordingVoice(true);
      appendEvent("Voice recording started.");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setRecordingVoice(false);
      appendLine("error", `Voice capture failed: ${msg}`);
    }
  }

  function stopVoiceCapture() {
    if (!recordingVoice) return;
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      recorder.stop();
    }
  }

  function appendVoiceStatus() {
    appendJson("Voice status", {
      recording: recordingVoice,
      capture_artifact: voiceCaptureArtifact,
      output_artifact: voiceOutputArtifact,
    });
  }

  async function persistVoiceCapture(blob: Blob) {
    if (blob.size === 0) {
      appendLine("error", "Voice capture was empty.");
      return;
    }
    try {
      if (voicePreviewUrl) {
        URL.revokeObjectURL(voicePreviewUrl);
      }
      const previewUrl = URL.createObjectURL(blob);
      setVoicePreviewUrl(previewUrl);
      const dataUrl = await blobToDataUrl(blob);
      const extension = voiceExtensionForBlob(blob);
      const artifact = await saveVoiceCaptureArtifact(dataUrl, `capture.${extension}`);
      setVoiceCaptureArtifact(artifact);
      setGeneratedArtifacts((artifacts) =>
        upsertGeneratedArtifact(artifacts, artifact),
      );
      setOpsId("voice_transcribe");
      setOpsValue(previewJson({ audio_path: artifact.path }));
      appendEvent(
        `Voice captured: ${fileName(artifact.path)} / ${formatBytes(artifact.bytes)}`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Voice capture save failed: ${msg}`);
    }
  }

  function blobToDataUrl(blob: Blob): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error ?? new Error("read failed"));
      reader.readAsDataURL(blob);
    });
  }

  function voiceExtensionForBlob(blob: Blob) {
    const type = blob.type.split(";")[0].toLowerCase();
    switch (type) {
      case "audio/wav":
      case "audio/x-wav":
        return "wav";
      case "audio/mpeg":
      case "audio/mp3":
        return "mp3";
      case "audio/mp4":
      case "audio/x-m4a":
        return "m4a";
      case "audio/ogg":
        return "ogg";
      case "audio/webm":
      default:
        return "webm";
    }
  }

  async function saveVoiceCaptureArtifact(dataUrl: string, filename: string) {
    if (transport === "daemon") {
      const result = await daemonJson<VoiceCaptureResponse>("/voice/capture", {
        data_url: dataUrl,
        filename,
      });
      return result.artifact;
    }
    return invoke<GeneratedArtifact>("voice_capture", {
      dataUrl,
      filename,
    });
  }

  async function transcribeVoiceCapture() {
    if (!voiceCaptureArtifact || running) return;
    await transcribeVoicePath(
      voiceCaptureArtifact.path,
      `voice transcribe ${fileName(voiceCaptureArtifact.path)}`,
    );
  }

  async function transcribeVoicePath(path: string, display: string) {
    await callToolDirect("voice_transcribe", { audio_path: path }, display);
  }

  async function speakVoiceOutput() {
    if (running || voiceOutputBusy) return;
    const text = voiceSpeakText();
    if (!text) {
      appendLine("error", "Voice output needs composer text or a recent assistant answer.");
      return;
    }
    setVoiceOutputBusy(true);
    try {
      await speakVoiceText(text, `voice speak ${previewText(text, 48)}`);
    } finally {
      setVoiceOutputBusy(false);
    }
  }

  async function speakVoiceText(text: string, display: string) {
    await callToolDirect("voice_speak", { text }, display);
  }

  function stageVoiceSpeak(textOverride?: string) {
    const text = voiceSpeakText();
    const resolvedText = textOverride?.trim() || text;
    if (!resolvedText) {
      appendLine("error", "Voice output needs composer text or a recent assistant answer.");
      return;
    }
    setOpsId("voice_speak");
    setOpsValue(previewJson({ text: resolvedText }));
    appendEvent("voice_speak staged.");
  }

  function voiceSpeakText() {
    return input.trim() || latestAssistantText();
  }

  function latestAssistantText() {
    return (
      [...transcript]
        .reverse()
        .find((line) => line.kind === "assistant")
        ?.text.trim() ?? ""
    );
  }

  async function refreshVoiceOutputFromToolOutput(value: unknown) {
    const artifact = voiceOutputArtifactFromToolOutput(value);
    if (!artifact) return;
    setGeneratedArtifacts((artifacts) =>
      upsertGeneratedArtifact(artifacts, artifact),
    );
    await previewGeneratedAudioArtifact(artifact);
  }

  function voiceOutputArtifactFromToolOutput(value: unknown) {
    const wrapper = isUnknownRecord(value) ? value : null;
    const output = wrapper && isUnknownRecord(wrapper.output) ? wrapper.output : wrapper;
    if (!output) return null;
    const artifactId = output.artifact_id;
    const format = output.format;
    const path = output.path;
    const bytes = output.bytes;
    if (
      typeof artifactId !== "string" ||
      typeof format !== "string" ||
      typeof path !== "string" ||
      typeof bytes !== "number" ||
      !isAudioFormat(format)
    ) {
      return null;
    }
    return {
      id: artifactId,
      format,
      path,
      bytes,
      modified_ms: null,
    };
  }

  async function previewGeneratedArtifact(artifact: GeneratedArtifact) {
    if (!isInlineArtifactFormat(artifact.format)) return;
    await previewGeneratedArtifactById(artifact.id);
  }

  async function previewGeneratedArtifactById(id: string) {
    try {
      const preview = await loadGeneratedArtifactDataUrl(id);
      setArtifactPreview(preview);
      if (isAudioFormat(preview.artifact.format)) {
        setVoiceOutputArtifact(preview.artifact);
        setVoiceOutputPreviewUrl(preview.data_url);
      }
      setGeneratedArtifacts((artifacts) =>
        upsertGeneratedArtifact(artifacts, preview.artifact),
      );
      const previewLabel = isAudioFormat(preview.artifact.format)
        ? "Voice output ready"
        : "Artifact preview ready";
      appendEvent(`${previewLabel}: ${fileName(preview.artifact.path)}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Artifact preview failed: ${msg}`);
    }
  }

  async function previewGeneratedAudioArtifact(artifact: GeneratedArtifact) {
    if (!isAudioFormat(artifact.format)) return;
    await previewGeneratedArtifact(artifact);
  }

  async function loadGeneratedArtifactDataUrl(id: string) {
    return transport === "daemon"
      ? await daemonJson<GeneratedArtifactDataUrl>(
          `/artifacts/${encodeURIComponent(id)}/data-url`,
        )
      : await invoke<GeneratedArtifactDataUrl>("artifact_data_url", { id });
  }

  function isInlineArtifactFormat(format: string) {
    return (
      isAudioFormat(format) ||
      isTextArtifactFormat(format) ||
      ["html", "pdf"].includes(format.toLowerCase())
    );
  }

  function isTextArtifactFormat(format: string) {
    return ["txt", "md", "csv", "json"].includes(format.toLowerCase());
  }

  function textFromDataUrl(dataUrl: string) {
    const marker = ";base64,";
    const markerIndex = dataUrl.indexOf(marker);
    if (markerIndex < 0) return "";
    try {
      const base64 = dataUrl.slice(markerIndex + marker.length);
      const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
      return new TextDecoder().decode(bytes);
    } catch {
      return "";
    }
  }

  function isAudioFormat(format: string) {
    return ["mp3", "wav", "webm", "m4a", "ogg"].includes(format.toLowerCase());
  }

  async function stageToolFromPreview(tool: ContextSnapshot["visible_tools"][number]) {
    try {
      const stagedTool = await toolWithParameters(tool);
      setOpsId(stagedTool.id);
      setOpsValue(previewJson(sampleToolInput(stagedTool.input_schema)));
      if (stagedTool.input_schema && !tool.input_schema) {
        setContextPreview((snapshot) =>
          snapshot
            ? {
                ...snapshot,
                visible_tools: snapshot.visible_tools.map((item) =>
                  item.id === stagedTool.id ? stagedTool : item,
                ),
              }
            : snapshot,
        );
        appendEvent(`Loaded tool parameters for manual call: ${stagedTool.id}`);
      }
      appendEvent(`Tool staged for manual call: ${stagedTool.id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Tool staging failed: ${msg}`);
    }
  }

  async function toolWithParameters(tool: ContextSnapshot["visible_tools"][number]) {
    if (tool.input_schema) {
      return tool;
    }
    const options = {
      ...runtimeOptions(),
      tool_visibility: "full_schema" as ToolVisibility,
    };
    const tools =
      transport === "daemon"
        ? await daemonJson<ContextSnapshot["visible_tools"]>(
            "/explain-tools",
            options,
          )
        : await invoke<ContextSnapshot["visible_tools"]>("explain_tools", {
            options,
          });
    return tools.find((candidate) => candidate.id === tool.id) ?? tool;
  }

  function sampleToolInput(schema: unknown): Record<string, JsonValue> {
    if (!isUnknownRecord(schema)) {
      return {};
    }
    const properties = isUnknownRecord(schema.properties)
      ? schema.properties
      : {};
    const required = Array.isArray(schema.required)
      ? schema.required.filter((key): key is string => typeof key === "string")
      : [];
    const keys = required.length ? required : Object.keys(properties);
    return keys.reduce<Record<string, JsonValue>>((input, key) => {
      input[key] = sampleSchemaValue(properties[key]);
      return input;
    }, {});
  }

  function toolParameters(schema: unknown): ToolParameterView[] {
    if (!isUnknownRecord(schema)) {
      return [];
    }
    const properties = isUnknownRecord(schema.properties)
      ? schema.properties
      : {};
    const required = new Set(
      Array.isArray(schema.required)
        ? schema.required.filter((key): key is string => typeof key === "string")
        : [],
    );
    return Object.entries(properties).map(([name, property]) => {
      const record = isUnknownRecord(property) ? property : {};
      return {
        name,
        type: schemaTypeLabel(record.type),
        required: required.has(name),
        description:
          typeof record.description === "string" ? record.description : null,
      };
    });
  }

  function schemaTypeLabel(type: unknown) {
    if (Array.isArray(type)) {
      return type.filter((item) => typeof item === "string").join(" | ") || "any";
    }
    return typeof type === "string" ? type : "any";
  }

  function sampleSchemaValue(schema: unknown): JsonValue {
    if (!isUnknownRecord(schema)) {
      return "replace me";
    }
    if (Array.isArray(schema.enum) && schema.enum.length) {
      const first = schema.enum[0];
      return isJsonValue(first) ? first : String(first);
    }
    switch (schema.type) {
      case "integer":
      case "number":
        return 0;
      case "boolean":
        return false;
      case "array":
        return [];
      case "object":
        return sampleToolInput(schema);
      case "string":
      default:
        return "replace me";
    }
  }

  function validateVisibleToolInput(
    toolId: string,
    inputBody: Record<string, unknown>,
  ) {
    const tool = contextPreview?.visible_tools.find((item) => item.id === toolId);
    if (!tool?.input_schema || !isUnknownRecord(tool.input_schema)) {
      return [];
    }
    const properties = isUnknownRecord(tool.input_schema.properties)
      ? tool.input_schema.properties
      : {};
    const required = Array.isArray(tool.input_schema.required)
      ? tool.input_schema.required.filter(
          (key): key is string => typeof key === "string",
        )
      : [];
    const errors = required
      .filter((key) => !(key in inputBody))
      .map((key) => `${tool.id}.${key} is required`);
    for (const [key, value] of Object.entries(inputBody)) {
      const property = properties[key];
      if (!isUnknownRecord(property) || !("type" in property)) {
        continue;
      }
      const typeError = validateSchemaType(key, value, property.type);
      if (typeError) {
        errors.push(typeError);
      }
    }
    return errors;
  }

  function validateSchemaType(key: string, value: unknown, schemaType: unknown) {
    const expected = Array.isArray(schemaType) ? schemaType : [schemaType];
    if (!expected.every((item) => typeof item === "string")) {
      return null;
    }
    const valid = expected.some((type) => {
      switch (type) {
        case "string":
          return typeof value === "string";
        case "integer":
          return typeof value === "number" && Number.isInteger(value);
        case "number":
          return typeof value === "number";
        case "boolean":
          return typeof value === "boolean";
        case "array":
          return Array.isArray(value);
        case "object":
          return isUnknownRecord(value);
        case "null":
          return value === null;
        default:
          return true;
      }
    });
    return valid ? null : `${key} must be ${expected.join(" or ")}`;
  }

  async function callToolDirect(
    name: string,
    inputBody: Record<string, unknown>,
    display: string,
  ) {
    appendLine("user", display);

    try {
      if (transport === "daemon") {
        const daemonInput = { ...inputBody };
        if (agentId.trim()) {
          daemonInput.__agent_id = agentId.trim();
        }
        if (requireApproval) {
          daemonInput.__require_approval = true;
        } else {
          daemonInput.__auto_approve = true;
        }
        const output = await daemonJson<unknown>(
          `/tool/${encodeURIComponent(name)}`,
          daemonInput,
        );
        captureDirectToolMetadata(output);
        void refreshVoiceOutputFromToolOutput(output);
        appendJson("Tool output", output);
        return output;
      }
      const output = await invoke<unknown>("call_tool", {
        name,
        input: inputBody,
        options: {
          ...runtimeOptions(),
          enable_shell: enableShell || isShellRuntimeToolName(name),
        },
      });
      captureDirectToolMetadata(output);
      void refreshVoiceOutputFromToolOutput(output);
      appendJson("Tool output", output);
      return output;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      captureRunIdFromError(msg);
      appendLine("error", `Tool call failed: ${msg}`);
      return null;
    }
  }

  async function previewCurrentContext(promptOverride?: string) {
    const prompt = promptOverride?.trim() || input.trim() || "preview";
    try {
      const snapshot =
        transport === "daemon"
          ? await daemonJson<ContextSnapshot>("/preview-context", {
              input: prompt,
              ...runtimeOptions(),
            })
          : await invoke<ContextSnapshot>("preview_context", {
              input: prompt,
              options: runtimeOptions(),
      });
      setContextPreview(snapshot);
      setContextPreviewPrompt(prompt);
      setContextCopyStatus("");
      appendEvent(
        `Preview: ~${snapshot.estimated_input_tokens} input tokens, ${snapshot.visible_tools.length} tools, ${snapshot.visible_skills.length} skills, ${snapshot.loaded_memory.length} memory, ${snapshot.loaded_artifacts.length} artifacts, ${snapshot.provenance.length} provenance records`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Preview failed: ${msg}`);
    }
  }

  async function keepContextPreviewCompaction(
    snapshot: ContextSnapshot | null = contextPreview,
    sourceBase = "auto-preview",
  ) {
    const content = snapshot?.compacted?.trim();
    if (!content) {
      appendLine("error", "No compacted context to keep.");
      return false;
    }
    const conversationId = selectedCompactionConversationId();
    const guidance = compactionGuidance.trim() || null;
    const maxOutputTokens = parseOptionalPositiveInt(maxCompactionOutputTokens);
    const source = conversationId
      ? `${sourceBase}:${conversationId}`
      : sourceBase;
    try {
      const record =
        transport === "daemon"
          ? await daemonJson<CompactionRecord>("/compactions/keep", {
              content,
              guidance,
              source,
              conversation_id: conversationId,
              max_output_tokens: maxOutputTokens,
            })
          : await invoke<CompactionRecord>("compaction_keep", {
              content,
              guidance,
              source,
              conversationId,
              maxOutputTokens,
            });
      setOpsId(record.id);
      setOpsValue(record.content);
      setCompactionRecords((records) => upsertCompactionRecord(records, record));
      appendEvent(
        `Kept compacted context ${record.id}${record.conversation_id ? ` for ${record.conversation_id}` : ""}.`,
      );
      return true;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Keep compacted context failed: ${msg}`);
      return false;
    }
  }

  async function keepPostRunCompaction() {
    if (!postRunCompactionPrompt) {
      return;
    }
    const kept = await keepContextPreviewCompaction(
      postRunCompactionPrompt.snapshot,
      `auto-run:${postRunCompactionPrompt.runId}`,
    );
    if (kept) {
      setPostRunCompactionPrompt(null);
    }
  }

  async function listCompactionsFromOps() {
    try {
      const records =
        transport === "daemon"
          ? await daemonJson<CompactionRecord[]>("/compactions")
          : await invoke<CompactionRecord[]>("compaction_list");
      setCompactionRecords(records);
      appendEvent(`Loaded ${records.length} compacted-context artifacts.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Compaction list failed: ${msg}`);
    }
  }

  async function showCompactionFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Compaction show");
    if (!id) return null;
    try {
      const record =
        transport === "daemon"
          ? await daemonJson<CompactionRecord>(
              `/compactions/${encodeURIComponent(id)}`,
            )
          : await invoke<CompactionRecord>("compaction_show", { id });
      setOpsId(record.id);
      setOpsValue(record.content);
      setCompactionRecords((records) => upsertCompactionRecord(records, record));
      appendJson("Compaction", record);
      return record;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Compaction show failed: ${msg}`);
      return null;
    }
  }

  async function useCompactionFromOps(explicitId?: string) {
    const record = await showCompactionFromOps(explicitId);
    if (!record) return;
    setManualCompactedContext(record.content);
    appendEvent(`Manual compacted context set from ${record.id}.`);
  }

  async function exportCompactionFromOps(explicitId?: string, explicitPath?: string) {
    const id = explicitId?.trim() || requireOpsId("Compaction export");
    if (!id) return;
    const path = explicitPath?.trim() || opsValue.trim() || defaultCompactionPath(id);
    setOpsValue(path);
    await exportCompactionToPath(id, path);
  }

  async function exportCompactionToPath(id: string, path: string) {
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<CompactionExportResult>("/compactions/export", {
              id,
              path,
            })
          : await invoke<CompactionExportResult>("compaction_export", {
              id,
              path,
            });
      setCompactionTransferStatus({
        operation: "exported",
        path: result.path,
        record: result.record,
      });
      setCompactionRecords((records) =>
        upsertCompactionRecord(records, result.record),
      );
      appendEvent(`Exported compacted context ${result.record.id} to ${result.path}.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Compaction export failed: ${msg}`);
    }
  }

  async function importCompactionFromOps(explicitPath?: string) {
    const path = explicitPath?.trim() || requireOpsValue("Compaction import");
    if (!path) return;
    try {
      const record =
        transport === "daemon"
          ? await daemonJson<CompactionRecord>("/compactions/import", { path })
          : await invoke<CompactionRecord>("compaction_import", { path });
      setCompactionTransferStatus({ operation: "imported", path, record });
      setCompactionRecords((records) => upsertCompactionRecord(records, record));
      setOpsId(record.id);
      setOpsValue(record.content);
      appendJson("Compaction imported", record);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Compaction import failed: ${msg}`);
    }
  }

  async function deleteCompactionFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Compaction delete");
    if (!id) return;
    if (!confirmLocalChange(`Delete compacted context ${id}`)) return;
    try {
      if (transport === "daemon") {
        await daemonJson<{ deleted: boolean; id: string }>(
          `/compactions/${encodeURIComponent(id)}/delete`,
          {},
        );
      } else {
        await invoke<boolean>("compaction_delete", { id });
      }
      setCompactionRecords((records) => records.filter((record) => record.id !== id));
      if (opsId.trim() === id) {
        setOpsId("");
      }
      appendEvent(`Deleted compacted context ${id}.`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Compaction delete failed: ${msg}`);
    }
  }

  function selectedCompactionConversationId() {
    const id = opsId.trim();
    if (!id) {
      return null;
    }
    if (expandedConversation?.conversation.id === id) {
      return id;
    }
    return conversationDocs.some((conversation) => conversation.id === id)
      ? id
      : null;
  }

  async function explainCurrentConfig() {
    try {
      const explanation =
        transport === "daemon"
          ? await daemonJson<unknown>("/explain-config", runtimeOptions())
          : await invoke<unknown>("explain_config", { options: runtimeOptions() });
      appendJson("Effective config", explanation);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Explain config failed: ${msg}`);
    }
  }

  async function explainCurrentTools() {
    try {
      const tools =
        transport === "daemon"
          ? await daemonJson<unknown>("/explain-tools", runtimeOptions())
          : await invoke<unknown>("explain_tools", { options: runtimeOptions() });
      appendJson("Visible tools", tools);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Explain tools failed: ${msg}`);
    }
  }

  async function reviewMemory() {
    try {
      const records =
        transport === "daemon"
          ? await daemonJson<MemoryRecord[]>("/memory")
          : await invoke<MemoryRecord[]>("memory_list");
      setMemoryRecords(records);
      appendEvent(`Memory records: ${records.length}`);
      appendLine("assistant", JSON.stringify(records, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory review failed: ${msg}`);
    }
  }

  async function reviewMemoryAccess() {
    const topics = parsedMemoryTopics();
    try {
      const report =
        transport === "daemon"
          ? await daemonJson<MemoryAccessReport>("/memory/access", { topics })
          : await invoke<MemoryAccessReport>("memory_access", { topics });
      setMemoryRecords(report.records.map((entry) => entry.record));
      appendEvent(
        `Memory access: ${report.local_records} local, ${report.granted_records} granted`,
      );
      appendJson("Memory access", report);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory access failed: ${msg}`);
    }
  }

  async function reviewMemoryBackends() {
    try {
      const backends =
        transport === "daemon"
          ? await daemonJson<MemoryBackendDescriptor[]>("/memory/backends")
          : await invoke<MemoryBackendDescriptor[]>("memory_backends");
      setMemoryBackends(backends);
      appendJson("Memory backends", backends);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory backends failed: ${msg}`);
    }
  }

  async function previewWithMemoryFromOps() {
    const prompt = input.trim() || "preview";
    const options = {
      ...runtimeOptions(),
      load_memory: true,
    };
    setLoadMemory(true);
    try {
      const snapshot =
        transport === "daemon"
          ? await daemonJson<ContextSnapshot>("/preview-context", {
              input: prompt,
              ...options,
            })
          : await invoke<ContextSnapshot>("preview_context", {
              input: prompt,
              options,
            });
      setContextPreview(snapshot);
      setContextPreviewPrompt(prompt);
      setContextCopyStatus("");
      setActiveSection("chat");
      appendEvent(
        `Memory preview: ${snapshot.loaded_memory.length} memory fragments, ~${snapshot.estimated_input_tokens} input tokens`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory preview failed: ${msg}`);
    }
  }

  async function reviewSkills() {
    try {
      const docs =
        transport === "daemon"
          ? await daemonJson<SkillDoc[]>("/skills")
          : await invoke<SkillDoc[]>("skill_list");
      setSkillDocs(docs);
      appendEvent(`Skills: ${docs.length}`);
      appendJson("Skills", docs);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill review failed: ${msg}`);
    }
  }

  async function reviewAgents() {
    try {
      const docs =
        transport === "daemon"
          ? await daemonJson<AgentSummary[]>("/agents")
          : await invoke<AgentSummary[]>("agent_list");
      setAgentConfigs(docs);
      appendEvent(`Agents: ${docs.length}`);
      appendJson("Agents", docs);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent review failed: ${msg}`);
    }
  }

  async function showAgentFromOps() {
    const id = requireOpsId("Agent show");
    if (!id) return;
    await showAgent(id);
  }

  async function showAgent(id: string) {
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<AgentConfigFile>(`/agents/${encodeURIComponent(id)}`)
          : await invoke<AgentConfigFile>("agent_show", { id });
      setAgentConfigs((docs) =>
        upsertAgentConfig(docs, withExistingAgentMetadata(docs, doc)),
      );
      appendJson("Agent", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent show failed: ${msg}`);
    }
  }

  function agentConfigFromCurrentControls(id: string, systemPrompt: string): AgentConfigFile {
    return {
      id,
      name: id,
      system_prompt: systemPrompt,
      model: model.trim() || null,
      max_tool_calls: parseOptionalNonNegativeInt(maxToolCalls),
      max_tokens_before_compaction: parseOptionalPositiveInt(
        maxTokensBeforeCompaction,
      ),
      max_compaction_output_tokens: parseOptionalPositiveInt(
        maxCompactionOutputTokens,
      ),
      compaction_guidance: compactionGuidance.trim() || null,
      stop_retention_mode: stopRetentionMode,
      capability_drafts_enabled: enableCapabilityDrafts || null,
      capability_draft_guidance: capabilityDraftGuidance.trim() || null,
      tool_output_mode: rawToolOutput ? "raw" : "interpreted",
      tool_routing_model: toolRoutingModel.trim() || null,
      tool_output_interpretation_model:
        toolOutputInterpretationModel.trim() || null,
      tool_visibility: toolVisibility || null,
      load_memory: loadMemory || null,
      load_skills: loadSkills || null,
      allowed_tool_categories: optionalList(
        parsedCategoryList(allowedToolCategories),
      ),
      allowed_skill_categories: optionalList(
        parsedCategoryList(allowedSkillCategories),
      ),
    };
  }

  async function saveAgentFromOps() {
    const id = requireOpsId("Agent save");
    const systemPrompt = requireOpsValue("Agent save system prompt");
    if (!id || !systemPrompt) return;
    const doc = agentConfigFromCurrentControls(id, systemPrompt);
    try {
      const saved =
        transport === "daemon"
          ? await daemonJson<AgentConfigFile>("/agents", doc)
          : await invoke<AgentConfigFile>("agent_save", { agent: doc });
      setAgentConfigs((docs) => upsertAgentConfig(docs, saved));
      setAgentId(saved.id);
      appendJson("Agent saved", saved);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent save failed: ${msg}`);
    }
  }

  async function exportAgentFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Agent export");
    const path = requireOpsValue("Agent export");
    if (!id || !path) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<AgentConfigFile>(
              `/agents/${encodeURIComponent(id)}/export`,
              { path },
            )
          : await invoke<AgentConfigFile>("agent_export", { id, path });
      setAgentConfigs((docs) =>
        upsertAgentConfig(docs, withExistingAgentMetadata(docs, doc)),
      );
      appendJson("Agent exported", { path, agent: doc });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent export failed: ${msg}`);
    }
  }

  async function importAgentFromOps() {
    const path = requireOpsValue("Agent import");
    if (!path) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<AgentConfigFile>("/agents/import", { path })
          : await invoke<AgentConfigFile>("agent_import", { path });
      setAgentConfigs((docs) => upsertAgentConfig(docs, doc));
      setAgentId(doc.id);
      appendJson("Agent imported", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent import failed: ${msg}`);
    }
  }

  function knownProfileGrantedAgent(id: string) {
    return agentConfigs.some(
      (doc) => doc.id === id && agentSharedProfile(doc) != null,
    );
  }

  async function deleteAgentFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Agent delete");
    if (!id) return;
    if (knownProfileGrantedAgent(id)) {
      appendLine(
        "error",
        `Agent ${id} is shared from another profile; revoke its profile grant instead of deleting it here.`,
      );
      return;
    }
    if (!confirmLocalChange(`Delete agent ${id}`)) return;
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<unknown>(`/agents/${encodeURIComponent(id)}/delete`, {})
          : await invoke<unknown>("agent_delete", { id });
      if ((result as AgentDeleteResult | null)?.deleted === false) {
        appendLine(
          "event",
          `No active-profile agent config was deleted for ${id}.`,
        );
        return;
      }
      setAgentConfigs((docs) => docs.filter((doc) => doc.id !== id));
      if (agentId.trim() === id) {
        setAgentId("");
      }
      appendJson("Agent deleted", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent delete failed: ${msg}`);
    }
  }

  async function showCurrentProfileFromOps() {
    try {
      const profile =
        transport === "daemon"
          ? await daemonJson<ProfileSummary>("/profiles/current")
          : await invoke<ProfileSummary>("profile_current");
      setCurrentProfile(profile);
      setProfileSummaries((profiles) => upsertProfileSummary(profiles, profile));
      appendJson("Current profile", profile);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Current profile failed: ${msg}`);
    }
  }

  async function listProfilesFromOps() {
    try {
      const profiles =
        transport === "daemon"
          ? await daemonJson<ProfileSummary[]>("/profiles")
          : await invoke<ProfileSummary[]>("profile_list");
      setProfileSummaries(profiles);
      appendEvent(`Profiles: ${profiles.length}`);
      appendJson("Profiles", profiles);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Profile list failed: ${msg}`);
    }
  }

  async function showProfileFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Profile show");
    if (!id) return;
    try {
      const profile =
        transport === "daemon"
          ? await daemonJson<ProfileSummary>(`/profiles/${encodeURIComponent(id)}`)
          : await invoke<ProfileSummary>("profile_show", { id });
      setProfileSummaries((profiles) => upsertProfileSummary(profiles, profile));
      if (currentProfile?.id === profile.id) {
        setCurrentProfile(profile);
      }
      appendJson("Profile", profile);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Profile show failed: ${msg}`);
    }
  }

  async function createProfileFromOps() {
    const id = requireOpsId("Profile create");
    if (!id) return;
    const name = opsValue.trim() || null;
    await createProfile(id, name);
  }

  async function createProfile(id: string, name: string | null = null) {
    try {
      const profile =
        transport === "daemon"
          ? await daemonJson<ProfileSummary>("/profiles", { id, name })
          : await invoke<ProfileSummary>("profile_create", { id, name });
      setProfileSummaries((profiles) => upsertProfileSummary(profiles, profile));
      appendJson("Profile created", profile);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Profile create failed: ${msg}`);
    }
  }

  async function deleteProfileFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Profile delete");
    if (!id) return;
    if (id === "main") {
      appendLine("error", "The main profile cannot be deleted.");
      return;
    }
    if (currentProfile?.id === id) {
      appendLine(
        "error",
        `Profile ${id} is active; switch to another profile before deleting it.`,
      );
      return;
    }
    if (!confirmLocalChange(`Delete profile ${id}`)) return;
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<unknown>(`/profiles/${encodeURIComponent(id)}/delete`, {})
          : await invoke<unknown>("profile_delete", { id });
      setProfileSummaries((profiles) => profiles.filter((profile) => profile.id !== id));
      setProfileGrants((grants) =>
        grants.filter((grant) => grant.from_profile !== id && grant.to_profile !== id),
      );
      if (currentProfile?.id === id) {
        setCurrentProfile(null);
      }
      appendJson("Profile deleted", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Profile delete failed: ${msg}`);
    }
  }

  async function listProfileGrantsFromOps(fromProfile?: string) {
    const from_profile = fromProfile ?? null;
    try {
      const grants =
        transport === "daemon"
          ? await daemonJson<ProfileGrant[]>(
              from_profile ? "/profile-grants/list" : "/profile-grants",
              from_profile ? { from_profile } : undefined,
            )
          : await invoke<ProfileGrant[]>("profile_grant_list", {
              fromProfile: from_profile,
            });
      setProfileGrants(grants);
      appendEvent(`Profile grants: ${grants.length}`);
      appendJson("Profile grants", grants);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Profile grants failed: ${msg}`);
    }
  }

  async function listProfileGrantsForOps() {
    const fromProfile = opsId.trim() || null;
    await listProfileGrantsFromOps(fromProfile ?? undefined);
  }

  async function grantProfileFromOps() {
    const toProfile = requireOpsId("Profile grant");
    const payload = parseOpsJsonObject("Profile grant");
    if (!toProfile || !payload) return;
    const kind = payload.kind;
    const resource = payload.resource;
    const fromProfile =
      typeof payload.from_profile === "string"
        ? payload.from_profile
        : typeof payload.fromProfile === "string"
          ? payload.fromProfile
          : null;
    if (!isProfileGrantKind(kind) || typeof resource !== "string" || !resource.trim()) {
      appendLine(
        "error",
        'Profile grant needs Value JSON like { "kind": "memory", "resource": "critic" }.',
      );
      return;
    }
    await grantProfile(toProfile, kind, resource, fromProfile);
  }

  async function grantProfile(
    toProfile: string,
    kind: ProfileGrantKind,
    resource: string,
    fromProfile: string | null = null,
  ) {
    try {
      const grant =
        transport === "daemon"
          ? await daemonJson<ProfileGrant>("/profile-grants", {
              from_profile: fromProfile,
              to_profile: toProfile,
              kind,
              resource,
            })
          : await invoke<ProfileGrant>("profile_grant", {
              fromProfile,
              toProfile,
              kind,
              resource,
            });
      setProfileGrants((grants) => upsertProfileGrant(grants, grant));
      appendJson("Profile grant", grant);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Profile grant failed: ${msg}`);
    }
  }

  async function revokeProfileGrantFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Profile grant revoke");
    if (!id) return;
    if (!confirmLocalChange(`Revoke profile grant ${id}`)) return;
    try {
      const revoked =
        transport === "daemon"
          ? await daemonJson<ProfileGrant>(
              `/profile-grants/${encodeURIComponent(id)}/revoke`,
              {},
            )
          : await invoke<ProfileGrant>("profile_grant_revoke", { id });
      setProfileGrants((grants) => grants.filter((grant) => grant.id !== revoked.id));
      appendJson("Profile grant revoked", revoked);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Profile grant revoke failed: ${msg}`);
    }
  }

  async function listSecretBackendsFromOps() {
    try {
      const backends =
        transport === "daemon"
          ? await daemonJson<SecretBackendDescriptor[]>("/secrets/backends")
          : await invoke<SecretBackendDescriptor[]>("secret_backend_list");
      setSecretBackends(backends);
      appendJson("Secret backends", backends);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Secret backends failed: ${msg}`);
    }
  }

  async function listSecretsFromOps() {
    try {
      const records =
        transport === "daemon"
          ? await daemonJson<SecretRecord[]>("/secrets")
          : await invoke<SecretRecord[]>("secret_list");
      setSecretRecords(records);
      appendEvent(`Secrets: ${records.length}`);
      appendJson("Secrets", records);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Secret list failed: ${msg}`);
    }
  }

  async function showSecretFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Secret show");
    if (!id) return;
    try {
      const record =
        transport === "daemon"
          ? await daemonJson<SecretRecord>(`/secrets/${encodeURIComponent(id)}`)
          : await invoke<SecretRecord>("secret_show", { id });
      setSecretRecords((records) => upsertSecretRecord(records, record));
      appendJson("Secret", record);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Secret show failed: ${msg}`);
    }
  }

  async function setSecretFromOps() {
    const id = requireOpsId("Secret store");
    const value = requireOpsValue("Secret store");
    if (!id || value === null) return;
    try {
      const label = secretLabel.trim() || null;
      const result =
        transport === "daemon"
          ? await daemonJson<SecretWriteResult>("/secrets", { id, value, label })
          : await invoke<SecretWriteResult>("secret_set", { id, value, label });
      setSecretRecords((records) => upsertSecretRecord(records, result.record));
      setSecretStatus(result as unknown as JsonValue);
      setOpsValue("");
      appendJson("Secret stored", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Secret store failed: ${msg}`);
    }
  }

  async function rotateSecretFromOps() {
    const id = requireOpsId("Secret rotate");
    const value = requireOpsValue("Secret rotate");
    if (!id || value === null) return;
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<SecretWriteResult>(
              `/secrets/${encodeURIComponent(id)}/rotate`,
              { value },
            )
          : await invoke<SecretWriteResult>("secret_rotate", { id, value });
      setSecretRecords((records) => upsertSecretRecord(records, result.record));
      setSecretStatus(result as unknown as JsonValue);
      setOpsValue("");
      appendJson("Secret rotated", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Secret rotate failed: ${msg}`);
    }
  }

  async function deleteSecretFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Secret delete");
    if (!id) return;
    if (!confirmLocalChange(`Delete secret ${id}`)) return;
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<JsonValue>(`/secrets/${encodeURIComponent(id)}/delete`, {})
          : await invoke<JsonValue>("secret_delete", { id });
      setSecretRecords((records) => records.filter((record) => record.id !== id));
      setSecretStatus(result);
      appendJson("Secret deleted", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Secret delete failed: ${msg}`);
    }
  }

  async function reviewCapabilities() {
    try {
      const drafts =
        transport === "daemon"
          ? await daemonJson<CapabilityDraft[]>("/capabilities")
          : await invoke<CapabilityDraft[]>("capability_list");
      setCapabilityDrafts(drafts);
      appendEvent(`Capability drafts: ${drafts.length}`);
      appendJson("Capability drafts", drafts);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Capability review failed: ${msg}`);
    }
  }

  async function proposeCapabilityFromOps() {
    const name = requireOpsId("Capability propose");
    const body = requireOpsValue("Capability propose");
    if (!name || !body) return;
    try {
      const draft =
        transport === "daemon"
          ? await daemonJson<CapabilityDraft>("/capabilities/propose", {
              kind: capabilityKind,
              name,
              body,
              created_by: "user",
            })
          : await invoke<CapabilityDraft>("capability_propose", {
              kind: capabilityKind,
              name,
              body,
              guidance: null,
              createdBy: "user",
            });
      setCapabilityDrafts((drafts) => upsertCapabilityDraft(drafts, draft));
      appendJson("Capability draft proposed", draft);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Capability propose failed: ${msg}`);
    }
  }

  async function showCapabilityFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Capability show");
    if (!id) return;
    try {
      const draft =
        transport === "daemon"
          ? await daemonJson<CapabilityDraft>(`/capabilities/${id}`)
          : await invoke<CapabilityDraft>("capability_show", { id });
      setCapabilityDrafts((drafts) => upsertCapabilityDraft(drafts, draft));
      appendJson("Capability draft", draft);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Capability show failed: ${msg}`);
    }
  }

  async function exportCapabilityFromOps(explicitId?: string, explicitPath?: string) {
    const id = explicitId ?? requireOpsId("Capability export");
    const path = explicitPath ?? requireOpsValue("Capability export");
    if (!id || !path) return;
    try {
      const draft =
        transport === "daemon"
          ? await daemonJson<CapabilityDraft>(`/capabilities/${id}/export`, {
              path,
            })
          : await invoke<CapabilityDraft>("capability_export", { id, path });
      setCapabilityDrafts((drafts) => upsertCapabilityDraft(drafts, draft));
      appendJson("Capability draft exported", draft);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Capability export failed: ${msg}`);
    }
  }

  async function importCapabilityFromOps(explicitPath?: string) {
    const path = explicitPath ?? requireOpsValue("Capability import");
    if (!path) return;
    try {
      const draft =
        transport === "daemon"
          ? await daemonJson<CapabilityDraft>("/capabilities/import", { path })
          : await invoke<CapabilityDraft>("capability_import", { path });
      setCapabilityDrafts((drafts) => upsertCapabilityDraft(drafts, draft));
      appendJson("Capability draft imported", draft);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Capability import failed: ${msg}`);
    }
  }

  async function reviewCapabilityDraft(allow: boolean, explicitId?: string) {
    const id =
      explicitId ?? requireOpsId(allow ? "Capability allow" : "Capability reject");
    if (!id) return;
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<CapabilityReviewResult>(
              `/capabilities/${id}/${allow ? "allow" : "reject"}`,
              {},
            )
          : await invoke<CapabilityReviewResult>(
              allow ? "capability_allow" : "capability_reject",
              { id },
            );
      const draft = capabilityDraftFromReviewResult(result);
      setCapabilityDrafts((drafts) => upsertCapabilityDraft(drafts, draft));
      const skill = skillFromCapabilityReviewResult(result);
      if (skill) {
        setSkillDocs((docs) => upsertSkillDoc(docs, skill));
      }
      const adapterPackage = adapterPackageFromCapabilityReviewResult(result);
      if (adapterPackage) {
        setAdapterPackages((packages) =>
          upsertAdapterPackage(packages, adapterPackage),
        );
      }
      appendJson(allow ? "Capability allowed" : "Capability rejected", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Capability review failed: ${msg}`);
    }
  }

  async function deleteCapabilityFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Capability delete");
    if (!id) return;
    if (!confirmLocalChange(`Delete capability draft ${id}`)) return;
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<unknown>(`/capabilities/${id}/delete`, {})
          : await invoke<unknown>("capability_delete", { id });
      setCapabilityDrafts((drafts) => drafts.filter((draft) => draft.id !== id));
      appendJson("Capability draft deleted", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Capability delete failed: ${msg}`);
    }
  }

  async function previewWithSkillsFromOps() {
    const prompt = input.trim() || "preview";
    const options = {
      ...runtimeOptions(),
      load_skills: true,
    };
    setLoadSkills(true);
    try {
      const snapshot =
        transport === "daemon"
          ? await daemonJson<ContextSnapshot>("/preview-context", {
              input: prompt,
              ...options,
            })
          : await invoke<ContextSnapshot>("preview_context", {
              input: prompt,
              options,
            });
      setContextPreview(snapshot);
      setContextPreviewPrompt(prompt);
      setContextCopyStatus("");
      setActiveSection("chat");
      appendEvent(
        `Skills preview: ${snapshot.visible_skills.length} skills, ~${snapshot.estimated_input_tokens} input tokens`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skills preview failed: ${msg}`);
    }
  }

  async function previewWithIngest(artifactId?: string) {
    const selectedId = artifactId?.trim() || opsId.trim();
    const nextInclude = selectedId
      ? Array.from(new Set([...includeIngestIds, selectedId]))
      : includeIngestIds;
    if (!nextInclude.length) {
      appendLine("error", "Ingest preview needs an included artifact or Id.");
      return;
    }
    const prompt = input.trim() || "preview";
    const options = {
      ...runtimeOptions(),
      include_ingest: nextInclude,
    };
    setIncludeIngestIds(nextInclude);
    try {
      const snapshot =
        transport === "daemon"
          ? await daemonJson<ContextSnapshot>("/preview-context", {
              input: prompt,
              ...options,
            })
          : await invoke<ContextSnapshot>("preview_context", {
              input: prompt,
              options,
            });
      setContextPreview(snapshot);
      setContextPreviewPrompt(prompt);
      setContextCopyStatus("");
      setActiveSection("chat");
      appendEvent(
        `Ingest preview: ${snapshot.loaded_artifacts.length} artifacts, ~${snapshot.estimated_input_tokens} input tokens`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest preview failed: ${msg}`);
    }
  }

  async function previewWithIngestFromOps(explicitId?: string) {
    await previewWithIngest(explicitId);
  }

  async function openSkillFromPreview(
    skill: ContextSnapshot["visible_skills"][number],
  ) {
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<SkillDoc>(`/skills/${skill.id}`)
          : await invoke<SkillDoc>("skill_inspect", { id: skill.id });
      setSkillDocs((docs) => upsertSkillDoc(docs, doc));
      setOpsId(skill.id);
      setActiveSection("skills");
      appendEvent(`Opened visible skill for review: ${skill.id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Open skill failed: ${msg}`);
    }
  }

  async function reviewPrompts() {
    const agent = promptScopeAgentId();
    try {
      const prompts =
        transport === "daemon"
          ? agent
            ? await daemonJson<PromptDoc[]>("/prompts/list", { agent_id: agent })
            : await daemonJson<PromptDoc[]>("/prompts")
          : await invoke<PromptDoc[]>("prompt_list", { agentId: agent });
      setPromptDocs(prompts);
      appendEvent(`Saved prompts (${promptScopeLabel(agent)}): ${prompts.length}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt review failed: ${msg}`);
    }
  }

  async function fetchConversationList() {
    return transport === "daemon"
      ? await daemonJson<ConversationDoc[]>("/conversations")
      : await invoke<ConversationDoc[]>("conversation_list");
  }

  async function fetchConversationTree() {
    return transport === "daemon"
      ? await daemonJson<ConversationTreeNode[]>("/conversations/tree")
      : await invoke<ConversationTreeNode[]>("conversation_tree");
  }

  async function fetchConversation(id: string) {
    return transport === "daemon"
      ? await daemonJson<ExpandedConversation>(
          `/conversations/${encodeURIComponent(id)}`,
        )
      : await invoke<ExpandedConversation>("conversation_show", { id });
  }

  async function fetchConversationRecovery(id: string) {
    return transport === "daemon"
      ? await daemonJson<ConversationRecoveryPlan>(
          `/conversations/${encodeURIComponent(id)}/recover`,
        )
      : await invoke<ConversationRecoveryPlan>("conversation_recover", { id });
  }

  async function setConversationPolicy(
    id: string,
    policy: ConversationPolicy,
  ) {
    return transport === "daemon"
      ? await daemonJson<ConversationDoc>(
          `/conversations/${encodeURIComponent(id)}/policy`,
          policy as Record<string, unknown>,
        )
      : await invoke<ConversationDoc>("conversation_set_policy", { id, policy });
  }

  function updateConversationDoc(doc: ConversationDoc) {
    setConversationDocs((docs) =>
      docs.some((conversation) => conversation.id === doc.id)
        ? docs.map((conversation) =>
            conversation.id === doc.id ? doc : conversation,
          )
        : [doc, ...docs],
    );
    setExpandedConversation((expanded) =>
      expanded?.conversation.id === doc.id
        ? { ...expanded, conversation: doc }
        : expanded,
    );
  }

  async function reviewConversations() {
    try {
      const [conversations, tree] = await Promise.all([
        fetchConversationList(),
        fetchConversationTree(),
      ]);
      setConversationDocs(conversations);
      setConversationTree(tree);
      setConversationDeletePlan([]);
      appendEvent(
        `Conversations: ${conversations.length} total, ${tree.length} root branches`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation review failed: ${msg}`);
    }
  }

  async function showConversationFromOps() {
    const id = requireOpsId("Conversation show");
    if (!id) return;
    await showConversation(id);
  }

  async function showConversation(id: string) {
    try {
      const conversation = await fetchConversation(id);
      setExpandedConversation(conversation);
      setOpsId(conversation.conversation.id);
      setConversationId(conversation.conversation.id);
      appendJson("Conversation", conversation);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation show failed: ${msg}`);
    }
  }

  async function recoverConversationFromOps() {
    const id = requireOpsId("Conversation recovery");
    if (!id) return;
    await recoverConversation(id);
  }

  async function recoverConversation(id: string) {
    try {
      const plan = await fetchConversationRecovery(id);
      const suggested = plan.suggested_run;
      setOpsId(suggested.conversation_id || plan.conversation_id);
      setConversationId(suggested.conversation_id || plan.conversation_id);
      setAgentId(plan.agent_id.trim());
      setLoadMemory(suggested.load_memory);
      const compactedContext = suggested.compacted_context?.trim()
        ? suggested.compacted_context
        : "";
      setManualCompactedContext(compactedContext);
      if (compactedContext) {
        setOpsValue(compactedContext);
      }
      appendJson("Conversation recovery plan", plan);
      appendEvent(
        `Recovery settings applied: conversation ${suggested.conversation_id || plan.conversation_id}, ${plan.linked_compactions.length} compactions, ${plan.linked_memories.length} memories, ${suggested.load_memory ? "memory on" : "memory off"}, ${compactedContext ? "compacted context on" : "no compacted context"}`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation recovery failed: ${msg}`);
    }
  }

  function conversationPolicyFromContextControls() {
    const toolCategories = parsedCategoryList(allowedToolCategories);
    const skillCategories = parsedCategoryList(allowedSkillCategories);
    return {
      load_memory: loadMemory,
      generate_memory:
        generateMemoryPolicy === "" ? null : generateMemoryPolicy === "on",
      allowed_tool_categories: toolCategories.length ? toolCategories : null,
      allowed_skill_categories: skillCategories.length ? skillCategories : null,
      capability_drafts_enabled: enableCapabilityDrafts,
      capability_draft_guidance: capabilityDraftGuidance.trim() || null,
      max_tokens_before_compaction: parseOptionalPositiveInt(
        maxTokensBeforeCompaction,
      ),
      max_compaction_output_tokens: parseOptionalPositiveInt(
        maxCompactionOutputTokens,
      ),
      compaction_guidance: compactionGuidance.trim() || null,
    } satisfies ConversationPolicy;
  }

  async function showConversationPolicy(id: string) {
    try {
      const conversation = await fetchConversation(id);
      setExpandedConversation(conversation);
      setOpsId(conversation.conversation.id);
      setConversationId(conversation.conversation.id);
      appendJson("Conversation policy", {
        id: conversation.conversation.id,
        policy: conversation.conversation.policy || {},
      });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation policy show failed: ${msg}`);
    }
  }

  async function saveConversationPolicy(id: string) {
    try {
      const doc = await setConversationPolicy(id, conversationPolicyFromContextControls());
      updateConversationDoc(doc);
      appendJson("Conversation policy saved", doc);
      appendEvent(`Conversation policy saved for ${doc.id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation policy save failed: ${msg}`);
    }
  }

  async function saveSelectedConversationPolicy() {
    if (!expandedConversation) return;
    await saveConversationPolicy(expandedConversation.conversation.id);
  }

  async function clearConversationPolicy(id: string) {
    try {
      const doc = await setConversationPolicy(id, {});
      updateConversationDoc(doc);
      appendJson("Conversation policy cleared", doc);
      appendEvent(`Conversation policy cleared for ${doc.id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation policy clear failed: ${msg}`);
    }
  }

  async function clearSelectedConversationPolicy() {
    if (!expandedConversation) return;
    await clearConversationPolicy(expandedConversation.conversation.id);
  }

  function applyConversationPolicyToContextControls(
    id: string,
    policy: ConversationPolicy,
  ) {
    if (typeof policy.load_memory === "boolean") {
      setLoadMemory(policy.load_memory);
    }
    setGenerateMemoryPolicy(
      typeof policy.generate_memory === "boolean"
        ? policy.generate_memory
          ? "on"
          : "off"
        : "",
    );
    setAllowedToolCategories(
      Array.isArray(policy.allowed_tool_categories)
        ? policy.allowed_tool_categories.join(", ")
        : "",
    );
    setAllowedSkillCategories(
      Array.isArray(policy.allowed_skill_categories)
        ? policy.allowed_skill_categories.join(", ")
        : "",
    );
    if (typeof policy.capability_drafts_enabled === "boolean") {
      setEnableCapabilityDrafts(policy.capability_drafts_enabled);
    }
    setCapabilityDraftGuidance(policy.capability_draft_guidance || "");
    setMaxTokensBeforeCompaction(
      policy.max_tokens_before_compaction
        ? String(policy.max_tokens_before_compaction)
        : "",
    );
    setMaxCompactionOutputTokens(
      policy.max_compaction_output_tokens
        ? String(policy.max_compaction_output_tokens)
        : "",
    );
    setCompactionGuidance(policy.compaction_guidance || "");
    appendEvent(
      `Conversation policy applied to context controls for ${id}`,
    );
  }

  function applySelectedConversationPolicy() {
    const conversation = expandedConversation?.conversation;
    if (!conversation?.policy) return;
    applyConversationPolicyToContextControls(conversation.id, conversation.policy);
  }

  async function applyConversationPolicyFromShortcut(id: string) {
    try {
      const conversation = await fetchConversation(id);
      setExpandedConversation(conversation);
      setOpsId(conversation.conversation.id);
      setConversationId(conversation.conversation.id);
      const policy = conversation.conversation.policy;
      if (!policy) {
        appendEvent(`Conversation ${conversation.conversation.id} has no policy overrides.`);
        return;
      }
      applyConversationPolicyToContextControls(conversation.conversation.id, policy);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation policy apply failed: ${msg}`);
    }
  }

  async function previewConversationDeleteFromOps(recursive: boolean) {
    const id = requireOpsId("Conversation delete preview");
    if (!id) return;
    await previewConversationDelete(id, recursive);
  }

  async function previewConversationDelete(id: string, recursive: boolean) {
    try {
      const plan =
        transport === "daemon"
          ? await daemonJson<string[]>(
              `/conversations/${encodeURIComponent(id)}/delete-plan`,
              { recursive },
            )
          : await invoke<string[]>("conversation_delete_plan", { id, recursive });
      setConversationDeletePlan(plan);
      appendJson("Conversation delete plan", { id, recursive, plan });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation delete preview failed: ${msg}`);
    }
  }

  async function deleteConversationFromOps(recursive: boolean) {
    const id = requireOpsId("Conversation delete");
    if (!id) return;
    await deleteConversation(id, recursive);
  }

  function applyConversationDeleteResult(result: ConversationDeleteResult) {
    const deleted = new Set(result.deleted);
    if (
      expandedConversation &&
      deleted.has(expandedConversation.conversation.id)
    ) {
      setExpandedConversation(null);
    }
    setConversationDocs((docs) =>
      docs.filter((conversation) => !deleted.has(conversation.id)),
    );
    setConversationTree((tree) => filterConversationTree(tree, deleted));
    setConversationDeletePlan([]);
  }

  async function deleteConversation(
    id: string,
    recursive: boolean,
    confirmed = false,
  ) {
    if (
      !confirmed &&
      !confirmLocalChange(`Delete conversation ${id}${recursive ? " recursively" : ""}`)
    ) {
      return;
    }
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<ConversationDeleteResult>(
              `/conversations/${encodeURIComponent(id)}/delete`,
              { recursive },
            )
          : await invoke<ConversationDeleteResult>("conversation_delete", {
              id,
              recursive,
            });
      applyConversationDeleteResult(result);
      appendJson("Conversation deleted", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation delete failed: ${msg}`);
    }
  }

  async function previewConversationDeleteAgent() {
    const agent = agentId.trim();
    if (!agent) {
      appendLine("error", "Set an Agent id before planning agent conversation deletion.");
      return;
    }
    try {
      const plan =
        transport === "daemon"
          ? await daemonJson<string[]>("/conversations/delete-agent-plan", {
              agent_id: agent,
              recursive: false,
            })
          : await invoke<string[]>("conversation_delete_agent_plan", {
              agentId: agent,
              recursive: false,
            });
      setConversationDeletePlan(plan);
      appendJson("Agent conversation delete plan", { agent_id: agent, plan });
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent conversation delete preview failed: ${msg}`);
    }
  }

  async function deleteConversationsForAgent() {
    const agent = agentId.trim();
    if (!agent) {
      appendLine("error", "Set an Agent id before deleting agent conversations.");
      return;
    }
    if (!confirmLocalChange(`Delete all conversations for agent ${agent}`)) {
      return;
    }
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<ConversationDeleteResult>(
              "/conversations/delete-agent",
              { agent_id: agent, recursive: false },
            )
          : await invoke<ConversationDeleteResult>("conversation_delete_agent", {
              agentId: agent,
              recursive: false,
            });
      applyConversationDeleteResult(result);
      appendJson("Agent conversations deleted", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Agent conversation delete failed: ${msg}`);
    }
  }

  async function deleteConversationRangeFromOps() {
    const id = requireOpsId("Conversation range delete");
    const range = parseConversationRangeFromOps();
    if (!id || !range) return;
    await deleteConversationRange(id, range);
  }

  async function deleteConversationRange(
    id: string,
    range: ConversationRange,
    confirmed = false,
  ) {
    if (
      !confirmed &&
      !confirmLocalChange(
        `Delete conversation messages ${range.from}:${range.to} from ${id}`,
      )
    ) {
      return;
    }
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<ConversationDeleteRangeResult>(
              `/conversations/${encodeURIComponent(id)}/delete-range`,
              { from: range.from, to: range.to },
            )
          : await invoke<ConversationDeleteRangeResult>(
              "conversation_delete_range",
              { id, from: range.from, to: range.to },
            );
      setConversationDocs((docs) => [
        result.conversation,
        ...docs.filter((conversation) => conversation.id !== result.id),
      ]);
      const expanded = await fetchConversation(id);
      setExpandedConversation(expanded);
      appendJson("Conversation message range deleted", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation range delete failed: ${msg}`);
    }
  }

  async function reviewIngestion() {
    try {
      const artifacts =
        transport === "daemon"
          ? await daemonJson<IngestionArtifact[]>("/ingest")
          : await invoke<IngestionArtifact[]>("ingest_list");
      setIngestionArtifacts(artifacts);
      appendEvent(`Ingestion artifacts: ${artifacts.length}`);
      appendLine("assistant", JSON.stringify(artifacts, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingestion review failed: ${msg}`);
    }
  }

  async function reviewGeneratedArtifacts() {
    try {
      const artifacts =
        transport === "daemon"
          ? await daemonJson<GeneratedArtifact[]>("/artifacts")
          : await invoke<GeneratedArtifact[]>("artifact_list");
      setGeneratedArtifacts(artifacts);
      appendEvent(`Generated artifacts: ${artifacts.length}`);
      appendLine("assistant", JSON.stringify(artifacts, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Artifact review failed: ${msg}`);
    }
  }

  async function reviewIngestionBackends() {
    try {
      const backends =
        transport === "daemon"
          ? await daemonJson<IngestionBackendDescriptor[]>("/ingest/backends")
          : await invoke<IngestionBackendDescriptor[]>("ingest_backends");
      setIngestionBackends(backends);
      if (!backends.some((backend) => backend.id === ingestBackend)) {
        setIngestBackend(backends[0]?.id ?? "local-v0");
      }
      appendJson("Ingestion backends", backends);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingestion backends failed: ${msg}`);
    }
  }

  async function loadLastTrace() {
    if (!lastRunId) return;
    await loadTraceById(lastRunId);
  }

  async function loadTraceFromOps() {
    const runId = opsId.trim() || lastRunId;
    if (!runId) return;
    await loadTraceById(runId);
  }

  async function guideLastRun(text = input.trim(), clearComposer = true) {
    const runId = activeGuidanceRunId();
    if (!runId) {
      appendLine("error", "Guide needs an active run.");
      return;
    }
    if (!text.trim()) {
      appendLine("error", "Guide needs guidance text.");
      return;
    }
    const guidance = text.trim();
    if (clearComposer) {
      setInput("");
    }
    try {
      if (transport === "daemon") {
        await daemonJson("/guide", {
          run_id: runId,
          text: guidance,
        });
      } else {
        await invoke("guide", { runId, text: guidance });
      }
      appendEvent(`Guidance recorded for ${runId}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Guide failed: ${msg}`);
    }
  }

  function activeGuidanceRunId() {
    return running ? rootRunIdRef.current : null;
  }

  async function scoreLastRun(scoreOverride?: number, targetOverride?: string) {
    if (!lastRunId) return;
    const qualityScore = scoreOverride ?? qualityScoreFromOps();
    if (qualityScore === null) return;
    const target = targetOverride?.trim() || opsId.trim() || "last_answer";
    try {
      if (transport === "daemon") {
        await daemonJson("/score", {
          run_id: lastRunId,
          target,
          score: qualityScore,
        });
      } else {
        await invoke("score", {
          runId: lastRunId,
          target,
          score: qualityScore,
        });
      }
      appendEvent(`Score ${qualityScore}/10 recorded for ${target} on ${lastRunId}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Score failed: ${msg}`);
    }
  }

  async function cancelLastRun(mode: StopRetentionMode | null = stopRetentionMode) {
    if (!lastRunId) return;
    const reason = mode ? `user requested stop; mode=${mode}` : "user requested stop";
    try {
      let result: CancelResult;
      if (transport === "daemon") {
        result = await daemonJson<CancelResult>("/cancel", {
          run_id: lastRunId,
          reason,
          mode,
        });
      } else {
        result = await invoke<CancelResult>("cancel", {
          runId: lastRunId,
          reason,
          mode,
        });
      }
      const retainedCompaction = result.compaction;
      if (retainedCompaction) {
        setCompactionRecords((records) =>
          upsertCompactionRecord(records, retainedCompaction),
        );
      }
      appendEvent(
        result.recorded === "not_active"
          ? `Run ${lastRunId} was already inactive.`
          : `Cancellation recorded for ${lastRunId} (${stopRetentionLabel(mode)}${retainedCompaction ? `, retained ${retainedCompaction.id}` : ""}).`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Cancel failed: ${msg}`);
    }
  }

  async function resumeRun(sourceRunId: string, fromEvent: number | null = null) {
    if (!sourceRunId || running) return;
    setRunning(true);
    setTokensIn(0);
    setTokensOut(0);
    setCostUsd(0);
    setCalls(0);
    setElapsedMs(0);
    setTraceEvents([]);
    setTraceSummary(null);
    setTraceTree(null);
    setCollapsedTraceTreeRuns([]);
    setTraceCompareSummary(null);
    setTraceCompareTree(null);
    setTraceCompareRunId("");
    setApprovals([]);
    terminalEventSeenRef.current = false;
    rootRunIdRef.current = null;
    remoteSeenEventKeysRef.current = new Set();
    runStartedAtRef.current = performance.now();
    appendEvent(`Resuming ${sourceRunId}`);
    try {
      if (transport === "daemon") {
        const started = await daemonJson<RemoteResumeStart>("/resume/start", {
          run_id: sourceRunId,
          from_event: fromEvent,
          demo,
          ...runtimeOptions(),
        });
        const resumedRunId = started.resumed_run_id ?? started.run_id;
        setLastRunId(resumedRunId);
        rootRunIdRef.current = resumedRunId;
        appendEvent(
          `Remote resume started: ${sourceRunId} -> ${resumedRunId}` +
            (started.retained_compaction ? ` (retained ${started.retained_compaction})` : ""),
        );
        await pollRemoteRun(resumedRunId);
        return;
      }
      const result = await invoke<ResumeResult>("resume_run", {
        runId: sourceRunId,
        fromEvent,
        demo,
        options: runtimeOptions(),
      });
      setLastRunId(result.resumed_run_id);
      rootRunIdRef.current = result.resumed_run_id;
      if (!terminalEventSeenRef.current) {
        appendLine("assistant", result.final_output);
        appendEvent(`Resumed ${sourceRunId} as ${result.resumed_run_id}`);
      }
      setRunning(false);
      runStartedAtRef.current = null;
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Resume failed: ${msg}`);
      setRunning(false);
      runStartedAtRef.current = null;
    }
  }

  async function resumeLastRun() {
    const sourceRunId = opsId.trim() || lastRunId || "";
    await resumeRun(sourceRunId);
  }

  async function reviewApprovals(explicitRunId?: string) {
    const runId = explicitRunId ?? lastRunId;
    if (!runId) return;
    try {
      setLastRunId(runId);
      const next = await loadApprovalsForRun(runId);
      setApprovals(next);
      appendEvent(`Approvals for ${runId}: ${next.length}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval review failed: ${msg}`);
    }
  }

  async function loadApprovalsForLastRun() {
    if (!lastRunId) return [];
    return loadApprovalsForRun(lastRunId);
  }

  async function loadApprovalsForRun(runId: string) {
    return transport === "daemon"
      ? await daemonJson<ApprovalRecord[]>(`/approvals/${runId}`)
      : await invoke<ApprovalRecord[]>("approval_list", { runId });
  }

  async function approveFirstPending() {
    if (!lastRunId) return;
    try {
      const next = await loadApprovalsForLastRun();
      setApprovals(next);
      const pending = next.find((approval) => approval.status === "pending");
      if (!pending) {
        appendEvent(`No pending approvals for ${lastRunId}`);
        return;
      }
      await decideApproval(pending.approval_id, true);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval decision failed: ${msg}`);
    }
  }

  async function rejectFirstPending() {
    if (!lastRunId) return;
    try {
      const next = await loadApprovalsForLastRun();
      setApprovals(next);
      const pending = next.find((approval) => approval.status === "pending");
      if (!pending) {
        appendEvent(`No pending approvals for ${lastRunId}`);
        return;
      }
      await decideApproval(pending.approval_id, false);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval rejection failed: ${msg}`);
    }
  }

  async function assessApproval(approvalId: string, explicitRunId?: string) {
    const runId = explicitRunId ?? lastRunId;
    if (!runId) return;
    const controllerAgent = approvalControllerAgent.trim() || undefined;
    try {
      setLastRunId(runId);
      const result =
        transport === "daemon"
          ? await daemonJson<ApprovalAssessResult>(
              `/approvals/${runId}/${approvalId}/assess`,
              { controller_agent: controllerAgent },
            )
          : await invoke<ApprovalAssessResult>("approval_assess", {
              runId,
              approvalId,
              controllerAgent,
            });
      setApprovals((items) =>
        upsertApprovalAssessment(items, approvalId, result.assessment),
      );
      appendEvent(
        `Approval assessment [${approvalId}] ${result.assessment.recommendation ?? result.assessment.status}`,
      );
      appendJson("Approval assessment", result);
      const events = await fetchTraceEvents(runId);
      applyTraceEvents(events);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval assessment failed: ${msg}`);
    }
  }

  async function decideApproval(
    approvalId: string,
    approved: boolean,
    explicitRunId?: string,
  ) {
    const runId = explicitRunId ?? lastRunId;
    if (!runId) return;
    const unlock = approved && approvalUnlock ? approvalUnlock : undefined;
    const signature =
      approved && approvalSignature ? approvalSignature : undefined;
    const controllerAgent =
      approved && approvalControllerAgent.trim()
        ? approvalControllerAgent.trim()
        : undefined;
    try {
      setLastRunId(runId);
      if (transport === "daemon") {
        await daemonJson(`/approvals/${runId}/${approvalId}/decide`, {
          approved,
          unlock,
          signature,
          controller_agent: controllerAgent,
        });
        if (approved) {
          const output = await daemonJson<unknown>(
            `/approvals/${runId}/${approvalId}/execute`,
            { unlock, signature },
          );
          captureDirectToolMetadata(output);
          void refreshVoiceOutputFromToolOutput(output);
          appendJson("Approved tool output", output);
        }
      } else {
        await invoke("approval_decide", {
          runId,
          approvalId,
          approved,
          unlock,
          signature,
          controllerAgent,
        });
        if (approved) {
          const output = await invoke<unknown>("approval_execute", {
            runId,
            approvalId,
            unlock,
            signature,
          });
          captureDirectToolMetadata(output);
          void refreshVoiceOutputFromToolOutput(output);
          appendJson("Approved tool output", output);
        }
      }
      const next = await loadApprovalsForRun(runId);
      setApprovals(next);
      appendEvent(
        approved ? `Approved and executed ${approvalId}` : `Rejected ${approvalId}`,
      );
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval decision failed: ${msg}`);
    }
  }

  async function executeApproval(approvalId: string, explicitRunId?: string) {
    const runId = explicitRunId ?? lastRunId;
    if (!runId) return;
    const unlock = approvalUnlock || undefined;
    const signature = approvalSignature || undefined;
    try {
      setLastRunId(runId);
      const output =
        transport === "daemon"
          ? await daemonJson<unknown>(
              `/approvals/${runId}/${approvalId}/execute`,
              { unlock, signature },
            )
          : await invoke<unknown>("approval_execute", {
              runId,
              approvalId,
              unlock,
              signature,
            });
      captureDirectToolMetadata(output);
      void refreshVoiceOutputFromToolOutput(output);
      appendJson("Approved tool output", output);
      const next = await loadApprovalsForRun(runId);
      setApprovals(next);
      appendEvent(`Executed approved action ${approvalId}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval execution failed: ${msg}`);
    }
  }

  async function runBatchFromInput() {
    const items = input
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    await runBatchItems(items);
  }

  async function runBatchItems(items: string[]) {
    if (!items.length || running) return;
    setInput("");
    appendLine("user", `batch ${items.length} items`);
    try {
      const summary =
        transport === "daemon"
          ? await daemonJson<unknown>("/batch", {
              items,
              demo: "echo",
              ...runtimeOptions(),
            })
          : await invoke<unknown>("batch_run", {
              items,
              demo: "echo",
              options: runtimeOptions(),
            });
      appendLine("assistant", JSON.stringify(summary, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Batch failed: ${msg}`);
    }
  }

  async function resumeBatchFromOps() {
    const batchId = opsId.trim();
    await resumeBatchById(batchId);
  }

  async function resumeBatchById(batchId: string) {
    if (!batchId || running) return;
    try {
      const summary =
        transport === "daemon"
          ? await daemonJson<unknown>("/batch/resume", {
              batch_id: batchId,
              demo: "echo",
              ...runtimeOptions(),
            })
          : await invoke<unknown>("batch_resume", {
              batchId,
              demo: "echo",
              options: runtimeOptions(),
            });
      appendLine("assistant", JSON.stringify(summary, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Batch resume failed: ${msg}`);
    }
  }

  async function reviewAdapters() {
    try {
      const packages =
        transport === "daemon"
          ? await daemonJson<AdapterPackage[]>("/adapters")
          : await invoke<AdapterPackage[]>("adapter_list");
      setAdapterPackages(packages);
      appendEvent(`Adapter manifests: ${packages.length}`);
      appendLine("assistant", JSON.stringify(packages, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter review failed: ${msg}`);
    }
  }

  async function adapterDoctorFromOps() {
    try {
      const report =
        transport === "daemon"
          ? await daemonJson<AdapterDoctorReport>("/adapters/doctor")
          : await invoke<AdapterDoctorReport>("adapter_doctor");
      setAdapterDoctorReport(report);
      appendJson("Adapter doctor", report);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter doctor failed: ${msg}`);
    }
  }

  async function createMemoryFromOps(explicitContent?: string) {
    const content = explicitContent?.trim() || requireOpsValue("Memory create");
    if (!content) return;
    const topics = parsedMemoryTopics();
    const ownerAgentId = agentId.trim() || null;
    try {
      const record =
        transport === "daemon"
          ? await daemonJson<MemoryRecord>("/memory", {
              content,
              user: opsUserMemory,
              agent_id: ownerAgentId,
              topics,
            })
          : await invoke<MemoryRecord>("memory_create", {
              content,
              user: opsUserMemory,
              agentId: ownerAgentId,
              topics,
            });
      setMemoryRecords((records) => upsertMemoryRecord(records, record));
      appendJson("Memory created", record);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory create failed: ${msg}`);
    }
  }

  async function generateMemoryFromOps(explicitText?: string) {
    const text = explicitText?.trim() || requireOpsValue("Memory generate");
    if (!text) return;
    const range = memorySourceRange.trim() || null;
    const topics = parsedMemoryTopics();
    const ownerAgentId = agentId.trim() || null;
    try {
      const records =
        transport === "daemon"
          ? await daemonJson<MemoryRecord[]>("/memory/generate", {
              text,
              user: opsUserMemory,
              range,
              agent_id: ownerAgentId,
              topics,
            })
          : await invoke<MemoryRecord[]>("memory_generate", {
              text,
              user: opsUserMemory,
              range,
              agentId: ownerAgentId,
              topics,
            });
      setMemoryRecords((current) =>
        records.reduce(upsertMemoryRecord, current),
      );
      appendJson("Memory generated", records);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory generate failed: ${msg}`);
    }
  }

  async function generateConversationMemoryFromOps(
    explicit?: { id: string; range: ConversationRange },
  ) {
    const conversationId = explicit?.id || expandedConversation?.conversation.id;
    if (!conversationId) {
      appendLine("error", "Conversation memory generation needs an expanded conversation.");
      return;
    }
    const range =
      explicit?.range || parseConversationRangeFromOps("Conversation memory generation");
    if (!range) return;
    const topics = parsedMemoryTopics();
    const ownerAgentId = agentId.trim() || null;
    try {
      const records =
        transport === "daemon"
          ? await daemonJson<MemoryRecord[]>("/memory/generate-conversation", {
              id: conversationId,
              from: range.from,
              to: range.to,
              user: opsUserMemory,
              agent_id: ownerAgentId,
              topics,
            })
          : await invoke<MemoryRecord[]>("memory_generate_conversation", {
              id: conversationId,
              from: range.from,
              to: range.to,
              user: opsUserMemory,
              agentId: ownerAgentId,
              topics,
            });
      setMemoryRecords((current) =>
        records.reduce(upsertMemoryRecord, current),
      );
      appendJson("Conversation memory generated", records);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Conversation memory generation failed: ${msg}`);
    }
  }

  async function classifyMemoryFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Memory classify");
    if (!id) return;
    const model = memoryClassificationModel.trim() || null;
    const policyAgentId = agentId.trim() || null;
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<MemoryClassifyResult>("/memory/classify", {
              id,
              model,
              agent_id: policyAgentId,
              apply: true,
            })
          : await invoke<MemoryClassifyResult>("memory_classify", {
              id,
              model,
              agentId: policyAgentId,
              apply: true,
            });
      const record = result.record;
      if (record) {
        setMemoryRecords((records) => upsertMemoryRecord(records, record));
      }
      appendJson("Memory classified", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory classify failed: ${msg}`);
    }
  }

  async function editMemoryFromOps(explicitId?: string, explicitContent?: string) {
    const id = explicitId ?? requireOpsId("Memory edit");
    const content = explicitContent?.trim() || requireOpsValue("Memory edit");
    if (!id || !content) return;
    try {
      const record =
        transport === "daemon"
          ? await daemonJson<MemoryRecord>(`/memory/${id}/edit`, { content })
          : await invoke<MemoryRecord>("memory_edit", { id, content });
      setMemoryRecords((records) => upsertMemoryRecord(records, record));
      appendJson("Memory edited", record);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory edit failed: ${msg}`);
    }
  }

  async function deleteMemoryFromOps(explicitId?: string, confirmed = false) {
    const id = explicitId ?? requireOpsId("Memory delete");
    if (!id) return;
    if (!confirmed && !confirmLocalChange(`Delete memory ${id}`)) return;
    try {
      if (transport === "daemon") {
        await daemonJson(`/memory/${id}/delete`, {});
      } else {
        await invoke("memory_delete", { id });
      }
      setMemoryRecords((records) => records.filter((record) => record.id !== id));
      appendEvent(`Memory deleted: ${id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory delete failed: ${msg}`);
    }
  }

  async function rollbackMemoryFromOps(confirmed = false) {
    if (
      !confirmed &&
      !confirmLocalChange(`Rollback ${opsUserMemory ? "user" : "agent"} memory`)
    ) {
      return;
    }
    try {
      if (transport === "daemon") {
        await daemonJson("/memory/rollback", { user: opsUserMemory });
      } else {
        await invoke("memory_rollback", { user: opsUserMemory });
      }
      appendEvent(`Memory rollback complete (${opsUserMemory ? "user" : "agent"})`);
      const records =
        transport === "daemon"
          ? await daemonJson<MemoryRecord[]>("/memory")
          : await invoke<MemoryRecord[]>("memory_list");
      setMemoryRecords(records);
      appendJson("Memory after rollback", records);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine(
        "error",
        msg.includes("not found")
          ? "Memory rollback failed: no rollback snapshot exists yet."
          : `Memory rollback failed: ${msg}`,
      );
    }
  }

  async function savePromptFromOps() {
    const name = requireOpsId("Prompt save");
    const body = requireOpsValue("Prompt save");
    if (!name || !body) return;
    const agent = promptScopeAgentId();
    try {
      const prompt =
        transport === "daemon"
          ? await daemonJson<PromptDoc>("/prompts", { name, body, agent_id: agent })
          : await invoke<PromptDoc>("prompt_save", { name, body, agentId: agent });
      setPromptDocs((docs) => upsertPromptDoc(docs, prompt));
      appendJson("Prompt saved", prompt);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt save failed: ${msg}`);
    }
  }

  async function showPromptFromOps(explicitName?: string) {
    const name = explicitName ?? requireOpsId("Prompt show");
    if (!name) return;
    try {
      const prompt = await fetchPrompt(name);
      setPromptDocs((docs) => upsertPromptDoc(docs, prompt));
      appendJson("Prompt", prompt);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt show failed: ${msg}`);
    }
  }

  async function usePromptFromOps() {
    const name = requireOpsId("Use prompt");
    if (!name) return;
    await usePromptByName(name);
  }

  async function runPromptFromOps() {
    const name = requireOpsId("Run prompt");
    if (!name) return;
    await runPromptByName(name);
  }

  async function runPromptByName(name: string, bodyOverride?: string) {
    if (running) return;
    try {
      const body = bodyOverride ?? (await loadPromptBody(name));
      appendEvent(`Loaded saved prompt for run: ${name}`);
      await runAgentPrompt(body, `/run ${name}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Run prompt failed: ${msg}`);
    }
  }

  async function previewPromptFromOps() {
    const name = requireOpsId("Preview prompt");
    if (!name) return;
    await previewPromptByName(name);
  }

  async function previewPromptByName(name: string, bodyOverride?: string) {
    if (running) return;
    try {
      const body = bodyOverride ?? (await loadPromptBody(name));
      await previewCurrentContext(body);
      setActiveSection("chat");
      appendEvent(`Previewed saved prompt context: ${name}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Preview prompt failed: ${msg}`);
    }
  }

  async function usePromptByName(name: string) {
    try {
      const body = await loadPromptBody(name);
      setInput(body);
      appendEvent(`Loaded saved prompt into composer: ${name}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Use prompt failed: ${msg}`);
    }
  }

  async function deletePromptFromOps() {
    const name = requireOpsId("Prompt delete");
    if (!name) return;
    await deletePromptByName(name);
  }

  async function deletePromptByName(name: string) {
    const agent = promptScopeAgentId();
    if (!confirmLocalChange(`Delete prompt ${name} (${promptScopeLabel(agent)})`)) return;
    try {
      const output =
        transport === "daemon"
          ? agent
            ? await daemonJson<unknown>("/prompts/delete", { name, agent_id: agent })
            : await daemonJson<unknown>(
                `/prompts/${encodeURIComponent(name)}/delete`,
                {},
              )
          : await invoke<unknown>("prompt_delete", { name, agentId: agent });
      setPromptDocs((docs) =>
        docs.filter(
          (prompt) => prompt.name !== name || (prompt.agent_id ?? null) !== agent,
        ),
      );
      appendJson("Prompt deleted", output);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt delete failed: ${msg}`);
    }
  }

  async function listModelsFromOps() {
    try {
      const models =
        transport === "daemon"
          ? await daemonJson<unknown>("/models")
          : await invoke<unknown>("model_list");
      appendJson("Models", models);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model list failed: ${msg}`);
    }
  }

  async function fetchModelProviderDescriptors() {
    return transport === "daemon"
      ? await daemonJson<ModelProviderDescriptor[]>("/model-providers")
      : await invoke<ModelProviderDescriptor[]>("model_provider_list");
  }

  async function refreshModelProviderDescriptors(quiet = false) {
    try {
      const providers = await fetchModelProviderDescriptors();
      setModelProviderDescriptors(providers);
      return providers;
    } catch (err: unknown) {
      if (!quiet) {
        const msg = err instanceof Error ? err.message : String(err);
        appendLine("error", `Model provider list failed: ${msg}`);
      }
      return null;
    }
  }

  async function listModelProvidersFromOps() {
    const providers = await refreshModelProviderDescriptors();
    if (providers) {
      appendJson("Model providers", providers);
    }
  }

  async function modelDoctorFromOps() {
    try {
      const report =
        transport === "daemon"
          ? await daemonJson<ModelDoctorReport>("/models/doctor")
          : await invoke<ModelDoctorReport>("model_doctor");
      appendJson("Model doctor", report);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model doctor failed: ${msg}`);
    }
  }

  async function showModelProviderCatalogFromOps() {
    try {
      const catalog =
        transport === "daemon"
          ? await daemonJson<ModelProviderCatalog | null>("/model-provider-catalog")
          : await invoke<ModelProviderCatalog | null>("model_provider_catalog_show");
      appendJson("Model provider catalog", catalog);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Provider catalog show failed: ${msg}`);
    }
  }

  async function exportModelProviderCatalogFromOps(explicitPath?: string) {
    const path = explicitPath?.trim() || requireOpsValue("Provider catalog export");
    if (!path) return;
    try {
      const catalog =
        transport === "daemon"
          ? await daemonJson<ModelProviderCatalog>("/model-provider-catalog/export", { path })
          : await invoke<ModelProviderCatalog>("model_provider_catalog_export", { path });
      appendJson("Model provider catalog exported", catalog);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Provider catalog export failed: ${msg}`);
    }
  }

  async function importModelProviderCatalogFromOps(
    explicitPath?: string,
    confirmed = false,
  ) {
    const path = explicitPath?.trim() || requireOpsValue("Provider catalog import");
    if (!path) return;
    if (!confirmed && !confirmLocalChange(`Import provider catalog ${path}`)) return;
    try {
      const catalog =
        transport === "daemon"
          ? await daemonJson<ModelProviderCatalog>("/model-provider-catalog/import", { path })
          : await invoke<ModelProviderCatalog>("model_provider_catalog_import", { path });
      appendJson("Model provider catalog imported", catalog);
      void refreshModelProviderDescriptors(true);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Provider catalog import failed: ${msg}`);
    }
  }

  async function showModelMetadataCatalogFromOps() {
    try {
      const catalog =
        transport === "daemon"
          ? await daemonJson<ModelMetadataCatalog | null>("/model-metadata-catalog")
          : await invoke<ModelMetadataCatalog | null>("model_metadata_catalog_show");
      appendJson("Model metadata catalog", catalog);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Metadata catalog show failed: ${msg}`);
    }
  }

  async function exportModelMetadataCatalogFromOps(explicitPath?: string) {
    const path = explicitPath?.trim() || requireOpsValue("Metadata catalog export");
    if (!path) return;
    try {
      const catalog =
        transport === "daemon"
          ? await daemonJson<ModelMetadataCatalog>("/model-metadata-catalog/export", { path })
          : await invoke<ModelMetadataCatalog>("model_metadata_catalog_export", { path });
      appendJson("Model metadata catalog exported", catalog);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Metadata catalog export failed: ${msg}`);
    }
  }

  async function importModelMetadataCatalogFromOps(
    explicitPath?: string,
    confirmed = false,
  ) {
    const path = explicitPath?.trim() || requireOpsValue("Metadata catalog import");
    if (!path) return;
    if (!confirmed && !confirmLocalChange(`Import metadata catalog ${path}`)) return;
    try {
      const catalog =
        transport === "daemon"
          ? await daemonJson<ModelMetadataCatalog>("/model-metadata-catalog/import", { path })
          : await invoke<ModelMetadataCatalog>("model_metadata_catalog_import", { path });
      appendJson("Model metadata catalog imported", catalog);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Metadata catalog import failed: ${msg}`);
    }
  }

  async function showModelFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Model show");
    if (!id) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<unknown>(`/models/${encodeURIComponent(id)}`)
          : await invoke<unknown>("model_show", { id });
      appendJson("Model", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model show failed: ${msg}`);
    }
  }

  async function probeModelFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Model probe");
    if (!id) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<unknown>(`/models/${encodeURIComponent(id)}/probe`)
          : await invoke<unknown>("model_probe", { id });
      appendJson("Model capability probe", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model probe failed: ${msg}`);
    }
  }

  async function saveModelFromOps() {
    const id = requireOpsId("Model save");
    if (!id) return;
    const input = parseOpsJsonObject("Model save");
    if (!input) return;
    const modelDoc = { id, ...input };
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<unknown>("/models", modelDoc)
          : await invoke<unknown>("model_save", { model: modelDoc });
      appendJson("Model saved", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model save failed: ${msg}`);
    }
  }

  async function saveCurrentModelFromControls() {
    if (provider === "fake") {
      appendLine("error", "Choose a non-fake provider before saving model metadata.");
      return;
    }
    const id = model.trim() || defaultModelForProvider(provider);
    const providerOptions = providerOptionsFromControls();
    if (providerOptions === false) return;
    const metadata =
      providerOptions && Object.keys(providerOptions).length
        ? { provider_options: providerOptions }
        : {};
    const modelDoc = {
      id,
      provider,
      api_base_url: supportsApiBaseUrl ? apiBaseUrl.trim() || null : null,
      api_key_env: apiKeyEnv.trim() || defaultApiKeyEnvForProvider(provider),
      allow_missing_api_key: provider === "ollama" || provider === "llama_cpp" ? true : null,
      available_modalities: modelSupportsImage ? ["text", "image"] : [],
      metadata,
    };
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<unknown>("/models", modelDoc)
          : await invoke<unknown>("model_save", { model: modelDoc });
      appendJson("Model saved", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model save failed: ${msg}`);
    }
  }

  async function exportModelFromOps() {
    const id = requireOpsId("Model export");
    if (!id) return;
    const path = requireOpsValue("Model export");
    if (!path) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<unknown>(`/models/${encodeURIComponent(id)}/export`, { path })
          : await invoke<unknown>("model_export", { id, path });
      appendJson("Model exported", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model export failed: ${msg}`);
    }
  }

  async function importModelFromOps() {
    const path = requireOpsValue("Model import");
    if (!path) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<unknown>("/models/import", { path })
          : await invoke<unknown>("model_import", { path });
      appendJson("Model imported", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model import failed: ${msg}`);
    }
  }

  function providerOptionsFromControls(): Record<string, unknown> | false | null {
    const options: Record<string, unknown> = {};
    if (
      !addProviderOption(options, "top_p", providerTopP) ||
      !addProviderOption(options, "top_k", providerTopK) ||
      !addProviderOption(options, "reasoning_effort", providerReasoningEffort) ||
      !addProviderOption(options, "frequency_penalty", providerFrequencyPenalty) ||
      !addProviderOption(options, "presence_penalty", providerPresencePenalty)
    ) {
      return false;
    }
    return Object.keys(options).length ? options : null;
  }

  function addProviderOption(
    options: Record<string, unknown>,
    key: string,
    raw: string,
  ) {
    const trimmed = raw.trim();
    if (!trimmed) {
      return true;
    }
    const descriptor = providerOptionDescriptor(key);
    if (!descriptor && !providerSupportsProviderOption(key)) {
      appendLine("error", `${providerOptionLabel(key)} is not supported by ${provider}.`);
      return false;
    }
    const value = parseProviderOptionValue(key, trimmed, descriptor ?? undefined);
    if (value === false) {
      return false;
    }
    options[key] = value;
    return true;
  }

  function parseProviderOptionValue(
    key: string,
    raw: string,
    descriptor?: ModelProviderOptionDescriptor,
  ): number | string | false {
    const kind = descriptor?.kind ?? fallbackProviderOptionKind(key);
    if (kind === "string") {
      return raw;
    }
    const value = Number(raw);
    if (!Number.isFinite(value)) {
      appendLine("error", `${providerOptionLabel(key)} must be a number.`);
      return false;
    }
    if (kind === "integer" && !Number.isInteger(value)) {
      appendLine("error", `${providerOptionLabel(key)} must be an integer.`);
      return false;
    }
    const min = descriptor?.min ?? fallbackProviderOptionMin(key);
    const max = descriptor?.max ?? fallbackProviderOptionMax(key);
    if (min !== null && value < min) {
      appendLine("error", `${providerOptionLabel(key)} must be at least ${min}.`);
      return false;
    }
    if (max !== null && value > max) {
      appendLine("error", `${providerOptionLabel(key)} must be at most ${max}.`);
      return false;
    }
    return value;
  }

  async function deleteModelFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Model delete");
    if (!id) return;
    if (!confirmLocalChange(`Delete model ${id}`)) return;
    try {
      const output =
        transport === "daemon"
          ? await daemonJson<unknown>(`/models/${encodeURIComponent(id)}/delete`, {})
          : await invoke<unknown>("model_delete", { id });
      appendJson("Model deleted", output);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Model delete failed: ${msg}`);
    }
  }

  async function importSkillFromOps(explicitPath?: string) {
    const path = explicitPath ?? requireOpsValue("Skill import");
    if (!path) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<SkillDoc>("/skills/import", { path })
          : await invoke<SkillDoc>("skill_import_openclaw", { path });
      setSkillDocs((docs) => upsertSkillDoc(docs, doc));
      appendJson("Skill imported into quarantine", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill import failed: ${msg}`);
    }
  }

  async function importSkillDocFromOps(explicitPath?: string) {
    const path = explicitPath ?? requireOpsValue("Skill import doc");
    if (!path) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<SkillDoc>("/skills/import-doc", { path })
          : await invoke<SkillDoc>("skill_import_doc", { path });
      setSkillDocs((docs) => upsertSkillDoc(docs, doc));
      appendJson("Skill document imported into quarantine", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill document import failed: ${msg}`);
    }
  }

  async function showSkillFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Skill show");
    if (!id) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<SkillDoc>(`/skills/${id}`)
          : await invoke<SkillDoc>("skill_inspect", { id });
      setSkillDocs((docs) => upsertSkillDoc(docs, doc));
      appendJson("Skill", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill show failed: ${msg}`);
    }
  }

  async function exportSkillFromOps(explicitId?: string, explicitPath?: string) {
    const id = explicitId ?? requireOpsId("Skill export");
    const path = explicitPath ?? requireOpsValue("Skill export");
    if (!id || !path) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<SkillDoc>(`/skills/${id}/export`, { path })
          : await invoke<SkillDoc>("skill_export", { id, path });
      setSkillDocs((docs) => upsertSkillDoc(docs, doc));
      appendJson("Skill exported", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill export failed: ${msg}`);
    }
  }

  async function setSkillQuarantine(allow: boolean, explicitId?: string) {
    const id =
      explicitId ?? requireOpsId(allow ? "Skill allow" : "Skill quarantine");
    if (!id) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<SkillDoc>(
              `/skills/${id}/${allow ? "allow" : "quarantine"}`,
              {},
            )
          : await invoke<SkillDoc>(allow ? "skill_allow" : "skill_quarantine", {
              id,
            });
      setSkillDocs((docs) => upsertSkillDoc(docs, doc));
      appendJson(allow ? "Skill allowed" : "Skill quarantined", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill update failed: ${msg}`);
    }
  }

  async function ingestPathFromOps(
    explicitPath?: string,
    options: IngestModelOptions = {},
  ) {
    const path = explicitPath?.trim() || requireOpsValue("Ingest add");
    if (!path) return;
    const backend = (options.backend ?? ingestBackend.trim()) || "local-v0";
    const visionModel =
      options.visionModel !== undefined
        ? options.visionModel
        : ingestVisionModel.trim() || null;
    const guardrailModel =
      options.guardrailModel !== undefined
        ? options.guardrailModel
        : ingestGuardrailModel.trim() || null;
    try {
      const artifact = await ingestPath(path, backend, visionModel, guardrailModel);
      setIngestionArtifacts((artifacts) => upsertArtifact(artifacts, artifact));
      appendJson("Ingestion artifact created", artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest failed: ${msg}`);
    }
  }

  async function ingestPath(
    path: string,
    backend: string,
    visionModel: string | null,
    guardrailModel: string | null,
  ) {
    if (transport === "daemon") {
      return normalizeIngestionResponse(
        await daemonJson<IngestionArtifact | IngestionResult>("/ingest", {
          path,
          backend,
          vision_model: visionModel,
          guardrail_model: guardrailModel,
        }),
      );
    }
    return invoke<IngestionArtifact>("ingest_add", {
      path,
      backend,
      visionModel,
      guardrailModel,
    });
  }

  async function probeIngestVisionFromOps(path: string, model: string) {
    try {
      const probe =
        transport === "daemon"
          ? await daemonJson<ModelVisionProbe>("/ingest/probe-vision", {
              path,
              model,
            })
          : await invoke<ModelVisionProbe>("ingest_probe_vision", { path, model });
      appendJson("Ingestion vision probe", probe);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest vision probe failed: ${msg}`);
    }
  }

  async function rerunIngestFromOps(
    explicitId?: string,
    options: IngestModelOptions = {},
  ) {
    const id = explicitId?.trim() || requireOpsId("Ingest rerun");
    if (!id) return;
    await rerunIngestId(id, options);
  }

  async function rerunIngestId(id: string, options: IngestModelOptions = {}) {
    const backend = (options.backend ?? ingestBackend.trim()) || "local-v0";
    const visionModel =
      options.visionModel !== undefined
        ? options.visionModel
        : ingestVisionModel.trim() || null;
    const guardrailModel =
      options.guardrailModel !== undefined
        ? options.guardrailModel
        : ingestGuardrailModel.trim() || null;
    try {
      const artifact =
        transport === "daemon"
          ? await rerunIngestViaDaemon(id, backend, visionModel, guardrailModel)
          : await invoke<IngestionArtifact>("ingest_rerun", {
              id,
              backend,
              visionModel,
              guardrailModel,
            });
      setIngestionArtifacts((artifacts) => upsertArtifact(artifacts, artifact));
      appendJson(`Ingestion artifact rerun with ${backend}`, artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest rerun failed: ${msg}`);
    }
  }

  async function rerunIngestViaDaemon(
    id: string,
    backend: string,
    visionModel: string | null,
    guardrailModel: string | null,
  ) {
    return normalizeIngestionResponse(
      await daemonJson<IngestionArtifact | IngestionResult>(
        `/ingest/${id}/rerun`,
        { backend, vision_model: visionModel, guardrail_model: guardrailModel },
      ),
    );
  }

  async function showIngestFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Ingest show");
    if (!id) return;
    try {
      const artifact =
        transport === "daemon"
          ? await daemonJson<IngestionArtifact>(`/ingest/${id}`)
          : await invoke<IngestionArtifact>("ingest_show", { id });
      setIngestionArtifacts((artifacts) => upsertArtifact(artifacts, artifact));
      appendJson("Ingestion artifact", artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest show failed: ${msg}`);
    }
  }

  async function reviewIngestFindingFromOps(
    explicit?: {
      id: string;
      finding: number;
      decision: IngestionFindingReviewDecision;
      note: string | null;
    },
  ) {
    const id = explicit?.id.trim() || requireOpsId("Review ingest");
    if (!id) return;
    const finding = explicit?.finding ?? Number(ingestFindingIndex);
    if (!Number.isInteger(finding) || finding < 0) {
      appendLine("error", "Finding index must be a zero-based integer.");
      return;
    }
    const decision = explicit?.decision ?? ingestReviewDecision;
    const note = explicit ? explicit.note : ingestReviewNote.trim() || null;
    try {
      const artifact =
        transport === "daemon"
          ? await daemonJson<IngestionArtifact>(`/ingest/${id}/review`, {
              finding,
              decision,
              note,
            })
          : await invoke<IngestionArtifact>("ingest_review", {
              id,
              finding,
              decision,
              note,
            });
      setIngestionArtifacts((artifacts) => upsertArtifact(artifacts, artifact));
      appendJson("Ingestion finding reviewed", artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest review failed: ${msg}`);
    }
  }

  async function showGeneratedArtifactFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Artifact show");
    if (!id) return;
    await showGeneratedArtifact(id);
  }

  async function showGeneratedArtifact(id: string) {
    try {
      const artifact =
        transport === "daemon"
          ? await daemonJson<GeneratedArtifact>(`/artifacts/${encodeURIComponent(id)}`)
          : await invoke<GeneratedArtifact>("artifact_show", { id });
      setGeneratedArtifacts((artifacts) =>
        upsertGeneratedArtifact(artifacts, artifact),
      );
      appendJson("Generated artifact", artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Artifact show failed: ${msg}`);
    }
  }

  async function openGeneratedArtifactFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Artifact open");
    if (!id) return;
    await openGeneratedArtifact(id);
  }

  async function openGeneratedArtifact(id: string) {
    try {
      const artifact =
        transport === "daemon"
          ? await daemonJson<GeneratedArtifact>(
              `/artifacts/${encodeURIComponent(id)}/open`,
              {},
            )
          : await invoke<GeneratedArtifact>("artifact_open", { id });
      setGeneratedArtifacts((artifacts) =>
        upsertGeneratedArtifact(artifacts, artifact),
      );
      appendEvent(`Opened artifact: ${artifact.id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Artifact open failed: ${msg}`);
    }
  }

  async function deleteGeneratedArtifactFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Artifact delete");
    if (!id) return;
    await deleteGeneratedArtifact(id);
  }

  async function deleteGeneratedArtifact(id: string) {
    if (!confirmLocalChange(`Delete generated artifact ${id}`)) return;
    try {
      const artifact =
        transport === "daemon"
          ? await daemonJson<GeneratedArtifact>(
              `/artifacts/${encodeURIComponent(id)}/delete`,
              {},
            )
          : await invoke<GeneratedArtifact>("artifact_delete", { id });
      setGeneratedArtifacts((artifacts) =>
        artifacts.filter((item) => item.id !== artifact.id),
      );
      if (artifactPreview?.artifact.id === artifact.id) {
        setArtifactPreview(null);
      }
      appendEvent(`Deleted artifact: ${artifact.id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Artifact delete failed: ${msg}`);
    }
  }

  async function removeIngestFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Ingest remove");
    if (!id) return;
    if (!confirmLocalChange(`Remove ingestion artifact ${id}`)) return;
    try {
      if (transport === "daemon") {
        await daemonJson(`/ingest/${id}/rm`, {});
      } else {
        await invoke("ingest_rm", { id });
      }
      setIncludeIngestIds((ids) => ids.filter((includedId) => includedId !== id));
      setIngestionArtifacts((artifacts) =>
        artifacts.filter((artifact) => artifact.id !== id),
      );
      appendEvent(`Ingestion artifact removed: ${id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest remove failed: ${msg}`);
    }
  }

  function includeIngestFromOps(explicitId?: string) {
    const id = explicitId?.trim() || requireOpsId("Use ingest");
    if (!id) return;
    includeIngestId(id);
  }

  function includeIngestId(id: string) {
    setIncludeIngestIds((ids) => (ids.includes(id) ? ids : [...ids, id]));
    const artifact = ingestionArtifacts.find((item) => item.id === id);
    if (artifact && hasHighRiskFindings(artifact)) {
      appendEvent(
        `Ingestion artifact ${id} selected; high-risk content will be withheld unless unsafe ingest is enabled.`,
      );
      return;
    }
    appendEvent(`Ingestion artifact will be included in context: ${id}`);
  }

  function toggleUnsafeIngest(checked: boolean) {
    if (
      checked &&
      !window.confirm(
        "Allow flagged ingestion content into model context? Review the artifact first.",
      )
    ) {
      appendEvent("Unsafe ingest override cancelled.");
      return;
    }
    setAllowUnsafeIngest(checked);
    appendEvent(
      checked
        ? "Unsafe ingest override enabled for this run."
        : "Unsafe ingest override disabled.",
    );
  }

  function clearIncludedIngest() {
    setIncludeIngestIds([]);
    setAllowUnsafeIngest(false);
    appendEvent("Cleared ingestion artifacts from run context.");
  }

  async function importAdapterFromOps(explicitPath?: string) {
    const path = explicitPath ?? requireOpsValue("Adapter import");
    if (!path) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<AdapterPackage>("/adapters/import", { path })
          : await invoke<AdapterPackage>("adapter_import", { path });
      setAdapterPackages((packages) => upsertAdapterPackage(packages, manifest));
      appendJson("Adapter imported into quarantine", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter import failed: ${msg}`);
    }
  }

  async function importAdapterManifestFromOps(explicitPath?: string) {
    const path = explicitPath ?? requireOpsValue("Adapter manifest import");
    if (!path) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<AdapterPackage>("/adapters/import-manifest", { path })
          : await invoke<AdapterPackage>("adapter_import_manifest", { path });
      setAdapterPackages((packages) => upsertAdapterPackage(packages, manifest));
      appendJson("Adapter manifest imported into quarantine", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter manifest import failed: ${msg}`);
    }
  }

  async function showAdapterFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Adapter show");
    if (!id) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<AdapterPackage>(`/adapters/${id}`)
          : await invoke<AdapterPackage>("adapter_show", { id });
      setAdapterPackages((packages) => upsertAdapterPackage(packages, manifest));
      appendJson("Adapter manifest", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter show failed: ${msg}`);
    }
  }

  async function exportAdapterFromOps(explicitId?: string, explicitPath?: string) {
    const id = explicitId ?? requireOpsId("Adapter export");
    const path = explicitPath ?? requireOpsValue("Adapter export");
    if (!id || !path) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<AdapterPackage>(`/adapters/${id}/export`, { path })
          : await invoke<AdapterPackage>("adapter_export", { id, path });
      setAdapterPackages((packages) => upsertAdapterPackage(packages, manifest));
      appendJson("Adapter manifest exported", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter export failed: ${msg}`);
    }
  }

  async function installAdapterSkillFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Adapter install skill");
    if (!id) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<SkillDoc>(`/adapters/${id}/install-skill`, {})
          : await invoke<SkillDoc>("adapter_install_skill", { id });
      setSkillDocs((docs) => upsertSkillDoc(docs, doc));
      appendJson("Adapter installed as quarantined skill", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter skill install failed: ${msg}`);
    }
  }

  async function setAdapterQuarantine(allow: boolean, explicitId?: string) {
    const id = explicitId ?? requireOpsId(allow ? "Adapter allow" : "Adapter quarantine");
    if (!id) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<AdapterPackage>(
              `/adapters/${id}/${allow ? "allow" : "quarantine"}`,
              {},
            )
          : await invoke<AdapterPackage>(
              allow ? "adapter_allow" : "adapter_quarantine",
              { id },
            );
      setAdapterPackages((packages) => upsertAdapterPackage(packages, manifest));
      appendJson(allow ? "Adapter allowed" : "Adapter quarantined", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter update failed: ${msg}`);
    }
  }

  async function exportBundleFromOps(explicitPath?: string) {
    const path = explicitPath?.trim() || requireOpsValue("Bundle export");
    if (!path) return;
    await exportBundleToPath(path);
  }

  async function backupBundleNow() {
    const path = defaultBundlePath();
    setOpsValue(path);
    await exportBundleToPath(path, "Backup exported");
  }

  async function exportBundleToPath(path: string, label = "Bundle exported") {
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<BundleManifest>("/bundles/export", { path })
          : await invoke<BundleManifest>("bundle_export", { path });
      setBundleStatus({ operation: "exported", path, manifest });
      appendEvent(`${label}: ${path}`);
      appendJson("Bundle manifest", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Bundle export failed: ${msg}`);
    }
  }

  async function importBundleFromOps(explicitPath?: string, confirmed = false) {
    const path = explicitPath?.trim() || requireOpsValue("Bundle import");
    if (!path) return;
    if (!confirmed && !confirmLocalChange(`Import bundle ${path}`)) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<BundleManifest>("/bundles/import", { path })
          : await invoke<BundleManifest>("bundle_import", { path });
      setBundleStatus({ operation: "imported", path, manifest });
      appendJson("Bundle imported", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Bundle import failed: ${msg}`);
    }
  }

  async function storageReportFromOps() {
    try {
      const report =
        transport === "daemon"
          ? await daemonJson<StorageReport>("/storage")
          : await invoke<StorageReport>("storage_report");
      setStorageReport(report);
      appendEvent(
        `Storage report: ${formatBytes(report.total_bytes)}, ${report.total_files} files, ${report.total_directories} directories`,
      );
      appendJson("Storage report", report);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Storage report failed: ${msg}`);
    }
  }

  async function storagePruneCacheFromOps(
    apply: boolean,
    explicitRetentionDays?: number,
  ) {
    const retentionDays =
      explicitRetentionDays ?? retentionDaysFromOps("Storage cache retention");
    if (retentionDays == null) return;
    if (apply && !confirmLocalChange(`Delete cache files older than ${retentionDays} day(s)`)) {
      return;
    }
    try {
      const result =
        transport === "daemon"
          ? await daemonJson<StorageRetentionResult>("/storage/prune-cache", {
              retention_days: retentionDays,
              apply,
            })
          : await invoke<StorageRetentionResult>("storage_prune_cache", {
              retentionDays,
              apply,
            });
      setStoragePruneResult(result);
      appendJson(apply ? "Storage cache pruned" : "Storage cache prune plan", result);
      if (apply) {
        await storageReportFromOps();
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Storage cache prune failed: ${msg}`);
    }
  }

  async function listBridgeDeliveriesFromOps() {
    if (transport !== "daemon") {
      appendLine("error", "Bridge deliveries are available over daemon transport.");
      return;
    }
    try {
      const result =
        await daemonJson<BridgeDeliveryListResponse>("/bridges/deliveries");
      const deliveries = result.deliveries ?? [];
      setBridgeDeliveries(deliveries);
      appendEvent(`Bridge deliveries: ${deliveries.length} pending`);
      appendJson("Bridge deliveries", result);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Bridge deliveries failed: ${msg}`);
    }
  }

  async function retryBridgeDeliveryFromOps(explicitId?: string) {
    const id = explicitId ?? requireOpsId("Bridge delivery retry");
    if (!id) return;
    if (transport !== "daemon") {
      appendLine("error", "Bridge delivery retry is available over daemon transport.");
      return;
    }
    try {
      const result = await daemonJson<JsonValue>(
        `/bridges/deliveries/${id}/retry`,
        {},
      );
      setBridgeDeliveryResult(result);
      appendJson("Bridge delivery retry", result);
      await listBridgeDeliveriesFromOps();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Bridge delivery retry failed: ${msg}`);
    }
  }

  async function retryAllBridgeDeliveriesFromOps() {
    if (transport !== "daemon") {
      appendLine("error", "Bridge delivery retry is available over daemon transport.");
      return;
    }
    try {
      const result = await daemonJson<JsonValue>(
        "/bridges/deliveries/retry-all",
        {},
      );
      setBridgeDeliveryResult(result);
      appendJson("Bridge delivery retry all", result);
      await listBridgeDeliveriesFromOps();
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Bridge delivery retry-all failed: ${msg}`);
    }
  }

  function applySlashCommand(command: string) {
    setInput(command);
    setSlashCommandIndex(0);
    setSlashCommandDismissed(true);
  }

  function updateComposerInput(value: string) {
    setInput(value);
    setSlashCommandDismissed(false);
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      if (e.shiftKey) {
        if (!running) {
          void previewCurrentContext();
        }
        return;
      }
      void submit();
      return;
    }
    if (slashCommandItems.length) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSlashCommandIndex((index) => (index + 1) % slashCommandItems.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSlashCommandIndex(
          (index) =>
            (index - 1 + slashCommandItems.length) % slashCommandItems.length,
        );
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        const selected = activeSlashCommand ?? slashCommandItems[0];
        if (selected) {
          applySlashCommand(selected.command);
        }
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        setSlashCommandDismissed(true);
      }
    }
  }

  function previewJson(value: unknown) {
    return JSON.stringify(value, null, 2);
  }

  function compactJson(value: unknown) {
    return JSON.stringify(value);
  }

  function formatDuration(ms: number) {
    if (ms < 1000) {
      return `${ms}ms`;
    }
    if (ms < 60_000) {
      return `${(ms / 1000).toFixed(1)}s`;
    }
    const minutes = Math.floor(ms / 60_000);
    const seconds = Math.floor((ms % 60_000) / 1000);
    return `${minutes}m ${seconds}s`;
  }

  function formatUnixMs(ms: number) {
    if (!Number.isFinite(ms) || ms <= 0) {
      return "unknown";
    }
    return new Date(ms).toLocaleString();
  }

  function jsonObject(value: JsonValue | undefined) {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return null;
    }
    return value as { [key: string]: JsonValue };
  }

  function deliveryStatusLabel(value: JsonValue) {
    const record = jsonObject(value);
    if (!record) {
      return "unknown";
    }
    const delivered = record.delivered;
    const attempts = record.attempts;
    const status =
      delivered === true ? "delivered" : delivered === false ? "failed" : "pending";
    return typeof attempts === "number" ? `${status}, ${attempts} attempts` : status;
  }

  function bridgeRetrySummary(value: JsonValue) {
    const record = jsonObject(value);
    if (!record) {
      return "retry result";
    }
    if (typeof record.retried === "number") {
      return `retried ${record.retried}`;
    }
    const attempted = record.attempted;
    const resolved = record.resolved;
    const remaining = record.remaining;
    if (
      typeof attempted === "number" &&
      typeof resolved === "number" &&
      typeof remaining === "number"
    ) {
      return `attempted ${attempted}, resolved ${resolved}, remaining ${remaining}`;
    }
    const delivery = jsonObject(record.delivery);
    if (delivery) {
      return deliveryStatusLabel(delivery);
    }
    return deliveryStatusLabel(value);
  }

  function formatCost(cost: number | null) {
    return cost === null ? "n/a" : `$${cost.toFixed(6)}`;
  }

  function stopRetentionLabel(mode: StopRetentionMode | null) {
    if (mode === null) return "use configured stopped-context default";
    return mode === "discard" ? "discard stopped context" : "summarise stopped context";
  }

  function parseStopRetentionMode(value: string): StopRetentionMode | "default" | null {
    if (value === "default" || value === "config" || value === "auto") return "default";
    if (value === "discard" || value === "off") return "discard";
    if (value === "summarise" || value === "summarize" || value === "on") {
      return "summarise";
    }
    return null;
  }

  function buildCompactionDraft(guidance: string) {
    const recentLines = transcript
      .filter((line) => line.kind === "user" || line.kind === "assistant")
      .slice(-16)
      .map((line) => `- ${prefixFor(line.kind)}: ${compactPreview(line.text, 360)}`);
    const sections = [
      "# Manual Compaction Draft",
      `Created: ${new Date().toISOString()}`,
      `Agent: ${activeAgentLabel()}`,
      `Run: ${lastRunId ?? "none"}`,
    ];
    if (guidance.trim()) {
      sections.push(`Guidance: ${guidance.trim()}`);
    }
    sections.push("## Retained Conversation", recentLines.join("\n") || "- No conversation messages yet.");
    sections.push(
      "## Retention Policy",
      "Preserve user goals, constraints, decisions, tool outcomes, unresolved questions, and next actions. Drop incidental UI logs and duplicate status messages.",
    );
    return sections.join("\n\n");
  }

  function estimateLocalTokens(text: string) {
    return Math.max(1, Math.ceil(text.trim().length / 4));
  }

  function compactionStatus() {
    if (!manualCompactedContext.trim()) {
      return "No manual compacted context is active.";
    }
    return `Manual compacted context active: ~${estimateLocalTokens(manualCompactedContext)} tokens.`;
  }

  function conversationPolicySummary(policy?: ConversationPolicy | null) {
    const parts: string[] = [];
    if (typeof policy?.load_memory === "boolean") {
      parts.push(`memory ${policy.load_memory ? "on" : "off"}`);
    }
    if (typeof policy?.generate_memory === "boolean") {
      parts.push(`memory generation ${policy.generate_memory ? "on" : "off"}`);
    }
    if (policy?.allowed_tool_categories?.length) {
      parts.push(`tools ${policy.allowed_tool_categories.join(", ")}`);
    }
    if (policy?.allowed_skill_categories?.length) {
      parts.push(`skills ${policy.allowed_skill_categories.join(", ")}`);
    }
    if (typeof policy?.capability_drafts_enabled === "boolean") {
      parts.push(`draft tool ${policy.capability_drafts_enabled ? "on" : "off"}`);
    }
    if (policy?.capability_draft_guidance?.trim()) {
      parts.push(
        `draft guidance ${previewText(policy.capability_draft_guidance, 80)}`,
      );
    }
    if (policy?.max_tokens_before_compaction) {
      parts.push(`compact at ${policy.max_tokens_before_compaction}`);
    }
    if (policy?.max_compaction_output_tokens) {
      parts.push(`output ${policy.max_compaction_output_tokens}`);
    }
    if (policy?.compaction_guidance?.trim()) {
      parts.push(`guidance ${previewText(policy.compaction_guidance, 80)}`);
    }
    return parts.length ? parts.join("; ") : "default policy";
  }

  function compactionMetrics(snapshot: ContextSnapshot): CompactionMetrics | null {
    const compacted = snapshot.compacted?.trim();
    if (!compacted) {
      return null;
    }
    const compactedTokens = estimateLocalTokens(compacted);
    const visibleConversationTokens = estimateLocalTokens(
      previewJson(snapshot.conversation),
    );
    const currentTokens = compactedTokens + visibleConversationTokens;
    const originalTokens = compactionAttributeNumber(compacted, "original_tokens");
    const savedTokens =
      originalTokens === null ? null : Math.max(0, originalTokens - currentTokens);
    return {
      mode: isAutoCompactionSnapshot(snapshot) ? "auto" : "manual",
      originalTokens,
      threshold: compactionAttributeNumber(compacted, "threshold"),
      maxOutputTokens: compactionAttributeNumber(compacted, "max_output_tokens"),
      compactedTokens,
      visibleConversationTokens,
      currentTokens,
      savedTokens,
    };
  }

  function compactionAttributeNumber(text: string, name: string) {
    const match = text.match(new RegExp(`${name}="([^"]+)"`));
    if (!match) {
      return null;
    }
    const value = Number(match[1]);
    return Number.isFinite(value) ? Math.max(0, Math.round(value)) : null;
  }

  function isAutoCompactionSnapshot(snapshot: ContextSnapshot) {
    return snapshot.provenance.some(
      (record) =>
        record.fragment === "compacted_context" &&
        record.source === "agent.context_policy.auto_compaction",
    );
  }

  function compactionCardValue(metrics: CompactionMetrics) {
    if (metrics.savedTokens !== null) {
      return `~${metrics.savedTokens} tokens saved`;
    }
    return `~${metrics.compactedTokens} compacted tokens`;
  }

  function compactionCardDetail(metrics: CompactionMetrics) {
    const mode = metrics.mode === "auto" ? "Auto" : "Manual";
    const threshold =
      metrics.threshold === null ? "" : `; threshold ~${metrics.threshold}`;
    const output =
      metrics.maxOutputTokens === null
        ? ""
        : `; output cap ~${metrics.maxOutputTokens}`;
    const original =
      metrics.originalTokens === null
        ? "original size unavailable"
        : `original ~${metrics.originalTokens}`;
    return `${mode} compaction: ${original}; compacted ~${metrics.compactedTokens}; visible messages ~${metrics.visibleConversationTokens}${threshold}${output}.`;
  }

  function compactionSavingsLabel(snapshot: ContextSnapshot) {
    const metrics = compactionMetrics(snapshot);
    if (!metrics) {
      return "";
    }
    if (metrics.savedTokens !== null) {
      return `~${metrics.savedTokens} tokens saved`;
    }
    return `~${metrics.compactedTokens} compacted tokens`;
  }

  function compactionReviewBeforeText(snapshot: ContextSnapshot) {
    const review = snapshot.compaction_review;
    if (!review) {
      return "";
    }
    const lines = [...review.before_messages];
    if (review.withheld_before_messages > 0) {
      lines.push(
        `[${review.withheld_before_messages} message${review.withheld_before_messages === 1 ? "" : "s"} withheld by secret-pattern guardrail]`,
      );
    }
    return lines.join("\n") || "(none)";
  }

  function compactionReviewAfterText(snapshot: ContextSnapshot) {
    const review = snapshot.compaction_review;
    if (!review) {
      return "";
    }
    const visible = review.visible_messages.length
      ? `\n\nVisible messages:\n${review.visible_messages.join("\n")}`
      : "";
    return `Compacted context:\n${review.compacted_context}${visible}`;
  }

  function compactionReviewLabel(snapshot: ContextSnapshot) {
    const review = snapshot.compaction_review;
    if (!review) {
      return "";
    }
    const before = Math.max(0, Math.round(review.before_tokens));
    const after = Math.max(0, Math.round(review.after_tokens));
    const saved = Math.max(0, before - after);
    const prefix = review.mode === "auto" ? "Auto" : "Manual";
    return `${prefix} compaction review: ~${before} before, ~${after} after, ~${saved} saved.`;
  }

  function currentUsageSummary() {
    return [
      `Current usage: tokens ${tokensIn}/${tokensOut}`,
      `cost ${formatCost(costUsd)}`,
      `time ${formatDuration(elapsedMs)}`,
      `tool calls ${calls}/${effectiveMaxToolCalls}`,
      `remaining ${remainingToolCalls}`,
    ].join(", ");
  }

  function traceUsageSummary(summary: TraceSummary) {
    return [
      `Trace usage: tokens ${summary.tokens_in}/${summary.tokens_out}`,
      `cost ${formatCost(summary.cost_usd)}`,
      `time ${summary.duration_ms === null ? "n/a" : formatDuration(summary.duration_ms)}`,
      `${summary.llm_calls} LLM calls`,
      `${summary.tool_calls} tool calls`,
      `${summary.hooks} hooks`,
      `${summary.hook_failures} hook failures`,
      `${summary.events} events`,
    ].join(", ");
  }

  function formatBytes(bytes: number) {
    if (bytes < 1024) {
      return `${bytes} B`;
    }
    const units = ["KB", "MB", "GB", "TB"];
    let value = bytes / 1024;
    let unitIndex = 0;
    while (value >= 1024 && unitIndex < units.length - 1) {
      value /= 1024;
      unitIndex += 1;
    }
    return `${value.toFixed(value >= 10 ? 1 : 2)} ${units[unitIndex]}`;
  }

  function estimatedPreviewInputCost(snapshot: ContextSnapshot) {
    const rate = parseOptionalNonNegativeFloat(inputCostPerMillion);
    if (rate === null) {
      return null;
    }
    return (snapshot.estimated_input_tokens * rate) / 1_000_000;
  }

  function toolBudgetMode(): AgentMode {
    const parsed = parseOptionalNonNegativeInt(maxToolCalls);
    if (parsed === 0) return "answer";
    if (parsed === 1) return "action";
    if (parsed === null || parsed === CALLS_MAX) return "workflow";
    return "custom";
  }

  function setAgentMode(mode: AgentMode) {
    if (mode === "answer") {
      setMaxToolCalls("0");
      return;
    }
    if (mode === "action") {
      setMaxToolCalls("1");
      return;
    }
    if (mode === "workflow") {
      setMaxToolCalls("");
    }
  }

  function runReadinessCards(): ContextReviewCard[] {
    const modelName = model.trim() || defaultModelForProvider(provider);
    const includedHighRisk = includeIngestIds.filter((id) => {
      const artifact = ingestionArtifacts.find((item) => item.id === id);
      return artifact ? hasHighRiskFindings(artifact) : false;
    }).length;
    const externalTone =
      includedHighRisk && allowUnsafeIngest
        ? "danger"
        : allowUnsafeIngest || includedHighRisk
          ? "warning"
          : includeIngestIds.length
            ? "ok"
            : "neutral";
    const safetyTone =
      enableShell && !requireApproval ? "warning" : requireApproval ? "ok" : "neutral";
    const hasConversationContext = Boolean(conversationId.trim());
    const preparationEnabled =
      enablePromptRefinement ||
      Boolean(manualCompactedContext.trim()) ||
      hasConversationContext;

    return [
      {
        title: "Agent",
        value: activeAgentLabel(),
        detail: `${provider} provider / ${modelName}`,
        tone: "neutral",
      },
      {
        title: "Tool Budget",
        value:
          effectiveMaxToolCalls === 0
            ? "Answer only"
            : `${effectiveMaxToolCalls} calls max`,
        detail: `${toolVisibility || "config"} visibility; ${remainingToolCalls} remaining now.`,
        tone:
          effectiveMaxToolCalls === 0
            ? "neutral"
            : effectiveMaxToolCalls === 1
              ? "ok"
              : "warning",
      },
      {
        title: "Safety",
        value: requireApproval ? "Approval gate on" : "Auto-approve on",
        detail: enableShell
          ? requireApproval
            ? "Shell access is enabled and gated."
            : "Shell access is enabled without a pause."
          : "Shell access is disabled.",
        tone: safetyTone,
      },
      {
        title: "Context Sources",
        value: `${loadMemory ? "Memory on" : "Memory off"} / ${loadSkills ? "Skills on" : "Skills off"}`,
        detail: `${manualCompactedContext.trim() ? "Compacted context active" : "No compacted context"}; ${hasConversationContext ? `conversation ${conversationId.trim()}` : "no conversation branch"}; ${includeIngestIds.length} ingest artifacts selected.`,
        tone:
          loadMemory || loadSkills || manualCompactedContext.trim() || hasConversationContext
            ? "ok"
            : "neutral",
      },
      {
        title: "External Content",
        value: includeIngestIds.length
          ? `${includeIngestIds.length} artifacts`
          : "No artifacts",
        detail: includedHighRisk
          ? `${includedHighRisk} high-risk artifacts; unsafe override ${allowUnsafeIngest ? "on" : "off"}.`
          : "Prompt-injection guardrail is blocking unsafe content by default.",
        tone: externalTone,
      },
      {
        title: "Prompt Prep",
        value: preparationEnabled ? "Preprocessing active" : "Direct prompt",
        detail: `${enablePromptRefinement ? "Refinement on" : "Refinement off"}; output ${rawToolOutput ? "raw" : "interpreted"}.`,
        tone: preparationEnabled ? "ok" : "neutral",
      },
    ];
  }

  function descriptorForProvider(value: Provider) {
    return modelProviderDescriptors.find((descriptor) => descriptor.id === value);
  }

  function runtimeOptionDescriptor(key: string) {
    return selectedProviderDescriptor?.option_schema.find(
      (option) => option.target === "runtime" && option.key === key,
    );
  }

  function providerOptionDescriptor(key: string) {
    return selectedProviderDescriptor?.option_schema.find(
      (option) => option.target === "provider_options" && option.key === key,
    );
  }

  function providerOptionSchema() {
    return (
      selectedProviderDescriptor?.option_schema.filter(
        (option) => option.target === "provider_options",
      ) ?? []
    );
  }

  function providerSupportsRuntimeOption(key: string) {
    if (provider === "fake") {
      return false;
    }
    if (selectedProviderDescriptor?.option_schema.length) {
      return Boolean(runtimeOptionDescriptor(key));
    }
    return key === "api_base_url"
      ? provider === "rig" || provider === "ollama" || provider === "llama_cpp"
      : key === "api_key_env";
  }

  function providerSupportsProviderOption(key: string) {
    if (provider === "fake") {
      return false;
    }
    if (selectedProviderDescriptor?.option_schema.length) {
      return Boolean(providerOptionDescriptor(key));
    }
    if (key === "reasoning_effort" || key === "frequency_penalty" || key === "presence_penalty") {
      return provider === "rig";
    }
    if (key === "top_k") {
      return provider !== "anthropic";
    }
    return key === "top_p";
  }

  function providerOptionLabel(key: string) {
    return providerOptionDescriptor(key)?.label ?? runtimeOptionDescriptor(key)?.label ?? key;
  }

  function fallbackProviderOptionKind(key: string) {
    return key === "reasoning_effort" ? "string" : key === "top_k" ? "integer" : "number";
  }

  function fallbackProviderOptionMin(key: string) {
    if (key === "top_p") return 0;
    if (key === "top_k") return 1;
    if (key === "frequency_penalty" || key === "presence_penalty") return -2;
    return null;
  }

  function fallbackProviderOptionMax(key: string) {
    if (key === "top_p") return 1;
    if (key === "frequency_penalty" || key === "presence_penalty") return 2;
    return null;
  }

  function defaultModelForProvider(value: Provider) {
    const descriptor = descriptorForProvider(value);
    if (descriptor?.default_model) {
      return descriptor.default_model;
    }
    switch (value) {
      case "fake":
        return "fake-model";
      case "ollama":
        return "llama3.1";
      case "llama_cpp":
        return "local-model";
      case "anthropic":
        return "claude-sonnet-4-5";
      case "gemini":
        return "gemini-2.5-flash";
      case "rig":
        return "gpt-4o-mini";
    }
  }

  function defaultApiKeyEnvForProvider(value: Provider) {
    const descriptor = descriptorForProvider(value);
    if (descriptor?.api_key_env) {
      return descriptor.api_key_env;
    }
    switch (value) {
      case "anthropic":
        return "ANTHROPIC_API_KEY";
      case "gemini":
        return "GEMINI_API_KEY";
      case "ollama":
        return "OLLAMA_API_KEY";
      case "llama_cpp":
        return "LLAMA_CPP_API_KEY";
      case "fake":
      case "rig":
        return "OPENAI_API_KEY";
    }
  }

  function defaultApiBaseUrlForProvider(value: Provider) {
    const descriptor = descriptorForProvider(value);
    if (descriptor?.api_base_url) {
      return descriptor.api_base_url;
    }
    switch (value) {
      case "ollama":
        return "http://127.0.0.1:11434/v1";
      case "llama_cpp":
        return "http://127.0.0.1:8080/v1";
      case "fake":
      case "rig":
      case "anthropic":
      case "gemini":
        return "";
    }
  }

  function fileName(path: string) {
    const parts = path.split(/[\\/]/);
    return parts[parts.length - 1] || path;
  }

  function ingestionCompatibilityMeta(item: {
    optional_tools?: string[];
    model_requirements?: string[];
  }) {
    return [
      item.optional_tools?.length
        ? `tools: ${item.optional_tools.join(", ")}`
        : null,
      item.model_requirements?.length
        ? `models: ${item.model_requirements.join(", ")}`
        : null,
    ]
      .filter(Boolean)
      .join(" / ");
  }

  function defaultBundlePath() {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    const profile = (currentProfile?.id || "active-profile")
      .replace(/[^a-z0-9._-]+/gi, "-")
      .replace(/^-+|-+$/g, "")
      || "active-profile";
    return `/tmp/shinkai-agents-${profile}-${stamp}.tar`;
  }

  function agentDisplayName(agent: Demo) {
    return agent === "tool" ? "Tool agent" : "Echo agent";
  }

  function activeAgentLabel() {
    return agentId.trim() || agentDisplayName(demo);
  }

  function flattenConversationTree(
    nodes: ConversationTreeNode[],
    depth = 0,
  ): ConversationTreeRow[] {
    return nodes.flatMap((node) => [
      { node, depth },
      ...flattenConversationTree(node.children, depth + 1),
    ]);
  }

  function conversationTreeStats(nodes: ConversationTreeNode[]): ConversationTreeStats {
    let total = 0;
    let branchPoints = 0;
    let leaves = 0;
    let maxDepth = 0;

    function visit(node: ConversationTreeNode, depth: number) {
      total += 1;
      maxDepth = Math.max(maxDepth, depth);
      if (node.children.length) {
        branchPoints += 1;
      } else {
        leaves += 1;
      }
      node.children.forEach((child) => visit(child, depth + 1));
    }

    nodes.forEach((node) => visit(node, 0));
    return {
      total,
      roots: nodes.length,
      branchPoints,
      leaves,
      maxDepth,
    };
  }

  function filterConversationTree(
    nodes: ConversationTreeNode[],
    deleted: Set<string>,
  ): ConversationTreeNode[] {
    return nodes
      .filter((node) => !deleted.has(node.id))
      .map((node) => ({
        ...node,
        children: filterConversationTree(node.children, deleted),
      }));
  }

  function conversationMessageTitle(message: ConversationMessage, index: number) {
    return `${index + 1}. ${message.role} / ${message.created_at}`;
  }

  function upsertArtifact(
    artifacts: IngestionArtifact[],
    artifact: IngestionArtifact,
  ) {
    const rest = artifacts.filter((item) => item.id !== artifact.id);
    return [artifact, ...rest];
  }

  function upsertGeneratedArtifact(
    artifacts: GeneratedArtifact[],
    artifact: GeneratedArtifact,
  ) {
    const rest = artifacts.filter((item) => item.id !== artifact.id);
    return [artifact, ...rest];
  }

  function upsertMemoryRecord(records: MemoryRecord[], record: MemoryRecord) {
    const rest = records.filter((item) => item.id !== record.id);
    return [record, ...rest];
  }

  function upsertCompactionRecord(
    records: CompactionRecord[],
    record: CompactionRecord,
  ) {
    const rest = records.filter((item) => item.id !== record.id);
    return [record, ...rest];
  }

  function upsertPromptDoc(docs: PromptDoc[], doc: PromptDoc) {
    const rest = docs.filter(
      (item) =>
        item.name !== doc.name ||
        (item.agent_id ?? null) !== (doc.agent_id ?? null),
    );
    return [doc, ...rest].sort((a, b) => a.name.localeCompare(b.name));
  }

  function upsertSkillDoc(docs: SkillDoc[], doc: SkillDoc) {
    const rest = docs.filter((item) => item.id !== doc.id);
    return [doc, ...rest];
  }

  function upsertAgentConfig(docs: AgentConfigEntry[], doc: AgentConfigEntry) {
    const rest = docs.filter((item) => item.id !== doc.id);
    return [doc, ...rest].sort((a, b) => a.id.localeCompare(b.id));
  }

  function withExistingAgentMetadata(
    docs: AgentConfigEntry[],
    doc: AgentConfigEntry,
  ): AgentConfigEntry {
    const existing = docs.find((item) => item.id === doc.id);
    if (!existing) return doc;
    return {
      ...doc,
      profile: doc.profile ?? existing.profile ?? null,
      shared_from_profile:
        doc.shared_from_profile ?? existing.shared_from_profile ?? null,
      grant_id: doc.grant_id ?? existing.grant_id ?? null,
    };
  }

  function upsertProfileSummary(profiles: ProfileSummary[], profile: ProfileSummary) {
    const rest = profiles.filter((item) => item.id !== profile.id);
    return [profile, ...rest].sort((a, b) => a.id.localeCompare(b.id));
  }

  function upsertProfileGrant(grants: ProfileGrant[], grant: ProfileGrant) {
    const rest = grants.filter((item) => item.id !== grant.id);
    return [grant, ...rest].sort((a, b) => a.id.localeCompare(b.id));
  }

  function upsertSecretRecord(records: SecretRecord[], record: SecretRecord) {
    const rest = records.filter((item) => item.id !== record.id);
    return [record, ...rest].sort((a, b) => a.id.localeCompare(b.id));
  }

  function isProfileGrantKind(value: unknown): value is ProfileGrantKind {
    return (
      value === "agent" ||
      value === "memory" ||
      value === "tool" ||
      value === "skill" ||
      value === "category"
    );
  }

  function upsertCapabilityDraft(
    drafts: CapabilityDraft[],
    draft: CapabilityDraft,
  ) {
    const rest = drafts.filter((item) => item.id !== draft.id);
    return [draft, ...rest];
  }

  function capabilityDraftFromReviewResult(result: CapabilityReviewResult) {
    return isUnknownRecord(result) && "draft" in result
      ? (result.draft as CapabilityDraft)
      : (result as CapabilityDraft);
  }

  function skillFromCapabilityReviewResult(result: CapabilityReviewResult) {
    if (!isUnknownRecord(result) || !("draft" in result)) return null;
    const promoted = result.promoted_skill;
    if (isUnknownRecord(promoted)) return promoted as SkillDoc;
    const quarantined = result.quarantined_skill;
    if (isUnknownRecord(quarantined)) return quarantined as SkillDoc;
    return null;
  }

  function adapterPackageFromCapabilityReviewResult(
    result: CapabilityReviewResult,
  ) {
    if (!isUnknownRecord(result) || !("draft" in result)) return null;
    const promoted = result.promoted_tool;
    if (isUnknownRecord(promoted)) return promoted as AdapterPackage;
    const quarantined = result.quarantined_tool;
    if (isUnknownRecord(quarantined)) return quarantined as AdapterPackage;
    return null;
  }

  function upsertAdapterPackage(
    packages: AdapterPackage[],
    adapterPackage: AdapterPackage,
  ) {
    const rest = packages.filter((item) => item.id !== adapterPackage.id);
    return [adapterPackage, ...rest];
  }

  function upsertApproval(records: ApprovalRecord[], record: ApprovalRecord) {
    const rest = records.filter((item) => item.approval_id !== record.approval_id);
    return [record, ...rest];
  }

  function upsertApprovalAssessment(
    records: ApprovalRecord[],
    approvalId: string,
    assessment: ApprovalAssessment,
  ) {
    if (records.some((item) => item.approval_id === approvalId)) {
      return records.map((item) =>
        item.approval_id === approvalId ? { ...item, assessment } : item,
      );
    }
    return [
      {
        approval_id: approvalId,
        action: null,
        reason: assessment.reason,
        controller_agent: assessment.controller_agent,
        controller_scope: assessment.scope,
        status: "pending",
        approved: null,
        assessment,
      },
      ...records,
    ];
  }

  function defaultCompactionPath(id: string) {
    return `/tmp/${id || "compacted-context"}.json`;
  }

  function hasHighRiskFindings(artifact: IngestionArtifact) {
    return artifact.findings.some((finding) => finding.severity === "high");
  }

  function reviewForFinding(artifact: IngestionArtifact, index: number) {
    return artifact.finding_reviews.find(
      (review) => review.finding_index === index,
    );
  }

  function hasUnapprovedHighRiskFindings(artifact: IngestionArtifact) {
    return artifact.findings.some(
      (finding, index) =>
        finding.severity === "high" &&
        reviewForFinding(artifact, index)?.decision !== "approve",
    );
  }

  function guardrailStateForArtifact(id: string) {
    const artifact = ingestionArtifacts.find((item) => item.id === id);
    if (!artifact) return "unknown artifact; review ingestion before running";
    if (!hasHighRiskFindings(artifact)) return "allowed; no high-risk findings";
    if (!hasUnapprovedHighRiskFindings(artifact)) {
      return "allowed; high-risk findings approved";
    }
    return allowUnsafeIngest
      ? "unsafe override enabled; flagged content will be included"
      : "blocked; flagged content will be withheld";
  }

  function guardrailReport() {
    if (!ingestionArtifacts.length) {
      return [
        "Guardrails: no ingestion artifacts loaded.",
        `Unsafe ingest override: ${allowUnsafeIngest ? "on" : "off"}`,
        "Use /ingest to list or create artifacts before including external content.",
      ].join("\n");
    }
    const highRisk = ingestionArtifacts.filter(hasHighRiskFindings);
    const unapprovedHighRisk = ingestionArtifacts.filter(
      hasUnapprovedHighRiskFindings,
    );
    const includedHighRisk = includeIngestIds.filter((id) => {
      const artifact = ingestionArtifacts.find((item) => item.id === id);
      return artifact ? hasUnapprovedHighRiskFindings(artifact) : false;
    });
    const lines = [
      `Guardrails: ${ingestionArtifacts.length} artifacts, ${highRisk.length} high-risk, ${unapprovedHighRisk.length} unapproved, ${includeIngestIds.length} included.`,
      `Unsafe ingest override: ${allowUnsafeIngest ? "on" : "off"}`,
      `Included unapproved high-risk artifacts: ${includedHighRisk.length}`,
    ];
    for (const artifact of ingestionArtifacts) {
      const included = includeIngestIds.includes(artifact.id) ? "included" : "not included";
      const findings = artifact.findings.length
        ? artifact.findings
            .map((finding) => `${finding.severity}: ${finding.message}`)
            .join("; ")
        : "no findings";
      lines.push(
        `- ${artifact.id} (${included}): ${guardrailStateForArtifact(artifact.id)}; ${findings}`,
      );
    }
    return lines.join("\n");
  }

  function hasHighRiskAdapterFindings(adapterPackage: AdapterPackage) {
    return adapterPackage.findings.some((finding) => finding.severity === "high");
  }

  function hasHighRiskSkillFindings(skill: SkillDoc) {
    return (skill.findings ?? []).some((finding) => finding.severity === "high");
  }

  function enabledPermissions(adapterPackage: AdapterPackage) {
    return Object.entries(adapterPackage.permissions)
      .filter(([, enabled]) => enabled)
      .map(([name]) => name);
  }

  function adapterRuntimeSummary(runtime?: AdapterCapabilityRuntime | null) {
    if (!runtime) return null;
    const parts = [runtime.transport];
    if (runtime.command) parts.push(runtime.command);
    if (runtime.endpoint) parts.push(compactPreview(runtime.endpoint, 80));
    if (runtime.env_keys?.length) parts.push(`env ${runtime.env_keys.join(", ")}`);
    if (runtime.header_keys?.length) {
      parts.push(`headers ${runtime.header_keys.join(", ")}`);
    }
    if (runtime.auth_schemes?.length) {
      parts.push(`auth ${runtime.auth_schemes.join(", ")}`);
    }
    return parts.join(" / ");
  }

  function normalizeIngestionResponse(
    value: IngestionArtifact | IngestionResult,
  ) {
    if ("artifact" in value) {
      return value.artifact;
    }
    return value;
  }

  function previewText(text: string, max = 150) {
    const compact = text.replace(/\s+/g, " ").trim();
    if (compact.length <= max) {
      return compact;
    }
    return `${compact.slice(0, max - 1)}...`;
  }

  function serializedContextPreview() {
    if (!contextPreview) {
      return "";
    }
    return JSON.stringify(
      {
        prompt: contextPreviewPrompt ?? "(loaded from trace)",
        snapshot: contextPreview,
      },
      null,
      2,
    );
  }

  async function copyContextPreview() {
    const payload = serializedContextPreview();
    if (!payload) {
      appendLine("error", "Preview context before copying it.");
      return;
    }
    try {
      await navigator.clipboard.writeText(payload);
      setContextCopyStatus("Copied");
      appendEvent("Context preview copied as JSON.");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setContextCopyStatus("Copy failed");
      appendLine("error", `Context copy failed: ${msg}`);
    }
  }

  function sendContextPreviewToValue() {
    const payload = serializedContextPreview();
    if (!payload) {
      appendLine("error", "Preview context before sending it to Value.");
      return;
    }
    setOpsValue(payload);
    setContextCopyStatus("Sent to Value");
    appendEvent("Context preview JSON sent to Value.");
  }

  function conversationRoleCount(role: string) {
    return contextPreview?.conversation.filter((message) => message.role === role)
      .length ?? 0;
  }

  function contextPreviewDraftStatus() {
    if (!contextPreview) {
      return null;
    }
    if (contextPreviewPrompt === null) {
      return {
        className: "preview-source",
        label: "from trace",
      };
    }
    const draft = input.trim() || "preview";
    const current = draft === contextPreviewPrompt;
    return {
      className: current ? "preview-source current" : "preview-source stale",
      label: current ? "draft current" : "draft changed",
    };
  }

  function highRiskPreviewFindings(snapshot: ContextSnapshot) {
    return snapshot.loaded_artifacts.flatMap((artifact) =>
      artifact.findings
        .filter((finding) => isHighRiskFindingText(finding))
        .map((finding) => ({ artifact: artifact.id, finding })),
    );
  }

  function isHighRiskFindingText(finding: string) {
    const normalized = finding.toLowerCase();
    return (
      normalized.startsWith("high") ||
      normalized.includes("prompt injection") ||
      normalized.includes("prompt-injection")
    );
  }

  function visibilitySummary(snapshot: ContextSnapshot) {
    const levels = Array.from(
      new Set(snapshot.visible_tools.map((tool) => tool.visibility)),
    );
    if (!levels.length) return "No tool details";
    if (levels.length === 1) return levels[0].replaceAll("_", " ");
    return "Mixed visibility";
  }

  function contextReviewCards(snapshot: ContextSnapshot): ContextReviewCard[] {
    const draftStatus = contextPreviewDraftStatus();
    const highRiskFindings = highRiskPreviewFindings(snapshot);
    const cost = estimatedPreviewInputCost(snapshot);
    const compaction = compactionMetrics(snapshot);
    const costText = cost === null ? "Cost rate not set" : `Est ${formatCost(cost)}`;
    const remaining = snapshot.limits.remaining_tool_calls;
    const max = snapshot.limits.max_tool_calls;
    const toolTone =
      remaining === 0 ? "danger" : remaining === 1 ? "warning" : "ok";
    const promptTone =
      draftStatus?.className.includes("stale") ? "warning" : "ok";
    const artifactTone = highRiskFindings.length
      ? allowUnsafeIngest
        ? "warning"
        : "danger"
      : snapshot.loaded_artifacts.length
        ? "ok"
        : "neutral";

    return [
      {
        title: "Prompt",
        value: draftStatus?.label ?? "not previewed",
        detail:
          contextPreviewPrompt === null
            ? "Loaded from an existing trace snapshot."
            : `Preview prompt: ${contextPreviewPrompt ?? "none"}`,
        tone: promptTone,
      },
      {
        title: "Context Size",
        value: `~${snapshot.estimated_input_tokens} input tokens`,
        detail: `${costText}; ${snapshot.conversation.length} conversation messages.`,
        tone: "neutral",
      },
      {
        title: "Tools",
        value:
          remaining === 0
            ? "Tool calls disabled"
            : `${remaining}/${max} tool calls left`,
        detail: `${snapshot.visible_tools.length} visible tools; ${visibilitySummary(snapshot)}.`,
        tone: toolTone,
      },
      {
        title: "Memory",
        value: snapshot.loaded_memory.length
          ? `${snapshot.loaded_memory.length} fragments loaded`
          : "No memory loaded",
        detail: snapshot.loaded_memory.length
          ? "Memory will be included in the next LLM context."
          : "Memory loading is off or no records are available.",
        tone: snapshot.loaded_memory.length ? "ok" : "neutral",
      },
      {
        title: "External Content",
        value: snapshot.loaded_artifacts.length
          ? `${snapshot.loaded_artifacts.length} artifacts included`
          : "No artifacts included",
        detail: highRiskFindings.length
          ? `${highRiskFindings.length} high-risk findings; ${allowUnsafeIngest ? "unsafe override is on" : "flagged content will be withheld"}.`
          : "Prompt-injection guardrails found no high-risk included content.",
        tone: artifactTone,
      },
      ...(compaction
        ? [
            {
              title: "Compaction",
              value: compactionCardValue(compaction),
              detail: compactionCardDetail(compaction),
              tone: "ok" as const,
            },
          ]
        : []),
      {
        title: "Provenance",
        value: `${snapshot.provenance.length} records`,
        detail: snapshot.provenance.length
          ? "Sources are attached to the context snapshot."
          : "No extra context sources are attached.",
        tone: snapshot.provenance.length ? "ok" : "neutral",
      },
    ];
  }

  function sectionClass(section: ActiveSection) {
    return activeSection === section ? "rail-item active" : "rail-item";
  }

  function showOperationsPanel() {
    return [
      "chat",
      "conversations",
      "profiles",
      "memory",
      "skills",
      "prompts",
      "ingest",
      "artifacts",
      "adapters",
    ].includes(activeSection);
  }

  function operationsTitle() {
    switch (activeSection) {
      case "chat":
        return "Tools";
      case "conversations":
        return "Conversations";
      case "profiles":
        return "Profiles";
      case "memory":
        return "Memory";
      case "skills":
        return "Skills";
      case "prompts":
        return "Prompts and models";
      case "ingest":
        return "Ingestion";
      case "artifacts":
        return "Artifacts";
      case "adapters":
        return "Adapters and storage";
      default:
        return "Operations";
    }
  }

  const conversationStats = conversationTreeStats(conversationTree);
  const canGuideRun = Boolean(activeGuidanceRunId() && input.trim());

  return (
    <div className="app-shell">
      <aside className="rail" aria-label="Agent workspace sections">
        <div className="rail-mark" title="Shinkai Agents" aria-label="Shinkai Agents">
          <span className="rail-letter">AI</span>
          <span className="rail-copy">
            <span className="rail-label">Shinkai</span>
            <span className="rail-hint">Agents</span>
          </span>
        </div>
        <button
          type="button"
          className={sectionClass("chat")}
          title="Chat"
          aria-label="Chat transcript"
          onClick={() => setActiveSection("chat")}
        >
          <span className="rail-letter">C</span>
          <span className="rail-copy">
            <span className="rail-label">Chat</span>
            <span className="rail-hint">Ask an agent</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("trace")}
          title="Trace"
          aria-label="Trace viewer"
          onClick={() => {
            setActiveSection("trace");
            if (lastRunId && !running) void loadLastTrace();
          }}
          disabled={running || !lastRunId}
        >
          <span className="rail-letter">T</span>
          <span className="rail-copy">
            <span className="rail-label">Trace</span>
            <span className="rail-hint">Inspect runs</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("conversations")}
          title="Conversations"
          aria-label="Conversation branches"
          onClick={() => {
            setActiveSection("conversations");
            if (!running && !conversationTree.length) void reviewConversations();
          }}
          disabled={running}
        >
          <span className="rail-letter">B</span>
          <span className="rail-copy">
            <span className="rail-label">Conversations</span>
            <span className="rail-hint">Branches</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("profiles")}
          title="Profiles"
          aria-label="Profiles and grants"
          onClick={() => {
            setActiveSection("profiles");
            if (!running) {
              void showCurrentProfileFromOps();
              void listProfilesFromOps();
              void listProfileGrantsFromOps();
            }
          }}
          disabled={running}
        >
          <span className="rail-letter">R</span>
          <span className="rail-copy">
            <span className="rail-label">Profiles</span>
            <span className="rail-hint">Grants</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("memory")}
          title="Memory"
          aria-label="Memory records"
          onClick={() => setActiveSection("memory")}
          disabled={running}
        >
          <span className="rail-letter">M</span>
          <span className="rail-copy">
            <span className="rail-label">Memory</span>
            <span className="rail-hint">Saved context</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("skills")}
          title="Skills"
          aria-label="Skill library"
          onClick={() => setActiveSection("skills")}
          disabled={running}
        >
          <span className="rail-letter">S</span>
          <span className="rail-copy">
            <span className="rail-label">Skills</span>
            <span className="rail-hint">Capabilities</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("prompts")}
          title="Prompts"
          aria-label="Saved prompts"
          onClick={() => setActiveSection("prompts")}
          disabled={running}
        >
          <span className="rail-letter">P</span>
          <span className="rail-copy">
            <span className="rail-label">Prompts</span>
            <span className="rail-hint">Reusable tasks</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("ingest")}
          title="Ingest"
          aria-label="Ingestion artifacts"
          onClick={() => setActiveSection("ingest")}
          disabled={running}
        >
          <span className="rail-letter">I</span>
          <span className="rail-copy">
            <span className="rail-label">Ingest</span>
            <span className="rail-hint">Documents</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("artifacts")}
          title="Artifacts"
          aria-label="Generated artifacts"
          onClick={() => setActiveSection("artifacts")}
          disabled={running}
        >
          <span className="rail-letter">G</span>
          <span className="rail-copy">
            <span className="rail-label">Artifacts</span>
            <span className="rail-hint">Generated files</span>
          </span>
        </button>
        <button
          type="button"
          className={sectionClass("adapters")}
          title="Adapters"
          aria-label="Adapter manifests"
          onClick={() => setActiveSection("adapters")}
          disabled={running}
        >
          <span className="rail-letter">A</span>
          <span className="rail-copy">
            <span className="rail-label">Adapters</span>
            <span className="rail-hint">Integrations</span>
          </span>
        </button>
        <button
          type="button"
          className={`${sectionClass("approvals")} rail-bottom`}
          title="Approvals"
          aria-label="Approvals"
          onClick={() => setActiveSection("approvals")}
          disabled={running || !lastRunId}
        >
          <span className="rail-letter">!</span>
          <span className="rail-copy">
            <span className="rail-label">Approvals</span>
            <span className="rail-hint">Review actions</span>
          </span>
        </button>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <h1>Shinkai Agents</h1>
            <div className="run-meta">
              {lastRunId ? `Run ${runLabel}` : "Ready for a new run"}
            </div>
          </div>
          <div className="status-pills">
            <span className="pill" title="Active agent">
              Agent {activeAgentLabel()}
            </span>
            <span className={running ? "pill running" : "pill idle"}>
              {running ? "Running" : "Idle"}
            </span>
            <span className="pill" title="Tool output mode">
              Output {rawToolOutput ? "Raw" : "Interpreted"}
            </span>
            <span className="pill">Tokens {tokensIn}/{tokensOut}</span>
            <span className="pill">Cost ${costUsd.toFixed(6)}</span>
            <span className="pill">Time {formatDuration(elapsedMs)}</span>
            <span
              className={budgetPillClass}
              title={`Tool-call budget: ${remainingToolCalls} remaining of ${effectiveMaxToolCalls}`}
            >
              Tools {remainingToolCalls} left
            </span>
          </div>
        </header>

        <section className="transcript" aria-live="polite" ref={transcriptRef}>
          {transcript.map((line, i) => (
            <div key={i} className={`line line-${line.kind}`}>
              <span className="prefix">{prefixFor(line.kind)}</span>
              <div className="content">{renderLineContent(line)}</div>
            </div>
          ))}
        </section>

        <footer className="composer">
          <textarea
            value={input}
            onChange={(e) => updateComposerInput(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={running ? "Type /guide to steer this run" : "Ask the agent"}
            aria-controls={slashCommandItems.length ? "slash-command-menu" : undefined}
            aria-expanded={slashCommandItems.length ? true : undefined}
            aria-activedescendant={activeSlashCommandId}
            rows={4}
          />
          {slashCommandItems.length ? (
            <div
              className="slash-command-menu"
              id="slash-command-menu"
              role="listbox"
              aria-label="Slash commands"
            >
              {slashCommandItems.map((item, index) => (
                <button
                  type="button"
                  role="option"
                  id={`slash-command-${index}`}
                  aria-selected={index === slashCommandIndex}
                  className={index === slashCommandIndex ? "selected" : ""}
                  key={`${item.command}:${item.label}`}
                  onClick={() => applySlashCommand(item.command)}
                  onMouseEnter={() => setSlashCommandIndex(index)}
                  title={item.label}
                >
                  <span>{item.command}</span>
                  <small>{item.label}</small>
                </button>
              ))}
            </div>
          ) : null}
          <div className="composer-actions">
            <button
              type="button"
              className="primary"
              onClick={() => void submit()}
              disabled={running || !input.trim()}
              title="Send"
            >
              Ask Agent
            </button>
            <button
              type="button"
              onClick={() => void previewCurrentContext()}
              disabled={running}
              title="Preview context"
            >
              Preview
            </button>
            <button
              type="button"
              onClick={() => void callShell()}
              disabled={running || !input.trim()}
            >
              Shell
            </button>
            <button
              type="button"
              onClick={() => void runBatchFromInput()}
              disabled={running || !input.trim()}
            >
              Batch
            </button>
            <button
              type="button"
              onClick={() => void resumeBatchFromOps()}
              disabled={running || !opsId.trim()}
            >
              Resume Batch
            </button>
            <button
              type="button"
              title="Resume the run id in the Id field, or the last run when Id is blank."
              onClick={() => void resumeLastRun()}
              disabled={running || (!opsId.trim() && !lastRunId)}
            >
              Resume Run
            </button>
            <button
              type="button"
              onClick={() => void guideLastRun()}
              disabled={!canGuideRun}
              title="Guide the active run"
            >
              Guide
            </button>
          </div>
        </footer>
      </main>

      <aside className="inspector">
        {activeSection === "chat" ? (
        <section className="panel">
          <div className="panel-title">Agent setup</div>
          <label>
            Transport
            <select
              value={transport}
              onChange={(e) => setTransport(e.target.value as Transport)}
              disabled={running}
            >
              <option value="in-process" disabled={!tauriRuntime}>
                in-process
              </option>
              <option value="daemon">daemon</option>
            </select>
          </label>
          <label>
            Daemon URL
            <input
              value={daemonUrl}
              onChange={(e) => setDaemonUrl(e.target.value)}
              disabled={running || transport !== "daemon"}
            />
          </label>
          <label>
            Demo behavior
            <select
              value={demo}
              onChange={(e) => setDemo(e.target.value as Demo)}
              disabled={running}
            >
              <option value="echo">Echo agent</option>
              <option value="tool">Tool agent</option>
            </select>
          </label>
          <label>
            Agent id
            <input
              value={agentId}
              onChange={(e) => setAgentId(e.target.value)}
              placeholder="blank for demo default"
              disabled={running}
            />
          </label>
          <label>
            Provider
            <select
              value={provider}
              title={
                providerOptionKeys
                  ? `Provider options: ${providerOptionKeys}`
                  : "Provider options unavailable"
              }
              onChange={(e) => {
                const nextProvider = e.target.value as Provider;
                setProvider(nextProvider);
                if (
                  [
                    "OPENAI_API_KEY",
                    "OLLAMA_API_KEY",
                    "LLAMA_CPP_API_KEY",
                    "ANTHROPIC_API_KEY",
                    "GEMINI_API_KEY",
                  ].includes(apiKeyEnv)
                ) {
                  setApiKeyEnv(defaultApiKeyEnvForProvider(nextProvider));
                }
              }}
              disabled={running}
            >
              <option value="fake">fake</option>
              <option value="rig">rig</option>
              <option value="ollama">ollama</option>
              <option value="llama_cpp">llama.cpp</option>
              <option value="anthropic">anthropic</option>
              <option value="gemini">gemini</option>
            </select>
          </label>
          <label>
            Model
            <input
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder={defaultModelForProvider(provider)}
              disabled={running}
            />
          </label>
          <label>
            Image input
            <input
              type="checkbox"
              checked={modelSupportsImage}
              onChange={(e) => setModelSupportsImage(e.target.checked)}
              disabled={running || provider === "fake"}
            />
          </label>
          <label>
            API base
            <input
              value={apiBaseUrl}
              onChange={(e) => setApiBaseUrl(e.target.value)}
              placeholder={defaultApiBaseUrlForProvider(provider) || "provider default"}
              title="Use a custom /v1 base URL only for local or OpenAI-compatible providers."
              disabled={running || !supportsApiBaseUrl}
            />
          </label>
          <label>
            API key env
            <input
              value={apiKeyEnv}
              onChange={(e) => setApiKeyEnv(e.target.value)}
              disabled={running || provider === "fake"}
            />
          </label>
          <label>
            API key
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="optional; not saved"
              disabled={running || provider === "fake"}
            />
          </label>
          <label>
            Top p
            <input
              value={providerTopP}
              onChange={(e) => setProviderTopP(e.target.value)}
              placeholder="provider default"
              inputMode="decimal"
              disabled={running || !supportsTopP}
            />
          </label>
          <label>
            Top k
            <input
              value={providerTopK}
              onChange={(e) => setProviderTopK(e.target.value)}
              placeholder="provider default"
              inputMode="numeric"
              disabled={running || !supportsTopK}
            />
          </label>
          <label>
            Reasoning effort
            <input
              value={providerReasoningEffort}
              onChange={(e) => setProviderReasoningEffort(e.target.value)}
              placeholder="provider default"
              list="reasoning-effort-options"
              disabled={running || !supportsReasoningEffort}
            />
            <datalist id="reasoning-effort-options">
              {(providerOptionDescriptor("reasoning_effort")?.allowed_values ?? [
                "low",
                "medium",
                "high",
              ]).map((value) => (
                <option key={value} value={value} />
              ))}
            </datalist>
          </label>
          {supportsFrequencyPenalty || providerFrequencyPenalty ? (
            <label>
              Frequency penalty
              <input
                value={providerFrequencyPenalty}
                onChange={(e) => setProviderFrequencyPenalty(e.target.value)}
                placeholder="provider default"
                inputMode="decimal"
                disabled={running || !supportsFrequencyPenalty}
              />
            </label>
          ) : null}
          {supportsPresencePenalty || providerPresencePenalty ? (
            <label>
              Presence penalty
              <input
                value={providerPresencePenalty}
                onChange={(e) => setProviderPresencePenalty(e.target.value)}
                placeholder="provider default"
                inputMode="decimal"
                disabled={running || !supportsPresencePenalty}
              />
            </label>
          ) : null}
          <label>
            Input $/M
            <input
              type="number"
              min="0"
              step="0.000001"
              value={inputCostPerMillion}
              onChange={(e) => setInputCostPerMillion(e.target.value)}
              placeholder="config"
              disabled={running}
            />
          </label>
          <label>
            Output $/M
            <input
              type="number"
              min="0"
              step="0.000001"
              value={outputCostPerMillion}
              onChange={(e) => setOutputCostPerMillion(e.target.value)}
              placeholder="config"
              disabled={running}
            />
          </label>
        </section>
        ) : null}

        {activeSection === "chat" ? (
        <section className="panel">
          <div className="panel-title">Context</div>
          <div className="run-readiness-grid">
            {runReadinessCards().map((card) => (
              <div className={`run-readiness-card ${card.tone}`} key={card.title}>
                <span>{card.title}</span>
                <strong>{card.value}</strong>
                <p>{card.detail}</p>
              </div>
            ))}
          </div>
          <div className="segmented-control" role="group" aria-label="Agent mode">
            <button
              type="button"
              className={agentMode === "answer" ? "selected" : ""}
              aria-pressed={agentMode === "answer"}
              onClick={() => setAgentMode("answer")}
              disabled={running}
              title="No tool calls. The agent answers from the visible context only."
            >
              Answer
            </button>
            <button
              type="button"
              className={agentMode === "action" ? "selected" : ""}
              aria-pressed={agentMode === "action"}
              onClick={() => setAgentMode("action")}
              disabled={running}
              title="One tool call. Useful for routing a simple action."
            >
              One action
            </button>
            <button
              type="button"
              className={agentMode === "workflow" ? "selected" : ""}
              aria-pressed={agentMode === "workflow"}
              onClick={() => setAgentMode("workflow")}
              disabled={running}
              title={`Default ${CALLS_MAX}-call budget for multi-step work.`}
            >
              Workflow
            </button>
          </div>
          {agentMode === "custom" ? (
            <div className="mode-note">
              Custom budget: {effectiveMaxToolCalls} calls
            </div>
          ) : null}
          <label className="switch">
            <input
              type="checkbox"
              checked={enableShell}
              onChange={(e) => setEnableShell(e.target.checked)}
              disabled={running}
            />
            <span>Shell</span>
          </label>
          <label>
            Max tool calls
            <input
              type="number"
              min="0"
              step="1"
              value={maxToolCalls}
              onChange={(e) => setMaxToolCalls(e.target.value)}
              placeholder="config"
              disabled={running}
            />
          </label>
          <label>
            Conversation id
            <input
              value={conversationId}
              onChange={(e) => setConversationId(e.target.value)}
              placeholder="optional branch"
              disabled={running}
            />
          </label>
          <label>
            Auto compact at
            <input
              type="number"
              min="1"
              step="1"
              value={maxTokensBeforeCompaction}
              onChange={(e) => setMaxTokensBeforeCompaction(e.target.value)}
              placeholder="config"
              disabled={running}
            />
          </label>
          <label>
            Compact output
            <input
              type="number"
              min="1"
              step="1"
              value={maxCompactionOutputTokens}
              onChange={(e) => setMaxCompactionOutputTokens(e.target.value)}
              placeholder="config"
              disabled={running}
            />
          </label>
          <label>
            Tool visibility
            <select
              value={toolVisibility}
              onChange={(e) => setToolVisibility(e.target.value as ToolVisibility | "")}
              disabled={running}
            >
              <option value="">config</option>
              <option value="full_schema">full schema</option>
              <option value="name_and_description">name and description</option>
              <option value="name_only">name only</option>
            </select>
          </label>
          <label>
            Tool categories
            <input
              value={allowedToolCategories}
              onChange={(e) => setAllowedToolCategories(e.target.value)}
              placeholder="all categories"
              disabled={running}
            />
          </label>
          <label>
            Skill categories
            <input
              value={allowedSkillCategories}
              onChange={(e) => setAllowedSkillCategories(e.target.value)}
              placeholder="all categories"
              disabled={running}
            />
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={enableSubagent}
              onChange={(e) => setEnableSubagent(e.target.checked)}
              disabled={running}
            />
            <span>Subagent</span>
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={enableCapabilityDrafts}
              onChange={(e) => setEnableCapabilityDrafts(e.target.checked)}
              disabled={running}
            />
            <span>Draft tool</span>
          </label>
          <label>
            Draft guidance
            <input
              value={capabilityDraftGuidance}
              onChange={(e) => setCapabilityDraftGuidance(e.target.value)}
              placeholder="optional capability drafting instructions"
              disabled={running}
            />
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={loadMemory}
              onChange={(e) => setLoadMemory(e.target.checked)}
              disabled={running}
            />
            <span>Memory</span>
          </label>
          <label>
            Memory generation
            <select
              value={generateMemoryPolicy}
              onChange={(e) =>
                setGenerateMemoryPolicy(e.target.value as "" | "on" | "off")
              }
              disabled={running}
            >
              <option value="">default</option>
              <option value="on">on</option>
              <option value="off">off</option>
            </select>
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={loadSkills}
              onChange={(e) => setLoadSkills(e.target.checked)}
              disabled={running}
            />
            <span>Skills</span>
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={allowUnsafeIngest}
              onChange={(e) => toggleUnsafeIngest(e.target.checked)}
              disabled={running || !includeIngestIds.length}
            />
            <span>Unsafe ingest</span>
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={requireApproval}
              onChange={(e) => setRequireApproval(e.target.checked)}
              disabled={running}
            />
            <span>Approval gate</span>
          </label>
          <div className="field-label">Output mode</div>
          <div className="segmented-control two" role="group" aria-label="Output mode">
            <button
              type="button"
              className={!rawToolOutput ? "selected" : ""}
              aria-pressed={!rawToolOutput}
              onClick={() => setRawToolOutput(false)}
              disabled={running}
              title="Let the agent interpret tool results before replying."
            >
              Interpret
            </button>
            <button
              type="button"
              className={rawToolOutput ? "selected" : ""}
              aria-pressed={rawToolOutput}
              onClick={() => setRawToolOutput(true)}
              disabled={running}
              title="Show original tool results without interpretation."
            >
              Raw
            </button>
          </div>
          {rawToolOutput ? (
            <div className="mode-note">
              Raw output preserves original tool results and skips interpretation.
            </div>
          ) : null}
          <label>
            Router model
            <input
              value={toolRoutingModel}
              onChange={(e) => setToolRoutingModel(e.target.value)}
              placeholder="agent default"
              disabled={running}
              title="Use a different model for tool-call selection."
            />
          </label>
          {!rawToolOutput ? (
            <label>
              Interpreter model
              <input
                value={toolOutputInterpretationModel}
                onChange={(e) =>
                  setToolOutputInterpretationModel(e.target.value)
                }
                placeholder="agent default"
                disabled={running}
                title="Use a different model for interpreted tool outputs."
              />
            </label>
          ) : null}
          <label>
            Compaction guidance
            <textarea
              className="ops-text"
              value={compactionGuidance}
              onChange={(e) => setCompactionGuidance(e.target.value)}
              placeholder="config"
              disabled={running}
              rows={2}
            />
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={enablePromptRefinement}
              onChange={(e) => setEnablePromptRefinement(e.target.checked)}
              disabled={running}
            />
            <span>Refine prompt</span>
          </label>
          {enablePromptRefinement ? (
            <>
              <label>
                Refiner model
                <input
                  value={promptRefinementModel}
                  onChange={(e) => setPromptRefinementModel(e.target.value)}
                  placeholder="agent model"
                  disabled={running}
                />
              </label>
              <label>
                Refinement instructions
                <textarea
                  className="ops-text"
                  value={promptRefinementInstructions}
                  onChange={(e) =>
                    setPromptRefinementInstructions(e.target.value)
                  }
                  placeholder="default refinement"
                  disabled={running}
                  rows={3}
                />
              </label>
            </>
          ) : null}
          {includeIngestIds.length ? (
            <div className="included-list">
              <strong>Included ingest</strong>
              {includeIngestIds.map((id) => (
                <span key={id} title={guardrailStateForArtifact(id)}>
                  {id} - {guardrailStateForArtifact(id)}
                </span>
              ))}
              <button
                type="button"
                onClick={clearIncludedIngest}
                disabled={running}
              >
                Clear Ingest
              </button>
            </div>
          ) : null}
          <div className="context-actions">
            <button
              type="button"
              onClick={() => void previewCurrentContext()}
              disabled={running}
            >
              Preview Context
            </button>
            <button
              type="button"
              onClick={() => void explainCurrentConfig()}
              disabled={running}
            >
              Explain Config
            </button>
            <button
              type="button"
              onClick={() => void explainCurrentTools()}
              disabled={running}
            >
              Explain Tools
            </button>
          </div>
          {postRunCompactionPrompt ? (
            <div className="mode-note compaction-prompt">
              <span>
                Auto compaction ready from run {postRunCompactionPrompt.runId.slice(0, 8)}
                {compactionSavingsLabel(postRunCompactionPrompt.snapshot)
                  ? `; ${compactionSavingsLabel(postRunCompactionPrompt.snapshot)}`
                  : ""}
              </span>
              <div className="mini-actions">
                <button
                  type="button"
                  onClick={() => void keepPostRunCompaction()}
                  disabled={running}
                >
                  Keep
                </button>
                <button
                  type="button"
                  onClick={() => setPostRunCompactionPrompt(null)}
                  disabled={running}
                >
                  Dismiss
                </button>
              </div>
            </div>
          ) : null}
          {contextPreview ? (
            <div className="context-preview">
              <div className="context-summary">
                {contextPreviewDraftStatus() ? (
                  <span className={contextPreviewDraftStatus()?.className}>
                    {contextPreviewDraftStatus()?.label}
                  </span>
                ) : null}
                <span>~{contextPreview.estimated_input_tokens} input tokens</span>
                {estimatedPreviewInputCost(contextPreview) !== null ? (
                  <span>
                    est ${estimatedPreviewInputCost(contextPreview)?.toFixed(6)}
                  </span>
                ) : null}
                <span>messages {contextPreview.conversation.length}</span>
                <span>compacted {contextPreview.compacted ? "on" : "off"}</span>
                <span>user {conversationRoleCount("user")}</span>
                <span>assistant {conversationRoleCount("assistant")}</span>
                <span>tools {contextPreview.visible_tools.length}</span>
                <span>skills {contextPreview.visible_skills.length}</span>
                <span>memory {contextPreview.loaded_memory.length}</span>
                <span>artifacts {contextPreview.loaded_artifacts.length}</span>
                <span>provenance {contextPreview.provenance.length}</span>
              </div>
              <div className="context-preview-toolbar">
                <button
                  type="button"
                  onClick={() => void copyContextPreview()}
                  disabled={running}
                  title="Copy the exact preview snapshot as JSON."
                >
                  Copy JSON
                </button>
                <button
                  type="button"
                  onClick={sendContextPreviewToValue}
                  disabled={running}
                  title="Put the exact preview snapshot into the Value field."
                >
                  Send to Value
                </button>
                <button
                  type="button"
                  onClick={() => void keepContextPreviewCompaction()}
                  disabled={running || !contextPreview.compacted}
                  title="Save the compacted context as a portable artifact."
                >
                  Keep Compact
                </button>
                {contextCopyStatus ? <span>{contextCopyStatus}</span> : null}
              </div>
              <div className="context-review-grid">
                {contextReviewCards(contextPreview).map((card) => (
                  <div
                    className={`context-review-card ${card.tone}`}
                    key={card.title}
                  >
                    <span>{card.title}</span>
                    <strong>{card.value}</strong>
                    <p>{card.detail}</p>
                  </div>
                ))}
              </div>
              {contextPreview.compaction_review ? (
                <section className="compaction-review">
                  <strong>Compaction Review</strong>
                  <span>{compactionReviewLabel(contextPreview)}</span>
                  <div className="compaction-review-grid">
                    <div>
                      <strong>Before</strong>
                      <pre>{compactionReviewBeforeText(contextPreview)}</pre>
                    </div>
                    <div>
                      <strong>After</strong>
                      <pre>{compactionReviewAfterText(contextPreview)}</pre>
                    </div>
                  </div>
                </section>
              ) : null}
              <section>
                <strong>System</strong>
                <pre>{contextPreview.system_prompt}</pre>
              </section>
              <section>
                <strong>Limits</strong>
                <pre>{previewJson(contextPreview.limits)}</pre>
              </section>
              <section>
                <strong>Compacted Context</strong>
                <pre>{contextPreview.compacted ?? "(none)"}</pre>
              </section>
              <section>
                <strong>Conversation ({contextPreview.conversation.length})</strong>
                <pre>{previewJson(contextPreview.conversation)}</pre>
              </section>
              <section>
                <strong>Tools ({contextPreview.visible_tools.length})</strong>
                {contextPreview.visible_tools.length ? (
                  <div className="context-cards">
                    {contextPreview.visible_tools.map((tool) => (
                      <div className="context-card" key={tool.id}>
                        <strong>{tool.name}</strong>
                        <span>{tool.visibility}</span>
                        <span>output {tool.output_mode}</span>
                        {tool.categories.length ? (
                          <span>categories {tool.categories.join(", ")}</span>
                        ) : null}
                        {tool.provenance ? <span>{tool.provenance}</span> : null}
                        {tool.description ? <p>{tool.description}</p> : null}
                        {tool.input_schema ? (
                          <div className="tool-parameters">
                            {toolParameters(tool.input_schema).length ? (
                              toolParameters(tool.input_schema).map((parameter) => (
                                <div className="tool-parameter" key={parameter.name}>
                                  <div className="tool-parameter-head">
                                    <strong>{parameter.name}</strong>
                                    <span>
                                      {parameter.type}
                                      {parameter.required ? " / required" : ""}
                                    </span>
                                  </div>
                                  {parameter.description ? (
                                    <p>{parameter.description}</p>
                                  ) : null}
                                </div>
                              ))
                            ) : (
                              <span>schema available</span>
                            )}
                          </div>
                        ) : null}
                        {tool.output_interpretation_guidance ? (
                          <p>{previewText(tool.output_interpretation_guidance, 180)}</p>
                        ) : null}
                        <div className="mini-actions">
                          <button
                            type="button"
                            title="Stage this tool in Id and Value for a direct manual call."
                            onClick={() => void stageToolFromPreview(tool)}
                            disabled={running}
                          >
                            Use Tool
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : null}
                <pre>{previewJson(contextPreview.visible_tools)}</pre>
              </section>
              <section>
                <strong>Skills ({contextPreview.visible_skills.length})</strong>
                {contextPreview.visible_skills.length ? (
                  <div className="context-cards">
                    {contextPreview.visible_skills.map((skill) => (
                      <div className="context-card" key={skill.id}>
                        <strong>{skill.name}</strong>
                        <span>{skill.visibility}</span>
                        <span>~{skill.estimated_tokens} tokens</span>
                        {skill.categories.length ? (
                          <span>categories {skill.categories.join(", ")}</span>
                        ) : null}
                        {skill.description ? <p>{skill.description}</p> : null}
                        <div className="mini-actions">
                          <button
                            type="button"
                            title="Open the full skill source and review status."
                            onClick={() => void openSkillFromPreview(skill)}
                            disabled={running}
                          >
                            Open Skill
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                ) : null}
                <pre>{previewJson(contextPreview.visible_skills)}</pre>
              </section>
              <section>
                <strong>Memory ({contextPreview.loaded_memory.length})</strong>
                {contextPreview.loaded_memory.length ? (
                  <div className="context-cards">
                    {contextPreview.loaded_memory.map((memory) => (
                      <div className="context-card" key={memory.id}>
                        <strong>{memory.id}</strong>
                        <span>{memory.provenance}</span>
                        <p>{previewText(memory.content)}</p>
                      </div>
                    ))}
                  </div>
                ) : null}
                <pre>{previewJson(contextPreview.loaded_memory)}</pre>
              </section>
              <section>
                <strong>Artifacts ({contextPreview.loaded_artifacts.length})</strong>
                {contextPreview.loaded_artifacts.length ? (
                  <div className="context-cards">
                    {contextPreview.loaded_artifacts.map((artifact) => (
                      <div className="context-card" key={artifact.id}>
                        <strong>{artifact.id}</strong>
                        <span>
                          {artifact.sections} sections / {artifact.provenance}
                        </span>
                        {artifact.findings.length ? (
                          <div className="finding-list">
                            {artifact.findings.map((finding) => (
                              <span className="finding high" key={finding}>
                                {finding}
                              </span>
                            ))}
                          </div>
                        ) : null}
                        <p>{previewText(artifact.content)}</p>
                      </div>
                    ))}
                  </div>
                ) : null}
                <pre>{previewJson(contextPreview.loaded_artifacts)}</pre>
              </section>
              <section>
                <strong>Provenance ({contextPreview.provenance.length})</strong>
                {contextPreview.provenance.length ? (
                  <div className="context-cards">
                    {contextPreview.provenance.map((record) => (
                      <div
                        className="context-card compact"
                        key={`${record.fragment}:${record.source}`}
                      >
                        <strong>{record.fragment}</strong>
                        <span>{record.source}</span>
                      </div>
                    ))}
                  </div>
                ) : null}
                <pre>{previewJson(contextPreview.provenance)}</pre>
              </section>
            </div>
          ) : null}
        </section>
        ) : null}

        {activeSection === "trace" ? (
        <section className="panel">
          <div className="panel-title">Trace</div>
          <div className="context-actions">
            <button
              type="button"
              title="Load the run id in the Id field, or the last run when Id is blank."
              onClick={() => void loadTraceFromOps()}
              disabled={running || (!opsId.trim() && !lastRunId)}
            >
              Load Trace
            </button>
            <button
              type="button"
              title="Load the comparison run id as a secondary trace."
              onClick={() => void loadTraceComparisonFromOps()}
              disabled={
                running ||
                !traceSummary ||
                (!traceCompareRunId.trim() && !opsId.trim())
              }
            >
              Compare
            </button>
            <button
              type="button"
              onClick={clearLoadedTrace}
              disabled={running || !traceEvents.length}
            >
              Clear Trace
            </button>
            <button
              type="button"
              onClick={clearTraceComparison}
              disabled={running || !traceCompareSummary}
            >
              Clear Compare
            </button>
            <button
              type="button"
              title="Load the original prompt from the loaded trace into the composer."
              onClick={loadTracePromptToComposer}
              disabled={running || !traceOriginalPrompt(traceEvents)}
            >
              Load Prompt
            </button>
            <button
              type="button"
              title="Run the original prompt from the loaded trace again."
              onClick={() => void replayTracePrompt()}
              disabled={running || !traceOriginalPrompt(traceEvents)}
            >
              Replay
            </button>
          </div>
          <label>
            Compare run
            <input
              aria-label="Compare run id"
              value={traceCompareRunId}
              onChange={(event) => setTraceCompareRunId(event.target.value)}
              placeholder="run id to compare"
            />
          </label>
          {traceSummary ? (
            <div className="trace-summary">
              <span>events {traceSummary.events}</span>
              <span>contexts {traceSummary.context_snapshots}</span>
              <span>llm {traceSummary.llm_calls}</span>
              <span>tools {traceSummary.tool_calls}</span>
              <span>tokens {traceSummary.tokens_in}/{traceSummary.tokens_out}</span>
              <span>
                cost{" "}
                {traceSummary.cost_usd === null
                  ? "n/a"
                  : `$${traceSummary.cost_usd.toFixed(6)}`}
              </span>
              <span>
                time{" "}
                {traceSummary.duration_ms === null
                  ? "n/a"
                  : `${traceSummary.duration_ms}ms`}
              </span>
              <span>approvals {traceSummary.approvals}</span>
              <span>guidance {traceSummary.guidance_injections}</span>
              <span>
                scores {traceSummary.quality_scores}
                {traceSummary.quality_score_average == null
                  ? ""
                  : ` avg ${traceSummary.quality_score_average.toFixed(1)}`}
              </span>
              <span>memory {traceSummary.memory_fragments}</span>
              <span>artifacts {traceSummary.artifact_refs}</span>
              <span>
                hooks {traceSummary.hooks}
                {traceSummary.hook_failures ? ` / ${traceSummary.hook_failures} failed` : ""}
              </span>
            </div>
          ) : (
            <div className="empty-note">No trace loaded.</div>
          )}
          {traceTree ? (
            <section className="trace-tree">
              <div className="trace-tree-head">
                <div>
                  <strong>Run Tree</strong>
                  <span>
                    {traceTreeNodeCount(traceTree)} run(s),{" "}
                    {traceTreeLeafCount(traceTree)} leaf run(s), depth{" "}
                    {traceTreeMaxDepth(traceTree)}
                  </span>
                </div>
                <div className="mini-actions">
                  <button
                    type="button"
                    title="Expand every branch in the run tree."
                    onClick={() => setCollapsedTraceTreeRuns([])}
                    disabled={running || !collapsedTraceTreeRuns.length}
                  >
                    Expand All
                  </button>
                  <button
                    type="button"
                    title="Collapse every branch in the run tree."
                    onClick={() =>
                      setCollapsedTraceTreeRuns(
                        expandableTraceTreeRunIds(traceTree),
                      )
                    }
                    disabled={running || !expandableTraceTreeRunIds(traceTree).length}
                  >
                    Collapse All
                  </button>
                </div>
              </div>
              <div className="trace-tree-list">{renderTraceTreeNode(traceTree)}</div>
            </section>
          ) : null}
          {traceSummary && traceCompareSummary ? (
            <section className="trace-tree">
              <div className="trace-tree-head">
                <div>
                  <strong>Trace Compare</strong>
                  <span>
                    primary {traceSummary.run_id} / compare{" "}
                    {traceCompareSummary.run_id}
                  </span>
                </div>
              </div>
              <div className="context-cards">
                {traceComparisonRows(
                  traceSummary,
                  traceCompareSummary,
                  traceTree,
                  traceCompareTree,
                ).map((row) => (
                  <div className="context-card compact" key={row.label}>
                    <strong>{row.label}</strong>
                    <span>primary {row.primary}</span>
                    <span>compare {row.compare}</span>
                    <span>delta {row.delta}</span>
                  </div>
                ))}
              </div>
            </section>
          ) : null}
          <section className="hook-remediation-list">
            <strong>Hook Catalog</strong>
            <div className="mini-actions">
              <button
                type="button"
                title="List allowed lifecycle hooks and their effective policy state."
                onClick={() => void refreshHookCatalog()}
              >
                List Hooks
              </button>
            </div>
            {hookCatalog.length ? (
              <div className="context-cards">
                {hookCatalog.map((hook) => (
                  <div
                    className={`context-card compact ${hook.disabled ? "warning" : ""}`}
                    key={hook.id}
                  >
                    <strong>{hook.id}</strong>
                    <span>
                      {hook.triggers.join(", ") || "no triggers"} - {hook.provenance}
                      {hook.disabled
                        ? ` - disabled by ${hook.disabled_source ?? "policy"}`
                        : ""}
                    </span>
                    <div className="mini-actions">
                      <button
                        type="button"
                        title="Persistently skip this lifecycle hook for future runs in the active profile."
                        onClick={() =>
                          void setPersistentHookDisabled(hook.id, true, "profile")
                        }
                      >
                        Disable Profile
                      </button>
                      <button
                        type="button"
                        title="Persistently skip this lifecycle hook for the active agent config."
                        onClick={() =>
                          void setPersistentHookDisabled(hook.id, true, "agent")
                        }
                      >
                        Disable Agent
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            ) : null}
          </section>
          {hookRemediationsFromEvents(traceEvents).length ? (
            <section className="hook-remediation-list">
              <strong>Hook Review</strong>
              <div className="mini-actions">
                <button
                  type="button"
                  title="Load persisted lifecycle hook policy for the active agent/profile."
                  onClick={() => void refreshHookPolicy()}
                >
                  Refresh Policy
                </button>
                {hookPolicy ? (
                  <span>
                    profile {hookPolicy.profile} - disabled{" "}
                    {hookPolicy.disabled_lifecycle_hooks.length}
                    {hookPolicy.effective_source
                      ? ` - source ${hookPolicy.effective_source}`
                      : ""}
                    {hookPolicy.agent_id ? ` - agent ${hookPolicy.agent_id}` : ""}
                  </span>
                ) : null}
              </div>
              <div className="context-cards">
                {hookRemediationsFromEvents(traceEvents).map((item) => {
                  const persistentlyDisabled = hookIsPersistentlyDisabled(item.hook_id);
                  const profileDisabled = hookIsProfileDisabled(item.hook_id);
                  const agentDisabled = hookIsAgentDisabled(item.hook_id);
                  return (
                    <div
                      className={`context-card compact ${item.final_failure ? "danger" : "warning"}`}
                      key={`${item.hook_id}:${item.event_id}`}
                    >
                      <strong>
                        {item.hook_id} / {item.trigger}
                      </strong>
                      <span>
                        event {item.event_id} - attempt {item.attempt} -{" "}
                        {item.final_failure ? "final" : "retrying"}
                        {persistentlyDisabled ? " - disabled for future runs" : ""}
                        {hookPolicyScopeSummary(item.hook_id)}
                      </span>
                      {hookPolicyConflictNote(item.hook_id) ? (
                        <p>{hookPolicyConflictNote(item.hook_id)}</p>
                      ) : null}
                      <p>{item.error}</p>
                      {item.policy_denials.length ? (
                        <p>{item.policy_denials.join(" | ")}</p>
                      ) : null}
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Load the original trace prompt into the composer."
                          onClick={loadTracePromptToComposer}
                          disabled={running || !traceOriginalPrompt(traceEvents)}
                        >
                          Load Prompt
                        </button>
                        <button
                          type="button"
                          title="Replay the original trace prompt with hooks enabled."
                          onClick={() => void replayTracePrompt()}
                          disabled={running || !traceOriginalPrompt(traceEvents)}
                        >
                          Replay
                        </button>
                        <button
                          type="button"
                          title="Replay the original trace prompt with lifecycle hooks skipped for this run."
                          onClick={() => void replayTracePromptWithoutHooks()}
                          disabled={running || !traceOriginalPrompt(traceEvents)}
                        >
                          Skip Hooks
                        </button>
                        <button
                          type="button"
                          title="Persistently skip or re-enable this lifecycle hook for future runs in the active profile."
                          onClick={() =>
                            void setPersistentHookDisabled(
                              item.hook_id,
                              !profileDisabled,
                              "profile",
                            )
                          }
                        >
                          {profileDisabled ? "Enable Profile" : "Disable Profile"}
                        </button>
                        <button
                          type="button"
                          title="Persistently skip or re-enable this lifecycle hook for the active agent config."
                          onClick={() =>
                            void setPersistentHookDisabled(
                              item.hook_id,
                              !agentDisabled,
                              "agent",
                            )
                          }
                        >
                          {agentDisabled ? "Enable Agent" : "Disable Agent"}
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            </section>
          ) : null}
          {traceEvents.length ? (
            <section className="trace-timeline">
              <strong>Run Timeline</strong>
              <div className="trace-timeline-list">
                {traceTimelineItems(traceEvents).map((item) => (
                  <div
                    className={`trace-timeline-item ${item.tone}`}
                    key={item.id}
                  >
                    <span className="trace-timeline-marker">{item.id}</span>
                    <div>
                      <strong>{item.title}</strong>
                      <span>{item.meta}</span>
                      <p>{item.detail}</p>
                    </div>
                  </div>
                ))}
              </div>
            </section>
          ) : null}
          {qualityScoresFromEvents(traceEvents).length ? (
            <section className="quality-score-list">
              <strong>Quality Scores</strong>
              <div className="context-cards">
                {qualityScoresFromEvents(traceEvents).map((record) => (
                  <div className="context-card compact" key={record.event_id}>
                    <strong>
                      {record.target} - {record.score}/10
                    </strong>
                    <span>
                      event {record.event_id} - {record.at}
                    </span>
                  </div>
                ))}
              </div>
            </section>
          ) : null}
          {traceEvents.length ? (
            <div className="trace-events">
              {traceEvents.map((event) => (
                <details key={`${event.run_id}:${event.id}`}>
                  <summary>
                    <span>[{event.id}]</span>
                    <strong>{event.kind.type}</strong>
                  </summary>
                  <pre>{previewJson(event)}</pre>
                </details>
              ))}
            </div>
          ) : null}
        </section>
        ) : null}

        {showOperationsPanel() ? (
        <section className="panel">
          <div className="panel-title">{operationsTitle()}</div>
          <label>
            Value
            <textarea
              className="ops-text"
              value={opsValue}
              onChange={(e) => setOpsValue(e.target.value)}
              placeholder="file path, text, or JSON payload"
              disabled={running}
              rows={3}
            />
          </label>
          <label>
            Id
            <input
              value={opsId}
              onChange={(e) => setOpsId(e.target.value)}
              placeholder="tool, model, prompt, conversation, artifact, skill, adapter, or batch id"
              disabled={running}
            />
          </label>
          <label className="switch">
            <input
              type="checkbox"
              checked={opsUserMemory}
              onChange={(e) => setOpsUserMemory(e.target.checked)}
              disabled={running}
            />
            <span>User memory</span>
          </label>
          <div className="operation-groups">
            {activeSection === "conversations" ? (
            <div className="operation-group">
              <div className="operation-title">Conversations</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List conversations and refresh the branch tree."
                  onClick={() => void reviewConversations()}
                  disabled={running}
                >
                  List
                </button>
                <button
                  type="button"
                  title="Show expanded conversation Id."
                  onClick={() => void showConversationFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show
                </button>
                <button
                  type="button"
                  title="Build a recovery plan and apply suggested run settings."
                  onClick={() => void recoverConversationFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Recover
                </button>
                <button
                  type="button"
                  title='Delete a leaf conversation message range using Value like { "from": 2, "to": 4 }.'
                  onClick={() => void deleteConversationRangeFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Delete Range
                </button>
                <button
                  type="button"
                  title="Preview which conversations would be deleted."
                  onClick={() => void previewConversationDeleteFromOps(false)}
                  disabled={running || !opsId.trim()}
                >
                  Plan Delete
                </button>
                <button
                  type="button"
                  title="Preview recursive deletion including child branches."
                  onClick={() => void previewConversationDeleteFromOps(true)}
                  disabled={running || !opsId.trim()}
                >
                  Plan Recursive
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete conversation Id if it has no child branches."
                  onClick={() => void deleteConversationFromOps(false)}
                  disabled={running || !opsId.trim()}
                >
                  Delete
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete conversation Id and all child branches."
                  onClick={() => void deleteConversationFromOps(true)}
                  disabled={running || !opsId.trim()}
                >
                  Delete Recursive
                </button>
                <button
                  type="button"
                  title="Preview deletion of all conversations owned by the active Agent id."
                  onClick={() => void previewConversationDeleteAgent()}
                  disabled={running || !agentId.trim()}
                >
                  Plan Agent
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete all conversations owned by the active Agent id."
                  onClick={() => void deleteConversationsForAgent()}
                  disabled={running || !agentId.trim()}
                >
                  Delete Agent
                </button>
              </div>
              {conversationDocs.length ? (
                <div className="empty-note">
                  Loaded {conversationDocs.length} conversation records.
                </div>
              ) : null}
              {conversationDeletePlan.length ? (
                <div className="ingestion-review">
                  <div className="ingestion-card high-risk">
                    <div className="ingestion-card-head">
                      <strong>Delete impact</strong>
                      <span>{conversationDeletePlan.length} conversations</span>
                    </div>
                    <div className="finding-list">
                      {conversationDeletePlan.map((id) => (
                        <span className="finding warning" key={id}>
                          {id}
                        </span>
                      ))}
                    </div>
                  </div>
                </div>
              ) : null}
              {conversationTree.length ? (
                <div className="conversation-tree-panel">
                  <div className="conversation-tree-summary">
                    <span>
                      <strong>{conversationStats.total}</strong> conversations
                    </span>
                    <span>
                      <strong>{conversationStats.roots}</strong> roots
                    </span>
                    <span>
                      <strong>{conversationStats.branchPoints}</strong> branch points
                    </span>
                    <span>
                      <strong>{conversationStats.leaves}</strong> leaves
                    </span>
                    <span>
                      <strong>{conversationStats.maxDepth}</strong> max depth
                    </span>
                  </div>
                  <div className="conversation-tree-list">
                    {flattenConversationTree(conversationTree).map(({ node, depth }) => (
                      <div
                        className={`conversation-tree-row${
                          expandedConversation?.conversation.id === node.id ? " selected" : ""
                        }`}
                        key={node.id}
                        style={{ paddingLeft: `${Math.min(depth, 6) * 0.85}rem` }}
                      >
                        <div className="conversation-tree-rail" aria-hidden="true">
                          <span>{depth}</span>
                        </div>
                        <div className="conversation-tree-card">
                          <div className="conversation-tree-head">
                            <strong>{node.title}</strong>
                            <span>{depth ? `branch ${depth}` : "root"}</span>
                          </div>
                          <div className="conversation-tree-meta">
                            <span>{node.id}</span>
                            <span>agent {node.agent_id}</span>
                            <span>{node.own_message_count} own</span>
                            <span>{node.expanded_message_count} expanded</span>
                            {node.branch_point !== undefined && node.branch_point !== null ? (
                              <span>branch point {node.branch_point}</span>
                            ) : null}
                            <span>
                              {node.children.length
                                ? `${node.children.length} children`
                                : "leaf"}
                            </span>
                          </div>
                          {node.parent_id ? (
                            <span className="conversation-tree-parent">
                              parent {node.parent_id}
                            </span>
                          ) : null}
                          {node.topic_preview ? (
                            <p>
                              <span className="conversation-tree-note-label">topic:</span>{" "}
                              {previewText(node.topic_preview, 180)}
                            </p>
                          ) : null}
                          {node.branch_reason ? (
                            <p>
                              <span className="conversation-tree-note-label">reason:</span>{" "}
                              {previewText(node.branch_reason, 180)}
                            </p>
                          ) : null}
                          <div className="mini-actions">
                            <button
                              type="button"
                              title="Move this conversation id into the Id field."
                              onClick={() => setOpsId(node.id)}
                              disabled={running}
                            >
                              Set Id
                            </button>
                            <button
                              type="button"
                              title="Show expanded conversation messages."
                              onClick={() => void showConversation(node.id)}
                              disabled={running}
                            >
                              Show
                            </button>
                            <button
                              type="button"
                              title="Build a recovery plan and apply suggested run settings."
                              onClick={() => void recoverConversation(node.id)}
                              disabled={running}
                            >
                              Recover
                            </button>
                            <button
                              type="button"
                              title="Preview deletion impact for this branch."
                              onClick={() => void previewConversationDelete(node.id, false)}
                              disabled={running}
                            >
                              Plan
                            </button>
                            <button
                              type="button"
                              title="Preview recursive deletion impact for this branch."
                              onClick={() => void previewConversationDelete(node.id, true)}
                              disabled={running}
                            >
                              Plan Rec
                            </button>
                            <button
                              type="button"
                              className="danger"
                              title="Delete this leaf conversation."
                              onClick={() => void deleteConversation(node.id, false)}
                              disabled={running}
                            >
                              Delete
                            </button>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <div className="empty-note">
                  No conversation tree loaded. List conversations to review branches.
                </div>
              )}
            </div>
            ) : null}

            {activeSection === "conversations" && expandedConversation ? (
            <div className="operation-group">
              <div className="operation-title">Selected Conversation</div>
              <div className="ingestion-review">
                <div className="ingestion-card">
                  <div className="ingestion-card-head">
                    <strong>{expandedConversation.conversation.title}</strong>
                    <span>{expandedConversation.messages.length} messages</span>
                  </div>
                  <span>{expandedConversation.conversation.id}</span>
                  <span>agent {expandedConversation.conversation.agent_id}</span>
                  <span>
                    policy{" "}
                    {conversationPolicySummary(
                      expandedConversation.conversation.policy,
                    )}
                  </span>
                  {expandedConversation.conversation.parent ? (
                    <span>
                      parent {expandedConversation.conversation.parent.conversation_id} @{" "}
                      {expandedConversation.conversation.parent.parent_message_count}
                    </span>
                  ) : null}
                  {expandedConversation.conversation.branch_reason ? (
                    <p>
                      {previewText(
                        expandedConversation.conversation.branch_reason,
                        220,
                      )}
                    </p>
                  ) : null}
                  <div className="mini-actions">
                    <button
                      type="button"
                      title="Move this conversation id into the Id field."
                      onClick={() => setOpsId(expandedConversation.conversation.id)}
                      disabled={running}
                    >
                      Set Id
                    </button>
                    <button
                      type="button"
                      title="Preview recursive deletion impact."
                      onClick={() =>
                        void previewConversationDelete(
                          expandedConversation.conversation.id,
                          true,
                        )
                      }
                      disabled={running}
                    >
                      Plan Recursive
                    </button>
                    <button
                      type="button"
                      title="Build a recovery plan and apply suggested run settings."
                      onClick={() =>
                        void recoverConversation(expandedConversation.conversation.id)
                      }
                      disabled={running}
                    >
                      Recover
                    </button>
                    <button
                      type="button"
                      title="Copy this conversation policy into the context controls."
                      onClick={() => applySelectedConversationPolicy()}
                      disabled={running || !expandedConversation.conversation.policy}
                    >
                      Apply Policy
                    </button>
                    <button
                      type="button"
                      title="Save current context controls as this conversation's policy."
                      onClick={() => void saveSelectedConversationPolicy()}
                      disabled={running}
                    >
                      Save Policy
                    </button>
                    <button
                      type="button"
                      title="Clear this conversation's context policy overrides."
                      onClick={() => void clearSelectedConversationPolicy()}
                      disabled={running}
                    >
                      Clear Policy
                    </button>
                  </div>
                </div>
                {expandedConversation.messages.map((message, index) => (
                  <div
                    className="memory-card"
                    key={`${expandedConversation.conversation.id}:${index}:${message.created_at}`}
                  >
                    <div className="memory-card-head">
                      <strong>{conversationMessageTitle(message, index)}</strong>
                      <span>{message.role}</span>
                    </div>
                    <p>{previewText(message.content, 420)}</p>
                    <div className="mini-actions">
                      <button
                        type="button"
                        title="Stage this single message index for range actions."
                        onClick={() =>
                          setOpsValue(JSON.stringify({ from: index, to: index }))
                        }
                        disabled={running}
                      >
                        Set Range
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
            ) : null}

            {activeSection === "profiles" ? (
            <div className="operation-group">
              <div className="operation-title">Profiles</div>
              <label>
                Secret label
                <input
                  value={secretLabel}
                  onChange={(e) => setSecretLabel(e.target.value)}
                  placeholder="optional display label"
                  disabled={running}
                />
              </label>
              <div className="button-grid">
                <button
                  type="button"
                  title="Show the active profile selected by the current harness environment."
                  onClick={() => void showCurrentProfileFromOps()}
                  disabled={running}
                >
                  Current
                </button>
                <button
                  type="button"
                  title="List local profile records."
                  onClick={() => void listProfilesFromOps()}
                  disabled={running}
                >
                  List
                </button>
                <button
                  type="button"
                  title="Show profile Id."
                  onClick={() => void showProfileFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show
                </button>
                <button
                  type="button"
                  title="Create profile Id, using Value as the optional display name."
                  onClick={() => void createProfileFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Create
                </button>
                <button
                  type="button"
                  className="danger"
                  title={
                    currentProfile?.id === opsId.trim()
                      ? "The active profile cannot be deleted from this session."
                      : opsId.trim() === "main"
                        ? "The main profile cannot be deleted."
                        : "Delete profile Id."
                  }
                  onClick={() => void deleteProfileFromOps()}
                  disabled={
                    running ||
                    !opsId.trim() ||
                    opsId.trim() === "main" ||
                    currentProfile?.id === opsId.trim()
                  }
                >
                  Delete
                </button>
                <button
                  type="button"
                  title="List all profile grants, or grants from profile Id when Id is set."
                  onClick={() => void listProfileGrantsForOps()}
                  disabled={running}
                >
                  List Grants
                </button>
                <button
                  type="button"
                  title='Grant to profile Id using Value JSON like { "kind": "memory", "resource": "critic" }.'
                  onClick={() => void grantProfileFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Grant
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Revoke profile grant Id."
                  onClick={() => void revokeProfileGrantFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Revoke Grant
                </button>
                <button
                  type="button"
                  title="Export the active profile/config/cache bundle to a timestamped path."
                  onClick={() => void backupBundleNow()}
                  disabled={running}
                >
                  Backup
                </button>
                <button
                  type="button"
                  title="Export the active profile/config/cache bundle to the path in Value."
                  onClick={() => void exportBundleFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Export Bundle
                </button>
                <button
                  type="button"
                  title="Import a profile/config/cache bundle from the path in Value."
                  onClick={() => void importBundleFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Bundle
                </button>
                <button
                  type="button"
                  title="List secret storage backends."
                  onClick={() => void listSecretBackendsFromOps()}
                  disabled={running}
                >
                  Secret Backends
                </button>
                <button
                  type="button"
                  title="List redacted secret metadata."
                  onClick={() => void listSecretsFromOps()}
                  disabled={running}
                >
                  List Secrets
                </button>
                <button
                  type="button"
                  title="Show redacted metadata for secret Id."
                  onClick={() => void showSecretFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Secret
                </button>
                <button
                  type="button"
                  title="Store secret Id using Value as the secret value."
                  onClick={() => void setSecretFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Store Secret
                </button>
                <button
                  type="button"
                  title="Rotate secret Id using Value as the new secret value."
                  onClick={() => void rotateSecretFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Rotate Secret
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete secret Id."
                  onClick={() => void deleteSecretFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Secret
                </button>
              </div>
              {currentProfile ? (
                <div className="empty-note">
                  Active profile: {currentProfile.name || currentProfile.id} ({currentProfile.id})
                </div>
              ) : null}
              {profileSummaries.length ? (
                <div className="ingestion-review">
                  {profileSummaries.map((profile) => (
                    <div className="ingestion-card" key={profile.id}>
                      <div className="ingestion-card-head">
                        <strong>{profile.name || profile.id}</strong>
                        <span>{profile.id}</span>
                      </div>
                      <span title={profile.path}>{fileName(profile.path)}</span>
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this profile id into the Id field."
                          onClick={() => setOpsId(profile.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Show this profile record."
                          onClick={() => void showProfileFromOps(profile.id)}
                          disabled={running}
                        >
                          Show
                        </button>
                        <button
                          type="button"
                          title="List grants from this profile."
                          onClick={() => void listProfileGrantsFromOps(profile.id)}
                          disabled={running}
                        >
                          Grants
                        </button>
                        <button
                          type="button"
                          className="danger"
                          title={
                            currentProfile?.id === profile.id
                              ? "The active profile cannot be deleted from this session."
                              : profile.id === "main"
                                ? "The main profile cannot be deleted."
                                : "Delete this profile."
                          }
                          onClick={() => void deleteProfileFromOps(profile.id)}
                          disabled={
                            running ||
                            profile.id === "main" ||
                            currentProfile?.id === profile.id
                          }
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
              {secretBackends.length ? (
                <div className="ingestion-review">
                  {secretBackends.map((backend) => (
                    <div className="ingestion-card" key={backend.id}>
                      <div className="ingestion-card-head">
                        <strong>{backend.name}</strong>
                        <span>{backend.active ? "active" : backend.id}</span>
                      </div>
                      <span>{backend.description}</span>
                      <span>{backend.supported ? "supported" : "unsupported"}</span>
                    </div>
                  ))}
                </div>
              ) : null}
              {secretRecords.length ? (
                <div className="ingestion-review">
                  {secretRecords.map((record) => (
                    <div className="ingestion-card" key={record.id}>
                      <div className="ingestion-card-head">
                        <strong>{record.label || record.id}</strong>
                        <span>
                          {record.backend} v{record.current_version}
                        </span>
                      </div>
                      <span>{record.id}</span>
                      <span title={record.value_fingerprint}>
                        fingerprint {record.value_fingerprint}
                      </span>
                      <span>{record.updated_at}</span>
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this secret id into the Id field."
                          onClick={() => setOpsId(record.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Show this secret metadata."
                          onClick={() => void showSecretFromOps(record.id)}
                          disabled={running}
                        >
                          Show
                        </button>
                        <button
                          type="button"
                          className="danger"
                          title="Delete this secret."
                          onClick={() => void deleteSecretFromOps(record.id)}
                          disabled={running}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
              {secretStatus ? (
                <div className="bundle-card">
                  <div className="bundle-card-head">
                    <strong>Secret result</strong>
                    <span>redacted</span>
                  </div>
                  <pre>{JSON.stringify(secretStatus, null, 2)}</pre>
                </div>
              ) : null}
              {profileGrants.length ? (
                <div className="ingestion-review">
                  {profileGrants.map((grant) => (
                    <div className="ingestion-card" key={grant.id}>
                      <div className="ingestion-card-head">
                        <strong>
                          {grant.from_profile} to {grant.to_profile}
                        </strong>
                        <span>{grant.kind}</span>
                      </div>
                      <span>{grant.resource}</span>
                      <span>{grant.id}</span>
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this grant id into the Id field."
                          onClick={() => setOpsId(grant.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Revoke this profile grant."
                          className="danger"
                          onClick={() => void revokeProfileGrantFromOps(grant.id)}
                          disabled={running}
                        >
                          Revoke
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
              {bundleStatus ? (
                <div className="bundle-card">
                  <div className="bundle-card-head">
                    <strong>Bundle {bundleStatus.operation}</strong>
                    <span>schema {bundleStatus.manifest.schema_version}</span>
                  </div>
                  <span title={bundleStatus.path}>{bundleStatus.path}</span>
                  <span>profile {bundleStatus.manifest.profile}</span>
                  <span>{bundleStatus.manifest.exported_at}</span>
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "memory" ? (
            <div className="operation-group">
              <div className="operation-title">Memory</div>
              <label>
                Source range
                <input
                  value={memorySourceRange}
                  onChange={(e) => setMemorySourceRange(e.target.value)}
                  placeholder="optional conversation range label"
                  title="Stored on generated memory records as source_range."
                  disabled={running}
                />
              </label>
              <label>
                Topics
                <input
                  value={memoryTopics}
                  onChange={(e) => setMemoryTopics(e.target.value)}
                  placeholder="finance, ops"
                  disabled={running}
                />
              </label>
              <label>
                Classification model
                <input
                  value={memoryClassificationModel}
                  onChange={(e) => setMemoryClassificationModel(e.target.value)}
                  placeholder="saved model id or env default"
                  disabled={running}
                />
              </label>
              <div className="button-grid">
                <button
                  type="button"
                  title="List stored memory records."
                  onClick={() => void reviewMemory()}
                  disabled={running}
                >
                  List Memory
                </button>
                <button
                  type="button"
                  title="Show local and profile-granted memory matching Topics."
                  onClick={() => void reviewMemoryAccess()}
                  disabled={running}
                >
                  Access
                </button>
                <button
                  type="button"
                  title="List available memory backends."
                  onClick={() => void reviewMemoryBackends()}
                  disabled={running}
                >
                  Backends
                </button>
                <button
                  type="button"
                  title="Turn memory loading on and preview the next agent context."
                  onClick={() => void previewWithMemoryFromOps()}
                  disabled={running}
                >
                  Preview With Memory
                </button>
                <button
                  type="button"
                  title="Create a memory from Value."
                  onClick={() => void createMemoryFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Add Memory
                </button>
                <button
                  type="button"
                  title="Generate memory candidates from Value."
                  onClick={() => void generateMemoryFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Generate
                </button>
                <button
                  type="button"
                  title='Generate memory from the expanded conversation range in Value, like { "from": 2, "to": 4 }.'
                  onClick={() => void generateConversationMemoryFromOps()}
                  disabled={
                    running || !expandedConversation || !opsValue.trim()
                  }
                >
                  Generate Range
                </button>
                <button
                  type="button"
                  title="Classify memory Id with the selected model."
                  onClick={() => void classifyMemoryFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Classify
                </button>
                <button
                  type="button"
                  title="Edit memory Id with Value."
                  onClick={() => void editMemoryFromOps()}
                  disabled={running || !opsValue.trim() || !opsId.trim()}
                >
                  Edit Mem
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete memory Id."
                  onClick={() => void deleteMemoryFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Mem
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Roll back the selected memory file."
                  onClick={() => void rollbackMemoryFromOps()}
                  disabled={running}
                >
                  Rollback
                </button>
              </div>
              {memoryBackends.length ? (
                <div className="memory-review">
                  {memoryBackends.map((backend) => (
                    <div className="memory-card" key={backend.id}>
                      <div className="memory-card-head">
                        <strong>{backend.id}</strong>
                        <span>{backend.name}</span>
                      </div>
                      <div className="memory-meta">
                        <span>
                          {backend.supports_generation
                            ? "generation"
                            : "no generation"}
                        </span>
                        <span>
                          {backend.supports_rollback ? "rollback" : "no rollback"}
                        </span>
                        <span title={backend.storage}>
                          storage {fileName(backend.storage)}
                        </span>
                      </div>
                      <p>{backend.description}</p>
                    </div>
                  ))}
                </div>
              ) : null}
              {memoryRecords.length ? (
                <div className="memory-review">
                  {memoryRecords.map((record) => (
                    <div className="memory-card" key={record.id}>
                      <div className="memory-card-head">
                        <strong>{record.id}</strong>
                        <span>{record.target}</span>
                      </div>
                      <div className="memory-meta">
                        <span>{record.author}</span>
                        <span>profile {record.owning_profile}</span>
                        {record.owning_agent ? (
                          <span>agent {record.owning_agent}</span>
                        ) : null}
                        {record.source_range ? (
                          <span>range {record.source_range}</span>
                        ) : null}
                        {record.generating_model ? (
                          <span>model {record.generating_model}</span>
                        ) : null}
                        {record.topics?.length ? (
                          <span>topics {record.topics.join(", ")}</span>
                        ) : null}
                        {record.classification?.tasks?.length ? (
                          <span>tasks {record.classification.tasks.join(", ")}</span>
                        ) : null}
                        {record.classification?.source ? (
                          <span>class {record.classification.source}</span>
                        ) : null}
                      </div>
                      <p>{previewText(record.content)}</p>
                      <div className="memory-meta">
                        <span>created {record.created_at}</span>
                        <span>updated {record.updated_at}</span>
                      </div>
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this memory id into the Id field."
                          onClick={() => setOpsId(record.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Load this memory content into Value for editing."
                          onClick={() => {
                            setOpsId(record.id);
                            setOpsValue(record.content);
                          }}
                          disabled={running}
                        >
                          Edit
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "prompts" ? (
            <div className="operation-group">
              <div className="operation-title">Prompts</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List saved prompts."
                  onClick={() => void reviewPrompts()}
                  disabled={running}
                >
                  List Prompts
                </button>
                <button
                  type="button"
                  title="Save prompt Id with Value as the prompt body."
                  onClick={() => void savePromptFromOps()}
                  disabled={running || !opsValue.trim() || !opsId.trim()}
                >
                  Save Prompt
                </button>
                <button
                  type="button"
                  title="Show saved prompt Id."
                  onClick={() => void showPromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Prompt
                </button>
                <button
                  type="button"
                  title="Load saved prompt Id into the composer."
                  onClick={() => void usePromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Use Prompt
                </button>
                <button
                  type="button"
                  title="Run saved prompt Id immediately."
                  onClick={() => void runPromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Run Prompt
                </button>
                <button
                  type="button"
                  title="Preview the exact context for saved prompt Id."
                  onClick={() => void previewPromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Preview Prompt
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete saved prompt Id."
                  onClick={() => void deletePromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Prompt
                </button>
              </div>
            </div>
            ) : null}

            {activeSection === "prompts" ? (
            <div className="operation-group">
              <div className="operation-title">Prompt Library</div>
              {promptDocs.length ? (
                <div className="ingestion-review">
                  {promptDocs.map((prompt) => (
                    <div className="ingestion-card" key={prompt.name}>
                      <div className="ingestion-card-head">
                        <strong>{prompt.name}</strong>
                        <span>
                          {prompt.agent_id ? `agent ${prompt.agent_id}` : "profile"} /{" "}
                          {prompt.body.length} chars
                        </span>
                      </div>
                      <p>{previewText(prompt.body, 220)}</p>
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Load this saved prompt into the composer."
                          onClick={() => void usePromptByName(prompt.name)}
                          disabled={running}
                        >
                          Use
                        </button>
                        <button
                          type="button"
                          title="Run this saved prompt immediately."
                          onClick={() =>
                            void runPromptByName(prompt.name, prompt.body)
                          }
                          disabled={running}
                        >
                          Run
                        </button>
                        <button
                          type="button"
                          title="Preview the exact context for this saved prompt."
                          onClick={() =>
                            void previewPromptByName(prompt.name, prompt.body)
                          }
                          disabled={running}
                        >
                          Preview
                        </button>
                        <button
                          type="button"
                          title="Move this prompt into the edit fields."
                          onClick={() => {
                            setOpsId(prompt.name);
                            setOpsValue(prompt.body);
                          }}
                          disabled={running}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          title="Move this prompt name into the Id field."
                          onClick={() => setOpsId(prompt.name)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          className="danger"
                          title="Delete this saved prompt."
                          onClick={() => void deletePromptByName(prompt.name)}
                          disabled={running}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="empty-note">
                  No saved prompts loaded. List prompts, or save one with Id and Value.
                </div>
              )}
            </div>
            ) : null}

            {activeSection === "prompts" ? (
            <div className="operation-group">
              <div className="operation-title">Models</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List configured model metadata."
                  onClick={() => void listModelsFromOps()}
                  disabled={running}
                >
                  List Models
                </button>
                <button
                  type="button"
                  title="List provider defaults and capability hints."
                  onClick={() => void listModelProvidersFromOps()}
                  disabled={running}
                >
                  Providers
                </button>
                <button
                  type="button"
                  title="Check saved models against providers and metadata catalogs."
                  onClick={() => void modelDoctorFromOps()}
                  disabled={running}
                >
                  Doctor
                </button>
                <button
                  type="button"
                  title="Show the active profile provider catalog JSON."
                  onClick={() => void showModelProviderCatalogFromOps()}
                  disabled={running}
                >
                  Provider Catalog
                </button>
                <button
                  type="button"
                  title="Show the active profile metadata catalog JSON."
                  onClick={() => void showModelMetadataCatalogFromOps()}
                  disabled={running}
                >
                  Metadata Catalog
                </button>
                <button
                  type="button"
                  title="Show model Id."
                  onClick={() => void showModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Model
                </button>
                <button
                  type="button"
                  title="Probe declared and live capabilities for model Id."
                  onClick={() => void probeModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Probe Model
                </button>
                <button
                  type="button"
                  title="Save model Id using Value as a JSON metadata object."
                  onClick={() => void saveModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Save Model
                </button>
                <button
                  type="button"
                  title="Save the current provider, model, API settings, and provider options."
                  onClick={() => void saveCurrentModelFromControls()}
                  disabled={running || provider === "fake"}
                >
                  Save Current
                </button>
                <button
                  type="button"
                  title="Export model Id to the path in Value."
                  onClick={() => void exportModelFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Export Model
                </button>
                <button
                  type="button"
                  title="Import model metadata from the path in Value."
                  onClick={() => void importModelFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Model
                </button>
                <button
                  type="button"
                  title="Export provider catalog JSON to the path in Value."
                  onClick={() => void exportModelProviderCatalogFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Export Providers
                </button>
                <button
                  type="button"
                  title="Import provider catalog JSON from the path in Value."
                  onClick={() => void importModelProviderCatalogFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Providers
                </button>
                <button
                  type="button"
                  title="Export metadata catalog JSON to the path in Value."
                  onClick={() => void exportModelMetadataCatalogFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Export Metadata
                </button>
                <button
                  type="button"
                  title="Import metadata catalog JSON from the path in Value."
                  onClick={() => void importModelMetadataCatalogFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Metadata
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete model Id."
                  onClick={() => void deleteModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Model
                </button>
              </div>
            </div>
            ) : null}

            {activeSection === "skills" ? (
            <div className="operation-group">
              <div className="operation-title">Skills</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List imported skills and their review status."
                  onClick={() => void reviewSkills()}
                  disabled={running}
                >
                  List Skills
                </button>
                <button
                  type="button"
                  title="Import a SKILL.md file or folder path from Value."
                  onClick={() => void importSkillFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Skill
                </button>
                <button
                  type="button"
                  title="Show quarantined or allowed skill Id."
                  onClick={() => void showSkillFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Skill
                </button>
                <button
                  type="button"
                  title="Turn skill loading on and preview the next agent context."
                  onClick={() => void previewWithSkillsFromOps()}
                  disabled={running}
                >
                  Preview With Skills
                </button>
                <button
                  type="button"
                  title="Allow quarantined skill Id into context after digest and prompt-injection checks pass."
                  onClick={() => void setSkillQuarantine(true)}
                  disabled={running || !opsId.trim()}
                >
                  Allow Skill
                </button>
                <button
                  type="button"
                  title="Quarantine skill Id."
                  onClick={() => void setSkillQuarantine(false)}
                  disabled={running || !opsId.trim()}
                >
                  Quarantine
                </button>
              </div>
              <div className="operation-title">Capability Drafts</div>
              <label>
                Draft kind
                <select
                  value={capabilityKind}
                  onChange={(e) =>
                    setCapabilityKind(e.target.value as CapabilityKind)
                  }
                  disabled={running}
                >
                  <option value="skill">skill</option>
                  <option value="tool">tool</option>
                  <option value="agent">agent</option>
                  <option value="subagent">subagent</option>
                </select>
              </label>
              <div className="button-grid">
                <button
                  type="button"
                  title="List quarantined, allowed, and rejected capability drafts."
                  onClick={() => void reviewCapabilities()}
                  disabled={running}
                >
                  List Drafts
                </button>
                <button
                  type="button"
                  title="Create a quarantined draft from Id as name and Value as body."
                  onClick={() => void proposeCapabilityFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Propose Draft
                </button>
                <button
                  type="button"
                  title="Show capability draft Id."
                  onClick={() => void showCapabilityFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Draft
                </button>
                <button
                  type="button"
                  title="Export capability draft Id to Value path."
                  onClick={() => void exportCapabilityFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Export Draft
                </button>
                <button
                  type="button"
                  title="Import a capability draft from Value path. Imported drafts stay quarantined."
                  onClick={() => void importCapabilityFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Draft
                </button>
                <button
                  type="button"
                  title="Allow capability draft Id after review."
                  onClick={() => void reviewCapabilityDraft(true)}
                  disabled={running || !opsId.trim()}
                >
                  Allow Draft
                </button>
                <button
                  type="button"
                  title="Reject capability draft Id and quarantine its promoted capability if present."
                  onClick={() => void reviewCapabilityDraft(false)}
                  disabled={running || !opsId.trim()}
                >
                  Reject Draft
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete capability draft Id."
                  onClick={() => void deleteCapabilityFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Draft
                </button>
              </div>
              {capabilityDrafts.length ? (
                <div className="ingestion-review">
                  {capabilityDrafts.map((draft) => (
                    <div className="ingestion-card" key={draft.id}>
                      <div className="ingestion-card-head">
                        <strong>{draft.name}</strong>
                        <span>{draft.status}</span>
                      </div>
                      <span>{draft.id}</span>
                      <span>{draft.kind}</span>
                      <span>{draft.created_by}</span>
                      <p>{previewText(draft.body)}</p>
                      {draft.guidance ? <p>{previewText(draft.guidance)}</p> : null}
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this draft id into the Id field."
                          onClick={() => setOpsId(draft.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Load this draft into the edit fields."
                          onClick={() => {
                            setOpsId(draft.name);
                            setOpsValue(draft.body);
                            setCapabilityKind(draft.kind);
                          }}
                          disabled={running}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          title="Allow this draft after review."
                          onClick={() => {
                            setOpsId(draft.id);
                            void reviewCapabilityDraft(true, draft.id);
                          }}
                          disabled={running || draft.status === "allowed"}
                        >
                          Allow
                        </button>
                        <button
                          type="button"
                          title="Reject this draft."
                          onClick={() => {
                            setOpsId(draft.id);
                            void reviewCapabilityDraft(false, draft.id);
                          }}
                          disabled={running || draft.status === "rejected"}
                        >
                          Reject
                        </button>
                        <button
                          type="button"
                          title="Export this draft to the Value path."
                          onClick={() => {
                            setOpsId(draft.id);
                            void exportCapabilityFromOps(draft.id);
                          }}
                          disabled={running || !opsValue.trim()}
                        >
                          Export
                        </button>
                        <button
                          type="button"
                          className="danger"
                          title="Delete this draft."
                          onClick={() => void deleteCapabilityFromOps(draft.id)}
                          disabled={running}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
              {skillDocs.length ? (
                <div className="ingestion-review">
                  {skillDocs.map((skill) => {
                    const findings = skill.findings ?? [];
                    const highRisk = hasHighRiskSkillFindings(skill);
                    return (
                      <div
                        className={`ingestion-card ${highRisk ? "high-risk" : ""}`}
                        key={skill.id}
                      >
                        <div className="ingestion-card-head">
                          <strong>{skill.name}</strong>
                          <span>
                            {skill.quarantined ? "quarantined" : "allowed"}
                          </span>
                        </div>
                        <span>{skill.id}</span>
                        {skill.source_path ? (
                          <span title={skill.source_path}>
                            source {fileName(skill.source_path)}
                          </span>
                        ) : (
                          <span className="finding none">no source path</span>
                        )}
                        {skill.digest ? (
                          <span title={skill.digest}>
                            digest {skill.digest.slice(0, 16)}
                          </span>
                        ) : (
                          <span className="finding warning">
                            legacy skill without digest pin
                          </span>
                        )}
                        <span>~{skill.estimated_tokens} tokens</span>
                        {highRisk ? (
                          <span className="finding high">
                            activation blocked by high-risk prompt-injection findings
                          </span>
                        ) : null}
                        {findings.length ? (
                          <div className="finding-list">
                            {findings.map((finding) => (
                              <span
                                className={`finding ${finding.severity}`}
                                key={`${skill.id}:${finding.severity}:${finding.message}`}
                              >
                                {finding.severity}: {finding.message}
                              </span>
                            ))}
                          </div>
                        ) : (
                          <span className="finding none">no scan findings</span>
                        )}
                        <p>{skill.description}</p>
                        <p>{previewText(skill.body)}</p>
                        <div className="mini-actions">
                          <button
                            type="button"
                            title="Move this skill id into the Id field."
                            onClick={() => setOpsId(skill.id)}
                            disabled={running}
                          >
                            Set Id
                          </button>
                          <button
                            type="button"
                            title={
                              highRisk
                                ? "High-risk prompt-injection findings block activation."
                                : "Allow this skill if its source digest and prompt-injection checks pass."
                            }
                            onClick={() => {
                              setOpsId(skill.id);
                              void setSkillQuarantine(true, skill.id);
                            }}
                            disabled={running || !skill.quarantined || highRisk}
                          >
                            Allow
                          </button>
                          <button
                            type="button"
                            title="Keep this skill out of context."
                            onClick={() => {
                              setOpsId(skill.id);
                              void setSkillQuarantine(false, skill.id);
                            }}
                            disabled={running || skill.quarantined}
                          >
                            Quarantine
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "ingest" ? (
            <div className="operation-group">
              <div className="operation-title">Ingestion</div>
              <label>
                Backend
                <select
                  value={ingestBackend}
                  onChange={(e) => setIngestBackend(e.target.value)}
                  disabled={running}
                >
                  {(ingestionBackends.length
                    ? ingestionBackends
                    : [
                        {
                          id: "local-v0",
                          name: "Local Text",
                          description: "",
                          modalities: [],
                        },
                        {
                          id: "local-lines-v0",
                          name: "Local Lines",
                          description: "",
                          modalities: [],
                        },
                        {
                          id: "local-structured-v0",
                          name: "Local Structured",
                          description: "",
                          modalities: [],
                        },
                        {
                          id: "local-layout-v0",
                          name: "Local Layout/OCR",
                          description: "",
                          modalities: [],
                        },
                      ]
                  ).map((backend) => (
                    <option value={backend.id} key={backend.id}>
                      {backend.id}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Vision model
                <input
                  value={ingestVisionModel}
                  onChange={(e) => setIngestVisionModel(e.target.value)}
                  placeholder="optional OCR/layout model"
                  disabled={running}
                />
              </label>
              <label>
                Guardrail model
                <input
                  value={ingestGuardrailModel}
                  onChange={(e) => setIngestGuardrailModel(e.target.value)}
                  placeholder="profile default"
                  disabled={running}
                />
              </label>
              <label>
                Finding index
                <input
                  value={ingestFindingIndex}
                  onChange={(e) => setIngestFindingIndex(e.target.value)}
                  placeholder="0"
                  disabled={running}
                />
              </label>
              <label>
                Review decision
                <select
                  value={ingestReviewDecision}
                  onChange={(e) =>
                    setIngestReviewDecision(
                      e.target.value as IngestionFindingReviewDecision,
                    )
                  }
                  disabled={running}
                >
                  <option value="approve">approve</option>
                  <option value="acknowledge">acknowledge</option>
                  <option value="reject">reject</option>
                </select>
              </label>
              <label>
                Review note
                <input
                  value={ingestReviewNote}
                  onChange={(e) => setIngestReviewNote(e.target.value)}
                  placeholder="optional"
                  disabled={running}
                />
              </label>
              <div className="button-grid">
                <button
                  type="button"
                  title="List available ingestion backends."
                  onClick={() => void reviewIngestionBackends()}
                  disabled={running}
                >
                  Backends
                </button>
                <button
                  type="button"
                  title="List ingestion artifacts."
                  onClick={() => void reviewIngestion()}
                  disabled={running}
                >
                  List Ingest
                </button>
                <button
                  type="button"
                  title="Ingest the file path in Value using the selected backend."
                  onClick={() => void ingestPathFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Ingest
                </button>
                <button
                  type="button"
                  title="Show ingestion artifact Id."
                  onClick={() => void showIngestFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Ingest
                </button>
                <button
                  type="button"
                  title="Re-run ingestion artifact Id with the selected backend."
                  onClick={() => void rerunIngestFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Rerun Ingest
                </button>
                <button
                  type="button"
                  title="Include ingestion artifact Id in the next run context."
                  onClick={() => includeIngestFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Use Ingest
                </button>
                <button
                  type="button"
                  title="Include artifact Id and preview the next agent context."
                  onClick={() => void previewWithIngestFromOps()}
                  disabled={
                    running || (!opsId.trim() && !includeIngestIds.length)
                  }
                >
                  Preview With Ingest
                </button>
                <button
                  type="button"
                  title="Review prompt-injection and unsafe-ingest guardrail status."
                  onClick={() => appendLine("assistant", guardrailReport())}
                  disabled={running}
                >
                  Guardrails
                </button>
                <button
                  type="button"
                  title="Review finding index on ingestion artifact Id."
                  onClick={() => void reviewIngestFindingFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Review Finding
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Remove ingestion artifact Id."
                  onClick={() => void removeIngestFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Remove Ingest
                </button>
              </div>
              {ingestionBackends.length ? (
                <div className="ingestion-review">
                  {ingestionBackends.map((backend) => (
                    <div className="ingestion-card" key={backend.id}>
                      <div className="ingestion-card-head">
                        <strong>{backend.id}</strong>
                        <span>{backend.name}</span>
                      </div>
                      <span>
                        {backend.modalities.length
                          ? backend.modalities.join(", ")
                          : "no modalities"}
                      </span>
                      <p>{backend.description}</p>
                      {backend.compatibility?.length ? (
                        <div className="finding-list">
                          {backend.compatibility.map((item) => {
                            const meta = ingestionCompatibilityMeta(item);
                            return (
                              <span
                                className="finding"
                                key={`${backend.id}:${item.source_kind}:${item.extraction}`}
                                title={item.notes}
                              >
                                {item.source_kind} {"->"} {item.extraction}
                                {meta ? ` / ${meta}` : ""}
                                {item.notes ? ` / ${item.notes}` : ""}
                              </span>
                            );
                          })}
                        </div>
                      ) : null}
                    </div>
                  ))}
                </div>
              ) : null}
              {ingestionArtifacts.length ? (
                <div className="ingestion-review">
                  {ingestionArtifacts.map((artifact) => (
                    <div
                      className={`ingestion-card ${
                        hasUnapprovedHighRiskFindings(artifact)
                          ? "high-risk"
                          : ""
                      }`}
                      key={artifact.id}
                    >
                      <div className="ingestion-card-head">
                        <strong>{artifact.id}</strong>
                        <span>{artifact.backend}</span>
                      </div>
                      <span title={artifact.source}>
                        {fileName(artifact.source)} / {artifact.sections.length} sections
                      </span>
                      {artifact.findings.length ? (
                        <div className="finding-list">
                          {artifact.findings.map((finding, index) => {
                            const review = reviewForFinding(artifact, index);
                            return (
                              <span
                                className={`finding ${finding.severity}`}
                                key={`${artifact.id}:${index}:${finding.severity}:${finding.message}`}
                              >
                                #{index} {finding.severity}: {finding.message}
                                {review
                                  ? ` (${review.decision}${review.note ? `: ${review.note}` : ""})`
                                  : ""}
                              </span>
                            );
                          })}
                        </div>
                      ) : (
                        <span className="finding none">no findings</span>
                      )}
                      {artifact.extracted_text ? (
                        <p>{previewText(artifact.extracted_text)}</p>
                      ) : null}
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Use this artifact in the next context preview or run."
                          onClick={() => includeIngestId(artifact.id)}
                          disabled={running}
                        >
                          Use
                        </button>
                        <button
                          type="button"
                          title="Preview the next agent context with this artifact."
                          onClick={() => void previewWithIngest(artifact.id)}
                          disabled={running}
                        >
                          Preview
                        </button>
                        <button
                          type="button"
                          title="Move this artifact id into the Id field."
                          onClick={() => setOpsId(artifact.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Re-run this artifact with the selected backend."
                          onClick={() => void rerunIngestId(artifact.id)}
                          disabled={running}
                        >
                          Rerun
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "artifacts" ? (
            <div className="operation-group">
              <div className="operation-title">Voice</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="Start microphone capture."
                  onClick={() => void startVoiceCapture()}
                  disabled={running || recordingVoice}
                >
                  Record
                </button>
                <button
                  type="button"
                  title="Stop microphone capture."
                  onClick={stopVoiceCapture}
                  disabled={!recordingVoice}
                >
                  Stop
                </button>
                <button
                  type="button"
                  title="Transcribe the latest voice capture."
                  onClick={() => void transcribeVoiceCapture()}
                  disabled={running || recordingVoice || !voiceCaptureArtifact}
                >
                  Transcribe
                </button>
                <button
                  type="button"
                  title="Create speech audio from composer text or the latest assistant answer."
                  onClick={() => void speakVoiceOutput()}
                  disabled={running || recordingVoice || voiceOutputBusy}
                >
                  Speak
                </button>
                <button
                  type="button"
                  title="Stage voice_speak with composer text or the latest assistant answer."
                  onClick={() => stageVoiceSpeak()}
                  disabled={running || recordingVoice}
                >
                  Stage TTS
                </button>
              </div>
              {voicePreviewUrl || voiceCaptureArtifact ? (
                <div className="voice-capture">
                  {voicePreviewUrl ? (
                    <audio src={voicePreviewUrl} controls />
                  ) : null}
                  {voiceCaptureArtifact ? (
                    <span title={voiceCaptureArtifact.path}>
                      {voiceCaptureArtifact.id} / {formatBytes(voiceCaptureArtifact.bytes)}
                    </span>
                  ) : null}
                </div>
              ) : null}
              {voiceOutputPreviewUrl || voiceOutputArtifact ? (
                <div className="voice-capture">
                  {voiceOutputPreviewUrl ? (
                    <audio src={voiceOutputPreviewUrl} controls />
                  ) : null}
                  {voiceOutputArtifact ? (
                    <span title={voiceOutputArtifact.path}>
                      {voiceOutputArtifact.id} / {formatBytes(voiceOutputArtifact.bytes)}
                    </span>
                  ) : null}
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "artifacts" ? (
            <div className="operation-group">
              <div className="operation-title">Artifacts</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List generated document artifacts."
                  onClick={() => void reviewGeneratedArtifacts()}
                  disabled={running}
                >
                  List Artifacts
                </button>
                <button
                  type="button"
                  title="Show generated artifact Id."
                  onClick={() => void showGeneratedArtifactFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Artifact
                </button>
                <button
                  type="button"
                  title="Open generated artifact Id in the OS default app."
                  onClick={() => void openGeneratedArtifactFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Open Artifact
                </button>
                <button
                  type="button"
                  title="Delete generated artifact Id from the local artifact cache."
                  onClick={() => void deleteGeneratedArtifactFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Artifact
                </button>
              </div>
              {generatedArtifacts.length ? (
                <div className="ingestion-review">
                  {generatedArtifacts.map((artifact) => (
                    <div className="ingestion-card" key={artifact.id}>
                      <div className="ingestion-card-head">
                        <strong>{artifact.id}</strong>
                        <span>{artifact.format}</span>
                      </div>
                      <span title={artifact.path}>
                        {fileName(artifact.path)} / {formatBytes(artifact.bytes)}
                      </span>
                      {artifact.modified_ms ? (
                        <span>
                          modified {new Date(artifact.modified_ms).toLocaleString()}
                        </span>
                      ) : null}
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this artifact id into the Id field."
                          onClick={() => setOpsId(artifact.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Show this generated artifact."
                          onClick={() => void showGeneratedArtifact(artifact.id)}
                          disabled={running}
                        >
                          Show
                        </button>
                        <button
                          type="button"
                          title="Open this generated artifact in the OS default app."
                          onClick={() => void openGeneratedArtifact(artifact.id)}
                          disabled={running}
                        >
                          Open
                        </button>
                        {isInlineArtifactFormat(artifact.format) ? (
                          <button
                            type="button"
                            title="Preview this generated artifact inline."
                            onClick={() => void previewGeneratedArtifact(artifact)}
                            disabled={running}
                          >
                            {isAudioFormat(artifact.format) ? "Play" : "Preview"}
                          </button>
                        ) : null}
                        <button
                          type="button"
                          title="Delete this generated artifact from the local artifact cache."
                          onClick={() => void deleteGeneratedArtifact(artifact.id)}
                          disabled={running}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}
              {artifactPreview ? (
                <div className="artifact-preview">
                  <div className="artifact-preview-head">
                    <div>
                      <strong>{artifactPreview.artifact.id}</strong>
                      <span>
                        {artifactPreview.artifact.format} /{" "}
                        {formatBytes(artifactPreview.artifact.bytes)}
                      </span>
                    </div>
                    <div className="mini-actions">
                      <button
                        type="button"
                        title="Open this generated artifact in the OS default app."
                        onClick={() => void openGeneratedArtifact(artifactPreview.artifact.id)}
                        disabled={running}
                      >
                        Open
                      </button>
                      <button
                        type="button"
                        title="Close the inline artifact preview."
                        onClick={() => setArtifactPreview(null)}
                      >
                        Close
                      </button>
                    </div>
                  </div>
                  {isAudioFormat(artifactPreview.artifact.format) ? (
                    <audio src={artifactPreview.data_url} controls />
                  ) : isTextArtifactFormat(artifactPreview.artifact.format) ? (
                    <pre className="artifact-preview-text">
                      {textFromDataUrl(artifactPreview.data_url)}
                    </pre>
                  ) : (
                    <iframe
                      className="artifact-preview-frame"
                      title={`Artifact preview ${artifactPreview.artifact.id}`}
                      src={artifactPreview.data_url}
                      sandbox="allow-same-origin"
                    />
                  )}
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "chat" ? (
            <div className="operation-group">
              <div className="operation-title">Compactions</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List saved compacted-context artifacts."
                  onClick={() => void listCompactionsFromOps()}
                  disabled={running}
                >
                  List Compact
                </button>
                <button
                  type="button"
                  title="Show compacted-context artifact Id and load its content into Value."
                  onClick={() => void showCompactionFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Compact
                </button>
                <button
                  type="button"
                  title="Use compacted-context artifact Id as the next manual compacted context."
                  onClick={() => void useCompactionFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Use Compact
                </button>
                <button
                  type="button"
                  title="Export compacted-context artifact Id to Value, or to /tmp when Value is blank."
                  onClick={() => void exportCompactionFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Export Compact
                </button>
                <button
                  type="button"
                  title="Import a compacted-context JSON artifact from the path in Value."
                  onClick={() => void importCompactionFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Compact
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete compacted-context artifact Id."
                  onClick={() => void deleteCompactionFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Compact
                </button>
              </div>
              {compactionTransferStatus ? (
                <div className="bundle-card">
                  <div className="bundle-card-head">
                    <strong>
                      Compaction {compactionTransferStatus.operation}
                    </strong>
                    <span>{compactionTransferStatus.record.id}</span>
                  </div>
                  <span title={compactionTransferStatus.path}>
                    {compactionTransferStatus.path}
                  </span>
                  <span>{compactionTransferStatus.record.source}</span>
                  <span>{compactionTransferStatus.record.created_at}</span>
                </div>
              ) : null}
              {compactionRecords.length ? (
                <div className="memory-review">
                  {compactionRecords.map((record) => (
                    <div className="memory-card" key={record.id}>
                      <div className="memory-card-head">
                        <strong>{record.id}</strong>
                        <span>{record.max_output_tokens} tokens</span>
                      </div>
                      <div className="memory-meta">
                        <span>{record.source}</span>
                        {record.conversation_id ? (
                          <span>conversation {record.conversation_id}</span>
                        ) : null}
                        <span>{record.created_at}</span>
                      </div>
                      <p>{previewText(record.content, 260)}</p>
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this compaction id into the Id field."
                          onClick={() => setOpsId(record.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Load this compacted context into Value."
                          onClick={() => {
                            setOpsId(record.id);
                            setOpsValue(record.content);
                          }}
                          disabled={running}
                        >
                          Edit
                        </button>
                        <button
                          type="button"
                          title="Use this artifact as the next manual compacted context."
                          onClick={() => {
                            setOpsId(record.id);
                            setOpsValue(record.content);
                            setManualCompactedContext(record.content);
                          }}
                          disabled={running}
                        >
                          Use
                        </button>
                        <button
                          type="button"
                          title="Set Value to a default export path for this artifact."
                          onClick={() => {
                            setOpsId(record.id);
                            setOpsValue(defaultCompactionPath(record.id));
                          }}
                          disabled={running}
                        >
                          Path
                        </button>
                        <button
                          type="button"
                          className="danger"
                          title="Delete this compacted context artifact."
                          onClick={() => void deleteCompactionFromOps(record.id)}
                          disabled={running}
                        >
                          Delete
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="empty-note">
                  No compacted-context artifacts loaded. List saved artifacts or keep one from preview.
                </div>
              )}
            </div>
            ) : null}

            {activeSection === "chat" ? (
            <div className="operation-group">
              <div className="operation-title">Agents</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List saved agent configurations."
                  onClick={() => void reviewAgents()}
                  disabled={running}
                >
                  List Agents
                </button>
                <button
                  type="button"
                  title="Show saved agent Id."
                  onClick={() => void showAgentFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Agent
                </button>
                <button
                  type="button"
                  title="Use agent Id for future runs."
                  onClick={() => setAgentId(opsId.trim())}
                  disabled={running || !opsId.trim()}
                >
                  Use Agent
                </button>
                <button
                  type="button"
                  title="Save current setup as agent Id using Value as the system prompt."
                  onClick={() => void saveAgentFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Save Agent
                </button>
                <button
                  type="button"
                  title="Export saved agent Id to Value path."
                  onClick={() => void exportAgentFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Export Agent
                </button>
                <button
                  type="button"
                  title="Import saved agent config from Value path."
                  onClick={() => void importAgentFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Agent
                </button>
                <button
                  type="button"
                  className="danger"
                  title={
                    knownProfileGrantedAgent(opsId.trim())
                      ? "This agent is shared from another profile; revoke the profile grant to remove access."
                      : "Delete saved agent Id."
                  }
                  onClick={() => void deleteAgentFromOps()}
                  disabled={
                    running ||
                    !opsId.trim() ||
                    knownProfileGrantedAgent(opsId.trim())
                  }
                >
                  Delete Agent
                </button>
              </div>
              {agentConfigs.length ? (
                <div className="ingestion-review">
                  {agentConfigs.map((doc) => {
                    const sharedFrom = agentSharedProfile(doc);
                    return (
                      <div className="ingestion-card" key={doc.id}>
                        <div className="ingestion-card-head">
                          <strong>{doc.name || doc.id}</strong>
                          <span>{doc.id}</span>
                        </div>
                        {sharedFrom ? (
                          <span>
                            shared from {sharedFrom}
                            {doc.grant_id ? ` / ${doc.grant_id}` : ""}
                          </span>
                        ) : doc.profile ? (
                          <span>profile {doc.profile}</span>
                        ) : null}
                        {"system_prompt" in doc ? (
                          <>
                            <span>
                              {doc.model ? `model ${doc.model}` : "default model"}
                            </span>
                            <span>
                              {doc.max_tool_calls == null
                                ? "default tool budget"
                                : `${doc.max_tool_calls} tool calls`}
                            </span>
                            <p>{previewText(doc.system_prompt, 220)}</p>
                          </>
                        ) : (
                          <span title={doc.path}>{fileName(doc.path)}</span>
                        )}
                        <div className="mini-actions">
                          <button
                            type="button"
                            title="Move this agent id into the Id field."
                            onClick={() => setOpsId(doc.id)}
                            disabled={running}
                          >
                            Set Id
                          </button>
                          <button
                            type="button"
                            title="Use this saved agent for future runs."
                            onClick={() => setAgentId(doc.id)}
                            disabled={running}
                          >
                            Use
                          </button>
                          <button
                            type="button"
                            title="Show this saved agent config."
                            onClick={() => void showAgent(doc.id)}
                            disabled={running}
                          >
                            Show
                          </button>
                          <button
                            type="button"
                            title="Export this saved agent to the Value path."
                            onClick={() => void exportAgentFromOps(doc.id)}
                            disabled={running || !opsValue.trim()}
                          >
                            Export
                          </button>
                          <button
                            type="button"
                            className="danger"
                            title={
                              sharedFrom
                                ? "Revoke this profile grant to remove shared access."
                                : "Delete this saved agent config."
                            }
                            onClick={() => void deleteAgentFromOps(doc.id)}
                            disabled={running || sharedFrom != null}
                          >
                            Delete
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "chat" ? (
            <div className="operation-group">
              <div className="operation-title">Tools</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="Call tool Id directly with Value as JSON input."
                  onClick={() => void callToolFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Call Tool
                </button>
              </div>
            </div>
            ) : null}

            {activeSection === "adapters" ? (
            <div className="operation-group">
              <div className="operation-title">Adapters</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List imported adapter packages."
                  onClick={() => void reviewAdapters()}
                  disabled={running}
                >
                  List Adapters
                </button>
                <button
                  type="button"
                  title="Summarize adapter review and runtime operability status."
                  onClick={() => void adapterDoctorFromOps()}
                  disabled={running}
                >
                  Doctor
                </button>
                <button
                  type="button"
                  title="Import adapter manifest or package path from Value."
                  onClick={() => void importAdapterFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Adapter
                </button>
                <button
                  type="button"
                  title="Import portable adapter manifest JSON from Value."
                  onClick={() => void importAdapterManifestFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Manifest
                </button>
                <button
                  type="button"
                  title="Show adapter package Id."
                  onClick={() => void showAdapterFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Adapter
                </button>
                <button
                  type="button"
                  title="Export adapter package Id to the path in Value."
                  onClick={() => void exportAdapterFromOps()}
                  disabled={running || !opsId.trim() || !opsValue.trim()}
                >
                  Export Adapter
                </button>
                <button
                  type="button"
                  title="Install OpenClaw adapter package Id as a quarantined skill."
                  onClick={() => void installAdapterSkillFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Install Skill
                </button>
                <button
                  type="button"
                  title="Allow adapter package Id."
                  onClick={() => void setAdapterQuarantine(true)}
                  disabled={running || !opsId.trim()}
                >
                  Allow Adapter
                </button>
                <button
                  type="button"
                  title="Block adapter package Id."
                  onClick={() => void setAdapterQuarantine(false)}
                  disabled={running || !opsId.trim()}
                >
                  Block Adapter
                </button>
              </div>
              {adapterDoctorReport ? (
                <div className="bundle-card">
                  <div className="bundle-card-head">
                    <strong>Adapter doctor {adapterDoctorReport.status}</strong>
                    <span>
                      {adapterDoctorReport.ready_capability_count}/
                      {adapterDoctorReport.executable_capability_count} executable ready
                    </span>
                  </div>
                  <span>
                    {adapterDoctorReport.package_count} packages /{" "}
                    {adapterDoctorReport.quarantined_package_count} quarantined /{" "}
                    {adapterDoctorReport.unsupported_capability_count} unsupported
                  </span>
                  <span>
                    {adapterDoctorReport.secret_requirement_count} secrets /{" "}
                    {adapterDoctorReport.high_risk_finding_count} high-risk findings
                  </span>
                  {adapterDoctorReport.errors?.length ? (
                    <div className="finding-list">
                      {adapterDoctorReport.errors.slice(0, 4).map((error) => (
                        <span className="finding high" key={error}>
                          {error}
                        </span>
                      ))}
                    </div>
                  ) : null}
                  {adapterDoctorReport.warnings?.length ? (
                    <div className="finding-list">
                      {adapterDoctorReport.warnings.slice(0, 4).map((warning) => (
                        <span className="finding warning" key={warning}>
                          {warning}
                        </span>
                      ))}
                    </div>
                  ) : null}
                  {adapterDoctorReport.packages.length ? (
                    <div className="finding-list">
                      {adapterDoctorReport.packages.map((pkg) => (
                        <span
                          className={`finding ${
                            pkg.status === "error"
                              ? "high"
                              : pkg.status === "warning"
                                ? "warning"
                                : "none"
                          }`}
                          key={`adapter-doctor:${pkg.id}`}
                        >
                          {pkg.id}: {pkg.ready_capability_count} ready /{" "}
                          {pkg.unsupported_capability_count} unsupported
                        </span>
                      ))}
                    </div>
                  ) : null}
                </div>
              ) : null}
              {adapterPackages.length ? (
                <div className="ingestion-review">
                  {adapterPackages.map((adapterPackage) => {
                    const permissions = enabledPermissions(adapterPackage);
                    const secretRequirements = adapterPackage.secret_requirements ?? [];
                    const highRisk = hasHighRiskAdapterFindings(adapterPackage);
                    return (
                      <div
                        className={`ingestion-card ${
                          highRisk ? "high-risk" : ""
                        }`}
                        key={adapterPackage.id}
                      >
                        <div className="ingestion-card-head">
                          <strong>{adapterPackage.id}</strong>
                          <span>
                            {adapterPackage.quarantined ? "quarantined" : "allowed"}
                          </span>
                        </div>
                        <span title={adapterPackage.source}>
                          {adapterPackage.adapter} / {fileName(adapterPackage.source)}
                        </span>
                        <span title={adapterPackage.digest}>
                          digest {adapterPackage.digest.slice(0, 16)}
                        </span>
                        {highRisk ? (
                          <span className="finding high">
                            activation blocked until this package is re-inspected or removed
                          </span>
                        ) : null}
                        <div className="finding-list">
                          {permissions.length ? (
                            permissions.map((permission) => (
                              <span className="finding warning" key={permission}>
                                permission: {permission}
                              </span>
                            ))
                          ) : (
                            <span className="finding none">no requested permissions</span>
                          )}
                        </div>
                        {secretRequirements.length ? (
                          <div className="finding-list">
                            {secretRequirements.map((secret) => (
                              <span
                                className="finding warning"
                                key={`${adapterPackage.id}:secret:${secret.source}:${secret.name}`}
                                title={secret.description ?? secret.source}
                              >
                                secret: {secret.name} / {secret.source}
                                {secret.required === false ? " / optional" : ""}
                              </span>
                            ))}
                          </div>
                        ) : null}
                        {adapterPackage.findings.length ? (
                          <div className="finding-list">
                            {adapterPackage.findings.map((finding) => (
                              <span
                                className={`finding ${finding.severity}`}
                                key={`${adapterPackage.id}:${finding.severity}:${finding.message}`}
                              >
                                {finding.severity}: {finding.message}
                              </span>
                            ))}
                          </div>
                        ) : (
                          <span className="finding none">no scan findings</span>
                        )}
                        {adapterPackage.capabilities.length ? (
                          <div className="finding-list">
                            {adapterPackage.capabilities.map((capability) => {
                              const runtime = adapterRuntimeSummary(capability.runtime);
                              return (
                                <span className="finding" key={capability.id}>
                                  {capability.kind}: {capability.name}{" "}
                                  {capability.quarantined ? "(quarantined)" : "(allowed)"}
                                  {runtime ? ` / ${runtime}` : ""}
                                </span>
                              );
                            })}
                          </div>
                        ) : (
                          <span className="finding none">no capabilities</span>
                        )}
                        <div className="mini-actions">
                          <button
                            type="button"
                            title="Move this adapter package id into the Id field."
                            onClick={() => setOpsId(adapterPackage.id)}
                            disabled={running}
                          >
                            Set Id
                          </button>
                          <button
                            type="button"
                            title={
                              highRisk
                                ? "High-risk static scan findings block activation."
                                : "Allow this adapter package if digest and static scan checks pass."
                            }
                            onClick={() => {
                              setOpsId(adapterPackage.id);
                              void setAdapterQuarantine(true, adapterPackage.id);
                            }}
                            disabled={running || highRisk}
                          >
                            Allow
                          </button>
                          <button
                            type="button"
                            title="Install this OpenClaw adapter as a quarantined skill."
                            onClick={() => {
                              setOpsId(adapterPackage.id);
                              void installAdapterSkillFromOps(adapterPackage.id);
                            }}
                            disabled={
                              running ||
                              adapterPackage.adapter !== "open_claw_agent_skills"
                            }
                          >
                            Install Skill
                          </button>
                          <button
                            type="button"
                            title="Keep this adapter package quarantined."
                            onClick={() => {
                              setOpsId(adapterPackage.id);
                              void setAdapterQuarantine(false, adapterPackage.id);
                            }}
                            disabled={running}
                          >
                            Block
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "adapters" ? (
            <div className="operation-group">
              <div className="operation-title">Bridge Deliveries</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="List failed outbound bridge delivery dead letters."
                  onClick={() => void listBridgeDeliveriesFromOps()}
                  disabled={running || transport !== "daemon"}
                >
                  List Deliveries
                </button>
                <button
                  type="button"
                  title="Retry bridge delivery Id."
                  onClick={() => void retryBridgeDeliveryFromOps()}
                  disabled={running || transport !== "daemon" || !opsId.trim()}
                >
                  Retry Id
                </button>
                <button
                  type="button"
                  title="Retry all failed bridge deliveries up to the daemon batch limit."
                  onClick={() => void retryAllBridgeDeliveriesFromOps()}
                  disabled={running || transport !== "daemon"}
                >
                  Retry All
                </button>
              </div>
              {bridgeDeliveries.length ? (
                <div className="storage-buckets">
                  {bridgeDeliveries.map((delivery) => (
                    <div
                      className="storage-bucket"
                      key={delivery.id}
                      title={delivery.url}
                    >
                      <strong>{delivery.target}</strong>
                      <span>{delivery.id}</span>
                      <span>{delivery.url}</span>
                      <span>
                        updated {formatUnixMs(delivery.updated_ms)} /{" "}
                        {deliveryStatusLabel(delivery.last_delivery)}
                      </span>
                      <div className="mini-actions">
                        <button
                          type="button"
                          title="Move this delivery id into the Id field."
                          onClick={() => setOpsId(delivery.id)}
                          disabled={running}
                        >
                          Set Id
                        </button>
                        <button
                          type="button"
                          title="Retry this failed bridge delivery."
                          onClick={() => {
                            setOpsId(delivery.id);
                            void retryBridgeDeliveryFromOps(delivery.id);
                          }}
                          disabled={running || transport !== "daemon"}
                        >
                          Retry
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="empty-note">
                  No bridge delivery dead letters loaded.
                </div>
              )}
              {bridgeDeliveryResult ? (
                <div className="bundle-card">
                  <div className="bundle-card-head">
                    <strong>Last bridge retry</strong>
                    <span>{bridgeRetrySummary(bridgeDeliveryResult)}</span>
                  </div>
                  <span>{previewText(previewJson(bridgeDeliveryResult), 240)}</span>
                </div>
              ) : null}
            </div>
            ) : null}

            {activeSection === "adapters" ? (
            <div className="operation-group">
              <div className="operation-title">Bundles</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="Export a backup bundle to /tmp using a timestamped path."
                  onClick={() => void backupBundleNow()}
                  disabled={running}
                >
                  Backup Now
                </button>
                <button
                  type="button"
                  title="Export bundle to the path in Value."
                  onClick={() => void exportBundleFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Export
                </button>
                <button
                  type="button"
                  title="Import bundle from the path in Value."
                  onClick={() => void importBundleFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import
                </button>
              </div>
              {bundleStatus ? (
                <div className="bundle-card">
                  <div className="bundle-card-head">
                    <strong>
                      Bundle {bundleStatus.operation}
                    </strong>
                    <span>schema {bundleStatus.manifest.schema_version}</span>
                  </div>
                  <span title={bundleStatus.path}>{bundleStatus.path}</span>
                  <span>profile {bundleStatus.manifest.profile}</span>
                  <span>{bundleStatus.manifest.exported_at}</span>
                </div>
              ) : (
                <div className="empty-note">
                  No bundle activity yet. Export a backup or import a bundle.
                </div>
              )}
            </div>
            ) : null}

            {activeSection === "adapters" ? (
            <div className="operation-group">
              <div className="operation-title">Storage</div>
              <div className="button-grid">
                <button
                  type="button"
                  title="Show local harness storage footprint by bucket."
                  onClick={() => void storageReportFromOps()}
                  disabled={running}
                >
                  Report
                </button>
                <button
                  type="button"
                  title="Plan cache-file pruning using Value as retention days."
                  onClick={() => void storagePruneCacheFromOps(false)}
                  disabled={running || !opsValue.trim()}
                >
                  Prune Plan
                </button>
                <button
                  type="button"
                  className="danger"
                  title="Delete cache files older than the Value retention days."
                  onClick={() => void storagePruneCacheFromOps(true)}
                  disabled={running || !opsValue.trim()}
                >
                  Prune Apply
                </button>
              </div>
              {storageReport ? (
                <div className="storage-report">
                  <div className="storage-total">
                    <strong>{formatBytes(storageReport.total_bytes)}</strong>
                    <span>
                      {storageReport.total_files} files /{" "}
                      {storageReport.total_directories} dirs
                    </span>
                  </div>
                  {storageReport.quota_bytes !== undefined &&
                  storageReport.quota_bytes !== null ? (
                    <div
                      className={
                        storageReport.quota_exceeded
                          ? "storage-largest warning"
                          : "storage-largest"
                      }
                    >
                      <span>quota</span>
                      <strong>{formatBytes(storageReport.quota_bytes)}</strong>
                      <span>
                        {storageReport.quota_exceeded ? "over" : "left"}{" "}
                        {formatBytes(
                          Math.abs(storageReport.quota_remaining_bytes ?? 0),
                        )}
                      </span>
                    </div>
                  ) : null}
                  {storageReport.largest_file ? (
                    <div className="storage-largest">
                      <span>largest</span>
                      <strong title={storageReport.largest_file}>
                        {fileName(storageReport.largest_file)}
                      </strong>
                      <span>{formatBytes(storageReport.largest_file_bytes)}</span>
                    </div>
                  ) : null}
                  <div className="storage-buckets">
                    {storageReport.buckets.map((bucket) => (
                      <div
                        className={`storage-bucket ${bucket.exists ? "" : "missing"}`}
                        key={bucket.name}
                        title={bucket.path}
                      >
                        <strong>{bucket.name}</strong>
                        <span>{formatBytes(bucket.bytes)}</span>
                        <span>
                          {bucket.files} files / {bucket.directories} dirs
                        </span>
                        {bucket.largest_file ? (
                          <span title={bucket.largest_file}>
                            max {fileName(bucket.largest_file)}{" "}
                            {formatBytes(bucket.largest_file_bytes)}
                          </span>
                        ) : (
                          <span>{bucket.exists ? "empty" : "missing"}</span>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}
              {storagePruneResult ? (
                <div className="storage-report">
                  <div className="storage-total">
                    <strong>
                      {storagePruneResult.dry_run ? "plan" : "applied"}
                    </strong>
                    <span>
                      {storagePruneResult.plan.total_files} files /{" "}
                      {formatBytes(storagePruneResult.plan.total_bytes)}
                    </span>
                  </div>
                  {!storagePruneResult.dry_run ? (
                    <div className="storage-largest">
                      <span>deleted</span>
                      <strong>{storagePruneResult.deleted_files} files</strong>
                      <span>{formatBytes(storagePruneResult.deleted_bytes)}</span>
                    </div>
                  ) : null}
                  {(storagePruneResult.errors ?? []).length ? (
                    <div className="storage-largest warning">
                      <span>errors</span>
                      <strong>{storagePruneResult.errors?.length ?? 0}</strong>
                      <span>
                        {previewText(
                          (storagePruneResult.errors ?? []).join("; "),
                          160,
                        )}
                      </span>
                    </div>
                  ) : null}
                  <div className="storage-buckets">
                    {storagePruneResult.plan.candidates.slice(0, 8).map((candidate) => (
                      <div
                        className="storage-bucket"
                        key={candidate.path}
                        title={candidate.path}
                      >
                        <strong>{candidate.bucket}</strong>
                        <span>{formatBytes(candidate.bytes)}</span>
                        <span>{fileName(candidate.path)}</span>
                      </div>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>
            ) : null}
          </div>
        </section>
        ) : null}

        {activeSection === "chat" || activeSection === "approvals" ? (
        <section className="panel">
          <div className="panel-title">Control</div>
          {activeSection === "approvals" ? (
            <>
              <label>
                Unlock
                <input
                  type="password"
                  value={approvalUnlock}
                  onChange={(e) => setApprovalUnlock(e.target.value)}
                  placeholder="optional"
                  disabled={running}
                />
              </label>
              <label>
                Signature
                <input
                  value={approvalSignature}
                  onChange={(e) => setApprovalSignature(e.target.value)}
                  placeholder="optional"
                  disabled={running}
                />
              </label>
              <label>
                Controller
                <input
                  value={approvalControllerAgent}
                  onChange={(e) => setApprovalControllerAgent(e.target.value)}
                  placeholder="optional delegated agent"
                  disabled={running}
                />
              </label>
            </>
          ) : null}
          {activeSection === "approvals" ? (
            approvals.length ? (
              <div className="approval-list">
                {approvals.map((approval) => (
                  <div className="approval-card" key={approval.approval_id}>
                    <div className="approval-card-head">
                      <strong>{approval.action ?? approval.approval_id}</strong>
                      <span className={`approval-status ${approval.status}`}>
                        {approval.status}
                      </span>
                    </div>
                    <span className="approval-id">{approval.approval_id}</span>
                    {approval.reason ? (
                      <p>{previewText(approval.reason, 180)}</p>
                    ) : null}
                    {approval.controller_agent ? (
                      <p>
                        controller {approval.controller_agent}
                        {approval.controller_scope?.length
                          ? ` (${approval.controller_scope.join(", ")})`
                          : ""}
                      </p>
                    ) : null}
                    {approval.delegated_controller ? (
                      <p>delegated by {approval.delegated_controller}</p>
                    ) : null}
                    {approval.assessment ? (
                      <p>
                        assessment{" "}
                        {approval.assessment.recommendation ??
                          approval.assessment.status}{" "}
                        by {approval.assessment.controller_agent}
                        {approval.assessment.model
                          ? ` via ${approval.assessment.model}`
                          : ""}
                      </p>
                    ) : null}
                    <div className="mini-actions">
                      <button
                        type="button"
                        title="Ask the delegated controller agent to assess this approval."
                        onClick={() => void assessApproval(approval.approval_id)}
                        disabled={running || approval.status !== "pending"}
                      >
                        Assess
                      </button>
                      <button
                        type="button"
                        title="Approve and execute this pending action."
                        onClick={() => void decideApproval(approval.approval_id, true)}
                        disabled={running || approval.status !== "pending"}
                      >
                        Approve
                      </button>
                      <button
                        type="button"
                        className="danger"
                        title="Reject this pending action."
                        onClick={() => void decideApproval(approval.approval_id, false)}
                        disabled={running || approval.status !== "pending"}
                      >
                        Reject
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="empty-note">
                No approvals loaded. Use Review after a run requests approval.
              </div>
            )
          ) : null}
          <fieldset className="operation-group">
            <legend>Stop mode</legend>
            <div className="segmented-control" role="group" aria-label="Stop mode">
              <button
                type="button"
                className={stopRetentionMode === null ? "selected" : ""}
                title="Use the resolved agent, profile, or global stopped-run retention policy."
                onClick={() => setStopRetentionMode(null)}
              >
                Default
              </button>
              <button
                type="button"
                className={stopRetentionMode === "discard" ? "selected" : ""}
                title="Stop without retaining context from the cancelled task."
                onClick={() => setStopRetentionMode("discard")}
              >
                Discard
              </button>
              <button
                type="button"
                className={stopRetentionMode === "summarise" ? "selected" : ""}
                title="Stop and retain a concise summary of what happened."
                onClick={() => setStopRetentionMode("summarise")}
              >
                Summarise
              </button>
            </div>
            <div className="mode-note">
              {stopRetentionMode === null
                ? "Stopped tasks follow the resolved policy."
                : stopRetentionMode === "discard"
                  ? "Stopped tasks keep no attempted context."
                  : "Stopped tasks retain only a summary artifact."}
            </div>
          </fieldset>
          <div className="button-grid">
            <button
              type="button"
              title="List pending and resolved approvals for the current run."
              onClick={() => void reviewApprovals()}
              disabled={running || !lastRunId}
            >
              Review
            </button>
            <button
              type="button"
              title="Approve and execute the first pending tool approval."
              onClick={() => void approveFirstPending()}
              disabled={running || !lastRunId}
            >
              Approve
            </button>
            <button
              type="button"
              className="danger"
              title="Reject the first pending tool approval without executing it."
              onClick={() => void rejectFirstPending()}
              disabled={running || !lastRunId}
            >
              Reject
            </button>
            <button
              type="button"
              title="Score Id target using Value as 0-10; empty Id targets last_answer and empty Value records 10."
              onClick={() => void scoreLastRun()}
              disabled={running || !lastRunId}
            >
              Score
            </button>
            <button
              type="button"
              title="Mark the last answer as excellent."
              onClick={() => void scoreLastRun(10, "last_answer")}
              disabled={running || !lastRunId}
            >
              Great 10
            </button>
            <button
              type="button"
              title="Mark the last answer as acceptable."
              onClick={() => void scoreLastRun(7, "last_answer")}
              disabled={running || !lastRunId}
            >
              Good 7
            </button>
            <button
              type="button"
              title="Mark the last answer as poor."
              onClick={() => void scoreLastRun(3, "last_answer")}
              disabled={running || !lastRunId}
            >
              Poor 3
            </button>
            <button
              type="button"
              title={`Stop current run and ${stopRetentionLabel(stopRetentionMode)}.`}
              onClick={() => void cancelLastRun()}
              disabled={!running || !lastRunId}
            >
              Stop
            </button>
            <button
              type="button"
              title="Resume the run id in the Id field, or the last run when Id is blank."
              onClick={() => void resumeLastRun()}
              disabled={running || (!opsId.trim() && !lastRunId)}
            >
              Resume
            </button>
            <button
              type="button"
              onClick={() => void loadLastTrace()}
              disabled={running || !lastRunId}
            >
              Trace
            </button>
          </div>
        </section>
        ) : null}
      </aside>
    </div>
  );
}

function prefixFor(kind: LineKind): string {
  switch (kind) {
    case "user":
      return "You";
    case "assistant":
      return "AI";
    case "event":
      return "log";
    case "error":
      return "err";
  }
}

function renderLineContent(line: TranscriptLine) {
  if (line.kind !== "assistant") {
    return line.text;
  }
  const parsed = parseStructuredJson(line.text);
  if (parsed === null || typeof parsed !== "object") {
    return line.text;
  }
  const rows = structuredRows(parsed);
  return (
    <div className="structured-result">
      <div className="structured-head">
        <strong>{structuredTitle(parsed)}</strong>
        <span>{structuredMeta(parsed)}</span>
      </div>
      {rows.length ? (
        <dl className="structured-fields">
          {rows.map(([key, value]) => (
            <div key={key}>
              <dt>{humanLabel(key)}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      <details>
        <summary>Raw JSON</summary>
        <pre>{JSON.stringify(parsed, null, 2)}</pre>
      </details>
    </div>
  );
}

function parseStructuredJson(text: string): JsonValue | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) {
    return null;
  }
  try {
    return JSON.parse(trimmed) as JsonValue;
  } catch {
    return null;
  }
}

function structuredTitle(value: JsonValue) {
  if (Array.isArray(value)) {
    return "Result list";
  }
  if (isJsonRecord(value)) {
    const output = nestedOutputRecord(value);
    if (output) {
      const outputName = scalarText(output.name) ?? scalarText(output.title);
      const outputId = scalarText(output.id);
      if (outputName) return outputName;
      if (outputId) return outputId;
      return "Tool result";
    }
    const name = scalarText(value.name) ?? scalarText(value.title);
    const id = scalarText(value.id);
    if (name && id) return name;
    if (name) return name;
    if (id) return id;
  }
  return "Structured result";
}

function structuredMeta(value: JsonValue) {
  if (Array.isArray(value)) {
    return `${value.length} ${value.length === 1 ? "item" : "items"}`;
  }
  if (isJsonRecord(value)) {
    const output = nestedOutputRecord(value);
    if (output) {
      const status = scalarText(output.status);
      const parts = [status, "tool output"].filter(Boolean);
      return parts.join(" / ");
    }
    const status = scalarText(value.status);
    const quarantined =
      typeof value.quarantined === "boolean"
        ? value.quarantined
          ? "quarantined"
          : "allowed"
        : null;
    const parts = [status, quarantined].filter(Boolean);
    return parts.length ? parts.join(" / ") : `${Object.keys(value).length} fields`;
  }
  return "";
}

function structuredRows(value: JsonValue): Array<[string, string]> {
  if (Array.isArray(value)) {
    return value.slice(0, 4).map((item, index) => [
      `item ${index + 1}`,
      previewJsonValue(item),
    ]);
  }
  if (!isJsonRecord(value)) {
    return [];
  }
  const output = nestedOutputRecord(value);
  if (output) {
    const outputRows = structuredRows(output);
    const envelopeRows: Array<[string, string]> = [];
    if ("duration_ms" in value) {
      envelopeRows.push(["duration_ms", previewJsonValue(value.duration_ms)]);
    }
    if ("run_id" in value) {
      envelopeRows.push(["run_id", previewJsonValue(value.run_id)]);
    }
    return [...outputRows, ...envelopeRows].slice(0, 6);
  }
  const preferred = [
    "id",
    "description",
    "source_path",
    "source",
    "backend",
    "digest",
    "sections",
    "estimated_tokens",
    "final_output",
    "text",
  ];
  const keys = [
    ...preferred.filter((key) => key in value),
    ...Object.keys(value).filter((key) => !preferred.includes(key)),
  ];
  return keys
    .filter((key) => !["body", "content", "extracted_text"].includes(key))
    .slice(0, 6)
    .map((key) => [key, previewJsonValue(value[key])]);
}

function previewJsonValue(value: JsonValue) {
  if (value === null) return "none";
  if (typeof value === "string") return compactPreview(value, 140);
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) return `${value.length} items`;
  return `${Object.keys(value).length} fields`;
}

function compactPreview(text: string, max = 150) {
  const compact = text.replace(/\s+/g, " ").trim();
  if (compact.length <= max) {
    return compact;
  }
  return `${compact.slice(0, max - 1)}...`;
}

function scalarText(value: JsonValue | undefined) {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function isJsonRecord(value: JsonValue): value is { [key: string]: JsonValue } {
  return Boolean(value) && !Array.isArray(value) && typeof value === "object";
}

function isUnknownRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && !Array.isArray(value) && typeof value === "object";
}

function isJsonValue(value: unknown): value is JsonValue {
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return true;
  }
  if (Array.isArray(value)) {
    return value.every(isJsonValue);
  }
  if (!isUnknownRecord(value)) {
    return false;
  }
  return Object.values(value).every(isJsonValue);
}

function nestedOutputRecord(value: { [key: string]: JsonValue }) {
  const output = value.output;
  return isJsonRecord(output) ? output : null;
}

function humanLabel(key: string) {
  return key.replace(/_/g, " ");
}

function parseOptionalNonNegativeInt(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  if (!/^\d+$/.test(trimmed)) return null;
  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isFinite(parsed) || parsed < 0) return null;
  return parsed;
}

function parseOptionalPositiveInt(value: string): number | null {
  const parsed = parseOptionalNonNegativeInt(value);
  return parsed && parsed > 0 ? parsed : null;
}

function parseOptionalNonNegativeFloat(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number.parseFloat(trimmed);
  if (!Number.isFinite(parsed) || parsed < 0) return null;
  return parsed;
}
