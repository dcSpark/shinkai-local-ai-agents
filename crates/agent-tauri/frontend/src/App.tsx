import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  ContextSnapshot,
  Demo,
  Provider,
  RunEvent,
  RunOptions,
  RunSummary,
  ToolVisibility,
} from "./types";

type LineKind = "user" | "assistant" | "event" | "error";
type Transport = "in-process" | "daemon";

interface TranscriptLine {
  kind: LineKind;
  text: string;
}

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
  memory_fragments: number;
  artifact_refs: number;
  tokens_in: number;
  tokens_out: number;
  cost_usd: number | null;
  duration_ms: number | null;
}

const CALLS_MAX = 5;

function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export default function App() {
  const [transcript, setTranscript] = useState<TranscriptLine[]>([
    {
      kind: "event",
      text: "Welcome. Type a message and press Cmd/Ctrl+Enter to send.",
    },
  ]);
  const [input, setInput] = useState("");
  const [running, setRunning] = useState(false);
  const [tokensIn, setTokensIn] = useState(0);
  const [tokensOut, setTokensOut] = useState(0);
  const [costUsd, setCostUsd] = useState(0);
  const [calls, setCalls] = useState(0);
  const [demo, setDemo] = useState<Demo>("tool");
  const [provider, setProvider] = useState<Provider>("fake");
  const tauriRuntime = hasTauriRuntime();
  const [transport, setTransport] = useState<Transport>(() =>
    tauriRuntime ? "in-process" : "daemon",
  );
  const [daemonUrl, setDaemonUrl] = useState("http://127.0.0.1:7878");
  const [opsValue, setOpsValue] = useState("");
  const [opsId, setOpsId] = useState("");
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
  const [traceEvents, setTraceEvents] = useState<RunEvent[]>([]);
  const [traceSummary, setTraceSummary] = useState<TraceSummary | null>(null);

  const transcriptRef = useRef<HTMLElement>(null);
  const terminalEventSeenRef = useRef(false);
  const rootRunIdRef = useRef<string | null>(null);
  const remoteSeenEventKeysRef = useRef<Set<string>>(new Set());
  const runLabel = lastRunId ? lastRunId.slice(0, 8) : "none";

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

  function handleRunEvent(evt: RunEvent) {
    const k = evt.kind;
    switch (k.type) {
      case "RunStarted":
        if (rootRunIdRef.current === null) {
          rootRunIdRef.current = evt.run_id;
          setLastRunId(evt.run_id);
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
        appendEvent(`LLM call started (${k.model})`);
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
      case "ToolCallFailed":
        appendLine("error", `Tool failed [${k.call_id}]: ${k.error}`);
        return;
      case "ApprovalRequested":
        appendEvent(`Approval requested [${k.approval_id}] for ${k.action}`);
        return;
      case "ApprovalResolved":
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
      case "MemoryWritten":
        appendEvent(`Memory ${k.operation}: ${k.id}`);
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
        return;
      case "RunCancelled":
        if (evt.run_id !== rootRunIdRef.current) {
          appendEvent(`Run cancelled: ${evt.run_id.slice(0, 8)} (${k.reason})`);
          return;
        }
        terminalEventSeenRef.current = true;
        appendLine("error", `Run cancelled: ${k.reason}`);
        setRunning(false);
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
        setRunning(false);
        return;
      case "RunFailed":
        if (evt.run_id !== rootRunIdRef.current) {
          appendEvent(`Run failed: ${evt.run_id.slice(0, 8)} (${k.reason})`);
          return;
        }
        terminalEventSeenRef.current = true;
        appendLine("error", `Run failed: ${k.reason}`);
        setRunning(false);
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
        case "MemoryLoaded":
          memoryFragments += kind.ids.length;
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
    if (!prompt || running) return;

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

    setInput("");
    setRunning(true);
    setTokensIn(0);
    setTokensOut(0);
    setCostUsd(0);
    setCalls(0);
    setTraceEvents([]);
    setTraceSummary(null);
    terminalEventSeenRef.current = false;
    rootRunIdRef.current = null;
    remoteSeenEventKeysRef.current = new Set();
    appendLine("user", savedPromptName ? `/run ${savedPromptName}` : prompt);

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
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      captureRunIdFromError(msg);
      if (!terminalEventSeenRef.current) {
        appendLine("error", `Invoke failed: ${msg}`);
      }
      setRunning(false);
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
        appendLine("assistant", JSON.stringify(output, null, 2));
        return;
      }
      const output = await invoke<unknown>("call_tool", {
        name: "shell",
        input: { command },
        options: { ...runtimeOptions(), enable_shell: true },
      });
      appendLine("assistant", JSON.stringify(output, null, 2));
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
    await callToolDirect(name, inputBody, `tool ${name} ${JSON.stringify(inputBody)}`);
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
      appendJson("Tool output", output);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      captureRunIdFromError(msg);
      appendLine("error", `Tool call failed: ${msg}`);
    }
  }

  async function previewCurrentContext() {
    const prompt = input.trim() || "preview";
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
      appendEvent(
        `Preview: ${snapshot.visible_tools.length} tools, ${snapshot.visible_skills.length} skills, ${snapshot.loaded_memory.length} memory, ${snapshot.loaded_artifacts.length} artifacts, ${snapshot.provenance.length} provenance records`,
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
          ? await daemonJson<unknown[]>("/memory")
          : await invoke<unknown[]>("memory_list");
      appendEvent(`Memory records: ${records.length}`);
      appendLine("assistant", JSON.stringify(records, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory review failed: ${msg}`);
    }
  }

  async function reviewSkills() {
    try {
      const docs =
        transport === "daemon"
          ? await daemonJson<unknown[]>("/skills")
          : await invoke<unknown[]>("skill_list");
      appendEvent(`Skills: ${docs.length}`);
      appendLine("assistant", JSON.stringify(docs, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill review failed: ${msg}`);
    }
  }

  async function reviewPrompts() {
    try {
      const prompts =
        transport === "daemon"
          ? await daemonJson<unknown[]>("/prompts")
          : await invoke<unknown[]>("prompt_list");
      appendEvent(`Saved prompts: ${prompts.length}`);
      appendLine("assistant", JSON.stringify(prompts, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt review failed: ${msg}`);
    }
  }

  async function reviewIngestion() {
    try {
      const artifacts =
        transport === "daemon"
          ? await daemonJson<unknown[]>("/ingest")
          : await invoke<unknown[]>("ingest_list");
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

  async function guideLastRun() {
    if (!lastRunId || !input.trim()) return;
    const text = input.trim();
    setInput("");
    try {
      if (transport === "daemon") {
        await daemonJson("/guide", {
          run_id: lastRunId,
          text,
        });
      } else {
        await invoke("guide", { runId: lastRunId, text });
      }
      appendEvent(`Guidance recorded for ${lastRunId}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Guide failed: ${msg}`);
    }
  }

  async function scoreLastRun() {
    if (!lastRunId) return;
    try {
      if (transport === "daemon") {
        await daemonJson("/score", {
          run_id: lastRunId,
          target: "last_answer",
          score: 10,
        });
      } else {
        await invoke("score", {
          runId: lastRunId,
          target: "last_answer",
          score: 10,
        });
      }
      appendEvent(`Score recorded for ${lastRunId}`);
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
      const approvals =
        transport === "daemon"
          ? await daemonJson<Array<{ approval_id: string; status: string }>>(
              `/approvals/${lastRunId}`,
            )
          : await invoke<Array<{ approval_id: string; status: string }>>(
              "approval_list",
              { runId: lastRunId },
            );
      appendEvent(`Approvals for ${lastRunId}: ${approvals.length}`);
      appendLine("assistant", JSON.stringify(approvals, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval review failed: ${msg}`);
    }
  }

  async function approveFirstPending() {
    if (!lastRunId) return;
    try {
      const approvals =
        transport === "daemon"
          ? await daemonJson<Array<{ approval_id: string; status: string }>>(
              `/approvals/${lastRunId}`,
            )
          : await invoke<Array<{ approval_id: string; status: string }>>(
              "approval_list",
              { runId: lastRunId },
            );
      const pending = approvals.find((approval) => approval.status === "pending");
      if (!pending) {
        appendEvent(`No pending approvals for ${lastRunId}`);
        return;
      }
      const output =
        transport === "daemon"
          ? await (async () => {
              await daemonJson(
                `/approvals/${lastRunId}/${pending.approval_id}/decide`,
                { approved: true },
              );
              return daemonJson<unknown>(
                `/approvals/${lastRunId}/${pending.approval_id}/execute`,
                {},
              );
            })()
          : await (async () => {
              await invoke("approval_decide", {
                runId: lastRunId,
                approvalId: pending.approval_id,
                approved: true,
              });
              return invoke<unknown>("approval_execute", {
                runId: lastRunId,
                approvalId: pending.approval_id,
              });
            })();
      appendEvent(`Approved and executed ${pending.approval_id}`);
      appendLine("assistant", JSON.stringify(output, null, 2));
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Approval decision failed: ${msg}`);
    }
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
          ? await daemonJson<unknown[]>("/adapters")
          : await invoke<unknown[]>("adapter_list");
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
          ? await daemonJson<unknown>("/memory", {
              content,
              user: opsUserMemory,
            })
          : await invoke<unknown>("memory_create", {
              content,
              user: opsUserMemory,
            });
      appendJson("Memory created", record);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory create failed: ${msg}`);
    }
  }

  async function generateMemoryFromOps() {
    const text = requireOpsValue("Memory generate");
    if (!text) return;
    try {
      const records =
        transport === "daemon"
          ? await daemonJson<unknown>("/memory/generate", {
              text,
              user: opsUserMemory,
              range: null,
            })
          : await invoke<unknown>("memory_generate", {
              text,
              user: opsUserMemory,
              range: null,
            });
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
          ? await daemonJson<unknown>(`/memory/${id}/edit`, { content })
          : await invoke<unknown>("memory_edit", { id, content });
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
          ? await daemonJson<unknown[]>("/memory")
          : await invoke<unknown[]>("memory_list");
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
          ? await daemonJson<unknown>("/prompts", { name, body })
          : await invoke<unknown>("prompt_save", { name, body });
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
          ? await daemonJson<unknown>(`/prompts/${encodeURIComponent(name)}`)
          : await invoke<unknown>("prompt_show", { name });
      appendJson("Prompt", prompt);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Prompt show failed: ${msg}`);
    }
  }

  async function usePromptFromOps() {
    const name = requireOpsId("Use prompt");
    if (!name) return;
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
    if (!confirmLocalChange(`Delete prompt ${name}`)) return;
    try {
      const output =
        transport === "daemon"
          ? await daemonJson<unknown>(
              `/prompts/${encodeURIComponent(name)}/delete`,
              {},
            )
          : await invoke<unknown>("prompt_delete", { name });
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
          ? await daemonJson<unknown>("/skills/import", { path })
          : await invoke<unknown>("skill_import_openclaw", { path });
      appendJson("Skill imported into quarantine", doc);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Skill import failed: ${msg}`);
    }
  }

  async function setSkillQuarantine(allow: boolean) {
    const id = requireOpsId(allow ? "Skill allow" : "Skill quarantine");
    if (!id) return;
    try {
      const doc =
        transport === "daemon"
          ? await daemonJson<unknown>(
              `/skills/${id}/${allow ? "allow" : "quarantine"}`,
              {},
            )
          : await invoke<unknown>(allow ? "skill_allow" : "skill_quarantine", {
              id,
            });
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
      const artifact =
        transport === "daemon"
          ? await daemonJson<unknown>("/ingest", { path, backend })
          : await invoke<unknown>("ingest_add", { path, backend });
      appendJson("Ingestion artifact created", artifact);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest failed: ${msg}`);
    }
  }

  async function showIngestFromOps() {
    const id = requireOpsId("Ingest show");
    if (!id) return;
    try {
      const artifact =
        transport === "daemon"
          ? await daemonJson<unknown>(`/ingest/${id}`)
          : await invoke<unknown>("ingest_show", { id });
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
      appendEvent(`Ingestion artifact removed: ${id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest remove failed: ${msg}`);
    }
  }

  function includeIngestFromOps() {
    const id = requireOpsId("Use ingest");
    if (!id) return;
    setIncludeIngestIds((ids) => (ids.includes(id) ? ids : [...ids, id]));
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
          ? await daemonJson<unknown>("/adapters/import", { path })
          : await invoke<unknown>("adapter_import", { path });
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
          ? await daemonJson<unknown>(`/adapters/${id}`)
          : await invoke<unknown>("adapter_show", { id });
      appendJson("Adapter manifest", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter show failed: ${msg}`);
    }
  }

  async function setAdapterQuarantine(allow: boolean) {
    const id = requireOpsId(allow ? "Adapter allow" : "Adapter quarantine");
    if (!id) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<unknown>(
              `/adapters/${id}/${allow ? "allow" : "quarantine"}`,
              {},
            )
          : await invoke<unknown>(
              allow ? "adapter_allow" : "adapter_quarantine",
              { id },
            );
      appendJson(allow ? "Adapter allowed" : "Adapter quarantined", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Adapter update failed: ${msg}`);
    }
  }

  async function exportBundleFromOps() {
    const path = requireOpsValue("Bundle export");
    if (!path) return;
    try {
      const manifest =
        transport === "daemon"
          ? await daemonJson<unknown>("/bundles/export", { path })
          : await invoke<unknown>("bundle_export", { path });
      appendJson("Bundle exported", manifest);
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
          ? await daemonJson<unknown>("/bundles/import", { path })
          : await invoke<unknown>("bundle_import", { path });
      appendJson("Bundle imported", manifest);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Bundle import failed: ${msg}`);
    }
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void submit();
    }
  }

  function previewJson(value: unknown) {
    return JSON.stringify(value, null, 2);
  }

  return (
    <div className="app-shell">
      <aside className="rail" aria-label="Agent workspace sections">
        <div className="rail-mark" title="Agent Harness" aria-label="Agent Harness">
          <span className="rail-letter">AI</span>
          <span className="rail-label">Harness</span>
        </div>
        <button
          type="button"
          className="rail-item active"
          title="Chat"
          aria-label="Chat transcript"
        >
          <span className="rail-letter">C</span>
          <span className="rail-label">Chat</span>
        </button>
        <button
          type="button"
          className="rail-item"
          title="Trace"
          aria-label="Trace viewer"
          onClick={() => void loadLastTrace()}
          disabled={running || !lastRunId}
        >
          <span className="rail-letter">T</span>
          <span className="rail-label">Trace</span>
        </button>
        <button
          type="button"
          className="rail-item"
          title="Memory"
          aria-label="Memory records"
          onClick={() => void reviewMemory()}
          disabled={running}
        >
          <span className="rail-letter">M</span>
          <span className="rail-label">Memory</span>
        </button>
        <button
          type="button"
          className="rail-item"
          title="Skills"
          aria-label="Skill library"
          onClick={() => void reviewSkills()}
          disabled={running}
        >
          <span className="rail-letter">S</span>
          <span className="rail-label">Skills</span>
        </button>
        <button
          type="button"
          className="rail-item"
          title="Prompts"
          aria-label="Saved prompts"
          onClick={() => void reviewPrompts()}
          disabled={running}
        >
          <span className="rail-letter">P</span>
          <span className="rail-label">Prompts</span>
        </button>
        <button
          type="button"
          className="rail-item"
          title="Ingest"
          aria-label="Ingestion artifacts"
          onClick={() => void reviewIngestion()}
          disabled={running}
        >
          <span className="rail-letter">I</span>
          <span className="rail-label">Ingest</span>
        </button>
        <button
          type="button"
          className="rail-item"
          title="Adapters"
          aria-label="Adapter manifests"
          onClick={() => void reviewAdapters()}
          disabled={running}
        >
          <span className="rail-letter">A</span>
          <span className="rail-label">Adapters</span>
        </button>
        <button
          type="button"
          className="rail-item rail-bottom"
          title="Approvals"
          aria-label="Approvals"
          onClick={() => void reviewApprovals()}
          disabled={running || !lastRunId}
        >
          <span className="rail-letter">!</span>
          <span className="rail-label">Approvals</span>
        </button>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <h1>Agent Harness</h1>
            <div className="run-meta">run {runLabel}</div>
          </div>
          <div className="status-pills">
            <span className={running ? "pill running" : "pill idle"}>
              {running ? "running" : "idle"}
            </span>
            <span className="pill">tokens {tokensIn}/{tokensOut}</span>
            <span className="pill">cost ${costUsd.toFixed(6)}</span>
            <span className="pill">calls {calls}/{CALLS_MAX}</span>
          </div>
        </header>

        <section className="transcript" aria-live="polite" ref={transcriptRef}>
          {transcript.map((line, i) => (
            <div key={i} className={`line line-${line.kind}`}>
              <span className="prefix">{prefixFor(line.kind)}</span>
              <span className="content">{line.text}</span>
            </div>
          ))}
        </section>

        <footer className="composer">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={running ? "Run in progress" : "Ask the agent"}
            disabled={running}
            rows={4}
          />
          <div className="composer-actions">
            <button
              type="button"
              className="primary"
              onClick={() => void submit()}
              disabled={running || !input.trim()}
            >
              Send
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
              disabled={running || !lastRunId || !input.trim()}
            >
              Guide
            </button>
          </div>
        </footer>
      </main>

      <aside className="inspector">
        <section className="panel">
          <div className="panel-title">Run</div>
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
            Demo
            <select
              value={demo}
              onChange={(e) => setDemo(e.target.value as Demo)}
              disabled={running}
            >
              <option value="echo">echo</option>
              <option value="tool">tool</option>
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

        <section className="panel">
          <div className="panel-title">Context</div>
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
          <label className="switch">
            <input
              type="checkbox"
              checked={rawToolOutput}
              onChange={(e) => setRawToolOutput(e.target.checked)}
              disabled={running}
            />
            <span>Raw tool output</span>
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
              <section>
                <strong>System</strong>
                <pre>{contextPreview.system_prompt}</pre>
              </section>
              <section>
                <strong>Limits</strong>
                <pre>{previewJson(contextPreview.limits)}</pre>
              </section>
              <section>
                <strong>Conversation</strong>
                <pre>{previewJson(contextPreview.conversation)}</pre>
              </section>
              <section>
                <strong>Tools</strong>
                <pre>{previewJson(contextPreview.visible_tools)}</pre>
              </section>
              <section>
                <strong>Skills</strong>
                <pre>{previewJson(contextPreview.visible_skills)}</pre>
              </section>
              <section>
                <strong>Memory</strong>
                <pre>{previewJson(contextPreview.loaded_memory)}</pre>
              </section>
              <section>
                <strong>Artifacts</strong>
                <pre>{previewJson(contextPreview.loaded_artifacts)}</pre>
              </section>
              <section>
                <strong>Provenance</strong>
                <pre>{previewJson(contextPreview.provenance)}</pre>
              </section>
            </div>
          ) : null}
        </section>

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

        <section className="panel">
          <div className="panel-title">Operations</div>
          <label>
            Path or content
            <textarea
              className="ops-text"
              value={opsValue}
              onChange={(e) => setOpsValue(e.target.value)}
              disabled={running}
              rows={3}
            />
          </label>
          <label>
            Id
            <input
              value={opsId}
              onChange={(e) => setOpsId(e.target.value)}
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
            <div className="operation-group">
              <div className="operation-title">Memory</div>
              <div className="button-grid">
                <button
                  type="button"
                  onClick={() => void createMemoryFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Add Memory
                </button>
                <button
                  type="button"
                  onClick={() => void generateMemoryFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Generate
                </button>
                <button
                  type="button"
                  onClick={() => void editMemoryFromOps()}
                  disabled={running || !opsValue.trim() || !opsId.trim()}
                >
                  Edit Mem
                </button>
                <button
                  type="button"
                  className="danger"
                  onClick={() => void deleteMemoryFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Mem
                </button>
                <button
                  type="button"
                  className="danger"
                  onClick={() => void rollbackMemoryFromOps()}
                  disabled={running}
                >
                  Rollback
                </button>
              </div>
            </div>

            <div className="operation-group">
              <div className="operation-title">Prompts</div>
              <div className="button-grid">
                <button
                  type="button"
                  onClick={() => void savePromptFromOps()}
                  disabled={running || !opsValue.trim() || !opsId.trim()}
                >
                  Save Prompt
                </button>
                <button
                  type="button"
                  onClick={() => void showPromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Prompt
                </button>
                <button
                  type="button"
                  onClick={() => void usePromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Use Prompt
                </button>
                <button
                  type="button"
                  className="danger"
                  onClick={() => void deletePromptFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Prompt
                </button>
              </div>
            </div>

            <div className="operation-group">
              <div className="operation-title">Models</div>
              <div className="button-grid">
                <button
                  type="button"
                  onClick={() => void listModelsFromOps()}
                  disabled={running}
                >
                  List Models
                </button>
                <button
                  type="button"
                  onClick={() => void showModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Model
                </button>
                <button
                  type="button"
                  onClick={() => void saveModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Save Model
                </button>
                <button
                  type="button"
                  className="danger"
                  onClick={() => void deleteModelFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Delete Model
                </button>
              </div>
            </div>

            <div className="operation-group">
              <div className="operation-title">Skills</div>
              <div className="button-grid">
                <button
                  type="button"
                  onClick={() => void importSkillFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Skill
                </button>
                <button
                  type="button"
                  onClick={() => void setSkillQuarantine(true)}
                  disabled={running || !opsId.trim()}
                >
                  Allow Skill
                </button>
                <button
                  type="button"
                  onClick={() => void setSkillQuarantine(false)}
                  disabled={running || !opsId.trim()}
                >
                  Quarantine
                </button>
              </div>
            </div>

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
                  onClick={() => void ingestPathFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Ingest
                </button>
                <button
                  type="button"
                  onClick={() => void showIngestFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Ingest
                </button>
                <button
                  type="button"
                  onClick={includeIngestFromOps}
                  disabled={running || !opsId.trim()}
                >
                  Use Ingest
                </button>
                <button
                  type="button"
                  className="danger"
                  onClick={() => void removeIngestFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Remove Ingest
                </button>
              </div>
            </div>

            <div className="operation-group">
              <div className="operation-title">Tools</div>
              <div className="button-grid">
                <button
                  type="button"
                  onClick={() => void callToolFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Call Tool
                </button>
              </div>
            </div>

            <div className="operation-group">
              <div className="operation-title">Adapters</div>
              <div className="button-grid">
                <button
                  type="button"
                  onClick={() => void importAdapterFromOps()}
                  disabled={running || !opsValue.trim()}
                >
                  Import Adapter
                </button>
                <button
                  type="button"
                  onClick={() => void showAdapterFromOps()}
                  disabled={running || !opsId.trim()}
                >
                  Show Adapter
                </button>
                <button
                  type="button"
                  onClick={() => void setAdapterQuarantine(true)}
                  disabled={running || !opsId.trim()}
                >
                  Allow Adapter
                </button>
                <button
                  type="button"
                  onClick={() => void setAdapterQuarantine(false)}
                  disabled={running || !opsId.trim()}
                >
                  Block Adapter
                </button>
              </div>
            </div>

            <div className="operation-group">
              <div className="operation-title">Bundles</div>
              <div className="button-grid">
                <button
                  type="button"
                  onClick={() => void exportBundleFromOps()}
                  disabled={running || !opsValue.trim() || transport === "daemon"}
                >
                  Export
                </button>
                <button
                  type="button"
                  onClick={() => void importBundleFromOps()}
                  disabled={running || !opsValue.trim() || transport === "daemon"}
                >
                  Import
                </button>
              </div>
            </div>
          </div>
        </section>

        <section className="panel">
          <div className="panel-title">Control</div>
          <div className="button-grid">
            <button
              type="button"
              onClick={() => void approveFirstPending()}
              disabled={running || !lastRunId}
            >
              Approve
            </button>
            <button
              type="button"
              onClick={() => void scoreLastRun()}
              disabled={running || !lastRunId}
            >
              Score 10
            </button>
            <button
              type="button"
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
