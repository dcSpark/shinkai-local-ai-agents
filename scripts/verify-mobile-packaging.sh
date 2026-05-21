#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node - "$@" <<'NODE'
const fs = require("fs");
const { spawnSync } = require("child_process");

const args = new Set(process.argv.slice(2));
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

function fail(message) {
  console.error(`mobile packaging check failed: ${message}`);
  process.exit(1);
}

function assert(condition, message) {
  if (!condition) fail(message);
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
assert(
  workflow.includes("scripts/build-release-artifact.sh --platform=android"),
  "mobile workflow must build Android through the release artifact script",
);
assert(
  workflow.includes("scripts/build-release-artifact.sh --platform=ios"),
  "mobile workflow must build iOS through the release artifact script",
);

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
for (const env of ["ANDROID_KEYSTORE_BASE64", "ANDROID_KEYSTORE_PASSWORD", "ANDROID_KEY_ALIAS", "ANDROID_KEY_PASSWORD"]) {
  assert(android.signing?.env?.includes(env), `Android signing env is missing ${env}`);
}
for (const env of ["APPLE_API_KEY", "APPLE_API_ISSUER", "APPLE_TEAM_ID", "IOS_PROVISIONING_PROFILE"]) {
  assert(ios.signing?.env?.includes(env), `iOS signing env is missing ${env}`);
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
