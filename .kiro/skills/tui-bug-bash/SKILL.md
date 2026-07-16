---
name: tui-bug-bash
description: Run a thorough bug bash on the Kiro CLI TUI across every engine and surface (v2/Rust and v3/KAS engines; lite and full TUI surfaces), then ship each confirmed bug as its own verified fix PR with Knight Rider visual evidence. Use when asked to hunt bugs across the TUI, re-verify no regressions after a batch of fixes lands, audit commands/menus/shortcuts/rendering/verbosity, or fix "subagent errors bleed into scrollback" / trust-granularity / engine-or-surface-drift classes of bug. Triggers on bug bash, bug hunt, regression sweep, exploratory audit, or "find and fix bugs" for the CLI.
---

# TUI Bug Bash

Orchestrate a wide, high-confidence bug hunt across the Kiro CLI TUI and turn every *confirmed* bug into its own minimal fix PR backed by Knight Rider (KR) before/after visual evidence.

Cover the whole matrix — don't fixate on one cell:

|          | **lite** surface | **full TUI** surface |
|----------|------------------|----------------------|
| **v2** (Rust engine) | ✓ | ✓ |
| **v3** (KAS engine)  | ✓ | ✓ |

The **engine** axis (v2/Rust vs v3/KAS) and the **surface** axis (lite vs full TUI) are orthogonal and are selected independently. A bug can live in one cell, one row, one column, or the shared layer under all four. Test the cells a given bug can plausibly touch — in parallel where they're independent (dispatch a subagent per cell/lens; see "Parallelize" below).

This skill is the **campaign orchestrator**. It composes two narrower skills instead of duplicating them:
- **`knight-rider`** — how the KR HTTP harness works (endpoints, launching, frames).
- **`knight-rider-bug-hunter`** — how to fuzz/exploratory-test one KAS tool's schema.

Use this one when the ask is broad ("hunt for bugs", "re-verify no regressions after these fixes", "audit the TUI") and the deliverable is *multiple fixed PRs*, not a single test run.

## The core loop

```
discover (parallel, many lenses × the relevant engine/surface cells)
  → dedup
  → adversarially VERIFY each candidate (default: refute)
  → one fix agent per SURVIVING bug (isolated worktree)
      → reproduce live → minimal fix → KR before/after → unique PR
  → coordinator verifies every PR claim against the real PR + pristine base
```

You are the coordinator. **Orchestrate, don't hand-do.** Anything a fix subagent can do (write the fix, drive KR, open + green the PR) it *should* do. You dispatch, dedup, and verify claims — you do not hand-edit files or resolve review comments yourself.

## Parallelize with subagents

Fan work out to subagents so lenses and engine/surface cells run concurrently:
- **Discovery:** one subagent per lens (below). Independent, read-only — run them all at once.
- **Verify:** one subagent per candidate, refute-by-default.
- **Fix:** one subagent per confirmed bug, each in its own git worktree (below).

Scale the fleet to the ask: a quick check = a few lenses, single-vote verify; "thoroughly audit / be comprehensive" = the full lens set across all four cells + a stronger verify pass. Give substantive subagents a capable model, not a cheap one.

## Prerequisites

- Work from a checkout **synced to `origin/main`** so you test the merged code, not a stale branch.
- A built backend binary (`target/debug/chat_cli`); for v3/KAS, a built KAS server (`packages/kiro-agent/dist/server/acp-server.js`). See `knight-rider` for the full launch env per engine/surface.
- `gh` authenticated.
- If re-verifying after a prior bash: know exactly which PRs merged (so you don't re-file closed non-bugs) and keep a running list of PRs worked/merged.

## Phase 1 — Discover (fan out, many lenses)

Spawn read-only finder subagents in parallel, each with a **distinct lens** so they don't converge on the same surface. For each lens, consider all the cells it can touch (v2 lite, v2 TUI, v3 lite, v3 TUI):

- **regressions** — re-read the files each recently-merged fix touched; did a shared abstraction miss a case or break a sibling path on another engine/surface?
- **commands & menus** — every slash command + submenu, per engine: does it *do* something, or point the user at a feature that doesn't exist on that engine / is a no-op? Any help/autocomplete leak of hidden or unimplemented subcommands?
- **shortcuts** — every keybinding on each surface: does it fire the right action, get swallowed by an open menu/approval, or is it advertised-but-dead on one engine?
- **tool render & subagent bleed** — the headline class: a failed subagent-stage tool leaking into the main transcript as an "unknown" failed tool; wrong status glyph/label; the static/live partition dropping or duplicating a tool.
- **verbosity & scrollback** — every density preset × filter × args-mode × thinking tri-state × truncation combo: does the renderer honor it on both surfaces without blank/dup/mis-truncated scrollback? Any menu row the renderer ignores? (Note: some config surfaces are intentionally lite-only — confirm before flagging.)
- **trust & approval** — granular trust (single/base/partial command) vs "entire tool" full trust; the consent wire + wildcard semantics; cross-session approval keying; a write tool gated by a shell-only capability check; does the prompt render the right options per engine?

Each finder returns structured findings: `title, file, line, severity, engine (v2|v3|both), surface (lite|tui|both), is_regression, symptom, why_real (cited lines), repro_hint`. Tell finders: read the ACTUAL code (cite `file:line`), prefer bugs reachable by real user behavior, and treat an engine or surface *difference* as likely intentional — flag only if clearly wrong.

## Phase 2 — Verify (adversarial; this is the quality gate)

Dedup across all finders, then give **each** candidate to an independent verifier subagent whose **default is to refute**. Refute if it is:
- intentional-by-design (especially a deliberate engine or surface difference — do NOT "unify" a divergence that's on purpose),
- unreachable by real user behavior on the target engine/surface,
- already handled elsewhere,
- a **known closed non-bug** with no new evidence,
- or the cited lines don't actually do what's claimed.

Confirm ONLY when a real user in a specific engine×surface cell hits a wrong behavior. This gate is what stops you re-filing non-bugs. Prefer perspective-diverse verifiers (correctness / reachability / does-it-repro) over N identical ones.

## Phase 3 — Fix (one subagent per confirmed bug)

Dispatch a **distinct fix subagent per confirmed bug**, each following `scripts/fix-agent-playbook.md`. Requirements baked into that playbook:

- **Isolated worktree per bug** off `origin/main`. Subagents NEVER `git stash` (worktrees share one stash stack — it cross-contaminates siblings); stage by explicit path, never `git add -A`.
- **Reproduce live first** in the affected cell(s) (KR in the right engine+surface, or a failing test that drives the real layers). No repro → no fix; say so and stop.
- **Fix the deepest shared layer** that's wrong. If multiple cells misbehave, the bug is upstream — fix there, don't special-case one surface/engine. Prefer reusing/abstracting a shared helper all cells call over drift-prone per-cell copies.
- **Minimum diff.** Comments only for the non-obvious.
- **KR before/after visual evidence is mandatory** — a failing-then-passing unit test alone is NOT sufficient. Build a self-contained `report.html` titled `PR #<n> — BUG: <name>` with the PR link at the top, and open it (GitHub can't attach these via CLI, so the browser tab is the deliverable). If a fix spans engines, capture the affected cell before/after AND a regression frame on the unaffected cell.
- **Unique PR** against main with a self-contained body (what + why + repro + fix; describe only the final state).

## Phase 4 — Coordinator verification (never trust the final message)

For every PR a subagent reports, verify the claims yourself:
- `gh pr view <n> --json files` — is the diff exactly the intended files? (See changelog gotcha below.)
- Its own touched tests pass; typecheck clean (`bun run typecheck` in `packages/tui`).
- **CI red? Diff against the pristine base before blaming OR exonerating.** Run the CI command (bare `bun test`, not a hand-picked file/pair — cross-file isolation differs) on `origin/main`, then on the branch. NEW failure names = a real regression the subagent must fix. IDENTICAL names = a pre-existing flake — rerun, don't code-fix. A test that passes alone but fails in the full run is a state-leak, not a flake.

Re-dispatch is normal if a subagent misfires at startup. Always give substantive subagents a capable model.

## Hard-won steering (the expensive lessons)

**Engine vs surface are orthogonal axes.** Engine = v2/Rust vs v3/KAS (agent-command routing differs — a Rust handler vs a "not supported" branch). Surface = lite vs full TUI (which layout renders). "Works on v2, no-ops on v3" can be by-design engine routing, not a surface bug. Keep the two axes separate when reasoning; when a KR run selects an engine, confirm which *surface* it's on before labeling a frame — lite and TUI banners differ (lite shows a `· lite` version tag and a rotating tip line; the full TUI shows the welcome banner and no tips). State the engine AND surface in every KR frame label.

**"Regression" has a precise meaning.** Behavior CHANGED for the worse across a specific commit. Establish the BEFORE behavior on the same input — don't infer it from code shape. A no-op that a PR merely made newly-*reachable* on a surface, but that was already a no-op on that engine before, is a pre-existing limitation, not a regression. A finder's `is_regression` flag is a hypothesis; verify the before-state (git-blame the transport, or drive the pre-PR build) before repeating the label.

**A green unit test can MASK the very bug it covers if it mocks the transport under test.** A hook test that stubs the command layer to succeed proves the hook *calls* the command, never that the real client *honors* it. Before trusting "there's a passing test", check what it mocks.

**Don't refute a "side-effects lost" claim by confirming the primary call still happens.** The side-effects may be emitted by a different wrapper layer the alternate path bypasses (e.g. two code paths call the same inner swap, but only one goes through the wrapper that also emits the switch event and re-advertises commands). Trace each side-effect to its actual emission site. When a live repro contradicts a code-only refutation, the repro wins.

**When a fix adds a value via ONE render channel, enumerate the cases where that channel is empty.** Fixing "write diff shows no filename" by un-suppressing the diff's path *header* missed empty-diff writes (delete / empty create / no-op edit) — the body is empty so the header never renders. Ask "what if this channel produces nothing?"

**An engine or surface difference is often intentional.** Confirm the divergence isn't by design before unifying (e.g. one surface exposes a config mode the other doesn't; a boolean-vs-tri-state config projection can be a deliberate simplification; some commands are intentionally unsupported on one engine). When unsure whether a behavior is intended per engine/surface, ASK — don't assume the gap is a defect.

**Reproduce before fixing; re-run the exact repro after.** Keep "reproduced" and "theorized" as different words. Never present an artificial/harness-only repro as validation of a real bug. When KR can't script the exact upstream wire, drive the real layers the bug spans directly (real event-conversion → real store handler → real renderer) and capture before/after by reverting only the fix hunk — that IS a real repro.

## CI reality (kiro-cli)

- The `no-changelog` label gates OFF the "Changelog fragment required" check AND the changelog auto-job. Add it right after `gh pr create`. The docs-check bot may still RACE a `.changes/unreleased/*.json` fragment onto the branch shortly after (after the first check) — so RE-verify `gh pr view <n> --json files` has zero `.changes/` entries; if one landed, `git rm` it, commit `docs: drop auto-generated changelog fragment (no-changelog)`, push.
- Known flakes (rerun, don't code-fix — they fail on pristine main too): macOS `clang_rt.osx` link, Windows IPC on the KAS acp-integ test, ubuntu input-editing wordmark snapshots, wall-clock perf asserts, and order-dependent unit tests that pass in isolation but flip under CI's file scheduling. Required aggregators mirror the whole matrix, so one flaky leg blocks merge — the leg must catch a green rerun. `gh run rerun <id> --failed` only works once the whole run is terminal.
- Many PRs auto-get a `needs-ux-review` label — a human-review gate, not CI. Green CI ≠ mergeable; say so.

## Anti-goals

- Don't defer or skip a strictly-improving change by calling it "too risky" — everything here is verifiable (KR, tests, real-layer drive). Do it and prove it green, or don't because it isn't actually an improvement.
- Don't silently ship "obvious" fixes a review surfaces on a PR that's already green and in human review — propose and wait.
- Don't chase a red CI check with source changes until you've reproduced the exact CI command locally and diffed the failure set against pristine main.
