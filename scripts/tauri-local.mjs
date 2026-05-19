#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDir, "..");
const tauriDir = resolve(root, "crates/agent-tauri");
const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm";

const result = spawnSync(
  npmCommand,
  ["--prefix", "frontend", "exec", "tauri", "--", ...process.argv.slice(2)],
  {
    cwd: tauriDir,
    stdio: "inherit",
  },
);

if (result.error) {
  console.error(`failed to run project-local Tauri CLI: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
