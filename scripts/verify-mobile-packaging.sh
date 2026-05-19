#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

node - "$@" <<'NODE'
const fs = require("fs");
const { spawnSync } = require("child_process");

const args = new Set(process.argv.slice(2));
const strict = args.has("--strict");

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

function installedRustTargets() {
  const result = spawnSync("rustup", ["target", "list", "--installed"], {
    encoding: "utf8",
  });
  if (result.status !== 0) return new Set();
  return new Set(result.stdout.split(/\r?\n/).filter(Boolean));
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
  requireStrict(fs.existsSync("crates/agent-tauri/gen/android"), "generated Android Tauri project is missing");
  requireStrict(fs.existsSync("crates/agent-tauri/gen/apple"), "generated iOS Tauri project is missing");
  requireStrict(process.env.ANDROID_HOME && fs.existsSync(process.env.ANDROID_HOME), "ANDROID_HOME must point at the Android SDK");
  requireStrict(process.env.NDK_HOME && fs.existsSync(process.env.NDK_HOME), "NDK_HOME must point at the Android NDK");
  requireStrict(commandExists("java"), "Java must be installed for Android packaging");
  requireStrict(commandExists("pod"), "CocoaPods must be installed for iOS packaging");
  requireStrict(commandExists("xcodebuild", ["-version"]), "Xcode must be installed for iOS packaging");

  const targets = installedRustTargets();
  for (const target of [
    "aarch64-linux-android",
    "armv7-linux-androideabi",
    "i686-linux-android",
    "x86_64-linux-android",
    "aarch64-apple-ios",
    "aarch64-apple-ios-sim",
    "x86_64-apple-ios",
  ]) {
    requireStrict(targets.has(target), `Rust mobile target is missing: ${target}`);
  }

  if (strictFailures.length > 0) {
    fail(`strict requirements missing:\n- ${strictFailures.join("\n- ")}`);
  }
}

console.log(strict ? "native mobile packaging verified" : "native mobile packaging metadata verified");
NODE
