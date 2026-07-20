import { afterEach, describe, expect, it } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'user-input-session';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: SESSION_ID,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
  tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));
}

function requestQuestion(
  tc: AcpTestCase,
  toolCallId: string,
  question = '**Requirement 7.3:** What should happen on retry?'
) {
  return tc.mock.request('_kiro/userInput', {
    sessionId: SESSION_ID,
    toolCallId,
    question,
    options: [
      { title: 'Always return an error' },
      { title: 'Only after retries exhausted' },
      { title: 'Ask before retrying' },
    ],
  });
}

describe('KAS user input questions (wire)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('renders markdown as a question and returns the selected answer', async () => {
    tc = new AcpTestCase({ testName: 'user-input-question' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    const responsePromise = requestQuestion(tc, 'question-render');
    await tc.waitForStore((state) => state.pendingQuestion !== null, 5000);
    await tc.waitForVisibleText('Requirement 7.3', 3000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).not.toContain('requires approval');
    expect(snapshot).toContain('Question');

    await tc.pressEnter();
    expect(await responsePromise).toEqual({
      action: 'answered',
      answer: 'Always return an error',
    });
  }, 30000);

  for (const [surface, extraEnv] of [
    ['tui', {}],
    ['lite', { KIRO_UI_MODE: 'lite', KIRO_LITE_ROLLOUT_ENABLED: '1' }],
  ] as const) {
    it(`${surface} waits for keyboard idle before arming a question`, async () => {
      tc = new AcpTestCase({
        testName: `user-input-idle-${surface}`,
        extraEnv,
      });
      setupHandshake(tc);

      await tc.launch();
      await tc.mock.awaitConnection();
      await tc.waitForVisibleText('ask a question', 10000);
      await tc.sendKeys('3');
      await tc.waitForStore((state) => state.commandInputValue === '3', 5000);

      const responsePromise = requestQuestion(
        tc,
        `question-idle-${surface}`,
        'Choose after the idle guard'
      );
      await tc.waitForStore((state) => state.pendingQuestion !== null, 5000);
      await tc.sleepMs(300);
      expect(tc.getSnapshotFormatted()).not.toContain('Always return an error');

      await tc.waitForVisibleText('Always return an error', 6000);
      const armedSnapshot = tc.getSnapshotFormatted();
      expect(armedSnapshot).toContain('❯ 1. Always return an error');
      expect(armedSnapshot).not.toContain('❯ 3. Ask before retrying');

      await tc.pressEnter();
      expect(await responsePromise).toEqual({
        action: 'answered',
        answer: 'Always return an error',
      });
    }, 30000);
  }
});
