import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import React from 'react';
import { chalk } from '../../../utils/color.js';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 80, height: 24 };
const subscribeToMockTermSize = () => () => {};
const getMockTermSize = () => mockTermSize;
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '../../../hooks/useTerminalSize.js',
]);

mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () =>
    React.useSyncExternalStore(subscribeToMockTermSize, getMockTermSize),
}));

import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { RepoPickerPanel } from '../RepoPickerPanel.js';
import type { SourceProviderResource } from '@kiro/acp-type-covenant';

const DOWN = '\x1b[B';
const UP = '\x1b[A';
const ESC = '\x1b';
const TAB = '\t';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  /** Keystrokes sent before the renderer attaches its listener. Without this
   *  buffer the first sendInput of a test can race render() and be silently
   *  dropped (source of intermittent first-test failures). */
  private pending: string[] = [];
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
    for (const data of this.pending.splice(0)) onInput(data);
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
    if (this.onInput) this.onInput(data);
    else this.pending.push(data);
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

/** Wait until the panel has painted at least once before sending keys.
 *  The first mount in a test file can outlast a fixed 20ms sleep (module
 *  warm-up), and keys sent before useInput attaches are dropped — the
 *  historical source of intermittent first-test failures. */
async function waitForPaint(terminal: MockTerminal): Promise<void> {
  const deadline = Date.now() + 2000;
  while (!terminal.output.includes('/repo') && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await flush();
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
    await waitForPaint(terminal);
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
    await waitForPaint(terminal);
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
    await waitForPaint(terminal);
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
    await waitForPaint(terminal);
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
    await waitForPaint(terminal);
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
    await waitForPaint(terminal);
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
    await waitForPaint(terminal);
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app']);
  });

  test('a pre-checked repo can be toggled off before saving', async () => {
    const { terminal, onSubmit } = mountPicker(
      [repo('acme/app')],
      ['acme/app']
    );
    await waitForPaint(terminal);
    terminal.sendInput(' '); // cursor on acme/app -> uncheck the seeded selection
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith([]);
  });

  test('tab focuses the Selected panel; arrows + space uncheck under its own cursor', async () => {
    const { terminal, onSubmit } = mountPicker([
      repo('acme/app'),
      repo('acme/lib'),
      repo('acme/tool'),
    ]);
    await waitForPaint(terminal);
    terminal.sendInput(' '); // select acme/app
    await flush();
    terminal.sendInput(DOWN);
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(' '); // select acme/tool -> Selected = [app, tool]
    await flush();
    terminal.sendInput(TAB); // focus Selected panel (cursor at app)
    await flush();
    terminal.sendInput(DOWN); // Selected cursor -> tool
    await flush();
    terminal.sendInput(' '); // uncheck tool from the Selected panel
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app']);
  });

  test('Selected-panel space unchecks a repo hidden by the current filter', async () => {
    const { terminal, onSubmit } = mountPicker([
      repo('acme/app'),
      repo('zeta/z'),
    ]);
    await waitForPaint(terminal);
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(' '); // select zeta/z
    await flush();
    for (const ch of 'acme') terminal.sendInput(ch); // filter zeta/z out of All
    await flush();
    terminal.sendInput(TAB); // Selected panel still lists it
    await flush();
    terminal.sendInput(' '); // uncheck it without clearing the filter
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith([]);
  });

  test('Selected cursor clamps at both edges and when the last row is removed', async () => {
    const { terminal, onSubmit } = mountPicker(
      [repo('acme/app'), repo('acme/lib'), repo('acme/tool')],
      ['acme/app', 'acme/lib']
    );
    await waitForPaint(terminal);
    terminal.sendInput(TAB);
    await flush();
    terminal.sendInput(UP); // already at top — must clamp, not go negative
    terminal.sendInput(DOWN);
    terminal.sendInput(DOWN); // at bottom — must clamp on acme/lib
    await flush();
    terminal.sendInput(' '); // uncheck the last row -> cursor clamps back to 0
    await flush();
    terminal.sendInput(' '); // uncheck acme/app -> panel empties, focus -> All
    await flush();
    terminal.sendInput(' '); // now toggles the All cursor row (acme/app again)
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app']);
  });

  test('tab with nothing selected is a no-op — All keeps arrow/space', async () => {
    const { terminal, onSubmit } = mountPicker([
      repo('acme/app'),
      repo('acme/lib'),
    ]);
    await waitForPaint(terminal);
    terminal.sendInput(TAB); // empty Selected panel — focus must stay on All
    await flush();
    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(' ');
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/lib']);
  });

  test('tab never leaks a \\t into the search query', async () => {
    const { terminal, onSubmit } = mountPicker([repo('acme/app')]);
    await waitForPaint(terminal);
    terminal.sendInput(TAB); // if \t reached search, the filter would go empty
    terminal.sendInput(TAB);
    await flush();
    terminal.sendInput(' '); // still toggles the (unfiltered) cursor row
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app']);
  });

  test('typing while the Selected panel is focused still edits the search', async () => {
    const { terminal, onSubmit } = mountPicker(
      [repo('acme/app'), repo('other/tool')],
      ['acme/app']
    );
    await waitForPaint(terminal);
    terminal.sendInput(TAB); // focus Selected
    await flush();
    for (const ch of 'tool') terminal.sendInput(ch); // goes to search, not lost
    await flush();
    terminal.sendInput(TAB); // back to All (filtered to other/tool)
    await flush();
    terminal.sendInput(' ');
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onSubmit).toHaveBeenCalledWith(['acme/app', 'other/tool']);
  });

  test('checked non-cursor rows render an accent checkmark in the All list', async () => {
    const prevLevel = chalk.level;
    chalk.level = 3; // force truecolor so the accent SGR codes are emitted
    try {
      // acme/lib is pre-checked but the cursor sits on acme/app, so the only
      // checkmarks anywhere in the frame belong to non-cursor rows. They must
      // still be accent-colored (kiroDark accent #ff00ff), matching Selected.
      const { terminal } = mountPicker(
        [repo('acme/app'), repo('acme/lib')],
        ['acme/lib']
      );
      await waitForPaint(terminal);
      const accented = [
        chalk.hex('#ff00ff')('✓'), // unicode glyph set
        chalk.hex('#ff00ff')('+'), // ascii fallback glyph set
      ];
      expect(accented.some((mark) => terminal.output.includes(mark))).toBe(
        true
      );
    } finally {
      chalk.level = prevLevel;
    }
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
