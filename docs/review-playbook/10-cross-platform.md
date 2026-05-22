# Review 10 — Cross-platform compatibility (Windows / macOS / Linux)

**Why this class matters.** `kiro-cli` ships to all three major desktop platforms. Windows in particular has distinct behaviour for paths, line endings, signals, file locking, process trees, case-sensitivity, and shells. A single hard-coded forward-slash in a path or a single LF-only line-ending assumption can ship a silent regression to half your users. Cross-platform bugs are rarely caught by the developer who wrote the code, because the developer is usually on one platform — they surface as user-reported issues weeks later.

**Scope.** All runtime code that touches OS-visible behaviour. The scope is intentionally broad because cross-platform bugs lurk in almost any I/O path. Focus on: file-system operations, path manipulation, shell invocation, environment lookup, signal handling, process management, clipboard, notifications, installer, editor integration, log rotation, socket/pipe creation, file locking and atomic writes, filesystem case sensitivity, line-ending handling, and executable-permission assumptions.

Concrete anchors: anything importing from `node:fs`, `node:path`, `node:child_process`, `node:os`, `node:net`, or using `process.env` / `process.platform`. New platform-sensitive features (future auto-updater paths, OS keychain integration, system-theme detection, file-watcher backends) must be reviewed the same way.

## Techniques

1. **[code] Path-separator grep.** Grep for hard-coded forward-slash inside string literals that represent file paths, and hard-coded backslash in regexes or templates that represent filesystem separators. Every hit should use `path.join` or `path.sep`. The fix-set around `d01d949bb` / `f697f9614` ("unify path canonicalization") exists because this was a recurring regression.

2. **[code] Path-API audit.** Grep for `path.posix.` and `path.win32.`. Direct use of these must be justified; mixing with default `path` leads to bugs (for example canonicalizing with `path.posix` then passing to `fs` on Windows).

3. **[code] Line-ending audit.** Grep for `split('\n')`, `split(/\n/)`, `split('\r\n')`. Any file-reading code that splits on LF only will break on CRLF files; any write-back that joins with LF will silently rewrite CRLF files as LF on Windows (regression #2116). Prefer `split(/\r?\n/)` for reads and preserve the original ending on write.

4. **[code] `process.platform` branch audit.** Grep for `process.platform`. For every branch, list which platforms it covers: `win32`, `darwin`, `linux`, `freebsd`, `openbsd`, `sunos`. Any switch that does not have a default for unknown platforms is a finding. Any code that uses `os.platform()` instead should be consolidated.

5. **[code] Shell-invocation audit.** Grep for `spawn(`, `exec(`, `execSync(`, `spawnSync(`. For each:
   - Is `shell: true` used? On Windows that invokes `cmd.exe`, which has different quoting rules than bash. Prefer `shell: false` with an argv array.
   - Is the command `sh` or `bash` hard-coded? On Windows those may not exist; use `process.env.SHELL` or delegate to an abstraction.
   - Are quotes and escapes using shell-specific syntax?

6. **[code] Environment-variable audit.** Variables like `HOME` (POSIX) vs `USERPROFILE` (Windows), `TMPDIR` vs `TEMP`, PATH separator (colon vs semicolon). Grep for each POSIX env var and verify the Windows equivalent is checked. Use `os.homedir()`, `os.tmpdir()`, `path.delimiter`.

7. **[code] Case-sensitivity scan.** macOS's default filesystem is case-insensitive; Linux is case-sensitive; Windows is case-insensitive. Any file lookup that depends on case will work on two platforms and fail on the third. Grep for `.toLowerCase()` / `.toUpperCase()` against filenames and audit whether the comparison is cross-platform safe.

8. **[code] File-locking and atomic-write audit.** Windows disallows rename-over-open-file; POSIX allows it. Any atomic-write pattern (write temp, rename to destination) needs careful handling on Windows if the destination is open elsewhere. The settings atomic-write fix (`666cc274a`) is a reference; re-verify across the codebase.

9. **[code] Executable-permissions audit.** `chmod 0o755` is a no-op on Windows NTFS. Any code that relies on Unix permissions for security must have an equivalent Windows ACL path or explicitly not support Windows for that flow.

10. **[code] Socket-path audit.** Unix domain sockets work differently on Windows (named pipes via the pipe-path prefix). Grep for `net.createServer().listen(` with a string argument and verify the path construction is platform-aware.

11. **[code] Path-length scan for Windows limits.** Windows MAX_PATH is 260 by default (longer with opt-in). Any path concatenation that could exceed this — especially under deeply nested node_modules or temp dirs — is a finding. Prefer the long-path prefix on Windows when paths might be long.

12. **[code] CI matrix verification.** Check `.github/workflows/`. Every bun/TUI test must run on `ubuntu-latest`, `macos-latest`, and `windows-latest`. Single-platform CI misses cross-platform regressions by definition.

13. **[blackbox] Smoke-test on all three OSes.** At least once per release, manually launch the TUI on a fresh Windows VM, macOS, and a headless Linux server. Exercise: open a file, paste multiline text with CRLF, run a shell command, trigger notification, exit cleanly.

14. **[blackbox] Editor-integration probe.** `$EDITOR` / `$VISUAL` vary: `vim`, `nano`, `notepad.exe`, `code --wait`. Any flow that shells out to an editor must pass the correct quoting and path to each and handle non-existence.

15. **[blackbox] Notification-backend probe.** macOS uses `osascript` or Notification Center; Linux uses `notify-send` or D-Bus; Windows uses toast APIs or PowerShell. Fallback when the backend is missing must be a no-op, not a crash.

16. **[blackbox] Full e2e on each OS.** Run the entire `test:e2e` suite on each CI runner. Tests that pass on Linux and fail only on Windows are often case-sensitivity, path, or line-ending bugs — the exact failure modes this review targets.

17. **[blackbox] Filesystem-corner probe.** On each OS, run the TUI with the config directory on: a network share (SMB on Windows, NFS on Linux), a case-insensitive volume on Linux, a read-only mount, a directory with a Unicode name, a path longer than 200 chars. Each should either work or surface a clear error.

18. **[blackbox] Locale / codepage probe.** On Windows, run under CP 437, CP 936 (Chinese), and UTF-8 (Windows 10+). On POSIX, run under `LANG=C` and UTF-8. Input and output encoding must be consistent in each.

19. **[blackbox] Clipboard probe.** Paste from the system clipboard on each OS: plain text, multiline text, text with CRLF, text with tabs, text with emoji. The TUI must receive the exact bytes it would if typed.

## What to record

Finding, platforms affected, evidence (grep hit, test result, user report), proposed fix, which platforms need regression tests.

## Done criteria

No hard-coded path separators outside of tests and regexes that document the intent. `process.platform` branches cover at least `win32`, `darwin`, and `linux` (or explicitly reject others). CI runs the TUI test suite on all three platforms. Smoke-test and full e2e pass on all three.
