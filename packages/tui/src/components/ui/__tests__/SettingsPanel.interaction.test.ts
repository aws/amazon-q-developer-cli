/**
 * /settings → features, driven through the real Explorer.
 *
 * The workflows preference is the first input to the KAS command list that
 * can change while the process runs, so this pins WHEN the change lands: the
 * toggle persists and repaints its own row, and nothing else moves until the
 * next session boundary — where the agent is told, the workflow extension
 * starts or is disposed, and command filtering is resolved together. A list
 * that moved on the keypress would advertise commands whose control plane is
 * still gated off.
 */
import { afterEach, beforeEach, describe, expect, it, mock } from 'bun:test';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { Settings } from '../../../constants/settings.js';
import { Feature, features } from '../../../features.js';
import { SettingsPanel } from '../SettingsPanel.js';

const DOWN = '\x1b[B';
const ENTER = '\r';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  get columns() {
    return 160;
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
  write(): void {}
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

let active: Instance | null = null;
let tempHome: string;
let originalHome: string | undefined;
let originalFeatures: string | undefined;
let originalRollout: string | undefined;

async function flush(): Promise<void> {
  for (let i = 0; i < 4; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

function readSettings(): Record<string, unknown> {
  return JSON.parse(
    readFileSync(join(tempHome, 'settings', 'cli.json'), 'utf-8')
  );
}

beforeEach(() => {
  originalHome = process.env.KIRO_HOME;
  originalFeatures = process.env.KIRO_ENABLED_FEATURES;
  originalRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;
  tempHome = mkdtempSync(join(tmpdir(), 'kiro-settings-panel-test-'));
  process.env.KIRO_HOME = tempHome;
  // The rollout reaches this user, so the Features row exists and the
  // workflow commands are filtered on the opt-in alone.
  process.env.KIRO_ENABLED_FEATURES = JSON.stringify([Feature.Workflows]);
  delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  features._resetForTests();
  mkdirSync(join(tempHome, 'settings'), { recursive: true });
  writeFileSync(
    join(tempHome, 'settings', 'cli.json'),
    JSON.stringify({ [Settings.CHAT_UI_MODE]: 'tui' })
  );
});

afterEach(() => {
  active?.unmount();
  active = null;
  if (originalHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalHome;
  if (originalFeatures === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = originalFeatures;
  if (originalRollout === undefined) {
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  } else {
    process.env.KIRO_LITE_ROLLOUT_ENABLED = originalRollout;
  }
  features._resetForTests();
  rmSync(tempHome, { recursive: true, force: true });
});

describe('SettingsPanel features toggle', () => {
  it('persists the opt-in and leaves the KAS command list to the next session', async () => {
    const kiro = new Kiro();
    const store = createAppStore({ kiro, agentEngine: 'kas', uiMode: 'tui' });
    const commandsBefore = store
      .getState()
      .kasCommands.map((command) => command.name);
    // Opted out at launch, so the list starts without the workflow commands —
    // otherwise the assertion below could not tell "unchanged" from "correct".
    expect(commandsBefore).not.toContain('/workflow');
    const terminal = new MockTerminal();

    active = render(
      React.createElement(
        AppStoreContext.Provider,
        { value: store },
        React.createElement(SettingsPanel, { onClose: mock(() => {}) })
      ),
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    // Features is the last top row and arrow keys clamp, so overshooting
    // lands on it without depending on how many rows precede it.
    for (let i = 0; i < 10; i += 1) terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    terminal.sendInput(ENTER);
    await flush();

    expect(readSettings()[Settings.CHAT_ENABLE_WORKFLOWS]).toBe(true);
    expect(store.getState().kasCommands.map((command) => command.name)).toEqual(
      commandsBefore
    );
  });

  // Every consumer of this preference is KAS-only, so on V2 the row would
  // toggle, report success, and change nothing — now or at any later session.
  // The rollout is on for both engines here; only the engine differs.
  it.each<['kas' | 'v2', boolean]>([
    ['kas', true],
    ['v2', false],
  ])('offers the Features row on %s: %p', async (agentEngine, expected) => {
    const store = createAppStore({
      kiro: new Kiro(),
      agentEngine,
      uiMode: 'tui',
    });
    const terminal = new MockTerminal();
    const frames: string[] = [];
    terminal.write = ((data: string) => frames.push(data)) as never;

    active = render(
      React.createElement(
        AppStoreContext.Provider,
        { value: store },
        React.createElement(SettingsPanel, { onClose: mock(() => {}) })
      ),
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    expect(frames.join('').includes('Features')).toBe(expected);
  });
});
