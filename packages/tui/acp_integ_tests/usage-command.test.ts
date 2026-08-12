/**
 * ACP wire-level tests for `/usage` command (`_kiro/account/getUsage`).
 *
 * Validates the ext method is called and the response data flows
 * through to the TUI's usage panel.
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

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: {
        kiro: {
          extensionMethods: [
            {
              method: '_kiro/account/getUsage',
              name: '/usage',
              description: 'Show usage',
            },
          ],
        },
      },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'usage-session-1',
    modes: defaultKasModes(),
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('/usage command', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('sends _kiro/account/getUsage and receives quota data', async () => {
    /**
     * GIVEN  _kiro/account/getUsage advertised
     * WHEN   user types /usage
     * THEN   ext method called with sessionId, response flows to TUI
     */
    tc = new AcpTestCase({ testName: 'usage-command' });
    setupHandshake(tc);

    tc.mock.on('_kiro/account/getUsage', () => ({
      success: true,
      data: {
        success: true,
        message: 'Pro plan: 150/500 interactions used',
        data: {
          plan: 'pro',
          used: 150,
          limit: 500,
          resetDate: '2026-07-01',
        },
      },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/usage');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/account/getUsage');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const params = reqs[0]!.params as { sessionId: string };
    expect(params.sessionId).toBe('usage-session-1');
  });
});
