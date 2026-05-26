#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node - "$@" <<'NODE'
const fs = require("fs");
const path = require("path");

const rawArgs = process.argv.slice(2);
const args = new Set(rawArgs);

function fail(message) {
  console.error(`release artifact check failed: ${message}`);
  process.exit(1);
}

function usage() {
  console.log(`usage: scripts/verify-release-artifacts.sh [--manifest-only] [--strict] [--platform=<id>]

Verifies release artifact metadata. With --strict, also checks that the selected
platform's artifact globs and updater artifact globs match non-empty outputs.`);
}

for (const arg of rawArgs) {
  if (arg === "-h" || arg === "--help") {
    usage();
    process.exit(0);
  }
  if (arg === "--manifest-only" || arg === "--strict") continue;
  if (arg.startsWith("--platform=")) {
    if (!arg.slice("--platform=".length).trim()) fail("--platform needs a value");
    continue;
  }
  fail(`unknown argument ${arg}`);
}

const strict = args.has("--strict");
const manifestOnly = args.has("--manifest-only") || !strict;
const platformArg = process.argv
  .slice(2)
  .find((arg) => arg.startsWith("--platform="))
  ?.slice("--platform=".length);

function assert(condition, message) {
  if (!condition) fail(message);
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function hostPlatform() {
  if (process.platform === "darwin") return "macos";
  if (process.platform === "win32") return "windows";
  return "linux";
}

function escapeRegExp(char) {
  return /[\\^$+?.()|[\]{}]/.test(char) ? `\\${char}` : char;
}

function globToRegExp(pattern) {
  let out = "^";
  for (let i = 0; i < pattern.length; i += 1) {
    const char = pattern[i];
    const next = pattern[i + 1];
    if (char === "*" && next === "*") {
      out += ".*";
      i += 1;
    } else if (char === "*") {
      out += "[^/]*";
    } else if (char === "?") {
      out += "[^/]";
    } else {
      out += escapeRegExp(char);
    }
  }
  out += "$";
  return new RegExp(out);
}

function staticPrefix(pattern) {
  const wildcard = pattern.search(/[*?]/);
  const prefix = wildcard === -1 ? pattern : pattern.slice(0, wildcard);
  const slash = prefix.lastIndexOf("/");
  return slash === -1 ? "." : prefix.slice(0, slash + 1) || ".";
}

function walk(dir, out = []) {
  if (!fs.existsSync(dir)) return out;
  const stat = fs.statSync(dir);
  out.push(dir);
  if (stat.isFile()) {
    return out;
  }
  for (const entry of fs.readdirSync(dir)) {
    walk(path.join(dir, entry), out);
  }
  return out;
}

function matchesGlob(pattern) {
  const normalized = pattern.replaceAll("\\", "/");
  const prefix = staticPrefix(normalized);
  const candidates = walk(prefix).map((candidate) => candidate.replaceAll("\\", "/"));
  const regex = globToRegExp(normalized);
  return candidates.filter((candidate) => regex.test(candidate));
}

function artifactHasPayload(match) {
  const stat = fs.statSync(match);
  if (stat.isFile()) return stat.size > 0;
  if (!stat.isDirectory()) return false;
  return walk(match).some((candidate) => {
    if (candidate === match) return false;
    const candidateStat = fs.statSync(candidate);
    return candidateStat.isFile() && candidateStat.size > 0;
  });
}

function assertArtifactMatches(pattern, label) {
  const matches = matchesGlob(pattern);
  assert(matches.length > 0, `${label} artifact glob matched no files: ${pattern}`);
  for (const match of matches) {
    assert(artifactHasPayload(match), `${label} artifact is empty or unsupported: ${match}`);
  }
}

function nonEmptyStrings(values, label) {
  assert(Array.isArray(values) && values.length > 0, `${label} must be a non-empty array`);
  for (const value of values) {
    assert(typeof value === "string" && value.trim(), `${label} contains an empty value`);
  }
}

const manifestPath = "packaging/release-artifacts.json";
assert(fs.existsSync(manifestPath), `${manifestPath} is missing`);
assert(fs.existsSync("scripts/build-release-artifact.sh"), "release artifact build script is missing");
assert(fs.existsSync(".github/workflows/release-packaging.yml"), "release packaging workflow is missing");
const workflow = fs.readFileSync(".github/workflows/release-packaging.yml", "utf8");
const manifest = readJson(manifestPath);
assert(manifest.schema_version === 1, "release artifact manifest schema_version must be 1");
assert(manifest.product === "Shinkai", "release artifact manifest product must be Shinkai");
assert(fs.existsSync(manifest.tauri_config), `Tauri config missing at ${manifest.tauri_config}`);
nonEmptyStrings(manifest.official_docs, "official_docs");
assert(
  manifest.official_docs.some((url) => url.startsWith("https://v2.tauri.app/")),
  "official_docs must include Tauri v2 documentation",
);

const platforms = manifest.platforms || [];
const expected = ["linux", "macos", "windows", "android", "ios"];
assert(platforms.length === expected.length, "release artifact manifest must cover five platforms");
for (const id of expected) {
  assert(platforms.some((platform) => platform.id === id), `release artifact manifest missing ${id}`);
}

for (const platform of platforms) {
  assert(["desktop", "mobile"].includes(platform.kind), `${platform.id} kind must be desktop or mobile`);
  assert(typeof platform.runner === "string" && platform.runner.trim(), `${platform.id} runner is missing`);
  assert(
    typeof platform.command === "string"
      && platform.command.startsWith("npm --prefix crates/agent-tauri/frontend run tauri -- "),
    `${platform.id} command must use the project-local Tauri CLI`,
  );
  assert(
    !platform.command.includes("--config ../tauri.conf.json"),
    `${platform.id} command must rely on the project-root Tauri wrapper instead of a frontend-relative config path`,
  );
  nonEmptyStrings(platform.artifact_globs, `${platform.id} artifact_globs`);
  assert(platform.signing?.required === true, `${platform.id} signing must be required`);
  nonEmptyStrings(platform.signing?.env, `${platform.id} signing env`);
  if (platform.id === "macos") {
    nonEmptyStrings(platform.signing?.notarization_env, "macos notarization env");
  }
}

for (const platform of platforms.filter((item) => item.kind === "desktop")) {
  assert(
    workflow.includes(`- platform: ${platform.id}`),
    `release workflow matrix missing ${platform.id}`,
  );
  assert(
    workflow.includes(`runner: ${platform.runner}`),
    `release workflow matrix runner for ${platform.id} must match manifest`,
  );
}
for (const platform of platforms.filter((item) => item.kind === "mobile")) {
  assert(
    !workflow.includes(`- platform: ${platform.id}`),
    `release workflow must not build mobile platform ${platform.id}`,
  );
}
for (const option of ["all", "linux", "macos", "windows"]) {
  assert(workflow.includes(`- ${option}`), `release workflow input options missing ${option}`);
}
assert(
  workflow.includes("scripts/build-release-artifact.sh --env-only --platform=${{ matrix.platform }}"),
  "release workflow must validate signing inputs through the release artifact script",
);
assert(
  workflow.includes("scripts/build-release-artifact.sh --platform=${{ matrix.platform }}"),
  "release workflow must build through the release artifact script",
);
assert(
  workflow.includes("if-no-files-found: error"),
  "release workflow artifact upload must fail when bundles are missing",
);
assert(
  workflow.includes("target/release/bundle/**"),
  "release workflow must upload generated Tauri bundles",
);

assert(manifest.updater?.required === true, "updater signed artifact generation must be required");
nonEmptyStrings(manifest.updater?.signing_env, "updater signing_env");
nonEmptyStrings(manifest.updater?.artifact_globs, "updater artifact_globs");

if (!manifestOnly) {
  const selectedId = platformArg || hostPlatform();
  const selected = platforms.find((platform) => platform.id === selectedId);
  assert(selected, `unknown platform ${selectedId}`);
  for (const pattern of selected.artifact_globs) {
    assertArtifactMatches(pattern, selectedId);
  }
  for (const pattern of manifest.updater.artifact_globs) {
    assertArtifactMatches(pattern, "updater");
  }
}

console.log(
  manifestOnly ? "release artifact manifest verified" : "release artifacts verified",
);
NODE
