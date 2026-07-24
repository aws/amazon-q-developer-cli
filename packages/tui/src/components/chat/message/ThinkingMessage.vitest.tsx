/**
 * Behavioral test for the chat.showThinkingTips gate in ThinkingMessage.
 *
 * ThinkingMessage combines its `showTip` prop with the chat.showThinkingTips setting
 * (read once at mount) and passes the result as `enabled` to useThinkingTip.
 * This proves the setting gates the tip: with chat.showThinkingTips=false the tip is
 * disabled even when showTip=true. Reverting the gate (passing bare `showTip`)
 * makes the "not called" assertion below fail.
 *
 * Real timers, mirroring useThinkingTip.vitest.tsx: twinki renders through a
 * ConcurrentRoot react-reconciler whose passive effects flush via the React
 * scheduler, which vitest fake timers do not advance. Leaf hooks and StatusBar
 * are stubbed so ThinkingMessage renders without the full provider tree; the
 * gate and the real useThinkingTip hook run for real.
 */
import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';

const { pickTipMock, readBoolSettingMock } = vi.hoisted(() => ({
  pickTipMock: vi.fn(),
  readBoolSettingMock: vi.fn(),
}));

// Real useThinkingTip; mock the tip picker and the setting reader.
vi.mock('../../../tips/tips.js', () => ({ pickTip: pickTipMock }));
vi.mock('../../../utils/cli-settings.js', () => ({
  readBoolSetting: readBoolSettingMock,
}));

// Stub leaf hooks + StatusBar so ThinkingMessage renders without real providers.
vi.mock('../../../hooks/useThemeContext.js', () => ({
  useTheme: () => ({ getColor: () => (t: string) => t }),
}));
vi.mock('../../../hooks/useKeybindings.js', () => ({
  useKeybindings: () => ({ label: () => 'esc' }),
}));
vi.mock('../../../hooks/useGlyphs.js', () => ({
  useThinkingMode: () => ({ thinkingMode: 'collapsed' }),
  useGlyphs: () => ({ cornerBottomLeftRound: '+' }),
}));
vi.mock('../../../stores/app-store.js', () => ({
  useAppStore: (selector: (s: unknown) => unknown) =>
    selector({ retryStatus: null, agentEngine: 'kas' }),
}));
vi.mock('../status-bar/StatusBar.js', () => ({
  StatusBar: ({ children }: { children?: React.ReactNode }) => children,
  STATUS_BAR_CONTENT_OFFSET: 2,
}));

import { ThinkingMessage } from './ThinkingMessage.js';
import { TIP_SHOW_DELAY_MS } from './useThinkingTip.js';

const MOCK_TIP = 'Use /compact to free up context';

// Minimal headless terminal for twinki's render (mirrors useThinkingTip.vitest.tsx).
class MockTerminal implements Terminal {
  private _onInput: ((data: string) => void) | null = null;
  get columns() {
    return 80;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(onInput: (data: string) => void, _onResize: () => void): void {
    this._onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(_data: string): void {}
  moveBy(_lines: number): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(_title: string): void {}
  sendInput(data: string): void {
    if (this._onInput) {
      this._onInput(data);
    }
  }
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

let activeInstance: Instance | null = null;

function renderThinking(showTip: boolean): void {
  activeInstance = render(<ThinkingMessage showTip={showTip} />, {
    terminal: new MockTerminal(),
    exitOnCtrlC: false,
  });
}

describe('ThinkingMessage chat.showThinkingTips gate', () => {
  beforeEach(() => {
    pickTipMock.mockReset();
    pickTipMock.mockReturnValue(MOCK_TIP);
    readBoolSettingMock.mockReset();
  });

  afterEach(() => {
    if (activeInstance) {
      activeInstance.unmount();
      activeInstance = null;
    }
  });

  test(
    'chat.showThinkingTips=false disables the tip even when showTip is true',
    async () => {
      readBoolSettingMock.mockReturnValue(false);
      renderThinking(true);
      // Wait PAST the tip delay: with the gate active no timer is ever scheduled,
      // so pickTip stays uncalled. If the gate were reverted (bare showTip), the
      // timer would have fired by now and pickTip would be called, failing this.
      await sleep(TIP_SHOW_DELAY_MS + 500);
      expect(pickTipMock).not.toHaveBeenCalled();
    },
    TIP_SHOW_DELAY_MS + 3000
  );

  test(
    'chat.showThinkingTips=true (default) allows the tip when showTip is true',
    async () => {
      readBoolSettingMock.mockReturnValue(true);
      renderThinking(true);
      await sleep(TIP_SHOW_DELAY_MS + 500);
      expect(pickTipMock).toHaveBeenCalledTimes(1);
    },
    TIP_SHOW_DELAY_MS + 3000
  );
});
