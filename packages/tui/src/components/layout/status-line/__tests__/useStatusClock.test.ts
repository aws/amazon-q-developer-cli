/**
 * Unit tests for the status-line clock.
 *
 * Two guarantees matter. A disabled clock must schedule nothing at all, so the
 * common case where neither date nor time is shown costs nothing. And an enabled
 * clock must re-derive its value per tick rather than capture it once, which is the
 * freeze that made the git branch go stale.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { useStatusClock } from '../useStatusClock.js';

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

let active: Instance | null = null;

afterEach(() => {
  active?.unmount();
  active = null;
});

/** The real timer, so a wait in a test is never mistaken for the hook's. */
const realSetTimeout = globalThis.setTimeout;
const wait = (ms: number) =>
  new Promise((resolve) => realSetTimeout(resolve, ms));

interface Scheduled {
  id: unknown;
  delay: number;
}

/**
 * Mount the hook with timers recorded, then unmount.
 *
 * Nothing else in this harness calls setTimeout, so the record is exactly what the
 * hook did. Waiting through `realSetTimeout` keeps the helper out of its own record.
 */
async function measure(enabled: boolean): Promise<{
  values: (Date | null)[];
  scheduled: Scheduled[];
  cleared: unknown[];
}> {
  const realSet = globalThis.setTimeout;
  const realClear = globalThis.clearTimeout;
  const scheduled: Scheduled[] = [];
  const cleared: unknown[] = [];

  globalThis.setTimeout = ((
    fn: (...args: unknown[]) => void,
    delay?: number,
    ...rest: unknown[]
  ) => {
    const id = realSet(fn, delay as never, ...(rest as never[]));
    if (delay !== undefined) scheduled.push({ id, delay });
    return id;
  }) as typeof globalThis.setTimeout;
  globalThis.clearTimeout = ((id: unknown) => {
    cleared.push(id);
    return realClear(id as never);
  }) as typeof globalThis.clearTimeout;

  try {
    const values: (Date | null)[] = [];
    const Probe: React.FC = () => {
      values.push(useStatusClock(enabled));
      return null;
    };
    active = render(React.createElement(Probe), {
      terminal: new MockTerminal(),
      exitOnCtrlC: false,
    });
    await wait(60);
    active.unmount();
    active = null;
    await wait(30);
    return { values, scheduled, cleared };
  } finally {
    globalThis.setTimeout = realSet;
    globalThis.clearTimeout = realClear;
  }
}

describe('useStatusClock', () => {
  it('returns null and schedules nothing while disabled', async () => {
    const off = await measure(false);

    expect(off.values.every((v) => v === null)).toBe(true);
    expect(off.scheduled).toEqual([]);
  });

  it('schedules one realignment to the next minute while enabled', async () => {
    const on = await measure(true);

    // One timer, not a repeating interval, and aimed inside the coming minute.
    expect(on.scheduled.length).toBe(1);
    expect(on.scheduled[0]!.delay).toBeGreaterThan(0);
    expect(on.scheduled[0]!.delay).toBeLessThanOrEqual(60_000);
  });

  it('has a value straight away when switched on from off', async () => {
    // Without this the bar would show nothing until the next minute boundary,
    // which is up to a minute of a segment the user just enabled looking broken.
    const values: (Date | null)[] = [];
    const Probe: React.FC<{ on: boolean }> = ({ on }) => {
      values.push(useStatusClock(on));
      return null;
    };
    active = render(React.createElement(Probe, { on: false }), {
      terminal: new MockTerminal(),
      exitOnCtrlC: false,
    });
    await wait(40);
    expect(values.at(-1)).toBeNull();

    active.rerender(React.createElement(Probe, { on: true }));
    await wait(40);
    expect(values.at(-1)).toBeInstanceOf(Date);
  });

  it('clears that timer on unmount', async () => {
    const on = await measure(true);

    // Matching the id rather than counting clears: a count would pass even with
    // the cleanup deleted.
    expect(on.cleared).toContain(on.scheduled[0]!.id);
  });

  it('returns a clock close to now while enabled', async () => {
    const on = await measure(true);
    const last = on.values[on.values.length - 1];

    expect(last).toBeInstanceOf(Date);
    expect(Math.abs(Date.now() - (last as Date).getTime())).toBeLessThan(5_000);
  });
});
