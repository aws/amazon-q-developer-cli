import { afterEach, describe, expect, it } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import type { WorkflowLifecycleStatus } from '../../types/workflow-lifecycle.js';
import { WorkflowLifecycleRow } from './WorkflowLifecycleRow.js';

class MockTerminal implements Terminal {
  output = '';

  get columns() {
    return 100;
  }

  get rows() {
    return 12;
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

async function renderLifecycle(status: WorkflowLifecycleStatus) {
  const terminal = new MockTerminal();
  activeInstance = render(
    <WorkflowLifecycleRow workflowName="release-hardening" status={status} />,
    { terminal, exitOnCtrlC: false }
  );
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 20));
  return stripAnsi(terminal.output);
}

describe('WorkflowLifecycleRow', () => {
  const cases: Array<[WorkflowLifecycleStatus, string]> = [
    ['started', 'started'],
    ['completed', 'completed'],
    ['failed', 'failed'],
    ['aborted', 'aborted'],
  ];

  for (const [status, label] of cases) {
    it(`renders the ${status} workflow system row`, async () => {
      const output = await renderLifecycle(status);

      expect(output).toContain(`● Workflow ${label} "release-hardening"`);
      expect(output).not.toContain('──');
    });
  }
});
