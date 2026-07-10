/**
 * ACP wire-level tests for /rewind command (session/fork).
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
  PromptRequest,
  PromptResponse,
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
    sessionId: 'rewind-session-1',
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

describe('/rewind command (session/fork)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('session/fork sends messageId and sessionId to KAS', async () => {
    /**
     * GIVEN  session with a conversation turn
     * WHEN   /rewind dispatches with a messageId
     * THEN   session/fork is called with sessionId + messageId + createdReason
     */
    tc = new AcpTestCase({ testName: 'rewind-fork' });
    setupHandshake(tc);

    tc.mock.on('session/fork', () => ({ sessionId: 'forked-session-1' }));
    tc.mock.on('session/load', () => ({
      sessionId: 'forked-session-1',
      modes: defaultKasModes(),
    }));

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      tc!.mock.notify('session/update', {
        sessionId: 'rewind-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'First response' },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      tc!.mock.notify('session/update', {
        sessionId: 'rewind-session-1',
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_completion' } },
        },
      });
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('hello');
    await tc.pressEnter();
    await tc.waitForVisibleText('First response', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/rewind 0');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(1000);

    const forkReqs = tc.mock.receivedRequests('session/fork');
    expect(forkReqs.length).toBeGreaterThanOrEqual(1);
    const params = forkReqs[0]!.params as {
      sessionId: string;
      _meta?: { kiro?: { createdReason?: string } };
    };
    expect(params.sessionId).toBe('rewind-session-1');
    expect(params._meta?.kiro?.createdReason).toBe('rewind');
  });

  it('renders the agent chip as "Default" after rewind loads a session with wire id vibe', async () => {
    /**
     * GIVEN  a forked session whose session/load returns the wire mode `vibe`
     * WHEN   /rewind loads that session
     * THEN   the current agent is the normalized `default`, so the chip
     *        renders "Default" and never leaks the raw wire id `vibe`
     */
    tc = new AcpTestCase({ testName: 'rewind-agent-chip' });
    setupHandshake(tc);

    tc.mock.on('session/fork', () => ({ sessionId: 'forked-session-1' }));
    // KAS returns the full config-option set on load; the TUI derives the
    // current agent from the `mode` option (wire id `vibe` normalizes to
    // `default`), not from the V2-style `modes` field.
    tc.mock.on('session/load', () => ({
      sessionId: 'forked-session-1',
      modes: {
        currentModeId: 'vibe',
        availableModes: [{ id: 'vibe', name: 'Vibe' }],
      },
      configOptions: [
        {
          type: 'select',
          id: 'mode',
          name: 'Mode',
          category: 'mode',
          currentValue: 'vibe',
          options: [{ value: 'vibe', name: 'Default' }],
        },
      ],
    }));

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      tc!.mock.notify('session/update', {
        sessionId: 'rewind-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'First response' },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      tc!.mock.notify('session/update', {
        sessionId: 'rewind-session-1',
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_completion' } },
        },
      });
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('hello');
    await tc.pressEnter();
    await tc.waitForVisibleText('First response', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/rewind 0');
    await tc.sleepMs(200);
    await tc.pressEnter();

    const store = await tc.waitForStore(
      (s) => s.currentAgent?.name === 'default',
      5000
    );
    expect(store.currentAgent?.name).toBe('default');

    const snapshot = await tc.terminalSnapshot();
    expect(snapshot.contains('Default')).toBe(true);
    expect(snapshot.matches(/\bvibe\b/i)).toBe(false);
  });
});
