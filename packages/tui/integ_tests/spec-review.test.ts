/**
 * Integration tests for the spec review surface.
 *
 * Covers what unit tests can't: ctrl+X at a checkpoint opens the document in a
 * real PTY, a typed comment lands against the line under the cursor, esc
 * returns to the still-pending checkpoint question with the comment staged, and
 * sending answers the question with the comment rather than the option text.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType } from '../src/types/agent-events';
import type { QuestionRequestInfo } from '../src/types/agent-events';

const QUESTION = 'Review the requirements, then:';
const CONTINUE_OPTION = 'Continue to design phase';
const FIRST_CRITERION = 'WHEN the page loads, THE Mode_Selector SHALL select';

const DOCUMENT = `# Requirements Document

## Introduction

A browser clock with two modes.

### Requirement 1: Mode Selection

**User Story:** As a visitor, I want to choose between counting up and down.

#### Acceptance Criteria

1. ${FIRST_CRITERION} Count_Up_Mode.
2. WHEN the user switches modes, THE Web_Clock SHALL reset the display.
`;

/** A workspace holding a written requirements document for web-clock. */
function seedWorkspace(): string {
  const root = mkdtempSync(join(tmpdir(), 'spec-review-'));
  const dir = join(root, '.kiro', 'specs', 'web-clock');
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'requirements.md'), DOCUMENT, 'utf8');
  return root;
}

/** Launch in that workspace and reach the checkpoint question. */
async function atCheckpoint(testName: string): Promise<TestCase> {
  const testCase = await TestCase.builder()
    .withTestName(testName)
    .withTimeout(20000)
    .withCwd(seedWorkspace())
    .withEnv({ KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '20000' })
    .launch();
  await testCase.waitForVisibleText('ask a question', 15000);

  await testCase.mockSessionUpdate({
    type: AgentEventType.SpecPhaseCheckpoint,
    featureName: 'web-clock',
    phase: 'requirements',
    artifactPath: '.kiro/specs/web-clock/requirements.md',
  });
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

describe('spec review surface', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('opens the document from the checkpoint and returns to the question', async () => {
    testCase = await atCheckpoint('spec-review-open');

    await testCase.sendKeys('\x18'); // ctrl+X
    await testCase.waitForVisibleText('Review requirements.md', 10000);

    // The document's own text is on screen, not a summary of it.
    const open = testCase.getSnapshot().join('\n');
    expect(open).toContain('### Requirement 1: Mode Selection');
    expect(open).toContain(FIRST_CRITERION);
    expect(open).toContain('to comment');
    expect((await testCase.getStore()).specReviewView).toBeTruthy();

    await testCase.pressEscape();
    await testCase.sleepMs(400);

    // Back at the checkpoint, which never stopped waiting.
    const back = await testCase.getStore();
    expect(back.mode).toBe('inline');
    expect(back.specReviewView).toBeNull();
    expect(back.pendingQuestion).not.toBeNull();
    expect(testCase.getSnapshot().join('\n')).toContain(CONTINUE_OPTION);
  }, 40000);

  it('stages a typed comment against the line under the cursor', async () => {
    testCase = await atCheckpoint('spec-review-comment');
    await testCase.sendKeys('\x18');
    await testCase.waitForVisibleText('Review requirements.md', 10000);

    // Move onto the first acceptance criterion, then comment on it.
    for (let i = 0; i < 12; i++) {
      await testCase.sendKeys('\x1b[B'); // down arrow
    }
    await testCase.sleepMs(200);
    const cursorLine =
      (await testCase.getStore()).specReviewView?.cursor.lineIndex ?? 0;

    await testCase.pressEnter();
    await testCase.sleepMs(200);
    // While composing, the row says what enter and esc do — the shortcuts
    // differ from the viewer's, so they can't be left to memory.
    const composing = testCase.getSnapshot().join('\n');
    expect(composing).toContain('save');
    expect(composing).toContain('esc');
    expect(composing).not.toContain('n/N to jump');

    await testCase.sendKeys('drop the second criterion');
    await testCase.pressEnter();
    await testCase.sleepMs(300);

    const store = await testCase.getStore();
    const [action] = store.specReviewComments['web-clock/requirements'] ?? [];
    expect(action?.body).toBe('drop the second criterion');
    expect(action?.anchor.range.start).toBe(cursorLine);
    // The anchor carries what the document says, so the agent can locate it.
    expect(action?.anchor.heading).toBeTruthy();
    expect(testCase.getSnapshot().join('\n')).toContain(
      'drop the second criterion'
    );
  }, 40000);

  it('jumps between requirements instead of walking every line', async () => {
    testCase = await atCheckpoint('spec-review-jump');
    await testCase.sendKeys('\x18');
    await testCase.waitForVisibleText('Review requirements.md', 10000);

    // Each keystroke reaches the next heading, however far away it sits.
    await testCase.sendKeys('n');
    await testCase.sleepMs(200);
    const first = await testCase.getStore();
    const firstLine = first.specReviewView?.cursor.lineIndex ?? -1;
    expect(first.specReviewView?.lines[firstLine]).toContain('Introduction');

    await testCase.sendKeys('n');
    await testCase.sleepMs(200);
    const second = await testCase.getStore();
    const secondLine = second.specReviewView?.cursor.lineIndex ?? -1;
    expect(second.specReviewView?.lines[secondLine]).toContain('Requirement 1');

    await testCase.sendKeys('N');
    await testCase.sleepMs(200);
    const back = await testCase.getStore();
    expect(back.specReviewView?.cursor.lineIndex).toBe(firstLine);

    // The footer names the primary path; `?` carries the rest.
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain('n/N to jump');
    expect(snap).toContain('? to see keys');

    // `?` lists every binding, including the ones the footer has no room for.
    await testCase.sendKeys('?');
    await testCase.sleepMs(250);
    const help = testCase.getSnapshot().join('\n');
    expect(help).toContain('half a page');
    expect(help).toContain('next / previous comment');
  }, 30000);

  it('sends staged comments as the answer to the checkpoint question', async () => {
    testCase = await atCheckpoint('spec-review-send');
    await testCase.sendKeys('\x18');
    await testCase.waitForVisibleText('Review requirements.md', 10000);
    await testCase.sendKeys('\x1b[B');
    await testCase.sleepMs(150);
    await testCase.pressEnter();
    await testCase.sleepMs(200);
    await testCase.sendKeys('tighten the introduction');
    await testCase.pressEnter();
    await testCase.sleepMs(300);
    await testCase.pressEscape();
    await testCase.sleepMs(400);

    // The checkpoint now offers to send them.
    await testCase.waitForVisibleText('Send 1 comment and revise', 5000);
    expect(testCase.getSnapshot().join('\n')).toContain('1 comment staged');

    await testCase.pressEnter();
    await testCase.sleepMs(500);

    const store = await testCase.getStore();
    // Answering sent them, so the checkpoint and its comments both go.
    expect(store.pendingQuestion).toBeNull();
    expect(
      store.specReviewComments['web-clock/requirements'] ?? []
    ).toHaveLength(0);
    // The transcript shows the choice the user made. What the agent receives is
    // the composed request, asserted against the router's unit tests — this can
    // only see the display string.
    const userMessages = store.messages.filter((m) => m.role === 'user');
    const last = userMessages[userMessages.length - 1] as { content: string };
    expect(last.content).toContain('Send 1 comment');
  }, 40000);

  it('refuses to advance the phase while a comment is staged', async () => {
    testCase = await atCheckpoint('spec-review-refuse');
    await testCase.sendKeys('\x18');
    await testCase.waitForVisibleText('Review requirements.md', 10000);
    await testCase.sendKeys('\x1b[B');
    await testCase.sleepMs(150);
    await testCase.pressEnter();
    await testCase.sleepMs(200);
    await testCase.sendKeys('this needs an acceptance criterion');
    await testCase.pressEnter();
    await testCase.sleepMs(300);
    await testCase.pressEscape();
    await testCase.sleepMs(400);
    await testCase.waitForVisibleText('Send 1 comment and revise', 5000);

    // Move past the send option onto "Continue" and take it.
    await testCase.sendKeys('\x1b[B');
    await testCase.sleepMs(150);
    await testCase.pressEnter();
    await testCase.sleepMs(500);

    // The question is still waiting and the comment is still staged.
    const store = await testCase.getStore();
    expect(store.pendingQuestion).not.toBeNull();
    expect(
      store.specReviewComments['web-clock/requirements'] ?? []
    ).toHaveLength(1);
    expect(testCase.getSnapshot().join('\n')).toContain(
      '1 comment staged \u2014 pick'
    );
  }, 40000);
});
