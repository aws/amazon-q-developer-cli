/**
 * Regression test: chained switch_to_execution in KAS plan mode.
 *
 * Bug: when the planner emits `current_mode_update` → vibe exec2 inside a
 * single session/prompt response, exec2 text renders but tool-use cards
 * do NOT appear in store.messages.
 *
 * The test asserts that a tool_call emitted after a current_mode_update
 * within the same session/prompt response creates a `tool_use` entry in
 * store.messages. The assertion SHOULD FAIL while the bug is present.
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

const SESSION_ID = 'switch-exec-session-1';

describe('switch_to_execution chained render', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('tool_use card renders after current_mode_update within same session/prompt', async () => {
    /**
     * GIVEN  KAS mode starts as 'plan'
     * WHEN   session/prompt response contains:
     *          1. planner agent_message_chunk
     *          2. current_mode_update → 'vibe'
     *          3. exec2 agent_message_chunk
     *          4. exec2 tool_call (fs_write, kind:'edit', _meta.kiro.toolOrigin:'agent')
     *          5. tool_call_update completed
     *          6. session_info_update turn_completion
     * THEN   store.messages contains tool_use with id 'w1'  ← FAILS while bug is present
     */
    tc = new AcpTestCase({ testName: 'switch-exec-render' });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: SESSION_ID,
      modes: {
        currentModeId: 'plan',
        availableModes: [
          { id: 'plan', name: 'Plan' },
          { id: 'vibe', name: 'Default' },
        ],
      },
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

      // 1. Planner text
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Plan approved, handing off.' },
        },
      });
      await delay(100);

      // 2. Mode switch: planner → vibe (exec2)
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'current_mode_update',
          currentModeId: 'vibe',
        },
      });
      await delay(100);

      // 3. exec2 text
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Implementing the plan.' },
        },
      });
      await delay(100);

      // 4. exec2 tool_call (no agentSubtaskId — direct agent tool, not pipeline)
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'w1',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/tmp/out.ts', content: 'export const x = 1;' },
          _meta: { kiro: { toolOrigin: 'agent' } },
        },
      });
      await delay(200);

      // 5. tool_call completed
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'w1',
          status: 'completed',
          rawOutput: { response: 'written' },
        },
      });
      await delay(100);

      // 6. Single turn_completion (planner turn_end + exec2 turn_start suppressed)
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
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
    await tc.sendKeys('do the thing');
    await tc.pressEnter();
    await tc.waitForVisibleText('Implementing the plan', 10000);

    // Wait for the tool_use to appear in the store (the turn may still be in flight
    // when the text first renders). Poll up to 5s.
    const store = await tc
      .waitForStore(
        (s: any) =>
          s.messages.some((m: any) => m.role === 'tool_use' && m.id === 'w1'),
        5000,
        100
      )
      .catch(() => tc!.getStore());

    // Log observed message roles/ids for diagnostics
    const summary = store.messages.map((m: any) => ({
      role: m.role,
      id: m.id ?? null,
      name: m.name ?? null,
      agentName: m.agentName ?? null,
    }));
    console.log('store.messages:', JSON.stringify(summary, null, 2));

    // STORE ASSERTION — the tool_use must be in the store
    const toolMsg = store.messages.find(
      (m: any) => m.role === 'tool_use' && m.id === 'w1'
    );
    expect(toolMsg).toBeDefined();

    // RENDER ASSERTION — the tool card must be visually rendered in the terminal.
    // fs_write renders via the Write component which shows "Write" as its title
    // (getToolLabel('write') = 'Write'). Also visible: the file path '/tmp/out.ts'.
    // This assertion FAILS while the bug is present:
    //   store has the message but isSubagentToolCall() returns true because
    //   toolMsg.agentName ('kiro_default') !== mainAgentName ('kiro_planner'),
    //   which causes ConversationView to skip rendering the tool card entirely.
    await tc.waitForVisibleText('Write', 5000).catch(async () => {
      const snapshot = tc!.getSnapshot();
      console.log('[RENDER BUG] tool card NOT visible. Terminal snapshot:');
      console.log(snapshot.join('\n'));
      throw new Error(
        'Tool card for fs_write not rendered in terminal. ' +
          `store.messages has tool_use (agentName=${(toolMsg as any).agentName}), ` +
          'but isSubagentToolCall() hides it because agentName !== mainAgentName. ' +
          'See ConversationView.tsx isSubagentToolCall().'
      );
    });
  });

  it('[A] tool_use card renders with current_mode_update + welcomeMessage', async () => {
    /**
     * Variant A: mode switch WITH welcomeMessage on the vibe mode.
     * The AgentSwitched event carries welcomeMessage → setCurrentAgent inserts
     * a standalone Model message mid-turn → ConversationView turn-split →
     * executor ToolCalls become orphaned turns not rendered.
     *
     * If this FAILS but variant B passes → mechanism 2 (welcomeMessage turn-split) is the cause.
     * If both A and B fail → mechanism 1 (agentName mismatch) is primary.
     */
    const SESSION_ID_A = 'switch-exec-session-a';
    tc = new AcpTestCase({ testName: 'switch-exec-render-a' });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));

    // Variant A: vibe mode has _meta.welcomeMessage so AgentSwitched carries it
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: SESSION_ID_A,
      modes: {
        currentModeId: 'plan',
        availableModes: [
          { id: 'plan', name: 'Plan' },
          {
            id: 'vibe',
            name: 'Default',
            _meta: { welcomeMessage: 'Starting execution.' },
          },
        ],
      },
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

      // 1. Planner text
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_A,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Plan approved, handing off.' },
        },
      });
      await delay(100);

      // 2. Mode switch: planner → vibe WITH welcomeMessage
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_A,
        update: {
          sessionUpdate: 'current_mode_update',
          currentModeId: 'vibe',
        },
      });
      await delay(100);

      // 3. exec2 text
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_A,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Implementing the plan.' },
        },
      });
      await delay(100);

      // 4. exec2 tool_call
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_A,
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'w1',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/tmp/out.ts', content: 'export const x = 1;' },
          _meta: { kiro: { toolOrigin: 'agent' } },
        },
      });
      await delay(200);

      // 5. tool_call completed
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_A,
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'w1',
          status: 'completed',
          rawOutput: { response: 'written' },
        },
      });
      await delay(100);

      // 6. turn_completion
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_A,
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
    await tc.sendKeys('do the thing');
    await tc.pressEnter();
    await tc.waitForVisibleText('Implementing the plan', 10000);

    const store = await tc
      .waitForStore(
        (s: any) =>
          s.messages.some((m: any) => m.role === 'tool_use' && m.id === 'w1'),
        5000,
        100
      )
      .catch(() => tc!.getStore());

    const summary = store.messages.map((m: any) => ({
      role: m.role,
      id: m.id ?? null,
      agentName: m.agentName ?? null,
      standalone: (m as any).standalone ?? false,
    }));
    console.log(
      '[Variant A] store.messages:',
      JSON.stringify(summary, null, 2)
    );

    // Check previousAgentName after the AgentSwitched event
    const previousAgentName = store.previousAgentName;
    console.log('[Variant A] previousAgentName:', previousAgentName);

    const toolMsg = store.messages.find(
      (m: any) => m.role === 'tool_use' && m.id === 'w1'
    );
    expect(toolMsg).toBeDefined();

    // Render assertion — EXPECTED TO FAIL if welcomeMessage turn-split is mechanism 2
    await tc.waitForVisibleText('Write', 5000).catch(async () => {
      const snapshot = tc!.getSnapshot();
      console.log(
        '[Variant A RENDER BUG] tool card NOT visible. Terminal snapshot:'
      );
      console.log(snapshot.join('\n'));
      // Check if welcomeMessage was inserted as standalone
      const standaloneMsg = store.messages.find(
        (m: any) => (m as any).standalone
      );
      console.log('[Variant A] standalone message:', standaloneMsg);
      throw new Error(
        'Variant A: Tool card NOT rendered. ' +
          `previousAgentName=${previousAgentName}, ` +
          `standaloneWelcome=${!!standaloneMsg}. ` +
          'Mechanism 2 (welcomeMessage turn-split) confirmed if variant B passes.'
      );
    });
  });

  it('[A/B control] tool_use card renders WITHOUT current_mode_update', async () => {
    /**
     * Control: same sequence but WITHOUT the current_mode_update.
     * If this passes and the main test fails → current_mode_update is the trigger.
     */
    const SESSION_ID_AB = 'switch-exec-session-ab';
    tc = new AcpTestCase({ testName: 'switch-exec-render-ab' });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: SESSION_ID_AB,
      modes: {
        currentModeId: 'plan',
        availableModes: [
          { id: 'plan', name: 'Plan' },
          { id: 'vibe', name: 'Default' },
        ],
      },
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

      // 1. Text (no planner handoff, no mode switch)
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_AB,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Implementing the plan.' },
        },
      });
      await delay(100);

      // 2. tool_call directly (same payload as main test, no current_mode_update before it)
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_AB,
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'w1',
          title: 'fs_write',
          kind: 'edit',
          rawInput: { path: '/tmp/out.ts', content: 'export const x = 1;' },
          _meta: { kiro: { toolOrigin: 'agent' } },
        },
      });
      await delay(200);

      // 3. tool_call completed
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_AB,
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'w1',
          status: 'completed',
          rawOutput: { response: 'written' },
        },
      });
      await delay(100);

      // 4. turn_completion
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID_AB,
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
    await tc.sendKeys('do the thing');
    await tc.pressEnter();
    await tc.waitForVisibleText('Implementing the plan', 10000);

    const store = await tc
      .waitForStore(
        (s: any) =>
          s.messages.some((m: any) => m.role === 'tool_use' && m.id === 'w1'),
        5000,
        100
      )
      .catch(() => tc!.getStore());

    const summary = store.messages.map((m: any) => ({
      role: m.role,
      id: m.id ?? null,
      agentName: m.agentName ?? null,
    }));
    console.log(
      '[A/B control] store.messages:',
      JSON.stringify(summary, null, 2)
    );

    // Store assertion
    const toolMsg = store.messages.find(
      (m: any) => m.role === 'tool_use' && m.id === 'w1'
    );
    expect(toolMsg).toBeDefined();

    // Render assertion — in the control (no mode switch) the tool card SHOULD render.
    // If this passes but the main test fails, current_mode_update is the trigger.
    await tc.waitForVisibleText('Write', 5000);
  });
});
