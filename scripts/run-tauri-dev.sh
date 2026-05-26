#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAURI_DIR="$ROOT/crates/agent-tauri"
FRONTEND_DIR="$ROOT/crates/agent-tauri/frontend"

cd "$ROOT"

if [[ ! -d "$FRONTEND_DIR/node_modules" ]]; then
  npm ci --prefix "$FRONTEND_DIR"
fi

cd "$TAURI_DIR"
DEV_CONFIG='{"build":{"beforeDevCommand":{"script":"npm run dev","cwd":"frontend"}}}'
"$FRONTEND_DIR/node_modules/.bin/tauri" dev --config "$DEV_CONFIG" "$@"
