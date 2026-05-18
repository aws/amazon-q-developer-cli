# Review 1 — Async callbacks in the resize / render / stream-flush path

**Why this class matters.** `setTimeout`, `setInterval`, `queueMicrotask`, `setImmediate`, and Promise continuations inside resize handlers, SIGWINCH callbacks, render paths, or stream flush callbacks have repeatedly caused memory spirals. The pattern: an async callback interleaves with backed-up stdout writes (PTY buffer flush on tmux attach, dead TTY, background session), producing a cascade of renders or writes that grows memory at hundreds of MB/s until OOM. The hard rule established by prior fixes is: **no `setTimeout`/`setInterval` in the resize or render path**. Throttles must be leading-edge synchronous or use a throttle primitive that cannot queue state.

**Scope.** Anywhere a callback or handler runs in response to a terminal event, a stream event, or an async I/O completion, and then writes to the screen or mutates render state. This includes (but is not limited to) resize handlers, SIGWINCH listeners, stdout/stdin `data`/`drain`/`end`/`error` listeners, render scheduling (`requestRender`, `scheduleRender`, RAF-like throttles), reconciler commit phases, IPC message handlers, and any code that bridges an event-emitter event to React state.

For concrete starting points, inspect the resize path (`useTerminalSize`, `process-terminal.ts`, `tui.ts` in the twinki renderer) and any file that imports throttle/debounce utilities. New files that match the pattern — for example, a future "useStreamingOutput" hook or a new input-device listener — must be reviewed the same way.

## Techniques

1. **[code] Static grep for timers on hot paths.**
   - `rg -n "setTimeout|setInterval|setImmediate|queueMicrotask" packages/tui/src packages/twinki/packages/twinki/src`
   - For each hit, walk the call graph backwards. If any caller is a resize handler, render loop, stream flush, or SIGWINCH listener, flag it.

2. **[code] Event-listener audit.** Grep for `.on('resize'`, `.on('error'`, `.on('data'`, `.on('drain'`, `process.stdout`, `process.stderr`, `SIGWINCH`, `SIGPIPE`. For each listener, read the body and check whether it schedules async work (Promise chain, timer) or calls into the render pipeline.

3. **[code] Throttle/debounce audit.** `rg -n "throttle|debounce|es-toolkit" packages/tui packages/twinki`. Any throttle wrapping a render-path function must (a) be leading+trailing only if strictly needed, (b) use a library that documents it does not queue state between invocations, and (c) have a comment citing the reason (usually PR #2137 / #2150 / #2172).

4. **[code] Callback depth check.** For each resize-path function, count how many `await` points exist between the SIGWINCH event and the yoga `calculateLayout` call. Zero is safe. One or more means the next resize event can interleave.

5. **[blackbox] Tmux attach/detach probe.** Instrument `ProcessTerminal.onResize` and `requestRender` with monotonic counters and memory snapshots. Run the TUI inside tmux; attach and detach repeatedly; verify render count does not grow geometrically per attach.

6. **[blackbox] Resize storm test.** Script a loop that fires 1 000 SIGWINCH events in 10 seconds (via `kill -WINCH $PID` on POSIX, `SetConsoleWindowInfo` on Windows). Memory growth during and after should be bounded within plus or minus 20 MB of baseline.

7. **[blackbox] Backed-up PTY probe.** Launch the TUI under a PTY wrapper that delays reads by 500 ms. Trigger a resize. The TUI must not queue more than N renders behind the slow reader (tune N; the principle is that any unbounded queue is a finding).

8. **[blackbox] Long-session resize soak.** Run the TUI for 4 hours inside tmux, with an external script that sends a resize every 30 s and a prompt every 5 min. RSS at hour 4 must be within plus or minus 100 MB of RSS at hour 1.

## What to record

For each finding: the handler, the async primitive used, the interleaving scenario (detached tmux, dead TTY, rapid resize), whether there is a dimension guard in front of it, and the fix direction (make synchronous, add guard, or remove).

## Done criteria

Every async primitive in `packages/tui/src/hooks/useTerminalSize.ts`, `process-terminal.ts`, and `tui.ts` has been justified in writing. No unjustified `setTimeout`/`setInterval` exists in the resize or render path. All four blackbox probes pass.
