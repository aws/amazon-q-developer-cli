/**
 * ACP wire-level tests for hooks: `_kiro/hooks/list` (pull fallback)
 * and `_kiro/hooks/didChange` notification (push path).
 *
 * The CLI subscribes to `_kiro/hooks/didChange` for real-time hook
 * registry updates, and falls back to `_kiro/hooks/list` when the
 * cached list is empty.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const CANNED_HOOKS = [
  { name: 'pre-commit', event: 'onSave', path: '.kiro/hooks/pre-commit.md' },
  { name: 'on-error', event: 'onError', path: '.kiro/hooks/on-error.md' },
];

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'hooks-session-1',
    modes: defaultKasModes(),
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('hooks list + didChange notification', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('_kiro/hooks/didChange notification populates store hooksList', async () => {
    /**
     * GIVEN  TUI connected, KAS handshake complete
     * WHEN   server pushes _kiro/hooks/didChange with hooks array
     * THEN   store.hooksList reflects the pushed data
     *
     * This test validates the fix for BUG-1: kiro.ts now forwards
     * HooksUpdate events through initNotificationHandler at idle time.
     */
    tc = new AcpTestCase({ testName: 'hooks-didchange' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(500);

    tc.mock.notify('_kiro/hooks/didChange', {
      hooks: CANNED_HOOKS,
    });

    // With the fix, the notification should now propagate to the store
    const store = await tc.waitForStore((s) => s.hooksList.length >= 2, 3000);
    expect(store.hooksList.find((h) => h.name === 'pre-commit')).toBeDefined();
    expect(store.hooksList.find((h) => h.name === 'on-error')).toBeDefined();
  });

  it('/hooks command triggers _kiro/hooks/list when no cached hooks', async () => {
    /**
     * GIVEN  no _kiro/hooks/didChange received (cache empty)
     * WHEN   user types /hooks
     * THEN   _kiro/hooks/list is called as fallback, panel shows hooks
     */
    tc = new AcpTestCase({ testName: 'hooks-list-fallback' });
    setupHandshake(tc);

    tc.mock.on('_kiro/hooks/list', () => ({
      success: true,
      data: { hooks: CANNED_HOOKS },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/hooks');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/hooks/list');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const params = reqs[0]!.params as { sessionId: string };
    expect(params.sessionId).toBe('hooks-session-1');

    const store = await tc.getStore();
    expect(store.showHooksPanel).toBe(true);
  });
});
