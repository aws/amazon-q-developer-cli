/**
 * WorkflowMessageComposer rendering and paste behavior.
 *
 * Verifies the composer renders correctly with wrap="wrap" so long text
 * and multiline pastes are displayed without truncation.
 */

import {
  afterAll,
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
} from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { inspectCursorCell } from '../../../test-utils/cursor-cell.js';
import { WorkflowMessageComposer } from './WorkflowMessageComposer.js';

class MockTerminal implements Terminal {
  public output = '';

  constructor(
    private readonly width = 80,
    private readonly height = 24
  ) {}

  get columns() {
    return this.width;
  }
  get rows() {
    return this.height;
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

// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1b\[[0-9;]*[A-Za-z]|\x1b\][^\x07]*\x07/g;
function stripAnsi(str: string): string {
  return str.replace(ANSI_RE, '');
}

let activeInstance: Instance | null = null;

const CURSOR_ENV = ['TMUX', 'ZELLIJ', 'TWINKI_HARDWARE_CURSOR'] as const;
const ambient = CURSOR_ENV.map((key) => [key, process.env[key]] as const);

beforeEach(() => {
  for (const key of CURSOR_ENV) delete process.env[key];
});

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

afterAll(() => {
  for (const [key, value] of ambient) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 30));
  await Promise.resolve();
}

describe('WorkflowMessageComposer cursor', () => {
  it('paints an inverse cursor cell when the terminal draws no cursor', async () => {
    const terminal = new MockTerminal();
    activeInstance = render(
      <WorkflowMessageComposer
        mode="steer"
        targetLabel="coder"
        value="hello"
        width={40}
      />,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const cell = await inspectCursorCell(terminal.output);
    expect(cell.inverse).toBe(true);
  });

  it('yields the cursor cell to the terminal under a multiplexer', async () => {
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    const terminal = new MockTerminal();
    activeInstance = render(
      <WorkflowMessageComposer
        mode="steer"
        targetLabel="coder"
        value="hello"
        width={40}
      />,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const cell = await inspectCursorCell(terminal.output);
    expect(cell.inverse).toBe(false);
  });
});

describe('WorkflowMessageComposer wrap behavior', () => {
  it('renders the mode label and hints', async () => {
    const terminal = new MockTerminal();
    activeInstance = render(
      <WorkflowMessageComposer
        mode="steer"
        targetLabel="coder"
        value=""
        width={40}
      />,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    expect(output).toContain('Steer');
    expect(output).toContain('coder');
    expect(output).toContain('send');
    expect(output).toContain('esc close');
  });

  it('wraps long text instead of truncating', async () => {
    const terminal = new MockTerminal(40, 10);
    const longText =
      'This is a message that should wrap to the next line in the composer';
    activeInstance = render(
      <WorkflowMessageComposer
        mode="steer"
        targetLabel="coder"
        value={longText}
        width={40}
      />,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    // All content should be present (not truncated)
    expect(output).toContain('This is a message');
    expect(output).toContain('composer');
  });

  it('renders multiline content from paste', async () => {
    const terminal = new MockTerminal(60, 10);
    const multiline = 'line one\nline two\nline three';
    activeInstance = render(
      <WorkflowMessageComposer
        mode="steer"
        targetLabel="coder"
        value={multiline}
        width={60}
      />,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    expect(output).toContain('line one');
    expect(output).toContain('line two');
    expect(output).toContain('line three');
  });
});
