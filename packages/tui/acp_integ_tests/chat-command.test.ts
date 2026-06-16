import { describe, it, expect, afterEach } from 'bun:test';
import { requireChatCliBin } from '../src/utils/chat-cli-bin';
import {
  chmodSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { platform, tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

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
    modes: defaultKasModes(),
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

interface ScriptedBinary {
  binPath: string;
  cleanup: () => void;
}

interface ScriptedBinaryOptions {
  /** JSON emitted on stdout for any non-special subcommand. */
  defaultJson: string;
  /** Exit status for the default branch. */
  defaultExitCode?: number;
  /**
   * When true, requests of the form
   * `chat _ ensure-session --source-session-id <id> ...` short-circuit
   * to `{"kind":"ensureSession","data":{"sessionId":"<id>"}}` with exit 0.
   * Used by tests that need the import / picker flow's auto-format
   * probe to succeed without standing up real KAS storage.
   */
  ensureSessionPassthrough?: boolean;
  /**
   * JSON emitted on stdout when the binary is invoked with
   * `chat --list-sessions ...`. Lets a single stub serve both the
   * picker's listing fetch and the per-selection ensure-session
   * round-trip.
   */
  listSessionsJson?: string;
}

/**
 * Writes a tiny POSIX shell script (or `.cmd` on Windows) that
 * emulates the JSON-on-stdout contract of the `kiro-cli chat _` family.
 *
 * The chat-command integ suite uses this stub via `KIRO_CHAT_CLI_BIN`
 * so it can drive the TUI's spawn + parse + alert chain without
 * depending on a freshly built `target/debug/chat_cli`.
 *
 * The real binary's contract is locked in by
 * `e2e_tests/session-archive.test.ts`; the argv shape the TUI builds
 * is locked in by the unit tests for `session-archive-cli` and
 * `ensure-session-cli`. The integ-level concern is purely the TUI
 * side of the seam.
 */
function writeScriptedBinary(
  optsOrJson: ScriptedBinaryOptions | string,
  exitCodeArg: number = 0
): ScriptedBinary {
  const opts: ScriptedBinaryOptions =
    typeof optsOrJson === 'string'
      ? { defaultJson: optsOrJson, defaultExitCode: exitCodeArg }
      : optsOrJson;
  const defaultJson = opts.defaultJson;
  const defaultExit = opts.defaultExitCode ?? 0;
  const ensurePassthrough = opts.ensureSessionPassthrough === true;
  const listSessionsJson = opts.listSessionsJson;

  const dir = mkdtempSync(join(tmpdir(), 'kiro-acp-integ-stub-bin-'));
  const isWindows = platform() === 'win32';

  let script: string;
  let binPath: string;

  if (isWindows) {
    binPath = join(dir, 'chat_cli.cmd');
    const jsonPath = join(dir, 'output.json');
    writeFileSync(jsonPath, defaultJson);
    const listingPath = listSessionsJson
      ? join(dir, 'list-sessions.json')
      : null;
    if (listingPath) writeFileSync(listingPath, listSessionsJson!);
    // cmd's `echo` doesn't honor backslash escaping, and metacharacter
    // escapes mangle the JSON. Write the JSON byte-for-byte to a sibling
    // file and `type` it. The branched script walks %* looking for
    // `ensure-session` / `--list-sessions` / the matching
    // `--source-session-id` value.
    if (ensurePassthrough || listingPath) {
      script =
        `@echo off\r\n` +
        `set "FOUND_ENSURE="\r\n` +
        `set "FOUND_LIST="\r\n` +
        `set "SSID="\r\n` +
        `:loop\r\n` +
        `if "%~1"=="" goto :default\r\n` +
        `if "%~1"=="ensure-session" set "FOUND_ENSURE=1"\r\n` +
        `if "%~1"=="--list-sessions" set "FOUND_LIST=1"\r\n` +
        `if "%~1"=="--source-session-id" set "SSID=%~2"\r\n` +
        `shift\r\n` +
        `goto :loop\r\n` +
        `:default\r\n` +
        (listingPath
          ? `if defined FOUND_LIST (\r\n` +
            `  type "%~dp0list-sessions.json"\r\n` +
            `  exit /b 0\r\n` +
            `)\r\n`
          : '') +
        (ensurePassthrough
          ? `if defined FOUND_ENSURE if defined SSID (\r\n` +
            `  echo {"kind":"ensureSession","data":{"sessionId":"%SSID%"}}\r\n` +
            `  exit /b 0\r\n` +
            `)\r\n`
          : '') +
        `type "%~dp0output.json"\r\n` +
        `exit /b ${defaultExit}\r\n`;
    } else {
      script =
        `@echo off\r\n` +
        `type "%~dp0output.json"\r\n` +
        `exit /b ${defaultExit}\r\n`;
    }
  } else {
    binPath = join(dir, 'chat_cli');
    const listingPath = listSessionsJson
      ? join(dir, 'list-sessions.json')
      : null;
    if (listingPath) writeFileSync(listingPath, listSessionsJson!);
    if (ensurePassthrough || listingPath) {
      script =
        `#!/usr/bin/env bash\n` +
        `found_ensure=0\n` +
        `found_list=0\n` +
        `ssid=""\n` +
        `while [[ $# -gt 0 ]]; do\n` +
        `  case "$1" in\n` +
        `    "ensure-session") found_ensure=1 ;;\n` +
        `    "--source-session-id") ssid="$2"; shift ;;\n` +
        `    "--list-sessions") found_list=1 ;;\n` +
        `  esac\n` +
        `  shift\n` +
        `done\n` +
        (listingPath
          ? `if [[ $found_list -eq 1 ]]; then\n` +
            `  cat "${listingPath}"\n` +
            `  exit 0\n` +
            `fi\n`
          : '') +
        (ensurePassthrough
          ? `if [[ $found_ensure -eq 1 && -n "$ssid" ]]; then\n` +
            `  printf '{"kind":"ensureSession","data":{"sessionId":"%s"}}\\n' "$ssid"\n` +
            `  exit 0\n` +
            `fi\n`
          : '') +
        `cat <<'__KIRO_STUB_EOF__'\n` +
        `${defaultJson}\n` +
        `__KIRO_STUB_EOF__\n` +
        `exit ${defaultExit}\n`;
    } else {
      script =
        `#!/usr/bin/env bash\n` +
        `cat <<'__KIRO_STUB_EOF__'\n` +
        `${defaultJson}\n` +
        `__KIRO_STUB_EOF__\n` +
        `exit ${defaultExit}\n`;
    }
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
  let kiroHome: string | null = null;

  // Path to the built chat_cli binary. Tests that drive --list-sessions
  // through the real Rust merge surface use REAL_BIN; tests that only
  // need the JSON-on-stdout import/export contract use a stub. The
  // tui-integ workflow builds chat_cli before running this suite, so
  // resolution at describe time is the canonical contract.
  const REAL_BIN = requireChatCliBin();

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    if (stub) {
      stub.cleanup();
      stub = null;
    }
    if (kiroHome) {
      rmSync(kiroHome, { recursive: true, force: true });
      kiroHome = null;
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

  it("typing '/chat' + Enter renders sessions returned by --list-sessions", async () => {
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-chat-list-')));
    tc = new AcpTestCase({
      testName: 'chat-command-session-picker',
      cwd: kiroHome,
      mockKasSessionListResult: [
        {
          sessionId: 'sess-aaaa-1111',
          cwd: kiroHome,
          title: 'Refactor the dispatcher',
          updatedAt: new Date(Date.now() - 60_000).toISOString(),
        },
        {
          sessionId: 'sess-bbbb-2222',
          cwd: kiroHome,
          title: 'Fix login flow bug',
          updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
        },
      ],
      extraEnv: {
        KIRO_CHAT_CLI_BIN: REAL_BIN,
        KIRO_HOME: kiroHome,
      },
    });
    setupHandshake(tc);

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

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('Refactor the dispatcher');
    expect(snapshot).toContain('Fix login flow bug');
    expect(snapshot).toContain('sess-aaa');
    expect(snapshot).toContain('sess-bbb');
  }, 30000);

  it("'/chat' picker collapses raw newlines in multi-line session titles", async () => {
    // KAS seeds session titles from the first user prompt verbatim.
    // A multi-line first prompt would otherwise persist real `\n`s in
    // the title, and the autocomplete picker renders each option on
    // one row - so embedded newlines mangle the option layout. The
    // title must render as a single visual line in the picker.
    kiroHome = realpathSync(
      mkdtempSync(join(tmpdir(), 'kiro-chat-multiline-'))
    );
    tc = new AcpTestCase({
      testName: 'chat-command-multiline-title',
      cwd: kiroHome,
      mockKasSessionListResult: [
        {
          sessionId: 'sess-multiline-aaaa',
          cwd: kiroHome,
          title: 'fix the bug\nwhere foo crashes\nwhen bar is null',
          updatedAt: new Date(Date.now() - 60_000).toISOString(),
        },
      ],
      extraEnv: {
        KIRO_CHAT_CLI_BIN: REAL_BIN,
        KIRO_HOME: kiroHome,
      },
    });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    await tc.sendKeys('/chat');
    await tc.sleepMs(300);
    await tc.waitForVisibleText('/chat', 5000);
    await tc.sendKeys('\r');

    // The picker must render the title text. Wait on the first
    // segment - if sanitization is missing the picker may still
    // surface the leading line, which is exactly the regression
    // we want to catch with the layout assertions below.
    await tc.waitForVisibleText('fix the bug', 5000);

    const lines = tc.getSnapshotFormatted().split('\n');
    // Find the row carrying the picker option's truncated id
    // suffix. With sanitization that row also contains the title's
    // later segments (collapsed to one line); without sanitization
    // the row only carries the trailing segment because real `\n`s
    // pushed the leading text up.
    const optionRow = lines.find((l) => l.includes('sess-mul'));
    expect(optionRow).toBeDefined();
    expect(optionRow!).toContain('fix the bug');
    expect(optionRow!).toContain('when bar is null');
  }, 30000);

  it("'/chat save <path>' surfaces the binary's success path on alert", async () => {
    stub = writeScriptedBinary(
      JSON.stringify({
        kind: 'exportSession',
        data: { path: '/tmp/integ-export.zip' },
      }),
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
      JSON.stringify({
        kind: 'error',
        data: { message: 'session not found' },
      }),
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
    stub = writeScriptedBinary({
      defaultJson: JSON.stringify({
        kind: 'importSession',
        data: { path: importedPath },
      }),
      defaultExitCode: 0,
      ensureSessionPassthrough: true,
    });
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
        modes: defaultKasModes(),
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
      JSON.stringify({
        kind: 'error',
        data: { message: 'archive is not a zip' },
      }),
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
    kiroHome = realpathSync(
      mkdtempSync(join(tmpdir(), 'kiro-chat-pick-load-'))
    );
    const targetSessionId = 'sess-picker-load-1234';
    const otherSessionId = 'sess-picker-other-5678';
    const listingJson = JSON.stringify([
      {
        cwd: kiroHome,
        sessions: [
          {
            sessionId: targetSessionId,
            source: 'v3',
            title: 'Pick me to load',
            updatedAt: new Date(Date.now() - 60_000).toISOString(),
          },
          {
            sessionId: otherSessionId,
            source: 'v3',
            title: 'Some other session',
            updatedAt: new Date(Date.now() - 3_600_000).toISOString(),
          },
        ],
      },
    ]);
    stub = writeScriptedBinary({
      defaultJson: '{"kind":"error","data":{"message":"unhandled subcommand"}}',
      defaultExitCode: 1,
      listSessionsJson: listingJson,
      ensureSessionPassthrough: true,
    });
    tc = new AcpTestCase({
      testName: 'chat-command-picker-replays-history',
      cwd: kiroHome,
      extraEnv: {
        KIRO_CHAT_CLI_BIN: stub.binPath,
        KIRO_HOME: kiroHome,
      },
    });
    setupHandshake(tc);

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
        modes: defaultKasModes(),
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
