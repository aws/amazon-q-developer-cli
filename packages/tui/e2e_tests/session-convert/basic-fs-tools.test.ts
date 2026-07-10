/**
 * E2E test for the V2 -> KAS conversion of the `basic_fs_tools`
 * fixture. Asserts on the loaded `PersistedMessage[]`, NOT the raw
 * file: every other entry point (`--resume-id`, `--resume`, `/chat`,
 * `/chat load`) routes through the same converter, so pinning the
 * shape against this one fixture is enough.
 *
 * Reuses the `--resume-id` flow as the cheapest setup that drives a
 * real conversion via the real `target/debug/chat_cli`.
 */
import { describe, it, expect, afterEach, beforeEach } from 'bun:test';
import { requireChatCliBin } from '../../src/test-utils/chat-cli-bin';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { SessionPersistence } from '@kiro/agent';
import { AcpTestCase } from '../../acp_integ_tests/shared/AcpTestCase';
import { assertConvertedSession } from './assertions';
import { assertBasicFsToolsConverted } from './basic-fs-tools-assertions';
import { setupAcpHandshake } from './test-helpers';
import { BASIC_FS_TOOLS, seedV2Session } from './v2_fixtures';

const REAL_BIN = requireChatCliBin();

describe('V2 -> KAS conversion: basic_fs_tools fixture', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string;

  beforeEach(() => {
    // `mkdtempSync` may return a path with symlink components on
    // macOS; `realpath` matches the bucket the converter produces.
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-basic-fs-')));
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

  it('produces the expected user, tool_call, tool_result, and assistant payloads', async () => {
    tc = new AcpTestCase({
      testName: 'basic-fs-tools-convert',
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
    const targetSessionId = store.sessionId!;

    const persistence = new SessionPersistence();
    await persistence.initialize(join(kiroHome, 'sessions'));
    const loaded = await persistence.loadSession(targetSessionId, [kiroHome]);
    expect(loaded).not.toBeNull();
    const messages = loaded!.messages;

    assertConvertedSession(messages);
    assertBasicFsToolsConverted(messages);
  }, 60000);
});
