import React from 'react';
import { afterEach, describe, expect, test } from 'vitest';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import { ToolUseStatus } from '../../../stores/app-store.js';
import { WorkflowTool } from './WorkflowTool.js';

class MockTerminal implements Terminal {
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

async function renderWorkflowTool(
  props: React.ComponentProps<typeof WorkflowTool>
): Promise<string> {
  const terminal = new MockTerminal();
  activeInstance = render(<WorkflowTool {...props} />, {
    terminal,
    exitOnCtrlC: false,
  });
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 20));
  return stripAnsi(terminal.output);
}

describe('WorkflowTool', () => {
  test('renders a running workflow with its recursive step count', async () => {
    const output = await renderWorkflowTool({
      name: 'run_workflow',
      content: JSON.stringify({
        workflow: {
          name: 'release',
          steps: [
            { type: 'step', name: 'build' },
            {
              type: 'parallel',
              branches: [
                { type: 'step', name: 'test' },
                { type: 'step', name: 'review' },
              ],
            },
          ],
        },
      }),
    });

    expect(output).toContain('Starting workflow');
    expect(output).toContain('"release" 3 steps');
  });

  test('renders a rejected workflow as failed rather than started', async () => {
    const output = await renderWorkflowTool({
      name: 'run_workflow',
      content: JSON.stringify({ name: 'over-limit' }),
      isFinished: true,
      result: {
        status: 'error',
        error: 'Workflow exceeds the maximum of 20 step nodes',
      },
    });

    expect(output).toContain('Workflow failed to start');
    expect(output).toContain('maximum of 20 step nodes');
    expect(output).not.toContain('Started workflow');
  });

  test('renders a successful launch with the monitor shortcut', async () => {
    const output = await renderWorkflowTool({
      name: 'Run Workflow',
      content: JSON.stringify({ name: 'release' }),
      isFinished: true,
      result: {
        status: 'success',
        output: JSON.stringify({ workflowId: 'wf-ok' }),
      },
    });

    expect(output).toContain('Started workflow');
    expect(output).toContain('ctrl+g monitor');
  });

  test('renders workflow cancellation explicitly', async () => {
    const output = await renderWorkflowTool({
      name: 'run_workflow',
      content: JSON.stringify({ name: 'release' }),
      isFinished: true,
      status: ToolUseStatus.Rejected,
    });

    expect(output).toContain('Workflow start cancelled');
    expect(output).not.toContain('Started workflow');
  });

  test('renders inspect workflow failures', async () => {
    const output = await renderWorkflowTool({
      name: 'inspect_workflow',
      content: JSON.stringify({ workflowId: 'wf-missing' }),
      isFinished: true,
      result: { status: 'error', error: 'Workflow was not found' },
    });

    expect(output).toContain('Workflow inspection failed');
    expect(output).toContain('Workflow was not found');
  });
});
