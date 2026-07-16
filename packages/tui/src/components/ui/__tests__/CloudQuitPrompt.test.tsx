import { afterEach, describe, expect, test, vi } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { CloudQuitPrompt } from '../CloudQuitPrompt.js';

const DOWN = '\x1b[B';
const UP = '\x1b[A';
const ENTER = '\r';
const ESC = '\x1b';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
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
    this.onInput = onInput;
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
    this.onInput?.(data);
  }
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 20));
  await Promise.resolve();
}

function mountPrompt() {
  const terminal = new MockTerminal();
  const onKeepRunning = vi.fn();
  const onTurnOff = vi.fn();
  const onCancel = vi.fn();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <CloudQuitPrompt
        onKeepRunning={onKeepRunning}
        onTurnOff={onTurnOff}
        onCancel={onCancel}
      />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return { terminal, onKeepRunning, onTurnOff, onCancel };
}

/** Wait until the menu is mounted and listening (deflakes the cold first mount). */
async function settle(terminal: MockTerminal): Promise<void> {
  for (let i = 0; i < 10; i++) {
    await flush();
    if (terminal.output.includes('agent')) return;
  }
}

describe('CloudQuitPrompt key wiring', () => {
  test('enter on the default row fires onKeepRunning only', async () => {
    const { terminal, onKeepRunning, onTurnOff, onCancel } = mountPrompt();
    await settle(terminal);
    // Warm-up round-trip: proves input is wired and lands the cursor back on
    // row 0 deterministically before the assertion keypress.
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(UP);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onKeepRunning).toHaveBeenCalledTimes(1);
    expect(onTurnOff).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  test('down + enter fires onTurnOff only', async () => {
    const { terminal, onKeepRunning, onTurnOff } = mountPrompt();
    await flush();
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onTurnOff).toHaveBeenCalledTimes(1);
    expect(onKeepRunning).not.toHaveBeenCalled();
  });

  test('esc fires onCancel without selecting either action', async () => {
    const { terminal, onKeepRunning, onTurnOff, onCancel } = mountPrompt();
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onCancel).toHaveBeenCalled();
    expect(onKeepRunning).not.toHaveBeenCalled();
    expect(onTurnOff).not.toHaveBeenCalled();
  });
});
