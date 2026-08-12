import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
  InitializeRequest,
  InitializeResponse,
  LoadSessionRequest,
  LoadSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'resume-live-handoff';

describe('resume history-to-live handoff', () => {
  let tc: AcpTestCase | null = null;
  let cwd: string | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    if (cwd) rmSync(cwd, { recursive: true, force: true });
    tc = null;
    cwd = null;
  });

  it('renders the live tail of a replayed open turn without user input', async () => {
    cwd = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-resume-live-')));
    tc = new AcpTestCase({
      testName: 'resume-live-handoff',
      args: ['--resume'],
      cwd,
      mockKasSessionListResult: [
        {
          sessionId: SESSION_ID,
          cwd,
          title: 'Open turn',
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
    tc.mock.on<LoadSessionRequest, LoadSessionResponse>(
      'session/load',
      (request) => {
        expect(request.sessionId).toBe(SESSION_ID);
        tc!.mock.notify('session/update', {
          sessionId: SESSION_ID,
          update: {
            sessionUpdate: 'user_message_chunk',
            content: { type: 'text', text: 'continue the task' },
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
        tc!.mock.notify('session/update', {
          sessionId: SESSION_ID,
          update: {
            sessionUpdate: 'agent_message_chunk',
            content: { type: 'text', text: 'history prefix ' },
            _meta: { kiro: { messageId: 'model-1' } },
          },
        });
        return { modes: defaultKasModes() };
      }
    );
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore(
      (state) => state.sessionId === SESSION_ID && state.isInitialized,
      15_000
    );
    await tc.waitForVisibleText('history prefix', 10_000);

    tc.mock.notify('session/update', {
      sessionId: SESSION_ID,
      update: {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'live tail' },
      },
    });
    tc.mock.notify('session/update', {
      sessionId: SESSION_ID,
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: { kind: 'turn_end', stopReason: 'end_turn' },
        },
      },
    });

    const state = await tc.waitForStore(
      (store) =>
        store.messages.some(
          (message) =>
            message.role === 'model' &&
            message.content === 'history prefix live tail'
        ),
      10_000
    );
    expect(
      state.messages.filter(
        (message) =>
          message.role === 'model' &&
          message.content === 'history prefix live tail'
      )
    ).toHaveLength(1);
    await tc.waitForVisibleText('history prefix live tail', 10_000);
  }, 40_000);
});
