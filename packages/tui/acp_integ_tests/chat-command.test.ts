import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'test-session-1',
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Vibe' }],
    },
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('/chat command', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it("typing '/ch' filters autocomplete to /chat", async () => {
    tc = new AcpTestCase({ testName: 'chat-command-autocomplete' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/ch');
    await tc.sleepMs(300);

    await tc.waitForVisibleText('/chat', 5000);
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('/chat');
    // Commands that don't match `/ch` should be filtered out.
    expect(snapshot).not.toContain('/agent');
    expect(snapshot).not.toContain('/help');
  }, 30000);

  it("typing '/chat' + Enter renders sessions returned by session/list", async () => {
    tc = new AcpTestCase({ testName: 'chat-command-session-picker' });
    setupHandshake(tc);

    tc.mock.on('session/list', () => ({
      sessions: [
        {
          sessionId: 'sess-aaaa-1111',
          cwd: process.cwd(),
          title: 'Refactor the dispatcher',
          updatedAt: new Date(Date.now() - 60_000).toISOString(),
        },
        {
          sessionId: 'sess-bbbb-2222',
          cwd: process.cwd(),
          title: 'Fix login flow bug',
          updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
        },
      ],
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // `/chat` is the shortest prefix that disambiguates from `/changelog`,
    // which would otherwise be highlighted alphabetically-first at `/cha`
    // and trigger the changelog panel on Enter.
    await tc.sendKeys('/chat');
    await tc.sleepMs(300);
    await tc.waitForVisibleText('/chat', 5000);

    await tc.sendKeys('\r');

    await tc.waitForVisibleText('Refactor the dispatcher', 5000);
    await tc.waitForVisibleText('Fix login flow bug', 5000);

    const listReqs = tc.mock.receivedRequests('session/list');
    expect(listReqs.length).toBeGreaterThanOrEqual(1);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('Refactor the dispatcher');
    expect(snapshot).toContain('Fix login flow bug');
    expect(snapshot).toContain('sess-aaa');
    expect(snapshot).toContain('sess-bbb');
  }, 30000);

  it("'/chat save' alerts that the subcommand is not yet supported", async () => {
    tc = new AcpTestCase({ testName: 'chat-command-save-unsupported' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // The trailing space dismisses the autocomplete menu so Enter submits.
    await tc.sendKeys('/chat save');
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText('/chat save is not yet supported', 5000);
    expect(tc.mock.receivedRequests('session/list')).toHaveLength(0);
  }, 30000);

  it("'/chat load' alerts that the subcommand is not yet supported", async () => {
    tc = new AcpTestCase({ testName: 'chat-command-load-unsupported' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/chat load');
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText('/chat load is not yet supported', 5000);
    expect(tc.mock.receivedRequests('session/list')).toHaveLength(0);
  }, 30000);

  it('selecting a session from the /chat picker renders streamed history', async () => {
    tc = new AcpTestCase({
      testName: 'chat-command-picker-replays-history',
    });
    setupHandshake(tc);

    const targetSessionId = 'sess-picker-load-1234';
    const otherSessionId = 'sess-picker-other-5678';

    tc.mock.on('session/list', () => ({
      sessions: [
        {
          sessionId: targetSessionId,
          cwd: process.cwd(),
          title: 'Pick me to load',
          updatedAt: new Date(Date.now() - 60_000).toISOString(),
        },
        {
          sessionId: otherSessionId,
          cwd: process.cwd(),
          title: 'Some other session',
          updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
        },
      ],
    }));

    tc.mock.on('session/load', async (params) => {
      const req = params as { sessionId: string };
      tc!.mock.notify('session/update', {
        sessionId: req.sessionId,
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: 'Hello from history' },
        },
      });
      tc!.mock.notify('session/update', {
        sessionId: req.sessionId,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Reply from the past' },
        },
      });
      // Drain notifications before unblocking the load response.
      await new Promise((r) => setTimeout(r, 100));
      return {
        modes: {
          currentModeId: 'vibe',
          availableModes: [{ id: 'vibe', name: 'Vibe' }],
        },
      };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/chat');
    await tc.sleepMs(300);
    await tc.waitForVisibleText('/chat', 5000);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText('Pick me to load', 5000);
    // The first option is highlighted by default; Enter selects it.
    await tc.sendKeys('\r');

    await tc.waitForVisibleText(`Loaded session ${targetSessionId}`, 5000);
    await tc.waitForVisibleText('Hello from history', 5000);
    await tc.waitForVisibleText('Reply from the past', 5000);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs.length).toBe(1);
    expect((loadReqs[0]!.params as { sessionId: string }).sessionId).toBe(
      targetSessionId
    );

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('Hello from history');
    expect(snapshot).toContain('Reply from the past');
  }, 30000);
});
