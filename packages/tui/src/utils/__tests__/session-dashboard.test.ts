import { describe, it, expect } from 'bun:test';
import { homedir } from 'node:os';
import { join } from 'node:path';
import {
  applySubagentNesting,
  filterSessionsByText,
  workspaceLabel,
  conversationKey,
  sessionIdentityKey,
  sessionMatchesActive,
  sessionDashboardPaneWidths,
  type WorkspaceGroup,
} from '../session-dashboard';
import { groupSessions } from '../session-grouping';
import type { SessionInfoEntry } from '../../types/session-client';

function makeSession(
  overrides: Partial<SessionInfoEntry> = {}
): SessionInfoEntry {
  return {
    sessionId: `session-${Math.random().toString(36).slice(2, 10)}`,
    cwd: '/home/user/project',
    title: 'Test session',
    updatedAt: '2026-07-20T10:00:00.000Z',
    ...overrides,
  };
}

/** The production grouping path, defaulted to the workspace dimension. */
function groupByWorkspace(
  sessions: SessionInfoEntry[],
  currentCwd: string,
  activeSessionId?: string | null
): WorkspaceGroup[] {
  return groupSessions(sessions, currentCwd, {
    groupBy: 'workspace',
    activeSessionId,
    meta: { isBookmarked: () => false, getTags: () => [] },
  });
}

describe('sessionDashboardPaneWidths', () => {
  it('suppresses preview before either pane can become negative or unusable', () => {
    expect(sessionDashboardPaneWidths(45, true)).toEqual({
      list: 45,
      preview: 0,
      showPreview: false,
    });
    expect(sessionDashboardPaneWidths(1, true)).toEqual({
      list: 1,
      preview: 0,
      showPreview: false,
    });
  });

  it('keeps both panes within the terminal when preview is available', () => {
    const widths = sessionDashboardPaneWidths(80, true);
    expect(widths.showPreview).toBe(true);
    expect(widths.list).toBeGreaterThanOrEqual(32);
    expect(widths.preview).toBeGreaterThanOrEqual(24);
    expect(widths.list + widths.preview + 1).toBe(80);
  });
});

describe('workspaceLabel', () => {
  it('returns basename of normal path', () => {
    expect(workspaceLabel('/home/user/my-project')).toBe('my-project');
  });

  it('returns parent/child for generic basenames', () => {
    expect(workspaceLabel('/home/user/my-project/src')).toBe('my-project/src');
    expect(workspaceLabel('/home/user/workspace')).toBe('user/workspace');
  });

  it('handles trailing slashes', () => {
    expect(workspaceLabel('/home/user/my-project/')).toBe('my-project');
  });

  it('handles empty string', () => {
    expect(workspaceLabel('')).toBe('(unknown)');
  });

  it('handles root path', () => {
    expect(workspaceLabel('/')).toBe('(unknown)');
  });

  it('treats a bare relative cwd as unknown', () => {
    expect(workspaceLabel('.')).toBe('(unknown)');
    expect(workspaceLabel('./')).toBe('(unknown)');
  });

  it('labels Windows paths by basename', () => {
    expect(workspaceLabel('C:\\Users\\bob\\my-project')).toBe('my-project');
    expect(workspaceLabel('C:\\Users\\bob\\my-project\\')).toBe('my-project');
    expect(workspaceLabel('C:\\Users\\bob\\my-project\\src')).toBe(
      'my-project/src'
    );
  });
});

describe('session type examples', () => {
  // These document how each derived-session type appears in the dashboard.
  function mk(
    overrides: Partial<SessionInfoEntry> & {
      parentSessionId?: string;
      createdReason?: 'subagent' | 'rewind';
    } = {}
  ) {
    return {
      sessionId: `s-${Math.random().toString(36).slice(2, 8)}`,
      cwd: '/workspace/proj',
      title: 'Session',
      updatedAt: '2026-07-20T10:00:00.000Z',
      ...overrides,
    };
  }

  it('EXAMPLE: subagent session nests under its parent', () => {
    const sessions = [
      mk({ sessionId: 'parent', title: 'review the codebase' }),
      mk({
        sessionId: 'sub-1',
        title: 'reviewing auth',
        parentSessionId: 'parent',
        createdReason: 'subagent',
      }),
      mk({
        sessionId: 'sub-2',
        title: 'reviewing db',
        parentSessionId: 'parent',
        createdReason: 'subagent',
      }),
    ];

    const groups = applySubagentNesting(
      groupByWorkspace(sessions, '/workspace/proj')
    );
    // Parent stays top-level; subagents nest as children.
    expect(groups[0]!.sessions).toHaveLength(1);
    const parent = groups[0]!.sessions[0]!;
    expect(parent.sessionId).toBe('parent');
    expect(parent.children.map((c) => c.sessionId).sort()).toEqual([
      'sub-1',
      'sub-2',
    ]);
    expect(parent.children.every((c) => c.isSubagent)).toBe(true);
  });

  it('EXAMPLE: rewind fork nests under parent (derived session)', () => {
    const sessions = [
      mk({ sessionId: 'orig', title: 'original conversation' }),
      mk({
        sessionId: 'fork',
        title: 'forked at turn 3',
        parentSessionId: 'orig',
        createdReason: 'rewind',
      }),
    ];

    const groups = applySubagentNesting(
      groupByWorkspace(sessions, '/workspace/proj')
    );
    // Rewind forks are derived sessions — nested under their parent and
    // hidden from the top-level list (shown in the parent's preview).
    expect(groups[0]!.sessions).toHaveLength(1);
    expect(groups[0]!.sessions[0]!.sessionId).toBe('orig');
    const orig = groups[0]!.sessions[0]!;
    expect(orig.children).toHaveLength(1);
    expect(orig.children[0]!.sessionId).toBe('fork');
    expect(orig.children[0]!.createdReason).toBe('rewind');
    expect(orig.children[0]!.isSubagent).toBe(true);
    expect(orig.children[0]!.parentSessionId).toBe('orig');
  });

  it('EXAMPLE: tangent is not a persisted session type', () => {
    // Tangent mode (V1) keeps state in memory and never writes a session
    // file, so it can never appear in a listing. A session must carry an
    // explicit subagent/rewind reason to be treated as derived; anything
    // else (including a hypothetical "tangent") is an ordinary top-level
    // session with no lineage.
    const sessions = [mk({ sessionId: 'plain', createdReason: undefined })];
    const groups = applySubagentNesting(
      groupByWorkspace(sessions, '/workspace/proj')
    );
    const entry = groups[0]!.sessions[0]!;
    expect(entry.createdReason).toBeUndefined();
    expect(entry.isSubagent).toBe(false);
  });

  it('EXAMPLE: orphan subagent (parent not listed) stays top-level', () => {
    const sessions = [
      mk({
        sessionId: 'orphan',
        parentSessionId: 'archived-parent',
        createdReason: 'subagent',
      }),
    ];
    const groups = applySubagentNesting(
      groupByWorkspace(sessions, '/workspace/proj')
    );
    // No parent in the listing → remains top-level, still flagged (the
    // dashboard's visibility predicate hides it from the master list).
    expect(groups[0]!.sessions).toHaveLength(1);
    expect(groups[0]!.sessions[0]!.isSubagent).toBe(true);
  });
});

describe('workspace grouping (production path)', () => {
  it('groups sessions by cwd', () => {
    const sessions = [
      makeSession({ cwd: '/workspace/a', sessionId: 'a1' }),
      makeSession({ cwd: '/workspace/b', sessionId: 'b1' }),
      makeSession({ cwd: '/workspace/a', sessionId: 'a2' }),
    ];

    const groups = groupByWorkspace(sessions, '/workspace/a');
    expect(groups).toHaveLength(2);
    expect(groups[0]!.workspace).toBe('/workspace/a');
    expect(groups[0]!.sessions).toHaveLength(2);
    expect(groups[1]!.workspace).toBe('/workspace/b');
    expect(groups[1]!.sessions).toHaveLength(1);
  });

  it('pins the current workspace before alphabetized remaining groups', () => {
    const sessions = [
      makeSession({ cwd: '/workspace/alpha', sessionId: 'a1' }),
      makeSession({ cwd: '/workspace/beta', sessionId: 'b1' }),
      makeSession({ cwd: '/workspace/gamma', sessionId: 'g1' }),
    ];

    const groups = groupByWorkspace(sessions, '/workspace/gamma');
    expect(groups.map((group) => group.workspace)).toEqual([
      '/workspace/gamma',
      '/workspace/alpha',
      '/workspace/beta',
    ]);
    expect(groups[0]!.isCurrent).toBe(true);
  });

  it('marks isCurrentWorkspace on entries', () => {
    const sessions = [
      makeSession({ cwd: '/workspace/a', sessionId: 'a1' }),
      makeSession({ cwd: '/workspace/b', sessionId: 'b1' }),
    ];

    const groups = groupByWorkspace(sessions, '/workspace/a');
    expect(groups[0]!.sessions[0]!.isCurrentWorkspace).toBe(true);
    expect(groups[1]!.sessions[0]!.isCurrentWorkspace).toBe(false);
  });

  it('sorts sessions by descending recency within each group', () => {
    const sessions = [
      makeSession({
        cwd: '/workspace/a',
        sessionId: 'old',
        updatedAt: '2026-07-01T10:00:00.000Z',
      }),
      makeSession({
        cwd: '/workspace/a',
        sessionId: 'new',
        updatedAt: '2026-07-20T10:00:00.000Z',
      }),
      makeSession({
        cwd: '/workspace/a',
        sessionId: 'mid',
        updatedAt: '2026-07-10T10:00:00.000Z',
      }),
    ];

    const groups = groupByWorkspace(sessions, '/workspace/a');
    const ids = groups[0]!.sessions.map((s) => s.sessionId);
    expect(ids).toEqual(['new', 'mid', 'old']);
  });

  it('marks active session', () => {
    const sessions = [
      makeSession({ cwd: '/workspace/a', sessionId: 'active-one' }),
      makeSession({ cwd: '/workspace/a', sessionId: 'other' }),
    ];

    const groups = groupByWorkspace(sessions, '/workspace/a', 'active-one');
    expect(groups[0]!.sessions[0]!.isActive).toBe(true);
    expect(groups[0]!.sessions[1]!.isActive).toBe(false);
  });

  it('handles empty session list', () => {
    const groups = groupByWorkspace([], '/workspace/a');
    expect(groups).toHaveLength(0);
  });

  it('handles sessions with missing cwd', () => {
    const sessions = [
      makeSession({ cwd: '', sessionId: 'orphan', title: 'orphan' }),
      makeSession({ cwd: '/workspace/a', sessionId: 'a1' }),
    ];

    const groups = groupByWorkspace(sessions, '/workspace/a');
    expect(groups).toHaveLength(2);
    // Empty cwd group should still exist.
    const emptyGroup = groups.find((g) => g.workspace === '');
    expect(emptyGroup).toBeDefined();
    expect(emptyGroup!.sessions).toHaveLength(1);
  });

  it('handles trailing slash normalization', () => {
    const sessions = [
      makeSession({ cwd: '/workspace/a/', sessionId: 'a1' }),
      makeSession({ cwd: '/workspace/a', sessionId: 'a2' }),
    ];

    const groups = groupByWorkspace(sessions, '/workspace/a/');
    expect(groups).toHaveLength(1);
    expect(groups[0]!.sessions).toHaveLength(2);
    expect(groups[0]!.isCurrent).toBe(true);
  });

  it('groups home-directory shorthand by canonical absolute workspace', () => {
    const sessions = [
      makeSession({ cwd: '~', sessionId: 'home' }),
      makeSession({ cwd: '~/work/repo', sessionId: 'repo' }),
    ];

    const groups = groupByWorkspace(sessions, homedir());
    expect(groups.map((group) => group.workspace)).toEqual([
      homedir(),
      join(homedir(), 'work', 'repo'),
    ]);
    expect(groups[0]!.isCurrent).toBe(true);
  });
});

describe('filterSessionsByText', () => {
  it('filters sessions by title match', () => {
    const groups: WorkspaceGroup[] = [
      {
        workspace: '/w/a',
        label: 'a',
        isCurrent: true,
        sessions: [
          {
            sessionId: 's1',
            title: 'Fix authentication bug',
            workspace: '/w/a',
            isCurrentWorkspace: true,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
          {
            sessionId: 's2',
            title: 'Add unit tests',
            workspace: '/w/a',
            isCurrentWorkspace: true,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
        ],
      },
    ];

    const filtered = filterSessionsByText(groups, 'auth');
    expect(filtered).toHaveLength(1);
    expect(filtered[0]!.sessions).toHaveLength(1);
    expect(filtered[0]!.sessions[0]!.title).toBe('Fix authentication bug');
  });

  it('filters by workspace label', () => {
    const groups: WorkspaceGroup[] = [
      {
        workspace: '/home/user/kiro-cli',
        label: 'kiro-cli',
        isCurrent: true,
        sessions: [
          {
            sessionId: 's1',
            title: 'some work',
            workspace: '/home/user/kiro-cli',
            isCurrentWorkspace: true,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
        ],
      },
      {
        workspace: '/home/user/other',
        label: 'other',
        isCurrent: false,
        sessions: [
          {
            sessionId: 's2',
            title: 'unrelated',
            workspace: '/home/user/other',
            isCurrentWorkspace: false,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
        ],
      },
    ];

    const filtered = filterSessionsByText(groups, 'kiro');
    expect(filtered).toHaveLength(1);
    expect(filtered[0]!.label).toBe('kiro-cli');
  });

  it('returns all groups for empty query', () => {
    const groups: WorkspaceGroup[] = [
      {
        workspace: '/w',
        label: 'w',
        isCurrent: true,
        sessions: [
          {
            sessionId: 's1',
            title: 'test',
            workspace: '/w',
            isCurrentWorkspace: true,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
        ],
      },
    ];

    expect(filterSessionsByText(groups, '')).toEqual(groups);
    expect(filterSessionsByText(groups, '   ')).toEqual(groups);
  });

  it('is case-insensitive', () => {
    const groups: WorkspaceGroup[] = [
      {
        workspace: '/w',
        label: 'w',
        isCurrent: true,
        sessions: [
          {
            sessionId: 's1',
            title: 'Deploy to Production',
            workspace: '/w',
            isCurrentWorkspace: true,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
        ],
      },
    ];

    const filtered = filterSessionsByText(groups, 'DEPLOY');
    expect(filtered).toHaveLength(1);
  });

  it('omits groups with no matching sessions', () => {
    const groups: WorkspaceGroup[] = [
      {
        workspace: '/w/a',
        label: 'a',
        isCurrent: true,
        sessions: [
          {
            sessionId: 's1',
            title: 'match me',
            workspace: '/w/a',
            isCurrentWorkspace: true,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
        ],
      },
      {
        workspace: '/w/b',
        label: 'b',
        isCurrent: false,
        sessions: [
          {
            sessionId: 's2',
            title: 'no match here',
            workspace: '/w/b',
            isCurrentWorkspace: false,
            updatedAt: '',
            isSubagent: false,
            children: [],
            isActive: false,
          },
        ],
      },
    ];

    const filtered = filterSessionsByText(groups, 'match me');
    expect(filtered).toHaveLength(1);
    expect(filtered[0]!.workspace).toBe('/w/a');
  });
});

const UUID = '44c187c1-3509-4b20-9965-6dbda1203007';

describe('conversationKey', () => {
  it('all spellings of a bridged session share one key', () => {
    expect(conversationKey(UUID)).toBe(UUID);
    expect(conversationKey(`sess_${UUID}`)).toBe(UUID);
    expect(conversationKey(`cli_${UUID}_oKhTMtya`)).toBe(UUID);
    expect(conversationKey(`cli_${UUID}_f5M2Cb27`)).toBe(UUID);
  });

  it('non-bridge ids stay distinct even when they contain a UUID', () => {
    expect(conversationKey('classic-42')).toBe('classic-42');
    expect(conversationKey('sess_custom')).toBe('sess_custom');
    expect(conversationKey(`prefix-${UUID}-suffix`)).toBe(
      `prefix-${UUID}-suffix`
    );
    expect(conversationKey(`cli_${UUID}`)).toBe(`cli_${UUID}`);
  });
});

describe('physical session identity', () => {
  it('keeps engine and discovery source in the identity key', () => {
    expect(
      sessionIdentityKey({ sessionId: UUID, engine: 'v2', source: 'local' })
    ).not.toBe(
      sessionIdentityKey({ sessionId: UUID, engine: 'v3', source: 'local' })
    );
    expect(
      sessionIdentityKey({ sessionId: UUID, engine: 'v3', source: 'local' })
    ).not.toBe(
      sessionIdentityKey({ sessionId: UUID, engine: 'v3', source: 'remote' })
    );
  });

  it('matches the active row only within the known physical store', () => {
    expect(
      sessionMatchesActive(
        { sessionId: UUID, engine: 'v3', source: 'local' },
        UUID,
        'v3',
        'local'
      )
    ).toBe(true);
    expect(
      sessionMatchesActive(
        { sessionId: UUID, engine: 'v2', source: 'local' },
        UUID,
        'v3',
        'local'
      )
    ).toBe(false);
    expect(
      sessionMatchesActive(
        { sessionId: UUID, engine: 'v3', source: 'remote' },
        UUID,
        'v3',
        'local'
      )
    ).toBe(false);
  });
});
