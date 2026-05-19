#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

metadata_file="$(mktemp)"
trap 'rm -f "$metadata_file"' EXIT

cargo metadata --no-deps --format-version=1 >"$metadata_file"
METADATA_FILE="$metadata_file" node <<'NODE'
const fs = require("fs");

function assert(condition, message) {
  if (!condition) {
    console.error(`packaging check failed: ${message}`);
    process.exit(1);
  }
}

const metadata = JSON.parse(fs.readFileSync(process.env.METADATA_FILE, "utf8"));
const packages = new Map(metadata.packages.map((pkg) => [pkg.name, pkg]));

for (const name of ["agent-cli", "agent-daemon", "agent-tauri"]) {
  assert(packages.has(name), `workspace package ${name} is missing`);
}

const cli = packages.get("agent-cli");
assert(
  cli.targets.some((target) => target.name === "agent" && target.kind.includes("bin")),
  "agent-cli must expose the agent binary",
);

const daemon = packages.get("agent-daemon");
assert(
  daemon.targets.some((target) => target.name === "agent-daemon" && target.kind.includes("bin")),
  "agent-daemon must expose the agent-daemon binary",
);

const tauri = packages.get("agent-tauri");
assert(
  tauri.targets.some((target) => target.name === "shinkai" && target.kind.includes("bin")),
  "agent-tauri must expose the shinkai binary",
);
assert(
  tauri.targets.some((target) => target.name === "agent_tauri_lib" && target.kind.includes("cdylib")),
  "agent-tauri must expose the Tauri cdylib",
);

const tauriConfig = JSON.parse(fs.readFileSync("crates/agent-tauri/tauri.conf.json", "utf8"));
assert(tauriConfig.productName === "Shinkai", "Tauri productName must be Shinkai");
assert(tauriConfig.identifier === "io.shinkai.agent-app", "Tauri identifier must be stable");
assert(
  tauriConfig.build?.beforeBuildCommand === "npm --prefix frontend run build",
  "Tauri build must invoke the frontend production build",
);
assert(
  tauriConfig.build?.frontendDist === "frontend/dist",
  "Tauri frontendDist must point at frontend/dist",
);
assert(tauriConfig.bundle?.active === true, "Tauri bundling must be active for release packaging");
assert(tauriConfig.bundle?.targets === "all", "Tauri bundle targets must cover all platforms");
assert(
  Array.isArray(tauriConfig.bundle?.icon) && tauriConfig.bundle.icon.includes("icons/icon.png"),
  "Tauri bundle must include the app icon",
);

const frontend = JSON.parse(fs.readFileSync("crates/agent-tauri/frontend/package.json", "utf8"));
assert(frontend.scripts?.build === "tsc -b && vite build", "frontend build script must type-check and bundle");
assert(frontend.scripts?.["type-check"] === "tsc -b --noEmit", "frontend type-check script is missing");
assert(
  frontend.scripts?.tauri === "node ../../../scripts/tauri-local.mjs",
  "frontend must expose the project-root Tauri CLI wrapper",
);
assert(
  frontend.scripts?.["tauri:version"] === "node ../../../scripts/tauri-local.mjs --version",
  "frontend must expose a project-root Tauri CLI version check",
);
assert(frontend.devDependencies?.["@tauri-apps/cli"] === "^2.11.2", "frontend must pin the Tauri CLI dev dependency");
assert(frontend.devDependencies?.vite === "^6.4.2", "frontend must use the patched Vite 6 line");

const tauriWrapper = fs.readFileSync("scripts/tauri-local.mjs", "utf8");
assert(tauriWrapper.includes("crates/agent-tauri"), "Tauri wrapper must run from the Rust Tauri project root");
assert(tauriWrapper.includes('"--prefix", "frontend"'), "Tauri wrapper must use the frontend-local CLI package");

const index = fs.readFileSync("crates/agent-tauri/frontend/index.html", "utf8");
assert(index.includes('rel="manifest"'), "frontend must link the mobile web manifest");
assert(index.includes('mobile-web-app-capable'), "frontend must declare mobile web app capability");
assert(index.includes('rel="apple-touch-icon"'), "frontend must expose an install icon for mobile browsers");

const manifest = JSON.parse(fs.readFileSync("crates/agent-tauri/frontend/public/manifest.webmanifest", "utf8"));
assert(manifest.name === "Shinkai Agents", "mobile web manifest name must be stable");
assert(manifest.display === "standalone", "mobile web manifest must be installable as standalone");
assert(manifest.start_url === "/", "mobile web manifest must start at the app root");
assert(Array.isArray(manifest.icons) && manifest.icons.length > 0, "mobile web manifest must include icons");
const manifestIconSources = new Set(manifest.icons.map((icon) => icon.src));
assert(manifestIconSources.has("/icon-192.png"), "mobile web manifest must include a 192px PNG icon");
assert(manifestIconSources.has("/icon-512.png"), "mobile web manifest must include a 512px PNG icon");

const mainTsx = fs.readFileSync("crates/agent-tauri/frontend/src/main.tsx", "utf8");
assert(mainTsx.includes("serviceWorker.register"), "frontend must register the mobile web service worker");
assert(fs.existsSync("crates/agent-tauri/frontend/public/sw.js"), "mobile web service worker is missing");

const releaseArtifacts = JSON.parse(fs.readFileSync("packaging/release-artifacts.json", "utf8"));
assert(releaseArtifacts.schema_version === 1, "release artifact manifest schema must be version 1");
const releasePlatforms = new Set(releaseArtifacts.platforms?.map((platform) => platform.id));
for (const platform of ["linux", "macos", "windows", "android", "ios"]) {
  assert(releasePlatforms.has(platform), `release artifact manifest must include ${platform}`);
}
for (const platform of releaseArtifacts.platforms) {
  assert(
    platform.command.startsWith("npm --prefix crates/agent-tauri/frontend run tauri -- "),
    `${platform.id} release command must use the project-local Tauri CLI`,
  );
  assert(
    !platform.command.includes("--config ../tauri.conf.json"),
    `${platform.id} release command must rely on the project-root Tauri wrapper instead of a frontend-relative config path`,
  );
}
assert(
  releaseArtifacts.updater?.signing_env?.includes("TAURI_SIGNING_PRIVATE_KEY"),
  "release artifact manifest must declare Tauri updater signing",
);

const ci = fs.readFileSync(".github/workflows/ci.yml", "utf8");
for (const os of ["ubuntu-latest", "macos-14", "windows-2022"]) {
  assert(ci.includes(os), `CI matrix must include ${os}`);
}
assert(ci.includes("npm run build"), "CI must run the frontend production build");
assert(ci.includes("npm run tauri:version"), "CI must verify the project-local Tauri CLI");
assert(ci.includes("scripts/verify-packaging.sh"), "CI must run the packaging verifier");
assert(ci.includes("scripts/verify-release-artifacts.sh"), "CI must run the release artifact manifest verifier");
assert(ci.includes("scripts/verify-mobile-packaging.sh"), "CI must run the native mobile packaging verifier");
assert(ci.includes("cargo build -p agent-cli -p agent-daemon -p agent-tauri --release --bins"), "CI must build release binaries");
assert(ci.includes("scripts/verify-release-binaries.sh"), "CI must verify release binary artifacts");

console.log("packaging metadata verified");
NODE
