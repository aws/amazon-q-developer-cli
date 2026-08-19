import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import React from 'react';
import { Terminal as XtermTerminal } from '@xterm/headless';
import { render, type Instance, type Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  type MessageType,
} from '../../../../stores/app-store.js';
import { Kiro } from '../../../../kiro.js';
import type { VariantLayoutProps } from '../../variant-layout.js';
import {
  LiteLayout,
  __liteSettleBufferSizeForTests,
  __liteSettlePromotionsForTests,
  __liteStaticItemIdsForTests,
} from '../LiteLayout.js';

class MockTerminal implements Terminal {
  readonly xterm: XtermTerminal;
  output = '';

  constructor(
    readonly columns = 60,
    readonly rows = 10
  ) {
    this.xterm = new XtermTerminal({
      cols: columns,
      rows,
      scrollback: 5000,
      allowProposedApi: true,
    });
  }

  get kittyProtocolActive() {
    return true;
  }

  start(): void {}
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
    this.xterm.write(data);
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

  async drainXterm(): Promise<void> {
    await new Promise<void>((resolve) => this.xterm.write('', resolve));
  }

  bufferText(): string {
    const lines: string[] = [];
    const buffer = this.xterm.buffer.active;
    for (let i = 0; i < buffer.length; i++) {
      lines.push(buffer.getLine(i)?.translateToString(true) ?? '');
    }
    return lines.join('\n');
  }
}

const surfaces = {
  ApprovalPrompt: () => null,
  StatusLine: () => null,
  ActivityTray: () => null,
} as unknown as VariantLayoutProps;

const user = (id: string): MessageType => ({
  id,
  role: MessageRole.User,
  content: id,
});

const model = (id: string, content: string): MessageType => ({
  id,
  role: MessageRole.Model,
  content,
});

let activeInstance: Instance | null = null;
let sandboxHome: string | undefined;
let previousKiroHome: string | undefined;
let nextClearToken = 10_000;

beforeEach(() => {
  previousKiroHome = process.env.KIRO_HOME;
  sandboxHome = mkdtempSync(join(tmpdir(), 'lite-settlement-'));
  process.env.KIRO_HOME = sandboxHome;
});

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
  if (previousKiroHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = previousKiroHome;
  if (sandboxHome) rmSync(sandboxHome, { recursive: true, force: true });
  sandboxHome = undefined;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 40));
  await Promise.resolve();
}

function createStore(messages: MessageType[], isProcessing: boolean) {
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
  const streamingModel = messages.findLast(
    (message) => message.role === MessageRole.Model
  );
  store.setState({
    uiMode: 'lite',
    mode: 'inline',
    sessionId: 'session-1',
    isInitialized: true,
    messages,
    isProcessing,
    streamingContent:
      isProcessing && streamingModel?.role === MessageRole.Model
        ? streamingModel.content
        : '',
    streamingMessageId: isProcessing ? streamingModel?.id : null,
    lite: {
      ...store.getState().lite,
      scrollbackClearToken: nextClearToken++,
    },
  });
  return store;
}

function mount(
  store: ReturnType<typeof createStore>,
  terminal: MockTerminal
): void {
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <LiteLayout {...surfaces} />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false, preserveScrollbackOnRedraw: true }
  );
}

describe('LiteLayout settle-then-flush', () => {
  it('promotes a finalized row after a covering paint', async () => {
    const terminal = new MockTerminal(80, 24);
    const messages = [user('turn-1'), model('answer-1', 'FINAL-SHORT')];
    const store = createStore(messages, true);
    mount(store, terminal);
    await flush();
    const before = __liteSettlePromotionsForTests();

    store.setState({
      isProcessing: false,
      streamingContent: '',
      streamingMessageId: null,
    });
    await flush();
    await flush();

    expect(__liteSettleBufferSizeForTests()).toBe(0);
    const after = __liteSettlePromotionsForTests();
    expect(after.guarantee).toBeGreaterThan(before.guarantee);
    expect(after.escape).toBe(before.escape);
  });

  it('uses the bounded stale-paint escape for rows above the viewport', async () => {
    const terminal = new MockTerminal(60, 10);
    const tall = Array.from(
      { length: 40 },
      (_, i) => `LITE-TALL-${i} keeps the settled block above the viewport`
    ).join('\n');
    const messages = [user('turn-1'), model('answer-1', tall)];
    const store = createStore(messages, true);
    mount(store, terminal);
    await flush();
    const changedTall = tall.replace('LITE-TALL-0', 'LITE-TALL-0-CHANGED');
    store.setState({
      messages: [user('turn-1'), model('answer-1', changedTall)],
      streamingContent: changedTall,
    });
    await flush();
    const before = __liteSettlePromotionsForTests();

    store.setState({
      isProcessing: false,
      streamingContent: '',
      streamingMessageId: null,
    });
    await flush();
    for (let i = 0; i < 6 && __liteSettleBufferSizeForTests() > 0; i++) {
      store.setState({ contextUsagePercent: i + 1 });
      await flush();
    }

    expect(__liteSettleBufferSizeForTests()).toBe(0);
    const after = __liteSettlePromotionsForTests();
    expect(after.escape).toBeGreaterThan(before.escape);
  });

  it('commits staged rows before a clear-token session boundary', async () => {
    const terminal = new MockTerminal(60, 10);
    const marker = 'BOUNDARY-FINAL-ONCE';
    const tall = Array.from(
      { length: 30 },
      (_, i) => `${i === 1 ? marker : `BOUNDARY-${i}`} fills the viewport`
    ).join('\n');
    const messages = [user('turn-1'), model('answer-1', tall)];
    const store = createStore(messages, true);
    mount(store, terminal);
    await flush();
    const changedTall = tall.replace('BOUNDARY-0', 'BOUNDARY-0-CHANGED');
    store.setState({
      messages: [user('turn-1'), model('answer-1', changedTall)],
      streamingContent: changedTall,
    });
    await flush();
    const beforeFinalization = __liteSettleBufferSizeForTests();

    store.setState({
      isProcessing: false,
      streamingContent: '',
      streamingMessageId: null,
    });
    await flush();
    expect(__liteSettleBufferSizeForTests()).toBeGreaterThan(
      beforeFinalization
    );
    store.setState((state) => ({
      messages: [],
      lite: {
        ...state.lite,
        scrollbackClearToken: state.lite.scrollbackClearToken + 1,
      },
    }));
    await flush();
    for (let i = 0; i < 6 && __liteSettleBufferSizeForTests() > 0; i++) {
      store.setState({ contextUsagePercent: i + 1 });
      await flush();
    }

    expect(__liteStaticItemIdsForTests()).toContain('answer-1');
    await terminal.drainXterm();

    const normalizedBuffer = terminal.bufferText().replaceAll('\n', '');
    expect(normalizedBuffer.split(marker).length - 1).toBe(1);
  });

  it('does not re-stage a carried row when messages survive a clear-token boundary', async () => {
    const terminal = new MockTerminal(60, 10);
    const marker = 'POPULATED-BOUNDARY-FINAL-ONCE';
    const tall = Array.from(
      { length: 30 },
      (_, i) => `${i === 1 ? marker : `POPULATED-${i}`} fills the viewport`
    ).join('\n');
    const messages = [user('turn-1'), model('answer-1', tall)];
    const store = createStore(messages, true);
    mount(store, terminal);
    await flush();
    const changedTall = tall.replace('POPULATED-0', 'POPULATED-0-CHANGED');
    store.setState({
      messages: [user('turn-1'), model('answer-1', changedTall)],
      streamingContent: changedTall,
    });
    await flush();

    store.setState({
      isProcessing: false,
      streamingContent: '',
      streamingMessageId: null,
    });
    await flush();
    expect(__liteSettleBufferSizeForTests()).toBeGreaterThan(0);

    store.setState((state) => ({
      lite: {
        ...state.lite,
        scrollbackClearToken: state.lite.scrollbackClearToken + 1,
      },
    }));
    await flush();
    for (let i = 0; i < 6 && __liteSettleBufferSizeForTests() > 0; i++) {
      store.setState({ contextUsagePercent: i + 1 });
      await flush();
    }

    const staticIds = __liteStaticItemIdsForTests();
    expect(staticIds.filter((id) => id === 'answer-1')).toHaveLength(1);
    expect(staticIds).toContain('turn-1');
    expect(staticIds.indexOf('turn-1')).toBeLessThan(
      staticIds.indexOf('answer-1')
    );
    await terminal.drainXterm();

    const normalizedBuffer = terminal.bufferText().replaceAll('\n', '');
    expect(normalizedBuffer.split(marker).length - 1).toBe(1);
  });
});
