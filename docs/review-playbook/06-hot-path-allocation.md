# Review 6 — Hot-path allocation and re-render amplification

**Why this class matters.** The TUI renders at up to 60 Hz during streaming. An allocation or work-unit that is cheap at 1 Hz becomes a noticeable CPU/GC load at 60 Hz. Prior fixes (#2095, #2172) show the pattern: per-frame work that copies large arrays (messages on every tool-call update) or allocates new strings per cell. The class of problem is: **work proportional to render frequency times state size where state size grows with session length**.

**Scope.** Any code that runs on a per-render basis or in reaction to a high-frequency event. This covers every React/Ink-like component, every store selector / memo / `useEffect` dependency, every render scheduler, every text layout pass, and every animation or cursor-blink tick. Anything whose cost multiplies by the render rate (typically up to about 60 Hz during streaming) qualifies.

Concrete starting points: the component tree under `packages/tui/src/components/`, store-subscription patterns, the path from `store.setState` to `doRender`, and any visualisation component that updates continuously (progress indicators, spinners, live shell output). New high-frequency components (future typing indicators, streaming markdown, live diffs, agent-status meters) must be reviewed the same way.

## Techniques

1. **[code] Selector stability audit.** Every `useSelector` / `useStore((s) => ...)` must return a stable reference when the relevant slice is unchanged. Grep for selectors in `packages/tui/src` then read each. Selectors of the form `s => s.foo.map(...)` allocate a fresh array per render, which is a bug.

2. **[code] `useMemo` / `useCallback` audit.** A missing memo in a component high in the tree can re-render every child per frame. Grep for components passing inline objects or arrays as props. Each is a re-render amplifier.

3. **[code] Message-array write audit.** Find every mutation of the main messages array. Each mutation triggers every subscriber. Tool-call updates, shell output, and notifications should write to side-channels (see `liveOutputs` Map in #2095), not the main array.

4. **[code] String-concat scan in render.** Grep for `+=` and template literals in render functions and text-render utilities. Each can be a source of per-character allocations. Prefer array join or pre-allocated buffers.

5. **[code] Static vs dynamic segregation.** Components that rarely change (headers, footers, completed turns) should live in the static region; components that change per frame should live in the dynamic region. Misclassification causes the whole conversation to redraw per frame. Grep for the static/dynamic split in twinki and audit the classification.

6. **[blackbox] Profile-driven review.** Capture a cpuprofile under normal use (see `docs/bun-performance-analysis.md`). For each function in the top 20, read its source and ask: is the work proportional to session length? If yes, and it runs per render, that is a finding.

7. **[blackbox] Yoga node count per tree.** Add a dev-mode assertion: if nodeCount > 500, warn. Any component tree above 500 nodes is a candidate for collapsing (see Shell component: 1000 to 6 nodes in #2095).

8. **[blackbox] React profiler run.** If a React DevTools bridge is usable, record an interaction trace and look for components that render without prop changes (Profiler "why did this render" column).

9. **[blackbox] Streaming-throughput benchmark.** Simulate 60 Hz of `ToolCallUpdate` events over 60 s and measure: render count, frames dropped, allocations per frame (via `performance.measureUserAgentSpecificMemory` if available, otherwise heapUsed delta). Ratio of renders-to-updates and allocations-per-frame must stay flat as the session accumulates messages.

10. **[blackbox] Idle-CPU soak.** After a long session (30+ turns), stop input and sample `ps -o %cpu` once per second for 60 seconds. Idle CPU must drop below 5% within 10 s. If it stays elevated, there is a timer or re-render loop firing without user input (the exact shape of the 50 to 65% stuck CPU observed in `docs/bun-performance-analysis.md`).

11. **[blackbox] Per-frame allocation budget.** In a scripted session, wrap each `doRender` in a `performance.memory.usedJSHeapSize` delta. The budget is project-specific — record it; regressions against the baseline are findings.

## What to record

Function or component, per-frame cost, what state it depends on, amplifier (prop instability / missing memo / array mutation), fix direction.

## Done criteria

No component re-renders solely because of live streaming output. No selector returns a fresh reference for unchanged input. Top 20 in the cpuprofile does not include any function whose self-time scales with session length. Streaming benchmark and idle-CPU soak pass.
