/**
 * ACP integration: the Explorer /tangent selection path (switch by session id).
 *
 * The picker dispatches the selected row's SESSION ID (see
 * useBackendPanelHandlers.handleTangentSelect + tangent.ts switchOrCreate's
 * id-path). These tests drive that dispatch over the real KAS wire and assert
 * which session/load fires — reproducing the two bugs the unit layer couldn't:
 *   1. Selecting root from a deep node jumped to the immediate PARENT.
 *   2. Selecting the current row CREATED a new tangent.
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

function handshake(tc: AcpTestCase) {
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
  tc.mock.on('session/load', (params: any) => ({
    sessionId: params.sessionId,
    modes: defaultKasModes(),
  }));
  // A tree: root -> ec2 -> t3 (linear). KAS wire format: parentSessionId in _meta.kiro.
  tc.mock.on('session/list', () => ({
    sessions: [
      { sessionId: 'root-session', title: 'Main', cwd: '/tmp' },
      {
        sessionId: 'ec2-session',
        title: 'ec2',
        cwd: '/tmp',
        _meta: { kiro: { parentSessionId: 'root-session' } },
      },
      {
        sessionId: 't3-session',
        title: 't3',
        cwd: '/tmp',
        _meta: { kiro: { parentSessionId: 'ec2-session' } },
      },
    ],
  }));
}

describe('/tangent Explorer selection (switch by session id)', () => {
  let tc: AcpTestCase | null = null;
  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('selecting root from a deep node jumps to root, not the immediate parent', async () => {
    tc = new AcpTestCase({
      testName: 'tangent-select-root-jump',
      extraEnv: TANGENT_ENV,
    });
    handshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // Move current to the deep node t3 (Explorer dispatches its session id).
    await tc.sendKeys('/tangent t3-session');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    // Now select the root row — the Explorer dispatches root's session id.
    await tc.sendKeys('/tangent root-session');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    const loads = tc.mock
      .receivedRequests('session/load')
      .map((r: any) => r.params?.sessionId);

    // Landed on root...
    expect(loads).toContain('root-session');
    // ...NOT the immediate parent (ec2) — the old "go back one level" bug.
    expect(loads).not.toContain('ec2-session');
    // Selecting an existing session never forks.
    expect(tc.mock.receivedRequests('session/fork').length).toBe(0);
  }, 30000);

  it('selecting the current session is a no-op (no fork, no load)', async () => {
    tc = new AcpTestCase({
      testName: 'tangent-select-current-noop',
      extraEnv: TANGENT_ENV,
    });
    handshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // Current session is root-session (from session/new). Select it.
    await tc.sendKeys('/tangent root-session');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    // No switch and no create — selecting the row you're on does nothing.
    expect(tc.mock.receivedRequests('session/load').length).toBe(0);
    expect(tc.mock.receivedRequests('session/fork').length).toBe(0);
  }, 30000);

  it('selecting a sibling tangent switches to that exact session', async () => {
    tc = new AcpTestCase({
      testName: 'tangent-select-sibling',
      extraEnv: TANGENT_ENV,
    });
    handshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    // From root, select the deep t3 node directly by id.
    await tc.sendKeys('/tangent t3-session');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(2000);

    const loads = tc.mock
      .receivedRequests('session/load')
      .map((r: any) => r.params?.sessionId);
    expect(loads).toContain('t3-session');
    expect(tc.mock.receivedRequests('session/fork').length).toBe(0);
  }, 30000);
});
