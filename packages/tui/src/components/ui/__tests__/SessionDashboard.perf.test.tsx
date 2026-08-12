/**
 * Per-keystroke latency regression test at realistic store scale (~9K rows).
 *
 * Disk-touching modules are mocked to in-memory stand-ins, so what is
 * measured is the component's own keypress → state → render pipeline. The
 * budgets are generous for CI jitter; a per-keystroke O(n) regression with
 * a heavy constant (or an accidental O(n²)) misses them by an order of
 * magnitude.
 */
import { describe, expect, mock, test, afterEach } from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';

const mockTermSize = { width: 120, height: 40 };
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));
mock.module('../../../utils/cli-settings.js', () => ({
  readCliSettings: () => ({}),
  updateCliSetting: async () => {},
}));
mock.module('../../../utils/session-lock.js', () => ({
  isSessionLocked: () => null,
  formatSessionLockOwner: () => 'PID 0',
}));
mock.module('../../../utils/session-mutations.js', () => ({
  deleteSession: () => ({ ok: true, store: 'kas' }),
  deleteLocalKasSessionWithAgent: async () => ({ ok: true, store: 'kas' }),
  gcScan: async () => ({
    candidates: [],
    skipped: { locked: 0, recent: 0, userTouched: 0, active: 0, hasParent: 0 },
  }),
  gcEmptySessions: async () => ({ deleted: 0, failed: 0, stale: 0 }),
}));
mock.module('../../../utils/session-search.js', () => ({
  getSessionSearchIndex: () => ({
    build: async () => {},
    refresh: async () => {},
    update: () => {},
    search: () => [],
    getStatus: () => ({ state: 'ready', indexed: 0, total: 0 }),
    onStatusChange: () => () => {},
    getCoverage: () => 'titles-and-prompts',
    getDocument: () => undefined,
    isPromptless: () => false,
    getPromptTitle: () => undefined,
    getPromptCount: () => 2,
  }),
}));
mock.module('../../../utils/session-preview.js', () => ({
  getSessionPreviewProvider: () => ({
    getPreview: () => null,
    invalidate: () => {},
  }),
}));
mock.module('../../../utils/session-bookmarks.js', () => ({
  getSessionBookmarkStore: () => ({
    isBookmarked: () => false,
    isArchived: () => false,
    getTags: () => [],
    getTitle: () => undefined,
    setTags: () => ({ ok: true, value: undefined }),
    setTitle: () => ({ ok: true, value: undefined }),
    toggleBookmark: () => ({ ok: true, value: true }),
    toggleArchived: () => ({ ok: true, value: true }),
    allBookmarked: () => [],
    allArchived: () => [],
    allUserTouched: () => [],
    prune: () => {},
  }),
  parseTags: () => [],
}));
mock.module('../../../utils/list-all-sessions-cli.js', () => ({
  deleteClassicSession: async () => ({ ok: true }),
}));

import { SessionDashboard } from '../SessionDashboard.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import type { SessionListingInput } from '../../../utils/session-dashboard.js';

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

let activeInstance: Instance | null = null;
afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

/** Minimal settle: microtasks + two zero-delay macrotasks (no sleeps). */
async function settle(): Promise<void> {
  await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

const ROWS = 9000;

function bigListing(): SessionListingInput[] {
  return Array.from({ length: ROWS }, (_, i) => ({
    sessionId: `00000000-0000-4000-8000-${String(i).padStart(12, '0')}`,
    cwd: `/w/proj-${i % 60}`,
    title: `conversation about topic ${i}`,
    updatedAt: new Date(1700000000000 + i * 60_000).toISOString(),
    messageCount: 3,
    engine: 'v3' as const,
  }));
}

describe('per-keystroke latency at 9K rows', () => {
  test('30 down-arrow presses render within budget', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <SessionDashboard
          sessions={bigListing()}
          currentCwd="/w/proj-0"
          activeSessionId={null}
          onSelect={() => {}}
          onClose={() => {}}
        />
      </AppStoreContext.Provider>,
      { terminal }
    );
    // Initial mount + first full pipeline pass.
    for (let i = 0; i < 10; i++) await settle();

    const perPress: number[] = [];
    for (let i = 0; i < 30; i++) {
      const start = performance.now();
      terminal.sendInput(DOWN);
      await settle();
      perPress.push(performance.now() - start);
    }
    perPress.sort((a, b) => a - b);
    const median = perPress[Math.floor(perPress.length / 2)]!;
    const worst = perPress[perPress.length - 1]!;

    // A responsive keystroke is ~16ms; budgets leave wide CI headroom. The
    // pre-fix behavior at this scale was hundreds of ms per press.
    expect(median).toBeLessThan(50);
    expect(worst).toBeLessThan(250);
  }, 120_000);
});
