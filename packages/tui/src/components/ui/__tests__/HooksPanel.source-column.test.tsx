/**
 * HooksPanel Source column gating (cloud config UX).
 *
 * Darkship local-control proof: with cloud_config OFF, the /hooks panel
 * output must be identical to main — no Source column — even when hooks
 * carry a descriptor-derived `configSource`. With the flag ON, the column
 * renders only when at least one hook actually carries a source (fact-based
 * gate), so a descriptor-free session is also unchanged.
 */

import { afterEach, describe, expect, test, vi } from 'vitest';
import { mock } from 'bun:test';
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 100, height: 24 };
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
restoreRealModulesAfterAll(import.meta.dir, [
  '../../../hooks/useTerminalSize.js',
]);
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));

import { HooksPanel } from '../HooksPanel.js';
import {
  AppStoreContext,
  createAppStore,
  type HookInfo,
} from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { features } from '../../../features.js';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return 100;
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

function mountPanel(hooks: HookInfo[]) {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  const element = (h: HookInfo[]) => (
    <AppStoreContext.Provider value={store}>
      <HooksPanel hooks={h} onClose={vi.fn()} />
    </AppStoreContext.Provider>
  );
  activeInstance = render(element(hooks), { terminal });
  const rerender = (h: HookInfo[]) => activeInstance!.rerender(element(h));
  return { terminal, rerender };
}

const TAGGED_HOOKS: HookInfo[] = [
  {
    name: 'team-guard',
    trigger: 'preToolUse',
    command: 'lint.sh',
    configSource: 'cloud',
  },
  { name: 'local-fmt', trigger: 'postToolUse', command: 'fmt.sh' },
];

describe('HooksPanel Source column gating', () => {
  test('flag OFF: no Source column even when hooks carry sources', async () => {
    setFeatures(undefined);
    const { terminal } = mountPanel(TAGGED_HOOKS);
    await flush();
    expect(terminal.output).toContain('Trigger');
    expect(terminal.output).not.toContain('Source');
  });

  test('flag ON but no descriptor on any hook: column stays hidden', async () => {
    setFeatures('["cloud_config"]');
    const { terminal } = mountPanel(
      TAGGED_HOOKS.map((h) => ({ ...h, configSource: undefined }))
    );
    await flush();
    expect(terminal.output).toContain('Trigger');
    expect(terminal.output).not.toContain('Source');
  });

  test('flag ON with a descriptor source: column renders per-hook values', async () => {
    setFeatures('["cloud_config"]');
    const { terminal } = mountPanel(TAGGED_HOOKS);
    await flush();
    expect(terminal.output).toContain('Source');
    expect(terminal.output).toContain('cloud');
  });

  test('gate is latched: a descriptor arriving mid-panel does not insert the column', async () => {
    setFeatures('["cloud_config"]');
    const untagged = TAGGED_HOOKS.map((h) => ({
      ...h,
      configSource: undefined,
    }));
    const { terminal, rerender } = mountPanel(untagged);
    await flush();
    expect(terminal.output).not.toContain('Source');
    // A hooks push carrying the first descriptor re-renders the open panel;
    // the mount-latched gate must hold — no column insertion mid-view.
    terminal.output = '';
    rerender(TAGGED_HOOKS);
    await flush();
    expect(terminal.output).not.toContain('Source');
  });
});
