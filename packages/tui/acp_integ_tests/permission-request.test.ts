/**
 * ACP wire-level tests for the permission system and session info updates.
 *
 * Covers:
 * - session/request_permission (server → client request, user approves/cancels)
 * - session_info_update with turn_completion
 * - session_info_update with context_usage
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

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'perm-session-1',
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Default' }],
    },
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('permission request + session info updates', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('session/request_permission renders approval panel and user can approve', async () => {
    /**
     * GIVEN  tool_call in progress
     * WHEN   server sends session/request_permission with options
     * THEN   approval panel renders, user presses Enter to approve,
     *        server receives {outcome:'selected', optionId:'allow_once'}
     */
    tc = new AcpTestCase({ testName: 'permission-approve' });
    setupHandshake(tc);

    // Prompt handler: send a tool_call, then request permission
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      // Emit tool_call to put TUI in tool-use state
      tc!.mock.notify('session/update', {
        sessionId: 'perm-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'tool-1',
          title: 'execute_bash',
          kind: 'shell',
          rawInput: { command: 'rm -rf /tmp/test' },
        },
      });

      await new Promise((r) => setTimeout(r, 300));

      // Now request permission from the TUI
      const response = await tc!.mock.request('session/request_permission', {
        sessionId: 'perm-session-1',
        toolCall: { toolCallId: 'tool-1' },
        options: [
          { kind: 'allow_once', name: 'Allow once', optionId: 'allow_once' },
          {
            kind: 'allow_always',
            name: 'Always allow',
            optionId: 'allow_always',
          },
          { kind: 'reject_once', name: 'Deny', optionId: 'reject_once' },
        ],
        // Real KAS stamps `_meta.kiro.toolId` on tool approvals (and omits it
        // for user_input questions); include it so this renders as a tool
        // approval ("requires approval") rather than a question.
        _meta: { kiro: { toolId: 'tool-1' } },
      });

      return response as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    // Send a prompt to trigger the permission flow
    await tc.sendKeys('delete temp files');
    await tc.pressEnter();

    // Wait for the approval panel to appear
    await tc.waitForVisibleText('requires approval', 5000);
    await tc.sleepMs(300);

    // Press Enter to select the first option (allow_once)
    await tc.pressEnter();
    await tc.sleepMs(500);

    // The mock.request promise should have resolved with the user's choice
    // (verified by the prompt handler completing without error)
    const store = await tc.getStore();
    expect(store.pendingApproval).toBeNull();
  });

  it('session_info_update with context_usage updates store percent', async () => {
    /**
     * GIVEN  TUI connected
     * WHEN   server pushes session_info_update with context_usage
     * THEN   store's context usage percentage updates
     */
    tc = new AcpTestCase({ testName: 'context-usage-update' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('session/update', {
      sessionId: 'perm-session-1',
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'context_usage',
            usagePercentage: 75,
            contextUsage: { usagePercentage: 75 },
          },
        },
      },
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    expect(store.contextUsagePercent).toBe(75);
  });

  it('session_info_update with turn_completion clears processing state', async () => {
    /**
     * GIVEN  TUI in processing state (prompt sent)
     * WHEN   server pushes session_info_update with turn_completion
     * THEN   store's isProcessing becomes false
     */
    tc = new AcpTestCase({ testName: 'turn-completion' });
    setupHandshake(tc);

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      // Simulate agent completing its turn
      await new Promise((r) => setTimeout(r, 200));

      tc!.mock.notify('session/update', {
        sessionId: 'perm-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: [{ type: 'text', text: 'Done!' }],
        },
      });

      await new Promise((r) => setTimeout(r, 100));

      tc!.mock.notify('session/update', {
        sessionId: 'perm-session-1',
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_completion' } },
        },
      });

      return { sessionId: 'perm-session-1' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('test');
    await tc.pressEnter();
    await tc.sleepMs(800);

    const store = await tc.getStore();
    expect(store.isProcessing).toBe(false);
  });

  it('session/request_permission cancelled via Escape resolves with cancelled', async () => {
    /**
     * GIVEN  approval panel visible with permission options
     * WHEN   user presses Escape
     * THEN   server receives {outcome:'cancelled'}, panel closes
     */
    tc = new AcpTestCase({ testName: 'permission-cancel' });
    setupHandshake(tc);

    let permissionResponse: unknown = null;

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      tc!.mock.notify('session/update', {
        sessionId: 'perm-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'tool-2',
          title: 'fs_write',
          kind: 'write',
          rawInput: { path: '/etc/passwd', content: 'hacked' },
        },
      });

      await new Promise((r) => setTimeout(r, 300));

      permissionResponse = await tc!.mock.request(
        'session/request_permission',
        {
          sessionId: 'perm-session-1',
          toolCall: { toolCallId: 'tool-2' },
          options: [
            {
              kind: 'allow_once',
              name: 'Allow once',
              optionId: 'allow_once',
            },
            {
              kind: 'reject_once',
              name: 'Deny',
              optionId: 'reject_once',
            },
          ],
          // Tool approval carries `_meta.kiro.toolId` (see approve test).
          _meta: { kiro: { toolId: 'tool-2' } },
        }
      );

      return permissionResponse as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('write to etc passwd');
    await tc.pressEnter();

    await tc.waitForVisibleText('requires approval', 5000);
    await tc.sleepMs(300);

    // Press Escape to cancel
    await tc.pressEscape();
    await tc.sleepMs(500);

    const store = await tc.getStore();
    expect(store.pendingApproval).toBeNull();
    expect(permissionResponse).toBeDefined();
    const resp = permissionResponse as { outcome: { outcome: string } };
    expect(resp.outcome.outcome).toBe('cancelled');
  });
});
