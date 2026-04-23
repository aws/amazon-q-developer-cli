import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  listSessionsForCwd,
  getMostRecentSessionId,
  formatRelativeTime,
  formatSessionEntry,
} from '../sessions.js';
import type { SessionEntry } from '../sessions.js';

let testDir: string;
let savedEnv: NodeJS.ProcessEnv;

function writeSessionFile(
  sessionId: string,
  data: Record<string, unknown>
): void {
  writeFileSync(join(testDir, `${sessionId}.json`), JSON.stringify(data));
}

function writeSessionLog(sessionId: string, lines: object[]): void {
  writeFileSync(
    join(testDir, `${sessionId}.jsonl`),
    lines.map((l) => JSON.stringify(l)).join('\n')
  );
}

beforeEach(() => {
  testDir = join(
    tmpdir(),
    `sessions-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(testDir, { recursive: true });
  savedEnv = { ...process.env };
  process.env.KIRO_TEST_SESSIONS_DIR = testDir;
});

afterEach(() => {
  process.env = savedEnv;
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
});

describe('sessions', () => {
  describe('formatRelativeTime', () => {
    it('returns seconds ago for date within last minute', () => {
      const date = new Date(Date.now() - 30 * 1000).toISOString();
      const result = formatRelativeTime(date);
      expect(result).toMatch(/^\d+ seconds ago$/);
    });

    it('returns minutes ago for date a few minutes ago', () => {
      const date = new Date(Date.now() - 5 * 60 * 1000).toISOString();
      const result = formatRelativeTime(date);
      expect(result).toBe('5 minutes ago');
    });

    it('returns hours ago for date a few hours ago', () => {
      const date = new Date(Date.now() - 3 * 3600 * 1000).toISOString();
      const result = formatRelativeTime(date);
      expect(result).toBe('3 hours ago');
    });

    it('returns days ago for date a few days ago', () => {
      const date = new Date(Date.now() - 2 * 86400 * 1000).toISOString();
      const result = formatRelativeTime(date);
      expect(result).toBe('2 days ago');
    });

    it('returns unknown for invalid date string', () => {
      expect(formatRelativeTime('not-a-date')).toBe('unknown');
    });

    it('returns unknown for empty string', () => {
      expect(formatRelativeTime('')).toBe('unknown');
    });
  });

  describe('formatSessionEntry', () => {
    it('formats entry with messages', () => {
      const entry: SessionEntry = {
        sessionId: 'abc',
        cwd: '/tmp',
        createdAt: new Date().toISOString(),
        updatedAt: new Date(Date.now() - 30 * 1000).toISOString(),
        msgCount: 5,
        summary: 'fix a bug',
      };
      const result = formatSessionEntry(entry);
      expect(result).toMatch(/seconds ago \| fix a bug \| 5 msgs$/);
    });

    it('omits msgs suffix when msgCount is 0', () => {
      const entry: SessionEntry = {
        sessionId: 'abc',
        cwd: '/tmp',
        createdAt: new Date().toISOString(),
        updatedAt: new Date(Date.now() - 60 * 1000).toISOString(),
        msgCount: 0,
        summary: 'empty session',
      };
      const result = formatSessionEntry(entry);
      expect(result).not.toContain('msgs');
      expect(result).toContain('empty session');
    });

    it('truncates long summary lines to fit terminal width', () => {
      const oldColumns = process.stderr.columns;
      try {
        Object.defineProperty(process.stderr, 'columns', {
          value: 40,
          writable: true,
          configurable: true,
        });
        const entry: SessionEntry = {
          sessionId: 'abc',
          cwd: '/tmp',
          createdAt: new Date().toISOString(),
          updatedAt: new Date(Date.now() - 30 * 1000).toISOString(),
          msgCount: 10,
          summary: 'a'.repeat(200),
        };
        const result = formatSessionEntry(entry);
        // maxLen = 40 - 4 = 36, truncated to 33 + "..."
        expect(result.length).toBeLessThanOrEqual(36);
        expect(result).toEndWith('...');
      } finally {
        if (oldColumns !== undefined) {
          Object.defineProperty(process.stderr, 'columns', {
            value: oldColumns,
            writable: true,
            configurable: true,
          });
        }
      }
    });

    it('shows unknown when updatedAt is empty', () => {
      const entry: SessionEntry = {
        sessionId: 'abc',
        cwd: '/tmp',
        createdAt: '',
        updatedAt: '',
        msgCount: 1,
        summary: 'test',
      };
      const result = formatSessionEntry(entry);
      expect(result).toStartWith('unknown |');
    });
  });

  describe('listSessionsForCwd', () => {
    it('returns empty array when sessions dir is empty', () => {
      const result = listSessionsForCwd('/some/cwd');
      expect(result).toEqual([]);
    });

    it('returns empty array when sessions dir does not exist', () => {
      process.env.KIRO_TEST_SESSIONS_DIR = join(testDir, 'nonexistent');
      const result = listSessionsForCwd('/some/cwd');
      expect(result).toEqual([]);
    });

    it('returns sessions matching cwd', () => {
      const cwd = join(testDir, 'project');
      mkdirSync(cwd, { recursive: true });

      writeSessionFile('sess1', {
        session_id: 'sess1',
        cwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
      });

      const result = listSessionsForCwd(cwd);
      expect(result).toHaveLength(1);
      expect(result[0]!.sessionId).toBe('sess1');
    });

    it('filters out sessions with different cwd', () => {
      const cwd = join(testDir, 'project');
      const otherCwd = join(testDir, 'other');
      mkdirSync(cwd, { recursive: true });
      mkdirSync(otherCwd, { recursive: true });

      writeSessionFile('sess1', {
        session_id: 'sess1',
        cwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
      });
      writeSessionFile('sess2', {
        session_id: 'sess2',
        cwd: otherCwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-03T00:00:00Z',
      });

      const result = listSessionsForCwd(cwd);
      expect(result).toHaveLength(1);
      expect(result[0]!.sessionId).toBe('sess1');
    });

    it('sorts by updatedAt descending (most recent first)', () => {
      const cwd = join(testDir, 'project');
      mkdirSync(cwd, { recursive: true });

      writeSessionFile('old', {
        session_id: 'old',
        cwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-01T00:00:00Z',
      });
      writeSessionFile('new', {
        session_id: 'new',
        cwd,
        created_at: '2024-01-02T00:00:00Z',
        updated_at: '2024-01-03T00:00:00Z',
      });
      writeSessionFile('mid', {
        session_id: 'mid',
        cwd,
        created_at: '2024-01-01T12:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
      });

      const result = listSessionsForCwd(cwd);
      expect(result).toHaveLength(3);
      expect(result[0]!.sessionId).toBe('new');
      expect(result[1]!.sessionId).toBe('mid');
      expect(result[2]!.sessionId).toBe('old');
    });

    it('reads .jsonl for message count', () => {
      const cwd = join(testDir, 'project');
      mkdirSync(cwd, { recursive: true });

      writeSessionFile('sess1', {
        session_id: 'sess1',
        cwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
      });

      writeSessionLog('sess1', [
        { kind: 'Prompt', data: { content: [{ kind: 'text', data: 'hi' }] } },
        {
          kind: 'AssistantMessage',
          data: { content: [{ kind: 'text', data: 'hello' }] },
        },
        {
          kind: 'Prompt',
          data: { content: [{ kind: 'text', data: 'thanks' }] },
        },
      ]);

      const result = listSessionsForCwd(cwd);
      expect(result).toHaveLength(1);
      expect(result[0]!.msgCount).toBe(3);
    });

    it('reads .jsonl for last user prompt as summary', () => {
      const cwd = join(testDir, 'project');
      mkdirSync(cwd, { recursive: true });

      writeSessionFile('sess1', {
        session_id: 'sess1',
        cwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
      });

      writeSessionLog('sess1', [
        {
          kind: 'Prompt',
          data: { content: [{ kind: 'text', data: 'first message' }] },
        },
        {
          kind: 'AssistantMessage',
          data: { content: [{ kind: 'text', data: 'response' }] },
        },
        {
          kind: 'Prompt',
          data: {
            content: [{ kind: 'text', data: 'last user prompt here' }],
          },
        },
      ]);

      const result = listSessionsForCwd(cwd);
      expect(result).toHaveLength(1);
      expect(result[0]!.summary).toBe('last user prompt here');
    });

    it('skips malformed JSON files', () => {
      const cwd = join(testDir, 'project');
      mkdirSync(cwd, { recursive: true });

      writeFileSync(join(testDir, 'bad.json'), 'not valid json{{{');
      writeSessionFile('good', {
        session_id: 'good',
        cwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
      });

      const result = listSessionsForCwd(cwd);
      expect(result).toHaveLength(1);
      expect(result[0]!.sessionId).toBe('good');
    });
  });

  describe('getMostRecentSessionId', () => {
    it('returns undefined when no sessions exist', () => {
      expect(getMostRecentSessionId('/nonexistent/path')).toBeUndefined();
    });

    it('returns the most recent session ID', () => {
      const cwd = join(testDir, 'project');
      mkdirSync(cwd, { recursive: true });

      writeSessionFile('old', {
        session_id: 'old',
        cwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-01T00:00:00Z',
      });
      writeSessionFile('new', {
        session_id: 'new',
        cwd,
        created_at: '2024-01-02T00:00:00Z',
        updated_at: '2024-01-03T00:00:00Z',
      });

      expect(getMostRecentSessionId(cwd)).toBe('new');
    });

    it('returns undefined when no sessions match cwd', () => {
      const cwd = join(testDir, 'project');
      const otherCwd = join(testDir, 'other');
      mkdirSync(cwd, { recursive: true });
      mkdirSync(otherCwd, { recursive: true });

      writeSessionFile('sess1', {
        session_id: 'sess1',
        cwd: otherCwd,
        created_at: '2024-01-01T00:00:00Z',
        updated_at: '2024-01-02T00:00:00Z',
      });

      expect(getMostRecentSessionId(cwd)).toBeUndefined();
    });
  });
});
