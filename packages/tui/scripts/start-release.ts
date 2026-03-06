#!/usr/bin/env bun
import { spawn } from "node:child_process";
import { resolve } from "node:path";

const REPO_ROOT = resolve(import.meta.dir, "../../..");
const TUI_ROOT = resolve(import.meta.dir, "..");
const CARGO_BIN = resolve(REPO_ROOT, "target/release/chat_cli");
const TUI_BUNDLE = resolve(TUI_ROOT, "dist/tui.js");

const devFlags = new Set(["--skip-rust-build"]);
const skipRustBuild = process.argv.includes("--skip-rust-build");
const tuiArgs = process.argv.slice(2).filter((arg) => !devFlags.has(arg));

function buildTUI() {
  console.log("Building TUI...");

  const build = spawn("bun", ["run", "build"], {
    cwd: TUI_ROOT,
    stdio: "inherit",
    env: {
      ...process.env,
      NODE_ENV: "production"
    }
  });

  build.on("exit", (code) => {
    if (code !== 0) {
      console.error("TUI build failed");
      process.exit(code ?? 1);
    }

    console.log("Starting TUI in production mode...");

    const bunProcess = spawn("bun", [TUI_BUNDLE, ...tuiArgs], {
      stdio: "inherit",
      cwd: TUI_ROOT,
      env: {
        ...process.env,
        KIRO_AGENT_PATH: CARGO_BIN,
        NODE_ENV: "production"
      }
    });

    bunProcess.on("exit", (code) => {
      process.exit(code ?? 0);
    });
  });
}

if (skipRustBuild) {
  console.log("Skipping Rust build...");
  buildTUI();
} else {
  console.log("Building chat_cli...");

  const cargoBuild = spawn("cargo", ["build", "--release", "-p", "chat_cli", "--bin", "chat_cli"], {
    cwd: REPO_ROOT,
    stdio: "inherit"
  });

  cargoBuild.on("exit", (code) => {
    if (code !== 0) {
      console.error("Cargo build failed");
      process.exit(code ?? 1);
    }

    buildTUI();
  });
}
