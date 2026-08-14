/**
 * Session-row enrichment — the title chain, Msgs-count resolution, and
 * emptiness classification the dashboard applies to every listed session.
 * Pure: all index/sidecar state comes in through {@link EnrichmentSource}.
 */

import {
  sessionIdentityKey,
  sessionMatchesActive,
  type SessionListingInput,
} from './session-dashboard.js';

/** The slice of a metadata document enrichment reads. */
export interface EnrichmentDoc {
  title?: string;
  entryCount: number;
  promptCount: number;
  /** True when the metadata read was size-capped — its counts are floors. */
  countCapped?: boolean;
}

/** Everything enrichment needs from the content index. */
export interface EnrichmentSource {
  getDocument(
    sessionId: string,
    engine: SessionListingInput['engine']
  ): EnrichmentDoc | undefined | null;
  isPromptless(
    sessionId: string,
    engine: SessionListingInput['engine']
  ): boolean;
  getPromptTitle(
    sessionId: string,
    engine: SessionListingInput['engine']
  ): string | undefined;
  getPromptCount(
    sessionId: string,
    engine: SessionListingInput['engine']
  ): number | undefined;
  /**
   * Whether "the index never saw this session" may be read as "empty".
   * Only sound when prompts are covered (not the titles-only degradation)
   * and the build is complete (a cold build's marks are still filling in).
   * Classic (V1) rows are exempted here regardless — the index never walks
   * their store.
   */
  canInferNeverIndexed: boolean;
}

export interface EnrichmentResult {
  enrichedSessions: SessionListingInput[];
  /** Sessions with zero user prompts — hidden outside the +empty modes. */
  emptyIds: Set<string>;
}

const isUnhelpfulTitle = (title: string | undefined): boolean =>
  !title || title === 'New Session' || title === '(no title)';

/** Cloud rows keep their transcript in the backend — an absent or thin
 *  local record says nothing about whether the conversation is empty. */
const isCloudRow = (s: SessionListingInput): boolean =>
  s.source === 'remote' ||
  (typeof s.executionTarget?.kind === 'string' &&
    s.executionTarget.kind !== 'local');

export function sessionEnrichmentKey(
  sessionId: string,
  engine: SessionListingInput['engine'],
  source?: SessionListingInput['source']
): string {
  return sessionIdentityKey({ sessionId, engine, source });
}

export function enrichSessions(
  sessions: readonly SessionListingInput[],
  activeSessionId: string | null | undefined,
  getTitleOverride: (sessionId: string) => string | undefined | null,
  source: EnrichmentSource | null,
  activeEngine?: SessionListingInput['engine'],
  activeSource?: SessionListingInput['source']
): EnrichmentResult {
  const empty = new Set<string>();
  const promptCount = (s: SessionListingInput) =>
    source?.getPromptCount(s.sessionId, s.engine);
  const promptless = (s: SessionListingInput) =>
    source?.isPromptless(s.sessionId, s.engine) ?? false;
  const isActive = (session: SessionListingInput) =>
    sessionMatchesActive(session, activeSessionId, activeEngine, activeSource);
  const enriched = sessions.map((s) => {
    // Title chain slot 1: the user's rename always wins.
    const override = getTitleOverride(s.sessionId);
    const doc = source?.getDocument(s.sessionId, s.engine);
    const unhelpful = isUnhelpfulTitle(s.title);
    if (!doc) {
      // No metadata doc: KAS-native session or index still building. The
      // index's content verdict is used when it has one; the placeholder
      // title is only a LAST-resort signal while the index is cold. The
      // ACTIVE session is exempt: it starts prompt-less but must stay
      // visible.
      const count = promptCount(s);
      const neverIndexed =
        source != null &&
        source.canInferNeverIndexed &&
        s.engine !== 'classic' &&
        !promptless(s) &&
        count === undefined;
      const isEmpty = source ? promptless(s) || neverIndexed : unhelpful;
      if (isEmpty && !isActive(s) && !isCloudRow(s)) {
        empty.add(sessionEnrichmentKey(s.sessionId, s.engine, s.source));
      }
      const countPatch =
        s.messageCount == null && count !== undefined
          ? { messageCount: count }
          : {};
      if (override) return { ...s, title: override, ...countPatch };
      const promptTitle = unhelpful
        ? source?.getPromptTitle(s.sessionId, s.engine)
        : undefined;
      return promptTitle
        ? { ...s, title: promptTitle, ...countPatch }
        : { ...s, ...countPatch };
    }
    // "Empty" = the user never sent a prompt. Boot-only sessions have log
    // entries (config events) but no prompt — sink those too. Capped docs
    // defer to the content index's verdict (their count is a floor).
    if (
      !isActive(s) &&
      !isCloudRow(s) &&
      (doc.countCapped
        ? promptless(s)
        : doc.entryCount === 0 || doc.promptCount === 0)
    ) {
      empty.add(sessionEnrichmentKey(s.sessionId, s.engine, s.source));
    }
    let out = s;
    // Messages column: the listing's count wins; then the content index's
    // exact count (marks); the doc's count last — it's a floor when the
    // metadata read was capped.
    if (out.messageCount == null) {
      const indexCount = promptCount(s);
      const docCount = doc.promptCount > 0 ? doc.promptCount : undefined;
      const best = doc.countCapped
        ? (indexCount ?? docCount)
        : (docCount ?? indexCount);
      if (best != null) out = { ...out, messageCount: best };
    }
    if (override) {
      out = { ...out, title: override };
    } else if (unhelpful && doc.title && doc.title !== '(no title)') {
      out = { ...out, title: doc.title };
    }
    return out;
  });
  return { enrichedSessions: enriched, emptyIds: empty };
}
