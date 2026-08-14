/**
 * Unit tests for the in-process telemetry drop counter (`otlp-emit.ts`).
 *
 * TUI telemetry is metrics-only (KUTS has no OTLP logs path). The metrics SDK
 * (`meter.ts`) reports export failures through `recordEmitDrop`, so this file
 * covers the kept drop-counter surface: the counter increments on every drop,
 * the first drop in a process logs once at `error`, and the rest log at
 * `trace`. (The former `/v1/logs` emitter and its envelope/POST tests were
 * removed with the log path.)
 */

import { beforeEach, describe, expect, it, mock } from 'bun:test';

// Register a COMPLETE spy logger before importing the module under test.
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../logger.js']);

// Several sibling test files `mock.module('../logger.js', …)` with a partial
// logger (some omit `.trace`); Bun's module mocks are process-global, so
// asserting via `spyOn` on the shared singleton is order-dependent and flaky
// under the full suite. Owning a complete spy logger here makes the
// drop-logging assertions deterministic regardless of suite order, and
// exercises the production `logAt()` against a logger that has every level.
const logSpy = {
  error: mock((_message: string) => {}),
  warn: mock((_message: string) => {}),
  info: mock((_message: string) => {}),
  debug: mock((_message: string) => {}),
  trace: mock((_message: string) => {}),
};
mock.module('../logger.js', () => ({ logger: logSpy }));

// @ts-expect-error - Bun query-string import isolates this module's globals.
const otlpEmit = await import('../otlp-emit?unit-test');
const { recordEmitDrop, getEmitFailedTotal, _resetEmitFailedTotalForTests } =
  otlpEmit as typeof import('../otlp-emit');

beforeEach(() => {
  _resetEmitFailedTotalForTests();
  logSpy.error.mockClear();
  logSpy.trace.mockClear();
});

describe('recordEmitDrop / getEmitFailedTotal', () => {
  it('increments the counter on every drop', () => {
    expect(getEmitFailedTotal()).toBe(0);
    recordEmitDrop('counter:foo: network');
    recordEmitDrop('histogram:bar: timeout');
    expect(getEmitFailedTotal()).toBe(2);
  });

  it('logs the first drop once at error, the rest at trace, but counts every drop', () => {
    recordEmitDrop('counter:foo: network');
    recordEmitDrop('counter:foo: network');

    // Counter tracks BOTH drops (the source of truth)...
    expect(getEmitFailedTotal()).toBe(2);
    // ...the first drop is loud (error, exactly once)...
    expect(logSpy.error).toHaveBeenCalledTimes(1);
    expect(logSpy.error.mock.calls[0]![0]).toContain('unreachable');
    // ...and the second drops to trace so we don't spam (locks the
    // "rest at trace" half — the counter increment is unconditional, so
    // without this the trace line could be deleted with green tests).
    expect(logSpy.trace).toHaveBeenCalledTimes(1);
    expect(logSpy.trace.mock.calls[0]![0]).toContain('failed_total=2');
  });

  it('_resetEmitFailedTotalForTests clears the counter and the one-shot guard', () => {
    recordEmitDrop('counter:foo: network');
    expect(getEmitFailedTotal()).toBe(1);
    expect(logSpy.error).toHaveBeenCalledTimes(1);

    _resetEmitFailedTotalForTests();
    logSpy.error.mockClear();

    // After reset the next drop is again the loud first one.
    recordEmitDrop('counter:foo: network');
    expect(getEmitFailedTotal()).toBe(1);
    expect(logSpy.error).toHaveBeenCalledTimes(1);
  });
});
