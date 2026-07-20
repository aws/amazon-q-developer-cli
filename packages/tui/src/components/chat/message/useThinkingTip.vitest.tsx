/**
 * Behavioral tests for the useThinkingTip hook.
 *
 * Runs under vitest (not bun:test) because the hook must be exercised through
 * twinki's react-reconciler render loop, mirroring hooks.vitest.tsx.
 *
 * Real timers, not fake timers: twinki renders through a ConcurrentRoot
 * react-reconciler whose passive effects (useEffect) flush via the React
 * scheduler (MessageChannel / real timers). vitest fake timers do not advance
 * that scheduler and twinki exposes no act(), so under fake timers the effect
 * never runs and the timer is never scheduled. Instead we inject a small delay
 * and await real time, exactly as hooks.vitest.tsx awaits a real flush.
 */
import { describe, test, expect, afterEach, beforeEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';

// Deterministic tip selection so we can assert what the hook returns and how
// often the picker runs. Hoisted so the vi.mock factory can reference it.
const { pickTipMock } = vi.hoisted(() => ({ pickTipMock: vi.fn() }));
vi.mock('../../../tips/tips.js', () => ({ pickTip: pickTipMock }));

import { useThinkingTip } from './useThinkingTip.js';

const MOCK_TIP = 'Use /compact to free up context';

// Small delay so the timer path resolves quickly under real timers.
const TEST_DELAY_MS = 10;
// Time to let the reconciler flush effects and any post-timer re-render.
// hooks.vitest.tsx uses 50ms for a single flush; we allow more to cover
// effect flush + timer fire + re-render flush.
const FLUSH_MS = 150;

const CTX = {
  surface: 'tui' as const,
  engine: 'kas' as const,
  recommendLiteUi: false,
};

// Minimal headless terminal for twinki's render (mirrors hooks.vitest.tsx).
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

/**
 * Mounts useThinkingTip and exposes the latest returned value via a mutable
 * capture object, so the test can inspect it after awaiting a real flush.
 */
function renderThinkingTip(
  enabled: boolean,
  delayMs: number
): { current: string | null } {
  const captured = { current: null as string | null };

  function TestComponent() {
    captured.current = useThinkingTip(CTX, enabled, delayMs);
    return null;
  }

  activeInstance = render(<TestComponent />, {
    terminal: new MockTerminal(),
    exitOnCtrlC: false,
  });

  return captured;
}

describe('useThinkingTip', () => {
  beforeEach(() => {
    pickTipMock.mockReset();
    pickTipMock.mockReturnValue(MOCK_TIP);
  });

  afterEach(() => {
    if (activeInstance) {
      activeInstance.unmount();
      activeInstance = null;
    }
  });

  test('returns null before the delay fires', async () => {
    // Delay far longer than the flush window, so it cannot fire in time.
    const captured = renderThinkingTip(true, 500);
    await sleep(FLUSH_MS);
    expect(captured.current).toBeNull();
    expect(pickTipMock).not.toHaveBeenCalled();
  });

  test('returns the picked tip after the delay fires', async () => {
    const captured = renderThinkingTip(true, TEST_DELAY_MS);
    await sleep(FLUSH_MS);
    expect(pickTipMock).toHaveBeenCalledTimes(1);
    expect(captured.current).toBe(MOCK_TIP);
  });

  test('when enabled is false: schedules no timer, never picks, returns null', async () => {
    const captured = renderThinkingTip(false, TEST_DELAY_MS);
    await sleep(FLUSH_MS);
    expect(pickTipMock).not.toHaveBeenCalled();
    expect(captured.current).toBeNull();
  });

  test('picks exactly one tip per mount (no rotation)', async () => {
    renderThinkingTip(true, TEST_DELAY_MS);
    await sleep(FLUSH_MS * 2);
    expect(pickTipMock).toHaveBeenCalledTimes(1);
  });

  // This asserts only that the hook forwards recommendLiteUi:false to pickTip
  // (pickTip is mocked here). The actual suppression, recommendLiteUi:false
  // means the Try-Lite tip never appears, is covered in src/tips/tips.test.ts.
  test('forwards recommendLiteUi:false to pickTip', async () => {
    renderThinkingTip(true, TEST_DELAY_MS);
    await sleep(FLUSH_MS);
    expect(pickTipMock).toHaveBeenCalledWith(
      expect.objectContaining({ recommendLiteUi: false })
    );
  });
});
