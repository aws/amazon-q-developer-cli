# Fix-Agent Playbook — one confirmed bug → verified fix → PR with KR evidence

You are fixing ONE confirmed bug in the Kiro CLI TUI. It may live in a specific engine×surface
cell (v2/Rust or v3/KAS engine; lite or full TUI surface), a whole row/column, or the shared
layer under all four. Follow this exactly. Minimum-LOC fix — shortest working diff wins; comments
only for the non-obvious. Do NOT fix unrelated things you notice — note them in your final report.

## 0. Context you'll be given
- The bug: title, `file:line`, symptom, why-it's-real (cited lines), a repro hint, and which
  engine(s)/surface(s) it affects.
- Base your work on `origin/main`.
- The KR launch env per engine/surface. See the `knight-rider` skill.
- `gh` is authenticated.

## 1. Isolated worktree (NEVER work in a shared checkout)
Create your own git worktree off `origin/main`. Config isolation if your bug touches settings/verbosity:
`export KIRO_HOME="$WORKTREE/.kiro-test"` (KAS auth lives outside KIRO_HOME, so auth still works).
- NEVER `git stash` — sibling worktrees share one stash stack and it cross-contaminates them. Set
  work aside with a temp commit on your branch instead.
- Stage by explicit path. Never `git add -A` / `commit -am`.

## 2. Reproduce the bug LIVE first (mandatory — no fix without a repro)
Prefer a real Knight Rider run in the affected cell(s). Boot ALREADY in the target engine+surface via
env, not by typing a slash command to switch (that opens autocomplete and swallows the command). Per
the `knight-rider` skill: pick the engine (default v2, or `--kas` for v3) and the surface (full TUI by
default, or the lite env vars for lite). Confirm the surface before any BEFORE frame — lite and full
TUI banners differ. Assert the bug's PRECONDITION is actually reached (read the store/screen), not a
lookalike — non-determinism can skip it.
If a live KR repro is genuinely impossible, fall back to a FAILING test that drives the REAL layers the
bug spans (import the real modules — real event-conversion → real store handler → real renderer);
capture before/after by reverting only the fix hunk. State clearly which you used. A harness-only
artifact is NOT a real repro. If a mock/replay repro is used, audit every scripted field against the
actual emitter — a fallback on a field the wire never sends is dead code.
Note: `/api/screen` reads the ENTIRE xterm scrollback; to compare a setting across values use a FRESH
KR instance per value or read only the last turn's rows.

## 3. Fix it (minimum diff, deepest shared layer)
- If multiple cells misbehave, the bug is upstream — fix the shared layer, don't special-case one
  surface/engine. Prefer reusing/abstracting a shared helper all cells call over drift-prone copies.
- Match surrounding code style. Cut comments.
- Leave ONE runnable check: a focused test that FAILS before your fix and PASSES after, next to the
  module's existing tests. If it's an order-dependent/state-leak class, verify it in the FULL suite
  (`bun test`), not just isolation — pin any env/config the test depends on so a leaked global can't
  flip it.

## 4. Re-verify (re-run the EXACT repro)
- Re-run the KR repro → capture AFTER frames proving the bug is gone. If the fix touches a shared layer
  reachable from multiple cells, also capture a regression frame on an unaffected cell (e.g. the other
  engine still behaves correctly).
- `bun run typecheck` (from `packages/tui`) — must pass.
- Your new test + the module's existing tests pass. Run the FULL `bun test` and confirm you introduced
  no new failures vs `origin/main`'s baseline (only pre-existing flakes may remain).
- Prettier the touched TS files (pre-empts the auto-format bot). If Rust changed: `cargo build` +
  `cargo clippy --locked -- -D warnings`.

## 5. KR evidence report — build it and OPEN it
Build ONE self-contained `report.html` in your KR output dir a reviewer can read without running
anything:
- Title: `PR #<n> — BUG: <short name>` (patch the PR number in right after `gh pr create`; build with a
  `(pending)` placeholder first). Put a visible `PR: <url>` line at the TOP of the body so the browser
  tab immediately identifies which PR it backs.
- Sections: the bug (1-2 sentences + file:line, and which cell(s) it hits); BEFORE frames inline;
  the fix (diff); AFTER frames inline; test/typecheck output. Label every frame with the engine AND
  surface it was captured on.
- `open` it. GitHub can't attach these via CLI, so the browser tab IS the deliverable.

## 6. Commit, push, open a UNIQUE PR
- Conventional commit (`fix(lite): …` / `fix(tui): …`). Push `-u origin <branch>`.
- If this batch wants NO changelog: do NOT add a `.changes/` fragment; add the label right after
  create: `gh pr edit <n> --add-label "no-changelog"`. The docs-check bot may RACE a fragment on
  shortly after (after your first check) — re-verify `gh pr view <n> --json files` has zero `.changes/`
  entries; if one landed, `git rm` it, commit `docs: drop auto-generated changelog fragment
  (no-changelog)`, push.
- `gh pr create --base main` with a self-contained body: what + why + repro + fix + affected cell(s).
  Describe ONLY the final state — no "previously we did X" narration. Add a `bookkeeping: <abs path to
  report.html>` line at the very bottom.
- Tear down your KR instance(s) when done.

## 7. Report back (final message = data, not prose)
PR_URL / BRANCH / REPORT (path opened; PR# at top?) / CELLS (which engine×surface affected) / REPRO
(KR live or failing test; the precondition you asserted) / FIX (file:line, what changed) / VERIFY
(typecheck + tests; full-suite delta vs main; regression frame on unaffected cell if shared) /
NOCHANGELOG (label + zero fragments confirmed via `gh pr view`?) / NOTES.
If you could NOT reproduce the bug, STOP: report `REPRO: NOT REPRODUCED` with what you tried, and do
not open a PR.
