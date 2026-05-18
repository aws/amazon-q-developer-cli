# Review 8 — Bundler, SDK, and dependency footprint

**Why this class matters.** A new SDK version, a new dependency, or a bundler config change can silently introduce large allocations, new async primitives, or polyfills that reintroduce bugs the team has already fixed. The recent ACP SDK upgrade is an example — the underscore-prefix change silently broke ext-method handlers and went unnoticed until runtime.

**Scope.** Anything that ships into the built user binary. This covers every runtime dependency declared in a `package.json` under `packages/`, every transitive dependency that is not tree-shaken out, the bundler configuration itself, the pinned bun version, any vendored dependency (for example twinki), and any global polyfill or side-effectful import.

Concrete starting points: lockfiles (`bun.lock`), the `dependencies` and `peerDependencies` blocks of each package's `package.json`, the bundle output in `packages/*/dist/`, and the pinned-bun scripts. Any future new dependency (an MCP-server framework, a telemetry SDK, a clipboard library, a markdown parser) must be reviewed the same way.

## Techniques

1. **[code] Dependency diff review.** On every `package.json` change, diff the new dependency's changelog for the range of versions being skipped. Flag: stream changes, error-handling changes, event-emitter changes, renames of methods, or underscore or dollar-sign naming conventions.

2. **[code] SDK surface-area audit.** For each third-party SDK used at runtime, list the functions called and the expected semantics (sync vs async, returns Promise vs value, throws vs returns error). On upgrade, re-verify each. Consider snapshot-testing the SDK surface so drift is visible.

3. **[code] Bundle-size diff.** Run `bun build` before and after the upgrade; diff the bytecode size and the unminified output's top-level definitions. Large unexpected additions are findings.

4. **[code] Tree-shake verification.** Grep the bundled output for symbols that should be dead-code-eliminated. SDKs with side-effectful imports can smuggle in polyfills.

5. **[code] Polyfill audit.** Grep the new dependency's dist files for `globalThis.` assignments or `Symbol.for(` calls — anything the SDK installs globally. Global polyfills can override bun's native implementations.

6. **[code] Changelog for method-name renames.** Specifically look for renames (e.g. `KillTerminalCommandRequest` renamed to `KillTerminalRequest`), prefix conventions (underscore, dollar, hash), and schema tightening (optional fields becoming required, null handling).

7. **[blackbox] Runtime method-call sniff.** Temporarily add a proxy wrapper in the SDK's entry point and log every method called during a 60 s session. Cross-check against documentation. Any method called that is not documented is a finding; any documented method that is never called but the code thinks it is, is also a finding.

8. **[blackbox] Contract replay test.** Record a real session's JSON-RPC traffic (ACP / MCP) against the old SDK version. After upgrade, replay the same traffic through the new SDK and assert equivalent domain-level behaviour. This is the test that would have caught the underscore-prefix regression preemptively.

9. **[blackbox] Per-method smoke run.** After every SDK upgrade, trigger each method the TUI calls at least once in an e2e run: newSession, prompt, cancel, setSessionMode, listSessions, each ext method, each notification handler. Any method that used to work and now errors is a finding.

10. **[blackbox] Startup-cost check.** Measure `time bun run dist/tui.js --version` before and after upgrade. Regressions beyond plus or minus 10% warrant investigation (bloated SDK init, new I/O).

11. **[blackbox] Idle-allocation check.** Start the TUI, wait 30 s in idle, capture a heap snapshot. Diff against the pre-upgrade baseline. New retained objects with an SDK prefix that are not explained are findings.

## What to record

Dependency, version range, surface area touched, behavioural change, mitigation.

## Done criteria

Every upgrade has an attached surface-area audit. Bundle size delta is within plus or minus 5% or explained. Contract replay test and per-method smoke run pass.
