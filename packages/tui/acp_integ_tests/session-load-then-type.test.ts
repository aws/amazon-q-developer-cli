/**
 * Repro check for the "/chat load then type" report: after `session/load`
 * replays a history taller than the viewport (lite, chat.preserveScrollback
 * on), newly typed input must appear promptly and the replayed history must
 * not be re-emitted (duplicated) into scrollback by the overflow repaint.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import {
  mkdirSync,
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
} from '@agentclientprotocol/sdk';
import { computeWorkspaceHash } from '@kiro/agent';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'load-then-type';
const HISTORY_PARAS = 12;

function historyPara(i: number): string {
  return `HISTPARA-${i} lorem ipsum dolor sit amet consectetur`;
}

describe('session load then new input (preserveScrollback)', () => {
  let tc: AcpTestCase | null = null;
  let cwd: string | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    if (cwd) rmSync(cwd, { recursive: true, force: true });
    tc = null;
    cwd = null;
  });

  it('typed input paints promptly and history is not duplicated', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-load-type-')));
    tc = new AcpTestCase({
      testName: 'load-then-type',
      args: ['--resume'],
      cwd,
      terminalSize: { width: 80, height: 14 },
      settings: { 'chat.preserveScrollback': true },
      extraEnv: {
        KIRO_UI_MODE: 'lite',
        KIRO_LITE_ROLLOUT_ENABLED: '1',
      },
      mockKasSessionListResult: [
        {
          sessionId: SESSION_ID,
          cwd,
          title: 'Long session',
          updatedAt: new Date().toISOString(),
        },
      ],
    });
    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));
    tc.mock.on<LoadSessionRequest, LoadSessionResponse>('session/load', () => {
      // Replay a closed turn whose assistant message is far taller than
      // the 14-row viewport.
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: 'old question' },
          _meta: { kiro: { messageId: 'user-1' } },
        },
      });
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_start' } },
        },
      });
      const body = Array.from({ length: HISTORY_PARAS }, (_, i) =>
        historyPara(i)
      ).join('\n\n');
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: body },
          _meta: { kiro: { messageId: 'model-1' } },
        },
      });
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_end', stopReason: 'end_turn' } },
        },
      });
      return { modes: defaultKasModes() };
    });
    tc.mock.on('session/set_config_option', () => ({}));
    tc.mock.on('session/prompt', () => {
      // Slow response: the window between submit and first chunk is where
      // the "input not treated as new" symptom lives.
      return new Promise((resolve) => {
        setTimeout(() => {
          tc!.mock.notify('session/update', {
            sessionId: SESSION_ID,
            update: {
              sessionUpdate: 'agent_message_chunk',
              content: { type: 'text', text: 'NEWRESPONSE ack' },
            },
          });
          resolve({ stopReason: 'end_turn' });
        }, 1500);
      });
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore(
      (s) => s.sessionId === SESSION_ID && s.isInitialized,
      15_000
    );
    // History replay painted through (tail visible or in scrollback).
    await tc.waitForVisibleText(`HISTPARA-${HISTORY_PARAS - 1}`, 10_000);

    // Type new input and submit.
    await tc.sendKeys('BRANDNEWINPUT run the checks');
    await tc.sleepMs(150);
    await tc.pressEnter();

    // SYMPTOM 1 check: the input row must be PHYSICALLY painted promptly —
    // before the (deliberately slow) response arrives.
    let inputVisibleBeforeResponse = false;
    const deadline = Date.now() + 1_200;
    while (Date.now() < deadline) {
      const snap = tc.getSnapshot().join('\n');
      if (snap.includes('BRANDNEWINPUT')) {
        inputVisibleBeforeResponse = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 100));
    }

    // Let the turn finish so we can inspect the settled buffer.
    await tc.waitForVisibleText('NEWRESPONSE', 10_000);
    await new Promise((r) => setTimeout(r, 500));

    const all = tc.getSnapshot();
    const counts = new Map<string, number>();
    for (let i = 0; i < HISTORY_PARAS; i++) {
      const key = `HISTPARA-${i} `;
      counts.set(key, all.filter((l) => l.includes(key)).length);
    }
    const inputCount = all.filter((l) => l.includes('BRANDNEWINPUT')).length;

    console.log('input visible before response:', inputVisibleBeforeResponse);
    console.log(
      'history para counts:',
      [...counts.entries()].map(([k, v]) => `${k}=${v}`).join(' ')
    );
    console.log('input row count:', inputCount);
    // SYMPTOM 1: input must paint before the response.
    expect(inputVisibleBeforeResponse).toBe(true);
    // SYMPTOM 2: no history paragraph duplicated in the buffer.
    for (const [key, n] of counts) {
      expect(`${key}:${n}`).toBe(`${key}:1`);
    }
    expect(inputCount).toBe(1);
  }, 60_000);

  it('in-session /chat load then typed input: prompt paints, history not duplicated', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-load-type-insess-')));
    // In-session picker selection shells out to `chat _ ensure-session`,
    // which requires the KAS session to exist on disk under
    // `$KIRO_HOME/sessions/<workspaceHash(cwd)>/<id>/session.json` before it
    // hands the id to `session/load` (our mock). Overriding KIRO_HOME also
    // bypasses the harness settings sandbox, so write settings here too.
    const kiroHome = realpathSync(
      mkdtempSync(join(tmpdir(), 'kiro-load-type-home-'))
    );
    const bucket = computeWorkspaceHash([cwd]);
    const sessionDir = join(kiroHome, 'sessions', bucket, SESSION_ID);
    mkdirSync(sessionDir, { recursive: true });
    writeFileSync(join(sessionDir, 'session.json'), '{}');
    const settingsPath = join(kiroHome, 'settings', 'cli.json');
    mkdirSync(join(kiroHome, 'settings'), { recursive: true });
    writeFileSync(
      settingsPath,
      JSON.stringify({ 'chat.preserveScrollback': true })
    );
    tc = new AcpTestCase({
      testName: 'load-then-type-insession',
      cwd,
      terminalSize: { width: 80, height: 14 },
      extraEnv: {
        KIRO_HOME: kiroHome,
        KIRO_UI_MODE: 'lite',
        KIRO_LITE_ROLLOUT_ENABLED: '1',
      },
      mockKasSessionListResult: [
        {
          sessionId: SESSION_ID,
          cwd,
          title: 'Long session',
          updatedAt: new Date().toISOString(),
        },
      ],
    });
    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));
    tc.mock.on('session/new', () => ({
      sessionId: 'fresh-session',
      modes: defaultKasModes(),
    }));
    tc.mock.on<LoadSessionRequest, LoadSessionResponse>('session/load', () => {
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'user_message_chunk',
          content: { type: 'text', text: 'old question' },
          _meta: { kiro: { messageId: 'user-1' } },
        },
      });
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_start' } },
        },
      });
      const body = Array.from({ length: HISTORY_PARAS }, (_, i) =>
        historyPara(i)
      ).join('\n\n');
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: body },
          _meta: { kiro: { messageId: 'model-1' } },
        },
      });
      tc!.mock.notify('session/update', {
        sessionId: SESSION_ID,
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_end', stopReason: 'end_turn' } },
        },
      });
      return { modes: defaultKasModes() };
    });
    tc.mock.on('session/set_config_option', () => ({}));
    tc.mock.on('session/prompt', () => {
      return new Promise((resolve) => {
        setTimeout(() => {
          tc!.mock.notify('session/update', {
            sessionId: SESSION_ID,
            update: {
              sessionUpdate: 'agent_message_chunk',
              content: { type: 'text', text: 'NEWRESPONSE ack' },
            },
          });
          resolve({ stopReason: 'end_turn' });
        }, 1500);
      });
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((s) => s.isInitialized, 15_000);

    // In-session load via the /chat picker (the user's actual flow).
    await tc.sendKeys('/chat');
    await tc.sleepMs(300);
    await tc.pressEnter();
    await tc.waitForVisibleText('Long session', 10_000);
    await tc.pressEnter(); // select the only session
    await tc.waitForStore((s) => s.sessionId === SESSION_ID, 15_000);
    await tc.waitForVisibleText(`HISTPARA-${HISTORY_PARAS - 1}`, 10_000);
    await tc.sleepMs(400);

    await tc.sendKeys('BRANDNEWINPUT run the checks');
    await tc.sleepMs(150);
    await tc.pressEnter();

    let inputVisibleBeforeResponse = false;
    const deadline = Date.now() + 1_200;
    while (Date.now() < deadline) {
      if (tc.getSnapshot().join('\n').includes('BRANDNEWINPUT')) {
        inputVisibleBeforeResponse = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 100));
    }

    await tc.waitForVisibleText('NEWRESPONSE', 10_000);
    await new Promise((r) => setTimeout(r, 500));

    const all = tc.getSnapshot();
    const counts = new Map<string, number>();
    for (let i = 0; i < HISTORY_PARAS; i++) {
      const key = `HISTPARA-${i} `;
      counts.set(key, all.filter((l) => l.includes(key)).length);
    }
    const inputCount = all.filter((l) => l.includes('BRANDNEWINPUT')).length;
    console.log(
      'in-session: input visible before response:',
      inputVisibleBeforeResponse
    );
    console.log(
      'in-session: history para counts:',
      [...counts.entries()].map(([k, v]) => `${k.trim()}=${v}`).join(' ')
    );
    console.log('in-session: input row count:', inputCount);

    expect(inputVisibleBeforeResponse).toBe(true);
    for (const [key, n] of counts) {
      expect(`${key.trim()}:${n}`).toBe(`${key.trim()}:1`);
    }
    expect(inputCount).toBe(1);
  }, 60_000);
});
