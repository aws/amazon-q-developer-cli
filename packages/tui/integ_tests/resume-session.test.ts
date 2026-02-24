/**
 * Integration tests for --resume and --resume-picker CLI flags.
 *
 * These tests verify the full TS flow: CLI arg parsing → session file
 * discovery → session picker UI → session ID passed to mock ACP client.
 *
 * Session files are created in a temp directory pointed to by
 * KIRO_TEST_SESSIONS_DIR so the real ~/.kiro/sessions/cli/ is never touched.
 */

import { describe, it, expect, afterEach, beforeEach } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { realpathSync } from 'fs';

let testCase: TestCase | null = null;
let sessionsDir: string;

/**
 * Create a fake V2 session on disk.
 *
 * Writes a .json metadata file and a .jsonl event log that the TUI's
 * session discovery code (sessions.ts) can parse.
 */
function createFakeSession(opts: {
  sessionId: string;
  cwd: string;
  updatedAt: string;
  userPrompt?: string;
}) {
  const metadata = {
    session_id: opts.sessionId,
    cwd: opts.cwd,
    created_at: opts.updatedAt,
    updated_at: opts.updatedAt,
    session_state: {
      version: 'v1',
      conversation_metadata: { total_turns: 1 },
      rts_model_state: {
        conversation_id: opts.sessionId,
        model_info: null,
        context_usage_percentage: null,
      },
      permissions: { allowed_paths: [], allowed_commands: [] },
    },
  };

  fs.writeFileSync(
    path.join(sessionsDir, `${opts.sessionId}.json`),
    JSON.stringify(metadata)
  );

  // Write a minimal JSONL event log with one Prompt entry
  const prompt = opts.userPrompt ?? 'hello from test';
  const logEntry = {
    version: 'v1',
    kind: 'Prompt',
    data: {
      message_id: 'msg-1',
      content: [{ kind: 'text', data: prompt }],
    },
  };
  // Add an assistant response so msgCount > 1
  const assistantEntry = {
    version: 'v1',
    kind: 'AssistantMessage',
    data: {
      message_id: 'msg-2',
      content: [{ kind: 'text', data: 'mock response' }],
    },
  };

  fs.writeFileSync(
    path.join(sessionsDir, `${opts.sessionId}.jsonl`),
    JSON.stringify(logEntry) + '\n' + JSON.stringify(assistantEntry) + '\n'
  );
}

beforeEach(() => {
  // Create a fresh temp sessions directory for each test
  sessionsDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-test-sessions-'));
});

afterEach(async () => {
  if (testCase) {
    await testCase.cleanup();
    testCase = null;
  }
  // Clean up temp sessions dir
  try {
    fs.rmSync(sessionsDir, { recursive: true });
  } catch {
    /* ignore */
  }
});

describe('--resume', () => {
  it('resumes the most recent session for cwd', async () => {
    // Use realpath to match the canonicalize() in sessions.ts
    const cwd = realpathSync(process.cwd());

    // Create two sessions — the newer one should be picked
    createFakeSession({
      sessionId: 'old-session-aaa',
      cwd,
      updatedAt: '2025-01-01T00:00:00Z',
      userPrompt: 'old prompt',
    });
    createFakeSession({
      sessionId: 'new-session-bbb',
      cwd,
      updatedAt: '2026-02-20T12:00:00Z',
      userPrompt: 'newest prompt',
    });

    testCase = await TestCase.builder()
      .withTestName('resume-most-recent')
      .withArgs(['--resume'])
      .withEnv({ KIRO_TEST_SESSIONS_DIR: sessionsDir })
      .withTimeout(15000)
      .launch();

    // Wait for the TUI to initialize
    await testCase.waitForVisibleText('ask a question', 10000);

    // Verify the store picked up the correct session ID
    const store = await testCase.getStore();
    expect(store.sessionId).toBe('new-session-bbb');

    // Exit cleanly
    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 20000);

  it('starts new session when no sessions exist for cwd', async () => {
    // sessionsDir is empty — no sessions to resume

    testCase = await TestCase.builder()
      .withTestName('resume-no-sessions')
      .withArgs(['--resume'])
      .withEnv({ KIRO_TEST_SESSIONS_DIR: sessionsDir })
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // Should fall back to a new session (mock returns 'mock-session-id')
    const store = await testCase.getStore();
    expect(store.sessionId).toBe('mock-session-id');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 20000);

  it('ignores sessions from a different cwd', async () => {
    // Create a session for a different directory
    createFakeSession({
      sessionId: 'other-dir-session',
      cwd: '/some/other/directory',
      updatedAt: '2026-02-20T12:00:00Z',
      userPrompt: 'wrong dir',
    });

    testCase = await TestCase.builder()
      .withTestName('resume-wrong-cwd')
      .withArgs(['--resume'])
      .withEnv({ KIRO_TEST_SESSIONS_DIR: sessionsDir })
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question', 10000);

    // No matching session → falls back to new session
    const store = await testCase.getStore();
    expect(store.sessionId).toBe('mock-session-id');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 20000);
});

describe('--resume-picker', () => {
  it('shows picker UI and selects session with Enter', async () => {
    const cwd = realpathSync(process.cwd());

    createFakeSession({
      sessionId: 'picker-session-1',
      cwd,
      updatedAt: '2026-02-20T10:00:00Z',
      userPrompt: 'first conversation',
    });
    createFakeSession({
      sessionId: 'picker-session-2',
      cwd,
      updatedAt: '2026-02-20T12:00:00Z',
      userPrompt: 'second conversation',
    });

    testCase = await TestCase.builder()
      .withTestName('resume-picker-select')
      .withArgs(['--resume-picker'])
      .withEnv({ KIRO_TEST_SESSIONS_DIR: sessionsDir })
      .withTimeout(15000)
      .launchWithoutWaiting();

    // The picker runs before Ink — interact with it in the PTY
    await testCase.waitForVisibleText('Select a chat session', 10000);

    // Verify both sessions appear in the picker
    const snapshot = testCase.getSnapshot().join('\n');
    expect(snapshot).toContain('second conversation');
    expect(snapshot).toContain('first conversation');

    // Press Enter to select the first (most recent) session
    await testCase.pressEnter();

    // Now wait for IPC to connect (Ink starts after picker completes)
    await testCase.waitForReady();
    await testCase.waitForVisibleText('ask a question', 10000);

    const store = await testCase.getStore();
    expect(store.sessionId).toBe('picker-session-2');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 20000);

  it('picker allows arrow-down to select second session', async () => {
    const cwd = realpathSync(process.cwd());

    createFakeSession({
      sessionId: 'arrow-session-1',
      cwd,
      updatedAt: '2026-02-20T10:00:00Z',
      userPrompt: 'older session',
    });
    createFakeSession({
      sessionId: 'arrow-session-2',
      cwd,
      updatedAt: '2026-02-20T12:00:00Z',
      userPrompt: 'newer session',
    });

    testCase = await TestCase.builder()
      .withTestName('resume-picker-arrow')
      .withArgs(['--resume-picker'])
      .withEnv({ KIRO_TEST_SESSIONS_DIR: sessionsDir })
      .withTimeout(15000)
      .launchWithoutWaiting();

    await testCase.waitForVisibleText('Select a chat session', 10000);

    // Arrow down to select the second (older) session
    await testCase.sendKeys('\x1b[B'); // Down arrow
    await testCase.sleepMs(200);
    await testCase.pressEnter();

    await testCase.waitForReady();
    await testCase.waitForVisibleText('ask a question', 10000);

    const store = await testCase.getStore();
    expect(store.sessionId).toBe('arrow-session-1');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 20000);

  it('picker cancels with Escape and starts new session', async () => {
    const cwd = realpathSync(process.cwd());

    createFakeSession({
      sessionId: 'escape-session',
      cwd,
      updatedAt: '2026-02-20T12:00:00Z',
      userPrompt: 'some conversation',
    });

    testCase = await TestCase.builder()
      .withTestName('resume-picker-escape')
      .withArgs(['--resume-picker'])
      .withEnv({ KIRO_TEST_SESSIONS_DIR: sessionsDir })
      .withTimeout(15000)
      .launchWithoutWaiting();

    await testCase.waitForVisibleText('Select a chat session', 10000);

    // Press Escape to cancel
    await testCase.pressEscape();

    await testCase.waitForReady();
    await testCase.waitForVisibleText('ask a question', 10000);

    const store = await testCase.getStore();
    expect(store.sessionId).toBe('mock-session-id');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 20000);

  it('picker with no sessions starts new session', async () => {
    // Empty sessions dir

    testCase = await TestCase.builder()
      .withTestName('resume-picker-empty')
      .withArgs(['--resume-picker'])
      .withEnv({ KIRO_TEST_SESSIONS_DIR: sessionsDir })
      .withTimeout(15000)
      .launch();

    // No sessions → picker skipped, goes straight to TUI
    await testCase.waitForVisibleText('ask a question', 10000);

    const store = await testCase.getStore();
    expect(store.sessionId).toBe('mock-session-id');

    await testCase.pressCtrlCTwice();
    const exitCode = await testCase.expectExit();
    expect(exitCode).toBe(0);
  }, 20000);
});
