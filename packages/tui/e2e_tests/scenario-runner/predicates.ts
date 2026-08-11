import type { TestHarness, VerifyResult } from './types';

const DEFAULT_VERIFY_TIMEOUT = 10_000;
const DEFAULT_ABSENCE_STABILITY = 1_000;
const POLL_INTERVAL = 100;

export interface VerifyContext {
  initialSessionId?: string;
}

export async function assertVerify(
  harness: TestHarness,
  predicate: string,
  ctx: VerifyContext = {}
): Promise<VerifyResult> {
  const colonIndex = predicate.indexOf(':');
  const command = colonIndex === -1 ? predicate : predicate.substring(0, colonIndex);
  const arg = colonIndex === -1 ? '' : predicate.substring(colonIndex + 1);

  try {
    switch (command) {
      case 'screen.contains': {
        const start = Date.now();
        let screen = '';
        while (Date.now() - start < DEFAULT_VERIFY_TIMEOUT) {
          const lines = harness.getSnapshot();
          screen = lines.join('\n');
          if (screen.includes(arg)) {
            return { predicate, passed: true, actual: `found "${arg}"` };
          }
          await harness.sleepMs(POLL_INTERVAL);
        }
        return {
          predicate,
          passed: false,
          actual: `text "${arg}" not found on screen. Screen content (last 5 lines): ${screen.split('\n').slice(-5).join(' | ')}`,
        };
      }
      case 'screen.notContains': {
        const start = Date.now();
        let screen = '';
        while (Date.now() - start < DEFAULT_ABSENCE_STABILITY) {
          screen = harness.getSnapshot().join('\n');
          if (screen.includes(arg)) {
            return {
              predicate,
              passed: false,
              actual: `text "${arg}" unexpectedly found on screen`,
            };
          }
          await harness.sleepMs(POLL_INTERVAL);
        }
        return {
          predicate,
          passed: true,
          actual: `"${arg}" remained absent for ${DEFAULT_ABSENCE_STABILITY}ms`,
        };
      }
      case 'process.exited': {
        try {
          await harness.expectExit(DEFAULT_VERIFY_TIMEOUT);
          return { predicate, passed: true, actual: 'process exited' };
        } catch {
          return {
            predicate,
            passed: false,
            actual: 'process did not exit within timeout',
          };
        }
      }
      case 'store.sessionId.changed': {
        if (!ctx.initialSessionId) {
          return {
            predicate,
            passed: false,
            actual: 'initial session id unavailable',
          };
        }

        const start = Date.now();
        while (Date.now() - start < DEFAULT_VERIFY_TIMEOUT) {
          const store = await harness.getStore();
          if (
            store.sessionId &&
            store.sessionId !== ctx.initialSessionId
          ) {
            return {
              predicate,
              passed: true,
              actual: `session changed from "${ctx.initialSessionId}" to "${store.sessionId}"`,
            };
          }
          await harness.sleepMs(POLL_INTERVAL);
        }

        return {
          predicate,
          passed: false,
          actual: `session id did not change from "${ctx.initialSessionId}"`,
        };
      }
      default:
        return {
          predicate,
          passed: false,
          actual: 'unknown predicate: not implemented',
        };
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      predicate,
      passed: false,
      actual: `assertion error: ${message}`,
    };
  }
}
