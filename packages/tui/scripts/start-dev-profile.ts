#!/usr/bin/env bun
import { spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { mkdirSync } from "node:fs";
import { resolveChatCliBin } from "../src/utils/chat-cli-bin";

const REPO_ROOT = resolve(import.meta.dir, "../../..");
const CARGO_BIN = resolveChatCliBin();
const PROFILES_DIR = resolve(import.meta.dir, "../profiles");
const ENTRY_FILE = resolve(import.meta.dir, "../src/index.tsx");

// Resolve the pinned bun binary
const result = spawnSync("bash", [resolve(REPO_ROOT, "scripts/ensure-pinned-bun.sh")], {
  stdio: ["inherit", "pipe", "inherit"],
});
if (result.status !== 0) {
  console.error("Failed to resolve pinned bun.");
  process.exit(1);
}
const PINNED_BUN = result.stdout.toString().trim();
console.log(`Using pinned bun: ${PINNED_BUN}`);

mkdirSync(PROFILES_DIR, { recursive: true });

const child = spawn(PINNED_BUN, [
  "--cpu-prof",
  `--cpu-prof-dir=${PROFILES_DIR}`,
  ENTRY_FILE,
], {
  stdio: "inherit",
  env: {
    ...process.env,
    KIRO_AGENT_PATH: CARGO_BIN,
    KIRO_INPUT_METRICS: "true",
    KIRO_PERF_METRICS: "true",
    KIRO_TUI_LOG_FILE: "/tmp/kiro-tui-perf.log",
    JSC_numberOfGCMarkers: "1",
  },
});

child.on("exit", (code) => process.exit(code ?? 0));
