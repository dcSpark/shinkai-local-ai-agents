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
unless the Vite server is reachable or `frontend/dist` is fresh and complete.

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
cargo run -p agent-tauri --bin shinkai
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

Daemon execution endpoints can also be paid-gated independently. Setting
`AGENT_DAEMON_X402_ACCEPTS` protects `/run`, `/run/start`, `/resume`,
`/resume/start`, `/batch`, `/batch/resume`, and `/tool/...`; health and
inspection endpoints stay open.

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

Release binary smoke build:

```bash
cargo build -p agent-cli -p agent-daemon -p agent-tauri --release --bins
scripts/verify-release-binaries.sh
```
