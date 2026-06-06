/**
 * Vitest configuration dedicated to lite-UI coverage measurement.
 *
 * Why a separate config (instead of extending vitest.config.ts):
 *   - The default `vitest.config.ts` runs `*.vitest.{ts,tsx}` files only and
 *     scopes coverage to `hooks/**` + `selectors.ts`. That's the right
 *     scope for the React-reconciler-blind-spot use case it was created for.
 *   - The lite UI's existing tests use the `*.test.ts` naming convention
 *     and are picked up by `bun test` today. We want to ALSO run them under
 *     vitest specifically to instrument their coverage — without changing
 *     the bun test path or risking the existing vitest run's behavior.
 *   - Keeping the two configs separate means a regression in one can't
 *     silently affect the other, and the lite-coverage job in CI can be
 *     run independently.
 *
 * Scope (matches docs/COVERAGE.md "Lite UI Coverage" section):
 *   - `src/lite/**`                      — pure-logic helpers (render,
 *                                            diff, tips, verbose,
 *                                            blank-rules)
 *   - `src/components/layout/lite/**`    — layout components + their pure
 *                                            helpers (static-flush,
 *                                            queue-preview, subagent-kill,
 *                                            boot-indicator, etc.)
 *
 * The `coverage.include` glob makes vitest report 0% for files we have
 * NOT imported via tests yet — so the percentage is honest. Without
 * include, untested files would silently drop out of the denominator.
 *
 * `coverage.all: true` is implied by setting `include` — vitest scans
 * the include glob at startup and instruments every match. The React
 * .tsx component bodies will mostly show as low coverage (they're not
 * unit-tested, they're exercised by integ + e2e tests which don't pipe
 * lcov data here). That's the honest story for the report.
 */
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // Discover the existing lite tests. They're named *.test.ts (not
    // *.vitest.ts) because they were originally run under bun:test.
    // Vitest's API is compatible with the `import from 'vitest'` calls
    // these files already make, so they run unchanged here.
    include: [
      'src/lite/__tests__/**/*.test.{ts,tsx}',
      'src/components/layout/lite/__tests__/**/*.test.{ts,tsx}',
    ],
    // Skip verbose.test.ts: it imports `from 'bun:test'` and uses
    // bun-test specific lifecycle to redirect KIRO_HOME at module-load
    // time. Vitest can't resolve `bun:test`, so it fails to load the
    // file. The bun pass in scripts/lite-coverage.sh covers verbose.ts
    // directly and merges the resulting lcov into the lite report.
    exclude: ['**/verbose.test.ts'],
    // Lite-pure unit tests are deferred to a follow-up PR. Tolerate
    // an empty include glob so this report-only job doesn't block PRs
    // during the ramp-up. Once tests land, we can flip this back to
    // false to enforce non-empty coverage.
    passWithNoTests: true,
    environment: 'node',
    // Pin chalk to truecolor BEFORE production modules build their
    // chalk.hex(...) wrappers at import time. See file header for the
    // ES-hoisting reason. Without this, four render/diff tests assert
    // truecolor SGR but receive chalk's auto-detected boot level (0–1
    // under bun-spawned vitest in non-TTY).
    setupFiles: ['./src/lite/__tests__/setup-chalk-level.ts'],
    coverage: {
      provider: 'v8',
      reportsDirectory: './coverage/lite',
      // lcov for the parsing script + CI summary; text for local-run
      // visibility; json-summary so machines (or future tooling) can
      // consume the totals without re-parsing lcov.
      reporter: ['text', 'lcov', 'json-summary'],
      // Scope: ONLY pure-logic .ts files. .tsx React component bodies
      // (LiteLayout, LiteLiveRegion, ApprovalPrompt, ...) are out of
      // scope here — vitest's V8 coverage in the node env cannot
      // instrument React reconciler code, so they would always show
      // near-0% even when fully exercised by integ/e2e tests
      // (lite-*.test.ts under e2e_tests/ and integ_tests/, which run
      // outside this lcov pipeline). Rather than dragging the
      // percentage down with unmeasurable code, we omit them
      // entirely. The package-wide bun config does the same thing for
      // the same reason — see docs/COVERAGE.md.
      include: [
        'src/lite/**/*.ts',
        'src/components/layout/lite/**/*.ts',
      ],
      // Exclude test files, type-only barrels, and the trivial index
      // re-export from the denominator. They'd otherwise either dilute
      // the percentage (index.ts at 100% is a freebie) or pollute it
      // (test files measuring themselves).
      exclude: [
        '**/__tests__/**',
        '**/*.test.{ts,tsx}',
        '**/index.ts',
      ],
    },
  },
});
