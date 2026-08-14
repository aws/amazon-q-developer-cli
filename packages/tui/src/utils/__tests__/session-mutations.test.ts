import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  rmSync,
  symlinkSync,
  existsSync,
  utimesSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { SessionBookmarkStore } from '../session-bookmarks';
import {
  deleteLocalKasSessionWithAgent,
  deleteSession,
  gcScan,
  gcEmptySessions,
} from '../session-mutations';

let root: string;

function v2Session(
  id: string,
  opts: {
    cwd?: string;
    withPrompt?: boolean;
    withCompaction?: boolean;
    lockPid?: number;
  } = {}
): void {
  const cliDir = join(root, 'cli');
  mkdirSync(cliDir, { recursive: true });
  writeFileSync(
    join(cliDir, `${id}.json`),
    JSON.stringify({ session_id: id, cwd: opts.cwd ?? '/w' })
  );
  const entries = opts.withPrompt
    ? [
        JSON.stringify({
          version: 'v1',
          kind: 'Prompt',
          data: { message_id: 'm1', content: [{ kind: 'text', data: 'hi' }] },
        }),
      ]
    : opts.withCompaction
      ? [
          JSON.stringify({
            version: 'v1',
            kind: 'Compaction',
            data: {
              summary: 'imported v1 conversation',
              strategy: 'default',
              messages_snapshot: [{ role: 'user', content: 'hello' }],
            },
          }),
        ]
      : [
          JSON.stringify({
            version: 'v1',
            kind: 'Clear',
            data: {},
          }),
        ];
  writeFileSync(join(cliDir, `${id}.jsonl`), entries.join('\n') + '\n');
  if (opts.lockPid !== undefined) {
    writeFileSync(
      join(cliDir, `${id}.lock`),
      JSON.stringify({
        pid: opts.lockPid,
        started_at: new Date().toISOString(),
      })
    );
  }
}

function kasSession(
  id: string,
  opts: {
    workspace?: string;
    title?: string;
    old?: boolean;
    transcript?: boolean;
    executionTarget?: { kind: string };
  } = {}
): string {
  const dir = join(root, 'hash1', `sess_${id}`);
  mkdirSync(dir, { recursive: true });
  const metaPath = join(dir, 'session.json');
  writeFileSync(
    metaPath,
    JSON.stringify({
      id: `sess_${id}`,
      title: opts.title ?? 'New Session',
      workspacePaths: [opts.workspace ?? '/w'],
      ...(opts.executionTarget
        ? { executionTarget: opts.executionTarget }
        : {}),
    })
  );
  // Emptiness is judged by the transcript, not the title — a session with
  // conversation must carry one for the fixture to be realistic.
  const logPath = join(dir, 'messages.jsonl');
  writeFileSync(
    logPath,
    opts.transcript
      ? JSON.stringify({ type: 'turn_start', executionId: 'e1' }) + '\n'
      : ''
  );
  if (opts.old) {
    // Backdate mtime beyond the 1h recency guard.
    const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
    utimesSync(metaPath, past, past);
    utimesSync(logPath, past, past);
  }
  return dir;
}

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'mutations-'));
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

describe('deleteSession', () => {
  it('deletes an unlocked V2 session (all file parts)', () => {
    v2Session('s1');
    const out = deleteSession('s1', null, root);
    expect(out).toEqual({ ok: true, store: 'v2' });
    expect(existsSync(join(root, 'cli', 's1.json'))).toBe(false);
    expect(existsSync(join(root, 'cli', 's1.jsonl'))).toBe(false);
  });

  it('preserves a sess_-prefixed V2 physical id during deletion', () => {
    v2Session('sess_x');
    v2Session('x');

    expect(deleteSession('sess_x', null, root, 'v2')).toEqual({
      ok: true,
      store: 'v2',
    });
    expect(existsSync(join(root, 'cli', 'sess_x.json'))).toBe(false);
    expect(existsSync(join(root, 'cli', 'x.json'))).toBe(true);
  });

  it('refuses a V2 session locked by a live process', () => {
    v2Session('s1', { lockPid: process.pid }); // our own PID is alive
    const out = deleteSession('s1', null, root);
    expect(out).toEqual({ ok: false, reason: 'locked' });
    expect(existsSync(join(root, 'cli', 's1.json'))).toBe(true);
  });

  it('refuses a V2 session when a competing lock wins acquisition', () => {
    v2Session('s1');
    const lockPath = join(root, 'cli', 's1.lock');
    const originalNow = Date.now;
    let competed = false;
    Date.now = () => {
      if (!competed) {
        competed = true;
        writeFileSync(
          lockPath,
          JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
        );
      }
      return originalNow();
    };
    try {
      expect(deleteSession('s1', null, root)).toEqual({
        ok: false,
        reason: 'locked',
      });
      expect(existsSync(join(root, 'cli', 's1.json'))).toBe(true);
    } finally {
      Date.now = originalNow;
    }
  });

  it('treats a dead-PID lock as stale and deletes', () => {
    v2Session('s1', { lockPid: 999999999 }); // certainly dead
    const out = deleteSession('s1', null, root);
    expect(out).toEqual({ ok: true, store: 'v2' });
  });

  it('fails closed for a malformed V2 lock', () => {
    v2Session('s1');
    writeFileSync(join(root, 'cli', 's1.lock'), '{"pid":');
    expect(deleteSession('s1', null, root)).toEqual({
      ok: false,
      reason: 'locked',
    });
    expect(existsSync(join(root, 'cli', 's1.json'))).toBe(true);
  });

  it('rejects traversal-shaped ids without touching files outside cli', () => {
    const sentinel = join(root, 'sentinel.json');
    writeFileSync(sentinel, 'keep');
    expect(deleteSession('../sentinel', null, root)).toEqual({
      ok: false,
      reason: 'invalid-id',
    });
    expect(existsSync(sentinel)).toBe(true);
  });

  it('refuses a V2 store symlink that escapes the sessions root', () => {
    const outside = mkdtempSync(join(tmpdir(), 'mutations-outside-'));
    try {
      writeFileSync(join(outside, 's1.json'), '{}');
      symlinkSync(outside, join(root, 'cli'), 'dir');

      expect(deleteSession('s1', null, root, 'v2')).toEqual({
        ok: false,
        reason: 'not-found',
      });
      expect(existsSync(join(outside, 's1.json'))).toBe(true);
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });

  it('refuses a V2 metadata symlink without removing its target', () => {
    const outside = join(root, 'outside.json');
    mkdirSync(join(root, 'cli'));
    writeFileSync(outside, '{"sentinel":true}');
    symlinkSync(outside, join(root, 'cli', 'linked.json'));

    expect(deleteSession('linked', null, root, 'v2')).toEqual({
      ok: false,
      reason: 'not-found',
    });
    expect(existsSync(outside)).toBe(true);
  });

  it('validates every V2 leaf before deleting any of them', () => {
    v2Session('linked-tail');
    const target = join(root, 'outside.jsonl');
    const transcript = join(root, 'cli', 'linked-tail.jsonl');
    writeFileSync(target, 'sentinel transcript');
    rmSync(transcript);
    symlinkSync(target, transcript);

    expect(deleteSession('linked-tail', null, root, 'v2')).toEqual({
      ok: false,
      reason: 'invalid-id',
    });
    expect(existsSync(join(root, 'cli', 'linked-tail.json'))).toBe(true);
    expect(existsSync(target)).toBe(true);
  });

  it('deletes the requested physical store when ids are identical', () => {
    v2Session('same-id');
    const kas = kasSession('same-id', { old: true });

    expect(deleteSession('same-id', null, root, 'kas')).toEqual({
      ok: true,
      store: 'kas',
    });
    expect(existsSync(kas)).toBe(false);
    expect(existsSync(join(root, 'cli', 'same-id.json'))).toBe(true);

    expect(deleteSession('same-id', null, root, 'v2')).toEqual({
      ok: true,
      store: 'v2',
    });
  });

  it('refuses the active session', () => {
    v2Session('s1');
    const out = deleteSession('s1', 's1', root);
    expect(out).toEqual({ ok: false, reason: 'active' });
    expect(existsSync(join(root, 'cli', 's1.json'))).toBe(true);
  });

  it('refuses KAS active-session aliases', () => {
    const dir = kasSession('k1', { old: true });
    const out = deleteSession('k1', 'sess_k1', root, 'kas');
    expect(out).toEqual({ ok: false, reason: 'active' });
    expect(existsSync(dir)).toBe(true);
  });

  it('deletes an old KAS-native session dir', () => {
    const dir = kasSession('k1', { old: true });
    const out = deleteSession('sess_k1', null, root);
    expect(out).toEqual({ ok: true, store: 'kas' });
    expect(existsSync(dir)).toBe(false);
  });

  it('fails closed when KAS metadata identity changes before removal', () => {
    const dir = kasSession('k1', { old: true });
    const metaPath = join(dir, 'session.json');
    const originalNow = Date.now;
    Date.now = () => {
      writeFileSync(metaPath, JSON.stringify({ id: 'sess_other' }));
      return originalNow();
    };
    try {
      expect(deleteSession('k1', null, root, 'kas')).toEqual({
        ok: false,
        reason: 'error',
      });
      expect(existsSync(dir)).toBe(true);
    } finally {
      Date.now = originalNow;
    }
  });

  it('fails closed when KAS placement changes to cloud before removal', () => {
    const dir = kasSession('k1', { old: true });
    const metaPath = join(dir, 'session.json');
    const originalNow = Date.now;
    Date.now = () => {
      writeFileSync(
        metaPath,
        JSON.stringify({
          id: 'sess_k1',
          executionTarget: { kind: 'cloud-sandbox' },
        })
      );
      return originalNow();
    };
    try {
      expect(deleteSession('k1', null, root, 'kas')).toEqual({
        ok: false,
        reason: 'cloud',
      });
      expect(existsSync(dir)).toBe(true);
    } finally {
      Date.now = originalNow;
    }
  });

  it('refuses a recently-modified KAS session (recency guard)', () => {
    const dir = kasSession('k1'); // fresh mtime
    const out = deleteSession('sess_k1', null, root);
    expect(out).toEqual({ ok: false, reason: 'recent' });
    expect(existsSync(dir)).toBe(true);
  });

  it('refuses a KAS session when a competing lock wins acquisition', () => {
    const dir = kasSession('locked-race', { old: true });
    const originalNow = Date.now;
    let competed = false;
    Date.now = () => {
      if (!competed) {
        competed = true;
        writeFileSync(
          join(dir, '.lock'),
          JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
        );
      }
      return originalNow();
    };
    try {
      expect(deleteSession('locked-race', null, root, 'kas')).toEqual({
        ok: false,
        reason: 'locked',
      });
      expect(existsSync(dir)).toBe(true);
    } finally {
      Date.now = originalNow;
    }
  });

  it('refuses a KAS session locked by another live process', () => {
    // PID 1 is always alive and owned by root — the signal-0 probe gets
    // EPERM, which must read as "alive".
    const dir = kasSession('lockedk', { old: true, transcript: true });
    writeFileSync(
      join(dir, '.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );
    const out = deleteSession('sess_lockedk', null, root);
    expect(out).toEqual({ ok: false, reason: 'locked' });
    expect(existsSync(dir)).toBe(true);
  });

  it('reports not-found for unknown ids', () => {
    expect(deleteSession('nope', null, root)).toEqual({
      ok: false,
      reason: 'not-found',
    });
  });

  it('refuses a cloud-placed KAS session (local record is a pointer)', () => {
    const dir = kasSession('kcloud', {
      old: true,
      executionTarget: { kind: 'cloud-sandbox' },
    });
    const out = deleteSession('sess_kcloud', null, root);
    expect(out).toEqual({ ok: false, reason: 'cloud' });
    expect(existsSync(dir)).toBe(true);
  });

  it('fails closed when KAS placement metadata is malformed', () => {
    const dir = kasSession('kunknown', { old: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({
        id: 'sess_kunknown',
        workspacePaths: ['/w'],
        executionTarget: { kind: 7 },
      })
    );

    const out = deleteSession('sess_kunknown', null, root);

    expect(out).toEqual({ ok: false, reason: 'error' });
    expect(existsSync(dir)).toBe(true);
  });
});

describe('deleteLocalKasSessionWithAgent', () => {
  it('holds every local KAS deletion lock through the agent callback', async () => {
    const first = kasSession('agent-locked', { old: true });
    const second = join(root, 'hash2', 'sess_agent-locked');
    mkdirSync(second, { recursive: true });
    writeFileSync(
      join(second, 'session.json'),
      JSON.stringify({
        id: 'sess_agent-locked',
        workspacePaths: ['/w'],
      })
    );
    writeFileSync(join(second, 'messages.jsonl'), '');
    const past = new Date(Date.now() - 60 * 60 * 1000);
    utimesSync(join(second, 'session.json'), past, past);
    utimesSync(join(second, 'messages.jsonl'), past, past);

    const result = await deleteLocalKasSessionWithAgent(
      'agent-locked',
      null,
      async () => {
        expect(existsSync(join(first, '.lock'))).toBe(true);
        expect(existsSync(join(second, '.lock'))).toBe(true);
        return true;
      },
      root
    );

    expect(result).toEqual({ ok: true, store: 'kas' });
    expect(existsSync(join(first, '.lock'))).toBe(false);
    expect(existsSync(join(second, '.lock'))).toBe(false);
  });

  it('does not call the agent when a local KAS deletion lock is held', async () => {
    const dir = kasSession('agent-blocked', { old: true });
    writeFileSync(
      join(dir, '.lock'),
      JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() })
    );
    let calls = 0;

    const result = await deleteLocalKasSessionWithAgent(
      'agent-blocked',
      null,
      async () => {
        calls += 1;
        return true;
      },
      root
    );

    expect(result).toEqual({ ok: false, reason: 'locked' });
    expect(calls).toBe(0);
    expect(existsSync(dir)).toBe(true);
  });

  it('refuses a recently active session without calling the agent', async () => {
    // A session created moments ago in another terminal may not hold its
    // lock yet — recency is the backstop.
    const dir = kasSession('agent-fresh');
    let calls = 0;

    const result = await deleteLocalKasSessionWithAgent(
      'agent-fresh',
      null,
      async () => {
        calls += 1;
        return true;
      },
      root
    );

    expect(result).toEqual({ ok: false, reason: 'recent' });
    expect(calls).toBe(0);
    expect(existsSync(dir)).toBe(true);
  });
});

describe('gcScan', () => {
  it('finds empty V2 and empty KAS sessions, keeping ones with content', async () => {
    v2Session('empty1'); // no prompt
    v2Session('full1', { withPrompt: true });
    kasSession('kempty', { old: true }); // no transcript + old
    kasSession('kfull', { title: 'real work', old: true, transcript: true });

    const scan = await gcScan(null, new Set(), root);
    const ids = scan.candidates.map((c) => c.sessionId).sort();
    expect(ids).toEqual(['empty1', 'kempty']);
  });

  it('keeps a compacted imported-V1 session (Compaction entry, no Prompt)', async () => {
    v2Session('compacted', { withCompaction: true });

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates).toHaveLength(0);
  });

  it('keeps an untitled KAS session that has a transcript', async () => {
    kasSession('untitled_with_content', { old: true, transcript: true });

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates).toHaveLength(0);
  });

  it('never surfaces cloud-placed sessions as GC candidates', async () => {
    kasSession('kcloud', {
      old: true,
      executionTarget: { kind: 'cloud-sandbox' },
    });

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates).toHaveLength(0);
  });

  it('never surfaces malformed placement metadata as a GC candidate', async () => {
    const dir = kasSession('kunknown', { old: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({
        id: 'sess_kunknown',
        workspacePaths: ['/w'],
        executionTarget: {},
      })
    );

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates).toHaveLength(0);
  });

  it('exempts user-touched sessions', async () => {
    v2Session('empty1');
    const scan = await gcScan(null, new Set(['empty1']), root);
    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.userTouched).toBe(1);
  });

  it('exempts the active session', async () => {
    v2Session('empty1');
    const scan = await gcScan('empty1', new Set(), root);
    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.active).toBe(1);
  });

  it('counts locked V2 sessions as skipped', async () => {
    v2Session('empty1', { lockPid: process.pid });
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.locked).toBe(1);
  });

  it('counts recent KAS sessions as skipped', async () => {
    kasSession('fresh'); // fresh placeholder
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.recent).toBe(1);
  });

  it('skips a KAS session locked by another live process', async () => {
    // PID 1 is always alive and owned by root — the signal-0 probe gets
    // EPERM, which must read as "alive".
    const dir = kasSession('lockedk', { old: true });
    writeFileSync(
      join(dir, '.lock'),
      JSON.stringify({ pid: 1, startedAt: new Date().toISOString() })
    );
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.locked).toBe(1);
  });

  it('does not classify a corrupt transcript as empty (V2)', async () => {
    v2Session('corrupt');
    writeFileSync(
      join(root, 'cli', 'corrupt.jsonl'),
      'not-json at all\n{"broken\n'
    );
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates.map((c) => c.sessionId)).not.toContain('corrupt');
  });

  it('does not classify a corrupt transcript as empty (KAS)', async () => {
    const dir = kasSession('corruptk', { old: true });
    writeFileSync(join(dir, 'messages.jsonl'), '\u0000\u0000 garbage\n');
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates.map((c) => c.sessionId)).not.toContain('corruptk');
  });

  it('ignores KAS metadata with an invalid session id', async () => {
    const dir = kasSession('safe-dir', { old: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({ id: '../../outside', workspacePaths: ['/w'] })
    );
    const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
    utimesSync(join(dir, 'session.json'), past, past);

    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(0);
  });

  it('ignores KAS metadata whose id names another physical session', async () => {
    const dirA = kasSession('a', { old: true });
    kasSession('b', { old: true });
    writeFileSync(
      join(dirA, 'session.json'),
      JSON.stringify({ id: 'sess_b', workspacePaths: ['/w'] })
    );
    const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
    utimesSync(join(dirA, 'session.json'), past, past);

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates.map((candidate) => candidate.sessionId)).toEqual([
      'b',
    ]);
  });

  it('filters by workspace when given', async () => {
    v2Session('a', { cwd: '/w/alpha' });
    v2Session('b', { cwd: '/w/beta' });
    const scan = await gcScan(null, new Set(), root, '/w/alpha');
    expect(scan.candidates.map((c) => c.sessionId)).toEqual(['a']);
  });

  it('handles an empty root', async () => {
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(0);
  });
});

describe('gcEmptySessions', () => {
  it('deletes scanned candidates and reports counts', async () => {
    v2Session('e1');
    v2Session('e2');
    kasSession('ke', { old: true });

    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(3);

    const result = await gcEmptySessions(scan.candidates, null, root);
    expect(result.deleted).toBe(3);
    expect(result.failed).toBe(0);
    expect(existsSync(join(root, 'cli', 'e1.json'))).toBe(false);
    expect(existsSync(join(root, 'hash1', 'sess_ke'))).toBe(false);
  });

  it('re-applies guards at deletion time (tolerates state change)', async () => {
    v2Session('e1');
    const scan = await gcScan(null, new Set(), root);
    // Session becomes locked between scan and delete.
    writeFileSync(
      join(root, 'cli', 'e1.lock'),
      JSON.stringify({ pid: process.pid, started_at: new Date().toISOString() })
    );
    const result = await gcEmptySessions(scan.candidates, null, root);
    expect(result.deleted).toBe(0);
    expect(result.failed).toBe(1);
    expect(existsSync(join(root, 'cli', 'e1.json'))).toBe(true);
  });

  it('routes KAS candidates through the agent deleter when provided', async () => {
    kasSession('ka', { old: true });
    v2Session('e1'); // V2 rows never route through the agent
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(2);

    const agentDeleted: string[] = [];
    let agentHeldLock = false;
    const result = await gcEmptySessions(
      scan.candidates,
      null,
      root,
      async (id) => {
        agentDeleted.push(id);
        const lockPath = join(root, 'hash1', 'sess_ka', '.lock');
        agentHeldLock = existsSync(lockPath);
        return true; // agent handled it (removes the dir itself in production)
      }
    );

    expect(agentDeleted).toEqual(['ka']);
    expect(agentHeldLock).toBe(true);
    expect(existsSync(join(root, 'hash1', 'sess_ka', '.lock'))).toBe(false);
    expect(result.deleted).toBe(2);
    // V2 candidate still went through the disk path.
    expect(existsSync(join(root, 'cli', 'e1.json'))).toBe(false);
  });

  it('never agent-deletes a KAS alias of the active session', async () => {
    kasSession('ka', { old: true });
    const scan = await gcScan(null, new Set(), root);
    const agentDeleted: string[] = [];

    const result = await gcEmptySessions(
      scan.candidates,
      'sess_ka',
      root,
      async (id) => {
        agentDeleted.push(id);
        return true;
      }
    );

    expect(agentDeleted).toEqual([]);
    expect(result.deleted).toBe(0);
    expect(result.failed).toBe(1);
    expect(existsSync(join(root, 'hash1', 'sess_ka'))).toBe(true);
  });

  it('falls back to disk when the agent deleter refuses', async () => {
    const dir = kasSession('ka', { old: true });
    const scan = await gcScan(null, new Set(), root);

    const result = await gcEmptySessions(
      scan.candidates,
      null,
      root,
      async () => false
    );

    expect(result.deleted).toBe(1);
    expect(existsSync(dir)).toBe(false);
  });
});

describe('directory-name shapes and parent exemption', () => {
  function kasDirNamed(
    dirName: string,
    opts: { parent?: string; old?: boolean } = {}
  ): string {
    const dir = join(root, 'hash1', dirName);
    mkdirSync(dir, { recursive: true });
    const metaPath = join(dir, 'session.json');
    writeFileSync(
      metaPath,
      JSON.stringify({
        id: dirName,
        title: 'New Session',
        workspacePaths: ['/w'],
        ...(opts.parent ? { parentSessionId: opts.parent } : {}),
      })
    );
    const logPath = join(dir, 'messages.jsonl');
    writeFileSync(logPath, '');
    if (opts.old) {
      const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
      utimesSync(metaPath, past, past);
      utimesSync(logPath, past, past);
    }
    return dir;
  }

  it('scans and deletes sessions whose dirs lack the sess_ prefix', async () => {
    kasDirNamed('0f9e8d7c-aaaa-bbbb-cccc-121314151617', { old: true });
    kasDirNamed('cli_deadbeef_conv', { old: true });

    const scan = await gcScan(null, new Set(), root);
    expect(new Set(scan.candidates.map((c) => c.sessionId))).toEqual(
      new Set(['0f9e8d7c-aaaa-bbbb-cccc-121314151617', 'cli_deadbeef_conv'])
    );

    const out = deleteSession('cli_deadbeef_conv', null, root);
    expect(out).toEqual({ ok: true, store: 'kas' });
    expect(existsSync(join(root, 'hash1', 'cli_deadbeef_conv'))).toBe(false);
  });

  it('exempts sessions that have a parent from cleanup', async () => {
    kasDirNamed('child-1', { parent: 'parent-9', old: true });
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.hasParent).toBe(1);
  });

  it('exempts every copy when any physical copy has a parent', async () => {
    kasDirNamed('child-copy', { old: true });
    const second = join(root, 'hash2', 'child-copy');
    mkdirSync(second, { recursive: true });
    const metaPath = join(second, 'session.json');
    const logPath = join(second, 'messages.jsonl');
    writeFileSync(
      metaPath,
      JSON.stringify({
        id: 'child-copy',
        workspacePaths: ['/w'],
        parentSessionId: 'parent-9',
      })
    );
    writeFileSync(logPath, '');
    const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
    utimesSync(metaPath, past, past);
    utimesSync(logPath, past, past);

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.hasParent).toBe(1);
  });

  it('exempts V2 subagent children from cleanup', async () => {
    const cliDir = join(root, 'cli');
    mkdirSync(cliDir, { recursive: true });
    writeFileSync(
      join(cliDir, 'kid.json'),
      JSON.stringify({ session_id: 'kid', cwd: '/w', parent_session_id: 'p1' })
    );
    writeFileSync(join(cliDir, 'kid.jsonl'), '');
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(0);
    expect(scan.skipped.hasParent).toBe(1);
  });
});

describe('gcEmptySessions bookmark revalidation', () => {
  it('preserves a session marked after the dry-run scan', async () => {
    v2Session('marked-after-scan');
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates.map((candidate) => candidate.sessionId)).toEqual([
      'marked-after-scan',
    ]);

    const bookmarks = new SessionBookmarkStore(
      join(root, 'dashboard-meta.json')
    );
    bookmarks.setTitle('marked-after-scan', 'Keep this session');

    const result = await gcEmptySessions(scan.candidates, null, root);

    expect(result).toEqual({ deleted: 0, failed: 0, stale: 1 });
    expect(existsSync(join(root, 'cli', 'marked-after-scan.json'))).toBe(true);
  });
});
