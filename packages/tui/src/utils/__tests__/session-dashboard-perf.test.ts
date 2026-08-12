/**
 * Latency regression tests at realistic store scale.
 *
 * These exist because the dashboard shipped with an O(n²) enrichment path
 * (per-row accessors each running a full index table scan) that froze the
 * UI for minutes at ~9K sessions. Budgets are deliberately generous for CI
 * jitter — an accidental O(n²) reintroduction misses them by orders of
 * magnitude, a healthy O(n) pass clears them easily.
 */
import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { SessionSearchIndex } from '../session-search.js';
import { enrichSessions } from '../session-enrichment.js';
import { listAllWorkspaceSessionsFromDiskDetailed } from '../all-workspace-sessions.js';
import type { SessionListingInput } from '../session-dashboard.js';

const SESSION_COUNT = 3000;

let root: string;

function uuid(i: number): string {
  return `${String(i).padStart(8, '0')}-0000-4000-8000-${String(i).padStart(12, '0')}`;
}

function seedV2Store(count: number): void {
  const cliDir = join(root, 'cli');
  mkdirSync(cliDir, { recursive: true });
  for (let i = 0; i < count; i++) {
    const id = uuid(i);
    writeFileSync(
      join(cliDir, `${id}.json`),
      JSON.stringify({
        cwd: `/w/proj-${i % 40}`,
        title: `session number ${i}`,
        updated_at: new Date(1700000000000 + i * 1000).toISOString(),
      })
    );
    writeFileSync(
      join(cliDir, `${id}.jsonl`),
      JSON.stringify({ kind: 'Prompt', content: `prompt for session ${i}` }) +
        '\n'
    );
  }
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'dashboard-perf-'));
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

describe('dashboard latency at scale', () => {
  test(
    'enrichment over a large indexed store completes within budget',
    async () => {
      seedV2Store(SESSION_COUNT);
      const index = new SessionSearchIndex(join(root, 'cli'));
      try {
        await index.refresh();

        const sessions: SessionListingInput[] = Array.from(
          { length: SESSION_COUNT },
          (_, i) => ({
            sessionId: uuid(i),
            cwd: `/w/proj-${i % 40}`,
            title: 'New Session', // force the prompt-title path per row
            updatedAt: new Date(1700000000000 + i * 1000).toISOString(),
            engine: 'v2' as const,
          })
        );
        const source = {
          getDocument: (id: string, engine?: 'classic' | 'v2' | 'v3') =>
            index.getDocument(id, engine),
          isPromptless: (id: string, engine?: 'classic' | 'v2' | 'v3') =>
            index.isPromptless(id, engine),
          getPromptTitle: (id: string, engine?: 'classic' | 'v2' | 'v3') =>
            index.getPromptTitle(id, engine),
          getPromptCount: (id: string, engine?: 'classic' | 'v2' | 'v3') =>
            index.getPromptCount(id, engine),
          canInferNeverIndexed: true,
        };

        const started = performance.now();
        const { enrichedSessions } = enrichSessions(
          sessions,
          null,
          () => undefined,
          source
        );
        const elapsed = performance.now() - started;

        expect(enrichedSessions).toHaveLength(SESSION_COUNT);
        // O(n) with cached facts: ~tens of ms. The pre-fix O(n²) shape
        // (full table scan per accessor call) takes minutes at this size.
        expect(elapsed).toBeLessThan(2000);
      } finally {
        index.close();
      }
    },
    { timeout: 120_000 }
  );

  test(
    'repeat enrichment passes stay cheap (facts snapshot is reused)',
    async () => {
      seedV2Store(500);
      const index = new SessionSearchIndex(join(root, 'cli'));
      try {
        await index.refresh();
        const sessions: SessionListingInput[] = Array.from(
          { length: 500 },
          (_, i) => ({
            sessionId: uuid(i),
            cwd: '/w/proj',
            title: 'New Session',
            updatedAt: new Date().toISOString(),
            engine: 'v2' as const,
          })
        );
        const source = {
          getDocument: index.getDocument.bind(index),
          isPromptless: index.isPromptless.bind(index),
          getPromptTitle: index.getPromptTitle.bind(index),
          getPromptCount: index.getPromptCount.bind(index),
          canInferNeverIndexed: true,
        };
        // Warm pass builds the snapshot; the ten metaVersion-bump-style
        // re-passes must ride it.
        enrichSessions(sessions, null, () => undefined, source);
        const started = performance.now();
        for (let pass = 0; pass < 10; pass++) {
          enrichSessions(sessions, null, () => undefined, source);
        }
        const elapsed = performance.now() - started;
        expect(elapsed).toBeLessThan(1500);
      } finally {
        index.close();
      }
    },
    { timeout: 60_000 }
  );
});

describe('event-loop responsiveness during a cold index build', () => {
  test(
    'no macrotask stall exceeds the interaction budget',
    async () => {
      seedV2Store(SESSION_COUNT);
      const index = new SessionSearchIndex(join(root, 'cli'));
      try {
        // Probe: measure macrotask scheduling gaps while the build runs.
        // Keystroke handling rides these gaps — a slice-budget regression
        // (or a hot retry loop) shows up as multi-hundred-ms stalls.
        let worstGap = 0;
        let last = performance.now();
        let probing = true;
        const probe = (async () => {
          while (probing) {
            await new Promise((r) => setTimeout(r, 0));
            const now = performance.now();
            worstGap = Math.max(worstGap, now - last);
            last = now;
          }
        })();

        await index.refresh();
        probing = false;
        await probe;

        expect(worstGap).toBeLessThan(300);
      } finally {
        index.close();
      }
    },
    { timeout: 120_000 }
  );
});

describe('oversized V2 metadata', () => {
  test('lists a degraded row and keeps the catalog complete', async () => {
    seedV2Store(3);
    const bigId = uuid(999);
    writeFileSync(
      join(root, 'cli', `${bigId}.json`),
      JSON.stringify({
        cwd: '/w/huge',
        title: 'giant legacy session',
        history: 'x'.repeat(2 * 1024 * 1024),
      })
    );

    const result = await listAllWorkspaceSessionsFromDiskDetailed(root);

    const degraded = result.sessions.find((s) => s.sessionId === bigId);
    expect(degraded).toBeDefined();
    expect(degraded?.engine).toBe('v2');
    expect(degraded?.updatedAt).toBeTruthy();
    expect(result.complete).toBe(true);
  });
});
