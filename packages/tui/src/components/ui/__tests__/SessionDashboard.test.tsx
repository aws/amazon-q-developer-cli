/**
 * Component tests for the SessionDashboard keyboard state machine: the
 * delete-confirmation flow (destructive!), expand/collapse, controls,
 * search, and close paths — driven through a mock terminal against the
 * real component.
 */
import { afterEach, describe, expect, test } from 'bun:test';
import { mock } from 'bun:test';
import { vi } from 'vitest';
import React from 'react';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { render, type Instance, type Terminal } from 'twinki';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '../../../hooks/useTerminalSize.js',
  '../../../utils/cli-settings.js',
  '../../../utils/session-lock.js',
  '../../../utils/session-mutations.js',
  '../../../utils/session-search.js',
  '../../../utils/session-preview.js',
  '../../../utils/session-bookmarks.js',
  '../../../utils/list-all-sessions-cli.js',
]);

const mockTermSize = { width: 120, height: 40 };
mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...mockTermSize }),
}));

let persistedSettings: Record<string, unknown> = {};
mock.module('../../../utils/cli-settings.js', () => ({
  readCliSettings: () => ({ ...persistedSettings }),
  updateCliSetting: async (key: string, value: unknown) => {
    persistedSettings[key] = value;
  },
}));

// The dashboard touches on-disk stores through these modules — every one
// is replaced with an in-memory stand-in so tests never read ~/.kiro.
let lockedIds = new Set<string>();
let lockedIdentities = new Set<string>();
mock.module(
  '../../../utils/session-lock.js',
  () =>
    ({
      isSessionLocked: (
        id: string,
        identity?: {
          engine?: 'classic' | 'v2' | 'v3';
          source?: 'local' | 'remote';
        }
      ) =>
        lockedIds.has(id) ||
        lockedIdentities.has(
          `${identity?.engine ?? 'v3'}:${identity?.source ?? 'local'}:${id}`
        )
          ? {
              locked: true as const,
              pid: 4242,
              startedAt: '',
              state: 'live' as const,
            }
          : null,
      formatSessionLockOwner: (info: { pid?: number }) =>
        info.pid == null ? 'an unreadable lock' : `PID ${info.pid}`,
    }) satisfies Partial<typeof import('../../../utils/session-lock.js')>
);

let deleteOutcome: { ok: boolean; reason?: string } = { ok: true };
const deletedIds: string[] = [];
let gcCandidates: Array<{
  sessionId: string;
  store: 'v2' | 'kas';
  workspace: string;
}> = [];
const gcDeleted: string[][] = [];
mock.module('../../../utils/session-mutations.js', () => ({
  deleteSession: (id: string) => {
    deletedIds.push(id);
    return deleteOutcome.ok
      ? { ok: true, store: 'kas' }
      : { ok: false, reason: deleteOutcome.reason ?? 'error' };
  },
  deleteLocalKasSessionWithAgent: async (
    id: string,
    _activeId: string | null | undefined,
    agentDelete: (sessionId: string) => Promise<boolean>
  ) => {
    if (await agentDelete(id)) return { ok: true, store: 'kas' as const };
    deletedIds.push(id);
    return deleteOutcome.ok
      ? { ok: true, store: 'kas' as const }
      : { ok: false, reason: deleteOutcome.reason ?? 'error' };
  },
  gcScan: async () => ({
    candidates: gcCandidates,
    skipped: { locked: 0, recent: 0, userTouched: 0, active: 0, hasParent: 0 },
  }),
  gcEmptySessions: async (candidates: readonly { sessionId: string }[]) => {
    gcDeleted.push(candidates.map((c) => c.sessionId));
    return { deleted: candidates.length, failed: 0, stale: 0 };
  },
}));

// Pick ties each mocked method to the real class signature — a rename or
// shape change on the real index fails the typecheck here instead of
// silently passing against a stale mock.
type IndexMock = Pick<
  import('../../../utils/session-search.js').SessionSearchIndex,
  | 'build'
  | 'refresh'
  | 'update'
  | 'search'
  | 'getStatus'
  | 'onStatusChange'
  | 'getCoverage'
  | 'getDocument'
  | 'isPromptless'
  | 'getPromptTitle'
  | 'getPromptCount'
  | 'abort'
>;
type IndexStatus = ReturnType<IndexMock['getStatus']>;
let indexSearchImpl: IndexMock['search'] = () => [];
let indexStatus: IndexStatus = {
  state: 'idle',
  indexed: 0,
  total: 0,
};
const indexStatusListeners = new Set<(status: IndexStatus) => void>();
let indexPromptCounts = new Map<string, number>();
const indexMock: IndexMock = {
  build: async () => {},
  refresh: async () => {},
  update: () => {},
  search: (query: string, limit: number) => indexSearchImpl(query, limit),
  getStatus: () => ({ ...indexStatus }),
  onStatusChange: (listener) => {
    indexStatusListeners.add(listener);
    return () => indexStatusListeners.delete(listener);
  },
  getCoverage: () => 'titles-and-prompts',
  getDocument: () => undefined,
  isPromptless: () => false,
  getPromptTitle: () => undefined,
  getPromptCount: (id: string) => indexPromptCounts.get(id),
  abort: () => {},
};
mock.module('../../../utils/session-search.js', () => ({
  getSessionSearchIndex: () => indexMock,
}));

let previewByIdentity = new Map<string, SessionPreview>();
mock.module('../../../utils/session-preview.js', () => ({
  getSessionPreviewProvider: () => ({
    getPreview: (
      sessionId: string,
      engine?: string,
      source: 'local' | 'remote' = 'local'
    ) =>
      previewByIdentity.get(`${sessionId}:${engine ?? 'v3'}:${source}`) ?? null,
    invalidate: () => {},
  }),
}));

let userTouched: string[] = [];
let bookmarkedIds = new Set<string>();
let archivedIds = new Set<string>();
let titleOverrides = new Map<string, string>();
let bookmarkMutationFails = false;
mock.module('../../../utils/session-bookmarks.js', () => ({
  getSessionBookmarkStore: () => ({
    isBookmarked: (id: string) => bookmarkedIds.has(id),
    isArchived: (id: string) => archivedIds.has(id),
    getTags: () => [],
    getTitle: (id: string) => titleOverrides.get(id),
    setTags: () =>
      bookmarkMutationFails
        ? { ok: false as const, reason: 'write-failed' as const }
        : { ok: true as const, value: undefined },
    setTitle: (id: string, title: string) => {
      if (bookmarkMutationFails) {
        return { ok: false as const, reason: 'write-failed' as const };
      }
      titleOverrides.set(id, title);
      return { ok: true as const, value: undefined };
    },
    toggleBookmark: (id: string) => {
      if (bookmarkMutationFails) {
        return { ok: false as const, reason: 'write-failed' as const };
      }
      if (bookmarkedIds.has(id)) {
        bookmarkedIds.delete(id);
        return { ok: true as const, value: false };
      }
      bookmarkedIds.add(id);
      return { ok: true as const, value: true };
    },
    toggleArchived: (id: string) => {
      if (bookmarkMutationFails) {
        return { ok: false as const, reason: 'write-failed' as const };
      }
      if (archivedIds.has(id)) {
        archivedIds.delete(id);
        return { ok: true as const, value: false };
      }
      archivedIds.add(id);
      return { ok: true as const, value: true };
    },
    allBookmarked: () => [...bookmarkedIds],
    allArchived: () => [],
    allUserTouched: () => userTouched,
    prune: () => {},
  }),
  parseTags: (s: string) =>
    s
      .split(',')
      .map((t) => t.trim())
      .filter(Boolean),
}));

mock.module(
  '../../../utils/list-all-sessions-cli.js',
  () =>
    ({
      deleteClassicSession: async () => ({ ok: true as const }),
    }) satisfies Partial<
      typeof import('../../../utils/list-all-sessions-cli.js')
    >
);

import { stripAnsiQuick } from '../../../lite/render.js';
import { SessionDashboard } from '../SessionDashboard.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import type { SessionListingInput } from '../../../utils/session-dashboard.js';
import type { SessionPreview } from '../../../utils/session-preview.js';
import { visibleWidth } from '../../../utils/text-width.js';

const DOWN = '\x1b[B';
const LEFT = '\x1b[D';
const ESC = '\x1b';
const ENTER = '\r';
const CTRL_B = '\x02';
const CTRL_C = '\x03';
const CTRL_D = '\x04';
const CTRL_F = '\x06';
const CTRL_G = '\x07';
const CTRL_X = '\x18';
const CTRL_P = '\x10';
const CTRL_R = '\x12';
const CTRL_T = '\x14';
const CTRL_U = '\x15';
const CTRL_W = '\x17';
const RIGHT = '\x1b[C';

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
  /** Assertions read the latest full frame, not the accumulated stream. */
  reset(): void {
    this.output = '';
  }
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
  lockedIds = new Set();
  lockedIdentities = new Set();
  deleteOutcome = { ok: true };
  deletedIds.length = 0;
  gcCandidates = [];
  gcDeleted.length = 0;
  userTouched = [];
  bookmarkedIds = new Set();
  archivedIds = new Set();
  titleOverrides = new Map();
  bookmarkMutationFails = false;
  persistedSettings = {};
  indexSearchImpl = () => [];
  indexStatus = { state: 'idle', indexed: 0, total: 0 };
  indexStatusListeners.clear();
  indexPromptCounts = new Map();
  previewByIdentity = new Map();
  mockTermSize.width = 120;
  mockTermSize.height = 40;
});

async function flush(): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 40));
  }
}

/** Everything written so far, ANSI stripped — for "did X ever render". */
function textSoFar(terminal: MockTerminal): string {
  return stripAnsiQuick(terminal.output);
}

/** The CURRENT screen only (forced full redraw) — for "is X gone now". */
async function currentFrame(terminal: MockTerminal): Promise<string> {
  terminal.reset();
  activeInstance!.clear();
  await flush();
  return stripAnsiQuick(terminal.output);
}

function hardwareCursorCommand(terminal: MockTerminal): {
  direction?: 'A' | 'B';
  distance?: number;
  column: number;
} {
  const escape = String.fromCharCode(27);
  const cursorCommand = new RegExp(
    `(?:${escape}\\[(\\d+)([AB]))?${escape}\\[(\\d+)G`,
    'g'
  );
  const commands = [...terminal.output.matchAll(cursorCommand)];
  const match = commands.at(-1);
  if (!match) throw new Error('No hardware cursor command was rendered');
  return {
    direction: match[2] as 'A' | 'B' | undefined,
    distance: match[1] ? Number(match[1]) : undefined,
    column: Number(match[3]) - 1,
  };
}

function session(
  id: string,
  title: string,
  opts: Partial<SessionListingInput> = {}
): SessionListingInput {
  return {
    sessionId: id,
    cwd: '/w/proj',
    title,
    updatedAt: '2026-01-01T00:00:00.000Z',
    messageCount: 3,
    ...opts,
  } as SessionListingInput;
}

function mount(
  sessions: SessionListingInput[],
  opts: {
    activeSessionId?: string;
    hostedPreview?: boolean;
    catalogIncomplete?: boolean;
    kiro?: Kiro;
  } = {}
) {
  const terminal = new MockTerminal();
  const onSelect = vi.fn();
  const onClose = vi.fn();
  const onRefresh = vi.fn();
  const onTogglePreview = vi.fn();
  const store = createAppStore({
    kiro: opts.kiro ?? new Kiro(),
    agentEngine: 'kas',
  });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <SessionDashboard
        sessions={sessions}
        currentCwd="/w/proj"
        activeSessionId={opts.activeSessionId ?? null}
        catalogIncomplete={opts.catalogIncomplete}
        onTogglePreview={opts.hostedPreview ? onTogglePreview : undefined}
        onSelect={onSelect}
        onRefresh={onRefresh}
        onClose={onClose}
      />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  return { terminal, onSelect, onClose, onRefresh, onTogglePreview };
}

describe('hardware cursor ownership', () => {
  test('follows session rows and the expander during list navigation', async () => {
    const sessions = Array.from({ length: 7 }, (_, i) =>
      session(`row-${i}`, `row number ${i}`, { cwd: '/w/other' })
    );
    const { terminal } = mount(sessions);
    await flush();
    const rowColumn = hardwareCursorCommand(terminal).column;

    terminal.reset();
    terminal.sendInput(DOWN);
    await flush();
    const secondRowCursor = hardwareCursorCommand(terminal);
    expect(secondRowCursor.column).toBe(rowColumn);
    expect(secondRowCursor.distance).toBeGreaterThan(0);

    for (let i = 0; i < 4; i++) terminal.sendInput(DOWN);
    await flush();
    const expanderCursor = hardwareCursorCommand(terminal);
    expect(expanderCursor.column).toBe(rowColumn);
    expect(expanderCursor.distance).toBeGreaterThan(0);
    terminal.sendInput(ENTER);
    await flush();
    expect(await currentFrame(terminal)).toContain('row number 6');
  });

  test('moves between search, editors, controls, and the list owner', async () => {
    const { terminal } = mount([session('focus', 'focus target')]);
    await flush();
    const rowColumn = hardwareCursorCommand(terminal).column;

    terminal.reset();
    terminal.sendInput('f');
    await flush();
    expect(hardwareCursorCommand(terminal).column).toBeGreaterThan(rowColumn);

    terminal.reset();
    terminal.sendInput(DOWN);
    await flush();
    expect(hardwareCursorCommand(terminal).column).toBe(rowColumn);

    terminal.reset();
    terminal.sendInput(CTRL_T);
    await flush();
    const tagColumn = hardwareCursorCommand(terminal).column;
    expect(tagColumn).toBeGreaterThan(rowColumn);

    terminal.sendInput(ESC);
    await flush();
    terminal.reset();
    terminal.sendInput(CTRL_R);
    await flush();
    expect(hardwareCursorCommand(terminal).column).toBeGreaterThan(tagColumn);

    terminal.sendInput(ESC);
    await flush();
    terminal.reset();
    terminal.sendInput(CTRL_F);
    await flush();
    expect(hardwareCursorCommand(terminal)).toMatchObject({
      direction: 'A',
      column: rowColumn,
    });
  });
  test('moves the hardware cursor to delete and switch modal actions', async () => {
    const { terminal } = mount([
      session('delete-me', 'delete me'),
      session('switch-me', 'switch me', { cwd: process.cwd() }),
    ]);
    await flush();

    terminal.reset();
    terminal.sendInput('delete');
    await flush();
    const deleteSearchCursor = hardwareCursorCommand(terminal);
    expect(deleteSearchCursor.column).toBeGreaterThan(1);

    terminal.reset();
    terminal.sendInput(CTRL_D);
    await flush();
    expect(hardwareCursorCommand(terminal).column).toBe(1);
    terminal.sendInput(ESC);
    await flush();

    terminal.sendInput(CTRL_U);
    terminal.sendInput('switch');
    await flush();
    const switchSearchCursor = hardwareCursorCommand(terminal);
    expect(switchSearchCursor.column).toBeGreaterThan(1);

    terminal.reset();
    terminal.sendInput(ENTER);
    await flush();
    expect(textSoFar(terminal)).toContain('Switch directory and load session?');
    expect(hardwareCursorCommand(terminal).column).toBe(1);
  });
});

describe('cloud session rendering', () => {
  test('tags cloud rows with a [cloud] marker before the title', async () => {
    const { terminal } = mount([
      session('cloud-1', 'cloud session', { source: 'remote' }),
    ]);
    await flush();

    expect(await currentFrame(terminal)).toMatch(/\[cloud] {2}cloud session/);
  });
});

describe('tangent session rendering', () => {
  test('nests a tangent under its parent with a [tangent] marker', async () => {
    const { terminal } = mount([
      session('root-1', 'what does tangent do'),
      session('tangent-1', 'tangent-1', {
        parentSessionId: 'root-1',
        createdReason: 'tangent',
      } as Partial<SessionListingInput>),
    ]);
    await flush();

    const frame = await currentFrame(terminal);
    // Parent visible, and the tangent rendered as its own visible row tagged
    // [tangent] (resumable), not hidden away.
    expect(frame).toContain('what does tangent do');
    expect(frame).toMatch(/\[tangent] .*tangent-1/);
  });
});

describe('catalog status rendering', () => {
  test('warns when a partial refresh retained cached sessions', async () => {
    const { terminal } = mount([session('cached', 'cached session')], {
      catalogIncomplete: true,
    });
    await flush();

    expect(await currentFrame(terminal)).toContain(
      'some sessions couldn\u2019t be loaded'
    );
  });
});

describe('preview identity', () => {
  test('never pairs a stale preview with a different physical row', async () => {
    previewByIdentity.set('shared:v2:local', {
      sessionId: 'shared',
      summary: {
        title: 'V2 preview',
        firstPrompt: 'V2 PREVIEW ONLY',
        toolsSummary: [],
        turnCount: 1,
        isComplete: true,
        createdAt: '',
        updatedAt: '',
      },
      recentMessages: [],
    });
    previewByIdentity.set('shared:v3:local', {
      sessionId: 'shared',
      summary: {
        title: 'V3 preview',
        firstPrompt: 'V3 PREVIEW ONLY',
        toolsSummary: [],
        turnCount: 1,
        isComplete: true,
        createdAt: '',
        updatedAt: '',
      },
      recentMessages: [],
    });
    const { terminal } = mount([
      session('shared', 'V2 row', { engine: 'v2' }),
      session('shared', 'V3 row', { engine: 'v3' }),
    ]);
    await new Promise((resolve) => setTimeout(resolve, 180));
    await flush();
    expect(await currentFrame(terminal)).toContain('V2 PREVIEW ONLY');

    terminal.sendInput(DOWN);
    const duringTransition = await currentFrame(terminal);
    const currentPreview = duringTransition.slice(
      duringTransition.lastIndexOf('Preview of "V3 row"')
    );
    expect(currentPreview).not.toContain('V2 PREVIEW ONLY');

    await new Promise((resolve) => setTimeout(resolve, 80));
    await flush();
    expect(await currentFrame(terminal)).toContain('V3 PREVIEW ONLY');
  });

  test('does not show a local preview for a same-id remote row', async () => {
    previewByIdentity.set('shared:v3:local', {
      sessionId: 'shared',
      summary: {
        title: 'Local preview',
        firstPrompt: 'LOCAL PREVIEW ONLY',
        toolsSummary: [],
        turnCount: 1,
        isComplete: true,
        createdAt: '',
        updatedAt: '',
      },
      recentMessages: [],
    });
    const { terminal } = mount([
      session('shared', 'Remote row', {
        engine: 'v3',
        source: 'remote',
      }),
      session('shared', 'Local row', { engine: 'v3', source: 'local' }),
    ]);

    await new Promise((resolve) => setTimeout(resolve, 180));
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain('Preview of "Remote row"');
    expect(frame).not.toContain('LOCAL PREVIEW ONLY');
  });
});

describe('delete confirmation flow', () => {
  test('ctrl+d stages a confirmation; y deletes and refreshes', async () => {
    const { terminal, onRefresh } = mount([
      session('s-1', 'first session'),
      session('s-2', 'second session'),
    ]);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    expect(textSoFar(terminal)).toContain('Delete "first session"?');
    terminal.sendInput('y');
    await flush();
    expect(deletedIds).toEqual(['s-1']);
    expect(onRefresh).toHaveBeenCalled();
    expect(textSoFar(terminal)).toContain('Deleted "first session"');
  });

  test('cloud placement deletes through the remote store even without source metadata', async () => {
    const kiro = new Kiro();
    const deleteSessionById = vi
      .spyOn(kiro, 'deleteSessionById')
      .mockResolvedValue(true);
    const { terminal } = mount(
      [
        session('cloud-1', 'cloud session', {
          cwd: '/sandbox',
          executionTarget: { kind: 'cloud-sandbox' },
        }),
      ],
      { kiro }
    );
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    terminal.sendInput('y');
    await flush();
    expect(deleteSessionById).toHaveBeenCalledWith('cloud-1', {
      source: 'remote',
    });
    expect(deletedIds).toEqual([]);
  });

  test('remote source deletes through the remote store without placement metadata', async () => {
    const kiro = new Kiro();
    const deleteSessionById = vi
      .spyOn(kiro, 'deleteSessionById')
      .mockResolvedValue(true);
    const { terminal } = mount(
      [session('remote-1', 'remote session', { source: 'remote' })],
      { kiro }
    );
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    terminal.sendInput('y');
    await flush();
    expect(deleteSessionById).toHaveBeenCalledWith('remote-1', {
      source: 'remote',
    });
    expect(deletedIds).toEqual([]);
  });

  test('any key other than y cancels the staged delete', async () => {
    const { terminal } = mount([session('s-1', 'first session')]);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    expect(textSoFar(terminal)).toContain('Delete "first session"?');
    terminal.sendInput('n');
    await flush();
    expect(deletedIds).toEqual([]);
    // The confirmation is gone; the list is intact.
    expect(await currentFrame(terminal)).not.toContain('y delete');
  });

  test('enter never confirms a staged delete', async () => {
    const { terminal, onSelect } = mount([session('s-1', 'first session')]);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(deletedIds).toEqual([]);
    // Enter was consumed by the modal — it must not open the session either.
    expect(onSelect).not.toHaveBeenCalled();
  });

  test('ctrl+p is consumed by a modal before toggling hosted preview', async () => {
    const { terminal, onTogglePreview } = mount(
      [session('s-1', 'first session')],
      { hostedPreview: true }
    );
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();

    terminal.sendInput(CTRL_P);
    await flush();
    expect(onTogglePreview).not.toHaveBeenCalled();
    expect(await currentFrame(terminal)).not.toContain('y delete');

    terminal.sendInput(CTRL_P);
    await flush();
    expect(onTogglePreview).toHaveBeenCalledTimes(1);
  });

  test('the active session cannot be staged for deletion', async () => {
    const { terminal } = mount([session('s-1', 'the active one')], {
      activeSessionId: 's-1',
    });
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    expect(textSoFar(terminal)).toContain('Cannot delete the active session');
    expect(deletedIds).toEqual([]);
  });

  test('a KAS alias of the active session cannot be staged', async () => {
    const { terminal } = mount(
      [session('s-1', 'the active alias', { engine: 'v3' })],
      { activeSessionId: 'sess_s-1' }
    );
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    expect(textSoFar(terminal)).toContain('Cannot delete the active session');
    expect(deletedIds).toEqual([]);
  });

  test('a session locked by another terminal cannot be staged', async () => {
    lockedIds = new Set(['s-1']);
    const { terminal } = mount([session('s-1', 'locked elsewhere')]);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    expect(textSoFar(terminal)).toContain(
      'Cannot delete: session is open in another terminal (PID 4242)'
    );
    expect(deletedIds).toEqual([]);
  });

  test('a failed delete reports the reason instead of claiming success', async () => {
    deleteOutcome = { ok: false, reason: 'recent' };
    const { terminal } = mount([session('s-1', 'fresh session')]);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();
    terminal.sendInput('y');
    await flush();
    expect(textSoFar(terminal)).toContain(
      'Not deleted — in use moments ago, try again in a few minutes'
    );
  });
});

describe('resume and close paths', () => {
  test('enter resumes the selected classic session with its telemetry engine', async () => {
    const { terminal, onSelect } = mount([
      session('s-1', 'resume me', { engine: 'classic' }),
    ]);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('s-1', 'local', undefined, {
      via: 'browse',
      engine: 'classic',
    });
  });

  test('cloud placement resumes through the remote store even without source metadata', async () => {
    const { terminal, onSelect } = mount([
      session('cloud-1', 'cloud session', {
        cwd: '/sandbox',
        executionTarget: { kind: 'cloud-sandbox' },
      }),
    ]);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('cloud-1', 'cloud', undefined, {
      via: 'browse',
      engine: 'v3',
    });
  });

  test('remote source resumes remotely without placement metadata', async () => {
    const { terminal, onSelect } = mount([
      session('remote-1', 'remote session', { source: 'remote' }),
    ]);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('remote-1', 'cloud', undefined, {
      via: 'browse',
      engine: 'v3',
    });
  });

  test('a local same-id lock does not block a remote resume', async () => {
    lockedIdentities.add('v3:local:shared');
    const { terminal, onSelect } = mount([
      session('shared', 'remote session', {
        engine: 'v3',
        source: 'remote',
      }),
    ]);

    await flush();
    terminal.sendInput(ENTER);
    await flush();

    expect(onSelect).toHaveBeenCalledWith('shared', 'cloud', undefined, {
      via: 'browse',
      engine: 'v3',
    });
  });

  test('enter on the active session shows a notice instead of resuming', async () => {
    const { terminal, onSelect } = mount([session('s-1', 'active session')], {
      activeSessionId: 's-1',
    });
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).not.toHaveBeenCalled();
    expect(textSoFar(terminal)).toContain(
      'Session is already open in this terminal'
    );
  });

  test('a cross-workspace resume stages a directory-switch confirmation', async () => {
    const otherWorkspace = process.cwd();
    const { terminal, onSelect } = mount([
      session('s-1', 'elsewhere', {
        cwd: otherWorkspace,
        engine: 'v2',
      }),
    ]);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(textSoFar(terminal)).toContain('Switch directory and load session?');
    expect(onSelect).not.toHaveBeenCalled();
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).toHaveBeenCalledWith('s-1', 'local', otherWorkspace, {
      via: 'browse',
      engine: 'v2',
    });
  });

  test('a missing workspace refuses to load against the current directory', async () => {
    const { terminal, onSelect } = mount([
      session('s-1', 'missing workspace', {
        cwd: '/definitely/missing/session-dashboard-workspace',
      }),
    ]);
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    const frame = await currentFrame(terminal);
    expect(frame).toContain('Cannot open: session directory no longer exists');
    expect(frame).not.toContain('load in current directory');
    terminal.sendInput(ENTER);
    await flush();
    expect(onSelect).not.toHaveBeenCalled();
  });

  test('esc closes the dashboard; with a search active it clears first', async () => {
    const { terminal, onClose } = mount([session('s-1', 'anything')]);
    await flush();
    terminal.sendInput('a'); // type-to-search
    await flush();
    terminal.sendInput(ESC);
    await flush();
    expect(onClose).not.toHaveBeenCalled(); // first esc cleared the query
    terminal.sendInput(ESC);
    await flush();
    expect(onClose).toHaveBeenCalled();
  });

  test('ctrl+c closes from any sub-state', async () => {
    const { terminal, onClose } = mount([session('s-1', 'anything')]);
    await flush();
    terminal.sendInput(CTRL_D); // staged delete open
    await flush();
    terminal.sendInput(CTRL_C);
    await flush();
    expect(onClose).toHaveBeenCalled();
    expect(deletedIds).toEqual([]);
  });
});

describe('expand / collapse', () => {
  test('an ordinary group caps at 5 with an expander; enter reveals the rest', async () => {
    const others = Array.from({ length: 7 }, (_, i) =>
      session(`o-${i}`, `other number ${i}`, { cwd: '/w/other' })
    );
    const { terminal } = mount(others);
    await flush();
    const capped = await currentFrame(terminal);
    expect(capped).toContain('other number 4');
    expect(capped).not.toContain('other number 6'); // behind the cap
    expect(capped).toContain('more (');
    for (let i = 0; i < 5; i++) terminal.sendInput(DOWN); // onto the expander
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(await currentFrame(terminal)).toContain('other number 6');
  });

  test('← collapses the group the cursor is in', async () => {
    const others = Array.from({ length: 9 }, (_, i) =>
      session(`o-${i}`, `other number ${i}`, { cwd: '/w/other' })
    );
    const { terminal } = mount(others);
    await flush();
    // Walk onto the other group's expander (5-cap → expander at index 5).
    for (let i = 0; i < 5; i++) terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(ENTER); // expand
    await flush();
    const expanded = await currentFrame(terminal);
    expect(expanded).toContain('to collapse');
    expect(expanded).toContain('other number 8'); // beyond the cap
    terminal.sendInput(LEFT); // collapse from inside the group
    await flush();
    const collapsed = await currentFrame(terminal);
    expect(collapsed).not.toContain('other number 8');
    expect(collapsed).toContain('more (');
  });
});

describe('filter and group controls', () => {
  test('ctrl+f opens the filter control with its own hint line', async () => {
    const { terminal } = mount([session('s-1', 'x')]);
    await flush();
    terminal.sendInput(CTRL_F);
    await flush();
    expect(textSoFar(terminal)).toContain('select');
    expect(textSoFar(terminal)).toContain('done');
    expect(textSoFar(terminal)).toContain('esc cancel');
  });

  test('cycling the filter wraps through the remaining options', async () => {
    const { terminal } = mount([session('s-1', 'filter cycle target')]);
    await flush();
    terminal.sendInput(CTRL_F);
    await flush();
    // all → current → bookmarked → empties → all (archived is gone)
    for (let i = 0; i < 4; i++) {
      terminal.sendInput(RIGHT);
      await flush();
    }
    terminal.sendInput(ENTER);
    await flush();
    expect(await currentFrame(terminal)).toContain('filter: all');
    expect(await currentFrame(terminal)).toContain('filter cycle target');
  });

  test('ctrl+x clears the filter but keeps the search text', async () => {
    const { terminal } = mount([
      session('s-1', 'findable row'),
      session('s-2', 'other row'),
    ]);
    await flush();
    terminal.sendInput(CTRL_F);
    await flush();
    terminal.sendInput(RIGHT); // → current workspace
    terminal.sendInput(ENTER);
    await flush();
    for (const c of 'findable') terminal.sendInput(c);
    await flush();
    terminal.sendInput(CTRL_X);
    await flush();
    const frame = await currentFrame(terminal);
    expect(frame).toContain('filter: all');
    expect(frame).toContain('search: findable');
  });

  test('ctrl+g cycles grouping without losing rows', async () => {
    const { terminal } = mount([session('s-1', 'group cycling target')]);
    await flush();
    terminal.sendInput(CTRL_G);
    await flush();
    terminal.sendInput(RIGHT); // workspace → recency
    await flush();
    terminal.sendInput(ENTER);
    await flush();
    expect(await currentFrame(terminal)).toContain('group cycling target');
  });
});

describe('cleanup review and bulk GC', () => {
  // Seven husks in a non-current workspace: 5-cap → expander row at nav
  // index 5. ^d on the expander opens the cleanup review scoped to that
  // workspace; ^d again stages the bulk GC; y confirms.
  const husks = Array.from({ length: 7 }, (_, i) =>
    session(`husk-${i}`, `husk number ${i}`, { cwd: '/w/other' })
  );
  const candidatesFor = (ids: string[]) =>
    ids.map((sessionId) => ({
      sessionId,
      store: 'kas' as const,
      workspace: '/w/other',
    }));

  async function stageGc(terminal: MockTerminal) {
    for (let i = 0; i < 5; i++) terminal.sendInput(DOWN); // onto the expander
    await flush();
    terminal.sendInput(CTRL_D); // open cleanup review
    await flush();
    terminal.sendInput(CTRL_D); // stage the bulk GC
    await flush();
  }

  test('cleanup review matches the candidate physical store for same-id rows', async () => {
    gcCandidates = [
      { sessionId: 'shared', store: 'kas', workspace: '/w/other' },
    ];
    const rows = [
      session('shared', 'V3 cleanup candidate', {
        cwd: '/w/other',
        engine: 'v3',
        updatedAt: '2026-08-07T00:00:00.000Z',
      }),
      session('shared', 'V2 same-id session', {
        cwd: '/w/other',
        engine: 'v2',
        updatedAt: '2026-08-06T00:00:00.000Z',
      }),
      ...Array.from({ length: 5 }, (_, index) =>
        session(`ordinary-${index}`, `ordinary ${index}`, {
          cwd: '/w/other',
          updatedAt: `2026-08-0${5 - index}T00:00:00.000Z`,
        })
      ),
    ];
    const { terminal } = mount(rows);
    await flush();

    for (let i = 0; i < 5; i++) terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain('V3 cleanup candidate');
    expect(frame).not.toContain('V2 same-id session');
  });

  test('^d on the expander opens the review; ^d + y bulk-deletes the candidates', async () => {
    gcCandidates = candidatesFor(husks.map((h) => h.sessionId));
    const { terminal, onRefresh } = mount(husks);
    await flush();
    await stageGc(terminal);
    expect(textSoFar(terminal)).toContain('cleanup review:');
    expect(textSoFar(terminal)).toContain('Delete 7 empty sessions?');
    terminal.sendInput('y');
    await flush();
    expect(gcDeleted).toHaveLength(1);
    expect(gcDeleted[0]).toEqual(husks.map((h) => h.sessionId));
    expect(onRefresh).toHaveBeenCalled();
  });

  test('typing inside cleanup review never swaps the list to search results', async () => {
    gcCandidates = candidatesFor(husks.map((h) => h.sessionId));
    const { terminal } = mount([
      ...husks,
      session('unrelated', 'unrelated searchable row', { cwd: '/w/proj' }),
    ]);
    await flush();
    // The /w/proj row precedes the /w/other block, so the expander sits one
    // row further down than in the husks-only fixtures.
    for (let i = 0; i < 6; i++) terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(CTRL_D); // open cleanup review
    await flush();
    for (const c of 'unrelated') terminal.sendInput(c);
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain('husk number 0');
    expect(frame).not.toContain('unrelated searchable row');

    terminal.sendInput(CTRL_D); // bulk-stage still refers to the review
    await flush();
    expect(textSoFar(terminal)).toContain('Delete 7 empty sessions?');
  });

  test('candidates absent from the listing are never staged', async () => {
    gcCandidates = candidatesFor([
      ...husks.map((h) => h.sessionId),
      'phantom-never-listed',
    ]);
    const { terminal } = mount(husks);
    await flush();
    await stageGc(terminal);
    expect(textSoFar(terminal)).toContain('Delete 7 empty sessions?');
    terminal.sendInput('y');
    await flush();
    expect(gcDeleted).toHaveLength(1);
    expect(gcDeleted[0]).toEqual(husks.map((h) => h.sessionId));
  });

  test('user-touched candidates are exempted at stage time', async () => {
    gcCandidates = candidatesFor(husks.map((h) => h.sessionId));
    userTouched = ['husk-0', 'husk-3'];
    const { terminal } = mount(husks);
    await flush();
    await stageGc(terminal);
    terminal.sendInput('y');
    await flush();
    expect(gcDeleted).toHaveLength(1);
    expect(gcDeleted[0]).toEqual(
      husks.map((h) => h.sessionId).filter((id) => !userTouched.includes(id))
    );
  });

  test('esc leaves the cleanup review without deleting', async () => {
    gcCandidates = candidatesFor(husks.map((h) => h.sessionId));
    const { terminal, onClose } = mount(husks);
    await flush();
    for (let i = 0; i < 5; i++) terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(CTRL_D); // open review
    await flush();
    terminal.sendInput(ESC); // leave review — not the dashboard
    await flush();
    expect(onClose).not.toHaveBeenCalled();
    expect(gcDeleted).toHaveLength(0);
  });
});

describe('narrow layout', () => {
  test('keeps rendered lines and row budgeting within the terminal', async () => {
    mockTermSize.width = 32;
    mockTermSize.height = 16;
    const { terminal } = mount(
      Array.from({ length: 20 }, (_, index) =>
        session(
          `narrow-${index}`,
          `narrow row ${index} with a title that must be clipped`,
          { cwd: '/tmp' }
        )
      )
    );
    await flush();

    const expectWithinTerminal = async () => {
      const frame = await currentFrame(terminal);
      const lines = frame.split('\n').filter((line) => line.length > 0);
      expect(lines.length).toBeLessThanOrEqual(mockTermSize.height);
      expect(
        lines.every((line) => visibleWidth(line) <= mockTermSize.width)
      ).toBe(true);
      return frame;
    };

    const frame = await expectWithinTerminal();
    expect(frame).toContain('narrow row 0');
    expect(frame).not.toContain('narrow row 19');

    terminal.sendInput(ENTER);
    await flush();
    expect(await expectWithinTerminal()).toContain('Switch directory');

    terminal.sendInput(ESC);
    terminal.sendInput(CTRL_T);
    terminal.sendInput('a-very-long-tag-name-that-must-not-wrap-the-editor');
    await flush();
    await expectWithinTerminal();
  });

  test('a 15-line pane with the gc hint still shows session rows', async () => {
    mockTermSize.width = 100;
    mockTermSize.height = 15;
    const { terminal } = mount([
      ...Array.from({ length: 6 }, (_, index) =>
        session(`short-${index}`, `short pane row ${index}`, { cwd: '/tmp' })
      ),
      // Placeholder title classifies as empty → the gc hint line renders.
      session('short-empty', 'New Session', { cwd: '/tmp' }),
    ]);
    await flush();

    const frame = await currentFrame(terminal);
    const lines = frame.split('\n').filter((line) => line.length > 0);
    expect(lines.length).toBeLessThanOrEqual(mockTermSize.height);
    // Hints may be dropped at this height, session rows must not be.
    expect(frame).toContain('short pane row 0');
  });
});

describe('tiny terminal safety', () => {
  test('disables session actions while preserving the close key', async () => {
    mockTermSize.height = 10;
    const { terminal, onSelect, onClose } = mount([
      session('tiny', 'must not open'),
    ]);
    await flush();

    expect(await currentFrame(terminal)).toContain(
      'Terminal is too short to select a session safely.'
    );
    terminal.sendInput(ENTER);
    terminal.sendInput(CTRL_D);
    await flush();
    expect(onSelect).not.toHaveBeenCalled();
    expect(deletedIds).toEqual([]);

    terminal.sendInput(ESC);
    await flush();
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe('empty states', () => {
  test('refreshing with no sessions shows the loading indicator', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <SessionDashboard
          sessions={[]}
          currentCwd="/w/proj"
          activeSessionId={null}
          onSelect={vi.fn()}
          onClose={vi.fn()}
          isRefreshing
        />
      </AppStoreContext.Provider>,
      { terminal }
    );
    await flush();
    expect(textSoFar(terminal)).toContain('Loading sessions…');
    expect(textSoFar(terminal)).not.toContain('No sessions found.');
  });

  test('an empty store without a refresh in flight says no sessions', async () => {
    const { terminal } = mount([]);
    await flush();
    expect(textSoFar(terminal)).toContain('No sessions found.');
  });
});

describe('search respects grouping', () => {
  test('reruns an eligible query when the content index becomes ready', async () => {
    indexPromptCounts = new Map([['indexed', 1]]);
    let searchCalls = 0;
    indexSearchImpl = () => {
      searchCalls += 1;
      return [
        {
          sessionId: 'indexed',
          engine: 'v3',
          score: 1,
          snippet: 'needle from the prompt',
          matchField: 'prompt',
        },
      ];
    };
    const { terminal } = mount([
      session('indexed', 'conversation without title match', { engine: 'v3' }),
    ]);
    await flush();

    for (const c of 'needle') terminal.sendInput(c);
    await new Promise((resolve) => setTimeout(resolve, 500));
    await flush();
    expect(searchCalls).toBe(0);
    expect(await currentFrame(terminal)).not.toContain(
      'conversation without title match'
    );

    indexStatus = { state: 'ready', indexed: 1, total: 1 };
    for (const listener of indexStatusListeners) listener(indexStatus);
    await flush();

    expect(searchCalls).toBe(1);
    expect(await currentFrame(terminal)).toContain(
      'conversation without title match'
    );
  });

  test('title-matched subagent rows surface their parent, never themselves', async () => {
    indexStatus = { state: 'ready', indexed: 1, total: 1 };
    indexPromptCounts = new Map([
      ['indexed', 2],
      ['root', 2],
      ['child', 2],
      ['orphan', 2],
    ]);
    indexSearchImpl = () => [
      {
        sessionId: 'indexed',
        engine: 'v3',
        score: 1,
        snippet: 'needle from the prompt',
        matchField: 'prompt',
      },
    ];
    const { terminal } = mount([
      session('indexed', 'indexed prompt hit row', { engine: 'v3' }),
      session('root', 'root umbrella session', { engine: 'v3' }),
      session('child', 'needle derived worker', {
        engine: 'v3',
        createdReason: 'subagent',
        parentSessionId: 'root',
      }),
      session('orphan', 'needle orphan task', {
        engine: 'v3',
        createdReason: 'subagent',
      }),
    ]);
    await flush();

    for (const c of 'needle') terminal.sendInput(c);
    await new Promise((resolve) => setTimeout(resolve, 500));
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain('indexed prompt hit row');
    expect(frame).toContain('root umbrella session');
    expect(frame).not.toContain('needle derived worker');
    expect(frame).not.toContain('needle orphan task');
  });

  test('content-search hits stay bucketed under their workspace bands', async () => {
    indexStatus = { state: 'ready', indexed: 3, total: 3 };
    indexPromptCounts = new Map([
      ['a-1', 2],
      ['b-1', 2],
      ['b-2', 2],
    ]);
    indexSearchImpl = () => [
      {
        sessionId: 'b-1',
        engine: 'v3',
        score: 2,
        snippet: 'beta',
        matchField: 'prompt',
      },
      {
        sessionId: 'a-1',
        engine: 'v3',
        score: 1,
        snippet: 'alpha',
        matchField: 'prompt',
      },
    ];
    const { terminal } = mount([
      session('a-1', 'alpha topic', { cwd: '/w/proj' }),
      session('b-1', 'beta topic', { cwd: '/w/other' }),
      session('b-2', 'gamma unrelated', { cwd: '/w/other' }),
    ]);
    await flush();

    for (const c of 'topic') terminal.sendInput(c);
    // Content search debounces 400ms before hitting the index.
    await new Promise((resolve) => setTimeout(resolve, 500));
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain('/w/proj');
    expect(frame).toContain('/w/other');
    expect(frame).not.toContain('Results');
    expect(frame).toContain('alpha topic');
    expect(frame).toContain('beta topic');
    expect(frame).not.toContain('gamma unrelated');
  });

  test('retains an unindexed remote title beside local content hits', async () => {
    indexStatus = { state: 'ready', indexed: 1, total: 1 };
    indexPromptCounts = new Map([['local', 1]]);
    indexSearchImpl = () => [
      {
        sessionId: 'local',
        engine: 'v3',
        score: 1,
        snippet: 'needle in a local prompt',
        matchField: 'prompt',
      },
    ];
    const { terminal } = mount([
      session('local', 'local conversation'),
      session('remote', 'needle in remote title', {
        cwd: '',
        source: 'remote',
        executionTarget: { kind: 'cloud-sandbox' },
      }),
    ]);
    await flush();

    for (const c of 'needle') terminal.sendInput(c);
    await new Promise((resolve) => setTimeout(resolve, 500));
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain('local conversation');
    expect(frame).toContain('needle in remote title');
  });

  test('sessions with a relative recorded cwd land in the unknown bucket', async () => {
    const { terminal } = mount([
      session('dot-1', 'dot session', { cwd: '.' }),
      session('ok-1', 'normal session', { cwd: '/w/proj' }),
    ]);
    await flush();
    const frame = textSoFar(terminal);
    expect(frame).toContain('(unknown workspace)');
    // The bare "." must never surface as a group band.
    expect(frame).not.toMatch(/^\s*\.\s*\(1\)/m);
  });
});

describe('physical session identity', () => {
  const UUID = '44c187c1-3509-4b20-9965-6dbda1203007';

  test('prop reordering keeps the cursor on the same physical row', async () => {
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    let replaceSessions!: (sessions: SessionListingInput[]) => void;
    const initial = [
      session(UUID, 'V2 row', {
        engine: 'v2',
        updatedAt: '2026-08-06T00:00:00.000Z',
      }),
      session(UUID, 'V3 row', {
        engine: 'v3',
        updatedAt: '2026-08-05T00:00:00.000Z',
      }),
    ];
    const Harness = () => {
      const [rows, setRows] = React.useState(initial);
      replaceSessions = setRows;
      return (
        <SessionDashboard
          sessions={rows}
          currentCwd="/w/proj"
          activeSessionId={null}
          onSelect={vi.fn()}
          onClose={vi.fn()}
        />
      );
    };
    activeInstance = render(
      <AppStoreContext.Provider value={store}>
        <Harness />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    await flush();

    terminal.sendInput(DOWN);
    await flush();
    replaceSessions([
      { ...initial[0]!, updatedAt: '2026-08-04T00:00:00.000Z' },
      { ...initial[1]!, updatedAt: '2026-08-07T00:00:00.000Z' },
    ]);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();

    expect(await currentFrame(terminal)).toContain('Delete "V3 row"?');
  });

  test('converted sessions remain independently selectable rows', async () => {
    const { terminal } = mount([
      session(UUID, 'dashboard plan', {
        engine: 'v2',
        updatedAt: '2026-08-01T00:00:00.000Z',
        messageCount: 196,
      }),
      session(`cli_${UUID}_oKhTMtya`, 'dashboard plan', {
        engine: 'v3',
        updatedAt: '2026-08-06T00:00:00.000Z',
        messageCount: 322,
      }),
      session(`cli_${UUID}_f5M2Cb27`, 'dashboard plan', {
        engine: 'v3',
        updatedAt: '2026-08-04T00:00:00.000Z',
        messageCount: 250,
      }),
    ]);
    await flush();

    const rows = (await currentFrame(terminal))
      .split('\n')
      .filter(
        (line) =>
          line.includes('dashboard plan') && !line.includes('Preview of')
      );
    expect(rows).toHaveLength(3);
    expect(rows.some((line) => line.includes('322'))).toBe(true);
    expect(rows.some((line) => line.includes('250'))).toBe(true);
    expect(rows.some((line) => line.includes('196'))).toBe(true);
  });

  test('deleting one conversion does not delete its source or siblings', async () => {
    const kiro = new Kiro();
    const deleteSessionById = vi
      .spyOn(kiro, 'deleteSessionById')
      .mockResolvedValue(true);
    const selectedId = `cli_${UUID}_newest`;
    const { terminal } = mount(
      [
        session(UUID, 'dashboard plan', {
          engine: 'v2',
          updatedAt: '2026-08-01T00:00:00.000Z',
        }),
        session(selectedId, 'dashboard plan', {
          engine: 'v3',
          updatedAt: '2026-08-06T00:00:00.000Z',
        }),
        session(`cli_${UUID}_older`, 'dashboard plan', {
          engine: 'v3',
          updatedAt: '2026-08-04T00:00:00.000Z',
        }),
      ],
      { kiro }
    );
    await flush();

    terminal.sendInput(CTRL_D);
    await flush();
    expect(textSoFar(terminal)).not.toContain('all 3 copies');
    terminal.sendInput('y');
    await flush();

    expect(deleteSessionById).toHaveBeenCalledTimes(1);
    expect(deleteSessionById).toHaveBeenCalledWith(selectedId, {
      source: 'local',
    });
    expect(deleteSessionById).not.toHaveBeenCalledWith(UUID, expect.anything());
    expect(deleteSessionById).not.toHaveBeenCalledWith(
      `cli_${UUID}_older`,
      expect.anything()
    );
  });

  test('exact-id V3 and V2 rows retain distinct delete routes', async () => {
    const kiro = new Kiro();
    const deleteSessionById = vi
      .spyOn(kiro, 'deleteSessionById')
      .mockResolvedValue(true);
    const { terminal } = mount(
      [
        session(UUID, 'same physical id', {
          engine: 'v2',
          updatedAt: '2026-08-01T00:00:00.000Z',
        }),
        session(UUID, 'same physical id', {
          engine: 'v3',
          updatedAt: '2026-08-06T00:00:00.000Z',
        }),
      ],
      { kiro }
    );
    await flush();

    expect(
      (await currentFrame(terminal))
        .split('\n')
        .filter(
          (line) =>
            line.includes('same physical id') && !line.includes('Preview of')
        )
    ).toHaveLength(2);
    terminal.sendInput(CTRL_D);
    await flush();
    terminal.sendInput('y');
    await flush();

    expect(deleteSessionById).toHaveBeenCalledTimes(1);
    expect(deleteSessionById).toHaveBeenCalledWith(UUID, { source: 'local' });
    expect(deletedIds).toEqual([]);
  });

  test('deleting a remote row affects only that physical source', async () => {
    const kiro = new Kiro();
    const deleteSessionById = vi
      .spyOn(kiro, 'deleteSessionById')
      .mockResolvedValue(true);
    const remoteId = `cli_${UUID}_remote`;
    const { terminal } = mount(
      [
        session(`cli_${UUID}_local`, 'dashboard plan', {
          engine: 'v3',
          updatedAt: '2026-08-05T00:00:00.000Z',
          source: 'local',
        }),
        session(remoteId, 'dashboard plan', {
          engine: 'v3',
          updatedAt: '2026-08-06T00:00:00.000Z',
          source: 'remote',
        }),
      ],
      { kiro }
    );
    await flush();

    terminal.sendInput(CTRL_D);
    await flush();
    terminal.sendInput('y');
    await flush();

    expect(deleteSessionById).toHaveBeenCalledTimes(1);
    expect(deleteSessionById).toHaveBeenCalledWith(remoteId, {
      source: 'remote',
    });
  });
});

describe('dashboard UX bindings and persistence', () => {
  test('routes V3 rename through its discovery source', async () => {
    const kiro = new Kiro();
    const renameSessionById = vi
      .spyOn(kiro, 'renameSessionById')
      .mockResolvedValue(true);
    const { terminal } = mount(
      [
        session('remote-rename', 'old remote title', {
          engine: 'v3',
          source: 'remote',
        }),
      ],
      { kiro }
    );
    await flush();

    terminal.sendInput(CTRL_R);
    await flush();
    terminal.sendInput(CTRL_U);
    terminal.sendInput('new remote title');
    await flush();
    terminal.sendInput(ENTER);
    await flush();

    expect(renameSessionById).toHaveBeenCalledWith(
      'remote-rename',
      'new remote title',
      { source: 'remote' }
    );
  });

  test('keeps V2 rename as a local dashboard override', async () => {
    const kiro = new Kiro();
    const renameSessionById = vi
      .spyOn(kiro, 'renameSessionById')
      .mockResolvedValue(true);
    const { terminal } = mount(
      [session('v2-rename', 'old V2 title', { engine: 'v2' })],
      { kiro }
    );
    await flush();

    terminal.sendInput(CTRL_R);
    await flush();
    terminal.sendInput(CTRL_U);
    terminal.sendInput('new V2 title');
    await flush();
    terminal.sendInput(ENTER);
    await flush();

    expect(renameSessionById).not.toHaveBeenCalled();
    expect(await currentFrame(terminal)).toContain('new V2 title');
  });

  test('failed bookmark persistence is reported without changing the row', async () => {
    bookmarkMutationFails = true;
    const { terminal } = mount([session('s-1', 'bookmark target')]);
    await flush();

    terminal.sendInput(CTRL_B);
    await flush();

    expect(await currentFrame(terminal)).toContain(
      'Bookmark was not saved (write-failed)'
    );
    expect(bookmarkedIds.has('s-1')).toBe(false);
  });

  test('bookmark reflow keeps the cursor on the same physical row', async () => {
    const { terminal } = mount([
      session('first', 'first row', {
        updatedAt: '2026-01-02T00:00:00.000Z',
      }),
      session('second', 'second row', {
        updatedAt: '2026-01-01T00:00:00.000Z',
      }),
    ]);
    await flush();

    terminal.sendInput(DOWN);
    await flush();
    terminal.sendInput(CTRL_B);
    await flush();
    terminal.sendInput(CTRL_D);
    await flush();

    expect(await currentFrame(terminal)).toContain('Delete "second row"?');
  });

  test('hosted dashboard lists ctrl+p as a preview shortcut', async () => {
    const { terminal } = mount([session('s-1', 'preview target')], {
      hostedPreview: true,
    });
    await flush();
    expect(await currentFrame(terminal)).toContain('tab/ctrl+p preview');
  });

  test('the selected grouping persists across dashboard remounts', async () => {
    const first = mount([session('s-1', 'persist grouping')]);
    await flush();
    first.terminal.sendInput(CTRL_G);
    await flush();
    first.terminal.sendInput(RIGHT);
    await flush();
    first.terminal.sendInput(ENTER);
    await flush();
    expect(persistedSettings['chat.sessionDashboard.groupBy']).toBe('recency');

    activeInstance?.unmount();
    activeInstance = null;
    const second = mount([session('s-1', 'persist grouping')]);
    await flush();
    expect(await currentFrame(second.terminal)).toContain('group: <recency>');
  });

  test('ctrl+w clears the last search word and ctrl+u clears the line', async () => {
    const { terminal } = mount([session('s-1', 'alpha beta')]);
    await flush();
    for (const c of 'alpha beta') terminal.sendInput(c);
    await flush();
    terminal.sendInput(CTRL_W);
    await flush();
    expect(await currentFrame(terminal)).toContain('search: alpha ');
    terminal.sendInput(CTRL_U);
    await flush();
    expect(await currentFrame(terminal)).toContain(
      'find by title, prompt or #tag'
    );
  });

  test('bracketed paste populates the search input', async () => {
    const { terminal } = mount([session('s-1', 'copied filter')]);
    await flush();
    terminal.sendInput('\x1b[200~copied filter\x1b[201~');
    await flush();
    expect(await currentFrame(terminal)).toContain('search: copied filter');
  });

  test('previously archived rows stay visible and the footer omits archive', async () => {
    archivedIds.add('s-1');
    const { terminal } = mount([session('s-1', 'shelved old row')]);
    await flush();
    const frame = await currentFrame(terminal);
    expect(frame).toContain('shelved old row');
    expect(frame).not.toContain('archive');
    expect(frame).toContain('ctrl+d delete');
  });
});

describe('workspace group ordering', () => {
  test('HOME renders as its canonical absolute path rather than shorthand', async () => {
    const { terminal } = mount([
      session('home-session', 'home workspace', { cwd: homedir() }),
    ]);
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain(homedir());
    expect(frame).not.toMatch(/^\s*~\s*\(1\)/m);
  });

  test('HOME descendants render with compact shorthand', async () => {
    const descendant = join(homedir(), 'work', 'repo');
    const { terminal } = mount([
      session('home-descendant', 'descendant workspace', {
        cwd: descendant,
      }),
    ]);
    await flush();

    const frame = await currentFrame(terminal);
    expect(frame).toContain(`~${descendant.slice(homedir().length)}`);
    expect(frame).not.toContain(descendant);
  });

  test('bookmarks lead, then cloud, then current and alphabetized workspaces', async () => {
    bookmarkedIds.add('bookmark');
    const { terminal } = mount([
      session('bookmark', 'bookmarked session', {
        cwd: '/w/bookmark',
        updatedAt: '2020-01-01T00:00:00.000Z',
      }),
      session('older', 'older workspace', {
        cwd: '/w/older',
        updatedAt: '2026-08-01T00:00:00.000Z',
      }),
      session('current', 'current session', {
        cwd: '/w/proj',
        updatedAt: '2020-01-01T00:00:00.000Z',
      }),
      session('cloud', 'cloud session', {
        cwd: '',
        source: 'remote',
        updatedAt: '2026-08-07T00:00:00.000Z',
      }),
      session('recent', 'recent workspace', {
        cwd: '/w/recent',
        updatedAt: '2026-08-06T00:00:00.000Z',
      }),
    ]);
    await flush();
    const frame = await currentFrame(terminal);
    // Order: Bookmarked, Cloud, current workspace, then others alphabetically.
    expect(frame.indexOf('Bookmarked')).toBeLessThan(
      frame.indexOf('cloud session')
    );
    expect(frame.indexOf('cloud session')).toBeLessThan(
      frame.indexOf('/w/proj')
    );
    expect(frame.indexOf('/w/proj')).toBeLessThan(frame.indexOf('/w/older'));
    expect(frame.indexOf('/w/older')).toBeLessThan(frame.indexOf('/w/recent'));
  });
});
