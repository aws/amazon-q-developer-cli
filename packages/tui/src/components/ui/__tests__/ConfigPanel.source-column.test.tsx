/**
 * ConfigPanel category-table Source column gating — the table builds its
 * columns inline, so only a component-level render exercises this surface:
 * sourcesReported false → no Source column anywhere.
 */

import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 100, height: 30 };
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
restoreRealModulesAfterAll(import.meta.dir, [
  '../../../hooks/useTerminalSize.js',
]);
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));

import { ConfigPanel } from '../ConfigPanel.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import type { ConfigSnapshot } from '../config-panel-model.js';

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

const baseSnapshot: Omit<ConfigSnapshot, 'sourcesReported'> = {
  cloudSession: false,
  agents: [{ id: 'default', name: 'Default' }],
  mcpServers: [{ name: 'github', status: 'running', toolCount: 3 }],
  steering: [],
  steeringDocs: [],
  skills: [],
  hooks: [],
  powers: [],
  diagnostics: [],
};

function mountTable(sourcesReported: boolean) {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  const element = (reported: boolean) => (
    <AppStoreContext.Provider value={store}>
      <ConfigPanel
        snapshot={{ ...baseSnapshot, sourcesReported: reported }}
        onClose={vi.fn()}
        onOpenMcp={vi.fn()}
        onOpenHooks={vi.fn()}
        onOpenAgent={vi.fn()}
      />
    </AppStoreContext.Provider>
  );
  activeInstance = render(element(sourcesReported), { terminal });
  const rerender = (reported: boolean) =>
    activeInstance!.rerender(element(reported));
  return { terminal, rerender };
}

describe('ConfigPanel category-table Source column gating', () => {
  test('sourcesReported false: table has Category|Status only', async () => {
    const { terminal } = mountTable(false);
    await flush();
    expect(terminal.output).toContain('Category');
    expect(terminal.output).toContain('Status');
    expect(terminal.output).not.toContain('Source');
  });

  test('sourcesReported true: Source column renders', async () => {
    const { terminal } = mountTable(true);
    await flush();
    expect(terminal.output).toContain('Source');
  });

  test('gate is latched: a snapshot flip mid-panel does not insert the column', async () => {
    const { terminal, rerender } = mountTable(false);
    await flush();
    expect(terminal.output).not.toContain('Source');
    // A descriptor push flipping sourcesReported re-renders the open panel;
    // the mount-latched gate must hold — no column insertion mid-view.
    terminal.output = '';
    rerender(true);
    await flush();
    expect(terminal.output).not.toContain('Source');
  });
});
