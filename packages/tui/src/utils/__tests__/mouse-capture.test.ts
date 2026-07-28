import { afterEach, describe, expect, it } from 'bun:test';
import {
  connectMouseCapture,
  isMouseCaptureEnabled,
  setMouseCaptureEnabled,
} from '../mouse-capture.js';

describe('mouse capture bridge', () => {
  let disconnect: (() => void) | null = null;

  afterEach(() => {
    disconnect?.();
    disconnect = null;
    setMouseCaptureEnabled(false);
  });

  it('starts disabled and supports an explicit runtime toggle', () => {
    const states: boolean[] = [];
    disconnect = connectMouseCapture({
      setMouseEnabled(nextEnabled) {
        states.push(nextEnabled);
      },
    });

    setMouseCaptureEnabled(true);
    setMouseCaptureEnabled(false);

    expect(states).toEqual([false, true, false]);
    expect(isMouseCaptureEnabled()).toBe(false);
  });

  it('applies a pending toggle when the renderer connects', () => {
    const states: boolean[] = [];
    setMouseCaptureEnabled(true);

    disconnect = connectMouseCapture({
      setMouseEnabled(nextEnabled) {
        states.push(nextEnabled);
      },
    });

    expect(states).toEqual([true]);
    expect(isMouseCaptureEnabled()).toBe(true);
  });
});
