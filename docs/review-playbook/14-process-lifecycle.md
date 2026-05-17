# Review 14 — Process lifecycle and orphan prevention

**Why this class matters.** When the parent process dies unexpectedly (SIGKILL, terminal close, OOM kill, crash), child processes become orphans reparented to init/launchd. Without explicit lifecycle management, these orphans spin at 100%+ CPU indefinitely — invisible to the user but consuming resources until the machine is rebooted or the process is manually killed. The generalized pattern is **Asymmetric Lifecycle**: any spawned process must be cleaned up on ALL exit paths, not just the happy path. This class produced at least 8 distinct production fixes: `e8eff8f83` (3-layer fix: SIGHUP in tokio::select, kill_on_drop, stdin EOF), `97b89b9cd` (SIGTERM/SIGINT in tokio::select + uncaughtException handler), `5512ad349` (SIGHUP handler threw on dead PTY, preventing exit), `5f5628e8c` (try/catch in cleanup, removed console.log to dead stdout), `a5bcd940d` (stdout error circuit breaker), `ff2947e22` (reset fd soft limit before spawning shell commands), `bf2676965` (remove FFI dlopen/getppid that crashes without JIT entitlement), and `e5aa52be0` (preserve caller's cwd when launching TUI).

**Scope.** Any code that spawns a child process — `Command::new()`, `tokio::process::Command`, `std::process::Command`, `spawn()`, `fork()`, `exec()`, or equivalent — regardless of what it launches. This covers the bun TUI subprocess, shell escapes (`!vim`, `!bash`), MCP server processes, subagent workers, language servers, and any future spawned process (e.g., a background indexer, a file-watcher daemon, or a remote tunnel helper). Concrete starting points: any file containing `Command::new` or `.spawn()` in the Rust codebase, the TUI process launcher, MCP server spawn logic, and shell-escape handlers. New code that spawns a process — for example a background build watcher or a credential-helper subprocess — must be reviewed the same way.

## Techniques

1. **[code] Spawn-site census with kill_on_drop verification.** `rg -n "Command::new\|\.spawn()" --type rust` across the entire workspace. For each site, verify that either `kill_on_drop(true)` is set on the `Child`, or there is explicit cleanup logic that kills the child on every exit path (normal return, error, panic, signal). A spawn without kill_on_drop and without manual cleanup on all paths is a finding.

2. **[code] Signal handler completeness audit.** `rg -n "SIGHUP\|SIGTERM\|SIGINT\|SIGCHLD\|signal::ctrl_c\|tokio::signal" --type rust`. For each spawned process, verify that the parent handles at minimum SIGTERM, SIGINT, and SIGHUP (POSIX) by killing the child before exiting. Missing any one of these three is a finding. On macOS/Linux, also check SIGTSTP/SIGCONT for suspend/resume correctness.

3. **[code] Cleanup-writes-to-dead-stdout check.** Read every signal handler and cleanup/shutdown function. If it writes to `stdout`, `stderr`, or any PTY-connected stream, flag it. When the terminal is closed, stdout is a dead fd — writing to it throws (Rust: BrokenPipe; Node: EIO/EPIPE), which prevents the cleanup from completing. The fix pattern is: wrap cleanup IO in try/catch or use `let _ = write!(...)` to discard errors. Reference: `5f5628e8c`, `5512ad349`.

4. **[code] Stdin EOF detection for parent death.** When the parent dies, the child's stdin pipe closes. The child must detect EOF on stdin and exit promptly. `rg -n "stdin\|BufReader\|read_line\|AsyncBufReadExt" --type rust` — for each child process that reads from stdin, verify there is an EOF check that triggers shutdown. A child that ignores stdin EOF will survive parent death indefinitely.

5. **[code] Process-group and session leadership.** `rg -n "setsid\|setpgid\|process_group\|pre_exec\|CommandExt" --type rust`. Verify that long-running children are placed in the parent's process group (so they receive the parent's signals) OR that the parent explicitly kills them. Children in a new session (`setsid`) will NOT receive the parent's SIGHUP — if that's intentional, there must be an alternative cleanup mechanism.

6. **[code] Platform-specific kill semantics.** On Windows, `kill()` sends `TerminateProcess` which has no graceful-shutdown equivalent. Verify that any cross-platform spawn code either (a) uses a platform-appropriate shutdown mechanism (e.g., closing stdin to signal exit on Windows) or (b) documents that hard-kill is acceptable. `rg -n "cfg.*windows\|target_os" --type rust` near spawn sites.

7. **[blackbox] SIGKILL parent, verify no orphans.** Start kiro-cli, let it spawn its child processes (TUI, MCP servers). Send `kill -9 $PARENT_PID`. Wait 5 seconds. Run `pgrep -P 1 | xargs ps` (or `ps aux | grep kiro\|bun\|node`) and verify zero orphaned children remain. Any surviving child process is a critical finding.

8. **[blackbox] Terminal tab close probe.** Launch kiro-cli in a terminal tab (iTerm, Terminal.app, Windows Terminal, tmux pane). Close the tab/pane (not Ctrl+C — use the window manager). Wait 5 seconds. Verify all child processes have exited. Repeat for at least two different terminal emulators. Surviving processes are findings.

9. **[blackbox] Rapid spawn/kill cycle.** Script a loop that starts kiro-cli, waits 2 seconds, then kills it (SIGTERM). Repeat 20 times. After all iterations, verify zero orphan processes remain and no zombie (Z state) processes exist. This catches cleanup races where the kill arrives during child startup.

10. **[blackbox] Suspend/resume with children.** Start kiro-cli, press Ctrl+Z to suspend. Verify child processes are also stopped (SIGTSTP propagated). Run `fg` to resume. Verify children resume. Then kill the parent while suspended (`kill %1` from another shell). Verify children do not survive.

## What to record

Spawn site (file:line), child process type, kill_on_drop present?, signal handlers covering this child (list), stdin EOF detection present?, blackbox probe result (orphan survived? yes/no).

## Done criteria

Every `Command::new` / `.spawn()` site has either `kill_on_drop(true)` or documented equivalent cleanup on all exit paths. All signal handlers (SIGTERM, SIGINT, SIGHUP) kill spawned children before exiting. No cleanup code writes to stdout without error handling. The SIGKILL-parent and terminal-close blackbox probes produce zero orphans.
