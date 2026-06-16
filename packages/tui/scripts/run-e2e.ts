#!/usr/bin/env bun
import { spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dir, "../../..");
const TUI_ROOT = resolve(import.meta.dir, "..");

const skipRustBuild = process.argv.includes("--skip-rust-build");

function buildTui(): boolean {
  console.log("Building TUI...");
  const result = spawnSync("bun", ["run", "build"], { 
    cwd: TUI_ROOT, 
    stdio: "inherit",
    env: { ...process.env, NODE_ENV: "production" }
  });
  return result.status === 0;
}

function runTests() {
  if (!buildTui()) {
    console.error("TUI build failed");
    process.exit(1);
  }

  // Pass through extra args (e.g. specific test file, -t "test name").
  // When a file/dir argument is passed, run just that path; otherwise
  // run the whole e2e_tests directory.
  const extraArgs = process.argv.slice(2).filter(a => a !== "--skip-rust-build");
  const hasPathArg = extraArgs.some(a => !a.startsWith("-") && (a.includes("/") || a.endsWith(".ts")));

  console.log("Running E2E tests...");
  // --max-concurrency=1 forces serial execution: these tests allocate PTYs,
  // and parallel PTY allocation deadlocks on the pinned Bun version. Setting
  // it here (CLI flag) rather than bunfig.toml because Bun ignores
  // parallel/maxConcurrency as [test] config keys — they're CLI-only, so the
  // config form silently ran the suite at full fan-out and could OOM dev
  // machines. (See commit 13644ce4b.)
  const testArgs = hasPathArg
    ? ["test", "--max-concurrency=1", ...extraArgs]
    : ["test", "--max-concurrency=1", "./e2e_tests/", ...extraArgs];
  const test = spawn("bun", testArgs, { cwd: TUI_ROOT, stdio: "inherit" });
  test.on("exit", (code) => process.exit(code ?? 0));
}

if (skipRustBuild) {
  console.log("Skipping Rust build...");
  runTests();
} else {
  console.log("Building chat_cli...");
  const cargo = spawn("cargo", ["build", "-p", "chat_cli", "--bin", "chat_cli"], { cwd: REPO_ROOT, stdio: "inherit" });

  cargo.on("exit", (code) => {
    if (code !== 0) {
      console.error("Cargo build failed");
      process.exit(code ?? 1);
    }
    runTests();
  });
}
