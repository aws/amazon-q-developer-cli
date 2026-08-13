/**
 * Unit tests for the shared `chat-internal-cli` runner. The shared
 * layer owns binary resolution, JSON-line parsing, exit-code
 * verification, the async spawn timeout, the Buffer-stdout sync
 * branch, and structured spawn-failure errors.
 *
 * Per-subcommand wrappers (`ensure-session-cli`, `session-archive-cli`)
 * delegate to this layer and only contribute argv assembly. Their
 * tests cover argv shape only; shared-layer behavior is exercised
 * here.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import {
  type AsyncSpawner,
  type SyncSpawner,
  runChatInternalAsync,
  runChatInternalSync,
} from '../chat-internal-cli';
import { ErrorCode } from '../../types/generated/chat-internal';

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

describe('binary fallback through the runner', () => {
  // Exercises the DEFAULT resolution wiring with a real filesystem: the
  // env path is gone, so the runner must spawn the stable install
  // location under $HOME. Resolution reads process.env.HOME first, so
  // the test owns the fallback location hermetically on every platform.
  let originalHome: string | undefined;
  let tempHome: string;
  let fallbackBin: string;

  beforeEach(() => {
    originalHome = process.env.HOME;
    tempHome = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-cli-bin-test-'));
    fallbackBin = path.join(tempHome, '.local', 'bin', 'kiro-cli');
    fs.mkdirSync(path.dirname(fallbackBin), { recursive: true });
    fs.writeFileSync(fallbackBin, '');
    process.env.HOME = tempHome;
    process.env.KIRO_CHAT_CLI_BIN = path.join(
      tempHome,
      'deleted-install',
      'kiro-cli-chat'
    );
  });

  afterEach(() => {
    if (originalHome === undefined) delete process.env.HOME;
    else process.env.HOME = originalHome;
    fs.rmSync(tempHome, { recursive: true, force: true });
  });

  it('sync: spawns ~/.local/bin/kiro-cli when the env path is deleted', () => {
    const calls: Array<{ cmd: string }> = [];
    const spawner: SyncSpawner = (cmd) => {
      calls.push({ cmd });
      return {
        status: 0,
        stdout: '{"kind":"exportSession","data":{"path":"/o.zip"}}',
        stderr: '',
      };
    };
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(true);
    expect(calls[0]?.cmd).toBe(fallbackBin);
  });

  it('async: spawns ~/.local/bin/kiro-cli when the env path is deleted', async () => {
    const calls: Array<{ cmd: string }> = [];
    const spawner: AsyncSpawner = (cmd) => {
      calls.push({ cmd });
      return Promise.resolve({
        status: 0,
        stdout: '{"kind":"exportSession","data":{"path":"/o.zip"}}',
        stderr: '',
      });
    };
    const result = await runChatInternalAsync(['x'], spawner);
    expect(result.ok).toBe(true);
    expect(calls[0]?.cmd).toBe(fallbackBin);
  });
});

describe('runChatInternalSync', () => {
  it('parses a successful single-line JSON response', () => {
    const calls: Array<{ cmd: string; args: string[] }> = [];
    const spawner: SyncSpawner = (cmd, args) => {
      calls.push({ cmd, args });
      return {
        status: 0,
        stdout: '{"kind":"exportSession","data":{"path":"/o.zip"}}',
        stderr: '',
      };
    };
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.output).toEqual({
        kind: 'exportSession',
        data: { path: '/o.zip' },
      });
    }
    expect(calls[0]?.cmd).toBe(FAKE_BIN);
  });

  it('extracts the LAST JSON line when log output precedes the contract', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout:
        '[2025-01-01] DEBUG some log\n' +
        '[2025-01-01] INFO another log\n' +
        '{"kind":"exportSession","data":{"path":"/o.zip"}}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.output.kind).toBe('exportSession');
    }
  });

  it('returns the error envelope as a typed `error` kind', () => {
    const spawner: SyncSpawner = () => ({
      status: 1,
      stdout: '{"kind":"error","data":{"message":"not implemented"}}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.output).toEqual({
        kind: 'error',
        data: { message: 'not implemented' },
      });
    }
  });

  it('carries a machine-readable code on the error envelope when present', () => {
    const spawner: SyncSpawner = () => ({
      status: 1,
      stdout:
        '{"kind":"error","data":{"message":"session not found: abc","code":"SESSION_NOT_FOUND"}}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(true);
    if (result.ok && result.output.kind === 'error') {
      expect(result.output.data.message).toBe('session not found: abc');
      expect(result.output.data.code).toBe(ErrorCode.SessionNotFound);
    }
  });

  it('classifies non-JSON stdout as a host-side failure', () => {
    const spawner: SyncSpawner = () => ({
      status: 1,
      stdout: 'panic: something',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.message).toMatch(/Unexpected output/);
    }
  });

  it('classifies a JSON object missing `kind` as a host-side failure', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout: '{"foo":"bar"}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.message).toMatch(/Malformed/);
    }
  });

  it('classifies a JSON object missing `data` as a host-side failure', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout: '{"kind":"exportSession"}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.message).toMatch(/Malformed/);
    }
  });

  it('classifies a null `data` as a host-side failure (avoid downstream NPE)', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout: '{"kind":"exportSession","data":null}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.message).toMatch(/Malformed/);
    }
  });

  it('errors when KIRO_CHAT_CLI_BIN is unset', () => {
    delete process.env.KIRO_CHAT_CLI_BIN;
    const calls: Array<{ cmd: string; args: string[] }> = [];
    const spawner: SyncSpawner = (cmd, args) => {
      calls.push({ cmd, args });
      return { status: 0, stdout: '{}', stderr: '' };
    };
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(false);
    expect(calls).toHaveLength(0);
  });

  /// Sync spawners that surface the spawn failure on `result.error`
  /// (mirrors `child_process.spawnSync` shape) get translated into a
  /// structured failure rather than passing through.
  it('classifies a spawn-time error result as a host-side failure', () => {
    const spawner: SyncSpawner = () => ({
      status: -1,
      stdout: '',
      stderr: '',
      error: Object.assign(new Error('spawn ENOENT'), { code: 'ENOENT' }),
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.message).toMatch(/Failed to spawn kiro-cli/);
      expect(result.message).toContain('ENOENT');
    }
  });

  /// Bun.spawnSync returns Buffer stdout; the helper must decode it
  /// rather than coerce a Buffer through `String()` (which would
  /// produce e.g. `"[object Object]"`).
  it('accepts Buffer stdout and decodes it to string', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout: Buffer.from('{"kind":"exportSession","data":{"path":"/o.zip"}}'),
      stderr: '',
    });
    const result = runChatInternalSync(['x'], spawner);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.output.kind).toBe('exportSession');
    }
  });
});

describe('runChatInternalAsync', () => {
  it('parses a successful response', async () => {
    const spawner: AsyncSpawner = async () => ({
      status: 0,
      stdout: '{"kind":"exportSession","data":{"path":"/o.zip"}}',
      stderr: '',
    });
    const result = await runChatInternalAsync(['x'], spawner);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.output.kind).toBe('exportSession');
    }
  });

  /// A spawner that hangs longer than the timeout produces a
  /// structured timeout error rather than blocking the caller forever.
  it('times out a spawn that never resolves', async () => {
    let resolved = false;
    const spawner: AsyncSpawner = () =>
      new Promise((r) => {
        // Resolve far past the timeout so the test sees the timeout
        // path, not the spawn-success path.
        setTimeout(() => {
          resolved = true;
          r({ status: 0, stdout: '{}', stderr: '' });
        }, 10_000);
      });
    const start = Date.now();
    const result = await runChatInternalAsync(['x'], spawner, 100);
    const elapsed = Date.now() - start;
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.message).toMatch(/did not complete/);
    }
    expect(elapsed).toBeLessThan(2_000);
    expect(resolved).toBe(false);
  });

  /// On timeout, the spawned child is aborted via the AbortSignal so
  /// its OS-level process gets killed rather than left running.
  it('aborts the spawner via signal when the timeout fires', async () => {
    let aborted = false;
    const spawner: AsyncSpawner = (_cmd, _args, options) =>
      new Promise((_, reject) => {
        options?.signal?.addEventListener('abort', () => {
          aborted = true;
          reject(new Error('aborted'));
        });
      });
    const result = await runChatInternalAsync(['x'], spawner, 50);
    expect(result.ok).toBe(false);
    expect(aborted).toBe(true);
  });

  /// On the spawn-success path, the timeout's `setTimeout` must be
  /// cleared so it does not keep the event loop alive (otherwise every
  /// auth-callback round trip leaks a 30s-pending timer).
  it('clears the timeout when the spawn resolves quickly', async () => {
    const originalSetTimeout = globalThis.setTimeout;
    const originalClearTimeout = globalThis.clearTimeout;
    const pending = new Set<unknown>();
    globalThis.setTimeout = ((fn: () => void, ms: number) => {
      const id = originalSetTimeout(fn, ms);
      pending.add(id);
      return id;
    }) as typeof setTimeout;
    globalThis.clearTimeout = ((id: unknown) => {
      pending.delete(id);
      return originalClearTimeout(id as Parameters<typeof clearTimeout>[0]);
    }) as typeof clearTimeout;
    try {
      const spawner: AsyncSpawner = async () => ({
        status: 0,
        stdout: '{"kind":"exportSession","data":{"path":"/o.zip"}}',
        stderr: '',
      });
      await runChatInternalAsync(['x'], spawner, 30_000);
      expect(pending.size).toBe(0);
    } finally {
      globalThis.setTimeout = originalSetTimeout;
      globalThis.clearTimeout = originalClearTimeout;
    }
  });

  /// A spawner that throws during invocation gets wrapped in a
  /// structured failure rather than propagating to the caller.
  it('classifies a spawn-time exception as a host-side failure', async () => {
    const spawner: AsyncSpawner = async () => {
      throw new Error('ENOENT');
    };
    const result = await runChatInternalAsync(['x'], spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.message).toMatch(/Failed to spawn kiro-cli: ENOENT/);
    }
  });
});
