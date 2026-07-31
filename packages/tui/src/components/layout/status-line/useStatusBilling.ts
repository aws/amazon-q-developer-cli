/**
 * Keeps the status line's billing figures current.
 *
 * Waits for a session before the first fetch: the command needs one, and asking
 * too early returns `success: false` with no payload, which would leave the
 * segments blank until the user happened to send a message.
 *
 * Refresh is then driven by completed turns rather than a timer, because the
 * allowance only moves when the user sends a request; polling while idle would
 * spend calls to learn nothing. Nothing is fetched at all unless a segment that
 * needs it is switched on.
 */
import { useEffect, useRef, useState } from 'react';
import { useAppStore, type UsageData } from '../../../stores/app-store.js';
import type { UiMode } from '../../../types/ui-mode.js';
import { logger } from '../../../utils/logger.js';
import {
  deriveStatusBilling,
  EMPTY_STATUS_BILLING,
  type StatusBilling,
} from './billing.js';
import { statusSegmentsNeedBilling } from './config.js';
import { useStatusSegments } from './useStatusSegments.js';

/** Consecutive failures after which the command is treated as unavailable. */
const MAX_CONSECUTIVE_FAILURES = 3;

export function useStatusBilling(surface: UiMode): StatusBilling {
  const enabled = statusSegmentsNeedBilling(useStatusSegments(surface));
  const kiro = useAppStore((s) => s.kiro);
  const sessionId = useAppStore((s) => s.sessionId);
  // Counts completed turns. `isProcessing` also drops on a shell escape, a cancel
  // and the observer watchdog, so it would fetch outside a turn boundary.
  const turnsCompleted = useAppStore((s) => s.turnsCompleted);
  // Reuse the panel's copy when it is there so the bar shows a figure sooner; the
  // fetch below still runs on its own schedule.
  const panelUsage = useAppStore((s) => s.usageData);

  const [billing, setBilling] = useState<StatusBilling>(EMPTY_STATUS_BILLING);
  const inFlightFor = useRef<string | null>(null);
  const fetchedFor = useRef<string | null>(null);
  const failures = useRef(0);
  // Only unmount discards a result. An effect re-run must not, or the first read
  // of a session would be thrown away and never retried.
  const disposed = useRef(false);
  useEffect(() => {
    // Cleared on mount as well as set on unmount, so a remount does not inherit
    // a disposed flag and silently stop populating.
    disposed.current = false;
    return () => {
      disposed.current = true;
    };
  }, []);

  useEffect(() => {
    if (!enabled || !panelUsage) return;
    const derived = deriveStatusBilling(panelUsage);
    if (derived.usagePercent != null) setBilling(derived);
  }, [enabled, panelUsage]);

  useEffect(() => {
    if (!enabled || !kiro || !sessionId) return;
    if (fetchedFor.current !== sessionId) failures.current = 0;
    // A command that keeps failing is not going to start working this session, and
    // each attempt costs a call and a warning line.
    if (failures.current >= MAX_CONSECUTIVE_FAILURES) return;
    // Keyed by session rather than a plain flag: a switch mid-fetch has to start
    // its own request, or the surface keeps showing the old session's figures.
    if (inFlightFor.current === sessionId) return;
    inFlightFor.current = sessionId;
    const forSession = sessionId;
    void kiro
      .executeCommand({ command: 'usage', args: {} })
      .then((result) => {
        if (disposed.current) return;
        // A session took over while this was open, so this answer is not the
        // account state on screen.
        if (inFlightFor.current !== forSession) return;
        // Stamped on any answer, not just a usable one: a plan with no limit
        // answers successfully and derives nothing, and leaving the session
        // unstamped would re-ask on every render for the rest of the session.
        fetchedFor.current = forSession;
        if (!result?.success) {
          failures.current += 1;
          logger.warn(
            `[status-line] usage unavailable: ${result?.message ?? 'no message'}`
          );
          return;
        }
        const next = deriveStatusBilling(result.data as UsageData | undefined);
        failures.current = 0;
        // Keep the last good reading when a response carries nothing usable, so a
        // transient failure does not blank a figure the user was reading.
        if (next.usagePercent == null) return;
        setBilling(next);
      })
      .catch((err) => {
        // Stamped like an answer: an unstamped session clears the failure count
        // every turn, so a rejecting command would never reach the cap.
        if (inFlightFor.current !== forSession) return;
        fetchedFor.current = forSession;
        failures.current += 1;
        logger.warn('[status-line] usage fetch failed', err);
      })
      .finally(() => {
        if (inFlightFor.current === forSession) inFlightFor.current = null;
      });
    // Re-runs on a new session and on each completed turn, which is exactly when
    // the figures can have moved.
  }, [enabled, kiro, sessionId, turnsCompleted]);

  return enabled ? billing : EMPTY_STATUS_BILLING;
}
