import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 80, height: 24 };
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '../../../hooks/useTerminalSize.js',
]);

mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));

import {
  SessionPickerPanel,
  type SessionPickerRow,
} from '../SessionPickerPanel.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';

const DOWN = '\x1b[B';
const UP = '\x1b[A';
const ESC = '\x1b';
const ENTER = '\r';

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
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

function row(
  sessionId: string,
  title: string,
  environment: 'local' | 'cloud' = 'local',
  status = 'idle',
  updatedAt = '2026-01-01T00:00:00.000Z'
): SessionPickerRow {
  return { sessionId, title, environment, status, updatedAt };
}

function mountPicker(rows: SessionPickerRow[]) {
  const terminal = new MockTerminal();
  const onSelect = vi.fn();
  const onClose = vi.fn();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <SessionPickerPanel rows={rows} onSelect={onSelect} onClose={onClose} />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return { terminal, onSelect, onClose };
}

describe('SessionPickerPanel key wiring', () => {
  test('renders the column headers and every row title', async () => {
    const { terminal } = mountPicker([
      row('sess-aaaaaaaa', 'First task'),
      row('sess-bbbbbbbb', 'Second task', 'cloud', 'working'),
    ]);
    await flush();
    for (const header of ['ID', 'Name', 'Environment', 'Status'])
      expect(terminal.output).toContain(header);
    expect(terminal.output).toContain('Last updated');
    expect(terminal.output).toContain('First task');
    expect(terminal.output).toContain('Second task');
  });

  test('enter resumes the highlighted row (id + environment) and closes', async () => {
    const { terminal, onSelect, onClose } = mountPicker([
      row('sess-aaaaaaaa', 'First', 'cloud'),
      row('sess-bbbbbbbb', 'Second'),
    ]);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('sess-aaaaaaaa', 'cloud');
    expect(onClose).toHaveBeenCalled();
  });

  test('esc closes without resuming anything', async () => {
    const { terminal, onSelect, onClose } = mountPicker([
      row('sess-aaaaaaaa', 'First'),
    ]);
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSelect).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  test('arrow-down moves the cursor before resuming the second row', async () => {
    const { terminal, onSelect } = mountPicker([
      row('sess-aaaaaaaa', 'First'),
      row('sess-bbbbbbbb', 'Second'),
    ]);
    await flush();
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('sess-bbbbbbbb', 'local');
  });

  test('arrow-up clamps at the top (resumes the first row)', async () => {
    const { terminal, onSelect } = mountPicker([
      row('sess-aaaaaaaa', 'First'),
      row('sess-bbbbbbbb', 'Second'),
    ]);
    await flush();
    terminal.sendInput(UP);
    terminal.sendInput(UP);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('sess-aaaaaaaa', 'local');
  });

  test('arrow-down clamps at the bottom (resumes the last row)', async () => {
    const { terminal, onSelect } = mountPicker([
      row('sess-aaaaaaaa', 'First'),
      row('sess-bbbbbbbb', 'Second'),
    ]);
    await flush();
    terminal.sendInput(DOWN);
    terminal.sendInput(DOWN);
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('sess-bbbbbbbb', 'local');
  });

  test('typing filters the list; enter resumes the surviving row', async () => {
    const { terminal, onSelect } = mountPicker([
      row('sess-aaaaaaaa', 'alpha'),
      row('sess-bbbbbbbb', 'bravo'),
      row('sess-cccccccc', 'charlie'),
    ]);
    await flush();
    for (const ch of 'bravo') terminal.sendInput(ch);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('sess-bbbbbbbb', 'local');
  });

  test('the filter matches the environment column (e.g. "cloud")', async () => {
    const { terminal, onSelect } = mountPicker([
      row('sess-aaaaaaaa', 'alpha', 'local'),
      row('sess-bbbbbbbb', 'bravo', 'cloud'),
    ]);
    await flush();
    for (const ch of 'cloud') terminal.sendInput(ch);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('sess-bbbbbbbb', 'cloud');
  });

  test('backspace widens the filter back out', async () => {
    const { terminal, onSelect } = mountPicker([
      row('sess-aaaaaaaa', 'alpha'),
      row('sess-zzzzzzzz', 'zeta'),
    ]);
    await flush();
    for (const ch of 'zeta') terminal.sendInput(ch);
    await flush();
    for (let i = 0; i < 4; i += 1) terminal.sendInput('\x7f');
    await flush();
    terminal.sendInput(ENTER); // cursor back on the first row of the widened list
    await flush();
    expect(onSelect).toHaveBeenCalledWith('sess-aaaaaaaa', 'local');
  });

  test('space is a no-op (single-select surface — resume is via enter)', async () => {
    const { terminal, onSelect, onClose } = mountPicker([
      row('sess-aaaaaaaa', 'First'),
    ]);
    await flush();
    terminal.sendInput(' ');
    await flush();
    expect(onSelect).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  test('shows the empty-state line when there are no sessions', async () => {
    const { terminal } = mountPicker([]);
    await flush();
    expect(terminal.output).toContain('No sessions found.');
  });

  test('windows the list on a short terminal and reports the hidden count', async () => {
    mockTermSize.height = 12; // termHeight - 9 = 3 visible rows
    try {
      const rows = Array.from({ length: 20 }, (_, i) =>
        row(`sess-${i}-xxxxxxxx`, `Task ${i}`)
      );
      const { terminal } = mountPicker(rows);
      await flush();
      const shown = (terminal.output.match(/Task \d+/g) ?? []).filter(
        (v, i, a) => a.indexOf(v) === i
      ).length;
      expect(shown).toBeGreaterThan(0);
      expect(shown).toBeLessThanOrEqual(4);
      // 20 rows, 3 visible -> 17 hidden.
      expect(terminal.output).toContain('(+17 more)');
    } finally {
      mockTermSize.height = 24;
    }
  });
});
