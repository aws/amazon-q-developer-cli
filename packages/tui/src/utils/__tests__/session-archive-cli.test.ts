/**
 * Unit tests for `session-archive-cli`. This wrapper contributes
 * argv assembly on top of `chat-internal-cli`; shared-layer
 * behavior (binary resolution, JSON parsing, error envelopes,
 * Buffer-stdout, spawn errors) is exercised in
 * `chat-internal-cli.test.ts`. Tests here assert argv shape
 * behaviorally via flag/value pairs. Real-binary integration
 * coverage lives in `e2e_tests/session-archive.test.ts`.
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

function makeSpawner(stdout: string, status = 0): SpawnerHarness {
  const calls: SpawnerHarness['calls'] = [];
  const spawner: SyncSpawner = (cmd, args) => {
    calls.push({ cmd, args });
    return { status, stdout, stderr: '' };
  };
  return { spawner, calls };
}

/// Find the value following `flag` in a positional argv. Returns
/// undefined when the flag is absent.
function flagValue(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  return idx >= 0 && idx + 1 < args.length ? args[idx + 1] : undefined;
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

describe('exportSession argv', () => {
  it('passes inputs as flag-value pairs to the binary', () => {
    const { spawner, calls } = makeSpawner('{"success":true,"path":"/o.zip"}');
    const result = exportSession(
      { sessionId: 'sess1', cwd: '/work', out: '/out.zip', force: false },
      spawner
    );

    expect(result).toEqual({ ok: true, path: '/o.zip' });
    expect(calls[0]!.cmd).toBe(FAKE_BIN);
    expect(calls[0]!.args.slice(0, 3)).toEqual(['chat', '_', 'export-session']);
    expect(flagValue(calls[0]!.args, '--id')).toBe('sess1');
    expect(flagValue(calls[0]!.args, '--cwd')).toBe('/work');
    expect(flagValue(calls[0]!.args, '--out')).toBe('/out.zip');
  });

  it('appends --force only when the flag is set', () => {
    const { spawner: s1, calls: c1 } = makeSpawner(
      '{"success":true,"path":"/o.zip"}'
    );
    exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip', force: true },
      s1
    );
    expect(c1[0]!.args).toContain('--force');

    const { spawner: s2, calls: c2 } = makeSpawner(
      '{"success":true,"path":"/o.zip"}'
    );
    exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip', force: false },
      s2
    );
    expect(c2[0]!.args).not.toContain('--force');
  });

  it('appends --base-path when provided', () => {
    const { spawner, calls } = makeSpawner('{"success":true,"path":"/o.zip"}');
    exportSession(
      { sessionId: 's', cwd: '/w', out: '/o.zip', basePath: '/sessions' },
      spawner
    );
    expect(flagValue(calls[0]!.args, '--base-path')).toBe('/sessions');
  });
});

describe('importSession argv', () => {
  it('passes inputs as flag-value pairs to the binary', () => {
    const { spawner, calls } = makeSpawner(
      '{"success":true,"path":"/sessions/h/sess_xxx"}'
    );
    const result = importSession(
      { archivePath: '/in.zip', cwd: '/work' },
      spawner
    );

    expect(result).toEqual({ ok: true, path: '/sessions/h/sess_xxx' });
    expect(calls[0]!.cmd).toBe(FAKE_BIN);
    expect(calls[0]!.args.slice(0, 3)).toEqual(['chat', '_', 'import-session']);
    expect(flagValue(calls[0]!.args, '--archive')).toBe('/in.zip');
    expect(flagValue(calls[0]!.args, '--cwd')).toBe('/work');
  });

  it('appends --base-path when provided', () => {
    const { spawner, calls } = makeSpawner('{"success":true,"path":"/p"}');
    importSession(
      { archivePath: '/in.zip', cwd: '/work', basePath: '/sessions' },
      spawner
    );
    expect(flagValue(calls[0]!.args, '--base-path')).toBe('/sessions');
  });
});
