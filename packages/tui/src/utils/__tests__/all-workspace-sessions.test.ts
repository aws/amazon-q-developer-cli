import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  mkdtempSync,
  mkdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { homedir, tmpdir } from 'node:os';
import {
  getCachedAllWorkspaceSessions,
  fetchClassicSessionsDetailed,
  listAllWorkspaceSessionsFromDisk,
  listAllWorkspaceSessionsFromDiskDetailed,
  listLiveDashboardSessions,
  listLiveDashboardSessionsDetailed,
  mergeSessionListings,
  invalidateAllWorkspaceSessionsCache,
  resetAllWorkspaceSessionsCache,
  scanAllWorkspaceSessionsDetailed,
} from '../all-workspace-sessions';
import type { SessionInfoEntry } from '../../types/session-client';
import { SESSION_METADATA_MAX_BYTES } from '../bounded-json';
import {
  buildDashboardEntries,
  filterSessionsByText,
} from '../session-dashboard';
import { sanitizeSessionTitleForDisplay } from '../sanitize-title';

describe('listAllWorkspaceSessionsFromDisk', () => {
  let root: string;

  beforeEach(() => {
    resetAllWorkspaceSessionsCache();
    root = mkdtempSync(join(tmpdir(), 'all-ws-sessions-'));
  });

  afterEach(() => {
    resetAllWorkspaceSessionsCache();
    rmSync(root, { recursive: true, force: true });
  });

  function writeV2Session(id: string, cwd: string, title?: string): void {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', `${id}.json`),
      JSON.stringify({
        session_id: id,
        cwd,
        title,
        updated_at: '2026-07-20T10:00:00.000Z',
      })
    );
  }

  function writeKasSession(
    hash: string,
    id: string,
    workspace: string,
    title: string
  ): void {
    const dir = join(root, hash, `sess_${id}`);
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({
        id: `sess_${id}`,
        title,
        workspacePaths: [workspace],
        createdAt: '2026-07-21T10:00:00.000Z',
        lastModifiedAt: '2026-07-21T11:00:00.000Z',
      })
    );
  }

  it('reads V2 store sessions with cwd', async () => {
    writeV2Session('v2-a', '/workspace/alpha', 'Alpha work');
    writeV2Session('v2-b', '/workspace/beta');

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(2);
    const a = entries.find((e) => e.sessionId === 'v2-a')!;
    expect(a.cwd).toBe('/workspace/alpha');
    expect(a.title).toBe('Alpha work');
  });

  it('preserves sess_-prefixed V2 filenames as physical ids', async () => {
    writeV2Session('sess_x', '/workspace/alpha', 'Prefixed V2');

    const entries = await listAllWorkspaceSessionsFromDisk(root);

    expect(entries).toEqual([
      expect.objectContaining({ sessionId: 'sess_x', engine: 'v2' }),
    ]);
  });

  it('expands V2 home-directory shorthand into absolute workspaces', async () => {
    writeV2Session('v2-home', '~');
    writeV2Session('v2-repo', '~/work/repo');

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries.find((e) => e.sessionId === 'v2-home')?.cwd).toBe(homedir());
    expect(entries.find((e) => e.sessionId === 'v2-repo')?.cwd).toBe(
      join(homedir(), 'work', 'repo')
    );
  });

  it('reads KAS store sessions across workspace hashes', async () => {
    writeKasSession('hash1', 'k1', '/workspace/alpha', 'KAS alpha');
    writeKasSession('hash2', 'k2', '/workspace/gamma', 'KAS gamma');

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(2);
    const k1 = entries.find((e) => e.sessionId === 'k1')!;
    expect(k1.cwd).toBe('/workspace/alpha');
    expect(k1.title).toBe('KAS alpha');
    expect(k1.updatedAt).toBe('2026-07-21T11:00:00.000Z');
  });

  it('preserves disk cloud placement and derives remote source', async () => {
    const dir = join(root, 'hash1', 'sess_cloud');
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({
        id: 'sess_cloud',
        title: 'Cloud work',
        workspacePaths: ['/workspace/cloud'],
        executionTarget: { kind: 'cloud-sandbox' },
      })
    );

    const entries = await listAllWorkspaceSessionsFromDisk(root);

    expect(entries).toEqual([
      expect.objectContaining({
        sessionId: 'cloud',
        source: 'remote',
        executionTarget: { kind: 'cloud-sandbox' },
        engine: 'v3',
      }),
    ]);
  });

  it('merges both stores', async () => {
    writeV2Session('v2-a', '/workspace/alpha');
    writeKasSession('hash1', 'k1', '/workspace/beta', 'KAS beta');

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(2);
  });

  it('dedupes a session present in several hash dirs, keeping the workspace copy', async () => {
    // Same id in _global (no workspace) and in its workspace hash dir.
    const g = join(root, '_global', 'sess_dup');
    mkdirSync(g, { recursive: true });
    writeFileSync(
      join(g, 'session.json'),
      JSON.stringify({ id: 'sess_dup', title: 'dup', workspacePaths: [] })
    );
    writeKasSession('hashW', 'dup', '/workspace/w', 'dup');

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    const dups = entries.filter((e) => e.sessionId === 'dup');
    expect(dups).toHaveLength(1);
    expect(dups[0]!.cwd).toBe('/workspace/w');
  });

  it('captures V2 subagent lineage (for nesting, not skipped)', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'sub-1.json'),
      JSON.stringify({
        session_id: 'sub-1',
        cwd: '/w',
        parent_session_id: 'parent-1',
        session_created_reason: 'subagent',
      })
    );

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.sessionId).toBe('sub-1');
    expect(entries[0]!.parentSessionId).toBe('parent-1');
    expect(entries[0]!.createdReason).toBe('subagent');
  });

  it('captures V2 rewind-fork lineage', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'fork-1.json'),
      JSON.stringify({
        session_id: 'fork-1',
        cwd: '/w',
        parent_session_id: 'orig-1',
        session_created_reason: 'rewind',
      })
    );

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.createdReason).toBe('rewind');
  });

  it('honors a subagent reason without a parent id', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'sub-2.json'),
      JSON.stringify({
        session_id: 'sub-2',
        cwd: '/w',
        session_created_reason: 'subagent',
      })
    );

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.createdReason).toBe('subagent');
    expect(entries[0]!.parentSessionId).toBeUndefined();
  });

  it('drops a rewind reason without a parent id (a real conversation)', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'fork-2.json'),
      JSON.stringify({
        session_id: 'fork-2',
        cwd: '/w',
        session_created_reason: 'rewind',
      })
    );

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.createdReason).toBeUndefined();
  });

  it('skips malformed files without failing', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(join(root, 'cli', 'bad.json'), 'not json');
    writeV2Session('good', '/w');

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(1);
  });

  it('marks a malformed disk catalog incomplete', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(join(root, 'cli', 'bad.json'), 'not json');

    const result = await listAllWorkspaceSessionsFromDiskDetailed(root);
    expect(result.sessions).toEqual([]);
    expect(result.complete).toBe(false);
  });

  it('uses the V2 filename as physical identity, not untrusted metadata', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'safe-id.json'),
      JSON.stringify({ session_id: '../../outside', cwd: '/w' })
    );

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries.map((entry) => entry.sessionId)).toEqual(['safe-id']);
  });

  it('skips a symlinked V2 catalog and marks the scan incomplete', async () => {
    const outside = mkdtempSync(join(tmpdir(), 'all-ws-outside-'));
    try {
      writeFileSync(
        join(outside, 'outside.json'),
        JSON.stringify({ session_id: 'outside', cwd: '/outside' })
      );
      symlinkSync(outside, join(root, 'cli'), 'dir');

      const result = await listAllWorkspaceSessionsFromDiskDetailed(root);

      expect(result.sessions).toEqual([]);
      expect(result.complete).toBe(false);
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });

  it('skips a symlinked KAS catalog and marks the scan incomplete', async () => {
    const outside = mkdtempSync(join(tmpdir(), 'all-ws-kas-outside-'));
    try {
      const sessionDir = join(outside, 'sess_outside');
      mkdirSync(sessionDir);
      writeFileSync(
        join(sessionDir, 'session.json'),
        JSON.stringify({ id: 'sess_outside', workspacePaths: ['/outside'] })
      );
      symlinkSync(outside, join(root, 'hash-link'), 'dir');

      const result = await listAllWorkspaceSessionsFromDiskDetailed(root);

      expect(result.sessions).toEqual([]);
      expect(result.complete).toBe(false);
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });

  it('handles missing sessions root', async () => {
    const entries = await listAllWorkspaceSessionsFromDisk('/nonexistent/xyz');
    expect(entries).toHaveLength(0);
  });

  it('handles KAS sess dirs without session.json', async () => {
    mkdirSync(join(root, 'hash1', 'sess_broken'), { recursive: true });
    writeKasSession('hash1', 'ok', '/w', 'fine');

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.sessionId).toBe('ok');
  });

  it('rejects KAS metadata whose id names a different physical directory', async () => {
    writeKasSession('hash1', 'b', '/workspace/b', 'legitimate B');
    const dirA = join(root, 'hash1', 'sess_a');
    mkdirSync(dirA, { recursive: true });
    writeFileSync(
      join(dirA, 'session.json'),
      JSON.stringify({ id: 'sess_b', workspacePaths: ['/workspace/a'] })
    );

    const result = await listAllWorkspaceSessionsFromDiskDetailed(root);

    expect(result.sessions.map((entry) => entry.sessionId)).toEqual(['b']);
    expect(result.complete).toBe(false);
  });

  it('normalizes malformed disk fields before dashboard rendering', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'safe.json'),
      JSON.stringify({
        cwd: 42,
        title: { replace: 'not callable' },
        updated_at: [],
      })
    );

    const entries = await listAllWorkspaceSessionsFromDisk(root);
    const dashboard = buildDashboardEntries(entries, '/workspace');

    expect(entries).toEqual([{ sessionId: 'safe', cwd: '', engine: 'v2' }]);
    expect(() =>
      filterSessionsByText(
        [
          {
            label: 'all',
            workspace: '',
            isCurrent: false,
            sessions: dashboard,
          },
        ],
        'x'
      )
    ).not.toThrow();
    expect(sanitizeSessionTitleForDisplay({ replace: 'nope' })).toBe('');
  });

  it('degrades oversized V2 metadata to a row; oversized KAS still marks incomplete', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'huge.json'),
      Buffer.alloc(SESSION_METADATA_MAX_BYTES + 1, 0x20)
    );
    const hugeKas = join(root, 'hash1', 'sess_huge-kas');
    mkdirSync(hugeKas, { recursive: true });
    writeFileSync(
      join(hugeKas, 'session.json'),
      Buffer.alloc(SESSION_METADATA_MAX_BYTES + 1, 0x20)
    );
    writeKasSession('hash1', 'good', '/workspace/good', 'Good');

    const result = await listAllWorkspaceSessionsFromDiskDetailed(root);

    // The oversized V2 session lists as a degraded row (old V2 files embed
    // whole conversations — hiding them punished exactly the long-time
    // users with the most sessions); the oversized KAS copy still can't be
    // identity-validated, so it is skipped and completeness drops.
    const ids = result.sessions.map((entry) => entry.sessionId).sort();
    expect(ids).toEqual(['good', 'huge']);
    const huge = result.sessions.find((s) => s.sessionId === 'huge');
    expect(huge?.engine).toBe('v2');
    expect(huge?.updatedAt).toBeTruthy();
    expect(result.complete).toBe(false);
  });

  it('retains the last complete catalog across a production invalidation', async () => {
    writeV2Session('kept-a', '/workspace/a');
    writeV2Session('kept-b', '/workspace/b');
    const complete = await scanAllWorkspaceSessionsDetailed(root);
    expect(complete.complete).toBe(true);

    invalidateAllWorkspaceSessionsCache();
    writeFileSync(join(root, 'cli', 'kept-b.json'), '{');
    const partial = await scanAllWorkspaceSessionsDetailed(root);

    expect(partial.complete).toBe(false);
    expect(partial.sessions.map((entry) => entry.sessionId).sort()).toEqual([
      'kept-a',
      'kept-b',
    ]);
  });

  it('keeps a newer cache generation when an invalidated scan finishes late', async () => {
    for (let i = 0; i < 51; i++) {
      writeV2Session(`old-${i}`, '/old');
    }
    const staleScan = scanAllWorkspaceSessionsDetailed(root);

    resetAllWorkspaceSessionsCache();
    rmSync(join(root, 'cli'), { recursive: true, force: true });
    writeV2Session('fresh', '/fresh');
    const freshScan = await scanAllWorkspaceSessionsDetailed(root);
    expect(freshScan.sessions.map((entry) => entry.sessionId)).toEqual([
      'fresh',
    ]);

    await staleScan;
    expect(
      getCachedAllWorkspaceSessions().map((entry) => entry.sessionId)
    ).toEqual(['fresh']);
  });
});

describe('listLiveDashboardSessions', () => {
  it('prefers an all-workspace live listing without passing cwd', async () => {
    const calls: string[] = [];
    const cloud: SessionInfoEntry = {
      sessionId: 'cloud-1',
      cwd: '/sandbox',
      source: 'remote',
      executionTarget: { kind: 'cloud-sandbox' },
    };

    const result = await listLiveDashboardSessions(
      {
        listAllWorkspaceSessions: async () => {
          calls.push('all');
          return { sessions: [cloud] };
        },
        listSessions: async (cwd) => {
          calls.push(cwd);
          return { sessions: [] };
        },
      },
      '/workspace/current'
    );

    expect(calls).toEqual(['all']);
    expect(result).toEqual([cloud]);
  });

  it('falls back to the current cwd when all-workspace listing is unsupported', async () => {
    const calls: string[] = [];
    const result = await listLiveDashboardSessions(
      {
        listAllWorkspaceSessions: async () => {
          calls.push('all');
          return { sessions: [], failed: true };
        },
        listSessions: async (cwd) => {
          calls.push(cwd);
          return { sessions: [] };
        },
      },
      '/workspace/current'
    );

    expect(calls).toEqual(['all', '/workspace/current']);
    expect(result).toEqual([]);
  });

  it('uses cwd-scoped listing for clients without all-workspace support', async () => {
    const calls: string[] = [];
    const result = await listLiveDashboardSessions(
      {
        listSessions: async (cwd) => {
          calls.push(cwd);
          return { sessions: [] };
        },
      },
      '/workspace/current'
    );

    expect(calls).toEqual(['/workspace/current']);
    expect(result).toEqual([]);
  });

  it('marks a cwd fallback incomplete for destructive reconciliation', async () => {
    const result = await listLiveDashboardSessionsDetailed(
      {
        listAllWorkspaceSessions: async () => ({
          sessions: [],
          failed: true,
        }),
        listSessions: async () => ({
          sessions: [{ sessionId: 'cwd-only', cwd: '/workspace/current' }],
        }),
      },
      '/workspace/current'
    );

    expect(result.sessions.map((entry) => entry.sessionId)).toEqual([
      'cwd-only',
    ]);
    expect(result.complete).toBe(false);
  });

  it('preserves incompleteness reported by an all-workspace client', async () => {
    const result = await listLiveDashboardSessionsDetailed(
      {
        listAllWorkspaceSessions: async () => ({
          sessions: [{ sessionId: 'partial', cwd: '/workspace/current' }],
          complete: false,
        }),
        listSessions: async () => ({ sessions: [] }),
      },
      '/workspace/current'
    );

    expect(result.sessions.map((entry) => entry.sessionId)).toEqual([
      'partial',
    ]);
    expect(result.complete).toBe(false);
  });

  it('normalizes malformed live rows and reports invalid identities incomplete', async () => {
    const malformed = [
      {
        sessionId: 'safe-live',
        cwd: 99,
        title: ['bad'],
        updatedAt: {},
        messageCount: Number.POSITIVE_INFINITY,
        source: 'unexpected',
        executionTarget: { kind: 'unexpected' },
      },
      { sessionId: '../escape', cwd: '/workspace' },
    ] as unknown as SessionInfoEntry[];

    const result = await listLiveDashboardSessionsDetailed(
      {
        listAllWorkspaceSessions: async () => ({ sessions: malformed }),
        listSessions: async () => ({ sessions: [] }),
      },
      '/workspace/current'
    );

    expect(result.sessions).toEqual([{ sessionId: 'safe-live', cwd: '' }]);
    expect(result.complete).toBe(false);
  });

  it('degrades to an empty overlay when the live request fails', async () => {
    const result = await listLiveDashboardSessions(
      {
        listSessions: async () => {
          throw new Error('unavailable');
        },
      },
      '/workspace/current'
    );

    expect(result).toEqual([]);
  });
});

describe('mergeSessionListings', () => {
  const mk = (
    id: string,
    extra: Partial<SessionInfoEntry> = {}
  ): SessionInfoEntry => ({
    sessionId: id,
    cwd: '/w',
    ...extra,
  });

  it('live entries win on conflicts', async () => {
    const disk = [mk('a', { title: 'disk title' })];
    const live = [mk('a', { title: 'live title', status: 'idle' })];

    const merged = mergeSessionListings(live, disk);
    expect(merged).toHaveLength(1);
    expect(merged[0]!.title).toBe('live title');
    expect(merged[0]!.status).toBe('idle');
  });

  it('live entries keep stronger disk fields they omit or abbreviate', async () => {
    const disk = [mk('a', { cwd: '/workspace/real', title: 'disk title' })];
    const live = [
      { sessionId: 'a', cwd: '~', status: 'idle' } as SessionInfoEntry,
    ];

    const merged = mergeSessionListings(live, disk);
    expect(merged).toHaveLength(1);
    expect(merged[0]!.cwd).toBe('/workspace/real');
    expect(merged[0]!.title).toBe('disk title');
    expect(merged[0]!.status).toBe('idle');
  });

  it('canonicalizes shorthand on live-only rows', async () => {
    const merged = mergeSessionListings([mk('home', { cwd: '~' })], []);
    expect(merged[0]!.cwd).toBe(homedir());
  });

  it('union of disjoint listings', async () => {
    const disk = [mk('disk-only')];
    const live = [mk('live-only')];

    const merged = mergeSessionListings(live, disk);
    expect(merged).toHaveLength(2);
  });

  it('handles empty inputs', async () => {
    expect(mergeSessionListings([], [])).toHaveLength(0);
    expect(mergeSessionListings([mk('a')], [])).toHaveLength(1);
    expect(mergeSessionListings([], [mk('a')])).toHaveLength(1);
  });

  it('preserves exact-id copies from different physical stores', () => {
    const disk = [
      { ...mk('same'), engine: 'classic' as const, title: 'classic' },
      { ...mk('same'), engine: 'v2' as const, title: 'v2' },
      { ...mk('same'), engine: 'v3' as const, title: 'v3 disk' },
    ];
    const live = [
      { ...mk('same'), engine: 'v3' as const, status: 'idle' as const },
    ];

    const merged = mergeSessionListings(live, disk);
    expect(merged).toHaveLength(3);
    expect(merged.map((entry) => entry.engine).sort()).toEqual([
      'classic',
      'v2',
      'v3',
    ]);
    expect(merged.find((entry) => entry.engine === 'v3')?.title).toBe(
      'v3 disk'
    );
  });

  it('preserves exact-id rows from local and remote discovery sources', () => {
    const merged = mergeSessionListings(
      [
        {
          ...mk('same-source-id'),
          engine: 'v3',
          source: 'remote',
          title: 'remote',
        },
      ],
      [
        {
          ...mk('same-source-id'),
          engine: 'v3',
          source: 'local',
          title: 'local',
        },
      ]
    );

    expect(merged).toHaveLength(2);
    expect(merged.map((entry) => entry.source).sort()).toEqual([
      'local',
      'remote',
    ]);
  });

  it('merges sess_-prefixed and bare spellings of the same id', async () => {
    // Disk scan reports the directory name; the live listing reports the
    // bare id. One session must yield one row, keeping the live id.
    const disk = [mk('sess_7b76aebe', { title: 'Rust guessing game' })];
    const live = [mk('7b76aebe', { status: 'idle' })];

    const merged = mergeSessionListings(live, disk);
    expect(merged).toHaveLength(1);
    expect(merged[0]!.sessionId).toBe('7b76aebe');
    expect(merged[0]!.title).toBe('Rust guessing game');
    expect(merged[0]!.status).toBe('idle');
  });
});

describe('validated KAS copy authority and partial cache retention', () => {
  let root: string;

  beforeEach(() => {
    resetAllWorkspaceSessionsCache();
    root = mkdtempSync(join(tmpdir(), 'kas-authority-'));
  });

  afterEach(() => {
    resetAllWorkspaceSessionsCache();
    rmSync(root, { recursive: true, force: true });
  });

  const writeCopy = (
    hash: string,
    id: string,
    metadata: Record<string, unknown>
  ): string => {
    const dir = join(root, hash, `sess_${id}`);
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({ id: `sess_${id}`, ...metadata })
    );
    return dir;
  };

  it('rejects malformed placement metadata instead of treating it as local', async () => {
    writeCopy('hash', 'unsafe', {
      workspacePaths: ['/workspace'],
      executionTarget: {},
    });

    const result = await listAllWorkspaceSessionsFromDiskDetailed(root);

    expect(result.sessions).toEqual([]);
    expect(result.complete).toBe(false);
  });

  it('uses one workspace-bearing copy without field-merging another copy', async () => {
    writeCopy('_global', 'shared', {
      title: 'Global title must not leak',
      workspacePaths: [],
      lastModifiedAt: '2026-08-01T12:00:00.000Z',
    });
    writeCopy('workspace', 'shared', {
      workspacePaths: ['/workspace/canonical'],
      lastModifiedAt: '2026-07-01T12:00:00.000Z',
    });

    const result = await listAllWorkspaceSessionsFromDiskDetailed(root);

    expect(result.sessions).toEqual([
      expect.objectContaining({
        sessionId: 'shared',
        cwd: '/workspace/canonical',
        engine: 'v3',
      }),
    ]);
    expect(result.sessions[0]?.title).toBeUndefined();
  });

  it('retains the last complete catalog when a refresh is partial', async () => {
    mkdirSync(join(root, 'cli'), { recursive: true });
    writeFileSync(
      join(root, 'cli', 'retained.json'),
      JSON.stringify({ cwd: '/retained' })
    );
    const complete = await scanAllWorkspaceSessionsDetailed(root);
    expect(complete.complete).toBe(true);

    writeFileSync(join(root, 'cli', 'retained.json'), '{broken');
    writeFileSync(
      join(root, 'cli', 'fresh.json'),
      JSON.stringify({ cwd: '/fresh' })
    );
    const partial = await scanAllWorkspaceSessionsDetailed(root);

    expect(partial.complete).toBe(false);
    expect(partial.sessions.map((session) => session.sessionId).sort()).toEqual(
      ['fresh', 'retained']
    );
    expect(
      getCachedAllWorkspaceSessions()
        .map((session) => session.sessionId)
        .sort()
    ).toEqual(['fresh', 'retained']);
  });

  it('retains classic rows after invalidation when fetching fails', async () => {
    const complete = await fetchClassicSessionsDetailed(async () => ({
      ok: true,
      complete: true,
      envelopes: [
        {
          cwd: '/classic',
          sessions: [
            {
              sessionId: 'classic-retained',
              source: 'classic',
              title: 'Retained classic session',
              updatedAt: '2026-08-01T00:00:00.000Z',
            },
          ],
        },
      ],
    }));
    expect(complete.complete).toBe(true);

    invalidateAllWorkspaceSessionsCache();
    const rejected = await fetchClassicSessionsDetailed(async () => ({
      ok: false,
      error: 'listing unavailable',
    }));
    expect(rejected.sessions.map((session) => session.sessionId)).toEqual([
      'classic-retained',
    ]);

    const thrown = await fetchClassicSessionsDetailed(async () => {
      throw new Error('spawn failed');
    });
    expect(thrown.sessions.map((session) => session.sessionId)).toEqual([
      'classic-retained',
    ]);
  });
});
