/**
 * ACP wire-level tests for:
 * - _kiro/mcp/status notification (server online + tools registered)
 * - agent_thought_chunk (thinking/reasoning display)
 *
 * These are the remaining session update types that affect user-visible UI.
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

function setupHandshake(tc: AcpTestCase, sessionId = 'mcp-session-1'): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId,
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Default' }],
    },
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

describe('MCP status + thinking', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('_kiro/mcp/status with running servers is accessible via /mcp command', async () => {
    /**
     * GIVEN  TUI connected
     * WHEN   server pushes _kiro/mcp/status with connected MCP servers
     * THEN   the data is cached and /mcp shows server count
     *
     * Note: _kiro/mcp/status populates KasAcpClient.mcpServerCache (used
     * by executeCommand('mcp')), not the store's mcpServers directly.
     * The store is only updated via McpServerInitialized events from the
     * init flow.
     */
    tc = new AcpTestCase({ testName: 'mcp-status-running' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(500);

    tc.mock.notify('_kiro/mcp/status', {
      sessionId: 'mcp-session-1',
      servers: [
        {
          name: 'github-mcp',
          status: 'connected',
          tools: [
            { name: 'search_repos', disabled: false },
            { name: 'get_file', disabled: false },
          ],
        },
        {
          name: 'memory-mcp',
          status: 'connected',
          tools: [{ name: 'learn', disabled: false }],
        },
      ],
    });
    await tc.sleepMs(300);

    // /mcp reads from the client cache populated by the notification
    await tc.sendKeys('/mcp');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    // The MCP panel should show server names
    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('github-mcp');
    expect(snap).toContain('memory-mcp');
  });

  it('agent_thought_chunk renders thinking content in store', async () => {
    /**
     * GIVEN  TUI ready, prompt sent
     * WHEN   server pushes agent_thought_chunk with reasoning text
     * THEN   thinking content appears in store messages (as model message with thinking)
     */
    tc = new AcpTestCase({ testName: 'thinking-chunk' });
    setupHandshake(tc);

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      // Send thinking first
      tc!.mock.notify('session/update', {
        sessionId: 'mcp-session-1',
        update: {
          sessionUpdate: 'agent_thought_chunk',
          content: {
            type: 'text',
            text: 'Let me think about this carefully...',
          },
        },
      });
      await new Promise((r) => setTimeout(r, 200));

      // Then the actual response
      tc!.mock.notify('session/update', {
        sessionId: 'mcp-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Here is my answer.' },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      tc!.mock.notify('session/update', {
        sessionId: 'mcp-session-1',
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

    await tc.sendKeys('think hard');
    await tc.pressEnter();
    await tc.waitForVisibleText('Here is my answer', 10000);

    // The thinking text should be visible on screen
    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('think about this');
  });
});
