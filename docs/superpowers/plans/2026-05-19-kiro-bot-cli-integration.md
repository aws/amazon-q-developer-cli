# kiro-bot Integration into kiro-cli

**Status:** ✅ Implemented
**Goal:** Ship `kiro-bot` functionality inside the `kiro-cli` binary behind a Cargo feature flag, so users run `kiro-cli bot <subcommand>` and ECS containers can auto-refresh the binary weekly.

---

## Design Decisions

### Why a runtime env var (not Cargo feature)?

1. **Zero build process changes** — same binary everywhere, no separate "headless" variant
2. **Simpler CI** — one artifact per platform, not two
3. **Instant rollout** — flip an env var in ECS task definition, no rebuild needed
4. **Easier testing** — any developer can test bot functionality without a special build
5. **No manifest changes** — no need for a `headless` variant in the update manifest

The tradeoff is ~5-10 MB extra binary size and additional compile-time deps for all users, but this is acceptable for operational simplicity.

### Why embed in kiro-cli (not keep separate binary)?

1. **Single binary deployment** — ECS pulls one artifact, gets both `kiro-cli acp` (worker) and bot orchestrator.
2. **Shared update mechanism** — Reuse the existing `kiro-cli update` infrastructure instead of building a parallel one.
3. **Simpler PATH** — No need to ensure two binaries are co-located and version-matched.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ kiro-cli binary (KIRO_ENABLE_BOT=1 to activate)             │
│                                                             │
│  kiro-cli bot install <path>                                │
│  kiro-cli bot start <name> [--foreground]                   │
│  kiro-cli bot stop <name>                                   │
│  kiro-cli bot status                                        │
│  kiro-cli bot run <name>        (cron one-shot)             │
│  kiro-cli bot chat <name>       (interactive debug)         │
│  kiro-cli bot update --if-stale 7d                          │
│                                                             │
│  Internally spawns: kiro-cli chat acp  (worker processes)   │
└─────────────────────────────────────────────────────────────┘
```

On ECS:
```
┌─────────────────────────────────────────────────────────────┐
│ ECS Fargate container                                       │
│                                                             │
│  env: KIRO_ENABLE_BOT=1                                     │
│                                                             │
│  entrypoint.sh:                                             │
│    1. kiro-cli bot update --if-stale 7d                     │
│    2. exec kiro-cli bot start kiro-help --foreground        │
│                                                             │
│  Weekly refresh: container restart pulls latest binary       │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation Plan

### Phase 1: Feature flag + subcommand wiring ✅

**Implemented in:**
- `crates/kiro-bot/src/lib.rs` — Added `pub mod cli;` to expose handlers
- `crates/kiro-bot/src/cli/{install,run,service}.rs` — Changed `use kiro_bot::` → `use crate::`
- `crates/kiro-bot/src/main.rs` — Changed `mod cli;` → `use kiro_bot::cli;`
- `crates/chat-cli/Cargo.toml` — Added `kiro-bot` as a required (non-optional) dependency
- `crates/chat-cli/src/cli/bot.rs` — New module with `BotArgs`/`BotCommand`/`BotUpdateArgs`
- `crates/chat-cli/src/cli/mod.rs` — Wired `Bot` variant with `#[command(hide = true)]` + runtime `KIRO_ENABLE_BOT` env var gate

**Approach:** Runtime env var gate instead of Cargo feature. The subcommand is always compiled in but:
- Hidden from `--help` via `#[command(hide = true)]`
- Gated at execution time: returns an error if `KIRO_ENABLE_BOT` is not set

**File: `crates/chat-cli/Cargo.toml`**
```toml
[features]
default = []
wayland = ["arboard/wayland-data-control"]
bot = ["dep:kiro-bot"]

[dependencies]
kiro-bot = { path = "../kiro-bot", optional = true }
```

**File: `crates/chat-cli/src/cli/mod.rs`** — Add variant to `RootSubcommand`:
```rust
#[cfg(feature = "bot")]
#[command(name = "bot", about = "Manage ACP-backed bot instances")]
Bot(crate::cli::bot::BotArgs),
```

**File: `crates/chat-cli/src/cli/bot.rs`** (new, feature-gated module):
```rust
//! Bot subcommand — delegates to kiro_bot library.
//! Only compiled when the `bot` feature is enabled.

use std::process::ExitCode;
use clap::{Args, Subcommand};
use eyre::Result;
use crate::os::Os;

#[derive(Debug, Args)]
pub struct BotArgs {
    #[command(subcommand)]
    pub command: BotCommand,
}

#[derive(Debug, Subcommand)]
pub enum BotCommand {
    /// Install a bot from a config directory
    Install { path: String },
    /// Uninstall a bot instance
    Uninstall {
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Start a bot instance
    Start {
        name: Option<String>,
        #[arg(long)]
        foreground: bool,
        #[arg(long)]
        all: bool,
    },
    /// Stop a running bot instance
    Stop {
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Show status of all bot instances
    Status,
    /// Run a one-shot cron execution
    Run { name: String },
    /// Interactive chat with a bot (debug)
    Chat { name: String },
    /// Check for and install binary updates
    Update {
        /// Only update if last update was older than this duration (e.g. "7d", "24h")
        #[arg(long)]
        if_stale: Option<String>,
        /// Force update even if current version matches
        #[arg(long)]
        force: bool,
    },
}

impl BotArgs {
    pub async fn execute(self, os: &mut Os) -> Result<ExitCode> {
        // Delegate to kiro_bot's existing CLI handlers
        // (reuse the same functions the standalone binary calls)
        todo!("wire to kiro_bot library functions")
    }
}
```

**Conditional compilation in dispatch** — Add to both OS match blocks in `RootSubcommand::execute`:
```rust
#[cfg(feature = "bot")]
Self::Bot(args) => args.execute(os).await?,
```

And the module declaration at the top of `cli/mod.rs`:
```rust
#[cfg(feature = "bot")]
pub mod bot;
```

### Phase 2: Expose kiro-bot internals for embedding

The kiro-bot binary's `src/cli/` module contains the handler functions (`cmd_install`, `cmd_run`, `cmd_stop`, etc.) but they're private to the binary. Two options:

**Option A (recommended): Re-export handlers from the library.**

Move the CLI handler logic from `kiro-bot/src/cli/` into the library as `kiro_bot::commands`:
```rust
// kiro_bot/src/lib.rs
pub mod commands;  // install, run, service, etc.
pub mod config;
pub mod engine;
pub mod frontend;
```

The standalone `kiro-bot` binary becomes a thin wrapper, and `chat_cli::cli::bot` calls the same functions.

**Option B: Keep handlers in binary, duplicate thin wrappers in chat_cli.**

Less clean but avoids touching kiro-bot's structure. Not recommended.

### Phase 3: Auto-update for ECS (`kiro-cli bot update`)

Reuse the existing update infrastructure with a twist for headless/server use:

```rust
// In BotCommand::Update handler:
impl BotCommand {
    async fn execute_update(if_stale: Option<String>, force: bool) -> Result<ExitCode> {
        // 1. Check staleness (if --if-stale provided)
        if let Some(duration) = if_stale {
            let marker = dirs::data_dir()
                .unwrap_or_default()
                .join(".kiro/bot-last-update");
            if let Ok(meta) = std::fs::metadata(&marker) {
                let age = meta.modified()?.elapsed()?;
                let max_age = parse_duration(&duration)?;
                if age < max_age && !force {
                    println!("Last update was {}s ago, skipping (threshold: {})", 
                             age.as_secs(), duration);
                    return Ok(ExitCode::SUCCESS);
                }
            }
        }

        // 2. Reuse existing update logic (fetch manifest, compare, download, install)
        let args = crate::cli::update::UpdateArgs { check: false, force };
        let result = args.execute(os).await?;

        // 3. Touch the staleness marker
        std::fs::write(&marker, [])?;

        Ok(result)
    }
}
```

**ECS entrypoint usage:**
```bash
#!/bin/bash
set -euo pipefail

# Try to update binary (skip if updated within 7 days)
kiro-cli bot update --if-stale 7d || true

# Start the bot
exec kiro-cli bot start kiro-help --foreground
```

The `|| true` ensures the container still starts even if the update endpoint is unreachable.

### Phase 4: Manifest changes for server variant

The existing manifest already has a `variant` field on artifacts. Add a `"headless"` variant for linux builds that includes the `bot` feature:

```json
{
  "kind": "tarXz",
  "os": "linux",
  "architecture": "x86_64",
  "variant": "headless",
  "channel": "nightly",
  "download": "nightly/1.30.0/kiro-cli-x86_64-linux-headless.tar.xz",
  "sha256": "...",
  "size": 45000000
}
```

The `bot update` command would prefer `variant: "headless"` when running on linux without a display (check `$DISPLAY` / `$WAYLAND_DISPLAY` unset, or explicit `--variant headless` flag).

---

## Build Matrix

Single binary for all targets — no feature flags needed:

| Target | Build command | Bot activation |
|--------|--------------|----------------|
| macOS desktop | `cargo build -p chat_cli` | `KIRO_ENABLE_BOT=1` |
| Windows desktop | `cargo build -p chat_cli` | `KIRO_ENABLE_BOT=1` |
| Linux desktop | `cargo build -p chat_cli` | `KIRO_ENABLE_BOT=1` |
| Linux server (ECS) | same binary from release | `KIRO_ENABLE_BOT=1` in task def |

CI builds one artifact per platform. No separate "headless" variant needed.

---

## ECS Container Lifecycle

```
Container start
    │
    ├─ entrypoint.sh
    │   ├─ kiro-cli bot update --if-stale 7d
    │   │   ├─ Fetch manifest from S3/CDN
    │   │   ├─ Compare version
    │   │   ├─ Download + SHA256 verify
    │   │   └─ Replace binary in-place (tar.xz extract)
    │   │
    │   └─ exec kiro-cli bot start kiro-help --foreground
    │       ├─ Load config from /etc/kiro-help/
    │       ├─ Connect to Slack (Socket Mode)
    │       └─ Spawn ACP workers as needed
    │
    └─ ECS health check: process alive + Slack connected
```

For weekly refresh without container restart, add a background task inside the bot:
```rust
// Optional: in-process periodic update check
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(7 * 24 * 3600));
    loop {
        interval.tick().await;
        if let Ok(true) = check_update_available().await {
            tracing::info!("Update available, signaling for restart");
            // Signal ECS to cycle the task (exit 0, ECS restarts)
            std::process::exit(0);
        }
    }
});
```

ECS `desiredCount: 1` with restart policy ensures the new container picks up the latest binary.

---

## Migration Path

1. **Now:** Separate `kiro-bot` binary in Docker (current plan works, no changes needed)
2. **Next:** Add `bot` feature to chat_cli, wire subcommands, ship headless variant
3. **Later:** ECS containers switch from `kiro-bot` binary to `kiro-cli bot` commands
4. **Eventually:** Remove standalone `kiro-bot` binary target (keep library only)

The standalone binary can coexist indefinitely — it's just a thin wrapper over the library.

---

## Open Questions

1. **Update channel for bots** — Should bots track `nightly` or a separate `bot` channel? Nightly moves fast; a dedicated channel gives more control.
2. **In-place binary replacement on Linux** — The current update mechanism runs an installer (pkg/msi/tar). For headless Linux, extracting a tar.xz over the running binary works (Linux allows this). Confirm this is the desired approach vs. a symlink swap.
3. **Config bundling** — Should bot configs ship inside the binary (embed at compile time) or stay external? External is more flexible for multi-bot setups.
4. **Auth for manifest** — The current manifest URL is public CDN. If the headless variant should be private, we'd need auth headers on the fetch (IAM role on ECS → signed S3 URL).
