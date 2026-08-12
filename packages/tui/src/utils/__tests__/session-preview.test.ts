import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  appendFileSync,
  closeSync,
  mkdtempSync,
  mkdirSync,
  openSync,
  writeFileSync,
  rmSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { readFileRange } from '../bounded-json';
import {
  SessionPreviewProvider,
  resetSessionPreviewProvider,
} from '../session-preview';

function createTestDir(): string {
  return mkdtempSync(join(tmpdir(), 'session-preview-test-'));
}

function writeMeta(
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

function writeLog(
  dir: string,
  sessionId: string,
  entries: Array<{ kind: string; data: unknown }>
): void {
  const lines = entries.map((e) => JSON.stringify({ version: 'v1', ...e }));
  writeFileSync(join(dir, `${sessionId}.jsonl`), lines.join('\n') + '\n');
}

describe('bounded descriptor reads', () => {
  it('returns only bytes actually read and honors a fixed snapshot length', () => {
    const dir = createTestDir();
    const path = join(dir, 'range.txt');
    writeFileSync(path, 'abc');
    const fd = openSync(path, 'r');
    try {
      appendFileSync(path, 'def');
      expect(readFileRange(fd, 0, 3).toString('utf-8')).toBe('abc');
      expect(readFileRange(fd, 0, 64).toString('utf-8')).toBe('abcdef');
    } finally {
      closeSync(fd);
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe('SessionPreviewProvider', () => {
  let testDir: string;

  beforeEach(() => {
    testDir = createTestDir();
    resetSessionPreviewProvider();
  });

  afterEach(() => {
    rmSync(testDir, { recursive: true, force: true });
  });

  it('extracts summary from session metadata and log', () => {
    writeMeta(testDir, 's1', { title: 'Fix auth bug' });
    writeLog(testDir, 's1', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [
            { kind: 'text', data: 'Fix the login authentication issue' },
          ],
        },
      },
      {
        kind: 'AssistantMessage',
        data: {
          message_id: 'm2',
          content: [
            { kind: 'text', data: 'Found the bug in token validation.' },
            { kind: 'tool_use', data: { name: 'fs_write', input: {} } },
          ],
        },
      },
    ]);

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('s1');

    expect(preview).not.toBeNull();
    expect(preview!.summary.title).toBe('Fix auth bug');
    expect(preview!.summary.firstPrompt).toContain('login authentication');
    expect(preview!.summary.toolsSummary).toContain('fs_write');
    expect(preview!.summary.turnCount).toBe(1);
    expect(preview!.summary.createdAt).toBe('2026-07-20T10:00:00.000Z');
  });

  it('previews the transcript when V2 metadata is over the read limit', () => {
    // Large legacy V2 sessions carry multi-MiB metadata; the transcript is
    // still previewable and must not be blanked by an unreadable meta file.
    writeFileSync(
      join(testDir, 'big.json'),
      JSON.stringify({
        session_id: 'big',
        cwd: '/w',
        title: 'huge legacy session',
        pad: 'x'.repeat(2 * 1024 * 1024),
      })
    );
    writeLog(testDir, 'big', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [{ kind: 'text', data: 'recover this transcript' }],
        },
      },
    ]);

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('big', 'v2');

    expect(preview).not.toBeNull();
    expect(preview!.summary.firstPrompt).toContain('recover this transcript');
    expect(preview!.summary.turnCount).toBe(1);
  });

  it('extracts recent messages', () => {
    writeMeta(testDir, 's1', { title: 'Chat session' });
    writeLog(testDir, 's1', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [{ kind: 'text', data: 'First user message' }],
        },
      },
      {
        kind: 'AssistantMessage',
        data: {
          message_id: 'm2',
          content: [{ kind: 'text', data: 'First assistant response' }],
        },
      },
      {
        kind: 'Prompt',
        data: {
          message_id: 'm3',
          content: [{ kind: 'text', data: 'Second user message' }],
        },
      },
      {
        kind: 'AssistantMessage',
        data: {
          message_id: 'm4',
          content: [{ kind: 'text', data: 'Second assistant response' }],
        },
      },
    ]);

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('s1');

    expect(preview!.recentMessages).toHaveLength(4);
    expect(preview!.recentMessages[0]!.role).toBe('user');
    expect(preview!.recentMessages[0]!.content).toBe('First user message');
    expect(preview!.recentMessages[1]!.role).toBe('assistant');
    expect(preview!.recentMessages[3]!.content).toBe(
      'Second assistant response'
    );
  });

  it('limits recent messages to last 10', () => {
    writeMeta(testDir, 's1', { title: 'Long session' });
    const entries: Array<{ kind: string; data: unknown }> = [];
    for (let i = 0; i < 15; i++) {
      entries.push({
        kind: 'Prompt',
        data: {
          message_id: `p${i}`,
          content: [{ kind: 'text', data: `User message ${i}` }],
        },
      });
      entries.push({
        kind: 'AssistantMessage',
        data: {
          message_id: `a${i}`,
          content: [{ kind: 'text', data: `Assistant response ${i}` }],
        },
      });
    }
    writeLog(testDir, 's1', entries);

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('s1');

    expect(preview!.recentMessages).toHaveLength(10);
    // Should contain the last messages.
    expect(preview!.recentMessages[9]!.content).toBe('Assistant response 14');
  });

  it('labels tail-only KAS summaries as partial and omits a false first prompt', () => {
    const storeRoot = join(testDir, 'store');
    const cliDir = join(storeRoot, 'cli');
    const kasDir = join(storeRoot, 'hash', 'sess_kas-large');
    mkdirSync(cliDir, { recursive: true });
    mkdirSync(kasDir, { recursive: true });
    writeFileSync(
      join(kasDir, 'session.json'),
      JSON.stringify({
        id: 'sess_kas-large',
        title: 'Large KAS session',
        workspacePaths: ['/w'],
      })
    );
    const lines = [
      JSON.stringify({ payload: { type: 'user', content: 'original prompt' } }),
      ...Array.from({ length: 80 }, () =>
        JSON.stringify({
          payload: { type: 'assistant', content: 'x'.repeat(1000) },
        })
      ),
      JSON.stringify({ payload: { type: 'user', content: 'recent prompt' } }),
    ];
    writeFileSync(join(kasDir, 'messages.jsonl'), lines.join('\n') + '\n');

    const preview = new SessionPreviewProvider(cliDir).getPreview('kas-large');

    expect(preview).not.toBeNull();
    expect(preview!.summary.isComplete).toBe(false);
    expect(preview!.summary.firstPrompt).toBe('');
    expect(preview!.summary.turnCount).toBe(1);
    expect(preview!.recentMessages.at(-1)?.content).toBe('recent prompt');
  });

  it('selects exact-id V2 and KAS copies by engine and cache key', () => {
    const storeRoot = join(testDir, 'store');
    const cliDir = join(storeRoot, 'cli');
    const kasDir = join(storeRoot, 'hash', 'sess_shared');
    mkdirSync(cliDir, { recursive: true });
    mkdirSync(kasDir, { recursive: true });
    writeMeta(cliDir, 'shared', { title: 'V2 copy' });
    writeLog(cliDir, 'shared', [
      {
        kind: 'Prompt',
        data: { content: [{ kind: 'text', data: 'V2 prompt' }] },
      },
    ]);
    writeFileSync(
      join(kasDir, 'session.json'),
      JSON.stringify({ id: 'sess_shared', title: 'KAS copy' })
    );
    writeFileSync(
      join(kasDir, 'messages.jsonl'),
      JSON.stringify({ payload: { type: 'user', content: 'KAS prompt' } }) +
        '\n'
    );

    const provider = new SessionPreviewProvider(cliDir);
    const v2 = provider.getPreview('shared', 'v2');
    const kas = provider.getPreview('shared', 'v3');
    const remote = provider.getPreview('shared', 'v3', 'remote');

    expect(v2?.summary.title).toBe('V2 copy');
    expect(v2?.summary.firstPrompt).toBe('V2 prompt');
    expect(kas?.summary.title).toBe('KAS copy');
    expect(kas?.summary.firstPrompt).toBe('KAS prompt');
    expect(remote).toBeNull();
    expect(provider.cacheSize).toBe(2);
  });

  it('reads the same canonical KAS copy selected by the catalog', () => {
    const storeRoot = join(testDir, 'store');
    const cliDir = join(storeRoot, 'cli');
    const globalDir = join(storeRoot, '_global', 'sess_shared-copy');
    const workspaceDir = join(storeRoot, 'workspace', 'sess_shared-copy');
    mkdirSync(cliDir, { recursive: true });
    mkdirSync(globalDir, { recursive: true });
    mkdirSync(workspaceDir, { recursive: true });
    writeFileSync(
      join(globalDir, 'session.json'),
      JSON.stringify({
        id: 'sess_shared-copy',
        title: 'Global copy',
        workspacePaths: [],
        lastModifiedAt: '2026-08-01T00:00:00.000Z',
      })
    );
    writeFileSync(
      join(globalDir, 'messages.jsonl'),
      JSON.stringify({ payload: { type: 'user', content: 'global prompt' } }) +
        '\n'
    );
    writeFileSync(
      join(workspaceDir, 'session.json'),
      JSON.stringify({
        id: 'sess_shared-copy',
        title: 'Workspace copy',
        workspacePaths: ['/workspace'],
        lastModifiedAt: '2026-07-01T00:00:00.000Z',
      })
    );
    writeFileSync(
      join(workspaceDir, 'messages.jsonl'),
      JSON.stringify({
        payload: { type: 'user', content: 'workspace prompt' },
      }) + '\n'
    );

    const preview = new SessionPreviewProvider(cliDir).getPreview(
      'shared-copy',
      'v3'
    );

    expect(preview?.summary.title).toBe('Workspace copy');
    expect(preview?.summary.firstPrompt).toBe('workspace prompt');
  });

  it('returns null for non-existent session', () => {
    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('nonexistent');
    expect(preview).toBeNull();
  });

  it('handles session with no log file', () => {
    writeMeta(testDir, 's1', { title: 'Empty session' });

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('s1');

    expect(preview).not.toBeNull();
    expect(preview!.summary.title).toBe('Empty session');
    expect(preview!.summary.firstPrompt).toBe('');
    expect(preview!.summary.turnCount).toBe(0);
    expect(preview!.recentMessages).toHaveLength(0);
  });

  it('handles malformed log lines gracefully', () => {
    writeMeta(testDir, 's1', { title: 'Bad logs' });
    writeFileSync(
      join(testDir, 's1.jsonl'),
      '{"version":"v1","kind":"Prompt","data":{"message_id":"m1","content":[{"kind":"text","data":"valid prompt"}]}}\n' +
        'not json\n' +
        '{"broken": true}\n'
    );

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('s1');

    expect(preview).not.toBeNull();
    expect(preview!.summary.firstPrompt).toBe('');
    expect(preview!.summary.turnCount).toBe(1);
    expect(preview!.summary.isComplete).toBe(false);
    expect(preview!.recentMessages[0]?.content).toBe('valid prompt');
  });

  it('truncates long content', () => {
    writeMeta(testDir, 's1', { title: 'Long content' });
    const longText = 'x'.repeat(500);
    writeLog(testDir, 's1', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [{ kind: 'text', data: longText }],
        },
      },
    ]);

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('s1');

    expect(preview!.summary.firstPrompt.length).toBeLessThanOrEqual(300);
    expect(preview!.summary.firstPrompt).toContain('...');
  });

  it('uses title from metadata, falls back to first prompt', () => {
    writeMeta(testDir, 's-no-title', { title: '' });
    writeLog(testDir, 's-no-title', [
      {
        kind: 'Prompt',
        data: {
          message_id: 'm1',
          content: [{ kind: 'text', data: 'My first prompt as title' }],
        },
      },
    ]);

    const provider = new SessionPreviewProvider(testDir);
    const preview = provider.getPreview('s-no-title');

    expect(preview!.summary.title).toBe('My first prompt as title');
  });

  describe('LRU cache', () => {
    it('returns cached preview on second call', () => {
      writeMeta(testDir, 's1', { title: 'Cached' });
      writeLog(testDir, 's1', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [{ kind: 'text', data: 'hello' }],
          },
        },
      ]);

      const provider = new SessionPreviewProvider(testDir);
      const first = provider.getPreview('s1');
      const second = provider.getPreview('s1');

      expect(first).toEqual(second);
      expect(provider.cacheSize).toBe(1);
    });

    it('clearCache rebuilds a changed preview', () => {
      writeMeta(testDir, 's1', { title: 'Before refresh' });
      writeLog(testDir, 's1', [
        {
          kind: 'Prompt',
          data: { content: [{ kind: 'text', data: 'old prompt' }] },
        },
      ]);
      const provider = new SessionPreviewProvider(testDir);
      expect(provider.getPreview('s1')?.summary.title).toBe('Before refresh');

      writeMeta(testDir, 's1', { title: 'After refresh' });
      writeLog(testDir, 's1', [
        {
          kind: 'Prompt',
          data: { content: [{ kind: 'text', data: 'new prompt' }] },
        },
      ]);
      expect(provider.getPreview('s1')?.summary.title).toBe('Before refresh');

      provider.clearCache();
      const refreshed = provider.getPreview('s1');
      expect(refreshed?.summary.title).toBe('After refresh');
      expect(refreshed?.summary.firstPrompt).toBe('new prompt');
    });

    it('evicts oldest entry when cache is full', () => {
      // Create 21 sessions (cache max is 20).
      for (let i = 0; i < 21; i++) {
        writeMeta(testDir, `s-${i}`, { title: `Session ${i}` });
        writeLog(testDir, `s-${i}`, [
          {
            kind: 'Prompt',
            data: {
              message_id: 'm1',
              content: [{ kind: 'text', data: `prompt ${i}` }],
            },
          },
        ]);
      }

      const provider = new SessionPreviewProvider(testDir);
      for (let i = 0; i < 21; i++) {
        provider.getPreview(`s-${i}`);
      }

      // Cache should be capped at 20.
      expect(provider.cacheSize).toBe(20);
    });

    it('invalidate clears cache', () => {
      writeMeta(testDir, 's1', { title: 'Will be invalidated' });
      writeLog(testDir, 's1', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [{ kind: 'text', data: 'hello' }],
          },
        },
      ]);

      const provider = new SessionPreviewProvider(testDir);
      provider.getPreview('s1');
      expect(provider.cacheSize).toBe(1);

      provider.invalidate('s1');
      expect(provider.cacheSize).toBe(0);
    });
  });

  describe('does NOT acquire session lock', () => {
    it('preview does not create lock file', () => {
      writeMeta(testDir, 's1', { title: 'No lock' });
      writeLog(testDir, 's1', [
        {
          kind: 'Prompt',
          data: {
            message_id: 'm1',
            content: [{ kind: 'text', data: 'hello' }],
          },
        },
      ]);

      const provider = new SessionPreviewProvider(testDir);
      provider.getPreview('s1');

      const { existsSync } = require('node:fs');
      expect(existsSync(join(testDir, 's1.lock'))).toBe(false);
    });
  });
});
