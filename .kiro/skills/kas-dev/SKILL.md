---
name: kas-dev
description: Cross-repo development loop for KAS (Kiro Agent Server) changes tested through kiro-cli. Use when developing or debugging the kiro-agent engine against the CLI TUI, wiring a local kiro-agent build into kiro-cli, refreshing CodeArtifact auth, or deciding where a KAS-path change should be tested. Complements the kas-agent skill, which documents the ACP interface itself.
---

# KAS Development Loop

How to develop, run, and verify changes to KAS (the TypeScript agent engine in the `kiro-agent` repo) through kiro-cli. For the ACP interface, engine selection, and `KasAcpClient` internals, read the `kas-agent` skill first.

## Repo Layout

Keep `kiro-agent` and `kiro-cli` as sibling folders — the kiro-agent dev scripts auto-detect `../kiro-cli` (override with `KIRO_CLI_PATH`).

```
workspace/
├── kiro-cli/      # this repo: Rust CLI + TUI (packages/tui)
└── kiro-agent/    # KAS engine: packages/kiro-agent → @kiro/agent
```

## Which Loop to Use

| You are changing | Loop |
|---|---|
| Only kiro-cli (TUI or Rust), running against published `@kiro/agent` | `bun run dev` from `packages/tui` (KAS is the dev default) |
| kiro-agent code, driven from this repo | `bun run dev --local-kas` (auto-clones/builds `local/kiro-agent`) |
| kiro-agent code, driven from a kiro-agent checkout | `npm run dev:build-cli` + `npm run dev:start-cli` from the kiro-agent repo |
| kiro-agent code, one-off manual wiring | `KIRO_KAS_SERVER_PATH` override (below) |

### Loop 1: kiro-cli against published KAS

Dev mode defaults to the KAS (V3) engine — opt out with `--v2` or `KIRO_AGENT_ENGINE=v2`:

```bash
# From packages/tui
bun run dev --skip-rust-build

# Or the convenience wrapper from the repo root (forces KIRO_AGENT_ENGINE=kas)
./scripts/test-kas.sh
```

`test-kas.sh` itself only sets the env var and execs `bun run dev`; the auth and setup work lives in `packages/tui/scripts/start-dev.ts`, which every path above runs through — it checks CodeArtifact token expiry (refreshing via `./scripts/codeartifact-login.sh`), runs `bun install`, and sets `KIRO_CHAT_CLI_BIN`.

### Loop 2: local kiro-agent, driven from this repo

```bash
# From packages/tui — clones kiro-team/kiro-agent into local/kiro-agent on
# first run, npm-installs and builds it, and sets KIRO_KAS_SERVER_PATH at it.
bun run dev --local-kas
```

Edit code under `local/kiro-agent`, rebuild it (`npm run build` in that checkout), and restart the dev server to pick up changes.

### Loop 3: sibling-checkout KAS development (from the kiro-agent repo)

From a kiro-agent checkout (see its `DEV-GUIDE-AGENT-CLI.md` for detail; it auto-detects `../kiro-cli`, override with `KIRO_CLI_PATH`):

```bash
# Terminal 1: install deps on first run, build both repos, watch for changes
npm run dev:build-cli

# Terminal 2: launch the kiro-cli TUI pointing at your local kiro-agent build
npm run dev:start-cli
```

`dev:start-cli` sets `NODE_OPTIONS=--inspect=9229`, so the KAS process is debuggable: attach VS Code ("Attach to KAS (kiro-agent)") and set breakpoints in kiro-agent TypeScript — source maps are enabled. Restart `dev:start-cli` (Ctrl+C, re-run) to pick up rebuilt code.

### Loop 4: manual server-path override

Point the TUI at any locally built KAS server without the watcher:

```bash
KIRO_KAS_SERVER_PATH=/path/to/kiro-agent/packages/kiro-agent/dist/server/acp-server.js \
  bun run dev --skip-rust-build     # from packages/tui
```

Resolution order when `KIRO_KAS_SERVER_PATH` is unset: walk up from the TUI bundle looking for `node_modules/@kiro/agent/dist/server/acp-server.js`.

## Auth: CodeArtifact Tokens

`@kiro/agent` is published to CodeArtifact; tokens expire after ~12 hours. The TUI dev script (`start-dev.ts`) checks expiry and refreshes automatically on every `bun run dev` (skipped with `--local-kas`, which needs no registry pull). If a build still fails with 401:

```bash
./scripts/codeartifact-login.sh          # run in BOTH repos when doing cross-repo work
```

The generated `.npmrc` contains the auth token — it is gitignored; never commit it.

## Testing a KAS-Path Change

Work up the pyramid; run the narrowest layer that can catch your bug first.

| Layer | Where | What it covers |
|---|---|---|
| Unit | `packages/tui/src/__tests__/kas-acp-client.test.ts` | `KasAcpClient` logic with the SDK stubbed |
| ACP integration | `packages/tui/acp_integ_tests/` | real `KasAcpClient` + `@kiro/client` against a mock ACP server — the wire protocol, initialize filter, session lifecycle |
| E2E through TUI | Loops 2–4 above, plus `packages/tui/e2e_tests/` | full TUI ↔ KAS behavior |
| Cross-repo E2E | `kiro-agent` repo: `.kiro/skills/kiro-cli-kiro-agent-e2e` | scripted end-to-end across both repos |

For TUI behavior that differs between engines, test both paths: `KIRO_AGENT_ENGINE=kas` and unset (Rust ACP). `packages/tui/src/stores/app-store.kas-pipeline.test.ts` shows the pattern for engine-conditional store logic.

## Debugging

- KAS runtime logs: `~/.kiro/logs/<timestamp>/kiro.log` (also see the `search-kiro-logs` skill in the kiro-agent repo)
- TUI side: `KIRO_TUI_LOG_LEVEL=debug`, write to `KIRO_TUI_LOG_FILE`
- KAS process verbosity: `KIRO_LOG_LEVEL=debug` (or `trace`), file via `KIRO_CHAT_LOG_FILE`
- Slash-command parity between engines is tracked in `docs/tasks/KAS_SLASH_COMMANDS_TODO.md`

## Common Failure Modes

- **401 on install/build** → expired CodeArtifact token; run `./scripts/codeartifact-login.sh` in both repos
- **TUI unexpectedly on the Rust engine** → something set `KIRO_AGENT_ENGINE=v2` or passed `--v2`; dev mode defaults to KAS (see `packages/tui/scripts/start-dev.ts`)
- **Stale KAS behavior after editing kiro-agent** → the TUI is resolving a published `@kiro/agent` from `node_modules` instead of your build; use `--local-kas` (and rebuild `local/kiro-agent`), or set `KIRO_KAS_SERVER_PATH`
- **KAS auth/token errors in dev** → KAS resolves OIDC tokens by shelling out to the chat-cli binary via `KIRO_CHAT_CLI_BIN` (`--auth=acp-callback` mode); the dev script sets it — make sure the Rust binary it points at exists (run without `--skip-rust-build` once)

## Cross-Repo Etiquette

A change spanning both repos means two PRs with their own conventions — read `CONTRIBUTING.md` in each. Land the kiro-agent change first when kiro-cli depends on a new `@kiro/agent` behavior, and gate the kiro-cli side on the published version. Keep `.kiro/skills/kas-agent/SKILL.md` (interface) and this skill (dev loop) updated when the wiring changes.
