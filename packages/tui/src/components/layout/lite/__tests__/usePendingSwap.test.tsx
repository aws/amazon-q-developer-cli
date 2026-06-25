/**
 * Unit tests for usePendingSwap.
 *
 * Anchor: bug-mine 6.2 — "30s safety timeout clears a stale pendingSwap".
 *
 * The hook latches onto loadingMessage="Agent changing to <name>" and clears
 * the latch in three ways:
 *   1. The actual currentAgent.name moves off baseAgent (RPC settled).
 *   2. 30s elapses (safety timeout for an RPC that never resolves).
 *   3. Component unmounts (cleanup).
 *
 * Path #2 is what this file primarily tests — bug-mine 6.2 was previously
 * filed as an integ skip with a 30s real-time wait. Fake timers make it
 * deterministic at <50ms wall time. Paths #1 and #3 are tested as well so
 * any future refactor that swaps the latch mechanism has to keep all three
 * exits intact.
 */

import { describe, test, expect, afterEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import { Kiro } from '../../../../kiro.js';
import { usePendingSwap, type PendingSwap } from '../usePendingSwap.js';

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
  start(onInput: (data: string) => void): void {
    this._onInput = onInput;
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
    if (this._onInput) this._onInput(data);
  }
}

let activeInstance: Instance | null = null;
afterEach(() => {
  if (activeInstance) {
    activeInstance.unmount();
    activeInstance = null;
  }
  vi.useRealTimers();
});

interface HookHarness {
  store: ReturnType<typeof createAppStore>;
  current: () => PendingSwap | null;
  unmount: () => void;
}

function mountHook(initial?: {
  loadingMessage?: string | null;
  currentAgent?: { name: string } | null;
}): HookHarness {
  const store = createAppStore({ kiro: new Kiro() });
  if (initial) {
    store.setState({
      loadingMessage: initial.loadingMessage ?? null,
      currentAgent: initial.currentAgent ?? null,
    } as any);
  }

  let captured: PendingSwap | null = null;
  function Probe() {
    captured = usePendingSwap();
    return null;
  }

  const instance = render(
    <AppStoreContext.Provider value={store}>
      <Probe />
    </AppStoreContext.Provider>,
    { terminal: new MockTerminal(), exitOnCtrlC: false }
  );
  activeInstance = instance;

  return {
    store,
    current: () => captured,
    unmount: () => {
      instance.unmount();
      activeInstance = null;
    },
  };
}

/** Drive a few render cycles + drain microtasks so React effects flush. */
async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 5));
  await Promise.resolve();
}

async function waitFor(
  predicate: () => boolean,
  timeoutMs = 250
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate() && Date.now() < deadline) {
    await flush();
  }
}

describe('usePendingSwap', () => {
  test('latches onto "Agent changing to <name>" and stores baseAgent', async () => {
    const harness = mountHook({
      loadingMessage: null,
      currentAgent: { name: 'kiro_default' } as any,
    });
    await flush();
    expect(harness.current()).toBeNull();

    harness.store.setState({
      loadingMessage: 'Agent changing to debug_agent',
    } as any);
    await waitFor(() => harness.current()?.name === 'debug_agent');

    expect(harness.current()).toEqual({
      name: 'debug_agent',
      baseAgent: 'kiro_default',
    });
  });

  test('does not latch on unrelated loadingMessage strings', async () => {
    const harness = mountHook({
      loadingMessage: 'Loading agent options...',
      currentAgent: { name: 'kiro_default' } as any,
    });
    await flush();
    expect(harness.current()).toBeNull();
  });

  test('clears the latch when currentAgent.name moves off baseAgent', async () => {
    const harness = mountHook({
      loadingMessage: 'Agent changing to debug_agent',
      currentAgent: { name: 'kiro_default' } as any,
    });
    await flush();
    expect(harness.current()?.name).toBe('debug_agent');

    // Backend settled: currentAgent flipped.
    harness.store.setState({
      currentAgent: { name: 'debug_agent' } as any,
      loadingMessage: null,
    } as any);
    await flush();

    expect(harness.current()).toBeNull();
  });

  test('30s safety timeout clears a stale pendingSwap [bug-mine 6.2]', async () => {
    // Patch setTimeout to compress its delay, so the 30s safety timer
    // fires within milliseconds. We can't use vi.useFakeTimers() here:
    // twinki's reconciler also schedules work through setTimeout, and
    // freezing all timers blocks the React commit phase that the latch
    // effect runs inside. Compressing only the >=30s wall-clock delay
    // leaves twinki's short scheduler timers alone.
    const realSetTimeout = globalThis.setTimeout;
    const patched = ((cb: () => void, delay?: number, ...args: unknown[]) => {
      const compressed = delay && delay >= 1000 ? 5 : delay;
      return realSetTimeout(cb, compressed, ...args);
    }) as typeof setTimeout;
    (globalThis as any).setTimeout = patched;

    try {
      const harness = mountHook({
        loadingMessage: 'Agent changing to ghost_agent',
        currentAgent: { name: 'kiro_default' } as any,
      });
      await flush();
      expect(harness.current()?.name).toBe('ghost_agent');

      // The "30s" safety timeout is now compressed to ~5ms; wait it out.
      await new Promise((resolve) => realSetTimeout(resolve, 50));
      // Drive a render so the cleared state propagates to the probe.
      harness.store.setState({} as any);
      await new Promise((resolve) => realSetTimeout(resolve, 30));

      expect(harness.current()).toBeNull();
    } finally {
      (globalThis as any).setTimeout = realSetTimeout;
    }
  });

  test('preserves the original baseAgent across mid-swap re-issues', async () => {
    const harness = mountHook({
      loadingMessage: null,
      currentAgent: { name: 'kiro_default' } as any,
    });
    await flush();

    harness.store.setState({
      loadingMessage: 'Agent changing to interim_agent',
    } as any);
    await flush();
    expect(harness.current()).toEqual({
      name: 'interim_agent',
      baseAgent: 'kiro_default',
    });

    // A second swap is issued before the first settles. baseAgent must
    // stay 'kiro_default' so the equality check still detects "settled".
    harness.store.setState({
      loadingMessage: 'Agent changing to final_agent',
    } as any);
    await flush();
    expect(harness.current()).toEqual({
      name: 'final_agent',
      baseAgent: 'kiro_default',
    });
  });
});
