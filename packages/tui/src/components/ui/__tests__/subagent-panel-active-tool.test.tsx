/**
 * SubagentToolPanel active-tool status line.
 *
 * The panel names an agent and, next to it, what that agent is doing right now.
 * It finds that by matching a running tool message's `agentName` against the
 * session's name. When nothing matches it falls back to "Thinking...", so a
 * mislabelled tool message is indistinguishable from an idle agent — the whole
 * status column silently degrades to "Thinking..." for the entire execution.
 *
 * KAS delivers a dispatched sub-agent's tool calls on the MAIN session id, tagged
 * only with `_meta.kiro.agentSubtaskId`, so those messages carry the MAIN agent's
 * name. Any lookup keyed on a tool message's `agentName` therefore misses. The panel
 * must key on the session id instead — the same source the crew monitor reads, and
 * the key the per-session conversation buffer is built under.
 */

import { describe, test, expect, afterEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  type MessageType,
} from '../../../stores/app-store.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import { Kiro } from '../../../kiro.js';
import type { AgentSession } from '../../../types/multi-session.js';
import { SubagentToolPanel } from '../SubagentToolPanel.js';

/** Session id KAS uses for a dispatched sub-agent: its `agentSubtaskId`. */
const SUBTASK_ID = '1bac2418-ba73-4e49-8252-4a2cc183dc20';
const SUBAGENT_NAME = 'feature-requirements-first-workflow';
const MAIN_SESSION_ID = 'sess_35460443';
/** What `state.currentAgent?.name` resolves to in a spec session. */
const MAIN_AGENT_NAME = 'spec';

class MockTerminal implements Terminal {
  private _onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return 120;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(onInput: (data: string) => void): void {
    this._onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
  sendInput(data: string): void {
    if (this._onInput) this._onInput(data);
  }
}

let activeInstance: Instance | null = null;
afterEach(() => {
  if (activeInstance) {
    activeInstance.unmount();
    activeInstance = null;
  }
  vi.useRealTimers();
});

async function flush(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

/** A running `fs_write` on design.md, labelled as belonging to `agentName`. */
function runningWriteTool(agentName: string): MessageType {
  return {
    id: 'tool-1',
    role: MessageRole.ToolUse,
    name: 'fs_write',
    content: JSON.stringify({ path: 'design.md' }),
    isFinished: false,
    agentName,
  } as MessageType;
}

function mountPanel(
  sessionMessages: MessageType[],
  mainStoreMessages: MessageType[] = []
) {
  sessionConversationsStore.setState({
    conversations: new Map(
      sessionMessages.length > 0 ? [[SUBTASK_ID, sessionMessages]] : []
    ),
  });
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
  const session: AgentSession = {
    id: SUBTASK_ID,
    name: SUBAGENT_NAME,
    status: 'busy',
    type: 'ephemeral',
    created: new Date(1000),
    lastActivity: new Date(),
  };
  store.setState({
    sessions: new Map([[SUBTASK_ID, session]]),
    sessionId: MAIN_SESSION_ID,
    messages: mainStoreMessages,
    focusedCrewIndex: 0,
  });

  const terminal = new MockTerminal();
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <SubagentToolPanel />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  return terminal;
}

describe('SubagentToolPanel active tool', () => {
  test('names the sub-agent it is tracking', async () => {
    const terminal = mountPanel([runningWriteTool(SUBAGENT_NAME)]);
    await flush();

    expect(terminal.output).toContain(SUBAGENT_NAME);
  });

  test('shows the running tool when the message is labelled with the sub-agent', async () => {
    const terminal = mountPanel([runningWriteTool(SUBAGENT_NAME)]);
    await flush();

    // formatToolDesc renders "<label> (<param>)" — the param is the path.
    expect(terminal.output).toContain('design.md');
    expect(terminal.output).not.toContain('Thinking...');
  });

  test('still shows the tool when the message carries the MAIN agent name', async () => {
    // The regression guard. KAS labels a dispatched sub-agent's tool calls with the
    // main agent (they arrive on the main session id), so a name-keyed lookup reads
    // "Thinking..." for the whole execution. Keying on the session id is immune.
    const terminal = mountPanel([runningWriteTool(MAIN_AGENT_NAME)]);
    await flush();

    expect(terminal.output).toContain('design.md');
    expect(terminal.output).not.toContain('Thinking...');
  });

  test('shows Thinking... when the sub-agent has no running tool', async () => {
    const terminal = mountPanel([]);
    await flush();

    expect(terminal.output).toContain('Thinking...');
  });

  test('falls back to the agentName match when the session buffer is empty', async () => {
    // Regression guard for flows that resolved before this component read the
    // per-session buffer: a tool message in the main store labelled with the
    // session's own name must still surface.
    const terminal = mountPanel([], [runningWriteTool(SUBAGENT_NAME)]);
    await flush();

    expect(terminal.output).toContain('design.md');
    expect(terminal.output).not.toContain('Thinking...');
  });

  test('ignores the sub-agent wrapper card when picking the active tool', async () => {
    // The wrapper ("Sub-agent: <role>") stays unfinished for the whole child
    // execution and lives in the child's own buffer, so a naive last-unfinished-tool
    // scan reported the sub-agent's name back at itself.
    const wrapper = {
      id: 'wrapper-1',
      role: MessageRole.ToolUse,
      name: `Sub-agent: ${SUBAGENT_NAME}`,
      kind: 'other',
      content: '{}',
      isFinished: false,
    } as MessageType;

    const terminal = mountPanel([wrapper, runningWriteTool(SUBAGENT_NAME)]);
    await flush();

    expect(terminal.output).toContain('design.md');
    expect(terminal.output).not.toContain('Sub-agent:');
  });

  test('reports Thinking... when the only unfinished tool is the wrapper', async () => {
    const wrapper = {
      id: 'wrapper-1',
      role: MessageRole.ToolUse,
      name: `Sub-agent: ${SUBAGENT_NAME}`,
      kind: 'other',
      content: '{}',
      isFinished: false,
    } as MessageType;

    const terminal = mountPanel([wrapper]);
    await flush();

    expect(terminal.output).toContain('Thinking...');
  });

  test('prints the agent name once when it matches the session name', async () => {
    // KAS names these sessions after the agent, so both columns held the same string
    // and the row read "<name> <name> Thinking...".
    const terminal = mountPanel([runningWriteTool(SUBAGENT_NAME)]);
    await flush();

    const occurrences = terminal.output.split(SUBAGENT_NAME).length - 1;
    expect(occurrences).toBe(1);
  });

  test('ignores a finished tool — a completed call is not current activity', async () => {
    const finished = {
      ...runningWriteTool(SUBAGENT_NAME),
      isFinished: true,
    } as MessageType;

    const terminal = mountPanel([finished]);
    await flush();

    expect(terminal.output).toContain('Thinking...');
  });
});
