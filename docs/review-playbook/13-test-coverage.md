# Review 13 — Test coverage for all classes above

**Why this class matters.** Most of the bugs fixed in the last 100 commits were hard to reproduce manually and easy to regress. The only durable defence is a test that turns red when the bug returns. The class of gap is: **a known-bad input or known-bad platform that has no regression test**.

**Scope.** All test code, all test harnesses, and all CI configuration. This covers unit tests (`*.test.*`), integration tests (`integ_tests/`), end-to-end tests (`e2e_tests/`), vitest suites (`*.vitest.*`), benchmarks, harness scripts (`scripts/` that orchestrate tests), changelog fragments under `.changes/`, and every workflow under `.github/workflows/`.

Concrete starting points: the test harness inventory in [blackbox-harness.md](blackbox-harness.md), the `.changes/` directory for historical fixes, and the existing CI matrix. New harnesses (future chaos-runner, future multi-OS matrix job) must be inventoried the same way.

## Techniques

1. **[code] Changelog-to-test cross-reference.** For every entry in `.changes/` tagged `fixed` that mentions memory, CPU, OOM, spiral, leak, resize, crash, Windows, macOS, Linux, type, schema, exception, or error, search for a matching test. Missing tests are findings.

2. **[code] Flake audit.** Bugs often surface as flakes before they surface as outages. Grep PR history for "flaky" fixes and check whether the underlying cause was resource, platform, or type related. Flakes without a root-cause note are followup work.

3. **[code] Cross-platform CI matrix.** Every bun/TUI test workflow must include `runs-on: [ubuntu-latest, macos-latest, windows-latest]`. Any single-OS workflow covering user-runtime code is a finding.

4. **[blackbox] Bounded-resource test pattern.** The canonical shape: call the suspect function N times with hostile input and assert `process.memoryUsage().rss < BUDGET` and wall time < DEADLINE. Extend this pattern to every utility identified in Reviews 2, 3, 5.

5. **[blackbox] Smoke-run in CI.** A 60 s smoke session that exercises resize, streaming, paste, cancel, and exit — with a memory budget. If the budget is exceeded, CI fails. Run on all three OS runners.

6. **[blackbox] Fuzz targets.** For each recursive or width-sensitive utility, write a short fuzz loop (random width 0 to 200, random text length 0 to 10 000, random ANSI noise) run under a time budget. Failures captured as corpus entries.

7. **[blackbox] Schema round-trip tests.** For every external boundary in Review 11, a round-trip test: send a recorded real payload, parse, re-serialize, deep-equal. Changes in the SDK will then fail here.

8. **[blackbox] Fault-injection tests.** For each boundary in Review 12, a test that forces the failure mode (closed socket, malformed JSON, missing config, ENOSPC on write) and asserts the TUI handles it without crashing and surfaces the error to the user.

9. **[blackbox] Property-based tests for invariants.** Use `fast-check` or similar to generate inputs for pure functions identified in any review (text wrap, markdown parse, path canonicalize, color convert) and assert invariants (length bounds, idempotency, round-trip equality). Shrinking reveals minimal failing cases.

10. **[blackbox] Chaos day exercise.** Once per quarter, run a half-day session where reviewers deliberately break things (unplug network, kill subprocesses, corrupt config files, resize rapidly) and note failure modes. Each finding becomes a fault-injection test.

## What to record

Fix, PR, test exists?, test name, coverage (unit / e2e / smoke / matrix).

## Done criteria

Every PR in the `.changes/` history that fixed a bug has at least one regression test identified by PR number in a code comment. CI matrix covers all three platforms. Fuzz targets and property-based tests run in CI.
