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

  console.log("Running E2E tests...");
  const test = spawn("bun", ["test", "./e2e_tests/"], { cwd: TUI_ROOT, stdio: "inherit" });
  test.on("exit", (code) => process.exit(code ?? 0));
}

if (skipRustBuild) {
  console.log("Skipping Rust build...");
  runTests();
} else {
  console.log("Building chat_cli...");
  const cargo = spawn("cargo", ["build", "--bin", "chat_cli_v2"], { cwd: REPO_ROOT, stdio: "inherit" });

  cargo.on("exit", (code) => {
    if (code !== 0) {
      console.error("Cargo build failed");
      process.exit(code ?? 1);
    }
    runTests();
  });
}
