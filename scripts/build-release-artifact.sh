#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node - "$@" <<'NODE'
const childProcess = require("child_process");
const fs = require("fs");

const args = process.argv.slice(2);

function fail(message) {
  console.error(`release artifact build failed: ${message}`);
  process.exit(1);
}

function usage() {
  console.log(`usage: scripts/build-release-artifact.sh [--platform=<id>] [--env-only] [--skip-env-check]

Builds the manifest-selected Tauri release artifact, then verifies strict
artifact outputs for that platform. Mobile targets also run strict native
packaging checks before the Tauri build starts.

Options:
  --platform=<id>     Build linux, macos, windows, android, or ios
  --env-only          Check signing/updater inputs without building
  --skip-env-check    Skip signing/updater environment validation
  -h, --help          Show this help text`);
}

for (const arg of args) {
  if (arg === "-h" || arg === "--help") {
    usage();
    process.exit(0);
  }
  if (arg === "--env-only" || arg === "--skip-env-check") continue;
  if (arg.startsWith("--platform=")) {
    if (!arg.slice("--platform=".length).trim()) fail("--platform needs a value");
    continue;
  }
  fail(`unknown argument ${arg}`);
}

const platformArg = args.find((arg) => arg.startsWith("--platform="))?.slice("--platform=".length);
const envOnly = args.includes("--env-only");
const skipEnvCheck = args.includes("--skip-env-check");

function assert(condition, message) {
  if (!condition) fail(message);
}

function hostPlatform() {
  if (process.platform === "darwin") return "macos";
  if (process.platform === "win32") return "windows";
  return "linux";
}

function missingEnv(names) {
  return names.filter((name) => !process.env[name] || !process.env[name].trim());
}

function run(command) {
  const result = childProcess.spawnSync(command, {
    cwd: process.cwd(),
    env: process.env,
    shell: true,
    stdio: "inherit",
  });
  if (result.error) fail(result.error.message);
  if (result.status !== 0) process.exit(result.status || 1);
}

const manifest = JSON.parse(fs.readFileSync("packaging/release-artifacts.json", "utf8"));
const platformId = platformArg || hostPlatform();
const platform = manifest.platforms.find((item) => item.id === platformId);
assert(platform, `unknown platform ${platformId}`);
assert(typeof platform.command === "string" && platform.command.trim(), `${platformId} command is missing`);

const requiredEnv = [
  ...(platform.signing?.env || []),
  ...(manifest.updater?.signing_env || []),
];
if (!skipEnvCheck) {
  const missing = missingEnv(requiredEnv);
  assert(
    missing.length === 0,
    `${platformId} signing/updater environment is incomplete: ${missing.join(", ")}`,
  );
}

console.log(`release artifact target: ${platformId}`);
console.log(`release artifact command: ${platform.command}`);
if (envOnly) {
  console.log(skipEnvCheck ? "release artifact build inputs verified" : "release artifact signing environment verified");
  process.exit(0);
}

if (platform.kind === "mobile") {
  run(`scripts/verify-mobile-packaging.sh --strict --platform=${platformId}`);
}
run(platform.command);
run(`scripts/verify-release-artifacts.sh --strict --platform=${platformId}`);
NODE
