# Agent Harness CLI — General Requirements

Last updated: 2026-04-24
Source of truth: `2_Ideal_Agent_Harness_CLI_specs_Gilles.txt` (April 2026)

This document captures **what** the runtime must do and **why**.
Implementation shape (crates, runtime stack, build sequence) lives in `architecture.md`.

---

## 1. Product Intent

### Goal
A transparent, configurable, low-overhead **CLI runtime** for controlled AI agents. UIs, messaging bridges, and mobile clients sit on top of the same runtime API.

### Not
- An opaque autonomous-agent platform
- A chat app first
- A memory-heavy assistant
- A provider-locked framework

### Origin
This spec captures the felt limitations with Shinkai and the recurring pain points with Hermes-Agent / Codex / similar tools:

- Opinionated, always-on defaults
- Hidden work the user did not ask for (slower, costlier, harder to reason about)
- Black-box behaviour and weak visibility into what is actually happening per call
- Rigid agent configuration
- Resource-heavy footprint (storage, compute, tokens, time)

The CLI is designed against those properties, not in spite of them.

---

## 2. Core Promise

The user can always inspect, for any run:

- Which model was used
- The exact context sent
- Which tools were visible (and at which level of disclosure)
- Which skills were visible
- Which memory was loaded
- Which policies were applied (and from which layer)
- Which tool calls were made
- Tokens, cost, and wall-clock time
- Provenance of the final output

If a fact about a run cannot be inspected, the runtime is wrong.

---

## 3. Default Posture (Safe Defaults)

```
memory                off
agent-created tools   off
payments              off
broad file access     off
document ingestion    explicit / manual
third-party imports   quarantined
dangerous actions     approval-gated
```

Each of these can be enabled, but only at an explicit, scoped level (per agent, per profile, per conversation, per run). "Explicit / manual" for ingestion means: the runtime never silently pulls files into context; the user (or an explicitly-authorised agent step) initiates every ingestion.

---

## 4. Use Cases

The runtime must serve all of the following, without forcing any of them on every user.

### 4.1 Simple low-overhead agent
Zero or one tool call, raw answer. No memory, no subagents, no compaction, no extra interpretation unless explicitly enabled.

### 4.2 Chat as action router
Natural language → small routing model → tool/action selection → raw tool result. Examples: game-by-chat, app workflow trigger, CLI command replacement, one-shot business action.

### 4.3 Cheap router + specialist interpreter
Small model picks tool → tool output → stronger / specialist / multimodal model interprets. Examples: test logs, PDFs, images, tables, complex documents.

### 4.4 Deterministic batch
Folder / file list / prompt list → runtime-owned loop. 100% item accounting, per-item trace, resume/retry. Never "the LLM promised to process all files."

### 4.5 Deterministic workflow with agent steps
Deterministic step → agent or subagent step → deterministic step → final artifact. Examples: code-review pipeline, document-processing pipeline, Shinkai-style tool with LLM steps inside.

### 4.6 Document-heavy agent
Replaceable ingestion backend with text / table / vision / layout-aware paths. Choice can vary per agent, per file type, per modality.

### 4.7 Memory-enabled personal or self-evolving agent
Generation off by default. Loading off by default. Generation ≠ loading. Manual edit + rollback supported.

### 4.8 Profiles and team sharing
Profile-level authorisation. No accidental global sharing. Access provenance recorded in the trace.

### 4.9 Long conversation management
Compaction, branching, deletion, quality scoring — all built on the same trace/run model.

### 4.10 Prompt refinement for external users
Per-agent / per-topic refinement step. Actual agent sees the improved prompt; refinement is a traceable preprocessing step.

### 4.11 External content with prompt-injection guardrail
Separate guardrail model evaluates risk; outcome (allow / warn / block) is acted on, but the guardrail's reasoning is **not** auto-injected into the main agent's context.

### 4.12 High-risk tool use
File write, broad file read, shell, network, secret, wallet/payment → human approval gate → traced execution.

### 4.13 Mobile / messaging / voice access
Voice, Telegram, Slack, Teams, web embed → runtime API → same agent run, same trace. Interface changes; runtime behaviour does not.

### 4.14 Document generation and viewing
Generated artifact (PDF / Word / CSV / Excel / PPTX) with one-click view or one-click open in the OS default app.

### 4.15 Agent-created tools / skills / agents
Agent proposes a capability → draft / quarantine → human review → scoped enablement. Disabled by default. No auto-trust. Provenance + rollback required.

---

## 5. Functional Requirements

These are the specific capabilities the runtime must expose, organised the way Gilles' source spec organises them.

### 5.1 Model provider agnostic

- One canonical function to call any LLM/agent regardless of provider (OpenAI-compatible by default).
- Provider integrations are independent: a failure in one must not regress the others.
- Backends can be cloud or local (Ollama, llama.cpp, etc.).
- New providers can be added without touching the run engine.

**Why:** ergonomics, fault isolation, freedom to swap inference backends.

### 5.2 User-configurable models

For every LLM the user can declare:

- Max context length
- Max output tokens
- Available modalities
- Tool support
- Reasoning mode
- Default temperature
- Cost (per-token in/out, user-overridable)
- Privacy level, cost tier, arbitrary metadata

Users can integrate new models themselves without runtime changes.

### 5.3 Tool / skill calling modularity

- **Default:** multi-call allowed, sequential and safe-parallel.
- **Configurable per agent:** zero-call, one-call, max-N calls.
- The agent must know its remaining tool-call budget and receive a warning as it approaches the limit.

**Why:** simple agents stay simple; complex tasks can be capped explicitly; the agent can plan around the budget instead of being silently truncated.

### 5.4 Output interpretation (raw vs LLM-interpreted)

- **Default:** tool/skill output may be processed by an LLM.
- Per tool/skill **and** per agent, the user can pin output to **raw** or **interpreted**.
- Per tool/skill: an optional `output_interpretation_guidance` field that can be:
  - activated/deactivated at the tool level,
  - overridden per agent,
  - overridden per tool *within* an agent.

**Why:**
- Reduces tokens and latency where interpretation adds nothing.
- Some outputs lose information when paraphrased by an LLM.
- Enables specialist routing agents (chat as a trigger only, not as an interpreter).
- Same tool can feed multiple downstream interpreters that each frame the result differently.

### 5.5 LLM granularity per agent

A single agent can use different LLMs at different stages:

- Routing / tool-call selection
- General prompt processing
- Tool/skill output interpretation (default model)
- Tool/skill output interpretation (per-tool override at agent level)

**Why:** narrow agents can route with tiny models (Function-Gemma, small Qwen, small Ministral) and only escalate to a strong or multimodal model when an output genuinely needs interpretation.

### 5.6 Subagents

- The LLM/agent prompt-processing core is callable from inside tools/skills and from agents themselves.
- Subagents behave like tool calls (sequential / parallel / max-N).
- Subagent activity is **observable in real time** from the parent run.
- Subagents can call subagents.
- Max depth is configurable.

**Why:** narrower tasks are cheaper and easier to maintain; deterministic workflows can embed agentic steps; nothing is hidden behind a black box.

### 5.7 Document ingestion

- The default ingestion backend must handle complex layouts, tables, graphs, and images.
- Optional vision-based ingestion path.
- The backend is replaceable.
- Multiple ingestion backends can coexist; the user chooses per agent / file type / modality.

**Why:** ingestion quality dominates downstream task quality; SOTA evolves quickly and the runtime must follow without rewriting agents.

### 5.8 Tool / skill visibility and progressive loading

How much information the agent sees about its tools/skills is configurable per item and per agent. Available levels:

- Full schema (names + descriptions + parameters)
- Names + descriptions, parameters loaded only when the tool is selected
- Names only, details fetched only when the tool is being considered

Defaults are set at the tool/skill level; agent-level overrides take precedence.

**Why:** small models stay accurate with progressive disclosure; broad toolsets stay viable without context bloat; primary tools can stay fully visible while secondary tools live "below the fold."

### 5.9 Tool / skill organisation (categories / packs)

- Tools/skills can belong to one or more categories.
- Whole categories can be activated/deactivated at global, agent, or conversation level.
- Categories can be granted to other profiles.

### 5.10 Memory

Memory is split into **generation** and **loading**, configured separately. Both are **off by default**.

#### 5.10.1 Generation
- Activatable per agent, per task/topic, per conversation.
- Non-blocking: real-time generation does not block agent usage.
- Can be **asynchronous** (cron) or **manual** (explicit command).
- Can be **scoped to a selected conversation range** (highlight a section, generate memory only from that part).
- Multiple memory backends / SDKs / frameworks can coexist; they are pluggable.
- Each memory backend can use its own LLM, overridable per agent.

#### 5.10.2 Loading
- Activatable/deactivatable **separately from generation**, at agent / conversation / topic level.
- Loaded memory must appear in the context preview (§5.12).

#### 5.10.3 Accessibility scope
- Default: memory is private to the generating agent.
- The user can grant memory access across agents and across profiles.
- Memory can be exported/imported per agent.

#### 5.10.4 Editable & versioned
- Memories live in a human-readable format.
- Versioned with at least 1–2 step rollback.
- Pre-defined starting memories can ship with an agent (useful for interactive experiences and for shipping curated knowledge).

### 5.11 Configurable context compaction

- Compaction = keep the most important parts of the conversation; the full history is no longer re-sent.
- The user can **guide** compaction (what to keep, what to drop) at agent / conversation / manual-trigger level.
- Compacted context is **portable**: exportable to other conversations, profiles, or machines.
- Manual trigger at any point in a conversation.
- Configurable max-tokens-before-compaction and max-output-tokens-of-compaction.
- Parameters layered: global → agent → conversation → manual trigger.
- **Branch-compatible:** the original full conversation remains accessible even after compaction, because branches may need it.

### 5.12 Observable modular context construction

For any prompt the user types — **before** it is sent — the user can preview the exact context that will go to the LLM:

- System prompt
- Conversation history
- Compacted context
- Loaded memory
- Visible tools (at the chosen disclosure level)
- Visible skills
- Profile provenance for any inherited fragment
- Runtime limits / tool-call budget

The same builder produces the preview and the actual request — they cannot diverge.

**Why:** no black box; users can verify their agent is configured as intended, spot misconfigurations, and reason about token cost before paying for it.

### 5.13 Branching conversations

- Conversations can branch at any point.
- Branches can be deleted independently of the main branch (deletion stops at the last branching point).
- The main branch is **not duplicated on disk** for each branch.
- Branches must be visualisable: a tree (or similar) view showing topics, divergence points, and the reason/topic of each branch.

**Why:** exploring alternatives without context bloat; only useful if navigation and pruning are easy.

### 5.14 Stopping, summarising, resuming runs

- Stoppable units: tool/skill processes, tool-call loops, agent thoughts, LLM inference, batch jobs, subagents.
- Stop modes (configurable globally and per agent):
  - **Discard** — zero context retained from the stopped task; the conversation snaps back to the last user prompt.
  - **Summarise** — a separate aggregation produces "what was attempted / observed / where it broke" and that summary becomes the only retained artifact (no reaction or follow-up reasoning attached).
- A stopped task can be restarted from a saved step.

**Why:** safety; saved time/cost when an agent goes off-rails; better testing during agent development; supports both self-evolving agents (learn from failure) and standard agents (clean rollback).

### 5.15 Guiding agents during a run

- The user can inject a message/prompt **mid-run** that lands between tool-call iterations without interrupting the current step or creating a new user turn.
- Course-correction is a first-class operation, not a stop-and-restart.

### 5.16 Profiles and team sharing

- One main profile by default. All configs (agents, tools/skills, prompt library, memories) live there unless declared otherwise.
- Additional profiles can be created.
- **Anything** can be granted across profiles, in either direction, with explicit authorisation:
  - An agent in profile A can be made available in profile B.
  - Memory generated by an agent in profile A can be read by an agent in profile B (with auth).
  - Tools/skills in profile A can be exposed to profile B.
- Provenance of every shared element is preserved through the run trace.

**Why:** single-user navigation, multi-user setups, sharing, team work — all without accidental global exposure.

### 5.17 Conversation deletion

- Full conversations can be **fully** deleted, including metadata, generated memories, compacted context, and any side data.
- Selected message ranges can be deleted.
- Before deletion, the user can:
  - Keep selected generated files.
  - Trigger compaction first and keep only the compacted context.
  - Trigger memory generation first and keep only those memories.
  - Combine the above.
- Bulk deletion is supported (multi-select; "delete all conversations with agent X").

**Why:** disk usage of agentic systems blows up fast (this was a recurring Shinkai complaint); the user must own that footprint.

### 5.18 Prompt refinement (per agent)

- Per agent, an optional preprocessing LLM rewrites the user's prompt before the main run.
- Multiple refinement instructions per agent (different ones per topic/task).
- The actual agent sees only the refined prompt; the original is preserved in the trace per privacy policy.
- The agent can optionally be **made aware** of its refinement instructions so it can guide the user toward better prompting.

**Why:** better results for non-expert users of an agent built by someone else; faster usage when prompting carefully isn't worth the effort.

### 5.19 Prompt-injection guardrails

- Per agent, an optional guardrail LLM evaluates incoming external content (documents, web pages, retrieved material).
- Outcome: **allow / warn / block**. Reasoning is **not** auto-attached to the main agent's context.
- The guardrail is a separate model call from the main agent.

### 5.20 Human in the loop

- Per tool/skill, per file/folder access pattern: human approval may be required.
- Approval can be delegated to a configured controller agent (with declared authorisation scope).
- High-sensitivity actions can require a password and/or cryptographic signature before unlocking.

**Why:** human/AI control where wanted; cryptographic gating when "someone in front of the device" is not a sufficient identity check.

### 5.21 List / bulk / loop mode

- Any agent or LLM task can be run over a list (folder, file set, prompt list) with **deterministic** per-item execution and 100% accounting.
- The runtime owns the iteration, not the LLM. No "make sure you process all files" prompts.

### 5.22 Cost and time observability

- Per LLM call / loop: tokens in/out, cost, processing time.
- Per conversation segment: aggregate tokens, cost, time (full conversation, last N messages, selected range).
- User-defined per-LLM cost rates (in/out).
- Foundation for evals and optimisation suites built on top.

### 5.23 Quality assessment

- One-click / shortcut quality score (e.g. n/10) on:
  - A single answer
  - A loop / multi-step run
  - A selected conversation range
  - A full conversation
- Scores are queryable and bookmarkable; they feed evals and self-evolving agents.

### 5.24 Agent-created tools / skills / agents

- Agents/LLMs can create new tools/skills/subagents and discover them from registered sources (saved local catalogue or external repository).
- Activatable/deactivatable at global / profile / agent / conversation level.
- Creation can be **guided** (instructions provided when the feature is enabled).
- Created capabilities are usable within the same run.
- Created subagents can be used as tools by their creator.
- Created artifacts can be ephemeral or saved for later reuse.

### 5.25 Accessibility shortcuts

- Slash-command-style triggers (e.g. `/<something>`) for:
  - Force a specific tool/skill call (LLM-filled inputs).
  - Force a tool/skill call with **direct manual input** — the user's typed values go straight into the tool, no LLM in the loop (Shinkai does not support this; this CLI must).
  - Switch to a specific agent.
  - Run a saved prompt from a global library or a per-agent library.

**Why:** fast, 100%-success simple tool calls; fewer retypes for repeated prompts; powerful per-agent command sets.

### 5.26 Export / import everything

- One command exports/imports any of: full profile, full agent config, a single tool/skill, an LLM config.
- Format is portable across machines and profiles.
- Useful as a backup mechanism outside the runtime's managed folders.

### 5.27 Voice mode

- Voice input and voice output are both supported.
- TTS configurable globally and per agent (provider, voice, tone).
- Both local and cloud backends supported.

### 5.28 Mobile and messaging access

- At least one common consumer messaging platform (e.g. Telegram).
- At least one common work messaging platform (e.g. Slack or Teams).
- Agents callable from external systems and embeddable in third-party UIs.

**Why:** users do not always sit in front of a computer; they should not have to install yet another mobile app; builders need to reach users where they already are.

---

## 6. External-to-Core Capabilities

These are required for the product but may live as tools/skills on top of the core runtime, not in the core itself.

### 6.1 Code execution
Agents/LLMs can run code snippets — at minimum shell, Python, TypeScript — under guardrails. Most of this should be packaged as tools/skills with declared permissions, not as core runtime features.

### 6.2 Payments
Agents must be able to pay for services they consume (e.g. x402) and be paid for services they provide. Spend limits and wallet access are configurable. Likely implemented as tools/skills, not core. (Identity, discoverability, verifiability open out from this; out of scope for MVP.)

### 6.3 Document generation
Default tools for Word, PDF, Excel/CSV, PowerPoint. Likely implemented as tools/skills, not core.

### 6.4 Document viewing
The interface must render, or one-click-open in the OS default app, common formats (image, audio, PDF, CSV) inline in chat.

---

## 7. Compatibility With External Ecosystems

### 7.1 Principle

> **Compatible with external ecosystems, governed by the harness's own runtime model.**

External packages — Hermes plugins, OpenClaw / AgentSkills / ClawHub skills, MCP servers, A2A agents — are **inputs** to be normalised, not runtimes whose semantics the harness inherits. A Hermes plugin can provide a tool, an OpenClaw skill can provide procedural instructions, an MCP server can expose tools, a remote Hermes agent can be called as a subagent — but every one of them is mapped into the harness's own visibility, budget, interpretation, memory, sandbox, approval, cost-tracing, and provenance policies before it can run.

### 7.2 Native ABI stays authoritative

The harness defines its own:

- `ToolDescriptor` — executable capability + permissions + visibility + interpretation policy + provenance.
- `SkillDoc` — instructional context + provenance + token cost + visibility + trust level.
- `PluginManifest` — a package providing tools, skills, hooks, secrets, permission requests.
- `RunTrace` — append-only fact stream of a run.

External formats are mapped into these. The native ABI is **stricter** than any external one — that strictness is the point.

### 7.3 Adapter scope

| Source | What is reused | Difficulty | Main caveat |
| --- | --- | --- | --- |
| OpenClaw / AgentSkills `SKILL.md` folders | Imported as `SkillDoc` with provenance + trust | Low–medium | Treat as untrusted prompt material until reviewed |
| ClawHub registry | Search / inspect / install / pin / quarantine | Medium | Public registry → supply-chain risk; quarantine by default |
| OpenClaw plugins | Through sandboxed adapter only | Medium–high | Must not bypass sandbox / permissions / HITL |
| Hermes plugins (`plugin.yaml`) | `provides_tools` → `ToolDescriptor`; hooks → run-lifecycle hooks; bundled skills → `SkillDoc`; env → secret requests | Medium | Hermes runtime assumptions (memory, context, lifecycle) must not leak into the harness |
| Hermes toolsets (web, terminal, file, browser, vision, etc.) | Mapped to native categories/packs | Medium | Preserve harness budgets and visibility rules, not Hermes defaults |
| Hermes skills | Imported as `SkillDoc` with `origin=hermes`, `trust=unverified` | Low–medium | May assume Hermes workspace conventions |
| Hermes subagents / delegation | Treat as **external subagent tools**, not internal scheduler entries | Medium | Streaming, cancellation, trace mapping, cost attribution required |
| MCP tools | Mapped to native `ToolDescriptor`; execution wrapped in policy engine | Medium | Trust, auth, streaming, side-effects vary |
| A2A (later) | Remote agents callable as subagents/tools | Medium | A2A may not surface enough trace detail |
| OpenAI-compatible function tools | Lowest-common-denominator schema | Low | Too weak alone — needs harness wrapping for lifecycle, sandboxing, budgets |

### 7.4 Adapter pipeline

```
external package
   → detect       (which adapter recognises it?)
   → inspect      (what does it claim to provide?)
   → normalise    (map to ToolDescriptor / SkillDoc / hooks / secrets)
   → validate     (schema, references, dependencies)
   → static scan  (suspicious shell, curl, wallet/credential paths, obfuscation)
   → quarantine   (off by default)
   → user / agent / profile approval for scope
   → native registry
```

Adapters never bypass policy, sandboxing, approvals, or trace events.

### 7.5 Security posture for third-party imports

The OpenClaw ecosystem already has reported cases of malicious skills targeting wallet keys, SSH credentials, browser passwords, and similar. Compatibility cannot mean "run their packages the way they run them." The following are mandatory from day one:

- **Default quarantine** for third-party skills and plugins.
- **Digest pinning** for reproducible installs.
- **Permission manifest** declared up front and shown to the user before enable.
- **Capability sandbox** — tools cannot exceed declared permissions.
- **Human-approval gates** for shell, write, network, payment, wallet, browser-profile, credential access.
- **Static scan** for install commands, obfuscation, credential paths, wallet paths, suspicious network calls.
- **Runtime audit log** — every tool call records input/output/permissions/model/cost/duration.
- **Skill prompt-injection scan** (skills are prompt material and can manipulate agents).
- **No auto-update execution** — updates require inspection before activation.
- **Per-agent allowlists** — imported capabilities never become globally available by default.

These are the difference between "compatible" and "dangerous."

### 7.6 Implementation order

```
1. Native ABI                           (ToolDescriptor / SkillDoc / PluginManifest / RunTrace)
2. OpenClaw / AgentSkills importer      (text-only, lowest risk)
3. ClawHub source provider              (search / inspect / install / pin, quarantine by default)
4. MCP adapter                          (broadest tool ecosystem)
5. Hermes plugin importer               (tools, bundled skills, hooks)
6. Hermes external-agent adapter        (call as subagent, do not import internals)
7. A2A support                          (when the ecosystem stabilises)
```

---

## 8. Policy Precedence

```
global  →  profile  →  agent  →  conversation  →  run / manual override
                                                          ↓
                                                   effective policy
```

`agent explain-config` (and equivalent commands) must show, for every effective value, the layer it came from.

Policy domains: models, tools, skills, visibility, interpretation, memory, context, execution limits, approval, sandboxing, deletion.

---

## 9. MVP Boundary

The MVP proves the run engine, the context builder, the policy model, the trace model, the **memory boundary**, and the **document-ingestion boundary**.

That last point is deliberate. Memory and ingestion are not advanced features that can be added later — they touch the context builder, trace model, policy precedence, deletion, import/export, and security posture. If the MVP cannot demonstrate that these systems plug into the runtime as first-class, inspectable, policy-controlled subsystems, those subsystems will be retrofitted, not designed. The MVP therefore ships a constrained v0 of each.

### In MVP

#### Core runtime
- Model registry
- Tool registry
- Skill registry
- Agent config
- Policy precedence and `explain-config`
- Context preview using the same builder as execution
- Bounded run engine
  - single run
  - sequential tool calls
  - safe-parallel where declared side-effect-free
  - max-N budget
- Raw / interpreted tool output
- RunTrace and trace viewer
- Cost + time accounting
- Manual tool calls, including direct user-input mode
- Import / export
- OpenClaw / AgentSkills importer (text skills)
- MCP adapter

#### Memory v0 (constrained, architecturally complete)
- Memory generation and loading are separate; both off by default (see §3, §5.10).
- Manual memory create / edit / delete.
- Manual generation from a user-selected conversation range.
- Loading toggle at agent and run level.
- Human-readable storage format.
- Loaded memory visible in `preview-context`.
- Memory provenance recorded in the run trace.
- Memory record fields: id, content, owning profile, owning agent (optional), source conversation range (if generated), generating model (if generated), `created_at`, `updated_at`, human/model-authored flag.

#### Document ingestion v0 (constrained, architecturally complete)
- Pluggable ingestion-backend interface.
- Default local backend for plain text, markdown, code, and PDF text extraction.
- Ingestion produces inspectable artifacts viewable **before** they enter context.
- Ingestion provenance: source file, backend used, timestamp, extracted sections/chunks, content hash.
- Ingested content only enters context via explicit policy.
- Ingestion emits `RunEvent`s (or distinct `ImportEvent`s on the same trace).
- User can delete ingestion artifacts.
- User can re-run ingestion with a different backend on the same source.

### Deferred

#### Memory — advanced
- Async / cron memory generation
- Topic-based automatic memory generation
- Multiple memory backends
- Cross-profile / team memory sharing
- Memory embeddings + retrieval ranking
- Rollback beyond basic version history

#### Document ingestion — advanced
- Vision-based OCR / vision-heavy ingestion
- Complex layout, table, graph, chart extraction
- Per-modality / per-filetype routing
- Remote ingestion services
- Ingestion-backend marketplace

#### Other
- Profiles and team sharing
- Payments
- Voice
- Mobile / messaging bridges
- Agent-created tools/skills/agents
- Branching, deletion-with-side-effects, quality scoring
- Hermes plugin importer
- A2A
- Tauri / web UI (CLI is v1 interface)

### Why memory and ingestion belong in v0

Both touch enough cross-cutting systems that adding them later means redesigning core surface:

| System            | Memory affects it because…                                | Ingestion affects it because…                                       |
| ----------------- | --------------------------------------------------------- | ------------------------------------------------------------------- |
| Context builder   | Loaded memory must appear before execution                | Extracted content may enter context                                 |
| RunTrace          | Memory provenance must be recorded                        | The user must see what was extracted, from where, by which backend  |
| Policy precedence | Generation and loading must respect global → run layering | Ingestion must respect per-agent policy and approval gates          |
| Deletion          | Derived memories may depend on deleted conversations      | Chunks, OCR output, embeddings, caches grow quickly                 |
| Import / export   | Agents may ship with starting memory                      | Imported agents may reference imported documents                    |
| Security          | Memory can leak cross-agent/profile context if mis-scoped | External documents are a primary prompt-injection vector            |

The MVP question is not "can we run an agent with tools" — it is **"can we run an agent with inspectable context, bounded tools, document-derived context, optional memory, provenance, and traceable cost."** Defaults from §3 do not change: memory stays off, ingestion stays explicit/manual, third-party imports stay quarantined, dangerous actions stay approval-gated.

---

## 10. First Commands

```
agent run
agent preview-context
agent explain-config
agent explain-tools
agent tool call                 # direct call, manual input
agent trace show
agent export
agent import
agent skill import-openclaw <path>
agent skill install <source>
agent skill inspect <id>
agent skill quarantine <id>
agent skill allow <agent> <skill>
```

The CLI is the primary interface in v1. UIs, daemons, and integrations come later as clients of the same runtime API.

---

## 11. Glossary

- **Tool** — executable capability. Has permissions and a schema; produces raw output.
- **Skill** — instructional context. Enters context, does not execute.
- **Agent** — a configured run unit: prompt + model policy + tool policy + skill policy + memory policy + context policy + execution policy + approval policy.
- **Run** — one execution instance of an agent against a user input.
- **ContextSnapshot** — the exact LLM input for one call. The same builder produces both the preview and the request.
- **RunEvent** — one fact in the run trace (LLM call, tool call, approval, subagent, stop, cost).
- **Profile** — top-level scope for agents, tools, skills, memories, conversations.
- **Quarantine** — installed but not enabled; cannot run, cannot enter context.
- **Compaction** — produces a portable, shorter representation of a conversation that replaces full history in subsequent calls.
- **Branching** — diverging from a conversation point without duplicating shared history on disk.
