/**
 * Integration tests for the spec phase checkpoint.
 *
 * The agent reports a finished phase (`_kiro/spec/phaseCheckpoint`) and then
 * asks whether to proceed. These cover the PTY-visible result: the phase-complete
 * marker sits with the turn, the check-in question renders as a normal question,
 * and the marker clears once that question resolves either way.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import type { QuestionRequestInfo } from '../src/types/agent-events';

const CHECKPOINT_MARKER = 'Requirements complete';
const QUESTION = 'Review the requirements, then:';
const CONTINUE_OPTION = 'Continue to design phase';

/** Launch a TUI, report a finished phase, and ask the check-in question. */
async function openCheckpoint(testName: string): Promise<TestCase> {
  const testCase = await TestCase.builder()
    .withTestName(testName)
    .withTimeout(15000)
    // Keep the mock turn open long enough to interact with the panel.
    .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '15000' })
    .launch();
  await testCase.waitForVisibleText('ask a question', 15000);

  await testCase.mockSessionUpdate({
    type: AgentEventType.SpecPhaseCheckpoint,
    featureName: 'web-clock',
    phase: 'requirements',
    artifactPath: '/tmp/.kiro/specs/web-clock/requirements.md',
  });
  // `resolve` is omitted — MockSessionClient reattaches it on injection.
  await testCase.mockSessionUpdate({
    type: AgentEventType.QuestionRequest,
    value: {
      sessionId: 'mock-session-id',
      toolCallId: 'checkpoint-1',
      question: QUESTION,
      options: [{ title: CONTINUE_OPTION }],
    } as QuestionRequestInfo,
  });
  await testCase.typeAndSubmit('start');
  await testCase.waitForVisibleText(CONTINUE_OPTION, 10000);
  return testCase;
}

describe('spec phase checkpoint', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('marks the finished phase above the check-in question', async () => {
    testCase = await openCheckpoint('spec-checkpoint-render');

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain(CHECKPOINT_MARKER);
    expect(snap).toContain(QUESTION);
    expect(snap).toContain(CONTINUE_OPTION);
    // The free-text row is what turns a typed reply into feedback.
    expect(snap).toContain('or type feedback to request changes');
    // The marker names the phase, not the whole "complete + view" chip.
    expect(snap).not.toContain('ctrl+X');
    // A lone option has nothing to recommend against.
    expect(snap).not.toContain('(recommended)');

    const store = await testCase.getStore();
    expect(store.specPhaseCheckpoint?.phase).toBe('requirements');

    // The marker sits above the question panel, with the turn it concludes.
    const lines = testCase.getSnapshot();
    const markerRow = lines.findIndex((l) => l.includes(CHECKPOINT_MARKER));
    const questionRow = lines.findIndex((l) => l.includes(QUESTION));
    expect(markerRow).toBeGreaterThanOrEqual(0);
    expect(markerRow).toBeLessThan(questionRow);
  }, 30000);

  it('stays hidden until the agent actually asks', async () => {
    // The document is written well before the phase settles (detailer
    // sub-agents keep refining requirements), so the notification alone must
    // not claim the phase is done.
    testCase = await TestCase.builder()
      .withTestName('spec-checkpoint-not-yet-asked')
      .withTimeout(15000)
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '15000' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    await testCase.mockSessionUpdate({
      type: AgentEventType.SpecPhaseCheckpoint,
      featureName: 'web-clock',
      phase: 'requirements',
      artifactPath: '/tmp/.kiro/specs/web-clock/requirements.md',
    });
    await testCase.typeAndSubmit('start');
    await testCase.sleepMs(600);

    const store = await testCase.getStore();
    expect(store.specPhaseCheckpoint?.phase).toBe('requirements');
    expect(testCase.getSnapshot().join('\n')).not.toContain(CHECKPOINT_MARKER);
  }, 30000);

  it('clears the marker when the user continues', async () => {
    testCase = await openCheckpoint('spec-checkpoint-continue');

    await testCase.pressEnter();
    await testCase.sleepMs(400);

    const store = await testCase.getStore();
    expect(store.specPhaseCheckpoint).toBeNull();
    expect(store.pendingQuestion).toBeNull();
    expect(testCase.getSnapshot().join('\n')).not.toContain(CHECKPOINT_MARKER);
  }, 30000);

  it('sends typed text as feedback and clears the marker', async () => {
    testCase = await openCheckpoint('spec-checkpoint-feedback');

    // Typing goes to the free-text row rather than picking the option.
    await testCase.sendKeys('drop the alarm requirement');
    await testCase.sleepMs(200);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    const store = await testCase.getStore();
    expect(store.pendingQuestion).toBeNull();
    expect(store.specPhaseCheckpoint).toBeNull();
    // The answer carries what the user typed, not the option title.
    const answers = store.messages.filter((m) => m.role === 'user');
    const last = answers[answers.length - 1] as { content: string };
    expect(last.content).toContain('drop the alarm requirement');
    expect(testCase.getSnapshot().join('\n')).not.toContain(CHECKPOINT_MARKER);
  }, 30000);

  it('expires a checkpoint no question claimed, so a later one is undecorated', async () => {
    testCase = await TestCase.builder()
      .withTestName('spec-checkpoint-expiry')
      .withTimeout(15000)
      .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '15000' })
      .launch();
    await testCase.waitForVisibleText('ask a question', 15000);

    // A phase completes, but the turn ends before any check-in question — the
    // agent was interrupted, or moved on.
    await testCase.mockSessionUpdate({
      type: AgentEventType.SpecPhaseCheckpoint,
      featureName: 'web-clock',
      phase: 'requirements',
      artifactPath: '/tmp/.kiro/specs/web-clock/requirements.md',
    });
    await testCase.typeAndSubmit('draft it');
    await testCase.sleepMs(300);
    await testCase.completeTurn();
    await testCase.sleepMs(400);
    expect((await testCase.getStore()).specPhaseCheckpoint).toBeNull();

    // An unrelated question in a later turn must not inherit the marker.
    await testCase.mockSessionUpdate({
      type: AgentEventType.QuestionRequest,
      value: {
        sessionId: 'mock-session-id',
        toolCallId: 'unrelated-1',
        question: 'Should I also update the README?',
        options: [{ title: 'Yes' }],
      } as QuestionRequestInfo,
    });
    await testCase.typeAndSubmit('go on');
    await testCase.waitForVisibleText('Should I also update', 10000);

    expect(testCase.getSnapshot().join('\n')).not.toContain(CHECKPOINT_MARKER);
  }, 30000);

  it('clears the marker when the question is cancelled', async () => {
    testCase = await openCheckpoint('spec-checkpoint-cancel');

    await testCase.pressEscape();
    await testCase.sleepMs(400);

    const store = await testCase.getStore();
    expect(store.specPhaseCheckpoint).toBeNull();
    expect(testCase.getSnapshot().join('\n')).not.toContain(CHECKPOINT_MARKER);
  }, 30000);
});
