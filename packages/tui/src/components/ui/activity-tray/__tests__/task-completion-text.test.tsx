/**
 * Completed tasks must say so in text.
 *
 * A finished row is set apart by a filled dot, a muted colour, and
 * strikethrough — none of which a screen reader announces, and the dot
 * disappears entirely when icons are off. Asserting against ANSI-stripped
 * output is what makes that visible: whatever survives the strip is what a
 * reader announces.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { Terminal as XtermTerminal } from '@xterm/headless';
import { render, type Instance, type Terminal } from 'twinki';
import { Kiro } from '../../../../kiro.js';
import { TASK_DONE_MARKER } from '../../../../constants/tasks.js';
import { setTerminalSizeForTests } from '../../../../hooks/useTerminalSize.js';
import { inspectCell } from '../../../../test-utils/cursor-cell.js';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import type { TaskItem } from '../../../../types/tasks.js';
import { ActivityTrayExpanded } from '../ActivityTrayExpanded.js';

class MockTerminal implements Terminal {
  output = '';

  get columns() {
    return 120;
  }

  get rows() {
    return 24;
  }

  get kittyProtocolActive() {
    return true;
  }

  start(): void {}
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
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 30));
  await Promise.resolve();
}

async function renderFrame(tasks: TaskItem[]): Promise<string> {
  const appStore = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  appStore.setState({ tasks, activityTrayExpanded: true });

  const terminal = new MockTerminal();
  // Row width reads the terminal size from this module, not from the mock, so
  // pin it or the suite's ambient width decides where subjects break.
  setTerminalSizeForTests(terminal.columns, terminal.rows);
  activeInstance = render(
    <AppStoreContext.Provider value={appStore}>
      <ActivityTrayExpanded
        activeTab="tasks"
        hasTasks
        inputOwnership={{
          rowNavigationActive: false,
          tabNavigationActive: false,
          workflowNavigationActive: false,
        }}
        navigationActive={false}
        tabs={['tasks']}
      />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false, patchConsole: false }
  );
  await flush();
  return terminal.output;
}

async function renderTasks(tasks: TaskItem[]): Promise<string> {
  return stripAnsi(await renderFrame(tasks));
}

/** The frame's non-blank screen rows, as the terminal laid them out. */
async function screenRows(frame: string): Promise<string[]> {
  const terminal = new XtermTerminal({
    cols: 120,
    rows: 24,
    allowProposedApi: true,
  });
  try {
    await new Promise<void>((resolve) => terminal.write(frame, resolve));
    const buffer = terminal.buffer.active;
    const rows: string[] = [];
    for (let row = 0; row < buffer.baseY + 24; row++) {
      const text = buffer.getLine(row)?.translateToString(true) ?? '';
      if (text.trim()) rows.push(text);
    }
    return rows;
  } finally {
    terminal.dispose();
  }
}

describe('ActivityTrayExpanded task completion', () => {
  // The marker stops short of the strikethrough: struck through, the word reads
  // as retracted rather than as the state it reports.
  it('marks a completed task as done without striking the marker', async () => {
    const frame = await renderFrame([
      { id: '1', subject: 'Read the config', status: 'completed' },
    ]);

    expect(stripAnsi(frame)).toContain('Read the config [done]');
    expect((await inspectCell(frame, 'R'))?.strikethrough).toBe(true);
    expect(await inspectCell(frame, '[')).toMatchObject({
      char: '[',
      strikethrough: false,
    });
  });

  it('leaves a pending task unmarked', async () => {
    const output = await renderTasks([
      { id: '1', subject: 'Wire up the parser', status: 'pending' },
    ]);

    expect(output).toContain('Wire up the parser');
    expect(output).not.toContain('[done]');
  });

  it('keeps the marker on the row the subject ends on', async () => {
    // A subject that fills the row exactly is where the marker used to be
    // floated onto a row of its own.
    const frame = await renderFrame([
      { id: '1', subject: 'q'.repeat(104), status: 'completed' },
    ]);
    const rows = (await screenRows(frame)).filter((row) => row.includes('q'));

    expect(rows.length).toBeGreaterThan(0);
    expect(rows.filter((row) => row.includes(TASK_DONE_MARKER))).toHaveLength(
      1
    );
    expect(rows[rows.length - 1] ?? '').toContain(TASK_DONE_MARKER);
  });
});
