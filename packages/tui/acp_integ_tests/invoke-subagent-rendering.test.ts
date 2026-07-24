/**
 * ACP wire-level test for standalone `invoke_sub_agent` rendering.
 *
 * KAS's default (IDE/cloud) delegation tool emits parent cards with
 * `_meta.kiro = { kind: 'agent-subtask', agentSubtaskId }` and NO pipeline
 * metadata. The InvokeSubagentPipelineAdapter ports those cards onto the
 * orchestrate_subagent pipeline contract so the existing crew rendering
 * (roster session, child routing, footer) applies. Wire shapes mirror
 * kiro-agent/src/acp/acp-event-adapter.ts `handleSubAgentAction`.
 *
 * Covers:
 * - Standalone invoke parent creates a roster session from synthesized
 *   pipeline metadata (groupId `invoke-<toolCallId>`).
 * - The parent card lands in the MAIN conversation as a pipeline parent
 *   (name rewritten, pipelineGroupId stamped).
 * - Child events tagged with the delegation's agentSubtaskId route to the
 *   subagent session, never to main.
 * - Terminal completion marks the roster session terminated.
 * - An orchestrate stage's own invoke wrapper card (same envelope shape)
 *   is NOT claimed and stays suppressed (regression guard vs. the
 *   pipeline-orchestration suite).
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

const MAIN_SESSION_ID = 'invoke-session-1';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: MAIN_SESSION_ID,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

function notifyInvokeParent(
  tc: AcpTestCase,
  toolCallId: string,
  agentSubtaskId: string,
  agentName = 'architect'
): void {
  tc.mock.notify('session/update', {
    sessionId: MAIN_SESSION_ID,
    update: {
      sessionUpdate: 'tool_call',
      toolCallId,
      title: `Sub-agent: ${agentName}`,
      kind: 'other',
      rawInput: {
        name: agentName,
        prompt: 'Design the module layout',
        explanation: 'Designing layout',
      },
      _meta: { kiro: { kind: 'agent-subtask', agentSubtaskId } },
    },
  });
}

describe('KAS standalone invoke_sub_agent via ACP', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('renders the invoke parent as a pipeline card with a roster session', async () => {
    tc = new AcpTestCase({
      testName: 'invoke-parent-pipeline-card',
      extraEnv: { KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION: '1' },
    });
    setupHandshake(tc);

    // Tool cards only land in main messages during an active prompt turn
    // (same harness pattern as mcp-transform.test.ts).
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      notifyInvokeParent(tc!, 'invoke-001', 'sub-exec-1');
      await new Promise((r) => setTimeout(r, 200));
      tc!.mock.notify('session/update', {
        sessionId: MAIN_SESSION_ID,
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'invoke-001',
          status: 'completed',
          title: 'Sub-agent: architect',
          kind: 'other',
          rawOutput: 'Here is the design.',
          _meta: {
            kiro: { kind: 'agent-subtask', agentSubtaskId: 'sub-exec-1' },
          },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      tc!.mock.notify('session/update', {
        sessionId: MAIN_SESSION_ID,
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
    await tc.sendKeys('delegate this');
    await tc.sleepMs(100);
    await tc.pressEnter();
    await tc.waitForVisibleText('Done', 10000);
    await tc.sleepMs(500);

    const store = (await tc.getStore()) as any;

    // Roster session synthesized from the one-stage pipeline projection.
    const sessionIds = Object.keys(store.sessions ?? {});
    expect(sessionIds).toContain('sub-exec-1');
    const session = store.sessions['sub-exec-1'];
    expect(session.group).toBe('invoke-invoke-001');
    expect(session.role).toBe('architect');
    // Terminal update flips the roster session to terminated.
    expect(session.status).toBe('terminated');

    // Parent card is in MAIN messages as a pipeline parent.
    const parent = store.messages.find(
      (m: any) => m?.id === 'invoke-001' && m?.role === 'tool_use'
    );
    expect(parent).toBeDefined();
    expect(parent.name).toBe('orchestrate_subagent');
    expect(parent.pipelineGroupId).toBe('invoke-invoke-001');
    expect(parent.isFinished).toBe(true);
    expect(parent.result?.status).toBe('success');
    expect(parent.result?.output).toBe('Here is the design.');
  });

  it('coalesces concurrent invokes and retains every result independent of completion order', async () => {
    tc = new AcpTestCase({
      testName: 'invoke-concurrent-coalescing',
      extraEnv: { KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION: '1' },
    });
    setupHandshake(tc);

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      notifyInvokeParent(tc!, 'invoke-a', 'sub-a', 'architect');
      notifyInvokeParent(tc!, 'invoke-b', 'sub-b', 'reviewer');
      await new Promise((r) => setTimeout(r, 200));

      tc!.mock.notify('session/update', {
        sessionId: MAIN_SESSION_ID,
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'invoke-b',
          status: 'completed',
          title: 'Sub-agent: reviewer',
          kind: 'other',
          rawOutput: 'review result',
          _meta: {
            kiro: { kind: 'agent-subtask', agentSubtaskId: 'sub-b' },
          },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      tc!.mock.notify('session/update', {
        sessionId: MAIN_SESSION_ID,
        update: {
          sessionUpdate: 'tool_call_update',
          toolCallId: 'invoke-a',
          status: 'failed',
          title: 'Sub-agent: architect',
          kind: 'other',
          rawOutput: 'architecture failed',
          _meta: {
            kiro: { kind: 'agent-subtask', agentSubtaskId: 'sub-a' },
          },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      tc!.mock.notify('session/update', {
        sessionId: MAIN_SESSION_ID,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Parallel work finished' },
        },
      });
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);
    await tc.sendKeys('delegate twice');
    await tc.sleepMs(100);
    await tc.pressEnter();
    await tc.waitForVisibleText('Parallel work finished', 10000);
    await tc.sleepMs(500);

    const store = (await tc.getStore()) as any;
    const parents = store.messages.filter(
      (message: any) =>
        message?.role === 'tool_use' && message?.name === 'orchestrate_subagent'
    );
    expect(parents).toHaveLength(1);
    expect(parents[0].id).toBe('invoke-a');
    expect(JSON.parse(parents[0].content).stages).toHaveLength(2);
    expect(parents[0].result).toMatchObject({
      status: 'error',
      error: 'Designing layout: architecture failed',
      output: {
        type: 'invoke_sub_agent_pipeline_results',
        stages: [
          {
            name: 'Designing layout',
            status: 'error',
            error: 'architecture failed',
          },
          {
            name: 'Designing layout #2',
            status: 'success',
            output: 'review result',
          },
        ],
      },
    });
    expect(store.sessions['sub-a'].status).toBe('terminated');
    expect(store.sessions['sub-b'].status).toBe('terminated');
  });

  it('routes delegation child events to the subagent session, not main', async () => {
    tc = new AcpTestCase({
      testName: 'invoke-child-routing',
      extraEnv: { KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION: '1' },
    });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    notifyInvokeParent(tc, 'invoke-002', 'sub-exec-2', 'reviewer');
    await new Promise((r) => setTimeout(r, 200));

    const before = (await tc.getStore()) as any;
    const mainMsgsBefore = before.messages.length;

    // Child narration + child tool call, tagged with the delegation id
    // (same contract KAS uses for orchestrate stages).
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'child narration text' },
        _meta: { kiro: { agentSubtaskId: 'sub-exec-2' } },
      },
    });
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'child-read-1',
        title: 'read_file',
        kind: 'read',
        rawInput: { path: '/x.ts' },
        _meta: { kiro: { agentSubtaskId: 'sub-exec-2' } },
      },
    });
    await new Promise((r) => setTimeout(r, 300));

    const after = (await tc.getStore()) as any;
    expect(after.messages.length).toBe(mainMsgsBefore);
    const mainTexts = after.messages
      .map((m: any) => m?.content?.text ?? '')
      .join('\n');
    expect(mainTexts).not.toContain('child narration text');
  });

  it('does not claim an orchestrate stage wrapper card (same envelope shape)', async () => {
    tc = new AcpTestCase({
      testName: 'invoke-stage-exclusion',
      extraEnv: { KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION: '1' },
    });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    // Genuine orchestrate parent publishes stage ids first.
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'orch-1',
        title: 'Orchestrate Sub-agent',
        kind: 'other',
        rawInput: { task: 'crew task' },
        _meta: {
          kiro: {
            pipeline: {
              groupId: 'pipeline-crew',
              stages: [
                {
                  name: 'worker',
                  role: 'general-task-execution',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'stage-sub-9',
                },
              ],
            },
          },
        },
      },
    });
    await new Promise((r) => setTimeout(r, 200));

    const before = (await tc.getStore()) as any;
    const mainMsgsBefore = before.messages.length;

    // The stage's invoke wrapper card — identical envelope to a standalone
    // invoke parent. Must stay suppressed as crew activity (not become a
    // second pipeline parent in main).
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'wrap-9',
        title: 'Sub-agent: worker',
        kind: 'other',
        rawInput: { name: 'worker', prompt: 'go' },
        _meta: {
          kiro: { kind: 'agent-subtask', agentSubtaskId: 'stage-sub-9' },
        },
      },
    });
    await new Promise((r) => setTimeout(r, 300));

    const after = (await tc.getStore()) as any;
    expect(after.messages.length).toBe(mainMsgsBefore);
    // Only the crew group exists — no invoke-wrap-9 group was synthesized.
    const groups = new Set(
      Object.values(after.sessions ?? {}).map((s: any) => s.group)
    );
    expect(groups.has('invoke-wrap-9')).toBe(false);
  });
});

describe('local sessions: adapter is inert (no UX change for existing users)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('renders an invoke wrapper card the pre-existing way on a local session', async () => {
    // Override env EXPLICITLY CLEARED: this is a plain local session, like
    // every released build. The invoke parent card must NOT be claimed —
    // no name rewrite, no synthesized pipeline group; the pre-existing
    // independent-subagent lifecycle applies.
    tc = new AcpTestCase({
      testName: 'invoke-local-inert',
      extraEnv: { KIRO_TEST_DISABLE_SUBAGENT_ORCHESTRATION: '' },
    });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    notifyInvokeParent(tc, 'invoke-local-1', 'sub-local-1');
    await new Promise((r) => setTimeout(r, 300));

    const store = (await tc.getStore()) as any;
    // Pre-existing local behavior: independent-subagent lifecycle creates
    // an ephemeral roster session WITHOUT a pipeline group.
    const session = store.sessions?.['sub-local-1'];
    expect(session).toBeDefined();
    expect(session.group).toBeUndefined();
    // No synthesized invoke-* pipeline group anywhere.
    const groups = Object.values(store.sessions ?? {}).map((s: any) => s.group);
    expect(groups.some((g: any) => `${g}`.startsWith('invoke-'))).toBe(false);
    // No message was rewritten to the crew tool name.
    const rewritten = store.messages.find(
      (m: any) => m?.name === 'orchestrate_subagent'
    );
    expect(rewritten).toBeUndefined();
  });
});
