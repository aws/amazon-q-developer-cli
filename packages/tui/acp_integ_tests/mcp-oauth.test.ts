/**
 * E2E test for MCP OAuth flow via `_kiro/openExternalUrl`.
 *
 * Verifies the full pipeline:
 *   mock ACP server sends `_kiro/mcp/status` (failedAuthorization)
 *   → KasAcpClient auto-triggers `_kiro/mcp/resetServer`
 *   → mock server sends `_kiro/openExternalUrl` request
 *   → KasAcpClient correlates URL to server, broadcasts McpOauthRequest
 *   → app store populates `pendingOAuthServers`
 */
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

describe('MCP OAuth via openExternalUrl', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('advertises openExternalUrl in initialize clientCapabilities', async () => {
    tc = new AcpTestCase({ testName: 'mcp-oauth-advertise' });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    const initReqs = tc.mock.receivedRequests('initialize');
    expect(initReqs).toHaveLength(1);
    const params = initReqs[0]!.params as any;
    // clientMeta merges into clientCapabilities._meta.kiro
    expect(params.clientCapabilities?._meta?.kiro?.openExternalUrl).toBe(true);
  });

  it('full OAuth flow: failedAuth → resetServer → openExternalUrl → pendingOAuthServers', async () => {
    tc = new AcpTestCase({ testName: 'mcp-oauth-full-flow' });
    setupHandshake(tc);

    // Handle the resetServer call that KasAcpClient will auto-trigger
    tc.mock.on('_kiro/mcp/resetServer', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    // 1. Send MCP status with failedAuthorization → triggers resetServer
    tc.mock.notify('_kiro/mcp/status', {
      sessionId: 'test-session-1',
      servers: [
        {
          name: 'github-mcp',
          status: 'failed',
          failedAuthorization: true,
          errorMessage: 'Unauthorized',
        },
      ],
    });

    // Wait for the reset to be triggered
    await new Promise((r) => setTimeout(r, 500));

    // Verify resetServer was called with startOAuth: true
    const resetReqs = tc.mock.receivedRequests('_kiro/mcp/resetServer');
    expect(resetReqs.length).toBeGreaterThanOrEqual(1);
    expect(resetReqs[0]!.params).toMatchObject({
      serverName: 'github-mcp',
      startOAuth: true,
    });

    // 2. KAS sends _kiro/openExternalUrl request (agent → client)
    const response = await tc.mock.request('_kiro/openExternalUrl', {
      url: 'https://github.com/login/oauth/authorize?state=abc123',
      serverName: 'github-mcp',
    });
    expect(response).toEqual({ success: true });

    // Wait for store to update
    await new Promise((r) => setTimeout(r, 300));

    // 3. Verify pendingOAuthServers is populated in the store
    const store = await tc.getStore();
    const pending = (store as any).pendingOAuthServers;
    expect(pending).toBeDefined();
    expect(pending['github-mcp']).toBe(
      'https://github.com/login/oauth/authorize?state=abc123'
    );
  });

  it('openExternalUrl with no pending server returns success: false', async () => {
    tc = new AcpTestCase({ testName: 'mcp-oauth-no-pending' });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    // Send openExternalUrl without any prior failedAuthorization
    const response = await tc.mock.request('_kiro/openExternalUrl', {
      url: 'https://example.com/orphan',
    });
    expect(response).toEqual({ success: false });
  });
});
