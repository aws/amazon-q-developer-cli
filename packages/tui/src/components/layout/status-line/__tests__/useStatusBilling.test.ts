/**
 * Unit tests for the status-line billing fetch.
 *
 * Three guarantees matter. Nothing is requested while the segments are off, so
 * users who never enable them pay nothing. Nothing is requested before a session
 * exists, because the command fails then and the figures would stay blank until
 * the user happened to send a message. And a reading already on screen survives a
 * later response that carries nothing usable.
 */
import {
  describe,
  it,
  expect,
  afterAll,
  afterEach,
  beforeEach,
} from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

const tempHome = mkdtempSync(join(tmpdir(), 'kiro-status-billing-test-'));
const originalHome = process.env.KIRO_HOME;
// Set per test rather than here: a module-scope assignment outlives this file
// and would point other suites at these fixtures.
mkdirSync(join(tempHome, 'settings'), { recursive: true });

const { useStatusBilling } = await import('../useStatusBilling.js');
const { AppStoreContext, createAppStore } =
  await import('../../../../stores/app-store.js');
type StatusBilling = import('../billing.js').StatusBilling;
type AppStore = ReturnType<typeof createAppStore>;

class MockTerminal implements Terminal {
  public output = '';
  get columns() {
    return 120;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(): void {}
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

function writeVisibility(override: Record<string, boolean>): void {
  writeVisibilityFor({ tui: override });
}

function writeVisibilityFor(
  perSurface: Partial<Record<'tui' | 'lite', Record<string, boolean>>>
): void {
  const settings: Record<string, unknown> = {};
  for (const [surface, override] of Object.entries(perSurface)) {
    settings[`chat.statusLine.${surface}`] = override;
  }
  writeFileSync(
    join(tempHome, 'settings', 'cli.json'),
    JSON.stringify(settings)
  );
  invalidateStatusSegments();
}

/** A usage payload with one limited dimension, matching the shape KAS returns. */
function usagePayload(used: number, limit: number) {
  return { usageBreakdowns: [{ hasLimit: true, limit, used }] };
}

interface FakeKiro {
  calls: number;
  executeCommand: (req: unknown) => Promise<unknown>;
}

/** A kiro stand-in whose responses are scripted per call. */
function fakeKiro(responses: unknown[]): FakeKiro {
  const k: FakeKiro = {
    calls: 0,
    executeCommand: async () => {
      const next = responses[Math.min(k.calls, responses.length - 1)];
      k.calls += 1;
      return next;
    },
  };
  return k;
}

interface DeferredKiro extends FakeKiro {
  settle: (index: number, response: unknown) => void;
}

/** A backend whose answers are released by the test, one call at a time. */
function deferredKiro(): DeferredKiro {
  const waiting: Array<(value: unknown) => void> = [];
  const k: DeferredKiro = {
    calls: 0,
    executeCommand: () =>
      new Promise((resolve) => {
        k.calls += 1;
        waiting.push(resolve);
      }),
    settle: (index, response) => waiting[index]?.(response),
  };
  return k;
}

/** A backend whose command rejects rather than answering. */
function rejectingKiro(): FakeKiro {
  const k: FakeKiro = {
    calls: 0,
    executeCommand: async () => {
      k.calls += 1;
      throw new Error('transport closed');
    },
  };
  return k;
}

const { invalidateStatusSegments } = await import('../config.js');
let active: Instance | null = null;
let store: AppStore;

beforeEach(() => {
  process.env.KIRO_HOME = tempHome;
  invalidateStatusSegments();
  writeVisibility({ usage: true, credits: true });
});

afterEach(() => {
  if (originalHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalHome;
  active?.unmount();
  active = null;
});

/** Mount the hook and expose the latest value it returned. */
function mount(
  kiro: unknown,
  uiMode: 'tui' | 'lite' = 'tui'
): {
  latest: () => StatusBilling;
} {
  const seen: StatusBilling[] = [];
  const Probe: React.FC = () => {
    seen.push(useStatusBilling(uiMode));
    return null;
  };
  store = createAppStore({ kiro: kiro as never, agentEngine: 'v2', uiMode });
  active = render(
    React.createElement(
      AppStoreContext.Provider,
      { value: store },
      React.createElement(Probe)
    ),
    { terminal: new MockTerminal(), exitOnCtrlC: false }
  );
  return {
    latest: () => {
      const last = seen[seen.length - 1];
      if (!last) throw new Error('hook produced no value');
      return last;
    },
  };
}

const settle = () => new Promise((r) => setTimeout(r, 60));

describe('useStatusBilling', () => {
  it('requests nothing while no billing segment is configured', async () => {
    writeVisibility({ usage: false, credits: false });
    const kiro = fakeKiro([{ success: true, data: usagePayload(20, 100) }]);
    const probe = mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();

    expect(kiro.calls).toBe(0);
    expect(probe.latest().usagePercent).toBeNull();
  });

  it('requests nothing until a session exists', async () => {
    const kiro = fakeKiro([{ success: true, data: usagePayload(20, 100) }]);
    mount(kiro);
    await settle();

    expect(kiro.calls).toBe(0);
  });

  it('gates on the requested surface, not any surface', async () => {
    // Only the tui surface enables billing, so a lite bar must not fetch.
    writeVisibilityFor({
      tui: { usage: true, credits: true },
      lite: { usage: false, credits: false },
    });
    const kiro = fakeKiro([{ success: true, data: usagePayload(20, 100) }]);
    mount(kiro, 'lite');
    store.setState({ sessionId: 's1' });
    await settle();

    expect(kiro.calls).toBe(0);
  });

  it('fetches once the session is established', async () => {
    const kiro = fakeKiro([{ success: true, data: usagePayload(2040, 10000) }]);
    const probe = mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();

    expect(kiro.calls).toBe(1);
    expect(probe.latest().usagePercent).toBe(20);
    expect(probe.latest().creditsRemaining).toBe(7960);
  });

  it('refreshes when a turn completes', async () => {
    const kiro = fakeKiro([
      { success: true, data: usagePayload(2000, 10000) },
      { success: true, data: usagePayload(3000, 10000) },
    ]);
    const probe = mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();
    expect(probe.latest().usagePercent).toBe(20);

    store.setState({ turnsCompleted: 1 });
    await settle();

    expect(kiro.calls).toBe(2);
    expect(probe.latest().usagePercent).toBe(30);
  });

  it('asks once on a plan that reports no limit', async () => {
    // An unlimited plan answers successfully but derives nothing. Treating that
    // as "not yet fetched" would re-ask on every render for the whole session.
    const kiro = fakeKiro([
      {
        success: true,
        data: { usageBreakdowns: [{ hasLimit: false, limit: 0, used: 0 }] },
      },
    ]);
    const probe = mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();
    expect(kiro.calls).toBe(1);

    // Only a completed turn refreshes; the busy flag dropping does not.
    store.setState({ isProcessing: false });
    await settle();

    expect(kiro.calls).toBe(1);
    expect(probe.latest().usagePercent).toBeNull();
  });

  it('does not re-ask mid-turn after a response it cannot use', async () => {
    const kiro = fakeKiro([{ success: false, message: 'not supported' }]);
    mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();
    expect(kiro.calls).toBe(1);

    // The busy flag dropping is not a turn boundary: a shell escape or a cancel
    // clears it too, and asking then costs a call for nothing.
    store.setState({ isProcessing: true });
    await settle();
    store.setState({ isProcessing: false });
    await settle();
    expect(kiro.calls).toBe(1);

    // A completed turn is the refresh point.
    store.setState({ turnsCompleted: 1 });
    await settle();
    expect(kiro.calls).toBe(2);
  });

  it('stops asking after repeated failures', async () => {
    const kiro = fakeKiro([{ success: false, message: 'not supported' }]);
    mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();

    for (let turn = 1; turn <= 6; turn += 1) {
      store.setState({ turnsCompleted: turn });
      await settle();
    }

    // Three strikes, then it stays quiet rather than spending a call and a warning
    // line on every turn for the rest of the session.
    expect(kiro.calls).toBe(3);
  });

  it('stops asking after repeated rejections', async () => {
    // A rejection never reaches the success path, so the cap has to be reached
    // from the failure handler too or a broken transport is retried every turn.
    const kiro = rejectingKiro();
    mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();

    for (let turn = 1; turn <= 6; turn++) {
      store.setState({ turnsCompleted: turn });
      await settle();
    }

    expect(kiro.calls).toBe(3);
  });

  it('asks again for a session that arrived mid-fetch', async () => {
    // The in-flight guard must not swallow the new session: its answer is the one
    // being displayed, and the open call belongs to a session already gone.
    const kiro = deferredKiro();
    const probe = mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();
    expect(kiro.calls).toBe(1);

    store.setState({ sessionId: 's2' });
    await settle();

    expect(kiro.calls).toBe(2);

    // The new session answers first, then the call the old one left open. That
    // order is what makes dropping the late answer observable.
    kiro.settle(1, { success: true, data: usagePayload(30, 100) });
    await settle();
    expect(probe.latest().usagePercent).toBe(30);

    kiro.settle(0, { success: true, data: usagePayload(20, 100) });
    await settle();

    expect(probe.latest().usagePercent).toBe(30);
  });

  it('keeps the last good reading when a later response is unusable', async () => {
    const kiro = fakeKiro([
      { success: true, data: usagePayload(2000, 10000) },
      { success: false, message: 'no session' },
    ]);
    const probe = mount(kiro);
    store.setState({ sessionId: 's1' });
    await settle();
    expect(probe.latest().usagePercent).toBe(20);

    store.setState({ turnsCompleted: 1 });
    await settle();

    expect(kiro.calls).toBe(2);
    expect(probe.latest().usagePercent).toBe(20);
  });
});

afterAll(() => {
  if (originalHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalHome;
  rmSync(tempHome, { recursive: true, force: true });
});
