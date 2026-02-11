#!/usr/bin/env bun
import { spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dir, "../../..");
const CARGO_BIN = resolve(REPO_ROOT, "target/debug/chat_cli_v2");
const INK_DIR = resolve(REPO_ROOT, "packages/ink");

const skipRustBuild = process.argv.includes("--skip-rust-build");

function buildInk(): boolean {
  console.log("Building ink...");
  const result = spawnSync("bunx", ["tsc", "--project", "tsconfig.json"], {
    cwd: INK_DIR,
    stdio: "inherit"
  });
  return result.status === 0;
}

function startTUI() {
  console.log("Starting TUI...");

  // Start bun with watch mode and KIRO_AGENT_PATH set
  const bunProcess = spawn("bun", ["--watch", "./src/index.tsx"], {
    stdio: "inherit",
    cwd: resolve(import.meta.dir, ".."),
    env: {
      ...process.env,
      KIRO_AGENT_PATH: CARGO_BIN
    }
  });

  bunProcess.on("exit", (code) => {
    process.exit(code ?? 0);
  });
}

if (skipRustBuild) {
  console.log("Skipping Rust build...");
  if (!buildInk()) {
    console.error("Ink build failed");
    process.exit(1);
  }
  startTUI();
} else {
  console.log("Building chat_cli...");

  // Build the Rust binary
  const cargoBuild = spawn("cargo", ["build", "-p", "chat_cli_v2", "--bin", "chat_cli_v2"], {
    cwd: REPO_ROOT,
    stdio: "inherit"
  });

  cargoBuild.on("exit", (code) => {
    if (code !== 0) {
      console.error("Cargo build failed");
      process.exit(code ?? 1);
    }

    console.log("Generating TypeScript types...");

    // Generate types
    const typeGen = spawn("./scripts/generate-types.sh", [], {
      cwd: REPO_ROOT,
      stdio: "inherit"
    });

    typeGen.on("exit", (code) => {
      if (code !== 0) {
        console.error("Type generation failed");
        process.exit(code ?? 1);
      }

      if (!buildInk()) {
        console.error("Ink build failed");
        process.exit(1);
      }

      startTUI();
    });
  });
}
