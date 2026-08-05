/**
 * SubagentToolPanel crew navigation (ctrl+d / ctrl+u).
 *
 * The footer strip lets the user move the focused-agent highlight without
 * opening the crew monitor. Navigation is circular: ctrl+d past the last row
 * wraps to the first, ctrl+u before the first wraps to the last.
 */

import { describe, test, expect, afterEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import type { AgentSession } from '../../../types/multi-session.js';
import { SubagentToolPanel } from '../SubagentToolPanel.js';

// Raw control bytes twinki decodes into ctrl+d / ctrl+u Key events.
const CTRL_D = '\x04';
const CTRL_U = '\x15';

class MockTerminal implements Terminal {
  private _onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return 80;
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

/**
 * Settle render + effects so twinki's useInput subscription is live before we
 * send keys. The subscription registers on a macrotask turn after the first
 * commit, so pump several timer turns for headroom.
 */
async function flush(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

function makeSession(overrides: Partial<AgentSession>): AgentSession {
  return {
    id: 'sess-1',
    name: 'writer',
    status: 'busy',
    type: 'ephemeral',
    created: new Date(1000),
    lastActivity: new Date(),
    ...overrides,
  };
}

function mountPanel() {
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
  const sessions = new Map<string, AgentSession>([
    ['s1', makeSession({ id: 's1', name: 'alpha' })],
    ['s2', makeSession({ id: 's2', name: 'beta' })],
    ['s3', makeSession({ id: 's3', name: 'gamma' })],
  ]);
  store.setState({ sessions, sessionId: 'main', focusedCrewIndex: 0 });

  const terminal = new MockTerminal();
  const instance = render(
    <AppStoreContext.Provider value={store}>
      <SubagentToolPanel />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  activeInstance = instance;
  return { store, terminal };
}

describe('SubagentToolPanel crew navigation', () => {
  test('ctrl+d moves down and wraps from last to first', async () => {
    const { store, terminal } = mountPanel();
    await flush();

    terminal.sendInput(CTRL_D);
    await flush();
    expect(store.getState().focusedCrewIndex).toBe(1);

    terminal.sendInput(CTRL_D);
    await flush();
    expect(store.getState().focusedCrewIndex).toBe(2);

    terminal.sendInput(CTRL_D);
    await flush();
    expect(store.getState().focusedCrewIndex).toBe(0);
  });

  test('ctrl+u moves up and wraps from first to last', async () => {
    const { store, terminal } = mountPanel();
    await flush();

    terminal.sendInput(CTRL_U);
    await flush();
    expect(store.getState().focusedCrewIndex).toBe(2);

    terminal.sendInput(CTRL_U);
    await flush();
    expect(store.getState().focusedCrewIndex).toBe(1);
  });
});
