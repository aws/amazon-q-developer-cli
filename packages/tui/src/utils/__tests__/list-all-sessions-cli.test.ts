/**
 * Unit tests for `listAllSessions`. The helper shells out to
 * `kiro-cli chat --list-sessions --format json`; tests pin the
 * stdout-parse contract and the failure modes (missing binary,
 * non-zero exit, malformed JSON, missing required fields).
 *
 * WORKAROUND: Other test files (dispatcher.test.ts, chat.test.ts)
 * globally mock `list-all-sessions-cli` via Bun's `mock.module()`.
 * These mocks are process-wide and leak across all files in the
 * same `bun test` run. Since we cannot un-mock a module that another
 * file mocked, this test re-implements the function logic locally.
 * The implementation is a direct copy of `../list-all-sessions-cli.ts`;
 * if that file changes, this test MUST be updated to match.
 *
 * TODO: Migrate to a separate bun test invocation (isolated process)
 * once the CI supports multiple unit test steps.
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
  AsyncSpawner,
  ListAllSessionsResult,
  SessionEntry,
  SessionSource,
} from '../list-all-sessions-cli';
import { resolveChatCliBinFromEnv } from '../chat-cli-bin';

const importUnmocked = (specifier: string) => import(specifier);

// --- Local copy of listAllSessions (immune to mock.module pollution) ---

async function listAllSessions(
  spawner: AsyncSpawner,
  timeoutMs: number = 10_000
): Promise<ListAllSessionsResult> {
  let bin: string;
  try {
    bin = resolveChatCliBinFromEnv();
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : String(e) };
  }
  const args = ['chat', '--list-sessions', '--format', 'json'];
  let result: {
    exitCode: number | null;
    stdout: string;
    stderr: string;
    error?: Error;
  };
  try {
    result = await Promise.race([
      spawner(bin, args),
      new Promise<never>((_, reject) =>
        setTimeout(
          () =>
            reject(
              new Error(
                `kiro-cli chat --list-sessions did not complete within ${timeoutMs}ms`
              )
            ),
          timeoutMs
        )
      ),
    ]);
  } catch (e) {
    return {
      ok: false,
      error: `Failed to spawn kiro-cli: ${e instanceof Error ? e.message : String(e)}`,
    };
  }
  if (result.error) {
    return {
      ok: false,
      error: `Failed to spawn kiro-cli: ${result.error.message}`,
    };
  }
  if (result.exitCode !== 0) {
    return {
      ok: false,
      error: `kiro-cli exited ${result.exitCode}: ${result.stderr.trim() || result.stdout.trim()}`,
    };
  }
  return parseListing(result.stdout);
}

function parseListing(stdout: string): ListAllSessionsResult {
  const lines = stdout.split('\n').filter((l) => l.length > 0);
  if (lines.length === 0)
    return { ok: false, error: 'No output from kiro-cli --list-sessions' };
  const last = lines[lines.length - 1]!;
  let parsed: unknown;
  try {
    parsed = JSON.parse(last);
  } catch {
    return {
      ok: false,
      error: `Unexpected output from kiro-cli --list-sessions: ${last}`,
    };
  }
  if (!Array.isArray(parsed) || parsed.length === 0)
    return {
      ok: false,
      error: `kiro-cli --list-sessions returned an unexpected shape: ${last}`,
    };
  const envelope = parsed[0] as Record<string, unknown>;
  const cwd = typeof envelope.cwd === 'string' ? envelope.cwd : null;
  const sessionsRaw = Array.isArray(envelope.sessions)
    ? envelope.sessions
    : null;
  if (cwd === null || sessionsRaw === null)
    return {
      ok: false,
      error: `kiro-cli --list-sessions envelope missing cwd or sessions: ${last}`,
    };
  const sessions: SessionEntry[] = [];
  for (const raw of sessionsRaw) {
    const entry = raw as Record<string, unknown>;
    if (
      typeof entry.sessionId !== 'string' ||
      typeof entry.source !== 'string' ||
      typeof entry.title !== 'string' ||
      typeof entry.updatedAt !== 'string'
    ) {
      return {
        ok: false,
        error: `kiro-cli --list-sessions entry missing required field: ${JSON.stringify(entry)}`,
      };
    }
    sessions.push({
      sessionId: entry.sessionId,
      source: entry.source as SessionSource,
      title: entry.title,
      updatedAt: entry.updatedAt,
      ...(typeof entry.messageCount === 'number'
        ? { messageCount: entry.messageCount }
        : {}),
    });
  }
  return { ok: true, cwd, sessions };
}

// --- Tests ----------------------------------------------------------------

// A real on-disk path: bin resolution probes for existence. Never executed - spawners are injected.
const FAKE_BIN = process.execPath;
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
  it.skipIf(process.platform === 'win32')(
    'terminates a child whose stdout exceeds the resource cap',
    async () => {
      const dir = mkdtempSync(join(tmpdir(), 'session-list-output-cap-'));
      const script = join(dir, 'oversized-output.sh');
      writeFileSync(
        script,
        "#!/bin/sh\nhead -c 4194305 /dev/zero | tr '\\0' x\n"
      );
      chmodSync(script, 0o755);
      process.env.KIRO_CHAT_CLI_BIN = script;
      try {
        const { listAllSessions: listWithRealSpawner } = await importUnmocked(
          '../list-all-sessions-cli?output-cap'
        );

        const result = await listWithRealSpawner(undefined, 10_000);

        expect(result.ok).toBe(false);
        if (!result.ok) expect(result.error).toContain('exceeded 4 MiB');
      } finally {
        rmSync(dir, { recursive: true, force: true });
      }
    }
  );
});

describe('deleteClassicSession', () => {
  it('aborts the destructive child when the deadline expires', async () => {
    const { deleteClassicSession } = await importUnmocked(
      '../list-all-sessions-cli?classic-delete-timeout'
    );
    let observedSignal: AbortSignal | undefined;
    const spawner: AsyncSpawner = (_bin, _args, signal) =>
      new Promise((resolve) => {
        observedSignal = signal;
        signal?.addEventListener(
          'abort',
          () =>
            resolve({
              exitCode: null,
              stdout: '',
              stderr: '',
              error: new Error('aborted'),
            }),
          { once: true }
        );
      });

    const result = await deleteClassicSession('classic-1', spawner, 5);

    expect(result.ok).toBe(false);
    expect(observedSignal?.aborted).toBe(true);
  });
});
