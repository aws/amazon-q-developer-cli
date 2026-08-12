/**
 * Host-level per-keystroke latency: the full-screen dashboard SCREEN over a
 * seeded on-disk store with real adapters. The component-level perf tests
 * missed the host's highlight side effects (preview + turn-tree file I/O on
 * every cursor settle), which is exactly what this exercises: presses are
 * human-paced so the host's debounced effects actually fire between them.
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

const mockTermSize = { width: 140, height: 42 };
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));
mock.module('../../../utils/cli-settings.js', () => ({
  readCliSettings: () => ({}),
  updateCliSetting: async () => {},
}));
// The classic listing shells out to the CLI binary; irrelevant here.
mock.module('../../../utils/list-all-sessions-cli.js', () => ({
  listAllSessionsAllCwds: async () => ({
    ok: false,
    error: 'not under test',
  }),
  deleteClassicSession: async () => ({ ok: true }),
}));

import { SessionDashboardScreen } from '../SessionDashboardScreen.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import type { SessionListingInput } from '../../../utils/session-dashboard.js';
import { resetSessionSearchIndex } from '../../../utils/session-search.js';
import { resetSessionBookmarkStore } from '../../../utils/session-bookmarks.js';
import { resetAllWorkspaceSessionsCache } from '../../../utils/all-workspace-sessions.js';

const DOWN = '\x1b[B';

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

const ROWS = 2500;
const SUBEXECUTIONS = 24;
let root: string;
let previousDir: string | undefined;
let activeInstance: Instance | null = null;

function kasId(i: number): string {
  return `sess_${String(i).padStart(8, '0')}-0000-4000-8000-${String(i).padStart(12, '0')}`;
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'dashboard-screen-perf-'));
  // KAS-native store: hash dirs with session dirs, transcripts, and enough
  // sub-execution files that a per-keystroke turn-tree build is measurable.
  for (let i = 0; i < ROWS; i++) {
    const id = kasId(i);
    const dir = join(root, `hash${i % 32}`, id);
    mkdirSync(join(dir, 'sub-executions'), { recursive: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({
        id,
        title: `kas conversation ${i}`,
        workspacePaths: [`/w/proj-${i % 40}`],
        lastModifiedAt: new Date(1700000000000 + i * 60_000).toISOString(),
      })
    );
    writeFileSync(
      join(dir, 'messages.jsonl'),
      Array.from({ length: 30 }, (_, t) =>
        JSON.stringify({
          type: 'user',
          text: `turn ${t} of session ${i} with some payload text`,
        })
      ).join('\n') + '\n'
    );
    // Only spread sub-executions across the first rows the cursor will
    // actually visit — keeps seeding time sane.
    if (i < 40) {
      for (let s = 0; s < SUBEXECUTIONS; s++) {
        writeFileSync(
          join(dir, 'sub-executions', `${String(s).padStart(3, '0')}.jsonl`),
          Array.from({ length: 20 }, (_, t) =>
            JSON.stringify({
              type: 'tool_use',
              name: `tool-${t}`,
              summary: 'sub execution payload',
            })
          ).join('\n') + '\n'
        );
      }
    }
  }
  previousDir = process.env.KIRO_TEST_SESSIONS_DIR;
  process.env.KIRO_TEST_SESSIONS_DIR = root;
  resetSessionBookmarkStore();
  resetSessionSearchIndex();
  resetAllWorkspaceSessionsCache();
});

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
  resetSessionSearchIndex();
  resetSessionBookmarkStore();
  resetAllWorkspaceSessionsCache();
  if (previousDir === undefined) delete process.env.KIRO_TEST_SESSIONS_DIR;
  else process.env.KIRO_TEST_SESSIONS_DIR = previousDir;
  rmSync(root, { recursive: true, force: true });
});

function listing(): SessionListingInput[] {
  return Array.from({ length: ROWS }, (_, i) => ({
    sessionId: kasId(i),
    cwd: `/w/proj-${i % 40}`,
    title: `kas conversation ${i}`,
    updatedAt: new Date(1700000000000 + i * 60_000).toISOString(),
    messageCount: 3,
    engine: 'v3' as const,
  }));
}

describe.skipIf(!PERF)(
  'screen-level keystroke latency (preview hidden)',
  () => {
    test(
      'human-paced arrow presses do not stall the loop on highlight side effects',
      async () => {
        const terminal = new MockTerminal();
        const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
        store.getState().setShowSessionDashboard(true, listing(), 'slash');
        activeInstance = render(
          <AppStoreContext.Provider value={store}>
            <SessionDashboardScreen />
          </AppStoreContext.Provider>,
          { terminal }
        );
        // Let mount + initial refresh settle.
        await new Promise((r) => setTimeout(r, 600));

        // Pace presses ~200ms apart so the host's 150ms-debounced highlight
        // effects FIRE between presses (they are invisible to rapid-fire
        // tests: each press clears the previous debounce timer). Probe the
        // event loop the whole time; the probe gap IS the felt keystroke lag.
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

        for (let i = 0; i < 12; i++) {
          terminal.sendInput(DOWN);
          await new Promise((r) => setTimeout(r, 200));
        }
        probing = false;
        await probe;

        expect(worstGap).toBeLessThan(120);
      },
      { timeout: 240_000 }
    );
  }
);
