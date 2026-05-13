import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AdapterPackage,
  BundleManifest,
  ContextSnapshot,
  Demo,
  IngestionArtifact,
  IngestionResult,
  MemoryRecord,
  PromptDoc,
  Provider,
  RunEvent,
  RunOptions,
  RunSummary,
  SkillDoc,
  ToolVisibility,
} from "./types";

type LineKind = "user" | "assistant" | "event" | "error";
type Transport = "in-process" | "daemon";
type ActiveSection =
  | "chat"
  | "trace"
  | "memory"
  | "skills"
  | "prompts"
  | "ingest"
  | "adapters"
  | "approvals";
type AgentMode = "answer" | "action" | "workflow" | "custom";

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
  memory_fragments: number;
  artifact_refs: number;
  tokens_in: number;
  tokens_out: number;
  cost_usd: number | null;
  duration_ms: number | null;
}

interface ApprovalRecord {
  approval_id: string;
  action: string | null;
  reason: string | null;
  status: string;
  approved?: boolean | null;
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
  buckets: StorageBucket[];
}

interface BundleStatus {
  operation: "exported" | "imported";
  path: string;
  manifest: BundleManifest;
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
  const [opsValue, setOpsValue] = useState("");
  const [opsId, setOpsId] = useState("");
  const [memorySourceRange, setMemorySourceRange] = useState("");
  const [opsUserMemory, setOpsUserMemory] = useState(false);
  const [ingestBackend, setIngestBackend] = useState("local-v0");
  const [model, setModel] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [apiKeyEnv, setApiKeyEnv] = useState("OPENAI_API_KEY");
  const [apiKey, setApiKey] = useState("");
  const [inputCostPerMillion, setInputCostPerMillion] = useState("");
  const [outputCostPerMillion, setOutputCostPerMillion] = useState("");
  const [maxToolCalls, setMaxToolCalls] = useState("");
  const [toolVisibility, setToolVisibility] = useState<ToolVisibility | "">("");
  const [enableShell, setEnableShell] = useState(false);
  const [enableSubagent, setEnableSubagent] = useState(false);
  const [loadMemory, setLoadMemory] = useState(false);
  const [loadSkills, setLoadSkills] = useState(false);
  const [includeIngestIds, setIncludeIngestIds] = useState<string[]>([]);
  const [allowUnsafeIngest, setAllowUnsafeIngest] = useState(false);
  const [enablePromptRefinement, setEnablePromptRefinement] = useState(false);
  const [promptRefinementInstructions, setPromptRefinementInstructions] =
    useState("");
  const [promptRefinementModel, setPromptRefinementModel] = useState("");
  const [requireApproval, setRequireApproval] = useState(false);
  const [rawToolOutput, setRawToolOutput] = useState(false);
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
  const [storageReport, setStorageReport] = useState<StorageReport | null>(null);
  const [bundleStatus, setBundleStatus] = useState<BundleStatus | null>(null);
  const [ingestionArtifacts, setIngestionArtifacts] = useState<
    IngestionArtifact[]
  >([]);
  const [memoryRecords, setMemoryRecords] = useState<MemoryRecord[]>([]);
  const [promptDocs, setPromptDocs] = useState<PromptDoc[]>([]);
  const [skillDocs, setSkillDocs] = useState<SkillDoc[]>([]);
  const [adapterPackages, setAdapterPackages] = useState<AdapterPackage[]>([]);
  const [activeSection, setActiveSection] = useState<ActiveSection>("chat");
  const [approvals, setApprovals] = useState<ApprovalRecord[]>([]);

  const transcriptRef = useRef<HTMLElement>(null);
  const terminalEventSeenRef = useRef(false);
  const rootRunIdRef = useRef<string | null>(null);
  const runStartedAtRef = useRef<number | null>(null);
  const remoteSeenEventKeysRef = useRef<Set<string>>(new Set());
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
  const slashCommandItems = slashCommandSuggestions();

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
    const transcriptEl = transcriptRef.current;
    if (transcriptEl) {
      transcriptEl.scrollTo({
        top: transcriptEl.scrollHeight,
        behavior: "smooth",
      });
    }
  }, [transcript]);

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
  }, [lastRunId, running]);

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
        appendEvent(
          `Context built (${k.snapshot.visible_tools.length} tools, ${k.snapshot.loaded_memory.length} memory fragments)`,
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
          `Tool proposed: ${k.tool_id}(${JSON.stringify(k.input)}) [${k.call_id}]`,
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
                }
              : approval,
          ),
        );
        appendEvent(
          `Approval resolved [${k.approval_id}] approved=${String(k.approved)}`,
        );
        return;
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
      case "IngestionCompleted":
        appendEvent(`Ingestion completed: ${k.artifact_id} (${k.sections} sections)`);
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

  function runtimeOptions(): RunOptions {
    return {
      provider,
      model: model.trim() || null,
      api_base_url: apiBaseUrl.trim() || null,
      api_key_env: apiKeyEnv.trim() || "OPENAI_API_KEY",
      api_key: apiKey.trim() || null,
      max_output_tokens: null,
      temperature: null,
      input_cost_per_million: parseOptionalNonNegativeFloat(inputCostPerMillion),
      output_cost_per_million: parseOptionalNonNegativeFloat(outputCostPerMillion),
      max_tool_calls: parseOptionalNonNegativeInt(maxToolCalls),
      tool_visibility: toolVisibility || null,
      enable_shell: enableShell,
      enable_subagent: enableSubagent,
      load_memory: loadMemory,
      load_skills: loadSkills,
      include_ingest: includeIngestIds,
      allow_unsafe_ingest: allowUnsafeIngest,
      enable_prompt_refinement: enablePromptRefinement,
      prompt_refinement_instructions:
        promptRefinementInstructions.trim() || null,
      prompt_refinement_model: promptRefinementModel.trim() || null,
      require_approval: requireApproval,
      raw_tool_output: rawToolOutput,
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

  function parseToolShortcut(text: string) {
    const trimmed = text.trim();
    const rest = trimmed.startsWith("/tool!")
      ? trimmed.slice("/tool!".length).trim()
      : trimmed.startsWith("/tool ")
        ? trimmed.slice("/tool ".length).trim()
        : null;
    if (rest === null) return null;
    const match = rest.match(/^(\S+)(?:\s+([\s\S]*))?$/);
    if (!match) {
      appendLine("error", "Tool shortcut needs a tool name.");
      return null;
    }
    return {
      name: match[1],
      inputText: match[2]?.trim() || "{}",
    };
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
      appendLine("error", "Agent shortcut needs an agent id: echo or tool.");
      return null;
    }
    if (!trimmed.startsWith("/agent ")) {
      return null;
    }
    const id = trimmed.slice("/agent ".length).trim();
    if (id === "echo" || id === "tool") {
      return id as Demo;
    }
    appendLine("error", `Unknown agent ${id}. Available agents: echo, tool.`);
    return null;
  }

  function slashCommandSuggestions(): SlashCommandSuggestion[] {
    const trimmed = input.trimStart();
    if (!trimmed.startsWith("/") || trimmed.includes("\n")) {
      return [];
    }
    const query = trimmed.slice(1).toLowerCase();
    const commands: SlashCommandSuggestion[] = [
      { command: "/preview", label: "Preview context" },
      { command: "/agent tool", label: "Switch to Tool agent" },
      { command: "/agent echo", label: "Switch to Echo agent" },
      { command: '/tool!echo {"text":"hello"}', label: "Call echo directly" },
      { command: "/run ", label: "Run saved prompt" },
    ];
    if (lastRunId) {
      commands.push(
        { command: "/score 10", label: "Score last answer" },
        { command: "/guide ", label: "Guide current run" },
      );
    }
    for (const prompt of promptDocs.slice(0, 5)) {
      commands.push({
        command: `/run ${prompt.name}`,
        label: `Run ${prompt.name}`,
      });
    }
    return commands
      .filter((item) => {
        const haystack = `${item.command} ${item.label}`.toLowerCase();
        return haystack.includes(query);
      })
      .slice(0, 6);
  }

  function parseScoreShortcut(text: string) {
    const rest = text.trim().startsWith("/score ")
      ? text.trim().slice("/score ".length).trim()
      : null;
    if (rest === null) return null;
    const match = rest.match(/^(\S+)(?:\s+(.+))?$/);
    if (!match) {
      appendLine("error", "Score shortcut needs a number from 0 to 10.");
      return null;
    }
    const score = Number(match[1]);
    if (!Number.isFinite(score) || score < 0 || score > 10) {
      appendLine("error", "Score shortcut needs a number from 0 to 10.");
      return null;
    }
    return {
      score,
      target: match[2]?.trim() || "last_answer",
    };
  }

  function appendJson(label: string, value: unknown) {
    appendEvent(label);
    appendLine("assistant", JSON.stringify(value, null, 2));
  }

  function summarizeTrace(events: RunEvent[]): TraceSummary | null {
    if (!events.length) return null;
    let contextSnapshots = 0;
    let llmCalls = 0;
    let toolCalls = 0;
    let approvals = 0;
    let guidanceInjections = 0;
    let qualityScores = 0;
    let memoryFragments = 0;
    let artifactRefs = 0;
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
      memory_fragments: memoryFragments,
      artifact_refs: artifactRefs,
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

  function confirmLocalChange(action: string) {
    const confirmed = window.confirm(`${action}? This changes local harness data.`);
    if (!confirmed) {
      appendEvent(`${action} cancelled.`);
    }
    return confirmed;
  }

  async function loadPromptBody(name: string) {
    const prompt =
      transport === "daemon"
        ? await daemonJson<{ body: string }>(`/prompts/${encodeURIComponent(name)}`)
        : await invoke<{ body: string }>("prompt_show", { name });
    return prompt.body;
  }

  async function loadTraceFor(runId: string) {
    const events =
      transport === "daemon"
        ? await daemonJson<RunEvent[]>(`/trace/${runId}`)
        : await invoke<RunEvent[]>("trace_show", { runId });
    const summary = summarizeTrace(events);
    const latestContext = latestContextSnapshot(events);
    setTraceEvents(events);
    setTraceSummary(summary);
    if (latestContext) {
      setContextPreview(latestContext);
      setContextPreviewPrompt(null);
    }
    appendEvent(`Loaded trace ${runId} (${events.length} events)`);
    if (summary) {
      appendEvent(
        `Trace summary: ${summary.context_snapshots} contexts, ${summary.llm_calls} LLM calls, ${summary.tool_calls} tools, tokens ${summary.tokens_in}/${summary.tokens_out}`,
      );
    }
  }

  async function submit() {
    let prompt = input.trim();
    if (!prompt) return;

    if (prompt.startsWith("/guide ")) {
      const guidance = prompt.slice("/guide ".length).trim();
      if (!lastRunId) {
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

    if (running) return;

    const isAgentShortcut = prompt === "/agent" || prompt.startsWith("/agent ");
    const nextAgent = parseAgentShortcut(prompt);
    if (nextAgent) {
      setDemo(nextAgent);
      setInput("");
      appendLine("user", `/agent ${nextAgent}`);
      appendEvent(`Switched agent to ${agentDisplayName(nextAgent)}`);
      return;
    }
    if (isAgentShortcut) return;

    const previewPrompt = parsePreviewShortcut(prompt);
    if (previewPrompt !== null) {
      setInput("");
      appendLine("user", previewPrompt ? `/preview ${previewPrompt}` : "/preview");
      await previewCurrentContext(previewPrompt || "preview");
      return;
    }

    const isToolShortcut = prompt.startsWith("/tool!") || prompt.startsWith("/tool ");
    const toolShortcut = parseToolShortcut(prompt);
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
      if (!lastRunId) {
        appendLine("error", "Score shortcut needs a completed or active run.");
        return;
      }
      setInput("");
      await scoreLastRun(scoreShortcut.score, scoreShortcut.target);
      return;
    }
    if (prompt === "/score") {
      appendLine("error", "Score shortcut needs a number from 0 to 10.");
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

    await runAgentPrompt(prompt, savedPromptName ? `/run ${savedPromptName}` : prompt);
  }

  async function runAgentPrompt(prompt: string, displayText: string) {
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
    setApprovals([]);
    terminalEventSeenRef.current = false;
    rootRunIdRef.current = null;
    remoteSeenEventKeysRef.current = new Set();
    runStartedAtRef.current = performance.now();
    appendLine("user", displayText);

    try {
      if (transport === "daemon") {
        const started = await daemonJson<RemoteRunStart>("/run/start", {
          input: prompt,
          demo,
          ...runtimeOptions(),
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
        options: runtimeOptions(),
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
        if (requireApproval) {
          daemonInput.__require_approval = true;
        }
        const output = await daemonJson<unknown>(
          `/tool/${encodeURIComponent(name)}`,
          daemonInput,
        );
        captureDirectToolMetadata(output);
        appendJson("Tool output", output);
        return;
      }
      const output = await invoke<unknown>("call_tool", {
        name,
        input: inputBody,
        options: {
          ...runtimeOptions(),
          enable_shell: enableShell || name === "shell",
        },
      });
      captureDirectToolMetadata(output);
      appendJson("Tool output", output);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      captureRunIdFromError(msg);
      appendLine("error", `Tool call failed: ${msg}`);
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

  async function previewWithIngestFromOps() {
    await previewWithIngest();
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
    try {
      const prompts =
        transport === "daemon"
          ? await daemonJson<PromptDoc[]>("/prompts")
          : await invoke<PromptDoc[]>("prompt_list");
      setPromptDocs(prompts);
      appendEvent(`Saved prompts: ${prompts.length}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt review failed: ${msg}`);
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

  async function loadLastTrace() {
    if (!lastRunId) return;
    try {
      await loadTraceFor(lastRunId);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Trace load failed: ${msg}`);
    }
  }

  async function guideLastRun(text = input.trim(), clearComposer = true) {
    if (!lastRunId || !text.trim()) return;
    const guidance = text.trim();
    if (clearComposer) {
      setInput("");
    }
    try {
      if (transport === "daemon") {
        await daemonJson("/guide", {
          run_id: lastRunId,
          text: guidance,
        });
      } else {
        await invoke("guide", { runId: lastRunId, text: guidance });
      }
      appendEvent(`Guidance recorded for ${lastRunId}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Guide failed: ${msg}`);
    }
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

  async function cancelLastRun() {
    if (!lastRunId) return;
    try {
      if (transport === "daemon") {
        await daemonJson("/cancel", {
          run_id: lastRunId,
          reason: "user requested stop",
        });
      } else {
        await invoke("cancel", {
          runId: lastRunId,
          reason: "user requested stop",
        });
      }
      appendEvent(`Cancellation recorded for ${lastRunId}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Cancel failed: ${msg}`);
    }
  }

  async function reviewApprovals() {
    if (!lastRunId) return;
    try {
      const next = await loadApprovalsForLastRun();
      setApprovals(next);
      appendEvent(`Approvals for ${lastRunId}: ${next.length}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval review failed: ${msg}`);
    }
  }

  async function loadApprovalsForLastRun() {
    if (!lastRunId) return [];
    return transport === "daemon"
      ? await daemonJson<ApprovalRecord[]>(`/approvals/${lastRunId}`)
      : await invoke<ApprovalRecord[]>("approval_list", { runId: lastRunId });
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

  async function decideApproval(approvalId: string, approved: boolean) {
    if (!lastRunId) return;
    if (transport === "daemon") {
      await daemonJson(`/approvals/${lastRunId}/${approvalId}/decide`, {
        approved,
      });
      if (approved) {
        const output = await daemonJson<unknown>(
          `/approvals/${lastRunId}/${approvalId}/execute`,
          {},
        );
        captureDirectToolMetadata(output);
        appendJson("Approved tool output", output);
      }
    } else {
      await invoke("approval_decide", {
        runId: lastRunId,
        approvalId,
        approved,
      });
      if (approved) {
        const output = await invoke<unknown>("approval_execute", {
          runId: lastRunId,
          approvalId,
        });
        captureDirectToolMetadata(output);
        appendJson("Approved tool output", output);
      }
    }
    const next = await loadApprovalsForLastRun();
    setApprovals(next);
    appendEvent(
      approved ? `Approved and executed ${approvalId}` : `Rejected ${approvalId}`,
    );
  }

  async function runBatchFromInput() {
    const items = input
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
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

  async function createMemoryFromOps() {
    const content = requireOpsValue("Memory create");
    if (!content) return;
    try {
      const record =
        transport === "daemon"
          ? await daemonJson<MemoryRecord>("/memory", {
              content,
              user: opsUserMemory,
            })
          : await invoke<MemoryRecord>("memory_create", {
              content,
              user: opsUserMemory,
            });
      setMemoryRecords((records) => upsertMemoryRecord(records, record));
      appendJson("Memory created", record);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory create failed: ${msg}`);
    }
  }

  async function generateMemoryFromOps() {
    const text = requireOpsValue("Memory generate");
    if (!text) return;
    const range = memorySourceRange.trim() || null;
    try {
      const records =
        transport === "daemon"
          ? await daemonJson<MemoryRecord[]>("/memory/generate", {
              text,
              user: opsUserMemory,
              range,
            })
          : await invoke<MemoryRecord[]>("memory_generate", {
              text,
              user: opsUserMemory,
              range,
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

  async function editMemoryFromOps() {
    const id = requireOpsId("Memory edit");
    const content = requireOpsValue("Memory edit");
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

  async function deleteMemoryFromOps() {
    const id = requireOpsId("Memory delete");
    if (!id) return;
    if (!confirmLocalChange(`Delete memory ${id}`)) return;
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

  async function rollbackMemoryFromOps() {
    if (!confirmLocalChange(`Rollback ${opsUserMemory ? "user" : "agent"} memory`)) return;
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
    try {
      const prompt =
        transport === "daemon"
          ? await daemonJson<PromptDoc>("/prompts", { name, body })
          : await invoke<PromptDoc>("prompt_save", { name, body });
      setPromptDocs((docs) => upsertPromptDoc(docs, prompt));
      appendJson("Prompt saved", prompt);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt save failed: ${msg}`);
    }
  }

  async function showPromptFromOps() {
    const name = requireOpsId("Prompt show");
    if (!name) return;
    try {
      const prompt =
        transport === "daemon"
          ? await daemonJson<PromptDoc>(`/prompts/${encodeURIComponent(name)}`)
          : await invoke<PromptDoc>("prompt_show", { name });
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
    if (!confirmLocalChange(`Delete prompt ${name}`)) return;
    try {
      const output =
        transport === "daemon"
          ? await daemonJson<unknown>(
              `/prompts/${encodeURIComponent(name)}/delete`,
              {},
            )
          : await invoke<unknown>("prompt_delete", { name });
      setPromptDocs((docs) => docs.filter((prompt) => prompt.name !== name));
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

  async function showModelFromOps() {
    const id = requireOpsId("Model show");
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

  async function deleteModelFromOps() {
    const id = requireOpsId("Model delete");
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

  async function importSkillFromOps() {
    const path = requireOpsValue("Skill import");
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

  async function showSkillFromOps() {
    const id = requireOpsId("Skill show");
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

  async function ingestPathFromOps() {
    const path = requireOpsValue("Ingest add");
    if (!path) return;
    const backend = ingestBackend.trim() || "local-v0";
    try {
      const artifact = await ingestPath(path, backend);
      setIngestionArtifacts((artifacts) => upsertArtifact(artifacts, artifact));
      appendJson("Ingestion artifact created", artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest failed: ${msg}`);
    }
  }

  async function ingestPath(path: string, backend: string) {
    if (transport === "daemon") {
      return normalizeIngestionResponse(
        await daemonJson<IngestionArtifact | IngestionResult>("/ingest", {
          path,
          backend,
        }),
      );
    }
    return invoke<IngestionArtifact>("ingest_add", { path, backend });
  }

  async function rerunIngestFromOps() {
    const id = requireOpsId("Ingest rerun");
    if (!id) return;
    await rerunIngestId(id);
  }

  async function rerunIngestId(id: string) {
    const backend = ingestBackend.trim() || "local-v0";
    try {
      const artifact =
        transport === "daemon"
          ? await rerunIngestViaDaemon(id, backend)
          : await invoke<IngestionArtifact>("ingest_rerun", { id, backend });
      setIngestionArtifacts((artifacts) => upsertArtifact(artifacts, artifact));
      appendJson(`Ingestion artifact rerun with ${backend}`, artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest rerun failed: ${msg}`);
    }
  }

  async function rerunIngestViaDaemon(id: string, backend: string) {
    return normalizeIngestionResponse(
      await daemonJson<IngestionArtifact | IngestionResult>(
        `/ingest/${id}/rerun`,
        { backend },
      ),
    );
  }

  async function showIngestFromOps() {
    const id = requireOpsId("Ingest show");
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

  async function removeIngestFromOps() {
    const id = requireOpsId("Ingest remove");
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

  function includeIngestFromOps() {
    const id = requireOpsId("Use ingest");
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

  async function importAdapterFromOps() {
    const path = requireOpsValue("Adapter import");
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

  async function showAdapterFromOps() {
    const id = requireOpsId("Adapter show");
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

  async function exportBundleFromOps() {
    const path = requireOpsValue("Bundle export");
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

  async function importBundleFromOps() {
    const path = requireOpsValue("Bundle import");
    if (!path) return;
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
    }
  }

  function previewJson(value: unknown) {
    return JSON.stringify(value, null, 2);
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

  function fileName(path: string) {
    const parts = path.split(/[\\/]/);
    return parts[parts.length - 1] || path;
  }

  function defaultBundlePath() {
    const stamp = new Date().toISOString().replace(/[:.]/g, "-");
    return `/tmp/shinkai-agents-main-${stamp}.tar`;
  }

  function agentDisplayName(agent: Demo) {
    return agent === "tool" ? "Tool agent" : "Echo agent";
  }

  function upsertArtifact(
    artifacts: IngestionArtifact[],
    artifact: IngestionArtifact,
  ) {
    const rest = artifacts.filter((item) => item.id !== artifact.id);
    return [artifact, ...rest];
  }

  function upsertMemoryRecord(records: MemoryRecord[], record: MemoryRecord) {
    const rest = records.filter((item) => item.id !== record.id);
    return [record, ...rest];
  }

  function upsertPromptDoc(docs: PromptDoc[], doc: PromptDoc) {
    const rest = docs.filter((item) => item.name !== doc.name);
    return [doc, ...rest].sort((a, b) => a.name.localeCompare(b.name));
  }

  function upsertSkillDoc(docs: SkillDoc[], doc: SkillDoc) {
    const rest = docs.filter((item) => item.id !== doc.id);
    return [doc, ...rest];
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

  function hasHighRiskFindings(artifact: IngestionArtifact) {
    return artifact.findings.some((finding) => finding.severity === "high");
  }

  function hasHighRiskAdapterFindings(adapterPackage: AdapterPackage) {
    return adapterPackage.findings.some((finding) => finding.severity === "high");
  }

  function enabledPermissions(adapterPackage: AdapterPackage) {
    return Object.entries(adapterPackage.permissions)
      .filter(([, enabled]) => enabled)
      .map(([name]) => name);
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

  function sectionClass(section: ActiveSection) {
    return activeSection === section ? "rail-item active" : "rail-item";
  }

  function showOperationsPanel() {
    return ["chat", "memory", "skills", "prompts", "ingest", "adapters"].includes(
      activeSection,
    );
  }

  function operationsTitle() {
    switch (activeSection) {
      case "chat":
        return "Tools";
      case "memory":
        return "Memory";
      case "skills":
        return "Skills";
      case "prompts":
        return "Prompts and models";
      case "ingest":
        return "Ingestion";
      case "adapters":
        return "Adapters and storage";
      default:
        return "Operations";
    }
  }

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
              Agent {agentDisplayName(demo)}
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
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={running ? "Type /guide to steer this run" : "Ask the agent"}
            rows={4}
          />
          {slashCommandItems.length ? (
            <div className="slash-command-menu" role="listbox" aria-label="Slash commands">
              {slashCommandItems.map((item) => (
                <button
                  type="button"
                  role="option"
                  key={`${item.command}:${item.label}`}
                  onClick={() => setInput(item.command)}
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
              onClick={() => void guideLastRun()}
              disabled={!lastRunId || !input.trim()}
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
            Agent
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
            Provider
            <select
              value={provider}
              onChange={(e) => setProvider(e.target.value as Provider)}
              disabled={running}
            >
              <option value="fake">fake</option>
              <option value="rig">rig</option>
            </select>
          </label>
          <label>
            Model
            <input
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder={provider === "rig" ? "gpt-4o-mini" : "fake-model"}
              disabled={running}
            />
          </label>
          <label>
            API base
            <input
              value={apiBaseUrl}
              onChange={(e) => setApiBaseUrl(e.target.value)}
              placeholder="blank for OpenAI"
              title="Use a custom /v1 base URL only for local or OpenAI-compatible providers."
              disabled={running || provider !== "rig"}
            />
          </label>
          <label>
            API key env
            <input
              value={apiKeyEnv}
              onChange={(e) => setApiKeyEnv(e.target.value)}
              disabled={running || provider !== "rig"}
            />
          </label>
          <label>
            API key
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="optional; not saved"
              disabled={running || provider !== "rig"}
            />
          </label>
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
              checked={loadMemory}
              onChange={(e) => setLoadMemory(e.target.checked)}
              disabled={running}
            />
            <span>Memory</span>
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
                <span key={id}>{id}</span>
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
                {contextCopyStatus ? <span>{contextCopyStatus}</span> : null}
              </div>
              <section>
                <strong>System</strong>
                <pre>{contextPreview.system_prompt}</pre>
              </section>
              <section>
                <strong>Limits</strong>
                <pre>{previewJson(contextPreview.limits)}</pre>
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
              onClick={() => void loadLastTrace()}
              disabled={running || !lastRunId}
            >
              Load Trace
            </button>
            <button
              type="button"
              onClick={() => {
                setTraceEvents([]);
                setTraceSummary(null);
              }}
              disabled={running || !traceEvents.length}
            >
              Clear Trace
            </button>
          </div>
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
              <span>scores {traceSummary.quality_scores}</span>
              <span>memory {traceSummary.memory_fragments}</span>
              <span>artifacts {traceSummary.artifact_refs}</span>
            </div>
          ) : (
            <div className="empty-note">No trace loaded.</div>
          )}
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
              placeholder="tool, model, prompt, artifact, skill, adapter, or batch id"
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
                        <span>{prompt.body.length} chars</span>
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
                  title="Show model Id."
                  onClick={() => void showModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Model
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
              {skillDocs.length ? (
                <div className="ingestion-review">
                  {skillDocs.map((skill) => (
                    <div className="ingestion-card" key={skill.id}>
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
                          title="Allow this skill if its source digest and prompt-injection checks pass."
                          onClick={() => {
                            setOpsId(skill.id);
                            void setSkillQuarantine(true, skill.id);
                          }}
                          disabled={running || !skill.quarantined}
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
                  ))}
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
                  <option value="local-v0">local-v0</option>
                  <option value="local-lines-v0">local-lines-v0</option>
                </select>
              </label>
              <div className="button-grid">
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
                  onClick={includeIngestFromOps}
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
                  className="danger"
                  title="Remove ingestion artifact Id."
                  onClick={() => void removeIngestFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Remove Ingest
                </button>
              </div>
              {ingestionArtifacts.length ? (
                <div className="ingestion-review">
                  {ingestionArtifacts.map((artifact) => (
                    <div
                      className={`ingestion-card ${
                        hasHighRiskFindings(artifact) ? "high-risk" : ""
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
                          {artifact.findings.map((finding) => (
                            <span
                              className={`finding ${finding.severity}`}
                              key={`${artifact.id}:${finding.severity}:${finding.message}`}
                            >
                              {finding.severity}: {finding.message}
                            </span>
                          ))}
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
                  title="Import adapter manifest or package path from Value."
                  onClick={() => void importAdapterFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Adapter
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
              {adapterPackages.length ? (
                <div className="ingestion-review">
                  {adapterPackages.map((adapterPackage) => {
                    const permissions = enabledPermissions(adapterPackage);
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
                            {adapterPackage.capabilities.map((capability) => (
                              <span className="finding" key={capability.id}>
                                {capability.kind}: {capability.name}{" "}
                                {capability.quarantined ? "(quarantined)" : "(allowed)"}
                              </span>
                            ))}
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
            </div>
            ) : null}
          </div>
        </section>
        ) : null}

        {activeSection === "chat" || activeSection === "approvals" ? (
        <section className="panel">
          <div className="panel-title">Control</div>
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
                    <div className="mini-actions">
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
              title="Stop current run"
              onClick={() => void cancelLastRun()}
              disabled={!running || !lastRunId}
            >
              Stop
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

function parseOptionalNonNegativeFloat(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  const parsed = Number.parseFloat(trimmed);
  if (!Number.isFinite(parsed) || parsed < 0) return null;
  return parsed;
}
