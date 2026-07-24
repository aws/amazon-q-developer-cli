/**
 * Integration tests for the interactive Question panel.
 *
 * Covers the PTY-visible behavior the unit tests can't: focused-option
 * highlight, footer copy, type-to-answer focus jump, and the pending
 * question rendering exactly once (panel only — the transcript row is
 * suppressed until the question resolves).
 */
import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import type { QuestionRequestInfo } from '../src/types/agent-events';

const QUESTION = 'Is this a new feature or a bugfix?';

function countOccurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

/** Launch a TUI, open a mid-turn question, and wait for the panel. */
async function openQuestion(testName: string): Promise<TestCase> {
  const testCase = await TestCase.builder()
    .withTestName(testName)
    .withTimeout(15000)
    // Keep the mock turn open long enough to interact with the panel.
    .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '15000' })
    .launch();
  await testCase.waitForVisibleText('ask a question', 15000);

  // The question arrives mid-turn, like a real user_input tool call.
  // `resolve` is omitted — MockSessionClient reattaches it on injection.
  await testCase.mockSessionUpdate({
    type: AgentEventType.QuestionRequest,
    value: {
      sessionId: 'mock-session-id',
      toolCallId: 'q-1',
      question: QUESTION,
      options: [
        {
          title: 'Build a Feature',
          description: 'Implement new functionality',
          recommended: true,
        },
        {
          title: 'Fix a Bug',
          description: 'Fix something that is broken',
        },
      ],
    } as QuestionRequestInfo,
  });
  await testCase.typeAndSubmit('start');
  await testCase.waitForVisibleText('Build a Feature', 10000);
  return testCase;
}

describe('Question panel', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('renders once with highlight, descriptions, and the submit footer', async () => {
    testCase = await openQuestion('question-panel-render');

    const snap = testCase.getSnapshot().join('\n');
    // The pending question appears exactly once: the interactive panel.
    // The transcript copy is suppressed until it resolves.
    expect(countOccurrences(snap, QUESTION)).toBe(1);
    // Focused (recommended) option carries the chevron.
    expect(snap).toMatch(/❯.*1\. Build a Feature/);
    expect(snap).toContain('(recommended)');
    expect(snap).toContain('Implement new functionality');
    expect(snap).toContain('Type a different answer');
    expect(snap).toContain('to submit');
    expect(snap).toContain('esc to cancel');
    expect(snap).not.toContain('to answer');
  }, 30000);

  it('typing immediately moves focus into the free-text input', async () => {
    testCase = await openQuestion('question-panel-type');

    await testCase.sendKeys('my own answer');
    await testCase.sleepMs(300);
    const snap = testCase.getSnapshot().join('\n');
    // The typed text lands in the input row without navigating down first.
    expect(snap).toContain('my own answer');

    await testCase.pressEnter();
    await testCase.sleepMs(400);

    const store = await testCase.getStore();
    expect(store.pendingQuestion).toBeNull();
    // Answered: the transcript record (question + answer) now renders.
    const after = testCase.getSnapshot().join('\n');
    expect(after).toContain('my own answer');
  }, 30000);

  it('arrow navigation moves the highlight between options', async () => {
    testCase = await openQuestion('question-panel-arrows');

    await testCase.sendKeys('\x1b[B'); // down
    await testCase.sleepMs(300);
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toMatch(/❯.*2\. Fix a Bug/);
    expect(snap).not.toMatch(/❯.*1\. Build a Feature/);
  }, 30000);
});
