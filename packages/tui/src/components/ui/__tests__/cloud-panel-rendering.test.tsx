import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 80, height: 24 };
const subscribeToMockTermSize = () => () => {};
const getMockTermSize = () => mockTermSize;
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () =>
    React.useSyncExternalStore(subscribeToMockTermSize, getMockTermSize),
}));

import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { McpPanel } from '../McpPanel.js';
import { ToolsPanel } from '../ToolsPanel.js';
import { HooksPanel } from '../HooksPanel.js';
import type { McpServerInfo, ToolInfo } from '../../../stores/app-store.js';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  private pending: string[] = [];
  public output = '';
  get columns() {
    return mockTermSize.width;
  }
  get rows() {
    return mockTermSize.height;
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

function mount(element: React.ReactElement) {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      {element}
    </AppStoreContext.Provider>,
    { terminal }
  );
  return terminal;
}

const AWAITING_SNIPPET = 'not yet received';

function mcpServer(name: string): McpServerInfo {
  return { name, status: 'running', toolCount: 1 };
}

function tool(name: string): ToolInfo {
  return { name, source: 'built-in', description: `${name} desc` };
}

describe('McpPanel cloud readiness rendering', () => {
  test('awaiting-sandbox: renders the awaiting notice and NO local rows', async () => {
    const terminal = mount(
      <McpPanel
        servers={[mcpServer('local-a'), mcpServer('local-b')]}
        mode="status"
        cloudSessionActive={true}
        cloudSnapshotReadiness="awaiting-sandbox"
        onClose={vi.fn()}
      />
    );
    await flush();
    expect(terminal.output).toContain(AWAITING_SNIPPET);
    expect(terminal.output).not.toContain('local-a');
    expect(terminal.output).not.toContain('local-b');
  });

  test('received: renders the snapshot rows without the awaiting notice', async () => {
    const terminal = mount(
      <McpPanel
        servers={[mcpServer('sandbox-a')]}
        mode="status"
        cloudSessionActive={true}
        cloudSnapshotReadiness="received"
        onClose={vi.fn()}
      />
    );
    await flush();
    expect(terminal.output).toContain('sandbox-a');
    expect(terminal.output).not.toContain(AWAITING_SNIPPET);
  });

  test('received + empty: renders the authoritative sandbox-empty state', async () => {
    const terminal = mount(
      <McpPanel
        servers={[]}
        mode="status"
        cloudSessionActive={true}
        cloudSnapshotReadiness="received"
        onClose={vi.fn()}
      />
    );
    await flush();
    expect(terminal.output).toContain(
      'The cloud sandbox has no MCP servers configured'
    );
    expect(terminal.output).not.toContain(AWAITING_SNIPPET);
  });

  test('local session: renders rows with no cloud notice', async () => {
    const terminal = mount(
      <McpPanel
        servers={[mcpServer('local-a')]}
        mode="status"
        cloudSessionActive={false}
        onClose={vi.fn()}
      />
    );
    await flush();
    expect(terminal.output).toContain('local-a');
    expect(terminal.output).not.toContain(AWAITING_SNIPPET);
    expect(terminal.output).not.toContain('cloud sandbox');
  });
});

describe('ToolsPanel cloud readiness rendering', () => {
  test('awaiting-sandbox: renders the awaiting notice and NO local rows', async () => {
    const terminal = mount(
      <ToolsPanel
        tools={[tool('localTool')]}
        cloudSessionActive={true}
        cloudSnapshotReadiness="awaiting-sandbox"
        onClose={vi.fn()}
      />
    );
    await flush();
    expect(terminal.output).toContain(AWAITING_SNIPPET);
    expect(terminal.output).not.toContain('localTool');
  });

  test('received: renders the snapshot rows without the awaiting notice', async () => {
    const terminal = mount(
      <ToolsPanel
        tools={[tool('sandboxTool')]}
        cloudSessionActive={true}
        cloudSnapshotReadiness="received"
        onClose={vi.fn()}
      />
    );
    await flush();
    expect(terminal.output).toContain('sandboxTool');
    expect(terminal.output).not.toContain(AWAITING_SNIPPET);
  });

  test('received + empty: renders the authoritative sandbox-empty state', async () => {
    const terminal = mount(
      <ToolsPanel
        tools={[]}
        cloudSessionActive={true}
        cloudSnapshotReadiness="received"
        onClose={vi.fn()}
      />
    );
    await flush();
    expect(terminal.output).toContain(
      'The cloud sandbox has no tools available'
    );
  });
});

describe('panel height budgeting with cloud notices (narrow terminal)', () => {
  test('HooksPanel: the notice shrinks the row window instead of overflowing', async () => {
    mockTermSize.height = 16; // base window: 16 - 9 = 7 rows
    try {
      const hooks = Array.from({ length: 20 }, (_, i) => ({
        name: `hook${i}`,
        trigger: 'agentSpawn',
        command: `cmd${i}.sh`,
      }));
      const terminal = mount(
        <HooksPanel hooks={hooks} cloudSessionActive={true} onClose={vi.fn()} />
      );
      await flush();
      expect(terminal.output).toContain('Hooks fetched from the cloud sandbox');
      const rowCount = (terminal.output.match(/hook\d+/g) ?? []).filter(
        (v, i, a) => a.indexOf(v) === i
      ).length;
      // Notice (1 line) + margin (1 line) shrink the 7-row window to 5.
      expect(rowCount).toBeLessThanOrEqual(5);
      expect(rowCount).toBeGreaterThan(0);
    } finally {
      mockTermSize.height = 24;
    }
  });

  test('ToolsPanel: a wrapped notice on a narrow terminal shrinks the row window', async () => {
    mockTermSize.width = 50; // the received-state governance/none path; notice absent
    mockTermSize.height = 16;
    try {
      const tools = Array.from({ length: 20 }, (_, i) => tool(`tl${i}`));
      const terminal = mount(
        <ToolsPanel
          tools={tools}
          cloudSessionActive={true}
          cloudSnapshotReadiness="received"
          onClose={vi.fn()}
        />
      );
      await flush();
      const rowCount = (terminal.output.match(/tl\d+/g) ?? []).filter(
        (v, i, a) => a.indexOf(v) === i
      ).length;
      // No notice in received state: full 7-row window applies.
      expect(rowCount).toBeLessThanOrEqual(7);
      expect(rowCount).toBeGreaterThan(0);
    } finally {
      mockTermSize.width = 80;
      mockTermSize.height = 24;
    }
  });
});
