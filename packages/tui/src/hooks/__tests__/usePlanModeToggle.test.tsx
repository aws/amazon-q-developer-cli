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
  // When set, setConfigOption rejects with this value (an RPC error).
  rejectWith?: unknown;
}) {
  const store = createAppStore({ kiro: new Kiro() });
  const sendModeChanged = vi.fn();
  const setConfigOption = vi.fn(async () => {
    if (initial.rejectWith !== undefined) throw initial.rejectWith;
  });
  store.setState({
    currentAgent: initial.currentAgent ?? null,
    previousAgentName: initial.previousAgentName ?? null,
    kiro: { setConfigOption, sendModeChanged, sessionId: 's1' },
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
  return { store, setConfigOption, sendModeChanged };
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
    const { store, setConfigOption, sendModeChanged } = mount({
      currentAgent: { name: 'kiro_default' },
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    expect(setConfigOption).toHaveBeenCalledWith('mode', 'kiro_planner');
    // Records where we came from so the reverse toggle can restore it.
    expect(store.getState().previousAgentName).toBe('kiro_default');
    // A successful (non-rejecting) switch flips the chip and fires the event.
    expect(store.getState().currentAgent?.name).toBe('kiro_planner');
    expect(sendModeChanged).toHaveBeenCalledWith(
      expect.objectContaining({
        fromMode: 'kiro_default',
        toMode: 'kiro_planner',
      })
    );
    expect(store.getState().transientAlert?.status).toBe('success');
  });

  test('Shift+Tab while in plan mode exits back to the previous agent', async () => {
    const { store, setConfigOption } = mount({
      currentAgent: { name: 'kiro_planner' },
      previousAgentName: 'kiro_default',
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    expect(setConfigOption).toHaveBeenCalledWith('mode', 'kiro_default');
    expect(store.getState().currentAgent?.name).toBe('kiro_default');
  });

  test('Shift+Tab before an agent is active (session initializing) is a no-op', async () => {
    const { store, setConfigOption } = mount({ currentAgent: null });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    // The primitive no-ops without a session, so toggling here must not flip
    // the chip or claim success.
    expect(setConfigOption).not.toHaveBeenCalled();
    expect(store.getState().currentAgent).toBeNull();
    expect(store.getState().transientAlert).toBeNull();
  });

  test('Shift+Tab in plan mode with no previous agent is a no-op', async () => {
    const { setConfigOption } = mount({
      currentAgent: { name: 'kiro_planner' },
      previousAgentName: null,
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    expect(setConfigOption).not.toHaveBeenCalled();
  });

  test('setConfigOption rejection surfaces an error and does not claim success', async () => {
    const { store, sendModeChanged } = mount({
      currentAgent: { name: 'kiro_default' },
      rejectWith: {
        code: -32603,
        message: 'Internal error',
        data: { details: 'planner unavailable' },
      },
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    const alert = store.getState().transientAlert;
    expect(alert?.status).toBe('error');
    // extractRpcErrorMessage prefers data.details over the generic RPC message.
    expect(alert?.message).toBe('planner unavailable');
    expect(sendModeChanged).not.toHaveBeenCalled();
    // A rejected switch must not flip the chip.
    expect(store.getState().currentAgent?.name).toBe('kiro_default');
  });

  // KAS resolves setConfigOption('mode', …) with void (no CommandResult), unlike
  // the v2-only executeCommand('agent') path that returned { success, data }.
  // The hook ignores the return value: a resolve means the swap landed, so it
  // flips the chip and fires sendModeChanged on both engines.
  test('KAS-shaped switch (setConfigOption resolves void) still updates the agent', async () => {
    const { store, setConfigOption, sendModeChanged } = mount({
      currentAgent: { name: 'kiro_default' },
    });
    await flush();

    activeTerminal!.sendInput(SHIFT_TAB);
    await flush();

    expect(setConfigOption).toHaveBeenCalledWith('mode', 'kiro_planner');
    expect(store.getState().currentAgent?.name).toBe('kiro_planner');
    expect(sendModeChanged).toHaveBeenCalledWith(
      expect.objectContaining({
        fromMode: 'kiro_default',
        toMode: 'kiro_planner',
      })
    );
  });
});
