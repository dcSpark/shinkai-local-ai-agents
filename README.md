# Shinkai AI Agent Harness

Local-first AI agent harness with a shared runtime, headless CLI, HTTP daemon,
TUI, and Tauri desktop app.

The implementation target is tracked in:

- `specs/architecture.md`
- `specs/general_requirements.md`
- `specs/gils_feedback.md`
- `specs/implementation_audit.md`

## Prerequisites

- Rust from `rust-toolchain.toml`
- Node.js and npm for the Tauri frontend
- Tauri v2 system prerequisites for your OS

Install frontend dependencies once:

```bash
npm ci --prefix crates/agent-tauri/frontend
```

For isolated local state while testing, set:

```bash
export AGENT_HARNESS_HOME="$PWD/.agent-harness-dev"
export AGENT_HARNESS_PROFILE=main
```

## Run The Desktop App

Use the project-local Tauri wrapper for normal development:

```bash
npm --prefix crates/agent-tauri/frontend run tauri -- dev
```

This starts Vite and launches the Tauri shell. Running the Rust binary directly
does not start Vite, so debug `cargo run --bin shinkai` now exits with guidance
unless the Vite server is reachable.

Manual two-terminal flow:

```bash
npm --prefix crates/agent-tauri/frontend run dev
```

```bash
cargo run -p agent-tauri --bin shinkai
```

Production-style local smoke test:

```bash
npm --prefix crates/agent-tauri/frontend run build
cargo run --release -p agent-tauri --bin shinkai
```

## Run The CLI

Launch the TUI:

```bash
cargo run -p agent-cli -- run
```

Run headless with the fake provider:

```bash
cargo run -p agent-cli -- run --input "Say hello from the harness" --print --provider fake --demo echo
```

Emit JSON trace events:

```bash
cargo run -p agent-cli -- run --input "Trace this" --json --provider fake --demo echo
```

## External Memory Backends

The optional `external-command-v0` and `external-http-v0` memory backends let a
saved agent or run configuration load memory records from an external adapter.
For a local adapter process, set:

```bash
export AGENT_MEMORY_EXTERNAL_COMMAND=/path/to/memory-adapter
export AGENT_MEMORY_EXTERNAL_ARGS_JSON='["--profile", "main"]'
export AGENT_MEMORY_EXTERNAL_TIMEOUT_MS=30000
export AGENT_MEMORY_EXTERNAL_ENABLE_WRITES=true
export AGENT_MEMORY_EXTERNAL_ENABLE_ROLLBACK=true
```

The args and timeout variables are optional; args must be a JSON array when set,
and the timeout defaults to 30 seconds. The command adapter receives a JSON request on
stdin and prints either an array of records or an object with a `records` array.
Each record must include `content`; `id`, `topics`, `target`, `author`,
`owning_profile`, `owning_agent`, and `generating_model` are optional. Writes
and memory generation are disabled unless `AGENT_MEMORY_EXTERNAL_ENABLE_WRITES`
is set to `true`; rollback is disabled unless
`AGENT_MEMORY_EXTERNAL_ENABLE_ROLLBACK` is set to `true`.

For an HTTP(S) memory service, set:

```bash
export AGENT_MEMORY_EXTERNAL_HTTP_URL=http://127.0.0.1:8787/memory
export AGENT_MEMORY_EXTERNAL_HTTP_BEARER_TOKEN=optional-token
export AGENT_MEMORY_EXTERNAL_HTTP_TIMEOUT_MS=5000
export AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_WRITES=true
export AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_ROLLBACK=true
```

The HTTP adapter receives the same JSON request as the command adapter by POST
and returns the same record JSON shape. The bearer token and timeout are
optional; the timeout defaults to 5 seconds. HTTP writes and generation are
disabled unless `AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_WRITES` is set to `true`;
rollback is disabled unless `AGENT_MEMORY_EXTERNAL_HTTP_ENABLE_ROLLBACK` is set
to `true`.

Adapters receive `operation: "load_records"` for reads, `operation:
"write_record"` for explicit memory creation, `operation: "generate_records"`
for memory generation, `operation: "edit_record"` for edits, `operation:
"delete_record"` for deletes, and `operation: "rollback"` for target rollback.
Write and edit responses may return a single `record`, generation responses
should return `records`, delete responses may return an empty body or
`{ "deleted": true }`, and rollback responses may return an empty body or
`{ "rolled_back": true }`. Conversation deletion cleanup uses `load_records` to
find records with matching `source_conversation_id` values and sends
`delete_record` for each linked external record.

Profile bundles do not export these adapter settings or bearer tokens, but the
bundle manifest records credential reminders for the adapter-related environment
variables when a saved config uses an external memory backend. Bundle exports
also scan adapter manifests for redacted connector `secret_requirements` and add
per-manifest reminders for values that must be recreated after import.

Probe backend readiness without running an agent:

```bash
cargo run -p agent-cli -- memory probe external-command-v0 --topic launch --json
cargo run -p agent-cli -- memory probe external-http-v0 --topic launch --json
```

`agent memory export <path> [--user] [--agent <id>]` reads from the active
memory backend and can export only records owned by one agent. `agent memory
import <path> [--user] [--agent <id>]` writes through the active backend, so
external imports require the matching write-enable environment variable. The
app exposes the same portability flow through `/memory export ...` and
`/memory import ...` shortcuts.
When memory loading is enabled, profile-granted memory uses the same shared
backend scan for CLI, daemon, and Tauri runs and records source profile/backend
provenance in the loaded fragments.

## Generated Artifacts

The native `artifact_generate` tool writes scoped artifacts into the harness
artifact cache. It supports text/markdown, CSV/JSON/HTML, SVG image artifacts,
PDF, DOCX, XLSX, and PPTX. The app can preview text, audio, SVG, HTML, and PDF
artifacts inline, open cached artifacts in the OS default app, download artifacts
to the current device with `/artifacts download <id>`, and export cached
artifacts with the Artifacts panel or `/artifacts export <id> <path>`. CLI users
can copy cached files out of the managed artifact cache with `agent artifact
export <id> <path>` or `agent artifact download <id> [path]`; TUI/headless slash
users can use `/artifacts download <id> [path]`. Daemon-hosted artifacts can be
retrieved with `agent remote artifact export <id> <path>` or `agent remote
artifact download <id> [path]`, and daemon/web clients can fetch raw bytes from
`GET /artifacts/<id>/download`.

## Run The Daemon

Start the HTTP daemon:

```bash
cargo run -p agent-daemon --bin agent-daemon
```

The default address is `127.0.0.1:7878`. Override it with:

```bash
AGENT_DAEMON_ADDR=127.0.0.1:7879 cargo run -p agent-daemon --bin agent-daemon
```

Use the CLI against the daemon:

```bash
cargo run -p agent-cli -- remote health
cargo run -p agent-cli -- remote run --input "Hello through the daemon" --provider fake
```

Use a non-default daemon URL:

```bash
cargo run -p agent-cli -- remote --url http://127.0.0.1:7879 health
```

Paid bridge endpoints can be protected with x402 by setting per-platform
requirements, for example:

```bash
export AGENT_SLACK_X402_ACCEPTS='[{"scheme":"exact","network":"base-sepolia","maxAmountRequired":"5","payTo":"0x...","asset":"0x...","resource":"http://localhost:7878/bridges/slack/slash"}]'
export AGENT_SLACK_X402_FACILITATOR_URL=http://127.0.0.1:8787
```

The same `X402_*` suffixes work for `TELEGRAM`, `TEAMS`, `WHATSAPP`, and
`WEBHOOK`, with `AGENT_BRIDGE_X402_ACCEPTS` and `AGENT_X402_FACILITATOR_URL`
available as shared fallbacks.

Embed a lightweight web chat widget in a third-party page by loading the daemon
bridge script:

```html
<script
  async
  src="http://localhost:7878/bridges/embed.js"
  data-agent-bridge-token="optional-webhook-secret"
  data-title="Ask Shinkai"
></script>
```

The widget posts to `/bridges/webhook` with `source: "web-embed"` metadata and
the optional `x-agent-bridge-token` header. Use `data-endpoint` to point the
widget at another compatible webhook gateway.

Slack bridge replies use the slash-command `response_url` when provided. If the
inbound request has no response URL, set `AGENT_SLACK_BOT_TOKEN` to deliver via
Slack Web API instead; `AGENT_SLACK_ACCESS_TOKEN`, `SLACK_BOT_TOKEN`, and
`SLACK_ACCESS_TOKEN` are accepted aliases. Ephemeral responses use
`chat.postEphemeral` with the inbound `channel_id` and `user_id`; `in_channel`
responses use `chat.postMessage`. Override the Slack API base URL for tests or
private gateways with `AGENT_SLACK_API_BASE_URL`.

Teams bridge replies can be delivered through the Bot Framework reply endpoint
when the inbound activity includes `serviceUrl` and `conversation.id`. Set
`AGENT_TEAMS_BOT_TOKEN` for the bearer token; `AGENT_TEAMS_ACCESS_TOKEN`,
`TEAMS_BOT_TOKEN`, and `TEAMS_ACCESS_TOKEN` are accepted aliases. Explicit
`response_url` values still take precedence for custom gateways and tests.

WhatsApp bridge replies can be delivered through WhatsApp Cloud by setting
`AGENT_WHATSAPP_ACCESS_TOKEN`; override the Graph API base URL for tests or
private gateways with `AGENT_WHATSAPP_API_BASE_URL`. Configure
`AGENT_WHATSAPP_VERIFY_TOKEN` for the WhatsApp Cloud webhook verification
challenge on `GET /bridges/whatsapp/webhook`.

Daemon execution endpoints can also be paid-gated independently. Setting
`AGENT_DAEMON_X402_ACCEPTS` protects `/run`, `/run/start`, `/resume`,
`/resume/start`, `/batch`, `/batch/resume`, and `/tool/...`; health and
inspection endpoints stay open. Set `AGENT_DAEMON_X402_PATHS` to a comma- or
newline-separated list of exact paths or `/prefix/*` patterns to override that
default protected route set.

For outbound x402 payment retries, enable the native payment tools with
`AGENT_PAYMENT_TOOLS=1` and either pass a per-call signature, use a
`payment_signature_secret`/`AGENT_X402_SIGNATURE_SECRET`, or configure
`AGENT_X402_WALLET_COMMAND`. The wallet command receives JSON on stdin with
`payment_required`, `url`, `method`, and `max_amount`, then prints either the raw
`PAYMENT-SIGNATURE` value or JSON containing `payment_signature`.
`AGENT_X402_WALLET_ARGS_JSON` and `AGENT_X402_WALLET_TIMEOUT_MS` customize the
wallet process.

## Verification

Core test suite:

```bash
cargo test --workspace --no-fail-fast
```

Frontend type-check and production build:

```bash
npm --prefix crates/agent-tauri/frontend run type-check
npm --prefix crates/agent-tauri/frontend run build
```

Packaging metadata checks:

```bash
scripts/verify-packaging.sh
scripts/verify-release-artifacts.sh --manifest-only
scripts/verify-mobile-packaging.sh
```

Strict native mobile packaging checks require generated Tauri mobile projects
and platform SDKs. After installing the Android or iOS prerequisites, generate
and verify each platform independently:

```bash
scripts/init-mobile-packaging.sh --platform=android
```

```bash
scripts/init-mobile-packaging.sh --platform=ios
```

The init helper preflights platform SDK tools and Rust mobile targets before it
generates Tauri mobile projects, so install any reported prerequisite before
rerunning it.

Signed mobile builds also run `scripts/prepare-mobile-signing.sh` in CI after
initialization. Android uses the `ANDROID_KEYSTORE_BASE64`,
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, and `ANDROID_KEY_PASSWORD`
secrets. iOS uses `APPLE_API_KEY`, `APPLE_API_ISSUER`,
`APPLE_API_KEY_BASE64`, and `APPLE_TEAM_ID`; the helper writes the
`APPLE_API_KEY_PATH` and `APPLE_DEVELOPMENT_TEAM` environment values expected by
Tauri.

Release binary smoke build:

```bash
cargo build -p agent-cli -p agent-daemon -p agent-tauri --release --bins
scripts/verify-release-binaries.sh
```
