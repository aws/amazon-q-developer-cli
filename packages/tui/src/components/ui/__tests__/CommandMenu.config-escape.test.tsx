/**
 * /config → agents ESC-back: closing the /agent picker with ESC walks back
 * to the /config table when configReturnOnEscape is primed (row-select or
 * typed /config agents), while a bare /agent (flag unset) closes outright.
 * Selecting an agent consumes the flag instead of bouncing back.
 */

import { afterEach, describe, expect, test } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { CommandMenu } from '../CommandMenu.js';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
} from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';

const ESC = '\x1b';
const ENTER = '\r';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return 100;
  }
  get rows() {
    return 30;
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
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

function mountPicker(opts: { fromConfig: boolean }): {
  terminal: MockTerminal;
  store: AppStoreApi;
} {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  store.getState().setActiveCommand({
    command: {
      name: '/agent',
      description: '',
      meta: { inputType: 'selection' },
    },
    options: [
      { value: 'default', label: 'Default', description: '[active]' },
      { value: 'mine', label: 'Mine', description: 'd' },
    ],
  });
  store.getState().setConfigReturnOnEscape(opts.fromConfig);
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <CommandMenu />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return { terminal, store };
}

describe('/config-routed agent picker ESC behavior', () => {
  test('ESC with configReturnOnEscape primed returns to the /config table', async () => {
    const { terminal, store } = mountPicker({ fromConfig: true });
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(store.getState().activeCommand).toBeNull();
    expect(store.getState().showConfigPanel).toBe(true);
    // Consumed on the walk-back: the next overlay close must not bounce.
    expect(store.getState().configReturnOnEscape).toBe(false);
  });

  test('ESC from a bare /agent picker (flag unset) closes outright', async () => {
    const { terminal, store } = mountPicker({ fromConfig: false });
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(store.getState().activeCommand).toBeNull();
    expect(store.getState().showConfigPanel).toBe(false);
  });

  test('selecting an agent consumes the flag and does not reopen /config', async () => {
    const { terminal, store } = mountPicker({ fromConfig: true });
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(store.getState().activeCommand).toBeNull();
    expect(store.getState().showConfigPanel).toBe(false);
    expect(store.getState().configReturnOnEscape).toBe(false);
  });
});
