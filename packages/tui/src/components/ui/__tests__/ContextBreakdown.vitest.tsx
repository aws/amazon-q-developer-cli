import { afterEach, describe, expect, test, vi } from 'vitest';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';

vi.mock('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ width: 100, height: 40 }),
}));

vi.mock('../../../hooks/useKeybindings.js', () => ({
  useKeybindings: () => ({
    matches: () => false,
    label: () => 'esc',
  }),
}));

import { ContextBreakdown } from '../ContextBreakdown.js';
import type { ContextBreakdownData } from '../../../types/context.js';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return 100;
  }
  get rows() {
    return 40;
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
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 20));
}

const breakdown: ContextBreakdownData = {
  contextFiles: { percent: 1, tokens: 2720, items: [] },
  tools: {
    percent: 4,
    tokens: 10880,
    groups: [
      {
        name: 'Built-in',
        source: 'built-in',
        percent: 1,
        tokens: 2720,
        items: [{ name: 'read', percent: 1, tokens: 2720 }],
      },
      {
        name: 'builder-mcp',
        source: 'mcp:builder-mcp',
        percent: 3,
        tokens: 8160,
        items: [
          { name: 'SkillsTool', percent: 2, tokens: 5440 },
          { name: 'WorkspaceSearch', percent: 1, tokens: 2720 },
        ],
      },
    ],
  },
  kiroResponses: { percent: 2, tokens: 5440 },
  yourPrompts: { percent: 1, tokens: 2720 },
  sessionFiles: { percent: 0, tokens: 0, items: [] },
};

describe('ContextBreakdown tool diagnostics', () => {
  test('renders tool sources and individual tools when expanded', async () => {
    const terminal = new MockTerminal();
    activeInstance = render(
      <ContextBreakdown
        percent={8}
        breakdown={breakdown}
        model="gpt-5.6-sol"
        agentName="kiro_default"
        initialExpanded
        onClose={() => {}}
      />,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    expect(output).toContain('Built-in');
    expect(output).toContain('builder-mcp');
    expect(output).toContain('read');
    expect(output).toContain('SkillsTool');
    expect(output).toContain('WorkspaceSearch');
    expect(output).toContain('2 tools');
  });
});
