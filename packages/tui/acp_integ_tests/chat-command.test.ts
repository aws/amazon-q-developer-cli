import { describe, it, expect, afterEach } from 'bun:test';
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { platform, tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';

function setupHandshake(
  tc: AcpTestCase,
  sessionId: string = 'test-session-1'
): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId,
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Vibe' }],
    },
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

interface ScriptedBinary {
  binPath: string;
  cleanup: () => void;
}

/**
 * Writes a tiny POSIX shell script that emulates the JSON-on-stdout
 * contract of `kiro-cli chat _ export-session` / `import-session`:
 * prints exactly one JSON line, exits with the matching status. Used
 * as a stub for `KIRO_CHAT_CLI_BIN` so this integ suite can exercise
 * the TUI's spawn + parse + alert chain without depending on a freshly
 * built `target/debug/chat_cli`.
 *
 * The real binary's contract is locked in by
 * `e2e_tests/session-archive.test.ts`, and the argv shape the TUI
 * builds is locked in by the unit tests for `session-archive-cli`,
 * so the integ-level concern is purely the TUI side of the seam.
 *
 * The stub ignores all args - argv assertions are not the integ
 * test's job.
 */
function writeScriptedBinary(
  jsonOutput: string,
  exitCode: number = 0
): ScriptedBinary {
  const dir = mkdtempSync(join(tmpdir(), 'kiro-acp-integ-stub-bin-'));
  const isWindows = platform() === 'win32';

  let script: string;
  let binPath: string;

  if (isWindows) {
    binPath = join(dir, 'chat_cli.cmd');
    // cmd's `echo` doesn't honor backslash escaping, and metacharacter
    // escapes mangle the JSON. Write the JSON byte-for-byte to a sibling
    // file and `type` it.
    const jsonPath = join(dir, 'output.json');
    writeFileSync(jsonPath, jsonOutput);
    // %~dp0 expands to the directory the .cmd lives in (with trailing \).
    script =
      `@echo off\r\n` +
      `type "%~dp0output.json"\r\n` +
      `exit /b ${exitCode}\r\n`;
  } else {
    binPath = join(dir, 'chat_cli');
    script =
      `#!/usr/bin/env bash\n` +
      `cat <<'__KIRO_STUB_EOF__'\n` +
      `${jsonOutput}\n` +
      `__KIRO_STUB_EOF__\n` +
      `exit ${exitCode}\n`;
  }

  writeFileSync(binPath, script);
  if (!isWindows) chmodSync(binPath, 0o755);
  return {
    binPath,
    cleanup: () => rmSync(dir, { recursive: true, force: true }),
  };
}

describe('/chat command', () => {
  let tc: AcpTestCase | null = null;
  let stub: ScriptedBinary | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    if (stub) {
      stub.cleanup();
      stub = null;
    }
  });

  it("typing '/ch' filters autocomplete to /chat", async () => {
    tc = new AcpTestCase({ testName: 'chat-command-autocomplete' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/ch');
    await tc.sleepMs(300);

    await tc.waitForVisibleText('/chat', 5000);
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('/chat');
    // Commands that don't match `/ch` should be filtered out.
    expect(snapshot).not.toContain('/agent');
    expect(snapshot).not.toContain('/help');
  }, 30000);

  it("typing '/chat' + Enter renders sessions returned by session/list", async () => {
    tc = new AcpTestCase({ testName: 'chat-command-session-picker' });
    setupHandshake(tc);

    tc.mock.on('session/list', () => ({
      sessions: [
        {
          sessionId: 'sess-aaaa-1111',
          cwd: process.cwd(),
          title: 'Refactor the dispatcher',
          updatedAt: new Date(Date.now() - 60_000).toISOString(),
        },
        {
          sessionId: 'sess-bbbb-2222',
          cwd: process.cwd(),
          title: 'Fix login flow bug',
          updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
        },
      ],
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // `/chat` is the shortest prefix that disambiguates from `/changelog`,
    // which would otherwise be highlighted alphabetically-first at `/cha`
    // and trigger the changelog panel on Enter.
    await tc.sendKeys('/chat');
    await tc.sleepMs(300);
    await tc.waitForVisibleText('/chat', 5000);

    await tc.sendKeys('\r');

    await tc.waitForVisibleText('Refactor the dispatcher', 5000);
    await tc.waitForVisibleText('Fix login flow bug', 5000);

    const listReqs = tc.mock.receivedRequests('session/list');
    expect(listReqs.length).toBeGreaterThanOrEqual(1);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('Refactor the dispatcher');
    expect(snapshot).toContain('Fix login flow bug');
    expect(snapshot).toContain('sess-aaa');
    expect(snapshot).toContain('sess-bbb');
  }, 30000);

  it("'/chat save <path>' surfaces the binary's success path on alert", async () => {
    stub = writeScriptedBinary(
      JSON.stringify({ success: true, path: '/tmp/integ-export.zip' }),
      0
    );
    tc = new AcpTestCase({
      testName: 'chat-save-success',
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    setupHandshake(tc, 'sess-active-1');

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/chat save /tmp/anywhere.zip');
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText('Saved session to /tmp/integ-export.zip', 5000);
    // Save must NOT trigger session/list - the picker should stay closed.
    expect(tc.mock.receivedRequests('session/list')).toHaveLength(0);
  }, 30000);

  it("'/chat save <path>' surfaces the binary's error path on alert", async () => {
    stub = writeScriptedBinary(
      JSON.stringify({ success: false, error: 'session not found' }),
      1
    );
    tc = new AcpTestCase({
      testName: 'chat-save-failure',
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    setupHandshake(tc, 'sess-active-1');

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/chat save /tmp/anywhere.zip');
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText('session not found', 5000);
  }, 30000);

  it("'/chat load <path>' calls session/load with basename and replays history on success", async () => {
    const importedSessionId = 'sess_imported-from-archive-1';
    const importedPath = `/sessions/abc123/${importedSessionId}`;
    stub = writeScriptedBinary(
      JSON.stringify({ success: true, path: importedPath }),
      0
    );
    // Real archive file the handler can stat; the stub binary ignores
    // argv and emits canned JSON, so contents don't matter.
    const archiveDir = mkdtempSync(join(tmpdir(), 'kiro-integ-archive-'));
    const archivePath = join(archiveDir, 'session.zip');
    writeFileSync(archivePath, 'not a real zip');
    tc = new AcpTestCase({
      testName: 'chat-load-success',
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    setupHandshake(tc, 'sess-active-1');

    tc.mock.on('session/load', async (params) => {
      const req = params as { sessionId: string };
      tc!.mock.notify('session/update', {
        sessionId: req.sessionId,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Loaded from archive' },
        },
      });
      // Drain notifications before unblocking the load response so the
      // history renders before the "Loaded session from ..." system message.
      await new Promise((r) => setTimeout(r, 100));
      return {
        modes: {
          currentModeId: 'vibe',
          availableModes: [{ id: 'vibe', name: 'Vibe' }],
        },
      };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys(`/chat load ${archivePath}`);
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText(`Loaded session from ${archivePath}`, 5000);
    await tc.waitForVisibleText('Loaded from archive', 5000);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs.length).toBe(1);
    expect((loadReqs[0]!.params as { sessionId: string }).sessionId).toBe(
      importedSessionId
    );

    rmSync(archiveDir, { recursive: true, force: true });
  }, 30000);

  it("'/chat load <path>' surfaces the binary's error path and skips session/load", async () => {
    stub = writeScriptedBinary(
      JSON.stringify({ success: false, error: 'archive is not a zip' }),
      1
    );
    const archiveDir = mkdtempSync(join(tmpdir(), 'kiro-integ-archive-'));
    const archivePath = join(archiveDir, 'bogus.zip');
    writeFileSync(archivePath, 'not a real zip');
    tc = new AcpTestCase({
      testName: 'chat-load-failure',
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    setupHandshake(tc, 'sess-active-1');

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys(`/chat load ${archivePath}`);
    await tc.sleepMs(300);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText('archive is not a zip', 5000);
    expect(tc.mock.receivedRequests('session/load')).toHaveLength(0);

    rmSync(archiveDir, { recursive: true, force: true });
  }, 30000);

  it('selecting a session from the /chat picker renders streamed history', async () => {
    tc = new AcpTestCase({
      testName: 'chat-command-picker-replays-history',
    });
    setupHandshake(tc);

    const targetSessionId = 'sess-picker-load-1234';
    const otherSessionId = 'sess-picker-other-5678';

    tc.mock.on('session/list', () => ({
      sessions: [
        {
          sessionId: targetSessionId,
          cwd: process.cwd(),
          title: 'Pick me to load',
          updatedAt: new Date(Date.now() - 60_000).toISOString(),
        },
        {
          sessionId: otherSessionId,
          cwd: process.cwd(),
          title: 'Some other session',
          updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
        },
      ],
    }));

    tc.mock.on('session/load', async (params) => {
      const req = params as { sessionId: string };
      tc!.mock.notify('session/update', {
        sessionId: req.sessionId,
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: 'Hello from history' },
        },
      });
      tc!.mock.notify('session/update', {
        sessionId: req.sessionId,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Reply from the past' },
        },
      });
      // Drain notifications before unblocking the load response.
      await new Promise((r) => setTimeout(r, 100));
      return {
        modes: {
          currentModeId: 'vibe',
          availableModes: [{ id: 'vibe', name: 'Vibe' }],
        },
      };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/chat');
    await tc.sleepMs(300);
    await tc.waitForVisibleText('/chat', 5000);
    await tc.sendKeys('\r');

    await tc.waitForVisibleText('Pick me to load', 5000);
    // The first option is highlighted by default; Enter selects it.
    await tc.sendKeys('\r');

    await tc.waitForVisibleText(`Loaded session ${targetSessionId}`, 5000);
    await tc.waitForVisibleText('Hello from history', 5000);
    await tc.waitForVisibleText('Reply from the past', 5000);

    const loadReqs = tc.mock.receivedRequests('session/load');
    expect(loadReqs.length).toBe(1);
    expect((loadReqs[0]!.params as { sessionId: string }).sessionId).toBe(
      targetSessionId
    );

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('Hello from history');
    expect(snapshot).toContain('Reply from the past');
  }, 30000);
});
