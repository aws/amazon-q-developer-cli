/**
 * Unit tests for the `session-archive-cli` helper.
 *
 * These cover the spawn-and-parse contract in isolation - argv assembly,
 * JSON-on-stdout extraction, error path classification - using a fake
 * spawner injected at call time. The real binary is never invoked here;
 * the integration coverage of the binary is in
 * `e2e_tests/session-archive.test.ts`.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import {
  exportSession,
  importSession,
  type SyncSpawner,
} from '../session-archive-cli';

interface SpawnerHarness {
  spawner: SyncSpawner;
  calls: Array<{ cmd: string; args: string[] }>;
}

function makeSpawner(result: {
  status: number;
  stdout: string;
  stderr?: string;
  error?: Error;
}): SpawnerHarness {
  const calls: SpawnerHarness['calls'] = [];
  const spawner: SyncSpawner = (cmd, args) => {
    calls.push({ cmd, args });
    return {
      status: result.status,
      stdout: result.stdout,
      stderr: result.stderr ?? '',
      ...(result.error ? { error: result.error } : {}),
    };
  };
  return { spawner, calls };
}

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

describe('exportSession', () => {
  it('invokes the binary with the canonical argv', () => {
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"path":"/out.zip"}',
    });
    const result = exportSession(
      { sessionId: 'sess1', cwd: '/work', out: '/out.zip', force: false },
      spawner
    );

    expect(result).toEqual({ ok: true, path: '/out.zip' });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.cmd).toBe(FAKE_BIN);
    expect(calls[0]!.args).toEqual([
      'chat',
      '_',
      'export-session',
      '--id',
      'sess1',
      '--cwd',
      '/work',
      '--out',
      '/out.zip',
    ]);
  });

  it('appends --force only when the flag is set', () => {
    const { spawner: s1, calls: c1 } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"path":"/o.zip"}',
    });
    exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip', force: true },
      s1
    );
    expect(c1[0]!.args).toContain('--force');

    const { spawner: s2, calls: c2 } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"path":"/o.zip"}',
    });
    exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip', force: false },
      s2
    );
    expect(c2[0]!.args).not.toContain('--force');
  });

  it('appends --base-path when provided', () => {
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"path":"/o.zip"}',
    });
    exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip', basePath: '/sessions' },
      spawner
    );
    const idx = calls[0]!.args.indexOf('--base-path');
    expect(idx).toBeGreaterThanOrEqual(0);
    expect(calls[0]!.args[idx + 1]).toBe('/sessions');
  });

  it('returns the parsed error when the binary exits non-zero', () => {
    const { spawner } = makeSpawner({
      status: 1,
      stdout: '{"success":false,"error":"session not found"}',
    });
    const result = exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip' },
      spawner
    );
    expect(result).toEqual({ ok: false, error: 'session not found' });
  });

  it('returns the JSON success even with preceding stdout log lines', () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout:
        '[2025-05-21T17:00:00Z INFO  some::module] preflight ok\n' +
        '[2025-05-21T17:00:00Z DEBUG some::module] computed hash\n' +
        '{"success":true,"path":"/o.zip"}',
    });
    const result = exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip' },
      spawner
    );
    expect(result).toEqual({ ok: true, path: '/o.zip' });
  });

  it('returns a generic error when stdout has no parseable JSON', () => {
    const { spawner } = makeSpawner({
      status: 1,
      stdout: 'totally broken output with no json',
    });
    const result = exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip' },
      spawner
    );
    expect(result.ok).toBe(false);
  });

  it('returns an error when the spawn itself fails (binary missing)', () => {
    const { spawner } = makeSpawner({
      status: -1,
      stdout: '',
      error: Object.assign(new Error('spawn ENOENT'), { code: 'ENOENT' }),
    });
    const result = exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip' },
      spawner
    );
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain('ENOENT');
  });

  it('returns an error when KIRO_CHAT_CLI_BIN is not set', () => {
    delete process.env.KIRO_CHAT_CLI_BIN;
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"path":"/o.zip"}',
    });
    const result = exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip' },
      spawner
    );
    expect(result.ok).toBe(false);
    // The spawner should never be called when the binary path can't be resolved.
    expect(calls).toHaveLength(0);
  });

  it('accepts Buffer stdout (matches Bun.spawnSync return shape)', () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: '', // overridden below
    });
    // Overwrite: Buffer-typed stdout, exercising the Buffer branch.
    const bufferSpawner: SyncSpawner = (cmd, args) => {
      const r = spawner(cmd, args);
      return {
        ...r,
        stdout: Buffer.from('{"success":true,"path":"/o.zip"}'),
      };
    };
    const result = exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip' },
      bufferSpawner
    );
    expect(result).toEqual({ ok: true, path: '/o.zip' });
  });
});

describe('importSession', () => {
  it('invokes the binary with the canonical argv', () => {
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"path":"/sessions/h/sess_xxx"}',
    });
    const result = importSession(
      { archivePath: '/in.zip', cwd: '/work' },
      spawner
    );

    expect(result).toEqual({ ok: true, path: '/sessions/h/sess_xxx' });
    expect(calls[0]!.cmd).toBe(FAKE_BIN);
    expect(calls[0]!.args).toEqual([
      'chat',
      '_',
      'import-session',
      '--archive',
      '/in.zip',
      '--cwd',
      '/work',
    ]);
  });

  it('appends --base-path when provided', () => {
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"path":"/p"}',
    });
    importSession(
      { archivePath: '/in.zip', cwd: '/work', basePath: '/sessions' },
      spawner
    );
    const idx = calls[0]!.args.indexOf('--base-path');
    expect(idx).toBeGreaterThanOrEqual(0);
    expect(calls[0]!.args[idx + 1]).toBe('/sessions');
  });

  it('returns the parsed error on non-zero exit', () => {
    const { spawner } = makeSpawner({
      status: 1,
      stdout: '{"success":false,"error":"archive is not a zip"}',
    });
    const result = importSession(
      { archivePath: '/in.zip', cwd: '/work' },
      spawner
    );
    expect(result).toEqual({ ok: false, error: 'archive is not a zip' });
  });
});
