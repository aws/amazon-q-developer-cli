/**
 * Unit tests for `listAllSessions`. The helper shells out to
 * `kiro-cli chat --list-sessions --format json`; tests pin the
 * stdout-parse contract and the failure modes (missing binary,
 * non-zero exit, malformed JSON, missing required fields).
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { type AsyncSpawner, listAllSessions } from '../list-all-sessions-cli';

const FAKE_BIN = '/fake/chat_cli';
let originalBin: string | undefined;

beforeEach(() => {
  originalBin = process.env.KIRO_CHAT_CLI_BIN;
  process.env.KIRO_CHAT_CLI_BIN = FAKE_BIN;
});

afterEach(() => {
  if (originalBin === undefined) delete process.env.KIRO_CHAT_CLI_BIN;
  else process.env.KIRO_CHAT_CLI_BIN = originalBin;
});

describe('listAllSessions', () => {
  it('parses the [{cwd, sessions}] envelope into a flat SessionEntry list', async () => {
    const calls: Array<{ cmd: string; args: string[] }> = [];
    const spawner: AsyncSpawner = async (cmd, args) => {
      calls.push({ cmd, args });
      return {
        exitCode: 0,
        stdout: JSON.stringify([
          {
            cwd: '/some/cwd',
            sessions: [
              {
                sessionId: 'v2-1',
                source: 'v2',
                title: 'Fix auth bug',
                updatedAt: '2026-05-31T18:00:00.000Z',
                messageCount: 7,
              },
              {
                sessionId: 'kas-1',
                source: 'v3',
                title: 'Stale KAS',
                updatedAt: '2026-05-30T18:00:00.000Z',
              },
            ],
          },
        ]),
        stderr: '',
      };
    };
    const result = await listAllSessions(spawner);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(calls).toEqual([
      { cmd: FAKE_BIN, args: ['chat', '--list-sessions', '--format', 'json'] },
    ]);
    expect(result.cwd).toBe('/some/cwd');
    expect(result.sessions).toHaveLength(2);
    expect(result.sessions[0]).toEqual({
      sessionId: 'v2-1',
      source: 'v2',
      title: 'Fix auth bug',
      updatedAt: '2026-05-31T18:00:00.000Z',
      messageCount: 7,
    });
    // KAS entry: messageCount omitted entirely.
    expect(result.sessions[1]).toEqual({
      sessionId: 'kas-1',
      source: 'v3',
      title: 'Stale KAS',
      updatedAt: '2026-05-30T18:00:00.000Z',
    });
    expect(result.sessions[1]).not.toHaveProperty('messageCount');
  });

  it('returns ok with an empty sessions array on an empty envelope', async () => {
    const spawner: AsyncSpawner = async () => ({
      exitCode: 0,
      stdout: '[{"cwd":"/x","sessions":[]}]',
      stderr: '',
    });
    const result = await listAllSessions(spawner);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.cwd).toBe('/x');
      expect(result.sessions).toEqual([]);
    }
  });

  it('errors when KIRO_CHAT_CLI_BIN is unset', async () => {
    delete process.env.KIRO_CHAT_CLI_BIN;
    const result = await listAllSessions(async () => ({
      exitCode: 0,
      stdout: '[]',
      stderr: '',
    }));
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toBe('Failed to find the kiro-cli binary');
    }
  });

  it('errors with a structured message on non-zero exit', async () => {
    const spawner: AsyncSpawner = async () => ({
      exitCode: 1,
      stdout: '',
      stderr: 'something broke',
    });
    const result = await listAllSessions(spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toContain('exited 1');
      expect(result.error).toContain('something broke');
    }
  });

  it('errors when stdout is not JSON', async () => {
    const spawner: AsyncSpawner = async () => ({
      exitCode: 0,
      stdout: 'not json at all',
      stderr: '',
    });
    const result = await listAllSessions(spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toContain('Unexpected output');
    }
  });

  it('errors when an entry is missing a required field', async () => {
    const spawner: AsyncSpawner = async () => ({
      exitCode: 0,
      stdout: JSON.stringify([
        {
          cwd: '/x',
          sessions: [{ sessionId: 'broken', source: 'v2' }], // no title/updatedAt
        },
      ]),
      stderr: '',
    });
    const result = await listAllSessions(spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toContain('missing required field');
    }
  });

  it('parses a stdout with prepended log lines (last JSON line wins)', async () => {
    const spawner: AsyncSpawner = async () => ({
      exitCode: 0,
      stdout:
        'INFO some log line\n' +
        'INFO another\n' +
        '[{"cwd":"/x","sessions":[]}]\n',
      stderr: '',
    });
    const result = await listAllSessions(spawner);
    expect(result.ok).toBe(true);
  });
});
