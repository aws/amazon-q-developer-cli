/**
 * In-process drop counter for the TUI's OTLP pipeline. First drop per process
 * logs at `error` (the default log level, so it actually reaches the file);
 * the rest at `trace`. No retry/queue/fallback — losing metrics on
 * endpoint-down is acceptable, silence is not.
 */

import { logger } from './logger.js';

/**
 * Tolerate a logger missing `level`: sibling tests `mock.module('./logger.js')`
 * with a partial logger and Bun's mocks are process-global, so an unguarded
 * `logger.trace(…)` would throw here even when this module isn't under test.
 */
function logAt(level: 'error' | 'trace', message: string): void {
  const fn = logger[level];
  if (typeof fn === 'function') fn.call(logger, message);
}

let emitFailedTotal = 0;
let loggedFirstDrop = false;

export function getEmitFailedTotal(): number {
  return emitFailedTotal;
}

/** Bump the drop counter; log first at `error`, rest at `trace`. Never throws. */
export function recordEmitDrop(detail: string): void {
  emitFailedTotal++;
  if (!loggedFirstDrop) {
    loggedFirstDrop = true;
    logAt(
      'error',
      `[otlp-emit] OTLP endpoint unreachable at the configured OTLP ` +
        `endpoint; dropping TUI telemetry (first drop: ${detail}). Further ` +
        `drops logged at trace level. failed_total=${emitFailedTotal}`
    );
    return;
  }
  logAt(
    'trace',
    `[otlp-emit] dropped: ${detail} (failed_total=${emitFailedTotal})`
  );
}

/** Reset the drop counter and one-shot log guard — test-only. */
export function _resetEmitFailedTotalForTests(): void {
  emitFailedTotal = 0;
  loggedFirstDrop = false;
}
