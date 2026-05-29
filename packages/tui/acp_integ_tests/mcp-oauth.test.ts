/**
 * E2E test for MCP OAuth flow via `authorizationUrl` in `_kiro/mcp/status`.
 *
 * Verifies the pipeline:
 *   mock ACP server sends `_kiro/mcp/status` with failedAuthorization + authorizationUrl
 *   → KasAcpClient broadcasts McpOauthRequest
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

describe('MCP OAuth via authorizationUrl in status', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('broadcasts McpOauthRequest when status has failedAuthorization + authorizationUrl', async () => {
    tc = new AcpTestCase({ testName: 'mcp-oauth-authurl' });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    // Send MCP status with failedAuthorization + authorizationUrl
    tc.mock.notify('_kiro/mcp/status', {
      sessionId: 'test-session-1',
      servers: [
        {
          name: 'github-mcp',
          status: 'failed',
          failedAuthorization: true,
          authorizationUrl: 'https://github.com/login/oauth/authorize?state=abc123',
          errorMessage: 'Unauthorized',
        },
      ],
    });

    // Wait for store to update
    await new Promise((r) => setTimeout(r, 300));

    // Verify pendingOAuthServers is populated in the store
    const store = await tc.getStore();
    const pending = (store as any).pendingOAuthServers;
    expect(pending).toBeDefined();
    expect(pending['github-mcp']).toBe(
      'https://github.com/login/oauth/authorize?state=abc123'
    );
  });
});
