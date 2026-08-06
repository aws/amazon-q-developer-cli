import { afterEach, beforeEach, describe, expect, it, mock } from 'bun:test';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { ModeChangeSource } from '../../../types/generated/chat-cli.js';
import { Kiro } from '../../../kiro.js';
import { Settings } from '../../../constants/settings.js';
import { DisplaySettingsPanel } from '../DisplaySettingsPanel.js';

const RIGHT = '\x1b[C';
const ENTER = '\r';
const ESCAPE = '\x1b';

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
let originalRollout: string | undefined;

async function flush(): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

beforeEach(() => {
  originalHome = process.env.KIRO_HOME;
  originalRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;
  tempHome = mkdtempSync(join(tmpdir(), 'kiro-display-panel-test-'));
  process.env.KIRO_HOME = tempHome;
  process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
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
  if (originalRollout === undefined) {
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  } else {
    process.env.KIRO_LITE_ROLLOUT_ENABLED = originalRollout;
  }
  rmSync(tempHome, { recursive: true, force: true });
});

describe('DisplaySettingsPanel UI mode switch', () => {
  it('switches immediately, keeps the panel open, and records its source', async () => {
    const kiro = new Kiro();
    const setSetting = mock(async () => {});
    const sendUiModeChanged = mock(() => {});
    (kiro as any).setSetting = setSetting;
    (kiro as any).sendUiModeChanged = sendUiModeChanged;
    (kiro as any).sendUiModeDefaultChanged = mock(() => {});

    const store = createAppStore({ kiro, agentEngine: 'v2', uiMode: 'tui' });
    const setUiMode = mock(() => {});
    store.setState({ setUiMode });
    const onClose = mock(() => {});
    const terminal = new MockTerminal();

    active = render(
      React.createElement(
        AppStoreContext.Provider,
        { value: store },
        React.createElement(DisplaySettingsPanel, {
          surface: 'tui',
          onClose,
        })
      ),
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    terminal.sendInput(RIGHT);
    await flush();

    expect(setUiMode).toHaveBeenCalledWith('lite');
    expect(onClose).not.toHaveBeenCalled();
    expect(setSetting).toHaveBeenCalledWith(Settings.CHAT_UI_MODE, 'lite');
    expect(sendUiModeChanged).toHaveBeenCalledWith({
      from: 'tui',
      to: 'lite',
      source: ModeChangeSource.SettingsPanel,
      sessionId: undefined,
    });
  });

  it('does not record a live switch when only the persisted default changes', async () => {
    const kiro = new Kiro();
    const setSetting = mock(async () => {});
    const sendUiModeChanged = mock(() => {});
    (kiro as any).setSetting = setSetting;
    (kiro as any).sendUiModeChanged = sendUiModeChanged;
    (kiro as any).sendUiModeDefaultChanged = mock(() => {});

    const store = createAppStore({ kiro, agentEngine: 'v2', uiMode: 'lite' });
    const setUiMode = mock(() => {});
    store.setState({ setUiMode });
    const terminal = new MockTerminal();

    active = render(
      React.createElement(
        AppStoreContext.Provider,
        { value: store },
        React.createElement(DisplaySettingsPanel, {
          surface: 'lite',
          onClose: mock(() => {}),
        })
      ),
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    terminal.sendInput(RIGHT);
    await flush();

    expect(setSetting).toHaveBeenCalledWith(Settings.CHAT_UI_MODE, 'lite');
    expect(setUiMode).not.toHaveBeenCalled();
    expect(sendUiModeChanged).not.toHaveBeenCalled();
  });

  it('applies the highlighted UI mode and closes on Enter', async () => {
    const kiro = new Kiro();
    const setSetting = mock(async () => {});
    const sendUiModeChanged = mock(() => {});
    (kiro as any).setSetting = setSetting;
    (kiro as any).sendUiModeChanged = sendUiModeChanged;
    (kiro as any).sendUiModeDefaultChanged = mock(() => {});

    const store = createAppStore({ kiro, agentEngine: 'v2', uiMode: 'tui' });
    const setUiMode = mock(() => {});
    store.setState({ setUiMode });
    const onClose = mock(() => {});
    const onDismiss = mock(() => {});
    const terminal = new MockTerminal();

    active = render(
      React.createElement(
        AppStoreContext.Provider,
        { value: store },
        React.createElement(DisplaySettingsPanel, {
          surface: 'tui',
          onClose,
          onDismiss,
        })
      ),
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    terminal.sendInput(ENTER);
    await flush();

    expect(setUiMode).toHaveBeenCalledWith('lite');
    expect(setSetting).toHaveBeenCalledWith(Settings.CHAT_UI_MODE, 'lite');
    expect(sendUiModeChanged).toHaveBeenCalledWith({
      from: 'tui',
      to: 'lite',
      source: ModeChangeSource.SettingsPanel,
      sessionId: undefined,
    });
    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(onClose).not.toHaveBeenCalled();
  });

  it('closes without changing the UI mode on Escape', async () => {
    const kiro = new Kiro();
    const setSetting = mock(async () => {});
    const sendUiModeChanged = mock(() => {});
    (kiro as any).setSetting = setSetting;
    (kiro as any).sendUiModeChanged = sendUiModeChanged;
    (kiro as any).sendUiModeDefaultChanged = mock(() => {});

    const store = createAppStore({ kiro, agentEngine: 'v2', uiMode: 'tui' });
    const setUiMode = mock(() => {});
    store.setState({ setUiMode });
    const onClose = mock(() => {});
    const terminal = new MockTerminal();

    active = render(
      React.createElement(
        AppStoreContext.Provider,
        { value: store },
        React.createElement(DisplaySettingsPanel, {
          surface: 'tui',
          onClose,
        })
      ),
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    terminal.sendInput(ESCAPE);
    await flush();

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(setUiMode).not.toHaveBeenCalled();
    expect(setSetting).not.toHaveBeenCalled();
    expect(sendUiModeChanged).not.toHaveBeenCalled();
  });
});
