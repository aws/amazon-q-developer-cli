# Review 4 — Dead file descriptors, broken streams, and closed terminals

**Why this class matters.** When a user closes the terminal, suspends the process (`Ctrl-Z`), disconnects an SSH session, or the parent shell exits, `process.stdout` becomes a dead file descriptor. `write()` on a dead fd emits an async `'error'` event (EIO, EPIPE, EBADF). Without a listener, Node escalates to `uncaughtException`, and if the handler writes to stdout too, you get the exact infinite loop that caused the #1808 TTY memory spiral. The class of problem is: **asynchronous stream errors propagating into handlers that themselves write to the broken stream**.

**Scope.** Any writable stream, pipe, socket, or file descriptor that the process writes to after start-up, plus every signal handler, `uncaughtException` handler, `unhandledRejection` handler, and process-exit hook. This covers `process.stdout` / `process.stderr` / `process.stdin`, subprocess pipes (`spawn` / `exec` / `fork`), Unix domain sockets and Windows named pipes, TCP/UDP sockets if any, `fs.createWriteStream` sinks, and the bridge between any event-emitter and those streams.

Concrete starting points: the top of the TUI entry (`packages/tui/src/index.tsx`) for global handlers, any file that calls `spawn` / `fork` / `child_process`, and any IPC file (the `TuiIpcConnection` / `AcpTestHelper` family and the Rust-side bridge). Any future stream — for example a new telemetry pipe, a log-forwarding socket, or a remote-agent transport — must be reviewed the same way.

## Techniques

1. **[code] Error-listener census.** Every writable stream accessed at runtime must have an `.on('error', ...)` listener registered before first write. Grep for `process.stdout`, `process.stderr`, `stdin`, `socket.write`, `proc.stdio`, `.pipe(` in `packages/tui`. For each target, verify there is an error listener. The circuit-breaker in `packages/tui/src/index.tsx` for `stdout` is the reference pattern.

2. **[code] uncaughtException handler audit.** Read every `process.on('uncaughtException', ...)` and `process.on('unhandledRejection', ...)` handler. The handler must not write to the potentially-dead stream. Ideal: log to file then `process.exit(1)`. Anti-pattern: call `cleanup()` which does `console.error` / `stdout.write`.

3. **[code] Signal-handler audit.** `SIGINT`, `SIGTERM`, `SIGHUP`, `SIGPIPE`, `SIGWINCH`, `SIGCHLD`. For each, check the handler is idempotent and does not hold locks or schedule work that survives process exit. `SIGPIPE` in particular should not throw.

4. **[code] Subprocess stdio audit.** For every `spawn(...)`, confirm `stdio` configuration matches how the code reads the pipes. A pipe the parent never drains will fill the kernel buffer and block the child — a different class of deadlock but the same root failure mode (unbalanced IO). See `execute_cmd` backlog in PR #0fed79952 for the Rust side; the TS side has the equivalent risk via `spawn().stdout?.on('data', ...)`.

5. **[code] Platform-specific signal audit.** `SIGPIPE` and `SIGHUP` do not exist on Windows; `SIGINT` is delivered via a different mechanism. Any handler registered for these must check `process.platform !== 'win32'`. Grep for `process.on('SIG` and cross-reference each signal against Node's signal-event docs. Missing platform guards are findings.

6. **[code] Subprocess-kill audit.** `subprocess.kill('SIGTERM')` on Windows only kills the immediate child, not its tree. For every `spawn` that launches a shell command, verify there is a tree-kill strategy (`taskkill /T /F` on Windows, process-group kill on POSIX).

7. **[blackbox] Back-pressure probe.** Simulate a slow consumer by wrapping `process.stdout` in a stream that delays writes by 100 ms. Run the TUI. The renderer must stop enqueuing writes once `write()` returns `false` (high-water mark). If it keeps pushing, that is a finding.

8. **[blackbox] Suspend/resume probe.** Run the TUI, press `Ctrl-Z`, then `fg`. The TUI should resume cleanly. Repeat 50 times. No memory growth, no orphan process. (POSIX only; on Windows the equivalent is window-minimize / restore and `CTRL_BREAK_EVENT`.)

9. **[blackbox] SSH-disconnect probe.** Run the TUI over SSH, kill the SSH client. The TUI process should exit cleanly within about 5 s with exit code 1. It must not hang or grow memory.

10. **[blackbox] FD-exhaustion probe.** `ulimit -n 16` then start the TUI. Every file/socket open must either succeed with reuse or fail with a surfaced error — never crash the process or leak an fd.

11. **[blackbox] Terminal-close probe.** Launch the TUI inside a terminal emulator, then close the emulator window (not the process). The TUI must exit within 5 s with a clean exit code. Repeat for iTerm, Terminal.app, Windows Terminal, gnome-terminal, and tmux-detached-then-killed.

12. **[blackbox] Write-to-closed-fd fuzz.** Close `process.stdout` programmatically mid-render (via a test fixture) and assert the TUI exits via the circuit breaker rather than looping. This is the exact test that would have caught #1808 preemptively.

## What to record

Stream, owner, error handler present?, handler writes back to same stream?, probe result.

## Done criteria

Every writable stream has an error listener. Every `uncaughtException` / `unhandledRejection` handler writes only to file or `process.exit`. The suspend/resume, SSH-disconnect, terminal-close, and write-to-closed-fd probes all pass.
