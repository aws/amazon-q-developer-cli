/**
 * E2E tests for `kiro-cli chat _ ensure-session --target-format v2`.
 *
 * The default (Rust/V2) engine resumes a V2 id by handing it straight
 * to `session/load`; the TUI calls `ensure-session` first to confirm
 * the id resolves. A V2 source is already in the V2 target format, so
 * the subcommand only checks the session exists:
 *   - present id  -> `{kind: "ensureSession", data: {sessionId: "<id>"}}`, exit 0
 *   - missing id  -> `{kind: "error", data: {code: "SESSION_NOT_FOUND"}}`, exit 1
 *
 * The TUI branches on `SESSION_NOT_FOUND` to fall through to a fresh
 * session silently, matching the backend's own not-found behavior.
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { requireChatCliBin } from '../../src/utils/chat-cli-bin';
import type { CliInternalOutput } from '../../src/types/generated/chat-internal';
import { ErrorCode } from '../../src/types/generated/chat-internal';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  BASIC_FS_TOOLS,
  seedV2Session,
} from './v2_fixtures';

const BIN = requireChatCliBin();

interface RunResult {
  exitCode: number | null;
  parsed: CliInternalOutput;
}

function runEnsureSession(sessionsDir: string, args: string[]): RunResult {
  const res = spawnSync(BIN, ['chat', '_', 'ensure-session', ...args], {
    encoding: 'utf8',
    env: { ...process.env, KIRO_TEST_SESSIONS_DIR: sessionsDir },
  });
  if (res.error) throw res.error;
  const lines = res.stdout.trim().split('\n').filter(Boolean);
  if (lines.length === 0) {
    throw new Error(
      `binary produced no stdout (exit=${res.status}); stderr=${res.stderr}`
    );
  }
  const parsed = JSON.parse(lines[lines.length - 1]!) as CliInternalOutput;
  return { exitCode: res.status, parsed };
}

function seedV2(rootDir: string, sessionId: string, fixtureDir: string): void {
  // `ensure-session --target-format v2` resolves a session by direct
  // filename probe, never via `list_sessions_impl`. The seeded `cwd`
  // value is irrelevant to this test - any string is safe. Use the
  // sessions dir itself for symmetry.
  seedV2Session({ rootDir, sessionId, fixtureDir, cwd: rootDir });
}

describe('chat _ ensure-session --target-format v2', () => {
  let tmp: string;
  let sessionsDir: string;

  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), 'kiro-ensure-v2-'));
    sessionsDir = join(tmp, 'v2-sessions');
    mkdirSync(sessionsDir, { recursive: true });
  });

  afterEach(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  it('returns the id unchanged when the V2 session exists', () => {
    seedV2(sessionsDir, BASIC_FS_TOOLS.sessionId, BASIC_FS_TOOLS.fixtureDir);

    const res = runEnsureSession(sessionsDir, [
      '--source-format',
      'auto',
      '--source-session-id',
      BASIC_FS_TOOLS.sessionId,
      '--target-format',
      'v2',
      '--cwd',
      tmp,
    ]);

    expect(res.exitCode).toBe(0);
    expect(res.parsed.kind).toBe('ensureSession');
    if (res.parsed.kind === 'ensureSession') {
      expect(res.parsed.data.sessionId).toBe(BASIC_FS_TOOLS.sessionId);
    }
  });

  it('reports SESSION_NOT_FOUND for a missing V2 session', () => {
    const res = runEnsureSession(sessionsDir, [
      '--source-format',
      'auto',
      '--source-session-id',
      'does-not-exist',
      '--target-format',
      'v2',
      '--cwd',
      tmp,
    ]);

    expect(res.exitCode).toBe(1);
    expect(res.parsed.kind).toBe('error');
    if (res.parsed.kind === 'error') {
      expect(res.parsed.data.code).toBe(ErrorCode.SessionNotFound);
    }
  });
});
