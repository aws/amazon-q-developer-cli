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

describe('ConversationView inter-turn system notices', () => {
  it('renders a notice added after an idle turn below that turn, in order', async () => {
    // Regression: an unowned system notice (e.g. the /autonomous toggle line)
    // added while the previous turn was idle-but-uncommitted went straight to
    // <Static>, jumping ABOVE the turn body still rendered in the dynamic
    // region — observed as "Autonomous mode on" displaying above an earlier
    // turn-error row.
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const initialMessages: MessageType[] = [
      user('turn-1'),
      model('answer-1'),
      {
        id: 'turn-error',
        role: MessageRole.System,
        content: 'Remote session source error: submitPrompt: UnknownError',
        success: false,
        turnOwned: true,
      },
    ];
    store.setState({ messages: initialMessages, isProcessing: false });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    store.setState({
      messages: [
        ...initialMessages,
        {
          id: 'autonomous-on',
          role: MessageRole.System,
          content: 'Autonomous mode on, agent switched to Kiro Default',
          success: true,
        },
      ],
    });
    await flush();

    const output = stripAnsi(terminal.output);
    const noticeAt = output.lastIndexOf('Autonomous mode on');
    const errorAt = output.lastIndexOf('Remote session source error');
    const answerAt = output.lastIndexOf('answer-1');
    expect(noticeAt).toBeGreaterThanOrEqual(0);
    expect(errorAt).toBeGreaterThanOrEqual(0);
    // Wall-clock order: turn body first, then the later notice below it.
    expect(noticeAt).toBeGreaterThan(errorAt);
    expect(errorAt).toBeGreaterThan(answerAt);
  });

  it('keeps multiple inter-turn notices in emission order below the turn', async () => {
    // Two consecutive unowned notices (e.g. /autonomous off then on) must
    // render in the order they were emitted, both below the idle turn's body,
    // and commit to <Static> in that same order.
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const initialMessages: MessageType[] = [user('turn-1'), model('answer-1')];
    store.setState({ messages: initialMessages, isProcessing: false });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const notice = (id: string, content: string): MessageType => ({
      id,
      role: MessageRole.System,
      content,
      success: true,
    });
    store.setState({
      messages: [
        ...initialMessages,
        notice('notice-a', 'Autonomous mode off'),
        notice('notice-b', 'Autonomous mode on'),
      ],
    });
    await flush();

    // Next turn commits the previous turn and both notices to <Static>.
    store.setState({
      messages: [
        ...store.getState().messages,
        user('turn-2'),
        model('answer-2'),
      ],
      isProcessing: false,
    });
    await flush();

    // lastIndexOf reads the final committed layout. (Occurrence counting is
    // invalid here: MockTerminal accumulates every write, so a row that
    // renders dynamically and then commits to <Static> appears twice in the
    // raw stream even though the real terminal erases the dynamic paint.)
    const output = stripAnsi(terminal.output);
    const answerAt = output.lastIndexOf('answer-1');
    const offAt = output.lastIndexOf('Autonomous mode off');
    const onAt = output.lastIndexOf('Autonomous mode on');
    const secondTurnAt = output.lastIndexOf('answer-2');
    expect(offAt).toBeGreaterThan(answerAt);
    expect(onAt).toBeGreaterThan(offAt);
    expect(secondTurnAt).toBeGreaterThan(onAt);
  });

  it('commits repeated idle-session notices in order without an open dynamic tail', async () => {
    // P1 guard: repeated toggles on an idle session (no next prompt ever)
    // must not accumulate in a dynamic queue. Each notice closes the idle
    // turn (first one) or appends directly (later ones), so all of them land
    // in scrollback in emission order even though no prompt follows.
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const initialMessages: MessageType[] = [user('turn-1'), model('answer-1')];
    store.setState({ messages: initialMessages, isProcessing: false });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const notice = (id: string, content: string): MessageType => ({
      id,
      role: MessageRole.System,
      content,
      success: true,
    });
    // Three notices, added one render at a time — like a user toggling
    // repeatedly — with NO subsequent prompt.
    let msgs = [...initialMessages, notice('n-1', 'NOTICE_ONE')];
    store.setState({ messages: msgs });
    await flush();
    msgs = [...msgs, notice('n-2', 'NOTICE_TWO')];
    store.setState({ messages: msgs });
    await flush();
    msgs = [...msgs, notice('n-3', 'NOTICE_THREE')];
    store.setState({ messages: msgs });
    await flush();

    const output = stripAnsi(terminal.output);
    const answerAt = output.lastIndexOf('answer-1');
    const oneAt = output.lastIndexOf('NOTICE_ONE');
    const twoAt = output.lastIndexOf('NOTICE_TWO');
    const threeAt = output.lastIndexOf('NOTICE_THREE');
    expect(oneAt).toBeGreaterThan(answerAt);
    expect(twoAt).toBeGreaterThan(oneAt);
    expect(threeAt).toBeGreaterThan(twoAt);
  });

  it('keeps an unowned workflow row standalone while later turn body streams', async () => {
    // P2 guard: a workflow completion (placement owned by
    // includeInterleavedSystemRows) arriving mid-turn must NOT be deferred
    // below turn body that streams AFTER it — workflow rows keep their
    // immediate static path even when they fall after the active anchor.
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const streaming: MessageType[] = [
      user('turn-1'),
      model('early-body'),
      {
        id: 'wf-done',
        role: MessageRole.System,
        content: 'Workflow "release" completed',
        success: true,
        kind: 'workflow-completion',
        workflowId: 'wf-1',
        workflowName: 'release',
        workflowStatus: 'completed',
      },
    ];
    store.setState({ messages: streaming, isProcessing: true });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    // The workflow row must already be on screen BEFORE later body streams:
    // it took the immediate static path, not the deferral queue.
    let output = stripAnsi(terminal.output);
    expect(output).toContain('Workflow completed "release"');

    store.setState({
      messages: [...streaming, model('late-body')],
      isProcessing: false,
    });
    await flush();

    output = stripAnsi(terminal.output);
    // Later body renders after the workflow row — the row was not re-ordered
    // below content that arrived after it.
    expect(output.lastIndexOf('late-body')).toBeGreaterThan(
      output.lastIndexOf('Workflow completed "release"')
    );
  });

  it('defers a trailing notice while the turn is streaming, then shows it once', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const streaming: MessageType[] = [
      user('turn-1'),
      model('partial-answer'),
      {
        id: 'mid-flight-notice',
        role: MessageRole.System,
        content: 'Autonomous mode on',
        success: true,
      },
    ];
    store.setState({ messages: streaming, isProcessing: true });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    // Turn ends with more body after the notice: the notice folds inline.
    store.setState({
      messages: [...streaming, model('final-answer')],
      isProcessing: false,
    });
    await flush();

    const output = stripAnsi(terminal.output);
    const noticeAt = output.lastIndexOf('Autonomous mode on');
    expect(noticeAt).toBeGreaterThan(output.lastIndexOf('partial-answer'));
    expect(output.lastIndexOf('final-answer')).toBeGreaterThan(noticeAt);
  });
});
