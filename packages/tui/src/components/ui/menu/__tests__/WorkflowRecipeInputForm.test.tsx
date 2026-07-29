import { afterEach, describe, expect, it, mock } from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { WorkflowRecipeInputForm } from '../WorkflowRecipeInputForm.js';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  output = '';
  columns = 80;
  rows = 24;
  kittyProtocolActive = true;

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

let instance: Instance | null = null;

afterEach(() => {
  instance?.unmount();
  instance = null;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 50));
  await Promise.resolve();
}

function mountForm(
  initialValues: Record<string, string> = {},
  inputs: Record<string, string> = { target: 'prompt', branch: 'string' }
) {
  const terminal = new MockTerminal();
  const onSubmit = mock((_values: Record<string, string>) => {});
  const onCancel = mock(() => {});
  instance = render(
    <WorkflowRecipeInputForm
      recipe={{
        name: 'release',
        description: 'Build and validate a release',
        inputs,
      }}
      initialValues={initialValues}
      onSubmit={onSubmit}
      onCancel={onCancel}
    />,
    { terminal, exitOnCtrlC: false }
  );
  return { terminal, onSubmit, onCancel };
}

describe('WorkflowRecipeInputForm', () => {
  it('offers an explicit launch action when there are no inputs', async () => {
    const { terminal, onSubmit } = mountForm({}, {});
    await flush();

    expect(terminal.output).toContain('No inputs required.');
    expect(terminal.output).toContain('run');

    terminal.sendInput('\r');
    await flush();

    expect(onSubmit).toHaveBeenCalledWith({});
  });

  it('shows default hints and the action for the active field', async () => {
    const { terminal } = mountForm(
      {},
      { target: 'prompt', reportPath: 'file', branch: 'string' }
    );
    await flush();

    expect(terminal.output).toContain('Describe the task or goal');
    expect(terminal.output).toContain('Enter a file path');
    expect(terminal.output).toContain('Enter a value');
    expect(terminal.output).toContain('next');

    terminal.output = '';
    terminal.sendInput('\x1b[B');
    terminal.sendInput('\x1b[B');
    await flush();

    expect(terminal.output).toContain('run');
  });

  it('preserves initial values and submits every declared input', async () => {
    const { terminal, onSubmit } = mountForm({ branch: 'main' });
    await flush();

    terminal.sendInput('ship staging');
    terminal.sendInput('\r');
    terminal.sendInput('\r');
    await flush();

    expect(onSubmit).toHaveBeenCalledWith({
      target: 'ship staging',
      branch: 'main',
    });
  });

  it('blocks launch and focuses the first empty field', async () => {
    const { terminal, onSubmit } = mountForm({ branch: 'main' });
    await flush();

    terminal.sendInput('\x1b[B');
    terminal.sendInput('\r');
    await flush();

    expect(onSubmit).not.toHaveBeenCalled();
    expect(terminal.output).toContain('target is required');
  });

  it('cancels without submitting', async () => {
    const { terminal, onSubmit, onCancel } = mountForm();
    await flush();

    terminal.sendInput('\x1b');
    await flush();

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
