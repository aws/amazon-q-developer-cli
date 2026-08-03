/**
 * The live "thinking" counter resets per ROUND, not per thinking batch. The
 * store toggles `thinkingContent` empty↔non-empty repeatedly inside one
 * reasoning stretch, so resetting on that transition zeroed the timer mid-round
 * (regardless of the /verbose display setting — the store fills it either way).
 */
import { describe, test, expect, afterEach } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import stripAnsi from 'strip-ansi';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  type MessageType,
} from '../../../../stores/app-store.js';
import { Kiro } from '../../../../kiro.js';
import { LiteLiveRegion } from '../LiteLiveRegion.js';
import {
  resetVerboseCache,
  setVerboseConfig,
} from '../../../../lite/verbose.js';
import { useTempKiroHome as prepareTempKiroHome } from '../../../../lite/__tests__/temp-kiro-home.js';

prepareTempKiroHome();
let out = '';
const noop = () => {};
const terminal = {
  columns: 80,
  rows: 24,
  kittyProtocolActive: true,
  start: noop,
  stop: noop,
  drainInput: async () => {},
  write: (d: string) => {
    out += d;
  },
  moveBy: noop,
  hideCursor: noop,
  showCursor: noop,
  clearLine: noop,
  clearFromCursor: noop,
  clearScreen: noop,
  enableMouse: noop,
  disableMouse: noop,
  setTitle: noop,
} as unknown as Terminal;

let instance: Instance | null = null;
let realNow: () => number = Date.now;
let now = 1_000_000;

afterEach(() => {
  instance?.unmount();
  instance = null;
  Date.now = realNow;
  setVerboseConfig({ filters: [] });
  resetVerboseCache();
});

/** Settle render + effects + a few 150ms spinner ticks. */
async function flush(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 30));
  }
}

/** Displayed thinking seconds from the freshest frame only. */
async function displayedSecs(): Promise<number> {
  out = '';
  now += 200;
  await flush();
  let max = 0;
  for (const m of out.matchAll(/(\d+)s/g)) max = Math.max(max, Number(m[1]));
  return max;
}

function mount(): ReturnType<typeof createAppStore> {
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
  store.setState({ isProcessing: true, thinkingContent: 'reasoning' } as never);
  instance = render(
    <AppStoreContext.Provider value={store}>
      <LiteLiveRegion />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  return store;
}

async function renderBareMcpLiveOutput(filters: string[]): Promise<string> {
  instance?.unmount();
  instance = null;
  out = '';
  setVerboseConfig({ filters });
  resetVerboseCache();
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'v2',
    uiMode: 'lite',
  });
  const tool: MessageType = {
    id: 'bare-mcp-live',
    role: MessageRole.ToolUse,
    name: 'InternalCodeSearch',
    mcpServerName: 'builder-mcp',
    content: JSON.stringify({ query: 'handler' }),
    isFinished: false,
  };
  store.setState({
    isProcessing: true,
    messages: [tool],
    liveOutputs: new Map([['bare-mcp-live', [['MCP_LIVE_OUTPUT_MARKER']]]]),
  } as never);
  instance = render(
    <AppStoreContext.Provider value={store}>
      <LiteLiveRegion />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  await flush();
  return stripAnsi(out);
}

describe('LiteLiveRegion thinking timer', () => {
  test('a fresh thinking burst mid-round does NOT reset the timer', async () => {
    realNow = Date.now;
    Date.now = () => now;
    const store = mount();
    await flush();

    now += 5000;
    const before = await displayedSecs();
    expect(before).toBeGreaterThanOrEqual(4);

    // Commit/tool boundary clears thinkingContent; the next Thought refills it.
    store.setState({ thinkingContent: '' } as never);
    await flush();
    store.setState({ thinkingContent: 'more reasoning' } as never);
    await flush();

    expect(await displayedSecs()).toBeGreaterThanOrEqual(before);
  });

  test('a real round boundary (tool runs, then idle) DOES reset the timer', async () => {
    // Guard: the per-round reset must survive removing the per-batch one.
    realNow = Date.now;
    Date.now = () => now;
    const store = mount();
    await flush();

    now += 7000;
    const before = await displayedSecs();
    expect(before).toBeGreaterThanOrEqual(6);

    const tool: MessageType = {
      id: 'tool-1',
      role: MessageRole.ToolUse,
      name: 'execute_bash',
      content: JSON.stringify({ command: 'ls' }),
      isFinished: false,
    };
    store.setState({ messages: [tool], thinkingContent: '' } as never);
    await flush();
    store.setState({
      messages: [{ ...tool, isFinished: true }],
      thinkingContent: 'reasoning after the tool',
    } as never);
    await flush();

    expect(await displayedSecs()).toBeLessThan(before);
  });

  test('bare MCP live output obeys the mcp filter', async () => {
    const hidden = await renderBareMcpLiveOutput(['all', '-mcp']);
    expect(hidden).not.toContain('MCP_LIVE_OUTPUT_MARKER');

    const shown = await renderBareMcpLiveOutput(['mcp']);
    expect(shown).toContain('MCP_LIVE_OUTPUT_MARKER');
  });
});
