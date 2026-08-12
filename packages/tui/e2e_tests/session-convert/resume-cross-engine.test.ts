/**
 * E2E test for cross-engine `--resume` (most-recent).
 *
 * Drives the real TUI via `AcpTestCase` with `--resume` (no id)
 * against a mock KAS ACP wire. The TUI is expected to shell out
 * to the real `chat --list-sessions --format json`, pick the
 * most-recent entry across V1+V2+KAS for the launch cwd, and
 * route through `chat _ ensure-session` if the winner is in a
 * non-active engine.
 *
 * Scenario covered: V2-only on disk in KAS mode. The V2 session
 * is the only candidate, so it wins, gets converted, and
 * `session/load` is called with `cli_<v2_id>_<random>`.
 *
 * Asserts (end-state, not spawn-spy):
 *   - The TUI's resolved session id matches `cli_<v2_id>_<8 alphanumeric>`.
 *   - A KAS session dir exists at
 *     `<kiroHome>/sessions/<wsHash([kiroHome])>/<that-id>/`.
 *   - `session/load` was called exactly once with that id.
 */
import { describe, it, expect, afterEach, beforeEach } from 'bun:test';
import { requireChatCliBin } from '../../src/test-utils/chat-cli-bin';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { LoadSessionRequest } from '@agentclientprotocol/sdk';
import { computeWorkspaceHash } from '@kiro/agent';
import { AcpTestCase } from '../../src/test-utils/acp-mock/AcpTestCase';
import { kasSessionIdPatternFor, setupAcpHandshake } from './test-helpers';
import {
  BASIC_FS_TOOLS,
  seedV2Session,
} from './v2_fixtures';

const REAL_BIN = requireChatCliBin();

describe('Cross-engine --resume: V2 session in KAS mode', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string;

  beforeEach(() => {
    // `mkdtempSync` may produce a path with symlink components on
    // macOS (`/var/folders` -> `/private/var/folders`). The TUI
    // resolves symlinks via `process.cwd()`, so the realpath is what
    // the bucket-hash will be computed from downstream.
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-resume-')));
    seedV2Session({
      rootDir: join(kiroHome, 'sessions', 'cli'),
      sessionId: BASIC_FS_TOOLS.sessionId,
      fixtureDir: BASIC_FS_TOOLS.fixtureDir,
      cwd: kiroHome,
    });
  });

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    rmSync(kiroHome, { recursive: true, force: true });
  });

  it('discovers a V2 session via --list-sessions and converts it on resume', async () => {
    tc = new AcpTestCase({
      testName: 'resume-cross-engine-v2-only',
      args: ['--resume'],
      cwd: kiroHome,
      extraEnv: {
        KIRO_CHAT_CLI_BIN: REAL_BIN,
        KIRO_HOME: kiroHome,
        // No KAS sessions on disk; the listing is V2-only after the
        // empty KAS array merges in. The V2 entry is the lone candidate
        // and the most-recent winner by default.
        KIRO_TEST_MOCK_KAS_SESSIONS: '[]',
      },
    });

    setupAcpHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();

    const store = await tc.waitForStore((s) => Boolean(s.sessionId), 30_000);

    const idPattern = kasSessionIdPatternFor(BASIC_FS_TOOLS.sessionId);
    const targetSessionId = store.sessionId!;
    expect(targetSessionId).toMatch(idPattern);

    // Bucket is derived from the TUI's launch cwd. The V2 source
    // session's recorded cwd is irrelevant on cross-engine import;
    // the converter buckets by the runtime cwd so resume works from
    // any shell directory.
    const wsHash = computeWorkspaceHash([kiroHome]);
    const kasSessionDir = join(kiroHome, 'sessions', wsHash, targetSessionId);
    expect(existsSync(join(kasSessionDir, 'session.json'))).toBe(true);
    expect(existsSync(join(kasSessionDir, 'messages.jsonl'))).toBe(true);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs).toHaveLength(1);
    expect((loadReqs[0]!.params as LoadSessionRequest).sessionId).toBe(
      targetSessionId
    );
  }, 60000);
});
