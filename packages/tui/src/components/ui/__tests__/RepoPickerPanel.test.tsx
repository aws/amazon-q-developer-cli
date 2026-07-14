import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 80, height: 24 };
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));

import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { RepoPickerPanel } from '../RepoPickerPanel.js';
import type { SourceProviderResource } from '@kiro/acp-type-covenant';

const DOWN = '\x1b[B';
const UP = '\x1b[A';
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

function repo(name: string): SourceProviderResource {
  return { providerType: 'GITHUB', name } as SourceProviderResource;
}

function mountPicker(
  resources: SourceProviderResource[],
  initialSelected?: string[]
) {
  const terminal = new MockTerminal();
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <RepoPickerPanel
        resources={resources}
        initialSelected={initialSelected}
        onSubmit={onSubmit}
        onClose={onClose}
      />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return { terminal, onSubmit, onClose };
}

describe('RepoPickerPanel key wiring', () => {
  test('esc with no changes submits an empty selection and closes', async () => {
    const { terminal, onSubmit, onClose } = mountPicker([repo('acme/app')]);
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith([]);
    expect(onClose).toHaveBeenCalled();
  });

  test('space toggles the cursor row on and off; esc saves the selection', async () => {
    const { terminal, onSubmit } = mountPicker([
      repo('acme/app'),
      repo('acme/lib'),
    ]);
    await flush();
    terminal.sendInput(' '); // select acme/app
    await flush();
    terminal.sendInput(' '); // toggle it back off
    await flush();
    terminal.sendInput(' '); // select again
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app']);
  });

  test('arrow keys move the cursor and clamp at both list edges', async () => {
    const { terminal, onSubmit } = mountPicker([
      repo('acme/app'),
      repo('acme/lib'),
    ]);
    await flush();
    terminal.sendInput(UP); // already at top — must clamp, not go negative
    await flush();
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(DOWN); // at bottom — must clamp
    await flush();
    terminal.sendInput(' '); // toggles the LAST row if clamping worked
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/lib']);
  });

  test('typing filters the list; space then toggles the filtered row', async () => {
    const { terminal, onSubmit } = mountPicker([
      repo('acme/app'),
      repo('acme/lib'),
      repo('other/tool'),
    ]);
    await flush();
    for (const ch of 'tool') terminal.sendInput(ch);
    await flush();
    terminal.sendInput(' ');
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['other/tool']);
  });

  test('backspace edits the filter back out (selection targets the widened list)', async () => {
    const { terminal, onSubmit } = mountPicker([
      repo('acme/app'),
      repo('zeta/z'),
    ]);
    await flush();
    for (const ch of 'zeta') terminal.sendInput(ch);
    await flush();
    for (let i = 0; i < 4; i++) terminal.sendInput('\x7f'); // backspace all
    await flush();
    terminal.sendInput(' '); // cursor row in the unfiltered list
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app']);
  });

  test('space on an empty filtered list is a no-op (no crash, empty submit)', async () => {
    const { terminal, onSubmit } = mountPicker([repo('acme/app')]);
    await flush();
    for (const ch of 'nomatch') terminal.sendInput(ch);
    await flush();
    terminal.sendInput(' ');
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith([]);
  });

  test('initialSelected pre-checks attached repos; esc re-submits them unchanged', async () => {
    const { terminal, onSubmit } = mountPicker(
      [repo('acme/app'), repo('acme/lib')],
      ['acme/app']
    );
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app']);
  });

  test('a pre-checked repo can be toggled off before saving', async () => {
    const { terminal, onSubmit } = mountPicker(
      [repo('acme/app')],
      ['acme/app']
    );
    await flush();
    terminal.sendInput(' '); // cursor on acme/app -> uncheck the seeded selection
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith([]);
  });

  test('renders without overflow on a short terminal (derived row window)', async () => {
    mockTermSize.height = 12; // termHeight - 9 = 3 rows
    try {
      const terminal = new MockTerminal();
      const onSubmit = vi.fn();
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      activeInstance = render(
        <AppStoreContext.Provider value={store}>
          <RepoPickerPanel
            resources={Array.from({ length: 20 }, (_, i) => repo(`acme/r${i}`))}
            onSubmit={onSubmit}
            onClose={vi.fn()}
          />
        </AppStoreContext.Provider>,
        { terminal }
      );
      await flush();
      // Only the derived window (3 rows), not all 20 repos, is rendered.
      const rendered = terminal.output;
      const rowCount = (rendered.match(/acme\/r\d+/g) ?? []).filter(
        (v, i, a) => a.indexOf(v) === i
      ).length;
      expect(rowCount).toBeLessThanOrEqual(4);
      expect(rowCount).toBeGreaterThan(0);
    } finally {
      mockTermSize.height = 24;
    }
  });
});
