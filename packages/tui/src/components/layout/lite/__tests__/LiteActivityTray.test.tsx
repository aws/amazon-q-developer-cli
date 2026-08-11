import { afterEach, describe, expect, it, mock } from 'bun:test';

// Row wrapping reads the terminal size through this hook, and a sibling suite
// mocks the same module process-wide, so pin it here or the ambient value
// decides where subjects break.
const DEFAULT_TERM_WIDTH = 120;
const TERM_SIZE = { width: DEFAULT_TERM_WIDTH, height: 16 };
mock.module('../../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...TERM_SIZE }),
  setTerminalSizeForTests: () => {},
}));
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import { Kiro } from '../../../../kiro.js';
import { inspectCell } from '../../../../test-utils/cursor-cell.js';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import { createWorkflowStore } from '../../../../stores/workflow-store.js';
import type { TaskItem } from '../../../../types/tasks.js';
import type { WorkflowRunView } from '../../../../types/workflow-monitor.js';
import { LiteActivityTray } from '../LiteActivityTray.js';

class MockTerminal implements Terminal {
  output = '';

  get columns() {
    return 120;
  }

  get rows() {
    return 16;
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
  TERM_SIZE.width = DEFAULT_TERM_WIDTH;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 30));
  await Promise.resolve();
}

function runningWorkflow(): WorkflowRunView {
  return {
    workflowId: 'workflow-1',
    parentSessionId: 'session-1',
    name: 'release-hardening',
    status: 'running',
    nodes: [
      {
        id: 'completed-step',
        type: 'step',
        status: 'completed',
        label: 'Completed step',
        parentId: null,
        depth: 0,
      },
      {
        id: 'running-step',
        type: 'step',
        status: 'running',
        label: 'Running step',
        parentId: null,
        depth: 0,
      },
    ],
    stepSessions: [],
    startedAt: Date.now(),
    completedAt: null,
  };
}

async function renderTray(expanded: boolean): Promise<string> {
  const appStore = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'kas',
  });
  appStore.setState({ activityTrayExpanded: expanded });

  const workflowStore = createWorkflowStore();
  workflowStore.getState().openHistoricalWorkflow(runningWorkflow());

  const terminal = new MockTerminal();
  activeInstance = render(
    <AppStoreContext.Provider value={appStore}>
      <LiteActivityTray store={workflowStore} />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false, patchConsole: false }
  );
  await flush();
  return stripAnsi(terminal.output);
}

async function renderFrame(tasks: TaskItem[]): Promise<string> {
  const appStore = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'kas',
  });
  appStore.setState({ tasks, activityTrayExpanded: true });

  const terminal = new MockTerminal();
  activeInstance = render(
    <AppStoreContext.Provider value={appStore}>
      <LiteActivityTray store={createWorkflowStore()} />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false, patchConsole: false }
  );
  await flush();
  return terminal.output;
}

async function renderTasks(tasks: TaskItem[]): Promise<string> {
  return stripAnsi(await renderFrame(tasks));
}

describe('LiteActivityTray', () => {
  it('shows aggregate workflow step progress when collapsed', async () => {
    const output = await renderTray(false);

    expect(output).toContain('1 workflow running');
    expect(output).toContain('steps 1/2');
    expect(output).toContain('ctrl+x expand');
  });

  it('shows per-workflow step progress when expanded', async () => {
    const output = await renderTray(true);

    expect(output).toContain('release-hardening');
    expect(output).toContain('running');
    expect(output).toContain('steps 1/2');
    expect(output).toContain('ctrl+g monitor');
  });

  // Strikethrough and colour are the only other completion cues, and a screen
  // reader announces neither, so the assertions run on stripped output. The
  // marker itself stays out of the strikethrough: struck through, the word
  // reads as retracted rather than as the state it reports.
  it('marks a completed task as done without striking the marker', async () => {
    const frame = await renderFrame([
      { id: '1', subject: 'Wire up the parser', status: 'completed' },
    ]);

    expect(stripAnsi(frame)).toContain('Wire up the parser [done]');
    expect((await inspectCell(frame, 'W'))?.strikethrough).toBe(true);
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

  it('marks a wrapped completed task on its last line', async () => {
    const output = await renderTasks([
      {
        id: '1',
        subject: `${'wrap '.repeat(30)}zzsentinel`,
        status: 'completed',
      },
    ]);
    const rows = output.split('\n').filter((row) => row.includes('wrap'));
    const lastRow = rows.find((row) => row.includes('zzsentinel'));

    expect(rows.length).toBeGreaterThan(1);
    expect(lastRow).toBe(rows.at(-1));
    expect(lastRow).toContain('[done]');
    expect(rows.slice(0, -1).join('')).not.toContain('[done]');
  });

  it('keeps a marked row inside the pane width', async () => {
    // Ten-column words pack the wrap budget exactly, so the last row runs to
    // the edge and the marker has nowhere to go unless its width was reserved.
    const output = await renderTasks([
      {
        id: '1',
        subject: 'abcdefghij '.repeat(20).trim(),
        status: 'completed',
      },
    ]);
    const rows = output
      .split('\n')
      .filter((row) => row.includes('abcdefghij'))
      .map((row) => row.replace(/\r$/, ''));

    expect(rows.length).toBeGreaterThan(1);
    for (const row of rows) {
      expect(row.length).toBeLessThanOrEqual(TERM_SIZE.width);
    }
  });

  it('keeps a marked row inside a narrow pane', async () => {
    // Narrow enough that the minimum-subject-width floor decides the budget.
    // Taken from inside the floor, the marker widens a completed row past the
    // pane while a pending one still fits. The word count is a multiple of the
    // three that fit a row, so the marker lands on a row already at the edge.
    TERM_SIZE.width = 33;
    const output = await renderTasks([
      { id: '1', subject: 'abcde '.repeat(9).trim(), status: 'completed' },
    ]);
    const rows = output
      .split('\n')
      .filter((row) => row.includes('abcde'))
      .map((row) => row.replace(/\r$/, ''));

    expect(rows.length).toBeGreaterThan(1);
    for (const row of rows) {
      expect(row.length).toBeLessThanOrEqual(33);
    }
  });
});
