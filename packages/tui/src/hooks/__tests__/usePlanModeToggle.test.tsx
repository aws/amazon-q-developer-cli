// Shift+Tab arrives as CSI Z (`\x1b[Z`), which twinki decodes to key.tab +
// key.shift; MockTerminal.sendInput drives it through the real input pipeline.
import { describe, test, expect, afterEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { AppStoreContext, createAppStore } from '../../stores/app-store.js';
import { Kiro } from '../../kiro.js';
import { usePlanModeToggle } from '../usePlanModeToggle.js';

const SHIFT_TAB = '\x1b[Z';

class MockTerminal implements Terminal {
  private _onInput: ((data: string) => void) | null = null;
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
  write(): void {}
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
let activeTerminal: MockTerminal | null = null;
afterEach(() => {
  if (activeInstance) {
    activeInstance.unmount();
    activeInstance = null;
  }
  activeTerminal = null;
  vi.useRealTimers();
});

function mount(initial: {
  currentAgent?: { name: string } | null;
  previousAgentName?: string | null;
  swapResult?: { success: boolean; data?: unknown };
}) {
  const store = createAppStore({ kiro: new Kiro() });
  const executeCommand = vi.fn(
    async () =>
      initial.swapResult ?? { success: true, data: { agent: { name: 'x' } } }
  );
  const sendModeChanged = vi.fn();
  store.setState({
    currentAgent: initial.currentAgent ?? null,
    previousAgentName: initial.previousAgentName ?? null,
    kiro: { executeCommand, sendModeChanged, sessionId: 's1' },
  } as any);

  const terminal = new MockTerminal();
  activeTerminal = terminal;
  const instance = render(
    <AppStoreContext.Provider value={store}>
      <Probe />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  activeInstance = instance;
  return { store, executeCommand, sendModeChanged };
}

function Probe() {
  usePlanModeToggle();
  return null;
}

// twinki wires the input callback a tick after render; allow time for the
// handler to attach and the async swap RPC to settle.
async function flush(ms = 40): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, ms));
  await Promise.resolve();
}

describe('usePlanModeToggle', () => {
  test('Shift+Tab from a normal agent enters plan mode (swaps to kiro_planner)', async () => {
    const { store, executeCommand } = mount({
      currentAgent: { name: 'kiro_default' },
      swapResult: { success: true, data: { agent: { name: 'kiro_planner' } } },
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    expect(executeCommand).toHaveBeenCalledWith({
      command: 'agent',
      args: { agentName: 'kiro_planner' },
    });
    // Records where we came from so the reverse toggle can restore it.
    expect(store.getState().previousAgentName).toBe('kiro_default');
  });

  test('Shift+Tab while in plan mode exits back to the previous agent', async () => {
    const { executeCommand } = mount({
      currentAgent: { name: 'kiro_planner' },
      previousAgentName: 'kiro_default',
      swapResult: { success: true, data: { agent: { name: 'kiro_default' } } },
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    expect(executeCommand).toHaveBeenCalledWith({
      command: 'agent',
      args: { agentName: 'kiro_default' },
    });
  });

  test('Shift+Tab in plan mode with no previous agent is a no-op', async () => {
    const { executeCommand } = mount({
      currentAgent: { name: 'kiro_planner' },
      previousAgentName: null,
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    expect(executeCommand).not.toHaveBeenCalled();
  });
});
