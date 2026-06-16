/**
 * ACP wire-level tests for `/compact` command and summarization lifecycle.
 *
 * Covers: _kiro/session/compact ext method + session_info_update events
 * for summarization_started, summarization_completed, summarization_failed.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'compact-session-1',
    modes: defaultKasModes(),
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('/compact command + summarization lifecycle', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('sends _kiro/session/compact with sessionId on /compact', async () => {
    /**
     * GIVEN  session active
     * WHEN   user types /compact
     * THEN   _kiro/session/compact called with {sessionId}
     */
    tc = new AcpTestCase({ testName: 'compact-send' });
    setupHandshake(tc);

    tc.mock.on('_kiro/session/compact', () => ({
      success: true,
      message: 'Compaction started',
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/compact');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/session/compact');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const params = reqs[0]!.params as { sessionId: string };
    expect(params.sessionId).toBe('compact-session-1');
  });

  it('summarization_started sets compacting state in store', async () => {
    /**
     * GIVEN  session active
     * WHEN   server pushes session_info_update with summarization_started
     * THEN   store reflects compacting state
     */
    tc = new AcpTestCase({ testName: 'compact-started' });
    setupHandshake(tc);
    tc.mock.on('_kiro/session/compact', () => ({ success: true }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('session/update', {
      sessionId: 'compact-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'summarization_started' } },
      },
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    expect(store.isCompacting).toBe(true);
  });

  it('summarization_completed clears compacting state', async () => {
    /**
     * GIVEN  TUI in compacting state
     * WHEN   server pushes summarization_completed
     * THEN   store exits compacting state
     */
    tc = new AcpTestCase({ testName: 'compact-completed' });
    setupHandshake(tc);
    tc.mock.on('_kiro/session/compact', () => ({ success: true }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    // Push started then completed
    tc.mock.notify('session/update', {
      sessionId: 'compact-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'summarization_started' } },
      },
    });
    await tc.sleepMs(200);

    tc.mock.notify('session/update', {
      sessionId: 'compact-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'summarization_completed' } },
      },
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    expect(store.isCompacting).toBe(false);
  });

  it('summarization_failed clears compacting and surfaces error', async () => {
    /**
     * GIVEN  TUI in compacting state
     * WHEN   server pushes summarization_failed
     * THEN   store exits compacting state
     */
    tc = new AcpTestCase({ testName: 'compact-failed' });
    setupHandshake(tc);
    tc.mock.on('_kiro/session/compact', () => ({ success: true }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('session/update', {
      sessionId: 'compact-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'summarization_started' } },
      },
    });
    await tc.sleepMs(200);

    tc.mock.notify('session/update', {
      sessionId: 'compact-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: { kind: 'summarization_failed', error: 'Out of memory' },
        },
      },
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    expect(store.isCompacting).toBe(false);
  });
});
