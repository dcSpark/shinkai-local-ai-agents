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
} from "./types";

type LineKind = "user" | "assistant" | "event" | "error";
type Transport = "in-process" | "daemon";

interface TranscriptLine {
  kind: LineKind;
  text: string;
}

interface RemoteRunSummary {
  run_id: string;
  final_output: string;
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
  const [calls, setCalls] = useState(0);
  const [demo, setDemo] = useState<Demo>("tool");
  const [provider, setProvider] = useState<Provider>("fake");
  const [transport, setTransport] = useState<Transport>("in-process");
  const [daemonUrl, setDaemonUrl] = useState("http://127.0.0.1:7878");
  const [opsValue, setOpsValue] = useState("");
  const [opsId, setOpsId] = useState("");
  const [opsUserMemory, setOpsUserMemory] = useState(false);
  const [model, setModel] = useState("");
  const [apiBaseUrl, setApiBaseUrl] = useState("");
  const [apiKeyEnv, setApiKeyEnv] = useState("OPENAI_API_KEY");
  const [apiKey, setApiKey] = useState("");
  const [enableShell, setEnableShell] = useState(false);
  const [loadMemory, setLoadMemory] = useState(false);
  const [loadSkills, setLoadSkills] = useState(false);
  const [requireApproval, setRequireApproval] = useState(false);
  const [lastRunId, setLastRunId] = useState<string | null>(null);
  const [contextPreview, setContextPreview] = useState<ContextSnapshot | null>(
    null,
  );

  const transcriptRef = useRef<HTMLElement>(null);
  const terminalEventSeenRef = useRef(false);
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
        setLastRunId(evt.run_id);
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
        appendEvent(
          `LLM call completed (in: ${k.tokens_in}, out: ${k.tokens_out}, ${k.duration_ms} ms)`,
        );
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
        appendEvent(
          `Tool completed [${k.call_id}] -> ${JSON.stringify(k.output)} (${k.duration_ms} ms)`,
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
        terminalEventSeenRef.current = true;
        appendLine("error", `Run paused: ${k.reason}`);
        setRunning(false);
        return;
      case "RunCancelled":
        terminalEventSeenRef.current = true;
        appendLine("error", `Run cancelled: ${k.reason}`);
        setRunning(false);
        return;
      case "RunCompleted":
        terminalEventSeenRef.current = true;
        appendLine("assistant", k.final_output);
        appendEvent(`Run completed in ${k.total_duration_ms} ms`);
        setRunning(false);
        return;
      case "RunFailed":
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
      enable_shell: enableShell,
      load_memory: loadMemory,
      load_skills: loadSkills,
      include_ingest: [],
      require_approval: requireApproval,
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

  function appendJson(label: string, value: unknown) {
    appendEvent(label);
    appendLine("assistant", JSON.stringify(value, null, 2));
  }

  async function loadTraceFor(runId: string) {
    const events =
      transport === "daemon"
        ? await daemonJson<RunEvent[]>(`/trace/${runId}`)
        : await invoke<RunEvent[]>("trace_show", { runId });
    appendEvent(`Loaded trace ${runId} (${events.length} events)`);
    for (const evt of events) {
      appendEvent(`[${evt.id}] ${evt.kind.type}`);
    }
  }

  async function submit() {
    const prompt = input.trim();
    if (!prompt || running) return;
    setInput("");
    setRunning(true);
    setTokensIn(0);
    setTokensOut(0);
    setCalls(0);
    terminalEventSeenRef.current = false;
    appendLine("user", prompt);

    try {
      if (transport === "daemon") {
        const summary = await daemonJson<RemoteRunSummary>("/run", {
          input: prompt,
          demo,
          ...runtimeOptions(),
        });
        setLastRunId(summary.run_id);
        appendLine("assistant", summary.final_output);
        appendEvent(`Remote run completed: ${summary.run_id}`);
        setRunning(false);
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
    try {
      if (transport === "daemon") {
        await daemonJson("/memory/rollback", { user: opsUserMemory });
      } else {
        await invoke("memory_rollback", { user: opsUserMemory });
      }
      appendEvent(`Memory rollback complete (${opsUserMemory ? "user" : "agent"})`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Memory rollback failed: ${msg}`);
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
    try {
      const artifact =
        transport === "daemon"
          ? await daemonJson<unknown>("/ingest", { path })
          : await invoke<unknown>("ingest_add", { path });
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
    try {
      if (transport === "daemon") {
        await daemonJson(`/ingest/${id}/rm`, {});
      } else {
        await invoke("ingest_rm", { id });
      }
      appendEvent(`Ingestion artifact removed: ${id}`);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      appendLine("error", `Ingest remove failed: ${msg}`);
    }
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
        <div className="rail-mark">AI</div>
        <button type="button" className="rail-item active" title="Chat">
          C
        </button>
        <button
          type="button"
          className="rail-item"
          title="Trace"
          onClick={() => void loadLastTrace()}
          disabled={running || !lastRunId}
        >
          T
        </button>
        <button
          type="button"
          className="rail-item"
          title="Memory"
          onClick={() => void reviewMemory()}
          disabled={running}
        >
          M
        </button>
        <button
          type="button"
          className="rail-item"
          title="Skills"
          onClick={() => void reviewSkills()}
          disabled={running}
        >
          S
        </button>
        <button
          type="button"
          className="rail-item"
          title="Ingest"
          onClick={() => void reviewIngestion()}
          disabled={running}
        >
          I
        </button>
        <button
          type="button"
          className="rail-item"
          title="Adapters"
          onClick={() => void reviewAdapters()}
          disabled={running}
        >
          A
        </button>
        <button
          type="button"
          className="rail-item rail-bottom"
          title="Approvals"
          onClick={() => void reviewApprovals()}
          disabled={running || !lastRunId}
        >
          !
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
              <option value="in-process">in-process</option>
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
              checked={requireApproval}
              onChange={(e) => setRequireApproval(e.target.checked)}
              disabled={running}
            />
            <span>Approval gate</span>
          </label>
          <button
            type="button"
            onClick={() => void previewCurrentContext()}
            disabled={running}
          >
            Preview Context
          </button>
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
              onClick={() => void deleteMemoryFromOps()}
              disabled={running || !opsId.trim()}
            >
              Delete Mem
            </button>
            <button
              type="button"
              onClick={() => void rollbackMemoryFromOps()}
              disabled={running}
            >
              Rollback
            </button>
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
              onClick={() => void removeIngestFromOps()}
              disabled={running || !opsId.trim()}
            >
              Remove Ingest
            </button>
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
              disabled={!running || !lastRunId || transport === "daemon"}
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
