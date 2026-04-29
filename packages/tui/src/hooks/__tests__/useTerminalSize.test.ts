import { describe, it, expect } from 'bun:test';
import { throttle } from 'es-toolkit/compat';

// Tests the leading+trailing throttle contract that Twinki's resize handler uses.

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe('Debounces Screen resize throttling', () => {
  it('leading: fires the first call immediately', () => {
    const calls: number[] = [];
    const fn = throttle((v: number) => calls.push(v), 100, {
      leading: true,
      trailing: true,
    });
    fn(1);
    expect(calls).toEqual([1]);
  });

  it('trailing: fires after the window with the last value', async () => {
    const calls: number[] = [];
    const fn = throttle((v: number) => calls.push(v), 100, {
      leading: true,
      trailing: true,
    });
    fn(1); // leading
    fn(2); // throttled
    fn(3); // throttled — trailing value
    expect(calls).toEqual([1]);
    await sleep(150);
    expect(calls).toEqual([1, 3]);
  });

  it('at most 2 calls (leading + trailing) during rapid bursts', async () => {
    const calls: number[] = [];
    const fn = throttle((v: number) => calls.push(v), 100, {
      leading: true,
      trailing: true,
    });
    for (let i = 0; i < 20; i++) fn(i);
    expect(calls).toEqual([0]);
    await sleep(150);
    expect(calls).toEqual([0, 19]);
  });

  it('calls resume normally after throttle window expires', async () => {
    const calls: number[] = [];
    const fn = throttle((v: number) => calls.push(v), 100, {
      leading: true,
      trailing: true,
    });
    fn(1);
    await sleep(150);
    fn(2);
    expect(calls).toEqual([1, 2]);
  });
});
