import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { Database } from 'bun:sqlite';
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import {
  SessionSearchIndex,
  getSessionSearchIndex,
  resetSessionSearchIndex,
} from '../session-search';

function createTestSessionsDir(): string {
  const dir = mkdtempSync(join(tmpdir(), 'session-search-test-'));
  return dir;
}

function writeSessionMeta(
  dir: string,
  sessionId: string,
  meta: Record<string, unknown>
): void {
  writeFileSync(
    join(dir, `${sessionId}.json`),
    JSON.stringify({
      session_id: sessionId,
      cwd: '/workspace/test',
      created_at: '2026-07-20T10:00:00.000Z',
      updated_at: '2026-07-20T12:00:00.000Z',
      ...meta,
    })
  );
}

function writeSessionLog(
  dir: string,
  sessionId: string,
  entries: Array<{ kind: string; data: unknown }>
): void {
  const lines = entries.map((e) => JSON.stringify({ version: 'v1', ...e }));
  writeFileSync(join(dir, `${sessionId}.jsonl`), lines.join('\n') + '\n');
}

describe('SessionSearchIndex', () => {
  let testDir: string;

  beforeEach(() => {
    testDir = createTestSessionsDir();
  });

  afterEach(() => {
    rmSync(testDir, { recursive: true, force: true });
    resetSessionSearchIndex();
  });

  it('builds index from session files', async () => {
    writeSessionMeta(testDir, 'session-1', { title: 'Fix auth bug' });
    writeSessionLog(testDir, 'session-1', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [
            { kind: 'text', data: 'Fix the authentication issue in login.ts' },
          ],
        },
      },
      {
        kind: 'AssistantMessage',
        data: {
          message_id: 'm2',
          content: [
            {
              kind: 'text',
              data: 'I found the bug in the token validation logic.',
            },
          ],
        },
      },
    ]);

    const index = new SessionSearchIndex(testDir);
    await index.build();

    expect(index.size).toBe(1);
    const doc = index.getDocument('session-1');
    expect(doc).toBeDefined();
    expect(doc!.title).toBe('Fix auth bug');
    expect(doc!.promptCount).toBe(1);
    expect(doc!.entryCount).toBe(1);
    // Prompt text is searchable through the content index, not retained.
    const hits = index.search('authentication');
    expect(hits.map((h) => h.sessionId)).toContain('session-1');
  });

  it('hydrates prompt title and count from the persistent index on a fresh instance', async () => {
    writeSessionMeta(testDir, 'warm', { title: '' });
    writeSessionLog(testDir, 'warm', [
      {
        kind: 'Prompt',
        data: {
          content: [{ kind: 'text', data: 'persistent prompt title' }],
        },
      },
      {
        kind: 'Prompt',
        data: { content: [{ kind: 'text', data: 'second prompt' }] },
      },
      {
        kind: 'AssistantMessage',
        data: { content: [{ kind: 'text', data: 'x'.repeat(600_000) }] },
      },
    ]);

    const first = new SessionSearchIndex(testDir);
    await first.build();
    first.close();

    const fresh = new SessionSearchIndex(testDir);
    await fresh.build();
    expect(fresh.getDocument('warm')?.title).toBe('persistent prompt title');
    expect(fresh.getDocument('warm')?.promptCount).toBe(2);
    fresh.close();
  });

  it('skips subagent sessions', async () => {
    writeSessionMeta(testDir, 'parent-1', { title: 'Parent session' });
    writeSessionLog(testDir, 'parent-1', [
      {
        kind: 'Prompt',
        data: { message_id: 'm1', content: [{ kind: 'text', data: 'hello' }] },
      },
    ]);

    writeSessionMeta(testDir, 'subagent-1', {
      title: 'Subagent session',
      parent_session_id: 'parent-1',
      session_created_reason: 'subagent',
    });
    writeSessionLog(testDir, 'subagent-1', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [{ kind: 'text', data: 'sub task' }],
        },
      },
    ]);

    const index = new SessionSearchIndex(testDir);
    await index.build();

    expect(index.size).toBe(1);
    expect(index.getDocument('parent-1')).toBeDefined();
    expect(index.getDocument('subagent-1')).toBeUndefined();
  });

  it('indexes an oversized-metadata session as a degraded row with a prompt title', async () => {
    // Seed a session that will vanish; its stale index rows are swept only
    // when the listing counts as complete.
    writeSessionMeta(testDir, 'gone', { title: 'Gone session' });
    writeSessionLog(testDir, 'gone', [
      {
        kind: 'Prompt',
        data: { content: [{ kind: 'text', data: 'ephemeral zebra content' }] },
      },
    ]);
    const seed = new SessionSearchIndex(testDir);
    await seed.build();
    seed.close();
    rmSync(join(testDir, 'gone.json'));
    rmSync(join(testDir, 'gone.jsonl'));

    writeFileSync(
      join(testDir, 'big.json'),
      `{"session_id":"big","title":"Big","cwd":"/w","filler":"${'x'.repeat(1024 * 1024)}"}`
    );
    writeSessionLog(testDir, 'big', [
      {
        kind: 'Prompt',
        data: { content: [{ kind: 'text', data: 'giant metadata prompt' }] },
      },
    ]);

    const index = new SessionSearchIndex(testDir);
    await index.build();

    // Oversized metadata degrades to a minimal row instead of vanishing;
    // its transcript still yields a searchable, prompt-derived title.
    const doc = index.getDocument('big');
    expect(doc).toBeDefined();
    expect(doc!.title).toBe('giant metadata prompt');
    expect(doc!.workspace).toBe('');
    expect(index.search('giant').map((h) => h.sessionId)).toContain('big');
    // The oversized row no longer marks the listing incomplete, so the
    // vanished session's stale index rows were swept.
    expect(index.search('zebra')).toHaveLength(0);
    index.close();
  });

  it('counts prompts but not assistant messages', async () => {
    writeSessionMeta(testDir, 'session-tools', { title: 'Tools session' });
    writeSessionLog(testDir, 'session-tools', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [{ kind: 'text', data: 'write a file' }],
        },
      },
      {
        kind: 'AssistantMessage',
        data: {
          message_id: 'm2',
          content: [
            {
              kind: 'tool_use',
              data: { name: 'fs_write', input: { path: '/tmp/test.ts' } },
            },
          ],
        },
      },
    ]);

    const index = new SessionSearchIndex(testDir);
    await index.build();

    const doc = index.getDocument('session-tools');
    expect(doc!.promptCount).toBe(1);
    expect(doc!.entryCount).toBe(1);
  });

  it('titles from the first prompt line, skipping code fences', async () => {
    writeSessionMeta(testDir, 's-multiline', { title: '' });
    writeSessionLog(testDir, 's-multiline', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [
            {
              kind: 'text',
              data: '```\nconst pasted = true;\n```\nwhy does this throw?\nmore detail below',
            },
          ],
        },
      },
    ]);

    const index = new SessionSearchIndex(testDir);
    await index.build();

    const doc = index.getDocument('s-multiline');
    // First usable line only — not the fenced code, not the later lines.
    expect(doc!.title).toBe('why does this throw?');
    expect(doc!.title).not.toContain('\n');
    index.close();
  });

  it('handles malformed JSONL lines gracefully', async () => {
    writeSessionMeta(testDir, 'session-bad', { title: 'Bad logs' });
    writeFileSync(
      join(testDir, 'session-bad.jsonl'),
      '{"version":"v1","kind":"Prompt","data":{"message_id":"m1","content":[{"kind":"text","data":"valid"}]}}\n' +
        'not valid json\n' +
        '{"broken": true}\n'
    );

    const index = new SessionSearchIndex(testDir);
    await index.build();

    expect(index.size).toBe(1);
    const doc = index.getDocument('session-bad');
    expect(doc!.promptCount).toBe(1);
    const hits = index.search('valid');
    expect(hits.map((h) => h.sessionId)).toContain('session-bad');
  });

  it('skips symlinked V2 metadata leaves', async () => {
    const outside = mkdtempSync(join(tmpdir(), 'session-search-outside-'));
    try {
      const target = join(outside, 'metadata');
      writeFileSync(target, JSON.stringify({ title: 'outside secret title' }));
      symlinkSync(target, join(testDir, 'linked.json'));

      const index = new SessionSearchIndex(testDir);
      await index.build();

      expect(index.size).toBe(0);
      expect(index.search('outside secret')).toEqual([]);
      index.close();
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });

  it('handles missing JSONL file', async () => {
    writeSessionMeta(testDir, 'session-nolog', { title: 'No log file' });
    // No .jsonl file created.

    const index = new SessionSearchIndex(testDir);
    await index.build();

    expect(index.size).toBe(1);
    const doc = index.getDocument('session-nolog');
    expect(doc!.promptCount).toBe(0);
    expect(doc!.entryCount).toBe(0);
    // The title is still searchable without a transcript.
    const hits = index.search('log file');
    expect(hits.map((h) => h.sessionId)).toContain('session-nolog');
  });

  it('does not index a KAS directory whose metadata names another session', async () => {
    const cliDir = join(testDir, 'cli');
    mkdirSync(cliDir);
    for (const [dirName, metadataId, prompt] of [
      ['sess_a', 'sess_b', 'spoofed alpha content'],
      ['sess_b', 'sess_b', 'legitimate bravo content'],
    ] as const) {
      const dir = join(testDir, 'hash1', dirName);
      mkdirSync(dir, { recursive: true });
      writeFileSync(
        join(dir, 'session.json'),
        JSON.stringify({ id: metadataId, title: prompt })
      );
      writeFileSync(
        join(dir, 'messages.jsonl'),
        JSON.stringify({ payload: { type: 'user', content: prompt } }) + '\n'
      );
    }

    const index = new SessionSearchIndex(cliDir);
    await index.build();

    expect(index.search('spoofed alpha')).toEqual([]);
    expect(
      index.search('legitimate bravo').map((hit) => hit.sessionId)
    ).toEqual(['b']);
    index.close();
  });

  it('does not traverse a symlinked KAS session directory', async () => {
    const cliDir = join(testDir, 'cli');
    const hashDir = join(testDir, 'hash1');
    const outside = mkdtempSync(join(tmpdir(), 'session-search-kas-outside-'));
    mkdirSync(cliDir);
    mkdirSync(hashDir);
    try {
      writeFileSync(
        join(outside, 'session.json'),
        JSON.stringify({ id: 'sess_linked', title: 'outside KAS title' })
      );
      writeFileSync(
        join(outside, 'messages.jsonl'),
        JSON.stringify({
          payload: { type: 'user', content: 'outside KAS prompt' },
        }) + '\n'
      );
      symlinkSync(outside, join(hashDir, 'sess_linked'), 'dir');

      const index = new SessionSearchIndex(cliDir);
      await index.build();

      expect(index.search('outside KAS')).toEqual([]);
      expect(index.getDocument('linked')).toBeUndefined();
      index.close();
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });

  it('handles empty sessions directory', async () => {
    const emptyDir = mkdtempSync(join(tmpdir(), 'session-search-empty-'));

    const index = new SessionSearchIndex(emptyDir);
    await index.build();

    expect(index.size).toBe(0);
    expect(index.getStatus().state).toBe('ready');

    rmSync(emptyDir, { recursive: true, force: true });
  });

  it('handles non-existent sessions directory', async () => {
    const index = new SessionSearchIndex('/nonexistent/path');
    await index.build();

    expect(index.size).toBe(0);
    expect(index.getStatus().state).toBe('ready');
  });

  it('retries a transiently unavailable SQLite index on build', async () => {
    writeSessionMeta(testDir, 'retry', { title: 'Recovered search index' });
    writeSessionLog(testDir, 'retry', [
      {
        kind: 'Prompt',
        data: {
          content: [{ kind: 'text', data: 'contention cleared prompt' }],
        },
      },
    ]);
    const dbPath = join(testDir, 'dashboard-search.db');
    const lock = new Database(dbPath, { create: true });
    const index = new SessionSearchIndex(testDir);
    try {
      lock.run('BEGIN EXCLUSIVE');
      expect(index.getCoverage()).toBe('titles-only');
      lock.run('ROLLBACK');
      lock.close();

      await index.build();

      expect(index.getCoverage()).toBe('titles-and-prompts');
      expect(index.search('contention cleared')).toMatchObject([
        { sessionId: 'retry', engine: 'v2' },
      ]);
    } finally {
      try {
        lock.run('ROLLBACK');
      } catch {
        // Cleanup also runs when an assertion exits before the primary rollback.
      }
      try {
        lock.close();
      } catch {
        // The contention connection may already be closed after releasing its lock.
      }
      index.close();
    }
  });

  describe('search', () => {
    beforeEach(async () => {
      writeSessionMeta(testDir, 's-auth', { title: 'Fix authentication bug' });
      writeSessionLog(testDir, 's-auth', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [
              {
                kind: 'text',
                data: 'The login page throws a 401 error when using OAuth tokens',
              },
            ],
          },
        },
        {
          kind: 'AssistantMessage',
          data: {
            message_id: 'm2',
            content: [
              {
                kind: 'text',
                data: 'The token refresh logic has a race condition.',
              },
            ],
          },
        },
      ]);

      writeSessionMeta(testDir, 's-deploy', { title: 'Deploy to production' });
      writeSessionLog(testDir, 's-deploy', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [
              {
                kind: 'text',
                data: 'Help me deploy the service to the production cluster',
              },
            ],
          },
        },
      ]);

      writeSessionMeta(testDir, 's-tests', {
        title: 'Add unit tests for utils',
      });
      writeSessionLog(testDir, 's-tests', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [
              {
                kind: 'text',
                data: 'Write tests for the session-dashboard utility functions',
              },
            ],
          },
        },
      ]);
    });

    it('finds sessions by title', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      const results = index.search('authentication');
      expect(results.length).toBeGreaterThan(0);
      expect(results[0]!.sessionId).toBe('s-auth');
      expect(results[0]!.snippet.length).toBeGreaterThan(0);
    });

    it('finds sessions by prompt content', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      const results = index.search('OAuth tokens');
      expect(results.length).toBeGreaterThan(0);
      expect(results[0]!.sessionId).toBe('s-auth');
      expect(results[0]!.matchField).toBe('prompt');
    });

    it('does not search assistant responses', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      // "race condition" appears only in an assistant response, which the
      // content index deliberately does not cover.
      const results = index.search('race condition');
      expect(results).toHaveLength(0);
    });

    it('finds sessions by keyword across multiple terms', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      const results = index.search('deploy production');
      expect(results.length).toBeGreaterThan(0);
      expect(results[0]!.sessionId).toBe('s-deploy');
    });

    it('returns empty array for no matches', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      const results = index.search('nonexistent gibberish xyz123');
      expect(results).toHaveLength(0);
    });

    it('returns empty array for empty query', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      expect(index.search('')).toHaveLength(0);
      expect(index.search('   ')).toHaveLength(0);
    });

    it('respects limit parameter', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      const results = index.search('the', 1);
      expect(results).toHaveLength(1);
    });

    it('ranks title matches higher than prompt matches', async () => {
      const index = new SessionSearchIndex(testDir);
      await index.build();

      // "unit tests" appears in both a title and a prompt.
      const results = index.search('unit tests');
      expect(results.length).toBeGreaterThan(0);
      expect(results[0]!.sessionId).toBe('s-tests');
    });
  });

  describe('incremental update', () => {
    it('adds new session to existing index', async () => {
      writeSessionMeta(testDir, 's-1', { title: 'First session' });
      writeSessionLog(testDir, 's-1', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [{ kind: 'text', data: 'hello' }],
          },
        },
      ]);

      const index = new SessionSearchIndex(testDir);
      await index.build();
      expect(index.size).toBe(1);

      // Create a new session file.
      writeSessionMeta(testDir, 's-2', { title: 'Second session' });
      writeSessionLog(testDir, 's-2', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [{ kind: 'text', data: 'world' }],
          },
        },
      ]);

      // Incrementally update.
      index.update('s-2');
      expect(index.size).toBe(2);
      expect(index.getDocument('s-2')).toBeDefined();
      expect(index.search('First session').map((hit) => hit.sessionId)).toEqual(
        ['s-1']
      );
    });

    it('removes deleted sessions on update', async () => {
      writeSessionMeta(testDir, 's-1', { title: 'Will be removed' });
      writeSessionLog(testDir, 's-1', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [{ kind: 'text', data: 'hello' }],
          },
        },
      ]);

      const index = new SessionSearchIndex(testDir);
      await index.build();
      expect(index.size).toBe(1);

      // Delete the session metadata file.
      rmSync(join(testDir, 's-1.json'));

      // Update — should detect deletion.
      index.update('s-1');
      expect(index.size).toBe(0);
    });
  });

  describe('status reporting', () => {
    it('reports indexing progress', async () => {
      writeSessionMeta(testDir, 's-1', { title: 'Session 1' });
      writeSessionLog(testDir, 's-1', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [{ kind: 'text', data: 'hello' }],
          },
        },
      ]);

      const index = new SessionSearchIndex(testDir);
      const statuses: string[] = [];
      index.onStatusChange((s) => statuses.push(s.state));

      await index.build();

      expect(statuses).toContain('indexing');
      expect(statuses[statuses.length - 1]).toBe('ready');
    });

    it('supports unsubscribe', async () => {
      const index = new SessionSearchIndex(testDir);
      const statuses: string[] = [];
      const unsub = index.onStatusChange((s) => statuses.push(s.state));
      unsub();

      await index.build();
      expect(statuses).toHaveLength(0);
    });
  });

  it('a refresh picks up sessions created after the first build', async () => {
    writeSessionMeta(testDir, 's-1', { title: 'First session' });
    writeSessionLog(testDir, 's-1', [
      {
        kind: 'Prompt',
        data: { message_id: 'm1', content: [{ kind: 'text', data: 'hello' }] },
      },
    ]);

    const index = new SessionSearchIndex(testDir);
    await index.build();
    expect(index.size).toBe(1);

    // Another window writes a session; build() alone would never see it.
    writeSessionMeta(testDir, 's-2', { title: 'Second session' });
    writeSessionLog(testDir, 's-2', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [{ kind: 'text', data: 'about warp drives' }],
        },
      },
    ]);
    await index.build(); // no-op: already ready
    expect(index.size).toBe(1);

    await index.refresh();
    expect(index.size).toBe(2);
    expect(index.search('warp drives').map((h) => h.sessionId)).toContain(
      's-2'
    );

    // And a deletion drops out on the next refresh.
    rmSync(join(testDir, 's-2.json'));
    rmSync(join(testDir, 's-2.jsonl'));
    await index.refresh();
    expect(index.size).toBe(1);
    expect(index.search('warp drives')).toHaveLength(0);
    index.close();
  });

  it('runs a trailing pass when refresh arrives during a build', async () => {
    writeSessionMeta(testDir, 's-1', { title: 'First session' });
    writeSessionLog(testDir, 's-1', [
      { kind: 'Prompt', data: { content: [{ kind: 'text', data: 'first' }] } },
    ]);
    const index = new SessionSearchIndex(testDir);

    const building = index.build();
    writeSessionMeta(testDir, 's-2', { title: 'Second session' });
    writeSessionLog(testDir, 's-2', [
      {
        kind: 'Prompt',
        data: { content: [{ kind: 'text', data: 'trailing refresh needle' }] },
      },
    ]);
    const refreshing = index.refresh();

    await Promise.all([building, refreshing]);
    expect(
      index.search('trailing refresh needle').map((hit) => hit.sessionId)
    ).toContain('s-2');
    index.close();
  });

  describe('KAS store discovery', () => {
    it('indexes prompts from the KAS store, whatever the directory name shape', async () => {
      // A realistic store root: cli/ (V2) beside workspace-hash dirs (KAS).
      const root = mkdtempSync(join(tmpdir(), 'session-search-root-'));
      const cliDir = join(root, 'cli');
      mkdirSync(cliDir, { recursive: true });

      // KAS sessions in one workspace hash — bare-UUID and converted (cli_*)
      // directory names, neither carrying the sess_ prefix.
      for (const dirName of [
        'sess_abc123',
        '0f9e8d7c-aaaa-bbbb-cccc-121314151617',
        'cli_deadbeef_conv',
      ]) {
        const d = join(root, 'hash-1', dirName);
        mkdirSync(d, { recursive: true });
        writeFileSync(
          join(d, 'session.json'),
          JSON.stringify({ id: dirName, title: `Session ${dirName}` })
        );
        writeFileSync(
          join(d, 'messages.jsonl'),
          JSON.stringify({
            payload: {
              type: 'user',
              content: `unique marker ${dirName.replace(/[^a-z0-9]/gi, '')}`,
            },
          }) + '\n'
        );
      }

      const index = new SessionSearchIndex(cliDir);
      await index.build();

      for (const dirName of [
        'sess_abc123',
        '0f9e8d7c-aaaa-bbbb-cccc-121314151617',
        'cli_deadbeef_conv',
      ]) {
        const marker = `marker ${dirName.replace(/[^a-z0-9]/gi, '')}`;
        const hits = index.search(marker);
        // Ids are normalized (sess_ stripped) at indexing time.
        expect(hits.map((h) => h.sessionId)).toContain(
          dirName.replace(/^sess_/, '')
        );
      }

      index.close();
      rmSync(root, { recursive: true, force: true });
    });

    it('keeps unrelated exact-id V2 and KAS content independently searchable', async () => {
      const root = mkdtempSync(join(tmpdir(), 'session-search-root-'));
      const cliDir = join(root, 'cli');
      const kasDir = join(root, 'hash-1', 'sess_shared');
      mkdirSync(cliDir, { recursive: true });
      mkdirSync(kasDir, { recursive: true });
      writeSessionMeta(cliDir, 'shared', { title: 'V2 shared' });
      writeSessionLog(cliDir, 'shared', [
        {
          kind: 'Prompt',
          data: { content: [{ kind: 'text', data: 'v2 needle' }] },
        },
      ]);
      writeFileSync(
        join(kasDir, 'session.json'),
        JSON.stringify({ id: 'sess_shared', title: 'KAS shared' })
      );
      writeFileSync(
        join(kasDir, 'messages.jsonl'),
        JSON.stringify({ payload: { type: 'user', content: 'kas needle' } }) +
          '\n'
      );

      const index = new SessionSearchIndex(cliDir);
      await index.build();

      expect(index.search('v2 needle')).toMatchObject([
        { sessionId: 'shared', engine: 'v2' },
      ]);
      expect(index.search('kas needle')).toMatchObject([
        { sessionId: 'shared', engine: 'v3' },
      ]);
      expect(index.getPromptTitle('shared', 'v2')).toBe('v2 needle');
      expect(index.getPromptTitle('shared', 'v3')).toBe('kas needle');
      expect(index.getPromptCount('shared', 'v2')).toBe(1);
      expect(index.getPromptCount('shared', 'v3')).toBe(1);
      expect(index.getDocument('shared', 'v2')?.title).toBe('V2 shared');
      expect(index.getDocument('shared', 'v3')).toBeUndefined();
      index.close();
      rmSync(root, { recursive: true, force: true });
    });

    it('preserves indexed KAS rows when metadata discovery is incomplete', async () => {
      const root = mkdtempSync(join(tmpdir(), 'session-search-root-'));
      const cliDir = join(root, 'cli');
      const sessionDir = join(root, 'hash-1', 'sess_preserved');
      mkdirSync(cliDir, { recursive: true });
      mkdirSync(sessionDir, { recursive: true });
      writeFileSync(
        join(sessionDir, 'session.json'),
        JSON.stringify({ id: 'sess_preserved', title: 'preserved metadata' })
      );
      writeFileSync(
        join(sessionDir, 'messages.jsonl'),
        JSON.stringify({
          payload: { type: 'user', content: 'preserved discovery prompt' },
        }) + '\n'
      );

      const index = new SessionSearchIndex(cliDir);
      await index.build();
      expect(index.search('discovery prompt')).toHaveLength(1);

      writeFileSync(join(sessionDir, 'session.json'), '{"id":');
      await index.refresh();

      expect(index.search('discovery prompt')).toMatchObject([
        { sessionId: 'preserved', engine: 'v3' },
      ]);
      index.close();
      rmSync(root, { recursive: true, force: true });
    });

    it('indexes transcript-less sessions as prompt-less; skips cloud-placed ones', async () => {
      const root = mkdtempSync(join(tmpdir(), 'session-search-root-'));
      const cliDir = join(root, 'cli');
      mkdirSync(cliDir, { recursive: true });

      // Local session that was never prompted: session.json, no messages.jsonl.
      const bare = join(root, 'hash-1', 'sess_neverprompted');
      mkdirSync(bare, { recursive: true });
      writeFileSync(
        join(bare, 'session.json'),
        JSON.stringify({ id: 'sess_neverprompted', title: 'New Session' })
      );

      // Cloud-placed record: also no local transcript, but the conversation
      // lives in the backend — must NOT be classified prompt-less.
      const cloud = join(root, 'hash-1', 'sess_cloudy');
      mkdirSync(cloud, { recursive: true });
      writeFileSync(
        join(cloud, 'session.json'),
        JSON.stringify({
          id: 'sess_cloudy',
          title: 'cloud work',
          executionTarget: { kind: 'cloud-sandbox' },
        })
      );

      const index = new SessionSearchIndex(cliDir);
      await index.build();

      expect(index.isPromptless('neverprompted', 'v3')).toBe(true);
      expect(index.isPromptless('cloudy', 'v3')).toBe(false);

      index.close();
      rmSync(root, { recursive: true, force: true });
    });
  });

  describe('singleton', () => {
    it('getSessionSearchIndex returns same instance', () => {
      // Override sessions dir via env to point at test dir.
      process.env.KIRO_TEST_SESSIONS_DIR = testDir;
      resetSessionSearchIndex();

      const a = getSessionSearchIndex();
      const b = getSessionSearchIndex();
      expect(a).toBe(b);

      delete process.env.KIRO_TEST_SESSIONS_DIR;
    });
  });
});
