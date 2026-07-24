import { describe, expect, it } from 'bun:test';
import { createAppStore, MessageRole } from '../app-store';
import { Kiro } from '../../kiro';

const REPLAY: any[] = [
  {
    sessionUpdate: 'user_message_chunk',
    content: {
      type: 'text',
      text: 'Spin up a subagent team of 3 subagents in parallel to research…',
    },
  },
  {
    sessionUpdate: 'tool_call',
    toolCallId: '4fd16206',
    title: 'get_learnings_for_prompt',
    status: 'completed',
    kind: 'other',
    rawInput: {
      toolName: 'get_learnings_for_prompt',
      repositories: [],
      limit: 50,
    },
  },
  {
    sessionUpdate: 'tool_call_update',
    toolCallId: '4fd16206',
    status: 'completed',
  },
  {
    sessionUpdate: 'tool_call',
    toolCallId: '7ba014fe',
    title: 'get_steering_files',
    status: 'completed',
    kind: 'other',
    rawInput: { toolName: 'get_steering_files' },
  },
  {
    sessionUpdate: 'tool_call_update',
    toolCallId: '7ba014fe',
    status: 'completed',
  },
  {
    sessionUpdate: 'session_info_update',
    _meta: { kiro: { kind: 'turn_start', turnStart: true } },
  },
  {
    sessionUpdate: 'tool_call',
    toolCallId: 'tooluse_HwA',
    title: 'Update Session Information',
    status: 'completed',
    kind: 'other',
    rawInput: { title: 't', description: 'd', status: 's' },
  },
  {
    sessionUpdate: 'tool_call_update',
    toolCallId: 'tooluse_HwA',
    status: 'completed',
  },
  {
    sessionUpdate: 'tool_call',
    toolCallId: 'invoke_subagent_A-sub-agent-start',
    title: 'Sub-agent: general-task-execution',
    status: 'in_progress',
    kind: 'other',
    rawInput: { prompt: 'Research volcanoes…', explanation: 'x' },
  },
  {
    sessionUpdate: 'tool_call',
    toolCallId: 'tooluse_Oeq',
    title: 'Subagent Response',
    status: 'failed',
    kind: 'other',
    rawInput: {},
  },
  {
    sessionUpdate: 'tool_call_update',
    toolCallId: 'tooluse_Oeq',
    status: 'failed',
  },
  {
    sessionUpdate: 'tool_call',
    toolCallId: 'invoke_subagent_B-sub-agent-start',
    title: 'Sub-agent: general-task-execution',
    status: 'in_progress',
    kind: 'other',
    rawInput: { prompt: 'Research glaciers…', explanation: 'x' },
  },
  {
    sessionUpdate: 'tool_call_update',
    toolCallId: 'tooluse_REn-sub-agent-start',
    status: 'failed',
  },
  {
    sessionUpdate: 'tool_call_update',
    toolCallId: 'invoke_subagent_A-sub-agent-start',
    status: 'failed',
  },
  {
    sessionUpdate: 'tool_call_update',
    toolCallId: 'invoke_subagent_B-sub-agent-start',
    status: 'failed',
  },
  {
    sessionUpdate: 'session_info_update',
    _meta: {
      kiro: {
        kind: 'turn_completion',
        promptTurnSummaries: [],
        elapsedTime: 11118,
        status: 'aborted',
      },
    },
  },
  {
    sessionUpdate: 'session_info_update',
    _meta: {
      kiro: {
        kind: 'turn_end',
        turnEnd: { stopReason: 'cancelled' },
        stopReason: 'cancelled',
      },
    },
  },
];

describe('cancelled subagent session replay', () => {
  it('preserves completed tools and cancels unfinished subagents', async () => {
    const kiro = new Kiro();
    const store = createAppStore({ kiro });
    store.setState({ isInitialized: true });
    const handler = store.getState().createStreamEventHandler();

    // Use the real historical mapper via a lightweight BaseAcpClient shim.
    const { RustAcpClient } = await import('../../acp-client/rust');
    const client: any = Object.create(RustAcpClient.prototype);
    client.sessionId = 'main';
    client.kasSteerBuffer = new Map();
    client.artifactWriteCallsById = new Map();
    client.pendingDisplayError = null;
    client.updateHandlers = new Set();
    const broadcasts: any[] = [];
    client.broadcastStreamEvent = (e: any) => broadcasts.push(e);

    const events: any[] = [];
    for (const u of REPLAY) {
      const mapped = client.convertAcpUpdateToEvent(u) ?? null;
      if (mapped) events.push(mapped);
      while (broadcasts.length) events.push(broadcasts.shift());
    }
    for (const e of events) handler(e);
    handler.flush();
    await new Promise((r) => setTimeout(r, 80));

    const rows = store
      .getState()
      .messages.filter((m) => m.role === MessageRole.ToolUse) as any[];
    // Internal context tools replay completed → success, never "Cancelled".
    for (const id of ['4fd16206', '7ba014fe', 'tooluse_HwA']) {
      const row = rows.find((r) => r.id === id);
      expect(row?.isFinished).toBe(true);
      expect(row?.result?.status).toBe('success');
    }
    // Cancelled turn's subagent stages end failed/cancelled, never success.
    for (const id of [
      'invoke_subagent_A-sub-agent-start',
      'invoke_subagent_B-sub-agent-start',
    ]) {
      const row = rows.find((r) => r.id === id);
      expect(row?.isFinished).toBe(true);
      expect(['error', 'cancelled']).toContain(row?.result?.status);
    }
    expect(store.getState().isProcessing).toBe(false);
    expect(rows.length).toBeGreaterThan(0);
  });
});
