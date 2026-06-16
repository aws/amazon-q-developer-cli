/**
 * Verifies that the TUI transformation layer strips KAS MCP tool titles
 * and unwraps the KAS output envelope for V2 parity.
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

function setupHandshake(tc: AcpTestCase, sessionId = 'test-session-1'): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

describe('MCP tool title and output transformation', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('strips @serverName/ prefix and unwraps KAS MCP output', async () => {
    tc = new AcpTestCase({ testName: 'mcp-transform' });
    setupHandshake(tc);

    // When the TUI sends a prompt, respond with a tool_call then complete it
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      // Send tool_call start with KAS-style @server/tool title
      tc!.mock.notify('session/update', {
        sessionId: 'test-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'call-1',
          title: '@test-mock/echo',
          kind: 'other',
          rawInput: { message: 'hello' },
        },
      });

      await new Promise((r) => setTimeout(r, 200));

      // Send tool_call_update with KAS-style output envelope
      tc!.mock.notify('session/update', {
        sessionId: 'test-session-1',
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'call-1',
          status: 'completed',
          rawOutput: {
            response: '{"echoed": "hello"}',
            imageBase64Urls: [],
          },
        },
      });

      await new Promise((r) => setTimeout(r, 200));

      // Send agent message to finish the turn
      tc!.mock.notify('session/update', {
        sessionId: 'test-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Done' },
        },
      });

      await new Promise((r) => setTimeout(r, 100));
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // Send a user message to trigger the prompt
    await tc.sendKeys('test mcp');
    await tc.sleepMs(100);
    await tc.pressEnter();
    // Wait for the agent's "Done" response to appear
    await tc.waitForVisibleText('Done', 10000);
    await tc.sleepMs(500);

    const store = await tc.getStore();
    const toolMsg = store.messages.find(
      (m) => m.role === 'tool_use' && m.id === 'call-1'
    ) as any;
    expect(toolMsg).toBeDefined();
    // Title should be stripped to just "echo"
    expect(toolMsg.name).toBe('echo');
    // Tool should be finished with output unwrapped to V2 content format
    expect(toolMsg.isFinished).toBe(true);
    expect(toolMsg.result).toBeDefined();
    expect(toolMsg.result.status).toBe('success');
    expect(toolMsg.result.output).toEqual({
      content: [{ type: 'text', text: '{"echoed": "hello"}' }],
    });
  });
});
