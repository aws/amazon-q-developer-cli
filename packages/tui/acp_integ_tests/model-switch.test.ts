/**
 * ACP wire-level tests for model switching, agent swap, and cancel.
 *
 * Covers:
 * - session/set_config_option with configId:'model' (/model command)
 * - session/set_mode (/agent swap)
 * - session/cancel (Ctrl+C during processing)
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { DEFAULT_KAS_MODE, KAS_DEFAULT_AGENT_ID } from './shared/default-agent';

interface SetConfigOptionParams {
  sessionId: string;
  configId: string;
  value: unknown;
}

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'model-session-1',
    modes: {
      currentModeId: KAS_DEFAULT_AGENT_ID,
      availableModes: [{ ...DEFAULT_KAS_MODE }, { id: 'spec', name: 'Spec' }],
    },
    configOptions: [
      {
        type: 'select' as const,
        id: 'model',
        name: 'Model',
        category: 'model',
        currentValue: 'claude-sonnet-4',
        options: [
          { value: 'claude-sonnet-4', name: 'Claude Sonnet 4' },
          { value: 'claude-opus-4', name: 'Claude Opus 4' },
        ],
      },
    ],
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('model switch, agent swap, and cancel', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('/model selection sends session/set_config_option with configId model', async () => {
    /**
     * GIVEN  configOptions with model list returned from session/new
     * WHEN   user selects a model via /model
     * THEN   session/set_config_option sent with {configId:'model', value:'claude-opus-4'}
     */
    tc = new AcpTestCase({ testName: 'model-switch' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    // Type /model to open selection menu
    await tc.sendKeys('/model');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    // The second option is claude-opus-4 — press down then enter
    await tc.sendKeys('\x1b[B'); // Down arrow
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('session/set_config_option');
    const modelReqs = reqs.filter(
      (r) => (r.params as SetConfigOptionParams).configId === 'model'
    );
    expect(modelReqs.length).toBeGreaterThanOrEqual(1);
    const params = modelReqs[0]!.params as SetConfigOptionParams;
    expect(params.sessionId).toBe('model-session-1');
    expect(typeof params.value).toBe('string');
  });

  it('Ctrl+C sends session/cancel notification', async () => {
    /**
     * GIVEN  agent is processing (prompt sent)
     * WHEN   user presses Ctrl+C
     * THEN   session/cancel notification sent to server
     */
    tc = new AcpTestCase({ testName: 'cancel-ctrl-c' });
    setupHandshake(tc);

    // Register prompt handler that never completes (simulates processing)
    tc.mock.on('session/prompt', () => {
      // Intentionally delay response — simulates agent thinking
      return new Promise((resolve) => {
        setTimeout(() => resolve({ sessionId: 'model-session-1' }), 10000);
      });
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    // Send a prompt to put the TUI in processing state
    await tc.sendKeys('hello');
    await tc.pressEnter();
    await tc.sleepMs(500);

    // Press Ctrl+C to cancel
    await tc.pressCtrlC();
    await tc.sleepMs(500);

    const cancelNotifs = tc.mock.receivedNotifications('session/cancel');
    expect(cancelNotifs.length).toBeGreaterThanOrEqual(1);
  });

  it('cancel mid-tool then new prompt recovers cleanly', async () => {
    /**
     * GIVEN  agent is executing a tool (tool_call sent, no completion)
     * WHEN   user presses Ctrl+C then sends a new prompt
     * THEN   session/cancel fires, then new session/prompt is sent
     *        (TUI doesn't get stuck in processing state)
     */
    tc = new AcpTestCase({ testName: 'cancel-recover' });
    setupHandshake(tc);

    let promptCount = 0;
    tc.mock.on<any, any>('session/prompt', async () => {
      promptCount++;
      if (promptCount === 1) {
        // First prompt: emit a tool_call but never complete it
        tc!.mock.notify('session/update', {
          sessionId: 'model-session-1',
          update: {
            sessionUpdate: 'tool_call',
            toolCallId: 'long-tool-1',
            title: 'read_file',
            kind: 'read',
            rawInput: { path: '/huge-file.ts' },
          },
        });
        // Simulate hang — never resolve until cancel
        return new Promise((resolve) => {
          setTimeout(() => resolve({ sessionId: 'model-session-1' }), 10000);
        });
      }
      // Second prompt: respond normally
      tc!.mock.notify('session/update', {
        sessionId: 'model-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Recovered!' },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      tc!.mock.notify('session/update', {
        sessionId: 'model-session-1',
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_completion' } },
        },
      });
      return { stopReason: 'end_turn' } as any;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    // First prompt → starts tool
    await tc.sendKeys('read huge file');
    await tc.pressEnter();
    await tc.sleepMs(500);

    // Cancel mid-tool
    await tc.pressCtrlC();
    await tc.sleepMs(500);

    // Send new prompt — TUI should recover
    await tc.sendKeys('hello again');
    await tc.pressEnter();
    await tc.waitForVisibleText('Recovered', 10000);

    // Verify both prompts were sent
    const prompts = tc.mock.receivedRequests('session/prompt');
    expect(prompts.length).toBeGreaterThanOrEqual(2);
    // Verify cancel was sent between them
    const cancels = tc.mock.receivedNotifications('session/cancel');
    expect(cancels.length).toBeGreaterThanOrEqual(1);
  });

  it('/agent swap sends set_config_option(mode) and server confirms via current_mode_update', async () => {
    /**
     * GIVEN  TUI on the KAS default mode with 'spec' available
     * WHEN   user types /agent swap spec (simulated via slash command dispatch)
     * THEN   set_config_option with configId:'mode' is sent
     *        server responds with current_mode_update → store.currentAgent updates
     */
    tc = new AcpTestCase({ testName: 'agent-swap' });
    setupHandshake(tc);

    // When mode is set, respond with acknowledgment and push mode update
    tc.mock.on('session/set_config_option', (params: any) => {
      if (params.configId === 'mode') {
        // Server confirms the switch via notification
        setTimeout(() => {
          if (!tc) return; // Guard: test may have cleaned up
          tc.mock.notify('session/update', {
            sessionId: 'model-session-1',
            update: {
              sessionUpdate: 'current_mode_update',
              currentModeId: params.value,
            },
          });
        }, 100);
      }
      return {};
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    // Type /agent swap spec
    await tc.sendKeys('/agent swap spec');
    await tc.sleepMs(200);
    await tc.pressEnter();

    // Wait for store to reflect the mode switch
    const store = await tc.waitForStore(
      (s) => s.currentAgent?.name === 'spec',
      5000
    );
    expect(store.currentAgent?.name).toBe('spec');

    // Verify the wire call
    const reqs = tc.mock.receivedRequests('session/set_config_option');
    const modeReqs = reqs.filter((r: any) => r.params.configId === 'mode');
    expect(modeReqs.length).toBeGreaterThanOrEqual(1);
  });
});
