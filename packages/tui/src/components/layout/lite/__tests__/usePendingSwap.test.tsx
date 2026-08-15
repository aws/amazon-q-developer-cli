/**
 * Unit tests for usePendingSwap: the latch onto
 * loadingMessage="Agent changing to <name>" plus each of its exits — the real
 * agent name moving off baseAgent, loadingMessage clearing without an agent
 * change, and the safety timeout for an RPC that never resolves.
 */

import { describe, test, expect, afterEach } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import { Kiro } from '../../../../kiro.js';
import {
  usePendingSwap,
  parseSwapTarget,
  SAFETY_TIMEOUT_MS,
  type PendingSwap,
} from '../usePendingSwap.js';

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
});

interface HookHarness {
  store: ReturnType<typeof createAppStore>;
  current: () => PendingSwap | null;
  latched: () => readonly PendingSwap[];
  unmount: () => void;
}

function mountHook(initial?: {
  loadingMessage?: string | null;
  currentAgent?: { name: string } | null;
  safetyTimeoutMs?: number;
}): HookHarness {
  const store = createAppStore({ kiro: new Kiro() });
  if (initial) {
    store.setState({
      loadingMessage: initial.loadingMessage ?? null,
      currentAgent: initial.currentAgent ?? null,
    } as any);
  }

  const options = { safetyTimeoutMs: initial?.safetyTimeoutMs };
  let captured: PendingSwap | null = null;
  // Every non-null value ever rendered, in order. A point-in-time read of the
  // hook cannot tell "never latched" apart from "the latch effect has not run
  // yet", so a negative claim made against it can pass for the wrong reason;
  // the history only grows, so it stays truthful whatever the render timing.
  const latched: PendingSwap[] = [];
  function Probe() {
    captured = usePendingSwap(options);
    if (captured) latched.push(captured);
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
    latched: () => latched,
    unmount: () => {
      instance.unmount();
      activeInstance = null;
    },
  };
}

/** One pump of render cycles + microtasks; a deadline belongs to waitFor. */
async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 5));
  await Promise.resolve();
}

async function waitFor(
  predicate: () => boolean,
  timeoutMs = 2000
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

    harness.store.setState({
      loadingMessage: 'Agent changing to debug_agent',
    } as any);
    await waitFor(() => harness.current()?.name === 'debug_agent');

    expect(harness.current()).toEqual({
      name: 'debug_agent',
      baseAgent: 'kiro_default',
    });
    // The first latch ever rendered already carried both fields, so the footer
    // never shows a swap whose base agent is still unknown.
    expect(harness.latched()[0]).toEqual({
      name: 'debug_agent',
      baseAgent: 'kiro_default',
    });
  });

  test("reads the swap target only from the dispatcher's own message", () => {
    // Asserted on the pure decision rather than through a mounted hook: a null
    // latch cannot tell "this message names no swap" apart from "the effect has
    // not run yet", and every barrier that would prove the effect ran is itself
    // one of the exits that clears the latch, so the same commit erases the
    // latch a too-broad pattern would have produced.
    expect(parseSwapTarget('Agent changing to debug_agent')).toBe(
      'debug_agent'
    );
    // `/agent swap  debug_agent` reaches the message with its extra space, so
    // an untrimmed capture would label the footer chip " debug_agent".
    expect(parseSwapTarget('Agent changing to  debug_agent')).toBe(
      'debug_agent'
    );
    expect(parseSwapTarget('Agent changing to   ')).toBeNull();
    expect(parseSwapTarget('Loading agent options...')).toBeNull();
    expect(parseSwapTarget('Agent changed to debug_agent')).toBeNull();
    expect(parseSwapTarget('...Agent changing to debug_agent')).toBeNull();
    expect(parseSwapTarget('')).toBeNull();
    expect(parseSwapTarget(null)).toBeNull();
  });

  test('clears the latch when currentAgent.name moves off baseAgent', async () => {
    const harness = mountHook({
      loadingMessage: 'Agent changing to debug_agent',
      currentAgent: { name: 'kiro_default' } as any,
    });
    await waitFor(() => harness.current()?.name === 'debug_agent');
    expect(harness.current()?.name).toBe('debug_agent');

    // Backend settled: currentAgent flipped.
    harness.store.setState({
      currentAgent: { name: 'debug_agent' } as any,
      loadingMessage: null,
    } as any);
    await waitFor(() => harness.current() === null);

    expect(harness.current()).toBeNull();
  });

  test('safety timeout clears a stale pendingSwap', async () => {
    const harness = mountHook({
      loadingMessage: 'Agent changing to ghost_agent',
      currentAgent: { name: 'kiro_default' } as any,
      safetyTimeoutMs: 25,
    });

    await waitFor(() => harness.latched().length > 0);
    expect(harness.latched()[0]?.name).toBe('ghost_agent');

    // Nothing else can clear this swap: the agent never moves and
    // loadingMessage stays set, so only the timeout can drop the latch.
    await waitFor(() => harness.current() === null);
    expect(harness.current()).toBeNull();
  });

  test('defaults the safety timeout to a duration longer than a swap RPC', () => {
    // Every other case injects a short timeout, so nothing else constrains the
    // shipped default, and a floor beneath the point where a stalled RPC is
    // presumed dead would clear the swap indicator while it is still in flight.
    expect(SAFETY_TIMEOUT_MS).toBeGreaterThanOrEqual(10_000);
  });

  test('clears the latch when loadingMessage clears without an agent change (failed swap)', async () => {
    // `/agent nonexistent`: the dispatcher writes "Agent changing to ..." then
    // clears loadingMessage when the RPC resolves success:false. currentAgent
    // never moves, so without this exit the spinner would hang for 30s.
    const harness = mountHook({
      loadingMessage: 'Agent changing to nonexistent',
      currentAgent: { name: 'kiro_default' } as any,
    });
    await waitFor(() => harness.current()?.name === 'nonexistent');
    expect(harness.current()?.name).toBe('nonexistent');

    // RPC resolved (failure): dispatcher clears loadingMessage, agent unchanged.
    harness.store.setState({
      loadingMessage: null,
    } as any);
    await waitFor(() => harness.current() === null);

    expect(harness.current()).toBeNull();
  });

  test('preserves the original baseAgent across mid-swap re-issues', async () => {
    const harness = mountHook({
      loadingMessage: null,
      currentAgent: { name: 'kiro_default' } as any,
    });

    harness.store.setState({
      loadingMessage: 'Agent changing to interim_agent',
    } as any);
    await waitFor(() => harness.current()?.name === 'interim_agent');
    expect(harness.current()).toEqual({
      name: 'interim_agent',
      baseAgent: 'kiro_default',
    });

    // A second swap is issued before the first settles. baseAgent must
    // stay 'kiro_default' so the equality check still detects "settled".
    harness.store.setState({
      loadingMessage: 'Agent changing to final_agent',
    } as any);
    await waitFor(() => harness.current()?.name === 'final_agent');
    expect(harness.current()).toEqual({
      name: 'final_agent',
      baseAgent: 'kiro_default',
    });
  });
});
