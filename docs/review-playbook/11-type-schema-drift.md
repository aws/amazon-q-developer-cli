# Review 11 — Type drift, schema changes, and structural mismatches

**Why this class matters.** The TUI talks to an agent via the ACP SDK, reads/writes config files, consumes IPC messages from the Rust backend, and exchanges data with MCP servers. Every one of these is a boundary where types are asserted but not always verified. When the other side changes shape — a field renamed, a value that used to be required becomes optional, a `{}` literal becomes a `Record<string, unknown>`, an enum gains a variant, `null` becomes `undefined` — the TypeScript compiler often cannot catch it because the boundary uses `any`, `unknown`, casts, or because the SDK's own types drifted with the change. The underscore-prefix change in the recent ACP upgrade is a canonical example: types compiled, tests passed, runtime was silently broken.

**Scope.** Every boundary between our code and any external system whose shape we do not control. Examples include but are not limited to:
- Protocol SDKs (for example ACP, MCP) — method names, notification payloads, session-update variants, capability negotiation shapes.
- IPC with other processes — the Rust `chat_cli` backend, helper processes, external tools invoked as subprocesses.
- Third-party JSON-RPC / HTTP APIs — MCP servers, telemetry endpoints, auth services, package registries.
- Config files on disk — user settings, agent configs, knowledge-base entries, shell-integration files, session-resume state.
- Command-line args, environment variables, and anything else the user can hand to the process.
- Filesystem layouts we rely on (directory names, well-known file shapes, conventions like `.git/` or `.kiro/`).
- Any `as`, `as any`, `as unknown as X`, or unchecked type predicate at any of the above boundaries.

Any new external boundary added later (a new SDK, a new daemon, a new plugin protocol) must be reviewed the same way.

## Techniques

1. **[code] Cast census.** Grep for `as any`, `as unknown`, `as {}`, `as Record<`, `as SomeType`. Each cast is a place where the compiler is being told to trust a shape. For each, answer: (a) where does the value come from? (b) what enforces the shape? (c) what happens if the shape is wrong at runtime? Unjustified casts are findings.

2. **[code] `any` audit.** Grep for `: any`, `<any>`, `as any` in `packages/tui/src` and `packages/twinki`. `any` disables type checking at that boundary. For each, propose `unknown` plus narrowing, a Zod schema, or an explicit interface.

3. **[code] `@ts-ignore` / `@ts-expect-error` audit.** Every suppression is a known mismatch. List them; each needs a dated comment explaining why the suppression exists and when it will be removed.

4. **[code] `as unknown as X` audit (the "double cast").** Grep specifically for `as unknown as`. This is the pattern used to convert truly unrelated types — always worth a second look, because it is the pattern someone used when the single cast did not compile.

5. **[code] External-type drift scan.** For each third-party SDK, list every type imported. On upgrade, diff the types between versions (compile against old and new; capture the error list). Specifically look for:
   - Renamed types (e.g. `KillTerminalCommandRequest` renamed to `KillTerminalRequest` in the recent ACP upgrade)
   - Renamed string literals (ext-method name changes)
   - Tightened optionality (field was optional becoming required, or vice versa)
   - `null` vs `undefined` swaps (the `_meta?: Record<string, unknown> | null` change)
   - New enum variants that exhaustive-switches do not handle
   - Zod refinements that reject previously-valid input

6. **[code] Exhaustiveness audit.** Every `switch` on a union type should end with a `default` branch that hits a `never` assertion. Grep for `switch (` and inspect each; add the assertion where missing. New SDK enum variants will then fail compilation loudly instead of silently falling through.

7. **[code] Runtime-schema validation.** Every payload received from an external system should be parsed through a runtime schema (Zod, io-ts, or hand-rolled) before use. Grep for `JSON.parse(` and list every site. Each should immediately pipe into a `parse()` call, not into an `as T` cast.

8. **[code] IPC-wire-format audit.** The Rust backend sends typed messages over a Unix socket / named pipe. The TUI receives them as JSON. Cross-reference the Rust struct definitions (`crates/chat-cli-v2/src/`) with the TS types (`packages/tui/src/test-utils/shared/` and wherever IPC is parsed). Any field that exists in one but not the other, or has a mismatched type, is a finding.

9. **[code] Config-migration audit.** When the shape of `~/.kiro/settings/cli.json` changes, existing user configs on disk are in the old shape. Verify each config-reading path (a) tolerates missing fields, (b) tolerates extra fields, (c) has a versioning strategy for incompatible changes. Default values must match the schema.

10. **[code] Env-var shape audit.** Grep for `process.env.`. Every env var is a string; any code that treats it as a boolean, number, or JSON must validate. Prefer a central `parseBoolEnv` helper over ad-hoc comparisons.

11. **[code] Null/undefined discipline.** `strictNullChecks` must be on in `tsconfig`. Verify. Then grep for non-null assertion operator. Each is a boundary where we are telling the compiler to trust us. Review each for a safer alternative.

12. **[code] Empty-object vs record.** `{}` in TypeScript means "any non-null value", not "empty object". Prior fix site earlier in this branch (`args: {} as Record<string, unknown>`) is a live example. Grep for empty-object literals in positions that expect `Record<string, unknown>` and replace with `Record<string, never>` (empty) or the full record type.

13. **[code] Discriminated-union audit.** For each discriminated union (e.g. `AgentStreamEvent`, `SessionUpdate`), verify every variant has a unique discriminator value and that all call sites handle all variants. Grep for `.type ===` / `.kind ===` and audit the branches against the union definition.

14. **[code] Snapshot the SDK surface.** Maintain a `docs/acp-surface.md` or similar that lists every ACP method, notification, and payload field we depend on. On SDK upgrade, diff the doc against the new SDK types.

15. **[blackbox] Serialization round-trip tests.** For every persisted shape (session log, config, knowledge base entry), write a test that creates a value, serializes, deserializes, and asserts deep equality. This catches `Date` becoming string, `Set` becoming array, `undefined` becoming missing-key silent drops.

16. **[blackbox] Wire-payload replay.** Capture real wire payloads (ACP notifications, MCP tool responses, IPC messages) from a live session and store them as fixtures. Replay each through the TUI's parsing layer and assert the resulting domain object matches a frozen snapshot. SDK upgrades that change payload shape will break these tests loudly.

17. **[blackbox] Contract tests against the Rust backend.** Spawn the real `chat_cli acp` process and exercise every message type in both directions. Failures are either a type drift (our TS type does not match what Rust sends) or a regression in Rust. This is stronger than code-side cross-referencing because it exercises the actual wire format.

18. **[blackbox] Malformed-payload fuzz.** For each JSON-parsing entry point, feed fuzzer-generated malformed inputs: truncated JSON, extra fields, wrong types, nulls where strings were expected, arrays where objects were expected, UTF-8 surrogate pairs, very large numbers, numbers-as-strings. The TUI must parse-and-reject, never crash, never silently coerce.

19. **[blackbox] Config-migration replay.** Collect historical config files (from old releases, user bug reports, corrupted-but-recoverable configs) and run each through the config loader. Every one should load cleanly with an explicit migration path or fail with a clear error — never silently lose fields or use default values for fields the user set.

20. **[blackbox] SDK-version compatibility sweep.** Install each supported SDK version (N, N-1, N-2) in a branch and run the full test suite against each. Regressions at any version are findings.

21. **[blackbox] Enum-variant exhaustiveness test.** Maintain a runtime registry of every enum value we expect from the SDK. On session boot, if the SDK emits a value not in the registry, log and surface a warning. This catches new variants added by SDK upgrades that would otherwise fall through `switch` defaults silently.

22. **[blackbox] JSON Schema validation at runtime.** If the SDK publishes a JSON Schema, validate every inbound payload against it in dev builds. Assertion failures are findings — either our SDK pin is wrong or the SDK published an out-of-band change.

## What to record

Boundary, external type source, TS type, validation mechanism, drift risk, test evidence.

## Done criteria

Every external-system boundary has a runtime validator. Every cast to a domain type has an explaining comment or a test. `any` counts are tracked in CI and not allowed to grow. SDK upgrades carry a surface diff. Wire-payload replay and contract tests pass.
