/**
 * Tests for the global console interceptor. Pins:
 *   - Each `console.*` level routes to the matching `logger.*` level
 *   - The `[console]` prefix is forwarded so the leaked source is identifiable in logs
 *   - install is idempotent
 *   - uninstall restores the originals (so test isolation works)
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { logger } from '../logger';
import {
  installConsoleInterceptor,
  uninstallConsoleInterceptor,
} from '../console-interceptor';

interface LoggerSpy {
  error: Array<unknown[]>;
  warn: Array<unknown[]>;
  info: Array<unknown[]>;
  debug: Array<unknown[]>;
}

let originalLogger: {
  error: typeof logger.error;
  warn: typeof logger.warn;
  info: typeof logger.info;
  debug: typeof logger.debug;
};
let spy: LoggerSpy;

beforeEach(() => {
  originalLogger = {
    error: logger.error.bind(logger),
    warn: logger.warn.bind(logger),
    info: logger.info.bind(logger),
    debug: logger.debug.bind(logger),
  };
  spy = { error: [], warn: [], info: [], debug: [] };
  logger.error = ((...a: unknown[]) =>
    spy.error.push(a)) as typeof logger.error;
  logger.warn = ((...a: unknown[]) => spy.warn.push(a)) as typeof logger.warn;
  logger.info = ((...a: unknown[]) => spy.info.push(a)) as typeof logger.info;
  logger.debug = ((...a: unknown[]) =>
    spy.debug.push(a)) as typeof logger.debug;
});

afterEach(() => {
  uninstallConsoleInterceptor();
  logger.error = originalLogger.error;
  logger.warn = originalLogger.warn;
  logger.info = originalLogger.info;
  logger.debug = originalLogger.debug;
});

describe('installConsoleInterceptor', () => {
  it('routes console.error to logger.error with a `[console]` prefix', () => {
    installConsoleInterceptor();
    console.error('boom', { detail: 1 });
    expect(spy.error).toEqual([['[console]', 'boom', { detail: 1 }]]);
  });

  it('routes console.warn to logger.warn', () => {
    installConsoleInterceptor();
    console.warn('careful');
    expect(spy.warn).toEqual([['[console]', 'careful']]);
  });

  it('routes console.log to logger.info', () => {
    installConsoleInterceptor();
    console.log('hello');
    expect(spy.info).toEqual([['[console]', 'hello']]);
  });

  it('routes console.info to logger.info', () => {
    installConsoleInterceptor();
    console.info('fyi');
    expect(spy.info).toEqual([['[console]', 'fyi']]);
  });

  it('routes console.debug to logger.debug', () => {
    installConsoleInterceptor();
    console.debug('trace');
    expect(spy.debug).toEqual([['[console]', 'trace']]);
  });

  it('captures the SDK signature exactly: `Error handling request`, message, error', () => {
    // Mirrors `@agentclientprotocol/sdk/dist/acp.js:1028`. This is the
    // exact call the TUI was leaking pre-fix; the test pins that the
    // routing handles the full argument list (label + structured args).
    installConsoleInterceptor();
    const message = {
      jsonrpc: '2.0',
      id: 1,
      method: '_kiro/auth/getAccessToken',
    };
    const error = {
      code: -32603,
      message: 'Internal error',
      data: { details: 'x' },
    };
    console.error('Error handling request', message, error);
    expect(spy.error).toEqual([
      ['[console]', 'Error handling request', message, error],
    ]);
  });

  it('is idempotent: install x2 routes a single console.error to logger.error once', () => {
    installConsoleInterceptor();
    installConsoleInterceptor();
    console.error('boom');
    expect(spy.error).toEqual([['[console]', 'boom']]);
  });
});

describe('uninstallConsoleInterceptor', () => {
  it('restores the original console.* methods', () => {
    const baseline = {
      error: console.error,
      warn: console.warn,
      log: console.log,
      info: console.info,
      debug: console.debug,
    };

    installConsoleInterceptor();
    expect(console.error).not.toBe(baseline.error);

    uninstallConsoleInterceptor();
    expect(console.error).toBe(baseline.error);
    expect(console.warn).toBe(baseline.warn);
    expect(console.log).toBe(baseline.log);
    expect(console.info).toBe(baseline.info);
    expect(console.debug).toBe(baseline.debug);
  });

  it('is a no-op when the interceptor was never installed', () => {
    const baseline = console.error;
    uninstallConsoleInterceptor();
    expect(console.error).toBe(baseline);
  });
});
