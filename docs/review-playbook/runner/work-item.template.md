# Work-item template

Every file under `work/` conforms to this shape. The planner produces these. The worker (sub-agent or human) executes them.

A work item is **one review × one partition × one kind × one platform**. Never a mix.

## Template

```markdown
---
id: 01-hooks-code
review: 01-async-render-path
kind: code                   # code | blackbox
partition: directory         # directory | feature | recency | pattern | size | platform | harness
scope:
  include:
    - "packages/tui/src/hooks/**"
    - "packages/twinki/packages/twinki/src/terminal/**"
  exclude:
    - "**/__tests__/**"
    - "**/*.test.*"
    - "**/*.vitest.*"
platform: any                # any | linux | macos | windows | headless-ci | docker-linux
harness: null                # null (code) | unit | integration | e2e | knight-rider | cpu-prof | heap-snapshot | ad-hoc | ci-matrix
duration: fast               # fast (< 5 min) | medium (< 30 min) | long (> 30 min — prefer defer)
techniques: [1, 2, 3, 4]     # technique numbers from the referenced review file
budget:
  input-tokens: 60000
  output-tokens: 8000
  wall-clock: 30m
findings-prefix: 01-hooks-code  # findings emitted as docs/review-playbook/runner/findings/01-hooks-code-<date>-<time>-<slug>.md
depends-on: []               # other work-item ids that must finish first
defer: null                  # null | runbook — runbook means "emit a human runbook, do not execute"
knight-rider: false          # true if this work item benefits from Knight Rider interactive TUI testing
---

# Work item: <review-title> — <partition-label> — <kind>

## Scope in one line

<one sentence describing what this item covers>

## What to do

1. Read the techniques listed in the frontmatter `techniques:` field from
   `docs/review-playbook/<review>.md`.
2. Apply them to the files listed in `scope.include`, excluding `scope.exclude`.
3. **Write one finding per file** under `docs/review-playbook/runner/findings/`.
   File naming convention:

   ```
   <work-item-id>-<YYYYMMDD>-<HHMM>-<short-kebab-slug>.md
   ```

   The slug is a short (~50 chars max) kebab-case summary of the finding's
   one-line description. Date and time are UTC. Time lets multiple findings
   from the same work item in the same day coexist without collision.

4. Each finding file uses the schema below. Write files as you go — do not
   batch them up in context.
5. When the work item is complete, append a single `<id>-<date>-<time>-done.md`
   marker file with `status: done` frontmatter and a one-line summary
   ("N findings emitted, M techniques ran, budget consumed X%"). If zero
   findings were produced, emit only the done marker.

## Budget rules

- If you finish under budget, do not expand scope.
- If you exceed budget before finishing, stop and emit a
  `<id>-<date>-<time>-continuation.md` marker describing what remains.
  The planner picks it up next run.
- Never re-read the same file within a work item.

## Finding-file schema

```markdown
---
id: <work-item-id>-<YYYYMMDD>-<HHMM>-<slug>
work-item: <work-item-id>
review: <review-id>           # e.g. 01-async-render-path
technique: <number>           # technique number within the review
class: <short-class-tag>      # e.g. async-in-resize-path, yoga-zero-width
severity: <severity>          # crash | spiral | regression | slowdown | smell
file: <path:line>             # single file:line if specific, else the directory
line: <number>                # optional, numeric line number
platforms-affected: [<list>]  # [any] | [linux, macos, windows] | subset
discovered-by: <code|blackbox>
discovered-at: <ISO-8601>
status: open                  # open | resolved | duplicate | wontfix | needs-human-check
duplicate-of: null            # null or another finding id (set when status == duplicate)
corroborated-by: []           # list of work-item ids that independently found the same issue (set by review-dedup)
resolved-at: null             # ISO-8601 UTC when status transitioned to resolved (set by review-validate)
resolved-at-commit: null      # git HEAD at validate time when status transitioned to resolved
---

# <one-line title, same as the slug reading>

<one or two paragraphs: what is the finding, why it matters>

## Evidence

<grep output, code reference, blackbox artifact path, test failure, etc.>

## Proposed fix

<short paragraph or snippet — optional if the fix needs discussion>

## Related

<cross-references to other findings, PRs, tickets — optional>
```

## Platform and harness notes

<filled in only for blackbox items>

- `platform: linux` — runs on an Ubuntu 22.04+ host, or Docker for non-Linux dev machines.
- `platform: macos` — runs on macOS 14+ with a real PTY.
- `platform: windows` — runs via the `review-playbook.yml` workflow (manual trigger).
- `platform: headless-ci` — runs without a real TTY; `TERM=dumb`, `CI=1`.
- `harness: ci-matrix` — dispatches to `.github/workflows/review-playbook.yml`.
- `harness: ad-hoc` — runs `packages/tui/scripts/probes/<name>.ts` locally.

## Completion criteria

- `output` file exists and ends with a "done" marker line.
- All techniques in the frontmatter have been attempted (findings or "none found" explicitly noted).
- Any deferred work is captured as a continuation note.
```

## Field reference

### `kind`

- `code` — static analysis only. No process spawns. No runtime observation. Reads source, writes findings.
- `blackbox` — runtime observation. Runs a probe, a test, or a benchmark. May require a specific platform or harness.
- `ui-test` — interactive UI testing via Knight Rider. The agent drives the TUI directly through HTTP calls (keystrokes, screen reads, frame captures). No probe script needed. Never deferred to runbook.

The planner never mixes these in one work item.

### `partition`

The axis chosen when breaking this review's work up. Informational — used by the rollup stage to group findings back together.

### `scope.include` / `scope.exclude`

Globs. The worker enforces these; anything outside the glob is out of scope.

### `platform`

- `any` — code work items, or blackbox work items where platform does not materially matter (e.g. yoga fuzzing).
- `linux` / `macos` / `windows` — platform-specific behaviour.
- `headless-ci` — CI without a TTY. Useful for no-TTY probes.
- `docker-linux` — runs in a Docker container from a non-Linux host.

### `harness`

For blackbox work items only. Identifies the existing test infrastructure to use. See `docs/review-playbook/blackbox-harness.md` for the catalogue.

### `duration`

- `fast` — a few minutes at most. Can run inline during a review session.
- `medium` — 5–30 minutes. Runs in the worker, but the worker should not block on it if async execution is possible.
- `long` — over 30 minutes (soaks, multi-hour stress tests). Planner should set `defer: runbook` and emit a runbook work item instead.

### `techniques`

Array of technique numbers from the referenced review file. Only those techniques are in scope for this work item. If a technique number is not listed, the worker does not run it even if it looks applicable.

### `budget`

- `input-tokens` — soft cap on context used for this item.
- `output-tokens` — soft cap on findings output length.
- `wall-clock` — hard cap on elapsed time.

The worker tracks these and writes a continuation note if any cap is hit.

### `findings-prefix`

Prefix for finding files emitted by this work item. Under the autoloop
pipeline, the worker writes each finding to its per-run scratchpad:

```
{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-<slug>.md
```

The `review-dedup` hat then promotes surviving findings into the
committed path:

```
docs/review-playbook/runner/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-<slug>.md
```

Plus one marker file per work-item:

```
{{STATE_DIR}}/findings/<findings-prefix>-<YYYYMMDD>-<HHMM>-done.md
```

(or `continuation.md` if budget was exhausted, or `runbook.md` if
`defer: runbook`).

Ralph and manual runners that skip dedup may write straight to
`docs/review-playbook/runner/findings/` — the naming convention is the
same.

### `depends-on`

Other work-item ids that must finish before this one. Usually empty. Non-empty when a blackbox work item depends on a code work item's output (e.g. "only probe the functions flagged as candidates by the code review").

### `defer`

- `null` — execute normally.
- `runbook` — do not execute. Instead, the worker writes a runbook at `output` describing how a human should run the probe and what to look for. Used for long-duration probes and for probes that need manual interaction (e.g. "drag the terminal from full width down to 1 column").

### `knight-rider`

- `false` (default) — work item does not use Knight Rider.
- `true` — the work item benefits from interactive TUI testing via the Knight Rider sidecar. When running in CI, the blackbox worker should use the Knight Rider HTTP API at `http://localhost:3001` to drive the TUI, observe screen state, and capture evidence frames. If Knight Rider is not available (build skipped, boot failed), fall back to the non-interactive technique path.

## Finding lifecycle

Every finding file carries a `status` field. The canonical values are:

- `open` — active finding. Workers always emit findings with `status: open`.
- `resolved` — validated as fixed. Set by the `review-validate` hat
  when the evidence no longer holds against the current source. The
  validator also sets `resolved-at` (ISO-8601 UTC) and
  `resolved-at-commit` (git HEAD at validation time).
- `duplicate` — merged into another finding. Set by the `review-dedup`
  hat when a new finding is a duplicate of an existing one. The
  existing finding keeps `status: open` and gets a back-reference in
  its `## Related` section; the duplicate either (a) is dropped
  entirely before it reaches the committed tree, or (b) is kept for
  audit with `status: duplicate` and `duplicate-of: <existing-id>`.
  The dedup hat logs every such decision in
  `docs/review-playbook/runner/findings/_dedup-log.md`.
- `wontfix` — manually marked as intentional or accepted. A human
  sets this; no automated hat writes `wontfix`. Validate skips
  `wontfix` findings when re-checking evidence.
- `needs-human-check` — validate could not determine autonomously
  whether the finding is resolved. Set by the `review-validate` hat
  when the grep fast-path fails and the LLM fallback is also
  uncertain (e.g. the file was moved, the pattern is descriptive
  prose, the evidence is a stack trace that no longer has a direct
  mapping to the current source). Validation reports surface these
  for human triage; they are not silently downgraded.

Companion frontmatter fields:

- `duplicate-of: <finding-id>` — set when `status == duplicate`.
- `corroborated-by: [<work-item-id>, ...]` — set by dedup when an
  ENRICH decision merges evidence from another finding into this one.
- `resolved-at: <ISO-8601 UTC>` — set when `status == resolved`.
- `resolved-at-commit: <git-sha>` — set when `status == resolved`.

Transitions are one-way except for human intervention: workers only
write `open`; dedup only writes `duplicate`; validate writes
`resolved` or `needs-human-check`; humans write `wontfix` and may
re-open any finding by editing the file back to `status: open`.

## Examples

### Example A — code review, directory partition

```yaml
---
id: 01-hooks-code
review: 01-async-render-path
kind: code
partition: directory
scope:
  include: ["packages/tui/src/hooks/**"]
  exclude: ["**/__tests__/**"]
platform: any
harness: null
duration: fast
techniques: [1, 2, 3, 4]
budget: { input-tokens: 40000, output-tokens: 6000, wall-clock: 20m }
findings-prefix: 01-hooks-code
depends-on: []
defer: null
---
```

### Example B — blackbox probe, platform-specific

```yaml
---
id: 04-ssh-disconnect-blackbox-linux
review: 04-dead-fds
kind: blackbox
partition: platform
scope:
  include: ["packages/tui/scripts/probes/ssh-disconnect.ts"]
  exclude: []
platform: linux
harness: ad-hoc
duration: medium
techniques: [9]
budget: { input-tokens: 10000, output-tokens: 4000, wall-clock: 15m }
findings-prefix: 04-ssh-disconnect-blackbox-linux
depends-on: []
defer: null
---
```

### Example C — blackbox probe, deferred runbook

```yaml
---
id: 01-long-resize-soak-blackbox-macos
review: 01-async-render-path
kind: blackbox
partition: platform
scope:
  include: []
platform: macos
harness: ad-hoc
duration: long
techniques: [8]
budget: { input-tokens: 6000, output-tokens: 4000, wall-clock: 240m }
findings-prefix: 01-long-resize-soak-blackbox-macos
depends-on: []
defer: runbook
---
```

The worker sees `defer: runbook` and writes a single runbook file at
`findings/01-long-resize-soak-blackbox-macos-<date>-<time>-runbook.md`
describing the 4-hour soak procedure, memory budgets, and what to record.
No probe runs.

### Example D — blackbox via CI matrix

```yaml
---
id: 10-clipboard-blackbox-ci-matrix
review: 10-cross-platform
kind: blackbox
partition: platform
scope:
  include: ["packages/tui/scripts/probes/clipboard.ts"]
platform: windows
harness: ci-matrix
duration: medium
techniques: [19]
budget: { input-tokens: 8000, output-tokens: 4000, wall-clock: 30m }
findings-prefix: 10-clipboard-blackbox-windows
depends-on: []
defer: null
---
```

The worker dispatches `.github/workflows/review-playbook.yml` with `probe=clipboard platforms=windows`, waits for completion, downloads artifacts, and emits one finding file per discrepancy found in the artifacts, using the `findings-prefix` plus date/time/slug naming.