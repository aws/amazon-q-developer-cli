/**
 * McpPanel Source column gating (cloud config UX).
 *
 * Darkship local-control proof: with cloud_config OFF, the /mcp panel output
 * must be identical to main — no Source column, no cloud empty state — even
 * when servers carry a `source` field or the session is cloud. With the flag
 * ON, the Source column renders and the cloud empty-state copy appears.
 */

import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '../../../hooks/useTerminalSize.js',
]);

const mockTermSize = { width: 80, height: 24 };
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));

import { McpPanel } from '../McpPanel.js';
import {
  AppStoreContext,
  createAppStore,
  type McpServerInfo,
} from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { features } from '../../../features.js';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
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

const ORIGINAL_FEATURES = process.env.KIRO_ENABLED_FEATURES;

function setFeatures(json: string | undefined) {
  if (json === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = json;
  features._resetForTests();
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
  setFeatures(ORIGINAL_FEATURES);
});

async function flush(): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

function mountPanel(servers: McpServerInfo[], cloudSessionActive = false) {
  const terminal = new MockTerminal();
  const onClose = vi.fn();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <McpPanel
        servers={servers}
        mode="status"
        onClose={onClose}
        cloudSessionActive={cloudSessionActive}
        cloudSnapshotReadiness="received"
      />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return { terminal };
}

const SERVERS: McpServerInfo[] = [
  { name: 'github', status: 'running', toolCount: 26, source: 'local' },
  { name: 'aws-docs', status: 'disabled', toolCount: 7, source: 'cloud' },
];

describe('McpPanel Source column gating', () => {
  test('flag OFF: no Source column even when servers carry sources', async () => {
    setFeatures(undefined);
    const { terminal } = mountPanel(SERVERS);
    await flush();
    expect(terminal.output).toContain('Name');
    expect(terminal.output).toContain('Status');
    expect(terminal.output).not.toContain('Source');
    // The frame-24 edit-path footer is also gated off-cohort.
    expect(terminal.output).not.toContain('To edit local configs');
  });

  test('flag ON: frame-24 footer shows edit paths; conflict line only on a mix', async () => {
    setFeatures('["cloud_config"]');
    const allLocal = mountPanel(
      SERVERS.map((s) => ({ ...s, source: 'local' as const }))
    );
    await flush();
    expect(allLocal.terminal.output).toContain('To edit local configs');
    expect(allLocal.terminal.output).not.toContain('In case of conflict');
    allLocal.terminal.write('');
    activeInstance?.unmount();
    activeInstance = null;

    const mixed = mountPanel(SERVERS); // github local + aws-docs cloud
    await flush();
    expect(mixed.terminal.output).toContain(
      'In case of conflict, local will override cloud configurations'
    );
    expect(mixed.terminal.output).toContain(
      'To edit cloud configs: https://app.kiro.dev/settings'
    );
  });

  test('flag OFF: cloud empty state still shows (owned by main, not the flag)', async () => {
    // Main's cloud-panel-notice empty state ships independently of
    // cloud_config; only the Source column and edit-path footer are gated.
    setFeatures(undefined);
    const { terminal } = mountPanel([], true);
    await flush();
    expect(terminal.output).toContain(
      'The cloud sandbox has no MCP servers configured'
    );
    expect(terminal.output).not.toContain('Source');
    expect(terminal.output).not.toContain('To edit local configs');
  });

  test('routed from /config, the panel titles itself /config — MCP', async () => {
    setFeatures('["cloud_config"]');
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    store.getState().setConfigReturnOnEscape(true);
    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <McpPanel servers={SERVERS} mode="status" onClose={vi.fn()} />
      </AppStoreContext.Provider>,
      { terminal }
    );
    await flush();
    expect(terminal.output).toContain('/config — MCP');
    expect(terminal.output).not.toContain('/mcp ·');
  });

  test('flag ON: Source column renders with per-server values', async () => {
    setFeatures('["cloud_config"]');
    const { terminal } = mountPanel(SERVERS);
    await flush();
    expect(terminal.output).toContain('Source');
    expect(terminal.output).toContain('local');
    expect(terminal.output).toContain('cloud');
  });

  test('flag ON: sourceless servers still render without the column', async () => {
    setFeatures('["cloud_config"]');
    const { terminal } = mountPanel(
      SERVERS.map((s) => ({ ...s, source: undefined }))
    );
    await flush();
    expect(terminal.output).not.toContain('Source');
  });

  test('cloud session with no servers shows the sandbox empty state', async () => {
    // The cloud empty-state copy is owned by main's shared
    // cloud-panel-notice module (supersedes the earlier frame-28 hardcode).
    setFeatures('["cloud_config"]');
    const { terminal } = mountPanel([], true);
    await flush();
    expect(terminal.output).toContain(
      'The cloud sandbox has no MCP servers configured'
    );
  });
});
