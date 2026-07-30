/**
 * ACP wire-level tests for /tangent command (tangent create/switch/back).
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

// /tangent is feature-gated (internal nightly); the spawned TUI only
// registers the command when the launcher-provided env enables it.
const TANGENT_ENV = { KIRO_ENABLED_FEATURES: JSON.stringify(['tangent']) };

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'root-session',
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

/**
 * Send one user turn with a mocked reply so the conversation has messages.
 * The tangent create path refuses to fork an empty conversation (the
 * empty-chat guard), matching KAS's own NO_FORK_POINT behavior.
 */
async function converseOnce(tc: AcpTestCase, sessionId: string): Promise<void> {
  tc.mock.on('session/prompt', () => {
    tc.mock.notify('session/update', {
      sessionId,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'ok' },
      },
    });
    return { stopReason: 'end_turn' };
  });
  await tc.sendKeys('hello');
  await tc.pressEnter();
  await tc.waitForStore((s) => !s.isProcessing, 10000);
}

describe('/tangent command (tangent)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('/tangent <name> calls session/fork with createdReason tangent and title', async () => {
    tc = new AcpTestCase({ testName: 'tangent-create', extraEnv: TANGENT_ENV });
    setupHandshake(tc);

    tc.mock.on('session/list', () => ({
      sessions: [{ sessionId: 'root-session', title: 'Main', cwd: '/tmp' }],
    }));
    tc.mock.on('session/fork', () => ({ sessionId: 'tangent-session-1' }));
    tc.mock.on('session/load', () => ({
      sessionId: 'tangent-session-1',
      modes: defaultKasModes(),
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await converseOnce(tc, 'root-session');

    // Create a tangent
    await tc.sendKeys('/tangent experiment');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(1500);

    const forkReqs = tc.mock.receivedRequests('session/fork');
    expect(forkReqs.length).toBeGreaterThanOrEqual(1);
    const params = forkReqs[0]!.params as {
      sessionId: string;
      _meta?: { kiro?: { createdReason?: string; title?: string } };
    };
    expect(params.sessionId).toBe('root-session');
    expect(params._meta?.kiro?.createdReason).toBe('tangent');
    expect(params._meta?.kiro?.title).toBe('experiment');
  });

  it('/tangent (no args) auto-creates tangent when on root', async () => {
    tc = new AcpTestCase({ testName: 'tangent-back', extraEnv: TANGENT_ENV });
    setupHandshake(tc);

    // Mock the initial session as a child of root (KAS puts parentSessionId in _meta.kiro)
    tc.mock.on('session/list', () => ({
      sessions: [
        { sessionId: 'root-session', title: 'Main', cwd: '/tmp' },
        {
          sessionId: 'child-session',
          title: 'experiment',
          cwd: '/tmp',
          _meta: { kiro: { parentSessionId: 'root-session' } },
        },
      ],
    }));
    tc.mock.on('session/load', () => ({
      sessionId: 'root-session',
      modes: defaultKasModes(),
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await converseOnce(tc, 'root-session');

    // TUI's sessionId is 'root-session' from session/new, so
    // /tangent with no args should auto-create a tangent
    tc.mock.on('session/fork', () => ({ sessionId: 'auto-tangent-1' }));
    tc.mock.on('session/load', () => ({
      sessionId: 'auto-tangent-1',
      modes: defaultKasModes(),
    }));

    await tc.sendKeys('/tangent');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    // Should auto-create tangent-1 since we're on root
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('tangent-1');
  });

  it('/tangent <existing-name> calls session/load to switch', async () => {
    tc = new AcpTestCase({ testName: 'tangent-switch', extraEnv: TANGENT_ENV });
    setupHandshake(tc);

    tc.mock.on('session/list', () => ({
      sessions: [
        { sessionId: 'root-session', title: 'Main', cwd: '/tmp' },
        {
          sessionId: 'other-tangent',
          title: 'refactor',
          cwd: '/tmp',
          _meta: { kiro: { parentSessionId: 'root-session' } },
        },
      ],
    }));
    tc.mock.on('session/load', () => ({
      sessionId: 'other-tangent',
      modes: defaultKasModes(),
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('/tangent refactor');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(1500);

    const loadReqs = tc.mock.receivedRequests('session/load');
    // Should have loaded the 'refactor' session
    const switchReq = loadReqs.find(
      (r: any) => r.params?.sessionId === 'other-tangent'
    );
    expect(switchReq).toBeDefined();
  });

  it('/tangent ls shows the tangent tree inline', async () => {
    tc = new AcpTestCase({ testName: 'tangent-ls', extraEnv: TANGENT_ENV });
    setupHandshake(tc);

    tc.mock.on('session/list', () => ({
      sessions: [
        { sessionId: 'root-session', title: 'Main', cwd: '/tmp' },
        {
          sessionId: 'child-1',
          title: 'experiment',
          cwd: '/tmp',
          _meta: { kiro: { parentSessionId: 'root-session' } },
        },
        {
          sessionId: 'child-2',
          title: 'refactor',
          cwd: '/tmp',
          _meta: { kiro: { parentSessionId: 'root-session' } },
        },
      ],
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('/tangent ls');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(1500);

    // Tree should be rendered inline as a system message
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('root');
    expect(snapshot).toContain('experiment');
    expect(snapshot).toContain('refactor');
    expect(snapshot).toContain('current');
  });
});
