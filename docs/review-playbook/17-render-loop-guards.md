# Review 17 — Render Loop Guards

**Why this class matters.** A single trigger — a resize event, a store update, a layout measurement — can amplify into an infinite re-render loop that pegs the CPU at 100% or exhausts memory until OOM kills the process. The root cause is always the same: mutation without invariant maintenance. A `useLayoutEffect` measures a DOM element, calls `setState` unconditionally, which triggers a re-render, which re-measures, which calls `setState` again — forever. PR #1761 (`5eb517922`) fixed a SIGWINCH handler that lacked a dimension-equality check, causing yoga to allocate unboundedly on every resize. PR `72082c152` fixed StatusBar calling `setLineCount` on every measure without comparing to the previous value. PR `0a1af2163` broke a resize oscillation where a scrollbar appearing changed the width, triggering another resize. The hard rule: every `setState` reachable from a layout effect or event handler must be guarded by a value-changed check. No exceptions.

**Scope.** Any component that can trigger its own re-render through measurement, subscription, or event handling. This includes `useLayoutEffect`/`useEffect` callbacks that call `setState`, resize/SIGWINCH handlers that propagate dimensions into state, Zustand selectors that return new object references on every render, and components subscribing to high-frequency store values (full input strings, streaming token counts). Concrete starting points: `packages/tui/src/hooks/useTerminalSize.ts`, `packages/tui/src/components/StatusBar.tsx`, any component using `measureElement`, and all Zustand `useAppStore` selectors in `packages/tui/src/store/`. New components that measure layout and write results to state — for example, a future split-pane resizer or dynamic toolbar — must be reviewed the same way.

## Techniques

1. **[code] Audit setState inside useLayoutEffect/useEffect.** Search for all `setState` calls reachable from layout effects: `rg -n "useLayoutEffect|useEffect" packages/tui/src/ -l` then inspect each hit for unconditional `set` calls. Every `setState` must be wrapped in a guard: `if (newValue !== prevValue)` or equivalent ref comparison. An unguarded `setState` inside a layout effect is a finding.

2. **[code] Verify dimension-unchanged guards in resize handlers.** Find all resize/SIGWINCH handlers: `rg -n "resize|SIGWINCH|onResize|useTerminalSize" packages/`. Each handler that propagates dimensions into state must compare `newWidth === prevWidth && newHeight === prevHeight` before calling `setState`. A handler that sets dimensions unconditionally is a finding — this is exactly what caused the yoga OOM in `5eb517922`.

3. **[code] Audit Zustand selectors for new-object-reference-per-render.** Find all `useAppStore` calls: `rg -n "useAppStore\(" packages/tui/src/`. Any selector that returns a computed object or array (e.g., `(state) => ({ a: state.x, b: state.y })`) creates a new reference every render, defeating React's shallow equality check. Flag selectors that don't use `shallow` from `zustand/shallow` or don't select a single primitive. PR `caae3d926` fixed typing lag caused by `commandInputValue` triggering full-tree re-renders.

4. **[code] Identify components subscribing to high-frequency store values.** Find components that select values changing on every keystroke or streaming token: `rg -n "inputValue|streamingContent|tokenCount|messageText" packages/tui/src/`. Components that subscribe to the full string when they only need a boolean (e.g., `hasInput = input.length > 0`) are over-subscribing. Each such subscription is a finding — derive the minimal selector.

5. **[code] Check for setState during render.** Search for state updates outside effects: `rg -n "set[A-Z].*\(" packages/tui/src/components/` and cross-reference with component body (not inside `useEffect`/`useLayoutEffect`/event handlers). PR `32bfd047f` fixed a stack overflow where `setApprovalScroll(0)` was called during render via a ref comparison pattern. Any `setState` in the render body that isn't behind a strict conditional is a finding.

6. **[code] Verify React.memo on hot-path children.** Identify components rendered inside lists or streaming views: conversation message lists, tool output panels, status indicators. If a parent re-renders on every token and children lack `React.memo` (or have unstable props like inline objects/callbacks), the entire subtree re-renders. Check with `rg -n "export (default )?function|export const" packages/tui/src/components/` and verify memo wrapping on frequently-rendered components.

7. **[blackbox] Rapid resize storm.** Resize the terminal 50 times in 2 seconds (script: `for i in $(seq 50); do printf '\e[8;$((20+i));80t'; sleep 0.04; done`). Measure: render count via React DevTools profiler or instrumented `console.count` in the root component. Pass criteria: render count ≤ 55 (at most 1.1× the stimulus count). CPU must return to idle within 1 s of last resize. If renders exceed 2× stimulus or CPU stays above 20% for more than 2 s, it is a finding.

8. **[blackbox] Long conversation with streaming — render frequency.** Open a conversation with 200+ messages and trigger a streaming response. Measure render frequency of the root TUI component during streaming. Pass criteria: renders per second ≤ 30 (matching typical frame budget). If renders exceed 60/s or the process exceeds 150% CPU during streaming, it is a finding. Use `why-did-you-render` or a patched `useState` wrapper to log render triggers.

9. **[blackbox] Scrollbar oscillation probe.** Set terminal width to exactly the threshold where content wraps (typically 1 character wider than the longest line). Trigger content that toggles the scrollbar visibility. Observe for 5 s. Pass criteria: layout stabilizes within 3 frames. If width oscillates indefinitely (scrollbar appears → content shrinks → scrollbar disappears → content expands → repeat), it is a finding — this is the pattern from `0a1af2163`.

10. **[blackbox] Idle CPU after state settlement.** After any user action (send message, resize, cancel), measure CPU at T+2 s. Pass criteria: process CPU ≤ 5% when idle. If the process sustains >10% CPU with no user input, a render loop or polling cycle is active — trace with `--inspect` and identify the re-render source.

## What to record

Component name, trigger (resize/effect/selector), guard present?, renders-per-stimulus ratio, CPU at idle after trigger, PR reference if previously fixed.

## Done criteria

Every `setState` inside a `useLayoutEffect` or `useEffect` in `packages/tui/src/` has a value-changed guard. No Zustand selector returns a new object reference without `shallow` comparison. All four blackbox probes pass with renders-per-stimulus ≤ 1.1× and idle CPU ≤ 5%.
