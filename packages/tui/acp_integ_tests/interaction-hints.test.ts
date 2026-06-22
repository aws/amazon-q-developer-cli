import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
  PromptRequest,
  PromptResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'hints-session-1',
    modes: defaultKasModes(),
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('interaction hints', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('slash menu surfaces navigate/select hints', async () => {
    tc = new AcpTestCase({ testName: 'slash-menu-hints' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(200);

    tc.mock.notify('session/update', {
      sessionId: 'hints-session-1',
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          {
            name: 'review',
            description: 'Workspace review prompt',
            _meta: { kiro: { type: 'prompt', scope: 'workspace' } },
          },
        ],
      },
    });
    await tc.sleepMs(300);

    await tc.sendKeys('/');
    await tc.sleepMs(300);

    await tc.waitForVisibleText('to navigate', 5000);
    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('to navigate');
    expect(snap).toContain('to select');
  });

  it('approval panel surfaces navigate/select/edit hints', async () => {
    tc = new AcpTestCase({ testName: 'approval-hints' });
    setupHandshake(tc);

    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      tc!.mock.notify('session/update', {
        sessionId: 'hints-session-1',
        update: {
          sessionUpdate: 'tool_call',
          toolCallId: 'tool-1',
          title: 'execute_bash',
          kind: 'shell',
          rawInput: { command: 'echo hello' },
        },
      });

      await new Promise((r) => setTimeout(r, 300));

      return (await tc!.mock.request('session/request_permission', {
        sessionId: 'hints-session-1',
        toolCall: { toolCallId: 'tool-1' },
        options: [
          { kind: 'allow_once', name: 'Allow once', optionId: 'allow_once' },
          {
            kind: 'allow_always',
            name: 'Always allow',
            optionId: 'allow_always',
          },
          { kind: 'reject_once', name: 'Deny', optionId: 'reject_once' },
        ],
        _meta: { kiro: { toolId: 'tool-1' } },
      })) as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('run echo');
    await tc.pressEnter();

    await tc.waitForVisibleText('requires approval', 5000);
    await tc.sleepMs(300);

    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('to navigate');
    expect(snap).toContain('to select');
    expect(snap).toContain('to edit');

    await tc.pressEscape();
    await tc.sleepMs(200);
  });
});
