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
