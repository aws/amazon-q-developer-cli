# Review 9 — Terminal, TTY, and environment edge cases

**Why this class matters.** The TUI runs on many terminals (iTerm, Ghostty, Kitty, tmux, Alacritty, Windows Terminal, dumb CI terminals) and under many conditions (SSH, CI, no TTY, piped stdout, redirected stderr). Each combination exposes different edge cases: SIGWINCH behaviour, color support, Unicode width, 256-color vs truecolor, mouse support, paste handling.

**Scope.** Anything whose behaviour depends on terminal capabilities, connection transport, or shell environment. This covers TTY detection, color detection, Unicode-width computation, ANSI parsing/stripping, mouse / paste / focus / bracketed-paste handling, cursor-visibility state, notification APIs, and any code branching on environment variables (`TERM`, `COLORTERM`, `SSH_*`, `CI`, `TMUX`, `WT_SESSION`, `TERM_PROGRAM`, locale/codepage variables).

Concrete starting points: `terminal-capabilities.ts`, `terminal-detection.ts`, `terminal-theme.ts`, `colorUtils.ts`, `notification.ts`, and any file that reads `process.env`. New terminal-facing features (future mouse-gesture handlers, OSC-sequence integrations, sixel/image protocols, shell integration) must be reviewed the same way.

## Techniques

1. **[code] Environment-branch audit.** Grep for `process.env.` and classify each branch: which terminals does it cover, which does it miss? Build a table of terminal times SSH times tmux times CI and mark which branches handle each cell.

2. **[code] Unicode-width audit.** Any place that measures text width must use a unicode-aware width function (wide CJK, zero-width joiners, emoji ZWJ sequences). Grep for `text.length` in width contexts — `.length` counts code units, not display columns.

3. **[code] ANSI-strip audit.** Any place that passes user text through yoga measure must first account for ANSI escape sequences. A string with color codes is longer in bytes than on screen. Misclassifying this causes yoga overflow.

4. **[code] Paste-handling scan.** Long pastes and pastes containing control sequences have caused past bugs (`d61ec4b1a`). Grep for paste detection and verify it handles very large pastes (over 1 MB) without per-byte processing.

5. **[blackbox] No-TTY probe.** Run the TUI with stdout piped to `cat` or redirected to a file. It must detect no-TTY and refuse to start (or run in a degraded mode). It must not spin in a render loop.

6. **[blackbox] 256-color vs truecolor round-trip.** Prior fix `e3205d5fa` caught a 256-color double-conversion. For each color utility, round-trip a sample palette and assert no drift.

7. **[blackbox] Headless CI mode.** Run every test in an environment with `CI=1`, `TERM=dumb`, no TTY. Any test that requires a real TTY must opt out explicitly.

8. **[blackbox] Real-terminal matrix.** Launch the TUI under each of: iTerm2, Terminal.app, Ghostty, Kitty, Alacritty, Windows Terminal, Windows PowerShell, `cmd.exe`, gnome-terminal, xterm, tmux, screen. For each, exercise resize, truecolor, mouse, paste, and exit. Maintain a compatibility matrix with dated results.

9. **[blackbox] SSH / remote-session probe.** Run the TUI over SSH, mosh, and a tmux-over-SSH session. Verify SIGWINCH propagates, keystrokes are not dropped, and disconnect cleans up. Differences from local are findings.

10. **[blackbox] Locale/encoding probe.** Run the TUI under `LANG=C`, `LANG=ja_JP.UTF-8`, `LANG=zh_CN.UTF-8`, `LANG=ru_RU.UTF-8`. Paste content in each locale's script. Wide-char handling, sorting, and text width must all behave correctly.

11. **[blackbox] Terminal-size extremes.** Run the TUI in a 10 by 5 terminal, a 500 by 200 terminal, a 1 by 1000 (tall and skinny) terminal, and a 1000 by 1 (short and wide) terminal. It must not crash or produce garbled output. Degrade gracefully at the small end (see Review 2).

## What to record

Environment / terminal, expected behaviour, actual branch, gap.

## Done criteria

The terminal times transport times CI matrix has at least one test per cell, or a documented skip reason. The real-terminal matrix is run at least once per release.
