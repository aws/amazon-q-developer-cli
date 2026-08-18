import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { Terminal as XtermTerminal } from '@xterm/headless';
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
import {
  ConversationView,
  __settleBufferSizeForTests,
  __settlePromotionsForTests,
} from './ConversationView.js';

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

describe('ConversationView settle-then-flush', () => {
  it('promotes a completed turn to <Static> without rewriting settled rows', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const turnMessages: MessageType[] = [
      user('turn-1'),
      {
        id: 'tool-1',
        role: MessageRole.ToolUse,
        name: 'read_files',
        content: JSON.stringify({ paths: ['README.md'] }),
        isFinished: true,
        status: ToolUseStatus.Approved,
        result: { status: 'success', output: 'README.md' },
      },
      model('answer-1'),
    ];
    store.setState({ messages: turnMessages, isProcessing: true });
    const promotionsBefore = __settlePromotionsForTests();

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    // Complete the turn and start the next one: the finished turn settles in
    // the live region for one painted frame, then promotes to <Static>.
    store.setState({
      messages: [...turnMessages, user('turn-2')],
      isProcessing: false,
    });
    await flush();
    const settledOutput = stripAnsi(terminal.output);
    const countIn = (haystack: string, needle: string): number =>
      haystack.split(needle).length - 1;
    // The user row is byte-identical between tail and settle form: painted
    // once, never rewritten. The model row restyles in place when streaming
    // ends (exactly one extra live repaint), never as a divergent copy at
    // the static boundary.
    expect(countIn(settledOutput, 'turn-1')).toBe(1);
    expect(countIn(settledOutput, 'answer-1')).toBe(2);

    // Frames after promotion (spinner ticks, next turn) re-emit nothing:
    // the static commit matched the settled rows byte-for-byte.
    store.setState({ isProcessing: true });
    await flush();
    store.setState({ isProcessing: false });
    await flush();

    const output = stripAnsi(terminal.output);
    expect(countIn(output, 'turn-1')).toBe(1);
    expect(countIn(output, 'answer-1')).toBe(2);
    // The short turn fits the viewport, so the paint guarantee — not the
    // stale-paint escape — must be what released the buffer.
    const promotions = __settlePromotionsForTests();
    expect(promotions.guarantee).toBeGreaterThan(promotionsBefore.guarantee);
    expect(promotions.escape).toBe(promotionsBefore.escape);
  });

  it('a turn taller than the terminal promotes without loss and with bounded duplication', async () => {
    const terminal = new MockTerminal();
    terminal.rows = 20;
    // Raw-stream counts are meaningless here — every viewport-tail paint
    // legitimately rewrites the visible window. Judge the final terminal
    // buffer (screen + scrollback) instead.
    const xterm = new XtermTerminal({
      cols: terminal.columns,
      rows: terminal.rows,
      scrollback: 5000,
      allowProposedApi: true,
    });
    const baseWrite = terminal.write.bind(terminal);
    terminal.write = (data: string) => {
      baseWrite(data);
      xterm.write(data);
    };
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const tallBody = Array.from(
      { length: 40 },
      (_, i) => `TALL-LINE-${i} pads the turn far beyond the viewport`
    ).join('\n');
    const turnMessages: MessageType[] = [
      user('tall-turn'),
      model('tall-answer'),
    ];
    (turnMessages[1] as { content: string }).content = tallBody;
    store.setState({ messages: turnMessages, isProcessing: true });
    const promotionsBefore = __settlePromotionsForTests();

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, preserveScrollbackOnRedraw: true }
    );
    await flush();

    store.setState({
      messages: [...turnMessages, user('next-turn')],
      isProcessing: false,
    });
    await flush();
    store.setState({ isProcessing: true });
    await flush();
    // The escape needs a bounded run of stale paints; each state change
    // drives one. Keep cycling until the buffer drains or the bound proves
    // broken.
    for (let i = 0; i < 6 && __settleBufferSizeForTests() > 0; i++) {
      store.setState({ isProcessing: i % 2 === 0 });
      await flush();
    }

    // Drain pending xterm writes, then read the full buffer.
    await new Promise<void>((resolve) => xterm.write('', () => resolve()));
    const buf = xterm.buffer.active;
    const bufferLines: string[] = [];
    for (let i = 0; i < buf.length; i++) {
      bufferLines.push(buf.getLine(i)?.translateToString(true) ?? '');
    }
    const committed = bufferLines.join('\n');

    // Promotion must actually happen — a gate that never releases would
    // leave the turn pinned in the live block and still pass the occurrence
    // bounds below. With scrollback preservation on, the tall turn takes
    // the viewport-tail path: the gate holds while live rows above the
    // window are stale and releases when a paint covers the live region
    // again (or via the bounded stale-paint escape).
    expect(__settleBufferSizeForTests()).toBe(0);
    // And say which mechanism drained it: the tall settle card keeps the
    // live block above the viewport, so no covering paint arrives while it
    // is pinned — the bounded stale-paint escape must be what released it.
    const promotions = __settlePromotionsForTests();
    expect(promotions.escape).toBeGreaterThan(promotionsBefore.escape);

    // The settle rows sit above the viewport-tail window, so the paint gate
    // holds them and then escapes after bounded stale paints. Loss is never
    // acceptable; duplication stays bounded at the lossless-fallback level.
    const countIn = (haystack: string, needle: string): number =>
      haystack.split(needle).length - 1;
    for (let i = 0; i < 40; i++) {
      const n = countIn(committed, `TALL-LINE-${i} `);
      expect(n).toBeGreaterThanOrEqual(1);
      expect(n).toBeLessThanOrEqual(2);
    }
  });
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

describe('ConversationView cloud prefetch hold', () => {
  const prefetch = (id: string, name: string): MessageType => ({
    id,
    role: MessageRole.ToolUse,
    name,
    content: '{}',
    isFinished: true,
    status: ToolUseStatus.Approved,
    result: { status: 'success', output: 'ok' },
  });

  it('hides cloud prefetch tool cards until the first user message', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    store.setState({ cloudSessionActive: true });
    // Bring-up prefetch arrives before the user types.
    store.setState({
      messages: [
        prefetch('p1', 'get_steering_files'),
        prefetch('p2', 'get_learnings_for_prompt'),
      ],
    });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    // No prefetch cards on screen yet — the connect screen owns this view.
    let output = stripAnsi(terminal.output);
    expect(output).not.toContain('get_steering_files');
    expect(output).not.toContain('get_learnings_for_prompt');

    // The user's first prompt lifts the hold; the transcript renders.
    store.setState({
      messages: [
        prefetch('p1', 'get_steering_files'),
        prefetch('p2', 'get_learnings_for_prompt'),
        user('first-prompt'),
      ],
    });
    await flush();

    output = stripAnsi(terminal.output);
    expect(output).toContain('get_steering_files');
  });

  it('does not hold prefetch-shaped messages in a non-cloud session', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    // No cloudSessionActive — local sessions must be unaffected.
    store.setState({ messages: [prefetch('p1', 'get_steering_files')] });

    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <ConversationView />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    expect(stripAnsi(terminal.output)).toContain('get_steering_files');
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
