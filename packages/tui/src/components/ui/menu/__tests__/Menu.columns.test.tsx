/**
 * Opt-in Menu column mode: without `columnHeaders` the menu ignores item
 * annotations and renders exactly as before; with it, a dim header row and an
 * aligned annotation column appear between label and group.
 */

import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import { restoreRealModulesAfterAll } from '../../../../test-utils/restore-modules.js';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 100, height: 30 };
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
restoreRealModulesAfterAll(import.meta.dir, [
  '../../../../hooks/useTerminalSize.js',
]);
mock.module('../../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));

import { Menu, type MenuItem } from '../Menu.js';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import { Kiro } from '../../../../kiro.js';

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

const ITEMS: MenuItem[] = [
  {
    label: 'Default',
    description: '[active]',
    group: 'Bundled',
    annotation: 'local',
  },
  { label: 'Mine', description: 'd', group: 'Workspace', annotation: 'cloud' },
];

function mount(columnHeaders?: {
  label: string;
  annotation: string;
  group?: string;
  description?: string;
}) {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <Menu
        items={ITEMS}
        prefix=""
        onSelect={vi.fn()}
        showSelectedIndicator={true}
        columnHeaders={columnHeaders}
      />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return terminal;
}

describe('Menu opt-in column mode', () => {
  test('no columnHeaders: annotations are ignored, no header row', async () => {
    const terminal = mount(undefined);
    await flush();
    const text = stripAnsi(terminal.output);
    expect(text).toContain('Default');
    expect(text).toContain('Mine');
    expect(text).not.toContain('Name');
    expect(text).not.toContain('Source');
    expect(text).not.toContain('local');
    expect(text).not.toContain('cloud');
  });

  test('columnHeaders: full header row aligned over every column', async () => {
    const terminal = mount({
      label: 'Name',
      annotation: 'Source',
      group: 'Scope',
      description: 'Description',
    });
    await flush();
    const lines = stripAnsi(terminal.output).split('\n');
    const headerLine = lines.find((l) => l.includes('Name'));
    const defaultLine = lines.find((l) => l.includes('Default'));
    const mineLine = lines.find((l) => l.includes('Mine'));
    expect(headerLine).toBeDefined();
    expect(headerLine).toContain('Source');
    expect(headerLine).toContain('Scope');
    expect(headerLine).toContain('Description');
    expect(defaultLine).toContain('local');
    expect(mineLine).toContain('cloud');
    // Aligned: every header starts at the same offset as its column's cells.
    expect(headerLine!.indexOf('Source')).toBe(defaultLine!.indexOf('local'));
    expect(defaultLine!.indexOf('local')).toBe(mineLine!.indexOf('cloud'));
    expect(headerLine!.indexOf('Scope')).toBe(defaultLine!.indexOf('Bundled'));
    expect(headerLine!.indexOf('Scope')).toBe(mineLine!.indexOf('Workspace'));
    expect(headerLine!.indexOf('Description')).toBe(
      defaultLine!.indexOf('[active]')
    );
    expect(headerLine!.indexOf('Name')).toBe(defaultLine!.indexOf('Default'));
    // Header lands above the first item.
    expect(lines.indexOf(headerLine!)).toBeLessThan(
      lines.indexOf(defaultLine!)
    );
  });

  test('group/description headers are omitted when not provided', async () => {
    const terminal = mount({ label: 'Name', annotation: 'Source' });
    await flush();
    const lines = stripAnsi(terminal.output).split('\n');
    const headerLine = lines.find((l) => l.includes('Name'));
    expect(headerLine).toBeDefined();
    expect(headerLine).toContain('Source');
    expect(headerLine).not.toContain('Scope');
    expect(headerLine).not.toContain('Description');
  });
});
