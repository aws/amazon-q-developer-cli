/**
 * E2E test for `/chat load <v1.json>` against the KAS engine.
 *
 * The user passes a path to a legacy V1 `ConversationState` JSON
 * file. The TUI shells out to `chat _ import-session`, which is
 * expected to detect the V1 format, run V1 -> V2 -> KAS internally,
 * and return the minted KAS session directory. The TUI then calls
 * `session/load` with the resulting id.
 *
 * Mock + real-binary setup:
 *   - KAS ACP wire is a mock socket (no real KAS process).
 *   - `KIRO_CHAT_CLI_BIN` points at the local debug binary.
 *   - `KIRO_HOME` is a sandbox so V2 byproducts and the KAS dir
 *     land under the sandbox.
 *
 * Assertions:
 *   1. The TUI's resolved session id matches
 *      `cli_<v1_id>_<8 alphanumeric>`. The V1 conversation id is
 *      preserved as the V2 byproduct id by the V1 exporter; the
 *      V2 -> KAS converter then encodes that id into the KAS
 *      `cli_<v2_id>_<random>` shape.
 *   2. The KAS session dir exists at the expected workspace bucket
 *      with `session.json` and `messages.jsonl`.
 *   3. `session/load` is called with the converted id.
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { requireChatCliBin } from '../../src/utils/chat-cli-bin';
import {
  copyFileSync,
  existsSync,
  mkdtempSync,
  realpathSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import type { LoadSessionRequest } from '@agentclientprotocol/sdk';
import { computeWorkspaceHash } from '@kiro/agent';
import { AcpTestCase } from '../../acp_integ_tests/shared/AcpTestCase';
import { kasSessionIdPatternFor, setupAcpHandshake } from './test-helpers';

const REAL_BIN = requireChatCliBin();

const V1_FIXTURE_PATH = resolve(
  __dirname,
  '../../../../crates/chat-cli/src/cli/chat/v1_export/fixtures/basic_tool_use.json'
);

// Conversation id baked into the fixture JSON. Asserted so a
// fixture swap forces an explicit test update.
const V1_CONVERSATION_ID = '26a591e0-dd86-44ee-84e2-7c44ddda8fdb';

describe('Cross-engine import: /chat load with V1 ConversationState JSON', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string;
  let v1Path: string;

  beforeEach(() => {
    // `mkdtempSync` may return a path with symlink components on
    // macOS; use the realpath so the bucket the importer derives
    // matches the TUI's resolved cwd.
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-chat-load-v1-')));
    v1Path = join(kiroHome, 'v1.json');
    copyFileSync(V1_FIXTURE_PATH, v1Path);
  });

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    rmSync(kiroHome, { recursive: true, force: true });
  });

  it('imports a V1 ConversationState JSON into a fresh KAS session bucket', async () => {
    tc = new AcpTestCase({
      testName: 'chat-load-v1-json',
      cwd: kiroHome,
      extraEnv: {
        KIRO_CHAT_CLI_BIN: REAL_BIN,
        KIRO_HOME: kiroHome,
      },
    });
    setupAcpHandshake(tc, { bootSessionId: 'sess-boot-v1' });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys(`/chat load ${v1Path}`);
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText(`Loaded session from ${v1Path}`, 15000);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs).toHaveLength(1);
    const loadedId = (loadReqs[0]!.params as LoadSessionRequest).sessionId;

    // V1 conv id reused as V2 byproduct id; V2 -> KAS encodes it
    // into the KAS id with a random suffix.
    expect(loadedId).toMatch(kasSessionIdPatternFor(V1_CONVERSATION_ID));

    const wsHash = computeWorkspaceHash([kiroHome]);
    const kasSessionDir = join(kiroHome, 'sessions', wsHash, loadedId);
    expect(existsSync(join(kasSessionDir, 'session.json'))).toBe(true);
    expect(existsSync(join(kasSessionDir, 'messages.jsonl'))).toBe(true);
  }, 60000);
});
