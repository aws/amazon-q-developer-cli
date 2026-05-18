# Review 16 — Terminal protocol symmetry

**Why this class matters.** Terminal protocols — Kitty keyboard protocol (CSI u), bracketed paste mode, modifyOtherKeys, alt screen buffer, synchronized output, mouse tracking — change the terminal's input/output contract. When the TUI enables a protocol but fails to disable it on suspend, shell escape, crash, or normal exit, the user's shell inherits the modified state. The result: garbage escape sequences echoed on every keypress, paste events wrapped in spurious brackets, or a completely unresponsive terminal requiring `reset`. PR #b12c56364 fixed Kitty protocol leaking CSI-u sequences into the parent shell after Ctrl+Z. PR #ea5ca9ae0 fixed bracketed paste staying enabled after `!vim`. PR #579923350 added stdin drain on exit to flush queued protocol responses. The hard rule: **every protocol enable sequence must have a paired disable on every exit path — normal, error, suspend, shell escape, and unmount**.

**Scope.** Any code that writes a terminal escape sequence that changes terminal mode or input interpretation. This covers Kitty keyboard protocol (`\x1b[>1u` / `\x1b[<u`), bracketed paste (`\x1b[?2004h` / `\x1b[?2004l`), modifyOtherKeys (`\x1b[>4;2m` / `\x1b[>4;0m`), alt screen buffer (`\x1b[?1049h` / `\x1b[?1049l`), synchronized output (`\x1b[?2026h` / `\x1b[?2026l`), mouse tracking (`\x1b[?1000h` etc.), focus events (`\x1b[?1004h`), and any future OSC/CSI sequence that alters terminal state. Concrete starting points: the terminal initialization and teardown paths in `crates/kiro-tui/`, signal handlers in `crates/kiro-shell/`, and shell-escape dispatch logic. Any new protocol adoption — for example sixel graphics, OSC 52 clipboard, or progressive enhancement queries — must be reviewed the same way.

## Techniques

1. **[code] Enable/disable sequence census.** Grep for all protocol enable sequences: `rg -n '\\x1b\[[\?>]' crates/` and `rg -n 'CSI|\\e\[' crates/`. Build a table: protocol name, enable sequence, disable sequence, file:line of each. Every enable must have a corresponding disable. A missing disable is a finding.

2. **[code] Exit-path coverage for each protocol.** For each protocol identified in technique 1, trace every code path that can leave the TUI: normal exit, `process::exit`, panic handler, `uncaughtException`, `SIGTERM`/`SIGINT` handler, and drop impls. Each path must execute the disable sequence. A path that skips disable is a finding.

3. **[code] SIGTSTP/SIGCONT handler audit.** Grep for `SIGTSTP`, `SIGCONT`, `SIGSUSP`, `Ctrl-Z`, `suspend` in signal handler registration. The SIGTSTP handler must disable all active protocols before raising the default SIGTSTP (to actually suspend). The SIGCONT handler must re-enable all protocols and issue a full redraw (`\x1b[2J` or equivalent). Missing either half is a finding.

4. **[code] Shell-escape protocol bracket.** Find the shell-escape implementation (the code path for `!cmd` or spawning a user shell). Before spawning, all protocols must be disabled and the terminal restored to cooked mode. After the child exits, all protocols must be re-enabled. Grep for `shell_escape`, `spawn_shell`, `execute_command`, `!` command dispatch. Verify the disable-spawn-restore sequence is atomic (no early return between disable and restore).

5. **[code] Panic/crash handler audit.** Read the panic hook (`std::panic::set_hook`) and any `Drop` impl on the terminal state struct. The handler must disable all protocols unconditionally — it cannot assume which protocols are currently active, so it must send all disable sequences. Verify it does not write to stdout after the disable (which could re-enable protocols via buffered output).

6. **[code] Stdin drain on exit.** After disabling protocols, queued protocol responses may still be in the stdin buffer. Verify the exit path drains stdin (non-blocking read loop until empty or timeout) before returning control to the parent shell. PR #579923350 is the reference. Missing drain is a finding.

7. **[blackbox] Suspend/resume probe.** Start the TUI, type some text to confirm input works, press Ctrl+Z. In the parent shell, type `echo hello` and press Enter — output must be clean (no escape sequences interleaved). Run `fg` to resume. Type in the TUI — input must work correctly with no garbage characters. Repeat 10 times. Any garbage on suspend or broken input on resume is a failure.

8. **[blackbox] Shell-escape round-trip.** Start the TUI, invoke a shell escape (e.g., `!bash`). Inside the spawned shell, verify: arrow keys produce correct movement (not `^[[A` literals), pasting works without bracket artifacts, Ctrl+C works. Exit the shell. Back in the TUI, verify input still works correctly. Repeat with `!vim` to test alt-screen nesting.

9. **[blackbox] Kill -9 from another terminal.** Start the TUI in terminal A. From terminal B, send `kill -9 <pid>`. In terminal A, type `echo test` and press Enter. The parent shell must produce clean output — no protocol sequences visible. If the shell is corrupted, the TUI's exit-time cleanup is insufficient (expected for SIGKILL, but the parent shell's SIGCHLD handling or the TUI's wrapper script should run `reset` or equivalent).

10. **[blackbox] Crash-exit protocol cleanup.** Trigger a panic or forced error in the TUI (via a test fixture or debug command). After the process exits, verify the terminal is clean: type in the shell, paste text, use arrow keys. All must work without `reset`. If they don't, the panic handler is not disabling protocols.

## What to record

Protocol name, enable sequence location (file:line), disable sequence location (file:line), exit paths covered, exit paths missing, probe result.

## Done criteria

Every protocol enable has a verified paired disable on all five exit paths (normal, error, suspend, shell-escape, panic). The SIGTSTP handler disables all protocols before suspend and SIGCONT re-enables them. All four blackbox probes pass without requiring manual `reset`.
