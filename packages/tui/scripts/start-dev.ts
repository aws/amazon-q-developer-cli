#!/usr/bin/env bun
import { spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dir, "../../..");
const CARGO_BIN = resolve(REPO_ROOT, "target/debug/chat_cli");
const INK_DIR = resolve(REPO_ROOT, "packages/ink");

// Separate dev-script flags from flags to forward to the TUI
const devFlags = new Set(["--skip-rust-build"]);
const skipRustBuild = process.argv.includes("--skip-rust-build");

// Everything after "dev" that isn't a dev-script flag gets forwarded to the TUI
const tuiArgs = process.argv.slice(2).filter((arg) => !devFlags.has(arg));

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
  // Forward any extra CLI args (e.g. --agent <name>) to the TUI process
  const bunProcess = spawn("bun", ["--watch", "./src/index.tsx", ...tuiArgs], {
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
  const cargoBuild = spawn("cargo", ["build", "-p", "chat_cli", "--bin", "chat_cli"], {
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
