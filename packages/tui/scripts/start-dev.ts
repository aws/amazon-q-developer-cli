#!/usr/bin/env bun
import { spawn, spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { readFileSync, existsSync } from "node:fs";
import { resolveChatCliBin } from "../src/utils/chat-cli-bin";

const REPO_ROOT = resolve(import.meta.dir, "../../..");
const CARGO_BIN = resolveChatCliBin();
const TWINKI_DIR = resolve(REPO_ROOT, "packages/twinki/packages/twinki");

// Resolve the pinned bun binary (matches the version shipped in the release binary)
function getPinnedBun(): string {
  // Read pinned version from const.py
  const constPy = resolve(REPO_ROOT, "scripts/const.py");
  const versionMatch = readFileSync(constPy, "utf8").match(/^BUN_VERSION\s*=\s*"(.+)"/m);
  const pinnedVersion = versionMatch?.[1] ?? "unknown";
  console.log(`Pinned bun version: ${pinnedVersion}`);

  // Check if already cached
  const cachedBin = resolve(REPO_ROOT, `.bun-pinned/${pinnedVersion}/bun`);
  const isCached = existsSync(cachedBin);
  console.log(isCached ? `Using cached bun at ${cachedBin}` : `Downloading bun v${pinnedVersion}...`);

  const result = spawnSync("bash", [resolve(REPO_ROOT, "scripts/ensure-pinned-bun.sh")], {
    stdio: ["inherit", "pipe", "inherit"],
  });
  if (result.status !== 0) {
    console.error("Failed to resolve pinned bun. Falling back to system bun.");
    return "bun";
  }
  return result.stdout.toString().trim();
}

const PINNED_BUN = getPinnedBun();

// Separate dev-script flags from flags to forward to the TUI
const devFlags = new Set(["--skip-rust-build"]);
const skipRustBuild = process.argv.includes("--skip-rust-build");

// Optional: --rust-bin-path <path> overrides the default chat_cli binary
// and implicitly skips the Rust build (the path must already exist).
let rustBinOverride: string | null = null;
{
  const idx = process.argv.indexOf("--rust-bin-path");
  if (idx !== -1) {
    const value = process.argv[idx + 1];
    if (!value || value.startsWith("--")) {
      console.error("--rust-bin-path requires a path argument");
      process.exit(1);
    }
    rustBinOverride = resolve(process.cwd(), value);
    if (!existsSync(rustBinOverride)) {
      console.error(`--rust-bin-path: file not found: ${rustBinOverride}`);
      process.exit(1);
    }
  }
}

const RUST_BIN = rustBinOverride ?? CARGO_BIN;

// Everything after "dev" that isn't a dev-script flag gets forwarded to the TUI
const tuiArgs = process.argv.slice(2).filter((arg, i, arr) => {
  if (devFlags.has(arg)) return false;
  if (arg === "--rust-bin-path") return false;
  if (i > 0 && arr[i - 1] === "--rust-bin-path") return false;
  return true;
});

function buildTwinki(): boolean {
  console.log("Building twinki...");
  const result = spawnSync("bunx", ["tsc", "--project", "tsconfig.build.json"], {
    cwd: TWINKI_DIR,
    stdio: "inherit"
  });
  return result.status === 0;
}

function startTUI() {
  if (!existsSync(RUST_BIN)) {
    console.error(`\nError: Rust binary not found at ${RUST_BIN}`);
    console.error(`Run one of:`);
    console.error(`  cargo build -p chat_cli --bin chat_cli`);
    console.error(`  bun run dev  (without --skip-rust-build)`);
    console.error(`  bun run dev --rust-bin-path <path>\n`);
    process.exit(1);
  }

  console.log("Starting TUI...");

  // Forward any extra CLI args (e.g. --agent <name>) to the TUI process.
  // Use absolute path to entry file so the caller's cwd is preserved.
  const entryFile = resolve(import.meta.dir, "../src/index.tsx");
  const bunProcess = spawn(PINNED_BUN, ["--watch", entryFile, ...tuiArgs], {
    stdio: "inherit",
    env: {
      ...process.env,
      // KAS in `--auth=acp-callback` mode shells out to this binary for
      // `chat _ get-kas-token` (host-mediated OIDC refresh). Also used by
      // V2's ACP child spawn and /chat save/load. Must be set regardless
      // of the active engine.
      KIRO_CHAT_CLI_BIN: RUST_BIN,
      JSC_numberOfGCMarkers: "1",
    }
  });

  bunProcess.on("exit", (code) => {
    process.exit(code ?? 0);
  });
}

// Verify CodeArtifact auth is valid and refresh if expired
{
  const npmrc = resolve(REPO_ROOT, ".npmrc");
  let needsLogin = !existsSync(npmrc) || !readFileSync(npmrc, "utf8").includes("@kiro:registry");
  if (!needsLogin) {
    const tokenMatch = readFileSync(npmrc, "utf8").match(/:_authToken=(.+)/);
    if (tokenMatch) {
      try {
        const header = JSON.parse(Buffer.from(tokenMatch[1]!.split('.')[0]!, 'base64url').toString());
        if (header.exp && header.exp < Date.now() / 1000) needsLogin = true;
      } catch { needsLogin = true; }
    } else {
      needsLogin = true;
    }
  }
  if (needsLogin) {
    console.log("CodeArtifact token missing or expired, refreshing...");
    const login = spawnSync("bash", [resolve(REPO_ROOT, "scripts/codeartifact-login.sh")], {
      cwd: REPO_ROOT,
      stdio: "inherit",
    });
    if (login.status !== 0) {
      console.error("CodeArtifact login failed. Run manually: ./scripts/codeartifact-login.sh");
      process.exit(1);
    }
  }
}

// Ensure dependencies are installed (<50ms when no deps changed)
spawnSync("bun", ["install"], { cwd: REPO_ROOT, stdio: "inherit" });

if (skipRustBuild || rustBinOverride) {
  console.log(rustBinOverride ? `Using Rust binary at ${RUST_BIN}` : "Skipping Rust build...");
  if (!buildTwinki()) {
    console.error("Twinki build failed");
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

      if (!buildTwinki()) {
        console.error("Twinki build failed");
        process.exit(1);
      }

      startTUI();
    });
  });
}
