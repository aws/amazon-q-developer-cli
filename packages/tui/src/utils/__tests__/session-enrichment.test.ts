import { describe, it, expect } from 'bun:test';
import {
  enrichSessions,
  sessionEnrichmentKey,
  type EnrichmentDoc,
  type EnrichmentSource,
} from '../session-enrichment';
import type { SessionListingInput } from '../session-dashboard';

function mk(overrides: Partial<SessionListingInput> = {}): SessionListingInput {
  return {
    sessionId: 's1',
    cwd: '/w',
    title: 'New Session',
    updatedAt: '2026-07-20T10:00:00.000Z',
    ...overrides,
  } as SessionListingInput;
}

type SourceOverrides = Partial<EnrichmentSource> & {
  promptless?: ReadonlySet<string>;
  promptTitles?: ReadonlyMap<string, string>;
  promptCounts?: ReadonlyMap<string, number>;
};

function source(overrides: SourceOverrides = {}): EnrichmentSource {
  const {
    promptless = new Set<string>(),
    promptTitles = new Map<string, string>(),
    promptCounts = new Map<string, number>(),
    ...methods
  } = overrides;
  return {
    getDocument: () => undefined,
    isPromptless: (id) => promptless.has(id),
    getPromptTitle: (id) => promptTitles.get(id),
    getPromptCount: (id) => promptCounts.get(id),
    canInferNeverIndexed: true,
    ...methods,
  };
}

const noOverride = () => undefined;
const isEmpty = (
  result: ReturnType<typeof enrichSessions>,
  id = 's1',
  engine?: SessionListingInput['engine']
) => result.emptyIds.has(sessionEnrichmentKey(id, engine));

describe('emptiness classification (no metadata doc)', () => {
  it('classifies index-verified promptless sessions as empty', () => {
    const r = enrichSessions(
      [mk()],
      null,
      noOverride,
      source({ promptless: new Set(['s1']) })
    );
    expect(isEmpty(r)).toBe(true);
  });

  it('classifies never-indexed sessions as empty when inference is sound', () => {
    const r = enrichSessions([mk()], null, noOverride, source());
    expect(isEmpty(r)).toBe(true);
  });

  it('never infers emptiness for classic rows (the index cannot see them)', () => {
    const r = enrichSessions(
      [mk({ engine: 'classic' })],
      null,
      noOverride,
      source()
    );
    expect(isEmpty(r, 's1', 'classic')).toBe(false);
  });

  it('never infers emptiness while the inference is unsound (cold build / titles-only)', () => {
    const r = enrichSessions(
      [mk()],
      null,
      noOverride,
      source({ canInferNeverIndexed: false })
    );
    expect(isEmpty(r)).toBe(false);
  });

  it('a session the index counted prompts for is not never-indexed', () => {
    const r = enrichSessions(
      [mk()],
      null,
      noOverride,
      source({ promptCounts: new Map([['s1', 4]]) })
    );
    expect(isEmpty(r)).toBe(false);
    expect(r.enrichedSessions[0]!.messageCount).toBe(4);
  });

  it('exempts the active session', () => {
    const r = enrichSessions(
      [mk()],
      's1',
      noOverride,
      source({ promptless: new Set(['s1']) })
    );
    expect(isEmpty(r)).toBe(false);
  });

  it('never classifies cloud rows as empty (transcript lives in the backend)', () => {
    const r = enrichSessions(
      [
        mk({ executionTarget: { kind: 'cloud-sandbox' } }),
        mk({ sessionId: 's2', source: 'remote' }),
      ],
      null,
      noOverride,
      source({ promptless: new Set(['s1', 's2']) })
    );
    expect(isEmpty(r)).toBe(false);
    expect(isEmpty(r, 's2')).toBe(false);
  });

  it('falls back to the placeholder-title heuristic with no index at all', () => {
    const r = enrichSessions(
      [
        mk({ title: 'New Session' }),
        mk({ sessionId: 's2', title: 'real work' }),
      ],
      null,
      noOverride,
      null
    );
    expect(isEmpty(r)).toBe(true);
    expect(isEmpty(r, 's2')).toBe(false);
  });
});

describe('emptiness classification (with metadata doc)', () => {
  const doc = (d: Partial<EnrichmentDoc>): EnrichmentDoc => ({
    entryCount: 1,
    promptCount: 1,
    ...d,
  });

  it('prompt-less docs are empty', () => {
    const r = enrichSessions(
      [mk()],
      null,
      noOverride,
      source({ getDocument: () => doc({ promptCount: 0 }) })
    );
    expect(isEmpty(r)).toBe(true);
  });

  it('capped docs defer to the index verdict, not the floor count', () => {
    // Capped doc shows 0 prompts (floor), but the index read the transcript
    // and found prompts — NOT empty.
    const r = enrichSessions(
      [mk()],
      null,
      noOverride,
      source({
        getDocument: () => doc({ promptCount: 0, countCapped: true }),
        promptCounts: new Map([['s1', 182]]),
      })
    );
    expect(isEmpty(r)).toBe(false);
    expect(r.enrichedSessions[0]!.messageCount).toBe(182);
  });

  it('capped docs prefer the exact index count over the floor', () => {
    const r = enrichSessions(
      [mk()],
      null,
      noOverride,
      source({
        getDocument: () => doc({ promptCount: 3, countCapped: true }),
        promptCounts: new Map([['s1', 42]]),
      })
    );
    expect(r.enrichedSessions[0]!.messageCount).toBe(42);
  });

  it('the listing count always wins', () => {
    const r = enrichSessions(
      [mk({ messageCount: 7 })],
      null,
      noOverride,
      source({
        getDocument: () => doc({ promptCount: 3 }),
        promptCounts: new Map([['s1', 42]]),
      })
    );
    expect(r.enrichedSessions[0]!.messageCount).toBe(7);
  });
});

describe('title chain', () => {
  it('a rename override beats everything', () => {
    const r = enrichSessions(
      [mk()],
      null,
      (id) => (id === 's1' ? 'My Renamed Session' : undefined),
      source({
        getDocument: () => ({
          entryCount: 1,
          promptCount: 1,
          title: 'doc title',
        }),
      })
    );
    expect(r.enrichedSessions[0]!.title).toBe('My Renamed Session');
  });

  it('doc title replaces a placeholder title', () => {
    const r = enrichSessions(
      [mk({ title: 'New Session' })],
      null,
      noOverride,
      source({
        getDocument: () => ({
          entryCount: 1,
          promptCount: 1,
          title: 'fix the bug',
        }),
      })
    );
    expect(r.enrichedSessions[0]!.title).toBe('fix the bug');
  });

  it('a helpful listing title is kept over the doc title', () => {
    const r = enrichSessions(
      [mk({ title: 'my real title' })],
      null,
      noOverride,
      source({
        getDocument: () => ({
          entryCount: 1,
          promptCount: 1,
          title: 'doc title',
        }),
      })
    );
    expect(r.enrichedSessions[0]!.title).toBe('my real title');
  });

  it('first-prompt title fills in for doc-less placeholder rows', () => {
    const r = enrichSessions(
      [mk({ title: 'New Session' })],
      null,
      noOverride,
      source({
        promptTitles: new Map([['s1', 'help me refactor']]),
        promptCounts: new Map([['s1', 2]]),
      })
    );
    expect(r.enrichedSessions[0]!.title).toBe('help me refactor');
    expect(r.enrichedSessions[0]!.messageCount).toBe(2);
  });
});
