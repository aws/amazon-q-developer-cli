/**
 * Unit tests for the shared `chat-internal-cli` runner. The shared
 * layer owns binary resolution, JSON-line parsing, error envelope
 * classification, the async spawn timeout, the Buffer-stdout sync
 * branch, and structured spawn-failure errors.
 *
 * Per-subcommand wrappers (`ensure-session-cli`, `session-archive-cli`)
 * delegate to this layer and only contribute argv assembly. Their
 * tests cover argv shape only; shared-layer behavior is exercised
 * here.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import {
  type AsyncSpawner,
  type SyncSpawner,
  runChatInternalAsync,
  runChatInternalSync,
} from '../chat-internal-cli';

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

const pickPath = (parsed: Record<string, unknown>): { path: string } | null =>
  typeof parsed.path === 'string' ? { path: parsed.path } : null;

describe('runChatInternalSync', () => {
  it('parses a successful single-line JSON response', () => {
    const calls: Array<{ cmd: string; args: string[] }> = [];
    const spawner: SyncSpawner = (cmd, args) => {
      calls.push({ cmd, args });
      return {
        status: 0,
        stdout: '{"success":true,"path":"/o.zip"}',
        stderr: '',
      };
    };
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result).toEqual({ ok: true, path: '/o.zip' });
    expect(calls[0]?.cmd).toBe(FAKE_BIN);
  });

  it('extracts the LAST JSON line when log output precedes the contract', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout:
        '[2025-01-01] DEBUG some log\n' +
        '[2025-01-01] INFO another log\n' +
        '{"success":true,"path":"/o.zip"}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result).toEqual({ ok: true, path: '/o.zip' });
  });

  it('surfaces the binary error message on a structured failure JSON', () => {
    const spawner: SyncSpawner = () => ({
      status: 1,
      stdout: '{"success":false,"error":"not implemented"}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result).toEqual({ ok: false, error: 'not implemented' });
  });

  it('carries a machine-readable code on the failure branch when present', () => {
    const spawner: SyncSpawner = () => ({
      status: 1,
      stdout:
        '{"success":false,"error":"session not found: abc","code":"SESSION_NOT_FOUND"}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result).toEqual({
      ok: false,
      error: 'session not found: abc',
      code: 'SESSION_NOT_FOUND',
    });
  });

  it('classifies non-JSON stdout as a structured error', () => {
    const spawner: SyncSpawner = () => ({
      status: 1,
      stdout: 'panic: something',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toMatch(/Unexpected output/);
    }
  });

  it('errors when the success JSON is missing the required fields', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout: '{"success":true}',
      stderr: '',
    });
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toMatch(/Malformed/);
    }
  });

  it('errors when KIRO_CHAT_CLI_BIN is unset', () => {
    delete process.env.KIRO_CHAT_CLI_BIN;
    const calls: Array<{ cmd: string; args: string[] }> = [];
    const spawner: SyncSpawner = (cmd, args) => {
      calls.push({ cmd, args });
      return { status: 0, stdout: '{}', stderr: '' };
    };
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result.ok).toBe(false);
    expect(calls).toHaveLength(0);
  });

  /// Sync spawners that surface the spawn failure on `result.error`
  /// (mirrors `child_process.spawnSync` shape) get translated into a
  /// structured failure rather than passing through.
  it('classifies a spawn-time error result as a structured failure', () => {
    const spawner: SyncSpawner = () => ({
      status: -1,
      stdout: '',
      stderr: '',
      error: Object.assign(new Error('spawn ENOENT'), { code: 'ENOENT' }),
    });
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toMatch(/Failed to spawn kiro-cli/);
      expect(result.error).toContain('ENOENT');
    }
  });

  /// Bun.spawnSync returns Buffer stdout; the helper must decode it
  /// rather than coerce a Buffer through `String()` (which would
  /// produce e.g. `"[object Object]"`).
  it('accepts Buffer stdout and decodes it to string', () => {
    const spawner: SyncSpawner = () => ({
      status: 0,
      stdout: Buffer.from('{"success":true,"path":"/o.zip"}'),
      stderr: '',
    });
    const result = runChatInternalSync(['x'], pickPath, spawner);
    expect(result).toEqual({ ok: true, path: '/o.zip' });
  });
});

describe('runChatInternalAsync', () => {
  it('parses a successful response', async () => {
    const spawner: AsyncSpawner = async () => ({
      status: 0,
      stdout: '{"success":true,"path":"/o.zip"}',
      stderr: '',
    });
    const result = await runChatInternalAsync(['x'], pickPath, spawner);
    expect(result).toEqual({ ok: true, path: '/o.zip' });
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
    const result = await runChatInternalAsync(['x'], pickPath, spawner, 100);
    const elapsed = Date.now() - start;
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toMatch(/did not complete/);
    }
    expect(elapsed).toBeLessThan(2_000);
    expect(resolved).toBe(false);
  });

  /// A spawner that throws during invocation gets wrapped in a
  /// structured failure rather than propagating to the caller.
  it('classifies a spawn-time exception as a structured error', async () => {
    const spawner: AsyncSpawner = async () => {
      throw new Error('ENOENT');
    };
    const result = await runChatInternalAsync(['x'], pickPath, spawner);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error).toMatch(/Failed to spawn kiro-cli: ENOENT/);
    }
  });
});
