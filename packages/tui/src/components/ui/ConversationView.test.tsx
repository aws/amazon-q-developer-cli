import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  ToolUseStatus,
  type MessageType,
} from '../../stores/app-store.js';
import { resetVerboseCache, setVerboseConfig } from '../../lite/verbose.js';
import { AgentEventType } from '../../types/agent-events.js';
import { ConversationView } from './ConversationView.js';

class MockTerminal implements Terminal {
  output = '';
  columns = 120;
  rows = 60;
  kittyProtocolActive = true;

  start(): void {}
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
}

let activeInstance: Instance | null = null;
let priorRollout: string | undefined;

beforeEach(() => {
  priorRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;
});

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
  if (priorRollout === undefined) {
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  } else {
    process.env.KIRO_LITE_ROLLOUT_ENABLED = priorRollout;
  }
  setVerboseConfig({ filters: [] }, 'tui');
  resetVerboseCache();
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 50));
  await Promise.resolve();
}

const user = (id: string): MessageType => ({
  id,
  role: MessageRole.User,
  content: id,
});

const model = (id: string): MessageType => ({
  id,
  role: MessageRole.Model,
  content: id,
});

const completion = (
  id: string,
  workflowName: string,
  workflowTurnId: string
): MessageType => ({
  id,
  role: MessageRole.System,
  content: `Workflow "${workflowName}" completed`,
  success: true,
  kind: 'workflow-completion',
  workflowId: id,
  workflowTurnId,
  workflowName,
  workflowStatus: 'completed',
});

describe('ConversationView workflow lifecycle rows', () => {
  it('keeps orphan anchors valid and appends late completion exactly once', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const initialMessages: MessageType[] = [
      {
        id: 'orphan-tool',
        role: MessageRole.ToolUse,
        name: 'read_files',
        content: JSON.stringify({ paths: ['README.md'] }),
        isFinished: true,
        status: ToolUseStatus.Approved,
        result: { status: 'success', output: 'README.md' },
      },
      user('workflow-only-turn'),
      completion('inline-completion', 'inline-release', 'workflow-only-turn'),
      user('launch-turn'),
      model('launch-answer'),
      user('later-turn'),
      model('later-answer'),
      user('active-turn'),
    ];
    store.setState({ messages: initialMessages, isProcessing: true });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    let output = stripAnsi(terminal.output);
    expect(output).not.toContain('Cancelled');
    expect(output).toContain('Workflow completed "inline-release"');

    store.setState({
      messages: [
        ...initialMessages,
        completion('late-completion', 'late-release', 'launch-turn'),
      ],
    });
    await flush();

    output = stripAnsi(terminal.output);
    expect(output.match(/Workflow completed "late-release"/g)).toHaveLength(1);
  });

  it('carries MCP identity from ToolCall metadata into output filtering', async () => {
    process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
    setVerboseConfig(
      {
        filters: ['all', '-mcp'],
        display: { persistOutput: true, outputMaxLines: 5 },
      },
      'tui'
    );
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    const handler = store.getState().createStreamEventHandler();
    handler({
      type: AgentEventType.ToolCall,
      id: 'bare-mcp-from-wire',
      name: 'InternalCodeSearch',
      args: { query: 'handler' },
      meta: { kiro: { mcpServerName: 'builder-mcp' } },
    });
    handler({
      type: AgentEventType.ToolCall,
      id: 'bare-non-mcp-from-wire',
      name: 'InternalCodeSearch',
      args: { query: 'handler' },
    });

    const stored = store
      .getState()
      .messages.find((message) => message.id === 'bare-mcp-from-wire');
    expect(
      stored?.role === MessageRole.ToolUse ? stored.mcpServerName : undefined
    ).toBe('builder-mcp');

    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'bare-mcp-from-wire',
      result: { status: 'success', output: 'FILTERED_STATIC_BODY_FROM_WIRE' },
    });
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'bare-non-mcp-from-wire',
      result: { status: 'success', output: 'VISIBLE_STATIC_BODY_FROM_WIRE' },
    });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    expect(output).not.toContain('FILTERED_STATIC_BODY_FROM_WIRE');
    expect(output).toContain('VISIBLE_STATIC_BODY_FROM_WIRE');
  });
});
