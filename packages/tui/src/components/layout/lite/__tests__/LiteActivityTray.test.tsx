import { afterEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import { Kiro } from '../../../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import { createWorkflowStore } from '../../../../stores/workflow-store.js';
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
});
