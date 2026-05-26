#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

platform=""

for arg in "$@"; do
  case "$arg" in
    --platform=android|--platform=ios)
      platform="${arg#--platform=}"
      ;;
    -h|--help)
      cat <<'EOF'
usage: scripts/prepare-mobile-signing.sh --platform=android|ios

Prepares generated Tauri mobile project signing inputs from CI secrets.
Run after scripts/init-mobile-packaging.sh and before build-release-artifact.sh.
EOF
      exit 0
      ;;
    *)
      echo "mobile signing prep failed: unknown argument $arg" >&2
      exit 1
      ;;
  esac
done

if [[ -z "$platform" ]]; then
  echo "mobile signing prep failed: --platform is required" >&2
  exit 1
fi

case "$platform" in
  android)
    node <<'NODE'
const fs = require("fs");
const path = require("path");

function fail(message) {
  console.error(`mobile signing prep failed: ${message}`);
  process.exit(1);
}

function requireEnv(name) {
  const value = process.env[name];
  if (!value || !value.trim()) fail(`${name} is required`);
  return value;
}

const androidDir = "crates/agent-tauri/gen/android";
const appGradle = path.join(androidDir, "app/build.gradle.kts");
if (!fs.existsSync(androidDir)) fail("generated Android Tauri project is missing");
if (!fs.existsSync(appGradle)) fail("generated Android app/build.gradle.kts is missing");

const keystoreBase64 = requireEnv("ANDROID_KEYSTORE_BASE64");
const keystorePassword = requireEnv("ANDROID_KEYSTORE_PASSWORD");
const keyAlias = requireEnv("ANDROID_KEY_ALIAS");
const keyPassword = requireEnv("ANDROID_KEY_PASSWORD");
const runnerTemp = process.env.RUNNER_TEMP || path.join(process.cwd(), "target/mobile-signing");
fs.mkdirSync(runnerTemp, { recursive: true });

const keystorePath = path.join(runnerTemp, "shinkai-upload-keystore.jks");
fs.writeFileSync(keystorePath, Buffer.from(keystoreBase64, "base64"), { mode: 0o600 });

const properties = [
  `keyAlias=${keyAlias}`,
  `password=${keyPassword}`,
  `storePassword=${keystorePassword}`,
  `storeFile=${keystorePath.replaceAll("\\", "/")}`,
  "",
].join("\n");
fs.writeFileSync(path.join(androidDir, "keystore.properties"), properties, { mode: 0o600 });

let gradle = fs.readFileSync(appGradle, "utf8");
if (!gradle.includes("import java.io.FileInputStream")) {
  gradle = `import java.io.FileInputStream\n${gradle}`;
}
if (!gradle.includes("import java.util.Properties")) {
  gradle = `import java.util.Properties\n${gradle}`;
}
if (!gradle.includes('rootProject.file("keystore.properties")')) {
  const signingConfig = [
    "    signingConfigs {",
    '        create("release") {',
    '            val keystorePropertiesFile = rootProject.file("keystore.properties")',
    "            val keystoreProperties = Properties()",
    "            if (keystorePropertiesFile.exists()) {",
    "                keystoreProperties.load(FileInputStream(keystorePropertiesFile))",
    "            }",
    '            keyAlias = keystoreProperties["keyAlias"] as String',
    '            keyPassword = keystoreProperties["password"] as String',
    '            storeFile = file(keystoreProperties["storeFile"] as String)',
    '            storePassword = (keystoreProperties["storePassword"] ?: keystoreProperties["password"]) as String',
    "        }",
    "    }",
    "",
  ].join("\n");
  const buildTypesMatch = gradle.match(/^[ \t]*buildTypes\s*\{/m);
  if (!buildTypesMatch || buildTypesMatch.index === undefined) {
    fail("generated Android Gradle file has no buildTypes block");
  }
  gradle = `${gradle.slice(0, buildTypesMatch.index)}${signingConfig}${gradle.slice(buildTypesMatch.index)}`;
}

if (!gradle.includes('signingConfig = signingConfigs.getByName("release")')) {
  const releasePattern = /getByName\("release"\)\s*\{|^[ \t]*release\s*\{/m;
  const match = gradle.match(releasePattern);
  if (!match || match.index === undefined) {
    fail("generated Android Gradle file has no release build type");
  }
  const insertAt = match.index + match[0].length;
  gradle = `${gradle.slice(0, insertAt)}\n            signingConfig = signingConfigs.getByName("release")${gradle.slice(insertAt)}`;
}

fs.writeFileSync(appGradle, gradle);
console.log("Android signing inputs prepared");
NODE
    ;;
  ios)
    node <<'NODE'
const fs = require("fs");
const path = require("path");

function fail(message) {
  console.error(`mobile signing prep failed: ${message}`);
  process.exit(1);
}

function requireEnv(name) {
  const value = process.env[name];
  if (!value || !value.trim()) fail(`${name} is required`);
  return value;
}

if (!fs.existsSync("crates/agent-tauri/gen/apple")) {
  fail("generated iOS Tauri project is missing");
}

const apiKeyId = requireEnv("APPLE_API_KEY");
const apiIssuer = requireEnv("APPLE_API_ISSUER");
const apiKeyBase64 = requireEnv("APPLE_API_KEY_BASE64");
const teamId = requireEnv("APPLE_TEAM_ID");
const runnerTemp = process.env.RUNNER_TEMP || path.join(process.cwd(), "target/mobile-signing");
fs.mkdirSync(runnerTemp, { recursive: true });

const apiKeyPath = path.join(runnerTemp, `AuthKey_${apiKeyId}.p8`);
fs.writeFileSync(apiKeyPath, Buffer.from(apiKeyBase64, "base64"), { mode: 0o600 });

const exports = [
  `APPLE_API_KEY_PATH=${apiKeyPath}`,
  `APPLE_API_ISSUER=${apiIssuer}`,
  `APPLE_DEVELOPMENT_TEAM=${teamId}`,
];
if (process.env.GITHUB_ENV) {
  fs.appendFileSync(process.env.GITHUB_ENV, `${exports.join("\n")}\n`);
}
console.log(`iOS signing inputs prepared at ${apiKeyPath}`);
NODE
    ;;
esac
