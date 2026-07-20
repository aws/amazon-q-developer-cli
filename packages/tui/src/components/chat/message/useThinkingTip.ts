import { useState, useEffect, useRef } from 'react';
import { pickTip } from '../../../tips/tips.js';
import type { TipContext } from '../../../tips/tips.js';

/** Delay before showing the tip (ms). */
export const TIP_SHOW_DELAY_MS = 2_000;

/**
 * Picks a single tip after a delay. Returns null until the delay fires,
 * then the tip text for the remainder of the mount. One pick per mount,
 * no rotation.
 *
 * When `enabled` is false, no timer is created and null is always returned.
 * This prevents unnecessary work in secondary mount sites (subagent panels).
 */
export function useThinkingTip(
  ctx: TipContext,
  enabled: boolean,
  /**
   * Delay before the tip appears. Defaults to the production value. Tests
   * override it with a small value so the timer path can be exercised with
   * real timers: twinki renders through a ConcurrentRoot react-reconciler
   * whose passive effects flush via the React scheduler, which fake timers
   * do not advance.
   */
  delayMs: number = TIP_SHOW_DELAY_MS
): string | null {
  const [tipText, setTipText] = useState<string | null>(null);
  // Capture ctx at mount time — these values are static after boot.
  const ctxRef = useRef(ctx);

  useEffect(() => {
    if (!enabled) return;

    const delayTimer = setTimeout(() => {
      setTipText(pickTip(ctxRef.current) ?? null);
    }, delayMs);

    return () => clearTimeout(delayTimer);
  }, []); // Empty deps: pick once per mount, never re-fire.

  return enabled ? tipText : null;
}
