#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node - "$@" <<'NODE'
const fs = require("fs");
const path = require("path");

const args = new Set(process.argv.slice(2));
const strict = args.has("--strict");
const manifestOnly = args.has("--manifest-only") || !strict;
const platformArg = process.argv
  .slice(2)
  .find((arg) => arg.startsWith("--platform="))
  ?.slice("--platform=".length);

function fail(message) {
  console.error(`release artifact check failed: ${message}`);
  process.exit(1);
}

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
  if (stat.isFile()) {
    out.push(dir);
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

function nonEmptyStrings(values, label) {
  assert(Array.isArray(values) && values.length > 0, `${label} must be a non-empty array`);
  for (const value of values) {
    assert(typeof value === "string" && value.trim(), `${label} contains an empty value`);
  }
}

const manifestPath = "packaging/release-artifacts.json";
assert(fs.existsSync(manifestPath), `${manifestPath} is missing`);
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

assert(manifest.updater?.required === true, "updater signed artifact generation must be required");
nonEmptyStrings(manifest.updater?.signing_env, "updater signing_env");
nonEmptyStrings(manifest.updater?.artifact_globs, "updater artifact_globs");

if (!manifestOnly) {
  const selectedId = platformArg || hostPlatform();
  const selected = platforms.find((platform) => platform.id === selectedId);
  assert(selected, `unknown platform ${selectedId}`);
  for (const pattern of selected.artifact_globs) {
    const matches = matchesGlob(pattern);
    assert(matches.length > 0, `${selectedId} artifact glob matched no files: ${pattern}`);
  }
  for (const pattern of manifest.updater.artifact_globs) {
    const matches = matchesGlob(pattern);
    assert(matches.length > 0, `updater artifact glob matched no files: ${pattern}`);
  }
}

console.log(
  manifestOnly ? "release artifact manifest verified" : "release artifacts verified",
);
NODE
