# Bun/TUI Code-Safety Review Playbook

A systematic review playbook for finding classes of bug in the bun-side code (`packages/tui/`, `packages/twinki/`, and any JavaScript/TypeScript that runs under bun at runtime) that recent history has shown tend to reach users: resource exhaustion (CPU, memory, stack), cross-platform regressions, schema/type drift against external systems, and unsafe exception handling.

This document is organised as a menu of **focused, independently executable reviews**. Each review lives in its own file, targets one class of problem, lists multiple techniques for finding it, and defines what "done" looks like. The reviews are independent: you can run them sequentially over days, delegate each to a separate subagent, or parallelise them. Thoroughness beats speed.

## How to use this playbook

1. **Scope.** Unless a review explicitly narrows it, the default scope is **all runtime TypeScript / JavaScript code that ships into the user binary**. That includes everything under `packages/tui/`, vendored code under `packages/twinki/`, any future package that the build pulls into the binary, and any script that is bundled into the release. Tests, harnesses, and the Rust backend are out of scope for most reviews (tests are covered separately in Review 13; the Rust side has its own review conventions).

   **Do not artificially narrow the scope to the files mentioned in each review.** Those files are illustrative anchors — the concrete places where the problem has bitten us before. Reviews target a class of code pattern. Apply the techniques anywhere the pattern appears, including in files that do not exist yet.

   **Excluded from most reviews** (covered separately):
   - Generated files (anything under a `generated/` directory).
   - Test files (`__tests__/`, `*.test.*`, `integ_tests/`, `e2e_tests/`) — covered by Review 13.
   - Build artifacts (`dist/`, `build/`).
   - `node_modules/` unless auditing a new dependency (Review 8).

2. **Pick a review.** Each file below is self-contained. Start with whichever class you suspect has the highest blast radius, or follow the suggested run order.

3. **Follow the techniques.** Each review lists several techniques. Use as many as are useful. Techniques are deliberately overlapping — different techniques catch different instances of the same class.

4. **Record findings.** For each candidate issue, capture: file:line, the technique that found it, the class of problem, a brief description, severity, and a proposed fix. Group similar findings.

5. **Propose fixes, don't make them.** Unless explicitly asked, the output of a review is a report, not code changes. Fixes are prioritised and scheduled separately.

6. **Re-run after fixes.** Each review is idempotent. After fixes land, re-run the relevant techniques to confirm the class of problem has not regressed.

7. **Cross-platform matters everywhere.** Windows, macOS, and Linux each have subtly different behaviour for signals, paths, line endings, processes, permissions, and terminals. Several reviews are explicitly cross-platform; where they are not, still mentally ask "does this assume POSIX?" as you work through the techniques.

8. **Use code and blackbox techniques together.** Every review includes both:
   - **Code techniques** (tagged `[code]`) — static analysis, greps, type/lint checks, reading source. Cheap, reproducible, low coverage (they only find what we know to look for).
   - **Blackbox techniques** (tagged `[blackbox]`) — runtime probes, fault injection, fuzzing, real-user scenario tests. Expensive, flakier, high coverage (they find bugs the code review missed, including the ones we didn't anticipate).

   You can run a code-only pass on CI and a blackbox-only pass on a test rig. Both passes are required for "done". If a blackbox technique calls for a probe and you are not sure where to start, see [blackbox-harness.md](blackbox-harness.md) — it maps every technique class to an existing test harness and explains how to run each cross-OS.

## Severity rubric

- **Crash**: process exits unexpectedly, throws to top level, or corrupts state.
- **Spiral**: unbounded resource growth that continues without external intervention (the user cannot recover without killing the process).
- **Slowdown**: bounded but noticeable degradation (redundant work, extra allocations, avoidable re-renders).
- **Regression**: works on one platform / terminal / locale, fails on another.
- **Smell**: suspicious pattern that works today but could regress into one of the above under small changes.

## Reviews

| # | File | Topic |
|---|------|-------|
| 1 | [01-async-render-path.md](01-async-render-path.md) | Async callbacks in resize / render / stream-flush path |
| 2 | [02-yoga-layout.md](02-yoga-layout.md) | Yoga layout edge cases |
| 3 | [03-unbounded-growth.md](03-unbounded-growth.md) | Unbounded growth in long-running state |
| 4 | [04-dead-fds.md](04-dead-fds.md) | Dead file descriptors, broken streams, closed terminals |
| 5 | [05-recursion-stack.md](05-recursion-stack.md) | Recursion and stack growth |
| 6 | [06-hot-path-allocation.md](06-hot-path-allocation.md) | Hot-path allocation and re-render amplification |
| 7 | [07-concurrency.md](07-concurrency.md) | Concurrency, ordering, race conditions |
| 8 | [08-dependency-footprint.md](08-dependency-footprint.md) | Bundler, SDK, dependency footprint |
| 9 | [09-terminal-tty.md](09-terminal-tty.md) | Terminal, TTY, environment edge cases |
| 10 | [10-cross-platform.md](10-cross-platform.md) | Cross-platform compatibility (Windows / macOS / Linux) |
| 11 | [11-type-schema-drift.md](11-type-schema-drift.md) | Type drift, schema changes, structural mismatches |
| 12 | [12-exception-handling.md](12-exception-handling.md) | Exception handling, error propagation, failure observability |
| 13 | [13-test-coverage.md](13-test-coverage.md) | Test coverage for all classes above |
| 14 | [14-process-lifecycle.md](14-process-lifecycle.md) | Process lifecycle and orphan prevention |
| 15 | [15-state-machine-transitions.md](15-state-machine-transitions.md) | State machine transitions and flag lifecycle |
| 16 | [16-terminal-protocol-symmetry.md](16-terminal-protocol-symmetry.md) | Terminal protocol symmetry (enable/disable pairing) |
| 17 | [17-render-loop-guards.md](17-render-loop-guards.md) | Render loop guards (measure→setState cycles) |
| 18 | [18-input-handler-exclusivity.md](18-input-handler-exclusivity.md) | Input handler exclusivity (competing key handlers) |

Cross-cutting techniques and the suggested run order live in [cross-cutting.md](cross-cutting.md). The blackbox harness guide (how to actually run the runtime techniques — existing test harnesses, cross-OS execution, kiro-cli self-driving patterns, evidence capture) lives in [blackbox-harness.md](blackbox-harness.md). For turning this playbook into executable work items that sub-agents can run one at a time, see [runner/](runner/README.md).

## Quick triggers

Run a subset of the playbook on common triggers:

- **SDK upgrade** (e.g. ACP, MCP): Reviews 11 (type drift), 8 (dependency footprint), 1 (async).
- **Bun version bump**: Reviews 1, 2, 6 (async, yoga, hot paths).
- **Before a Windows release**: Reviews 10 (cross-platform), 4 (signals/FDs), 9 (terminal).
- **After a user-reported crash or spiral**: Reviews 12, 4, 5 (exceptions, dead FDs, recursion).
- **Before enabling a new agent model or MCP server type**: Reviews 11, 12 (type drift, error handling).

Run the playbook in full at least once per quarter.

## Why these reviews exist

Each review targets a class of bug that has **repeatedly shipped to users**. The following analysis is based on the last 1000 commits in `packages/`, of which 524 were bug fixes. The reviews are grouped by the generalized failure pattern they detect.

### Generalized Pattern: Asymmetric Lifecycle

**Any resource/state that is acquired must be released on every exit path (success, error, cancel, signal, crash).**

| Review | Historical bugs | Example fixes |
|--------|----------------|---------------|
| **01 — Async render path** | 9+ timer/setImmediate leaks | `b9543eeb9` — clear Wordmark setInterval after animation; `25e3930a8` — clear isProcessing on cancel |
| **04 — Dead FDs** | 8+ orphaned process bugs | `e8eff8f83` — prevent orphaned bun processes on terminal close (3-layer fix: SIGHUP, kill_on_drop, stdin EOF); `5512ad349` — SIGTERM handler threw on dead PTY |
| **09 — Terminal/TTY** | 6+ protocol state leaks | `b12c56364` — suspend/resume Kitty protocol on Ctrl+Z; `ea5ca9ae0` — restore terminal modes after shell escape |
| **07 — Concurrency** | 10+ state-machine flag bugs | `667ba719f` — clear pending_prompt_response on cancel; `34556e8b5` — drain message queue after compaction |

### Generalized Pattern: Mutation Without Invariant Maintenance

**Any mutation to shared state must maintain all dependent invariants (cursors, selectors, layout measurements).**

| Review | Historical bugs | Example fixes |
|--------|----------------|---------------|
| **02 — Yoga layout** | 7+ render loop bugs | `72082c152` — prevent infinite re-render in StatusBar (measureElement → setLineCount unconditionally); `5eb517922` — dimension guard to prevent resize cascade and yoga OOM |
| **03 — Unbounded growth** | 5+ cursor/index desync bugs | `217ab6d7c` — static item trimming desyncs twinki cursor; `62a764e34` — trim-reappend cycle from deleted IDs |
| **06 — Hot-path allocation** | 7+ unnecessary re-render bugs | `caae3d926` — typing lag from commandInputValue selector causing full tree re-render; `0a1af2163` — break resize oscillation loop |

### Generalized Pattern: Handler Exclusivity / Event Routing

**At any point in time, exactly one handler should own a given event.**

| Review | Historical bugs | Example fixes |
|--------|----------------|---------------|
| **07 — Concurrency** (input races) | 5+ competing handler bugs | `d29df4a02` — approval trust selection always treated as rejection (InlineLayout Enter raced Menu onSelect); `628821997` — prevent global Esc from cancelling approval when panel showing |

### Generalized Pattern: Boundary Crossing Fidelity

**Data/state that crosses a boundary (serialize, spawn, IPC) must be validated at the boundary.**

| Review | Historical bugs | Example fixes |
|--------|----------------|---------------|
| **11 — Type/schema drift** | 5+ encoding roundtrip bugs | `208716b4f` — decode `&amp;` last to prevent double-unescaping; `624d84827` — unescape literal `\\n` in command history |
| **10 — Cross-platform** | 5+ environment inheritance bugs | `ff2947e22` — reset fd soft limit before spawning shell commands on macOS (Python overflows); `1b5b30058` — set USERPROFILE for Windows |
| **08 — Dependency footprint** | 4+ singleton duplication bugs | `3a399c3b2` — pin twinki react to 19.2.4 to prevent dual-React bundle crash |

### Additional patterns

| Review | Historical bugs | Rationale |
|--------|----------------|-----------|
| **05 — Recursion/stack** | 3+ stack overflow bugs | `32bfd047f` — stack overflow in CrewMonitorScreen from setApprovalScroll(0) during render |
| **12 — Exception handling** | 8+ swallowed error bugs | `a5bcd940d` — stdout error circuit breaker (process.stdout.on('error') prevents infinite loop when PTY dies) |
| **13 — Test coverage** | N/A | Ensures the above classes have regression tests |

### Linter checks (mechanical detection)

Two bug classes are better caught by static analysis than LLM review:

| Check | Plugin | Bugs caught | Status |
|-------|--------|-------------|--------|
| **React hooks ordering** | `eslint-plugin-react-hooks` (`rules-of-hooks`) | 5+ commits (e.g. `41a536633` — useMemo after early return) | ✅ Added as `warn` |
| **Exhaustive deps** | `eslint-plugin-react-hooks` (`exhaustive-deps`) | 10+ stale closure bugs (e.g. `08c998975` — dropped characters from stale segments ref) | ✅ Added as `warn` |
| **Singleton alignment** | Manual check (`bun.lock` for duplicate React) | 4+ commits (e.g. `3a399c3b2` — dual-React crash) | Covered by Review 08 |

### Proposed future reviews (not yet written)

Based on the commit analysis, these additional review classes have high bug density but aren't fully covered by the existing 13:

| Proposed review | Bug count | Key pattern |
|----------------|-----------|-------------|
| `process-lifecycle` | 8+ | Orphaned child processes (partially covered by Review 04) |
| `state-machine-transitions` | 10+ | Flags not cleared on all exit paths (partially covered by Review 07) |
| `terminal-protocol-symmetry` | 6+ | Enable without matching disable on suspend/crash (partially covered by Review 09) |
| `render-loop-guards` | 7+ | useLayoutEffect → setState → re-render cycles (partially covered by Review 02) |
| `input-handler-exclusivity` | 5+ | Multiple handlers for same key (partially covered by Review 07) |

These are partially covered by existing reviews but may warrant dedicated playbook files if the existing reviews don't catch them reliably.
