/**
 * Regression: the cloud cold-boot connect screen (milestone checklist) must be
 * gated on a one-way "conversation entered" latch, NOT on `isInitialized`.
 *
 * `isInitialized` flips to true DURING cold boot — on a non-resume boot it lands
 * in the same render batch as the `session_create: ready` milestone — so gating
 * the connect screen on `!isInitialized` tore it down in the very frame that
 * would first paint "Cloud session created", and the checklist never rendered
 * (the macOS cloud-boot E2E caught this). The latch instead stays false through
 * the whole first boot (messages empty → checklist shows) and only latches true
 * once a message appears, so a later in-session `/chat new` that re-empties
 * `messages` cannot re-open the cold-boot screen.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import { InlineLayout } from '../InlineLayout.js';
import { UI_VARIANTS } from '../ui-variants.js';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
} from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';

class MockTerminal implements Terminal {
  public output = '';
  constructor(private readonly width = 120) {}
  get columns() {
    return this.width;
  }
  get rows() {
    return 42;
  }
  get kittyProtocolActive() {
    return true;
  }
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

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 30));
  await Promise.resolve();
}

function bootingCloudStore() {
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
  store.setState({
    uiMode: 'tui',
    mode: 'inline',
    sessionId: 'session-1',
    cloudSessionActive: true,
    bootProgress: new Map([
      ['agent_connect', { label: 'Connecting', status: 'ready', startTime: 0 }],
      [
        'session_create',
        { label: 'Creating session', status: 'ready', startTime: 0 },
      ],
    ]),
    messages: [],
  });
  return store;
}

function renderInline(store: ReturnType<typeof bootingCloudStore>) {
  const terminal = new MockTerminal();
  activeInstance = render(
    React.createElement(
      AppStoreContext.Provider,
      { value: store },
      React.createElement(InlineLayout, UI_VARIANTS.tui)
    ),
    { terminal, exitOnCtrlC: false }
  );
  return terminal;
}

describe('cloud cold-boot connect screen latch', () => {
  it('shows the checklist during cold boot even though isInitialized is already true', async () => {
    const store = bootingCloudStore();
    // The regression case: init completed in the same batch as the milestone.
    store.setState({ isInitialized: true });
    const terminal = renderInline(store);
    await flush();
    const output = stripAnsi(terminal.output);
    // The checklist milestone renders — it does NOT depend on !isInitialized.
    expect(output).toContain('Cloud session created');
  });

  it('shows the post-/chat new checklist when armed, and hides it after the first message', async () => {
    const store = bootingCloudStore();
    store.setState({ isInitialized: true });
    const terminal = renderInline(store);
    await flush();
    // Enter the conversation so the cold-boot connect screen is latched off.
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hi' }] as never,
    });
    await flush();
    // /chat new: previous rows cleared, session recreated → checklist armed.
    terminal.output = '';
    store.setState({ messages: [], cloudNewSessionChecklist: true });
    await flush();
    expect(stripAnsi(terminal.output)).toContain('Cloud session created');
    // First message in the new session dismisses the checklist.
    terminal.output = '';
    store.setState({
      messages: [{ id: 'u2', role: MessageRole.User, content: 'go' }] as never,
      cloudNewSessionChecklist: false,
    });
    await flush();
    expect(stripAnsi(terminal.output)).not.toContain('Cloud session created');
  });

  it('keeps the connect screen hidden after the conversation was entered (in-session /chat new)', async () => {
    const store = bootingCloudStore();
    store.setState({ isInitialized: true });
    const terminal = renderInline(store);
    await flush();
    // Enter the conversation (a message renders) — this latches the screen off.
    store.setState({
      messages: [
        { id: 'u1', role: MessageRole.User, content: 'hello' },
      ] as never,
    });
    await flush();
    // /chat new re-empties the message list; the connect screen must NOT return.
    terminal.output = '';
    store.setState({ messages: [] });
    await flush();
    const output = stripAnsi(terminal.output);
    expect(output).not.toContain('Cloud session created');
    expect(output).not.toContain('Creating cloud session');
  });

  it('latch survives an InlineLayout remount (crew-monitor / session-view round-trip)', async () => {
    // The latch is store-held: a component-local ref would reset when
    // AppContainer unmounts InlineLayout (Ctrl+G crew monitor, /switch
    // session-view, lite↔tui swap) and the connect screen would replay over
    // the post-/chat-new empty conversation with a stale checklist.
    const store = bootingCloudStore();
    store.setState({ isInitialized: true });
    renderInline(store);
    await flush();
    store.setState({
      messages: [{ id: 'u1', role: MessageRole.User, content: 'hi' }] as never,
    });
    await flush();
    // /chat new: conversation re-emptied, then the layout unmounts/remounts.
    store.setState({ messages: [] });
    await flush();
    activeInstance?.unmount();
    activeInstance = null;
    const terminal = renderInline(store);
    await flush();
    const output = stripAnsi(terminal.output);
    expect(output).not.toContain('Cloud session created');
    expect(output).not.toContain('Creating cloud session');
  });
});
