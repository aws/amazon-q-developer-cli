/**
 * ACP integration test: full tangent lifecycle flow.
 *
 * Tests the critical sequential user journey:
 *   1. Create a tangent (/tan experiment)
 *   2. Go back to parent (/tan)
 *   3. List tangents and see the tree (/tangent ls)
 *
 * Unlike the per-operation tests in tangent-command.test.ts, this test verifies
 * the full round-trip: create → load → switch back → list shows both sessions.
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

describe('/tangent full lifecycle flow', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('creates a tangent, switches back, and lists both sessions', async () => {
    tc = new AcpTestCase({ testName: 'tangent-flow', extraEnv: TANGENT_ENV });

    // --- Handshake ---
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

    // Track which session the TUI thinks is current
    let currentSession = 'root-session';

    // Phase 1: /tan experiment → calls session/list, then session/fork, then session/load
    // session/list initially returns only root (no tangents exist yet)
    tc.mock.on('session/list', () => ({
      sessions: [{ sessionId: 'root-session', title: 'Main', cwd: '/tmp' }],
    }));

    // Fork creates the tangent
    tc.mock.on('session/fork', (params: any) => {
      // Verify fork params are correct
      expect(params.sessionId).toBe('root-session');
      expect(params._meta?.kiro?.createdReason).toBe('tangent');
      expect(params._meta?.kiro?.title).toBe('experiment');
      return { sessionId: 'tangent-experiment' };
    });

    // After fork, tangent handler calls loadSession on the new session
    tc.mock.on('session/load', (params: any) => {
      currentSession = params.sessionId;
      return {
        sessionId: params.sessionId,
        modes: defaultKasModes(),
      };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await converseOnce(tc, 'root-session');

    // --- Step 1: Create tangent ---
    await tc.sendKeys('/tangent experiment');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    // Verify fork was called
    const forkReqs = tc.mock.receivedRequests('session/fork');
    expect(forkReqs.length).toBe(1);

    // Verify session/load was called with the new tangent ID
    const loadReqs1 = tc.mock.receivedRequests('session/load');
    const tangentLoad = loadReqs1.find(
      (r: any) => r.params?.sessionId === 'tangent-experiment'
    );
    expect(tangentLoad).toBeDefined();

    // Verify TUI shows success
    const snap1 = tc.getSnapshotFormatted();
    expect(snap1).toContain('experiment');

    // --- Step 2: Go back to parent ---
    // Update session/list to now include the tangent (KAS wire format: parentSessionId in _meta.kiro)
    tc.mock.on('session/list', () => ({
      sessions: [
        { sessionId: 'root-session', title: 'Main', cwd: '/tmp' },
        {
          sessionId: 'tangent-experiment',
          title: 'experiment',
          cwd: '/tmp',
          _meta: { kiro: { parentSessionId: 'root-session' } },
        },
      ],
    }));

    await tc.sendKeys('/tangent');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    // Verify session/load was called with parent session
    const loadReqs2 = tc.mock.receivedRequests('session/load');
    const backLoad = loadReqs2.find(
      (r: any) => r.params?.sessionId === 'root-session'
    );
    expect(backLoad).toBeDefined();

    // Verify TUI shows "Returned to parent"
    const snap2 = tc.getSnapshotFormatted();
    expect(snap2).toContain('root');

    // --- Step 3: List tangents ---
    await tc.sendKeys('/tangent ls');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    // Verify tree is shown with both sessions
    const snap3 = tc.getSnapshotFormatted();
    expect(snap3).toContain('root');
    expect(snap3).toContain('experiment');
    expect(snap3).toContain('current');
  }, 30000);

  it('/tangent from root auto-creates a tangent instead of crashing', async () => {
    tc = new AcpTestCase({
      testName: 'tangent-flow-root-back',
      extraEnv: TANGENT_ENV,
    });

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

    // Root has no parent
    tc.mock.on('session/list', () => ({
      sessions: [{ sessionId: 'root-session', title: 'Main', cwd: '/tmp' }],
    }));

    // Mock fork+load for auto-create
    tc.mock.on('session/fork', () => ({ sessionId: 'auto-tangent-1' }));
    tc.mock.on('session/load', () => ({
      sessionId: 'auto-tangent-1',
      modes: defaultKasModes(),
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await converseOnce(tc, 'root-session');

    // /tangent from root should auto-create a tangent, not crash
    await tc.sendKeys('/tangent');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('tangent-1');
  }, 20000);

  it('creating a tangent with same name as existing switches to it instead', async () => {
    tc = new AcpTestCase({
      testName: 'tangent-flow-switch-existing',
      extraEnv: TANGENT_ENV,
    });

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

    // Session list already has a tangent named "refactor" (KAS wire format)
    tc.mock.on('session/list', () => ({
      sessions: [
        { sessionId: 'root-session', title: 'Main', cwd: '/tmp' },
        {
          sessionId: 'tangent-refactor',
          title: 'refactor',
          cwd: '/tmp',
          _meta: { kiro: { parentSessionId: 'root-session' } },
        },
      ],
    }));

    tc.mock.on('session/load', () => ({
      sessionId: 'tangent-refactor',
      modes: defaultKasModes(),
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // /tan refactor should switch (not fork) since name exists
    await tc.sendKeys('/tangent refactor');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    // NO fork should have been called
    const forkReqs = tc.mock.receivedRequests('session/fork');
    expect(forkReqs.length).toBe(0);

    // session/load should have been called with the existing tangent
    const loadReqs = tc.mock.receivedRequests('session/load');
    const switchReq = loadReqs.find(
      (r: any) => r.params?.sessionId === 'tangent-refactor'
    );
    expect(switchReq).toBeDefined();
  }, 20000);
});
