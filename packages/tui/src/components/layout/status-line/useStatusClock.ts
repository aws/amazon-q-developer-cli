/**
 * Wall clock for the status line's date/time segments.
 *
 * Returns null when `enabled` is false so no timer exists for the common case
 * where neither clock segment is configured. When enabled it re-aligns to the
 * next minute boundary rather than ticking on a fixed interval, so the displayed
 * minute flips within a second of the real one without polling every second.
 *
 * A date segment left running across midnight is the freeze this guards against:
 * the value is derived per tick, never captured once at mount.
 */
import { useEffect, useState } from 'react';

const MINUTE_MS = 60_000;

export function useStatusClock(enabled: boolean): Date | null {
  const [now, setNow] = useState<Date | null>(() =>
    enabled ? new Date() : null
  );

  useEffect(() => {
    if (!enabled) {
      setNow(null);
      return;
    }
    // Fill it when switching on from off, where state still holds null and the
    // first tick is up to a minute away. On mount the initializer already ran.
    setNow((current) => current ?? new Date());
    let timer: ReturnType<typeof setTimeout> | undefined;
    let cancelled = false;
    const scheduleNext = () => {
      const current = new Date();
      const delay = MINUTE_MS - (current.getTime() % MINUTE_MS);
      timer = setTimeout(() => {
        if (cancelled) return;
        setNow(new Date());
        scheduleNext();
      }, delay);
    };
    scheduleNext();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [enabled]);

  return now;
}
