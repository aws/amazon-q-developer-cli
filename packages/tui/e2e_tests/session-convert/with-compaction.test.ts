/**
 * E2E test for cross-engine resume of a V2 session that contains a
 * `/compact` (a `LogEntry::Compaction`).
 *
 * Drives the real TUI via `AcpTestCase` with `--resume-id <v2-uuid>`
 * against a mock KAS ACP wire; the TUI shells out to the real
 * `chat _ ensure-session` for V2 -> KAS conversion.
 *
 * Asserts:
 *   - The compaction summary text survives into the KAS session.
 *   - The turn that ran AFTER the compaction is preserved.
 */
import { describe, it, expect, afterEach, beforeEach } from 'bun:test';
import { requireChatCliBin } from '../../src/test-utils/chat-cli-bin';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { LoadSessionRequest } from '@agentclientprotocol/sdk';
import { computeWorkspaceHash, SessionPersistence } from '@kiro/agent';
import { AcpTestCase } from '../../acp_integ_tests/shared/AcpTestCase';
import {
  assertConvertedSession,
  assertPromptIdsMapToUsers,
  assertUniqueMessageIds,
} from './assertions';
import { kasSessionIdPatternFor, setupAcpHandshake } from './test-helpers';
import {
  seedV2Session,
  WITH_COMPACTION,
} from './v2_fixtures';

const REAL_BIN = requireChatCliBin();

/** Seed `<kiroHome>/sessions/cli/<id>.{json,jsonl}` from a V2 fixture. */
function seedV2(kiroHome: string, sessionId: string, fixtureDir: string): void {
  // V2's `list_sessions_impl` filters by canonical `cwd` equality, so
  // the seeded session.json's `cwd` is rewritten to match the cwd the
  // TUI is launched in (the same `kiroHome` tempdir, realpath'd to
  // resolve macOS `/var` -> `/private/var` symlinks).
  seedV2Session({
    rootDir: join(kiroHome, 'sessions', 'cli'),
    sessionId,
    fixtureDir,
    cwd: kiroHome,
  });
}

describe('Cross-engine resume: V2 session with /compact', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string;

  beforeEach(() => {
    // `mkdtempSync` may return a path with symlink components (e.g.
    // macOS `/var/folders` -> `/private/var/folders`). The TUI's
    // `process.cwd()` resolves symlinks, so use the realpath to match
    // the bucket the converter will produce.
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-compaction-')));
    seedV2(kiroHome, WITH_COMPACTION.sessionId, WITH_COMPACTION.fixtureDir);
  });

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    rmSync(kiroHome, { recursive: true, force: true });
  });

  it('converts the compaction and preserves the summary + post-compaction turn', async () => {
    tc = new AcpTestCase({
      testName: 'ensure-session-compaction',
      args: ['--resume-id', WITH_COMPACTION.sessionId],
      // Drive the TUI from the sandbox tempdir. The converter buckets
      // by `process.cwd()` of the TUI process, so this controls the
      // workspace-hash directory the session lands under.
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

    // ID format: `cli_{v2_session_id}_{8 random alphanumeric}`. Match
    // the pattern rather than a literal so the random suffix doesn't
    // make the test brittle.
    const idPattern = kasSessionIdPatternFor(WITH_COMPACTION.sessionId);
    const targetSessionId = store.sessionId!;
    expect(targetSessionId).toMatch(idPattern);

    // Bucket is derived from the TUI's launch cwd. V2 stores sessions
    // flat by id; the converted KAS session lands in the bucket of
    // the cwd the TUI was launched in so resume works from any shell
    // directory.
    const wsHash = computeWorkspaceHash([kiroHome]);
    const kasSessionDir = join(kiroHome, 'sessions', wsHash, targetSessionId);
    expect(existsSync(join(kasSessionDir, 'session.json'))).toBe(true);
    expect(existsSync(join(kasSessionDir, 'messages.jsonl'))).toBe(true);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs).toHaveLength(1);
    expect((loadReqs[0]!.params as LoadSessionRequest).sessionId).toBe(
      targetSessionId
    );

    const persistence = new SessionPersistence();
    await persistence.initialize(join(kiroHome, 'sessions'));
    const loaded = await persistence.loadSession(targetSessionId, [kiroHome]);
    expect(loaded).not.toBeNull();
    const messages = loaded!.messages;

    // Generic converter invariants: paired tool_call/tool_result,
    // every assistant/tool_call userMessageId resolves to a real user,
    // and no two messages share an id (the duplicate-id regression).
    assertConvertedSession(messages);
    assertUniqueMessageIds(messages);
    assertPromptIdsMapToUsers(WITH_COMPACTION.messagesJsonl, messages);

    // Compaction maps to a contiguous tombstone + Summary pair.
    const tombstoneIndex = messages.findIndex(
      (m) => m.payload.type === 'tombstone'
    );
    expect(tombstoneIndex).toBeGreaterThanOrEqual(0);
    const tombstone = messages[tombstoneIndex]!;
    const summaryMsg = messages[tombstoneIndex + 1]!;
    expect(tombstone.payload).toMatchObject({
      type: 'tombstone',
      kind: 'summarization',
    });
    expect(summaryMsg.payload).toMatchObject({
      type: 'assistant',
      operationType: 'Summary',
      content: WITH_COMPACTION.summary,
    });

    // Exactly one tombstone and one Summary in the whole session.
    expect(messages.filter((m) => m.payload.type === 'tombstone')).toHaveLength(
      1
    );
    expect(
      messages.filter(
        (m) =>
          m.payload.type === 'assistant' &&
          m.payload.operationType === 'Summary'
      )
    ).toHaveLength(1);

    // The tombstone targets the first on-disk message and covers every
    // message before it, so KAS replay drops the pre-summary turns from
    // model context while they stay on disk for UI drill-down.
    if (tombstone.payload.type === 'tombstone') {
      expect(tombstone.payload.effectiveFromMessageId).toBe(messages[0]!.id);
      const effectiveBefore = messages
        .slice(0, tombstoneIndex)
        .filter((m) => m.payload.type !== 'tombstone').length;
      expect(tombstone.payload.metadata?.truncatedMessageCount).toBe(
        effectiveBefore
      );
    }

    // The post-compaction prompt is preserved with its original V2 id.
    const postCompactionPrompt = messages.find(
      (m) =>
        m.id === WITH_COMPACTION.postCompactionPromptId &&
        m.payload.type === 'user'
    );
    expect(postCompactionPrompt).toBeDefined();
  }, 60000);

  // TODO: re-capture the `with_compaction` fixture with a second
  // `/compact` driven on top of the current session (interactive TUI;
  // see fixtures/v2/with_compaction/README.md). The converter's
  // namespaced snapshot tail already makes a second tombstone target a
  // currently-effective id, so this test should pass once the fixture
  // carries two Compaction entries.
  it.todo(
    'double compaction: two tombstones each target a currently-effective id, ' +
      'no duplicate ids, both summaries present, replay keeps only the second summary + final tail',
    () => {}
  );
});
