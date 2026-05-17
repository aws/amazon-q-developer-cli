# Review 12 — Exception handling, error propagation, and failure observability

**Why this class matters.** The TUI runs at user-facing priority — a thrown exception that reaches the top level crashes the session, loses in-flight work, and produces a confusing trace. Worse are silent swallows: `catch {}` blocks that hide real failures, `Promise.catch(() => undefined)` that makes a network error look like an empty response, or a try/catch that eats errors but leaves the app in a half-updated state. Exception-handling bugs also directly interact with the dead-FD class (Review 4) and the concurrency class (Review 7): a handler that writes to stdout from an `uncaughtException` creates the #1808 loop; a reducer that half-updates and throws leaves the store desynced.

**Scope.** Every control-flow path where a failure can happen: any `try`/`catch`/`throw`, any `.catch(` / `.then(..., onRejected)`, every global handler (`uncaughtException`, `unhandledRejection`, `window.onerror`, error boundaries), every async function call that the caller does not await, every retry loop, every boundary between layers where an error might cross. Also every external-system call whose failure could propagate — SDK calls, filesystem reads, subprocess launches, network requests, IPC messages.

Concrete starting points: handlers registered at TUI start-up (`packages/tui/src/index.tsx`), error boundaries in the component tree, and any module that interacts with an external system. New error-handling surfaces (a future retry wrapper, a new crash-reporting path, a new user-facing error surface) must be reviewed the same way.

## Techniques

1. **[code] Silent-swallow census.** Grep for empty catch blocks and `.catch(() => {})`. Each silent swallow is a finding unless accompanied by a comment justifying why the error is recoverable without logging. "Best-effort" comments are acceptable only with a one-line reason.

2. **[code] Logged-but-ignored scan.** Grep for catch blocks followed by `console.` or `logger.` only — no rethrow, no user surface, no state rollback. Is the caller expecting success? If yes, the log is not enough; the state must be rolled back or the caller informed.

3. **[code] Error-type audit.** `catch (err)` where `err` is implicitly `unknown` in modern TS. For each, verify the code that reads `err.message` or `err.code` narrows via `err instanceof Error` or similar. Accessing fields on `unknown` without narrowing is a finding.

4. **[code] Custom-error hierarchy audit.** List every custom `Error` subclass. Each should set `this.name`; each should preserve cause (`{ cause: original }`); each should be distinguishable at the catch site by `instanceof`, not by string-matching the message.

5. **[code] Rethrow-preserving-cause audit.** `throw new Error('wrapper: ' + e.message)` loses the stack. Prefer `throw new Error('wrapper', { cause: e })`. Grep for the anti-pattern.

6. **[code] `uncaughtException` handler audit.** The handler must not do anything that can itself throw or write to a dead stream (see Review 4, #1808). Must log to file (if possible) and `process.exit(1)` with a finite timeout.

7. **[code] `Promise.all` vs `allSettled` audit.** `Promise.all` rejects on the first failure and cancels the others. `allSettled` completes everything. Grep for `Promise.all(` and verify the rejection semantics are intentional at each call site.

8. **[code] AbortSignal propagation audit.** Every long-running async operation (fetch, timer, stream read) should accept an `AbortSignal`. Grep for `new AbortController`, `signal:` param, and `fetch(` without a signal. Operations with no cancellation path keep running after the user cancels, wasting CPU and sometimes causing races.

9. **[code] State-rollback audit.** For every try/catch around a store mutation, verify that partial updates are rolled back on throw. Grep for `store.set(` / `setState(` inside try blocks; trace the catch path. Half-updated state is a correctness bug that often surfaces only after the user retries.

10. **[code] User-surfacing audit.** Every error that reaches the user must appear in the TUI's inbox or notification area with actionable text, not just a stack trace in the log file. Grep for `logger.error` that has no matching user-surface path; grep for user-surface strings that include raw `err.stack`.

11. **[code] Error-translation audit.** Errors crossing layer boundaries should be translated: e.g. an ACP SDK error should not leak into a view component as `AgentError: ECONNREFUSED`. List the layers (view, controller, client, transport) and verify each layer catches and remaps.

12. **[code] Retry-budget audit.** For each retry loop, verify: a maximum attempt count, an exponential backoff, and an explicit giving-up path. Grep for `while (true)` and `for (;;)` in async code. An unbounded retry loop on a persistent failure is a spiral.

13. **[code] Global-handler registration audit.** Verify `process.on('uncaughtException', ...)` and `process.on('unhandledRejection', ...)` are both registered exactly once, early in startup, before any async work. Multiple registrations in different modules is a smell; missing registration on one of them is a crash.

14. **[code] `finally` correctness.** Grep for `finally {` and verify the block is side-effect safe: no `return` that swallows a throw, no `await` that delays a fatal propagation, no resource-close that could itself throw without being caught.

15. **[blackbox] Unhandled-rejection runtime scan.** ESLint `no-floating-promises` is the static half; this is the runtime half. Register a `process.on('unhandledRejection', ...)` that exits in dev and run the full e2e suite — any rejection becomes a test failure.

16. **[blackbox] Fault-injection probe.** For each external boundary (ACP connection, config file, IPC socket, MCP server), manually inject failures: close the socket mid-response, return malformed JSON, delete the config file between reads, hang the subprocess. The TUI must degrade gracefully: clear error surfaced to the user, logs written, session recoverable.

17. **[blackbox] Disk-full probe.** Fill the disk (or `ulimit -f` to a small size) and run a session that writes logs, snapshots, and config. Every write path must either succeed with degraded behaviour (skip logging) or surface the error — never corrupt the file or crash.

18. **[blackbox] Network-fault matrix.** For each network boundary (auto-update check, telemetry, MCP server, embedded cloud features), simulate: DNS failure, connection refused, TLS handshake failure, slow response (30 s delay), truncated response, HTTP 500, HTTP 429 rate limit. Every case must surface a specific error to the user and not block the TUI.

19. **[blackbox] Filesystem-permissions probe.** Set the config directory to `chmod 000` mid-session and perform an operation that writes config. The TUI must surface a permission error, not crash and not silently lose the change.

20. **[blackbox] Process-death probe.** For each subprocess (agent, MCP server, shell), kill it mid-operation (`kill -9`). The TUI must detect the death, surface it, and allow re-spawn without leaked resources.

21. **[blackbox] Retry-exhaustion probe.** For each retry loop, force persistent failure and assert the loop terminates within the budget. An unbounded retry loop would spin forever — the probe should have a hard deadline to catch that.

22. **[blackbox] Crash-replay corpus.** Collect crash reports (user-submitted stack traces) and write a replay test for each one. Each becomes a regression test and documents a known failure mode. This is the exception-handling equivalent of Review 2's yoga OOM corpus.

## What to record

Site, error shape, handling mode (swallow, log, rethrow, user-surface, rollback, exit), test coverage, severity if misclassified.

## Done criteria

Grep for empty catch blocks returns only sites with explanatory comments. Every external-boundary error has a user-surface path. `no-floating-promises` is enforced. Fault-injection probes pass for every boundary in the scope. Network-fault matrix and process-death probe pass.
