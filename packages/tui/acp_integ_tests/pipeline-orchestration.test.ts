/**
 * ACP wire-level test for KAS pipeline orchestration (`orchestrate_subagent`).
 *
 * Uses REAL KAS pipeline metadata shapes from
 * kiro-agent/src/acp/acp-event-adapter.ts to drive the TUI through the
 * full ACP path:
 *   mock ACP → KasAcpClient.wireSessionListeners
 *     → handlePipelineStateUpdate → broadcastSubagentList → store.sessions
 *     → broadcastMultiSession → sessionConversationsStore
 *
 * Covers:
 * - Pipeline parent `tool_call` populates the subagent list (DAG state).
 * - Per-stage events (tagged with `_meta.kiro.agentSubtaskId`) route to
 *   per-stage handlers and DO NOT leak into the main conversation.
 * - The `invoke_sub_agent` wrapper events (`_meta.kiro.kind = 'agent-subtask'`)
 *   are also suppressed from the main view.
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

const MAIN_SESSION_ID = 'pipeline-session-1';

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

describe('KAS pipeline orchestration via ACP', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('populates subagent list from pipeline metadata on parent tool_call', async () => {
    tc = new AcpTestCase({ testName: 'pipeline-subagent-list' });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    // KAS emits the parent orchestrate_subagent tool call with pipeline metadata.
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'orchestrate-001',
        title: 'Orchestrate Sub-agent',
        kind: 'other',
        rawInput: { task: 'multi-stage exploration' },
        _meta: {
          kiro: {
            pipeline: {
              groupId: 'pipeline-test',
              stages: [
                {
                  name: 'explore_a',
                  role: 'general-task-execution',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'sub-a',
                },
                {
                  name: 'explore_b',
                  role: 'general-task-execution',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'sub-b',
                },
                {
                  name: 'synthesize',
                  role: 'general-task-execution',
                  status: 'pending',
                  dependsOn: ['explore_a', 'explore_b'],
                  agentSubtaskId: null,
                },
              ],
            },
          },
        },
      },
    });
    await new Promise((r) => setTimeout(r, 300));

    const store = (await tc.getStore()) as any;
    expect(store.sessions).toBeDefined();

    // sessions map keys are the agentSubtaskIds; pending stages stay in
    // pendingStages and don't create per-stage sessions yet.
    // Note: TestModeProvider serializes the Map as a plain object via Object.fromEntries.
    const sessionIds = Object.keys(store.sessions);
    expect(sessionIds).toContain('sub-a');
    expect(sessionIds).toContain('sub-b');
    expect(sessionIds).not.toContain('null');
  });

  it('routes per-stage events to multi-session, NOT to main conversation', async () => {
    tc = new AcpTestCase({ testName: 'pipeline-no-main-leak' });
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await new Promise((r) => setTimeout(r, 300));

    // First emit the pipeline parent so the subagent list exists.
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'orchestrate-002',
        title: 'Orchestrate Sub-agent',
        kind: 'other',
        rawInput: { task: 'leak test' },
        _meta: {
          kiro: {
            pipeline: {
              groupId: 'pipeline-leak-test',
              stages: [
                {
                  name: 'stage_one',
                  role: 'general-task-execution',
                  status: 'running',
                  dependsOn: [],
                  agentSubtaskId: 'sub-leak-1',
                },
              ],
            },
          },
        },
      },
    });
    await new Promise((r) => setTimeout(r, 200));

    const storeBefore = (await tc.getStore()) as any;
    const mainMsgsBefore = storeBefore.messages.length;

    // Per-stage assistant text — should never reach main.
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'I am narrating from stage one' },
        _meta: { kiro: { agentSubtaskId: 'sub-leak-1' } },
      },
    });
    await new Promise((r) => setTimeout(r, 100));

    // Per-stage tool call — should never reach main.
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'inner-read-1',
        title: 'read_file',
        kind: 'read',
        rawInput: { path: '/test.ts' },
        _meta: { kiro: { agentSubtaskId: 'sub-leak-1' } },
      },
    });
    await new Promise((r) => setTimeout(r, 100));

    // invoke_sub_agent wrapper — should also never reach main.
    tc.mock.notify('session/update', {
      sessionId: MAIN_SESSION_ID,
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'wrap-1',
        title: 'invoke_sub_agent',
        kind: 'other',
        rawInput: { name: 'general-task-execution', prompt: 'go' },
        _meta: {
          kiro: { kind: 'agent-subtask', agentSubtaskId: 'sub-leak-1' },
        },
      },
    });
    await new Promise((r) => setTimeout(r, 200));

    const storeAfter = (await tc.getStore()) as any;
    expect(storeAfter.messages.length).toBe(mainMsgsBefore);

    // Sanity check: the per-stage assistant narration must never end up
    // anywhere in the main conversation messages (this was the duplicate
    // bug — sub-agent text leaking alongside the SUBAGENT OUTPUT panel).
    const mainTexts = storeAfter.messages
      .map((m: any) => m?.content?.text ?? '')
      .join('\n');
    expect(mainTexts).not.toContain('I am narrating from stage one');
  });
});
