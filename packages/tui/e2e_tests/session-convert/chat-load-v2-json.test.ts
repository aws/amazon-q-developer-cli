/**
 * E2E test for `/chat load <path>` with a `kiro-session-export-v1`
 * JSON file produced from a V2 session.
 *
 * Drives the real TUI via `AcpTestCase`. The slash command handler
 * shells out to the real `chat _ import-session`, which detects the
 * V2 portable export envelope, runs the V2 -> KAS converter, and
 * writes a fresh KAS session under
 * `<kiroHome>/sessions/<workspaceHash>/cli_<v2_id>_<random>/`. The
 * TUI then issues `session/load` with the new id against a mock KAS
 * ACP wire.
 *
 * Asserts:
 *   - `session/load` is called with an id matching
 *     `cli_<v2_id>_<8 alphanumeric>`. The random suffix prevents
 *     two imports of the same V2 source from colliding.
 *   - The KAS session lands in the bucket of the TUI's launch cwd,
 *     not the source V2 cwd recorded in the export envelope.
 *   - `session.json` and `messages.jsonl` exist on disk under that
 *     bucket and the converted log carries the V2 user prompt.
 *   - A second `/chat load` of the same JSON file produces a NEW
 *     KAS session id - cross-engine import is non-idempotent.
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { requireChatCliBin } from '../../src/test-utils/chat-cli-bin';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { LoadSessionRequest } from '@agentclientprotocol/sdk';
import { computeWorkspaceHash } from '@kiro/agent';
import { AcpTestCase } from '../../src/test-utils/acp-mock/AcpTestCase';
import { kasSessionIdPatternFor, setupAcpHandshake } from './test-helpers';
import { BASIC_FS_TOOLS } from './v2_fixtures';

const REAL_BIN = requireChatCliBin();

/**
 * Build a `kiro-session-export-v1` JSON payload from the V2 fixture
 * on disk. The envelope shape is `{format, metadata, log_entries}`
 * - the same shape V2's `/chat save` writes for portable export.
 */
function buildKiroV1Json(fixtureDir: string): string {
  const metadata = JSON.parse(
    readFileSync(join(fixtureDir, 'session.json'), 'utf8')
  );
  const logText = readFileSync(join(fixtureDir, 'messages.jsonl'), 'utf8');
  const log_entries = logText
    .split('\n')
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line));
  return JSON.stringify({
    format: 'kiro-session-export-v1',
    metadata,
    log_entries,
  });
}

describe('Cross-engine import: /chat load with kiro-session-export-v1 JSON', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string;
  let exportPath: string;

  beforeEach(() => {
    // `mkdtempSync` may return a path with symlink components on macOS
    // (e.g. `/var/folders` -> `/private/var/folders`). The TUI's
    // `process.cwd()` resolves symlinks, so use the realpath to match
    // the bucket the importer will produce.
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-chat-load-v2-')));
    exportPath = join(kiroHome, 'export.json');
    writeFileSync(exportPath, buildKiroV1Json(BASIC_FS_TOOLS.fixtureDir));
  });

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    rmSync(kiroHome, { recursive: true, force: true });
  });

  it('imports a V2 portable export JSON into a fresh KAS session bucket', async () => {
    tc = new AcpTestCase({
      testName: 'chat-load-v2-json',
      // Drive the TUI from the sandbox tempdir. The importer buckets
      // by `process.cwd()` of the TUI process, so this controls the
      // workspace-hash directory the imported session lands under.
      cwd: kiroHome,
      extraEnv: {
        KIRO_CHAT_CLI_BIN: REAL_BIN,
        KIRO_HOME: kiroHome,
      },
    });
    setupAcpHandshake(tc, { bootSessionId: 'sess-boot-1' });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys(`/chat load ${exportPath}`);
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText(`Loaded session from ${exportPath}`, 15000);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs).toHaveLength(1);
    const loadedId = (loadReqs[0]!.params as LoadSessionRequest).sessionId;

    // The new KAS id encodes the V2 source id and a fresh random
    // suffix that prevents collisions across repeated imports.
    const idPattern = kasSessionIdPatternFor(BASIC_FS_TOOLS.sessionId);
    expect(loadedId).toMatch(idPattern);

    // Bucket is derived from the TUI's launch cwd, not the source V2
    // session's recorded `cwd`. The source `cwd` is captured in the
    // fixture's session.json but is irrelevant on cross-engine import.
    const wsHash = computeWorkspaceHash([kiroHome]);
    const kasSessionDir = join(kiroHome, 'sessions', wsHash, loadedId);
    expect(existsSync(join(kasSessionDir, 'session.json'))).toBe(true);
    expect(existsSync(join(kasSessionDir, 'messages.jsonl'))).toBe(true);

    // Converted log carries the original V2 user prompt id verbatim.
    const messagesJsonl = readFileSync(
      join(kasSessionDir, 'messages.jsonl'),
      'utf8'
    );
    const userIds = messagesJsonl
      .split('\n')
      .filter((l) => l.length > 0)
      .map((l) => JSON.parse(l) as { id: string; payload: { type: string } })
      .filter((m) => m.payload.type === 'user')
      .map((m) => m.id);
    expect(userIds).toContain(BASIC_FS_TOOLS.userPromptId);
  }, 60000);

  it('produces a fresh KAS session id on every import of the same JSON', async () => {
    tc = new AcpTestCase({
      testName: 'chat-load-v2-json-non-idempotent',
      cwd: kiroHome,
      extraEnv: {
        KIRO_CHAT_CLI_BIN: REAL_BIN,
        KIRO_HOME: kiroHome,
      },
    });
    setupAcpHandshake(tc, { bootSessionId: 'sess-boot-2' });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys(`/chat load ${exportPath}`);
    await tc.sleepMs(300);
    await tc.sendKeys('\r');
    await tc.waitForVisibleText(`Loaded session from ${exportPath}`, 15000);
    await tc.sleepMs(500);

    await tc.sendKeys(`/chat load ${exportPath}`);
    await tc.sleepMs(300);
    await tc.sendKeys('\r');
    await tc.sleepMs(2000);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs.length).toBeGreaterThanOrEqual(2);
    const ids = loadReqs.map((r) => (r.params as LoadSessionRequest).sessionId);
    const idPattern = kasSessionIdPatternFor(BASIC_FS_TOOLS.sessionId);
    for (const id of ids) {
      expect(id).toMatch(idPattern);
    }
    // Every import mints a distinct KAS session id - cross-engine
    // conversion is non-idempotent at the id level so the same source
    // can be imported any number of times without collision.
    expect(new Set(ids).size).toBe(ids.length);
  }, 90000);
});
