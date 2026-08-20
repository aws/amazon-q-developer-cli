/**
 * Coverage wrapper — hosts the TUI entrypoint inside a bun test so that
 * `bun test --coverage` can instrument and collect coverage from the TUI process.
 *
 * This file is NOT run directly. It is the entrypoint the coverage launch paths
 * spawn (`bun test … coverage-wrapper.ts`) when KIRO_COVERAGE=1.
 *
 * The TUI runs normally (same behavior, same PTY), but bun tracks coverage.
 */
import { test } from 'bun:test';

/** Settles the holder test below, so `bun test` can finalize and write lcov. */
let releaseHolder: (() => void) | undefined;
let exitCode: number | undefined;

// Intercept process.exit: a TUI-initiated exit must end the *holder test*, not
// the process. Calling the real exit here would kill bun mid-run and lose this
// scenario's lcov, and `beforeExit` does not fire for an explicit process.exit,
// so the holder would otherwise stay pending until the process died.
const originalExit = process.exit;
process.exit = ((code?: number) => {
  exitCode = code ?? 0;
  if (releaseHolder) {
    releaseHolder();
    // Not reached by the TUI's own call stack: returning lets bun unwind,
    // finalize coverage, and exit on its own.
    return undefined as never;
  }
  // No holder yet (an exit during module init) — nothing has been measured, so
  // let the real exit through rather than hanging.
  return originalExit(code ?? 0);
}) as never;

// The TUI is imported dynamically INSIDE the holder test, not with a static
// import: static imports are hoisted, so a top-level `import '../index.tsx'`
// would start the TUI before this file's body runs — before process.exit is
// intercepted and before the holder exists to keep the process alive.
test('tui-coverage-holder', async () => {
  const done = new Promise<void>((resolve) => {
    releaseHolder = resolve;
    process.on('SIGTERM', resolve);
    process.on('SIGINT', resolve);
    process.on('beforeExit', resolve);
  });
  await import('../index.tsx');
  await done;
}, 999_999_000);

// After bun has written lcov, honor the exit code the TUI asked for.
process.on('beforeExit', () => {
  if (exitCode !== undefined && exitCode !== 0) originalExit(exitCode);
});
