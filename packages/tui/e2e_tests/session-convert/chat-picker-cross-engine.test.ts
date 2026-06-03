/**
 * E2E test for cross-engine `/chat` picker.
 *
 * Drives the real TUI via `AcpTestCase`. With a real V2 session
 * on disk and a mocked KAS-side entry (older), the picker is
 * expected to consume the merged `chat --list-sessions --format
 * json` output, sort by `updatedAt` desc (V2 wins), and on
 * selection route the V2 entry through `chat _ ensure-session`
 * before issuing `session/load` against the converted id.
 *
 * Scenario covered: both engines populated, V2 is most-recent.
 *
 * Asserts (end-state, not spawn-spy):
 *   - On `/chat` + Enter the picker becomes visible.
 *   - Selecting the highlighted (most-recent) entry triggers
 *     a `session/load` with id `cli_<v2_id>_<8 alphanumeric>`.
 *   - A KAS session dir exists at
 *     `<kiroHome>/sessions/<wsHash([kiroHome])>/<that-id>/`.
 */
import { describe, it, expect, afterEach, beforeEach } from 'bun:test';
import { requireChatCliBin } from '../../src/utils/chat-cli-bin';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { LoadSessionRequest } from '@agentclientprotocol/sdk';
import { computeWorkspaceHash } from '@kiro/agent';
import { AcpTestCase } from '../../acp_integ_tests/shared/AcpTestCase';
import { kasSessionIdPatternFor, setupAcpHandshake } from './test-helpers';
import {
  BASIC_FS_TOOLS,
  seedV2Session,
} from './v2_fixtures';

const REAL_BIN = requireChatCliBin();

describe('Cross-engine /chat picker: V2 wins over KAS', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string;

  beforeEach(() => {
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-chatpicker-')));
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

  it('shows V2 + KAS entries merged, picks V2 (most recent), converts on selection', async () => {
    // KAS-side entry that pre-dates the V2 fixture's captured
    // updated_at by a year. Returned by the binary's
    // --list-sessions path via the harness's mock-KAS hook so the
    // listing covers V1+V2+KAS without an actual KAS spawn.
    const olderKasUpdatedAt = '2025-01-01T00:00:00.000Z';

    tc = new AcpTestCase({
      testName: 'chat-picker-cross-engine',
      cwd: kiroHome,
      extraEnv: {
        KIRO_CHAT_CLI_BIN: REAL_BIN,
        KIRO_HOME: kiroHome,
      },
      mockKasSessionListResult: [
        {
          sessionId: 'sess-stale-kas-1',
          cwd: kiroHome,
          title: 'Stale KAS session',
          updatedAt: olderKasUpdatedAt,
        },
      ],
    });

    // The boot path issues `session/new` since there's no `--resume*`
    // flag; only the picker selection later triggers `session/load`.
    setupAcpHandshake(tc, { bootSessionId: 'sess-boot-fresh' });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // Open the picker. The V2 fixture is more recent than the mocked
    // KAS entry, so it lands at the top and is highlighted by default.
    await tc.sendKeys('/chat');
    await tc.sleepMs(300);
    await tc.waitForVisibleText('/chat', 5000);
    await tc.sendKeys('\r');

    // Both entries must be visible in the merged listing. The V2 entry's
    // session id appears in the picker line; the KAS entry's title
    // is enough evidence the merge ran.
    await tc.waitForVisibleText(BASIC_FS_TOOLS.sessionId.slice(0, 8), 5000);
    await tc.waitForVisibleText('Stale KAS session', 5000);

    // Enter on the highlighted (top, most-recent) entry. The V2 entry
    // routes through ensure-session before `session/load`.
    await tc.sendKeys('\r');

    const idPattern = kasSessionIdPatternFor(BASIC_FS_TOOLS.sessionId);
    const store = await tc.waitForStore(
      (s) => Boolean(s.sessionId) && idPattern.test(s.sessionId!),
      30_000
    );
    const targetSessionId = store.sessionId!;

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
