/**
 * ACP-integ tests for `--resume` (most-recent winner across engines).
 *
 * Drives the TUI with a stubbed `chat_cli` binary that emits a
 * canned `--list-sessions` envelope and (for the cross-engine
 * scenario) a canned `chat _ ensure-session` response. Active
 * engine is KAS by default in `AcpTestCase`. The mock ACP wire
 * registers `session/new` and `session/load` so each scenario
 * asserts which one the TUI invoked.
 *
 * Scenarios covered:
 *   - empty listing -> stderr "No saved sessions" + boots fresh
 *     via `session/new`.
 *   - listing call fails -> same stderr + same boot path.
 *   - listing has a native KAS winner -> `session/load` with the
 *     winner's id and NO `ensure-session` passthrough.
 *   - listing has a cross-engine V2 winner -> ensure-session
 *     converts to a KAS id, `session/load` runs with that id.
 *
 * Cross-engine `--resume` against the real Rust merge surface is
 * covered separately by `e2e_tests/session-convert/resume-cross-engine.test.ts`.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import {
  chmodSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import type {
  InitializeRequest,
  InitializeResponse,
  LoadSessionRequest,
  LoadSessionResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';

import { AcpTestCase } from './shared/AcpTestCase';

interface ScriptedBinary {
  binPath: string;
  cleanup: () => void;
}

interface ScriptedBinaryOptions {
  /**
   * JSON returned for `chat --list-sessions ...`. Must match the
   * `[{cwd, sessions: [...]}]` envelope `parseListing` expects.
   */
  listSessionsJson: string;
  /** Exit code for the `--list-sessions` branch. Defaults to 0. */
  listSessionsExitCode?: number;
  /**
   * When true, `chat _ ensure-session --source-session-id <id> ...`
   * passes the id through unchanged: emits
   * `{kind: "ensureSession", data: {sessionId: "<id>"}}` and exits 0.
   */
  ensureSessionPassthrough?: boolean;
}

/**
 * Writes a tiny shell stub that branches on argv. Only the two
 * subcommands `--resume` exercises (`--list-sessions` and
 * `ensure-session`) need real responses; everything else exits 0
 * with no output so unrelated probes don't break the harness.
 */
function writeResumeStub(opts: ScriptedBinaryOptions): ScriptedBinary {
  const dir = mkdtempSync(join(tmpdir(), 'kiro-resume-stub-'));
  const binPath = join(dir, 'chat_cli');
  const listingPath = join(dir, 'list-sessions.json');
  writeFileSync(listingPath, opts.listSessionsJson);
  const listExit = opts.listSessionsExitCode ?? 0;
  const passthrough = opts.ensureSessionPassthrough === true;

  const script =
    `#!/usr/bin/env bash\n` +
    `found_list=0\n` +
    `found_ensure=0\n` +
    `ssid=""\n` +
    `while [[ $# -gt 0 ]]; do\n` +
    `  case "$1" in\n` +
    `    "--list-sessions") found_list=1 ;;\n` +
    `    "ensure-session") found_ensure=1 ;;\n` +
    `    "--source-session-id") ssid="$2"; shift ;;\n` +
    `  esac\n` +
    `  shift\n` +
    `done\n` +
    `if [[ $found_list -eq 1 ]]; then\n` +
    `  cat "${listingPath}"\n` +
    `  exit ${listExit}\n` +
    `fi\n` +
    (passthrough
      ? `if [[ $found_ensure -eq 1 && -n "$ssid" ]]; then\n` +
        `  printf '{"kind":"ensureSession","data":{"sessionId":"%s"}}\\n' "$ssid"\n` +
        `  exit 0\n` +
        `fi\n`
      : '') +
    `exit 0\n`;

  writeFileSync(binPath, script);
  chmodSync(binPath, 0o755);
  return {
    binPath,
    cleanup: () => rmSync(dir, { recursive: true, force: true }),
  };
}

function listingEnvelope(
  cwd: string,
  sessions: Array<{
    sessionId: string;
    source: 'classic' | 'v2' | 'v3';
    title: string;
    updatedAt: string;
  }>
): string {
  return JSON.stringify([{ cwd, sessions }]);
}

function setupHandshake(
  tc: AcpTestCase,
  newSessionId: string
): { newCalls: number; loadCalls: LoadSessionRequest[] } {
  const tracker = { newCalls: 0, loadCalls: [] as LoadSessionRequest[] };
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => {
    tracker.newCalls += 1;
    return {
      sessionId: newSessionId,
      modes: {
        currentModeId: 'vibe',
        availableModes: [{ id: 'vibe', name: 'Default' }],
      },
    };
  });
  tc.mock.on<LoadSessionRequest, LoadSessionResponse>(
    'session/load',
    (params) => {
      tracker.loadCalls.push(params);
      return {
        modes: {
          currentModeId: 'vibe',
          availableModes: [{ id: 'vibe', name: 'Default' }],
        },
      };
    }
  );
  tc.mock.on('session/set_config_option', () => ({}));
  return tracker;
}

describe('--resume scenarios', () => {
  let tc: AcpTestCase | null = null;
  let stub: ScriptedBinary | null = null;
  let cwd: string;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    if (stub) {
      stub.cleanup();
      stub = null;
    }
    if (cwd) {
      rmSync(cwd, { recursive: true, force: true });
    }
  });

  it('boots a fresh session and warns when no saved sessions exist for the cwd', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-resume-empty-')));
    stub = writeResumeStub({ listSessionsJson: listingEnvelope(cwd, []) });

    tc = new AcpTestCase({
      testName: 'resume-empty-listing',
      args: ['--resume'],
      cwd,
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    const tracker = setupHandshake(tc, 'sess-fresh-1');

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('No saved sessions found', 10000);

    const store = await tc.waitForStore(
      (s) => s.sessionId === 'sess-fresh-1',
      10000
    );
    expect(store.sessionId).toBe('sess-fresh-1');
    expect(tracker.newCalls).toBe(1);
    expect(tracker.loadCalls).toHaveLength(0);
  }, 30000);

  it('boots a fresh session when --list-sessions itself fails', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-resume-list-fail-')));
    stub = writeResumeStub({
      listSessionsJson: 'not a valid envelope',
      listSessionsExitCode: 1,
    });

    tc = new AcpTestCase({
      testName: 'resume-list-failure',
      args: ['--resume'],
      cwd,
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    const tracker = setupHandshake(tc, 'sess-fresh-after-fail');

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('No saved sessions found', 10000);

    const store = await tc.waitForStore(
      (s) => s.sessionId === 'sess-fresh-after-fail',
      10000
    );
    expect(store.sessionId).toBe('sess-fresh-after-fail');
    expect(tracker.newCalls).toBe(1);
    expect(tracker.loadCalls).toHaveLength(0);
  }, 30000);

  it('loads the most-recent native KAS winner without going through ensure-session', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-resume-kas-native-')));
    const winnerId = 'sess-native-kas-winner';
    stub = writeResumeStub({
      listSessionsJson: listingEnvelope(cwd, [
        {
          sessionId: winnerId,
          source: 'v3',
          title: 'Recent KAS work',
          updatedAt: '2026-06-01T12:00:00.000Z',
        },
        {
          sessionId: 'sess-older',
          source: 'v3',
          title: 'Older KAS work',
          updatedAt: '2026-05-01T12:00:00.000Z',
        },
      ]),
      // No passthrough: a native KAS winner must NOT shell out to
      // ensure-session. If the TUI did, this stub would emit nothing
      // and the call would fail.
    });

    tc = new AcpTestCase({
      testName: 'resume-native-kas-winner',
      args: ['--resume'],
      cwd,
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    const tracker = setupHandshake(tc, 'unused-fresh-id');

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((s) => s.sessionId === winnerId, 15000);

    expect(tracker.loadCalls).toHaveLength(1);
    expect(tracker.loadCalls[0]!.sessionId).toBe(winnerId);
    expect(tracker.newCalls).toBe(0);
  }, 30000);

  it('routes a cross-engine V2 winner through ensure-session before session/load', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-resume-cross-')));
    const v2Id = '11111111-2222-3333-4444-555555555555';
    stub = writeResumeStub({
      listSessionsJson: listingEnvelope(cwd, [
        {
          sessionId: v2Id,
          source: 'v2',
          title: 'V2 session',
          updatedAt: '2026-06-01T12:00:00.000Z',
        },
      ]),
      ensureSessionPassthrough: true,
    });

    tc = new AcpTestCase({
      testName: 'resume-cross-engine-stub',
      args: ['--resume'],
      cwd,
      extraEnv: { KIRO_CHAT_CLI_BIN: stub.binPath },
    });
    const tracker = setupHandshake(tc, 'unused-fresh-id');

    await tc.launch();
    await tc.mock.awaitConnection();
    // Passthrough echoes the source id back, so session/load receives
    // exactly that id.
    await tc.waitForStore((s) => s.sessionId === v2Id, 15000);

    expect(tracker.loadCalls).toHaveLength(1);
    expect(tracker.loadCalls[0]!.sessionId).toBe(v2Id);
    expect(tracker.newCalls).toBe(0);
  }, 30000);
});
