import type { SessionEntry } from './list-all-sessions-cli';

import { isCloudExecutionTargetKind } from '../types/multi-session';

/**
 * A full-form session id starts `xxxxxxxx-xxxx…`; anything shorter is treated as
 * a short prefix (the value `--list-sessions` renders is an 8-char head).
 */
export function isFullSessionId(id: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}/i.test(id);
}

export interface ResumeTargetResolution {
  /** Resume id after short-prefix expansion; unchanged when ambiguous or no match. */
  resumeId: string;
  /** True when the (uniquely) matched row is a cloud sandbox, flipping the launch to cloud. */
  cloud: boolean;
  /** True when a short prefix matched more than one session — the caller must NOT resume. */
  ambiguous: boolean;
  /** Match count, for the ambiguity error message. */
  matchCount: number;
}

/**
 * Resolve a `--resume-id` against the merged local+cloud listing. A unique prefix
 * expands to the full id and a cloud-sandbox row flips `cloud` on. An ambiguous
 * prefix resolves to nothing (`ambiguous: true`) so the caller starts fresh rather
 * than resuming a guessed session. Pure so it is unit-testable.
 */
export function resolveResumeTarget(
  resumeId: string,
  cloud: boolean,
  sessions: SessionEntry[]
): ResumeTargetResolution {
  const matches = isFullSessionId(resumeId)
    ? sessions.filter((s) => s.sessionId === resumeId)
    : sessions.filter((s) => s.sessionId.startsWith(resumeId));
  if (matches.length > 1) {
    return { resumeId, cloud, ambiguous: true, matchCount: matches.length };
  }
  if (matches.length === 1) {
    const row = matches[0]!;
    return {
      resumeId: row.sessionId,
      cloud: cloud || isCloudExecutionTargetKind(row.executionTarget),
      ambiguous: false,
      matchCount: 1,
    };
  }
  return { resumeId, cloud, ambiguous: false, matchCount: 0 };
}
