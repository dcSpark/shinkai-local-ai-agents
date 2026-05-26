#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node - "$@" <<'NODE'
const fs = require("fs");
const { spawnSync } = require("child_process");

const rawArgs = process.argv.slice(2);
const args = new Set(rawArgs);

function fail(message) {
  console.error(`mobile packaging check failed: ${message}`);
  process.exit(1);
}

function usage() {
  console.log(`usage: scripts/verify-mobile-packaging.sh [--strict] [--platform=android|ios]

Verifies native mobile packaging metadata. With --strict, also checks generated
Tauri mobile projects, platform SDK tools, and installed Rust mobile targets.`);
}

for (const arg of rawArgs) {
  if (arg === "-h" || arg === "--help") {
    usage();
    process.exit(0);
  }
  if (arg === "--strict") continue;
  if (arg.startsWith("--platform=")) {
    if (!arg.slice("--platform=".length).trim()) fail("--platform needs android or ios");
    continue;
  }
  fail(`unknown argument ${arg}`);
}

const strict = args.has("--strict");
const platformArg = process.argv
  .slice(2)
  .find((arg) => arg.startsWith("--platform="))
  ?.slice("--platform=".length);
const checkAndroid = !platformArg || platformArg === "android";
const checkIos = !platformArg || platformArg === "ios";

assert(
  !platformArg || ["android", "ios"].includes(platformArg),
  "--platform must be android or ios",
);

function assert(condition, message) {
  if (!condition) fail(message);
}

function assertIncludes(haystack, needle, message) {
  assert(haystack.includes(needle), message);
}

function assertBefore(haystack, first, second, message) {
  const firstIndex = haystack.indexOf(first);
  const secondIndex = haystack.indexOf(second);
  assert(firstIndex !== -1, `${message}: missing ${first}`);
  assert(secondIndex !== -1, `${message}: missing ${second}`);
  assert(firstIndex < secondIndex, message);
}

function assertWorkflowSecret(workflow, name) {
  assertIncludes(
    workflow,
    `${name}: ` + "${{ secrets." + name + " }}",
    `mobile workflow secret wiring is missing ${name}`,
  );
}

const strictFailures = [];
function requireStrict(condition, message) {
  if (!condition) strictFailures.push(message);
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function commandExists(command, args = ["--version"]) {
  const result = spawnSync(command, args, { stdio: "ignore" });
  return result.status === 0;
}

function directoryHasFiles(dir) {
  if (!fs.existsSync(dir)) return false;
  const stack = [dir];
  while (stack.length > 0) {
    const current = stack.pop();
    const stat = fs.statSync(current);
    if (stat.isFile()) return true;
    if (stat.isDirectory()) {
      for (const entry of fs.readdirSync(current)) {
        stack.push(`${current}/${entry}`);
      }
    }
  }
  return false;
}

function installedRustTargets() {
  const result = spawnSync("rustup", ["target", "list", "--installed"], {
    encoding: "utf8",
  });
  if (result.status !== 0) return new Set();
  return new Set(result.stdout.split(/\r?\n/).filter(Boolean));
}

function requireGeneratedProject(dir, label) {
  const exists = fs.existsSync(dir);
  requireStrict(exists, `generated ${label} Tauri project is missing`);
  if (exists) {
    requireStrict(directoryHasFiles(dir), `generated ${label} Tauri project is empty`);
  }
}

const manifest = readJson("packaging/release-artifacts.json");
const docs = new Set(manifest.official_docs || []);
for (const doc of [
  "https://v2.tauri.app/start/prerequisites/",
  "https://v2.tauri.app/distribute/google-play/",
  "https://v2.tauri.app/distribute/sign/ios/",
]) {
  assert(docs.has(doc), `release artifact manifest must reference ${doc}`);
}

const frontend = readJson("crates/agent-tauri/frontend/package.json");
assert(
  frontend.scripts?.tauri === "node ../../../scripts/tauri-local.mjs",
  "frontend Tauri script must use the project-root wrapper",
);
assert(fs.existsSync("scripts/tauri-local.mjs"), "project-root Tauri wrapper is missing");
assert(fs.existsSync(".github/workflows/mobile-packaging.yml"), "mobile packaging workflow is missing");
const workflow = fs.readFileSync(".github/workflows/mobile-packaging.yml", "utf8");
assert(workflow.includes("Build signed Android mobile artifacts"), "mobile workflow must expose an Android build step");
assert(workflow.includes("Build signed iOS mobile artifacts"), "mobile workflow must expose an iOS build step");
assert(workflow.includes("if: inputs.build"), "mobile workflow must gate signed builds behind the build input");
assertIncludes(
  workflow,
  "scripts/init-mobile-packaging.sh --platform=android",
  "mobile workflow must initialize and verify Android through the shared helper",
);
assertIncludes(
  workflow,
  "scripts/init-mobile-packaging.sh --platform=ios",
  "mobile workflow must initialize and verify iOS through the shared helper",
);
assertIncludes(
  workflow,
  "scripts/build-release-artifact.sh --platform=android",
  "mobile workflow must build Android through the release artifact script",
);
assertIncludes(
  workflow,
  "scripts/build-release-artifact.sh --platform=ios",
  "mobile workflow must build iOS through the release artifact script",
);
assertIncludes(
  workflow,
  "scripts/prepare-mobile-signing.sh --platform=android",
  "mobile workflow must prepare Android signing before signed builds",
);
assertIncludes(
  workflow,
  "scripts/prepare-mobile-signing.sh --platform=ios",
  "mobile workflow must prepare iOS signing before signed builds",
);
assertBefore(
  workflow,
  "scripts/init-mobile-packaging.sh --platform=android",
  "scripts/prepare-mobile-signing.sh --platform=android",
  "mobile workflow must initialize Android before preparing signing",
);
assertBefore(
  workflow,
  "scripts/prepare-mobile-signing.sh --platform=android",
  "scripts/build-release-artifact.sh --platform=android",
  "mobile workflow must prepare Android signing before building",
);
assertBefore(
  workflow,
  "scripts/init-mobile-packaging.sh --platform=ios",
  "scripts/prepare-mobile-signing.sh --platform=ios",
  "mobile workflow must initialize iOS before preparing signing",
);
assertBefore(
  workflow,
  "scripts/prepare-mobile-signing.sh --platform=ios",
  "scripts/build-release-artifact.sh --platform=ios",
  "mobile workflow must prepare iOS signing before building",
);
assert(fs.existsSync("scripts/init-mobile-packaging.sh"), "mobile packaging init helper is missing");
const initScript = fs.readFileSync("scripts/init-mobile-packaging.sh", "utf8");
assert(initScript.includes('run tauri -- "$target" init'), "mobile init helper must call Tauri mobile init");
assert(initScript.includes("preflight_platform"), "mobile init helper must preflight platform prerequisites");
assert(initScript.includes("--preflight-only"), "mobile init helper must expose a preflight-only mode");
assert(initScript.includes("--ci --skip-targets-install"), "mobile init helper must run Tauri init non-interactively after target preflight");
assert(initScript.includes("init_platform android"), "mobile init helper must initialize Android");
assert(initScript.includes("init_platform ios"), "mobile init helper must initialize iOS");
assert(
  initScript.includes("scripts/verify-mobile-packaging.sh --strict"),
  "mobile init helper must run strict verification after initialization",
);
assert(fs.existsSync("scripts/prepare-mobile-signing.sh"), "mobile signing prep helper is missing");
const signingScript = fs.readFileSync("scripts/prepare-mobile-signing.sh", "utf8");
assert(signingScript.includes("keystore.properties"), "mobile signing prep must write Android keystore.properties");
assert(signingScript.includes("rootProject.file(\"keystore.properties\")"), "mobile signing prep must patch Android Gradle signing config");
assert(signingScript.includes("buildTypesMatch"), "mobile signing prep must locate Android buildTypes robustly");
assert(signingScript.includes("APPLE_API_KEY_PATH"), "mobile signing prep must expose the iOS App Store Connect key path");
assert(signingScript.includes("APPLE_DEVELOPMENT_TEAM"), "mobile signing prep must expose the iOS development team");

const tauriConfig = readJson("crates/agent-tauri/tauri.conf.json");
assert(tauriConfig.identifier === "io.shinkai.agent-app", "Tauri identifier must be stable for mobile packages");
assert(tauriConfig.bundle?.active === true, "Tauri bundling must be active");
assert(Array.isArray(tauriConfig.bundle?.icon) && tauriConfig.bundle.icon.length > 0, "Tauri mobile bundles need icons");

const platforms = new Map((manifest.platforms || []).map((platform) => [platform.id, platform]));
const android = platforms.get("android");
const ios = platforms.get("ios");
assert(android?.kind === "mobile", "release artifact manifest must declare Android as mobile");
assert(ios?.kind === "mobile", "release artifact manifest must declare iOS as mobile");
assert(
  android.command === "npm --prefix crates/agent-tauri/frontend run tauri -- android build --apk --aab",
  "Android release command must build both APK and AAB through the project-local Tauri wrapper",
);
assert(
  ios.command === "npm --prefix crates/agent-tauri/frontend run tauri -- ios build --export-method app-store-connect",
  "iOS release command must build an App Store export through the project-local Tauri wrapper",
);
const androidSigningEnv = ["ANDROID_KEYSTORE_BASE64", "ANDROID_KEYSTORE_PASSWORD", "ANDROID_KEY_ALIAS", "ANDROID_KEY_PASSWORD"];
const iosSigningEnv = [
  "APPLE_API_KEY",
  "APPLE_API_ISSUER",
  "APPLE_API_KEY_BASE64",
  "APPLE_API_KEY_PATH",
  "APPLE_TEAM_ID",
  "APPLE_DEVELOPMENT_TEAM",
];
for (const env of androidSigningEnv) {
  assert(android.signing?.env?.includes(env), `Android signing env is missing ${env}`);
  assertWorkflowSecret(workflow, env);
}
for (const env of iosSigningEnv) {
  assert(ios.signing?.env?.includes(env), `iOS signing env is missing ${env}`);
}
for (const env of iosSigningEnv.filter((name) => !["APPLE_API_KEY_PATH", "APPLE_DEVELOPMENT_TEAM"].includes(name))) {
  assertWorkflowSecret(workflow, env);
}
for (const glob of [
  "crates/agent-tauri/gen/android/app/build/outputs/apk/**/*.apk",
  "crates/agent-tauri/gen/android/app/build/outputs/bundle/**/*.aab",
]) {
  assert(android.artifact_globs?.includes(glob), `Android artifact glob is missing ${glob}`);
}
for (const glob of [
  "crates/agent-tauri/gen/apple/build/**/*.ipa",
  "crates/agent-tauri/gen/apple/build/**/*.xcarchive",
]) {
  assert(ios.artifact_globs?.includes(glob), `iOS artifact glob is missing ${glob}`);
}

if (strict) {
  if (checkAndroid) {
    const androidProject = "crates/agent-tauri/gen/android";
    requireGeneratedProject(androidProject, "Android");
    requireStrict(process.env.ANDROID_HOME && fs.existsSync(process.env.ANDROID_HOME), "ANDROID_HOME must point at the Android SDK");
    requireStrict(process.env.NDK_HOME && fs.existsSync(process.env.NDK_HOME), "NDK_HOME must point at the Android NDK");
    requireStrict(commandExists("java"), "Java must be installed for Android packaging");
  }
  if (checkIos) {
    const iosProject = "crates/agent-tauri/gen/apple";
    requireGeneratedProject(iosProject, "iOS");
    requireStrict(commandExists("pod"), "CocoaPods must be installed for iOS packaging");
    requireStrict(commandExists("xcodebuild", ["-version"]), "Xcode must be installed for iOS packaging");
  }

  const targets = installedRustTargets();
  const androidTargets = [
    "aarch64-linux-android",
    "armv7-linux-androideabi",
    "i686-linux-android",
    "x86_64-linux-android",
  ];
  const iosTargets = [
    "aarch64-apple-ios",
    "aarch64-apple-ios-sim",
    "x86_64-apple-ios",
  ];
  for (const target of checkAndroid ? androidTargets : []) {
    requireStrict(targets.has(target), `Rust mobile target is missing: ${target}`);
  }
  for (const target of checkIos ? iosTargets : []) {
    requireStrict(targets.has(target), `Rust mobile target is missing: ${target}`);
  }

  if (strictFailures.length > 0) {
    fail(`strict requirements missing:\n- ${strictFailures.join("\n- ")}`);
  }
}

console.log(strict ? "native mobile packaging verified" : "native mobile packaging metadata verified");
NODE
