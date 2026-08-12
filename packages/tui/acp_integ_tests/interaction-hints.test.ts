import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
  PromptRequest,
  PromptResponse,
  SessionNotification,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
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

async function launchInitialized(tc: AcpTestCase): Promise<void> {
  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForStore((state) => state.isInitialized, 10_000);
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

    await launchInitialized(tc);

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
    await tc.sendKeys('/');

    await tc.waitForVisibleText('to navigate', 10_000);
    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('to navigate');
    expect(snap).toContain('to select');
  }, 20_000);

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
          kind: 'execute',
          rawInput: { command: 'echo hello' },
        },
      } satisfies SessionNotification);

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

    await launchInitialized(tc);

    await tc.sendKeys('run echo');
    await tc.pressEnter();

    await tc.waitForVisibleText('requires approval', 10_000);

    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('to navigate');
    expect(snap).toContain('to select');
    expect(snap).toContain('to edit');

    await tc.pressEscape();
    await tc.waitForStore((state) => state.pendingApproval === null, 10_000);
  }, 20_000);
});
