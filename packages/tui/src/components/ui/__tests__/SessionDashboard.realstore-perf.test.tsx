/**
 * Per-keystroke latency against REAL adapter modules (bookmark sidecar,
 * search index, preview provider, locks) over a seeded on-disk store. The
 * mocked-adapter perf test isolates the component pipeline; this one
 * catches per-keystroke O(n) work hiding inside the adapters themselves.
 */
import { afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';

// Heavy real-adapter run, opt-in via `bun run test:perf`: global singletons
// and background I/O from this file bleed into other suites in bun's shared
// test process and time them out.
const PERF = Boolean(process.env.KIRO_PERF_TESTS);
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const mockTermSize = { width: 120, height: 40 };
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));
mock.module('../../../utils/cli-settings.js', () => ({
  readCliSettings: () => ({}),
  updateCliSetting: async () => {},
}));

import { SessionDashboard } from '../SessionDashboard.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import type { SessionListingInput } from '../../../utils/session-dashboard.js';
import { resetSessionSearchIndex } from '../../../utils/session-search.js';
import { resetSessionBookmarkStore } from '../../../utils/session-bookmarks.js';

const DOWN = '\x1b[B';
const BACKSPACE = '\x7f';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return mockTermSize.width;
  }
  get rows() {
    return mockTermSize.height;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(onInput: (data: string) => void): void {
    this.onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
  sendInput(data: string): void {
    this.onInput?.(data);
  }
}

const ROWS = 6000;
let root: string;
let previousDir: string | undefined;
let activeInstance: Instance | null = null;

function uuid(i: number): string {
  return `${String(i).padStart(8, '0')}-0000-4000-8000-${String(i).padStart(12, '0')}`;
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'dashboard-real-perf-'));
  const cliDir = join(root, 'cli');
  mkdirSync(cliDir, { recursive: true });
  for (let i = 0; i < ROWS; i++) {
    const id = uuid(i);
    writeFileSync(
      join(cliDir, `${id}.json`),
      JSON.stringify({
        cwd: `/w/proj-${i % 50}`,
        title: `conversation about topic ${i}`,
        updated_at: new Date(1700000000000 + i * 60_000).toISOString(),
      })
    );
    writeFileSync(
      join(cliDir, `${id}.jsonl`),
      JSON.stringify({ kind: 'Prompt', content: `searchable payload ${i}` }) +
        '\n'
    );
  }
  // Sidecar with realistic sparse marks so the store map is non-empty.
  writeFileSync(
    join(root, 'dashboard-meta.json'),
    JSON.stringify(
      Object.fromEntries(
        Array.from({ length: 400 }, (_, i) => [
          uuid(i * 15),
          { bookmarked: i % 2 === 0, tags: ['alpha', 'beta'] },
        ])
      )
    )
  );
  previousDir = process.env.KIRO_TEST_SESSIONS_DIR;
  // Points the bookmark sidecar at the temp store; the index and listing
  // take explicit paths below.
  process.env.KIRO_TEST_SESSIONS_DIR = root;
  resetSessionBookmarkStore();
  resetSessionSearchIndex();
});

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
  resetSessionSearchIndex();
  resetSessionBookmarkStore();
  if (previousDir === undefined) delete process.env.KIRO_TEST_SESSIONS_DIR;
  else process.env.KIRO_TEST_SESSIONS_DIR = previousDir;
  rmSync(root, { recursive: true, force: true });
});

async function settle(): Promise<void> {
  await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

function listing(): SessionListingInput[] {
  return Array.from({ length: ROWS }, (_, i) => ({
    sessionId: uuid(i),
    cwd: `/w/proj-${i % 50}`,
    title: `conversation about topic ${i}`,
    updatedAt: new Date(1700000000000 + i * 60_000).toISOString(),
    messageCount: 3,
    engine: 'v2' as const,
  }));
}

describe.skipIf(!PERF)(
  'per-keystroke latency with real adapters at 6K rows',
  () => {
    test('arrows and backspace stay within budget', async () => {
      const terminal = new MockTerminal();
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      activeInstance = render(
        <AppStoreContext.Provider value={store}>
          <SessionDashboard
            sessions={listing()}
            currentCwd="/w/proj-0"
            activeSessionId={null}
            onSelect={() => {}}
            onClose={() => {}}
            backgroundReady={true}
          />
        </AppStoreContext.Provider>,
        { terminal }
      );
      for (let i = 0; i < 10; i++) await settle();

      const measure = async (input: string, presses: number) => {
        const samples: number[] = [];
        for (let i = 0; i < presses; i++) {
          const start = performance.now();
          terminal.sendInput(input);
          await settle();
          samples.push(performance.now() - start);
        }
        samples.sort((a, b) => a - b);
        return {
          median: samples[Math.floor(samples.length / 2)]!,
          worst: samples[samples.length - 1]!,
        };
      };

      const arrows = await measure(DOWN, 20);
      // Type a query, then hold backspace through it — the reported-bad path.
      for (const c of 'searchable payload') {
        terminal.sendInput(c);
        await settle();
      }
      const backspace = await measure(BACKSPACE, 18);

      expect(arrows.median).toBeLessThan(60);
      expect(arrows.worst).toBeLessThan(400);
      expect(backspace.median).toBeLessThan(80);
      expect(backspace.worst).toBeLessThan(400);
    }, 240_000);
  }
);
