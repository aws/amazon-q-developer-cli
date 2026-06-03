/**
 * E2E tests for `kiro-cli chat _ derive-messages`.
 *
 * `derive-messages` exposes V2's `EventLog::derive_messages` so cross-
 * engine converter tests can use V2's own derived messages as the
 * "expected" side when comparing against KAS replay output. The
 * subcommand is headless and JSON-on-stdout; spawnSync is the right
 * driver, not the PTY-based `E2ETestCase`.
 *
 * Output contract:
 * - Success (exit 0): `{success: true, sessionId: "<v2-uuid>", messages: [...]}`
 * - Failure (exit 1): `{success: false, error: "<message>"}`
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { requireChatCliBin } from '../../src/utils/chat-cli-bin';
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
  stdout: string;
  stderr: string;
  /** Last stdout line parsed as JSON. Logs may precede it. */
  parsed: Record<string, unknown>;
}

function runCli(args: string[]): RunResult {
  const res = spawnSync(BIN, ['chat', '_', ...args], { encoding: 'utf8' });
  if (res.error) throw res.error;
  const lines = res.stdout.trim().split('\n').filter(Boolean);
  if (lines.length === 0) {
    throw new Error(
      `binary produced no stdout (exit=${res.status}); stderr=${res.stderr}`
    );
  }
  const last = lines[lines.length - 1]!;
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(last);
  } catch (cause) {
    throw new Error(
      `last stdout line is not JSON: ${JSON.stringify(last)} (exit=${res.status}, stderr=${res.stderr})`,
      { cause }
    );
  }
  return {
    exitCode: res.status,
    stdout: res.stdout,
    stderr: res.stderr,
    parsed,
  };
}

/** Copy the V2 fixture files into `<root>/<id>.{json,jsonl}`. */
function seedV2(rootDir: string, sessionId: string, fixtureDir: string): void {
  // `derive-messages` loads sessions by id, never via `list_sessions_impl`,
  // so the seeded `cwd` value is irrelevant to this test - any string is
  // safe. Use the test root for symmetry with other e2e tests.
  seedV2Session({ rootDir, sessionId, fixtureDir, cwd: rootDir });
}

describe('chat _ derive-messages (V2 -> Vec<Message>)', () => {
  let tmp: string;

  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), 'kiro-derive-test-'));
  });

  afterEach(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  // Skipped until `derive-messages` is implemented alongside the
  // KAS -> V2 migration flow. The handler today emits
  // `{success:false, error:"not implemented"}`; this test pins the
  // success-path payload that the implementation will produce.
  it.skip('returns V2 model-context messages on success', () => {
    const sessionsDir = join(tmp, 'v2-sessions');
    seedV2(sessionsDir, BASIC_FS_TOOLS.sessionId, BASIC_FS_TOOLS.fixtureDir);

    const res = runCli([
      'derive-messages',
      '--sessions-dir',
      sessionsDir,
      '--session-id',
      BASIC_FS_TOOLS.sessionId,
    ]);

    expect(res.exitCode).toBe(0);
    expect(res.parsed.success).toBe(true);
    expect(res.parsed.error).toBeUndefined();
    expect(res.parsed.sessionId).toBe(BASIC_FS_TOOLS.sessionId);

    const messages = res.parsed.messages as unknown[];
    expect(Array.isArray(messages)).toBe(true);
    // The fixture has 1 user prompt + 7 assistant turns + 6 tool result
    // turns. V2's `derive_messages` collapses those into model-shaped
    // user/assistant pairs; assert non-empty here, count-pinning lives
    // in the parity test.
    expect(messages.length).toBeGreaterThan(0);
  });

  it('emits {success:false, error} for a missing session id', () => {
    const sessionsDir = join(tmp, 'v2-sessions');
    mkdirSync(sessionsDir, { recursive: true });

    const res = runCli([
      'derive-messages',
      '--sessions-dir',
      sessionsDir,
      '--session-id',
      'does-not-exist',
    ]);

    expect(res.exitCode).toBe(1);
    expect(res.parsed.success).toBe(false);
    expect(typeof res.parsed.error).toBe('string');
    expect(res.parsed.messages).toBeUndefined();
  });
});
