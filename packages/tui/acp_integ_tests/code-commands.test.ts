/**
 * ACP wire-level tests for `/code` command (`_kiro/codeIntelligence`).
 *
 * Covers all 3 subcommands: status, init, overview.
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

interface CodeIntelParams {
  sessionId: string;
  subcommand: string;
}

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: {
        kiro: {
          extensionMethods: [
            {
              method: '_kiro/codeIntelligence',
              name: '/code',
              description: 'Code intelligence',
            },
          ],
        },
      },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'code-session-1',
    modes: defaultKasModes(),
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('/code command', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('/code status sends _kiro/codeIntelligence with subcommand status', async () => {
    /**
     * GIVEN  _kiro/codeIntelligence advertised
     * WHEN   user types /code status
     * THEN   wire sends {subcommand:'status', sessionId}
     */
    tc = new AcpTestCase({ testName: 'code-status' });
    setupHandshake(tc);

    tc.mock.on('_kiro/codeIntelligence', () => ({
      success: true,
      data: { status: 'ready', languages: ['typescript', 'rust'] },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/code status');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/codeIntelligence');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const params = reqs[0]!.params as CodeIntelParams;
    expect(params.subcommand).toBe('status');
    expect(params.sessionId).toBe('code-session-1');
  });

  it('/code init sends subcommand init', async () => {
    /**
     * GIVEN  _kiro/codeIntelligence advertised
     * WHEN   user types /code init
     * THEN   wire sends {subcommand:'init', sessionId}
     */
    tc = new AcpTestCase({ testName: 'code-init' });
    setupHandshake(tc);

    tc.mock.on('_kiro/codeIntelligence', () => ({
      success: true,
      data: { message: 'LSP initialized' },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/code init');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/codeIntelligence');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const initReq = reqs.find(
      (r) => (r.params as CodeIntelParams).subcommand === 'init'
    );
    expect(initReq).toBeDefined();
  });

  it('/code overview sends subcommand overview', async () => {
    /**
     * GIVEN  _kiro/codeIntelligence advertised
     * WHEN   user types /code overview
     * THEN   wire sends {subcommand:'overview', sessionId}
     */
    tc = new AcpTestCase({ testName: 'code-overview' });
    setupHandshake(tc);

    tc.mock.on('_kiro/codeIntelligence', () => ({
      success: true,
      data: { overview: '42 files, 3 languages' },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/code overview');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/codeIntelligence');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const overviewReq = reqs.find(
      (r) => (r.params as CodeIntelParams).subcommand === 'overview'
    );
    expect(overviewReq).toBeDefined();
  });
});
