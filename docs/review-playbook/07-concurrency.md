# Review 7 — Concurrency, ordering, and race conditions

**Why this class matters.** The TUI coordinates several event sources: ACP notifications from the agent, SIGWINCH, stdin reads, React state updates, timer-driven animations, IPC sockets. Races between these sources have caused cursor desync, duplicated renders, and dropped updates. While most are correctness bugs rather than resource bugs, a race that causes repeated writes or re-renders becomes a resource bug.

**Scope.** Any code where two or more logical actors (event emitters, async handlers, IPC messages, timers, user input, render ticks) can read or write overlapping state. This covers store reducers, message-stream handlers, notification queues, cross-session state, any `await` between a read and a write, any handler that may be re-entered before a previous invocation finishes, and any Promise chain that can interleave with another.

Concrete starting points: `acp-client.ts` and anything that subscribes to agent notifications, the message-stream handler, any IPC socket handler, hook `useEffect` bodies, and any timer-driven animation. New coordination surfaces (a future multi-agent orchestrator, a shared MCP-server pool, a distributed-session feature) must be reviewed the same way.

## Techniques

1. **[code] Reentrancy audit.** Every handler that reads from the store and writes back must be reentrancy-safe. Grep for `store.set` or `setState` inside handlers that also call `store.getState()`. A second event arriving before the first completes can observe stale state and write conflicting updates.

2. **[code] Async-in-reducer scan.** Store reducers should be synchronous. Any `await` in a reducer is a finding. State updates must happen in a single atomic swap.

3. **[code] Promise-swallowing scan.** Grep for `.catch(() => {})`, `try { await ... } catch {}`, `.then(..., () => {})`. Each silenced error is a potential silent failure that leaks the handler promise.

4. **[code] Unawaited-Promise scan.** Grep for call sites of async functions whose returned promise is discarded. The `no-floating-promises` ESLint rule catches these; verify the rule is on. If it is off, turn it on and audit.

5. **[code] Lock-free invariants.** For each shared mutable piece of state, document which actors can read it and which can write it. If more than one actor can write, a lock or a single-writer discipline must exist.

6. **[blackbox] Ordering probe.** Simulate out-of-order delivery of ACP notifications (SessionUpdate before NewSession, ToolCallUpdate before ToolCall). The TUI should not crash, double-render, or drop events. If it does, there is a missing ordering guarantee.

7. **[blackbox] Delivery-delay fuzz.** Wrap the ACP transport in a proxy that randomly delays each notification by 0 to 500 ms. Run the e2e suite. Any test that becomes flaky under random delays reveals an ordering bug.

8. **[blackbox] Concurrent-operation stress.** Issue 100 parallel operations (cancel, prompt, session-switch, resize) and assert the final state is consistent, no duplicated events, no lost events.

9. **[blackbox] Rapid-cancel fuzz.** Send a prompt, cancel it within 10 to 500 ms (random), send another prompt immediately. Repeat 200 times. State must remain consistent — no orphan tool calls, no duplicated responses, no hung handlers.

10. **[blackbox] Store-invariant assertions.** In dev builds, wrap each reducer with an assertion that pre/post invariants hold (messages length monotonic within a session, tool-call map keys are a superset of referenced IDs, etc.). Run the e2e suite; any broken invariant is a finding.

## What to record

Shared state, actors, ordering assumption, failure if assumption breaks.

## Done criteria

Every reducer is synchronous. Every async call site either awaits or explicitly documents fire-and-forget. ESLint `no-floating-promises` is enforced. Delivery-delay fuzz and concurrent-operation stress pass.
