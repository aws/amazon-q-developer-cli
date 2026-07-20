---
name: upgrade-kas
description: SOP for upgrading the KAS (Kiro Agent Server) version in kiro-cli — the @kiro/agent, @kiro/client, and @kiro/acp-type-covenant npm packages. Use when bumping, upgrading, or updating the KAS / @kiro/agent / kiro-agent version, or verifying a KAS version bump is safe for the CLI. Triggers on "bump KAS", "upgrade KAS", "update @kiro/agent", "bump kiro-agent version", "new KAS version".
---

# KAS Version Upgrade SOP

Upgrade the KAS TypeScript agent engine (`@kiro/agent`) and its sibling packages, then verify the bump is safe for the CLI across every contract tier.

> **Related:** the `kas-agent` skill documents the KAS ↔ CLI integration architecture. Read it if you need context on how the pieces connect. This skill is the *how to upgrade safely* procedure.

## Current State

!`echo "=== Declared @kiro versions (packages/tui/package.json) ===" && grep -E '"@kiro/(agent|client|acp-type-covenant)"' packages/tui/package.json 2>/dev/null; echo "" && echo "=== Installed @kiro/agent ===" && grep '"version"' packages/tui/node_modules/@kiro/agent/package.json 2>/dev/null | head -1 || echo "(not installed — run bun install)"; echo "" && echo "=== Rust on-disk session schema contract ===" && grep -E 'CURRENT_SCHEMA_VERSION|SUPPORTED_SCHEMA_VERSIONS' crates/chat-cli-v2/src/agent/kas/schema.rs 2>/dev/null | grep -v '///'`

## The Golden Rule: bump all three in lockstep

`@kiro/agent`, `@kiro/client`, and `@kiro/acp-type-covenant` are versioned together and **must move to the same version in one change**. `@kiro/client` declares a peer dependency on an exact `@kiro/acp-type-covenant` version, so the registry itself will reject a mismatch — but bumping only one in `package.json` is the most common mistake. All three live in `packages/tui/package.json`:

- `@kiro/acp-type-covenant` — **dependencies** (the shared type contract)
- `@kiro/client` — **dependencies** (the `KiroClient` RPC surface)
- `@kiro/agent` — **devDependencies** (the KAS server, used for local/E2E runs)

## Prerequisites

- CodeArtifact access via `ada` (the `@kiro/*` packages come from a private registry, not public npm).
- Logged in: `./target/debug/chat_cli whoami` (KAS's auth callback needs a valid token).
- `python3`, `bun`, `cargo`, and (for the smoke test) `lsof`/`curl` available.

## Procedure

### Step 1 — Bump the three versions

Edit `packages/tui/package.json`, setting all three `@kiro/*` entries to the target version. Do not touch the transitive `@kiro/*` deps in the lockfile by hand — `bun install` resolves those.

### Step 2 — Authenticate to CodeArtifact + reinstall

```bash
./scripts/codeartifact-login.sh   # writes root .npmrc with a scoped token (~12h TTL)
bun install                        # regenerates bun.lock from the private registry
```

Verify the lockfile picked up the target version and that transitive `@kiro/*` deps (`@kiro/context-providers`, `@kiro/sandbox-proxy`) moved in lockstep:

```bash
grep -E '"@kiro/(agent|client|acp-type-covenant)": \[' bun.lock
```

> If `bun install` hits `registry.npmjs.org … 404` for `@kiro/*`, the `.npmrc` is missing/expired — re-run `codeartifact-login.sh`.

### Step 3 — Rebuild twinki (avoids a false typecheck failure)

`bun install` refreshes workspace links; twinki's built types can go stale, producing errors like `Property 'setWideLines' does not exist on type 'Instance'` that are **not** caused by the KAS bump. Rebuild the dependency-ordered workspace first:

```bash
bun run --filter twinki-monorepo build
```

### Step 4 — Typecheck (Tier-1 compile-time contract)

```bash
cd packages/tui && bun run typecheck
```

This catches breaks in the *type-checked* subset of the KAS interface: `KiroClient` method signatures, `ClientCapability`, `GetAccessTokenResponse`, and the `_kiro/spec/*` request/response types. Fix any errors before continuing.

### Step 5 — KAS unit/integration tests

```bash
cd packages/tui
bun test src/__tests__/kas-acp-client.test.ts \
         src/auth/__tests__/acp-auth-callback.test.ts \
         src/stores/app-store.kas-pipeline.test.ts
```

> These mock `KiroClient`, so they validate *our usage patterns*, not the real package's new shape. They are necessary but not sufficient — the live smoke test (Step 8) is the real gate.

### Step 6 — Verify the Rust session-schema contract (Tier-3 split-brain check)

The Rust side hard-mirrors KAS's **on-disk** session schema version. Confirm `@kiro/agent`'s persisted `schemaVersion` still matches `CURRENT_SCHEMA_VERSION` / `SUPPORTED_SCHEMA_VERSIONS` in `crates/chat-cli-v2/src/agent/kas/schema.rs`:

```bash
echo "Rust:"; grep -E 'CURRENT_SCHEMA_VERSION|SUPPORTED_SCHEMA_VERSIONS' crates/chat-cli-v2/src/agent/kas/schema.rs | grep -v '///'
echo "KAS on-disk writes:"; grep -rn 'schemaVersion:\s*"' packages/tui/node_modules/@kiro/agent/dist/server/acp-server.js | head -3
```

They must agree (currently `"1.0.0"`). If KAS bumped its on-disk schema, update the Rust constants **and** re-run `cargo test -p chat_cli_v2`, or cross-engine session resume/import will break.

> **Do NOT use** the covenant's exported `SCHEMA_VERSION` for this check — it is the *package* version (a documented name collision), not the on-disk contract. See the comment in `schema.rs`.

### Step 7 — Build the Rust binary

Needed for the smoke test: KAS's `--auth=acp-callback` shells out to `chat _ get-kas-token`, and Knight Rider sets `KIRO_CHAT_CLI_BIN`.

```bash
cargo build -p chat_cli --bin chat_cli
```

### Step 8 — Live Knight Rider `--kas` smoke test (the real gate)

This is the only step that exercises the `tsc`-blind surfaces (see Gotchas). Run the TUI from source against the newly installed `@kiro/agent`:

```bash
cd packages/tui
for pid in $(lsof -ti:3001 2>/dev/null); do kill $pid 2>/dev/null; done; sleep 1
nohup bun run knight-rider --kas > /tmp/knight-rider.log 2>&1 &
sleep 25
curl -s http://localhost:3001/api/status   # expect ready:true
```

Then drive scenarios via the Knight Rider HTTP API (see the `knight-rider` skill for the shell helpers and full API). Minimum coverage for a KAS bump — each maps to a specific contract surface:

| Check | Verifies |
|---|---|
| Boot: status bar shows model/effort/context (e.g. `Default · Claude Opus 4.8 · High · ◔ 2%`) | initialize handshake + `newSession`/`setSessionConfigOption` return shapes (`configOptions`/`modes`) |
| `/model` picker shows credit multipliers + `[active]` | `KiroModelOptionMeta` `_meta.kiro` cast path |
| `/tools` shows MCP `@server/tool` titles; `/mcp` shows servers running | `stripMcpTitlePrefix`, `_kiro/secret/*` capability |
| A real prompt turn with a read tool → response + `▸ Credits: … • Time: …` | auth callback produced a valid token; turn-completion telemetry (`normalizeKasTurnCompletion`) |
| A shell prompt → approval dialog → approve → executes | permission/consent meta path |

Confirm `/tmp/knight-rider.log` has no errors, then stop the server:

```bash
for pid in $(lsof -ti:3001 2>/dev/null); do kill $pid 2>/dev/null; done
```

Record the results in a table for the PR description (Step 10c):

| Check | Evidence |
|---|---|
| Boot: status bar | e.g. `Default · Claude Opus 4.8 · High · ◔ 1%` |
| `/model` picker | N models with `x.xxX credits`, active model marked `[active]` |
| `/tools` + `/mcp` | tool count + server count with `● running` |
| Real prompt turn | tool used + `▸ Credits: X.XX • Time: Xs` |
| Tool approval | command → Allow → output |
| Log errors | empty or list findings |

### Step 9 — Create `[V3]` changelog fragments

V3 is available to all users via `--v3` / `kiro-cli chat --agent-engine=kas`. Create `.changes/` fragments for commits that change observable behavior in V3 mode.

> **Skip fragments for features gated behind settings the CLI doesn't send yet** (e.g. `kiroMemoryEnable`). If a feature requires a client-side opt-in that doesn't exist, it's invisible to users — no fragment needed until the CLI wires up the setting.

#### Decision criteria

A KAS commit warrants a `[V3]` fragment when it:
- Adds a new user-facing feature (new tool, new subagent, new hook loading path)
- Fixes a bug users could hit (approval loops, proxy failures, publish behavior)
- Changes UX behavior (new slash commands, renamed concepts users see)

Skip fragments for:
- Internal telemetry/instrumentation
- Test-only changes
- Backend-only changes with no wire/UX impact (e.g. `scopeKey` the CLI sends but users never see)
- Features gated behind a setting the CLI doesn't send yet (e.g. `kiroMemoryEnable`)

#### Creating fragments

Use `./scripts/new-change.sh` when possible:

```bash
./scripts/new-change.sh added '[V3] Description of the feature'
./scripts/new-change.sh fixed '[V3] Description of the fix'
```

If the validator rejects valid descriptions (e.g. paths like `~/.kiro/hooks/` trigger the slash-command check), create fragments manually:

```bash
TIMESTAMP=$(date +%Y%m%d-%H%M)
echo '{"type":"added","description":"[V3] Description here"}' | jq . > ".changes/${TIMESTAMP}-added-v3-slug.json"
```

#### Convention

- Always prefix with `[V3]` — this distinguishes KAS/V3-only features from V2 changes in the combined changelog
- Follow `.changes/GUIDELINES.md` rules: no verb prefix, capitalize, one change per entry, under 100 chars
- Wrap paths and commands in backticks

### Step 10 — Document the upgrade (commit diff → impact analysis → PR)

Produce a PR description that (1) captures the full KAS commit changelog across the two versions, (2) enumerates which commits impact the interface or CLI behavior with a one-liner + why-not-a-concern for each, and (3) links to the complete change list.

#### 10a. Capture the commit diff across versions

The KAS repo (`kiro-team/kiro-agent`) is a local checkout/worktree (ask the user for the path; e.g. `~/work/kas/<worktree>`). Versions map to git tags as **`agent-v-X.Y.Z`** (note the `-v-`), which does *not* match the npm scheme directly — always `fetch --tags` first, the local worktree is usually missing them.

```bash
KAS=~/work/kas/<worktree>            # the kiro-agent checkout
OLD=agent-v-0.8.0                    # = current @kiro/agent version
NEW=agent-v-0.15.0                   # = target version
git -C "$KAS" fetch origin --tags 2>&1 | tail -2
git -C "$KAS" rev-parse --short "$OLD" "$NEW"          # confirm both resolve
git -C "$KAS" rev-list --count "$OLD..$NEW"            # commit count
git -C "$KAS" diff --stat "$OLD..$NEW" | tail -1       # overall churn
```

#### 10b. Find the impacting commits

The **interface** set = commits that touched the shared contract package the CLI consumes:

```bash
git -C "$KAS" log --pretty=format:'%h %s' "$OLD..$NEW" -- packages/acp-type-covenant
```

Inspect the covenant diff to classify each change — the safe pattern is **additive/optional** (new optional fields, new capabilities, new enum values), which is why `bun run typecheck` stays clean:

```bash
git -C "$KAS" diff "$OLD..$NEW" -- packages/acp-type-covenant/session/types.ts \
                                    packages/acp-type-covenant/session/session-info-update.ts
```

The **CLI-behavior** set = commits outside the covenant that change observable TUI/session behavior. Surface candidates and filter by judgment (drop tests/CI/cucumber/sandbox-internals/c2s-tooling the CLI doesn't surface):

```bash
git -C "$KAS" log --oneline "$OLD..$NEW" | grep -iE "session|model|mode|mcp|auth|token|context|effort|slash|hook|tool"
```

For each impacting commit write a one-liner + **why it's not a concern**, grounded in evidence (not boilerplate):
- *additive/optional* — no consumed field removed or retyped
- *opt-in* — default behavior unchanged
- *progressive-enhancement* — clients that don't read the new field fall back
- *server-side/internal* — no change to the wire shape the CLI consumes
- *validated by the Step 8 smoke test* — cite the specific check that exercised it

Call out any **residual watch item** honestly (e.g. a new stop reason the TUI degrades rather than renders) as a non-blocking follow-up — do not claim coverage you didn't verify.

#### 10c. Build the PR description

Assemble: Summary / Changes / Testing (from Steps 4–8) + the two impact-analysis tables (interface, CLI-behavior) + a changelog section with the release link, compare link, and the full commit list in a `<details>` block. Linkify each `(#NNNN)` to the kiro-agent PR.

> **⚠️ Commit message rule:** In the kiro-cli commit message body, do NOT include bare `(#NNNN)` PR references from kiro-agent commits. GitHub auto-links `#NNNN` to the **current repo** (kiro-cli), producing misleading links to unrelated PRs. Either strip them entirely or use the cross-repo syntax `(kiro-team/kiro-agent#NNNN)`.

```bash
BODY=/tmp/kas-pr-body.md
# ... write Summary/Changes/Testing/impact tables into $BODY first ...
{
  echo "## KAS changelog: \`$OLD\` → \`$NEW\`"; echo
  echo "Release: https://github.com/kiro-team/kiro-agent/releases/tag/$NEW"
  echo "Compare: https://github.com/kiro-team/kiro-agent/compare/$OLD...$NEW"; echo
  echo "<details><summary>All commits in the range</summary>"; echo
  git -C "$KAS" log --pretty=format:'%s (%h)' "$OLD..$NEW" | python3 -c '
import sys, re
for l in (x.rstrip() for x in sys.stdin if x.strip()):
    m = re.search(r"\(#(\d+)\)", l)
    if m: l = l.replace(f"(#{m.group(1)})", f"([#{m.group(1)}](https://github.com/kiro-team/kiro-agent/pull/{m.group(1)}))")
    print(f"- {l}")'
  echo; echo "</details>"
} >> "$BODY"
gh pr create --base main --title "chore: Bump KAS packages to <version>" --body-file "$BODY"
# or, to update an existing PR: gh pr edit <number> --body-file "$BODY"
```

## Gotchas & contract tiers

The KAS interface is only *partially* type-checked. Know where the blind spots are:

- **Tier 1 (type-checked):** `KiroClient` methods, `ClientCapability`, `GetAccessTokenResponse`, spec types. `bun run typecheck` catches these.
- **Tier 1 (`tsc`-blind — casts & strings):** `newSession`/`setSessionConfigOption` **return** shapes (read via `as { … : unknown }`), `KiroModelOptionMeta` (cast at `acp-client.ts`), and wire-method string literals (`_kiro/secret/*`, `_kiro/openExternalUrl`, `session/fork`, `configId` values). Only the **live smoke test** catches these.
- **Tier 1 (cross-language):** `GetAccessTokenResponse` is mirrored in Rust `crates/chat-cli-v2/src/auth/kas_token.rs` via typeshare. `tsc` sees only the TS half — if the covenant's auth token shape changed, regenerate/verify the Rust side.
- **Tier 2 (`_meta.kiro` parsers):** the `normalizeKas*`/`extractKas*` family in `acp-client.ts` degrade **silently** on a renamed field. The smoke test surfaces most of these visually (missing credits, blank model list, dropped subagent names).
- **Tier 3 (session schema):** Step 6.

## Post-upgrade

- **Do not commit `.npmrc`** — it holds a CodeArtifact token (should be gitignored; confirm before `git add`).
- Intended diff is only `packages/tui/package.json` and `bun.lock` (plus `schema.rs` if the on-disk schema changed).
- Read the KAS changelog for the target version, specifically for: ACP SDK version bumps, `_meta.kiro` field renames, on-disk session schema changes, capability additions/removals, agent-mode changes, and the MCP output envelope shape. These are the Tier-2/Tier-3 items that pass typecheck but break at runtime.

## Quick checklist

- [ ] All three `@kiro/*` bumped in lockstep in `packages/tui/package.json`
- [ ] `codeartifact-login.sh` + `bun install`; lockfile shows target version (+ transitive lockstep)
- [ ] `bun run --filter twinki-monorepo build`
- [ ] `bun run typecheck` clean
- [ ] KAS unit tests pass
- [ ] Rust `schema.rs` on-disk schema version matches `@kiro/agent`
- [ ] `cargo build -p chat_cli` succeeds
- [ ] Knight Rider `--kas` smoke: boot, /model, /tools, /mcp, real turn, tool approval — no log errors
- [ ] KAS changelog reviewed for Tier-2/Tier-3 breaking changes
- [ ] `[V3]` changelog fragments created for user-visible features/fixes
- [ ] Commit diff captured across `agent-v-<old>..agent-v-<new>`; interface + CLI-behavior commits analyzed (one-liner + why-not-a-concern each)
- [ ] PR description includes the impact analysis + Knight Rider results table + release/compare links + full commit list
- [ ] Commit message uses cross-repo syntax `(kiro-team/kiro-agent#NNNN)` — no bare `(#NNNN)` refs
- [ ] `.npmrc` not staged for commit
