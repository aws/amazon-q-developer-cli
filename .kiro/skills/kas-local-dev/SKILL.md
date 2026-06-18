---
name: kas-local-dev
description: Set up and run KAS (kiro-agent) locally without CodeArtifact permissions. Use when a user says they want to use KAS locally, develop with local kiro-agent, can't install @kiro/agent, has no CodeArtifact access, or wants to iterate on kiro-agent source. Triggers on "use KAS locally", "local KAS", "local kiro-agent", "can't install kiro-agent", "no CodeArtifact access", "local agent development", "test KAS", "run KAS".
---

# KAS Local Development

## When This Skill Activates

When the user wants to run or develop with KAS locally.

## Procedure

### Run KAS locally (one command)

```bash
cd packages/tui && bun run dev --local-kas --skip-rust-build
```

This automatically:
1. Clones `kiro-team/kiro-agent` into `local/kiro-agent` (if not already cloned)
2. Runs `npm install` + `npm run build` (if the built server doesn't exist)
3. Sets `KIRO_KAS_SERVER_PATH` and `KIRO_AGENT_ENGINE=kas`
4. Skips CodeArtifact login and `bun install` entirely
5. Launches the TUI

### Iterate on kiro-agent source

```bash
cd local/kiro-agent
# Make changes...
npm run build
# Restart: bun run dev --local-kas --skip-rust-build
```

### Use a specific KAS version

```bash
cd local/kiro-agent
git fetch --tags
git checkout v1.2.3      # or any tag/branch/commit
npm install              # in case deps changed
npm run build
cd ../..
bun run dev --local-kas --skip-rust-build
```

To see available versions:
```bash
cd local/kiro-agent && git tag --sort=-v:refname | head -20
```

### Rebuild from scratch

```bash
rm -rf local/kiro-agent
bun run dev --local-kas --skip-rust-build  # re-clones and rebuilds
```

## How It Works

`KasAcpClient` in `packages/tui/src/acp-client.ts` resolves the server in order:
1. **`KIRO_KAS_SERVER_PATH` env var** ← `--local-kas` sets this
2. Walk up from `__dirname` looking for `node_modules/@kiro/agent/dist/server/acp-server.js`

## Safety Guardrails

Three mechanisms prevent accidentally committing the local checkout:

1. **`.gitignore`** — `local/` is ignored at the repo root
2. **Pre-commit hook** — blocks commits containing paths under `local/`
3. **Convention** — the directory name `local/` signals "not for version control"

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| Clone fails | Check GitHub access to `kiro-team/kiro-agent` |
| `Cannot find module` at KAS runtime | `cd local/kiro-agent && npm run build` |
| Want to update kiro-agent | `cd local/kiro-agent && git pull && npm run build` |
