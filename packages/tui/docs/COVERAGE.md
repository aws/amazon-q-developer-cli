# Coverage Strategy

## Goal

90% line coverage across the TUI package.

## Shared Configuration

`coverage-config.json` is the single source of truth for all coverage exclusion
patterns. It is consumed by:

- `src/test-utils/coverage-preload.ts` (imported directly)
- `scripts/combined-coverage.sh` (read via `jq`)
- `bunfig.toml` (manually synced; TOML cannot import JSON)

## Why Two Test Runners

The TUI uses **two** test runners because of a measurement limitation in bun's
V8 coverage engine:

- **Bun** runs pure-logic tests: stores, utils, commands, types, constants, and
  theme modules. Bun's built-in coverage accurately instruments these files.
- **Vitest** runs React reconciler code: hooks, selectors, and components. Bun's
  V8 coverage cannot track code that executes inside React's reconciler loop, so
  those files would show 0% even when fully tested under bun.

Both runners produce lcov output that is merged into a single report by the
`scripts/combined-coverage.sh` script.

## What's Excluded from Bun Coverage

These patterns are defined in `coverage-config.json` and applied in
`bunfig.toml` (`coveragePathIgnorePatterns`):

| Pattern | Reason |
|---------|--------|
| `**/dist/**` | Build output |
| `**/node_modules/**` | Third-party code |
| `**/twinki/**` | Separate package with its own coverage |
| `**/ink/**` | Vendored Ink fork, tested upstream |
| `**/renderer.ts` | Ink renderer bootstrap, no testable logic |
| `**/test-utils/**` | Test infrastructure, not production code |
| `**/e2e_tests/**` | E2E test infrastructure |
| `**/components/**` | Bun V8 coverage blind spot inside React reconciler |
| `**/hooks/**` | Hooks use React reconciler, covered by vitest |
| `**/contexts/**` | React contexts, covered by vitest |
| `**/kiro.ts` | App entry point with side effects |
| `**/acp-client.ts` | ACP client, heavy side effects |

## Preload Mechanism

`src/test-utils/coverage-preload.ts` is an opt-in preload script that reads its
exclusion list from `coverage-config.json`. It is **not** enabled by default in
`bunfig.toml` because it lowers the overall coverage percentages reported by bun
(uncovered files appear as 0%). Use it when generating comprehensive combined
coverage reports.

It uses `Bun.Glob` to scan all `*.ts` and `*.tsx` files under `src/` and
dynamically imports each one. This forces bun's V8 coverage engine to "see"
every production file so that untested files appear as 0% in the report rather
than being silently omitted.

Import failures are expected (many files depend on packages that may not be
installed in every environment) and are silently caught.

## Running Coverage

```bash
# Bun coverage (stores, utils, commands, types, constants, theme)
# Use --preload for comprehensive coverage with zero-coverage visibility
cd packages/tui && bun test --preload ./src/test-utils/coverage-preload.ts

# TUI vitest coverage (hooks, selectors, components)
cd packages/tui && npx vitest run --coverage

# Twinki vitest coverage
cd packages/twinki && npx vitest run --coverage

# Merge all three
cd packages/tui && bash scripts/combined-coverage.sh
```

Bun writes its lcov to `coverage/lcov.info` automatically. Vitest lcov files
should be configured to write to `coverage/vitest-lcov.info` (TUI) and
`coverage/lcov.info` (twinki) respectively.

## LCOV Merging

`scripts/combined-coverage.sh` reads filter patterns from `coverage-config.json`
and merges coverage from all three sources while avoiding double-counting:

1. **Bun lcov** -- records for `hooks/`, `selectors`, `components/`, `contexts/`,
   `kiro.ts`, and `acp-client.ts` are filtered out (vitest covers those).
2. **TUI vitest lcov** -- records for `.test.` and `.spec.` files are filtered
   out (test infrastructure).
3. **Twinki vitest lcov** -- records for `wrap-ansi-optimized` are filtered out.

The filtered streams are concatenated into `coverage/combined-lcov.info`. The
script then parses the combined file to compute total and hit line counts and
prints a summary with the current percentage and gap to the 90% target.

Missing lcov files are skipped with a warning, so the script works even when
only a subset of runners has been executed.


## Lite UI Coverage

Independent of the package-wide percentage, the lite UI has its own scoped
coverage report — produced by `scripts/lite-coverage.sh` and surfaced in CI
by the `tui-lite-coverage` job. It exists because the lite UI is on a
separate release track and the team wants to track its progress against the
90% goal without it being averaged into the broader package number.

### Scope

The "lite UI" is exactly two directories:

| Path | Contents |
|------|----------|
| `src/lite/**` | Pure-logic helpers (render, diff, tips, verbose, blank-rules) |
| `src/components/layout/lite/**` | Layout components and their helpers (LiteLayout, LiteLiveRegion, static-flush, queue-preview, subagent-kill, boot-indicator, etc.) |

Excluded from the denominator: `__tests__/`, `*.test.{ts,tsx}`, the trivial
`index.ts` re-export. Source-of-truth scope is the `coverage.include` glob
in `vitest.lite-coverage.config.ts`; the matching scope regex in
`scripts/lite-coverage.sh` (`SCOPE_RE`) must stay in lockstep.

### Two Runners, One Report

Same dual-runner reasoning as the package-wide setup:

1. **vitest** (`vitest.lite-coverage.config.ts`) runs every lite test that
   imports `from 'vitest'`. Coverage `include` covers the full lite scope so
   files with no test yet show as 0% — keeps the percentage honest.
2. **bun test** (one focused invocation on `src/lite/__tests__/verbose.test.ts`)
   runs the one lite test that imports `from 'bun:test'` for KIRO_HOME
   redirection. Vitest cannot load the file; bun produces real coverage data
   for `verbose.ts` directly.

The script filters both lcov outputs to the lite scope, then merges with
DA-line dedup (max hit count per line per file), recomputes `LF`/`LH` per
file, and prints the percentage. Same merge algorithm as the package-wide
`scripts/combined-coverage.sh`.

### What's In Scope

The script measures **`.ts` files only** under the two lite directories
above — pure-logic helpers, blank-rules, render, diff, verbose, tips,
static-flush, queue-preview, subagent-kill, boot-indicator, and the
single hook (`usePendingSwap.ts`).

**`.tsx` files are deliberately out of scope.** React component bodies
(LiteLayout, LiteLiveRegion, ApprovalPrompt, LiteSubagentPanel,
LiteTaskTray, SubagentFooter, ConnectingPanel) cannot be instrumented by
vitest's V8 coverage in the `node` environment — the reconciler runs the
component code in a way V8 attribution can't see, so those files would
always show near-0% even when fully exercised. Including them would
produce a misleading number and make the report look like there's a huge
test gap when there isn't. The package-wide bun coverage excludes
`**/components/**` for the same reason — see the upstream sections of
this doc.

The lite React components are covered by 10+ integ/e2e tests
(`lite-*.test.ts` under `e2e_tests/` and `integ_tests/`). That coverage
is real and behavioral, it just doesn't flow through this lcov pipeline.

If we ever want measurable component-line coverage, the path is a
jsdom-backed vitest config + React Testing Library tests — out of scope
for this report and a separate workstream.

### Running Locally

```bash
cd packages/tui

# Full output
bash scripts/lite-coverage.sh

# Summary only (suppresses test runner output)
bash scripts/lite-coverage.sh --quiet
```

Outputs:

- `coverage/lite/lcov.info` — vitest's lite-scoped lcov
- `coverage/lite/merged-lcov.info` — vitest + bun, filtered + merged
- `coverage/lite/coverage-summary.json` — vitest summary (machine-readable)
- `coverage/lite/lcov-report/` — vitest HTML report (browsable line-by-line)

### CI Behavior

The `tui-lite-coverage` job in `.github/workflows/tui.yml` runs on every PR
that touches `packages/tui/**` or `packages/twinki/**`. It:

- Calls `scripts/lite-coverage.sh`
- Uploads the merged lcov files as a build artifact (14-day retention)
- Surfaces a markdown table in the GitHub Actions job summary panel showing
  both numbers, the goal, and the gap

**Report-only mode:** The script always exits 0, so the job never blocks
PRs. To switch to a hard gate, change the script's exit policy or add an
explicit threshold check in the workflow step. The current baseline is
intentionally below 90% on the full scope; ratcheting up the gate
prematurely would block every PR until tests catch up.
