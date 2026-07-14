import type {
  SessionActivityStatus,
  SessionsChangedNotification,
  SessionRosterEntry,
  ProvisioningFailureCode,
} from '../types/session-client';

/**
 * A locally-tracked roster row. Unlike the wire's full `SessionRosterEntry`,
 * every snapshot field is optional: a partial first upsert (change-data-capture)
 * may arrive before the full snapshot, and the type says so instead of lying
 * via a cast.
 */
export type RosterEntry = { sessionId: string } & Partial<
  Omit<SessionRosterEntry, 'sessionId'>
>;

/** The active session's live status + failure detail, or null when the roster
 *  does not track it (local session, never seen, or retracted). */
export interface ActiveSessionStatus {
  status: SessionActivityStatus;
  provisioningFailure?: { code: ProvisioningFailureCode };
}

/**
 * Merge one `_kiro/sessions/changed` delta into a roster map, returning a new
 * map. Change-data-capture: an upsert carries only changed fields over
 * `sessionId` (absent == unchanged; the wire is JSON, so a field is omitted
 * rather than sent as `undefined`), so present fields are spread over the
 * prior entry. `deleted` ids are retracted.
 */
export function mergeRosterDelta(
  roster: ReadonlyMap<string, RosterEntry>,
  delta: SessionsChangedNotification
): Map<string, RosterEntry> {
  const next = new Map(roster);
  for (const up of delta.upserted ?? []) {
    if (!up?.sessionId) continue;
    const prior = next.get(up.sessionId);
    next.set(up.sessionId, { ...(prior ?? {}), ...up });
  }
  for (const id of delta.deleted ?? []) {
    next.delete(id);
  }
  return next;
}

/**
 * Derive the attached session's status from the roster. Returns null when the
 * roster has no row for it — including after a `deleted` retraction, so a
 * stale status never lingers once the session is gone.
 */
export function deriveActiveSessionStatus(
  roster: ReadonlyMap<string, RosterEntry>,
  activeSessionId: string | null | undefined
): ActiveSessionStatus | null {
  if (!activeSessionId) return null;
  const entry = roster.get(activeSessionId);
  if (!entry?.status) return null;
  return {
    status: entry.status,
    ...(entry.provisioningFailure && {
      provisioningFailure: entry.provisioningFailure,
    }),
  };
}
