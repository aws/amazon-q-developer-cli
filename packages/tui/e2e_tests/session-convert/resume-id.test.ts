/**
 * E2E tests for `--resume-id <id>` against the real
 * `target/debug/chat_cli` binary in KAS mode.
 *
 * Active engine is KAS. The user supplies a session id from any
 * source format (V2 UUID, V1 SQLite conversation id, or an id no
 * source recognizes). The TUI shells out to
 * `chat _ ensure-session --source-format auto ...` to resolve the
 * id into a KAS-owned session before the agent handshake:
 *
 *   - V2 UUID -> V2 -> KAS conversion via the converter, minted
 *     id `cli_<v2_id>_<8 alphanumeric>`.
 *   - V1 SQLite conversation id -> V1 -> V2 -> KAS conversion,
 *     minted id `cli_<v1_id>_<8 alphanumeric>`.
 *   - Unknown id (no source recognizes it) -> error banner
 *     surfaced via `setAgentError`, fall through to a fresh
 *     session via `session/new` so the user isn't stuck.
 *
 * Mock + real-binary setup:
 *   - The KAS ACP wire is a mock socket (no real KAS process).
 *   - `KIRO_CHAT_CLI_BIN` points at `target/debug/chat_cli`.
 *   - `KIRO_HOME` points at a sandbox so V2 byproducts and the
 *     KAS dir land under the sandbox.
 *   - V1 tests additionally set `KIRO_TEST_DB_PATH` so the
 *     classic SQLite store is sandboxed too.
 *
 * V1 seeding spawns the binary's hidden `chat _ test-seed-v1`
 * subcommand against a `ConversationState` fixture; that keeps
 * SQLite schema knowledge in rust where it belongs.
 *
 * Fixture-content assertions live in `basic-fs-tools.test.ts`
 * (reused here via `assertBasicFsToolsConverted` for the V2 case).
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import type { LoadSessionRequest } from '@agentclientprotocol/sdk';
import { computeWorkspaceHash, SessionPersistence } from '@kiro/agent';

import { AcpTestCase } from '../../acp_integ_tests/shared/AcpTestCase';
import { requireChatCliBin } from '../../src/utils/chat-cli-bin';
import { assertConvertedSession } from './assertions';
import { assertBasicFsToolsConverted } from './basic-fs-tools-assertions';
import { kasSessionIdPatternFor, setupAcpHandshake } from './test-helpers';
import { BASIC_FS_TOOLS, seedV2Session } from './v2_fixtures';

const REAL_BIN = requireChatCliBin();

const V1_FIXTURE_PATH = resolve(
  __dirname,
  '../../../../crates/chat-cli/src/cli/chat/v1_export/fixtures/basic_tool_use.json'
);

// Conversation id baked into the V1 fixture JSON. Asserted directly
// so a fixture swap forces an explicit test update.
const V1_CONVERSATION_ID = '26a591e0-dd86-44ee-84e2-7c44ddda8fdb';

/**
 * Seed a V1 (classic) conversation row into a sandbox SQLite by
 * shelling out to the binary's hidden `chat _ test-seed-v1`
 * subcommand. Reuses the rust-side fixture JSONs and
 * `Database::set_conversation_by_path` so the SQLite schema lives
 * exclusively in rust. The sandbox path is communicated via the
 * `KIRO_TEST_DB_PATH` env var rather than a CLI arg so the seed
 * subcommand and the binary's runtime DB resolver share the same
 * source of truth.
 */
function seedV1Session(opts: {
  binPath: string;
  fixturePath: string;
  dbPath: string;
  cwd: string;
}): { conversationId: string } {
  const result = spawnSync(
    opts.binPath,
    [
      'chat',
      '_',
      'test-seed-v1',
      '--fixture-path',
      opts.fixturePath,
      '--cwd',
      opts.cwd,
    ],
    {
      encoding: 'utf8',
      env: { ...process.env, KIRO_TEST_DB_PATH: opts.dbPath },
    }
  );
  if (result.status !== 0) {
    throw new Error(
      `test-seed-v1 failed (status ${result.status}):\n` +
        `stdout: ${result.stdout}\nstderr: ${result.stderr}`
    );
  }
  const parsed = JSON.parse(result.stdout) as {
    success: boolean;
    conversationId?: string;
    error?: string;
  };
  if (!parsed.success || !parsed.conversationId) {
    throw new Error(`test-seed-v1 returned not-ok: ${result.stdout}`);
  }
  return { conversationId: parsed.conversationId };
}

describe('--resume-id', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string;

  // realpath() because macOS `/var/folders` symlinks resolve on the
  // converter side via `process.cwd()`; the bucket-hash uses the
  // resolved path so the test sandbox path must match.
  beforeEach(() => {
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-resume-id-')));
  });

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    rmSync(kiroHome, { recursive: true, force: true });
  });

  describe('with a V2 source session id', () => {
    beforeEach(() => {
      seedV2Session({
        rootDir: join(kiroHome, 'sessions', 'cli'),
        sessionId: BASIC_FS_TOOLS.sessionId,
        fixtureDir: BASIC_FS_TOOLS.fixtureDir,
        cwd: kiroHome,
      });
    });

    it(`converts the ${BASIC_FS_TOOLS.name} fixture and loads the resulting KAS id`, async () => {
      tc = new AcpTestCase({
        testName: 'ensure-session-resume-id',
        args: ['--resume-id', BASIC_FS_TOOLS.sessionId],
        cwd: kiroHome,
        extraEnv: {
          KIRO_CHAT_CLI_BIN: REAL_BIN,
          KIRO_HOME: kiroHome,
        },
      });

      setupAcpHandshake(tc);

      await tc.launch();
      await tc.mock.awaitConnection();

      const store = await tc.waitForStore((s) => Boolean(s.sessionId), 30_000);

      const wsHash = computeWorkspaceHash([kiroHome]);
      const bucketDir = join(kiroHome, 'sessions', wsHash);
      expect(existsSync(bucketDir)).toBe(true);

      // ID format: `cli_{v2_session_id}_{8 random alphanumeric}`.
      // The suffix prevents collision when the same V2 session is
      // converted twice; assert the format pattern, not a literal.
      const idPattern = kasSessionIdPatternFor(BASIC_FS_TOOLS.sessionId);
      const targetSessionId = store.sessionId!;
      expect(targetSessionId).toMatch(idPattern);

      const kasSessionDir = join(bucketDir, targetSessionId);
      expect(existsSync(kasSessionDir)).toBe(true);
      expect(existsSync(join(kasSessionDir, 'session.json'))).toBe(true);
      expect(existsSync(join(kasSessionDir, 'messages.jsonl'))).toBe(true);

      const metadata = JSON.parse(
        readFileSync(join(kasSessionDir, 'session.json'), 'utf8')
      );
      expect(metadata.id).toBe(targetSessionId);
      expect(metadata.schemaVersion).toBe('1.0.0');
      expect(metadata.agentMode).toBe('vibe');
      expect(metadata.workspacePaths).toEqual([kiroHome]);
      // KAS stamps `1` on save; converter output matches.
      expect(metadata.dataModelVersion).toBe(1);

      const loadReqs = tc.mock.receivedRequests('session/load');
      expect(loadReqs).toHaveLength(1);
      const params = loadReqs[0]!.params as LoadSessionRequest;
      expect(params.sessionId).toBe(targetSessionId);

      const persistence = new SessionPersistence();
      await persistence.initialize(join(kiroHome, 'sessions'));
      const loaded = await persistence.loadSession(targetSessionId, [
        kiroHome,
      ]);
      expect(loaded).not.toBeNull();
      const messages = loaded!.messages;

      assertConvertedSession(messages);
      assertBasicFsToolsConverted(messages);
    }, 60000);
  });

  describe('with a V1 (classic) source session id', () => {
    let dbPath: string;

    beforeEach(() => {
      dbPath = join(kiroHome, 'data.sqlite3');
      // V1 buckets the conversation by the cwd it was last used in.
      // Using `kiroHome` keeps the bucket aligned with the launch
      // cwd so the converter's workspace-hash and V1's lookup agree.
      const seeded = seedV1Session({
        binPath: REAL_BIN,
        fixturePath: V1_FIXTURE_PATH,
        dbPath,
        cwd: kiroHome,
      });
      expect(seeded.conversationId).toBe(V1_CONVERSATION_ID);
    });

    it('converts the V1 fixture and loads the resulting KAS id', async () => {
      tc = new AcpTestCase({
        testName: 'ensure-session-resume-id-v1',
        args: ['--resume-id', V1_CONVERSATION_ID],
        cwd: kiroHome,
        extraEnv: {
          KIRO_CHAT_CLI_BIN: REAL_BIN,
          KIRO_HOME: kiroHome,
          KIRO_TEST_DB_PATH: dbPath,
        },
      });

      setupAcpHandshake(tc);

      await tc.launch();
      await tc.mock.awaitConnection();

      const store = await tc.waitForStore((s) => Boolean(s.sessionId), 30_000);

      const idPattern = kasSessionIdPatternFor(V1_CONVERSATION_ID);
      const targetSessionId = store.sessionId!;
      expect(targetSessionId).toMatch(idPattern);

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

  describe('with an unknown id (no source recognizes it)', () => {
    it('boots a fresh session and surfaces an error banner', async () => {
      const bogusId = '00000000-0000-0000-0000-000000000000';
      const freshBootId = 'sess-fresh-after-not-found';

      tc = new AcpTestCase({
        testName: 'resume-id-not-found',
        args: ['--resume-id', bogusId],
        cwd: kiroHome,
        extraEnv: {
          KIRO_CHAT_CLI_BIN: REAL_BIN,
          KIRO_HOME: kiroHome,
        },
      });

      // The fall-through path issues `session/new` to mint a fresh
      // session after the not-found error is surfaced.
      setupAcpHandshake(tc, { bootSessionId: freshBootId });

      await tc.launch();
      await tc.mock.awaitConnection();

      const store = await tc.waitForStore(
        (s) => s.sessionId === freshBootId && Boolean(s.agentError),
        15000
      );
      expect(store.sessionId).toBe(freshBootId);
      expect(store.agentError).toBeTruthy();
      expect(store.agentError!).toContain(bogusId);
      expect(store.agentErrorGuidance).toBeTruthy();
      expect(store.agentErrorGuidance!.toLowerCase()).toContain(
        'starting a new session'
      );

      // The TUI must NOT have attempted to load the bogus id - the
      // ensure-session pre-resolution prevents the agent from ever
      // seeing an id it doesn't own.
      expect(tc.mock.receivedRequests('session/load')).toHaveLength(0);
      expect(tc.mock.receivedRequests('session/new')).toHaveLength(1);
    }, 30000);
  });
});
