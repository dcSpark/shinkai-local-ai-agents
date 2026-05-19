#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node <<'NODE'
const fs = require("fs");
const path = require("path");

function assert(condition, message) {
  if (!condition) {
    console.error(`release binary check failed: ${message}`);
    process.exit(1);
  }
}

const suffix = process.platform === "win32" ? ".exe" : "";
for (const name of ["agent", "agent-daemon", "shinkai"]) {
  const binary = path.join("target", "release", `${name}${suffix}`);
  assert(fs.existsSync(binary), `missing ${binary}`);
  const stat = fs.statSync(binary);
  assert(stat.isFile(), `${binary} is not a file`);
  assert(stat.size > 0, `${binary} is empty`);
  if (process.platform !== "win32") {
    assert((stat.mode & 0o111) !== 0, `${binary} is not executable`);
  }
}

console.log("release binaries verified");
NODE
