/**
 * Globally redirect `console.{error,warn,log,info,debug}` to the TUI's
 * file-based `logger`.
 *
 * # Why
 *
 * The TUI is a foreground process whose stderr IS the user's terminal.
 * Any third-party library calling `console.*` leaks straight to the
 * user's screen, bypassing the codebase's `logger` -> file convention.
 *
 * The most consistent offender is `@agentclientprotocol/sdk`, which
 * fires `console.error("Error handling request", ...)` on every
 * JSON-RPC error response. Other libraries (Ink dev warnings, future
 * deps) can leak the same way.
 *
 * This module enforces the codebase invariant - all diagnostics go
 * through `logger` - for code we don't control.
 *
 * # Invocation
 *
 * Call {@link installConsoleInterceptor} once at TUI startup, BEFORE
 * any module that might wire up SDK connections or load third-party
 * code. The earliest line in `index.tsx` is the right spot.
 *
 * # Test isolation
 *
 * Tests that exercise the interceptor must call
 * {@link uninstallConsoleInterceptor} in their teardown so the
 * patched globals don't leak between test files.
 */

import { logger } from './logger';

interface ConsoleSnapshot {
  error: typeof console.error;
  warn: typeof console.warn;
  log: typeof console.log;
  info: typeof console.info;
  debug: typeof console.debug;
}

let original: ConsoleSnapshot | null = null;

/**
 * Patch the global `console` object to route every level into
 * {@link logger}. Idempotent: a second call is a no-op until
 * {@link uninstallConsoleInterceptor} runs.
 */
export function installConsoleInterceptor(): void {
  if (original !== null) return;
  original = {
    error: console.error,
    warn: console.warn,
    log: console.log,
    info: console.info,
    debug: console.debug,
  };
  console.error = (...args: unknown[]) => logger.error('[console]', ...args);
  console.warn = (...args: unknown[]) => logger.warn('[console]', ...args);
  console.log = (...args: unknown[]) => logger.info('[console]', ...args);
  console.info = (...args: unknown[]) => logger.info('[console]', ...args);
  console.debug = (...args: unknown[]) => logger.debug('[console]', ...args);
}

/** Restore the pre-install `console` methods. No-op when not installed. */
export function uninstallConsoleInterceptor(): void {
  if (original === null) return;
  console.error = original.error;
  console.warn = original.warn;
  console.log = original.log;
  console.info = original.info;
  console.debug = original.debug;
  original = null;
}
