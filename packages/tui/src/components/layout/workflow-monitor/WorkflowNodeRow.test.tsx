import { afterEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import type { WorkflowMonitorNode } from '../../../types/workflow-monitor.js';
import { WorkflowNodeRow } from './WorkflowNodeRow.js';

class MockTerminal implements Terminal {
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

async function paintRow(node: WorkflowMonitorNode): Promise<string> {
  const terminal = new MockTerminal();
  activeInstance = render(
    <WorkflowNodeRow node={node} isSelected={false} isLast width={80} />,
    { terminal, exitOnCtrlC: false }
  );
  await new Promise((resolve) => setTimeout(resolve, 30));
  return stripAnsi(terminal.output);
}

// A watch node that polls and is then parked holds both fields at once, so the
// two cases below pin which one the row is allowed to speak for.
const POLLED_WATCH: WorkflowMonitorNode = {
  id: 'signal',
  type: 'watch',
  status: 'paused',
  label: 'signal',
  parentId: null,
  depth: 0,
  watchOutcome: 'new-activity',
  pauseReason: 'Approve the deploy?',
};

describe('WorkflowNodeRow activity', () => {
  it('shows a parked watch node why the run stopped, not its last poll', async () => {
    const output = await paintRow(POLLED_WATCH);
    expect(output).toContain('[watch] signal');
    expect(output).toContain('Approve the deploy?');
    expect(output).not.toContain('new activity');
  });

  it('shows the latest poll again once a parked watch node restarts', async () => {
    const output = await paintRow({ ...POLLED_WATCH, status: 'running' });
    expect(output).toContain('new activity');
    expect(output).not.toContain('Approve the deploy?');
  });
});
