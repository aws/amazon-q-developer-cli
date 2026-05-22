# Review 3 — Unbounded growth in long-running state

**Why this class matters.** The TUI runs for hours or days per session. Any collection that grows monotonically with user activity (messages array, session history, tool-call map, subscriber set, notification queue) is a potential unbounded-memory source. The static-item trimming desync (#2101) is a sub-case: trimming exists to bound memory, but a bug in the trim interaction silently dropped new messages. Both failure modes matter.

**Scope.** Any mutable state that outlives a single render or a single turn: store slices, singletons, module-level caches, event-handler registries, subscription sets, logger ring buffers, notification queues, recently-opened-file lists, and any persisted session/history state. Also any trim/evict utility that bounds such state.

Concrete starting points: files under a `stores/` or `state/` directory, anything exported as a module-level `Map` or `Set`, any `class ... { private events = new EventEmitter() }`, and any `trim*`/`evict*`/`prune*` helper. New long-lived caches in future code (completion providers, tool-result caches, MCP-server registries) must be reviewed the same way.

## Techniques

1. **[code] Long-lived container census.** Grep for top-level or store-level declarations: `new Map(`, `new Set(`, `: Array<`, `: Record<`, `Record<string`. For each, document the policy: does it grow per message / per turn / per session? Is there a trim/evict? What bounds the size?

2. **[code] Subscription leak scan.** Find every `.on(`, `.addEventListener`, `.subscribe`, `addHandler`, `onUpdate` call. Each must have a matching `.off` / `removeEventListener` / `unsubscribe` / `removeHandler` reachable from the same owner (component unmount, session end, process exit). Unpaired registrations leak closures.

3. **[code] Stream-buffer scan.** Find any place the TUI stores streaming output (`liveOutputs`, `messages`, `toolCalls`, `sessionLog`). Each must (a) have a per-item cap, (b) have an overall cap, or (c) stream to a file. Flag anything that appends indefinitely.

4. **[code] Trim-cursor invariant audit.** Every trim utility must document the invariant relating the trimmed array's length to any downstream cursor. Grep for `splice`, `slice(`, `shift(`, `pop(` across stores and assert the invariant is enforced. Read `trim-static-items.ts` and the `adjustStaticCursor` call site as the reference pattern.

5. **[code] WeakMap/WeakRef audit.** For caches keyed by a DOM-ish node or React element, check whether `WeakMap`/`WeakRef` would be more appropriate than `Map`. Long-lived `Map<Node, X>` entries prevent node GC.

6. **[code] Config-driven caps.** Verify every cap (MAX_EXPANDED_LINES, KIRO_MAX_STATIC_ITEMS, etc.) has an env-var override so reviewers can reproduce under stress.

7. **[blackbox] Session-lifetime probe.** Run the TUI for 60 minutes with a scripted turn loop. Sample `process.memoryUsage().rss` and open-FD count every 30 seconds; fit a linear regression through the RSS series after a 10-minute warm-up. Any sustained slope ≥ 1 MB/min or total RSS growth ≥ 50 MB is a finding. FD growth ≥ 5 over baseline is a finding. Implemented by `packages/tui/scripts/probes/session-lifetime.ts`.

8. **[blackbox] Heap snapshot diff.** Take pre and post heap snapshots around a workload (30 min for deep runs, 5 min default, 30 s in CI) via the TUI's test-mode IPC `HEAP_SNAPSHOT` command. Force GC before each snapshot, bucket nodes by constructor name, and compare `self_size` totals. Any constructor that grew more than 100 % (from a ≥ 64 KB baseline) or total heap Δ ≥ 50 MB is a finding; the probe retains the `.heapsnapshot` files for Chrome DevTools inspection. Implemented by `packages/tui/scripts/probes/heap-snapshot.ts`.

9. **[blackbox] Subscription stress test.** Spawn and tear down at least 100 TUI sessions in rapid succession. Measure probe-process RSS and FD after each teardown plus each child's peak RSS. Monotonic growth in probe resources, drift > 10 % in child steady-state RSS, or any child requiring SIGKILL is a finding. Note: this is a cross-process proxy for in-process listener audit; a direct `.listenerCount()` reading requires extending the test IPC with a `LISTENER_COUNTS` command. Implemented by `packages/tui/scripts/probes/subscription-stress.ts`.

10. **[blackbox] Forced-trim probe.** Drive a scripted turn loop well past the known trim thresholds (`MAX_SESSION_MESSAGES=50` and `MAX_HISTORY_SIZE=1000`). Default: 250 turns at 50 ms spacing. Fit RSS slope in the pre-threshold window `[0, threshold]` and the post-threshold window `[threshold, 2×threshold]`. Trim engaged cleanly only if the post-threshold slope is ≤ 10 % of the pre-threshold slope; RSS at 2×threshold must not exceed 2× RSS at threshold. Direct store-size observation would require either logger.debug at trim sites or an `ARRAY_SIZES` IPC; until then, RSS is the proxy. Implemented by `packages/tui/scripts/probes/forced-trim.ts`.

## What to record

Container, owner, growth rate, bound (hard cap / trim policy / none), leak risk.

## Done criteria

Every long-lived container has a documented bound. A 60-minute session run with scripted input shows RSS stable within plus or minus 50 MB after warmup.
