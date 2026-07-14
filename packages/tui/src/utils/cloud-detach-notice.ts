import { UNICODE_GLYPHS, type Glyphs } from './glyphs';
import { recordTuiCloudSession } from './tui-telemetry-observer.js';

/**
 * Message shown when the CLI disconnects from a cloud-sandbox session.
 * A cloud session keeps running on the sandbox after the CLI detaches —
 * closing the connection does not cancel it — so this reassures the user the
 * work continues, and surfaces the session id so they can reattach later.
 * Pure so it is unit-testable; the caller writes it to stderr.
 */
export function formatCloudDetachNotice(
  sessionId: string,
  glyphs: Glyphs = UNICODE_GLYPHS
): string {
  return (
    `${glyphs.checkmark} Quit session ${sessionId}\n` +
    `Your work continues while you're away.`
  );
}

let detachNoticePrinted = false;

/**
 * Emit the detach notice exactly once, from whichever exit path fires first
 * (keep-running quit, Ctrl+D, or a signal). Idempotent via a module flag so
 * overlapping exit handlers don't double-print. A falsy id is a no-op, so
 * local exits are unaffected.
 */
export function emitCloudDetachNoticeOnce(
  sessionId: string | null | undefined
): void {
  if (detachNoticePrinted || !sessionId) return;
  detachNoticePrinted = true;
  try {
    process.stderr.write(`\n${formatCloudDetachNotice(sessionId)}\n`);
    recordTuiCloudSession({ event: 'detached' });
  } catch {
    // Never block shutdown on the notice.
  }
}

/**
 * Mark the notice as already handled for exits where the user chose to STOP
 * the agent (turn-off). Without this, a signal arriving while the cancel is
 * in flight would print "your work continues" — the opposite of what the
 * user asked for.
 */
export function suppressCloudDetachNotice(): void {
  detachNoticePrinted = true;
}

/** Test-only: reset the once-flag between cases. */
export function resetCloudDetachNoticeForTest(): void {
  detachNoticePrinted = false;
}

/**
 * Keep-running quit: print the reattach notice, detach, exit. The notice is
 * emitted before exiting because exit skips later teardown.
 */
export function quitCloudSessionKeepRunning(
  kiro: { sessionId: string | null | undefined; close(): void },
  exit: (code: number) => void = process.exit
): void {
  emitCloudDetachNoticeOnce(kiro.sessionId);
  kiro.close();
  exit(0);
}

/**
 * Turn-off quit: the user chose to STOP the agent, so the "work continues"
 * notice is suppressed before anything else — a signal arriving mid-cancel
 * must not print it. Exit waits for cancel to settle (it is time-bounded
 * internally) so the stop request reaches the server before teardown. A
 * cancel rejection still tears down via finally; the trailing catch keeps
 * the rejection from surfacing as unhandled.
 */
export function quitCloudSessionTurnOff(
  kiro: { cancel(): Promise<void>; close(): void },
  exit: (code: number) => void = process.exit
): void {
  suppressCloudDetachNotice();
  recordTuiCloudSession({ event: 'turned_off' });
  void kiro
    .cancel()
    .finally(() => {
      // exit must fire even if close() throws — otherwise the process would
      // silently survive a turn-off (the trailing catch swallows the throw).
      try {
        kiro.close();
      } finally {
        exit(0);
      }
    })
    .catch(() => {});
}
